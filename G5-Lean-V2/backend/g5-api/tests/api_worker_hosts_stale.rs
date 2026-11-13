//! Integration tests for the stale-host detector ([D65]), the bounded
//! stockout retry ([D66]), and the provision-then-drain rotation ordering
//! ([D67]) — issues #782/#783/#1464/#1880.
//!
//! Fully offline against a real Postgres (`TestApp`) + `MockCloudProvider`:
//! `detect_once` / `rotate_once` are driven synchronously. Every test is
//! `#[serial]` because behavior is configured through process-global env vars.

use crate::helpers;

use std::sync::Arc;

use helpers::{wait_for_cloud_instance_id, TestApp, TestOrg, TestUser};
use reqwest::StatusCode;
use serde_json::{json, Value};
use serial_test::serial;
use uuid::Uuid;

use g5_api::cloud::rotation::{rotate_once, ROTATION_ENABLED_ENV};
use g5_api::cloud::stale_host::{
    detect_once, CLOUD_ORPHAN_GRACE_ENV, STALE_DETECTOR_ENABLED_ENV, STALE_DETECTOR_INTERVAL_ENV,
    STALE_PROVISIONING_GRACE_ENV, STALE_READY_HEARTBEAT_ENV,
};
use g5_api::cloud::{CloudProvider, InstanceState, ListedInstance, MockCall, MockCloudProvider};

const RETRY_ENABLED_ENV: &str = "G5_PROVISION_RETRY_ENABLED";
const RETRY_BUDGET_ENV: &str = "G5_PROVISION_RETRY_BUDGET_SECS";
const FALLBACK_REGIONS_ENV: &str = "G5_WORKER_FALLBACK_REGIONS";

/// Enable the detector (and optionally retry) for the duration of a test,
/// restoring the prior env on drop so test order can never leak config.
struct DetectorEnv {
    prior: Vec<(&'static str, Option<String>)>,
}

impl DetectorEnv {
    fn keys() -> [&'static str; 8] {
        [
            STALE_DETECTOR_ENABLED_ENV,
            STALE_DETECTOR_INTERVAL_ENV,
            STALE_PROVISIONING_GRACE_ENV,
            STALE_READY_HEARTBEAT_ENV,
            CLOUD_ORPHAN_GRACE_ENV,
            RETRY_ENABLED_ENV,
            RETRY_BUDGET_ENV,
            FALLBACK_REGIONS_ENV,
        ]
    }

    fn enabled() -> Self {
        let prior = Self::keys()
            .iter()
            .map(|k| (*k, std::env::var(k).ok()))
            .collect::<Vec<_>>();
        for k in Self::keys() {
            std::env::remove_var(k);
        }
        std::env::set_var(STALE_DETECTOR_ENABLED_ENV, "true");
        Self { prior }
    }

    fn with_retry(regions: &str) -> Self {
        let this = Self::enabled();
        std::env::set_var(RETRY_ENABLED_ENV, "true");
        std::env::set_var(FALLBACK_REGIONS_ENV, regions);
        this
    }
}

impl Drop for DetectorEnv {
    fn drop(&mut self) {
        for (k, v) in &self.prior {
            match v {
                Some(v) => std::env::set_var(k, v),
                None => std::env::remove_var(k),
            }
        }
    }
}

/// Enable rotation for the [D67] ordering test (restores prior env on drop).
struct RotationEnv {
    prior: Option<String>,
}

impl RotationEnv {
    fn enabled() -> Self {
        let prior = std::env::var(ROTATION_ENABLED_ENV).ok();
        std::env::set_var(ROTATION_ENABLED_ENV, "true");
        Self { prior }
    }
}

impl Drop for RotationEnv {
    fn drop(&mut self) {
        match &self.prior {
            Some(v) => std::env::set_var(ROTATION_ENABLED_ENV, v),
            None => std::env::remove_var(ROTATION_ENABLED_ENV),
        }
    }
}

fn response_data(body: &Value) -> &Value {
    body.get("data").expect("response body should include data")
}

async fn spawn_with_mock() -> (TestApp, Arc<MockCloudProvider>) {
    let mock = Arc::new(MockCloudProvider::new());
    let app = TestApp::spawn_with_cloud(mock.clone() as Arc<dyn CloudProvider>).await;
    (app, mock)
}

async fn legacy_control_hello(app: &TestApp, host_id: Uuid) {
    use g5_api::ws::host_ctl::{ControlPlaneHostState, HostConnection};
    let backend = app.host_control_backend();
    let accepted = backend
        .accept_hello(
            host_id,
            &g5_internal_api::host_ctl::HostHello {
                host_id,
                protocol_version: 1,
                capabilities: Vec::new(),
                slots_total: 1,
                slot_size: None,
                inventory: Vec::new(),
                retained_roots: Vec::new(),
                runtime: Some(g5_internal_api::host_ctl::HostRuntimeIdentity::bundled()),
            },
            &HostConnection::new(true),
        )
        .await
        .unwrap();
    assert_eq!(accepted.protocol_version, 1);
    assert!(!sqlx::query_scalar::<_, bool>(
        "SELECT slot_size_reported FROM worker_hosts WHERE host_id=$1"
    )
    .bind(host_id)
    .fetch_one(&app.db)
    .await
    .unwrap());
}

fn legacy_inventory_hello(
    host_id: Uuid,
    inventory: Vec<g5_internal_api::host_ctl::VmInventory>,
) -> g5_internal_api::host_ctl::HostHello {
    g5_internal_api::host_ctl::HostHello {
        host_id,
        protocol_version: 1,
        capabilities: Vec::new(),
        slots_total: 4,
        slot_size: None,
        inventory,
        retained_roots: Vec::new(),
        runtime: Some(g5_internal_api::host_ctl::HostRuntimeIdentity::bundled()),
    }
}

fn inventory_heartbeat(
    vms: &[(Uuid, g5_internal_api::host_ctl::HostVmState)],
) -> g5_internal_api::host_ctl::HostHeartbeat {
    g5_internal_api::host_ctl::HostHeartbeat {
        refresh_source_preparation: false,
        slots_in_use: vms.len() as u32,
        boots_in_progress: 0,
        load: 0.0,
        memory_free_mib: 0,
        disk_free_mib: 0,
        retained_roots: Vec::new(),
        vms: vms
            .iter()
            .map(|(vm_id, state)| g5_internal_api::host_ctl::VmHeartbeat {
                vm_id: *vm_id,
                state: *state,
                cpu_pct: 0.0,
                mem_mib: 0,
            })
            .collect(),
    }
}

async fn wait_for_host_row_waiter(app: &TestApp, query_pattern: &str) {
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let waiting: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM pg_stat_activity
                 WHERE datname=current_database() AND wait_event_type='Lock' AND query LIKE $1)",
            )
            .bind(query_pattern)
            .fetch_one(&app.db)
            .await
            .unwrap();
            if waiting {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("production transition is waiting for host row: {query_pattern}"));
}

/// Provision a host via the API and wait for the detached bring-up to persist
/// `cloud_instance_id`. Returns `(host_id, cloud_instance_id)`.
async fn provision_host(
    app: &TestApp,
    client: &helpers::AuthenticatedClient,
    org_id: Uuid,
) -> (Uuid, String) {
    let resp = client
        .post_json(
            &format!("/api/v1/orgs/{org_id}/worker-hosts"),
            &json!({
                "region": "us-central1",
                "instanceType": "n2-standard-8",
                "baseImageRef": helpers::TEST_WORKER_IMAGE_REF,
            }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::CREATED, "provision returns 201");
    let body: Value = resp.json().await.expect("provision response is JSON");
    let host_id =
        Uuid::parse_str(response_data(&body)["hostId"].as_str().unwrap()).expect("hostId is UUID");
    let cloud_instance_id = wait_for_cloud_instance_id(app, host_id).await;
    (host_id, cloud_instance_id)
}

/// Provision a host row via the API while the mock's bring-up fails — the row
/// stays `provisioning` with `cloud_instance_id IS NULL`. Returns the host id.
async fn provision_host_row_only(client: &helpers::AuthenticatedClient, org_id: Uuid) -> Uuid {
    let resp = client
        .post_json(
            &format!("/api/v1/orgs/{org_id}/worker-hosts"),
            &json!({
                "region": "us-central1",
                "instanceType": "n2-standard-8",
                "baseImageRef": helpers::TEST_WORKER_IMAGE_REF,
            }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::CREATED, "provision returns 201");
    let body: Value = resp.json().await.expect("provision response is JSON");
    Uuid::parse_str(response_data(&body)["hostId"].as_str().unwrap()).expect("hostId is UUID")
}

/// Wait until `predicate_sql` (a one-bind boolean query on the host row)
/// returns true — used to observe the detached bring-up's failure
/// classification / retry scheduling.
async fn wait_for_host_condition(app: &TestApp, host_id: Uuid, predicate_sql: &str, what: &str) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        // NULL-safe: `NULL = 'x'` is SQL NULL, not false — treat it as false.
        let ok: Option<bool> = sqlx::query_scalar(predicate_sql)
            .bind(host_id)
            .fetch_one(&app.db)
            .await
            .expect("host row exists");
        if ok.unwrap_or(false) {
            return;
        }
        if std::time::Instant::now() >= deadline {
            panic!("condition '{what}' never became true for host_id={host_id} within 5s");
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
}

async fn host_status(app: &TestApp, host_id: Uuid) -> (String, Option<String>, Option<Uuid>) {
    sqlx::query_as::<_, (String, Option<String>, Option<Uuid>)>(
        "SELECT status, provision_error_code, rotation_replaced_by \
         FROM worker_hosts WHERE host_id = $1",
    )
    .bind(host_id)
    .fetch_one(&app.db)
    .await
    .expect("host row exists")
}

// ─── Pass 1: never-registered reap (#782, contract D901) ────────────────────

/// `worker_hosts.created_by_user_id` for one host (migration 061).
async fn host_creator(app: &TestApp, host_id: Uuid) -> Option<Uuid> {
    sqlx::query_scalar("SELECT created_by_user_id FROM worker_hosts WHERE host_id = $1")
        .bind(host_id)
        .fetch_one(&app.db)
        .await
        .expect("host row exists")
}

#[tokio::test]
#[serial]
async fn detector_reaps_never_registered_host_and_replaces_once() {
    let _env = DetectorEnv::enabled();
    let (app, mock) = spawn_with_mock().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    let (host_id, cloud_instance_id) = provision_host(&app, &client, org.id).await;
    // Linked instance, no heartbeat, stale bring-up lease, past the 45min
    // grace — the #782 shape.
    sqlx::query(
        "UPDATE worker_hosts SET created_at = now() - interval '1 hour', \
             bringup_heartbeat_at = now() - interval '10 minutes' \
         WHERE host_id = $1",
    )
    .bind(host_id)
    .execute(&app.db)
    .await
    .expect("age the row");

    // A second host with a FRESH bring-up lease must never be selected even
    // when old — the [D60] lease means the bring-up is alive.
    let (fresh_lease_host, _) = provision_host(&app, &client, org.id).await;
    sqlx::query(
        "UPDATE worker_hosts SET created_at = now() - interval '1 hour', \
             bringup_heartbeat_at = now() \
         WHERE host_id = $1",
    )
    .bind(fresh_lease_host)
    .execute(&app.db)
    .await
    .expect("age the fresh-lease row");

    let pass = detect_once(&app.state).await;
    assert_eq!(
        pass.never_registered_reaped, 1,
        "exactly one host reaped: {pass:?}"
    );
    assert_eq!(pass.never_registered_replaced, 1, "replaced once: {pass:?}");
    assert_eq!(pass.errors, 0, "no errors: {pass:?}");

    let (status, code, replaced_by) = host_status(&app, host_id).await;
    assert_eq!(status, "terminated");
    assert_eq!(code.as_deref(), Some("never_registered"));
    let replacement_id = replaced_by.expect("replacement provisioned + marker stamped");
    assert!(
        mock.calls().contains(&MockCall::TerminateInstance {
            cloud_instance_id: cloud_instance_id.clone()
        }),
        "instance torn down"
    );
    // The replacement keeps naming who requested the original host.
    assert_eq!(host_creator(&app, replacement_id).await, Some(user.id));
    let (fresh_status, fresh_code, _) = host_status(&app, fresh_lease_host).await;
    assert_eq!(fresh_status, "provisioning", "fresh-lease host untouched");
    assert_eq!(fresh_code, None);

    // Replace-once: make the REPLACEMENT die never-registered too — no
    // second replacement (broken-image loop breaker, human decision
    // 547a4e7d).
    wait_for_cloud_instance_id(&app, replacement_id).await;
    sqlx::query(
        "UPDATE worker_hosts SET created_at = now() - interval '1 hour', \
             bringup_heartbeat_at = now() - interval '10 minutes' \
         WHERE host_id = $1",
    )
    .bind(replacement_id)
    .execute(&app.db)
    .await
    .expect("age the replacement");
    // An obsolete binary can keep its control carrier alive without ever
    // completing sizing registration. Heartbeats must not evade the bound.
    legacy_control_hello(&app, replacement_id).await;

    let pass = detect_once(&app.state).await;
    assert_eq!(pass.never_registered_reaped, 1, "{pass:?}");
    assert_eq!(
        pass.never_registered_replaced, 0,
        "second consecutive never_registered is NOT replaced: {pass:?}"
    );
    let (status, code, replaced_by) = host_status(&app, replacement_id).await;
    assert_eq!(status, "terminated");
    assert_eq!(code.as_deref(), Some("never_registered"));
    assert_eq!(replaced_by, None, "no third host in the chain");
    // A delayed qualified report cannot revive a registration already reaped.
    use g5_api::ws::host_ctl::{ControlPlaneHostState, HostConnection};
    let backend = app.host_control_backend();
    assert!(backend
        .accept_hello(
            replacement_id,
            &g5_internal_api::host_ctl::HostHello {
                host_id: replacement_id,
                protocol_version: g5_internal_api::host_ctl::PROTOCOL_VERSION,
                capabilities: Vec::new(),
                slots_total: 4,
                slot_size: Some(g5_internal_api::host_ctl::VmSizeSpec {
                    cpus: 2,
                    mem_mib: 4096,
                    scratch_disk_mib: 65536
                }),
                inventory: Vec::new(),
                retained_roots: Vec::new(),
                runtime: Some(g5_internal_api::host_ctl::HostRuntimeIdentity::bundled()),
            },
            &HostConnection::new(true)
        )
        .await
        .is_err());
    assert_eq!(host_status(&app, replacement_id).await.0, "terminated");
}

// ─── Pass 2: silent-ready reap (#783, contract D902) ────────────────────────

#[tokio::test]
#[serial]
async fn provisioning_grace_preserves_vm_custody_and_completed_publication() {
    use g5_api::ws::host_ctl::{ControlPlaneHostState, HostConnection};
    let _env = DetectorEnv::enabled();
    let (app, mock) = spawn_with_mock().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);
    let (occupied, _) = provision_host(&app, &client, org.id).await;
    legacy_control_hello(&app, occupied).await;
    let workspace = helpers::TestWorkspace::create(&app, &org).await;
    sqlx::query("INSERT INTO workspace_vms(org_id,workspace_id,host_id,vm_id,epoch,status,lease_expires_at) VALUES($1,$2,$3,$4,1,'running',now()+interval '5 minutes')")
        .bind(org.id).bind(workspace.id).bind(occupied).bind(Uuid::new_v4().to_string()).execute(&app.db).await.unwrap();
    let (published, _) = provision_host(&app, &client, org.id).await;
    let backend = app.host_control_backend();
    backend
        .accept_hello(
            published,
            &g5_internal_api::host_ctl::HostHello {
                host_id: published,
                protocol_version: g5_internal_api::host_ctl::PROTOCOL_VERSION,
                capabilities: Vec::new(),
                slots_total: 2,
                slot_size: Some(g5_internal_api::host_ctl::VmSizeSpec {
                    cpus: 2,
                    mem_mib: 4096,
                    scratch_disk_mib: 81920,
                }),
                inventory: Vec::new(),
                retained_roots: Vec::new(),
                runtime: Some(g5_internal_api::host_ctl::HostRuntimeIdentity::bundled()),
            },
            &HostConnection::new(true),
        )
        .await
        .unwrap();
    // Partial recovery can leave a published row awaiting its ready transition,
    // or legacy custody on an unpublished row. Neither may be timed out here.
    sqlx::query("UPDATE worker_hosts SET status='provisioning', created_at=now()-interval '1 hour', bringup_heartbeat_at=now()-interval '10 minutes' WHERE host_id=ANY($1)")
        .bind(vec![occupied,published]).execute(&app.db).await.unwrap();
    let pass = detect_once(&app.state).await;
    assert_eq!(pass.never_registered_reaped, 0, "{pass:?}");
    assert_eq!(host_status(&app, occupied).await.0, "provisioning");
    assert!(!mock
        .calls()
        .iter()
        .any(|call| matches!(call, MockCall::TerminateInstance { .. })));
}

#[tokio::test]
#[serial]
async fn provisioning_grace_preserves_reported_vm_without_durable_row() {
    use g5_api::ws::host_ctl::{ControlPlaneHostState, HostConnection};
    use g5_internal_api::host_ctl::{HostHello, HostVmState, VmInventory};

    let _env = DetectorEnv::enabled();
    let (app, mock) = spawn_with_mock().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);
    let (host_id, cloud_instance_id) = provision_host(&app, &client, org.id).await;
    let workspace = helpers::TestWorkspace::create(&app, &org).await;
    let backend = app.host_control_backend();
    backend
        .accept_hello(
            host_id,
            &HostHello {
                host_id,
                protocol_version: 1,
                capabilities: Vec::new(),
                slots_total: 1,
                slot_size: None,
                inventory: vec![VmInventory {
                    workspace_id: workspace.id,
                    vm_id: Uuid::new_v4(),
                    state: HostVmState::Running,
                    boot_at: chrono::Utc::now(),
                }],
                retained_roots: Vec::new(),
                runtime: Some(g5_internal_api::host_ctl::HostRuntimeIdentity::bundled()),
            },
            &HostConnection::new(true),
        )
        .await
        .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM workspace_vms WHERE host_id=$1")
            .bind(host_id)
            .fetch_one(&app.db)
            .await
            .unwrap(),
        0,
        "hello reconciliation does not create a durable incarnation for extra inventory"
    );
    sqlx::query("UPDATE worker_hosts SET created_at=now()-interval '1 hour', bringup_heartbeat_at=now()-interval '10 minutes' WHERE host_id=$1")
        .bind(host_id).execute(&app.db).await.unwrap();
    let pass = detect_once(&app.state).await;
    assert_eq!(
        pass.never_registered_reaped, 0,
        "reported VM retains custody: {pass:?}"
    );
    assert_eq!(host_status(&app, host_id).await.0, "provisioning");
    assert!(!mock
        .calls()
        .contains(&MockCall::TerminateInstance { cloud_instance_id }));
    // A protected older host must not hide a later eligible timeout candidate.
    let (empty_host, empty_instance) = provision_host(&app, &client, org.id).await;
    sqlx::query("UPDATE worker_hosts SET created_at=now()-interval '2 hours' WHERE host_id=$1")
        .bind(host_id)
        .execute(&app.db)
        .await
        .unwrap();
    sqlx::query("UPDATE worker_hosts SET created_at=now()-interval '1 hour', bringup_heartbeat_at=now()-interval '10 minutes' WHERE host_id=$1")
        .bind(empty_host).execute(&app.db).await.unwrap();
    assert_eq!(
        detect_once(&app.state).await.never_registered_reaped,
        1,
        "protected oldest host must not starve an eligible empty host"
    );
    assert_eq!(host_status(&app, host_id).await.0, "provisioning");
    assert_eq!(host_status(&app, empty_host).await.0, "terminated");
    assert!(mock.calls().contains(&MockCall::TerminateInstance {
        cloud_instance_id: empty_instance
    }));
}

#[tokio::test]
#[serial]
async fn provisioning_custody_survives_partial_exit_replay_and_stale_hello() {
    use g5_api::ws::host_ctl::{ControlPlaneHostState, HostConnection};
    use g5_internal_api::host_ctl::{HostVmState, VmEvent, VmEventKind, VmInventory};

    let _env = DetectorEnv::enabled();
    let (app, mock) = spawn_with_mock().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);
    let (host_id, cloud_instance_id) = provision_host(&app, &client, org.id).await;
    let workspace = helpers::TestWorkspace::create(&app, &org).await;
    let backend = app.host_control_backend();
    let ids = [
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
    ];
    let states = [
        HostVmState::Booting,
        HostVmState::Ready,
        HostVmState::Running,
        HostVmState::Stopping,
    ];
    backend
        .accept_hello(
            host_id,
            &legacy_inventory_hello(
                host_id,
                ids.iter()
                    .zip(states)
                    .map(|(vm_id, state)| VmInventory {
                        workspace_id: workspace.id,
                        vm_id: *vm_id,
                        state,
                        boot_at: chrono::Utc::now(),
                    })
                    .collect(),
            ),
            &HostConnection::new(true),
        )
        .await
        .unwrap();
    sqlx::query("UPDATE worker_hosts SET created_at=now()-interval '1 hour', bringup_heartbeat_at=now()-interval '10 minutes' WHERE host_id=$1")
        .bind(host_id).execute(&app.db).await.unwrap();
    assert_eq!(detect_once(&app.state).await.never_registered_reaped, 0);
    backend
        .heartbeat(host_id, &inventory_heartbeat(&[]))
        .await
        .unwrap();
    backend
        .heartbeat(
            host_id,
            &inventory_heartbeat(&[(ids[0], HostVmState::Exited)]),
        )
        .await
        .unwrap();
    let exit = VmEvent {
        workspace_id: workspace.id,
        vm_id: ids[1],
        seq: 1,
        kind: VmEventKind::Exited {
            code: Some(0),
            signal: None,
        },
        at: chrono::Utc::now(),
        log_tail: None,
    };
    backend.ingest_vm_event(host_id, &exit).await.unwrap();
    backend.ingest_vm_event(host_id, &exit).await.unwrap();
    backend
        .ingest_vm_event(
            host_id,
            &VmEvent {
                workspace_id: workspace.id,
                vm_id: ids[2],
                seq: 1,
                kind: VmEventKind::BootFailed {
                    reason: "child exit remains unproven".into(),
                },
                at: chrono::Utc::now(),
                log_tail: None,
            },
        )
        .await
        .unwrap();
    assert!(backend
        .accept_hello(
            host_id,
            &legacy_inventory_hello(host_id, Vec::new()),
            &HostConnection::new(false)
        )
        .await
        .is_err());
    let retained: Vec<Uuid> =
        sqlx::query_scalar("SELECT reported_live_vm_ids FROM worker_hosts WHERE host_id=$1")
            .bind(host_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(retained.len(), 2);
    assert!(retained.contains(&ids[2]) && retained.contains(&ids[3]));
    assert_eq!(detect_once(&app.state).await.never_registered_reaped, 0);
    assert!(!mock.calls().contains(&MockCall::TerminateInstance {
        cloud_instance_id: cloud_instance_id.clone()
    }));
    // A current full snapshot can prove that the remaining children exited.
    backend
        .accept_hello(
            host_id,
            &legacy_inventory_hello(
                host_id,
                ids[2..]
                    .iter()
                    .map(|vm_id| VmInventory {
                        workspace_id: workspace.id,
                        vm_id: *vm_id,
                        state: HostVmState::Exited,
                        boot_at: chrono::Utc::now(),
                    })
                    .collect(),
            ),
            &HostConnection::new(true),
        )
        .await
        .unwrap();
    assert_eq!(detect_once(&app.state).await.never_registered_reaped, 1);
    assert!(mock
        .calls()
        .contains(&MockCall::TerminateInstance { cloud_instance_id }));
}

#[tokio::test]
#[serial]
async fn provisioning_grace_preserves_heartbeat_only_vm_custody() {
    use g5_api::ws::host_ctl::{ControlPlaneHostState, HostConnection};
    use g5_internal_api::host_ctl::HostVmState;

    let _env = DetectorEnv::enabled();
    let (app, mock) = spawn_with_mock().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);
    let (host_id, cloud_instance_id) = provision_host(&app, &client, org.id).await;
    let backend = app.host_control_backend();
    backend
        .accept_hello(
            host_id,
            &legacy_inventory_hello(host_id, Vec::new()),
            &HostConnection::new(true),
        )
        .await
        .unwrap();
    let vm_id = Uuid::new_v4();
    backend
        .heartbeat(
            host_id,
            &inventory_heartbeat(&[(vm_id, HostVmState::Running)]),
        )
        .await
        .unwrap();
    backend
        .heartbeat(host_id, &inventory_heartbeat(&[]))
        .await
        .unwrap();
    sqlx::query("UPDATE worker_hosts SET created_at=now()-interval '1 hour', bringup_heartbeat_at=now()-interval '10 minutes' WHERE host_id=$1")
        .bind(host_id).execute(&app.db).await.unwrap();
    assert_eq!(detect_once(&app.state).await.never_registered_reaped, 0);
    // Contradictory partial reports must conservatively keep the live entry.
    backend
        .heartbeat(
            host_id,
            &inventory_heartbeat(&[(vm_id, HostVmState::Exited), (vm_id, HostVmState::Stopping)]),
        )
        .await
        .unwrap();
    assert_eq!(detect_once(&app.state).await.never_registered_reaped, 0);
    backend
        .heartbeat(
            host_id,
            &inventory_heartbeat(&[(vm_id, HostVmState::Exited)]),
        )
        .await
        .unwrap();
    assert_eq!(detect_once(&app.state).await.never_registered_reaped, 1);
    assert!(mock
        .calls()
        .contains(&MockCall::TerminateInstance { cloud_instance_id }));
}

#[tokio::test]
#[serial]
async fn provisioning_reap_cas_rechecks_inventory_published_after_selection() {
    use g5_api::ws::host_ctl::{ControlPlaneHostState, HostConnection};
    use g5_internal_api::host_ctl::{HostVmState, VmInventory};

    let _env = DetectorEnv::enabled();
    let (app, mock) = spawn_with_mock().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);
    let (host_id, cloud_instance_id) = provision_host(&app, &client, org.id).await;
    let workspace = helpers::TestWorkspace::create(&app, &org).await;
    sqlx::query("UPDATE worker_hosts SET created_at=now()-interval '1 hour', bringup_heartbeat_at=now()-interval '10 minutes' WHERE host_id=$1")
        .bind(host_id).execute(&app.db).await.unwrap();
    let mut lock = app.db.begin().await.unwrap();
    sqlx::query("SELECT host_id FROM worker_hosts WHERE host_id=$1 FOR UPDATE")
        .bind(host_id)
        .execute(&mut *lock)
        .await
        .unwrap();
    // The tiny test system pool has two connections; the detector holds one
    // for its single-flight lock and needs the other for its reads. Give the
    // blocked control transaction a separate pool with the same restricted role.
    let control_pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect_with((*app.state.system_pool.connect_options()).clone())
        .await
        .unwrap();
    let backend = app.host_control_backend_with_pool(control_pool.clone());
    let registration = tokio::spawn(async move {
        backend
            .accept_hello(
                host_id,
                &legacy_inventory_hello(
                    host_id,
                    vec![VmInventory {
                        workspace_id: workspace.id,
                        vm_id: Uuid::new_v4(),
                        state: HostVmState::Running,
                        boot_at: chrono::Utc::now(),
                    }],
                ),
                &HostConnection::new(true),
            )
            .await
    });
    wait_for_host_row_waiter(&app, "SELECT slots_total%FOR UPDATE%").await;
    let detector = tokio::spawn({
        let state = app.state.clone();
        async move { detect_once(&state).await }
    });
    wait_for_host_row_waiter(&app, "UPDATE worker_hosts%never_registered%").await;
    lock.commit().await.unwrap();
    registration.await.unwrap().unwrap();
    let pass = detector.await.unwrap();
    assert_eq!(
        pass.never_registered_reaped, 0,
        "CAS must retain newly reported custody: {pass:?}"
    );
    assert_eq!(host_status(&app, host_id).await.0, "provisioning");
    assert!(!mock
        .calls()
        .contains(&MockCall::TerminateInstance { cloud_instance_id }));
    control_pool.close().await;
}

#[tokio::test]
#[serial]
async fn custody_migration_preserves_unknown_inventory_until_current_hello() {
    use g5_api::ws::host_ctl::{ControlPlaneHostState, HostConnection};

    let _env = DetectorEnv::enabled();
    let (app, mock) = spawn_with_mock().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);
    let (legacy, legacy_instance) = provision_host(&app, &client, org.id).await;
    let (empty, _) = provision_host(&app, &client, org.id).await;
    sqlx::query("UPDATE worker_hosts SET last_heartbeat=now() WHERE host_id=$1")
        .bind(legacy)
        .execute(&app.db)
        .await
        .unwrap();
    // Exercise the actual additive migration against rows written by the old
    // control-plane schema, which had no persistent inventory evidence.
    sqlx::raw_sql("DROP TRIGGER worker_hosts_liveness_invalidates_inventory ON worker_hosts; DROP FUNCTION public.g5_host_liveness_invalidates_inventory(); ALTER TABLE worker_hosts DROP COLUMN reported_live_vm_ids")
        .execute(&app.db)
        .await
        .unwrap();
    sqlx::raw_sql(include_str!(
        "../src/db/migrations/047_host_reported_vm_custody.sql"
    ))
    .execute(&app.db)
    .await
    .unwrap();
    sqlx::raw_sql(include_str!(
        "../src/db/migrations/048_host_custody_legacy_writers.sql"
    ))
    .execute(&app.db)
    .await
    .unwrap();
    let reported: Option<Vec<Uuid>> =
        sqlx::query_scalar("SELECT reported_live_vm_ids FROM worker_hosts WHERE host_id=$1")
            .bind(legacy)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(
        reported.is_none(),
        "prior inventory is unknown, not known empty"
    );
    let never_registered: Vec<Uuid> =
        sqlx::query_scalar("SELECT reported_live_vm_ids FROM worker_hosts WHERE host_id=$1")
            .bind(empty)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(never_registered.is_empty());
    let backend = app.host_control_backend();
    backend
        .heartbeat(legacy, &inventory_heartbeat(&[]))
        .await
        .unwrap();
    sqlx::query("UPDATE worker_hosts SET created_at=now()-interval '1 hour', bringup_heartbeat_at=now()-interval '10 minutes' WHERE host_id=ANY($1)")
        .bind(vec![legacy,empty]).execute(&app.db).await.unwrap();
    sqlx::query("UPDATE worker_hosts SET created_at=now()-interval '2 hours' WHERE host_id=$1")
        .bind(legacy)
        .execute(&app.db)
        .await
        .unwrap();
    assert_eq!(detect_once(&app.state).await.never_registered_reaped, 1);
    assert_eq!(host_status(&app, legacy).await.0, "provisioning");
    assert_eq!(host_status(&app, empty).await.0, "terminated");
    assert!(!mock.calls().contains(&MockCall::TerminateInstance {
        cloud_instance_id: legacy_instance.clone()
    }));
    backend
        .accept_hello(
            legacy,
            &legacy_inventory_hello(legacy, Vec::new()),
            &HostConnection::new(true),
        )
        .await
        .unwrap();
    assert_eq!(detect_once(&app.state).await.never_registered_reaped, 1);
    assert!(mock.calls().contains(&MockCall::TerminateInstance {
        cloud_instance_id: legacy_instance
    }));
}

#[tokio::test]
#[serial]
async fn legacy_insert_after_migration_does_not_assert_empty_inventory() {
    let _env = DetectorEnv::enabled();
    let (app, mock) = spawn_with_mock().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let host_id = Uuid::new_v4();
    // Old provisioning SQL omits the newly added custody column, including
    // when its replica inserts the row after the migration has completed.
    sqlx::query("INSERT INTO worker_hosts(host_id,org_id,region,instance_type,cloud_instance_id,base_image_ref) VALUES($1,$2,'us-central1','n2-standard-8',$3,$4)")
        .bind(host_id).bind(org.id).bind(format!("g5-host-{host_id}")).bind(helpers::TEST_WORKER_IMAGE_REF).execute(&app.db).await.unwrap();
    let reported: Option<Vec<Uuid>> =
        sqlx::query_scalar("SELECT reported_live_vm_ids FROM worker_hosts WHERE host_id=$1")
            .bind(host_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(
        reported.is_none(),
        "legacy insertion does not establish known-empty custody"
    );
    sqlx::query("UPDATE worker_hosts SET last_heartbeat=now(), created_at=now()-interval '1 hour', bringup_heartbeat_at=now()-interval '10 minutes' WHERE host_id=$1")
        .bind(host_id).execute(&app.db).await.unwrap();
    assert_eq!(detect_once(&app.state).await.never_registered_reaped, 0);
    assert_eq!(host_status(&app, host_id).await.0, "provisioning");
    assert!(!mock
        .calls()
        .iter()
        .any(|call| matches!(call, MockCall::TerminateInstance { .. })));
}

#[tokio::test]
#[serial]
async fn legacy_control_write_invalidates_known_empty_inventory() {
    use g5_api::ws::host_ctl::{ControlPlaneHostState, HostConnection};
    let _env = DetectorEnv::enabled();
    let (app, mock) = spawn_with_mock().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);
    let (host_id, instance) = provision_host(&app, &client, org.id).await;
    let known: Vec<Uuid> =
        sqlx::query_scalar("SELECT reported_live_vm_ids FROM worker_hosts WHERE host_id=$1")
            .bind(host_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(
        known.is_empty(),
        "current provisioning creates a new, empty host"
    );
    // An old replica's legacy hello/heartbeat updates liveness but never
    // writes reported inventory, even if its accepted hello contains a VM.
    sqlx::query("UPDATE worker_hosts SET last_heartbeat=now(), capabilities=$2, slot_size_reported=false WHERE host_id=$1")
        .bind(host_id).bind(vec!["workspace_vm:v1"]).execute(&app.db).await.unwrap();
    sqlx::query("UPDATE worker_hosts SET created_at=now()-interval '1 hour', bringup_heartbeat_at=now()-interval '10 minutes' WHERE host_id=$1")
        .bind(host_id).execute(&app.db).await.unwrap();
    let pass = detect_once(&app.state).await;
    assert_eq!(
        pass.never_registered_reaped, 0,
        "older control write makes custody unknown: {pass:?}"
    );
    assert!(!mock.calls().contains(&MockCall::TerminateInstance {
        cloud_instance_id: instance.clone()
    }));
    let backend = app.host_control_backend();
    backend
        .heartbeat(host_id, &inventory_heartbeat(&[]))
        .await
        .unwrap();
    assert_eq!(
        detect_once(&app.state).await.never_registered_reaped,
        0,
        "partial current heartbeat cannot clear unknown legacy inventory"
    );
    backend
        .accept_hello(
            host_id,
            &legacy_inventory_hello(host_id, Vec::new()),
            &HostConnection::new(true),
        )
        .await
        .unwrap();
    assert_eq!(detect_once(&app.state).await.never_registered_reaped, 1);
    assert!(mock.calls().contains(&MockCall::TerminateInstance {
        cloud_instance_id: instance
    }));
}

#[tokio::test]
#[serial]
async fn detector_reaps_silent_ready_host_and_replaces() {
    let _env = DetectorEnv::enabled();
    let (app, mock) = spawn_with_mock().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    // Silent host: ready, heartbeat 25 min stale (past the 20 min threshold).
    // A legacy row from before migration 061 records no creator.
    let (silent_host, silent_instance) = provision_host(&app, &client, org.id).await;
    sqlx::query(
        "UPDATE worker_hosts SET status = 'ready', slot_size_reported = true, \
             last_heartbeat = now() - interval '25 minutes', created_by_user_id = NULL \
         WHERE host_id = $1",
    )
    .bind(silent_host)
    .execute(&app.db)
    .await
    .expect("seed silent host");
    // Briefly-quiet host: 4 min stale — inside rotation's liveness band,
    // FAR below the detector threshold. Must be untouched (disjointness).
    let (quiet_host, _) = provision_host(&app, &client, org.id).await;
    sqlx::query(
        "UPDATE worker_hosts SET status = 'ready', slot_size_reported = true, \
             last_heartbeat = now() - interval '4 minutes' \
         WHERE host_id = $1",
    )
    .bind(quiet_host)
    .execute(&app.db)
    .await
    .expect("seed quiet host");

    let pass = detect_once(&app.state).await;
    assert_eq!(pass.silent_reaped, 1, "exactly one silent reap: {pass:?}");
    assert_eq!(pass.errors, 0, "no errors: {pass:?}");

    let (status, _, replaced_by) = host_status(&app, silent_host).await;
    assert_eq!(status, "terminated", "silent host finalized");
    let replacement_id = replaced_by.expect("replacement provisioned");
    let (repl_status, _, _) = host_status(&app, replacement_id).await;
    assert_eq!(repl_status, "provisioning", "replacement is fresh");
    assert_eq!(
        host_creator(&app, replacement_id).await,
        None,
        "a legacy host's replacement records no creator either"
    );
    assert!(
        mock.calls().contains(&MockCall::TerminateInstance {
            cloud_instance_id: silent_instance.clone()
        }),
        "silent host instance torn down"
    );
    let (quiet_status, _, quiet_replaced) = host_status(&app, quiet_host).await;
    assert_eq!(quiet_status, "ready", "briefly-quiet host untouched");
    assert_eq!(quiet_replaced, None);
}

/// A [D67]-paired silent host is still reaped past the threshold, but the
/// reap must NOT provision a second replacement: the pairing marker is
/// preserved, the in-flight (stockout) replacement row is untouched, and no
/// extra host row appears (contract D902; CodeRabbit PR #2051).
#[tokio::test]
#[serial]
async fn detector_reaps_paired_silent_host_without_second_replacement() {
    let _env = DetectorEnv::enabled();
    let (app, mock) = spawn_with_mock().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    // Paired silent host: ready, heartbeat 25 min stale, already carrying a
    // rotation pairing marker toward a replacement stuck in stockout.
    let (silent_host, silent_instance) = provision_host(&app, &client, org.id).await;
    mock.fail_provision_stockout(true);
    let replacement_host = provision_host_row_only(&client, org.id).await;
    wait_for_host_condition(
        &app,
        replacement_host,
        "SELECT provision_error_code = 'zone_capacity_exhausted' \
         FROM worker_hosts WHERE host_id = $1",
        "replacement stockout classified",
    )
    .await;
    mock.fail_provision_stockout(false);
    sqlx::query(
        "UPDATE worker_hosts SET status = 'ready', slot_size_reported = true, \
             last_heartbeat = now() - interval '25 minutes', \
             rotation_replaced_by = $2 \
         WHERE host_id = $1",
    )
    .bind(silent_host)
    .bind(replacement_host)
    .execute(&app.db)
    .await
    .expect("seed paired silent host");

    let hosts_before =
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM worker_hosts WHERE org_id = $1")
            .bind(org.id)
            .fetch_one(&app.db)
            .await
            .expect("count before");

    let pass = detect_once(&app.state).await;
    assert_eq!(pass.silent_reaped, 1, "paired silent host reaped: {pass:?}");
    assert_eq!(pass.errors, 0, "no errors: {pass:?}");

    let (status, _, replaced_by) = host_status(&app, silent_host).await;
    assert_eq!(status, "terminated", "paired silent host finalized");
    assert_eq!(
        replaced_by,
        Some(replacement_host),
        "existing pairing marker preserved — no second replacement stamped"
    );
    assert!(
        mock.calls().contains(&MockCall::TerminateInstance {
            cloud_instance_id: silent_instance.clone()
        }),
        "paired silent host instance torn down"
    );

    // The in-flight stockout replacement is untouched.
    let (repl_status, repl_error, repl_replaced) = host_status(&app, replacement_host).await;
    assert_eq!(repl_status, "provisioning", "replacement row untouched");
    assert_eq!(repl_error.as_deref(), Some("zone_capacity_exhausted"));
    assert_eq!(repl_replaced, None);

    // No extra host row was provisioned by the reap.
    let hosts_after =
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM worker_hosts WHERE org_id = $1")
            .bind(org.id)
            .fetch_one(&app.db)
            .await
            .expect("count after");
    assert_eq!(
        hosts_after, hosts_before,
        "no second replacement provisioned"
    );
}

// ─── Pass 3: cloud-orphan sweep (#1464, contract D903) ──────────────────────

#[tokio::test]
#[serial]
async fn detector_sweeps_cloud_orphan_after_sustained_sighting() {
    let _env = DetectorEnv::enabled();
    let (app, mock) = spawn_with_mock().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    // A live host — its instance appears in the listing and must NEVER be
    // reaped. (Also anchors us-central1 into the swept region set.)
    let (live_host, _) = provision_host(&app, &client, org.id).await;
    sqlx::query(
        "UPDATE worker_hosts SET status = 'ready', slot_size_reported = true, last_heartbeat = now() WHERE host_id = $1",
    )
    .bind(live_host)
    .execute(&app.db)
    .await
    .expect("live host ready");

    // An orphan: a terminated row whose instance is still RUNNING (#1464).
    let (orphan_host, _) = provision_host(&app, &client, org.id).await;
    sqlx::query("UPDATE worker_hosts SET status = 'terminated' WHERE host_id = $1")
        .bind(orphan_host)
        .execute(&app.db)
        .await
        .expect("terminate orphan row");

    // A revival case: terminated now, revived between sightings.
    let (revived_host, _) = provision_host(&app, &client, org.id).await;
    sqlx::query("UPDATE worker_hosts SET status = 'terminated' WHERE host_id = $1")
        .bind(revived_host)
        .execute(&app.db)
        .await
        .expect("terminate revived row");

    let live_name = format!("g5-host-t-{live_host}");
    let orphan_name = format!("g5-host-t-{orphan_host}");
    let revived_name = format!("g5-host-t-{revived_host}");
    // Orphan in a region with NO worker_hosts rows at all (hard-deleted-row
    // corner, CodeRabbit round 2): the allowlist-based sweep set must still
    // list and reap it.
    let rowless_host = Uuid::new_v4();
    let rowless_name = format!("g5-host-t-{rowless_host}");
    mock.set_region_listing(
        "us-west2",
        vec![ListedInstance {
            name: rowless_name.clone(),
            zone: "us-west2-a".into(),
            state: InstanceState::Running,
        }],
    );
    mock.set_region_listing(
        "us-central1",
        vec![
            ListedInstance {
                name: live_name.clone(),
                zone: "us-central1-a".into(),
                state: InstanceState::Running,
            },
            ListedInstance {
                name: orphan_name.clone(),
                zone: "us-central1-b".into(),
                state: InstanceState::Running,
            },
            ListedInstance {
                name: revived_name.clone(),
                zone: "us-central1-a".into(),
                state: InstanceState::Running,
            },
        ],
    );

    // First pass: sightings only — a single observation must never kill.
    let pass = detect_once(&app.state).await;
    assert_eq!(
        pass.orphans_reaped, 0,
        "first sighting never reaps: {pass:?}"
    );
    assert_eq!(
        pass.orphan_sightings, 3,
        "all three orphans stamped: {pass:?}"
    );
    let sightings: i64 = sqlx::query_scalar("SELECT count(*) FROM cloud_orphan_sightings")
        .fetch_one(&app.db)
        .await
        .expect("count sightings");
    assert_eq!(sightings, 3);

    // Revive one row; backdate both sightings past the 15 min grace.
    sqlx::query("UPDATE worker_hosts SET status = 'provisioning' WHERE host_id = $1")
        .bind(revived_host)
        .execute(&app.db)
        .await
        .expect("revive row");
    sqlx::query("UPDATE cloud_orphan_sightings SET first_seen = now() - interval '16 minutes'")
        .execute(&app.db)
        .await
        .expect("backdate sightings");

    let pass = detect_once(&app.state).await;
    assert_eq!(
        pass.orphans_reaped, 2,
        "both true orphans reaped (incl. the rowless-region one): {pass:?}"
    );
    assert!(
        mock.calls().contains(&MockCall::TerminateInstance {
            cloud_instance_id: format!("us-central1-b/{orphan_name}")
        }),
        "orphan terminated in its FOUND zone"
    );
    assert!(
        mock.calls().contains(&MockCall::TerminateInstance {
            cloud_instance_id: format!("us-west2-a/{rowless_name}")
        }),
        "rowless-region orphan swept via the allowlist-based region set"
    );
    assert!(
        !mock.calls().iter().any(|c| matches!(c,
            MockCall::TerminateInstance { cloud_instance_id } if cloud_instance_id.contains(&live_name))),
        "live host never touched"
    );
    assert!(
        !mock.calls().iter().any(|c| matches!(c,
            MockCall::TerminateInstance { cloud_instance_id } if cloud_instance_id.contains(&revived_name))),
        "revived host never touched"
    );
    // The reaped orphan's sighting is gone; the revived one was cleared.
    let sightings: i64 = sqlx::query_scalar("SELECT count(*) FROM cloud_orphan_sightings")
        .fetch_one(&app.db)
        .await
        .expect("count sightings");
    assert_eq!(sightings, 0, "sightings cleared after reap/revival");
}

/// A short orphan grace cannot turn a partition into evidence that custody
/// is gone. Expired leases also wait for their owning lifecycle to release.
#[tokio::test]
#[serial]
async fn cloud_orphan_selection_preserves_partitioned_and_terminal_vm_custody() {
    let _env = DetectorEnv::enabled();
    std::env::set_var(CLOUD_ORPHAN_GRACE_ENV, "120");
    let (app, mock) = spawn_with_mock().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);
    let (host_id, _) = provision_host(&app, &client, org.id).await;
    let workspace = helpers::TestWorkspace::create(&app, &org).await;
    let vm_id = Uuid::new_v4();
    sqlx::query("UPDATE worker_hosts SET status = 'unreachable', slot_size_reported = true, last_heartbeat = now() WHERE host_id = $1")
        .bind(host_id).execute(&app.db).await.unwrap();
    sqlx::query("INSERT INTO workspace_vms(org_id, workspace_id, host_id, vm_id, epoch, status, lease_expires_at) VALUES ($1,$2,$3,$4,1,'running',now()+interval '5 minutes')")
        .bind(org.id).bind(workspace.id).bind(host_id).bind(vm_id.to_string()).execute(&app.db).await.unwrap();
    let name = format!("g5-host-t-{host_id}");
    let linkage = format!("us-central1-a/{name}");
    mock.set_region_listing(
        "us-central1",
        vec![ListedInstance {
            name: name.clone(),
            zone: "us-central1-a".into(),
            state: InstanceState::Running,
        }],
    );
    // A sighting left over from before custody was established is cleared.
    sqlx::query("INSERT INTO cloud_orphan_sightings(instance_name, zone, host_id, first_seen) VALUES ($1,'us-central1-a',$2,now()-interval '5 minutes')")
        .bind(&name).bind(host_id).execute(&app.db).await.unwrap();
    for (status, expired) in [
        ("unreachable", false),
        ("terminated", false),
        ("terminated", true),
    ] {
        sqlx::query("UPDATE worker_hosts SET status = $2 WHERE host_id = $1")
            .bind(host_id)
            .bind(status)
            .execute(&app.db)
            .await
            .unwrap();
        if expired {
            sqlx::query("UPDATE workspace_vms SET lease_expires_at = now()-interval '1 second' WHERE workspace_id = $1").bind(workspace.id).execute(&app.db).await.unwrap();
        }
        let pass = detect_once(&app.state).await;
        assert_eq!(pass.errors, 0, "{pass:?}");
        assert_eq!(
            (pass.orphans_reaped, pass.orphan_sightings),
            (0, 0),
            "protected {status}, expired={expired}: {pass:?}"
        );
        let sightings: i64 =
            sqlx::query_scalar("SELECT count(*) FROM cloud_orphan_sightings WHERE host_id = $1")
                .bind(host_id)
                .fetch_one(&app.db)
                .await
                .unwrap();
        assert_eq!(sightings, 0);
        assert!(!mock.calls().contains(&MockCall::TerminateInstance {
            cloud_instance_id: linkage.clone()
        }));
    }
    // Expiry releases through the real lifecycle owner, then orphan sweeping
    // starts a new grace period and retries an actual provider failure.
    let db = app.scoped_db_for_user_org(user.id, org.id).await;
    let scope = db.reopener();
    db.rollback().await.unwrap();
    let lifecycle =
        g5_api::services::WorkspaceVmService::new(g5_api::services::WorkspaceVmServiceConfig {
            system_pool: app.state.system_pool.clone(),
            scope,
            hosts: app.state.preview_tunnels.clone(),
            minter: app.state.jwt_minter.clone().unwrap(),
            runtime: app.state.workspace_runtime.clone(),
            agent_work: app.state.agent_work.clone(),
            repo_storage: app.state.repo_storage.clone(),
            git_external_url: app.state.git_external_url.clone(),
            ws_hub: app.state.event_bus.ws_hub().clone(),
        });
    let released = lifecycle.reconcile_expired_leases().await;
    assert!(released.is_ok(), "{released:?}");
    let custody: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM workspace_vms WHERE host_id = $1)")
            .bind(host_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(!custody, "the VM lifecycle released expired custody");
    let pass = detect_once(&app.state).await;
    assert_eq!(
        (pass.orphan_sightings, pass.orphans_reaped),
        (1, 0),
        "{pass:?}"
    );
    sqlx::query("UPDATE cloud_orphan_sightings SET first_seen = now()-interval '5 minutes' WHERE host_id = $1").bind(host_id).execute(&app.db).await.unwrap();
    mock.fail_terminate(true);
    let pass = detect_once(&app.state).await;
    assert_eq!((pass.orphans_reaped, pass.errors), (0, 1), "{pass:?}");
    mock.fail_terminate(false);
    let pass = detect_once(&app.state).await;
    assert_eq!(
        pass.orphans_reaped, 1,
        "released host eventually cleaned up: {pass:?}"
    );
    app.cleanup().await;
}

/// Force a changed custody observation between candidate selection and the
/// final cloud call using a real Postgres row-lock barrier on the sighting.
#[tokio::test]
#[serial]
async fn cloud_orphan_delete_rechecks_custody_after_selecting_a_candidate() {
    let _env = DetectorEnv::enabled();
    std::env::set_var(CLOUD_ORPHAN_GRACE_ENV, "120");
    let (app, mock) = spawn_with_mock().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);
    let (host_id, _) = provision_host(&app, &client, org.id).await;
    let workspace = helpers::TestWorkspace::create(&app, &org).await;
    sqlx::query("UPDATE worker_hosts SET status='terminated' WHERE host_id=$1")
        .bind(host_id)
        .execute(&app.db)
        .await
        .unwrap();
    let name = format!("g5-host-t-{host_id}");
    let linkage = format!("us-central1-a/{name}");
    mock.set_region_listing(
        "us-central1",
        vec![ListedInstance {
            name: name.clone(),
            zone: "us-central1-a".into(),
            state: InstanceState::Running,
        }],
    );
    sqlx::query("INSERT INTO cloud_orphan_sightings(instance_name,zone,host_id,first_seen) VALUES($1,'us-central1-a',$2,now()-interval '5 minutes')")
        .bind(&name).bind(host_id).execute(&app.db).await.unwrap();
    let mut barrier = app.db.begin().await.unwrap();
    sqlx::query("SELECT 1 FROM cloud_orphan_sightings WHERE host_id=$1 FOR UPDATE")
        .bind(host_id)
        .execute(&mut *barrier)
        .await
        .unwrap();
    let detector = tokio::spawn({
        let state = app.state.clone();
        async move { detect_once(&state).await }
    });
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let waiting: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND query LIKE 'INSERT INTO cloud_orphan_sightings%')")
                .fetch_one(&app.db).await.unwrap();
            if waiting { break; }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }).await.expect("candidate selected; sighting refresh parked before final admission");
    // Inject the changed observation directly: ordinary placement would also
    // require a live host, which the final guard protects independently.
    sqlx::query("INSERT INTO workspace_vms(org_id,workspace_id,host_id,vm_id,epoch,status,lease_expires_at) VALUES($1,$2,$3,$4,1,'stopping',now()+interval '5 minutes')")
        .bind(org.id).bind(workspace.id).bind(host_id).bind(Uuid::new_v4().to_string()).execute(&app.db).await.unwrap();
    barrier.commit().await.unwrap();
    let pass = detector.await.unwrap();
    assert_eq!(pass.errors, 0, "{pass:?}");
    assert_eq!(
        pass.orphans_reaped, 0,
        "new custody must fence the cloud call: {pass:?}"
    );
    assert!(!mock.calls().contains(&MockCall::TerminateInstance {
        cloud_instance_id: linkage
    }));
    let sightings: i64 =
        sqlx::query_scalar("SELECT count(*) FROM cloud_orphan_sightings WHERE host_id=$1")
            .bind(host_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(sightings, 0, "custody resets the sighting grace");
    app.cleanup().await;
}

// ─── Retry pass ([D66], #1880, contract D904) ───────────────────────────────

#[tokio::test]
#[serial]
async fn missing_disk_snapshot_preserves_manual_retry_schedule_and_budget() {
    let _env = DetectorEnv::with_retry("us-central1");
    let (app, mock) = spawn_with_mock().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let host_id = Uuid::new_v4();
    // A legacy/incomplete allocation must be refused without consuming the
    // scheduled retry; the operator can finish migration and retry safely.
    sqlx::query(
        "INSERT INTO worker_hosts(host_id,org_id,region,instance_type,status,base_image_ref,
        provision_error_code,provision_next_retry_at,provision_retry_count,created_by_user_id)
        VALUES($1,$2,'us-central1','c3-standard-4','provisioning',$3,
        'zone_capacity_exhausted',now(),1,$4)",
    )
    .bind(host_id)
    .bind(org.id)
    .bind(helpers::TEST_WORKER_IMAGE_REF)
    .bind(user.id)
    .execute(&app.db)
    .await
    .unwrap();
    let valid_host = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO worker_hosts(host_id,org_id,region,instance_type,status,base_image_ref,
        provision_error_code,provision_next_retry_at,provision_retry_count,created_by_user_id,
        host_disk_gib,scratch_disk_mib,provisioned_slots,resolved_disk_gib)
        VALUES($1,$2,'us-central1','c3-standard-4','provisioning',$3,
        'zone_capacity_exhausted',now(),1,$4,120,81920,1,200)",
    )
    .bind(valid_host)
    .bind(org.id)
    .bind(helpers::TEST_WORKER_IMAGE_REF)
    .bind(user.id)
    .execute(&app.db)
    .await
    .unwrap();
    let before: (i32, Option<String>, Option<chrono::DateTime<chrono::Utc>>) = sqlx::query_as(
        "SELECT provision_retry_count,provision_error_code,provision_next_retry_at FROM worker_hosts WHERE host_id=$1")
        .bind(host_id).fetch_one(&app.db).await.unwrap();
    let pass = detect_once(&app.state).await;
    assert_eq!(
        pass.retried, 1,
        "an incomplete snapshot cannot starve the next host"
    );
    wait_for_cloud_instance_id(&app, valid_host).await;
    let after = sqlx::query_as::<_, (i32, Option<String>, Option<chrono::DateTime<chrono::Utc>>)>(
        "SELECT provision_retry_count,provision_error_code,provision_next_retry_at FROM worker_hosts WHERE host_id=$1")
        .bind(host_id).fetch_one(&app.db).await.unwrap();
    assert_eq!(after, before);
    assert!(!mock.calls().iter().any(
        |call| matches!(call, MockCall::ProvisionInstance { host_id: id, .. } if *id == host_id)
    ));
    sqlx::query(
        "UPDATE worker_hosts SET host_disk_gib=120,scratch_disk_mib=81920,
        provisioned_slots=1,resolved_disk_gib=200 WHERE host_id=$1",
    )
    .bind(host_id)
    .execute(&app.db)
    .await
    .unwrap();
    assert_eq!(detect_once(&app.state).await.retried, 1);
    wait_for_cloud_instance_id(&app, host_id).await;
    app.cleanup().await;
}

#[tokio::test]
#[serial]
#[serial(worker_scratch_disk)]
async fn retry_pass_reprovisions_stockout_row_across_regions_until_budget() {
    let _env = DetectorEnv::with_retry("us-east1,us-west1");
    let _scratch = helpers::EnvVarGuard::set_many(&[("G5_WORKER_SCRATCH_DISK_MIB", "81920")]);
    let (app, mock) = spawn_with_mock().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);
    // Cross-region fallback targets only regions placement will use
    // (issue #627 S30): point the org's placement region at the basket's
    // first entry so the walk is allowed to leave us-central1.
    sqlx::query(
        "INSERT INTO configs(org_id,scope,scope_id,key,value) \
         VALUES($1,'org',$1,'worker_host_region','\"us-east1\"')",
    )
    .bind(org.id)
    .execute(&app.db)
    .await
    .expect("configure placement region");

    // Stockout on every insert: the POST leaves a classified, retry-scheduled
    // provisioning row instead of a dead-end.
    mock.fail_provision_stockout(true);
    let host_id = provision_host_row_only(&client, org.id).await;
    wait_for_host_condition(
        &app,
        host_id,
        "SELECT provision_error_code = 'zone_capacity_exhausted' \
             AND provision_next_retry_at IS NOT NULL \
         FROM worker_hosts WHERE host_id = $1",
        "stockout classified + retry scheduled",
    )
    .await;

    // Configuration may change while a host is retrying; keep the capacity
    // persisted for this host rather than resolving the setting again.
    let _changed_scratch =
        helpers::EnvVarGuard::set_many(&[("G5_WORKER_SCRATCH_DISK_MIB", "65536")]);

    assert_eq!(
        client
            .put_json(
                &format!("/api/v1/orgs/{}/admin-settings", org.id),
                &json!({"hostDiskAllowanceGib":160,"vmDiskAllowanceGib":32})
            )
            .await
            .status(),
        StatusCode::OK
    );

    // Attempt 1 (due now): same region.
    sqlx::query("UPDATE worker_hosts SET provision_next_retry_at = now() WHERE host_id = $1")
        .bind(host_id)
        .execute(&app.db)
        .await
        .expect("make retry due");
    let pass = detect_once(&app.state).await;
    assert_eq!(pass.retried, 1, "attempt 1 claimed: {pass:?}");
    let region: String = sqlx::query_scalar("SELECT region FROM worker_hosts WHERE host_id = $1")
        .bind(host_id)
        .fetch_one(&app.db)
        .await
        .expect("region");
    assert_eq!(
        region, "us-central1",
        "attempts 1-2 stay in the row's region"
    );
    wait_for_host_condition(
        &app,
        host_id,
        "SELECT provision_next_retry_at IS NOT NULL FROM worker_hosts WHERE host_id = $1",
        "attempt 1 failed and rescheduled",
    )
    .await;

    // Attempt 2: still same region.
    sqlx::query("UPDATE worker_hosts SET provision_next_retry_at = now() WHERE host_id = $1")
        .bind(host_id)
        .execute(&app.db)
        .await
        .expect("make retry due");
    let pass = detect_once(&app.state).await;
    assert_eq!(pass.retried, 1, "attempt 2 claimed: {pass:?}");
    wait_for_host_condition(
        &app,
        host_id,
        "SELECT provision_next_retry_at IS NOT NULL FROM worker_hosts WHERE host_id = $1",
        "attempt 2 failed and rescheduled",
    )
    .await;

    // Attempt 3: capacity returns; the basket's first region takes the insert.
    mock.fail_provision_stockout(false);
    sqlx::query("UPDATE worker_hosts SET provision_next_retry_at = now() WHERE host_id = $1")
        .bind(host_id)
        .execute(&app.db)
        .await
        .expect("make retry due");
    let pass = detect_once(&app.state).await;
    assert_eq!(pass.retried, 1, "attempt 3 claimed: {pass:?}");
    wait_for_host_condition(
        &app,
        host_id,
        "SELECT cloud_instance_id IS NOT NULL AND provision_error_code IS NULL \
             AND provision_next_retry_at IS NULL \
         FROM worker_hosts WHERE host_id = $1",
        "attempt 3 succeeded, error/retry state cleared",
    )
    .await;
    let (region, retry_count): (String, i32) =
        sqlx::query_as("SELECT region, provision_retry_count FROM worker_hosts WHERE host_id = $1")
            .bind(host_id)
            .fetch_one(&app.db)
            .await
            .expect("row");
    assert_eq!(region, "us-east1", "attempt 3 walked the fallback basket");
    assert_eq!(retry_count, 3);
    let disks: Vec<_> = mock
        .calls()
        .into_iter()
        .filter_map(|call| match call {
            MockCall::ProvisionInstance {
                host_id: id,
                boot_disk_gb,
                startup_script,
                ..
            } if id == host_id => {
                assert!(startup_script.contains("G5D_SCRATCH_DISK_MIB='81920'"));
                assert!(startup_script.contains("G5D_HOST_MAX_MICROVMS=2\n"));
                Some(boot_disk_gb)
            }
            _ => None,
        })
        .collect();
    assert!(disks.len() >= 4, "initial attempt and all three retries");
    assert!(
        disks.iter().all(|disk| *disk == 280),
        "the two-slot host uses the fixed policy on every retry: {disks:?}"
    );
    assert!(
        mock.calls().iter().any(|c| matches!(c,
            MockCall::ProvisionInstance { region, .. } if region == "us-east1")),
        "the successful insert targeted the fallback region"
    );

    // Budget exhaustion: a stuck row past the 30 min budget is frozen, not
    // retried — the terminal badge stays truthful.
    mock.fail_provision_stockout(true);
    let stuck_id = provision_host_row_only(&client, org.id).await;
    wait_for_host_condition(
        &app,
        stuck_id,
        "SELECT provision_error_code = 'zone_capacity_exhausted' \
             AND provision_next_retry_at IS NOT NULL \
         FROM worker_hosts WHERE host_id = $1",
        "stockout classified + retry scheduled",
    )
    .await;
    sqlx::query(
        "UPDATE worker_hosts SET created_at = now() - interval '31 minutes', \
             provision_next_retry_at = now() \
         WHERE host_id = $1",
    )
    .bind(stuck_id)
    .execute(&app.db)
    .await
    .expect("age past budget");
    let pass = detect_once(&app.state).await;
    assert_eq!(pass.retry_exhausted, 1, "budget lapsed: {pass:?}");
    assert_eq!(pass.retried, 0, "no retry past the budget: {pass:?}");
    let (code, next_retry): (Option<String>, Option<chrono::DateTime<chrono::Utc>>) =
        sqlx::query_as(
            "SELECT provision_error_code, provision_next_retry_at \
             FROM worker_hosts WHERE host_id = $1",
        )
        .bind(stuck_id)
        .fetch_one(&app.db)
        .await
        .expect("row");
    assert_eq!(code.as_deref(), Some("zone_capacity_exhausted"));
    assert_eq!(next_retry, None, "schedule frozen");
    // No zero-capacity alert: the recovered host (attempt 3) is linked and
    // healthy, so the org is NOT at zero capacity despite the frozen row.
    assert_eq!(
        pass.zero_capacity_orgs, 0,
        "org still has the recovered host"
    );
}

// ─── [D67] rotation ordering (#1880, contract D905) ─────────────────────────

/// An operator drain records a drain id; rotation's recovery sweep finishes
/// only its own drains, so it neither replaces nor tears down the host.
#[tokio::test]
#[serial]
async fn rotation_leaves_an_operator_drained_host_alone() {
    let _rot = RotationEnv::enabled();
    let (app, mock) = spawn_with_mock().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);
    let (host_id, instance_id) = provision_host(&app, &client, org.id).await;
    sqlx::query(
        "UPDATE worker_hosts SET status = 'ready', last_heartbeat = now(), \
             active_microvm_count = 0 WHERE host_id = $1",
    )
    .bind(host_id)
    .execute(&app.db)
    .await
    .expect("seed an idle ready host");
    let drain = client
        .post_json(
            &format!("/api/v1/orgs/{}/worker-hosts/{host_id}/drain", org.id),
            &json!({"reason": "operator maintenance"}),
        )
        .await;
    assert_eq!(drain.status(), StatusCode::OK);
    let drain: Value = drain.json().await.expect("drain response is JSON");
    assert_eq!(drain["data"]["activeVms"], 0);

    let calls_before = mock.calls().len();
    let pass = rotate_once(&app.state).await;
    assert_eq!(
        pass.resumed, 0,
        "an operator drain is not a rotation leftover: {pass:?}"
    );
    assert_eq!(host_status(&app, host_id).await.0, "draining");
    let new_calls = &mock.calls()[calls_before..];
    assert!(
        !new_calls
            .iter()
            .any(|call| matches!(call, MockCall::ProvisionInstance { .. })),
        "no replacement is provisioned: {new_calls:?}"
    );
    assert!(
        !new_calls.contains(&MockCall::TerminateInstance {
            cloud_instance_id: instance_id
        }),
        "the drained host is not torn down: {new_calls:?}"
    );
    app.cleanup().await;
}

/// A host rotation is draining stays rotation's: an operator drain is refused
/// and records no drain id, so the sweep still replaces and finishes it.
#[tokio::test]
#[serial]
async fn an_operator_drain_cannot_take_over_a_rotation_drain() {
    let _rot = RotationEnv::enabled();
    let (app, mock) = spawn_with_mock().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);
    let (host_id, instance_id) = provision_host(&app, &client, org.id).await;
    // Rotation's quiesce marks a host draining without a drain id.
    sqlx::query(
        "UPDATE worker_hosts SET status = 'draining', last_heartbeat = now(), \
             active_microvm_count = 0 WHERE host_id = $1",
    )
    .bind(host_id)
    .execute(&app.db)
    .await
    .expect("seed a rotation drain");

    let drain = client
        .post_json(
            &format!("/api/v1/orgs/{}/worker-hosts/{host_id}/drain", org.id),
            &json!({"reason": "operator maintenance"}),
        )
        .await;
    assert_eq!(drain.status(), StatusCode::CONFLICT);
    let drain_id: Option<Uuid> =
        sqlx::query_scalar("SELECT drain_id FROM worker_hosts WHERE host_id = $1")
            .bind(host_id)
            .fetch_one(&app.db)
            .await
            .expect("load drain id");
    assert_eq!(drain_id, None, "the operator drain records no drain id");

    let pass = rotate_once(&app.state).await;
    assert_eq!(
        pass.resumed, 1,
        "rotation still finishes its drain: {pass:?}"
    );
    assert!(
        mock.calls().contains(&MockCall::TerminateInstance {
            cloud_instance_id: instance_id
        }),
        "rotation tears down the host it drained"
    );
    app.cleanup().await;
}

#[tokio::test]
#[serial]
#[serial(worker_scratch_disk)]
async fn rotation_provisions_before_draining_and_recovers_from_failed_replacement() {
    let _rot = RotationEnv::enabled();
    let _scratch = helpers::EnvVarGuard::set_many(&[("G5_WORKER_SCRATCH_DISK_MIB", "81920")]);
    let (app, mock) = spawn_with_mock().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);
    assert_eq!(
        client
            .put_json(
                &format!("/api/v1/orgs/{}/admin-settings", org.id),
                &json!({"vmDiskAllowanceGib":64})
            )
            .await
            .status(),
        StatusCode::OK
    );

    // Image-stale idle ready host (seeded before the org tracks a default →
    // NULL digest counts as stale).
    let (old_host, old_instance) = provision_host(&app, &client, org.id).await;
    sqlx::query(
        "UPDATE worker_hosts SET status = 'unreachable', slot_size_reported = false, last_heartbeat = now(), \
             active_microvm_count = 0, created_at = now() - interval '6 hours', \
             last_active_at = NULL, slot_cpus = 4, slot_mem_mib = 8192, \
             scratch_disk_mib = 65536 \
         WHERE host_id = $1",
    )
    .bind(old_host)
    .execute(&app.db)
    .await
    .expect("seed ready host");
    assert_eq!(
        client
            .put_json(
                &format!("/api/v1/orgs/{}/admin-settings", org.id),
                &json!({"vmDiskAllowanceGib":80})
            )
            .await
            .status(),
        StatusCode::OK
    );
    legacy_control_hello(&app, old_host).await;
    assert_eq!(
        host_status(&app, old_host).await.0,
        "ready",
        "recovered legacy host remains rotatable"
    );
    // Track a default so the idle path has an image-staleness signal.
    let digest = format!("sha256:{}", "c".repeat(64));
    let image_ref = format!("reg.example.com/g5/track@{digest}");
    sqlx::query(
        "UPDATE orgs SET default_base_image_track = 'track', default_base_image_ref = $1 \
         WHERE id = $2",
    )
    .bind(&image_ref)
    .bind(org.id)
    .execute(&app.db)
    .await
    .expect("tracking org");
    // Idle window: dev-style small window via env is global; instead backdate
    // last_active via created_at (COALESCE uses created_at when NULL) — the
    // 6h age is past the 5h default min-idle.

    // Pass 1: PAIR — the old host must keep serving.
    let pass = rotate_once(&app.state).await;
    assert_eq!(pass.paired, 1, "paired, not drained: {pass:?}");
    assert_eq!(pass.rotated, 0, "{pass:?}");
    let (status, _, replaced_by) = host_status(&app, old_host).await;
    assert_eq!(status, "ready", "old host still serving after pairing");
    let replacement_id = replaced_by.expect("pairing marker stamped");
    let replacement_size: (i32, i32, i32) = sqlx::query_as(
        "SELECT slot_cpus, slot_mem_mib, scratch_disk_mib \
         FROM worker_hosts WHERE host_id = $1",
    )
    .bind(replacement_id)
    .fetch_one(&app.db)
    .await
    .expect("replacement sizing persisted");
    assert_eq!(
        replacement_size,
        (4, 8192, 81920),
        "replacement adopts the current 80 GiB setting; CPU/RAM stay provisional"
    );
    let old_capacity: i32 =
        sqlx::query_scalar("SELECT scratch_disk_mib FROM worker_hosts WHERE host_id = $1")
            .bind(old_host)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(
        old_capacity, 65536,
        "the serving predecessor stays at 64 GiB"
    );
    assert!(
        !mock.calls().contains(&MockCall::TerminateInstance {
            cloud_instance_id: old_instance.clone()
        }),
        "old instance NOT torn down while the replacement is unready"
    );

    // Replacement not ready yet → the await sweep keeps waiting.
    let pass = rotate_once(&app.state).await;
    assert_eq!(pass.rotated + pass.forced, 0, "still waiting: {pass:?}");
    let (status, _, _) = host_status(&app, old_host).await;
    assert_eq!(status, "ready");

    // Even a ready replacement cannot drain useful capacity before publication.
    wait_for_cloud_instance_id(&app, replacement_id).await;
    let script = helpers::provision_instance_startup_script(&mock, replacement_id).unwrap();
    assert!(script.contains("G5D_SCRATCH_DISK_MIB='81920'"));
    sqlx::query("UPDATE worker_hosts SET status='ready', last_heartbeat=now() WHERE host_id=$1")
        .bind(replacement_id)
        .execute(&app.db)
        .await
        .unwrap();
    let pass = rotate_once(&app.state).await;
    assert_eq!(
        pass.rotated + pass.forced,
        0,
        "unpublished replacement: {pass:?}"
    );
    assert_eq!(host_status(&app, old_host).await.0, "ready");
    // Replacement publishes newly measured sizes, rather than retaining old sizes.
    use g5_api::ws::host_ctl::{ControlPlaneHostState, HostConnection};
    let backend = app.host_control_backend();
    backend
        .accept_hello(
            replacement_id,
            &g5_internal_api::host_ctl::HostHello {
                host_id: replacement_id,
                protocol_version: g5_internal_api::host_ctl::PROTOCOL_VERSION,
                capabilities: Vec::new(),
                slots_total: 2,
                slot_size: Some(g5_internal_api::host_ctl::VmSizeSpec {
                    cpus: 3,
                    mem_mib: 13273,
                    scratch_disk_mib: 81920,
                }),
                inventory: Vec::new(),
                retained_roots: Vec::new(),
                runtime: Some(g5_internal_api::host_ctl::HostRuntimeIdentity::bundled()),
            },
            &HostConnection::new(true),
        )
        .await
        .unwrap();
    let measured: (i32, i32, i32, bool) = sqlx::query_as(
        "SELECT slot_cpus,slot_mem_mib,scratch_disk_mib,slot_size_reported FROM worker_hosts WHERE host_id=$1",
    )
    .bind(replacement_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(measured, (3, 13273, 81920, true));
    let pass = rotate_once(&app.state).await;
    assert_eq!(pass.rotated, 1, "drained after replacement ready: {pass:?}");
    let (status, _, _) = host_status(&app, old_host).await;
    assert_eq!(status, "terminated", "old host finalized");
    assert!(
        mock.calls().contains(&MockCall::TerminateInstance {
            cloud_instance_id: old_instance.clone()
        }),
        "old instance torn down after the replacement was ready"
    );

    // ── Failed-replacement recovery: a stocked-out replacement must not
    // consume the old host (retry disabled here ⇒ terminal immediately).
    let (old2, old2_instance) = provision_host(&app, &client, org.id).await;
    // Provisioned AFTER the org tracked a default, so the [D51] fallback made
    // it image-current — null the digest so it is stale and selectable.
    sqlx::query(
        "UPDATE worker_hosts SET status = 'ready', slot_size_reported = true, last_heartbeat = now(), \
             active_microvm_count = 0, created_at = now() - interval '6 hours', \
             last_active_at = NULL, base_image_ref = NULL, base_image_digest = NULL \
         WHERE host_id = $1",
    )
    .bind(old2)
    .execute(&app.db)
    .await
    .expect("seed second ready host");
    mock.fail_provision_stockout(true);
    let pass = rotate_once(&app.state).await;
    assert_eq!(pass.paired, 1, "second host paired: {pass:?}");
    let (_, _, replaced_by2) = host_status(&app, old2).await;
    let failed_replacement = replaced_by2.expect("pairing marker stamped");
    wait_for_host_condition(
        &app,
        failed_replacement,
        "SELECT provision_error_code = 'zone_capacity_exhausted' \
         FROM worker_hosts WHERE host_id = $1",
        "replacement bring-up classified stockout",
    )
    .await;

    let pass = rotate_once(&app.state).await;
    assert_eq!(pass.replacement_failed, 1, "pairing dissolved: {pass:?}");
    let (status, _, replaced_by2) = host_status(&app, old2).await;
    assert_eq!(status, "ready", "old host recovered, still serving");
    // The dissolve frees the marker, and the SAME pass legitimately re-pairs
    // the still-stale host with a fresh replacement attempt — the invariant
    // is that the FAILED pairing is gone and the old host never drained.
    assert_ne!(
        replaced_by2,
        Some(failed_replacement),
        "failed pairing dissolved"
    );
    let (failed_status, _, _) = host_status(&app, failed_replacement).await;
    assert_eq!(
        failed_status, "terminated",
        "failed replacement deallocated"
    );
    assert!(
        !mock.calls().contains(&MockCall::TerminateInstance {
            cloud_instance_id: old2_instance.clone()
        }),
        "old host's instance never touched by the failed rotation"
    );
    // The re-paired replacement (provisioned while stockout was still on)
    // classifies its own failure asynchronously — settle it so case C below
    // starts deterministic.
    let repaired = replaced_by2.expect("re-paired after dissolve");
    wait_for_host_condition(
        &app,
        repaired,
        "SELECT provision_error_code = 'zone_capacity_exhausted' \
         FROM worker_hosts WHERE host_id = $1",
        "re-paired replacement classified stockout",
    )
    .await;

    // ── Forced-lane drain deadline (TTL−15min, human decision 547a4e7d): a
    // paired past-deadline host drains even though its replacement is still
    // healthily in flight (provisioning, no error, just not registered). The
    // host is aged past the CLIFF but below the deadline first: the sweep
    // dissolves the classified-failed re-paired replacement and the force
    // lane re-pairs in the same tick (a past-DEADLINE dissolution would drain
    // immediately instead — covered by
    // deadline_drain_survives_persistent_replacement_failure).
    mock.fail_provision_stockout(false);
    sqlx::query(
        "UPDATE worker_hosts SET created_at = now() - interval '22 hours 30 minutes' \
         WHERE host_id = $1 AND status = 'ready'",
    )
    .bind(old2)
    .execute(&app.db)
    .await
    .expect("age old2 past the cliff, below the drain deadline");
    // Pass 1: dissolve + force re-pair in the same tick (old host keeps
    // serving; the new replacement's bring-up SUCCEEDS now but the row stays
    // `provisioning` — no heartbeat ever arrives in this test).
    let pass = rotate_once(&app.state).await;
    assert_eq!(pass.paired, 1, "forced lane paired old2: {pass:?}");
    assert_eq!(
        pass.replacement_failed, 1,
        "stale pairing dissolved: {pass:?}"
    );
    let (status, _, replaced_by) = host_status(&app, old2).await;
    assert_eq!(status, "ready", "old2 still serving right after pairing");
    let deadline_replacement = replaced_by.expect("pairing marker stamped");
    wait_for_cloud_instance_id(&app, deadline_replacement).await;
    // Pass 2: now age past the TTL−15min deadline — the await sweep sees it
    // lapsed with an unready (but healthy) replacement and drains anyway —
    // the credential dies at the TTL regardless.
    sqlx::query(
        "UPDATE worker_hosts SET created_at = now() - interval '23 hours 50 minutes' \
         WHERE host_id = $1 AND status = 'ready'",
    )
    .bind(old2)
    .execute(&app.db)
    .await
    .expect("age old2 past the drain deadline");
    let pass = rotate_once(&app.state).await;
    assert_eq!(pass.forced, 1, "deadline drain counted as forced: {pass:?}");
    let (status, _, _) = host_status(&app, old2).await;
    assert_eq!(
        status, "terminated",
        "past-deadline paired host drained despite unready replacement"
    );
    assert!(
        mock.calls().contains(&MockCall::TerminateInstance {
            cloud_instance_id: old2_instance.clone()
        }),
        "old2 instance torn down at the deadline drain"
    );
}

/// Ontology g5.runtime.placement.host-disk: a replacement reuses the operator's
/// stored boot-disk override, and a host that stored none re-derives the size
/// from ITS OWN budget — so a sizing-rule change reaches existing hosts when
/// they are replaced, exactly like an omitted `workspaceVmLimit`.
#[tokio::test]
#[serial]
#[serial(worker_scratch_disk)]
async fn rotation_reuses_a_stored_boot_disk_override_and_rederives_for_null() {
    let _rot = RotationEnv::enabled();
    let _scratch = helpers::EnvVarGuard::set_many(&[("G5_WORKER_SCRATCH_DISK_MIB", "")]);
    let (app, mock) = spawn_with_mock().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);
    assert_eq!(
        client
            .put_json(
                &format!("/api/v1/orgs/{}/admin-settings", org.id),
                &json!({"vmDiskAllowanceGib":64})
            )
            .await
            .status(),
        StatusCode::OK
    );

    // Two idle, image-stale hosts: one carries an operator override, the other
    // a NULL that must re-derive. The 5-slot shape keeps the derived size off
    // the floor so the two cases are distinguishable.
    let (override_host, _) = provision_host(&app, &client, org.id).await;
    let (derived_host, _) = provision_host(&app, &client, org.id).await;
    for (host_id, stored) in [(override_host, Some(911_i32)), (derived_host, None)] {
        sqlx::query(
            "UPDATE worker_hosts SET status = 'ready', last_heartbeat = now(), \
                 active_microvm_count = 0, created_at = now() - interval '6 hours', \
                 last_active_at = NULL, instance_type = 'c3-standard-22', \
                 scratch_disk_mib = 65536, boot_disk_gb = $2 \
             WHERE host_id = $1",
        )
        .bind(host_id)
        .bind(stored)
        .execute(&app.db)
        .await
        .expect("seed an idle, image-stale host");
    }
    assert_eq!(
        client
            .put_json(
                &format!("/api/v1/orgs/{}/admin-settings", org.id),
                &json!({"hostDiskAllowanceGib":160,"vmDiskAllowanceGib":96})
            )
            .await
            .status(),
        StatusCode::OK
    );
    // Track an org default so the idle path has an image-staleness signal.
    let image_ref = format!("reg.example.com/g5/track@sha256:{}", "c".repeat(64));
    sqlx::query(
        "UPDATE orgs SET default_base_image_track = 'track', default_base_image_ref = $1 \
         WHERE id = $2",
    )
    .bind(&image_ref)
    .bind(org.id)
    .execute(&app.db)
    .await
    .expect("org tracks a default image");

    // Rotation pairs one host per pass per org, so drive it until both hosts
    // carry a replacement (bounded so a regression fails rather than hangs).
    let mut paired = 0;
    for _ in 0..4 {
        let pass = rotate_once(&app.state).await;
        paired += pass.paired;
        if paired >= 2 {
            break;
        }
    }
    assert_eq!(paired, 2, "both hosts must be paired with replacements");
    let mut seen = 0;
    for (host_id, expected, why) in [
        (
            override_host,
            911,
            "a stored override must survive rotation verbatim",
        ),
        (
            derived_host,
            640,
            "a NULL must re-derive from the replacement's own 5-slot budget",
        ),
    ] {
        let replacement_id = host_status(&app, host_id)
            .await
            .2
            .expect("each idle stale host is paired with a replacement");
        wait_for_cloud_instance_id(&app, replacement_id).await;
        let script = helpers::provision_instance_startup_script(&mock, replacement_id).unwrap();
        assert!(
            script.contains("G5D_SCRATCH_DISK_MIB='98304'"),
            "replacement uses current organization V while predecessor retains its snapshot"
        );
        assert_eq!(
            helpers::provision_instance_boot_disk_gb(&mock, replacement_id),
            Some(expected),
            "{why}"
        );
        // The copied column keeps the same split: override persisted, NULL not
        // backfilled with the size that was just derived.
        let stored: Option<i32> =
            sqlx::query_scalar("SELECT boot_disk_gb FROM worker_hosts WHERE host_id = $1")
                .bind(replacement_id)
                .fetch_one(&app.db)
                .await
                .expect("replacement row");
        assert_eq!(
            stored,
            if host_id == override_host {
                Some(911)
            } else {
                None
            },
            "the replacement row must copy only an explicit override"
        );
        seen += 1;
    }
    assert_eq!(
        seen, 2,
        "both the override and the NULL case must be observed"
    );

    app.cleanup().await;
}

/// `g5.runtime.placement.runtime-identity`: a replacement that came up with
/// another runtime than the control plane's never drains its predecessor.
/// While it is still the image new hosts would boot the pair waits; once the
/// tracked image moves on the pair dissolves, so the predecessor is replaced
/// again from the new image and the stale replacement is left to stale
/// retirement.
#[tokio::test]
#[serial]
async fn a_stale_replacement_never_drains_its_predecessor_and_dissolves_once_the_track_moves() {
    use g5_api::ws::host_ctl::{ControlPlaneHostState, HostConnection};
    let _rot = RotationEnv::enabled();
    let (app, mock) = spawn_with_mock().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    let (old_host, old_instance) = provision_host(&app, &client, org.id).await;
    sqlx::query(
        "UPDATE worker_hosts SET status = 'ready', slot_size_reported = true, last_heartbeat = now(), \
             active_microvm_count = 0, created_at = now() - interval '6 hours', \
             last_active_at = NULL, base_image_ref = NULL, base_image_digest = NULL, \
             runtime_identity = $2 \
         WHERE host_id = $1",
    )
    .bind(old_host)
    .bind(helpers::current_runtime_identity())
    .execute(&app.db)
    .await
    .expect("seed a current, image-stale predecessor");
    let digest = format!("sha256:{}", "c".repeat(64));
    sqlx::query(
        "UPDATE orgs SET default_base_image_track = 'track', default_base_image_ref = $1 WHERE id = $2",
    )
    .bind(format!("reg.example.com/g5/track@{digest}"))
    .bind(org.id)
    .execute(&app.db)
    .await
    .expect("tracking org");

    let pass = rotate_once(&app.state).await;
    assert_eq!(pass.paired, 1, "paired: {pass:?}");
    let (_, _, replaced_by) = host_status(&app, old_host).await;
    let replacement_id = replaced_by.expect("pairing marker stamped");
    wait_for_cloud_instance_id(&app, replacement_id).await;

    // The replacement registers with an older runtime: the tracked image lags
    // this control plane.
    let mut stale_identity = g5_internal_api::host_ctl::HostRuntimeIdentity::bundled();
    stale_identity.module_catalog_version = "2026.01.01-older".to_string();
    stale_identity.module_catalog_hash = "0".repeat(64);
    let backend = app.host_control_backend();
    let hello = |runtime| g5_internal_api::host_ctl::HostHello {
        host_id: replacement_id,
        protocol_version: g5_internal_api::host_ctl::PROTOCOL_VERSION,
        capabilities: Vec::new(),
        slots_total: 2,
        slot_size: Some(g5_internal_api::host_ctl::VmSizeSpec {
            cpus: 2,
            mem_mib: 8192,
            scratch_disk_mib: 81920,
        }),
        inventory: Vec::new(),
        retained_roots: Vec::new(),
        runtime,
    };
    backend
        .accept_hello(
            replacement_id,
            &hello(Some(stale_identity.clone())),
            &HostConnection::new(true),
        )
        .await
        .unwrap();
    let pass = rotate_once(&app.state).await;
    assert_eq!(
        (pass.rotated, pass.forced, pass.replacement_failed),
        (0, 0, 0),
        "a stale replacement of the tracked image neither drains nor dissolves: {pass:?}"
    );
    let (status, _, replaced_by) = host_status(&app, old_host).await;
    assert_eq!(status, "ready", "the current predecessor keeps serving");
    assert_eq!(
        replaced_by,
        Some(replacement_id),
        "the pair waits for a better image"
    );
    assert!(
        !mock.calls().contains(&MockCall::TerminateInstance {
            cloud_instance_id: old_instance.clone()
        }),
        "old instance untouched"
    );

    // A guest-image change is not a runtime change: the pair still waits.
    sqlx::query("UPDATE orgs SET default_base_image_ref = $1 WHERE id = $2")
        .bind(format!(
            "reg.example.com/g5/track@sha256:{}",
            "d".repeat(64)
        ))
        .bind(org.id)
        .execute(&app.db)
        .await
        .unwrap();
    let pass = rotate_once(&app.state).await;
    assert_eq!(
        (pass.rotated, pass.forced, pass.replacement_failed),
        (0, 0, 0),
        "guest image moved, runtime did not: {pass:?}"
    );
    // The worker boot image moves on: the stale replacement is superseded.
    mock.set_boot_image("g5-worker-mock-v2");
    let pass = rotate_once(&app.state).await;
    assert_eq!(pass.replacement_failed, 1, "pair dissolved: {pass:?}");
    assert_eq!(pass.rotated + pass.forced, 0, "{pass:?}");
    let (status, _, replaced_by) = host_status(&app, old_host).await;
    assert_eq!(status, "ready", "the predecessor still serves");
    assert_ne!(
        replaced_by,
        Some(replacement_id),
        "the stale replacement is no longer this host's replacement"
    );
    let (stale_status, _, _) = host_status(&app, replacement_id).await;
    assert_eq!(
        stale_status, "ready",
        "rotation leaves the stale replacement to stale retirement; it holds no VM"
    );
    assert!(
        !mock.calls().contains(&MockCall::TerminateInstance {
            cloud_instance_id: old_instance
        }),
        "old instance never torn down for a stale replacement"
    );
    app.cleanup().await;
}

// ─── Issue #627: recovery after quota changes and cloud failures ─────────────

/// S2/PL-08 (`g5.runtime.host.rotation`): lowering the VM quota below the
/// live automatic host count must not defeat credential-deadline pairing —
/// renewal overlap is not growth. Only undeleted automatic DEBRIS defers the
/// pairing, and that deferral is an explicit, counted, non-destructive wait.
#[tokio::test]
#[serial]
async fn forced_pairing_survives_quota_reduction_and_defers_only_on_cleanup_backlog() {
    let _rot = RotationEnv::enabled();
    let (app, mock) = spawn_with_mock().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    // Two live AUTOMATIC hosts, the older one past the credential cliff.
    let (old_host, _) = provision_host(&app, &client, org.id).await;
    let (busy_host, _) = provision_host(&app, &client, org.id).await;
    for (host, age) in [(old_host, "23 hours"), (busy_host, "1 hour")] {
        sqlx::query(&format!(
            "UPDATE worker_hosts SET status = 'ready', slot_size_reported = true, \
                 last_heartbeat = now(), automatic_capacity = true, \
                 created_at = now() - interval '{age}' \
             WHERE host_id = $1",
        ))
        .bind(host)
        .execute(&app.db)
        .await
        .expect("seed live automatic host");
    }
    // Quota lowered below the existing host count.
    sqlx::query(
        "INSERT INTO configs(org_id,scope,scope_id,key,value) \
         VALUES($1,'org',$1,'max_running_vms','0')",
    )
    .bind(org.id)
    .execute(&app.db)
    .await
    .expect("lower quota");

    let pass = rotate_once(&app.state).await;
    assert_eq!(
        pass.paired, 1,
        "renewal pairing proceeds despite the lowered quota: {pass:?}"
    );
    assert_eq!(pass.pairing_deferred, 0, "{pass:?}");
    let (status, _, replaced_by) = host_status(&app, old_host).await;
    assert_eq!(status, "ready", "old host keeps serving after pairing");
    let replacement_id = replaced_by.expect("pairing marker stamped");
    let replacement_automatic: bool =
        sqlx::query_scalar("SELECT automatic_capacity FROM worker_hosts WHERE host_id = $1")
            .bind(replacement_id)
            .fetch_one(&app.db)
            .await
            .expect("replacement row");
    assert!(
        replacement_automatic,
        "replacement stays automatic capacity"
    );

    // Cleanup backlog: undeleted automatic debris over quota+1 (= 1) defers
    // the NEXT pairing — explicitly, without touching the old host.
    for _ in 0..2 {
        let (debris, _) = provision_host(&app, &client, org.id).await;
        sqlx::query(
            "UPDATE worker_hosts SET status = 'terminated', automatic_capacity = true, \
                 cloud_deleted_at = NULL WHERE host_id = $1",
        )
        .bind(debris)
        .execute(&app.db)
        .await
        .expect("seed undeleted automatic debris");
    }
    sqlx::query(
        "UPDATE worker_hosts SET created_at = now() - interval '23 hours' WHERE host_id = $1",
    )
    .bind(busy_host)
    .execute(&app.db)
    .await
    .expect("age the second host past the cliff");
    let calls_before = mock.calls().len();
    let pass = rotate_once(&app.state).await;
    assert_eq!(
        pass.pairing_deferred, 1,
        "debris over the bound defers pairing: {pass:?}"
    );
    assert_eq!(pass.paired, 0, "{pass:?}");
    let (status, _, replaced_by) = host_status(&app, busy_host).await;
    assert_eq!(status, "ready", "deferred host keeps serving");
    assert_eq!(replaced_by, None, "no pairing marker on a deferral");
    let new_calls = &mock.calls()[calls_before..];
    assert!(
        !new_calls
            .iter()
            .any(|c| matches!(c, MockCall::ProvisionInstance { .. })),
        "a deferral provisions nothing: {new_calls:?}"
    );
    assert!(
        !new_calls
            .iter()
            .any(|c| matches!(c, MockCall::TerminateInstance { .. })),
        "a deferral authorizes no teardown: {new_calls:?}"
    );

    // Debris cleared ⇒ the wait resolves on the next tick.
    sqlx::query(
        "UPDATE worker_hosts SET cloud_deleted_at = now() \
         WHERE org_id = $1 AND status = 'terminated'",
    )
    .bind(org.id)
    .execute(&app.db)
    .await
    .expect("confirm cloud cleanup");
    let pass = rotate_once(&app.state).await;
    assert_eq!(
        pass.paired, 1,
        "pairing resumes once debris clears: {pass:?}"
    );
    app.cleanup().await;
}

/// S1 (`g5.runtime.host.rotation`): an unreachable PAIRED predecessor drains
/// through the forced lane once its replacement is ready — the pair cannot
/// freeze, and the successor's own renewal is unblocked.
#[tokio::test]
#[serial]
async fn await_sweep_drains_unreachable_paired_predecessor_and_unblocks_successor() {
    let _rot = RotationEnv::enabled();
    let (app, mock) = spawn_with_mock().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    // Tracked org + image-stale idle host ⇒ the idle lane pairs it.
    let (old_host, old_instance) = provision_host(&app, &client, org.id).await;
    sqlx::query(
        "UPDATE worker_hosts SET status = 'ready', slot_size_reported = true, \
             last_heartbeat = now(), created_at = now() - interval '6 hours', \
             last_active_at = NULL, base_image_ref = NULL, base_image_digest = NULL \
         WHERE host_id = $1",
    )
    .bind(old_host)
    .execute(&app.db)
    .await
    .expect("seed stale idle host");
    let digest = format!("sha256:{}", "d".repeat(64));
    let image_ref = format!("reg.example.com/g5/track@{digest}");
    sqlx::query(
        "UPDATE orgs SET default_base_image_track = 'track', default_base_image_ref = $1 \
         WHERE id = $2",
    )
    .bind(&image_ref)
    .bind(org.id)
    .execute(&app.db)
    .await
    .expect("tracking org");
    let pass = rotate_once(&app.state).await;
    assert_eq!(pass.paired, 1, "paired: {pass:?}");
    let (_, _, replaced_by) = host_status(&app, old_host).await;
    let replacement_id = replaced_by.expect("pairing marker stamped");
    wait_for_cloud_instance_id(&app, replacement_id).await;

    // The old host goes silent: the registry marks it unreachable.
    sqlx::query(
        "UPDATE worker_hosts SET status = 'unreachable', \
             last_heartbeat = now() - interval '2 minutes' WHERE host_id = $1",
    )
    .bind(old_host)
    .execute(&app.db)
    .await
    .expect("old host unreachable");

    // Replacement publishes and becomes ready.
    sqlx::query("UPDATE worker_hosts SET status='ready', last_heartbeat=now() WHERE host_id=$1")
        .bind(replacement_id)
        .execute(&app.db)
        .await
        .unwrap();
    use g5_api::ws::host_ctl::{ControlPlaneHostState, HostConnection};
    let backend = app.host_control_backend();
    backend
        .accept_hello(
            replacement_id,
            &g5_internal_api::host_ctl::HostHello {
                host_id: replacement_id,
                protocol_version: g5_internal_api::host_ctl::PROTOCOL_VERSION,
                capabilities: Vec::new(),
                slots_total: 2,
                slot_size: Some(g5_internal_api::host_ctl::VmSizeSpec {
                    cpus: 2,
                    mem_mib: 8192,
                    scratch_disk_mib: 81920,
                }),
                inventory: Vec::new(),
                retained_roots: Vec::new(),
                runtime: Some(g5_internal_api::host_ctl::HostRuntimeIdentity::bundled()),
            },
            &HostConnection::new(true),
        )
        .await
        .unwrap();

    // The await sweep drains the UNREACHABLE predecessor via the forced lane.
    let pass = rotate_once(&app.state).await;
    assert_eq!(
        pass.rotated, 1,
        "unreachable paired predecessor drained once its replacement is ready: {pass:?}"
    );
    let (status, _, _) = host_status(&app, old_host).await;
    assert_eq!(status, "terminated", "predecessor finalized");
    assert!(
        mock.calls().contains(&MockCall::TerminateInstance {
            cloud_instance_id: old_instance
        }),
        "predecessor instance torn down"
    );

    // Successor renewal unblocked: with the predecessor terminated, the chain
    // guard admits the successor's own credential-deadline pairing.
    sqlx::query(
        "UPDATE worker_hosts SET created_at = now() - interval '23 hours' WHERE host_id = $1",
    )
    .bind(replacement_id)
    .execute(&app.db)
    .await
    .expect("age successor past the cliff");
    let pass = rotate_once(&app.state).await;
    assert_eq!(pass.paired, 1, "successor renewal proceeds: {pass:?}");
    assert_eq!(pass.pairing_deferred, 0, "{pass:?}");
    let (_, _, successor_replaced_by) = host_status(&app, replacement_id).await;
    assert!(
        successor_replaced_by.is_some(),
        "successor paired with its own replacement"
    );
    app.cleanup().await;
}

/// S1 (`g5.runtime.host.rotation` / [D65] Pass 2): a silent UNREACHABLE host
/// is reaped and replaced. The registry flips `ready → unreachable` after 30s
/// of silence, so a reap lane pinned to `ready` could never match in practice.
#[tokio::test]
#[serial]
async fn detector_reaps_silent_unreachable_host_and_replaces() {
    let _env = DetectorEnv::enabled();
    let (app, mock) = spawn_with_mock().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    let (host_id, instance_id) = provision_host(&app, &client, org.id).await;
    sqlx::query(
        "UPDATE worker_hosts SET status = 'unreachable', slot_size_reported = true, \
             last_heartbeat = now() - interval '25 minutes' WHERE host_id = $1",
    )
    .bind(host_id)
    .execute(&app.db)
    .await
    .expect("seed silent unreachable host");

    let pass = detect_once(&app.state).await;
    assert_eq!(pass.silent_reaped, 1, "unreachable host reaped: {pass:?}");
    let (status, _, _) = host_status(&app, host_id).await;
    assert_eq!(status, "terminated");
    assert!(
        mock.calls().contains(&MockCall::TerminateInstance {
            cloud_instance_id: instance_id
        }),
        "silent host's instance torn down"
    );
    // The replacement ROW is committed synchronously by the reap (its cloud
    // bring-up is detached and may still be in flight here).
    let replacements: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM worker_hosts \
         WHERE org_id = $1 AND status = 'provisioning' AND host_id <> $2",
    )
    .bind(org.id)
    .bind(host_id)
    .fetch_one(&app.db)
    .await
    .expect("count replacements");
    assert_eq!(replacements, 1, "capacity self-heals with a replacement");
    app.cleanup().await;
}

/// S18 (`g5.runtime.host.cloud-linkage`): the retry claim consumes the stale
/// classification WITH the schedule, so a follow-up UNCLASSIFIED failure
/// leaves a plain unlinked provisioning row that ordinary reconciliation
/// bounds (`abandoned_bringup`) — never a badge no selector covers.
#[tokio::test]
#[serial]
async fn stockout_then_unclassified_failure_reaches_bounded_recovery() {
    let _env = DetectorEnv::with_retry("us-east1,us-west1");
    let (app, mock) = spawn_with_mock().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    mock.fail_provision_stockout(true);
    let host_id = provision_host_row_only(&client, org.id).await;
    wait_for_host_condition(
        &app,
        host_id,
        "SELECT provision_error_code = 'zone_capacity_exhausted' \
             AND provision_next_retry_at IS NOT NULL \
         FROM worker_hosts WHERE host_id = $1",
        "stockout classified + retry scheduled",
    )
    .await;

    // The next attempt fails with a GENERIC 500 — not a stockout.
    mock.fail_provision_stockout(false);
    mock.fail_provision(true);
    sqlx::query("UPDATE worker_hosts SET provision_next_retry_at = now() WHERE host_id = $1")
        .bind(host_id)
        .execute(&app.db)
        .await
        .expect("make retry due");
    let pass = detect_once(&app.state).await;
    assert_eq!(pass.retried, 1, "attempt claimed: {pass:?}");
    wait_for_host_condition(
        &app,
        host_id,
        "SELECT provision_error_code IS NULL AND provision_next_retry_at IS NULL \
             AND cloud_instance_id IS NULL AND status = 'provisioning' \
         FROM worker_hosts WHERE host_id = $1",
        "claim consumed classification + schedule; unclassified failure left a plain row",
    )
    .await;
    // Let the detached retry bring-up finish (its [D60] heartbeat stops with
    // it) so the lease NULLed below cannot be re-stamped by a straggler.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let provision_calls = mock
            .calls()
            .iter()
            .filter(|c| matches!(c, MockCall::ProvisionInstance { .. }))
            .count();
        if provision_calls >= 2 {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "retry bring-up never reached the provider"
        );
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    // Ordinary reconciliation now bounds the row: lease lapsed, instance
    // confirmed missing at the threshold ⇒ explicit terminal failure.
    sqlx::query(
        "UPDATE worker_hosts SET created_at = now() - interval '120 seconds', \
             bringup_heartbeat_at = NULL, orphan_check_misses = $2 - 1 \
         WHERE host_id = $1",
    )
    .bind(host_id)
    .bind(g5_api::cloud::reconciler::ORPHAN_TERMINATE_MISS_THRESHOLD)
    .execute(&app.db)
    .await
    .expect("age into the orphan window at threshold-1 misses");
    let orphan = g5_api::cloud::reconciler::reconcile_orphans_once(app.state.as_ref()).await;
    assert_eq!(
        orphan.terminated_abandoned, 1,
        "bounded failure: {orphan:?}"
    );
    let (status, code, _) = host_status(&app, host_id).await;
    assert_eq!(status, "terminated");
    assert_eq!(code.as_deref(), Some("abandoned_bringup"));
    app.cleanup().await;
}

/// S30 (`g5.runtime.host.cloud-linkage`): with no configured placement region
/// (default us-central1), the basket's other regions are regions placement
/// will never use — attempts 3+ stay in-region instead of buying unusable
/// capacity.
#[tokio::test]
#[serial]
async fn retry_fallback_respects_the_placement_region() {
    let _env = DetectorEnv::with_retry("us-east1,us-west1");
    let (app, mock) = spawn_with_mock().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    mock.fail_provision_stockout(true);
    let host_id = provision_host_row_only(&client, org.id).await;
    wait_for_host_condition(
        &app,
        host_id,
        "SELECT provision_error_code = 'zone_capacity_exhausted' \
             AND provision_next_retry_at IS NOT NULL \
         FROM worker_hosts WHERE host_id = $1",
        "stockout classified + retry scheduled",
    )
    .await;
    // Jump straight to attempt 3 territory: two attempts already consumed.
    sqlx::query(
        "UPDATE worker_hosts SET provision_retry_count = 2, provision_next_retry_at = now() \
         WHERE host_id = $1",
    )
    .bind(host_id)
    .execute(&app.db)
    .await
    .expect("seed attempt counter");
    mock.fail_provision_stockout(false);
    let pass = detect_once(&app.state).await;
    assert_eq!(pass.retried, 1, "attempt 3 claimed: {pass:?}");
    wait_for_host_condition(
        &app,
        host_id,
        "SELECT cloud_instance_id IS NOT NULL FROM worker_hosts WHERE host_id = $1",
        "attempt 3 succeeded",
    )
    .await;
    let region: String = sqlx::query_scalar("SELECT region FROM worker_hosts WHERE host_id = $1")
        .bind(host_id)
        .fetch_one(&app.db)
        .await
        .expect("region");
    assert_eq!(
        region, "us-central1",
        "attempt 3 stays in the placement region instead of walking the basket"
    );
    assert!(
        !mock.calls().iter().any(|c| matches!(c,
            MockCall::ProvisionInstance { region, .. } if region != "us-central1")),
        "no insert ever targeted a region placement will not use"
    );
    app.cleanup().await;
}

/// Issue #627 Codex review: the silent reap requires CLOUD OWNERSHIP. A
/// self-hosted registration (no cloud linkage) that goes unreachable must
/// never be finalized by the detector — termination would permanently refuse
/// its reconnection and could provision an unintended cloud replacement
/// (`g5.architecture.host_idle_retirement`: never automatically delete
/// self-hosted hosts). Unlinked CLOUD rows belong to the orphan machinery.
#[tokio::test]
#[serial]
async fn detector_never_reaps_a_silent_host_without_cloud_linkage() {
    let _env = DetectorEnv::enabled();
    let (app, mock) = spawn_with_mock().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    let (host_id, _) = provision_host(&app, &client, org.id).await;
    sqlx::query(
        "UPDATE worker_hosts SET cloud_instance_id = NULL, vpc_id = NULL, \
             status = 'unreachable', slot_size_reported = true, \
             last_heartbeat = now() - interval '25 minutes' WHERE host_id = $1",
    )
    .bind(host_id)
    .execute(&app.db)
    .await
    .expect("seed a silent unlinked host");

    let calls_before = mock.calls().len();
    let pass = detect_once(&app.state).await;
    assert_eq!(
        pass.silent_reaped, 0,
        "no reap without cloud ownership: {pass:?}"
    );
    let (status, _, _) = host_status(&app, host_id).await;
    assert_eq!(status, "unreachable", "registration preserved");
    let new_calls = &mock.calls()[calls_before..];
    assert!(
        !new_calls
            .iter()
            .any(|c| matches!(c, MockCall::ProvisionInstance { .. })),
        "no replacement provisioned for an unowned host: {new_calls:?}"
    );
    app.cleanup().await;
}

/// Issue #627 CodeRabbit (Major): a rotation leftover must not be finalized
/// while its replacement is deferred for cloud-cleanup backlog — that would
/// end the org's one-for-one renewal with no lane left to retry it. The row
/// stays draining; once cleanup confirms, the sweep provisions AND finalizes.
#[tokio::test]
#[serial]
async fn a_draining_leftover_waits_out_the_cloud_cleanup_backlog() {
    let _rot = RotationEnv::enabled();
    let (app, mock) = spawn_with_mock().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    // An automatic rotation leftover: draining, no drain id, no marker.
    let (leftover, leftover_instance) = provision_host(&app, &client, org.id).await;
    sqlx::query(
        "UPDATE worker_hosts SET status = 'draining', automatic_capacity = true \
         WHERE host_id = $1",
    )
    .bind(leftover)
    .execute(&app.db)
    .await
    .expect("seed rotation leftover");
    // Quota 0 ⇒ debris bound is 1; two undeleted automatic rows exceed it.
    sqlx::query(
        "INSERT INTO configs(org_id,scope,scope_id,key,value) \
         VALUES($1,'org',$1,'max_running_vms','0')",
    )
    .bind(org.id)
    .execute(&app.db)
    .await
    .expect("lower quota");
    for _ in 0..2 {
        let (debris, _) = provision_host(&app, &client, org.id).await;
        sqlx::query(
            "UPDATE worker_hosts SET status = 'terminated', automatic_capacity = true, \
                 cloud_deleted_at = NULL WHERE host_id = $1",
        )
        .bind(debris)
        .execute(&app.db)
        .await
        .expect("seed undeleted automatic debris");
    }

    let pass = rotate_once(&app.state).await;
    assert_eq!(pass.pairing_deferred, 1, "deferred on backlog: {pass:?}");
    assert_eq!(pass.resumed, 0, "not finalized while deferred: {pass:?}");
    let (status, _, _) = host_status(&app, leftover).await;
    assert_eq!(status, "draining", "leftover stays retryable");
    assert!(
        !mock.calls().contains(&MockCall::TerminateInstance {
            cloud_instance_id: leftover_instance.clone()
        }),
        "no teardown while the replacement is deferred"
    );

    // Cleanup confirms ⇒ the next pass provisions the replacement and
    // finalizes the leftover.
    sqlx::query(
        "UPDATE worker_hosts SET cloud_deleted_at = now() \
         WHERE org_id = $1 AND status = 'terminated'",
    )
    .bind(org.id)
    .execute(&app.db)
    .await
    .expect("confirm cloud cleanup");
    let pass = rotate_once(&app.state).await;
    assert_eq!(
        pass.resumed, 1,
        "resumed once the backlog cleared: {pass:?}"
    );
    let (status, _, _) = host_status(&app, leftover).await;
    assert_eq!(
        status, "terminated",
        "leftover finalized with a replacement"
    );
    assert!(
        mock.calls().contains(&MockCall::TerminateInstance {
            cloud_instance_id: leftover_instance
        }),
        "teardown ran with the replacement provisioned"
    );
    let replacements: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM worker_hosts \
         WHERE org_id = $1 AND status = 'provisioning'",
    )
    .bind(org.id)
    .fetch_one(&app.db)
    .await
    .expect("count replacements");
    assert_eq!(replacements, 1, "one-for-one replacement provisioned");
    app.cleanup().await;
}

/// Issue #627 Codex PR review P1: the TTL−15min deadline drain is independent
/// of the replacement's fate. Under a PERSISTENT terminal failure (stockout
/// with retry disabled) every pass used to dissolve/re-pair without ever
/// reaching the deadline branch, so the old host sailed past its credential's
/// death. Custody still gates destructive teardown: a busy old host quiesces
/// but is torn down only after its VM releases.
#[tokio::test]
#[serial]
async fn deadline_drain_survives_persistent_replacement_failure() {
    let _rot = RotationEnv::enabled();
    let (app, mock) = spawn_with_mock().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    // ── Busy host: quiesce at the deadline, no teardown before VM release.
    let (busy_host, busy_instance) = provision_host(&app, &client, org.id).await;
    sqlx::query(
        "UPDATE worker_hosts SET status = 'ready', slot_size_reported = true, \
             last_heartbeat = now(), created_at = now() - interval '23 hours 50 minutes' \
         WHERE host_id = $1",
    )
    .bind(busy_host)
    .execute(&app.db)
    .await
    .expect("seed past-deadline host");
    let workspace = helpers::TestWorkspace::create(&app, &org).await;
    let vm_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO workspace_vms(
             org_id, workspace_id, host_id, vm_id, epoch, status, lease_expires_at)
         VALUES ($1,$2,$3,$4,1,'running',now() + interval '5 minutes')",
    )
    .bind(org.id)
    .bind(workspace.id)
    .bind(busy_host)
    .bind(vm_id.to_string())
    .execute(&app.db)
    .await
    .expect("seed active VM custody");
    // Every replacement insert stocks out, terminally (retry disabled).
    mock.fail_provision_stockout(true);

    let pass = rotate_once(&app.state).await;
    assert_eq!(pass.paired, 1, "force pairs the past-cliff host: {pass:?}");
    let (_, _, replaced_by) = host_status(&app, busy_host).await;
    let replacement = replaced_by.expect("pairing marker stamped");
    wait_for_host_condition(
        &app,
        replacement,
        "SELECT provision_error_code = 'zone_capacity_exhausted' \
         FROM worker_hosts WHERE host_id = $1",
        "replacement classified stockout",
    )
    .await;

    let pass = rotate_once(&app.state).await;
    assert_eq!(pass.replacement_failed, 1, "pair dissolved: {pass:?}");
    assert_eq!(pass.forced, 0, "busy host not torn down: {pass:?}");
    let (status, _, _) = host_status(&app, busy_host).await;
    assert_eq!(
        status, "draining",
        "deadline quiesce ran despite the failed replacement"
    );
    assert!(
        !mock.calls().contains(&MockCall::TerminateInstance {
            cloud_instance_id: busy_instance.clone()
        }),
        "teardown never precedes VM release"
    );

    // The VM exits through the PRODUCTION release path — custody columns and
    // related state settle together — and the resumability sweep finishes
    // the drain.
    {
        use g5_api::ws::host_ctl::ControlPlaneHostState;
        use g5_internal_api::host_ctl::{VmEvent, VmEventKind};
        app.host_control_backend()
            .ingest_vm_event(
                busy_host,
                &VmEvent {
                    workspace_id: workspace.id,
                    vm_id,
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
            .expect("release custody via the exit event");
    }
    let pass = rotate_once(&app.state).await;
    assert_eq!(pass.resumed, 1, "drain completed after release: {pass:?}");
    let (status, _, _) = host_status(&app, busy_host).await;
    assert_eq!(status, "terminated");
    assert!(
        mock.calls().contains(&MockCall::TerminateInstance {
            cloud_instance_id: busy_instance
        }),
        "busy host torn down only after its VM released"
    );

    // ── Idle host: dissolve AND deadline-drain in the same pass, with the
    // stockout still failing every replacement.
    mock.fail_provision_stockout(false);
    let (idle_host, idle_instance) = provision_host(&app, &client, org.id).await;
    mock.fail_provision_stockout(true);
    sqlx::query(
        "UPDATE worker_hosts SET status = 'ready', slot_size_reported = true, \
             last_heartbeat = now(), created_at = now() - interval '23 hours 50 minutes' \
         WHERE host_id = $1",
    )
    .bind(idle_host)
    .execute(&app.db)
    .await
    .expect("seed past-deadline idle host");
    let pass = rotate_once(&app.state).await;
    assert_eq!(pass.paired, 1, "idle host paired: {pass:?}");
    let (_, _, replaced_by) = host_status(&app, idle_host).await;
    let replacement = replaced_by.expect("pairing marker stamped");
    wait_for_host_condition(
        &app,
        replacement,
        "SELECT provision_error_code = 'zone_capacity_exhausted' \
         FROM worker_hosts WHERE host_id = $1",
        "replacement classified stockout",
    )
    .await;
    let pass = rotate_once(&app.state).await;
    assert_eq!(
        (pass.replacement_failed, pass.forced),
        (1, 1),
        "dissolution and deadline drain in one pass: {pass:?}"
    );
    let (status, _, _) = host_status(&app, idle_host).await;
    assert_eq!(
        status, "terminated",
        "old host drained before credential death"
    );
    assert!(
        mock.calls().contains(&MockCall::TerminateInstance {
            cloud_instance_id: idle_instance
        }),
        "idle past-deadline host torn down despite the failed replacement"
    );
    app.cleanup().await;
}
/// Idle rotation locks the host row and re-reads custody before draining, so a
/// placement that commits while the quiesce waits for the lock keeps the host
/// serving (`g5.runtime.host.rotation`, G5-Lean#626 S22). The pre-fix single
/// `UPDATE … WHERE NOT EXISTS` read its snapshot before the lock and drained
/// under the freshly placed VM.
#[tokio::test]
#[serial]
async fn idle_quiesce_yields_to_a_placement_that_commits_while_it_waits() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = helpers::TestWorkspace::create(&app, &org).await;
    let host_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO worker_hosts \
         (host_id, org_id, region, status, last_heartbeat, slot_size_reported, \
          slots_total, slot_cpus, slot_mem_mib, scratch_disk_mib, runtime_identity) \
         VALUES ($1, $2, 'us-central1', 'ready', now(), true, 4, 2, 4096, 10240, '{}'::jsonb)",
    )
    .bind(host_id)
    .bind(org.id)
    .execute(&app.db)
    .await
    .expect("insert an idle ready host");

    // A transaction locks the host row and, still holding it, places a VM.
    let mut placement = app.db.begin().await.expect("begin the racing placement");
    sqlx::query("SELECT status FROM worker_hosts WHERE org_id = $1 AND host_id = $2 FOR UPDATE")
        .bind(org.id)
        .bind(host_id)
        .execute(&mut *placement)
        .await
        .expect("hold the host row ahead of the quiesce");

    // The quiesce blocks on the host-row lock the placement holds.
    let quiesce = tokio::spawn({
        let state = app.state.clone();
        let org_id = org.id;
        async move {
            g5_api::cloud::rotation::idle_quiesce_host_for_test(
                &state,
                org_id,
                host_id,
                "us-central1",
            )
            .await
        }
    });
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let waiting: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM pg_stat_activity
                 WHERE datname = current_database() AND wait_event_type = 'Lock'
                   AND query LIKE 'SELECT status%FROM worker_hosts%')",
            )
            .fetch_one(&app.db)
            .await
            .expect("read lock waits");
            if waiting {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the quiesce reaches the host-row lock");

    // The placement commits its starting VM, then releases the row lock.
    sqlx::query(
        "INSERT INTO workspace_vms \
         (org_id, workspace_id, host_id, vm_id, epoch, status, lease_expires_at, \
          start_command_sent_at) \
         VALUES ($1, $2, $3, $4, 1, 'starting', now() + interval '5 minutes', now())",
    )
    .bind(org.id)
    .bind(workspace.id)
    .bind(host_id)
    .bind(Uuid::new_v4().to_string())
    .execute(&mut *placement)
    .await
    .expect("place a starting VM under the held row lock");
    placement
        .commit()
        .await
        .expect("commit the racing placement");

    let drained = quiesce
        .await
        .expect("the quiesce task joins")
        .expect("the quiesce query runs");
    assert!(
        !drained,
        "a VM placed while the quiesce waited must keep the host serving"
    );
    let status: String = sqlx::query_scalar("SELECT status FROM worker_hosts WHERE host_id = $1")
        .bind(host_id)
        .fetch_one(&app.db)
        .await
        .expect("read host status");
    assert_eq!(status, "ready", "the host is not drained under a live VM");
    app.cleanup().await;
}

/// An authenticated complete hello that omits a `starting` incarnation does
/// not release it while its Start was never recorded as sent: that incarnation
/// is still the sender's to settle (`g5.runtime.start.settlement`, G5-Lean#626
/// D2/H1). Once the Start is recorded as sent, the same absence releases it.
#[tokio::test]
#[serial]
async fn hello_absence_spares_an_unsent_start_and_releases_a_sent_one() {
    use g5_api::ws::host_ctl::{ControlPlaneHostState, HostConnection};

    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let host_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO worker_hosts          (host_id, org_id, region, status, last_heartbeat, slot_size_reported,           slots_total, slot_cpus, slot_mem_mib, scratch_disk_mib, runtime_identity)          VALUES ($1, $2, 'us-central1', 'ready', now(), true, 4, 2, 4096, 10240, '{}'::jsonb)",
    )
    .bind(host_id)
    .bind(org.id)
    .execute(&app.db)
    .await
    .expect("insert a ready host");
    let unsent = helpers::TestWorkspace::create(&app, &org).await;
    let sent = helpers::TestWorkspace::create(&app, &org).await;
    let seed = |workspace_id: Uuid, owner_id: Uuid, stamped: bool| {
        let pool = app.db.clone();
        async move {
            let now = chrono::Utc::now();
            let intent = json!({
                "id": Uuid::new_v4(),
                "kind": "resume",
                "owner_user_id": owner_id,
                "lifecycle_generation": 0,
                "status": "starting",
                "created_at": now,
                "expires_at": now + chrono::Duration::minutes(15),
                "attempts": 1,
            });
            sqlx::query(
                "INSERT INTO workspace_vms \
                 (org_id, workspace_id, host_id, vm_id, epoch, status, start_intent, \
                  lease_expires_at, start_deadline_at, start_command_sent_at) \
                 VALUES ($1, $2, $3, $4, 1, 'starting', $5, \
                   now() + interval '5 minutes', now() + interval '1 minute', \
                   CASE WHEN $6 THEN now() ELSE NULL END)",
            )
            .bind(org.id)
            .bind(workspace_id)
            .bind(host_id)
            .bind(Uuid::new_v4().to_string())
            .bind(intent)
            .bind(stamped)
            .execute(&pool)
            .await
            .expect("seed a starting incarnation");
        }
    };
    seed(unsent.id, user.id, false).await;
    seed(sent.id, user.id, true).await;

    let backend = app.host_control_backend();
    backend
        .accept_hello(
            host_id,
            &legacy_inventory_hello(host_id, Vec::new()),
            &HostConnection::new(true),
        )
        .await
        .expect("accept a complete empty hello");

    let unsent_status: String =
        sqlx::query_scalar("SELECT status FROM workspace_vms WHERE workspace_id = $1")
            .bind(unsent.id)
            .fetch_one(&app.db)
            .await
            .expect("read the unsent incarnation");
    assert_eq!(
        unsent_status, "starting",
        "an unsent Start is not released on hello absence"
    );
    let sent_status: String =
        sqlx::query_scalar("SELECT status FROM workspace_vms WHERE workspace_id = $1")
            .bind(sent.id)
            .fetch_one(&app.db)
            .await
            .expect("read the sent incarnation");
    assert_eq!(
        sent_status, "failed",
        "a sent Start absent from a complete hello is released"
    );
    // The released row's live Start intent fails with it, while the spared
    // row's intent is still the one being started.
    let intents: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT workspace_id, start_intent ->> 'status' FROM workspace_vms
         WHERE workspace_id = ANY($1)",
    )
    .bind(vec![unsent.id, sent.id])
    .fetch_all(&app.db)
    .await
    .expect("read the start intents");
    let intent_of = |id: Uuid| {
        intents
            .iter()
            .find(|(w, _)| *w == id)
            .map(|(_, s)| s.as_str())
    };
    assert_eq!(intent_of(unsent.id), Some("starting"));
    assert_eq!(
        intent_of(sent.id),
        Some("failed"),
        "a lost starting incarnation does not leave a live starting intent"
    );

    // An exit event for a sent, starting incarnation releases it the same way:
    // the row fails and takes its live intent with it.
    use g5_internal_api::host_ctl::{VmEvent, VmEventKind};
    let exited = helpers::TestWorkspace::create(&app, &org).await;
    let exited_vm = Uuid::new_v4();
    let now = chrono::Utc::now();
    let intent = json!({
        "id": Uuid::new_v4(),
        "kind": "resume",
        "owner_user_id": user.id,
        "lifecycle_generation": 0,
        "status": "starting",
        "created_at": now,
        "expires_at": now + chrono::Duration::minutes(15),
        "attempts": 1,
    });
    sqlx::query(
        "INSERT INTO workspace_vms \
         (org_id, workspace_id, host_id, vm_id, epoch, status, start_intent, \
          lease_expires_at, start_deadline_at, start_command_sent_at) \
         VALUES ($1, $2, $3, $4, 1, 'starting', $5, \
           now() + interval '5 minutes', now() + interval '1 minute', now())",
    )
    .bind(org.id)
    .bind(exited.id)
    .bind(host_id)
    .bind(exited_vm.to_string())
    .bind(intent)
    .execute(&app.db)
    .await
    .expect("seed a starting incarnation that will exit");
    backend
        .ingest_vm_event(
            host_id,
            &VmEvent {
                workspace_id: exited.id,
                vm_id: exited_vm,
                seq: 1,
                kind: VmEventKind::Exited {
                    code: Some(1),
                    signal: None,
                },
                at: chrono::Utc::now(),
                log_tail: None,
            },
        )
        .await
        .expect("ingest the exit");
    let (status, intent_status): (String, String) = sqlx::query_as(
        "SELECT status, start_intent ->> 'status' FROM workspace_vms WHERE workspace_id = $1",
    )
    .bind(exited.id)
    .fetch_one(&app.db)
    .await
    .expect("read the exited incarnation");
    assert_eq!(
        (status.as_str(), intent_status.as_str()),
        ("failed", "failed"),
        "an exit during boot fails the row and its intent together"
    );
    app.cleanup().await;
}

/// The occupancy the capacity sweep attributes to a host: durable custody plus
/// reported ids no current incarnation accounts for.
async fn host_occupancy(app: &TestApp, host_id: Uuid) -> i64 {
    sqlx::query_scalar(
        "SELECT (SELECT count(*) FROM workspace_vms vm WHERE vm.host_id = h.host_id
                    AND vm.status IN ('starting','running','stopping'))
              + (SELECT count(*) FROM unnest(h.reported_live_vm_ids) live(id)
                    WHERE NOT EXISTS (SELECT 1 FROM workspace_vms vm WHERE vm.host_id = h.host_id
                        AND vm.vm_id = live.id::text
                        AND vm.status IN ('starting','running','stopping')))
         FROM worker_hosts h WHERE h.host_id = $1",
    )
    .bind(host_id)
    .fetch_one(&app.db)
    .await
    .expect("read host occupancy")
}

async fn reported_ids(app: &TestApp, host_id: Uuid) -> Vec<Uuid> {
    sqlx::query_scalar::<_, Option<Vec<Uuid>>>(
        "SELECT reported_live_vm_ids FROM worker_hosts WHERE host_id = $1",
    )
    .bind(host_id)
    .fetch_one(&app.db)
    .await
    .expect("read reported inventory")
    .unwrap_or_default()
}

fn starting_intent(owner_id: Uuid) -> Value {
    let now = chrono::Utc::now();
    json!({
        "id": Uuid::new_v4(),
        "kind": "resume",
        "owner_user_id": owner_id,
        "lifecycle_generation": 0,
        "status": "starting",
        "created_at": now,
        "expires_at": now + chrono::Duration::minutes(15),
        "attempts": 1,
    })
}

/// A reconnect hello advertises a pending Start as booting; when the host then
/// refuses that exact Start, the settled release retracts its id from the
/// host's reported inventory on the same live connection, so the host's
/// occupancy and usable capacity recover without another hello
/// (`g5.runtime.start.settlement`, G5-Lean#626 review P2).
#[tokio::test]
#[serial]
async fn a_settled_refusal_retracts_the_pending_start_a_hello_advertised() {
    use g5_api::ws::host_ctl::{ControlPlaneHostState, HostConnection};
    use g5_internal_api::host_ctl::{HostVmState, VmInventory};

    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let host_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO worker_hosts \
         (host_id, org_id, region, status, last_heartbeat, slot_size_reported, \
          slots_total, slot_cpus, slot_mem_mib, scratch_disk_mib, runtime_identity) \
         VALUES ($1, $2, 'us-central1', 'ready', now(), true, 4, 2, 4096, 10240, '{}'::jsonb)",
    )
    .bind(host_id)
    .bind(org.id)
    .execute(&app.db)
    .await
    .expect("insert a ready host");
    let workspace = helpers::TestWorkspace::create(&app, &org).await;
    let vm_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO workspace_vms \
         (org_id, workspace_id, host_id, vm_id, epoch, status, start_intent, \
          lease_expires_at, start_deadline_at, start_command_sent_at) \
         VALUES ($1, $2, $3, $4, 1, 'starting', $5, \
           now() + interval '5 minutes', now() + interval '1 minute', now())",
    )
    .bind(org.id)
    .bind(workspace.id)
    .bind(host_id)
    .bind(vm_id.to_string())
    .bind(starting_intent(user.id))
    .execute(&app.db)
    .await
    .expect("seed a sent, pending Start");

    // The host reconnects while the Start is still pending and advertises it.
    app.host_control_backend()
        .accept_hello(
            host_id,
            &legacy_inventory_hello(
                host_id,
                vec![VmInventory {
                    workspace_id: workspace.id,
                    vm_id,
                    state: HostVmState::Booting,
                    boot_at: chrono::Utc::now(),
                }],
            ),
            &HostConnection::new(true),
        )
        .await
        .expect("accept the hello advertising the pending Start");
    assert_eq!(reported_ids(&app, host_id).await, vec![vm_id]);
    assert_eq!(host_occupancy(&app, host_id).await, 1);

    // The host refuses that exact Start before reserving it. Its settled
    // release retracts the advertised id; no further hello is needed.
    let mut db = app.scoped_db_for_org(org.id).await;
    assert!(app
        .state
        .worker_host_service
        .release_placement(
            &mut db,
            &g5_api::services::worker_host_service::PlacementReservation {
                host_id,
                vm_id,
                epoch: 1,
                occupancy_before: 0,
                previous_activity_at: None,
            },
            org.id,
            workspace.id,
            true,
            "host_capacity_refused",
            Some("slots"),
        )
        .await
        .expect("release the refused placement"));
    db.commit().await.expect("commit the release");
    let status: String =
        sqlx::query_scalar("SELECT status FROM workspace_vms WHERE workspace_id = $1")
            .bind(workspace.id)
            .fetch_one(&app.db)
            .await
            .expect("read the released row");
    assert_eq!(status, "stopped");
    assert!(
        reported_ids(&app, host_id).await.is_empty(),
        "the refused Start no longer counts as reported inventory"
    );
    assert_eq!(
        host_occupancy(&app, host_id).await,
        0,
        "usable capacity recovers without another hello"
    );
    app.cleanup().await;
}

/// The same retraction through the Stop path: a Stop for a Start the host
/// cancelled before reservation is answered absent, releases the incarnation
/// as stopped, and retracts the id the hello had advertised
/// (`g5.runtime.start.settlement`, G5-Lean#626 review P2).
#[tokio::test]
#[serial]
async fn a_stop_answered_absent_retracts_the_pending_start_a_hello_advertised() {
    use g5_api::ws::host_ctl::{ControlPlaneHostState, HostConnection};
    use g5_internal_api::host_ctl::{CommandResult, HostVmState, VmInventory};

    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = helpers::TestWorkspace::create(&app, &org).await;
    // The scripted host answers the Stop `NotFound`: it cancelled the pending
    // Start, so nothing can admit the incarnation afterwards.
    let runtime = helpers::TestWorkspaceRuntimePeer::attach_with_vm_stop_result(
        &app,
        &workspace,
        owner.id,
        CommandResult::NotFound,
    )
    .await;
    sqlx::query(
        "UPDATE workspace_vms
         SET status = 'starting', started_at = NULL, start_intent = $2,
             start_command_sent_at = now(), start_deadline_at = now() + interval '1 minute'
         WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .bind(starting_intent(owner.id))
    .execute(&app.db)
    .await
    .expect("turn the incarnation into a sent, pending Start");
    app.host_control_backend()
        .accept_hello(
            runtime.host_id(),
            &legacy_inventory_hello(
                runtime.host_id(),
                vec![VmInventory {
                    workspace_id: workspace.id,
                    vm_id: runtime.vm_id(),
                    state: HostVmState::Booting,
                    boot_at: chrono::Utc::now(),
                }],
            ),
            &HostConnection::new(true),
        )
        .await
        .expect("accept the hello advertising the pending Start");
    assert_eq!(
        reported_ids(&app, runtime.host_id()).await,
        vec![runtime.vm_id()]
    );

    let response = app
        .authenticated_client(&owner)
        .post_json(
            &format!("/api/v1/workspaces/{}/stop", workspace.id),
            &json!({"operationId": Uuid::new_v4()}),
        )
        .await;
    assert!(response.status().is_success(), "stop admitted");
    let status: String =
        sqlx::query_scalar("SELECT status FROM workspace_vms WHERE workspace_id = $1")
            .bind(workspace.id)
            .fetch_one(&app.db)
            .await
            .expect("read the stopped row");
    assert_eq!(status, "stopped", "the absent Start settles as stopped");
    assert!(
        reported_ids(&app, runtime.host_id()).await.is_empty(),
        "the cancelled Start no longer counts as reported inventory"
    );
    assert_eq!(host_occupancy(&app, runtime.host_id()).await, 0);
    app.cleanup().await;
}

#[tokio::test]
#[serial]
async fn replacement_refuses_incompatible_retained_override_without_draining_a_running_vm() {
    let _rot = RotationEnv::enabled();
    let (app, mock) = spawn_with_mock().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);
    let workspace = helpers::TestWorkspace::create(&app, &org).await;
    let (host, _) = provision_host(&app, &client, org.id).await;
    let vm = Uuid::new_v4();
    let age = app.state.host_jwt_ttl.as_secs().saturating_sub(600) as i64;
    sqlx::query("UPDATE worker_hosts SET status='ready', last_heartbeat=now(), slot_size_reported=true, boot_disk_gb=280, created_at=now()-make_interval(secs=>$2::double precision), reported_live_vm_ids=ARRAY[$3]::uuid[], active_microvm_count=1 WHERE host_id=$1")
        .bind(host).bind(age).bind(vm).execute(&app.db).await.unwrap();
    sqlx::query("INSERT INTO workspace_vms(org_id,workspace_id,host_id,vm_id,epoch,status,lease_expires_at) VALUES($1,$2,$3,$4,1,'running',now()+interval '5 minutes')")
        .bind(org.id).bind(workspace.id).bind(host).bind(vm.to_string()).execute(&app.db).await.unwrap();
    assert_eq!(
        client
            .put_json(
                &format!("/api/v1/orgs/{}/admin-settings", org.id),
                &json!({"hostDiskAllowanceGib":160,"vmDiskAllowanceGib":96})
            )
            .await
            .status(),
        StatusCode::OK
    );
    let pass = rotate_once(&app.state).await;
    assert_eq!(pass.pairing_deferred, 1, "{pass:?}");
    assert_eq!(pass.paired + pass.forced + pass.rotated, 0, "{pass:?}");
    let (status, _, replacement) = host_status(&app, host).await;
    assert_eq!(status, "ready");
    assert_eq!(replacement, None);
    let status: String =
        sqlx::query_scalar("SELECT status FROM workspace_vms WHERE workspace_id=$1")
            .bind(workspace.id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(status, "running");
    assert!(!mock
        .calls()
        .iter()
        .any(|call| matches!(call, MockCall::TerminateInstance { .. })));
    app.cleanup().await;
}
