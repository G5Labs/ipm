//! Real HTTP, PostgreSQL, maintenance and host-control coverage. Cloud calls use
//! the existing provider double; this does not qualify real GCE boots/deletion.
use crate::helpers;
use g5_api::cloud::{CloudProvider, MockCall, MockCloudProvider};
use g5_api::services::worker_host_service::{PlacementOutcome, PlacementRequest};
use g5_api::ws::host_ctl::{ControlPlaneHostState, HostConnection};
use g5_api_core::services::hosts::host_capacity::{maintain, sweep_boot_image, CapacityPass};
use g5_internal_api::host_ctl::{HostHeartbeat, HostHello, VmSizeSpec};
use helpers::{
    AuthenticatedClient, EnvVarGuard, TestApp, TestOrg, TestUser, TestWorkspace,
    TEST_WORKER_IMAGE_REF,
};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

async fn setup() -> (
    TestApp,
    Arc<MockCloudProvider>,
    TestOrg,
    TestUser,
    EnvVarGuard,
) {
    let env = EnvVarGuard::set_many(&[("G5_STALE_HOST_DETECTOR_ENABLED", "true")]);
    let cloud = Arc::new(MockCloudProvider::new());
    let app = TestApp::spawn_with_cloud(cloud.clone() as Arc<dyn CloudProvider>).await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    app.state
        .worker_host_service
        .set_org_image_policy(&app.db, org.id, None, TEST_WORKER_IMAGE_REF)
        .await
        .unwrap();
    (app, cloud, org, user, env)
}

async fn pending(app: &TestApp, org: &TestOrg) -> (Uuid, Uuid) {
    let w = TestWorkspace::create(app, org).await;
    let (owner, generation): (Uuid, i64) =
        sqlx::query_as("SELECT owner_user_id,lifecycle_generation FROM workspaces WHERE id=$1")
            .bind(w.id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    let intent = Uuid::new_v4();
    sqlx::query("INSERT INTO workspace_vms(org_id,workspace_id,start_intent) VALUES($1,$2,$3)
        ON CONFLICT(workspace_id) DO UPDATE SET start_intent=EXCLUDED.start_intent")
        .bind(org.id).bind(w.id).bind(json!({"id":intent,"kind":"resume","status":"queued_for_capacity",
            "owner_user_id":owner,"lifecycle_generation":generation,"attempts":0,
            "created_at":chrono::Utc::now(),"expires_at":chrono::Utc::now()+chrono::TimeDelta::minutes(15)}))
        .execute(&app.db).await.unwrap();
    (w.id, intent)
}

async fn host(app: &TestApp, org: Uuid, slots: i32, managed: bool) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO worker_hosts(host_id,org_id,region,instance_type,status,last_heartbeat,
        slots_total,slot_size_reported,reported_live_vm_ids,cloud_instance_id,idle_since,runtime_identity)
        VALUES($1,$2,'us-central1','c3-standard-4','ready',now(),$3,true,'{}', $4,now()-interval '61 minutes',$5)")
        .bind(id).bind(org).bind(slots).bind(managed.then(|| format!("us-central1-a/g5-host-{id}")))
        .bind(helpers::current_runtime_identity())
        .execute(&app.db).await.unwrap();
    id
}

/// The boot image a sweep decides with, resolved as `maintain` does: before
/// the decision transaction and outside every lock.
async fn boot_image(app: &TestApp, org: Uuid) -> Option<String> {
    let db = app.scoped_db_for_org(org).await;
    let scope = db.reopener();
    db.rollback().await.unwrap();
    sweep_boot_image(&app.state, &scope, org).await.unwrap()
}
async fn decide(app: &TestApp, org: Uuid) -> CapacityPass {
    let desired = boot_image(app, org).await;
    let mut db = app.scoped_db_for_org(org).await;
    let pass = app
        .state
        .worker_host_service
        .reconcile_capacity(&app.state, &mut db, org, desired)
        .await
        .unwrap();
    db.commit().await.unwrap();
    pass
}
async fn tick(app: &TestApp, org: Uuid) -> CapacityPass {
    let db = app.scoped_db_for_org(org).await;
    let scope = db.reopener();
    db.rollback().await.unwrap();
    maintain(&app.state, scope, org).await.unwrap()
}
async fn wait_sql(app: &TestApp, query: &str, id: Uuid) {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if sqlx::query_scalar::<_, Option<bool>>(query)
                .bind(id)
                .fetch_one(&app.db)
                .await
                .unwrap()
                .unwrap_or(false)
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect(query);
}
async fn place(
    app: &TestApp,
    org: Uuid,
    workspace: Uuid,
) -> g5_api::services::worker_host_service::PlacementReservation {
    // Database-only capacity fixtures explicitly supply their eligible hosts.
    let reachable_host_ids: Vec<Uuid> =
        sqlx::query_scalar("SELECT host_id FROM worker_hosts WHERE org_id=$1")
            .bind(org)
            .fetch_all(&app.db)
            .await
            .unwrap();
    let mut db = app.scoped_db_for_org(org).await;
    let p = app
        .state
        .worker_host_service
        .place(
            &mut db,
            PlacementRequest {
                org_id: org,
                workspace_id: workspace,
                region: "us-central1",
                reachable_host_ids: &reachable_host_ids,
                excluded_host_ids: &[],
                start_deadline_at: chrono::Utc::now() + chrono::TimeDelta::minutes(2),
            },
        )
        .await
        .unwrap();
    db.commit().await.unwrap();
    let PlacementOutcome::Reserved(p) = p else {
        panic!("{p:?}")
    };
    p
}
fn hello(id: Uuid, slots: u32) -> HostHello {
    HostHello {
        host_id: id,
        protocol_version: g5_internal_api::host_ctl::PROTOCOL_VERSION,
        capabilities: vec![],
        slots_total: slots,
        slot_size: Some(VmSizeSpec {
            cpus: 3,
            mem_mib: 13000,
            scratch_disk_mib: 65536,
        }),
        inventory: vec![],
        retained_roots: vec![],
        runtime: Some(g5_internal_api::host_ctl::HostRuntimeIdentity::bundled()),
    }
}

async fn allocated_hello(app: &TestApp, id: Uuid) -> HostHello {
    let (slots, disk): (i32, i32) = sqlx::query_as(
        "SELECT provisioned_slots,scratch_disk_mib FROM worker_hosts WHERE host_id=$1",
    )
    .bind(id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    let mut report = hello(id, slots as u32);
    report.slot_size.as_mut().unwrap().scratch_disk_mib = disk as u32;
    report
}

async fn idle_since(app: &TestApp, host_id: Uuid) -> Option<chrono::DateTime<chrono::Utc>> {
    sqlx::query_scalar("SELECT idle_since FROM worker_hosts WHERE host_id=$1")
        .bind(host_id)
        .fetch_one(&app.db)
        .await
        .unwrap()
}

async fn establish_idle(app: &TestApp, host_id: Uuid) -> chrono::DateTime<chrono::Utc> {
    app.host_control_backend()
        .accept_hello(host_id, &hello(host_id, 1), &HostConnection::new(true))
        .await
        .unwrap();
    sqlx::query_scalar(
        "UPDATE worker_hosts SET idle_since=now()-interval '61 minutes'
         WHERE host_id=$1 RETURNING idle_since",
    )
    .bind(host_id)
    .fetch_one(&app.db)
    .await
    .unwrap()
}

fn empty_heartbeat() -> HostHeartbeat {
    HostHeartbeat {
        refresh_source_preparation: false,
        vms: vec![],
        slots_in_use: 0,
        boots_in_progress: 0,
        load: 0.0,
        memory_free_mib: 16000,
        disk_free_mib: 100000,
        retained_roots: vec![],
    }
}

async fn wait_for_db_wait(app: &TestApp, query_prefix: &str, event: &str) {
    loop {
        let waiting: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database()
             AND starts_with(query,$1) AND wait_event=$2)",
        )
        .bind(query_prefix)
        .bind(event)
        .fetch_one(&app.db)
        .await
        .unwrap();
        if waiting {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test]
#[serial_test::serial]
async fn a_cleaned_up_failed_claim_still_exhausts_its_original_start() {
    let (app, _, org, _, _env) = setup().await;
    let (_, intent) = pending(&app, &org).await;
    assert_eq!(decide(&app, org.id).await.provisioned, 1);
    sqlx::query(
        "UPDATE worker_hosts SET status='terminated', cloud_deleted_at=now(),
        provision_retry_count=3, provision_error_code='zone_capacity_exhausted'
        WHERE org_id=$1 AND capacity_start_intent_id=$2",
    )
    .bind(org.id)
    .bind(intent)
    .execute(&app.db)
    .await
    .unwrap();
    let pass = decide(&app, org.id).await;
    assert_eq!(
        pass.provisioned, 0,
        "cleanup cannot reset a Start's retry bound"
    );
    assert!(pass.launches.is_empty());
    app.cleanup().await;
}

#[tokio::test]
#[serial_test::serial]
async fn an_unused_claim_releases_its_start_if_covering_capacity_later_fails() {
    let (app, _, org, user, _env) = setup().await;
    // Growth for the Start alone: the free-slot floor is off here.
    set_capacity(&app, &user, org.id, json!({"minFreeSlots": 0})).await;
    let (_, intent) = pending(&app, &org).await;
    let claim = decide(&app, org.id).await.launches[0];
    let covering = Uuid::new_v4();
    sqlx::query("INSERT INTO worker_hosts(host_id,org_id,region,instance_type,status,workspace_vm_limit,cloud_instance_id)
        VALUES($1,$2,'us-central1','c3-standard-4','provisioning',1,'mock-manual-capacity')")
        .bind(covering).bind(org.id).execute(&app.db).await.unwrap();
    tick(&app, org.id).await;
    wait_sql(&app, "SELECT status='terminated' AND capacity_start_intent_id IS NULL
        AND provision_retry_count=0 AND cloud_deleted_at IS NOT NULL FROM worker_hosts WHERE host_id=$1", claim).await;
    sqlx::query("UPDATE worker_hosts SET status='terminated' WHERE host_id=$1")
        .bind(covering)
        .execute(&app.db)
        .await
        .unwrap();
    assert_eq!(tick(&app, org.id).await.provisioned, 1);
    let replacement: Uuid = sqlx::query_scalar(
        "SELECT host_id FROM worker_hosts WHERE org_id=$1 AND capacity_start_intent_id=$2",
    )
    .bind(org.id)
    .bind(intent)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_ne!(claim, replacement);
    helpers::wait_for_cloud_instance_id(&app, replacement).await;
    app.cleanup().await;
}

#[tokio::test]
#[serial_test::serial]
async fn stopped_claim_requires_successful_deletion_and_name_absence_before_retry() {
    use g5_api::cloud::InstanceState;
    let (app, cloud, org, _, _env) = setup().await;
    pending(&app, &org).await;
    let claim = decide(&app, org.id).await.launches[0];
    let name = g5_api::cloud::host_instance_name(claim, "");
    for state in [
        InstanceState::Stopping,
        InstanceState::Unknown,
        InstanceState::Terminated,
    ] {
        cloud.set_instance_by_name_in_zone("us-central1", &name, state, "us-central1-c");
        let before = cloud.calls().len();
        tick(&app, org.id).await;
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                if cloud.calls()[before..].iter().any(|call| {
                    matches!(call,
                    MockCall::GetInstanceByName { name: found, .. } if found == &name)
                }) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        // Wait for this drive to release its host lock before changing the cloud observation.
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 821))")
            .bind(claim.to_string())
            .execute(&app.db)
            .await
            .unwrap();
        let calls = cloud.calls();
        assert!(!calls
            .iter()
            .any(|c| matches!(c, MockCall::ProvisionInstance { .. })));
        assert_eq!(
            calls[before..]
                .iter()
                .any(|c| matches!(c, MockCall::TerminateInstance { .. })),
            state == InstanceState::Terminated
        );
    }
    assert!(cloud.calls().iter().any(
        |c| matches!(c, MockCall::TerminateInstance { cloud_instance_id }
        if cloud_instance_id == &format!("us-central1-c/{name}"))
    ));
    // A successful DELETE with the object still visible must not launch a duplicate.
    // The provider double keeps name visibility independently programmable.
    cloud.forget_instance_by_name("us-central1", &name);
    tick(&app, org.id).await;
    helpers::wait_for_cloud_instance_id(&app, claim).await;
    let attempts: i32 =
        sqlx::query_scalar("SELECT provision_retry_count FROM worker_hosts WHERE host_id=$1")
            .bind(claim)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(attempts, 1);
    app.cleanup().await;
}

#[tokio::test]
#[serial_test::serial]
async fn failed_cleanup_batches_do_not_starve_later_retired_hosts() {
    let (app, cloud, org, _, _env) = setup().await;
    cloud.fail_terminate(true);
    for _ in 0..5 {
        let h = host(&app, org.id, 1, true).await;
        sqlx::query(
            "UPDATE worker_hosts SET automatic_capacity=true,status='terminated' WHERE host_id=$1",
        )
        .bind(h)
        .execute(&app.db)
        .await
        .unwrap();
    }
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            tick(&app, org.id).await;
            let attempted: std::collections::HashSet<_> = cloud
                .calls()
                .into_iter()
                .filter_map(|c| match c {
                    MockCall::TerminateInstance { cloud_instance_id } => Some(cloud_instance_id),
                    _ => None,
                })
                .collect();
            if attempted.len() == 5 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("later hosts get a cleanup turn even when the first four deletions keep failing");
    let pending: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM worker_hosts
        WHERE org_id=$1 AND cloud_deleted_at IS NULL AND cloud_delete_attempted_at IS NOT NULL",
    )
    .bind(org.id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(
        pending, 5,
        "failed attempts still count as unconfirmed resources"
    );
    cloud.fail_terminate(false);
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            tick(&app, org.id).await;
            let remaining: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM worker_hosts WHERE org_id=$1 AND cloud_deleted_at IS NULL",
            )
            .bind(org.id)
            .fetch_one(&app.db)
            .await
            .unwrap();
            if remaining == 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    app.cleanup().await;
}

#[tokio::test]
#[serial_test::serial]
async fn two_free_hosts_leave_one_and_failed_deletion_retries_without_replacement() {
    let (app, cloud, org, _, _env) = setup().await;
    host(&app, org.id, 1, true).await;
    host(&app, org.id, 4, true).await;
    cloud.fail_terminate(true);
    let (a, b) = tokio::join!(decide(&app, org.id), decide(&app, org.id));
    assert_eq!(
        a.retired + b.retired,
        1,
        "concurrent sweeps retain one free host"
    );
    let retired: Uuid = sqlx::query_scalar(
        "SELECT host_id FROM worker_hosts WHERE org_id=$1 AND status='terminated'",
    )
    .bind(org.id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    let slots: i32 = sqlx::query_scalar(
        "SELECT slots_total FROM worker_hosts WHERE org_id=$1 AND status='ready'",
    )
    .bind(org.id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(slots, 1, "retain the smallest empty host");
    tick(&app, org.id).await;
    wait_sql(&app,"SELECT EXISTS(SELECT 1 FROM worker_hosts WHERE host_id=$1 AND status='terminated' AND cloud_deleted_at IS NULL)",retired).await;
    cloud.fail_terminate(false);
    tick(&app, org.id).await;
    wait_sql(
        &app,
        "SELECT cloud_deleted_at IS NOT NULL FROM worker_hosts WHERE host_id=$1",
        retired,
    )
    .await;
    assert!(!cloud
        .calls()
        .iter()
        .any(|c| matches!(c, MockCall::ProvisionInstance { .. })));
    assert_eq!(decide(&app, org.id).await.retired, 0);
    app.cleanup().await;
}

#[tokio::test]
#[serial_test::serial]
async fn idle_boundary_unknown_custody_liveness_and_external_hosts_are_protected() {
    let (app, _, org, _, _env) = setup().await;
    let candidate = host(&app, org.id, 2, true).await;
    let reserve = host(&app, org.id, 1, false).await;
    sqlx::query("UPDATE worker_hosts SET idle_since=now()-interval '59 minutes' WHERE host_id=$1")
        .bind(candidate)
        .execute(&app.db)
        .await
        .unwrap();
    assert_eq!(decide(&app, org.id).await.retired, 0);
    for update in [
        "reported_live_vm_ids=NULL",
        "reported_live_vm_ids=ARRAY[gen_random_uuid()]",
        "slot_size_reported=false",
        "status='unreachable'",
        "last_heartbeat=now()-interval '31 seconds'",
    ] {
        sqlx::query("UPDATE worker_hosts SET status='ready',slot_size_reported=true,last_heartbeat=now() WHERE host_id=$1")
            .bind(candidate).execute(&app.db).await.unwrap();
        sqlx::query("UPDATE worker_hosts SET reported_live_vm_ids='{}',idle_since=now()-interval '61 minutes' WHERE host_id=$1")
            .bind(candidate).execute(&app.db).await.unwrap();
        sqlx::query(&format!(
            "UPDATE worker_hosts SET {update} WHERE host_id=$1"
        ))
        .bind(candidate)
        .execute(&app.db)
        .await
        .unwrap();
        assert_eq!(decide(&app, org.id).await.retired, 0, "{update}");
        let idle: Option<chrono::DateTime<chrono::Utc>> =
            sqlx::query_scalar("SELECT idle_since FROM worker_hosts WHERE host_id=$1")
                .bind(candidate)
                .fetch_one(&app.db)
                .await
                .unwrap();
        assert!(idle.is_none(), "{update}");
    }
    let status: String = sqlx::query_scalar("SELECT status FROM worker_hosts WHERE host_id=$1")
        .bind(reserve)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(status, "ready", "self-hosted reserve is never deleted");
    app.cleanup().await;
}

#[tokio::test]
#[serial_test::serial]
async fn active_reservations_and_short_vm_between_sweeps_reset_idle() {
    let (app, _, org, _, _env) = setup().await;
    let h = host(&app, org.id, 2, true).await;
    host(&app, org.id, 1, false).await;
    let (w, _) = pending(&app, &org).await;
    let p = place(&app, org.id, w).await;
    // Older host is deterministic? Both created same window; force the reserved
    // host's timer and verify guards on every active state.
    for status in ["starting", "running", "stopping"] {
        sqlx::query("UPDATE workspace_vms SET status=$2 WHERE workspace_id=$1")
            .bind(w)
            .bind(status)
            .execute(&app.db)
            .await
            .unwrap();
        assert_eq!(decide(&app, org.id).await.retired, 0);
        let idle: bool =
            sqlx::query_scalar("SELECT idle_since IS NULL FROM worker_hosts WHERE host_id=$1")
                .bind(p.host_id)
                .fetch_one(&app.db)
                .await
                .unwrap();
        assert!(idle);
    }
    // A brief reservation and refusal between sweeps is enough to restart the hour.
    sqlx::query("UPDATE workspace_vms SET status='starting' WHERE workspace_id=$1")
        .bind(w)
        .execute(&app.db)
        .await
        .unwrap();
    let mut db = app.scoped_db_for_org(org.id).await;
    app.state
        .worker_host_service
        .release_placement(
            &mut db,
            &p,
            org.id,
            w,
            false,
            "host_capacity_refused",
            Some("disk"),
        )
        .await
        .unwrap();
    db.commit().await.unwrap();
    let idle: bool =
        sqlx::query_scalar("SELECT idle_since IS NULL FROM worker_hosts WHERE host_id=$1")
            .bind(p.host_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(idle);
    assert_eq!(
        decide(&app, org.id).await.retired,
        0,
        "queued demand blocks shrink"
    );
    let _ = h;
    app.cleanup().await;
}

#[tokio::test]
#[serial_test::serial]
async fn empty_reconnect_hello_preserves_continuous_idle_evidence() {
    let (app, _, org, _, _env) = setup().await;
    let h = host(&app, org.id, 1, true).await;
    let backend = app.host_control_backend();
    let hello = hello(h, 1);
    backend
        .accept_hello(h, &hello, &HostConnection::new(true))
        .await
        .unwrap();
    let before: chrono::DateTime<chrono::Utc> = sqlx::query_scalar(
        "UPDATE worker_hosts SET idle_since=now()-interval '61 minutes'
         WHERE host_id=$1 RETURNING idle_since",
    )
    .bind(h)
    .fetch_one(&app.db)
    .await
    .unwrap();

    backend
        .accept_hello(h, &hello, &HostConnection::new(true))
        .await
        .unwrap();
    let after: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT idle_since FROM worker_hosts WHERE host_id=$1")
            .bind(h)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(
        after,
        Some(before),
        "reconnect must not restart the idle hour"
    );
    host(&app, org.id, 1, false).await;
    assert_eq!(decide(&app, org.id).await.retired, 1);
    app.cleanup().await;
}

#[tokio::test]
#[serial_test::serial]
async fn reconnect_resets_idle_for_live_inventory_changed_sizing_and_readiness() {
    use g5_internal_api::host_ctl::{HostVmState, VmInventory};
    let (app, _, org, _, _env) = setup().await;
    for case in [
        "live_vm",
        "slots",
        "cpus",
        "memory",
        "disk",
        "legacy",
        "provisioning",
        "unreachable",
        "unknown_inventory",
        "unpublished_sizing",
    ] {
        let h = host(&app, org.id, 1, true).await;
        establish_idle(&app, h).await;
        let mut report = hello(h, 1);
        match case {
            "live_vm" => report.inventory.push(VmInventory {
                workspace_id: Uuid::new_v4(),
                vm_id: Uuid::new_v4(),
                state: HostVmState::Running,
                boot_at: chrono::Utc::now(),
            }),
            "slots" => report.slots_total += 1,
            "cpus" => report.slot_size.as_mut().unwrap().cpus += 1,
            "memory" => report.slot_size.as_mut().unwrap().mem_mib += 1,
            "disk" => report.slot_size.as_mut().unwrap().scratch_disk_mib += 1,
            "legacy" => {
                report.protocol_version = 1;
                report.slot_size = None;
            }
            "unknown_inventory" => {
                sqlx::query("UPDATE worker_hosts SET reported_live_vm_ids=NULL WHERE host_id=$1")
                    .bind(h)
                    .execute(&app.db)
                    .await
                    .unwrap();
            }
            "unpublished_sizing" => {
                sqlx::query("UPDATE worker_hosts SET slot_size_reported=false WHERE host_id=$1")
                    .bind(h)
                    .execute(&app.db)
                    .await
                    .unwrap();
            }
            status => {
                sqlx::query("UPDATE worker_hosts SET status=$2 WHERE host_id=$1")
                    .bind(h)
                    .bind(status)
                    .execute(&app.db)
                    .await
                    .unwrap();
            }
        }
        app.host_control_backend()
            .accept_hello(h, &report, &HostConnection::new(true))
            .await
            .unwrap();
        assert!(idle_since(&app, h).await.is_none(), "{case}");
    }
    app.cleanup().await;
}

#[tokio::test]
#[serial_test::serial]
async fn delayed_old_tunnel_heartbeat_cannot_restore_discarded_idle_evidence() {
    use g5_internal_api::host_ctl::{HostVmState, VmInventory};
    let (app, _, org, _, _env) = setup().await;
    let h = host(&app, org.id, 1, true).await;
    let backend = Arc::new(app.host_control_backend());
    let old_heartbeat = empty_heartbeat();
    // Pause the real hello transaction at COMMIT, after its inventory update.
    // The heartbeat's row-locking read starts before that update is committed.
    sqlx::raw_sql("CREATE FUNCTION pause_idle_hello_commit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_advisory_xact_lock(hashtextextended(NEW.host_id::text,1)); RETURN NEW; END $$; CREATE CONSTRAINT TRIGGER pause_idle_hello_commit AFTER UPDATE ON worker_hosts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION pause_idle_hello_commit();")
        .execute(&app.db).await.unwrap();
    for case in ["empty", "resized", "live_vm"] {
        let before = establish_idle(&app, h).await;
        let mut report = hello(h, 1);
        if case == "resized" {
            report.slot_size.as_mut().unwrap().mem_mib += 1;
        } else if case == "live_vm" {
            report.inventory.push(VmInventory {
                workspace_id: Uuid::new_v4(),
                vm_id: Uuid::new_v4(),
                state: HostVmState::Running,
                boot_at: chrono::Utc::now(),
            });
        }
        let mut commit_lock = app.db.begin().await.unwrap();
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1::text,1))")
            .bind(h.to_string())
            .execute(&mut *commit_lock)
            .await
            .unwrap();
        let reconnect = tokio::spawn({
            let backend = backend.clone();
            let report = report.clone();
            async move {
                backend
                    .accept_hello(h, &report, &HostConnection::new(true))
                    .await
            }
        });
        wait_for_db_wait(&app, "COMMIT", "advisory").await;
        // A heartbeat already dispatched by the old carrier has no connection
        // argument. Its row lock must return the new hello's committed evidence.
        let heartbeat = tokio::spawn({
            let backend = backend.clone();
            let old_heartbeat = old_heartbeat.clone();
            async move { backend.heartbeat(h, &old_heartbeat).await }
        });
        wait_for_db_wait(&app, "SELECT reported_live_vm_ids", "transactionid").await;
        commit_lock.commit().await.unwrap();
        reconnect.await.unwrap().unwrap();
        heartbeat.await.unwrap().unwrap();
        let expected = (case == "empty").then_some(before);
        assert_eq!(idle_since(&app, h).await, expected, "{case}");
        if case == "live_vm" {
            let live: Vec<Uuid> = sqlx::query_scalar(
                "SELECT reported_live_vm_ids FROM worker_hosts WHERE host_id=$1",
            )
            .bind(h)
            .fetch_one(&app.db)
            .await
            .unwrap();
            assert_eq!(live, vec![report.inventory[0].vm_id]);
        }
    }
    app.cleanup().await;
}

#[tokio::test]
#[serial_test::serial]
async fn a_sweep_during_a_short_tunnel_gap_keeps_idle_evidence() {
    use g5_internal_api::host_ctl::{method, HelloDisposition, RpcMessage, RpcRequest};
    use g5_internal_api::stream_mux::{Channel, Mux};
    let (app, _, org, _, _env) = setup().await;
    let h = host(&app, org.id, 1, true).await;
    assert!(app.state.install_host_control_backend());
    let (api_io, host_io) = tokio::io::duplex(65536);
    let host_mux = Mux::new(host_io, false);
    let tunnel_task = tokio::spawn({
        let registry = app.state.preview_tunnels.clone();
        async move { registry.attach_test_mux(h, Mux::new(api_io, true)).await }
    });
    let (writer, mut reader) = host_mux.open(Channel::Ctl).await.unwrap().into_split();
    let request =
        RpcMessage::Request(RpcRequest::new("hello", method::HOST_HELLO, &hello(h, 1)).unwrap());
    let mut encoded = serde_json::to_vec(&request).unwrap();
    encoded.push(b'\n');
    writer.send(&encoded).await.unwrap();
    let line = reader.read_line().await.unwrap();
    let RpcMessage::Response(response) = serde_json::from_str(&line).unwrap() else {
        panic!("expected hello response")
    };
    assert!(matches!(
        response.result_as::<HelloDisposition>().unwrap(),
        HelloDisposition::Accept(_)
    ));
    let before: chrono::DateTime<chrono::Utc> = sqlx::query_scalar(
        "UPDATE worker_hosts SET idle_since=now()-interval '61 minutes' WHERE host_id=$1 RETURNING idle_since",
    ).bind(h).fetch_one(&app.db).await.unwrap();

    assert!(app.state.preview_tunnels.terminate(&h).await);
    tunnel_task.await.unwrap();
    assert!(app.state.preview_tunnels.get(&h).is_none());
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_eq!(
        decide(&app, org.id).await.retired,
        0,
        "only host is retained"
    );
    assert_eq!(
        idle_since(&app, h).await,
        Some(before),
        "tunnel absence is not stale liveness"
    );
    app.host_control_backend()
        .accept_hello(h, &hello(h, 1), &HostConnection::new(true))
        .await
        .unwrap();
    assert_eq!(idle_since(&app, h).await, Some(before));
    host(&app, org.id, 1, false).await;
    assert_eq!(decide(&app, org.id).await.retired, 1);
    app.cleanup().await;
}

#[tokio::test]
#[serial_test::serial]
async fn a_reservation_between_sweeps_cannot_be_undone_by_waiting_hello() {
    let (app, _, org, _, _env) = setup().await;
    let h = host(&app, org.id, 1, true).await;
    establish_idle(&app, h).await;
    let (workspace, _) = pending(&app, &org).await;
    let mut db = app.scoped_db_for_org(org.id).await;
    let reservation = app
        .state
        .worker_host_service
        .place(
            &mut db,
            PlacementRequest {
                org_id: org.id,
                workspace_id: workspace,
                region: "us-central1",
                reachable_host_ids: &[h],
                excluded_host_ids: &[],
                start_deadline_at: chrono::Utc::now() + chrono::TimeDelta::minutes(2),
            },
        )
        .await
        .unwrap();
    let PlacementOutcome::Reserved(reservation) = reservation else {
        panic!("{reservation:?}")
    };
    assert_eq!(reservation.host_id, h);
    let reset: bool =
        sqlx::query_scalar("SELECT idle_since IS NULL FROM worker_hosts WHERE host_id=$1")
            .bind(h)
            .fetch_one(db.conn())
            .await
            .unwrap();
    assert!(
        reset,
        "reservation trigger resets idle before the next sweep"
    );
    let reconnect = tokio::spawn({
        let backend = app.host_control_backend();
        async move {
            backend
                .accept_hello(h, &hello(h, 1), &HostConnection::new(true))
                .await
        }
    });
    wait_for_db_wait(&app, "SELECT pg_advisory_xact_lock", "advisory").await;
    app.state
        .worker_host_service
        .release_placement(
            &mut db,
            &reservation,
            org.id,
            workspace,
            false,
            "host_capacity_refused",
            Some("disk"),
        )
        .await
        .unwrap();
    db.commit().await.unwrap();
    reconnect.await.unwrap().unwrap();
    assert!(
        idle_since(&app, h).await.is_none(),
        "empty hello cannot resurrect pre-reservation evidence"
    );
    app.cleanup().await;
}

#[tokio::test]
#[serial_test::serial]
async fn stale_reconnect_resets_idle_with_or_without_an_intervening_sweep() {
    let (app, _, org, _, _env) = setup().await;
    let direct = host(&app, org.id, 1, true).await;
    let swept = host(&app, org.id, 1, true).await;
    for h in [direct, swept] {
        establish_idle(&app, h).await;
    }
    // Let a real freshness boundary elapse, without a liveness UPDATE clearing
    // the timer on our behalf. Both hosts still have valid evidence at this write.
    sqlx::query("UPDATE worker_hosts SET last_heartbeat=now() WHERE org_id=$1")
        .bind(org.id)
        .execute(&app.db)
        .await
        .unwrap();
    sqlx::query("UPDATE worker_hosts SET reported_live_vm_ids='{}',idle_since=now()-interval '61 minutes' WHERE org_id=$1")
        .bind(org.id).execute(&app.db).await.unwrap();
    loop {
        let stale: bool = sqlx::query_scalar("SELECT bool_and(last_heartbeat < now()-interval '31 seconds') FROM worker_hosts WHERE org_id=$1")
            .bind(org.id).fetch_one(&app.db).await.unwrap();
        if stale {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(idle_since(&app, direct).await.is_some());
    assert!(idle_since(&app, swept).await.is_some());
    app.host_control_backend()
        .accept_hello(direct, &hello(direct, 1), &HostConnection::new(true))
        .await
        .unwrap();
    assert!(idle_since(&app, direct).await.is_none());
    assert_eq!(decide(&app, org.id).await.retired, 0);
    assert!(idle_since(&app, swept).await.is_none());
    app.host_control_backend()
        .accept_hello(swept, &hello(swept, 1), &HostConnection::new(true))
        .await
        .unwrap();
    assert!(idle_since(&app, swept).await.is_none());
    assert_eq!(decide(&app, org.id).await.retired, 0);
    let new_idle = idle_since(&app, swept).await.unwrap();
    assert!(new_idle > chrono::Utc::now() - chrono::TimeDelta::minutes(1));
    app.cleanup().await;
}

#[tokio::test]
#[serial_test::serial]
async fn modern_heartbeats_preserve_idle_but_legacy_control_resets_it() {
    let (app, _, org, _, _env) = setup().await;
    let h = host(&app, org.id, 1, true).await;
    app.host_control_backend()
        .accept_hello(h, &hello(h, 1), &HostConnection::new(true))
        .await
        .unwrap();
    sqlx::query("UPDATE worker_hosts SET idle_since=now()-interval '61 minutes' WHERE host_id=$1")
        .bind(h)
        .execute(&app.db)
        .await
        .unwrap();
    let before: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT idle_since FROM worker_hosts WHERE host_id=$1")
            .bind(h)
            .fetch_one(&app.db)
            .await
            .unwrap();
    app.host_control_backend()
        .heartbeat(
            h,
            &HostHeartbeat {
                refresh_source_preparation: false,
                vms: vec![],
                slots_in_use: 0,
                boots_in_progress: 0,
                load: 0.0,
                memory_free_mib: 16000,
                disk_free_mib: 100000,
                retained_roots: vec![],
            },
        )
        .await
        .unwrap();
    let after: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT idle_since FROM worker_hosts WHERE host_id=$1")
            .bind(h)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(after, Some(before));
    sqlx::query("UPDATE worker_hosts SET last_heartbeat=now() WHERE host_id=$1")
        .bind(h)
        .execute(&app.db)
        .await
        .unwrap();
    let empty:bool=sqlx::query_scalar("SELECT idle_since IS NULL AND reported_live_vm_ids IS NULL FROM worker_hosts WHERE host_id=$1")
        .bind(h).fetch_one(&app.db).await.unwrap();
    assert!(empty);
    app.cleanup().await;
}

#[tokio::test]
#[serial_test::serial]
async fn burst_claims_are_durable_deduplicated_and_capped_by_unchanged_quota() {
    let (app, cloud, org, user, _env) = setup().await;
    // Growth for the Starts alone: the free-slot floor is off here.
    set_capacity(&app, &user, org.id, json!({"minFreeSlots": 0})).await;
    for _ in 0..12 {
        pending(&app, &org).await;
    }
    for _ in 0..4 {
        let _ = tokio::join!(decide(&app, org.id), decide(&app, org.id));
    }
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM worker_hosts WHERE org_id=$1")
        .bind(org.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(count, 10);
    assert!(cloud.calls().is_empty(), "claims commit before cloud work");
    // A later process/tick recovers those rows rather than issuing new claims.
    tick(&app, org.id).await;
    wait_sql(
        &app,
        "SELECT count(*) > 0 FROM worker_hosts WHERE org_id=$1 AND cloud_instance_id IS NOT NULL",
        org.id,
    )
    .await;
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            tick(&app, org.id).await;
            let linked: i64 = sqlx::query_scalar("SELECT count(*) FROM worker_hosts WHERE org_id=$1 AND cloud_instance_id IS NOT NULL")
                .bind(org.id).fetch_one(&app.db).await.unwrap();
            if linked == 10 { break; }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }).await.expect("bounded launches finish over successive maintenance polls");
    assert_eq!(
        cloud
            .calls()
            .iter()
            .filter(|c| matches!(c, MockCall::ProvisionInstance { .. }))
            .count(),
        10
    );
    let defaults: bool = sqlx::query_scalar(
        "SELECT bool_and(instance_type='c3-standard-4' AND workspace_vm_limit IS NULL
        AND boot_disk_gb IS NULL AND automatic_capacity) FROM worker_hosts WHERE org_id=$1",
    )
    .bind(org.id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(defaults);
    app.cleanup().await;
}

/// With the free-slot floor off, taking the last slot creates nothing: growth
/// waits for a queued Start (`g5.architecture.host_demand_capacity`).
#[tokio::test]
#[serial_test::serial]
async fn floor_off_no_proactive_spare_and_manual_inflight_capacity_covers_demand() {
    let (app, cloud, org, user, _env) = setup().await;
    set_capacity(&app, &user, org.id, json!({"minFreeSlots": 0})).await;
    assert_eq!(decide(&app, org.id).await.provisioned, 0);
    let response = app
        .authenticated_client(&user)
        .post_json(
            &format!("/api/v1/orgs/{}/worker-hosts", org.id),
            &json!({"region":"us-central1"}),
        )
        .await;
    assert_eq!(response.status(), reqwest::StatusCode::CREATED);
    let body: Value = response.json().await.unwrap();
    let id = Uuid::parse_str(body["data"]["hostId"].as_str().unwrap()).unwrap();
    helpers::wait_for_cloud_instance_id(&app, id).await;
    let (w, _) = pending(&app, &org).await;
    assert_eq!(
        decide(&app, org.id).await.provisioned,
        0,
        "manual provisioning covers the queued Start"
    );
    app.host_control_backend()
        .accept_hello(
            id,
            &allocated_hello(&app, id).await,
            &HostConnection::new(true),
        )
        .await
        .unwrap();
    place(&app, org.id, w).await;
    assert_eq!(
        decide(&app, org.id).await.provisioned,
        0,
        "last slot consumed without queued demand"
    );
    assert_eq!(
        cloud
            .calls()
            .iter()
            .filter(|c| matches!(c, MockCall::ProvisionInstance { .. }))
            .count(),
        1
    );
    app.cleanup().await;
}

#[tokio::test]
#[serial_test::serial]
async fn quota_only_wait_cancellation_and_configuration_do_not_launch_hosts() {
    let (app, cloud, org, _, _env) = setup().await;
    let (w, _) = pending(&app, &org).await;
    sqlx::query("INSERT INTO configs(org_id,scope,scope_id,key,value) VALUES($1,'org',$1,'max_running_vms','0')
        ON CONFLICT(org_id,scope,scope_id,key) DO UPDATE SET value='0'").bind(org.id).execute(&app.db).await.unwrap();
    assert_eq!(tick(&app, org.id).await.provisioned, 0);
    sqlx::query("DELETE FROM configs WHERE org_id=$1 AND key='max_running_vms'")
        .bind(org.id)
        .execute(&app.db)
        .await
        .unwrap();
    assert_eq!(decide(&app, org.id).await.provisioned, 1);
    sqlx::query("UPDATE workspace_vms SET start_intent=jsonb_set(start_intent,'{status}','\"cancelled\"') WHERE workspace_id=$1")
        .bind(w).execute(&app.db).await.unwrap();
    tick(&app, org.id).await;
    wait_sql(
        &app,
        "SELECT bool_and(status='terminated') FROM worker_hosts WHERE org_id=$1",
        org.id,
    )
    .await;
    assert!(!cloud
        .calls()
        .iter()
        .any(|c| matches!(c, MockCall::ProvisionInstance { .. })));
    pending(&app, &org).await;
    sqlx::query(
        "UPDATE orgs SET default_base_image_ref=NULL,default_base_image_track=NULL WHERE id=$1",
    )
    .bind(org.id)
    .execute(&app.db)
    .await
    .unwrap();
    let db = app.scoped_db_for_org(org.id).await;
    let scope = db.reopener();
    db.rollback().await.unwrap();
    assert!(maintain(&app.state, scope, org.id).await.is_err());
    let failed:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM workspace_vms WHERE org_id=$1 AND last_error->>'reason'='capacity_configuration')")
        .bind(org.id).fetch_one(&app.db).await.unwrap();
    assert!(failed);
    app.cleanup().await;
}

#[tokio::test]
#[serial_test::serial]
async fn missing_disk_snapshot_preserves_automatic_retry_budget() {
    let (app, cloud, org, user, _env) = setup().await;
    set_capacity(&app, &user, org.id, json!({"minFreeSlots":0})).await;
    let (_, intent) = pending(&app, &org).await;
    let claim = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO worker_hosts(host_id,org_id,region,instance_type,status,
        base_image_ref,automatic_capacity,capacity_start_intent_id,provision_retry_count,
        provision_error_code,bringup_heartbeat_at,created_by_user_id)
        VALUES($1,$2,'us-central1','c3-standard-4','provisioning',$3,true,$4,1,
        'zone_capacity_exhausted',now()-interval '2 minutes',$5)",
    )
    .bind(claim)
    .bind(org.id)
    .bind(TEST_WORKER_IMAGE_REF)
    .bind(intent)
    .bind(user.id)
    .execute(&app.db)
    .await
    .unwrap();
    let before: (i32, Option<String>, Option<chrono::DateTime<chrono::Utc>>) = sqlx::query_as(
        "SELECT provision_retry_count,provision_error_code,bringup_heartbeat_at FROM worker_hosts WHERE host_id=$1")
        .bind(claim).fetch_one(&app.db).await.unwrap();
    let calls_before = cloud.calls().len();
    tick(&app, org.id).await;
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if cloud.calls()[calls_before..]
                .iter()
                .any(|call| matches!(call, MockCall::GetInstanceByName { .. }))
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    // The lookup proves this driver started; its advisory lock fences the
    // complete attempt, so observing its release avoids timing-based assertions.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 821))")
        .bind(claim.to_string())
        .execute(&app.db)
        .await
        .unwrap();
    let after = sqlx::query_as::<_, (i32, Option<String>, Option<chrono::DateTime<chrono::Utc>>)>(
        "SELECT provision_retry_count,provision_error_code,bringup_heartbeat_at FROM worker_hosts WHERE host_id=$1")
        .bind(claim).fetch_one(&app.db).await.unwrap();
    assert_eq!(after, before);
    assert!(!cloud
        .calls()
        .iter()
        .any(|call| matches!(call, MockCall::ProvisionInstance { .. })));
    sqlx::query(
        "UPDATE worker_hosts SET host_disk_gib=120,scratch_disk_mib=81920,
        provisioned_slots=1,resolved_disk_gib=200 WHERE host_id=$1",
    )
    .bind(claim)
    .execute(&app.db)
    .await
    .unwrap();
    tick(&app, org.id).await;
    helpers::wait_for_cloud_instance_id(&app, claim).await;
    app.cleanup().await;
}

#[tokio::test]
#[serial_test::serial]
async fn stockout_retries_same_record_same_region_and_stops_after_three_attempts() {
    let (app, cloud, org, _, _env) = setup().await;
    let _retry = EnvVarGuard::set_many(&[
        ("G5_PROVISION_RETRY_ENABLED", "true"),
        ("G5_WORKER_FALLBACK_REGIONS", "us-east1"),
    ]);
    cloud.fail_provision_stockout(true);
    pending(&app, &org).await;
    tick(&app, org.id).await;
    for attempt in 1..=3 {
        wait_sql(&app,"SELECT bool_and(provision_error_code='zone_capacity_exhausted') FROM worker_hosts WHERE org_id=$1",org.id).await;
        let count: i32 =
            sqlx::query_scalar("SELECT provision_retry_count FROM worker_hosts WHERE org_id=$1")
                .bind(org.id)
                .fetch_one(&app.db)
                .await
                .unwrap();
        assert_eq!(count, attempt);
        sqlx::query("UPDATE worker_hosts SET bringup_heartbeat_at=now()-interval '2 minutes' WHERE org_id=$1")
            .bind(org.id).execute(&app.db).await.unwrap();
        tick(&app, org.id).await;
        if attempt < 3 {
            wait_sql(
                &app,
                &format!(
                    "SELECT provision_retry_count > {attempt} FROM worker_hosts WHERE org_id=$1"
                ),
                org.id,
            )
            .await;
        }
    }
    let rows: (i64, i32) = sqlx::query_as(
        "SELECT count(*),max(provision_retry_count) FROM worker_hosts WHERE org_id=$1",
    )
    .bind(org.id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(rows, (1, 3));
    for call in cloud.calls() {
        if let MockCall::ProvisionInstance { region, .. } = call {
            assert_eq!(region, "us-central1");
        }
    }
    app.cleanup().await;
}

#[tokio::test]
#[serial_test::serial]
async fn queued_http_start_provisions_default_host_and_reaches_running_over_control() {
    use g5_internal_api::host_ctl::{CommandResult, VmEvent, VmEventKind};
    use g5_internal_api::host_ctl_test_support::{ScriptedHostFaults, ScriptedHostPeer};
    use g5_internal_api::stream_mux::{Channel, Mux};
    let (app, cloud, org, user, _env) = setup().await;
    // Growth for the Start alone: the free-slot floor is off here.
    set_capacity(&app, &user, org.id, json!({"minFreeSlots": 0})).await;
    let w = TestWorkspace::create(&app, &org).await;
    w.attach_parent(&app).await;
    let client = app.authenticated_client(&user);
    let response = client
        .post_json(
            &format!("/api/v1/workspaces/{}/resume", w.id),
            &json!({"operationId":Uuid::new_v4()}),
        )
        .await;
    let status = response.status();
    let body: Value = response.json().await.unwrap();
    assert_eq!(status, reqwest::StatusCode::OK, "{body}");
    assert_eq!(body["data"]["outcome"], "queued_for_capacity");
    assert_eq!(tick(&app, org.id).await.provisioned, 1);
    let id: Uuid = sqlx::query_scalar("SELECT host_id FROM worker_hosts WHERE org_id=$1")
        .bind(org.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    helpers::wait_for_cloud_instance_id(&app, id).await;
    assert!(app.state.install_host_control_backend());
    let (api_io, host_io) = tokio::io::duplex(256 * 1024);
    let api_mux = Mux::new(api_io, true);
    let host_mux = Mux::new(host_io, false);
    let stream = host_mux.open(Channel::Ctl).await.unwrap();
    let peer = ScriptedHostPeer::new(id, 1)
        .with_slot_size(allocated_hello(&app, id).await.slot_size.unwrap());
    let control = tokio::spawn({
        let peer = peer.clone();
        async move {
            peer.run_control_stream(
                stream,
                ScriptedHostFaults {
                    command_results: std::collections::HashMap::from([(
                        "vm.start".into(),
                        CommandResult::Started,
                    )]),
                    ..Default::default()
                },
            )
            .await
        }
    });
    let registry = app.state.preview_tunnels.clone();
    let registry_task = tokio::spawn(async move { registry.attach_test_mux(id, api_mux).await });
    wait_sql(
        &app,
        "SELECT status='ready' AND slot_size_reported FROM worker_hosts WHERE host_id=$1",
        id,
    )
    .await;
    let db = app.scoped_db_for_user_org(user.id, org.id).await;
    let scope = db.reopener();
    db.rollback().await.unwrap();
    let service =
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
    service.reconcile_start_queue(16).await.unwrap();
    assert!(peer
        .received_commands()
        .await
        .iter()
        .any(|c| c.method == "vm.start"));
    let (vm_id, epoch, catalog): (String, i64, String) = sqlx::query_as(
        "SELECT vm_id,epoch,module_catalog_version FROM workspace_vms WHERE workspace_id=$1",
    )
    .bind(w.id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    app.host_control_backend()
        .ingest_vm_event(
            id,
            &VmEvent {
                workspace_id: w.id,
                vm_id: Uuid::parse_str(&vm_id).unwrap(),
                seq: 1,
                kind: VmEventKind::Ready {
                    epoch: epoch as u64,
                    module_catalog_version: catalog,
                    dispatcher_contract_version:
                        g5_internal_api::host_ctl::DISPATCHER_CONTRACT_VERSION,
                },
                at: chrono::Utc::now(),
                log_tail: None,
            },
        )
        .await
        .unwrap();
    let view: Value = client
        .get(&format!("/api/v1/workspaces/{}/vm", w.id))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(view["data"]["status"], "running");
    assert_eq!(
        tick(&app, org.id).await.provisioned,
        0,
        "no proactive replenishment"
    );
    // Manual Add Host passes the same defaults to the same cloud bring-up code.
    let response = client
        .post_json(
            &format!("/api/v1/orgs/{}/worker-hosts", org.id),
            &json!({"region":"us-central1"}),
        )
        .await;
    assert_eq!(response.status(), reqwest::StatusCode::CREATED);
    let manual: Value = response.json().await.unwrap();
    let manual_id = Uuid::parse_str(manual["data"]["hostId"].as_str().unwrap()).unwrap();
    helpers::wait_for_cloud_instance_id(&app, manual_id).await;
    let specs: Vec<_> = cloud
        .calls()
        .into_iter()
        .filter_map(|call| match call {
            MockCall::ProvisionInstance {
                instance_type,
                region,
                boot_disk_gb,
                startup_script,
                ..
            } => {
                assert!(startup_script.contains(TEST_WORKER_IMAGE_REF));
                Some((instance_type, region, boot_disk_gb))
            }
            _ => None,
        })
        .collect();
    assert_eq!(specs.len(), 2);
    assert_eq!(specs[0], specs[1]);
    assert_eq!(specs[0].0, "c3-standard-4");
    assert_eq!(specs[0].2, 200, "one slot gets the automatic disk floor");
    control.abort();
    registry_task.abort();
    drop(host_mux);
    app.cleanup().await;
}

#[tokio::test]
#[serial_test::serial]
async fn failed_deletions_count_toward_resource_ceiling() {
    let (app, cloud, org, _, _env) = setup().await;
    cloud.fail_terminate(true);
    for _ in 0..11 {
        let h = host(&app, org.id, 1, true).await;
        sqlx::query(
            "UPDATE worker_hosts SET automatic_capacity=true,status='terminated' WHERE host_id=$1",
        )
        .bind(h)
        .execute(&app.db)
        .await
        .unwrap();
    }
    let (w, _) = pending(&app, &org).await;
    assert_eq!(decide(&app, org.id).await.provisioned, 0);
    let reason: String =
        sqlx::query_scalar("SELECT last_error->>'reason' FROM workspace_vms WHERE workspace_id=$1")
            .bind(w)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(reason, "capacity_limit");
    // A confirmed deletion, unlike logical retirement, releases this ceiling.
    sqlx::query(
        "UPDATE worker_hosts SET cloud_deleted_at=now() WHERE host_id IN
        (SELECT host_id FROM worker_hosts WHERE org_id=$1 LIMIT 1)",
    )
    .bind(org.id)
    .execute(&app.db)
    .await
    .unwrap();
    assert_eq!(decide(&app, org.id).await.provisioned, 1);
    app.cleanup().await;
}

#[tokio::test]
#[serial_test::serial]
async fn running_vm_preference_precedes_greater_starting_occupancy_and_refusal_fallback() {
    let (app, _, org, _, _env) = setup().await;
    let first = host(&app, org.id, 4, false).await;
    let (w, _) = pending(&app, &org).await;
    let p = place(&app, org.id, w).await;
    assert_eq!(p.host_id, first);
    let (w2, _) = pending(&app, &org).await;
    place(&app, org.id, w2).await;
    let running = host(&app, org.id, 4, false).await;
    let (w3, _) = pending(&app, &org).await;
    sqlx::query("UPDATE workspace_vms SET start_intent=start_intent || jsonb_build_object('excluded_host_ids',jsonb_build_array($2::text)) WHERE workspace_id=$1")
        .bind(w3).bind(first).execute(&app.db).await.unwrap();
    assert_eq!(place(&app, org.id, w3).await.host_id, running);
    sqlx::query("UPDATE workspace_vms SET status='running' WHERE workspace_id=$1")
        .bind(w3)
        .execute(&app.db)
        .await
        .unwrap();
    let (w4, _) = pending(&app, &org).await;
    assert_eq!(
        place(&app, org.id, w4).await.host_id,
        running,
        "one running outranks two starting"
    );
    app.cleanup().await;
}

#[tokio::test]
#[serial_test::serial]
async fn retirement_fences_a_rotation_selected_before_the_retirement_commit() {
    let (app, cloud, org, _, _env) = setup().await;
    let _rotation = EnvVarGuard::set_many(&[("G5_HOST_ROTATION_ENABLED", "true")]);
    let h = host(&app, org.id, 4, true).await;
    host(&app, org.id, 1, false).await;
    sqlx::query("UPDATE worker_hosts SET created_at=now()-interval '25 hours',base_image_ref=$2,
        base_image_digest=$3,base_image_source_kind='pin',base_image_source_value=$2 WHERE host_id=$1")
        .bind(h).bind(TEST_WORKER_IMAGE_REF).bind(format!("sha256:{}","a".repeat(64)))
        .execute(&app.db).await.unwrap();
    let desired = boot_image(&app, org.id).await;
    let mut db = app.scoped_db_for_org(org.id).await;
    let pass = app
        .state
        .worker_host_service
        .reconcile_capacity(&app.state, &mut db, org.id, desired)
        .await
        .unwrap();
    assert_eq!(pass.retired, 1);
    let rotation = tokio::spawn({
        let state = app.state.clone();
        async move { g5_api::cloud::rotation::rotate_once(&state).await }
    });
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let waiting: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database()
                AND wait_event='advisory' AND query LIKE 'SELECT pg_advisory_xact_lock%')",
            )
            .fetch_one(&app.db)
            .await
            .unwrap();
            if waiting {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("rotation reaches the admission fence after selecting the old ready host");
    db.commit().await.unwrap();
    let pass = rotation.await.unwrap();
    assert_eq!(pass.errors, 0, "{pass:?}");
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM worker_hosts WHERE org_id=$1")
        .bind(org.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(count, 2, "stale selection must not create a successor");
    assert!(!cloud
        .calls()
        .iter()
        .any(|c| matches!(c, MockCall::ProvisionInstance { .. })));
    app.cleanup().await;
}

#[tokio::test]
#[serial_test::serial]
async fn start_racing_retirement_uses_the_remaining_host() {
    let (app, _, org, _, _env) = setup().await;
    let retired = host(&app, org.id, 4, true).await;
    let reserve = host(&app, org.id, 1, false).await;
    let desired = boot_image(&app, org.id).await;
    let mut db = app.scoped_db_for_org(org.id).await;
    assert_eq!(
        app.state
            .worker_host_service
            .reconcile_capacity(&app.state, &mut db, org.id, desired)
            .await
            .unwrap()
            .retired,
        1
    );
    let (w, _) = pending(&app, &org).await;
    let mut start_db = app.scoped_db_for_org(org.id).await;
    let service = app.state.worker_host_service.clone();
    let org_id = org.id;
    let start = tokio::spawn(async move {
        let result = service
            .place(
                &mut start_db,
                PlacementRequest {
                    org_id,
                    workspace_id: w,
                    region: "us-central1",
                    reachable_host_ids: &[retired, reserve],
                    excluded_host_ids: &[],
                    start_deadline_at: chrono::Utc::now() + chrono::TimeDelta::minutes(2),
                },
            )
            .await
            .unwrap();
        start_db.commit().await.unwrap();
        result
    });
    db.commit().await.unwrap();
    let PlacementOutcome::Reserved(p) = start.await.unwrap() else {
        panic!("remaining capacity must accept Start")
    };
    assert_eq!(p.host_id, reserve);
    assert_ne!(p.host_id, retired);
    app.cleanup().await;
}

#[tokio::test]
#[serial_test::serial]
async fn uncertain_insert_is_attached_in_its_found_zone_without_duplicate_launch() {
    let (app, cloud, org, _, _env) = setup().await;
    pending(&app, &org).await;
    assert_eq!(decide(&app, org.id).await.provisioned, 1);
    let id: Uuid = sqlx::query_scalar("SELECT host_id FROM worker_hosts WHERE org_id=$1")
        .bind(org.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    // Recovered insert under the historical name, in a fallback zone.
    let name = g5_api::cloud::host_instance_name(id, "");
    cloud.set_instance_by_name_in_zone(
        "us-central1",
        &name,
        g5_api::cloud::InstanceState::Running,
        "us-central1-c",
    );
    tick(&app, org.id).await;
    helpers::wait_for_cloud_instance_id(&app, id).await;
    let linkage: String =
        sqlx::query_scalar("SELECT cloud_instance_id FROM worker_hosts WHERE host_id=$1")
            .bind(id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(linkage, format!("us-central1-c/{name}"));
    assert!(!cloud
        .calls()
        .iter()
        .any(|c| matches!(c, MockCall::ProvisionInstance { .. })));
    app.cleanup().await;
}

#[tokio::test]
#[serial_test::serial]
async fn expired_start_and_foreign_region_capacity_do_not_override_consent_or_region() {
    let (app, _, org, user, _env) = setup().await;
    // Growth for the Start alone: the free-slot floor is off here.
    set_capacity(&app, &user, org.id, json!({"minFreeSlots": 0})).await;
    let candidate = host(&app, org.id, 1, true).await;
    let foreign = host(&app, org.id, 4, false).await;
    sqlx::query("UPDATE worker_hosts SET region='us-east1' WHERE host_id=$1")
        .bind(foreign)
        .execute(&app.db)
        .await
        .unwrap();
    assert_eq!(
        decide(&app, org.id).await.retired,
        0,
        "reserve must serve the same pool"
    );
    let (w, _) = pending(&app, &org).await;
    sqlx::query("UPDATE workspace_vms SET start_intent=jsonb_set(start_intent,'{status}','\"expired\"') WHERE workspace_id=$1")
        .bind(w).execute(&app.db).await.unwrap();
    assert_eq!(
        decide(&app, org.id).await.provisioned,
        0,
        "expired consent creates no capacity"
    );
    let (w, _) = pending(&app, &org).await;
    place(&app, org.id, w).await; // fills the only local slot
    pending(&app, &org).await;
    assert_eq!(
        decide(&app, org.id).await.provisioned,
        1,
        "foreign region slots cannot cover this Start"
    );
    let _ = candidate;
    app.cleanup().await;
}

#[tokio::test]
#[serial_test::serial]
async fn cancellation_during_cloud_insert_never_reauthorizes_the_vm() {
    let (app, cloud, org, _, _env) = setup().await;
    cloud.set_provision_delay(Duration::from_millis(300));
    let (w, _) = pending(&app, &org).await;
    tick(&app, org.id).await;
    tokio::time::timeout(Duration::from_secs(5), async {
        while !cloud
            .calls()
            .iter()
            .any(|c| matches!(c, MockCall::ProvisionInstance { .. }))
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    sqlx::query("UPDATE workspace_vms SET start_intent=jsonb_set(start_intent,'{status}','\"cancelled\"') WHERE workspace_id=$1")
        .bind(w).execute(&app.db).await.unwrap();
    let id: Uuid = sqlx::query_scalar("SELECT host_id FROM worker_hosts WHERE org_id=$1")
        .bind(org.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    helpers::wait_for_cloud_instance_id(&app, id).await;
    app.host_control_backend()
        .accept_hello(
            id,
            &allocated_hello(&app, id).await,
            &HostConnection::new(true),
        )
        .await
        .unwrap();
    let mut db = app.scoped_db_for_org(org.id).await;
    let outcome = app
        .state
        .worker_host_service
        .place(
            &mut db,
            PlacementRequest {
                org_id: org.id,
                workspace_id: w,
                region: "us-central1",
                reachable_host_ids: &[id],
                excluded_host_ids: &[],
                start_deadline_at: chrono::Utc::now() + chrono::TimeDelta::minutes(2),
            },
        )
        .await
        .unwrap();
    assert_eq!(outcome, PlacementOutcome::StartIntentUnavailable);
    db.rollback().await.unwrap();
    assert_eq!(decide(&app, org.id).await.provisioned, 0);
    let vm: (String, String) = sqlx::query_as(
        "SELECT status,start_intent->>'status' FROM workspace_vms WHERE workspace_id=$1",
    )
    .bind(w)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(vm, ("stopped".into(), "cancelled".into()));
    app.cleanup().await;
}

#[tokio::test]
#[serial_test::serial]
async fn rotation_successor_reserves_the_predecessors_occupied_slots_and_counts_once() {
    let (app, _, org, user, _env) = setup().await;
    // Growth for the Start alone: the free-slot floor is off here.
    set_capacity(&app, &user, org.id, json!({"minFreeSlots": 0})).await;
    let old = host(&app, org.id, 4, true).await;
    let (w, _) = pending(&app, &org).await;
    place(&app, org.id, w).await;
    let new = Uuid::new_v4();
    sqlx::query("INSERT INTO worker_hosts(host_id,org_id,region,instance_type,status,workspace_vm_limit,cloud_instance_id)
        VALUES($1,$2,'us-central1','c3-standard-4','provisioning',4,'mock-replacement')")
        .bind(new).bind(org.id).execute(&app.db).await.unwrap();
    sqlx::query("UPDATE worker_hosts SET rotation_replaced_by=$2 WHERE host_id=$1")
        .bind(old)
        .bind(new)
        .execute(&app.db)
        .await
        .unwrap();
    for _ in 0..3 {
        pending(&app, &org).await;
    }
    assert_eq!(
        decide(&app, org.id).await.provisioned,
        0,
        "three free successor slots cover demand after retaining one old VM"
    );
    pending(&app, &org).await;
    assert_eq!(
        decide(&app, org.id).await.provisioned,
        1,
        "old and new cannot each promise their free slots"
    );
    app.cleanup().await;
}

#[tokio::test]
#[serial_test::serial]
async fn legacy_pairing_copies_origin_and_cannot_replace_an_idle_retired_host() {
    let (app, _, org, _, _env) = setup().await;
    let old = host(&app, org.id, 4, true).await;
    let next = host(&app, org.id, 4, true).await;
    sqlx::query("UPDATE worker_hosts SET automatic_capacity=true WHERE host_id=$1")
        .bind(old)
        .execute(&app.db)
        .await
        .unwrap();
    sqlx::query("UPDATE worker_hosts SET rotation_replaced_by=$2 WHERE host_id=$1")
        .bind(old)
        .bind(next)
        .execute(&app.db)
        .await
        .unwrap();
    let automatic: bool =
        sqlx::query_scalar("SELECT automatic_capacity FROM worker_hosts WHERE host_id=$1")
            .bind(next)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(automatic);
    sqlx::query("UPDATE worker_hosts SET rotation_replaced_by=NULL WHERE host_id=$1")
        .bind(old)
        .execute(&app.db)
        .await
        .unwrap();
    assert_eq!(decide(&app, org.id).await.retired, 1);
    let retired: Uuid = sqlx::query_scalar(
        "SELECT host_id FROM worker_hosts WHERE org_id=$1 AND status='terminated'",
    )
    .bind(org.id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    let mut tx = app.db.begin().await.unwrap();
    let successor = Uuid::new_v4();
    sqlx::query("INSERT INTO worker_hosts(host_id,org_id,region) VALUES($1,$2,'us-central1')")
        .bind(successor)
        .bind(org.id)
        .execute(&mut *tx)
        .await
        .unwrap();
    assert!(
        sqlx::query("UPDATE worker_hosts SET rotation_replaced_by=$2 WHERE host_id=$1")
            .bind(retired)
            .bind(successor)
            .execute(&mut *tx)
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM worker_hosts WHERE host_id=$1)")
            .bind(successor)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(!exists);
    app.cleanup().await;
}

async fn set_capacity(app: &TestApp, user: &TestUser, org: Uuid, body: Value) {
    let response = app
        .authenticated_client(user)
        .put_json(&format!("/api/v1/orgs/{org}/admin-settings"), &body)
        .await;
    assert_eq!(response.status(), reqwest::StatusCode::OK, "{body}");
}

fn provisioned_types(cloud: &MockCloudProvider) -> Vec<String> {
    cloud
        .calls()
        .into_iter()
        .filter_map(|call| match call {
            MockCall::ProvisionInstance { instance_type, .. } => Some(instance_type),
            _ => None,
        })
        .collect()
}

#[tokio::test]
#[serial_test::serial]
async fn admin_settings_turn_growth_off_and_choose_the_machine_type() {
    let (app, cloud, org, user, _env) = setup().await;
    set_capacity(
        &app,
        &user,
        org.id,
        json!({"autoAddHosts": false, "autoHostInstanceType": "c3-standard-8",
            "hostDiskAllowanceGib": 160, "vmDiskAllowanceGib": 32}),
    )
    .await;
    let (workspace, _) = pending(&app, &org).await;

    // Growth off: nothing is claimed and the Start says why it waits.
    assert_eq!(decide(&app, org.id).await.provisioned, 0);
    let reason: Option<String> = sqlx::query_scalar(
        "SELECT last_error ->> 'reason' FROM workspace_vms WHERE workspace_id=$1",
    )
    .bind(workspace)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(reason.as_deref(), Some("capacity_growth_disabled"));
    let hosts: i64 = sqlx::query_scalar("SELECT count(*) FROM worker_hosts WHERE org_id=$1")
        .bind(org.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(hosts, 0);
    assert!(provisioned_types(&cloud).is_empty());

    // Growth on again: the waiting Start gets a host of the chosen type, in
    // the claim and in the cloud request.
    set_capacity(&app, &user, org.id, json!({"autoAddHosts": true})).await;
    assert_eq!(tick(&app, org.id).await.provisioned, 1);
    let instance_type: String = sqlx::query_scalar(
        "SELECT instance_type FROM worker_hosts WHERE org_id=$1 AND automatic_capacity",
    )
    .bind(org.id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(instance_type, "c3-standard-8");
    let allocation: (i32, i32, i32, i32) = sqlx::query_as(
        "SELECT host_disk_gib,scratch_disk_mib,provisioned_slots,resolved_disk_gib
         FROM worker_hosts WHERE org_id=$1 AND automatic_capacity",
    )
    .bind(org.id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(allocation, (160, 32 * 1024, 2, 224));
    tokio::time::timeout(Duration::from_secs(10), async {
        while provisioned_types(&cloud).is_empty() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the claim is launched");
    assert_eq!(provisioned_types(&cloud), vec!["c3-standard-8".to_string()]);
    app.cleanup().await;
}

#[tokio::test]
#[serial_test::serial]
async fn admin_settings_turn_idle_removal_off_and_set_its_delay() {
    let (app, cloud, org, user, _env) = setup().await;
    // Both hosts have been empty for 61 minutes (the `host` fixture).
    let older = host(&app, org.id, 2, true).await;
    let newer = host(&app, org.id, 1, true).await;

    // Removal off: past the default hour, nothing is retired, and the idle
    // timers keep running.
    set_capacity(&app, &user, org.id, json!({"autoRetireIdleHosts": false})).await;
    assert_eq!(decide(&app, org.id).await.retired, 0);
    let timed: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM worker_hosts WHERE org_id=$1 AND idle_since IS NOT NULL",
    )
    .bind(org.id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(timed, 2, "idle timers keep running while removal is off");

    // Removal on with a two-hour delay: 61 minutes is not enough.
    set_capacity(
        &app,
        &user,
        org.id,
        json!({"autoRetireIdleHosts": true, "idleHostRetireSecs": 7200}),
    )
    .await;
    assert_eq!(decide(&app, org.id).await.retired, 0);

    // Past the delay, one host retires and the other stays as the reserve.
    sqlx::query("UPDATE worker_hosts SET idle_since=now()-interval '121 minutes' WHERE host_id=$1")
        .bind(older)
        .execute(&app.db)
        .await
        .unwrap();
    assert_eq!(decide(&app, org.id).await.retired, 1);
    let status = |id: Uuid| {
        let db = app.db.clone();
        async move {
            sqlx::query_scalar::<_, String>("SELECT status FROM worker_hosts WHERE host_id=$1")
                .bind(id)
                .fetch_one(&db)
                .await
                .unwrap()
        }
    };
    assert_eq!(status(older).await, "terminated");
    assert_eq!(status(newer).await, "ready");

    // A shorter delay applies to the next sweep: the minimum is 15 minutes.
    sqlx::query("UPDATE worker_hosts SET idle_since=now()-interval '14 minutes' WHERE host_id=$1")
        .bind(newer)
        .execute(&app.db)
        .await
        .unwrap();
    let extra = host(&app, org.id, 1, true).await;
    sqlx::query("UPDATE worker_hosts SET idle_since=now()-interval '16 minutes' WHERE host_id=$1")
        .bind(extra)
        .execute(&app.db)
        .await
        .unwrap();
    set_capacity(&app, &user, org.id, json!({"idleHostRetireSecs": 900})).await;
    assert_eq!(decide(&app, org.id).await.retired, 1);
    assert_eq!(status(extra).await, "terminated");
    assert_eq!(status(newer).await, "ready");
    assert!(provisioned_types(&cloud).is_empty());
    app.cleanup().await;
}

/// A host that stops matching the control plane's runtime
/// (`g5.runtime.placement.runtime-identity`): its row is stale, not failed.
async fn make_runtime_stale(app: &TestApp, host_id: Uuid) {
    let mut identity = helpers::current_runtime_identity();
    identity["module_catalog_version"] = json!("2026.01.01-older");
    identity["module_catalog_hash"] = json!("0".repeat(64));
    sqlx::query("UPDATE worker_hosts SET runtime_identity=$2 WHERE host_id=$1")
        .bind(host_id)
        .bind(identity)
        .execute(&app.db)
        .await
        .unwrap();
}

/// A stale host's free slots cover no demand: a queued Start on an otherwise
/// stale fleet grows the fleet exactly as an empty fleet would, and placement
/// never reserves the stale host (`g5.architecture.host_demand_capacity`,
/// `g5.runtime.placement.runtime-identity`).
#[tokio::test]
#[serial_test::serial]
async fn a_stale_runtime_host_covers_no_demand_so_the_fleet_grows() {
    let (app, _cloud, org, user, _env) = setup().await;
    // Growth for the Start alone: the free-slot floor is off here.
    set_capacity(&app, &user, org.id, json!({"minFreeSlots": 0})).await;
    let stale = host(&app, org.id, 4, true).await;
    make_runtime_stale(&app, stale).await;
    let (w, intent) = pending(&app, &org).await;
    let mut db = app.scoped_db_for_org(org.id).await;
    let outcome = app
        .state
        .worker_host_service
        .place(
            &mut db,
            PlacementRequest {
                org_id: org.id,
                workspace_id: w,
                region: "us-central1",
                reachable_host_ids: &[stale],
                excluded_host_ids: &[],
                start_deadline_at: chrono::Utc::now() + chrono::TimeDelta::minutes(2),
            },
        )
        .await
        .unwrap();
    db.commit().await.unwrap();
    assert!(
        matches!(outcome, PlacementOutcome::NoCapacity),
        "four free slots on a stale host are no capacity: {outcome:?}"
    );
    let pass = decide(&app, org.id).await;
    assert_eq!(
        (pass.provisioned, pass.retired),
        (1, 1),
        "uncovered demand grows the fleet past the stale host, which retires at once"
    );
    let reason: String =
        sqlx::query_scalar("SELECT last_error->>'reason' FROM workspace_vms WHERE workspace_id=$1")
            .bind(w)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(reason, "preparing_capacity");
    let claim: Option<Uuid> = sqlx::query_scalar(
        "SELECT host_id FROM worker_hosts WHERE org_id=$1 AND automatic_capacity
         AND capacity_start_intent_id=$2 AND status='provisioning'",
    )
    .bind(org.id)
    .bind(intent)
    .fetch_optional(&app.db)
    .await
    .unwrap();
    assert!(
        claim.is_some(),
        "the Start claims a new host, not the stale one"
    );
    assert_ne!(claim, Some(stale));
    let (stale_status, drain_reason): (String, Option<String>) =
        sqlx::query_as("SELECT status, drain_reason FROM worker_hosts WHERE host_id=$1")
            .bind(stale)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(
        (stale_status.as_str(), drain_reason.as_deref()),
        ("terminated", Some("automatic_stale")),
        "an empty stale host off the pinned image is retired without the idle delay"
    );
    app.cleanup().await;
}

/// Idle retirement counts only current hosts as the reserve: the last current
/// host stays even beside an idle stale host, while the idle stale host
/// retires behind it (`g5.architecture.host_idle_retirement`).
#[tokio::test]
#[serial_test::serial]
async fn a_stale_runtime_host_is_no_reserve_and_retires_behind_a_current_one() {
    let (app, _cloud, org, _, _env) = setup().await;
    let current = host(&app, org.id, 1, true).await;
    let stale = host(&app, org.id, 1, true).await;
    make_runtime_stale(&app, stale).await;
    let status = |id: Uuid| {
        let db = app.db.clone();
        async move {
            sqlx::query_scalar::<_, String>("SELECT status FROM worker_hosts WHERE host_id=$1")
                .bind(id)
                .fetch_one(&db)
                .await
                .unwrap()
        }
    };
    assert_eq!(decide(&app, org.id).await.retired, 1);
    assert_eq!(
        status(stale).await,
        "terminated",
        "the idle stale host retires"
    );
    assert_eq!(
        status(current).await,
        "ready",
        "the current host was its reserve"
    );
    assert_eq!(
        decide(&app, org.id).await.retired,
        0,
        "the last current host has no reserve"
    );
    assert_eq!(status(current).await, "ready");
    app.cleanup().await;
}

/// The worker boot image, which carries the runtime, can lag the control
/// plane (the deploy pins the nearest published image). A host of the boot
/// image the family resolves to now that came up stale proves another claim
/// would be stale too, so growth waits with `capacity_image_stale` and the
/// proving host stays; once the family moves on, the stale host retires at
/// once and the Start claims a new host
/// (`g5.architecture.host_demand_capacity`, `g5.architecture.host_idle_retirement`).
#[tokio::test]
#[serial_test::serial]
async fn a_stale_host_of_the_pinned_image_holds_growth_until_the_pin_moves() {
    let (app, cloud, org, user, _env) = setup().await;
    // Growth for the Start alone: the free-slot floor is off here.
    set_capacity(&app, &user, org.id, json!({"minFreeSlots": 0})).await;
    let stale = host(&app, org.id, 4, true).await;
    make_runtime_stale(&app, stale).await;
    // Its idle age is past the retirement delay: evidence still outranks idle.
    // Its guest image is irrelevant: only the boot image carries the runtime.
    sqlx::query(
        "UPDATE worker_hosts SET boot_image_id='g5-worker-mock-v1', base_image_digest=$2 WHERE host_id=$1",
    )
    .bind(stale)
    .bind(format!("sha256:{}", "e".repeat(64)))
    .execute(&app.db)
    .await
    .unwrap();
    let (w, _) = pending(&app, &org).await;
    let pass = decide(&app, org.id).await;
    assert_eq!(
        (pass.provisioned, pass.retired),
        (0, 0),
        "no claim boots the same stale image again, and the proving host stays past its idle delay"
    );
    let reason: String =
        sqlx::query_scalar("SELECT last_error->>'reason' FROM workspace_vms WHERE workspace_id=$1")
            .bind(w)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(reason, "capacity_image_stale");

    // A newer boot image is promoted: the old stale host is no evidence any
    // more. (A guest-image change alone changes nothing.)
    let newer = "ghcr.io/g5labs/test-worker@sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    app.state
        .worker_host_service
        .set_org_image_policy(&app.db, org.id, None, newer)
        .await
        .unwrap();
    let pass = decide(&app, org.id).await;
    assert_eq!(
        (pass.provisioned, pass.retired),
        (0, 0),
        "guest image is not the runtime"
    );
    cloud.set_boot_image("g5-worker-mock-v2");
    let pass = decide(&app, org.id).await;
    assert_eq!(
        (pass.provisioned, pass.retired),
        (1, 1),
        "the Start claims a host of the new image and the stale host retires without a delay"
    );
    let (status, drain_reason): (String, Option<String>) =
        sqlx::query_as("SELECT status, drain_reason FROM worker_hosts WHERE host_id=$1")
            .bind(stale)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(
        (status.as_str(), drain_reason.as_deref()),
        ("terminated", Some("automatic_stale"))
    );
    app.cleanup().await;
}

/// A claim fulfilled by a host that is no longer current is spent, and an
/// automatically created stale fleet retires at once, so the resource bound
/// (quota plus one) frees for a current host instead of holding every Start
/// at `capacity_limit` (`g5.architecture.host_demand_capacity`).
#[tokio::test]
#[serial_test::serial]
async fn a_spent_stale_claim_and_a_stale_fleet_free_the_resource_bound() {
    let (app, _cloud, org, user, _env) = setup().await;
    // Growth for the Start alone: the free-slot floor is off here.
    set_capacity(&app, &user, org.id, json!({"minFreeSlots": 0})).await;
    sqlx::query("INSERT INTO configs(org_id,scope,scope_id,key,value) VALUES($1,'org',$1,'max_running_vms','1')
        ON CONFLICT(org_id,scope,scope_id,key) DO UPDATE SET value='1'").bind(org.id).execute(&app.db).await.unwrap();
    let (w, intent) = pending(&app, &org).await;
    let claimed = host(&app, org.id, 1, true).await;
    let other = host(&app, org.id, 1, true).await;
    for id in [claimed, other] {
        make_runtime_stale(&app, id).await;
        sqlx::query(
            "UPDATE worker_hosts SET automatic_capacity=true, idle_since=NULL WHERE host_id=$1",
        )
        .bind(id)
        .execute(&app.db)
        .await
        .unwrap();
    }
    sqlx::query("UPDATE worker_hosts SET capacity_start_intent_id=$2 WHERE host_id=$1")
        .bind(claimed)
        .bind(intent)
        .execute(&app.db)
        .await
        .unwrap();
    let pass = decide(&app, org.id).await;
    assert_eq!(
        (pass.provisioned, pass.retired),
        (0, 2),
        "two unconfirmed stale resources fill the bound; both retire at once"
    );
    let reason: String =
        sqlx::query_scalar("SELECT last_error->>'reason' FROM workspace_vms WHERE workspace_id=$1")
            .bind(w)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(
        reason, "capacity_limit",
        "the spent claim is not retried as a failed claim"
    );
    // Cloud cleanup of the retired hosts releases the bound; the Start then
    // claims a current host.
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if tick(&app, org.id).await.provisioned == 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("a new claim follows the stale fleet's cloud cleanup");
    let reason: String =
        sqlx::query_scalar("SELECT last_error->>'reason' FROM workspace_vms WHERE workspace_id=$1")
            .bind(w)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(reason, "preparing_capacity");
    let claim: Option<Uuid> = sqlx::query_scalar(
        "SELECT host_id FROM worker_hosts WHERE org_id=$1 AND capacity_start_intent_id=$2
         AND status='provisioning'",
    )
    .bind(org.id)
    .bind(intent)
    .fetch_optional(&app.db)
    .await
    .unwrap();
    assert!(claim.is_some() && claim != Some(claimed));
    app.cleanup().await;
}

/// A spent claim is released even when the quota hides its Start from the
/// sweep, so the retired host never reads as that Start's failed claim once
/// the quota admits it again (`g5.architecture.host_demand_capacity`).
#[tokio::test]
#[serial_test::serial]
async fn a_spent_claim_is_released_while_the_quota_hides_its_start() {
    let (app, _cloud, org, user, _env) = setup().await;
    // Growth for the Start alone: the free-slot floor is off here.
    set_capacity(&app, &user, org.id, json!({"minFreeSlots": 0})).await;
    let (w, intent) = pending(&app, &org).await;
    let claimed = host(&app, org.id, 1, true).await;
    make_runtime_stale(&app, claimed).await;
    sqlx::query(
        "UPDATE worker_hosts SET automatic_capacity=true, capacity_start_intent_id=$2 WHERE host_id=$1",
    )
    .bind(claimed)
    .bind(intent)
    .execute(&app.db)
    .await
    .unwrap();
    sqlx::query("INSERT INTO configs(org_id,scope,scope_id,key,value) VALUES($1,'org',$1,'max_running_vms','0')
        ON CONFLICT(org_id,scope,scope_id,key) DO UPDATE SET value='0'").bind(org.id).execute(&app.db).await.unwrap();
    let pass = decide(&app, org.id).await;
    assert_eq!(
        (pass.provisioned, pass.retired),
        (0, 1),
        "the stale claim host retires"
    );
    let link: Option<Uuid> =
        sqlx::query_scalar("SELECT capacity_start_intent_id FROM worker_hosts WHERE host_id=$1")
            .bind(claimed)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(
        link, None,
        "the spent claim was released although its Start was hidden"
    );
    sqlx::query("DELETE FROM configs WHERE org_id=$1 AND key='max_running_vms'")
        .bind(org.id)
        .execute(&app.db)
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if tick(&app, org.id).await.provisioned == 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the Start claims a new host once the quota admits it");
    let reason: String =
        sqlx::query_scalar("SELECT last_error->>'reason' FROM workspace_vms WHERE workspace_id=$1")
            .bind(w)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(reason, "preparing_capacity");
    app.cleanup().await;
}

/// A claim host that an operator removed before the sweep saw it stale still
/// holds the Start's unique claim link; the sweep releases it before the
/// successor claims, instead of failing on the unique index every tick
/// (`g5.architecture.host_demand_capacity`).
#[tokio::test]
#[serial_test::serial]
async fn a_removed_stale_claim_releases_its_link_before_the_successor_claims() {
    let (app, _cloud, org, user, _env) = setup().await;
    let (w, intent) = pending(&app, &org).await;
    let claimed = host(&app, org.id, 1, true).await;
    make_runtime_stale(&app, claimed).await;
    sqlx::query(
        "UPDATE worker_hosts SET automatic_capacity=true, capacity_start_intent_id=$2 WHERE host_id=$1",
    )
    .bind(claimed)
    .bind(intent)
    .execute(&app.db)
    .await
    .unwrap();
    let response = app
        .authenticated_client(&user)
        .delete(&format!("/api/v1/orgs/{}/worker-hosts/{claimed}", org.id))
        .await;
    assert_eq!(response.status(), reqwest::StatusCode::NO_CONTENT);
    let pass = decide(&app, org.id).await;
    assert_eq!(
        pass.provisioned, 1,
        "the Start claims a successor without a link conflict"
    );
    let successor: Option<Uuid> = sqlx::query_scalar(
        "SELECT host_id FROM worker_hosts WHERE org_id=$1 AND capacity_start_intent_id=$2",
    )
    .bind(org.id)
    .bind(intent)
    .fetch_optional(&app.db)
    .await
    .unwrap();
    assert!(successor.is_some() && successor != Some(claimed));
    let reason: String =
        sqlx::query_scalar("SELECT last_error->>'reason' FROM workspace_vms WHERE workspace_id=$1")
            .bind(w)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(reason, "preparing_capacity");
    app.cleanup().await;
}

/// While a stale host exists, an unresolvable boot image fails the sweep
/// loudly: nothing is provisioned or retired on a guess, and the Start sees
/// `capacity_configuration` until the resolver recovers.
#[tokio::test]
#[serial_test::serial]
async fn an_unresolvable_boot_image_fails_the_sweep_instead_of_guessing() {
    let (app, cloud, org, user, _env) = setup().await;
    // Growth for the Start alone: the free-slot floor is off here.
    set_capacity(&app, &user, org.id, json!({"minFreeSlots": 0})).await;
    let stale = host(&app, org.id, 1, true).await;
    make_runtime_stale(&app, stale).await;
    let (w, _) = pending(&app, &org).await;
    cloud.fail_resolve_boot_image(true);
    let db = app.scoped_db_for_org(org.id).await;
    let scope = db.reopener();
    db.rollback().await.unwrap();
    let outcome = maintain(&app.state, scope, org.id).await;
    assert!(outcome.is_err(), "the sweep must not guess the boot image");
    let (status, reason): (String, String) = sqlx::query_as(
        "SELECT h.status, vm.last_error->>'reason' FROM worker_hosts h, workspace_vms vm
         WHERE h.host_id=$1 AND vm.workspace_id=$2",
    )
    .bind(stale)
    .bind(w)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(
        (status.as_str(), reason.as_str()),
        ("ready", "capacity_configuration")
    );
    cloud.fail_resolve_boot_image(false);
    let pass = decide(&app, org.id).await;
    assert_eq!(
        (pass.provisioned, pass.retired),
        (1, 1),
        "recovers once the family resolves"
    );
    app.cleanup().await;
}

/// The worker boot image lookup runs before the sweep takes any lock: a slow
/// cloud answer must not hold organization placement, hello publication or
/// heartbeats (`g5.architecture.host_demand_capacity`).
#[tokio::test]
#[serial_test::serial]
async fn the_boot_image_lookup_never_holds_admission_locks() {
    let (app, cloud, org, user, _env) = setup().await;
    // Growth for the Start alone: the free-slot floor is off here.
    set_capacity(&app, &user, org.id, json!({"minFreeSlots": 0})).await;
    let stale = host(&app, org.id, 1, true).await;
    make_runtime_stale(&app, stale).await;
    let (w, _) = pending(&app, &org).await;
    cloud.set_resolve_boot_image_delay(Duration::from_millis(1500));
    let placement = async {
        // Let the sweep reach its lookup first.
        tokio::time::sleep(Duration::from_millis(200)).await;
        let started = std::time::Instant::now();
        let mut db = app.scoped_db_for_org(org.id).await;
        let outcome = app
            .state
            .worker_host_service
            .place(
                &mut db,
                PlacementRequest {
                    org_id: org.id,
                    workspace_id: w,
                    region: "us-central1",
                    reachable_host_ids: &[stale],
                    excluded_host_ids: &[],
                    start_deadline_at: chrono::Utc::now() + chrono::TimeDelta::minutes(2),
                },
            )
            .await
            .unwrap();
        db.commit().await.unwrap();
        (outcome, started.elapsed(), std::time::Instant::now())
    };
    let sweep = async {
        let pass = decide(&app, org.id).await;
        (pass, std::time::Instant::now())
    };
    let ((pass, sweep_done), (outcome, elapsed, placed)) = tokio::join!(sweep, placement);
    assert!(
        matches!(outcome, PlacementOutcome::NoCapacity),
        "the stale host is no capacity: {outcome:?}"
    );
    // Placement finished while the sweep was still waiting on its lookup, so
    // the lookup held no lock placement needed.
    assert!(
        placed < sweep_done && elapsed < Duration::from_millis(1000),
        "placement waited {elapsed:?} on the sweep's cloud lookup"
    );
    assert_eq!(
        (pass.provisioned, pass.retired),
        (1, 1),
        "the sweep still grows and retires once the lookup answers"
    );
    app.cleanup().await;
}

/// A claim that never registered keeps its link when it is retired: its
/// attempts are exhaustion evidence, and a Start the quota briefly hid must
/// not get a fresh budget from a terminated row
/// (`g5.architecture.host_demand_capacity`).
#[tokio::test]
#[serial_test::serial]
async fn an_unregistered_terminated_claim_keeps_its_exhaustion_evidence() {
    let (app, _cloud, org, _, _env) = setup().await;
    let (w, intent) = pending(&app, &org).await;
    let exhausted = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO worker_hosts(host_id,org_id,region,instance_type,status,slots_total,
            automatic_capacity,capacity_start_intent_id,provision_retry_count,cloud_deleted_at)
         VALUES($1,$2,'us-central1','c3-standard-4','terminated',4,true,$3,3,now())",
    )
    .bind(exhausted)
    .bind(org.id)
    .bind(intent)
    .execute(&app.db)
    .await
    .unwrap();
    let pass = decide(&app, org.id).await;
    assert_eq!(
        pass.provisioned, 0,
        "no fresh budget for an exhausted claim"
    );
    let link: Option<Uuid> =
        sqlx::query_scalar("SELECT capacity_start_intent_id FROM worker_hosts WHERE host_id=$1")
            .bind(exhausted)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(link, Some(intent), "the attempts stay linked to the Start");
    let reason: String =
        sqlx::query_scalar("SELECT last_error->>'reason' FROM workspace_vms WHERE workspace_id=$1")
            .bind(w)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(reason, "capacity_failed");
    app.cleanup().await;
}

/// The cloud lookup runs before the decision transaction opens, so a reserve
/// whose heartbeat ages past the 30-second window during the lookup is not
/// read as healthy by a transaction clock from before it
/// (`g5.architecture.host_idle_retirement`).
#[tokio::test]
#[serial_test::serial]
async fn the_decision_clock_starts_after_the_boot_image_lookup() {
    let (app, cloud, org, _, _env) = setup().await;
    let idle = host(&app, org.id, 1, true).await;
    let reserve = host(&app, org.id, 1, true).await;
    sqlx::query(
        "UPDATE worker_hosts SET last_heartbeat = now() - interval '29 seconds', idle_since = NULL
         WHERE host_id = $1",
    )
    .bind(reserve)
    .execute(&app.db)
    .await
    .unwrap();
    let stale = host(&app, org.id, 1, true).await;
    make_runtime_stale(&app, stale).await;
    cloud.set_resolve_boot_image_delay(Duration::from_millis(1500));
    let pass = decide(&app, org.id).await;
    let status = |id: Uuid| {
        let db = app.db.clone();
        async move {
            sqlx::query_scalar::<_, String>("SELECT status FROM worker_hosts WHERE host_id=$1")
                .bind(id)
                .fetch_one(&db)
                .await
                .unwrap()
        }
    };
    assert_eq!(pass.retired, 1, "only the stale host retires: {pass:?}");
    assert_eq!(status(stale).await, "terminated");
    assert_eq!(
        status(idle).await,
        "ready",
        "the reserve's heartbeat had aged out by decision time, so the idle host stays"
    );
    assert_eq!(status(reserve).await, "ready");
    app.cleanup().await;
}

/// A hanging boot image lookup fails the sweep within its bound instead of
/// stalling lifecycle maintenance for every organization behind it.
#[tokio::test]
#[serial_test::serial]
async fn a_hanging_boot_image_lookup_fails_the_sweep_within_its_bound() {
    let (app, cloud, org, _, _env) = setup().await;
    let stale = host(&app, org.id, 1, true).await;
    make_runtime_stale(&app, stale).await;
    let (w, _) = pending(&app, &org).await;
    cloud.set_resolve_boot_image_delay(Duration::from_secs(20));
    let db = app.scoped_db_for_org(org.id).await;
    let scope = db.reopener();
    db.rollback().await.unwrap();
    let started = std::time::Instant::now();
    let outcome = maintain(&app.state, scope, org.id).await;
    let elapsed = started.elapsed();
    assert!(outcome.is_err(), "the sweep gives up on the lookup");
    assert!(
        elapsed < Duration::from_secs(8),
        "the sweep returned after {elapsed:?}, past its lookup bound"
    );
    let reason: String =
        sqlx::query_scalar("SELECT last_error->>'reason' FROM workspace_vms WHERE workspace_id=$1")
            .bind(w)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(reason, "capacity_configuration");
    app.cleanup().await;
}

async fn floor_claims(app: &TestApp, org: Uuid) -> Vec<Uuid> {
    sqlx::query_scalar(
        "SELECT host_id FROM worker_hosts WHERE org_id=$1 AND capacity_floor_claim
         AND automatic_capacity AND capacity_start_intent_id IS NULL ORDER BY created_at",
    )
    .bind(org)
    .fetch_all(&app.db)
    .await
    .unwrap()
}

/// The free-slot floor (`g5.architecture.host_demand_capacity`): a region
/// with no host is not grown; a ready host with a free slot satisfies the
/// default floor of one; taking that last slot claims a host with no Start
/// behind it, launched on the claim path; while it is in flight no second
/// claim is made.
#[tokio::test]
#[serial_test::serial]
async fn taking_the_last_free_slot_claims_a_floor_host_once() {
    let (app, cloud, org, user, _env) = setup().await;
    assert_eq!(
        decide(&app, org.id).await.provisioned,
        0,
        "the floor never creates a region's first host"
    );
    let response = app
        .authenticated_client(&user)
        .post_json(
            &format!("/api/v1/orgs/{}/worker-hosts", org.id),
            &json!({"region":"us-central1"}),
        )
        .await;
    assert_eq!(response.status(), reqwest::StatusCode::CREATED);
    let body: Value = response.json().await.unwrap();
    let manual = Uuid::parse_str(body["data"]["hostId"].as_str().unwrap()).unwrap();
    helpers::wait_for_cloud_instance_id(&app, manual).await;
    app.host_control_backend()
        .accept_hello(
            manual,
            &allocated_hello(&app, manual).await,
            &HostConnection::new(true),
        )
        .await
        .unwrap();
    assert_eq!(
        decide(&app, org.id).await.provisioned,
        0,
        "one free slot meets the default floor"
    );
    let (w, _) = pending(&app, &org).await;
    place(&app, org.id, w).await;
    assert_eq!(
        decide(&app, org.id).await.provisioned,
        1,
        "the last slot taken: a floor host is claimed"
    );
    let claims = floor_claims(&app, org.id).await;
    assert_eq!(claims.len(), 1);
    let claim = claims[0];
    let (instance_type, created_by): (String, Option<Uuid>) = sqlx::query_as(
        "SELECT instance_type, created_by_user_id FROM worker_hosts WHERE host_id=$1",
    )
    .bind(claim)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(
        (instance_type.as_str(), created_by),
        ("c3-standard-4", None)
    );
    assert_eq!(
        decide(&app, org.id).await.provisioned,
        0,
        "the in-flight claim covers the floor"
    );
    // The claim launches through the shared claim path once a sweep runs it.
    tick(&app, org.id).await;
    helpers::wait_for_cloud_instance_id(&app, claim).await;
    assert_eq!(
        cloud
            .calls()
            .iter()
            .filter(|c| matches!(c, MockCall::ProvisionInstance { .. }))
            .count(),
        2
    );
    // Once it registers, the floor is met again and nothing else moves.
    app.host_control_backend()
        .accept_hello(
            claim,
            &allocated_hello(&app, claim).await,
            &HostConnection::new(true),
        )
        .await
        .unwrap();
    let pass = decide(&app, org.id).await;
    assert_eq!((pass.provisioned, pass.retired), (0, 0));
    assert_eq!(floor_claims(&app, org.id).await.len(), 1);
    app.cleanup().await;
}

/// The floor needs automatic growth and follows the administrator's value;
/// a stale host's free slots never count, so a stale fleet gets a current
/// floor host and its empty stale host retires at once.
#[tokio::test]
#[serial_test::serial]
async fn the_floor_follows_growth_and_its_value_and_ignores_stale_slots() {
    let (app, _cloud, org, user, _env) = setup().await;
    let h = host(&app, org.id, 1, true).await;
    let (w, _) = pending(&app, &org).await;
    place(&app, org.id, w).await;
    set_capacity(&app, &user, org.id, json!({"autoAddHosts": false})).await;
    assert_eq!(
        decide(&app, org.id).await.provisioned,
        0,
        "growth off: the floor claims nothing"
    );
    assert!(floor_claims(&app, org.id).await.is_empty());
    set_capacity(
        &app,
        &user,
        org.id,
        json!({"autoAddHosts": true, "minFreeSlots": 0}),
    )
    .await;
    assert_eq!(decide(&app, org.id).await.provisioned, 0, "floor off");
    set_capacity(&app, &user, org.id, json!({"minFreeSlots": 1})).await;
    assert_eq!(decide(&app, org.id).await.provisioned, 1);
    let claim = floor_claims(&app, org.id).await[0];
    // The claim registers as a current host; the occupied host goes stale.
    // The current host holds the free slot, so nothing more is claimed.
    app.host_control_backend()
        .accept_hello(
            claim,
            &allocated_hello(&app, claim).await,
            &HostConnection::new(true),
        )
        .await
        .unwrap();
    make_runtime_stale(&app, h).await;
    assert_eq!(
        decide(&app, org.id).await.provisioned,
        0,
        "the current floor host holds the free slot"
    );
    app.cleanup().await;
}

/// A stale fleet alone: the empty stale host's slots count for nothing, so
/// the floor claims a current host and the stale host retires without delay
/// (`g5.runtime.placement.runtime-identity`).
#[tokio::test]
#[serial_test::serial]
async fn a_stale_fleet_gets_a_current_floor_host_and_the_empty_stale_host_retires() {
    let (app, _cloud, org, _, _env) = setup().await;
    let stale = host(&app, org.id, 4, true).await;
    make_runtime_stale(&app, stale).await;
    let pass = decide(&app, org.id).await;
    assert_eq!(
        (pass.provisioned, pass.retired),
        (1, 1),
        "a current floor host is claimed and the empty stale host retires"
    );
    assert_eq!(floor_claims(&app, org.id).await.len(), 1);
    // The region now holds no established host, yet the claim already
    // exists: recovery launches it instead of reading the region as empty.
    let claim = floor_claims(&app, org.id).await[0];
    tick(&app, org.id).await;
    helpers::wait_for_cloud_instance_id(&app, claim).await;
    let drain_reason: Option<String> =
        sqlx::query_scalar("SELECT drain_reason FROM worker_hosts WHERE host_id=$1")
            .bind(stale)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(drain_reason.as_deref(), Some("automatic_stale"));
    app.cleanup().await;
}

/// A floor claim shares the Start claim's lifecycle: it is abandoned once
/// other supply covers the floor; and when a Start on a full fleet claimed
/// a host beside the floor's claim and the Start is then cancelled, the
/// floor keeps exactly one of the two claims and launches it.
#[tokio::test]
#[serial_test::serial]
async fn floor_claims_are_abandoned_or_kept_by_what_the_floor_needs() {
    let (app, _cloud, org, _, _env) = setup().await;
    host(&app, org.id, 1, true).await;
    let (w, _) = pending(&app, &org).await;
    place(&app, org.id, w).await;
    assert_eq!(decide(&app, org.id).await.provisioned, 1);
    let claim = floor_claims(&app, org.id).await[0];
    // Manual in-flight capacity appears before the claim launches.
    let covering = Uuid::new_v4();
    sqlx::query("INSERT INTO worker_hosts(host_id,org_id,region,instance_type,status,workspace_vm_limit,cloud_instance_id)
        VALUES($1,$2,'us-central1','c3-standard-4','provisioning',1,'mock-manual-capacity')")
        .bind(covering).bind(org.id).execute(&app.db).await.unwrap();
    tick(&app, org.id).await;
    wait_sql(
        &app,
        "SELECT status='terminated' AND cloud_deleted_at IS NOT NULL
        FROM worker_hosts WHERE host_id=$1",
        claim,
    )
    .await;

    // A Start on a full fleet claims a host; cancelling the Start leaves the
    // floor short, so the claim stays and launches instead of being dropped.
    sqlx::query(
        "UPDATE worker_hosts SET status='terminated', cloud_deleted_at=now() WHERE host_id=$1",
    )
    .bind(covering)
    .execute(&app.db)
    .await
    .unwrap();
    let (w2, intent) = pending(&app, &org).await;
    let pass = decide(&app, org.id).await;
    assert!(pass.provisioned >= 1);
    let start_claim: Uuid = sqlx::query_scalar(
        "SELECT host_id FROM worker_hosts WHERE org_id=$1 AND capacity_start_intent_id=$2",
    )
    .bind(org.id)
    .bind(intent)
    .fetch_one(&app.db)
    .await
    .unwrap();
    let claims_before: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM worker_hosts WHERE org_id=$1 AND status='provisioning'",
    )
    .bind(org.id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(
        claims_before, 2,
        "one claim for the Start, one for the floor"
    );
    sqlx::query("UPDATE workspace_vms SET start_intent=jsonb_set(start_intent,'{status}','\"cancelled\"') WHERE workspace_id=$1")
        .bind(w2).execute(&app.db).await.unwrap();
    tick(&app, org.id).await;
    // Exactly one claim survives for the floor and launches; the other is
    // abandoned as unnecessary.
    wait_sql(
        &app,
        "SELECT (SELECT count(*) FROM worker_hosts WHERE org_id=$1 AND status='provisioning') = 1
            AND (SELECT count(*) FROM worker_hosts WHERE org_id=$1 AND status='provisioning'
                 AND cloud_instance_id IS NOT NULL) = 1",
        org.id,
    )
    .await;
    let _ = start_claim;
    app.cleanup().await;
}

/// A floor claim that hits a stockout retries on its own row and, while it
/// is young, holds the floor: no second claim is made.
#[tokio::test]
#[serial_test::serial]
async fn a_failed_floor_claim_retries_on_its_row_and_blocks_a_second_claim() {
    let (app, cloud, org, _, _env) = setup().await;
    let _retry = EnvVarGuard::set_many(&[
        ("G5_PROVISION_RETRY_ENABLED", "true"),
        ("G5_WORKER_FALLBACK_REGIONS", "us-east1"),
    ]);
    cloud.fail_provision_stockout(true);
    host(&app, org.id, 1, true).await;
    let (w, _) = pending(&app, &org).await;
    place(&app, org.id, w).await;
    assert_eq!(tick(&app, org.id).await.provisioned, 1);
    let claim = floor_claims(&app, org.id).await[0];
    wait_sql(
        &app,
        "SELECT provision_error_code='zone_capacity_exhausted' FROM worker_hosts WHERE host_id=$1",
        claim,
    )
    .await;
    // The manual cross-region stockout retry cannot adopt a floor claim: the
    // capacity guard clears its schedule, as it does for a Start's claim.
    let scheduled: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
        "UPDATE worker_hosts SET provision_next_retry_at=now() WHERE host_id=$1
         RETURNING provision_next_retry_at",
    )
    .bind(claim)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(scheduled, None);
    sqlx::query(
        "UPDATE worker_hosts SET bringup_heartbeat_at=now()-interval '2 minutes' WHERE host_id=$1",
    )
    .bind(claim)
    .execute(&app.db)
    .await
    .unwrap();
    assert_eq!(tick(&app, org.id).await.provisioned, 0);
    wait_sql(
        &app,
        "SELECT provision_retry_count=2 FROM worker_hosts WHERE host_id=$1",
        claim,
    )
    .await;
    assert_eq!(floor_claims(&app, org.id).await, vec![claim]);
    let region: String = sqlx::query_scalar("SELECT region FROM worker_hosts WHERE host_id=$1")
        .bind(claim)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(region, "us-central1", "retries stay in the demand region");
    app.cleanup().await;
}

/// A claim that demand is assigned to in a later sweep launches like a
/// Start's claim, whatever it was claimed for: here a floor claim left
/// unlaunched (as a restart would) that a queued Start then lands on.
#[tokio::test]
#[serial_test::serial]
async fn a_floor_claim_that_demand_is_assigned_to_still_launches() {
    let (app, _cloud, org, _, _env) = setup().await;
    host(&app, org.id, 1, true).await;
    let (w, _) = pending(&app, &org).await;
    place(&app, org.id, w).await;
    assert_eq!(decide(&app, org.id).await.provisioned, 1);
    let claim = floor_claims(&app, org.id).await[0];
    let (w2, _) = pending(&app, &org).await;
    let pass = tick(&app, org.id).await;
    assert_eq!(
        pass.provisioned, 1,
        "the Start takes the floor claim's slot, so the floor claims again"
    );
    helpers::wait_for_cloud_instance_id(&app, claim).await;
    let reason: String =
        sqlx::query_scalar("SELECT last_error->>'reason' FROM workspace_vms WHERE workspace_id=$1")
            .bind(w2)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(reason, "preparing_capacity");
    app.cleanup().await;
}

/// A Start's claim from an earlier sweep serves its Start; it does not cover
/// the floor, which claims its own host.
#[tokio::test]
#[serial_test::serial]
async fn a_start_claim_from_an_earlier_sweep_does_not_cover_the_floor() {
    let (app, _cloud, org, user, _env) = setup().await;
    host(&app, org.id, 1, true).await;
    let (w, _) = pending(&app, &org).await;
    place(&app, org.id, w).await;
    set_capacity(&app, &user, org.id, json!({"minFreeSlots": 0})).await;
    let (_, intent) = pending(&app, &org).await;
    assert_eq!(decide(&app, org.id).await.provisioned, 1);
    assert!(floor_claims(&app, org.id).await.is_empty());
    set_capacity(&app, &user, org.id, json!({"minFreeSlots": 1})).await;
    assert_eq!(
        decide(&app, org.id).await.provisioned,
        1,
        "the Start's claim is assigned to its Start; the floor claims beside it"
    );
    assert_eq!(floor_claims(&app, org.id).await.len(), 1);
    let start_claims: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM worker_hosts WHERE org_id=$1 AND capacity_start_intent_id=$2",
    )
    .bind(org.id)
    .bind(intent)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(start_claims, 1);
    app.cleanup().await;
}

/// The demand region moves before a floor claim launches: the claim is in
/// the old region, so the floor no longer needs it and nothing boots it in
/// either region.
#[tokio::test]
#[serial_test::serial]
async fn a_floor_claim_outside_the_demand_region_is_abandoned_not_moved() {
    let (app, cloud, org, _, _env) = setup().await;
    host(&app, org.id, 1, true).await;
    let (w, _) = pending(&app, &org).await;
    place(&app, org.id, w).await;
    assert_eq!(decide(&app, org.id).await.provisioned, 1);
    let claim = floor_claims(&app, org.id).await[0];
    sqlx::query(
        "INSERT INTO configs(org_id,scope,scope_id,key,value) VALUES($1,'org',$1,'worker_host_region','\"us-east1\"')
         ON CONFLICT(org_id,scope,scope_id,key) DO UPDATE SET value=EXCLUDED.value",
    )
    .bind(org.id)
    .execute(&app.db)
    .await
    .unwrap();
    tick(&app, org.id).await;
    wait_sql(
        &app,
        "SELECT status='terminated' AND cloud_deleted_at IS NOT NULL AND region='us-central1'
         FROM worker_hosts WHERE host_id=$1",
        claim,
    )
    .await;
    assert!(
        !cloud
            .calls()
            .iter()
            .any(|c| matches!(c, MockCall::ProvisionInstance { .. })),
        "no cloud insert in either region"
    );
    app.cleanup().await;
}

/// Removal keeps the floor (`g5.architecture.host_idle_retirement`): with a
/// floor of two, the other hosts must keep two free slots, so the small host
/// retires and the large one stays; a floor above the remaining supply then
/// stops removal and claims instead.
#[tokio::test]
#[serial_test::serial]
async fn removal_keeps_the_floor_of_free_slots() {
    let (app, _cloud, org, user, _env) = setup().await;
    let large = host(&app, org.id, 4, true).await;
    let small = host(&app, org.id, 1, true).await;
    set_capacity(&app, &user, org.id, json!({"minFreeSlots": 2})).await;
    let pass = decide(&app, org.id).await;
    assert_eq!((pass.provisioned, pass.retired), (0, 1));
    let status = |id: Uuid| {
        let db = app.db.clone();
        async move {
            sqlx::query_scalar::<_, String>("SELECT status FROM worker_hosts WHERE host_id=$1")
                .bind(id)
                .fetch_one(&db)
                .await
                .unwrap()
        }
    };
    assert_eq!(status(small).await, "terminated");
    assert_eq!(status(large).await, "ready");
    set_capacity(&app, &user, org.id, json!({"minFreeSlots": 5})).await;
    let pass = decide(&app, org.id).await;
    assert_eq!(
        (pass.provisioned, pass.retired),
        (1, 0),
        "four free slots are below a floor of five: claim, never retire"
    );
    assert_eq!(status(large).await, "ready");
    app.cleanup().await;
}

/// The resource bound holds the floor claim back while two empty stale
/// automatic hosts fill it; both retire at once, and the floor still claims
/// a successor after their cloud cleanup frees the bound: automatic
/// retirement of the region's last established host does not end the floor
/// (`g5.architecture.host_demand_capacity`).
#[tokio::test]
#[serial_test::serial]
async fn the_floor_recovers_after_a_stale_fleet_retires_against_the_resource_bound() {
    let (app, _cloud, org, _, _env) = setup().await;
    sqlx::query("INSERT INTO configs(org_id,scope,scope_id,key,value) VALUES($1,'org',$1,'max_running_vms','1')
        ON CONFLICT(org_id,scope,scope_id,key) DO UPDATE SET value='1'").bind(org.id).execute(&app.db).await.unwrap();
    let hosts = [
        host(&app, org.id, 1, true).await,
        host(&app, org.id, 1, true).await,
    ];
    for id in hosts {
        make_runtime_stale(&app, id).await;
        sqlx::query(
            "UPDATE worker_hosts SET automatic_capacity=true, idle_since=NULL WHERE host_id=$1",
        )
        .bind(id)
        .execute(&app.db)
        .await
        .unwrap();
    }
    let pass = decide(&app, org.id).await;
    assert_eq!(
        (pass.provisioned, pass.retired),
        (0, 2),
        "two unconfirmed stale resources fill the bound; both retire, no claim yet"
    );
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if tick(&app, org.id).await.provisioned == 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the floor claims once the stale fleet's cloud cleanup frees the bound");
    assert_eq!(floor_claims(&app, org.id).await.len(), 1);
    app.cleanup().await;
}

/// An operator's removal ends the floor's standing in the region: after an
/// automatic retirement, deleting the last host through the API leaves the
/// floor nothing to top up, so no host is re-created
/// (`g5.architecture.host_demand_capacity`).
#[tokio::test]
#[serial_test::serial]
async fn an_operator_removal_ends_the_floor_after_an_automatic_retirement() {
    let (app, cloud, org, user, _env) = setup().await;
    let a = host(&app, org.id, 1, true).await;
    let b = host(&app, org.id, 1, true).await;
    let pass = decide(&app, org.id).await;
    assert_eq!((pass.provisioned, pass.retired), (0, 1));
    let remaining: Uuid =
        sqlx::query_scalar("SELECT host_id FROM worker_hosts WHERE org_id=$1 AND status='ready'")
            .bind(org.id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(remaining == a || remaining == b);
    let retired_presence: bool = sqlx::query_scalar(
        "SELECT bool_and(capacity_floor_presence) FROM worker_hosts WHERE org_id=$1 AND status='terminated'",
    )
    .bind(org.id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(
        retired_presence,
        "automatic retirement keeps the region present"
    );
    let response = app
        .authenticated_client(&user)
        .delete(&format!(
            "/api/v1/orgs/{}/worker-hosts/{}",
            org.id, remaining
        ))
        .await;
    assert!(response.status().is_success(), "{}", response.status());
    let any_presence: bool = sqlx::query_scalar(
        "SELECT bool_or(capacity_floor_presence) FROM worker_hosts WHERE org_id=$1",
    )
    .bind(org.id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(
        !any_presence,
        "the operator's removal ends the region's presence"
    );
    for _ in 0..3 {
        assert_eq!(tick(&app, org.id).await.provisioned, 0);
    }
    assert!(floor_claims(&app, org.id).await.is_empty());
    assert!(!cloud
        .calls()
        .iter()
        .any(|c| matches!(c, MockCall::ProvisionInstance { .. })));
    app.cleanup().await;
}

/// An exhausted floor claim past its window is terminated even while a Start
/// waits, so it stops holding a resource against the bound; a Start's own
/// exhausted claim stays as that Start's evidence
/// (`g5.architecture.host_demand_capacity`).
#[tokio::test]
#[serial_test::serial]
async fn an_expired_floor_claim_frees_the_bound_for_a_waiting_start() {
    let (app, _cloud, org, _, _env) = setup().await;
    sqlx::query("INSERT INTO configs(org_id,scope,scope_id,key,value) VALUES($1,'org',$1,'max_running_vms','2')
        ON CONFLICT(org_id,scope,scope_id,key) DO UPDATE SET value='2'").bind(org.id).execute(&app.db).await.unwrap();
    // One occupied automatic host and two exhausted claims fill `quota + 1`.
    let occupied = host(&app, org.id, 1, true).await;
    sqlx::query(
        "UPDATE worker_hosts SET automatic_capacity=true, idle_since=NULL WHERE host_id=$1",
    )
    .bind(occupied)
    .execute(&app.db)
    .await
    .unwrap();
    let (w, _) = pending(&app, &org).await;
    place(&app, org.id, w).await;
    let floor_claim = Uuid::new_v4();
    let start_claim = Uuid::new_v4();
    for (id, floor, intent) in [
        (floor_claim, true, None::<Uuid>),
        (start_claim, false, Some(Uuid::new_v4())),
    ] {
        sqlx::query(
            "INSERT INTO worker_hosts(host_id,org_id,region,instance_type,status,automatic_capacity,
                capacity_floor_claim,capacity_start_intent_id,provision_retry_count,
                provision_error_code,created_at)
             VALUES($1,$2,'us-central1','c3-standard-4','provisioning',true,$3,$4,3,
                'zone_capacity_exhausted',now()-interval '16 minutes')",
        )
        .bind(id)
        .bind(org.id)
        .bind(floor)
        .bind(intent)
        .execute(&app.db)
        .await
        .unwrap();
    }
    let (w2, _) = pending(&app, &org).await;
    assert_eq!(tick(&app, org.id).await.provisioned, 0, "the bound is full");
    let reason: String =
        sqlx::query_scalar("SELECT last_error->>'reason' FROM workspace_vms WHERE workspace_id=$1")
            .bind(w2)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(reason, "capacity_limit");
    // The expired floor claim is terminated although the Start waits; the
    // Start's claim stays as evidence.
    wait_sql(
        &app,
        "SELECT status='terminated' AND cloud_deleted_at IS NOT NULL FROM worker_hosts WHERE host_id=$1",
        floor_claim,
    )
    .await;
    let start_claim_status: String =
        sqlx::query_scalar("SELECT status FROM worker_hosts WHERE host_id=$1")
            .bind(start_claim)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(start_claim_status, "provisioning");
    // With the bound freed, the waiting Start claims.
    assert_eq!(tick(&app, org.id).await.provisioned, 1);
    let reason: String =
        sqlx::query_scalar("SELECT last_error->>'reason' FROM workspace_vms WHERE workspace_id=$1")
            .bind(w2)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(reason, "preparing_capacity");
    app.cleanup().await;
}

/// An operator's removal takes the organization admission lock before any
/// host row, so it waits for an in-flight capacity decision instead of
/// deadlocking with the sweep's row locks.
#[tokio::test]
#[serial_test::serial]
async fn an_operator_removal_waits_for_an_in_flight_capacity_decision() {
    let (app, _cloud, org, user, _env) = setup().await;
    let id = host(&app, org.id, 1, true).await;
    let mut sweep_db = app.scoped_db_for_org(org.id).await;
    g5_api::services::org_admin_settings::lock_org_vm_quota(sweep_db.conn(), org.id)
        .await
        .unwrap();
    let removal = tokio::spawn({
        let client = AuthenticatedClient::new(
            reqwest::Client::new(),
            app.base_url.clone(),
            user.jwt.clone(),
        );
        let path = format!("/api/v1/orgs/{}/worker-hosts/{}", org.id, id);
        async move { client.delete(&path).await.status() }
    });
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(
        !removal.is_finished(),
        "the removal must wait for the admission lock"
    );
    sweep_db.commit().await.unwrap();
    let status = tokio::time::timeout(Duration::from_secs(10), removal)
        .await
        .expect("the removal completes once the sweep commits")
        .unwrap();
    assert!(status.is_success(), "{status}");
    app.cleanup().await;
}

/// An operator's removal of the last established host also ends a floor
/// claim still awaiting launch: recovery requires the region to be present,
/// so the claim is abandoned without a cloud insert
/// (`g5.architecture.host_demand_capacity`).
#[tokio::test]
#[serial_test::serial]
async fn a_pending_floor_claim_is_abandoned_after_the_operator_removes_the_last_host() {
    let (app, cloud, org, user, _env) = setup().await;
    set_capacity(&app, &user, org.id, json!({"minFreeSlots": 2})).await;
    let only = host(&app, org.id, 1, true).await;
    sqlx::query("UPDATE worker_hosts SET idle_since=NULL WHERE host_id=$1")
        .bind(only)
        .execute(&app.db)
        .await
        .unwrap();
    assert_eq!(decide(&app, org.id).await.provisioned, 1);
    let claim = floor_claims(&app, org.id).await[0];
    let response = app
        .authenticated_client(&user)
        .delete(&format!("/api/v1/orgs/{}/worker-hosts/{}", org.id, only))
        .await;
    assert!(response.status().is_success(), "{}", response.status());
    tick(&app, org.id).await;
    wait_sql(
        &app,
        "SELECT status='terminated' AND cloud_deleted_at IS NOT NULL FROM worker_hosts WHERE host_id=$1",
        claim,
    )
    .await;
    for _ in 0..2 {
        assert_eq!(tick(&app, org.id).await.provisioned, 0);
    }
    assert!(!cloud
        .calls()
        .iter()
        .any(|c| matches!(c, MockCall::ProvisionInstance { .. })));
    app.cleanup().await;
}

/// Issue #627 Codex review: identity reconciliation is independent of retry
/// eligibility. An ASSIGNED claim at the attempt limit whose last insert
/// created an instance but lost linkage persistence must still be looked up
/// by its deterministic name — drive_claim attaches without consuming an
/// attempt and never inserts past the budget.
#[tokio::test]
#[serial_test::serial]
async fn an_exhausted_assigned_claim_still_reconciles_its_identity() {
    let (app, cloud, org, _, _env) = setup().await;
    let (_, intent) = pending(&app, &org).await;
    assert_eq!(decide(&app, org.id).await.provisioned, 1);
    let claim: Uuid = sqlx::query_scalar(
        "SELECT host_id FROM worker_hosts WHERE org_id=$1 AND capacity_start_intent_id=$2",
    )
    .bind(org.id)
    .bind(intent)
    .fetch_one(&app.db)
    .await
    .unwrap();
    // Attempts exhausted, lease lapsed, linkage lost — retry_due is false and
    // the claim is minutes old (three backoffs alone exceed the young-row
    // window), so it is not in flight either.
    sqlx::query(
        "UPDATE worker_hosts SET provision_retry_count = 3, cloud_instance_id = NULL,
             bringup_heartbeat_at = now() - interval '10 minutes',
             created_at = now() - interval '5 minutes' WHERE host_id = $1",
    )
    .bind(claim)
    .execute(&app.db)
    .await
    .unwrap();
    // The instance its last attempt created IS alive under the deterministic name.
    let name = g5_api::cloud::host_instance_name(claim, &org.slug);
    cloud.set_instance_by_name("us-central1", &name, g5_api::cloud::InstanceState::Running);

    let pass = decide(&app, org.id).await;
    assert!(
        pass.launches.contains(&claim),
        "exhausted assigned claim still launches identity reconciliation: {pass:?}"
    );
    tick(&app, org.id).await;
    wait_sql(
        &app,
        "SELECT cloud_instance_id IS NOT NULL FROM worker_hosts WHERE host_id=$1",
        claim,
    )
    .await;
    let (linkage, attempts): (Option<String>, i32) = sqlx::query_as(
        "SELECT cloud_instance_id, provision_retry_count FROM worker_hosts WHERE host_id=$1",
    )
    .bind(claim)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(
        linkage.as_deref(),
        Some(format!("us-central1-a/{name}").as_str())
    );
    assert_eq!(attempts, 3, "attaching consumed no retry attempt");
    app.cleanup().await;
}
