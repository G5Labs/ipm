//! Integration tests for the rev-7 worker-host CLOUD bring-up wire-in
//! (PR-rev-7-9c): POST → `ensure_vpc_for_org` + `provision_instance` + ref
//! persistence; DELETE → `terminate_instance`; the background reconciler's
//! `provisioning → ready`/`terminated` transitions.
//!
//! These run against a real Postgres (via `TestApp`) but are **fully offline**:
//! the only `CloudProvider` is [`MockCloudProvider`]; no real GCP call is ever
//! made. The mock records every call so we can assert the handler invoked the
//! cloud backend with the expected arguments, and its status is programmable so
//! the reconciler state machine can be driven deterministically without sleeps.
//!
//! Contract: `generation/.semlift/specs/g5/g5-api/worker_hosts.semlift.md`
//! [D20], [D22], [D23], [D24], [D25].

use crate::helpers;

use std::sync::Arc;

use helpers::{
    provision_instance_boot_disk_gb, provision_instance_startup_script, wait_for_cloud_instance_id,
    EnvVarGuard, TestApp, TestOrg, TestUser,
};
use reqwest::StatusCode;
use serde_json::{json, Value};
use uuid::Uuid;

use g5_api::cloud::{CloudProvider, InstanceState, MockCall, MockCloudProvider};

fn response_data(body: &Value) -> &Value {
    body.get("data").expect("response body should include data")
}

/// Spawn a TestApp with an injected `MockCloudProvider`, returning both the app
/// and the concrete mock handle (for call assertions + status programming).
async fn spawn_with_mock() -> (TestApp, Arc<MockCloudProvider>) {
    let mock = Arc::new(MockCloudProvider::new());
    let app = TestApp::spawn_with_cloud(mock.clone() as Arc<dyn CloudProvider>).await;
    (app, mock)
}

/// Provision one host via the API and return the parsed `data` object.
async fn provision(client: &helpers::AuthenticatedClient, org_id: Uuid, body: Value) -> Value {
    let mut body = body;
    let object = body.as_object_mut().expect("provision body is an object");
    if !object.contains_key("baseImageRef") && !object.contains_key("imageTrack") {
        object.insert(
            "baseImageRef".to_string(),
            json!(helpers::TEST_WORKER_IMAGE_REF),
        );
    }
    let resp = client
        .post_json(&format!("/api/v1/orgs/{org_id}/worker-hosts"), &body)
        .await;
    assert_eq!(resp.status(), StatusCode::CREATED, "provision returns 201");
    let body: Value = resp.json().await.expect("provision response is JSON");
    response_data(&body).clone()
}

async fn accept_host_hello(app: &TestApp, host_id: Uuid) {
    use g5_api::ws::host_ctl::{ControlPlaneHostState, HostConnection};

    let (slots, capacity): (i32, i32) =
        sqlx::query_as("SELECT slots_total,scratch_disk_mib FROM worker_hosts WHERE host_id=$1")
            .bind(host_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    app.host_control_backend()
        .accept_hello(
            host_id,
            &g5_internal_api::host_ctl::HostHello {
                host_id,
                protocol_version: g5_internal_api::host_ctl::PROTOCOL_VERSION,
                capabilities: Vec::new(),
                slots_total: slots as u32,
                slot_size: Some(g5_internal_api::host_ctl::VmSizeSpec {
                    cpus: 3,
                    mem_mib: 13273,
                    scratch_disk_mib: capacity as u32,
                }),
                inventory: Vec::new(),
                retained_roots: Vec::new(),
                runtime: Some(g5_internal_api::host_ctl::HostRuntimeIdentity::bundled()),
            },
            &HostConnection::new(true),
        )
        .await
        .expect("host registers its measured slot contract");
}

/// Wait for the spawned bring-up task to finish — used by tests that want to
/// observe the failure-path side effects (`provision_error_code` write,
/// orphan log). Polls the predicate every 25ms up to 5s; panics on timeout
/// with a diagnostic about which observable the test was waiting on.
async fn wait_for<F, Fut>(predicate: F, what: &str) -> ()
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        if predicate().await {
            return;
        }
        if std::time::Instant::now() >= deadline {
            panic!("predicate {what} never became true within 5s");
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
}

#[tokio::test]
#[serial_test::serial(worker_scratch_disk)]
async fn provision_triggers_vpc_and_instance_and_persists_refs() {
    let _env = EnvVarGuard::set_many(&[("G5_WORKER_SCRATCH_DISK_MIB", "81920")]);
    let (app, mock) = spawn_with_mock().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    let data = provision(
        &client,
        org.id,
        json!({ "region": "us-central1", "instanceType": "n2-standard-16" }),
    )
    .await;
    let host_id = Uuid::parse_str(data["hostId"].as_str().unwrap()).unwrap();
    let host_jwt = data["hostJwt"].as_str().unwrap().to_string();

    // Round-3 async-spawn (ontology [D22]): the 201 returns immediately and
    // the bring-up runs in a detached `tokio::spawn`. Wait for the spawned
    // task's `UPDATE worker_hosts SET cloud_instance_id` to land before we
    // assert on cloud-side calls and DB state.
    let _ = wait_for_cloud_instance_id(&app, host_id).await;

    // The handler invoked the cloud backend with the expected arguments.
    let calls = mock.calls();
    assert!(
        calls.contains(&MockCall::EnsureVpc { org_id: org.id }),
        "ensure_vpc_for_org called with the path org; calls={calls:?}"
    );
    let provision_call = calls
        .iter()
        .find_map(|c| match c {
            MockCall::ProvisionInstance {
                host_id: h,
                org_slug,
                created_by,
                vpc_id,
                instance_type,
                region,
                startup_script,
                ..
            } => Some((
                *h,
                vpc_id.clone(),
                instance_type.clone(),
                region.clone(),
                startup_script.clone(),
                org_slug.clone(),
                created_by.clone(),
            )),
            _ => None,
        })
        .expect("provision_instance was called");
    assert_eq!(provision_call.0, host_id, "provisioned for the new host");
    assert_eq!(
        provision_call.1,
        format!("g5-org-{}", org.id),
        "VPC ref threaded through"
    );
    assert_eq!(provision_call.2, "n2-standard-16", "requested machine type");
    assert_eq!(provision_call.3, "us-central1", "requested region");
    // [D44]: the org slug and creator identity reach the provider call so the
    // instance name and `g5_created_by*` labels can be stamped.
    assert_eq!(provision_call.5, org.slug, "org slug threaded through");
    let actor = provision_call.6.expect("creator identity threaded through");
    assert_eq!(actor.user_id, user.id, "creator is the POSTing user");
    assert_eq!(
        actor.email.as_deref(),
        Some(user.email.as_str()),
        "creator email resolved from users.email"
    );
    // The startup script carries the per-host identity (host_jwt + host_id).
    assert!(
        provision_call.4.contains(&host_jwt),
        "startup script embeds host_jwt"
    );
    assert!(
        provision_call.4.contains(&host_id.to_string()),
        "startup script embeds host_id"
    );
    assert!(
        !provision_call.4.contains("G5D_HOST_CPUS_PER_VM="),
        "startup script leaves CPU sizing to the measured host"
    );
    assert!(
        !provision_call.4.contains("G5D_HOST_MEM_MIB_PER_VM="),
        "startup script leaves memory sizing to the measured host"
    );
    assert!(
        provision_call.4.contains("G5D_SCRATCH_DISK_MIB='81920'"),
        "startup script pins the configured scratch size"
    );

    // The cloud refs were persisted onto the row (not NULL — this is the 9c
    // delta from 9b's DB-only contract).
    let row = sqlx::query_as::<_, (Option<String>, Option<String>, i32, i32, i32)>(
        "SELECT vpc_id, cloud_instance_id, slot_cpus, slot_mem_mib, scratch_disk_mib \
         FROM worker_hosts WHERE host_id = $1",
    )
    .bind(host_id)
    .fetch_one(&app.db)
    .await
    .expect("row exists");
    assert_eq!(
        row.0.as_deref(),
        Some(format!("g5-org-{}", org.id).as_str())
    );
    assert_eq!(row.1.as_deref(), Some("mock-instance-1"));
    assert_eq!((row.2, row.3, row.4), (2, 4096, 81920));

    app.cleanup().await;
}

#[tokio::test]
async fn cloud_instance_wait_allows_healthy_detached_bringup_beyond_five_seconds() {
    let (app, mock) = spawn_with_mock().await;
    mock.set_provision_delay(std::time::Duration::from_millis(5_250));
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    let data = provision(
        &client,
        org.id,
        json!({ "region": "us-central1", "instanceType": "n2-standard-16" }),
    )
    .await;
    let host_id = Uuid::parse_str(data["hostId"].as_str().unwrap()).unwrap();

    let cloud_instance_id = wait_for_cloud_instance_id(&app, host_id).await;
    assert!(
        cloud_instance_id.starts_with("mock-instance-"),
        "healthy delayed bring-up should persist its provider linkage"
    );
    app.cleanup().await;
}

#[tokio::test]
async fn cloud_instance_wait_reports_terminal_provisioning_error_snapshot() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    let data = provision(
        &client,
        org.id,
        json!({ "region": "us-central1", "instanceType": "n2-standard-16" }),
    )
    .await;
    let host_id = Uuid::parse_str(data["hostId"].as_str().unwrap()).unwrap();
    sqlx::query(
        "UPDATE worker_hosts SET provision_error_code = 'base_image_below_glibc_floor' \
         WHERE host_id = $1",
    )
    .bind(host_id)
    .execute(&app.db)
    .await
    .expect("stamp terminal provisioning error");

    let diagnostic = helpers::wait_for_cloud_instance_id_result(&app, host_id)
        .await
        .expect_err("terminal provisioning errors must stop the semantic wait");
    assert!(diagnostic.contains("terminal provisioning error"));
    assert!(diagnostic.contains("status=provisioning"));
    assert!(diagnostic.contains("base_image_below_glibc_floor"));
    assert!(diagnostic.contains("provision_retry_count=0"));
    assert!(diagnostic.contains("provision_next_retry_at=None"));
    app.cleanup().await;
}

#[tokio::test]
async fn bringup_threads_base_image_into_startup_script() {
    // PR1 (ontology [D44]): a digest-pinned baseImageRef is threaded into the
    // startup script as G5D_HOST_BASE_IMAGE; an omitted ref keeps the
    // buildpack-deps:noble default. We assert against the mock-captured startup_script.
    let (app, mock) = spawn_with_mock().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    let hex = "a".repeat(64);
    let reference = format!("ghcr.io/acme/app@sha256:{hex}");

    // (a) Digest-pinned ref → present in G5D_HOST_BASE_IMAGE.
    let data = provision(
        &client,
        org.id,
        json!({ "region": "us-central1", "baseImageRef": reference }),
    )
    .await;
    let host_id = Uuid::parse_str(data["hostId"].as_str().unwrap()).unwrap();
    let _ = wait_for_cloud_instance_id(&app, host_id).await;

    let script_with_ref = provision_instance_startup_script(&mock, host_id)
        .expect("provision_instance captured for the customer-ref host");
    assert!(
        script_with_ref.contains(&format!("G5D_HOST_BASE_IMAGE='{reference}'")),
        "startup script must thread the digest-pinned base image ref"
    );
    assert!(
        !script_with_ref.contains("G5D_HOST_BASE_IMAGE='buildpack-deps:noble'"),
        "default must not appear when a customer ref is supplied"
    );

    // (b) Omitted request selector → the org's configured image policy. There
    // is no untracked buildpack-deps fallback.
    sqlx::query("UPDATE orgs SET default_base_image_ref = $1 WHERE id = $2")
        .bind(helpers::TEST_WORKER_IMAGE_REF)
        .bind(org.id)
        .execute(&app.db)
        .await
        .expect("configure org worker-image pin");
    let response = client
        .post_json(
            &format!("/api/v1/orgs/{}/worker-hosts", org.id),
            &json!({ "region": "us-east1" }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let response: Value = response.json().await.expect("provision response is JSON");
    let data = response_data(&response);
    let default_host_id = Uuid::parse_str(data["hostId"].as_str().unwrap()).unwrap();
    let _ = wait_for_cloud_instance_id(&app, default_host_id).await;

    let script_default = provision_instance_startup_script(&mock, default_host_id)
        .expect("provision_instance captured for the policy-backed host");
    assert!(
        script_default.contains(&format!(
            "G5D_HOST_BASE_IMAGE='{}'",
            helpers::TEST_WORKER_IMAGE_REF
        )),
        "omitted request selector must resolve the org's immutable image policy"
    );
    assert!(!script_default.contains("buildpack-deps:noble"));

    app.cleanup().await;
}

#[tokio::test]
async fn bringup_threads_workspace_vm_limit_into_max_microvms() {
    // Ontology [D53]/[D54]: `workspaceVmLimit` becomes G5D_HOST_MAX_MICROVMS in
    // the startup script; omitted ⇒ the RAM-targeted default (vCPUs/4 ≈ 16 GB
    // per VM on the 4 GB/vCPU allowlisted shapes). The API also writes the
    // persisted slot size so the host cannot derive a different VmSpec size.
    let (app, mock) = spawn_with_mock().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    // (a) Explicit workspaceVmLimit=2 on an 8-vCPU type ⇒ MAX_MICROVMS=2.
    let data = provision(
        &client,
        org.id,
        json!({ "region": "us-central1", "instanceType": "n2-standard-8", "workspaceVmLimit": 2 }),
    )
    .await;
    let host_id = Uuid::parse_str(data["hostId"].as_str().unwrap()).unwrap();
    let _ = wait_for_cloud_instance_id(&app, host_id).await;
    let script = provision_instance_startup_script(&mock, host_id)
        .expect("provision_instance captured for the explicit-concurrency host");
    assert!(
        script.contains("G5D_HOST_MAX_MICROVMS=2"),
        "explicit workspaceVmLimit must set G5D_HOST_MAX_MICROVMS: {script}"
    );
    assert!(
        !script.contains("G5D_HOST_CPUS_PER_VM=") && !script.contains("G5D_HOST_MEM_MIB_PER_VM="),
        "API must allow hardware-derived CPU/RAM: {script}"
    );

    // (b) Omitted ⇒ the RAM-targeted default ([D54]: n2-standard-16, 64 GB ⇒
    // 16/4 = 4 workspace VMs ≈ 16 GB each).
    let data = provision(
        &client,
        org.id,
        json!({ "region": "us-east1", "instanceType": "n2-standard-16" }),
    )
    .await;
    let default_host_id = Uuid::parse_str(data["hostId"].as_str().unwrap()).unwrap();
    let _ = wait_for_cloud_instance_id(&app, default_host_id).await;
    let script = provision_instance_startup_script(&mock, default_host_id)
        .expect("provision_instance captured for the default-concurrency host");
    assert!(
        script.contains("G5D_HOST_MAX_MICROVMS=4"),
        "omitted workspaceVmLimit must resolve the ~16 GB/VM default: {script}"
    );

    app.cleanup().await;
}

#[tokio::test]
#[serial_test::serial(worker_scratch_disk)]
async fn bringup_sizes_the_boot_disk_from_the_host_vm_budget() {
    // Ontology g5.runtime.placement.host-disk (issue #344): all of a host's VMs
    // share its one boot disk and the host refuses a start unless
    // `scratch_disk_mib` MiB is free. Provisioning uses the fixed
    // 120 GB base plus 80 GB per slot policy; admission checks free space.
    let _env = EnvVarGuard::set_many(&[("G5_WORKER_SCRATCH_DISK_MIB", "65536")]);
    let (app, mock) = spawn_with_mock().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    // (a) 5 slots (c3-standard-22 at its default budget) ⇒ 120 + 5 x 80 = 520.
    let data = provision(
        &client,
        org.id,
        json!({ "region": "us-central1", "instanceType": "c3-standard-22" }),
    )
    .await;
    let five_slot_host = Uuid::parse_str(data["hostId"].as_str().unwrap()).unwrap();
    let _ = wait_for_cloud_instance_id(&app, five_slot_host).await;
    assert_eq!(
        provision_instance_boot_disk_gb(&mock, five_slot_host),
        Some(520),
        "a 5-slot host must get base + 5 slot allowances"
    );

    // (b) A single slot gets the 200 GB automatic floor.
    let data = provision(
        &client,
        org.id,
        json!({ "region": "us-central1", "instanceType": "c3-standard-4" }),
    )
    .await;
    let one_slot_host = Uuid::parse_str(data["hostId"].as_str().unwrap()).unwrap();
    let _ = wait_for_cloud_instance_id(&app, one_slot_host).await;
    assert_eq!(
        provision_instance_boot_disk_gb(&mock, one_slot_host),
        Some(200),
        "a 1-slot host must get the automatic floor"
    );

    // (c) An explicit `workspaceVmLimit` pin drives the disk too: the budget and
    // the disk are resolved from the same number, so they cannot disagree.
    let data = provision(
        &client,
        org.id,
        json!({ "region": "us-central1", "instanceType": "c3-standard-22", "workspaceVmLimit": 11 }),
    )
    .await;
    let pinned_host = Uuid::parse_str(data["hostId"].as_str().unwrap()).unwrap();
    let _ = wait_for_cloud_instance_id(&app, pinned_host).await;
    assert_eq!(
        provision_instance_boot_disk_gb(&mock, pinned_host),
        Some(1000),
        "a pinned 11-slot budget must size the disk for 11 slots"
    );

    // (d) A stored override wins over the derivation, when above the
    // calculated minimum, and is persisted for rotation to reuse.
    let data = provision(
        &client,
        org.id,
        json!({ "region": "us-central1", "instanceType": "c3-standard-22", "bootDiskGb": 600 }),
    )
    .await;
    let override_host = Uuid::parse_str(data["hostId"].as_str().unwrap()).unwrap();
    let _ = wait_for_cloud_instance_id(&app, override_host).await;
    assert_eq!(
        provision_instance_boot_disk_gb(&mock, override_host),
        Some(600),
        "a larger typed size must override the derivation"
    );

    // (e) Private VM disk capacity does not change the host provisioning policy.
    let _larger_scratch = EnvVarGuard::set_many(&[("G5_WORKER_SCRATCH_DISK_MIB", "131072")]);
    let data = provision(
        &client,
        org.id,
        json!({ "region": "us-central1", "instanceType": "c3-standard-22" }),
    )
    .await;
    let larger_scratch_host = Uuid::parse_str(data["hostId"].as_str().unwrap()).unwrap();
    wait_for_cloud_instance_id(&app, larger_scratch_host).await;
    assert_eq!(
        provision_instance_boot_disk_gb(&mock, larger_scratch_host),
        Some(520),
        "a 5-slot host uses the fixed policy even with a different VM disk size"
    );

    // The explicit pin remains separate from the immutable resolved snapshot;
    // replacements resolve current settings while retries retain the snapshot.
    let stored: Vec<(Uuid, Option<i32>)> = sqlx::query_as(
        "SELECT host_id, boot_disk_gb FROM worker_hosts WHERE org_id = $1 ORDER BY created_at",
    )
    .bind(org.id)
    .fetch_all(&app.db)
    .await
    .expect("read boot_disk_gb");
    let stored_for = |id: Uuid| {
        stored
            .iter()
            .find(|(h, _)| *h == id)
            .map(|(_, gb)| *gb)
            .expect("host row")
    };
    assert_eq!(stored_for(override_host), Some(600));
    assert_eq!(
        stored_for(five_slot_host),
        None,
        "derived size is not stored"
    );
    assert_eq!(stored_for(pinned_host), None, "derived size is not stored");

    app.cleanup().await;
}

#[tokio::test]
async fn provision_failure_still_returns_201_and_leaves_refs_null() {
    // cloud-call-after-db-commit ([D21]): a bring-up failure must NOT roll back
    // the record or fail the request. The row stays provisioning with NULL
    // cloud columns — the queryable "cloud half didn't complete" signal.
    //
    // After round-3 async-spawn ([D22]): the failure happens inside the
    // detached `tokio::spawn`d task, so we must wait for that task to have
    // observed and processed the failure before asserting. The mock's
    // failure path runs `provision_instance` → returns the configured Api
    // error → calls `log_cloud_bringup_error` + `persist_provision_error_if_classified`
    // (which is a no-op here because mock's error message has no
    // ZONE_RESOURCE_POOL_EXHAUSTED substring). We wait for the
    // `MockCall::ProvisionInstance` to have been recorded before reading the
    // row — the mock records calls synchronously inside its impl, so seeing
    // the call IS the synchronization signal that the failure branch ran.
    let (app, mock) = spawn_with_mock().await;
    mock.fail_provision(true);
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    let data = provision(&client, org.id, json!({ "region": "us-central1" })).await;
    let host_id = Uuid::parse_str(data["hostId"].as_str().unwrap()).unwrap();
    assert_eq!(data["status"], "provisioning");

    let mock_for_wait = mock.clone();
    wait_for(
        || {
            let m = mock_for_wait.clone();
            async move {
                m.calls()
                    .iter()
                    .any(|c| matches!(c, MockCall::ProvisionInstance { .. }))
            }
        },
        "spawned bring-up reached provision_instance",
    )
    .await;

    let row = sqlx::query_as::<_, (String, Option<String>, Option<String>)>(
        "SELECT status, vpc_id, cloud_instance_id FROM worker_hosts WHERE host_id = $1",
    )
    .bind(host_id)
    .fetch_one(&app.db)
    .await
    .expect("row exists despite cloud failure");
    assert_eq!(row.0, "provisioning");
    assert!(row.1.is_none(), "vpc_id NULL after provision failure");
    assert!(
        row.2.is_none(),
        "cloud_instance_id NULL after provision failure"
    );

    app.cleanup().await;
}

#[tokio::test]
async fn deallocate_triggers_terminate_instance() {
    let (app, mock) = spawn_with_mock().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    let data = provision(&client, org.id, json!({ "region": "us-central1" })).await;
    let host_id = data["hostId"].as_str().unwrap().to_string();
    // Round-3 async-spawn: wait for the detached bring-up task to land its
    // `cloud_instance_id` UPDATE before DELETE-ing, otherwise DELETE may see
    // a NULL cloud_instance_id and skip the terminate_instance call.
    let cloud_instance_id =
        wait_for_cloud_instance_id(&app, Uuid::parse_str(&host_id).unwrap()).await;

    let resp = client
        .delete(&format!("/api/v1/orgs/{}/worker-hosts/{}", org.id, host_id))
        .await;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    assert!(
        mock.calls().contains(&MockCall::TerminateInstance {
            cloud_instance_id: cloud_instance_id.clone()
        }),
        "terminate_instance called with the row's cloud_instance_id"
    );

    app.cleanup().await;
}

#[tokio::test]
async fn deallocate_succeeds_even_if_terminate_fails() {
    // terminate-best-effort ([D23]): a GCP teardown failure must not fail the
    // DELETE — the row is already terminated from the user's perspective.
    let (app, mock) = spawn_with_mock().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    let data = provision(&client, org.id, json!({ "region": "us-central1" })).await;
    let host_id = data["hostId"].as_str().unwrap().to_string();
    // Wait for the detached bring-up before deleting (round-3 async-spawn).
    let _ = wait_for_cloud_instance_id(&app, Uuid::parse_str(&host_id).unwrap()).await;

    mock.fail_terminate(true);
    let resp = client
        .delete(&format!("/api/v1/orgs/{}/worker-hosts/{}", org.id, host_id))
        .await;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT, "DELETE still 204");

    let status =
        sqlx::query_scalar::<_, String>("SELECT status FROM worker_hosts WHERE host_id = $1")
            .bind(Uuid::parse_str(&host_id).unwrap())
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(
        status, "terminated",
        "row terminated despite teardown failure"
    );

    app.cleanup().await;
}

#[tokio::test]
async fn reconciler_marks_ready_only_after_running_heartbeat_and_reported_sizes() {
    let (app, mock) = spawn_with_mock().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    let data = provision(&client, org.id, json!({ "region": "us-central1" })).await;
    let host_id = Uuid::parse_str(data["hostId"].as_str().unwrap()).unwrap();
    // Round-3 async-spawn: wait for detached bring-up to land.
    let instance_id = wait_for_cloud_instance_id(&app, host_id).await;

    // Cloud says Running, but no heartbeat yet → stays provisioning.
    mock.set_status(&instance_id, InstanceState::Running);
    let pass = g5_api::cloud::reconciler::reconcile_once(app.state.as_ref()).await;
    assert_eq!(pass.checked, 1);
    assert_eq!(pass.marked_ready, 0, "no heartbeat → not ready yet");
    let status = host_status(&app, host_id).await;
    assert_eq!(status, "provisioning");

    // Record a heartbeat and plant a stale [D61] gone window (as if a prior
    // pass observed the instance gone), then reconcile → ready AND the
    // window is cleared: the ready arm is an alive direct observation too.
    sqlx::query(
        "UPDATE worker_hosts SET last_heartbeat = now(), cloud_gone_since = now() \
         WHERE host_id = $1",
    )
    .bind(host_id)
    .execute(&app.db)
    .await
    .unwrap();
    let pass = g5_api::cloud::reconciler::reconcile_once(app.state.as_ref()).await;
    assert_eq!(
        pass.marked_ready, 0,
        "heartbeat without sizing cannot qualify readiness"
    );
    assert_eq!(host_status(&app, host_id).await, "provisioning");
    use g5_api::ws::host_ctl::{ControlPlaneHostState, HostConnection};
    let backend = app.host_control_backend();
    backend
        .accept_hello(
            host_id,
            &g5_internal_api::host_ctl::HostHello {
                host_id,
                protocol_version: g5_internal_api::host_ctl::PROTOCOL_VERSION,
                capabilities: Vec::new(),
                slots_total: 1,
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
    // Exercise the cloud transition with an already-published contract.
    sqlx::query(
        "UPDATE worker_hosts SET status='provisioning', cloud_gone_since=now() WHERE host_id=$1",
    )
    .bind(host_id)
    .execute(&app.db)
    .await
    .unwrap();
    let pass = g5_api::cloud::reconciler::reconcile_once(app.state.as_ref()).await;
    assert_eq!(
        pass.marked_ready, 1,
        "Running + heartbeat + publication → ready"
    );
    assert_eq!(host_status(&app, host_id).await, "ready");
    let gone_after: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT cloud_gone_since FROM worker_hosts WHERE host_id = $1")
            .bind(host_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(
        gone_after, None,
        "alive observation on the ready arm clears a pending gone window"
    );

    app.cleanup().await;
}

#[tokio::test]
async fn reconciler_terminates_only_after_wall_clock_gone_window() {
    // Ontology [D61] (issue #1512): a Terminated/404 observation is NOT a
    // one-shot kill — during a [D59] stockout fallback the linked failed
    // attempt reads as gone while the sibling-zone insert is in flight. The
    // row terminates only after GONE_CONFIRMATION_SECS of wall-clock
    // sustained gone-ness, which also makes the window replica-count
    // independent: ANY number of rapid passes inside the window must not
    // terminate.
    let (app, mock) = spawn_with_mock().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    let data = provision(&client, org.id, json!({ "region": "us-central1" })).await;
    let host_id = Uuid::parse_str(data["hostId"].as_str().unwrap()).unwrap();
    // Round-3 async-spawn: wait for detached bring-up to land.
    let instance_id = wait_for_cloud_instance_id(&app, host_id).await;

    // Direct GET says gone; no by-name entry programmed → re-search confirms
    // gone everywhere on every pass.
    mock.set_status(&instance_id, InstanceState::Terminated);

    // Many rapid passes (simulating several replicas ticking at once): the
    // window is stamped once but never lapses inside the burst.
    for burst_pass in 1..=5 {
        let pass = g5_api::cloud::reconciler::reconcile_once(app.state.as_ref()).await;
        assert_eq!(pass.marked_terminated, 0, "inside window → no kill");
        assert_eq!(pass.gone_pending, 1, "pass {burst_pass} observes gone");
        assert_eq!(host_status(&app, host_id).await, "provisioning");
    }
    let gone_since = sqlx::query_scalar::<_, Option<chrono::DateTime<chrono::Utc>>>(
        "SELECT cloud_gone_since FROM worker_hosts WHERE host_id = $1",
    )
    .bind(host_id)
    .fetch_one(&app.db)
    .await
    .unwrap()
    .expect("window stamped");

    // Backdate the window past GONE_CONFIRMATION_SECS → the next pass
    // terminates atomically.
    sqlx::query(
        "UPDATE worker_hosts SET cloud_gone_since = now() - interval '120 seconds' \
         WHERE host_id = $1",
    )
    .bind(host_id)
    .execute(&app.db)
    .await
    .unwrap();
    let pass = g5_api::cloud::reconciler::reconcile_once(app.state.as_ref()).await;
    assert_eq!(pass.marked_terminated, 1, "lapsed window → terminated");
    assert_eq!(host_status(&app, host_id).await, "terminated");
    // The COALESCE in mark_worker_host_cloud_gone must have preserved the
    // earliest observation (our backdate), not re-stamped it — the terminate
    // CAS keeps the column, so re-read and compare against the burst stamp.
    let final_gone = sqlx::query_scalar::<_, Option<chrono::DateTime<chrono::Utc>>>(
        "SELECT cloud_gone_since FROM worker_hosts WHERE host_id = $1",
    )
    .bind(host_id)
    .fetch_one(&app.db)
    .await
    .unwrap()
    .expect("window retained through terminate");
    assert!(
        final_gone < gone_since,
        "COALESCE preserved the backdated stamp ({final_gone}) over the \
         original burst stamp ({gone_since})"
    );

    app.cleanup().await;
}

#[tokio::test]
async fn reconciler_repoints_linkage_when_instance_found_in_other_zone() {
    // Ontology [D61] (issue #1512, RED for the race): the linked primary-zone
    // attempt is gone, but the by-name re-search finds the instance alive in a
    // sibling zone (the [D59] fallback placed it there). The row must be
    // REPOINTED, not terminated — pre-[D61] this was the one-shot kill that
    // leaked the fallback VM on g5-deploy-dev build fa1034a2.
    let (app, mock) = spawn_with_mock().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    let data = provision(&client, org.id, json!({ "region": "us-central1" })).await;
    let host_id = Uuid::parse_str(data["hostId"].as_str().unwrap()).unwrap();
    let _ = wait_for_cloud_instance_id(&app, host_id).await;

    // Re-link the row to a realistic `{zone}/{name}` encoding pointing at the
    // failed primary-zone attempt (the mock's own linkage is a bare
    // `mock-instance-N`, which never exercises the re-search path). Also
    // pre-stamp a gone window to prove the repoint clears it.
    let deterministic_name = g5_api::cloud::host_instance_name(host_id, &org.slug);
    let stale_linkage = format!("us-central1-b/{deterministic_name}");
    sqlx::query(
        "UPDATE worker_hosts SET cloud_instance_id = $2, \
         cloud_gone_since = now() - interval '30 seconds' WHERE host_id = $1",
    )
    .bind(host_id)
    .bind(&stale_linkage)
    .execute(&app.db)
    .await
    .unwrap();

    // Direct GET on the stale linkage: gone. By-name search: alive in zone c.
    mock.set_status(&stale_linkage, InstanceState::Terminated);
    mock.set_instance_by_name_in_zone(
        "us-central1",
        &deterministic_name,
        InstanceState::Provisioning,
        "us-central1-c",
    );

    let pass = g5_api::cloud::reconciler::reconcile_once(app.state.as_ref()).await;
    assert_eq!(
        pass.marked_terminated, 0,
        "found elsewhere → NOT terminated"
    );
    assert_eq!(pass.repointed, 1, "linkage repointed to the found zone");
    assert_eq!(host_status(&app, host_id).await, "provisioning");

    let (linkage, gone_since) =
        sqlx::query_as::<_, (Option<String>, Option<chrono::DateTime<chrono::Utc>>)>(
            "SELECT cloud_instance_id, cloud_gone_since FROM worker_hosts WHERE host_id = $1",
        )
        .bind(host_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(
        linkage.as_deref(),
        Some(format!("us-central1-c/{deterministic_name}").as_str()),
        "linkage encodes the zone the instance was FOUND in"
    );
    assert!(gone_since.is_none(), "repoint clears the gone window");

    app.cleanup().await;
}

#[tokio::test]
async fn reconciler_gone_window_resets_on_alive_observation() {
    // Ontology [D61]: the gone window must be CONTINUOUS — an alive
    // observation clears it, so flapping reads cannot accumulate into a kill.
    let (app, mock) = spawn_with_mock().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    let data = provision(&client, org.id, json!({ "region": "us-central1" })).await;
    let host_id = Uuid::parse_str(data["hostId"].as_str().unwrap()).unwrap();
    let instance_id = wait_for_cloud_instance_id(&app, host_id).await;

    // A gone observation opens the window…
    mock.set_status(&instance_id, InstanceState::Terminated);
    let pass = g5_api::cloud::reconciler::reconcile_once(app.state.as_ref()).await;
    assert_eq!(pass.gone_pending, 1);

    // …an alive observation (Running, no heartbeat → stays provisioning)
    // clears it…
    mock.set_status(&instance_id, InstanceState::Running);
    let pass = g5_api::cloud::reconciler::reconcile_once(app.state.as_ref()).await;
    assert_eq!(pass.marked_ready, 0, "no heartbeat → not ready");
    let gone_since = sqlx::query_scalar::<_, Option<chrono::DateTime<chrono::Utc>>>(
        "SELECT cloud_gone_since FROM worker_hosts WHERE host_id = $1",
    )
    .bind(host_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(gone_since.is_none(), "alive observation clears the window");

    // …so a fresh gone observation restarts the window from now() and an
    // immediate pass cannot terminate.
    mock.set_status(&instance_id, InstanceState::Terminated);
    let pass = g5_api::cloud::reconciler::reconcile_once(app.state.as_ref()).await;
    assert_eq!(pass.marked_terminated, 0, "restarted window → no kill");
    assert_eq!(host_status(&app, host_id).await, "provisioning");

    app.cleanup().await;
}

#[tokio::test]
async fn terminate_gone_cas_noops_on_repointed_linkage() {
    // Ontology [D61] (Codex design-review blocker 2): the terminate is a
    // single atomic CAS on status + EXPECTED linkage + lapsed window. If
    // another replica repointed the row between the gone observation and the
    // terminate, the stale-linkage terminate must be a no-op even though the
    // window has lapsed.
    let (app, _mock) = spawn_with_mock().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    let data = provision(&client, org.id, json!({ "region": "us-central1" })).await;
    let host_id = Uuid::parse_str(data["hostId"].as_str().unwrap()).unwrap();
    let _ = wait_for_cloud_instance_id(&app, host_id).await;

    // Lapsed window under linkage A…
    sqlx::query(
        "UPDATE worker_hosts SET cloud_instance_id = 'us-central1-b/x', \
         cloud_gone_since = now() - interval '600 seconds' WHERE host_id = $1",
    )
    .bind(host_id)
    .execute(&app.db)
    .await
    .unwrap();
    // …but a concurrent repoint moved the row to linkage B (and cleared the
    // window; re-lapse it manually to isolate the linkage CAS itself).
    sqlx::query("UPDATE worker_hosts SET cloud_instance_id = 'us-central1-c/x' WHERE host_id = $1")
        .bind(host_id)
        .execute(&app.db)
        .await
        .unwrap();

    let terminated = sqlx::query_scalar::<_, Option<chrono::DateTime<chrono::Utc>>>(
        "SELECT authz.terminate_gone_worker_host($1, $2, $3)",
    )
    .bind(host_id)
    .bind("us-central1-b/x") // the STALE expectation
    .bind(g5_api::cloud::reconciler::GONE_CONFIRMATION_SECS)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(terminated.is_none(), "stale-linkage terminate is a no-op");
    assert_eq!(host_status(&app, host_id).await, "provisioning");

    app.cleanup().await;
}

async fn host_status(app: &TestApp, host_id: Uuid) -> String {
    sqlx::query_scalar::<_, String>("SELECT status FROM worker_hosts WHERE host_id = $1")
        .bind(host_id)
        .fetch_one(&app.db)
        .await
        .unwrap()
}

#[tokio::test]
async fn no_cloud_provider_preserves_9b_db_only_contract() {
    // [I6]/[D25]: when no provider is wired, POST must reproduce 9b's behavior
    // exactly — 201, NULL cloud columns, and no MockCall recorded. This is the
    // explicit unchanged-behavior assertion the rev-7 §5.7 acceptance gate
    // lists for the test view.
    let app = TestApp::spawn().await; // no cloud wired → cloud_provider == None
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    let data = provision(&client, org.id, json!({ "region": "us-central1" })).await;
    let host_id = Uuid::parse_str(data["hostId"].as_str().unwrap()).unwrap();
    assert_eq!(data["status"], "provisioning");

    // The cloud half is wired off → the row stays as 9b left it.
    let row = sqlx::query_as::<_, (Option<String>, Option<String>)>(
        "SELECT vpc_id, cloud_instance_id FROM worker_hosts WHERE host_id = $1",
    )
    .bind(host_id)
    .fetch_one(&app.db)
    .await
    .expect("row exists in DB-only mode");
    assert!(row.0.is_none(), "vpc_id NULL when no cloud provider");
    assert!(
        row.1.is_none(),
        "cloud_instance_id NULL when no cloud provider"
    );

    app.cleanup().await;
}

#[tokio::test]
async fn instance_type_outside_allowlist_is_rejected() {
    // The handler interpolates `instance_type` into the GCP machine-type URL
    // segment; the allowlist closes both cost (admin can't ask for a 96-core
    // VM) and path-injection (`n2-standard-8/global/networks/x`) vectors.
    let (app, mock) = spawn_with_mock().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    let resp = client
        .post_json(
            &format!("/api/v1/orgs/{}/worker-hosts", org.id),
            &json!({ "region": "us-central1", "instanceType": "n2-standard-8/extra/segment" }),
        )
        .await;
    assert_eq!(
        resp.status(),
        StatusCode::UNPROCESSABLE_ENTITY,
        "off-allowlist machine type rejected at boundary"
    );

    // And the cloud provider was never invoked.
    assert!(
        !mock
            .calls()
            .iter()
            .any(|c| matches!(c, MockCall::ProvisionInstance { .. })),
        "rejected request must not have reached the cloud provider"
    );

    app.cleanup().await;
}

#[tokio::test]
async fn region_outside_allowlist_is_rejected() {
    // Round-2 codex HIGH + opus48 B-1: the region value flows into
    // `format!`-built GCP REST URL segments (`regions/{region}/...`,
    // `zones/{region}-{suffix}/...`), so we must reject off-allowlist
    // values at the boundary — same vector the `instance_type` allowlist
    // already closes. A successful 422 also proves the provider was never
    // invoked.
    let (app, mock) = spawn_with_mock().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    // Path-injection shape that GCP itself would have rejected, but the
    // host record would already exist by then. We fail closed first.
    let resp = client
        .post_json(
            &format!("/api/v1/orgs/{}/worker-hosts", org.id),
            &json!({ "region": "us-central1/../../global", "instanceType": "n2-standard-8" }),
        )
        .await;
    assert_eq!(
        resp.status(),
        StatusCode::UNPROCESSABLE_ENTITY,
        "off-allowlist region rejected at boundary"
    );

    // A plausibly-named but unsupported region also fails closed.
    let resp = client
        .post_json(
            &format!("/api/v1/orgs/{}/worker-hosts", org.id),
            &json!({ "region": "mars-northwest1" }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);

    assert!(
        !mock.calls().iter().any(|c| matches!(
            c,
            MockCall::EnsureVpc { .. } | MockCall::ProvisionInstance { .. }
        )),
        "rejected request must not have reached the cloud provider; calls={:?}",
        mock.calls()
    );

    app.cleanup().await;
}

#[tokio::test]
async fn cloud_features_enabled_without_provider_returns_503() {
    // Round-2 codex HIGH: when an operator enables cloud features but no
    // provider resolves (creds/project not configured), a silent 201
    // DB-only response misleads operators into thinking a VM is being
    // brought up. Fail closed with 503 instead — preserving the 9b
    // regression fence only when the flag is OFF.
    let mut config = helpers::test_config();
    config.enable_cloud_features = true;
    let app = TestApp::spawn_with_config_and_cloud(config, None).await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    let resp = client
        .post_json(
            &format!("/api/v1/orgs/{}/worker-hosts", org.id),
            &json!({
                "region": "us-central1",
                "baseImageRef": helpers::TEST_WORKER_IMAGE_REF,
            }),
        )
        .await;
    assert_eq!(
        resp.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "enabled-but-unconfigured cloud → 503, not silent 201"
    );

    let body: Value = resp.json().await.expect("503 body is JSON");
    let message = body
        .get("error")
        .and_then(|e| e.get("message"))
        .and_then(|m| m.as_str())
        .unwrap_or_default();
    assert!(
        message.contains("cloud_features_enabled"),
        "operator-facing message mentions the misconfiguration; got: {message:?}"
    );

    app.cleanup().await;
}

#[tokio::test]
async fn delete_with_cloud_features_disabled_skips_teardown_and_logs_orphan() {
    // Round-2 codex important + opus47 important: DELETE must honor the
    // same `cloud_features_enabled` gate as POST. We exercise the
    // gate-mid-flight scenario: a row has a cloud_instance_id (set out of
    // band, simulating a host provisioned while the flag was on) but the
    // current process has the flag off. DELETE marks the row terminated,
    // skips the cloud call entirely, and the audit event is the only
    // signal an operator gets that a GCE instance may still be alive.
    let mock = Arc::new(MockCloudProvider::new());
    let mut config = helpers::test_config();
    config.enable_cloud_features = false; // flag off
    let app =
        TestApp::spawn_with_config_and_cloud(config, Some(mock.clone() as Arc<dyn CloudProvider>))
            .await;

    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    // Provision via API (flag off → no cloud call), then back-fill a
    // cloud_instance_id directly to simulate a row created while the flag
    // was on.
    let data = provision(&client, org.id, json!({ "region": "us-central1" })).await;
    let host_id = Uuid::parse_str(data["hostId"].as_str().unwrap()).unwrap();
    assert!(
        mock.calls().is_empty(),
        "POST must not call cloud when flag off"
    );

    sqlx::query("UPDATE worker_hosts SET cloud_instance_id = $1, vpc_id = $2 WHERE host_id = $3")
        .bind("us-central1-a/g5-host-orphan")
        .bind(format!("g5-org-{}", org.id))
        .bind(host_id)
        .execute(&app.db)
        .await
        .expect("backfill simulates pre-flip cloud row");

    let resp = client
        .delete(&format!("/api/v1/orgs/{}/worker-hosts/{}", org.id, host_id))
        .await;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // The cloud call must NOT have been issued (flag is off).
    assert!(
        !mock
            .calls()
            .iter()
            .any(|c| matches!(c, MockCall::TerminateInstance { .. })),
        "DELETE must not call terminate_instance when cloud_features_enabled=false"
    );

    // The row still got terminated locally.
    let status =
        sqlx::query_scalar::<_, String>("SELECT status FROM worker_hosts WHERE host_id = $1")
            .bind(host_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(status, "terminated");

    app.cleanup().await;
}

#[tokio::test]
async fn cloud_features_disabled_skips_bringup_even_with_provider() {
    // [D24]: an operator must be able to flip cloud bring-up off without
    // restarting / changing creds. Wire a mock provider but leave the
    // deployment flag at its default `false`; POST should still 201, but no
    // MockCall is recorded and the cloud columns stay NULL — same observable
    // shape as the no-provider case.
    let mock = Arc::new(MockCloudProvider::new());
    let mut config = helpers::test_config();
    config.enable_cloud_features = false; // explicit for clarity
    let app =
        TestApp::spawn_with_config_and_cloud(config, Some(mock.clone() as Arc<dyn CloudProvider>))
            .await;

    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    let data = provision(&client, org.id, json!({ "region": "us-central1" })).await;
    let host_id = Uuid::parse_str(data["hostId"].as_str().unwrap()).unwrap();

    assert!(
        mock.calls().is_empty(),
        "gate off → cloud provider must not be invoked; calls={:?}",
        mock.calls()
    );

    let row = sqlx::query_as::<_, (Option<String>, Option<String>)>(
        "SELECT vpc_id, cloud_instance_id FROM worker_hosts WHERE host_id = $1",
    )
    .bind(host_id)
    .fetch_one(&app.db)
    .await
    .expect("row exists when feature flag off");
    assert!(row.0.is_none(), "vpc_id NULL when feature flag off");
    assert!(
        row.1.is_none(),
        "cloud_instance_id NULL when feature flag off"
    );

    app.cleanup().await;
}

// ─── Round-3 async-spawn + orphan-reconciler (ontology [D22]/[D39], chat C2) ─

#[tokio::test]
async fn provision_failure_classifies_provision_error_under_detached_topology() {
    // Acceptance D40: with the bring-up detached via `tokio::spawn`, an HTTP
    // client that disconnects immediately after receiving the 201 must NOT
    // skip the D34 classifier write. We inject a MockCloudProvider whose
    // `provision_instance` returns the canonical GCP ZONE_RESOURCE_POOL_EXHAUSTED
    // message; after the 201 returns the detached task observes the failure,
    // runs `log_cloud_bringup_error` + `persist_provision_error_if_classified`,
    // and writes `provision_error_code='zone_capacity_exhausted'` to the row.
    // The test polls the row for that code — the round-2 design lost this
    // write entirely whenever the LB cancelled, which is what this test pins.
    use g5_api::cloud::CloudError;
    use std::sync::atomic::{AtomicBool, Ordering};

    // Custom mock that fails provision with a classifiable message. The
    // standard MockCloudProvider's `fail_provision` returns a generic
    // status:500 body that classify_provision_error returns None for, so we
    // need a purpose-built mock for this assertion.
    struct ZoneExhaustedMock {
        image: String,
        recorded_provision: AtomicBool,
    }
    #[async_trait::async_trait]
    impl CloudProvider for ZoneExhaustedMock {
        async fn ensure_vpc_for_org(
            &self,
            org_id: uuid::Uuid,
        ) -> Result<g5_api::cloud::VpcRef, CloudError> {
            Ok(g5_api::cloud::VpcRef {
                vpc_id: format!("g5-org-{org_id}"),
                region: "mock-region".to_string(),
                network_self_link: "mock://net".to_string(),
                subnetwork_self_link: Some("mock://subnet".to_string()),
            })
        }
        async fn provision_instance(
            &self,
            _host_id: uuid::Uuid,
            _org_slug: &str,
            _created_by: Option<&g5_api::cloud::ProvisioningActor>,
            _vpc_ref: &g5_api::cloud::VpcRef,
            _instance_type: &str,
            _region: &str,
            _image: &str,
            _startup_script: &str,
            _boot_disk_gb: u32,
        ) -> Result<g5_api::cloud::CloudInstanceRef, CloudError> {
            self.recorded_provision.store(true, Ordering::SeqCst);
            Err(CloudError::Api {
                status: 503,
                message: "The zone us-central1-a does not have enough resources \
                              available to fulfill the request."
                    .into(),
            })
        }
        async fn terminate_instance(&self, _cloud_instance_id: &str) -> Result<(), CloudError> {
            Ok(())
        }
        async fn get_instance_status(
            &self,
            _cloud_instance_id: &str,
        ) -> Result<InstanceState, CloudError> {
            Ok(InstanceState::Unknown)
        }
        async fn get_instance_by_name(
            &self,
            _region: &str,
            _name: &str,
        ) -> Result<Option<g5_api::cloud::FoundInstance>, CloudError> {
            Ok(None)
        }
        async fn list_instances_by_prefix(
            &self,
            _region: &str,
            _prefix: &str,
        ) -> Result<Vec<g5_api::cloud::ListedInstance>, CloudError> {
            Ok(Vec::new())
        }
        fn worker_image(&self) -> &str {
            &self.image
        }
        async fn resolve_boot_image(&self) -> Result<g5_api::cloud::BootImageIdentity, CloudError> {
            // [D56]: resolve the configured family to a concrete image. This
            // mock's assertion is about the provision failure below, so boot
            // resolution just succeeds plausibly and stays out of the way.
            Ok(g5_api::cloud::BootImageIdentity {
                image_id: "g5-worker-mock-v1".to_string(),
                self_link: "projects/g5-test/global/images/g5-worker-mock-v1".to_string(),
            })
        }
    }

    let mock = Arc::new(ZoneExhaustedMock {
        image: "projects/g5-test/global/images/family/g5-worker".to_string(),
        recorded_provision: AtomicBool::new(false),
    });
    let mock_dyn: Arc<dyn CloudProvider> = mock.clone();
    let app = TestApp::spawn_with_cloud(mock_dyn).await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    // POST returns 201 immediately (the round-3 async-spawn contract); the
    // bring-up will fail in a detached task.
    let data = provision(&client, org.id, json!({ "region": "us-central1" })).await;
    let host_id = Uuid::parse_str(data["hostId"].as_str().unwrap()).unwrap();

    // The classifier write lands post-response, asynchronously. Poll the row
    // until `provision_error_code` becomes the classified value — this proves
    // (1) the spawned task survived past the simulated HTTP-client drop, and
    // (2) `persist_provision_error_if_classified` was reachable on the failure
    // arm even with no awaiting handler. The round-2 design lost both
    // properties whenever the LB cancellation hit.
    wait_for(
        || {
            let pool = app.db.clone();
            async move {
                let code: Option<String> = sqlx::query_scalar::<_, Option<String>>(
                    "SELECT provision_error_code FROM worker_hosts WHERE host_id = $1",
                )
                .bind(host_id)
                .fetch_one(&pool)
                .await
                .expect("row exists");
                code.as_deref() == Some("zone_capacity_exhausted")
            }
        },
        "spawned bring-up classified zone_capacity_exhausted",
    )
    .await;

    // Defense in depth: the row's cloud_instance_id must STILL be NULL
    // (provision failed; vpc_id is also NULL because ensure_vpc succeeded but
    // the step-3 persist never ran).
    let row = sqlx::query_as::<_, (String, Option<String>, Option<String>, Option<String>)>(
        "SELECT status, vpc_id, cloud_instance_id, provision_error_code \
         FROM worker_hosts WHERE host_id = $1",
    )
    .bind(host_id)
    .fetch_one(&app.db)
    .await
    .expect("row exists");
    assert_eq!(row.0, "provisioning");
    assert!(row.2.is_none(), "cloud_instance_id NULL on failed bring-up");
    assert_eq!(row.3.as_deref(), Some("zone_capacity_exhausted"));
    assert!(
        mock.recorded_provision.load(Ordering::SeqCst),
        "spawned task did reach provision_instance"
    );

    app.cleanup().await;
}

#[tokio::test]
async fn reconciler_attaches_orphan_when_instance_alive() {
    // Acceptance D40 / ontology [D39]: a row whose POST handler failed to
    // persist `cloud_instance_id` (the load-bearing failure mode the round-2
    // bug introduced) MUST be re-attached by the orphan-reconciler when the
    // deterministic GCE instance is found alive. After re-attach, the
    // standard reconciler pass on the next tick takes over and (with
    // heartbeat present) promotes the row to `ready`.
    let (app, mock) = spawn_with_mock().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    // Provision via API but back-fill cloud_instance_id = NULL to simulate
    // the bug: the POST handler ran, the GCE instance is alive (we'll tell
    // the mock about it below), but cloud_instance_id never landed. Also
    // age the row past the 60s grace window so the orphan-list SELECT
    // includes it.
    let data = provision(&client, org.id, json!({ "region": "us-central1" })).await;
    let host_id = Uuid::parse_str(data["hostId"].as_str().unwrap()).unwrap();
    // Wait for the (in-process, sub-ms) detached bring-up to land first, then
    // null it back out — keeps the test independent of the spawn race.
    let _ = wait_for_cloud_instance_id(&app, host_id).await;
    sqlx::query(
        "UPDATE worker_hosts SET cloud_instance_id = NULL, vpc_id = NULL, \
         bringup_heartbeat_at = NULL, \
         created_at = now() - interval '120 seconds' WHERE host_id = $1",
    )
    .bind(host_id)
    .execute(&app.db)
    .await
    .expect("simulate orphaned row past grace window");

    // Program the mock to report the instance alive under the deterministic
    // slug-bearing name ([D44]) — the same helper provisioning uses.
    let deterministic_name = g5_api::cloud::host_instance_name(host_id, &org.slug);
    mock.set_instance_by_name("us-central1", &deterministic_name, InstanceState::Running);

    let pass = g5_api::cloud::reconciler::reconcile_orphans_once(app.state.as_ref()).await;
    assert_eq!(pass.checked, 1, "orphan SELECT found the back-filled row");
    assert_eq!(pass.reattached, 1, "found-alive → reattach");
    assert_eq!(pass.errors, 0);

    // The row now has a non-NULL cloud_instance_id matching the
    // deterministic encoding (`{zone}/{host_instance_name}`, [D44]), and on the next
    // standard pass with the mock reporting Running + a heartbeat it
    // would promote to ready.
    let row = sqlx::query_as::<_, (Option<String>, Option<String>)>(
        "SELECT vpc_id, cloud_instance_id FROM worker_hosts WHERE host_id = $1",
    )
    .bind(host_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(
        row.0.as_deref(),
        Some(format!("g5-org-{}", org.id).as_str())
    );
    let cloud_instance_id = row.1.expect("cloud_instance_id attached");
    assert!(cloud_instance_id.ends_with(&deterministic_name));

    app.cleanup().await;
}

#[tokio::test]
#[serial_test::serial]
async fn reconciler_repairs_ready_orphan_and_allows_rotation() {
    let _env = EnvVarGuard::set_many(&[
        (g5_api::cloud::rotation::ROTATION_ENABLED_ENV, "true"),
        (g5_api::cloud::rotation::ROTATION_MIN_IDLE_ENV, "300"),
    ]);
    let (app, mock) = spawn_with_mock().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);
    let data = provision(&client, org.id, json!({ "region": "us-central1" })).await;
    let host_id = Uuid::parse_str(data["hostId"].as_str().unwrap()).unwrap();
    let _ = wait_for_cloud_instance_id(&app, host_id).await;
    sqlx::query(
        "UPDATE worker_hosts SET cloud_instance_id = NULL, vpc_id = NULL, \
         boot_image_id = NULL, bringup_heartbeat_at = now(), \
         created_at = now() - interval '10 minutes', \
         last_active_at = now() - interval '10 minutes' WHERE host_id = $1",
    )
    .bind(host_id)
    .execute(&app.db)
    .await
    .unwrap();

    // Reproduce #400: registration wins the race with orphan repair.
    accept_host_hello(&app, host_id).await;
    assert_eq!(host_status(&app, host_id).await, "ready");
    mock.set_boot_image("g5-worker-newer-image");
    let before = g5_api::cloud::rotation::rotate_once(&app.state).await;
    assert_eq!(before.paired, 0, "unlinked hosts cannot be rotated");
    assert_eq!(before.errors, 0);

    let name = g5_api::cloud::host_instance_name(host_id, &org.slug);
    mock.set_instance_by_name_in_zone(
        "us-central1",
        &name,
        InstanceState::Running,
        "us-central1-f",
    );
    let fresh = g5_api::cloud::reconciler::reconcile_orphans_once(&app.state).await;
    assert_eq!(
        fresh.checked, 0,
        "ready hosts still honor the bring-up lease"
    );
    let attach: Option<Uuid> =
        sqlx::query_scalar("SELECT authz.attach_orphan_worker_host_cloud_refs($1, $2, $3)")
            .bind(host_id)
            .bind(format!("g5-org-{}", org.id))
            .bind(format!("us-central1-f/{name}"))
            .fetch_one(&app.state.system_pool)
            .await
            .unwrap();
    assert_eq!(attach, None, "the attach itself rechecks the lease");
    sqlx::query(
        "UPDATE worker_hosts SET bringup_heartbeat_at = now() - interval '120 seconds' \
         WHERE host_id = $1",
    )
    .bind(host_id)
    .execute(&app.db)
    .await
    .unwrap();
    let pass = g5_api::cloud::reconciler::reconcile_orphans_once(&app.state).await;
    assert_eq!(pass.checked, 1);
    assert_eq!(pass.reattached, 1);
    assert_eq!(pass.errors, 0);
    let row: (String, String, Option<String>) = sqlx::query_as(
        "SELECT vpc_id, cloud_instance_id, boot_image_id FROM worker_hosts WHERE host_id = $1",
    )
    .bind(host_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(row.0, format!("g5-org-{}", org.id));
    assert_eq!(row.1, format!("us-central1-f/{name}"));
    assert_eq!(row.2, None, "repair does not invent the old boot image");
    assert_eq!(host_status(&app, host_id).await, "ready");
    let standard = g5_api::cloud::reconciler::reconcile_once(&app.state).await;
    assert_eq!(
        standard.checked, 0,
        "ready host stays out of the standard pass"
    );
    assert_eq!(
        g5_api::cloud::reconciler::reconcile_orphans_once(&app.state)
            .await
            .checked,
        0,
        "repair is idempotent"
    );

    let rotation = g5_api::cloud::rotation::rotate_once(&app.state).await;
    assert_eq!(
        rotation.paired, 1,
        "repaired host can rotate to the desired image"
    );
    assert_eq!(rotation.errors, 0);
    let replacement: Uuid =
        sqlx::query_scalar("SELECT rotation_replaced_by FROM worker_hosts WHERE host_id = $1")
            .bind(host_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    let _ = wait_for_cloud_instance_id(&app, replacement).await;
    app.cleanup().await;
}

#[tokio::test]
async fn reconciler_repairs_draining_orphan_but_never_reattaches_terminated_host() {
    let (app, mock) = spawn_with_mock().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);
    let data = provision(&client, org.id, json!({ "region": "us-central1" })).await;
    let host_id = Uuid::parse_str(data["hostId"].as_str().unwrap()).unwrap();
    let _ = wait_for_cloud_instance_id(&app, host_id).await;
    sqlx::query(
        "UPDATE worker_hosts SET status = 'draining', cloud_instance_id = NULL, \
         vpc_id = NULL, bringup_heartbeat_at = NULL, \
         created_at = now() - interval '120 seconds' WHERE host_id = $1",
    )
    .bind(host_id)
    .execute(&app.db)
    .await
    .unwrap();
    let name = g5_api::cloud::host_instance_name(host_id, &org.slug);
    mock.set_instance_by_name("us-central1", &name, InstanceState::Running);
    let pass = g5_api::cloud::reconciler::reconcile_orphans_once(&app.state).await;
    assert_eq!(
        pass.reattached, 1,
        "a draining host also needs linkage for teardown"
    );
    assert_eq!(pass.errors, 0);
    assert_eq!(host_status(&app, host_id).await, "draining");

    sqlx::query(
        "UPDATE worker_hosts SET status = 'terminated', cloud_instance_id = NULL, \
         vpc_id = NULL WHERE host_id = $1",
    )
    .bind(host_id)
    .execute(&app.db)
    .await
    .unwrap();
    let pass = g5_api::cloud::reconciler::reconcile_orphans_once(&app.state).await;
    assert_eq!(pass.checked, 0, "terminated rows are excluded from repair");
    // Simulate a deletion that raced an earlier candidate read.
    let attached: Option<Uuid> =
        sqlx::query_scalar("SELECT authz.attach_orphan_worker_host_cloud_refs($1, $2, $3)")
            .bind(host_id)
            .bind(format!("g5-org-{}", org.id))
            .bind(format!("us-central1-a/{name}"))
            .fetch_one(&app.state.system_pool)
            .await
            .unwrap();
    assert_eq!(
        attached, None,
        "a raced deletion is fenced inside the attach"
    );
    app.cleanup().await;
}

#[tokio::test]
async fn orphan_pass_skips_row_with_fresh_bringup_heartbeat() {
    // Ontology [D60] (issue #1512, RED for the race): a row whose bring-up
    // task is alive (fresh `bringup_heartbeat_at`) is IN FLIGHT, not orphaned
    // — the orphan pass must not attach it, however old it is. Pre-[D60] the
    // 60s age grace was the only gate, so a slow stockout+fallback provision
    // got a premature (wrong-zone) linkage attached mid-provision.
    let (app, mock) = spawn_with_mock().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    let data = provision(&client, org.id, json!({ "region": "us-central1" })).await;
    let host_id = Uuid::parse_str(data["hostId"].as_str().unwrap()).unwrap();
    let _ = wait_for_cloud_instance_id(&app, host_id).await;
    // Old row, live bring-up: NULL linkage, aged past the 60s grace, but the
    // heartbeat is FRESH (the detached task is mid-provision).
    sqlx::query(
        "UPDATE worker_hosts SET cloud_instance_id = NULL, vpc_id = NULL, \
         bringup_heartbeat_at = now(), \
         created_at = now() - interval '120 seconds' WHERE host_id = $1",
    )
    .bind(host_id)
    .execute(&app.db)
    .await
    .expect("simulate in-flight slow bring-up");

    let deterministic_name = g5_api::cloud::host_instance_name(host_id, &org.slug);
    mock.set_instance_by_name(
        "us-central1",
        &deterministic_name,
        InstanceState::Provisioning,
    );

    let pass = g5_api::cloud::reconciler::reconcile_orphans_once(app.state.as_ref()).await;
    assert_eq!(
        pass.checked, 0,
        "fresh heartbeat → row excluded from the orphan set"
    );
    assert_eq!(pass.reattached, 0, "no mid-provision attach");

    // TOCTOU closure ([D60], Codex design-review blocker 1): even if a stale
    // list had already emitted this row, the attach itself re-checks the
    // lease — a direct attach call against the fresh lease must be a no-op.
    let attached = sqlx::query_scalar::<_, Option<Uuid>>(
        "SELECT authz.attach_orphan_worker_host_cloud_refs($1, $2, $3)",
    )
    .bind(host_id)
    .bind(format!("g5-org-{}", org.id))
    .bind(format!("us-central1-a/{deterministic_name}"))
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(
        attached.is_none(),
        "attach declines while the lease is fresh"
    );

    // Stale heartbeat (3 missed beats) → the task is dead; normal orphan
    // behavior resumes and the row is attached.
    sqlx::query(
        "UPDATE worker_hosts SET bringup_heartbeat_at = now() - interval '120 seconds' \
         WHERE host_id = $1",
    )
    .bind(host_id)
    .execute(&app.db)
    .await
    .unwrap();
    let pass = g5_api::cloud::reconciler::reconcile_orphans_once(app.state.as_ref()).await;
    assert_eq!(pass.checked, 1, "stale heartbeat → orphan set");
    assert_eq!(pass.reattached, 1, "dead-task orphan attached as before");

    app.cleanup().await;
}

#[tokio::test]
async fn provisioned_row_is_born_with_bringup_lease() {
    // Ontology [D60]: the birth lease is the column DEFAULT — a freshly
    // POSTed row must carry a non-NULL bringup_heartbeat_at without any code
    // stamping it, closing the commit-to-first-stamp window. Deliberately a
    // NO-CLOUD app (Codex round-3 N2): with no provider there is no bring-up
    // ticker, so a non-NULL value can only come from the schema DEFAULT.
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    let data = provision(&client, org.id, json!({ "region": "us-central1" })).await;
    let host_id = Uuid::parse_str(data["hostId"].as_str().unwrap()).unwrap();

    let lease = sqlx::query_scalar::<_, Option<chrono::DateTime<chrono::Utc>>>(
        "SELECT bringup_heartbeat_at FROM worker_hosts WHERE host_id = $1",
    )
    .bind(host_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(
        lease.is_some(),
        "row born with the [D60] lease (column DEFAULT)"
    );

    app.cleanup().await;
}

#[tokio::test]
async fn orphan_miss_and_terminate_are_lease_fenced() {
    // Ontology [D60] (Codex round-2 blocker 1): a lease revival between the
    // orphan SELECT and the miss/terminate writes must void counting and
    // killing, not just attaching — and the terminate must re-check the
    // threshold atomically.
    let (app, _mock) = spawn_with_mock().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    let data = provision(&client, org.id, json!({ "region": "us-central1" })).await;
    let host_id = Uuid::parse_str(data["hostId"].as_str().unwrap()).unwrap();
    let _ = wait_for_cloud_instance_id(&app, host_id).await;
    // Orphan-shaped row (NULL linkage, aged) whose lease is FRESH — the
    // revived-bring-up interleaving.
    sqlx::query(
        "UPDATE worker_hosts SET cloud_instance_id = NULL, vpc_id = NULL, \
         bringup_heartbeat_at = now(), \
         created_at = now() - interval '120 seconds' WHERE host_id = $1",
    )
    .bind(host_id)
    .execute(&app.db)
    .await
    .unwrap();

    let bumped =
        sqlx::query_scalar::<_, Option<i32>>("SELECT authz.set_worker_host_orphan_check_miss($1)")
            .bind(host_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(bumped.is_none(), "fresh lease → miss increment declines");

    // Terminate: even with a lapsed lease, a below-threshold counter must
    // decline (the threshold is re-checked inside the UPDATE)…
    sqlx::query(
        "UPDATE worker_hosts SET bringup_heartbeat_at = now() - interval '120 seconds', \
         orphan_check_misses = 9 WHERE host_id = $1",
    )
    .bind(host_id)
    .execute(&app.db)
    .await
    .unwrap();
    let killed = sqlx::query_scalar::<_, Option<Uuid>>(
        "SELECT authz.terminate_abandoned_worker_host($1, $2)",
    )
    .bind(host_id)
    .bind(g5_api::cloud::reconciler::ORPHAN_TERMINATE_MISS_THRESHOLD)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(killed.is_none(), "below-threshold terminate declines");

    // …and with a FRESH lease, even an at-threshold counter must decline.
    sqlx::query(
        "UPDATE worker_hosts SET bringup_heartbeat_at = now(), \
         orphan_check_misses = 10 WHERE host_id = $1",
    )
    .bind(host_id)
    .execute(&app.db)
    .await
    .unwrap();
    let killed = sqlx::query_scalar::<_, Option<Uuid>>(
        "SELECT authz.terminate_abandoned_worker_host($1, $2)",
    )
    .bind(host_id)
    .bind(g5_api::cloud::reconciler::ORPHAN_TERMINATE_MISS_THRESHOLD)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(
        killed.is_none(),
        "fresh lease → terminate declines at threshold"
    );
    assert_eq!(host_status(&app, host_id).await, "provisioning");

    app.cleanup().await;
}

#[tokio::test]
async fn orphan_attach_encodes_found_zone() {
    // Ontology [D61] (issue #1512, RED for the race): the attach must encode
    // the zone the by-name search FOUND the instance in — pre-[D61] it
    // re-derived the configured primary zone, stamping a wrong-zone linkage
    // whenever the [D59] fallback had placed the instance elsewhere.
    let (app, mock) = spawn_with_mock().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    let data = provision(&client, org.id, json!({ "region": "us-central1" })).await;
    let host_id = Uuid::parse_str(data["hostId"].as_str().unwrap()).unwrap();
    let _ = wait_for_cloud_instance_id(&app, host_id).await;
    sqlx::query(
        "UPDATE worker_hosts SET cloud_instance_id = NULL, vpc_id = NULL, \
         bringup_heartbeat_at = NULL, \
         created_at = now() - interval '120 seconds' WHERE host_id = $1",
    )
    .bind(host_id)
    .execute(&app.db)
    .await
    .expect("simulate orphaned row past grace window");

    // The instance lives in a NON-primary zone (fallback placed it there).
    let deterministic_name = g5_api::cloud::host_instance_name(host_id, &org.slug);
    mock.set_instance_by_name_in_zone(
        "us-central1",
        &deterministic_name,
        InstanceState::Running,
        "us-central1-f",
    );

    let pass = g5_api::cloud::reconciler::reconcile_orphans_once(app.state.as_ref()).await;
    assert_eq!(pass.reattached, 1);

    let linkage = sqlx::query_scalar::<_, Option<String>>(
        "SELECT cloud_instance_id FROM worker_hosts WHERE host_id = $1",
    )
    .bind(host_id)
    .fetch_one(&app.db)
    .await
    .unwrap()
    .expect("linkage attached");
    assert_eq!(
        linkage,
        format!("us-central1-f/{deterministic_name}"),
        "attached linkage encodes the FOUND zone, not the configured primary"
    );

    app.cleanup().await;
}

#[tokio::test]
async fn reconciler_attaches_orphan_under_legacy_instance_name() {
    // Ontology [D44] fallback: instances provisioned BEFORE the slug-bearing
    // name shipped (and provisions whose slug fetch failed) live under the
    // legacy `g5-host-{host_id}` name. The orphan pass must retry the legacy
    // name on a primary-name 404 and attach — NOT count the miss.
    let (app, mock) = spawn_with_mock().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    let data = provision(&client, org.id, json!({ "region": "us-central1" })).await;
    let host_id = Uuid::parse_str(data["hostId"].as_str().unwrap()).unwrap();
    let _ = wait_for_cloud_instance_id(&app, host_id).await;
    sqlx::query(
        "UPDATE worker_hosts SET cloud_instance_id = NULL, vpc_id = NULL, \
         bringup_heartbeat_at = NULL, \
         orphan_check_misses = 0, created_at = now() - interval '120 seconds' \
         WHERE host_id = $1",
    )
    .bind(host_id)
    .execute(&app.db)
    .await
    .expect("simulate orphaned row past grace window");

    // The mock knows the instance ONLY under the legacy (pre-[D44]) name.
    let legacy_name = format!("g5-host-{host_id}");
    mock.set_instance_by_name("us-central1", &legacy_name, InstanceState::Running);

    let pass = g5_api::cloud::reconciler::reconcile_orphans_once(app.state.as_ref()).await;
    assert_eq!(pass.checked, 1);
    assert_eq!(pass.reattached, 1, "legacy-name fallback must attach");
    assert_eq!(pass.miss_incremented, 0);
    assert_eq!(pass.errors, 0);

    // The attached cloud_instance_id must reference the name that actually
    // matched (the legacy one), so the standard pass polls the right
    // instance next tick.
    let attached: Option<String> = sqlx::query_scalar::<_, Option<String>>(
        "SELECT cloud_instance_id FROM worker_hosts WHERE host_id = $1",
    )
    .bind(host_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(attached
        .expect("cloud_instance_id attached")
        .ends_with(&legacy_name));

    app.cleanup().await;
}

#[tokio::test]
async fn reconciler_terminates_orphan_after_threshold_misses() {
    // Acceptance D40 / ontology [D39]: a row whose deterministic GCE
    // instance is NOT found (cloud 404) for `ORPHAN_TERMINATE_MISS_THRESHOLD`
    // consecutive passes must be terminated with
    // `provision_error_code='abandoned_bringup'`. Drives the threshold
    // synchronously (no sleep, no spawned loop).
    let (app, _mock) = spawn_with_mock().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    let data = provision(&client, org.id, json!({ "region": "us-central1" })).await;
    let host_id = Uuid::parse_str(data["hostId"].as_str().unwrap()).unwrap();
    let _ = wait_for_cloud_instance_id(&app, host_id).await;
    // Back-fill: simulate "POST persisted nothing, GCE never had this name".
    sqlx::query(
        "UPDATE worker_hosts SET cloud_instance_id = NULL, vpc_id = NULL, \
         bringup_heartbeat_at = NULL, \
         orphan_check_misses = 0, created_at = now() - interval '120 seconds' \
         WHERE host_id = $1",
    )
    .bind(host_id)
    .execute(&app.db)
    .await
    .unwrap();

    // The mock has no entry for the deterministic name → returns Ok(None) →
    // counts as a miss on each call. Drive the threshold.
    let threshold = g5_api::cloud::reconciler::ORPHAN_TERMINATE_MISS_THRESHOLD as usize;
    for tick in 1..=threshold {
        let pass = g5_api::cloud::reconciler::reconcile_orphans_once(app.state.as_ref()).await;
        if tick < threshold {
            assert_eq!(
                pass.miss_incremented, 1,
                "tick {tick}: bumped, below threshold"
            );
            assert_eq!(pass.terminated_abandoned, 0);
        } else {
            assert_eq!(pass.terminated_abandoned, 1, "threshold reached");
        }
        assert_eq!(pass.errors, 0);
    }

    // Final state: terminated + abandoned_bringup classified.
    let row = sqlx::query_as::<_, (String, Option<String>)>(
        "SELECT status, provision_error_code FROM worker_hosts WHERE host_id = $1",
    )
    .bind(host_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(row.0, "terminated");
    assert_eq!(row.1.as_deref(), Some("abandoned_bringup"));

    app.cleanup().await;
}

#[tokio::test]
async fn reconciler_leaves_ready_orphan_alone_after_threshold_misses() {
    let (app, mock) = spawn_with_mock().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);
    let data = provision(&client, org.id, json!({ "region": "us-central1" })).await;
    let host_id = Uuid::parse_str(data["hostId"].as_str().unwrap()).unwrap();
    let _ = wait_for_cloud_instance_id(&app, host_id).await;
    sqlx::query(
        "UPDATE worker_hosts SET cloud_instance_id = NULL, vpc_id = NULL, \
         bringup_heartbeat_at = NULL, orphan_check_misses = 3, \
         created_at = now() - interval '120 seconds' WHERE host_id = $1",
    )
    .bind(host_id)
    .execute(&app.db)
    .await
    .unwrap();
    accept_host_hello(&app, host_id).await;
    assert_eq!(host_status(&app, host_id).await, "ready");

    // Neither repeated 404s nor a stopped-instance observation proves a
    // registered host can be abandoned. Preserve even an earlier miss count.
    let name = g5_api::cloud::host_instance_name(host_id, &org.slug);
    for instance_state in [None, Some(InstanceState::Terminated)] {
        if let Some(state) = instance_state {
            mock.set_instance_by_name("us-central1", &name, state);
        }
        for _ in 0..=g5_api::cloud::reconciler::ORPHAN_TERMINATE_MISS_THRESHOLD {
            let pass = g5_api::cloud::reconciler::reconcile_orphans_once(&app.state).await;
            assert_eq!(pass.checked, 1, "ready orphan remains eligible for repair");
            assert_eq!(pass.reattached, 0);
            assert_eq!(pass.miss_incremented, 0);
            assert_eq!(pass.terminated_abandoned, 0);
            assert_eq!(pass.errors, 0);
        }
    }
    let row: (String, i32, Option<String>, Option<String>) = sqlx::query_as(
        "SELECT status, orphan_check_misses, provision_error_code, cloud_instance_id \
         FROM worker_hosts WHERE host_id = $1",
    )
    .bind(host_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(row, ("ready".to_string(), 3, None, None));
    app.cleanup().await;
}

#[tokio::test]
async fn reconciler_orphan_lookup_error_does_not_increment_miss_counter() {
    // Ontology [D39]: transport / auth failures on `get_instance_by_name`
    // mean "we couldn't reach GCP, the instance might still exist" — the
    // miss counter must NOT be bumped, or a network blip would terminate a
    // row whose GCE instance is fine.
    let (app, mock) = spawn_with_mock().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    let data = provision(&client, org.id, json!({ "region": "us-central1" })).await;
    let host_id = Uuid::parse_str(data["hostId"].as_str().unwrap()).unwrap();
    let _ = wait_for_cloud_instance_id(&app, host_id).await;
    sqlx::query(
        "UPDATE worker_hosts SET cloud_instance_id = NULL, vpc_id = NULL, \
         bringup_heartbeat_at = NULL, \
         orphan_check_misses = 0, created_at = now() - interval '120 seconds' \
         WHERE host_id = $1",
    )
    .bind(host_id)
    .execute(&app.db)
    .await
    .unwrap();

    mock.fail_get_by_name(true);
    let pass = g5_api::cloud::reconciler::reconcile_orphans_once(app.state.as_ref()).await;
    assert_eq!(pass.checked, 1);
    assert_eq!(pass.errors, 1, "lookup error counted");
    assert_eq!(
        pass.miss_incremented, 0,
        "lookup error MUST NOT bump miss counter"
    );
    assert_eq!(pass.terminated_abandoned, 0);

    // The DB column reflects the no-bump invariant.
    let misses: i32 =
        sqlx::query_scalar("SELECT orphan_check_misses FROM worker_hosts WHERE host_id = $1")
            .bind(host_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(misses, 0);

    app.cleanup().await;
}

#[tokio::test]
async fn reconciler_orphan_pass_no_op_when_handler_already_attached() {
    // Round-3 post-review (my-subagent P1): the handler-vs-reconciler attach
    // race. If the POST handler's step-3 (cloud_instance_id persist) lands
    // between `list_orphan_bringup_worker_hosts` and any per-row setter, the
    // row must NOT be double-handled. The SELECT predicate filters
    // `cloud_instance_id IS NULL`, so once the handler catches up the row
    // simply drops out of the orphan candidate set — checked stays 0 on the
    // next pass and no counters move.
    let (app, _mock) = spawn_with_mock().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    let data = provision(&client, org.id, json!({ "region": "us-central1" })).await;
    let host_id = Uuid::parse_str(data["hostId"].as_str().unwrap()).unwrap();

    // Wait for the detached bring-up to land its cloud_instance_id (this is
    // the "handler caught up" state we want to test against).
    let cloud_instance_id = wait_for_cloud_instance_id(&app, host_id).await;
    // Age the row so it WOULD be in the orphan window if its
    // cloud_instance_id were NULL; the SELECT predicate is what saves it.
    sqlx::query(
        "UPDATE worker_hosts SET created_at = now() - interval '120 seconds' WHERE host_id = $1",
    )
    .bind(host_id)
    .execute(&app.db)
    .await
    .unwrap();

    let pass = g5_api::cloud::reconciler::reconcile_orphans_once(app.state.as_ref()).await;
    assert_eq!(
        pass.checked, 0,
        "row with non-NULL cloud_instance_id must not appear in orphan SELECT"
    );
    assert_eq!(pass.reattached, 0);
    assert_eq!(pass.miss_incremented, 0);
    assert_eq!(pass.terminated_abandoned, 0);
    assert_eq!(pass.errors, 0);
    // Cloud refs unchanged.
    let stored: String =
        sqlx::query_scalar("SELECT cloud_instance_id FROM worker_hosts WHERE host_id = $1")
            .bind(host_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(stored, cloud_instance_id);

    app.cleanup().await;
}

#[tokio::test]
async fn reconciler_orphan_pass_treats_terminated_instance_as_missing() {
    // Round-3 post-review (chatgpt-codex-connector P2): a GCE 200 OK for a
    // STOPPED/TERMINATED instance maps to `InstanceState::Terminated`. Per
    // D39 contract, only NON-Terminated states count as "found alive". If we
    // attached a Terminated instance, the standard pass would later mark the
    // row `terminated` WITHOUT `provision_error_code='abandoned_bringup'`,
    // losing the orphan-pass classification. Confirm Terminated folds into
    // the missing-arm: miss counter bumps, no reattach.
    let (app, mock) = spawn_with_mock().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    let data = provision(&client, org.id, json!({ "region": "us-central1" })).await;
    let host_id = Uuid::parse_str(data["hostId"].as_str().unwrap()).unwrap();
    let _ = wait_for_cloud_instance_id(&app, host_id).await;
    sqlx::query(
        "UPDATE worker_hosts SET cloud_instance_id = NULL, vpc_id = NULL, \
         bringup_heartbeat_at = NULL, \
         orphan_check_misses = 0, created_at = now() - interval '120 seconds' \
         WHERE host_id = $1",
    )
    .bind(host_id)
    .execute(&app.db)
    .await
    .unwrap();

    // Mock reports the instance present but TERMINATED.
    let deterministic_name = g5_api::cloud::host_instance_name(host_id, &org.slug);
    mock.set_instance_by_name(
        "us-central1",
        &deterministic_name,
        InstanceState::Terminated,
    );

    let pass = g5_api::cloud::reconciler::reconcile_orphans_once(app.state.as_ref()).await;
    assert_eq!(pass.checked, 1);
    assert_eq!(pass.reattached, 0, "Terminated must NOT reattach");
    assert_eq!(
        pass.miss_incremented, 1,
        "Terminated folds into missing-arm"
    );
    assert_eq!(pass.terminated_abandoned, 0, "below threshold this pass");
    assert_eq!(pass.errors, 0);

    // Row stays NULL and provisioning — preserved for the next pass.
    let row = sqlx::query_as::<_, (Option<String>, String)>(
        "SELECT cloud_instance_id, status FROM worker_hosts WHERE host_id = $1",
    )
    .bind(host_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(row.0.is_none(), "cloud_instance_id stays NULL");
    assert_eq!(row.1, "provisioning");

    app.cleanup().await;
}

#[tokio::test]
async fn reconciler_orphan_pass_bounds_classified_rows_and_never_abandons_them() {
    // Issue #627 S18 (both CodeRabbit rounds on PR #637): a row carrying
    // `provision_error_code='zone_capacity_exhausted'` with NO retry schedule
    // is included in the orphan candidate set with a BOUNDED lookup budget —
    // confirmed-missing observations count misses until the row ages out of
    // the selector (a budget-frozen row costs a bounded number of GCE lookups
    // in total, while a pre-migration strand of ANY age gets checked), and
    // `terminate_abandoned_worker_host` keeps refusing classified rows so the
    // UI's "No capacity" badge and the [D66] freeze stay truthful.
    let (app, _mock) = spawn_with_mock().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    let data = provision(&client, org.id, json!({ "region": "us-central1" })).await;
    let host_id = Uuid::parse_str(data["hostId"].as_str().unwrap()).unwrap();
    let _ = wait_for_cloud_instance_id(&app, host_id).await;
    // Back-fill: an OLD stranded row (well past any time window) two misses
    // short of the threshold.
    let threshold = g5_api::cloud::reconciler::ORPHAN_TERMINATE_MISS_THRESHOLD;
    sqlx::query(
        "UPDATE worker_hosts SET cloud_instance_id = NULL, vpc_id = NULL, \
         bringup_heartbeat_at = NULL, \
         orphan_check_misses = $1 - 2, provision_error_code = 'zone_capacity_exhausted', \
         created_at = now() - interval '3 days' \
         WHERE host_id = $2",
    )
    .bind(threshold)
    .bind(host_id)
    .execute(&app.db)
    .await
    .unwrap();

    // No instance exists anywhere: the lookup counts a miss below threshold.
    let pass = g5_api::cloud::reconciler::reconcile_orphans_once(app.state.as_ref()).await;
    assert_eq!(pass.checked, 1, "old classified strand is still observed");
    assert_eq!(pass.miss_incremented, 1, "bounded counting: {pass:?}");
    assert_eq!(pass.terminated_abandoned, 0, "never abandoned: {pass:?}");

    // The next pass reaches the threshold: the terminate setter REFUSES the
    // classified row, so the badge survives and the row simply ages out.
    let pass = g5_api::cloud::reconciler::reconcile_orphans_once(app.state.as_ref()).await;
    assert_eq!(pass.checked, 1, "threshold-crossing pass: {pass:?}");
    assert_eq!(pass.terminated_abandoned, 0, "never abandoned: {pass:?}");
    let (status, code, misses): (String, Option<String>, i32) = sqlx::query_as(
        "SELECT status, provision_error_code, orphan_check_misses \
         FROM worker_hosts WHERE host_id = $1",
    )
    .bind(host_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(status, "provisioning", "freeze semantics preserved");
    assert_eq!(code.as_deref(), Some("zone_capacity_exhausted"));
    assert_eq!(misses, threshold, "counter aged the row to the bound");

    // At the bound the selector stops returning the row: no further lookups.
    let pass = g5_api::cloud::reconciler::reconcile_orphans_once(app.state.as_ref()).await;
    assert_eq!(
        pass.checked, 0,
        "aged-out row costs no more lookups: {pass:?}"
    );

    app.cleanup().await;
}

#[tokio::test]
async fn reconciler_orphan_pass_reattaches_stranded_stale_classification() {
    // Issue #627 S18 (`g5.runtime.host.cloud-recovery`): a retry that created
    // an instance but failed to persist linkage used to strand BOTH — the row
    // kept a stale `zone_capacity_exhausted` badge no selector covered while
    // the cloud-orphan sweep saw a live row and spared the instance. The
    // orphan pass now attaches such rows and clears the stale classification.
    let (app, mock) = spawn_with_mock().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    let data = provision(&client, org.id, json!({ "region": "us-central1" })).await;
    let host_id = Uuid::parse_str(data["hostId"].as_str().unwrap()).unwrap();
    let _ = wait_for_cloud_instance_id(&app, host_id).await;
    sqlx::query(
        "UPDATE worker_hosts SET cloud_instance_id = NULL, vpc_id = NULL, \
         bringup_heartbeat_at = NULL, \
         orphan_check_misses = 0, provision_error_code = 'zone_capacity_exhausted', \
         provision_next_retry_at = NULL, \
         created_at = now() - interval '120 seconds' \
         WHERE host_id = $1",
    )
    .bind(host_id)
    .execute(&app.db)
    .await
    .unwrap();
    // The stranded instance IS alive under the deterministic name.
    let deterministic_name = g5_api::cloud::host_instance_name(host_id, &org.slug);
    mock.set_instance_by_name("us-central1", &deterministic_name, InstanceState::Running);

    let pass = g5_api::cloud::reconciler::reconcile_orphans_once(app.state.as_ref()).await;
    assert_eq!(pass.reattached, 1, "stranded instance re-linked: {pass:?}");

    let (linkage, code, next_retry): (
        Option<String>,
        Option<String>,
        Option<chrono::DateTime<chrono::Utc>>,
    ) = sqlx::query_as(
        "SELECT cloud_instance_id, provision_error_code, provision_next_retry_at \
         FROM worker_hosts WHERE host_id = $1",
    )
    .bind(host_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(
        linkage.as_deref(),
        Some(format!("us-central1-a/{deterministic_name}").as_str()),
        "linkage attached in the found zone"
    );
    assert_eq!(code, None, "stale classification cleared on attach");
    assert_eq!(next_retry, None);

    app.cleanup().await;
}

#[tokio::test]
async fn reconciler_orphan_pass_leaves_capacity_claims_to_their_owner() {
    // Issue #627 S3 (`g5.runtime.host.cloud-recovery`): a managed-capacity
    // claim queued behind the launch lanes has no bring-up lease, but its
    // OWNER (the capacity sweep) is still honoring it inside the 15-minute
    // claim window — the orphan pass must neither miss-count nor abandon it.
    let (app, mock) = spawn_with_mock().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    let data = provision(&client, org.id, json!({ "region": "us-central1" })).await;
    let host_id = Uuid::parse_str(data["hostId"].as_str().unwrap()).unwrap();
    let _ = wait_for_cloud_instance_id(&app, host_id).await;
    // Shape the row into a queued floor claim: unlinked, past grace, no
    // lease, one miss short of the threshold.
    let threshold = g5_api::cloud::reconciler::ORPHAN_TERMINATE_MISS_THRESHOLD;
    sqlx::query(
        "UPDATE worker_hosts SET cloud_instance_id = NULL, vpc_id = NULL, \
         bringup_heartbeat_at = NULL, automatic_capacity = true, \
         capacity_floor_claim = true, \
         orphan_check_misses = $1 - 1, created_at = now() - interval '120 seconds' \
         WHERE host_id = $2",
    )
    .bind(threshold)
    .bind(host_id)
    .execute(&app.db)
    .await
    .unwrap();

    let pass = g5_api::cloud::reconciler::reconcile_orphans_once(app.state.as_ref()).await;
    assert_eq!(
        pass.checked, 0,
        "claims never enter the orphan set: {pass:?}"
    );

    // Defense in depth: the SQL setters refuse claims even when called
    // directly at/over the threshold.
    let bumped: Option<i32> =
        sqlx::query_scalar("SELECT authz.set_worker_host_orphan_check_miss($1)")
            .bind(host_id)
            .fetch_one(&app.system_pool)
            .await
            .unwrap();
    assert_eq!(bumped, None, "miss setter refuses a claim");
    sqlx::query("UPDATE worker_hosts SET orphan_check_misses = $1 WHERE host_id = $2")
        .bind(threshold)
        .bind(host_id)
        .execute(&app.db)
        .await
        .unwrap();
    let terminated: Option<Uuid> = sqlx::query_scalar::<_, Option<Uuid>>(
        "SELECT authz.terminate_abandoned_worker_host($1, $2)",
    )
    .bind(host_id)
    .bind(threshold)
    .fetch_one(&app.system_pool)
    .await
    .unwrap();
    assert_eq!(terminated, None, "terminate setter refuses a claim");
    let (status, code): (String, Option<String>) =
        sqlx::query_as("SELECT status, provision_error_code FROM worker_hosts WHERE host_id = $1")
            .bind(host_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(status, "provisioning", "claim stays its owner's to resolve");
    assert_eq!(code, None);

    // A claim host that REGISTERED while unlinked leaves the owner's recovery
    // scope (drive_claim handles provisioning rows only) and re-enters
    // ordinary linkage recovery (issue #627 Codex review P1): the orphan pass
    // must attach it so rotation and retirement see its instance again.
    sqlx::query(
        "UPDATE worker_hosts SET status = 'ready', slot_size_reported = true, \
             last_heartbeat = now(), orphan_check_misses = 0 WHERE host_id = $1",
    )
    .bind(host_id)
    .execute(&app.db)
    .await
    .unwrap();
    let deterministic_name = g5_api::cloud::host_instance_name(host_id, &org.slug);
    mock.set_instance_by_name("us-central1", &deterministic_name, InstanceState::Running);
    let pass = g5_api::cloud::reconciler::reconcile_orphans_once(app.state.as_ref()).await;
    assert_eq!(
        pass.reattached, 1,
        "registered claim re-linked by the orphan pass: {pass:?}"
    );
    let linkage: Option<String> =
        sqlx::query_scalar("SELECT cloud_instance_id FROM worker_hosts WHERE host_id = $1")
            .bind(host_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(
        linkage.as_deref(),
        Some(format!("us-central1-a/{deterministic_name}").as_str())
    );

    app.cleanup().await;
}

#[tokio::test]
async fn bringup_compensation_leaves_live_unlinked_row_for_reconciliation() {
    // Issue #627 S19 (`g5.runtime.host.compensation`): hello can promote a row
    // to `ready` BEFORE bring-up persists linkage. The persistence then
    // declines (row left `provisioning`), and compensation used to read the
    // live NULL-linkage row as "not owner" and terminate the just-registered
    // host's instance. A live unlinked row is UNCERTAIN ownership: leave the
    // instance; the orphan pass attaches it by its deterministic name.
    let (app, mock) = spawn_with_mock().await;
    mock.set_provision_delay(std::time::Duration::from_millis(3_000));
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    let data = provision(&client, org.id, json!({ "region": "us-central1" })).await;
    let host_id = Uuid::parse_str(data["hostId"].as_str().unwrap()).unwrap();
    // Mid-bring-up, the host registers: `provisioning → ready` (hello's
    // transition), before linkage persistence.
    sqlx::query(
        "UPDATE worker_hosts SET status = 'ready', slot_size_reported = true, \
             last_heartbeat = now() WHERE host_id = $1 AND status = 'provisioning'",
    )
    .bind(host_id)
    .execute(&app.db)
    .await
    .unwrap();

    // Wait out the delayed bring-up and its compensation decision.
    tokio::time::sleep(std::time::Duration::from_millis(4_500)).await;
    assert!(
        !mock
            .calls()
            .iter()
            .any(|c| matches!(c, MockCall::TerminateInstance { .. })),
        "a live unlinked row's instance is never compensation-terminated: {:?}",
        mock.calls()
    );
    let (status, linkage): (String, Option<String>) =
        sqlx::query_as("SELECT status, cloud_instance_id FROM worker_hosts WHERE host_id = $1")
            .bind(host_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(status, "ready", "registered host untouched");
    assert_eq!(linkage, None, "linkage persistence declined as designed");

    // Recovery: the orphan pass attaches the live instance by name once the
    // bring-up lease is stale.
    sqlx::query(
        "UPDATE worker_hosts SET bringup_heartbeat_at = NULL, \
             created_at = now() - interval '120 seconds' WHERE host_id = $1",
    )
    .bind(host_id)
    .execute(&app.db)
    .await
    .unwrap();
    let deterministic_name = g5_api::cloud::host_instance_name(host_id, &org.slug);
    mock.set_instance_by_name("us-central1", &deterministic_name, InstanceState::Running);
    let pass = g5_api::cloud::reconciler::reconcile_orphans_once(app.state.as_ref()).await;
    assert_eq!(
        pass.reattached, 1,
        "orphan pass re-linked the live host: {pass:?}"
    );

    app.cleanup().await;
}

#[tokio::test]
async fn bringup_compensation_still_terminates_for_a_terminated_row() {
    // The [D61] posture survives issue #627 S19: when the row RACED OUT of
    // the live set (operator DELETE during bring-up), the created instance
    // has no owner and compensation must still terminate it.
    let (app, mock) = spawn_with_mock().await;
    mock.set_provision_delay(std::time::Duration::from_millis(3_000));
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    let data = provision(&client, org.id, json!({ "region": "us-central1" })).await;
    let host_id = Uuid::parse_str(data["hostId"].as_str().unwrap()).unwrap();
    sqlx::query(
        "UPDATE worker_hosts SET status = 'terminated', last_heartbeat = NULL \
         WHERE host_id = $1 AND status = 'provisioning'",
    )
    .bind(host_id)
    .execute(&app.db)
    .await
    .unwrap();

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        if mock
            .calls()
            .iter()
            .any(|c| matches!(c, MockCall::TerminateInstance { .. }))
        {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "compensation never terminated the unowned instance: {:?}",
            mock.calls()
        );
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    app.cleanup().await;
}

#[tokio::test]
async fn reconciler_orphan_pass_skips_row_in_transient_cloud_state() {
    // Round-4 post-review (subagent §D): only `Running | Provisioning` count
    // as "found alive" for attach purposes. `Stopping` (mid-shutdown) and
    // `Unknown` (unrecognized GCE status) are TRANSIENT — the orphan pass
    // must NOT attach (would race a parallel terminate, and the standard
    // pass next tick would mark the row terminated without
    // `provision_error_code='abandoned_bringup'`) and must NOT bump the miss
    // counter (the instance IS there, just transitioning). It must skip and
    // retry next pass. This guards against the `#[non_exhaustive]` enum's
    // future variants quietly defaulting into the attach arm.
    let (app, mock) = spawn_with_mock().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    let data = provision(&client, org.id, json!({ "region": "us-central1" })).await;
    let host_id = Uuid::parse_str(data["hostId"].as_str().unwrap()).unwrap();
    let _ = wait_for_cloud_instance_id(&app, host_id).await;
    sqlx::query(
        "UPDATE worker_hosts SET cloud_instance_id = NULL, vpc_id = NULL, \
         bringup_heartbeat_at = NULL, \
         orphan_check_misses = 0, created_at = now() - interval '120 seconds' \
         WHERE host_id = $1",
    )
    .bind(host_id)
    .execute(&app.db)
    .await
    .unwrap();

    let deterministic_name = g5_api::cloud::host_instance_name(host_id, &org.slug);
    mock.set_instance_by_name("us-central1", &deterministic_name, InstanceState::Stopping);

    let pass = g5_api::cloud::reconciler::reconcile_orphans_once(app.state.as_ref()).await;
    assert_eq!(pass.checked, 1);
    assert_eq!(pass.reattached, 0, "Stopping must NOT attach");
    assert_eq!(
        pass.miss_incremented, 0,
        "Stopping must NOT bump miss counter (instance IS there)"
    );
    assert_eq!(pass.terminated_abandoned, 0);
    assert_eq!(pass.errors, 0);

    // Row stays NULL + provisioning + miss-counter at 0 → next pass reconciles
    // whatever cloud state the instance has reached.
    let row = sqlx::query_as::<_, (Option<String>, String, i32)>(
        "SELECT cloud_instance_id, status, orphan_check_misses \
         FROM worker_hosts WHERE host_id = $1",
    )
    .bind(host_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(row.0.is_none(), "cloud_instance_id stays NULL");
    assert_eq!(row.1, "provisioning");
    assert_eq!(row.2, 0, "miss counter unchanged");

    app.cleanup().await;
}

#[tokio::test]
async fn reconciler_orphan_transient_resets_miss_streak() {
    // Round-5 post-review (codex-connector P2): a live-but-transient cloud
    // observation MUST break any accumulated missing-streak. Otherwise:
    //   tick 1-9: Ok(None) → counter = 9
    //   tick 10:  Ok(Some(Stopping)) → counter unchanged at 9 (round-5 bug)
    //   tick 11:  Ok(None) → counter = 10 → terminate fires
    // but the misses weren't consecutive. The transient arm must reset
    // the counter via the migration-105 setter so the threshold continues
    // to mean "ORPHAN_TERMINATE_MISS_THRESHOLD consecutive missing
    // observations" per D39.
    let (app, mock) = spawn_with_mock().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    let data = provision(&client, org.id, json!({ "region": "us-central1" })).await;
    let host_id = Uuid::parse_str(data["hostId"].as_str().unwrap()).unwrap();
    let _ = wait_for_cloud_instance_id(&app, host_id).await;
    // Back-fill: orphan-eligible row with ALREADY-ACCUMULATED misses (9).
    sqlx::query(
        "UPDATE worker_hosts SET cloud_instance_id = NULL, vpc_id = NULL, \
         bringup_heartbeat_at = NULL, \
         orphan_check_misses = 9, created_at = now() - interval '120 seconds' \
         WHERE host_id = $1",
    )
    .bind(host_id)
    .execute(&app.db)
    .await
    .unwrap();

    // Mock returns Stopping for the deterministic name — transient arm fires.
    let deterministic_name = g5_api::cloud::host_instance_name(host_id, &org.slug);
    mock.set_instance_by_name("us-central1", &deterministic_name, InstanceState::Stopping);

    let pass = g5_api::cloud::reconciler::reconcile_orphans_once(app.state.as_ref()).await;
    assert_eq!(pass.checked, 1);
    assert_eq!(pass.reattached, 0);
    assert_eq!(pass.miss_incremented, 0);
    assert_eq!(pass.terminated_abandoned, 0);
    assert_eq!(pass.errors, 0);

    // Counter must be reset to 0 — the missing streak was broken by the
    // transient observation.
    let misses: i32 =
        sqlx::query_scalar("SELECT orphan_check_misses FROM worker_hosts WHERE host_id = $1")
            .bind(host_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(
        misses, 0,
        "transient observation must reset the missing-streak counter"
    );

    app.cleanup().await;
}

#[tokio::test]
async fn terminate_abandoned_preserves_classified_failure_under_setter_race() {
    // Round-4 post-review (subagent §E): the round-4 `provision_error_code IS NULL`
    // guard on `terminate_abandoned_worker_host` closes a specific race:
    // the orphan-pass increment bumps to THRESHOLD, but BEFORE the per-row
    // terminate setter fires, the D34 classifier writes
    // `zone_capacity_exhausted` (e.g. a late-arriving cloud bring-up failure
    // signal). Without the guard, terminate would overwrite the precise
    // classification with `abandoned_bringup`, losing the UI's "No capacity"
    // badge. The round-3 selector guard alone is insufficient because the
    // row was already in the orphan candidate set when the SELECT ran.
    //
    // This test exercises the SQL contract directly to pin the round-4
    // guard against regression: the reconciler entry point doesn't reach
    // this race window in a single deterministic pass.
    let (app, _mock) = spawn_with_mock().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    let data = provision(&client, org.id, json!({ "region": "us-central1" })).await;
    let host_id = Uuid::parse_str(data["hostId"].as_str().unwrap()).unwrap();
    let _ = wait_for_cloud_instance_id(&app, host_id).await;
    // Back-fill: orphan-eligible row, miss counter already at threshold.
    let threshold = g5_api::cloud::reconciler::ORPHAN_TERMINATE_MISS_THRESHOLD;
    sqlx::query(
        "UPDATE worker_hosts SET cloud_instance_id = NULL, vpc_id = NULL, \
         bringup_heartbeat_at = NULL, \
         orphan_check_misses = $1, created_at = now() - interval '120 seconds' \
         WHERE host_id = $2",
    )
    .bind(threshold)
    .bind(host_id)
    .execute(&app.db)
    .await
    .unwrap();

    // Simulate the classifier writing the code between the increment-to-
    // threshold and the terminate call.
    sqlx::query(
        "UPDATE worker_hosts SET provision_error_code = 'zone_capacity_exhausted' \
         WHERE host_id = $1",
    )
    .bind(host_id)
    .execute(&app.db)
    .await
    .unwrap();

    // Drive the SQL contract directly (the reconciler entry point would not
    // hit this race window deterministically).
    let terminated: Option<Uuid> = sqlx::query_scalar::<_, Option<Uuid>>(
        "SELECT authz.terminate_abandoned_worker_host($1, $2)",
    )
    .bind(host_id)
    .bind(g5_api::cloud::reconciler::ORPHAN_TERMINATE_MISS_THRESHOLD)
    .fetch_one(&app.system_pool)
    .await
    .unwrap();

    assert!(
        terminated.is_none(),
        "terminate setter must no-op when provision_error_code is already set"
    );

    // Classification preserved and row still provisioning (next standard
    // pass will reconcile from the classified failure, not abandoned_bringup).
    let row = sqlx::query_as::<_, (String, Option<String>)>(
        "SELECT status, provision_error_code FROM worker_hosts WHERE host_id = $1",
    )
    .bind(host_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(row.0, "provisioning", "status preserved");
    assert_eq!(
        row.1.as_deref(),
        Some("zone_capacity_exhausted"),
        "classification preserved (NOT overwritten with abandoned_bringup)"
    );

    app.cleanup().await;
}

#[tokio::test]
async fn reconciler_orphan_pass_skips_rows_within_grace_window() {
    // Ontology [D39]: a row younger than the 60s grace window must NOT be
    // examined — it might be a still-in-flight POST handler whose spawned
    // bring-up has not yet reached the cloud_instance_id persist step.
    // Preempting it would cause the orphan-pass to race the normal
    // bring-up.
    let (app, _mock) = spawn_with_mock().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    let data = provision(&client, org.id, json!({ "region": "us-central1" })).await;
    let host_id = Uuid::parse_str(data["hostId"].as_str().unwrap()).unwrap();
    let _ = wait_for_cloud_instance_id(&app, host_id).await;
    // Back-fill cloud_instance_id NULL but keep `created_at` recent (within
    // the grace window). The orphan SELECT must skip the row.
    sqlx::query(
        "UPDATE worker_hosts SET cloud_instance_id = NULL, vpc_id = NULL, \
         bringup_heartbeat_at = NULL \
         WHERE host_id = $1",
    )
    .bind(host_id)
    .execute(&app.db)
    .await
    .unwrap();

    let pass = g5_api::cloud::reconciler::reconcile_orphans_once(app.state.as_ref()).await;
    assert_eq!(
        pass.checked, 0,
        "row inside 60s grace window must not appear in orphan SELECT"
    );

    app.cleanup().await;
}
