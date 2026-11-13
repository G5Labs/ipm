//! Integration tests for the rev-7 worker-host provisioning REST endpoints
//! (PR-rev-7-9b): `POST/GET/DELETE /api/v1/orgs/:orgId/worker-hosts`.
//!
//! These run against a real Postgres (via `TestApp`) so they exercise the full
//! org-scoped authz/RLS path, the `host_jwt` mint, and the DB-only provisioning
//! contract — in particular that no GCP call leaks in early (`cloud_instance_id`
//! and `vpc_id` stay NULL after a POST). Contract:
//! `generation/.semlift/specs/g5/g5-api/worker_hosts.semlift.md` [D9]–[D15].

use crate::helpers;
use g5_api::ws::host_ctl::ControlPlaneHostState;

use helpers::{
    EnvVarGuard, TestApp, TestOrg, TestUser, TestWorkspace, TEST_PRIVATE_KEY_PEM,
    TEST_PUBLIC_KEY_PEM,
};
use reqwest::StatusCode;
use serde_json::{json, Value};
use std::time::Duration;
use uuid::Uuid;

use g5_auth::{JwtMinter, JwtValidator, Scope};
use g5_contracts::types::worker_host::{
    DEFAULT_WORKER_HOST_SLOT_CPUS, DEFAULT_WORKER_HOST_SLOT_MEM_MIB,
};

fn response_data(body: &Value) -> &Value {
    body.get("data")
        .expect("response body should include a top-level data field")
}

type StoppedVmState = (
    String,
    Option<chrono::DateTime<chrono::Utc>>,
    Option<chrono::DateTime<chrono::Utc>>,
    Option<Value>,
);

/// The cross-org host reconciler enumerates orgs with the system pool, then
/// mutates each tenant-owned registry through a scoped tenant transaction.
#[tokio::test]
async fn host_registry_reconciliation_marks_stale_hosts_with_tenant_scope() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let host_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO worker_hosts (host_id, org_id, region, status, last_heartbeat, slot_size_reported, runtime_identity) \
         VALUES ($1, $2, 'us-central1', 'ready', now() - interval '31 seconds', true, $3)",
    )
    .bind(host_id)
    .bind(org.id)
    .bind(helpers::current_runtime_identity())
    .execute(&app.db)
    .await
    .expect("seed stale ready host");

    g5_api_core::maintenance::host_registry::run_once(&app.state)
        .await
        .expect("host registry reconciliation should use tenant scope");

    let status: String = sqlx::query_scalar("SELECT status FROM worker_hosts WHERE host_id = $1")
        .bind(host_id)
        .fetch_one(&app.db)
        .await
        .expect("read reconciled host");
    assert_eq!(status, "unreachable");

    app.cleanup().await;
}

async fn seed_pending_start(app: &TestApp, workspace_id: Uuid, org_id: Uuid) {
    let (owner_user_id, lifecycle_generation): (Uuid, i64) =
        sqlx::query_as("SELECT owner_user_id, lifecycle_generation FROM workspaces WHERE id = $1")
            .bind(workspace_id)
            .fetch_one(&app.db)
            .await
            .expect("load workspace start authority");
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
    .expect("seed pending workspace start");
}

#[tokio::test]
async fn placement_ignores_reported_roots_orders_by_occupancy_and_enforces_region_quota() {
    use g5_api::services::worker_host_service::{
        PlacementOutcome, PlacementRequest, WorkerHostService,
    };

    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let retained_workspace = TestWorkspace::create(&app, &org).await;
    let occupancy_workspace = TestWorkspace::create(&app, &org).await;
    let quota_workspace = TestWorkspace::create(&app, &org).await;
    let wrong_region_workspace = TestWorkspace::create(&app, &org).await;
    for workspace in [
        &retained_workspace,
        &occupancy_workspace,
        &quota_workspace,
        &wrong_region_workspace,
    ] {
        seed_pending_start(&app, workspace.id, org.id).await;
    }

    // The older host is empty. The newer one still reports a root kept for
    // the first workspace, as a host from before root deletion at release
    // did; placement must not prefer it (`g5.runtime.placement`).
    let empty_host = Uuid::new_v4();
    let retained_host = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO worker_hosts(
             host_id, org_id, region, status, last_heartbeat, slots_total,
             slot_size_reported, created_at, runtime_identity)
         VALUES
             ($1, $3, 'us-central1', 'ready', now(), 2, true,
              now() - interval '1 minute', $4),
             ($2, $3, 'us-central1', 'ready', now(), 2, true, now(), $4)",
    )
    .bind(empty_host)
    .bind(retained_host)
    .bind(org.id)
    .bind(helpers::current_runtime_identity())
    .execute(&app.db)
    .await
    .expect("seed placement hosts");

    // Exercise the retained wire field against the current schema. No DB
    // inventory is needed or written, even by a legacy host's hello.
    let mut hello = measured_host_hello(retained_host);
    hello.slots_total = 2;
    hello.retained_roots = vec![g5_internal_api::host_ctl::RetainedRoot {
        workspace_id: retained_workspace.id,
        cache_generation: 7,
        expires_at: chrono::Utc::now() + chrono::TimeDelta::hours(1),
    }];
    app.host_control_backend()
        .accept_hello(
            retained_host,
            &hello,
            &g5_api::ws::host_ctl::HostConnection::new(true),
        )
        .await
        .expect("legacy root report is accepted without retired SQL columns");

    let service = WorkerHostService::new();
    // A start prepared before deletion must still be refused at placement.
    // Use the same valid intent/host that succeeds below to distinguish this
    // guard from missing capacity or malformed start state.
    sqlx::query("UPDATE workspaces SET deletion_operation_id=$2 WHERE id=$1")
        .bind(retained_workspace.id)
        .bind(Uuid::new_v4())
        .execute(&app.db)
        .await
        .unwrap();
    let mut deleting_db = app.scoped_db_for_org(org.id).await;
    let deleting = service
        .place(
            &mut deleting_db,
            PlacementRequest {
                org_id: org.id,
                workspace_id: retained_workspace.id,
                region: "us-central1",
                reachable_host_ids: &[empty_host, retained_host],
                excluded_host_ids: &[],
                start_deadline_at: chrono::Utc::now() + chrono::TimeDelta::minutes(2),
            },
        )
        .await
        .unwrap();
    assert!(matches!(deleting, PlacementOutcome::StartIntentUnavailable));
    deleting_db.rollback().await.unwrap();
    sqlx::query("UPDATE workspaces SET deletion_operation_id=NULL WHERE id=$1")
        .bind(retained_workspace.id)
        .execute(&app.db)
        .await
        .unwrap();
    let mut retained_db = app.scoped_db_for_org(org.id).await;
    let retained = service
        .place(
            &mut retained_db,
            PlacementRequest {
                org_id: org.id,
                workspace_id: retained_workspace.id,
                region: "us-central1",
                reachable_host_ids: &[empty_host, retained_host],
                excluded_host_ids: &[],
                start_deadline_at: chrono::Utc::now() + chrono::TimeDelta::minutes(2),
            },
        )
        .await
        .expect("place retained workspace");
    let PlacementOutcome::Reserved(retained) = retained else {
        panic!("retained workspace was not reserved: {retained:?}");
    };
    assert_eq!(
        retained.host_id, empty_host,
        "the older empty host wins over a host reporting the workspace's root"
    );
    assert_eq!(retained.occupancy_before, 0);
    retained_db
        .commit()
        .await
        .expect("commit retained placement");

    sqlx::query("UPDATE workspace_vms SET start_command_sent_at=now() WHERE workspace_id=$1")
        .bind(retained_workspace.id)
        .execute(&app.db)
        .await
        .unwrap();
    let mut release_db = app.scoped_db_for_org(org.id).await;
    assert!(service
        .release_placement(
            &mut release_db,
            &retained,
            org.id,
            retained_workspace.id,
            true,
            "host_capacity_refused",
            Some("disk"),
        )
        .await
        .expect("release transient host refusal"));
    release_db.commit().await.expect("commit transient release");
    // The host's own reason is kept, so the owner sees a full disk rather
    // than a missing host.
    let last_error: Value =
        sqlx::query_scalar("SELECT last_error FROM workspace_vms WHERE workspace_id = $1")
            .bind(retained_workspace.id)
            .fetch_one(&app.db)
            .await
            .expect("load released last error");
    assert_eq!(
        last_error,
        json!({"reason": "host_capacity_refused", "host_reason": "disk"})
    );
    let intent: Value =
        sqlx::query_scalar("SELECT start_intent FROM workspace_vms WHERE workspace_id = $1")
            .bind(retained_workspace.id)
            .fetch_one(&app.db)
            .await
            .expect("load released start intent");
    assert_eq!(intent["status"], "queued_for_capacity");
    assert!(intent.get("excluded_host_ids").is_none());

    // The definite capacity reply can beat its journal. Late durable facts
    // must preserve the same queued Start instead of making it permanent.
    let diagnostic = g5_contracts::error::HostDiskPressure {
        phase: "cold_image".into(),
        available_bytes: 1234,
        required_bytes: 4096,
    }
    .reason();
    let backend = app.host_control_backend();
    for (seq, kind) in [
        (
            1,
            g5_internal_api::host_ctl::VmEventKind::BootFailed {
                reason: diagnostic.clone(),
            },
        ),
        (
            2,
            g5_internal_api::host_ctl::VmEventKind::Exited {
                code: None,
                signal: None,
            },
        ),
    ] {
        backend
            .ingest_vm_event(
                empty_host,
                &g5_internal_api::host_ctl::VmEvent {
                    workspace_id: retained_workspace.id,
                    vm_id: retained.vm_id,
                    seq,
                    kind,
                    at: chrono::Utc::now(),
                    log_tail: Some(diagnostic.clone()),
                },
            )
            .await
            .unwrap();
    }
    let intent: Value =
        sqlx::query_scalar("SELECT start_intent FROM workspace_vms WHERE workspace_id=$1")
            .bind(retained_workspace.id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(intent["status"], "queued_for_capacity");

    let mut retry_db = app.scoped_db_for_org(org.id).await;
    let retry = service
        .place(
            &mut retry_db,
            PlacementRequest {
                org_id: org.id,
                workspace_id: retained_workspace.id,
                region: "us-central1",
                reachable_host_ids: &[empty_host, retained_host],
                excluded_host_ids: &[],
                start_deadline_at: chrono::Utc::now() + chrono::TimeDelta::minutes(2),
            },
        )
        .await
        .expect("retry transiently refused placement");
    let PlacementOutcome::Reserved(retry) = retry else {
        panic!("released workspace was not reserved again: {retry:?}");
    };
    assert_eq!(
        retry.host_id, retained_host,
        "a refusal cools down so another host can serve the Start"
    );
    retry_db.commit().await.expect("commit placement retry");

    let mut occupancy_db = app.scoped_db_for_org(org.id).await;
    let occupancy = service
        .place(
            &mut occupancy_db,
            PlacementRequest {
                org_id: org.id,
                workspace_id: occupancy_workspace.id,
                region: "us-central1",
                reachable_host_ids: &[empty_host, retained_host],
                excluded_host_ids: &[],
                start_deadline_at: chrono::Utc::now() + chrono::TimeDelta::minutes(2),
            },
        )
        .await
        .expect("place by derived occupancy");
    let PlacementOutcome::Reserved(occupancy) = occupancy else {
        panic!("occupancy workspace was not reserved: {occupancy:?}");
    };
    assert_eq!(
        occupancy.host_id, retained_host,
        "pack onto the host that already has a starting reservation"
    );
    assert_eq!(occupancy.occupancy_before, 1);
    occupancy_db
        .commit()
        .await
        .expect("commit occupancy placement");

    sqlx::query(
        "INSERT INTO configs(org_id, scope, scope_id, key, value)
         VALUES ($1, 'org', $1, 'max_running_vms', '2'::jsonb)
         ON CONFLICT (org_id, scope, scope_id, key)
         DO UPDATE SET value = EXCLUDED.value",
    )
    .bind(org.id)
    .execute(&app.db)
    .await
    .expect("set placement quota");
    let mut quota_db = app.scoped_db_for_org(org.id).await;
    let quota = service
        .place(
            &mut quota_db,
            PlacementRequest {
                org_id: org.id,
                workspace_id: quota_workspace.id,
                region: "us-central1",
                reachable_host_ids: &[empty_host, retained_host],
                excluded_host_ids: &[],
                start_deadline_at: chrono::Utc::now() + chrono::TimeDelta::minutes(2),
            },
        )
        .await
        .expect("evaluate placement quota");
    assert_eq!(quota, PlacementOutcome::QuotaExceeded { limit: 2 });
    quota_db
        .rollback()
        .await
        .expect("rollback quota observation");

    sqlx::query(
        "INSERT INTO configs(org_id, scope, scope_id, key, value)
         VALUES ($1, 'org', $1, 'max_running_vms', '10'::jsonb)
         ON CONFLICT (org_id, scope, scope_id, key)
         DO UPDATE SET value = EXCLUDED.value",
    )
    .bind(org.id)
    .execute(&app.db)
    .await
    .expect("restore placement quota");
    let mut region_db = app.scoped_db_for_org(org.id).await;
    let region = service
        .place(
            &mut region_db,
            PlacementRequest {
                org_id: org.id,
                workspace_id: wrong_region_workspace.id,
                region: "us-east1",
                reachable_host_ids: &[empty_host, retained_host],
                excluded_host_ids: &[],
                start_deadline_at: chrono::Utc::now() + chrono::TimeDelta::minutes(2),
            },
        )
        .await
        .expect("evaluate region capacity");
    assert_eq!(region, PlacementOutcome::NoCapacity);
    region_db.commit().await.expect("commit capacity queue");
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT start_intent ->> 'status' FROM workspace_vms WHERE workspace_id = $1"
        )
        .bind(wrong_region_workspace.id)
        .fetch_one(&app.db)
        .await
        .expect("load queued regional start"),
        "queued_for_capacity"
    );

    let client = app.authenticated_client(&user);
    let drain = client
        .post_json(
            &format!("/api/v1/orgs/{}/worker-hosts/{retained_host}/drain", org.id),
            &json!({"reason":"placement acceptance"}),
        )
        .await;
    assert_eq!(drain.status(), StatusCode::OK);
    let drain: Value = drain.json().await.expect("parse drain response");
    assert_eq!(drain["data"]["activeVms"], 2);
    assert_eq!(drain["data"]["alreadyDraining"], false);
    let drain_id = drain["data"]["drainId"].clone();
    let replay = client
        .post_json(
            &format!("/api/v1/orgs/{}/worker-hosts/{retained_host}/drain", org.id),
            &json!({"reason":"ignored replay reason"}),
        )
        .await;
    assert_eq!(replay.status(), StatusCode::OK);
    let replay: Value = replay.json().await.expect("parse drain replay");
    assert_eq!(replay["data"]["drainId"], drain_id);
    assert_eq!(replay["data"]["alreadyDraining"], true);
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT status FROM worker_hosts WHERE host_id = $1")
            .bind(retained_host)
            .fetch_one(&app.db)
            .await
            .expect("load drained host"),
        "draining"
    );

    app.cleanup().await;
}

#[tokio::test]
async fn placement_waits_for_resume_admission_without_holding_its_vm_lock() {
    use g5_api::services::worker_host_service::{
        PlacementOutcome, PlacementRequest, WorkerHostService,
    };

    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    seed_pending_start(&app, workspace.id, org.id).await;

    // Hold the first lock taken by record_start_intent, then let the real
    // placement service reach its contended lock before acquiring the VM.
    let mut admission = app.scoped_db_for_org(org.id).await;
    let admission_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(admission.conn())
        .await
        .unwrap();
    sqlx::query("SELECT id FROM workspaces WHERE id=$1 FOR UPDATE")
        .bind(workspace.id)
        .execute(admission.conn())
        .await
        .unwrap();
    let mut placement = app.scoped_db_for_org(org.id).await;
    let placement_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(placement.conn())
        .await
        .unwrap();
    let placement_task = tokio::spawn(async move {
        let outcome = WorkerHostService::new()
            .place(
                &mut placement,
                PlacementRequest {
                    org_id: org.id,
                    workspace_id: workspace.id,
                    region: "us-central1",
                    reachable_host_ids: &[],
                    excluded_host_ids: &[],
                    start_deadline_at: chrono::Utc::now() + chrono::TimeDelta::minutes(2),
                },
            )
            .await;
        placement.rollback().await.unwrap();
        outcome
    });
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let blocked: bool = sqlx::query_scalar("SELECT $2 = ANY(pg_blocking_pids($1))")
                .bind(placement_pid)
                .bind(admission_pid)
                .fetch_one(&app.db)
                .await
                .unwrap();
            if blocked {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("placement must reach the held workspace lock");
    sqlx::query("SET LOCAL lock_timeout = '500ms'")
        .execute(admission.conn())
        .await
        .unwrap();
    let vm_lock = sqlx::query(
        "SELECT status, start_intent FROM workspace_vms WHERE workspace_id=$1 FOR UPDATE",
    )
    .bind(workspace.id)
    .fetch_one(admission.conn())
    .await;
    admission.rollback().await.unwrap();
    let outcome = tokio::time::timeout(Duration::from_secs(5), placement_task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(
        vm_lock.is_ok(),
        "placement must not hold the VM while waiting for its workspace: {vm_lock:?}"
    );
    assert_eq!(outcome, PlacementOutcome::NoCapacity);
    app.cleanup().await;
}

/// Provisions one host via the API and returns the parsed `data` object.
async fn provision_host(client: &helpers::AuthenticatedClient, org_id: Uuid, body: Value) -> Value {
    let mut body = body;
    let object = body.as_object_mut().expect("provision body is an object");
    if !object.contains_key("baseImageRef") && !object.contains_key("imageTrack") {
        object.insert("baseImageRef".to_string(), json!(ORG_DEFAULT_REF));
    }
    provision_host_exact(client, org_id, body).await
}

async fn provision_host_exact(
    client: &helpers::AuthenticatedClient,
    org_id: Uuid,
    body: Value,
) -> Value {
    let resp = client
        .post_json(&format!("/api/v1/orgs/{org_id}/worker-hosts"), &body)
        .await;
    assert_eq!(
        resp.status(),
        StatusCode::CREATED,
        "provision should return 201 Created"
    );
    let body: Value = resp.json().await.expect("provision response is JSON");
    response_data(&body).clone()
}

#[tokio::test]
#[serial_test::serial(worker_scratch_disk)]
async fn provision_creates_db_only_record_with_host_jwt() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    let data = provision_host(
        &client,
        org.id,
        json!({ "region": "us-central1", "instanceType": "n2-standard-16" }),
    )
    .await;

    // Response shape.
    let host_id = data
        .get("hostId")
        .and_then(Value::as_str)
        .expect("hostId present");
    let host_id = Uuid::parse_str(host_id).expect("hostId is a UUID");
    assert_eq!(data["status"], "provisioning");
    assert_eq!(data["region"], "us-central1");
    assert_eq!(data["instanceType"], "n2-standard-16");
    let expected_scratch = 81920; // Managed hosts use the organization default.
    assert!(data.get("createdAt").is_some(), "createdAt present");

    // host_jwt validates and carries exactly Scope::WorkerHost, sub=host, org=org.
    let host_jwt = data
        .get("hostJwt")
        .and_then(Value::as_str)
        .expect("hostJwt present");
    let validator = JwtValidator::new(TEST_PUBLIC_KEY_PEM, None, None).unwrap();
    let claims = validator.validate(host_jwt).expect("host_jwt validates");
    assert_eq!(claims.sub, host_id.to_string());
    assert_eq!(claims.org, org.id.to_string());
    assert_eq!(claims.scopes, vec![Scope::WorkerHost]);

    // DB invariant: 9b performs NO cloud bring-up — both cloud columns stay NULL
    // and status is 'provisioning'.
    let row = sqlx::query_as::<_, (String, Option<String>, Option<String>, i32, i32, i32)>(
        "SELECT status, vpc_id, cloud_instance_id, slot_cpus, slot_mem_mib, scratch_disk_mib \
         FROM worker_hosts WHERE host_id = $1",
    )
    .bind(host_id)
    .fetch_one(&app.db)
    .await
    .expect("provisioned row exists");
    assert_eq!(row.0, "provisioning");
    assert!(row.1.is_none(), "vpc_id must be NULL in 9b");
    assert!(row.2.is_none(), "cloud_instance_id must be NULL in 9b");
    assert_eq!(row.3, DEFAULT_WORKER_HOST_SLOT_CPUS);
    assert_eq!(row.4, DEFAULT_WORKER_HOST_SLOT_MEM_MIB);
    assert_eq!(row.5, expected_scratch);
    // The row names the acting user as its creator (migration 061).
    let created_by: Option<Uuid> =
        sqlx::query_scalar("SELECT created_by_user_id FROM worker_hosts WHERE host_id = $1")
            .bind(host_id)
            .fetch_one(&app.db)
            .await
            .expect("provisioned row exists");
    assert_eq!(created_by, Some(user.id));

    app.cleanup().await;
}

#[tokio::test]
async fn host_hello_inventory_immediately_releases_a_missing_durable_incarnation() {
    use g5_api::ws::host_ctl::ControlPlaneHostState;
    use g5_internal_api::host_ctl::{HostHello, PROTOCOL_VERSION};

    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let host_id = Uuid::new_v4();
    let vm_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO worker_hosts(host_id, org_id, region, status, last_heartbeat, slot_size_reported, slots_total, runtime_identity)
         VALUES ($1,$2,'us-central1','ready',now(),true,1,$3)",
    )
    .bind(host_id)
    .bind(org.id)
    .bind(helpers::current_runtime_identity())
    .execute(&app.db)
    .await
    .expect("insert reconnecting host");
    sqlx::query(
        "INSERT INTO workspace_vms(
             org_id, workspace_id, host_id, vm_id, epoch, status, lease_expires_at
         ) VALUES ($1,$2,$3,$4,1,'running',now() + interval '5 minutes')",
    )
    .bind(org.id)
    .bind(workspace.id)
    .bind(host_id)
    .bind(vm_id.to_string())
    .execute(&app.db)
    .await
    .expect("insert durable incarnation absent from host hello");
    let backend = app.host_control_backend();
    let accepted = backend
        .accept_hello(
            host_id,
            &HostHello {
                host_id,
                protocol_version: PROTOCOL_VERSION,
                capabilities: Vec::new(),
                slots_total: 1,
                slot_size: Some(g5_internal_api::host_ctl::VmSizeSpec {
                    cpus: 2,
                    mem_mib: 4096,
                    scratch_disk_mib: 10240,
                }),
                inventory: Vec::new(),
                retained_roots: Vec::new(),
                runtime: Some(g5_internal_api::host_ctl::HostRuntimeIdentity::bundled()),
            },
            &g5_api::ws::host_ctl::HostConnection::new(true),
        )
        .await
        .expect("accept reconnecting host hello");
    assert!(accepted.current_incarnations.is_empty());
    let state: (String, Option<Uuid>, Option<String>, Value) = sqlx::query_as(
        "SELECT status, host_id, vm_id, last_error
         FROM workspace_vms WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .expect("load incarnation reconciled from host inventory");
    assert_eq!(state.0, "failed");
    assert_eq!(state.1, None);
    assert_eq!(state.2, None);
    assert_eq!(state.3["reason"], "host_inventory_missing");

    app.cleanup().await;
}

#[tokio::test]
async fn host_hello_inventory_confirms_an_absent_vm_after_stop_command() {
    use g5_api::ws::host_ctl::ControlPlaneHostState;
    use g5_internal_api::host_ctl::{HostHello, PROTOCOL_VERSION};

    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let host_id = Uuid::new_v4();
    let vm_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO worker_hosts(host_id, org_id, region, status, last_heartbeat, slot_size_reported, slots_total, runtime_identity)
         VALUES ($1,$2,'us-central1','ready',now(),true,1,$3)",
    )
    .bind(host_id)
    .bind(org.id)
    .bind(helpers::current_runtime_identity())
    .execute(&app.db)
    .await
    .expect("insert reconnecting stop host");
    sqlx::query(
        "INSERT INTO workspace_vms(
             org_id, workspace_id, host_id, vm_id, epoch, status,
             lease_expires_at, started_at, stop_reason, stop_operation_id,
             stop_generation, stop_deadline_at, stop_command_sent_at
         ) VALUES (
             $1,$2,$3,$4,1,'stopping',now() + interval '5 minutes',now(),
             'user',$5,1,now() + interval '5 minutes',now()
         )",
    )
    .bind(org.id)
    .bind(workspace.id)
    .bind(host_id)
    .bind(vm_id.to_string())
    .bind(Uuid::new_v4())
    .execute(&app.db)
    .await
    .expect("insert confirmed stop absent from host hello");
    let backend = app.host_control_backend();
    backend
        .accept_hello(
            host_id,
            &HostHello {
                host_id,
                protocol_version: PROTOCOL_VERSION,
                capabilities: Vec::new(),
                slots_total: 1,
                slot_size: Some(g5_internal_api::host_ctl::VmSizeSpec {
                    cpus: 2,
                    mem_mib: 4096,
                    scratch_disk_mib: 10240,
                }),
                inventory: Vec::new(),
                retained_roots: Vec::new(),
                runtime: Some(g5_internal_api::host_ctl::HostRuntimeIdentity::bundled()),
            },
            &g5_api::ws::host_ctl::HostConnection::new(true),
        )
        .await
        .expect("accept reconnecting host hello after stop");
    let state: StoppedVmState = sqlx::query_as(
        "SELECT status, unflushed_loss_at, session_harvest_loss_at, last_error
         FROM workspace_vms WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .expect("load stop confirmed from host inventory");
    assert_eq!(state.0, "stopped");
    assert_eq!(state.1, None);
    assert_eq!(state.2, None);
    assert_eq!(state.3, None);

    app.cleanup().await;
}

#[tokio::test]
async fn host_file_change_event_is_ephemeral_without_mutating_vm_lifecycle() {
    use g5_api::ws::host_ctl::ControlPlaneHostState;
    use g5_internal_api::host_ctl::{VmEvent, VmEventKind, VmFileChanged};

    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let host_id = Uuid::new_v4();
    let vm_id = Uuid::new_v4();
    let repo_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO worker_hosts(host_id, org_id, region, status, last_heartbeat, slot_size_reported, slots_total, runtime_identity)
         VALUES ($1,$2,'us-central1','ready',now(),true,1,$3)",
    )
    .bind(host_id)
    .bind(org.id)
    .bind(helpers::current_runtime_identity())
    .execute(&app.db)
    .await
    .expect("insert file-event host");
    sqlx::query(
        "INSERT INTO workspace_vms(
             org_id, workspace_id, host_id, vm_id, epoch, status, lease_expires_at
         ) VALUES ($1,$2,$3,$4,1,'running',now() + interval '5 minutes')",
    )
    .bind(org.id)
    .bind(workspace.id)
    .bind(host_id)
    .bind(vm_id.to_string())
    .execute(&app.db)
    .await
    .expect("insert file-event incarnation");
    // Exercise the production system role: it cannot read tenant workspace rows
    // directly, even though it can inspect the current host/VM registry.
    let visible_workspaces: i64 = sqlx::query_scalar("SELECT count(*) FROM workspaces")
        .fetch_one(&app.state.system_pool)
        .await
        .expect("query workspace visibility under RLS");
    assert_eq!(visible_workspaces, 0);
    let reader = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    sqlx::query("UPDATE org_memberships SET role = 'admin' WHERE org_id = $1 AND user_id = $2")
        .bind(org.id)
        .bind(reader.id)
        .execute(&app.db)
        .await
        .expect("grant durable workspace reader access");
    let channel = format!("workspace:{}", workspace.id);
    let mut owner_ws = app.ws_connect(&owner, &channel).await;
    owner_ws.expect_subscribed().await;
    let mut reader_ws = app.ws_connect(&reader, &channel).await;
    reader_ws.expect_subscribed().await;
    let backend = app.host_control_backend();
    backend
        .file_changed(
            host_id,
            &VmFileChanged {
                workspace_id: workspace.id,
                vm_id,
                repo_id,
                path: "src/main.rs".to_string(),
                rescan_required: false,
            },
        )
        .await
        .expect("ingest file change event");
    let delivered = owner_ws
        .expect_event("workspace.file_changed", Duration::from_secs(5))
        .await;
    assert_eq!(delivered["workspaceId"], workspace.id.to_string());
    assert_eq!(delivered["repoId"], repo_id.to_string());
    assert_eq!(delivered["path"], "src/main.rs");
    reader_ws.expect_silence(Duration::from_millis(100)).await;
    let durable_events: i64 =
        sqlx::query_scalar("SELECT count(*) FROM worker_vm_events WHERE vm_id = $1")
            .bind(vm_id.to_string())
            .fetch_one(&app.db)
            .await
            .expect("count durable VM events");
    assert_eq!(durable_events, 0);
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT status FROM workspace_vms WHERE workspace_id = $1",
        )
        .bind(workspace.id)
        .fetch_one(&app.db)
        .await
        .expect("load unchanged VM lifecycle"),
        "running"
    );

    // Legacy journalled notifications must use the same restricted-role lookup,
    // and replay must not publish a second notification.
    let journalled = VmEvent {
        workspace_id: workspace.id,
        vm_id,
        seq: 1,
        kind: VmEventKind::FileChanged {
            repo_id,
            path: "src/legacy.rs".to_string(),
            rescan_required: true,
        },
        at: chrono::Utc::now(),
        log_tail: None,
    };
    backend.ingest_vm_event(host_id, &journalled).await.unwrap();
    let delivered = owner_ws
        .expect_event("workspace.file_changed", Duration::from_secs(5))
        .await;
    assert_eq!(delivered["path"], "src/legacy.rs");
    assert_eq!(delivered["rescanRequired"], true);
    backend.ingest_vm_event(host_id, &journalled).await.unwrap();

    let event = VmFileChanged {
        workspace_id: workspace.id,
        vm_id,
        repo_id,
        path: "src/main.rs".to_string(),
        rescan_required: false,
    };
    backend.file_changed(Uuid::new_v4(), &event).await.unwrap();
    backend
        .file_changed(
            host_id,
            &VmFileChanged {
                vm_id: Uuid::new_v4(),
                ..event.clone()
            },
        )
        .await
        .unwrap();
    sqlx::query(
        "UPDATE workspace_vms SET status = 'stopped', host_id = NULL, vm_id = NULL,
                 lease_expires_at = NULL WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .execute(&app.db)
    .await
    .unwrap();
    backend.file_changed(host_id, &event).await.unwrap();
    sqlx::query(
        "UPDATE workspace_vms SET status = 'running', host_id = $2, vm_id = $3,
                 lease_expires_at = now() + interval '5 minutes' WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .bind(host_id)
    .bind(vm_id.to_string())
    .execute(&app.db)
    .await
    .unwrap();
    sqlx::query("UPDATE worker_hosts SET status = 'terminated' WHERE host_id = $1")
        .bind(host_id)
        .execute(&app.db)
        .await
        .unwrap();
    backend.file_changed(host_id, &event).await.unwrap();
    sqlx::query("UPDATE worker_hosts SET status = 'ready' WHERE host_id = $1")
        .bind(host_id)
        .execute(&app.db)
        .await
        .unwrap();
    sqlx::query("UPDATE workspaces SET deleted_at = now() WHERE id = $1")
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .unwrap();
    backend.file_changed(host_id, &event).await.unwrap();
    owner_ws.expect_silence(Duration::from_millis(100)).await;
    reader_ws.expect_silence(Duration::from_millis(100)).await;

    // A later valid event resolves the current owner, not the subscription's
    // original owner. No live paths may leak to the former owner.
    sqlx::query("UPDATE workspaces SET deleted_at = NULL, owner_user_id = $2 WHERE id = $1")
        .bind(workspace.id)
        .bind(reader.id)
        .execute(&app.db)
        .await
        .unwrap();
    backend.file_changed(host_id, &event).await.unwrap();
    let delivered = reader_ws
        .expect_event("workspace.file_changed", Duration::from_secs(5))
        .await;
    assert_eq!(delivered["path"], "src/main.rs");
    owner_ws.expect_silence(Duration::from_millis(100)).await;

    owner_ws.close().await;
    reader_ws.close().await;
    app.cleanup().await;
}

/// Ready must not consume a start intent or release held work after the
/// authoritative lease expired, including expiry during an unchanged-row lock.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ready_requires_a_live_lease_after_the_current_incarnation_lock() {
    let activity_events = helpers::tracing_capture();
    use g5_api::ws::host_ctl::ControlPlaneHostState;
    use g5_internal_api::host_ctl::{
        LeaseRenew, LeaseRenewResult, VmEvent, VmEventKind, DISPATCHER_CONTRACT_VERSION,
    };

    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let backend = app.host_control_backend();
    for case in ["live", "expired", "expires_while_waiting"] {
        let workspace = TestWorkspace::create(&app, &org).await;
        let host_id = Uuid::new_v4();
        let vm_id = Uuid::new_v4();
        let task_id = Uuid::new_v4();
        let (owner_user_id, lifecycle_generation): (Uuid, i64) = sqlx::query_as(
            "SELECT owner_user_id, lifecycle_generation FROM workspaces WHERE id = $1",
        )
        .bind(workspace.id)
        .fetch_one(&app.db)
        .await
        .expect("load current workspace start authority");
        let now = chrono::Utc::now();
        // Match the resume intent after placement: it remains bound to the
        // current owner/generation, and its first start attempt is recorded.
        let intent = json!({
            "id": Uuid::new_v4(),
            "kind": "resume",
            "status": "starting",
            "owner_user_id": owner_user_id,
            "lifecycle_generation": lifecycle_generation,
            "attempts": 1,
            "created_at": now.to_rfc3339(),
            "expires_at": (now + chrono::TimeDelta::minutes(5)).to_rfc3339(),
        });
        sqlx::query(
            "INSERT INTO worker_hosts(host_id, org_id, region, status, last_heartbeat, slot_size_reported)
             VALUES ($1,$2,'us-central1','ready',now(),true)",
        ).bind(host_id).bind(org.id).execute(&app.db).await.unwrap();
        sqlx::query(
            "INSERT INTO workspace_vms(org_id, workspace_id, host_id, vm_id, epoch,
                 status, lease_expires_at, module_catalog_version, boot_spec, start_intent)
             VALUES ($1,$2,$3,$4,1,'starting',clock_timestamp()+interval '5 minutes','fixture-v1',$5,$6)",
        ).bind(org.id).bind(workspace.id).bind(host_id).bind(vm_id.to_string())
            .bind(json!({"dispatcher_contract_version":DISPATCHER_CONTRACT_VERSION}))
            .bind(&intent).execute(&app.db).await.unwrap();
        sqlx::query(
            "INSERT INTO tasks(id, org_id, workspace_id, name, kind, status, hold_reason, spec, attempt, created_by)
             VALUES ($1,$2,$3,'Held command','command','waiting','owner_stop','{}'::jsonb,1,$4)",
        ).bind(task_id).bind(org.id).bind(workspace.id).bind(owner.id)
            .execute(&app.db).await.unwrap();
        let event = VmEvent {
            workspace_id: workspace.id,
            vm_id,
            seq: 1,
            kind: VmEventKind::Ready {
                epoch: 1,
                module_catalog_version: "fixture-v1".into(),
                dispatcher_contract_version: DISPATCHER_CONTRACT_VERSION,
            },
            at: chrono::Utc::now(),
            log_tail: None,
        };
        let renewal = LeaseRenew {
            workspace_id: workspace.id,
            vm_id,
            epoch: 1,
            seq: 1,
        };
        if case == "live" {
            assert!(
                matches!(
                    backend.renew_lease(host_id, &renewal).await.unwrap(),
                    LeaseRenewResult::Granted(_)
                ),
                "the first renewal is allowed while still starting"
            );
        } else if case == "expired" {
            sqlx::query("UPDATE workspace_vms SET lease_expires_at = clock_timestamp()-interval '1 second' WHERE workspace_id=$1")
                .bind(workspace.id).execute(&app.db).await.unwrap();
        }
        if case == "expires_while_waiting" {
            let holder = helpers::hold_while_the_lease_ends(
                &app,
                workspace.id,
                "SELECT 1 FROM workspace_vms WHERE workspace_id = $1 FOR UPDATE",
            )
            .await;
            let release = async {
                helpers::wait_for_lock_wait_past_the_lease(
                    &app,
                    workspace.id,
                    "SELECT last_activity_at FROM workspace_vms",
                )
                .await;
                holder.rollback().await.unwrap();
            };
            let (result, ()) = tokio::join!(backend.ingest_vm_event(host_id, &event), release);
            result.unwrap();
        } else {
            backend.ingest_vm_event(host_id, &event).await.unwrap();
        }
        // An event is acknowledged durably once even when it cannot change
        // current custody; replay must not later release the held task.
        backend.ingest_vm_event(host_id, &event).await.unwrap();
        let changes: Vec<_> = activity_events
            .lock()
            .unwrap()
            .iter()
            .filter(|e| {
                e.field("event") == Some("workspace_activity_changed")
                    && e.field("workspace_id") == Some(workspace.id.to_string().as_str())
            })
            .cloned()
            .collect();
        assert_eq!(
            changes.len(),
            usize::from(case == "live"),
            "only a committed Ready logs initialization, never replay/expiry"
        );
        if let Some(change) = changes.first() {
            assert_eq!(change.field("activity_kind"), Some("initialization"));
            assert_eq!(change.field("activity_reason"), Some("vm_ready"));
            assert_eq!(change.field("vm_id"), Some(vm_id.to_string().as_str()));
            assert_eq!(change.field("epoch"), Some("1"));
            assert!(change
                .field("correlation_id")
                .unwrap()
                .starts_with("host-event:"));
            assert_eq!(change.field("previous_last_activity_at"), Some("None"));
            assert!(change
                .field("new_last_activity_at")
                .unwrap()
                .contains("Some("));
        }
        let vm: (String, Value, Option<chrono::DateTime<chrono::Utc>>) = sqlx::query_as(
            "SELECT status, start_intent, started_at FROM workspace_vms WHERE workspace_id=$1",
        )
        .bind(workspace.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
        let task: (String, Option<String>) =
            sqlx::query_as("SELECT status, hold_reason FROM tasks WHERE id=$1")
                .bind(task_id)
                .fetch_one(&app.db)
                .await
                .unwrap();
        if case == "live" {
            assert_eq!(vm.0, "running");
            assert_eq!(vm.1["status"], "consumed");
            assert_eq!(vm.1["consumed_vm_id"], vm_id.to_string());
            assert!(vm.2.is_some());
            assert_eq!(task, ("queued".into(), None));
        } else {
            assert_eq!(vm.0, "starting", "{case}");
            assert_eq!(vm.1, intent, "{case}: start intent is unchanged");
            assert!(vm.2.is_none(), "{case}");
            assert_eq!(
                task,
                ("waiting".into(), Some("owner_stop".into())),
                "{case}"
            );
            assert!(
                matches!(
                    backend.renew_lease(host_id, &renewal).await.unwrap(),
                    LeaseRenewResult::Stale(_)
                ),
                "{case}: early scheduling cannot revive an expired authoritative lease"
            );
        }
    }
    app.cleanup().await;
}

#[tokio::test]
async fn exit_before_ready_does_not_claim_unflushed_workspace_state() {
    use g5_api::ws::host_ctl::ControlPlaneHostState;
    use g5_internal_api::host_ctl::{VmEvent, VmEventKind};

    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let host_id = Uuid::new_v4();
    let vm_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO worker_hosts(host_id, org_id, region, status, last_heartbeat, slot_size_reported, slots_total, runtime_identity)
         VALUES ($1,$2,'us-central1','ready',now(),true,1,$3)",
    )
    .bind(host_id)
    .bind(org.id)
    .bind(helpers::current_runtime_identity())
    .execute(&app.db)
    .await
    .expect("insert booting host");
    sqlx::query(
        "INSERT INTO workspace_vms(
             org_id, workspace_id, host_id, vm_id, epoch, status, lease_expires_at)
         VALUES ($1,$2,$3,$4,1,'starting',now() + interval '5 minutes')",
    )
    .bind(org.id)
    .bind(workspace.id)
    .bind(host_id)
    .bind(vm_id.to_string())
    .execute(&app.db)
    .await
    .expect("insert starting incarnation");
    let backend = app.host_control_backend();
    backend
        .ingest_vm_event(
            host_id,
            &VmEvent {
                workspace_id: workspace.id,
                vm_id,
                seq: 1,
                kind: VmEventKind::Exited {
                    code: Some(1),
                    signal: None,
                },
                at: chrono::Utc::now(),
                log_tail: Some("dispatcher exited before ready".to_string()),
            },
        )
        .await
        .expect("ingest startup exit");
    let state: (
        String,
        Option<chrono::DateTime<chrono::Utc>>,
        Option<chrono::DateTime<chrono::Utc>>,
    ) = sqlx::query_as(
        "SELECT status, unflushed_loss_at, session_harvest_loss_at
         FROM workspace_vms WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .expect("load startup exit state");
    assert_eq!(state.0, "failed");
    assert_eq!(state.1, None);
    assert_eq!(state.2, None);

    app.cleanup().await;
}

#[tokio::test]
async fn provision_defaults_instance_type_and_rejects_empty_region() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    // Omitted instanceType → default c3-standard-4.
    let data = provision_host(&client, org.id, json!({ "region": "europe-west1" })).await;
    assert_eq!(data["instanceType"], "c3-standard-4");

    // Empty region → 422 VALIDATION_ERROR.
    let resp = client
        .post_json(
            &format!("/api/v1/orgs/{}/worker-hosts", org.id),
            &json!({ "region": "   " }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["error"]["code"], "VALIDATION_ERROR");

    app.cleanup().await;
}

/// Boundary check for the `instance_type` allowlist (ontology [D41], chat C1).
/// Verifies (a) `c3-standard-8` — added in this PR — passes the route check
/// and round-trips through to the response/DB row, and (b) a value outside
/// the capability matrix fails closed with 422 + the `allowed` array surfaced
/// in the error envelope so the operator/UI sees what IS supported.
#[tokio::test]
async fn provision_accepts_c3_and_rejects_off_allowlist_instance_type() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    // (a) C3 round-trip — the latest family (third in the N2/C2/C3 set after N2D removal).
    let data = provision_host(
        &client,
        org.id,
        json!({ "region": "us-central1", "instanceType": "c3-standard-8" }),
    )
    .await;
    assert_eq!(data["instanceType"], "c3-standard-8");
    let host_id = Uuid::parse_str(data["hostId"].as_str().unwrap()).unwrap();
    let row: (String,) =
        sqlx::query_as("SELECT instance_type FROM worker_hosts WHERE host_id = $1")
            .bind(host_id)
            .fetch_one(&app.db)
            .await
            .expect("c3 row persisted");
    assert_eq!(row.0, "c3-standard-8");

    // (b) Off-allowlist value (an SKU we deliberately exclude — c3-standard-176
    // is real in GCE but outside our cost-vector ceiling per [D41]) → 422 with
    // VALIDATION_ERROR and an `allowed` array.
    let resp = client
        .post_json(
            &format!("/api/v1/orgs/{}/worker-hosts", org.id),
            &json!({ "region": "us-central1", "instanceType": "c3-standard-176" }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["error"]["code"], "VALIDATION_ERROR");
    let details = &body["error"]["details"];
    assert_eq!(details["field"], "instanceType");
    assert_eq!(details["value"], "c3-standard-176");
    let allowed = details["allowed"]
        .as_array()
        .expect("allowed must be a JSON array");
    assert!(
        allowed.iter().any(|v| v == "c3-standard-8"),
        "allowlist surfaced to the client should include c3-standard-8"
    );
    assert!(
        !allowed.iter().any(|v| v == "c3-standard-176"),
        "off-list SKU must not appear in the surfaced allowlist"
    );

    // (c) N2D is removed (PR1 customer-runtime): GCE nested virtualization is
    // unavailable on AMD-based VMs, so n2d-* is no longer in the capability
    // matrix. The boundary rejects it 422 and the surfaced allowlist has the
    // 12-SKU N2/C2/C3 set with no n2d entry.
    let resp = client
        .post_json(
            &format!("/api/v1/orgs/{}/worker-hosts", org.id),
            &json!({ "region": "us-central1", "instanceType": "n2d-standard-8" }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["error"]["code"], "VALIDATION_ERROR");
    let allowed = body["error"]["details"]["allowed"]
        .as_array()
        .expect("allowed must be a JSON array");
    assert_eq!(allowed.len(), 12, "12 SKUs across three families (no N2D)");
    assert!(
        !allowed
            .iter()
            .any(|v| v.as_str().is_some_and(|s| s.starts_with("n2d-"))),
        "no N2D SKU may appear in the surfaced allowlist"
    );

    app.cleanup().await;
}

/// Ontology [D53]: `workspaceVmLimit` accepts `1..=vCPUs(instanceType)` and 201s;
/// `0` and any value above the vCPU count fail closed with 422 + a
/// `workspaceVmLimit` field error naming the bound.
#[tokio::test]
async fn provision_validates_workspace_vm_limit_against_vcpu_count() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    // (a) In-range on an 8-vCPU type → 201.
    let data = provision_host(
        &client,
        org.id,
        json!({ "region": "us-central1", "instanceType": "n2-standard-8", "workspaceVmLimit": 4 }),
    )
    .await;
    assert_eq!(data["status"], "provisioning");

    // (b) 0 → 422 with a workspaceVmLimit field error.
    let resp = client
        .post_json(
            &format!("/api/v1/orgs/{}/worker-hosts", org.id),
            &json!({ "region": "us-central1", "instanceType": "n2-standard-8", "workspaceVmLimit": 0 }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["error"]["code"], "VALIDATION_ERROR");
    assert_eq!(body["error"]["details"]["field"], "workspaceVmLimit");

    // (c) Above the vCPU count (9 > 8) → 422 (1× ceiling, no oversubscription).
    let resp = client
        .post_json(
            &format!("/api/v1/orgs/{}/worker-hosts", org.id),
            &json!({ "region": "us-central1", "instanceType": "n2-standard-8", "workspaceVmLimit": 9 }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["error"]["details"]["field"], "workspaceVmLimit");
    assert_eq!(body["error"]["details"]["max"], 8);

    app.cleanup().await;
}

/// Ontology g5.runtime.placement.host-disk: `bootDiskGb` is optional and
/// accepts `30 ..= 2000` — deliberately including sizes below the floor and
/// below the size at which any VM can start. Anything outside fails closed with
/// 422 and a `bootDiskGb` field error naming the bound, and the host list
/// publishes the coefficients the dialog shows its default from.
#[tokio::test]
#[serial_test::serial(worker_scratch_disk)]
async fn provision_validates_boot_disk_gb_and_publishes_the_sizing_contract() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    // (a) An explicit size above H + V × two slots is accepted.
    let data = provision_host(
        &client,
        org.id,
        json!({ "region": "us-central1", "instanceType": "n2-standard-8", "bootDiskGb": 300 }),
    )
    .await;
    assert_eq!(data["status"], "provisioning");

    // (b) Below the provider minimum (GCE cannot create a disk smaller than
    // the source image) → 422 naming the bound.
    let resp = client
        .post_json(
            &format!("/api/v1/orgs/{}/worker-hosts", org.id),
            &json!({ "region": "us-central1", "baseImageRef": helpers::TEST_WORKER_IMAGE_REF, "bootDiskGb": 29 }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["error"]["code"], "VALIDATION_ERROR");
    assert_eq!(body["error"]["details"]["field"], "bootDiskGb");
    assert_eq!(body["error"]["details"]["min"], 200);

    // (c) Above the cost cap → 422.
    let resp = client
        .post_json(
            &format!("/api/v1/orgs/{}/worker-hosts", org.id),
            &json!({ "region": "us-central1", "baseImageRef": helpers::TEST_WORKER_IMAGE_REF, "bootDiskGb": 2001 }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["error"]["details"]["field"], "bootDiskGb");
    assert_eq!(body["error"]["details"]["max"], 2000);

    // (d) The envelope publishes the fixed policy, independent of VM disk size.
    for scratch in ["10240", "65536", "131072"] {
        let _env = helpers::EnvVarGuard::set_many(&[("G5_WORKER_SCRATCH_DISK_MIB", scratch)]);
        let resp = client
            .get(&format!("/api/v1/orgs/{}/worker-hosts", org.id))
            .await;
        assert_eq!(resp.status(), StatusCode::OK);
        let body: Value = resp.json().await.unwrap();
        assert_eq!(
            body["data"]["bootDiskSizing"],
            json!({"baseGb": 120, "perSlotGb": 80, "floorGb": 30, "minGb": 30, "maxGb": 2000}),
            "the policy must be independent of scratch size {scratch}"
        );
    }

    app.cleanup().await;
}

#[tokio::test]
async fn deallocate_terminates_row_and_is_idempotent() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    let data = provision_host(&client, org.id, json!({ "region": "us-central1" })).await;
    let host_id = data["hostId"].as_str().unwrap().to_string();
    let path = format!("/api/v1/orgs/{}/worker-hosts/{}", org.id, host_id);

    // First DELETE → 204.
    let resp = client.delete(&path).await;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // Row NOT deleted; status='terminated', last_heartbeat cleared (audit trail).
    let row = sqlx::query_as::<_, (String, Option<chrono::DateTime<chrono::Utc>>)>(
        "SELECT status, last_heartbeat FROM worker_hosts WHERE host_id = $1",
    )
    .bind(Uuid::parse_str(&host_id).unwrap())
    .fetch_one(&app.db)
    .await
    .expect("row still present after deallocate");
    assert_eq!(row.0, "terminated");
    assert!(row.1.is_none(), "last_heartbeat cleared on terminate");

    // Re-DELETE on an already-terminated host is idempotent → 204.
    let resp2 = client.delete(&path).await;
    assert_eq!(resp2.status(), StatusCode::NO_CONTENT);

    // Unknown host → 404. Per ontology [D13], the handler emits a
    // `worker_host.deallocate_not_found` warn before returning NotFound so
    // log analysis can flag stale tooling or enumeration probes. The status
    // code is the assertable contract from the integration boundary; the
    // warn is best-effort observability and is exercised by virtue of the
    // 404 code path running through this assertion.
    let unknown = format!("/api/v1/orgs/{}/worker-hosts/{}", org.id, Uuid::new_v4());
    let resp3 = client.delete(&unknown).await;
    assert_eq!(resp3.status(), StatusCode::NOT_FOUND);

    app.cleanup().await;
}

/// Placement holds the host row while it puts a VM on the host. A removal
/// that starts meanwhile waits for that row before it counts the host's VMs,
/// so it sees the new VM and refuses to remove the host without force.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn removal_counts_a_vm_placed_while_it_waits_for_the_host() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let host_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO worker_hosts(host_id, org_id, region, status, last_heartbeat, slot_size_reported, runtime_identity)
         VALUES ($1,$2,'us-central1','ready',now(),true,$3)",
    )
    .bind(host_id)
    .bind(org.id)
    .bind(helpers::current_runtime_identity())
    .execute(&app.db)
    .await
    .expect("insert host");

    // A placement in flight: the host row is held and the VM row written,
    // not yet committed.
    let mut placement = app.db.begin().await.expect("begin the placement");
    sqlx::query("SELECT 1 FROM worker_hosts WHERE host_id = $1 FOR UPDATE")
        .bind(host_id)
        .execute(&mut *placement)
        .await
        .expect("hold the host");
    sqlx::query(
        "INSERT INTO workspace_vms(
             org_id, workspace_id, host_id, vm_id, epoch, status, lease_expires_at
         ) VALUES ($1,$2,$3,$4,1,'starting',now() + interval '5 minutes')",
    )
    .bind(org.id)
    .bind(workspace.id)
    .bind(host_id)
    .bind(Uuid::new_v4().to_string())
    .execute(&mut *placement)
    .await
    .expect("place a VM");

    let client = app.authenticated_client(&user);
    let path = format!("/api/v1/orgs/{}/worker-hosts/{host_id}", org.id);
    let commit_placement = async {
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let waiting: bool = sqlx::query_scalar(
                    "SELECT EXISTS (SELECT 1 FROM pg_stat_activity
                     WHERE datname = current_database() AND wait_event_type = 'Lock'
                       AND query LIKE '%worker_hosts%')",
                )
                .fetch_one(&app.db)
                .await
                .expect("read lock waits");
                if waiting {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("the removal waits for the host row");
        placement.commit().await.expect("commit the placement");
    };
    let (response, ()) = tokio::join!(client.delete(&path), commit_placement);
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let status: String = sqlx::query_scalar("SELECT status FROM worker_hosts WHERE host_id = $1")
        .bind(host_id)
        .fetch_one(&app.db)
        .await
        .expect("load the host");
    assert_eq!(status, "ready", "a refused removal changes nothing");
    app.cleanup().await;
}

#[tokio::test]
async fn list_supports_status_filter_and_pagination() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    // Provision three hosts; terminate one so we can test the status filter.
    // Regions must be in `WORKER_HOST_REGION_ALLOWLIST` (Fix 1, round-2).
    let mut host_ids = Vec::new();
    for region in ["us-central1", "us-east1", "us-east4"] {
        let data = provision_host(&client, org.id, json!({ "region": region })).await;
        host_ids.push(data["hostId"].as_str().unwrap().to_string());
    }
    let terminated = &host_ids[0];
    let resp = client
        .delete(&format!(
            "/api/v1/orgs/{}/worker-hosts/{}",
            org.id, terminated
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // Unfiltered list: total = 3, ordered created_at DESC (last provisioned first).
    let resp = client
        .get(&format!("/api/v1/orgs/{}/worker-hosts", org.id))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = resp.json().await.unwrap();
    let data = response_data(&body);
    assert_eq!(data["total"], 3);
    let hosts = data["hosts"].as_array().unwrap();
    assert_eq!(hosts.len(), 3);
    // created_at DESC → us-east4 (last inserted) is first.
    assert_eq!(hosts[0]["region"], "us-east4");
    assert_eq!(hosts[2]["region"], "us-central1");
    // Each listed host names who created it.
    for host in hosts {
        assert_eq!(host["createdByUserId"], user.id.to_string());
        assert_eq!(host["createdByDisplayName"], user.display_name);
        assert_eq!(host["createdByEmail"], user.email);
    }

    // Status filter: only the two provisioning hosts.
    let resp = client
        .get(&format!(
            "/api/v1/orgs/{}/worker-hosts?status=provisioning",
            org.id
        ))
        .await;
    let body: Value = resp.json().await.unwrap();
    let data = response_data(&body);
    assert_eq!(data["total"], 2, "two hosts remain provisioning");
    let hosts = data["hosts"].as_array().unwrap();
    assert!(hosts.iter().all(|h| h["status"] == "provisioning"));

    // Status filter for terminated → exactly the one we deleted.
    let resp = client
        .get(&format!(
            "/api/v1/orgs/{}/worker-hosts?status=terminated",
            org.id
        ))
        .await;
    let body: Value = resp.json().await.unwrap();
    let data = response_data(&body);
    assert_eq!(data["total"], 1);
    assert_eq!(data["hosts"][0]["hostId"], *terminated);

    // Pagination: limit=2 returns 2 rows but total still reflects all 3.
    let resp = client
        .get(&format!(
            "/api/v1/orgs/{}/worker-hosts?limit=2&offset=0",
            org.id
        ))
        .await;
    let body: Value = resp.json().await.unwrap();
    let data = response_data(&body);
    assert_eq!(data["total"], 3);
    assert_eq!(data["hosts"].as_array().unwrap().len(), 2);

    // offset=2 returns the remaining 1 row.
    let resp = client
        .get(&format!(
            "/api/v1/orgs/{}/worker-hosts?limit=2&offset=2",
            org.id
        ))
        .await;
    let body: Value = resp.json().await.unwrap();
    let data = response_data(&body);
    assert_eq!(data["hosts"].as_array().unwrap().len(), 1);

    app.cleanup().await;
}

#[tokio::test]
async fn endpoints_reject_missing_and_cross_org_auth() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    // Seed one host in org A for the cross-org DELETE/GET probes.
    let data = provision_host(&client, org.id, json!({ "region": "us-central1" })).await;
    let host_id = data["hostId"].as_str().unwrap().to_string();

    // --- No JWT → 401 (bare client, no Authorization header). ---
    let no_auth = app
        .client
        .post(format!(
            "{}/api/v1/orgs/{}/worker-hosts",
            app.base_url, org.id
        ))
        .json(&json!({ "region": "us-central1" }))
        .send()
        .await
        .expect("unauthenticated POST sends");
    assert_eq!(no_auth.status(), StatusCode::UNAUTHORIZED);

    let no_auth_list = app
        .client
        .get(format!(
            "{}/api/v1/orgs/{}/worker-hosts",
            app.base_url, org.id
        ))
        .send()
        .await
        .expect("unauthenticated GET sends");
    assert_eq!(no_auth_list.status(), StatusCode::UNAUTHORIZED);

    // --- Cross-org: a user with no membership in org B is Forbidden (403). ---
    let mut other_user = TestUser::create(&app).await;
    let other_org = TestOrg::create_with_admin(&app, &mut other_user).await;
    let other_client = app.authenticated_client(&other_user);

    // other_user (admin of other_org) targets org A's collection → 403 on all three.
    let post = other_client
        .post_json(
            &format!("/api/v1/orgs/{}/worker-hosts", org.id),
            &json!({ "region": "us-central1" }),
        )
        .await;
    assert_eq!(
        post.status(),
        StatusCode::FORBIDDEN,
        "cross-org POST is 403"
    );

    let list = other_client
        .get(&format!("/api/v1/orgs/{}/worker-hosts", org.id))
        .await;
    assert_eq!(list.status(), StatusCode::FORBIDDEN, "cross-org GET is 403");

    let del = other_client
        .delete(&format!("/api/v1/orgs/{}/worker-hosts/{}", org.id, host_id))
        .await;
    assert_eq!(
        del.status(),
        StatusCode::FORBIDDEN,
        "cross-org DELETE is 403"
    );

    // Sanity: the host in org A is untouched by the rejected cross-org DELETE.
    let still_provisioning =
        sqlx::query_scalar::<_, String>("SELECT status FROM worker_hosts WHERE host_id = $1")
            .bind(Uuid::parse_str(&host_id).unwrap())
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(still_provisioning, "provisioning");

    // Keep other_org referenced so the unused-binding lint stays quiet and the
    // intent (a fully-owned but unrelated org) is explicit.
    assert_ne!(other_org.id, org.id);

    app.cleanup().await;
}

#[tokio::test]
async fn endpoints_reject_member_without_workers_admin_scope() {
    // The `workers:admin` scope is the only acceptable gate for these endpoints
    // ([D10]–[D12]); a member with arbitrary read scopes but no admin must see
    // 403 from `RequireScopes` on all three verbs. This proves a future scope
    // loosening can't silently regress the contract.
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let owner_client = app.authenticated_client(&owner);

    // Seed one host for the DELETE/GET probes.
    let data = provision_host(&owner_client, org.id, json!({ "region": "us-central1" })).await;
    let host_id = data["hostId"].as_str().unwrap().to_string();

    // Member of the same org but without `workers:admin`.
    let member = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::OrgRead, Scope::WorkersRead, Scope::TasksRead],
    )
    .await;
    let member_client = app.authenticated_client(&member);

    let post = member_client
        .post_json(
            &format!("/api/v1/orgs/{}/worker-hosts", org.id),
            &json!({ "region": "us-central1" }),
        )
        .await;
    assert_eq!(
        post.status(),
        StatusCode::FORBIDDEN,
        "member-without-workers:admin POST is 403"
    );

    let list = member_client
        .get(&format!("/api/v1/orgs/{}/worker-hosts", org.id))
        .await;
    assert_eq!(
        list.status(),
        StatusCode::FORBIDDEN,
        "member-without-workers:admin GET is 403"
    );

    let del = member_client
        .delete(&format!("/api/v1/orgs/{}/worker-hosts/{}", org.id, host_id))
        .await;
    assert_eq!(
        del.status(),
        StatusCode::FORBIDDEN,
        "member-without-workers:admin DELETE is 403"
    );

    // Belt-and-braces: the seeded host stays in `provisioning` (no side effect
    // from the rejected DELETE).
    let still_provisioning =
        sqlx::query_scalar::<_, String>("SELECT status FROM worker_hosts WHERE host_id = $1")
            .bind(Uuid::parse_str(&host_id).unwrap())
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(still_provisioning, "provisioning");

    app.cleanup().await;
}

#[tokio::test]
async fn endpoints_reject_host_jwt_token_confusion() {
    // A `host_jwt` carries `Scope::WorkerHost` and the auth middleware ([D4]
    // routing rules) rejects host tokens outright on `/api/v1/…` BEFORE the
    // per-route scope check runs — that's the `HOST_ONLY_INTERNAL_PREFIX`
    // path-prefix gate installed in 9a that protects user-facing endpoints
    // from a host-issued token. Outcome: 401 (layered-defense outcome),
    // STRONGER than the 403 that a per-route `RequireScopes` would yield if
    // the path-prefix gate didn't exist — 401 means "wrong token type for
    // this surface entirely", 403 means "right token type, wrong scopes".
    // The 401 here proves the outer gate fired (defense in depth).
    // Token confusion = if this ever flips to 200 or 403-without-401, a
    // host-issued JWT could reach the org-admin surface.
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let owner_client = app.authenticated_client(&owner);

    let data = provision_host(&owner_client, org.id, json!({ "region": "us-central1" })).await;
    let host_id = data["hostId"].as_str().unwrap().to_string();
    let host_uuid = Uuid::parse_str(&host_id).unwrap();

    // Mint a host JWT with the production-wired minter contract.
    let host_minter = JwtMinter::new(TEST_PRIVATE_KEY_PEM, "g5-api-test", "g5")
        .expect("mint host minter for test");
    let host_jwt = host_minter
        .mint_host_jwt(host_uuid, org.id, Duration::from_secs(3600))
        .expect("mint test host JWT");

    let bearer = format!("Bearer {host_jwt}");
    let post = app
        .client
        .post(format!(
            "{}/api/v1/orgs/{}/worker-hosts",
            app.base_url, org.id
        ))
        .header("Authorization", &bearer)
        .json(&json!({ "region": "us-central1" }))
        .send()
        .await
        .expect("host-jwt POST sends");
    assert_eq!(
        post.status(),
        StatusCode::UNAUTHORIZED,
        "host JWT on org-admin POST is 401 (path-prefix gate from [D4])"
    );

    let list = app
        .client
        .get(format!(
            "{}/api/v1/orgs/{}/worker-hosts",
            app.base_url, org.id
        ))
        .header("Authorization", &bearer)
        .send()
        .await
        .expect("host-jwt GET sends");
    assert_eq!(
        list.status(),
        StatusCode::UNAUTHORIZED,
        "host JWT on org-admin GET is 401 (path-prefix gate from [D4])"
    );

    let del = app
        .client
        .delete(format!(
            "{}/api/v1/orgs/{}/worker-hosts/{}",
            app.base_url, org.id, host_id
        ))
        .header("Authorization", &bearer)
        .send()
        .await
        .expect("host-jwt DELETE sends");
    assert_eq!(
        del.status(),
        StatusCode::UNAUTHORIZED,
        "host JWT on org-admin DELETE is 401 (path-prefix gate from [D4])"
    );

    app.cleanup().await;
}

#[tokio::test]
async fn list_pagination_is_deterministic_with_identical_created_at() {
    // Three rows inserted with the exact same `created_at` would let
    // pagination duplicate or skip entries if the only ORDER BY were
    // `created_at`. The `host_id DESC` tie-breaker ([D12]) prevents that;
    // this test forces the collision and walks the pages.
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    // Insert three rows with one shared `created_at` (`shared_ts`) so
    // Postgres cannot break the tie by physical order. Each row is its own
    // INSERT statement but all three bind the same `created_at` literal,
    // which is what actually forces the tie — a single multi-VALUES INSERT
    // would behave identically here.
    let shared_ts = chrono::Utc::now();
    let mut ids: Vec<Uuid> = Vec::new();
    for _ in 0..3 {
        let id: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO worker_hosts (org_id, region, instance_type, created_at)
            VALUES ($1, 'us-central1', 'n2-standard-8', $2)
            RETURNING host_id
            "#,
        )
        .bind(org.id)
        .bind(shared_ts)
        .fetch_one(&app.db)
        .await
        .expect("seed host with shared created_at");
        ids.push(id);
    }

    // Sort the inserted IDs `DESC` so we know the expected page order.
    ids.sort_by(|a, b| b.cmp(a));

    // Page 1 (limit=2, offset=0) must return the two largest host_ids in
    // DESC order; page 2 (limit=2, offset=2) must return exactly the third.
    let page1 = client
        .get(&format!(
            "/api/v1/orgs/{}/worker-hosts?limit=2&offset=0",
            org.id
        ))
        .await;
    assert_eq!(page1.status(), StatusCode::OK);
    let body: Value = page1.json().await.unwrap();
    let data = response_data(&body);
    let p1_hosts = data["hosts"].as_array().unwrap();
    assert_eq!(p1_hosts.len(), 2);
    assert_eq!(data["total"], 3);
    let p1_ids: Vec<Uuid> = p1_hosts
        .iter()
        .map(|h| Uuid::parse_str(h["hostId"].as_str().unwrap()).unwrap())
        .collect();
    assert_eq!(p1_ids, vec![ids[0], ids[1]], "page 1 ordered host_id DESC");

    let page2 = client
        .get(&format!(
            "/api/v1/orgs/{}/worker-hosts?limit=2&offset=2",
            org.id
        ))
        .await;
    let body: Value = page2.json().await.unwrap();
    let data = response_data(&body);
    let p2_hosts = data["hosts"].as_array().unwrap();
    assert_eq!(p2_hosts.len(), 1, "page 2 returns the remaining row");
    let p2_id = Uuid::parse_str(p2_hosts[0]["hostId"].as_str().unwrap()).unwrap();
    assert_eq!(p2_id, ids[2], "page 2 is the smallest host_id of the three");

    // Sanity: no host appears on both pages — the actual symptom of an
    // unstable ORDER BY.
    let mut seen: Vec<Uuid> = p1_ids.clone();
    seen.push(p2_id);
    seen.sort();
    seen.dedup();
    assert_eq!(seen.len(), 3, "pagination must not duplicate or skip rows");

    app.cleanup().await;
}

// ─── PR1 runtime/profile contract (ontology [D42]) ───────────────────────────

/// A digest-pinned `baseImageRef` is validated, its `sha256:` digest extracted,
/// and BOTH the ref and the digest are persisted + echoed on the 201.
#[tokio::test]
async fn provision_persists_digest_pinned_base_image() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    let hex = "a".repeat(64);
    let reference = format!("ghcr.io/acme/app@sha256:{hex}");
    let data = provision_host(
        &client,
        org.id,
        json!({ "region": "us-central1", "baseImageRef": reference }),
    )
    .await;

    // Response echoes the persisted ref + extracted digest.
    assert_eq!(data["baseImageRef"], reference);
    assert_eq!(data["baseImageDigest"], format!("sha256:{hex}"));
    assert_eq!(data["baseImageSourceKind"], "pin");
    assert_eq!(data["baseImageSourceValue"], reference);
    // worker_profile defaults to fat-default and is always present.
    assert_eq!(data["workerProfile"], "fat-default");

    // DB row carries both columns.
    let host_id = Uuid::parse_str(data["hostId"].as_str().unwrap()).unwrap();
    let row = sqlx::query_as::<_, (Option<String>, Option<String>, String)>(
        "SELECT base_image_ref, base_image_digest, worker_profile \
         FROM worker_hosts WHERE host_id = $1",
    )
    .bind(host_id)
    .fetch_one(&app.db)
    .await
    .expect("provisioned row exists");
    assert_eq!(row.0.as_deref(), Some(reference.as_str()));
    assert_eq!(row.1.as_deref(), Some(format!("sha256:{hex}").as_str()));
    assert_eq!(row.2, "fat-default");

    app.cleanup().await;
}

/// A tag-only (non-digest) `baseImageRef` is rejected 422 BEFORE any row is
/// created — PR1 does not resolve tags.
#[tokio::test]
async fn provision_rejects_tag_only_base_image() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    let resp = client
        .post_json(
            &format!("/api/v1/orgs/{}/worker-hosts", org.id),
            &json!({ "region": "us-central1", "baseImageRef": "ubuntu:24.04" }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["error"]["code"], "VALIDATION_ERROR");
    assert_eq!(body["error"]["details"]["field"], "baseImageRef");

    // No row was created for this org (the reject is stateless, pre-DB).
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM worker_hosts WHERE org_id = $1")
        .bind(org.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(count, 0, "tag-only reject must not create a row");

    app.cleanup().await;
}

/// `workerProfile` defaults to `fat-default`, accepts `slim`, and rejects
/// off-allowlist values 422 with the `allowed` set surfaced.
#[tokio::test]
async fn provision_worker_profile_default_and_slim_and_invalid() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    // Omitted → fat-default (response + DB).
    let data = provision_host(&client, org.id, json!({ "region": "us-central1" })).await;
    assert_eq!(data["workerProfile"], "fat-default");
    let host_id = Uuid::parse_str(data["hostId"].as_str().unwrap()).unwrap();
    let profile: String =
        sqlx::query_scalar("SELECT worker_profile FROM worker_hosts WHERE host_id = $1")
            .bind(host_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(profile, "fat-default");

    // "slim" accepted + persisted.
    let data = provision_host(
        &client,
        org.id,
        json!({ "region": "us-east1", "workerProfile": "slim" }),
    )
    .await;
    assert_eq!(data["workerProfile"], "slim");
    let slim_id = Uuid::parse_str(data["hostId"].as_str().unwrap()).unwrap();
    let profile: String =
        sqlx::query_scalar("SELECT worker_profile FROM worker_hosts WHERE host_id = $1")
            .bind(slim_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(profile, "slim");

    // Off-allowlist → 422 with the allowed set.
    let resp = client
        .post_json(
            &format!("/api/v1/orgs/{}/worker-hosts", org.id),
            &json!({ "region": "us-central1", "workerProfile": "gpu-xl" }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["error"]["code"], "VALIDATION_ERROR");
    assert_eq!(body["error"]["details"]["field"], "workerProfile");
    let allowed = body["error"]["details"]["allowed"]
        .as_array()
        .expect("allowed array present");
    assert!(allowed.iter().any(|v| v == "fat-default"));
    assert!(allowed.iter().any(|v| v == "slim"));

    app.cleanup().await;
}

/// Missing request source and missing org policy fail before creating a row.
#[tokio::test]
async fn provision_without_image_source_fails_before_row_creation() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    let resp = client
        .post_json(
            &format!("/api/v1/orgs/{}/worker-hosts", org.id),
            &json!({ "region": "us-central1" }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(
        body["error"]["details"]["code"],
        "worker_image_source_required"
    );
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM worker_hosts WHERE org_id = $1")
        .bind(org.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(count, 0);

    app.cleanup().await;
}

// ---------------------------------------------------------------------------
// Org default base image fallback (ontology [D50]/[D51], migrations 133/134).
// ---------------------------------------------------------------------------

const ORG_DEFAULT_REF: &str = "us-central1-docker.pkg.dev/g5-admin-1ccc/g5/g5d-pairsoft-preview@sha256:3a469b618a38c9b8ce958f149832e581576e55924fdb4cdb3503ce2e9253ecfc";
const ORG_DEFAULT_DIGEST: &str =
    "sha256:3a469b618a38c9b8ce958f149832e581576e55924fdb4cdb3503ce2e9253ecfc";
const TRACK_REF_A: &str =
    "ghcr.io/g5labs/worker@sha256:1111111111111111111111111111111111111111111111111111111111111111";
const TRACK_REF_B: &str =
    "ghcr.io/g5labs/worker@sha256:2222222222222222222222222222222222222222222222222222222222222222";

#[tokio::test]
#[serial_test::serial(worker_image_catalog)]
async fn promoted_track_policy_resolves_each_host_once_and_preserves_provenance() {
    let catalog = format!("current={TRACK_REF_A}");
    let catalog_guard = EnvVarGuard::set_many(&[("G5_PROMOTED_BASE_IMAGES", &catalog)]);
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    let list = client
        .get(&format!("/api/v1/orgs/{}/worker-hosts", org.id))
        .await;
    assert_eq!(list.status(), StatusCode::OK);
    let list: Value = list.json().await.unwrap();
    assert_eq!(list["data"]["promotedImageTracks"][0]["name"], "current");
    assert_eq!(
        list["data"]["promotedImageTracks"][0]["imageRef"],
        TRACK_REF_A
    );

    let policy = client
        .put_json(
            &format!("/api/v1/orgs/{}/worker-hosts/image-policy", org.id),
            &json!({ "imageTrack": "current" }),
        )
        .await;
    assert_eq!(policy.status(), StatusCode::OK);
    let policy: Value = policy.json().await.unwrap();
    assert_eq!(policy["data"]["sourceKind"], "track");
    assert_eq!(policy["data"]["resolvedImageRef"], TRACK_REF_A);

    let first = provision_host_exact(&client, org.id, json!({ "region": "us-central1" })).await;
    assert_eq!(first["baseImageRef"], TRACK_REF_A);
    assert_eq!(first["baseImageSourceKind"], "track");
    assert_eq!(first["baseImageSourceValue"], "current");
    let first_id = Uuid::parse_str(first["hostId"].as_str().unwrap()).unwrap();

    drop(catalog_guard);
    let moved_catalog = format!("current={TRACK_REF_B}");
    let _moved_guard = EnvVarGuard::set_many(&[("G5_PROMOTED_BASE_IMAGES", &moved_catalog)]);
    let second = provision_host_exact(&client, org.id, json!({ "region": "us-east1" })).await;
    assert_eq!(second["baseImageRef"], TRACK_REF_B);
    assert_eq!(second["baseImageSourceValue"], "current");

    let historical: (String, String, String, String) = sqlx::query_as(
        "SELECT base_image_ref, base_image_digest, base_image_source_kind, base_image_source_value \
         FROM worker_hosts WHERE host_id = $1",
    )
    .bind(first_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(historical.0, TRACK_REF_A);
    assert_eq!(historical.1, format!("sha256:{}", "1".repeat(64)));
    assert_eq!(historical.2, "track");
    assert_eq!(historical.3, "current");

    app.cleanup().await;
}

#[tokio::test]
#[serial_test::serial(worker_image_catalog)]
async fn image_source_preflight_failures_create_no_host_rows() {
    let _guard = EnvVarGuard::set_many(&[("G5_PROMOTED_BASE_IMAGES", "")]);
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);
    for body in [
        json!({ "region": "us-central1" }),
        json!({ "region": "us-central1", "imageTrack": "missing" }),
        json!({ "region": "us-central1", "baseImageRef": "ubuntu:24.04" }),
        json!({
            "region": "us-central1",
            "imageTrack": "current",
            "baseImageRef": TRACK_REF_A
        }),
    ] {
        let response = client
            .post_json(&format!("/api/v1/orgs/{}/worker-hosts", org.id), &body)
            .await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }
    let policy = client
        .put_json(
            &format!("/api/v1/orgs/{}/worker-hosts/image-policy", org.id),
            &json!({ "imageTrack": "missing" }),
        )
        .await;
    assert_eq!(policy.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM worker_hosts WHERE org_id = $1")
        .bind(org.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(
        count, 0,
        "all image preflight failures are side-effect free"
    );
    app.cleanup().await;
}

/// Stamps the org's default the way a per-release pin migration does ([D50]).
async fn set_org_default(app: &TestApp, org_id: Uuid, default_ref: &str) {
    sqlx::query("UPDATE orgs SET default_base_image_ref = $1 WHERE id = $2")
        .bind(default_ref)
        .bind(org_id)
        .execute(&app.db)
        .await
        .expect("org default pin applies");
}

#[tokio::test]
async fn provision_falls_back_to_org_default_base_image() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);
    set_org_default(&app, org.id, ORG_DEFAULT_REF).await;

    // Omitted baseImageRef → the org default is applied as if supplied ([D51]).
    let data = provision_host_exact(&client, org.id, json!({ "region": "us-central1" })).await;
    assert_eq!(data["baseImageRef"], ORG_DEFAULT_REF);
    assert_eq!(data["baseImageDigest"], ORG_DEFAULT_DIGEST);

    // Blank string is treated exactly like omitted.
    let data = provision_host(
        &client,
        org.id,
        json!({ "region": "us-central1", "baseImageRef": "   " }),
    )
    .await;
    assert_eq!(data["baseImageRef"], ORG_DEFAULT_REF);

    // Persisted on the host row, not just echoed.
    let host_id = Uuid::parse_str(data["hostId"].as_str().unwrap()).unwrap();
    let row = sqlx::query_as::<_, (Option<String>, Option<String>)>(
        "SELECT base_image_ref, base_image_digest FROM worker_hosts WHERE host_id = $1",
    )
    .bind(host_id)
    .fetch_one(&app.db)
    .await
    .expect("row exists");
    assert_eq!(row.0.as_deref(), Some(ORG_DEFAULT_REF));
    assert_eq!(row.1.as_deref(), Some(ORG_DEFAULT_DIGEST));

    app.cleanup().await;
}

#[tokio::test]
async fn explicit_base_image_ref_wins_over_org_default() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);
    set_org_default(&app, org.id, ORG_DEFAULT_REF).await;

    let explicit = format!("reg.example.com/custom/image@sha256:{}", "b".repeat(64));
    let data = provision_host(
        &client,
        org.id,
        json!({ "region": "us-central1", "baseImageRef": explicit }),
    )
    .await;
    assert_eq!(data["baseImageRef"], explicit.as_str());

    app.cleanup().await;
}

#[tokio::test]
async fn list_envelope_carries_only_the_orgs_own_default() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    // No default yet → the field is absent (additive wire compat, [D51]).
    let resp = client
        .get(&format!("/api/v1/orgs/{}/worker-hosts", org.id))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = resp.json().await.unwrap();
    assert!(
        response_data(&body).get("orgDefaultBaseImageRef").is_none(),
        "field absent when the org has no default"
    );

    // With a default → the org's OWN ref rides the envelope. No other org's
    // image (and no image list) is exposed anywhere ([D50]).
    set_org_default(&app, org.id, ORG_DEFAULT_REF).await;
    let resp = client
        .get(&format!("/api/v1/orgs/{}/worker-hosts", org.id))
        .await;
    let body: Value = resp.json().await.unwrap();
    assert_eq!(
        response_data(&body)["orgDefaultBaseImageRef"],
        ORG_DEFAULT_REF
    );

    app.cleanup().await;
}

#[tokio::test]
async fn org_default_check_constraint_rejects_non_digest_values() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;

    // The migration-133 CHECK mirrors the full validate_base_image_ref grammar,
    // so malformed values fail at write time (never a 500 at provision time).
    let no_path_segment = format!("not-an-image@sha256:{}", "c".repeat(64));
    let uppercase_host = format!("UPPER.example.com/img@sha256:{}", "c".repeat(64));
    for bad in [
        "reg.example.com/image:just-a-tag",
        // Digest OK but the name has no path segment — a digest-suffix-only
        // CHECK shape would have accepted this (post-push Codex finding).
        no_path_segment.as_str(),
        uppercase_host.as_str(),
    ] {
        let err = sqlx::query("UPDATE orgs SET default_base_image_ref = $1 WHERE id = $2")
            .bind(bad)
            .bind(org.id)
            .execute(&app.db)
            .await
            .expect_err("malformed value must violate the CHECK");
        assert!(
            err.to_string()
                .contains("orgs_default_base_image_ref_digest_pinned"),
            "unexpected error for {bad}: {err}"
        );
    }

    // The full grammar still admits every legitimate shape, incl. host:port.
    sqlx::query("UPDATE orgs SET default_base_image_ref = $1 WHERE id = $2")
        .bind(format!("localhost:5000/team/img@sha256:{}", "d".repeat(64)))
        .bind(org.id)
        .execute(&app.db)
        .await
        .expect("valid host:port ref passes the CHECK");

    app.cleanup().await;
}

// ---------------------------------------------------------------------------
// Promoted-image tracking seed (ontology [D52], migration 135).
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial_test::serial(worker_image_catalog)]
async fn seed_updates_tracking_orgs_and_leaves_pins_alone() {
    use g5_api::services::worker_host_service::{parse_promoted_base_images, WorkerHostService};

    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let tracking_org = TestOrg::create_with_admin(&app, &mut user).await;
    let mut user2 = TestUser::create(&app).await;
    let pinned_org = TestOrg::create_with_admin(&app, &mut user2).await;

    let old_ref = format!(
        "us-central1-docker.pkg.dev/g5-admin-1ccc/g5/g5d-pairsoft-preview@sha256:{}",
        "0".repeat(64)
    );
    let pinned_ref = format!("reg.example.com/pinned/img@sha256:{}", "9".repeat(64));
    sqlx::query(
        "UPDATE orgs SET default_base_image_track = 'g5d-pairsoft-preview', \
         default_base_image_ref = $1 WHERE id = $2",
    )
    .bind(&old_ref)
    .bind(tracking_org.id)
    .execute(&app.db)
    .await
    .expect("tracking org setup");
    sqlx::query("UPDATE orgs SET default_base_image_ref = $1 WHERE id = $2")
        .bind(&pinned_ref)
        .bind(pinned_org.id)
        .execute(&app.db)
        .await
        .expect("pinned org setup");

    // A deploy promotes a new digest for pairsoft (+ one for pnc, untracked here).
    let promoted = format!(
        "us-central1-docker.pkg.dev/g5-admin-1ccc/g5/g5d-pairsoft-preview@sha256:{}",
        "1".repeat(64)
    );
    let env_value = format!(
        "{promoted},us-central1-docker.pkg.dev/g5-admin-1ccc/g5/g5d-pnc-bankworkflow@sha256:{}",
        "2".repeat(64)
    );
    let _catalog_guard = EnvVarGuard::set_many(&[("G5_PROMOTED_BASE_IMAGES", &env_value)]);
    let entries = parse_promoted_base_images(&env_value);
    assert_eq!(entries.len(), 2);
    let updated = WorkerHostService::new()
        .seed_tracked_org_defaults(&app.db, &entries)
        .await
        .expect("seed applies");
    assert_eq!(updated, 1, "exactly the tracking org row moved");

    let now: Option<String> =
        sqlx::query_scalar("SELECT default_base_image_ref FROM orgs WHERE id = $1")
            .bind(tracking_org.id)
            .fetch_one(&app.db)
            .await
            .expect("tracking org row");
    assert_eq!(now.as_deref(), Some(promoted.as_str()));

    // Manually pinned org (track NULL) is untouched.
    let still: Option<String> =
        sqlx::query_scalar("SELECT default_base_image_ref FROM orgs WHERE id = $1")
            .bind(pinned_org.id)
            .fetch_one(&app.db)
            .await
            .expect("pinned org row");
    assert_eq!(still.as_deref(), Some(pinned_ref.as_str()));

    // Idempotent: re-seeding the same entries changes nothing.
    let again = WorkerHostService::new()
        .seed_tracked_org_defaults(&app.db, &entries)
        .await
        .expect("re-seed");
    assert_eq!(again, 0);

    // End-to-end: a blank-ref provision on the tracking org now lands on the
    // seeded (promoted) digest via the [D50]/[D51] fallback, unchanged code.
    let client = app.authenticated_client(&user);
    let data =
        provision_host_exact(&client, tracking_org.id, json!({ "region": "us-central1" })).await;
    assert_eq!(data["baseImageRef"], promoted.as_str());

    app.cleanup().await;
}

#[tokio::test]
async fn track_column_check_rejects_non_name_values() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;

    for bad in ["UPPER", "has space", "-leading", "reg.example.com/full/ref"] {
        let err = sqlx::query("UPDATE orgs SET default_base_image_track = $1 WHERE id = $2")
            .bind(bad)
            .bind(org.id)
            .execute(&app.db)
            .await
            .expect_err("non-name value must violate the CHECK");
        assert!(
            err.to_string()
                .contains("orgs_default_base_image_track_name"),
            "unexpected error for {bad}: {err}"
        );
    }
    sqlx::query("UPDATE orgs SET default_base_image_track = 'g5d-pairsoft-preview' WHERE id = $1")
        .bind(org.id)
        .execute(&app.db)
        .await
        .expect("valid image name passes the CHECK");

    app.cleanup().await;
}

/// `g5.runtime.placement.runtime-identity`: a host whose published runtime
/// differs from the control plane's, or that published none, keeps its row
/// and liveness but receives no placement; when no current host has a free
/// slot the Start waits for capacity (which grows the fleet) instead of
/// failing on the host. The list reports each host's state, and a hello
/// stores exactly what the host published.
#[tokio::test]
async fn placement_skips_stale_and_unpublished_runtime_hosts_and_the_list_says_why() {
    use g5_api::services::worker_host_service::{
        PlacementOutcome, PlacementRequest, WorkerHostService,
    };
    use g5_api::ws::host_ctl::{ControlPlaneHostState, HostConnection};

    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let first = TestWorkspace::create(&app, &org).await;
    let second = TestWorkspace::create(&app, &org).await;
    seed_pending_start(&app, first.id, org.id).await;
    seed_pending_start(&app, second.id, org.id).await;

    let mut stale_identity = helpers::current_runtime_identity();
    stale_identity["module_catalog_version"] = json!("2026.01.01-older");
    stale_identity["module_catalog_hash"] = json!("0".repeat(64));
    let stale_host = Uuid::new_v4();
    let unpublished_host = Uuid::new_v4();
    let current_host = Uuid::new_v4();
    // The older hosts would win placement by age if they were eligible.
    sqlx::query(
        "INSERT INTO worker_hosts(
             host_id, org_id, region, status, last_heartbeat, slots_total,
             slot_size_reported, created_at, runtime_identity)
         VALUES
             ($1, $4, 'us-central1', 'ready', now(), 1, true, now() - interval '3 minutes', $5),
             ($2, $4, 'us-central1', 'ready', now(), 1, true, now() - interval '2 minutes', '{}'::jsonb),
             ($3, $4, 'us-central1', 'ready', now(), 1, true, now(), $6)",
    )
    .bind(stale_host)
    .bind(unpublished_host)
    .bind(current_host)
    .bind(org.id)
    .bind(&stale_identity)
    .bind(helpers::current_runtime_identity())
    .execute(&app.db)
    .await
    .expect("seed hosts");

    let service = WorkerHostService::new();
    let place = |workspace_id: Uuid| {
        let app = &app;
        let service = &service;
        async move {
            let mut db = app.scoped_db_for_org(org.id).await;
            let outcome = service
                .place(
                    &mut db,
                    PlacementRequest {
                        org_id: org.id,
                        workspace_id,
                        region: "us-central1",
                        reachable_host_ids: &[stale_host, unpublished_host, current_host],
                        excluded_host_ids: &[],
                        start_deadline_at: chrono::Utc::now() + chrono::TimeDelta::minutes(2),
                    },
                )
                .await
                .expect("place");
            db.commit().await.expect("commit placement");
            outcome
        }
    };
    let PlacementOutcome::Reserved(reserved) = place(first.id).await else {
        panic!("the current host has a free slot");
    };
    assert_eq!(
        reserved.host_id, current_host,
        "only the runtime-current host is eligible, despite being the youngest"
    );
    assert!(
        matches!(place(second.id).await, PlacementOutcome::NoCapacity),
        "stale and unpublished hosts are not free slots"
    );
    let reason: Value =
        sqlx::query_scalar("SELECT last_error FROM workspace_vms WHERE workspace_id = $1")
            .bind(second.id)
            .fetch_one(&app.db)
            .await
            .expect("load wait reason");
    assert_eq!(
        reason["reason"], "no_ready_host",
        "the Start waits for capacity instead of failing on a stale host"
    );

    // The Workers page learns which hosts take no VMs and why.
    let client = app.authenticated_client(&user);
    let resp = client
        .get(&format!("/api/v1/orgs/{}/worker-hosts", org.id))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = resp.json().await.unwrap();
    let state_of = |host: Uuid| {
        response_data(&body)["hosts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|h| h["hostId"] == host.to_string())
            .map(|h| h["runtimeState"].as_str().unwrap().to_string())
            .unwrap_or_else(|| panic!("host {host} listed"))
    };
    assert_eq!(state_of(stale_host), "stale");
    assert_eq!(state_of(unpublished_host), "unknown");
    assert_eq!(state_of(current_host), "current");

    // A hello stores what the host published: an older identity, then none.
    let backend = app.host_control_backend();
    let mut hello = measured_host_hello(stale_host);
    hello.slots_total = 1;
    hello.runtime =
        Some(serde_json::from_value(stale_identity.clone()).expect("stale identity decodes"));
    backend
        .accept_hello(stale_host, &hello, &HostConnection::new(true))
        .await
        .expect("stale hello accepted: custody and liveness are unaffected");
    let stored: Value =
        sqlx::query_scalar("SELECT runtime_identity FROM worker_hosts WHERE host_id = $1")
            .bind(stale_host)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(stored, stale_identity);
    hello.runtime = None;
    backend
        .accept_hello(stale_host, &hello, &HostConnection::new(true))
        .await
        .expect("legacy hello accepted");
    let stored: Value =
        sqlx::query_scalar("SELECT runtime_identity FROM worker_hosts WHERE host_id = $1")
            .bind(stale_host)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(stored, json!({}), "a hello without an identity stores none");
    // A current hello makes the host eligible again.
    hello.runtime = Some(g5_internal_api::host_ctl::HostRuntimeIdentity::bundled());
    backend
        .accept_hello(stale_host, &hello, &HostConnection::new(true))
        .await
        .expect("current hello accepted");
    let PlacementOutcome::Reserved(reserved) = place(second.id).await else {
        panic!("the re-imaged host has a free slot");
    };
    assert_eq!(reserved.host_id, stale_host);

    app.cleanup().await;
}

fn measured_host_hello(host_id: Uuid) -> g5_internal_api::host_ctl::HostHello {
    g5_internal_api::host_ctl::HostHello {
        host_id,
        protocol_version: g5_internal_api::host_ctl::PROTOCOL_VERSION,
        capabilities: Vec::new(),
        slots_total: 6,
        slot_size: Some(g5_internal_api::host_ctl::VmSizeSpec {
            cpus: 3,
            mem_mib: 13273,
            scratch_disk_mib: 65536,
        }),
        inventory: Vec::new(),
        retained_roots: Vec::new(),
        runtime: Some(g5_internal_api::host_ctl::HostRuntimeIdentity::bundled()),
    }
}

#[tokio::test]
async fn host_sizing_registration_is_the_admission_and_readiness_boundary() {
    use g5_api::services::worker_host_service::{
        PlacementOutcome, PlacementRequest, WorkerHostService,
    };
    use g5_api::ws::host_ctl::ControlPlaneHostState;
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    seed_pending_start(&app, workspace.id, org.id).await;
    let host_id = Uuid::new_v4();
    sqlx::query("INSERT INTO worker_hosts(host_id,org_id,region,status,last_heartbeat,cloud_instance_id) VALUES($1,$2,'us-central1','provisioning',now(),'sizing-test')")
        .bind(host_id).bind(org.id).execute(&app.db).await.unwrap();
    let transitioned: Option<Uuid> =
        sqlx::query_scalar("SELECT authz.transition_provisioning_worker_host($1,'ready')")
            .bind(host_id)
            .fetch_one(&app.state.system_pool)
            .await
            .unwrap();
    assert!(
        transitioned.is_none(),
        "cloud observation cannot qualify unresolved sizes"
    );
    sqlx::query("UPDATE worker_hosts SET status='unreachable' WHERE host_id=$1")
        .bind(host_id)
        .execute(&app.db)
        .await
        .unwrap();
    let backend = app.host_control_backend();
    backend
        .heartbeat(
            host_id,
            &g5_internal_api::host_ctl::HostHeartbeat {
                refresh_source_preparation: false,
                slots_in_use: 0,
                boots_in_progress: 0,
                load: 0.,
                memory_free_mib: 0,
                disk_free_mib: 0,
                vms: Vec::new(),
                retained_roots: Vec::new(),
            },
        )
        .await
        .unwrap();
    let status: String = sqlx::query_scalar("SELECT status FROM worker_hosts WHERE host_id=$1")
        .bind(host_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(status, "ready", "authenticated control restores liveness");
    // Liveness recovery does not qualify an unpublished host for placement.
    let service = WorkerHostService::new();
    let reachable_host_ids = [host_id];
    let request = || PlacementRequest {
        org_id: org.id,
        workspace_id: workspace.id,
        region: "us-central1",
        reachable_host_ids: &reachable_host_ids,
        excluded_host_ids: &[],
        start_deadline_at: chrono::Utc::now() + chrono::TimeDelta::minutes(2),
    };
    let mut db = app.scoped_db_for_org(org.id).await;
    assert_eq!(
        service.place(&mut db, request()).await.unwrap(),
        PlacementOutcome::NoCapacity
    );
    db.commit().await.unwrap();
    let active = g5_api::ws::host_ctl::HostConnection::new(true);
    let hello = measured_host_hello(host_id);
    let mut missing = hello.clone();
    missing.slot_size = None;
    assert!(backend
        .accept_hello(host_id, &missing, &active)
        .await
        .is_err());
    let mut old = hello.clone();
    old.protocol_version = 0;
    assert!(backend.accept_hello(host_id, &old, &active).await.is_err());
    backend
        .accept_hello(host_id, &hello, &active)
        .await
        .unwrap();
    let actual: (i32,i32,i32,i32,bool,String) = sqlx::query_as("SELECT slots_total,slot_cpus,slot_mem_mib,scratch_disk_mib,slot_size_reported,status FROM worker_hosts WHERE host_id=$1").bind(host_id).fetch_one(&app.db).await.unwrap();
    assert_eq!(actual, (6, 3, 13273, 65536, true, "ready".into()));
    let mut db = app.scoped_db_for_org(org.id).await;
    let PlacementOutcome::Reserved(reservation) = service.place(&mut db, request()).await.unwrap()
    else {
        panic!("reported host must admit workspace");
    };
    assert_eq!(reservation.host_id, host_id);
    db.commit().await.unwrap();
    // An identical report is idempotent with a durable starting reservation.
    backend
        .accept_hello(host_id, &hello, &active)
        .await
        .unwrap();
    app.cleanup().await;
}

#[tokio::test]
async fn host_sizing_cannot_change_durable_or_reported_vm_custody() {
    use g5_api::ws::host_ctl::ControlPlaneHostState;
    use g5_internal_api::host_ctl::{HostVmState, VmInventory};
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let host_id = Uuid::new_v4();
    let vm_id = Uuid::new_v4();
    sqlx::query("INSERT INTO worker_hosts(host_id,org_id,region) VALUES($1,$2,'us-central1')")
        .bind(host_id)
        .bind(org.id)
        .execute(&app.db)
        .await
        .unwrap();
    let backend = app.host_control_backend();
    let active = g5_api::ws::host_ctl::HostConnection::new(true);
    let mut hello = measured_host_hello(host_id);
    backend
        .accept_hello(host_id, &hello, &active)
        .await
        .unwrap();
    sqlx::query("INSERT INTO workspace_vms(org_id,workspace_id,host_id,vm_id,epoch,status,lease_expires_at,boot_spec) VALUES($1,$2,$3,$4,1,'starting',now()+interval '5 minutes',$5)")
        .bind(org.id).bind(workspace.id).bind(host_id).bind(vm_id.to_string()).bind(json!({"size":{"cpus":3,"mem_mib":13273,"scratch_disk_mib":65536}})).execute(&app.db).await.unwrap();
    hello.inventory.push(VmInventory {
        workspace_id: workspace.id,
        vm_id,
        state: HostVmState::Running,
        boot_at: chrono::Utc::now(),
    });
    for status in ["starting", "running", "stopping"] {
        sqlx::query("UPDATE workspace_vms SET status=$2 WHERE workspace_id=$1")
            .bind(workspace.id)
            .bind(status)
            .execute(&app.db)
            .await
            .unwrap();
        backend
            .accept_hello(host_id, &hello, &active)
            .await
            .unwrap();
        for field in 0..4 {
            let mut changed = hello.clone();
            match field {
                0 => changed.slots_total -= 1,
                1 => changed.slot_size.as_mut().unwrap().cpus += 1,
                2 => changed.slot_size.as_mut().unwrap().mem_mib += 1,
                _ => changed.slot_size.as_mut().unwrap().scratch_disk_mib += 1,
            }
            changed.inventory.clear(); // Durable custody alone blocks resizing.
            assert!(backend
                .accept_hello(host_id, &changed, &active)
                .await
                .is_err());
        }
        let persisted: Value =
            sqlx::query_scalar("SELECT boot_spec FROM workspace_vms WHERE workspace_id=$1")
                .bind(workspace.id)
                .fetch_one(&app.db)
                .await
                .unwrap();
        assert_eq!(persisted["size"]["mem_mib"], 13273);
    }
    sqlx::query("DELETE FROM workspace_vms WHERE workspace_id=$1")
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .unwrap();
    hello.slot_size.as_mut().unwrap().mem_mib -= 1;
    assert!(
        backend
            .accept_hello(host_id, &hello, &active)
            .await
            .is_err(),
        "reported live VM also retains custody"
    );
    hello.inventory.clear();
    backend
        .accept_hello(host_id, &hello, &active)
        .await
        .unwrap();
    app.cleanup().await;
}

#[tokio::test]
async fn replaced_host_connection_cannot_publish_after_waiting_for_admission_lock() {
    use g5_api::ws::host_ctl::ControlPlaneHostState;
    use std::sync::Arc;
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let host_id = Uuid::new_v4();
    sqlx::query("INSERT INTO worker_hosts(host_id,org_id,region) VALUES($1,$2,'us-central1')")
        .bind(host_id)
        .bind(org.id)
        .execute(&app.db)
        .await
        .unwrap();
    let backend = Arc::new(app.host_control_backend());
    let mut lock = app.db.begin().await.unwrap();
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1::text,0))")
        .bind(org.id.to_string())
        .execute(&mut *lock)
        .await
        .unwrap();
    let active = Arc::new(g5_api::ws::host_ctl::HostConnection::new(true));
    let stale = tokio::spawn({
        let backend = backend.clone();
        let active = active.clone();
        async move {
            backend
                .accept_hello(host_id, &measured_host_hello(host_id), &active)
                .await
        }
    });
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let waiting: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE wait_event='advisory' AND query LIKE 'SELECT pg_advisory_xact_lock%')").fetch_one(&app.db).await.unwrap();
            if waiting { break; }
            tokio::task::yield_now().await;
        }
    }).await.expect("hello reached the real admission lock");
    active.deactivate().await;
    lock.commit().await.unwrap();
    assert!(stale.await.unwrap().is_err());
    let reported: bool =
        sqlx::query_scalar("SELECT slot_size_reported FROM worker_hosts WHERE host_id=$1")
            .bind(host_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(!reported);
    let mut current = measured_host_hello(host_id);
    current.slot_size.as_mut().unwrap().mem_mib = 12000;
    backend
        .accept_hello(
            host_id,
            &current,
            &g5_api::ws::host_ctl::HostConnection::new(true),
        )
        .await
        .unwrap();
    assert!(backend
        .accept_hello(host_id, &measured_host_hello(host_id), &active)
        .await
        .is_err());
    let memory: i32 = sqlx::query_scalar("SELECT slot_mem_mib FROM worker_hosts WHERE host_id=$1")
        .bind(host_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(memory, 12000);
    app.cleanup().await;
}

#[tokio::test]
async fn resumed_workspace_boot_spec_uses_reported_cpu_and_exact_memory_mib() {
    use g5_internal_api::host_ctl::{method, VmSizeSpec};
    use helpers::TestWorkspaceRuntimePeer;
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    workspace.attach_parent(&app).await;
    let size = VmSizeSpec {
        cpus: 3,
        mem_mib: 13273,
        scratch_disk_mib: 65536,
    };
    let peer = TestWorkspaceRuntimePeer::attach_with_host_resources(
        &app,
        &workspace,
        user.id,
        6,
        size.clone(),
    )
    .await;
    sqlx::query("UPDATE workspace_vms SET status='stopped',host_id=NULL,vm_id=NULL,lease_expires_at=NULL WHERE workspace_id=$1")
        .bind(workspace.id).execute(&app.db).await.unwrap();
    let client = app.authenticated_client(&user);
    let response = client
        .post_json(
            &format!("/api/v1/workspaces/{}/resume", workspace.id),
            &json!({"operationId":Uuid::new_v4()}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let commands = peer.received_host_command_params(method::VM_START).await;
    assert_eq!(commands.len(), 1);
    assert_eq!(
        commands[0]["spec"]["size"],
        serde_json::to_value(&size).unwrap()
    );
    let boot: Value =
        sqlx::query_scalar("SELECT boot_spec FROM workspace_vms WHERE workspace_id=$1")
            .bind(workspace.id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(boot["size"], serde_json::to_value(&size).unwrap());
    let published: bool =
        sqlx::query_scalar("SELECT slot_size_reported FROM worker_hosts WHERE host_id=$1")
            .bind(peer.host_id())
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(published);
    let tunnel = app.state.preview_tunnels.get(&peer.host_id()).unwrap();
    drop(peer);
    tokio::time::timeout(Duration::from_secs(2), async {
        while tunnel.command_client().is_some() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("dropping the scripted peer deactivates its published tunnel");
    app.cleanup().await;
}

async fn send_host_rpc(
    writer: &g5_internal_api::stream_mux::MuxWriter,
    message: &g5_internal_api::host_ctl::RpcMessage,
) {
    let mut encoded = serde_json::to_vec(message).unwrap();
    encoded.push(b'\n');
    writer.send(&encoded).await.unwrap();
}

async fn read_host_rpc(
    reader: &mut g5_internal_api::stream_mux::MuxReader,
) -> g5_internal_api::host_ctl::RpcMessage {
    let line = tokio::time::timeout(Duration::from_secs(5), reader.read_line())
        .await
        .unwrap()
        .unwrap();
    serde_json::from_str(&line).unwrap()
}

#[tokio::test]
async fn legacy_host_reconnect_retains_leases_events_and_stop_control_without_placement() {
    use g5_api::ws::host_ctl::{serve_control_stream, ControlSession, HostConnection};
    use g5_internal_api::host_ctl::*;
    use g5_internal_api::stream_mux::{Channel, Mux};
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let host_id = Uuid::new_v4();
    let vm_id = Uuid::new_v4();
    sqlx::query("INSERT INTO worker_hosts(host_id,org_id,region,status) VALUES($1,$2,'us-central1','unreachable')").bind(host_id).bind(org.id).execute(&app.db).await.unwrap();
    sqlx::query("INSERT INTO workspace_vms(org_id,workspace_id,host_id,vm_id,epoch,status,lease_expires_at) VALUES($1,$2,$3,$4,1,'running',now()+interval '5 minutes')")
        .bind(org.id).bind(workspace.id).bind(host_id).bind(vm_id.to_string()).execute(&app.db).await.unwrap();
    let backend = std::sync::Arc::new(app.host_control_backend());
    let (api_io, host_io) = tokio::io::duplex(65536);
    let mut api_mux = Mux::new(api_io, true);
    let host_mux = Mux::new(host_io, false);
    let stream = host_mux.open(Channel::Ctl).await.unwrap();
    let api_stream = api_mux.accept().await.unwrap();
    let session = ControlSession::new();
    let commands = session.client.clone();
    let task = tokio::spawn(serve_control_stream(
        host_id,
        api_stream,
        backend.clone(),
        session,
        std::sync::Arc::new(HostConnection::new(true)),
    ));
    let (writer, mut reader) = stream.into_split();
    let mut hello = measured_host_hello(host_id);
    hello.protocol_version = 1;
    hello.slot_size = None;
    hello.inventory = vec![VmInventory {
        workspace_id: workspace.id,
        vm_id,
        state: HostVmState::Running,
        boot_at: chrono::Utc::now(),
    }];
    send_host_rpc(
        &writer,
        &RpcMessage::Request(RpcRequest::new("hello", method::HOST_HELLO, &hello).unwrap()),
    )
    .await;
    let RpcMessage::Response(response) = read_host_rpc(&mut reader).await else {
        panic!("hello response")
    };
    let HelloDisposition::Accept(accepted) = response.result_as().unwrap() else {
        panic!("legacy custody hello rejected")
    };
    assert_eq!(accepted.protocol_version, 1);
    let reported: bool =
        sqlx::query_scalar("SELECT slot_size_reported FROM worker_hosts WHERE host_id=$1")
            .bind(host_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(!reported);
    let status: String = sqlx::query_scalar("SELECT status FROM worker_hosts WHERE host_id=$1")
        .bind(host_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(
        status, "ready",
        "legacy reconnect restores recovery eligibility"
    );
    sqlx::query("UPDATE worker_hosts SET status='unreachable' WHERE host_id=$1")
        .bind(host_id)
        .execute(&app.db)
        .await
        .unwrap();
    send_host_rpc(
        &writer,
        &RpcMessage::Request(
            RpcRequest::new(
                "heartbeat",
                method::HOST_HEARTBEAT,
                &HostHeartbeat {
                    refresh_source_preparation: false,
                    slots_in_use: 1,
                    boots_in_progress: 0,
                    load: 0.,
                    memory_free_mib: 0,
                    disk_free_mib: 0,
                    retained_roots: Vec::new(),
                    vms: Vec::new(),
                },
            )
            .unwrap(),
        ),
    )
    .await;
    let RpcMessage::Response(response) = read_host_rpc(&mut reader).await else {
        panic!("heartbeat response")
    };
    response.result_as::<HeartbeatAck>().unwrap();
    let recovered: (String, bool) =
        sqlx::query_as("SELECT status,slot_size_reported FROM worker_hosts WHERE host_id=$1")
            .bind(host_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(
        recovered,
        ("ready".into(), false),
        "legacy heartbeat restores only liveness"
    );
    send_host_rpc(
        &writer,
        &RpcMessage::Request(
            RpcRequest::new(
                "renew",
                method::LEASE_RENEW,
                &LeaseRenew {
                    workspace_id: workspace.id,
                    vm_id,
                    epoch: 1,
                    seq: 1,
                },
            )
            .unwrap(),
        ),
    )
    .await;
    let RpcMessage::Response(response) = read_host_rpc(&mut reader).await else {
        panic!("lease response")
    };
    assert!(matches!(
        response.result_as::<LeaseRenewResult>().unwrap(),
        LeaseRenewResult::Granted(_)
    ));
    send_host_rpc(
        &writer,
        &RpcMessage::Notification(
            RpcNotification::new(
                method::VM_EVENT,
                &VmEvent {
                    workspace_id: workspace.id,
                    vm_id,
                    seq: 1,
                    kind: VmEventKind::Booted,
                    at: chrono::Utc::now(),
                    log_tail: None,
                },
            )
            .unwrap(),
        ),
    )
    .await;
    let RpcMessage::Notification(ack) = read_host_rpc(&mut reader).await else {
        panic!("event acknowledgement")
    };
    assert_eq!(ack.params::<VmEventAck>().unwrap().through_seq, 1);
    let stop = tokio::spawn(async move {
        commands
            .execute_for_test(
                method::VM_STOP,
                &CommandParams {
                    idempotency_key: "legacy-stop".into(),
                    budget_ms: 5000,
                    command: VmStop {
                        workspace_id: workspace.id,
                        vm_id,
                        mode: StopMode::Graceful,
                        grace_secs: 1,
                    },
                },
                Duration::from_secs(5),
            )
            .await
    });
    let RpcMessage::Request(request) = read_host_rpc(&mut reader).await else {
        panic!("stop request")
    };
    assert_eq!(request.method, method::VM_STOP);
    send_host_rpc(
        &writer,
        &RpcMessage::Response(
            RpcResponse::result(
                request.id,
                &CommandResult::Stopped {
                    exit: VmExit {
                        code: Some(0),
                        signal: None,
                    },
                },
            )
            .unwrap(),
        ),
    )
    .await;
    assert!(matches!(
        stop.await.unwrap().unwrap(),
        CommandResult::Stopped { .. }
    ));
    task.abort();
    let _ = task.await;
    app.cleanup().await;
}

async fn pause_host_publication_commit(
    app: &TestApp,
    host_id: Uuid,
) -> sqlx::Transaction<'static, sqlx::Postgres> {
    sqlx::raw_sql("CREATE FUNCTION pause_sizing_commit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_advisory_xact_lock(hashtextextended(NEW.host_id::text,1)); RETURN NEW; END $$; CREATE CONSTRAINT TRIGGER pause_sizing_commit AFTER UPDATE ON worker_hosts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION pause_sizing_commit();")
        .execute(&app.db).await.unwrap();
    let mut lock = app.db.begin().await.unwrap();
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1::text,1))")
        .bind(host_id.to_string())
        .execute(&mut *lock)
        .await
        .unwrap();
    lock
}

async fn wait_for_paused_host_commit(app: &TestApp) {
    tokio::time::timeout(Duration::from_secs(5),async {
        loop {
            let waiting: bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND wait_event='advisory' AND query LIKE 'COMMIT%')").fetch_one(&app.db).await.unwrap();
            if waiting {break;}
            tokio::task::yield_now().await;
        }
    }).await.expect("production publication reached COMMIT");
}

#[tokio::test]
async fn cancelled_registration_keeps_ownership_until_its_dispatched_commit_finishes() {
    use g5_api::ws::host_ctl::{ControlPlaneHostState, HostConnection};
    use std::sync::Arc;
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let host_id = Uuid::new_v4();
    sqlx::query("INSERT INTO worker_hosts(host_id,org_id,region) VALUES($1,$2,'us-central1')")
        .bind(host_id)
        .bind(org.id)
        .execute(&app.db)
        .await
        .unwrap();
    let lock = pause_host_publication_commit(&app, host_id).await;
    let backend = Arc::new(app.host_control_backend());
    let active = Arc::new(HostConnection::new(true));
    let registration = tokio::spawn({
        let active = active.clone();
        async move {
            backend
                .accept_hello(host_id, &measured_host_hello(host_id), &active)
                .await
        }
    });
    wait_for_paused_host_commit(&app).await;
    registration.abort();
    assert!(registration.await.unwrap_err().is_cancelled());
    let mut invalidation = Box::pin(active.deactivate());
    assert!(
        futures_util::poll!(&mut invalidation).is_pending(),
        "takeover must wait for the in-flight commit despite carrier cancellation"
    );
    lock.commit().await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), invalidation)
        .await
        .unwrap();
    assert!(!active.is_active());
    let size: (i32, bool) =
        sqlx::query_as("SELECT slot_mem_mib,slot_size_reported FROM worker_hosts WHERE host_id=$1")
            .bind(host_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(size, (13273, true));
    app.cleanup().await;
}

#[tokio::test]
async fn registry_takeover_cannot_replace_a_host_until_publication_commits() {
    use g5_internal_api::host_ctl::{method, RpcMessage, RpcRequest};
    use g5_internal_api::stream_mux::{Channel, Mux};
    use std::sync::Arc;
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let host_id = Uuid::new_v4();
    sqlx::query("INSERT INTO worker_hosts(host_id,org_id,region) VALUES($1,$2,'us-central1')")
        .bind(host_id)
        .bind(org.id)
        .execute(&app.db)
        .await
        .unwrap();
    app.state
        .preview_tunnels
        .set_control_backend(Arc::new(app.host_control_backend()));
    let lock = pause_host_publication_commit(&app, host_id).await;
    let (api_io, host_io) = tokio::io::duplex(65536);
    let host_mux = Mux::new(host_io, false);
    let first = tokio::spawn({
        let registry = app.state.preview_tunnels.clone();
        async move {
            registry
                .attach_test_mux(host_id, Mux::new(api_io, true))
                .await
        }
    });
    let stream = host_mux.open(Channel::Ctl).await.unwrap();
    let (writer, mut reader) = stream.into_split();
    send_host_rpc(
        &writer,
        &RpcMessage::Request(
            RpcRequest::new("hello", method::HOST_HELLO, &measured_host_hello(host_id)).unwrap(),
        ),
    )
    .await;
    wait_for_paused_host_commit(&app).await;
    let previous = app.state.preview_tunnels.get(&host_id).unwrap();
    let (next_api_io, next_host_io) = tokio::io::duplex(65536);
    let next_host_mux = Mux::new(next_host_io, false);
    let mut replacement = Box::pin({
        let registry = app.state.preview_tunnels.clone();
        async move {
            registry
                .attach_test_mux(host_id, Mux::new(next_api_io, true))
                .await
        }
    });
    // Poll the actual attach path to its first pending await, making takeover
    // ordering deterministic rather than relying on task scheduling/sleep.
    assert!(futures_util::poll!(&mut replacement).is_pending());
    assert!(Arc::ptr_eq(
        &previous,
        &app.state.preview_tunnels.get(&host_id).unwrap()
    ));
    lock.commit().await.unwrap();
    let _ = read_host_rpc(&mut reader).await;
    let second = tokio::spawn(replacement);
    let stream = next_host_mux.open(Channel::Ctl).await.unwrap();
    let (next_writer, mut next_reader) = stream.into_split();
    let mut current = measured_host_hello(host_id);
    current.slot_size.as_mut().unwrap().mem_mib = 12000;
    send_host_rpc(
        &next_writer,
        &RpcMessage::Request(RpcRequest::new("current", method::HOST_HELLO, &current).unwrap()),
    )
    .await;
    let _ = read_host_rpc(&mut next_reader).await;
    let memory: i32 = sqlx::query_scalar("SELECT slot_mem_mib FROM worker_hosts WHERE host_id=$1")
        .bind(host_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(memory, 12000);
    assert!(!Arc::ptr_eq(
        &previous,
        &app.state.preview_tunnels.get(&host_id).unwrap()
    ));
    app.state.preview_tunnels.terminate(&host_id).await;
    let _ = tokio::time::timeout(Duration::from_secs(5), first)
        .await
        .unwrap();
    let _ = tokio::time::timeout(Duration::from_secs(5), second)
        .await
        .unwrap();
    app.cleanup().await;
}

#[tokio::test]
async fn a_concurrent_placement_prevents_resizing_its_reserved_slot() {
    use g5_api::services::worker_host_service::{
        PlacementOutcome, PlacementRequest, WorkerHostService,
    };
    use g5_api::ws::host_ctl::{ControlPlaneHostState, HostConnection};
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    seed_pending_start(&app, workspace.id, org.id).await;
    let host_id = Uuid::new_v4();
    sqlx::query("INSERT INTO worker_hosts(host_id,org_id,region) VALUES($1,$2,'us-central1')")
        .bind(host_id)
        .bind(org.id)
        .execute(&app.db)
        .await
        .unwrap();
    let backend = std::sync::Arc::new(app.host_control_backend());
    backend
        .accept_hello(
            host_id,
            &measured_host_hello(host_id),
            &HostConnection::new(true),
        )
        .await
        .unwrap();
    let mut db = app.scoped_db_for_org(org.id).await;
    assert!(matches!(
        WorkerHostService::new()
            .place(
                &mut db,
                PlacementRequest {
                    org_id: org.id,
                    workspace_id: workspace.id,
                    region: "us-central1",
                    reachable_host_ids: &[host_id],
                    excluded_host_ids: &[],
                    start_deadline_at: chrono::Utc::now() + chrono::TimeDelta::minutes(2)
                }
            )
            .await
            .unwrap(),
        PlacementOutcome::Reserved(_)
    ));
    let resize = tokio::spawn(async move {
        let mut changed = measured_host_hello(host_id);
        changed.slot_size.as_mut().unwrap().mem_mib = 12000;
        backend
            .accept_hello(host_id, &changed, &HostConnection::new(true))
            .await
    });
    tokio::time::timeout(Duration::from_secs(5),async {
        loop {
            let waiting:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND wait_event='advisory' AND query LIKE 'SELECT pg_advisory_xact_lock%')").fetch_one(&app.db).await.unwrap();
            if waiting{break;}tokio::task::yield_now().await;
        }
    }).await.unwrap();
    db.commit().await.unwrap();
    assert!(resize.await.unwrap().is_err());
    let actual:(i32,String)=sqlx::query_as("SELECT host.slot_mem_mib,vm.status FROM worker_hosts host JOIN workspace_vms vm ON vm.host_id=host.host_id WHERE vm.workspace_id=$1").bind(workspace.id).fetch_one(&app.db).await.unwrap();
    assert_eq!(actual, (13273, "starting".into()));
    app.cleanup().await;
}

/// The drain route and the host reconciler deliver `host.drain` through the
/// placement owner's one delivery function: the same command and idempotency
/// key, over the host's live control tunnel.
#[tokio::test]
async fn drain_reaches_the_live_host_from_the_route_and_the_reconciler() {
    use g5_internal_api::host_ctl::method;
    use helpers::TestWorkspaceRuntimePeer;
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let peer = TestWorkspaceRuntimePeer::attach(&app, &workspace, user.id).await;
    let host_id = peer.host_id();

    let response = app
        .authenticated_client(&user)
        .post_json(
            &format!("/api/v1/orgs/{}/worker-hosts/{host_id}/drain", org.id),
            &json!({"reason":"delivery check"}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("parse drain response");
    let drain_id = response_data(&body)["drainId"]
        .as_str()
        .expect("drain id")
        .to_string();
    let command = json!({
        "idempotency_key": format!("host-drain:{drain_id}"),
        "budget_ms": 5_000,
        "drain_id": drain_id,
    });
    assert_eq!(
        peer.received_host_command_params(method::HOST_DRAIN).await,
        std::slice::from_ref(&command),
        "the route delivers the recorded drain before it answers"
    );

    g5_api_core::maintenance::host_registry::run_once(&app.state)
        .await
        .expect("reconcile host registry");
    assert_eq!(
        peer.received_host_command_params(method::HOST_DRAIN).await,
        [command.clone(), command],
        "the reconciler redelivers the durable drain with the same key"
    );

    app.cleanup().await;
}

/// Removing a host by force with a live VM is a VM loss. Like every other
/// loss path, it cancels the workspace's queued preview and withdraws its
/// undelivered platform notices in the transaction that releases the VM.
/// A running VM on a ready host, with a queued preview and its intent, and an
/// undelivered platform notice in a workspace conversation.
struct QueuedAgentsWork {
    workspace_id: Uuid,
    host_id: Uuid,
    vm_id: Uuid,
    preview_task_id: Uuid,
    notice_id: Uuid,
}

async fn seed_queued_agents_work(
    app: &TestApp,
    org_id: Uuid,
    workspace_id: Uuid,
    user_id: Uuid,
) -> QueuedAgentsWork {
    let host_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO worker_hosts(host_id, org_id, region, status, last_heartbeat, slot_size_reported, runtime_identity)
         VALUES ($1,$2,'us-central1','ready',now(),true,$3)",
    )
    .bind(host_id)
    .bind(org_id)
    .bind(helpers::current_runtime_identity())
    .execute(&app.db)
    .await
    .expect("insert host");
    let vm_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO workspace_vms(
             org_id, workspace_id, host_id, vm_id, epoch, status, lease_expires_at
         ) VALUES ($1,$2,$3,$4,1,'running',now() + interval '1 hour')",
    )
    .bind(org_id)
    .bind(workspace_id)
    .bind(host_id)
    .bind(vm_id.to_string())
    .execute(&app.db)
    .await
    .expect("insert running VM");
    let preview_task_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO tasks(
             id, org_id, workspace_id, name, kind, status, spec, attempt, created_by
         ) VALUES ($1,$2,$3,'Queued preview','preview','queued','{}'::jsonb,1,$4)",
    )
    .bind(preview_task_id)
    .bind(org_id)
    .bind(workspace_id)
    .bind(user_id)
    .execute(&app.db)
    .await
    .expect("queue preview");
    sqlx::query(
        "UPDATE workspace_vms
         SET preview_generation = 1,
             preview_intent = jsonb_build_object(
                 'generation',1,'action','start','status','queued',
                 'requested_by',$2::text,'task_id',$3::text)
         WHERE workspace_id = $1",
    )
    .bind(workspace_id)
    .bind(user_id)
    .bind(preview_task_id)
    .execute(&app.db)
    .await
    .expect("queue preview intent");
    let session_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO chat_sessions(id, org_id, workspace_id, scope_type, title, created_by)
         VALUES ($1,$2,$3,'workspace','Queued Agents work',$4)",
    )
    .bind(session_id)
    .bind(org_id)
    .bind(workspace_id)
    .bind(user_id)
    .execute(&app.db)
    .await
    .expect("insert session");
    let notice_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO chat_messages(
             id, org_id, session_id, role, content, artifacts, citations,
             status, message_index)
         VALUES ($1,$2,$3,'system','Workspace shutdown is scheduled.',
                 jsonb_build_object('platform_notice',jsonb_build_object(
                     'schema_version',1,'operation_id',$4::text,
                     'stop_reason','drain','delivery','queued')),
                 '[]'::jsonb,'complete',0)",
    )
    .bind(notice_id)
    .bind(org_id)
    .bind(session_id)
    .bind(Uuid::new_v4())
    .execute(&app.db)
    .await
    .expect("queue platform notice");
    QueuedAgentsWork {
        workspace_id,
        host_id,
        vm_id,
        preview_task_id,
        notice_id,
    }
}

/// The VM was released as failed, the queued preview was cancelled and its
/// intent settled, and the notice was withdrawn, all with `reason`.
async fn assert_agents_work_settled(app: &TestApp, work: &QueuedAgentsWork, reason: &str) {
    let vm: (String, Value) =
        sqlx::query_as("SELECT status, last_error FROM workspace_vms WHERE workspace_id = $1")
            .bind(work.workspace_id)
            .fetch_one(&app.db)
            .await
            .expect("load released VM");
    assert_eq!(vm.0, "failed");
    let preview: (String, Option<String>, Option<String>) = sqlx::query_as(
        "SELECT task.status, task.outcome ->> 'reason', vm.preview_intent ->> 'status'
         FROM tasks task JOIN workspace_vms vm ON vm.workspace_id = task.workspace_id
         WHERE task.id = $1",
    )
    .bind(work.preview_task_id)
    .fetch_one(&app.db)
    .await
    .expect("load preview after the loss");
    assert_eq!(
        preview,
        (
            "cancelled".to_string(),
            Some(reason.to_string()),
            Some("settled".to_string())
        )
    );
    let notice: (Option<String>, Option<String>) = sqlx::query_as(
        "SELECT artifacts #>> '{platform_notice,delivery}',
                artifacts #>> '{platform_notice,withdrawn_reason}'
         FROM chat_messages WHERE id = $1",
    )
    .bind(work.notice_id)
    .fetch_one(&app.db)
    .await
    .expect("load notice after the loss");
    assert_eq!(
        notice,
        (Some("withdrawn".to_string()), Some(reason.to_string()))
    );
}

#[tokio::test]
async fn forced_host_removal_settles_queued_previews_and_notices() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let work = seed_queued_agents_work(&app, org.id, workspace.id, user.id).await;

    let response = app
        .authenticated_client(&user)
        .delete(&format!(
            "/api/v1/orgs/{}/worker-hosts/{}?force=true",
            org.id, work.host_id
        ))
        .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let last_error: Value =
        sqlx::query_scalar("SELECT last_error FROM workspace_vms WHERE workspace_id = $1")
            .bind(workspace.id)
            .fetch_one(&app.db)
            .await
            .expect("load released VM");
    assert_eq!(last_error["reason"], "host_force_terminated");
    assert_agents_work_settled(&app, &work, "host_force_terminated").await;

    app.cleanup().await;
}

/// A VM exit the host reports settles the workspace's Agents work like any
/// other loss: host control applies the fact under the host's organization
/// (`g5.runtime.events.org-scope`), and the scope ends with the transaction.
#[tokio::test]
async fn host_reported_vm_exit_settles_queued_previews_and_notices() {
    use g5_api::ws::host_ctl::ControlPlaneHostState;
    use g5_internal_api::host_ctl::{VmEvent, VmEventKind};

    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let work = seed_queued_agents_work(&app, org.id, workspace.id, user.id).await;
    // One tenant connection, so the scope check below reads the connection
    // that applied the host fact.
    let scoped_pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect_with((*app.state.tenant_pool.connect_options()).clone())
        .await
        .expect("open a one-connection tenant pool");

    app.host_control_backend_with_pools(app.state.system_pool.clone(), scoped_pool.clone())
        .ingest_vm_event(
            work.host_id,
            &VmEvent {
                workspace_id: workspace.id,
                vm_id: work.vm_id,
                seq: 1,
                kind: VmEventKind::Exited {
                    code: Some(137),
                    signal: None,
                },
                at: chrono::Utc::now(),
                log_tail: None,
            },
        )
        .await
        .expect("ingest the host-reported exit");

    assert_agents_work_settled(&app, &work, "vm_lost").await;
    let scope: Option<Vec<Uuid>> = sqlx::query_scalar("SELECT authz.current_org_ids()")
        .fetch_one(&scoped_pool)
        .await
        .expect("read the connection's scope");
    assert_eq!(
        scope, None,
        "the organization scope ends with the transaction"
    );
    scoped_pool.close().await;
    app.cleanup().await;
}

/// Host hello's inventory reconciliation settles a missing VM's Agents work
/// under the host's organization, like a reported exit.
#[tokio::test]
async fn host_hello_without_the_vm_settles_queued_previews_and_notices() {
    use g5_api::ws::host_ctl::ControlPlaneHostState;
    use g5_internal_api::host_ctl::{HostHello, PROTOCOL_VERSION};

    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let work = seed_queued_agents_work(&app, org.id, workspace.id, user.id).await;
    sqlx::query("UPDATE worker_hosts SET slots_total = 1 WHERE host_id = $1")
        .bind(work.host_id)
        .execute(&app.db)
        .await
        .expect("match the hello's slot contract");

    app.host_control_backend()
        .accept_hello(
            work.host_id,
            &HostHello {
                host_id: work.host_id,
                protocol_version: PROTOCOL_VERSION,
                capabilities: Vec::new(),
                slots_total: 1,
                slot_size: Some(g5_internal_api::host_ctl::VmSizeSpec {
                    cpus: 2,
                    mem_mib: 4096,
                    scratch_disk_mib: 10240,
                }),
                inventory: Vec::new(),
                retained_roots: Vec::new(),
                runtime: Some(g5_internal_api::host_ctl::HostRuntimeIdentity::bundled()),
            },
            &g5_api::ws::host_ctl::HostConnection::new(true),
        )
        .await
        .expect("accept the host hello");

    assert_agents_work_settled(&app, &work, "host_inventory_missing").await;
    app.cleanup().await;
}

#[derive(Clone, Copy)]
enum Release {
    /// A reported exit first records its event, whose foreign key takes the
    /// workspace row before anything else.
    Exit,
    Hello,
    LeaseExpiry,
}

async fn wait_for_lock_waiters(app: &TestApp, expected: i64) {
    tokio::time::timeout(Duration::from_secs(10), async {
        while sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM pg_stat_activity
             WHERE datname = current_database() AND wait_event_type = 'Lock'",
        )
        .fetch_one(&app.db)
        .await
        .unwrap()
            < expected
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("{expected} transactions wait for a lock"));
}

/// Make `release` applicable to the seeded work, before any test lock is held.
async fn prepare_release(app: &TestApp, work: &QueuedAgentsWork, release: Release) {
    sqlx::query("UPDATE worker_hosts SET slots_total = 1 WHERE host_id = $1")
        .bind(work.host_id)
        .execute(&app.db)
        .await
        .expect("match the hello's slot contract");
    if matches!(release, Release::LeaseExpiry) {
        sqlx::query(
            "UPDATE workspace_vms SET lease_expires_at = now() - interval '1 minute'
             WHERE workspace_id = $1",
        )
        .bind(work.workspace_id)
        .execute(&app.db)
        .await
        .expect("expire the VM's lease");
    }
}

/// Start `release` for the seeded work.
async fn spawn_release(
    app: &TestApp,
    user: &TestUser,
    org_id: Uuid,
    work: &QueuedAgentsWork,
    release: Release,
) -> tokio::task::JoinHandle<Result<(), String>> {
    use g5_api::ws::host_ctl::ControlPlaneHostState;
    use g5_internal_api::host_ctl::{HostHello, VmEvent, VmEventKind, PROTOCOL_VERSION};

    let lifecycle_scope = {
        let db = app.scoped_db_for_user_org(user.id, org_id).await;
        let scope = db.reopener();
        db.rollback().await.expect("close captured lifecycle scope");
        scope
    };
    let lifecycle =
        g5_api::services::WorkspaceVmService::new(g5_api::services::WorkspaceVmServiceConfig {
            system_pool: app.state.system_pool.clone(),
            scope: lifecycle_scope,
            hosts: app.state.preview_tunnels.clone(),
            minter: app.state.jwt_minter.clone().expect("lifecycle JWT minter"),
            runtime: app.state.workspace_runtime.clone(),
            agent_work: app.state.agent_work.clone(),
            repo_storage: app.state.repo_storage.clone(),
            git_external_url: app.state.git_external_url.clone(),
            ws_hub: app.state.event_bus.ws_hub().clone(),
        });
    let backend = app.host_control_backend();
    let (host_id, vm_id, workspace_id) = (work.host_id, work.vm_id, work.workspace_id);
    tokio::spawn(async move {
        match release {
            Release::LeaseExpiry => lifecycle
                .reconcile_expired_leases()
                .await
                .map(|_| ())
                .map_err(|error| error.to_string()),
            Release::Hello => backend
                .accept_hello(
                    host_id,
                    &HostHello {
                        host_id,
                        protocol_version: PROTOCOL_VERSION,
                        capabilities: Vec::new(),
                        slots_total: 1,
                        slot_size: Some(g5_internal_api::host_ctl::VmSizeSpec {
                            cpus: 2,
                            mem_mib: 4096,
                            scratch_disk_mib: 10240,
                        }),
                        inventory: Vec::new(),
                        retained_roots: Vec::new(),
                        runtime: Some(g5_internal_api::host_ctl::HostRuntimeIdentity::bundled()),
                    },
                    &g5_api::ws::host_ctl::HostConnection::new(true),
                )
                .await
                .map(|_| ())
                .map_err(|error| error.to_string()),
            Release::Exit => backend
                .ingest_vm_event(
                    host_id,
                    &VmEvent {
                        workspace_id,
                        vm_id,
                        seq: 1,
                        kind: VmEventKind::Exited {
                            code: Some(137),
                            signal: None,
                        },
                        at: chrono::Utc::now(),
                        log_tail: None,
                    },
                )
                .await
                .map(|_| ())
                .map_err(|error| error.to_string()),
        }
    })
}

fn settled_reason(release: Release) -> &'static str {
    match release {
        Release::Hello => "host_inventory_missing",
        Release::Exit | Release::LeaseExpiry => "vm_lost",
    }
}

/// A release and the owner cancelling the workspace's queued preview take the
/// workspace row first, so neither can deadlock the other. The VM row is held
/// while the release and then the cancellation queue up; once it is free, the
/// release settles the preview and the cancellation finds it already
/// cancelled.
async fn assert_release_and_preview_cancellation_do_not_deadlock(release: Release) {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let work = seed_queued_agents_work(&app, org.id, workspace.id, user.id).await;
    prepare_release(&app, &work, release).await;
    // The lock a release's VM update takes.
    let mut holder = app.db.begin().await.expect("begin VM row holder");
    sqlx::query("SELECT 1 FROM workspace_vms WHERE workspace_id = $1 FOR NO KEY UPDATE")
        .bind(workspace.id)
        .execute(&mut *holder)
        .await
        .expect("hold the VM row");
    let released = spawn_release(&app, &user, org.id, &work, release).await;
    wait_for_lock_waiters(&app, 1).await;
    let client = app.authenticated_client(&user);
    let preview_task_id = work.preview_task_id;
    let cancel = tokio::spawn(async move {
        let response = client
            .post_json(
                &format!("/api/v1/tasks/{preview_task_id}/cancel"),
                &json!({}),
            )
            .await;
        let status = response.status();
        (status, response.text().await.unwrap_or_default())
    });
    wait_for_lock_waiters(&app, 2).await;
    holder.rollback().await.expect("free the VM row");

    tokio::time::timeout(Duration::from_secs(30), released)
        .await
        .expect("the release finishes")
        .expect("join the release")
        .expect("the release is not a deadlock victim");
    let (status, body) = tokio::time::timeout(Duration::from_secs(30), cancel)
        .await
        .expect("the cancellation finishes")
        .expect("join the cancellation");
    assert_eq!(
        status,
        StatusCode::OK,
        "the cancellation is not a deadlock victim: {body}"
    );
    assert_agents_work_settled(&app, &work, settled_reason(release)).await;
    app.cleanup().await;
}

#[tokio::test]
async fn host_reported_exit_and_preview_cancellation_do_not_deadlock() {
    assert_release_and_preview_cancellation_do_not_deadlock(Release::Exit).await;
}

#[tokio::test]
async fn host_hello_and_preview_cancellation_do_not_deadlock() {
    assert_release_and_preview_cancellation_do_not_deadlock(Release::Hello).await;
}

#[tokio::test]
async fn lease_expiry_and_preview_cancellation_do_not_deadlock() {
    assert_release_and_preview_cancellation_do_not_deadlock(Release::LeaseExpiry).await;
}

/// While an owner operation holds the workspace row, a release waits for it
/// without holding the VM row. Owner operations lock the workspace before its
/// VM, tasks and notices, and settlement locks tasks and notices, so this is
/// what keeps a release out of every lock cycle with them.
async fn assert_release_waits_for_the_workspace_before_its_vm(release: Release) {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let work = seed_queued_agents_work(&app, org.id, workspace.id, user.id).await;
    prepare_release(&app, &work, release).await;
    // The first lock of every owner task operation (`lock_task_workspace`).
    let mut owner = app.db.begin().await.expect("begin owner operation");
    sqlx::query("SELECT 1 FROM workspaces WHERE id = $1 FOR UPDATE")
        .bind(workspace.id)
        .execute(&mut *owner)
        .await
        .expect("hold the workspace row");
    let released = spawn_release(&app, &user, org.id, &work, release).await;
    wait_for_lock_waiters(&app, 1).await;
    let mut probe = app.db.begin().await.expect("begin VM probe");
    sqlx::query("SELECT 1 FROM workspace_vms WHERE workspace_id = $1 FOR NO KEY UPDATE NOWAIT")
        .bind(workspace.id)
        .execute(&mut *probe)
        .await
        .expect("the waiting release holds no lock on the VM row");
    probe.rollback().await.expect("end VM probe");
    owner.rollback().await.expect("free the workspace row");

    tokio::time::timeout(Duration::from_secs(30), released)
        .await
        .expect("the release finishes")
        .expect("join the release")
        .expect("the release succeeds");
    assert_agents_work_settled(&app, &work, settled_reason(release)).await;
    app.cleanup().await;
}

#[tokio::test]
async fn host_reported_exit_waits_for_the_workspace_before_its_vm() {
    assert_release_waits_for_the_workspace_before_its_vm(Release::Exit).await;
}

#[tokio::test]
async fn host_hello_waits_for_the_workspace_before_its_vm() {
    assert_release_waits_for_the_workspace_before_its_vm(Release::Hello).await;
}

#[tokio::test]
async fn lease_expiry_waits_for_the_workspace_before_its_vm() {
    assert_release_waits_for_the_workspace_before_its_vm(Release::LeaseExpiry).await;
}

/// A delayed exit for a VM that already stopped neither releases nor waits for
/// that VM: a transaction that locks the stopped VM row and then updates the
/// host row, which the event holds, would otherwise deadlock with it.
#[tokio::test]
async fn delayed_exit_does_not_wait_for_a_locked_stopped_vm() {
    use g5_api::ws::host_ctl::ControlPlaneHostState;
    use g5_internal_api::host_ctl::{VmEvent, VmEventKind};

    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let host_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO worker_hosts(host_id, org_id, region, status, last_heartbeat, slot_size_reported, runtime_identity)
         VALUES ($1,$2,'us-central1','ready',now(),true,$3)",
    )
    .bind(host_id)
    .bind(org.id)
    .bind(helpers::current_runtime_identity())
    .execute(&app.db)
    .await
    .expect("insert host");
    sqlx::query(
        "INSERT INTO workspace_vms(org_id, workspace_id, epoch, status)
         VALUES ($1,$2,1,'stopped')",
    )
    .bind(org.id)
    .bind(workspace.id)
    .execute(&app.db)
    .await
    .expect("insert a stopped VM");

    // A transaction that locks the VM row, then the host row.
    let mut holder = app.db.begin().await.expect("begin VM-row holder");
    sqlx::query("SELECT 1 FROM workspace_vms WHERE workspace_id = $1 FOR UPDATE")
        .bind(workspace.id)
        .execute(&mut *holder)
        .await
        .expect("the holder locks the stopped VM");
    let backend = app.host_control_backend();
    let workspace_id = workspace.id;
    let delayed = tokio::spawn(async move {
        backend
            .ingest_vm_event(
                host_id,
                &VmEvent {
                    workspace_id,
                    vm_id: Uuid::new_v4(),
                    seq: 1,
                    kind: VmEventKind::Exited {
                        code: Some(0),
                        signal: None,
                    },
                    at: chrono::Utc::now(),
                    log_tail: None,
                },
            )
            .await
            .map(|_| ())
            .map_err(|error| error.to_string())
    });
    // Wait until the event finished, or is waiting for a lock.
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let waiting: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM pg_stat_activity
                 WHERE datname = current_database() AND wait_event_type = 'Lock'",
            )
            .fetch_one(&app.db)
            .await
            .unwrap();
            if delayed.is_finished() || waiting > 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the delayed exit finishes or waits");
    sqlx::query("UPDATE worker_hosts SET last_heartbeat = now() WHERE host_id = $1")
        .bind(host_id)
        .execute(&mut *holder)
        .await
        .expect("the holder updates the host row");
    holder.commit().await.expect("the holder commits");

    tokio::time::timeout(Duration::from_secs(30), delayed)
        .await
        .expect("the delayed exit finishes")
        .expect("join the delayed exit")
        .expect("the delayed exit is not a deadlock victim");
    let status: String =
        sqlx::query_scalar("SELECT status FROM workspace_vms WHERE workspace_id = $1")
            .bind(workspace.id)
            .fetch_one(&app.db)
            .await
            .expect("load VM");
    assert_eq!(status, "stopped", "a delayed exit releases nothing");
    app.cleanup().await;
}

/// A replayed exit is only acknowledged again: it takes no workspace lock
/// while it holds the host row. An owner Stop holding the workspace row and
/// waiting for the VM row, and a transaction holding the VM row and then
/// updating the host row, would otherwise form a cycle with it.
#[tokio::test]
async fn exit_replay_waits_for_neither_an_owner_stop_nor_a_vm_row_holder() {
    use g5_api::ws::host_ctl::ControlPlaneHostState;
    use g5_internal_api::host_ctl::{VmEvent, VmEventKind};

    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let work = seed_queued_agents_work(&app, org.id, workspace.id, user.id).await;
    let backend = app.host_control_backend();
    let exit = VmEvent {
        workspace_id: workspace.id,
        vm_id: work.vm_id,
        seq: 1,
        kind: VmEventKind::Exited {
            code: Some(137),
            signal: None,
        },
        at: chrono::Utc::now(),
        log_tail: None,
    };
    backend
        .ingest_vm_event(work.host_id, &exit)
        .await
        .expect("record and apply the exit");

    // A transaction first locks the stopped VM row.
    let mut holder = app.db.begin().await.expect("begin VM-row holder");
    sqlx::query("SELECT 1 FROM workspace_vms WHERE workspace_id = $1 FOR UPDATE")
        .bind(workspace.id)
        .execute(&mut *holder)
        .await
        .expect("the holder locks the stopped VM");
    // The owner's Stop locks the workspace row, then waits for the VM row.
    let client = app.authenticated_client(&user);
    let workspace_id = workspace.id;
    let stop = tokio::spawn(async move {
        let response = client
            .post_json(
                &format!("/api/v1/workspaces/{workspace_id}/stop"),
                &json!({}),
            )
            .await;
        let status = response.status();
        (status, response.text().await.unwrap_or_default())
    });
    wait_for_lock_waiters(&app, 1).await;
    let host_id = work.host_id;
    let replay = tokio::spawn(async move {
        backend
            .ingest_vm_event(host_id, &exit)
            .await
            .map(|_| ())
            .map_err(|error| error.to_string())
    });
    // Wait until the replay finished, or is waiting for a lock too.
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let waiting: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM pg_stat_activity
                 WHERE datname = current_database() AND wait_event_type = 'Lock'",
            )
            .fetch_one(&app.db)
            .await
            .unwrap();
            if replay.is_finished() || waiting > 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the replay finishes or waits");
    sqlx::query("UPDATE worker_hosts SET last_heartbeat = now() WHERE host_id = $1")
        .bind(work.host_id)
        .execute(&mut *holder)
        .await
        .expect("the holder updates the host row");
    holder.commit().await.expect("the holder commits");

    tokio::time::timeout(Duration::from_secs(30), replay)
        .await
        .expect("the replay finishes")
        .expect("join the replay")
        .expect("the replay is not a deadlock victim");
    let (status, body) = tokio::time::timeout(Duration::from_secs(30), stop)
        .await
        .expect("the stop finishes")
        .expect("join the stop");
    assert_eq!(status, StatusCode::OK, "the owner's stop succeeds: {body}");
    assert_agents_work_settled(&app, &work, "vm_lost").await;
    app.cleanup().await;
}

/// The pre-change API decoded these known variants and reserialized them for
/// storage. Keep this decoder independent of the current protocol type: adding
/// a proof field to BootFailed must not silently pass this rolling-upgrade test.
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum LegacyTerminalEventKind {
    BootFailed {
        reason: String,
    },
    Exited {
        code: Option<i32>,
        signal: Option<i32>,
    },
}

#[tokio::test]
async fn boot_failure_replay_across_api_versions_preserves_exit_evidence() {
    for legacy_received_exit in [false, true] {
        assert_boot_failure_version_transition(legacy_received_exit).await;
    }
}

async fn assert_boot_failure_version_transition(legacy_received_exit: bool) {
    use g5_api::ws::host_ctl::{
        serve_control_stream, ControlPlaneHostState, ControlSession, HostConnection,
    };
    use g5_internal_api::host_ctl::*;
    use g5_internal_api::stream_mux::{Channel, Mux};

    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let host_id = Uuid::new_v4();
    let failed_vm = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO worker_hosts(host_id, org_id, region, status, last_heartbeat,
             slot_size_reported, slots_total, slot_cpus, slot_mem_mib, scratch_disk_mib,
             runtime_identity, reported_live_vm_ids)
         VALUES ($1,$2,'us-central1','ready',now(),true,6,3,13273,65536,$3,ARRAY[$4]::uuid[])",
    )
    .bind(host_id)
    .bind(org.id)
    .bind(helpers::current_runtime_identity())
    .bind(failed_vm)
    .execute(&app.db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO workspace_vms(
             org_id, workspace_id, host_id, vm_id, epoch, status, lease_expires_at)
         VALUES ($1,$2,$3,$4,1,'starting',now() + interval '5 minutes')",
    )
    .bind(org.id)
    .bind(workspace.id)
    .bind(host_id)
    .bind(failed_vm.to_string())
    .execute(&app.db)
    .await
    .unwrap();

    let backend = std::sync::Arc::new(app.host_control_backend());
    let failure = VmEvent {
        workspace_id: workspace.id,
        vm_id: failed_vm,
        seq: 1,
        kind: VmEventKind::BootFailed {
            reason: "kernel panic before dispatcher".to_string(),
        },
        at: chrono::Utc::now(),
        log_tail: Some("boot diagnostics".into()),
    };
    let exit = VmEvent {
        seq: 2,
        kind: VmEventKind::Exited {
            code: None,
            signal: None,
        },
        log_tail: None,
        ..failure.clone()
    };
    let events = [failure, exit];
    for event in events.iter().take(if legacy_received_exit { 2 } else { 1 }) {
        let wire = serde_json::to_value(&event.kind).unwrap();
        let legacy: LegacyTerminalEventKind = serde_json::from_value(wire.clone()).unwrap();
        let legacy_payload = serde_json::to_value(legacy).unwrap();
        assert_eq!(wire, legacy_payload, "older APIs must preserve every fact");
        let mut decoded = event.clone();
        decoded.kind = serde_json::from_value(legacy_payload).unwrap();
        // This is the old decode/storage boundary. Drop its acknowledgement,
        // leaving the host's original journal events pending across upgrade.
        backend.ingest_vm_event(host_id, &decoded).await.unwrap();
    }
    let (status, last_error): (String, Value) =
        sqlx::query_as("SELECT status, last_error FROM workspace_vms WHERE workspace_id=$1")
            .bind(workspace.id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(status, "failed");
    assert_eq!(
        last_error["kind"], "boot_failed",
        "exit must preserve diagnostics"
    );
    let inventory: Vec<Uuid> =
        sqlx::query_scalar("SELECT reported_live_vm_ids FROM worker_hosts WHERE host_id=$1")
            .bind(host_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(
        inventory,
        if legacy_received_exit {
            vec![]
        } else {
            vec![failed_vm]
        },
        "failure without an exit fact must retain uncertain occupancy"
    );

    // A newer incarnation of the same workspace must survive both the old
    // failure's replay and its separately sequenced exit evidence.
    let live_vm = Uuid::new_v4();
    sqlx::query(
        "UPDATE workspace_vms SET host_id=$2, vm_id=$3, epoch=2,
        status='running', lease_expires_at=now() + interval '5 minutes'
        WHERE workspace_id=$1",
    )
    .bind(workspace.id)
    .bind(host_id)
    .bind(live_vm.to_string())
    .execute(&app.db)
    .await
    .unwrap();
    let before: Value =
        sqlx::query_scalar("SELECT to_jsonb(vm) FROM workspace_vms vm WHERE workspace_id=$1")
            .bind(workspace.id)
            .fetch_one(&app.db)
            .await
            .unwrap();

    let (api_io, host_io) = tokio::io::duplex(65536);
    let mut api_mux = Mux::new(api_io, true);
    let host_mux = Mux::new(host_io, false);
    let stream = host_mux.open(Channel::Ctl).await.unwrap();
    let api_stream = api_mux.accept().await.unwrap();
    let task = tokio::spawn(serve_control_stream(
        host_id,
        api_stream,
        backend.clone(),
        ControlSession::new(),
        std::sync::Arc::new(HostConnection::new(true)),
    ));
    let (writer, mut reader) = stream.into_split();
    let mut hello = measured_host_hello(host_id);
    // Retain stale reported occupancy through hello so only the exit fact can
    // repair it. The newer live incarnation is in the same full snapshot.
    hello.inventory = [failed_vm, live_vm]
        .into_iter()
        .map(|vm_id| VmInventory {
            workspace_id: workspace.id,
            vm_id,
            state: HostVmState::Running,
            boot_at: chrono::Utc::now(),
        })
        .collect();
    send_host_rpc(
        &writer,
        &RpcMessage::Request(RpcRequest::new("hello", method::HOST_HELLO, &hello).unwrap()),
    )
    .await;
    let RpcMessage::Response(response) = read_host_rpc(&mut reader).await else {
        panic!("hello response");
    };
    assert!(matches!(
        response.result_as::<HelloDisposition>().unwrap(),
        HelloDisposition::Accept(_)
    ));
    for event in &events {
        send_host_rpc(
            &writer,
            &RpcMessage::Notification(RpcNotification::new(method::VM_EVENT, event).unwrap()),
        )
        .await;
        let RpcMessage::Notification(ack) = read_host_rpc(&mut reader).await else {
            panic!("replayed event acknowledgement");
        };
        assert_eq!(ack.method, method::VM_EVENT_ACK);
        assert_eq!(
            ack.params::<VmEventAck>().unwrap(),
            VmEventAck {
                vm_id: failed_vm,
                through_seq: if legacy_received_exit { 2 } else { event.seq },
            }
        );
    }
    send_host_rpc(
        &writer,
        &RpcMessage::Request(
            RpcRequest::new(
                "heartbeat-after-replay",
                method::HOST_HEARTBEAT,
                &HostHeartbeat {
                    refresh_source_preparation: false,
                    slots_in_use: 1,
                    boots_in_progress: 0,
                    load: 0.,
                    memory_free_mib: 0,
                    disk_free_mib: 0,
                    retained_roots: Vec::new(),
                    vms: Vec::new(),
                },
            )
            .unwrap(),
        ),
    )
    .await;
    let RpcMessage::Response(response) = read_host_rpc(&mut reader).await else {
        panic!("normal control traffic must continue after replay");
    };
    assert_eq!(response.id, "heartbeat-after-replay");
    response.result_as::<HeartbeatAck>().unwrap();
    let after: Value =
        sqlx::query_scalar("SELECT to_jsonb(vm) FROM workspace_vms vm WHERE workspace_id=$1")
            .bind(workspace.id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(
        before, after,
        "no duplicate settlement or mutation of newer custody"
    );
    let inventory: Vec<Uuid> =
        sqlx::query_scalar("SELECT reported_live_vm_ids FROM worker_hosts WHERE host_id=$1")
            .bind(host_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(
        inventory,
        vec![live_vm],
        "only proven old occupancy is removed"
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM worker_vm_events WHERE vm_id=$1")
        .bind(failed_vm.to_string())
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(count, 2, "replay never appends duplicate durable facts");

    for field in ["reason", "log_tail", "at", "exit_proof"] {
        let mut conflict = events[0].clone();
        match field {
            "reason" => {
                conflict.kind = VmEventKind::BootFailed {
                    reason: "changed".into(),
                }
            }
            "log_tail" => conflict.log_tail = Some("changed".into()),
            "at" => conflict.at += chrono::Duration::seconds(1),
            "exit_proof" => conflict.kind = events[1].kind.clone(),
            _ => unreachable!(),
        }
        let error = backend
            .ingest_vm_event(host_id, &conflict)
            .await
            .unwrap_err();
        assert!(
            error.to_string().contains("sequence replay conflicts"),
            "{field}: {error}"
        );
    }
    task.abort();
    let _ = task.await;
    app.cleanup().await;
}

#[tokio::test]
async fn disk_pressure_events_preserve_start_until_proven_exit_in_either_order() {
    use g5_api::ws::host_ctl::ControlPlaneHostState;
    use g5_internal_api::host_ctl::{VmEvent, VmEventKind};
    for (exit_first, legacy_exit) in [(false, false), (true, false), (false, true)] {
        let app = TestApp::spawn().await;
        let mut owner = TestUser::create(&app).await;
        let org = TestOrg::create_with_admin(&app, &mut owner).await;
        let workspace = TestWorkspace::create(&app, &org).await;
        let host = Uuid::new_v4();
        let vm = Uuid::new_v4();
        sqlx::query("INSERT INTO worker_hosts(host_id,org_id,region,status,last_heartbeat,reported_live_vm_ids) VALUES($1,$2,'us-central1','ready',now(),ARRAY[$3]::uuid[])")
            .bind(host).bind(org.id).bind(vm).execute(&app.db).await.unwrap();
        let intent = json!({"id":Uuid::new_v4(),"kind":"resume","owner_user_id":owner.id,"lifecycle_generation":0,"status":"starting","created_at":chrono::Utc::now(),"expires_at":chrono::Utc::now()+chrono::Duration::minutes(15),"attempts":1});
        sqlx::query("INSERT INTO workspace_vms(org_id,workspace_id,host_id,vm_id,epoch,status,start_intent,start_command_sent_at,lease_expires_at) VALUES($1,$2,$3,$4,1,'starting',$5,now(),now()+interval '5 minutes')")
            .bind(org.id).bind(workspace.id).bind(host).bind(vm.to_string()).bind(&intent).execute(&app.db).await.unwrap();
        let diagnostic = g5_contracts::error::HostDiskPressure {
            phase: "disk_admission".into(),
            available_bytes: 168363999232,
            required_bytes: 85899345920,
        }
        .reason();
        let failure = VmEvent {
            workspace_id: workspace.id,
            vm_id: vm,
            seq: 1,
            kind: VmEventKind::BootFailed {
                reason: diagnostic.clone(),
            },
            at: chrono::Utc::now(),
            log_tail: Some(diagnostic.clone()),
        };
        let exit = VmEvent {
            seq: 2,
            kind: VmEventKind::Exited {
                code: None,
                signal: None,
            },
            log_tail: if legacy_exit {
                None
            } else {
                Some(diagnostic.clone())
            },
            ..failure.clone()
        };
        let backend = app.host_control_backend();
        let ordered = if exit_first {
            [&exit, &failure]
        } else {
            [&failure, &exit]
        };
        backend.ingest_vm_event(host, ordered[0]).await.unwrap();
        if !exit_first {
            let status: String =
                sqlx::query_scalar("SELECT status FROM workspace_vms WHERE workspace_id=$1")
                    .bind(workspace.id)
                    .fetch_one(&app.db)
                    .await
                    .unwrap();
            assert_eq!(
                status, "starting",
                "a diagnostic alone cannot release process custody or Start intent"
            );
        }
        backend.ingest_vm_event(host, ordered[1]).await.unwrap();
        let (status, saved, error): (String, Value, Value) = sqlx::query_as(
            "SELECT status,start_intent,last_error FROM workspace_vms WHERE workspace_id=$1",
        )
        .bind(workspace.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
        assert_eq!(status, "stopped");
        assert_eq!(saved["status"], "queued_for_capacity");
        assert_eq!(saved["id"], intent["id"]);
        assert_eq!(error["diagnostic"], diagnostic);
        // Replay after a new boot cannot settle that newer incarnation.
        sqlx::query("UPDATE workspace_vms SET status='running',vm_id=$2,epoch=2,host_id=$3,lease_expires_at=now()+interval '5 minutes' WHERE workspace_id=$1").bind(workspace.id).bind(Uuid::new_v4().to_string()).bind(host).execute(&app.db).await.unwrap();
        backend.ingest_vm_event(host, &failure).await.unwrap();
        backend.ingest_vm_event(host, &exit).await.unwrap();
        let status: String =
            sqlx::query_scalar("SELECT status FROM workspace_vms WHERE workspace_id=$1")
                .bind(workspace.id)
                .fetch_one(&app.db)
                .await
                .unwrap();
        assert_eq!(status, "running");
        app.cleanup().await;
    }
}
