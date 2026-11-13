use crate::helpers;

use std::time::Duration;

use base64::Engine;
use g5_auth::Scope;
use g5_internal_api::workspace_flush::FlushReason;
use reqwest::StatusCode;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::process::Command;
use uuid::Uuid;

use g5_api::services::workspace_flush_service::{WorkspaceFlushError, WorkspaceFlushService};
use g5_api::services::workspace_fork_service::{ForkSource, ForkWorkspaceRequest};
use helpers::{
    hold_while_the_lease_ends, test_jwt_minter, wait_for_lock_wait_past_the_lease, TestApp,
    TestOrg, TestUser, TestWorkspace, TestWorkspaceRuntimePeer, TEST_PRIVATE_KEY_PEM,
};

async fn workspace_vm_service(
    app: &TestApp,
    actor_user_id: Uuid,
    org_id: Uuid,
) -> g5_api::services::WorkspaceVmService {
    let db = app.scoped_db_for_user_org(actor_user_id, org_id).await;
    let scope = db.reopener();
    db.rollback().await.expect("close captured lifecycle scope");
    g5_api::services::WorkspaceVmService::new(g5_api::services::WorkspaceVmServiceConfig {
        system_pool: app.state.system_pool.clone(),
        scope,
        hosts: app.state.preview_tunnels.clone(),
        minter: app
            .state
            .jwt_minter
            .clone()
            .expect("test app configures a lifecycle JWT minter"),
        runtime: app.state.workspace_runtime.clone(),
        agent_work: app.state.agent_work.clone(),
        repo_storage: app.state.repo_storage.clone(),
        git_external_url: app.state.git_external_url.clone(),
        ws_hub: app.state.event_bus.ws_hub().clone(),
    })
}

fn data(body: &Value) -> &Value {
    body.get("data").expect("response should contain data")
}

/// `g5.preview.output.live-tail`: the owner terminal route carries a hosted
/// preview identity exactly as it carries a shell's. The adapter validates
/// nothing about the namespace and forwards `terminal_id` and `offset` in
/// the `open` frame unchanged; the guest decides whether it hosts the name.
#[tokio::test]
async fn owner_terminal_relays_a_hosted_preview_identity_unchanged() {
    use futures_util::StreamExt;
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let (peer, mut streams) =
        TestWorkspaceRuntimePeer::attach_with_streams(&app, &workspace, owner.id).await;
    let response = app
        .authenticated_client(&owner)
        .post_json(
            "/api/v1/auth/ws-ticket",
            &json!({"workspaceId": workspace.id}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let ticket: Value = response.json().await.unwrap();
    let task_id = Uuid::new_v4();
    let (mut browser, _) = tokio_tungstenite::connect_async(format!(
        "{}/api/v1/workspaces/{}/terminal?token={}&terminal_id=preview:{task_id}&offset=4096&cols=100&rows=30",
        app.ws_url,
        workspace.id,
        ticket["data"]["ticket"].as_str().unwrap()
    ))
    .await
    .expect("the owner terminal route accepts a hosted identity");
    let mut guest = tokio::time::timeout(Duration::from_secs(5), streams.recv())
        .await
        .expect("the PTY stream reaches the guest")
        .expect("the PTY stream is open");
    guest.recv().await.unwrap(); // Mux incarnation header.
    let open: Value = serde_json::from_slice(&guest.recv().await.unwrap()).unwrap();
    assert_eq!(open["type"], "open");
    assert_eq!(open["terminal_id"], format!("preview:{task_id}"));
    assert_eq!(open["offset"], 4096);
    assert_eq!(open["cols"], 100);
    assert_eq!(open["rows"], 30);
    // The guest's answer and its output flow back as for any terminal; here
    // the scripted guest replays a tail and ends the session.
    guest
        .send(
            br#"{"type":"opened","terminal_id":"preview:x","reattached":true,"offset":4096,"reset":false}
"#,
        )
        .await
        .unwrap();
    guest.send(b"retained tail").await.unwrap();
    guest
        .send(
            br#"{"type":"closed","reason":"session_ended"}
"#,
        )
        .await
        .unwrap();
    let mut relayed = Vec::new();
    while relayed.len() < 3 {
        let message = tokio::time::timeout(Duration::from_secs(5), browser.next())
            .await
            .expect("relayed frame")
            .expect("browser socket open")
            .expect("frame");
        relayed.push(message.into_data());
    }
    assert!(String::from_utf8_lossy(&relayed[0]).starts_with(r#"{"type":"opened""#));
    assert_eq!(relayed[1], b"retained tail");
    assert_eq!(
        serde_json::from_slice::<Value>(&relayed[2]).unwrap(),
        json!({"type":"closed","reason":"session_ended"})
    );
    drop(peer);
    app.cleanup().await;
}

#[tokio::test]
async fn pending_admission_close_does_not_hold_the_flush_receipt_lock() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let peer = TestWorkspaceRuntimePeer::attach_with_rpc_barrier(
        &app,
        &workspace,
        owner.id,
        "admission.close",
    )
    .await;
    let claims = g5_auth::WorkspaceIncarnationClaims {
        org_id: org.id,
        workspace_id: workspace.id,
        owner_user_id: owner.id,
        vm_id: peer.vm_id(),
        epoch: 1,
        iat: chrono::Utc::now().timestamp(),
        exp: chrono::Utc::now().timestamp() + 3600,
        iss: Some("g5-api-test".to_string()),
        aud: g5_auth::WORKSPACE_INCARNATION_AUDIENCE.to_string(),
        jti: Uuid::new_v4().to_string(),
        token_use: g5_auth::WORKSPACE_INCARNATION_TOKEN_USE.to_string(),
    };
    let receipts = WorkspaceFlushService::new(
        app.state.system_pool.clone(),
        app.state.repo_storage.clone(),
        app.state.git_bin.clone(),
    );
    let started = receipts
        .start(&claims, FlushReason::Shutdown, "concurrent-admission-close")
        .await
        .expect("start flush receipt");
    let handle = app
        .state
        .workspace_runtime
        .get(workspace.id)
        .await
        .expect("load runtime")
        .expect("runtime is attached");
    assert_eq!((handle.vm_id, handle.epoch), (peer.vm_id(), 1));
    let runtime = app.state.workspace_runtime.clone();
    let close = tokio::spawn(async move {
        runtime
            .lifecycle_call_for_test(
                &handle,
                "admission.close",
                json!({"prepare_only": true, "operation_id": Uuid::new_v4()}),
                "concurrent-admission-close",
            )
            .await
    });
    tokio::time::timeout(Duration::from_secs(10), async {
        while !peer
            .received_rpc_methods()
            .await
            .iter()
            .any(|method| method == "admission.close")
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("close reaches guest barrier");

    // A peer stop can wait at the guest lifecycle gate while the first stop
    // flushes and calls back to this service. Completion's authority read must
    // not wait on the peer RPC's workspace_vms read fence; an empty receipt
    // then stops at the member vector, which is checked after that read.
    let verification = tokio::time::timeout(
        Duration::from_secs(2),
        receipts.complete(&claims, started.flush_id, Vec::new()),
    )
    .await;
    peer.release_rpc_barrier();
    close.await.expect("join close").expect("close succeeds");
    let verification =
        verification.expect("receipt verification is independent of guest RPC completion");
    assert!(
        matches!(verification, Err(WorkspaceFlushError::MemberVector)),
        "the empty receipt reaches the member vector check: {verification:?}"
    );
    app.cleanup().await;
}

#[tokio::test]
async fn configured_image_default_repairs_existing_org_before_workspace_resume() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let missing_org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &missing_org).await;
    workspace.attach_parent(&app).await;
    let pinned_org = TestOrg::create(&app).await;
    let tracked_org = TestOrg::create(&app).await;
    let image = format!(
        "mirror.gcr.io/library/buildpack-deps@sha256:{}",
        "a".repeat(64)
    );
    let pinned = format!("registry.example.com/team/pinned@sha256:{}", "b".repeat(64));
    let tracked = format!(
        "registry.example.com/team/tracked@sha256:{}",
        "c".repeat(64)
    );
    sqlx::query("UPDATE orgs SET default_base_image_ref = $1 WHERE id = $2")
        .bind(&pinned)
        .bind(pinned_org.id)
        .execute(&app.db)
        .await
        .expect("pin existing org");
    sqlx::query(
        "UPDATE orgs SET default_base_image_track = 'other-track', default_base_image_ref = $1 WHERE id = $2",
    )
    .bind(&tracked)
    .bind(tracked_org.id)
    .execute(&app.db)
    .await
    .expect("track another image");

    let hosts = g5_api::services::worker_host_service::WorkerHostService::new();
    assert_eq!(
        hosts
            .reconcile_unconfigured_org_defaults(&app.db, "local-dev", &image)
            .await
            .expect("adopt configured default"),
        1
    );
    assert_eq!(
        hosts
            .reconcile_unconfigured_org_defaults(&app.db, "local-dev", &image)
            .await
            .expect("repeat reconciliation"),
        0
    );
    for (org_id, expected_track, expected_ref) in [
        (missing_org.id, Some("local-dev"), image.as_str()),
        (pinned_org.id, None, pinned.as_str()),
        (tracked_org.id, Some("other-track"), tracked.as_str()),
    ] {
        let policy: (Option<String>, Option<String>) = sqlx::query_as(
            "SELECT default_base_image_track, default_base_image_ref FROM orgs WHERE id = $1",
        )
        .bind(org_id)
        .fetch_one(&app.db)
        .await
        .expect("load organization image policy");
        assert_eq!(policy.0.as_deref(), expected_track);
        assert_eq!(policy.1.as_deref(), Some(expected_ref));
    }

    let response = app
        .authenticated_client(&owner)
        .post_json(
            &format!("/api/v1/workspaces/{}/resume", workspace.id),
            &json!({"operationId": Uuid::new_v4()}),
        )
        .await;
    let status = response.status();
    let body: Value = response.json().await.expect("parse resume response");
    assert_eq!(status, StatusCode::OK, "resume failed: {body}");
    assert_eq!(data(&body)["outcome"], "queued_for_capacity");
    app.cleanup().await;
}

/// A runtime call checks the VM lease once it holds the VM row. PostgreSQL
/// rechecks a row it waited for only if the row changed, so a lease that ran
/// out while the call waited for a row held without changes refuses the call,
/// and nothing reaches the guest.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn runtime_call_checks_the_lease_once_it_holds_the_vm_row() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let calls_before = runtime.received_rpc_methods().await.len();

    let holder = hold_while_the_lease_ends(
        &app,
        workspace.id,
        "SELECT 1 FROM workspace_vms WHERE workspace_id = $1 FOR UPDATE",
    )
    .await;
    let client = app.authenticated_client(&owner);
    let path = format!(
        "/api/v1/workspaces/{}/repos/{}/preview-applications?mode=unmerged",
        workspace.id, workspace.repo_id
    );
    let release = async {
        wait_for_lock_wait_past_the_lease(
            &app,
            workspace.id,
            "SELECT host_id, vm_id, epoch, status",
        )
        .await;
        holder.rollback().await.expect("release the VM row");
    };
    let (response, ()) = tokio::join!(client.get(&path), release);
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    assert!(!status.is_success(), "the call is refused: {status} {body}");
    assert_eq!(
        runtime.received_rpc_methods().await.len(),
        calls_before,
        "no call reaches the guest after its lease ended"
    );
    app.cleanup().await;
}

/// Resolving the running attachment reads the lease after its authority
/// lookup, which can wait for the workspace row: a lease that ran out during
/// that wait leaves no attachment, so terminal streams and owner commands,
/// which open on it without a runtime call, are refused.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn runtime_attachment_checks_the_lease_after_its_authority_lookup() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let _runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;

    let holder = hold_while_the_lease_ends(
        &app,
        workspace.id,
        "SELECT 1 FROM workspaces WHERE id = $1 FOR UPDATE",
    )
    .await;
    let resolve = app
        .state
        .workspace_runtime
        .owner_handle(workspace.id, owner.id);
    let release = async {
        wait_for_lock_wait_past_the_lease(
            &app,
            workspace.id,
            "SELECT vm.org_id, vm.workspace_id, workspace.owner_user_id",
        )
        .await;
        holder.rollback().await.expect("release the workspace row");
    };
    let (resolved, ()) = tokio::join!(resolve, release);
    assert!(
        matches!(
            resolved,
            Err(g5_contracts::error::G5Error::WorkspaceNotRunning)
        ),
        "an ended lease leaves no attachment: {:?}",
        resolved.map(|handle| (handle.vm_id, handle.epoch))
    );
    app.cleanup().await;
}

/// Admitting owner work holds the workspace and VM rows and then checks the
/// lease, so a lease that ran out while it waited for them refuses the work.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn owner_work_admission_checks_the_lease_once_it_holds_the_rows() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let _runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let handle = app
        .state
        .workspace_runtime
        .owner_handle(workspace.id, owner.id)
        .await
        .expect("the owner's running workspace");

    let holder = hold_while_the_lease_ends(
        &app,
        workspace.id,
        "SELECT 1 FROM workspace_vms WHERE workspace_id = $1 FOR SHARE",
    )
    .await;
    let admit = async {
        let mut db = app.scoped_db_for_org(org.id).await;
        let admitted = app
            .state
            .workspace_runtime
            .lock_owner_handle(&mut db, &handle)
            .await;
        db.rollback().await.expect("end the admission");
        admitted
    };
    let release = async {
        wait_for_lock_wait_past_the_lease(&app, workspace.id, "SELECT vm.status='running'").await;
        holder.rollback().await.expect("release the VM row");
    };
    let (admitted, ()) = tokio::join!(admit, release);
    assert!(
        matches!(
            admitted,
            Err(g5_contracts::error::G5Error::WorkspaceNotRunning)
        ),
        "an ended lease refuses owner work: {admitted:?}"
    );
    app.cleanup().await;
}

/// Starting a preview holds the workspace and VM rows and then checks the
/// lease: a lease that ran out while the start waited for the workspace row
/// refuses the preview and creates no task.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn preview_start_checks_the_lease_once_it_holds_the_vm_row() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    runtime
        .seed_file(
            workspace.repo_id,
            ".g5/workspace.yaml",
            "preview:\n  command: npm run live\n  port: 4173\n",
        )
        .await;

    // The start keeps the workspace shared and locks the VM row exclusively
    // (`g5.chat.control-locks`): a share on the VM row lets the start
    // authorize and read its configuration through the runtime (which shares
    // the row too) and holds back only its locking authority read.
    let holder = hold_while_the_lease_ends(
        &app,
        workspace.id,
        "SELECT 1 FROM workspace_vms WHERE workspace_id = $1 FOR SHARE",
    )
    .await;
    let client = app.authenticated_client(&owner);
    let path = format!("/api/v1/workspaces/{}/preview", workspace.id);
    let request = json!({
        "repoId": workspace.repo_id,
        "profile": null,
        "idempotencyKey": format!("preview-lease-{}", Uuid::new_v4()),
    });
    let start = client.post_json(&path, &request);
    let release = async {
        wait_for_lock_wait_past_the_lease(
            &app,
            workspace.id,
            "SELECT workspace.org_id, workspace.owner_user_id, workspace.is_base",
        )
        .await;
        holder.rollback().await.expect("release the VM row");
    };
    let (response, ()) = tokio::join!(start, release);
    let status = response.status();
    let body: Value = response.json().await.unwrap_or(Value::Null);
    assert!(
        !status.is_success(),
        "the start is refused: {status} {body}"
    );
    assert_eq!(body["error"]["code"], "WORKSPACE_NOT_RUNNING", "{body}");
    let previews: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM tasks WHERE workspace_id = $1 AND kind = 'preview'",
    )
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .expect("count preview tasks");
    assert_eq!(previews, 0, "a refused start creates no preview task");
    app.cleanup().await;
}

/// The committed profile list offers only what a launch accepts: launch's
/// typed parser (an unknown key refuses the file) and the shared kebab-case
/// name rule, which a launch applies too.
#[tokio::test]
async fn committed_preview_profiles_list_only_what_launch_accepts() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let mut workspace = TestWorkspace::create(&app, &org).await;
    let profile = "command: npm run web\nport: 4173\n";
    for (name, content) in [
        ("web", format!("{profile}cleanup_command: npm run down\n")),
        ("unknown-key", format!("{profile}restart: always\n")),
        ("web_client", profile.to_string()),
    ] {
        workspace
            .commit_file(&app, &format!(".g5/preview_profiles/{name}.yaml"), &content)
            .await;
    }
    let client = app.authenticated_client(&owner);
    let listed = client
        .get(&format!(
            "/api/v1/workspaces/{}/repos/{}/preview-profiles",
            workspace.id, workspace.repo_id
        ))
        .await;
    let status = listed.status();
    let listed: Value = listed.json().await.expect("parse the profile list");
    assert_eq!(status, StatusCode::OK, "{listed}");
    let names: Vec<_> = data(&listed)
        .as_array()
        .expect("a profile array")
        .iter()
        .map(|profile| profile["name"].as_str().unwrap_or_default().to_string())
        .collect();
    assert_eq!(names, ["web"], "{listed}");
    assert_eq!(
        data(&listed)[0]["cleanupCommand"],
        "npm run down",
        "the profile's cleanup command is listed: {listed}"
    );

    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    for (name, content) in [
        ("unknown-key", format!("{profile}restart: always\n")),
        ("web_client", profile.to_string()),
    ] {
        runtime
            .seed_file(
                workspace.repo_id,
                &format!(".g5/preview_profiles/{name}.yaml"),
                &content,
            )
            .await;
        let started = client
            .post_json(
                &format!("/api/v1/workspaces/{}/preview", workspace.id),
                &json!({
                    "repoId": workspace.repo_id,
                    "profile": name,
                    "idempotencyKey": format!("preview-{name}-{}", Uuid::new_v4()),
                }),
            )
            .await;
        let status = started.status();
        let body = started.text().await.expect("read the start response");
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{name}: {body}");
    }
    app.cleanup().await;
}

#[tokio::test]
async fn preview_discovery_and_start_resolve_the_same_live_config_bytes() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    runtime
        .seed_file(
            workspace.repo_id,
            ".g5/workspace.yaml",
            "preview:\n  command: npm run live\n  port: 4173\n",
        )
        .await;
    runtime
        .seed_file(
            workspace.repo_id,
            ".g5/preview_profiles/admin.yaml",
            "command: npm run admin\nport: 4174\ndescription: Admin UI\nenv:\n  API_URL: ignored-at-discovery\n",
        )
        .await;
    let client = app.authenticated_client(&owner);

    let discovered = client
        .get(&format!(
            "/api/v1/workspaces/{}/repos/{}/preview-applications?mode=unmerged",
            workspace.id, workspace.repo_id
        ))
        .await;
    let discovered_status = discovered.status();
    let discovered: Value = discovered.json().await.expect("parse discovery response");
    assert_eq!(
        discovered_status,
        StatusCode::OK,
        "discovery failed: {discovered}"
    );
    assert_eq!(data(&discovered).as_array().map(Vec::len), Some(2));
    assert!(data(&discovered).as_array().unwrap().iter().any(|item| {
        item["source"] == "workspaceDefault"
            && item["command"] == "npm run live"
            && item["port"] == 4173
    }));
    assert!(data(&discovered).as_array().unwrap().iter().any(|item| {
        item["profile"] == "admin"
            && item["description"] == "Admin UI"
            && item["envKeys"] == json!(["API_URL"])
    }));

    runtime
        .seed_file(
            workspace.repo_id,
            ".g5/workspace.yaml",
            "preview:\n  command: npm run live-next\n  port: 5173\n",
        )
        .await;
    let rediscovered = client
        .get(&format!(
            "/api/v1/workspaces/{}/repos/{}/preview-applications?mode=unmerged",
            workspace.id, workspace.repo_id
        ))
        .await;
    assert_eq!(rediscovered.status(), StatusCode::OK);
    let rediscovered: Value = rediscovered
        .json()
        .await
        .expect("parse rediscovery response");
    assert!(data(&rediscovered).as_array().unwrap().iter().any(|item| {
        item["source"] == "workspaceDefault"
            && item["command"] == "npm run live-next"
            && item["port"] == 5173
    }));
    let started = client
        .post_json(
            &format!("/api/v1/workspaces/{}/preview", workspace.id),
            &json!({
                "repoId": workspace.repo_id,
                "profile": null,
                "idempotencyKey": format!("preview-live-config-{}", Uuid::new_v4()),
            }),
        )
        .await;
    let started_status = started.status();
    let started_body = started.text().await.expect("read preview start response");
    assert_eq!(
        started_status,
        StatusCode::OK,
        "preview start failed: {started_body}"
    );
    let started: Value = serde_json::from_str(&started_body).expect("parse preview start");
    assert_eq!(data(&started)["port"], 5173);
    // Without a preview host suffix there is no isolated origin to export.
    let spec: Value = sqlx::query_scalar("SELECT spec FROM tasks WHERE id = $1::uuid")
        .bind(data(&started)["taskId"].as_str().expect("preview task id"))
        .fetch_one(&app.db)
        .await
        .expect("read preview task spec");
    assert!(spec["command"]["input"].get("origin").is_none(), "{spec}");
    // The run is named by its profile at creation (`g5.ui.activity.preview-row`).
    let name: String = sqlx::query_scalar("SELECT name FROM tasks WHERE id = $1::uuid")
        .bind(data(&started)["taskId"].as_str().expect("preview task id"))
        .fetch_one(&app.db)
        .await
        .expect("read preview task name");
    assert_eq!(name, "Preview · default profile");

    app.cleanup().await;
}

/// The preview command learns the isolated origin the browser reaches it
/// from (`g5.workspace.preview-origin-security`): the module input carries
/// the same `https://<task>.<suffix>` host the session handoff opens.
#[tokio::test]
async fn preview_start_hands_the_command_its_isolated_origin() {
    let mut config = helpers::test_config();
    config.server.external_url = "https://app.g5.example".into();
    config.server.preview_host_suffix = Some("preview.g5.example".into());
    let app = TestApp::spawn_with_config(config).await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    runtime
        .seed_file(
            workspace.repo_id,
            ".g5/preview_profiles/full-stack.yaml",
            "command: just dev-up\nport: 13000\n",
        )
        .await;
    let client = app.authenticated_client(&owner);

    let started = client
        .post_json(
            &format!("/api/v1/workspaces/{}/preview", workspace.id),
            &json!({
                "repoId": workspace.repo_id,
                "profile": "full-stack",
                "idempotencyKey": format!("preview-origin-{}", Uuid::new_v4()),
            }),
        )
        .await;
    let status = started.status();
    let started: Value = started.json().await.expect("parse preview start");
    assert_eq!(status, StatusCode::OK, "preview start failed: {started}");
    let task_id = data(&started)["taskId"].as_str().expect("preview task id");

    let (name, spec): (String, Value) =
        sqlx::query_as("SELECT name, spec FROM tasks WHERE id = $1::uuid")
            .bind(task_id)
            .fetch_one(&app.db)
            .await
            .expect("read preview task spec");
    assert_eq!(name, "Preview · full-stack");
    assert_eq!(
        spec["command"]["input"],
        json!({
            "repo_id": workspace.repo_id,
            "profile": "full-stack",
            "origin": format!("https://{task_id}.preview.g5.example"),
        })
    );

    app.cleanup().await;
}

#[tokio::test]
async fn workspace_vm_stop_then_explicit_resume_uses_the_host_control_boundary() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    workspace.attach_parent(&app).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let client = app.authenticated_client(&owner);

    let view = client
        .get(&format!("/api/v1/workspaces/{}/vm", workspace.id))
        .await;
    assert_eq!(view.status(), StatusCode::OK);
    let view: Value = view.json().await.expect("parse VM view");
    assert_eq!(data(&view)["status"], "running");
    assert_eq!(data(&view)["vmId"], runtime.vm_id().to_string());

    let operation_id = Uuid::new_v4();
    let stopped = client
        .post_json(
            &format!("/api/v1/workspaces/{}/stop", workspace.id),
            &json!({"operationId": operation_id}),
        )
        .await;
    let stopped_status = stopped.status();
    let stopped: Value = stopped.json().await.expect("parse stop outcome");
    assert_eq!(stopped_status, StatusCode::OK, "stop failed: {stopped}");
    assert_eq!(data(&stopped)["status"], "stopped");
    assert!(data(&stopped)["flushId"].is_string());
    assert_eq!(data(&stopped)["unflushedLoss"], false);

    let resumed = client
        .post_json(
            &format!("/api/v1/workspaces/{}/resume", workspace.id),
            &json!({"operationId": Uuid::new_v4()}),
        )
        .await;
    let resumed_status = resumed.status();
    let resumed: Value = resumed.json().await.expect("parse resume outcome");
    assert_eq!(resumed_status, StatusCode::OK, "resume failed: {resumed}");
    assert_eq!(data(&resumed)["outcome"], "starting");
    assert_eq!(data(&resumed)["vm"]["status"], "starting");

    let duplicate_resume = client
        .post_json(
            &format!("/api/v1/workspaces/{}/resume", workspace.id),
            &json!({"operationId": Uuid::new_v4()}),
        )
        .await;
    let duplicate_status = duplicate_resume.status();
    let duplicate_resume: Value = duplicate_resume
        .json()
        .await
        .expect("parse duplicate resume outcome");
    assert_eq!(
        duplicate_status,
        StatusCode::OK,
        "duplicate resume failed: {duplicate_resume}"
    );
    assert_eq!(data(&duplicate_resume)["outcome"], "starting");
    assert_eq!(
        data(&duplicate_resume)["vm"]["vmId"],
        data(&resumed)["vm"]["vmId"]
    );

    let commands = runtime.received_host_command_methods().await;
    assert!(commands.iter().any(|method| method == "vm.stop"));
    assert!(commands.iter().any(|method| method == "vm.start"));

    app.cleanup().await;
}

#[tokio::test]
async fn busy_live_fork_can_escalate_the_same_operation_to_interrupt() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    sqlx::query(
        "INSERT INTO tasks(
             id, org_id, workspace_id, name, kind, status, spec, attempt, created_by)
         VALUES ($1,$2,$3,'Busy command','command','queued','{}'::jsonb,1,$4)",
    )
    .bind(Uuid::new_v4())
    .bind(org.id)
    .bind(workspace.id)
    .bind(owner.id)
    .execute(&app.db)
    .await
    .expect("insert busy source work");
    let client = app.authenticated_client(&owner);
    let name = format!("live-fork-{}", Uuid::new_v4());
    let idempotency_key = format!("live-fork-request-{}", Uuid::new_v4());
    let first = client
        .post_json(
            &format!("/api/v1/workspaces/{}/clone", workspace.id),
            &json!({
                "name": name,
                "idempotencyKey": idempotency_key,
                "source": "live",
                "interrupt": false,
            }),
        )
        .await;
    assert_eq!(first.status(), StatusCode::CONFLICT);
    let pending: (Uuid, String, bool) = sqlx::query_as(
        "SELECT id, status,
                (repo_manifest -> 0 ->> 'live_interrupt')::boolean
         FROM workspace_forks WHERE idempotency_key = $1",
    )
    .bind(&idempotency_key)
    .fetch_one(&app.db)
    .await
    .expect("load pending live fork");
    assert_eq!(pending.1, "pending");
    assert!(!pending.2);

    let retry = client
        .post_json(
            &format!("/api/v1/workspaces/{}/clone", workspace.id),
            &json!({
                "name": name,
                "idempotencyKey": idempotency_key,
                "source": "live",
                "interrupt": true,
            }),
        )
        .await;
    let retry_status = retry.status();
    let retry_body: Value = retry.json().await.expect("parse live fork retry");
    assert_eq!(retry_status, StatusCode::CREATED, "{retry_body}");
    let completed: (Uuid, String, bool) = sqlx::query_as(
        "SELECT id, status,
                (repo_manifest -> 0 ->> 'live_interrupt')::boolean
         FROM workspace_forks WHERE idempotency_key = $1",
    )
    .bind(&idempotency_key)
    .fetch_one(&app.db)
    .await
    .expect("load completed live fork");
    assert_eq!(completed.0, pending.0, "retry must reuse the same claim");
    assert_eq!(completed.1, "completed");
    assert!(completed.2);
    let methods = runtime.received_rpc_methods().await;
    assert!(methods.iter().any(|method| method == "admission.close"));
    assert!(methods.iter().any(|method| method == "workspace.flush"));
    assert_eq!(methods.last().map(String::as_str), Some("admission.open"));

    app.cleanup().await;
}

#[tokio::test]
async fn concurrent_lost_key_retries_capture_one_pending_live_fork_once() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let unrelated_source = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach_with_rpc_barrier(
        &app,
        &workspace,
        owner.id,
        "admission.close",
    )
    .await;
    let task_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO tasks(
             id, org_id, workspace_id, name, kind, status, spec, attempt, created_by)
         VALUES ($1,$2,$3,'Busy command','command','queued','{}'::jsonb,1,$4)",
    )
    .bind(task_id)
    .bind(org.id)
    .bind(workspace.id)
    .bind(owner.id)
    .execute(&app.db)
    .await
    .expect("make initial live capture retryable");

    let name = format!("live-retry-{}", Uuid::new_v4());
    let path = format!("/api/v1/workspaces/{}/clone", workspace.id);
    let first = app
        .authenticated_client(&owner)
        .post_json(
            &path,
            &json!({"name": name, "idempotencyKey": "original-live-key", "source": "live"}),
        )
        .await;
    assert_eq!(first.status(), StatusCode::CONFLICT);
    sqlx::query("DELETE FROM tasks WHERE id = $1")
        .bind(task_id)
        .execute(&app.db)
        .await
        .expect("source becomes idle before retries");

    let operation_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM workspace_forks WHERE org_id = $1 AND proposed_name = $2",
    )
    .bind(org.id)
    .bind(&name)
    .fetch_one(&app.db)
    .await
    .expect("load pending fork authority");
    let request_url = format!("{}{path}", app.base_url);
    let first_client = app.client.clone();
    let first_token = owner.jwt.clone();
    let first_name = name.clone();
    let first_url = request_url.clone();
    let first_retry = tokio::spawn(async move {
        first_client
            .post(first_url)
            .bearer_auth(first_token)
            .json(&json!({"name": first_name, "idempotencyKey": "lost-key-a", "source": "live"}))
            .send()
            .await
            .expect("send first lost-key retry")
    });
    tokio::time::timeout(Duration::from_secs(10), async {
        while !runtime
            .received_rpc_methods()
            .await
            .iter()
            .any(|method| method == "admission.close")
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("first retry reaches live source capture");

    let second_client = app.client.clone();
    let second_token = owner.jwt.clone();
    let second_name = name.clone();
    let second_retry = tokio::spawn(async move {
        second_client
            .post(request_url)
            .bearer_auth(second_token)
            .json(&json!({"name": second_name, "idempotencyKey": "lost-key-b", "source": "live"}))
            .send()
            .await
            .expect("send second lost-key retry")
    });
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if app
                .state
                .workspace_fork_service
                .local_resume_waiters(operation_id)
                > 0
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("second retry must wait before calling the dispatcher");
    let mut extra_retries = Vec::new();
    for ordinal in 0..3 {
        let retry_client = app.client.clone();
        let retry_url = format!("{}/api/v1/workspaces/{}/clone", app.base_url, workspace.id);
        let retry_token = owner.jwt.clone();
        let retry_name = name.clone();
        extra_retries.push(tokio::spawn(async move {
            retry_client
                .post(retry_url)
                .bearer_auth(retry_token)
                .json(&json!({
                    "name": retry_name,
                    "idempotencyKey": format!("lost-key-extra-{ordinal}"),
                    "source": "live"
                }))
                .send()
                .await
                .expect("send additional lost-key retry")
        }));
    }
    tokio::time::timeout(Duration::from_secs(10), async {
        while app
            .state
            .workspace_fork_service
            .local_resume_waiters(operation_id)
            < 4
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("all four duplicate retries wait locally without consuming database sessions");
    let unrelated_name = format!("unrelated-durable-{}", Uuid::new_v4());
    let unrelated_client = app.authenticated_client(&owner);
    let unrelated = tokio::time::timeout(
        Duration::from_secs(10),
        unrelated_client.post_json(
            &format!("/api/v1/workspaces/{}/clone", unrelated_source.id),
            &json!({
                "name": unrelated_name,
                "idempotencyKey": format!("unrelated-durable-key-{}", Uuid::new_v4()),
                "source": "durable"
            }),
        ),
    )
    .await
    .expect("unrelated fork must keep capacity during retry burst");
    let unrelated_status = unrelated.status();
    let unrelated_body: Value = unrelated.json().await.expect("parse unrelated fork");
    assert_eq!(unrelated_status, StatusCode::CREATED, "{unrelated_body}");
    assert_eq!(
        runtime
            .received_rpc_methods()
            .await
            .iter()
            .filter(|method| *method == "admission.close")
            .count(),
        1,
        "only the active resumer may close admission"
    );
    runtime.release_rpc_barrier();

    let first_response = first_retry.await.expect("join first retry");
    let second_response = second_retry.await.expect("join second retry");
    let first_status = first_response.status();
    let second_status = second_response.status();
    let first_body: Value = first_response
        .json()
        .await
        .expect("parse first fork response");
    let second_body: Value = second_response
        .json()
        .await
        .expect("parse second fork response");
    assert_eq!(first_status, StatusCode::CREATED, "{first_body}");
    assert_eq!(second_status, StatusCode::CREATED, "{second_body}");
    assert_eq!(
        first_body["data"]["workspace"]["id"],
        second_body["data"]["workspace"]["id"]
    );
    for retry in extra_retries {
        let response = retry.await.expect("join additional retry");
        let status = response.status();
        let body: Value = response.json().await.expect("parse additional retry");
        assert_eq!(status, StatusCode::CREATED, "{body}");
        assert_eq!(
            body["data"]["workspace"]["id"],
            first_body["data"]["workspace"]["id"]
        );
    }
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT status FROM workspace_forks WHERE id = $1")
            .bind(operation_id)
            .fetch_one(&app.db)
            .await
            .expect("load final fork status"),
        "completed"
    );
    let methods = runtime.received_rpc_methods().await;
    for method in ["admission.close", "workspace.flush", "admission.open"] {
        assert_eq!(
            methods
                .iter()
                .filter(|candidate| *candidate == method)
                .count(),
            1,
            "{method} must run once per durable operation: {methods:?}"
        );
    }
    app.cleanup().await;
}

#[tokio::test]
async fn cancelled_live_fork_caller_leaves_one_resumer_to_finish_before_retry() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach_with_rpc_barrier(
        &app,
        &workspace,
        owner.id,
        "admission.close",
    )
    .await;
    let task_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO tasks(
             id, org_id, workspace_id, name, kind, status, spec, attempt, created_by)
         VALUES ($1,$2,$3,'Busy command','command','queued','{}'::jsonb,1,$4)",
    )
    .bind(task_id)
    .bind(org.id)
    .bind(workspace.id)
    .bind(owner.id)
    .execute(&app.db)
    .await
    .expect("make the first live attempt pending");
    let name = format!("cancelled-live-retry-{}", Uuid::new_v4());
    let initial = app
        .authenticated_client(&owner)
        .post_json(
            &format!("/api/v1/workspaces/{}/clone", workspace.id),
            &json!({"name": name, "idempotencyKey": "cancelled-live-original", "source": "live"}),
        )
        .await;
    assert_eq!(initial.status(), StatusCode::CONFLICT);
    sqlx::query("DELETE FROM tasks WHERE id = $1")
        .bind(task_id)
        .execute(&app.db)
        .await
        .expect("make the source idle for retry");
    let operation_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM workspace_forks WHERE org_id = $1 AND proposed_name = $2",
    )
    .bind(org.id)
    .bind(&name)
    .fetch_one(&app.db)
    .await
    .expect("load cancelled-call fork operation");

    let scoped_db = app.scoped_db_for_user_org(owner.id, org.id).await;
    let scope = scoped_db.reopener();
    scoped_db
        .rollback()
        .await
        .expect("close fork scope capture");
    let service = app.state.workspace_fork_service.scoped(scope);
    let request = |key: &str| ForkWorkspaceRequest {
        name: name.clone(),
        description: None,
        idempotency_key: key.to_string(),
        read_only: false,
        merge_target_workspace_id: None,
        chat_session_ids: Vec::new(),
        source: ForkSource::Live,
        interrupt: false,
        issue: None,
        start_chat: None,
        uniquify_name: false,
        reverification: None,
    };
    let first_service = service.clone();
    let first_request = request("cancelled-live-a");
    let org_id = org.id;
    let workspace_id = workspace.id;
    let owner_id = owner.id;
    let first = tokio::spawn(async move {
        first_service
            .fork_workspace(org_id, workspace_id, owner_id, first_request)
            .await
    });
    tokio::time::timeout(Duration::from_secs(10), async {
        while !runtime
            .received_rpc_methods()
            .await
            .iter()
            .any(|method| method == "admission.close")
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("first resumer reaches dispatcher close");
    first.abort();
    assert!(first
        .await
        .expect_err("cancelled caller has no reply")
        .is_cancelled());

    let second_request = request("cancelled-live-b");
    let second = tokio::spawn(async move {
        service
            .fork_workspace(org_id, workspace_id, owner_id, second_request)
            .await
    });
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if app
                .state
                .workspace_fork_service
                .local_resume_waiters(operation_id)
                > 0
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("cancelled caller must leave the active resumer holding the lock");
    assert_eq!(
        runtime
            .received_rpc_methods()
            .await
            .iter()
            .filter(|method| *method == "admission.close")
            .count(),
        1
    );
    runtime.release_rpc_barrier();

    let completed = second
        .await
        .expect("join surviving retry")
        .expect("surviving retry returns the completed fork");
    let operation: (String, Option<Uuid>) = sqlx::query_as(
        "SELECT status, target_workspace_id FROM workspace_forks \
         WHERE org_id = $1 AND proposed_name = $2",
    )
    .bind(org.id)
    .bind(&name)
    .fetch_one(&app.db)
    .await
    .expect("load completed fork after cancellation");
    assert_eq!(operation.0, "completed");
    assert_eq!(operation.1, Some(completed.workspace.id));
    let methods = runtime.received_rpc_methods().await;
    for method in ["admission.close", "workspace.flush", "admission.open"] {
        assert_eq!(
            methods
                .iter()
                .filter(|candidate| *candidate == method)
                .count(),
            1,
            "cancelled caller must not cause a second {method}: {methods:?}"
        );
    }
    app.cleanup().await;
}

#[tokio::test]
async fn fork_lock_timeout_and_caller_cancellation_keep_backend_capacity_bounded() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let task_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO tasks(
             id, org_id, workspace_id, name, kind, status, spec, attempt, created_by)
         VALUES ($1,$2,$3,'Busy command','command','queued','{}'::jsonb,1,$4)",
    )
    .bind(task_id)
    .bind(org.id)
    .bind(workspace.id)
    .bind(owner.id)
    .execute(&app.db)
    .await
    .expect("make original live fork pending");
    let name = format!("live-lock-timeout-{}", Uuid::new_v4());
    let initial = app
        .authenticated_client(&owner)
        .post_json(
            &format!("/api/v1/workspaces/{}/clone", workspace.id),
            &json!({"name": name, "idempotencyKey": "live-lock-original", "source": "live"}),
        )
        .await;
    assert_eq!(initial.status(), StatusCode::CONFLICT);
    sqlx::query("DELETE FROM tasks WHERE id = $1")
        .bind(task_id)
        .execute(&app.db)
        .await
        .expect("make source idle before lock retries");
    let operation_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM workspace_forks WHERE org_id = $1 AND proposed_name = $2",
    )
    .bind(org.id)
    .bind(&name)
    .fetch_one(&app.db)
    .await
    .expect("load pending fork operation");
    let scoped_db = app.scoped_db_for_user_org(owner.id, org.id).await;
    let scope = scoped_db.reopener();
    scoped_db
        .rollback()
        .await
        .expect("close fork scope capture");
    let service = app.state.workspace_fork_service.scoped(scope);
    let request = |key: &str| ForkWorkspaceRequest {
        name: name.clone(),
        description: None,
        idempotency_key: key.to_string(),
        read_only: false,
        merge_target_workspace_id: None,
        chat_session_ids: Vec::new(),
        source: ForkSource::Live,
        interrupt: false,
        issue: None,
        start_chat: None,
        uniquify_name: false,
        reverification: None,
    };
    let mut fence = app.db.begin().await.expect("begin fork advisory fence");
    sqlx::query(
        "SELECT pg_advisory_xact_lock(\
             hashtextextended('g5:workspace-fork-resume:' || $1::text, 0)\
         )",
    )
    .bind(operation_id)
    .execute(&mut *fence)
    .await
    .expect("hold pending operation's advisory key");

    let short_service = service.with_resume_limits_for_test(1, Duration::from_millis(200));
    let timed_out = tokio::time::timeout(
        Duration::from_secs(5),
        short_service.fork_workspace(org.id, workspace.id, owner.id, request("lock-timeout")),
    )
    .await
    .expect("lock wait must be bounded")
    .expect_err("blocked fork should be retryable");
    assert_eq!(timed_out.inner.error_code(), "SERVICE_UNAVAILABLE");

    let count_waiters = || async {
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM pg_stat_activity
             WHERE datname = current_database()
               AND wait_event_type = 'Lock'
               AND wait_event = 'advisory'
               AND query LIKE '%g5:workspace-fork-resume:%'",
        )
        .fetch_one(&app.db)
        .await
        .expect("count advisory waiters for the pending operation")
    };
    tokio::time::timeout(Duration::from_secs(10), async {
        while count_waiters().await != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("timed-out request must release its waiting backend");

    let long_service = service.with_resume_limits_for_test(1, Duration::from_secs(10));
    let cancelled_service = long_service.clone();
    let cancel_request = request("lock-cancelled");
    let org_id = org.id;
    let workspace_id = workspace.id;
    let owner_id = owner.id;
    let cancelled = tokio::spawn(async move {
        cancelled_service
            .fork_workspace(org_id, workspace_id, owner_id, cancel_request)
            .await
    });
    tokio::time::timeout(Duration::from_secs(10), async {
        while count_waiters().await == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("cancelled caller reaches the database advisory wait");
    cancelled.abort();
    assert!(cancelled
        .await
        .expect_err("caller was aborted")
        .is_cancelled());
    assert_eq!(
        count_waiters().await,
        1,
        "the orphaned task must retain its backend and semaphore permit"
    );
    let short_shared_service = long_service.with_resume_wait_for_test(Duration::from_millis(200));
    let same_operation = short_shared_service
        .fork_workspace(
            org.id,
            workspace.id,
            owner.id,
            request("same-operation-while-orphaned"),
        )
        .await
        .expect_err("same-operation retry must have a bounded local wait");
    assert_eq!(same_operation.inner.error_code(), "SERVICE_UNAVAILABLE");
    let other_name = format!("while-fork-waits-{}", Uuid::new_v4());
    let other_operation = short_shared_service
        .fork_workspace(
            org.id,
            workspace.id,
            owner.id,
            ForkWorkspaceRequest {
                name: other_name.clone(),
                description: None,
                idempotency_key: "other-operation-slot-wait".to_string(),
                read_only: false,
                merge_target_workspace_id: None,
                chat_session_ids: Vec::new(),
                source: ForkSource::Durable,
                interrupt: false,
                issue: None,
                start_chat: None,
                uniquify_name: false,
                reverification: None,
            },
        )
        .await
        .expect_err("distinct operation must not exceed the one-backend cap");
    assert_eq!(other_operation.inner.error_code(), "SERVICE_UNAVAILABLE");
    assert_eq!(
        count_waiters().await,
        1,
        "a cancelled caller and another operation must not create another backend waiter"
    );
    assert!(runtime.received_rpc_methods().await.is_empty());
    fence.commit().await.expect("release pending fork fence");

    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let status: String =
                sqlx::query_scalar("SELECT status FROM workspace_forks WHERE id = $1")
                    .bind(operation_id)
                    .fetch_one(&app.db)
                    .await
                    .expect("load orphaned fork status");
            if status == "completed" {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("orphaned lock waiter must complete after the fence releases");

    let resumed = long_service
        .fork_workspace(
            org.id,
            workspace.id,
            owner.id,
            request("live-lock-original"),
        )
        .await
        .expect("retry observes the orphaned operation's completed result");
    assert_eq!(resumed.workspace.name, name);
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT status FROM workspace_forks WHERE id = $1")
            .bind(operation_id)
            .fetch_one(&app.db)
            .await
            .expect("load completed retry"),
        "completed"
    );
    let methods = runtime.received_rpc_methods().await;
    for method in ["admission.close", "workspace.flush", "admission.open"] {
        assert_eq!(
            methods
                .iter()
                .filter(|candidate| *candidate == method)
                .count(),
            1,
            "orphaned attempt must drive one {method}: {methods:?}"
        );
    }
    tokio::time::timeout(Duration::from_secs(10), async {
        while count_waiters().await != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("completed orphan must release its advisory lock backend");
    app.cleanup().await;
}

#[tokio::test]
async fn distinct_live_forks_of_one_source_capture_serially() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach_with_rpc_barrier(
        &app,
        &workspace,
        owner.id,
        "admission.close",
    )
    .await;
    let source_id = workspace.id;
    let first_name = format!("first-live-source-{}", Uuid::new_v4());
    let second_name = format!("second-live-source-{}", Uuid::new_v4());
    let url = format!("{}/api/v1/workspaces/{source_id}/clone", app.base_url);
    let first_client = app.client.clone();
    let first_url = url.clone();
    let first_token = owner.jwt.clone();
    let first = tokio::spawn(async move {
        first_client
            .post(first_url)
            .bearer_auth(first_token)
            .json(&json!({
                "name": first_name,
                "idempotencyKey": format!("first-live-source-key-{}", Uuid::new_v4()),
                "source": "live"
            }))
            .send()
            .await
            .expect("send first live fork")
    });
    tokio::time::timeout(Duration::from_secs(10), async {
        while !runtime
            .received_rpc_methods()
            .await
            .iter()
            .any(|method| method == "admission.close")
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("first live fork reaches dispatcher close");

    let second_client = app.client.clone();
    let second_token = owner.jwt.clone();
    let second = tokio::spawn(async move {
        second_client
            .post(url)
            .bearer_auth(second_token)
            .json(&json!({
                "name": second_name,
                "idempotencyKey": format!("second-live-source-key-{}", Uuid::new_v4()),
                "source": "live"
            }))
            .send()
            .await
            .expect("send second live fork")
    });
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let waiting: bool = sqlx::query_scalar(
                "SELECT EXISTS (
                     SELECT 1 FROM pg_stat_activity
                     WHERE datname = current_database()
                       AND wait_event_type = 'Lock'
                       AND wait_event = 'advisory'
                       AND query LIKE '%g5:workspace-live-fork-source:%')",
            )
            .fetch_one(&app.db)
            .await
            .expect("observe second fork waiting for source capture");
            if waiting {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("two operations of one source must not overlap at the dispatcher");
    assert_eq!(
        runtime
            .received_rpc_methods()
            .await
            .iter()
            .filter(|method| *method == "admission.close")
            .count(),
        1
    );
    runtime.release_rpc_barrier();

    let first_response = first.await.expect("join first live fork");
    let second_response = second.await.expect("join second live fork");
    let first_status = first_response.status();
    let second_status = second_response.status();
    let first_body: Value = first_response.json().await.expect("parse first live fork");
    let second_body: Value = second_response
        .json()
        .await
        .expect("parse second live fork");
    assert_eq!(first_status, StatusCode::CREATED, "{first_body}");
    assert_eq!(second_status, StatusCode::CREATED, "{second_body}");
    assert_ne!(
        first_body["data"]["workspace"]["id"],
        second_body["data"]["workspace"]["id"]
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM workspace_forks \
             WHERE org_id = $1 AND source_workspace_id = $2 AND status = 'completed'",
        )
        .bind(org.id)
        .bind(source_id)
        .fetch_one(&app.db)
        .await
        .expect("count completed source forks"),
        2
    );
    let methods = runtime.received_rpc_methods().await;
    for method in ["admission.close", "workspace.flush", "admission.open"] {
        assert_eq!(
            methods
                .iter()
                .filter(|candidate| *candidate == method)
                .count(),
            2,
            "each distinct fork performs one {method}: {methods:?}"
        );
    }
    assert_eq!(
        methods,
        [
            "admission.close",
            "workspace.flush",
            "admission.open",
            "admission.close",
            "workspace.flush",
            "admission.open",
        ],
        "the second fork must not close admission until the first has reopened it"
    );
    app.cleanup().await;
}

#[tokio::test]
async fn workspace_stop_replays_cancelled_task_outcome_before_releasing_the_vm() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let task_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO tasks(
             id, org_id, workspace_id, name, kind, status, spec,
             attempt, vm_id, epoch, start_confirmed, started_at, created_by
         ) VALUES (
             $1,$2,$3,'Running command','command','running',
             jsonb_build_object('command',jsonb_build_object(
                 'schema_version','g5.command-task-spec/v1',
                 'module_id','workspace.bootstrap','module_version','1.0.0',
                 'operation','run','input_schema','g5.workspace.bootstrap.run/v1',
                 'input','{}'::jsonb)),
             1,$4,1,true,now(),$5)",
    )
    .bind(task_id)
    .bind(org.id)
    .bind(workspace.id)
    .bind(runtime.vm_id().to_string())
    .bind(owner.id)
    .execute(&app.db)
    .await
    .expect("insert running command");
    sqlx::query(
        "INSERT INTO task_attempts(
             org_id, task_id, attempt, vm_id, epoch, start_confirmed, started_at
         ) VALUES ($1,$2,1,$3,1,true,now())",
    )
    .bind(org.id)
    .bind(task_id)
    .bind(runtime.vm_id().to_string())
    .execute(&app.db)
    .await
    .expect("insert running command attempt");

    let response = app
        .authenticated_client(&owner)
        .post_json(
            &format!("/api/v1/workspaces/{}/stop", workspace.id),
            &json!({"operationId":Uuid::new_v4()}),
        )
        .await;
    let status = response.status();
    let body: Value = response.json().await.expect("parse stop response");
    assert_eq!(status, StatusCode::OK, "stop failed: {body}");

    let settled: (
        String,
        Option<chrono::DateTime<chrono::Utc>>,
        Option<String>,
    ) = sqlx::query_as(
        "SELECT task.status, attempt.ended_at, attempt.reason
             FROM tasks AS task
             JOIN task_attempts AS attempt
               ON attempt.task_id = task.id AND attempt.attempt = task.attempt
             WHERE task.id = $1",
    )
    .bind(task_id)
    .fetch_one(&app.db)
    .await
    .expect("load task settled during stop");
    assert_eq!(settled.0, "cancelled");
    assert!(settled.1.is_some(), "attempt must settle before VM release");
    assert_eq!(settled.2.as_deref(), Some("cancelled"));

    let methods = runtime.received_rpc_methods().await;
    assert!(methods.iter().any(|method| method == "modules.cancel"));
    assert!(methods.iter().any(|method| method == "events.replay"));
    assert!(methods.iter().any(|method| method == "events.ack"));

    app.cleanup().await;
}

async fn assert_unstarted_agent_does_not_block_stop(
    reason: g5_api::services::workspace_vm_service::StopReason,
) {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime =
        TestWorkspaceRuntimePeer::attach_with_unstarted_cancellation(&app, &workspace, owner.id)
            .await;
    let session_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO chat_sessions(
             id, org_id, workspace_id, scope_type, title, created_by
         ) VALUES ($1,$2,$3,'workspace','Unstarted stop',$4)",
    )
    .bind(session_id)
    .bind(org.id)
    .bind(workspace.id)
    .bind(owner.id)
    .execute(&app.db)
    .await
    .expect("insert unstarted task session");
    let task_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO tasks(
             id, org_id, workspace_id, name, kind, status, spec, attempt,
             vm_id, epoch, start_confirmed, started_at, chat_session_id, created_by
         ) VALUES (
             $1,$2,$3,'Reserved turn','agent','running',
             jsonb_build_object('agent',jsonb_build_object(
                 'schemaVersion','g5.agent-task-spec/v1',
                 'agentLaunch',jsonb_build_object('schemaVersion',1))),
             1,$4,1,false,now(),$5,$6)",
    )
    .bind(task_id)
    .bind(org.id)
    .bind(workspace.id)
    .bind(runtime.vm_id().to_string())
    .bind(session_id)
    .bind(owner.id)
    .execute(&app.db)
    .await
    .expect("insert unstarted running task");
    sqlx::query(
        "INSERT INTO task_attempts(
             org_id, task_id, attempt, vm_id, epoch, start_confirmed, started_at
         ) VALUES ($1,$2,1,$3,1,false,now())",
    )
    .bind(org.id)
    .bind(task_id)
    .bind(runtime.vm_id().to_string())
    .execute(&app.db)
    .await
    .expect("insert unstarted task attempt");
    let input_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO session_inputs(
             id, org_id, session_id, input_index, content, input_origin, delivery,
             reservation_id, reservation_revision, task_id, attempt
         ) VALUES ($1,$2,$3,0,'Reserved but not accepted','human','reserved',
                   $4,1,$5,1)",
    )
    .bind(input_id)
    .bind(org.id)
    .bind(session_id)
    .bind(Uuid::new_v4())
    .bind(task_id)
    .execute(&app.db)
    .await
    .expect("insert reserved input");

    let deadline_at = (reason == g5_api::services::workspace_vm_service::StopReason::Delete)
        .then(chrono::Utc::now);
    let outcome = workspace_vm_service(&app, owner.id, org.id)
        .await
        .stop(workspace.id, owner.id, Uuid::new_v4(), reason, deadline_at)
        .await
        .expect("unstarted task must settle without wedging the stop");
    assert_eq!(outcome.status, "stopped");
    if reason == g5_api::services::workspace_vm_service::StopReason::Delete {
        // A force Delete has no save window: it calls the guest no more (the
        // dispatcher refuses a request whose deadline has passed) and kills
        // the VM, and the attempt ends as a lost VM's would
        // (`g5.lifecycle.stop.force`).
        assert!(runtime.received_rpc_methods().await.is_empty());
        assert_killed(&vm_stop_commands(&runtime).await);
        g5_api_core::maintenance::agent_tasks::run_once(&app.state)
            .await
            .expect("reconcile agent tasks");
        let (status, ended): (String, Option<chrono::DateTime<chrono::Utc>>) = sqlx::query_as(
            "SELECT task.status, attempt.ended_at
             FROM tasks AS task
             JOIN task_attempts AS attempt
               ON attempt.task_id = task.id AND attempt.attempt = task.attempt
             WHERE task.id = $1",
        )
        .bind(task_id)
        .fetch_one(&app.db)
        .await
        .expect("load unstarted task after a force Delete");
        assert_ne!(status, "running");
        assert!(ended.is_some());
        app.cleanup().await;
        return;
    }
    let settled: (
        String,
        Option<String>,
        String,
        Option<chrono::DateTime<chrono::Utc>>,
    ) = sqlx::query_as(
        "SELECT task.status, task.hold_reason, input.delivery, attempt.ended_at
             FROM tasks AS task
             JOIN session_inputs AS input ON input.id = $2
             JOIN task_attempts AS attempt
               ON attempt.task_id = task.id AND attempt.attempt = task.attempt
             WHERE task.id = $1",
    )
    .bind(task_id)
    .bind(input_id)
    .fetch_one(&app.db)
    .await
    .expect("load unstarted task settlement");
    assert_eq!(settled.0, "waiting");
    assert_eq!(settled.1.as_deref(), Some("owner_stop"));
    assert_eq!(settled.2, "queued");
    assert!(settled.3.is_some());

    app.cleanup().await;
}

#[tokio::test]
async fn user_stop_settles_a_reserved_but_never_invoked_agent() {
    assert_unstarted_agent_does_not_block_stop(
        g5_api::services::workspace_vm_service::StopReason::User,
    )
    .await;
}

#[tokio::test]
async fn delete_stop_settles_a_reserved_but_never_invoked_agent() {
    assert_unstarted_agent_does_not_block_stop(
        g5_api::services::workspace_vm_service::StopReason::Delete,
    )
    .await;
}

/// Owner Stop settles a running turn by replaying its journal. A tool result
/// carrying U+0000 (a command that printed a binary file) is stored with
/// U+FFFD, so the replay reaches the outcome and the VM stops, instead of
/// failing `AGENT_STOP_REPLAY_COMMIT_FAILED` on every retry until the stop
/// deadline aborts it (QA 2026-10-06).
#[tokio::test]
async fn user_stop_settles_a_turn_whose_journal_carries_nul() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let elf = "\u{7f}ELF\u{2}\u{1}\u{1}\0\0\0";
    let runtime = TestWorkspaceRuntimePeer::attach_with_journal_events(
        &app,
        &workspace,
        owner.id,
        vec![
            json!({
                "seq": 1,
                "kind": "module.tool.completed",
                "payload": {
                    "call_id": "toolu_1", "name": "Bash", "status": "ok",
                    "output": elf, "content": elf, "output_truncated": false,
                },
            }),
            json!({
                "seq": 2,
                "kind": "invocation.outcome",
                "payload": {
                    "status": "succeeded",
                    "result": {"status": "succeeded", "code": "exit_code", "provider_started": true},
                },
                "terminal": true,
            }),
        ],
    )
    .await;
    let session_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO chat_sessions(id, org_id, workspace_id, scope_type, title, created_by)
         VALUES ($1,$2,$3,'workspace','Binary output',$4)",
    )
    .bind(session_id)
    .bind(org.id)
    .bind(workspace.id)
    .bind(owner.id)
    .execute(&app.db)
    .await
    .expect("insert session");
    let task_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO tasks(
             id, org_id, workspace_id, name, kind, status, spec, attempt,
             vm_id, epoch, start_confirmed, started_at, chat_session_id, created_by
         ) VALUES ($1,$2,$3,'Binary output turn','agent','running','{}'::jsonb,1,
                   $4,1,true,now(),$5,$6)",
    )
    .bind(task_id)
    .bind(org.id)
    .bind(workspace.id)
    .bind(runtime.vm_id().to_string())
    .bind(session_id)
    .bind(owner.id)
    .execute(&app.db)
    .await
    .expect("insert running agent task");
    sqlx::query(
        "INSERT INTO task_attempts(
             org_id, task_id, attempt, vm_id, epoch, start_confirmed, started_at
         ) VALUES ($1,$2,1,$3,1,true,now())",
    )
    .bind(org.id)
    .bind(task_id)
    .bind(runtime.vm_id().to_string())
    .execute(&app.db)
    .await
    .expect("insert running agent attempt");

    let outcome = workspace_vm_service(&app, owner.id, org.id)
        .await
        .stop(
            workspace.id,
            owner.id,
            Uuid::new_v4(),
            g5_api::services::workspace_vm_service::StopReason::User,
            None,
        )
        .await
        .expect("the journal replay must not wedge the stop");
    assert_eq!(outcome.status, "stopped");
    let (acked, ended): (i64, bool) = sqlx::query_as(
        "SELECT acked_event_seq, ended_at IS NOT NULL
         FROM task_attempts WHERE task_id = $1 AND attempt = 1",
    )
    .bind(task_id)
    .fetch_one(&app.db)
    .await
    .expect("load the attempt");
    assert_eq!(acked, 2, "the stop stored the whole journal");
    assert!(ended);
    let content: String =
        sqlx::query_scalar("SELECT content FROM task_events WHERE task_id = $1 AND seq = 1")
            .bind(task_id)
            .fetch_one(&app.db)
            .await
            .expect("load the tool result");
    assert_eq!(content, "\u{7f}ELF\u{2}\u{1}\u{1}\u{FFFD}\u{FFFD}\u{FFFD}");

    app.cleanup().await;
}

/// H2 at the stop boundary: a journal carrying a provider request id reuse
/// must not wedge `settle_stopped_execution` on every retry until the save
/// window expires. The rejected fact becomes consumable error evidence, the
/// terminal outcome settles the attempt, and the healthy capture still saves.
#[tokio::test]
async fn user_stop_settles_a_turn_past_a_rejected_provider_event_and_saves() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let deadline = chrono::Utc::now().timestamp_millis() + 60_000;
    let open = |seq: u64| {
        json!({
            "seq": seq,
            "kind": "module.provider_request.opened",
            "payload": {
                "provider_request_id": "req-dup", "request_kind": "permission",
                "tool_name": "Bash", "payload": {"command":"true"},
                "display": {"tool_name":"Bash"}, "deadline_unix_ms": deadline,
            },
        })
    };
    let runtime = TestWorkspaceRuntimePeer::attach_with_journal_events(
        &app,
        &workspace,
        owner.id,
        vec![
            open(1),
            open(2),
            json!({
                "seq": 3,
                "kind": "invocation.outcome",
                "payload": {
                    "status": "succeeded",
                    "result": {"status": "succeeded", "code": "exit_code", "provider_started": true},
                },
                "terminal": true,
            }),
        ],
    )
    .await;
    let session_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO chat_sessions(id, org_id, workspace_id, scope_type, title, created_by)
         VALUES ($1,$2,$3,'workspace','Reused request id',$4)",
    )
    .bind(session_id)
    .bind(org.id)
    .bind(workspace.id)
    .bind(owner.id)
    .execute(&app.db)
    .await
    .expect("insert session");
    let task_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO tasks(
             id, org_id, workspace_id, name, kind, status, spec, attempt,
             vm_id, epoch, start_confirmed, started_at, chat_session_id, created_by
         ) VALUES ($1,$2,$3,'Reused request id turn','agent','running','{}'::jsonb,1,
                   $4,1,true,now(),$5,$6)",
    )
    .bind(task_id)
    .bind(org.id)
    .bind(workspace.id)
    .bind(runtime.vm_id().to_string())
    .bind(session_id)
    .bind(owner.id)
    .execute(&app.db)
    .await
    .expect("insert running agent task");
    sqlx::query(
        "INSERT INTO task_attempts(
             org_id, task_id, attempt, vm_id, epoch, start_confirmed, started_at
         ) VALUES ($1,$2,1,$3,1,true,now())",
    )
    .bind(org.id)
    .bind(task_id)
    .bind(runtime.vm_id().to_string())
    .execute(&app.db)
    .await
    .expect("insert running agent attempt");

    let outcome = workspace_vm_service(&app, owner.id, org.id)
        .await
        .stop(
            workspace.id,
            owner.id,
            Uuid::new_v4(),
            g5_api::services::workspace_vm_service::StopReason::User,
            None,
        )
        .await
        .expect("a rejected provider fact must not wedge the stop");
    assert_eq!(outcome.status, "stopped");
    assert!(
        outcome.flush_id.is_some(),
        "healthy capture still saves the workspace"
    );
    assert!(!outcome.unflushed_loss, "no work may be recorded as lost");
    let (acked, ended): (i64, bool) = sqlx::query_as(
        "SELECT acked_event_seq, ended_at IS NOT NULL
         FROM task_attempts WHERE task_id = $1 AND attempt = 1",
    )
    .bind(task_id)
    .fetch_one(&app.db)
    .await
    .expect("load the attempt");
    assert_eq!(acked, 3, "the stop stored the whole journal");
    assert!(ended, "the terminal outcome settled the attempt");
    let (entry_type, content): (String, String) = sqlx::query_as(
        "SELECT entry_type, content FROM task_events WHERE task_id = $1 AND seq = 2",
    )
    .bind(task_id)
    .fetch_one(&app.db)
    .await
    .expect("load the rejected fact's evidence");
    assert_eq!(entry_type, "error");
    assert!(content.contains("already used"), "{content}");
    let requests: i64 =
        sqlx::query_scalar("SELECT count(*) FROM provider_requests WHERE task_id = $1")
            .bind(task_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(
        requests, 1,
        "the reuse must not merge into the first request"
    );

    app.cleanup().await;
}

/// A force Delete's admission stamps its supersession on the database clock
/// (the fencing trigger compares it with `statement_timestamp()`), and the
/// executor's claim is stamped there too: while the delayed host reply is
/// pending the row is claimed for at most the claim lease, never left for a
/// minutes-long recovery timer.
#[tokio::test]
async fn forced_delete_stop_is_claimed_on_the_database_clock_while_it_executes() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let _runtime = TestWorkspaceRuntimePeer::attach_with_host_command_delay(
        &app,
        &workspace,
        owner.id,
        std::time::Duration::from_secs(2),
    )
    .await;
    let service = workspace_vm_service(&app, owner.id, org.id).await;
    let workspace_id = workspace.id;
    let operation_id = Uuid::new_v4();
    let stop = tokio::spawn(async move {
        service
            .stop(
                workspace_id,
                owner.id,
                operation_id,
                g5_api::services::workspace_vm_service::StopReason::Delete,
                Some(chrono::Utc::now()),
            )
            .await
    });

    let retry_at = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if let Some(retry_at) = sqlx::query_scalar::<_, chrono::DateTime<chrono::Utc>>(
                "SELECT stop_retry_at FROM workspace_vms
                 WHERE workspace_id = $1 AND status = 'stopping'",
            )
            .bind(workspace.id)
            .fetch_optional(&app.db)
            .await
            .expect("load delete stop retry time")
            {
                break retry_at;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("delete stop must remain observable while the host reply is delayed");
    let database_now: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("SELECT now()")
        .fetch_one(&app.db)
        .await
        .expect("read database clock");
    let claim = retry_at - database_now;
    assert!(
        claim > chrono::TimeDelta::zero() && claim <= chrono::TimeDelta::seconds(30),
        "the executing delete stop holds a short claim: retry_at={retry_at}, now={database_now}"
    );

    let outcome = stop
        .await
        .expect("join delayed delete stop")
        .expect("delete stop completes after the delayed host reply");
    assert_eq!(outcome.status, "stopped");

    app.cleanup().await;
}

#[tokio::test]
async fn deadline_stop_keeps_the_workspace_running_until_the_nudge_deadline() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let service = workspace_vm_service(&app, owner.id, org.id).await;
    let operation_id = Uuid::new_v4();
    let requested_deadline = chrono::Utc::now() + chrono::Duration::minutes(5);
    let nudge_deadline =
        chrono::DateTime::from_timestamp_micros(requested_deadline.timestamp_micros())
            .expect("microsecond deadline is representable");
    let active_session = Uuid::new_v4();
    let idle_session = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO chat_sessions(id, org_id, workspace_id, scope_type, title, created_by)
         VALUES ($1,$3,$4,'workspace','Active',$5),
                ($2,$3,$4,'workspace','Idle',$5)",
    )
    .bind(active_session)
    .bind(idle_session)
    .bind(org.id)
    .bind(workspace.id)
    .bind(owner.id)
    .execute(&app.db)
    .await
    .expect("insert nudge sessions");
    sqlx::query(
        "INSERT INTO tasks(
             id, org_id, workspace_id, name, kind, status, spec, attempt,
             chat_session_id, continuation_reason, hold_reason, created_by)
         VALUES ($1,$2,$3,'Held turn','agent','waiting','{}'::jsonb,1,
                 $4,'interrupted','owner_stop',$5)",
    )
    .bind(Uuid::new_v4())
    .bind(org.id)
    .bind(workspace.id)
    .bind(active_session)
    .bind(owner.id)
    .execute(&app.db)
    .await
    .expect("insert held agent task");

    let outcome = service
        .stop_as_system_for_tests(
            workspace.id,
            operation_id,
            g5_api::services::workspace_vm_service::StopReason::Drain,
            Some(nudge_deadline),
        )
        .await
        .expect("schedule deadline stop");
    assert_eq!(outcome.status, "running");

    let pending: (
        String,
        Option<String>,
        Option<Uuid>,
        Option<chrono::DateTime<chrono::Utc>>,
    ) = sqlx::query_as(
        "SELECT status, pending_stop_reason, stop_operation_id,
                    pending_stop_deadline_at
             FROM workspace_vms WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .expect("load scheduled stop");
    assert_eq!(pending.0, "running");
    assert_eq!(pending.1.as_deref(), Some("drain"));
    assert_eq!(pending.2, Some(operation_id));
    assert_eq!(pending.3, Some(nudge_deadline));
    assert!(runtime.received_host_command_methods().await.is_empty());
    let notice: (Uuid, String, String) = sqlx::query_as(
        "SELECT session_id,
                artifacts #>> '{platform_notice,delivery}',
                artifacts #>> '{platform_notice,operation_id}'
         FROM chat_messages WHERE role = 'system'
           AND artifacts ? 'platform_notice'",
    )
    .fetch_one(&app.db)
    .await
    .expect("load queued lifecycle notice");
    assert_eq!(notice.0, active_session);
    assert_eq!(notice.1, "queued");
    assert_eq!(notice.2, operation_id.to_string());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM chat_messages WHERE session_id = $1",)
            .bind(idle_session)
            .fetch_one(&app.db)
            .await
            .expect("count idle session notices"),
        0
    );

    let replay = service
        .stop_as_system_for_tests(
            workspace.id,
            operation_id,
            g5_api::services::workspace_vm_service::StopReason::Drain,
            Some(nudge_deadline),
        )
        .await
        .expect("replay deadline stop");
    assert_eq!(replay.status, "running");
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM chat_messages WHERE session_id = $1",)
            .bind(active_session)
            .fetch_one(&app.db)
            .await
            .expect("count replayed notices"),
        1,
        "nudge replay must not duplicate the system notice"
    );

    sqlx::query(
        "UPDATE workspace_vms
         SET pending_stop_deadline_at = now() - interval '1 second'
         WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .execute(&app.db)
    .await
    .expect("expire the nudge deadline");

    assert_eq!(
        service
            .reconcile_pending_stops(1)
            .await
            .expect("reconcile scheduled stop"),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT status FROM workspace_vms WHERE workspace_id = $1",
        )
        .bind(workspace.id)
        .fetch_one(&app.db)
        .await
        .expect("load reconciled stop"),
        "stopped"
    );
    assert_eq!(
        sqlx::query_scalar::<_, Option<chrono::DateTime<chrono::Utc>>>(
            "SELECT unflushed_loss_at FROM workspace_vms WHERE workspace_id = $1",
        )
        .bind(workspace.id)
        .fetch_one(&app.db)
        .await
        .expect("load scheduled-stop loss marker"),
        None,
        "an expired nudge must still receive a fresh flush deadline"
    );
    assert!(runtime
        .received_rpc_methods()
        .await
        .iter()
        .any(|method| method == "workspace.flush"));
    assert!(runtime
        .received_host_command_methods()
        .await
        .iter()
        .any(|method| method == "vm.stop"));
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT artifacts #>> '{platform_notice,delivery}'
             FROM chat_messages WHERE session_id = $1 AND role = 'system'",
        )
        .bind(active_session)
        .fetch_one(&app.db)
        .await
        .expect("load released lifecycle notice"),
        "withdrawn"
    );

    app.cleanup().await;
}

/// A lifecycle notice racing another writer of the session's message
/// ordinals (journal projection of accepted input, or a draining pre-upgrade
/// pod) must neither duplicate a message index nor fail the stop: the notice
/// waits out the competing uncommitted insert, re-reads the committed
/// ordinal under a savepoint and lands on the next one (issue #629 S25).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn lifecycle_notice_racing_an_accepted_input_takes_the_next_index() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let _runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let service = workspace_vm_service(&app, owner.id, org.id).await;
    let session_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO chat_sessions(id, org_id, workspace_id, scope_type, title, created_by)
         VALUES ($1,$2,$3,'workspace','Active',$4)",
    )
    .bind(session_id)
    .bind(org.id)
    .bind(workspace.id)
    .bind(owner.id)
    .execute(&app.db)
    .await
    .expect("insert nudged session");
    sqlx::query(
        "INSERT INTO tasks(
             id, org_id, workspace_id, name, kind, status, spec, attempt,
             chat_session_id, created_by)
         VALUES ($1,$2,$3,'Turn','agent','queued','{}'::jsonb,1,$4,$5)",
    )
    .bind(Uuid::new_v4())
    .bind(org.id)
    .bind(workspace.id)
    .bind(session_id)
    .bind(owner.id)
    .execute(&app.db)
    .await
    .expect("insert active agent task");

    // The competing allocator: an uncommitted message at the index the
    // notice will also compute, as an input.accepted projection holds one
    // until its transaction commits.
    let mut accepted = app.db.begin().await.expect("begin the projection");
    sqlx::query(
        "INSERT INTO chat_messages(
             id, org_id, session_id, role, content, citations, status,
             message_index)
         VALUES ($1,$2,$3,'user','Accepted input','[]'::jsonb,'complete',0)",
    )
    .bind(Uuid::new_v4())
    .bind(org.id)
    .bind(session_id)
    .execute(&mut *accepted)
    .await
    .expect("hold the uncommitted accepted input");

    let stop = tokio::spawn({
        let workspace_id = workspace.id;
        async move {
            service
                .stop_as_system_for_tests(
                    workspace_id,
                    Uuid::new_v4(),
                    g5_api::services::workspace_vm_service::StopReason::Drain,
                    Some(chrono::Utc::now() + chrono::Duration::minutes(5)),
                )
                .await
        }
    });
    // Commit only once the notice insert is provably blocked on the
    // uncommitted index-0 row, so the race is exercised on every run instead
    // of depending on scheduler timing.
    tokio::time::timeout(std::time::Duration::from_secs(15), async {
        loop {
            let waiting: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM pg_stat_activity
                 WHERE datname = current_database() AND wait_event_type = 'Lock'
                   AND query LIKE 'INSERT INTO chat_messages%')",
            )
            .fetch_one(&app.db)
            .await
            .expect("inspect lock waits");
            if waiting {
                return;
            }
            assert!(
                !stop.is_finished(),
                "the notice must wait for the competing ordinal, not finish or fail"
            );
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the notice insert blocks on the uncommitted ordinal");
    accepted.commit().await.expect("commit the projection");

    let outcome = stop
        .await
        .expect("join the scheduled stop")
        .expect("the scheduled stop survives the ordinal race");
    assert_eq!(outcome.status, "running");
    let messages: Vec<(i32, String)> = sqlx::query_as(
        "SELECT message_index, role FROM chat_messages
         WHERE session_id = $1 ORDER BY message_index",
    )
    .bind(session_id)
    .fetch_all(&app.db)
    .await
    .expect("load raced messages");
    assert_eq!(
        messages,
        vec![(0, "user".to_string()), (1, "system".to_string())],
        "the notice took the next free ordinal"
    );

    app.cleanup().await;
}

#[tokio::test]
async fn owner_stop_supersedes_a_pending_deadline_nudge() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let service = workspace_vm_service(&app, owner.id, org.id).await;
    let scheduled_operation = Uuid::new_v4();

    service
        .stop_as_system_for_tests(
            workspace.id,
            scheduled_operation,
            g5_api::services::workspace_vm_service::StopReason::Drain,
            Some(chrono::Utc::now() + chrono::Duration::minutes(5)),
        )
        .await
        .expect("schedule deadline stop");

    let owner_operation = Uuid::new_v4();
    let outcome = service
        .stop(
            workspace.id,
            owner.id,
            owner_operation,
            g5_api::services::workspace_vm_service::StopReason::User,
            None,
        )
        .await
        .expect("owner stop must supersede pending nudge");
    assert_eq!(outcome.status, "stopped");
    let stopped: (String, Option<Uuid>, Option<String>) = sqlx::query_as(
        "SELECT status, stop_operation_id, pending_stop_reason
         FROM workspace_vms WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .expect("load superseding stop");
    assert_eq!(stopped, ("stopped".to_string(), None, None));
    assert!(runtime
        .received_host_command_methods()
        .await
        .iter()
        .any(|method| method == "vm.stop"));

    app.cleanup().await;
}

/// The host's `vm.stop` commands for this workspace, in arrival order.
async fn vm_stop_commands(runtime: &TestWorkspaceRuntimePeer) -> Vec<Value> {
    runtime.received_host_command_params("vm.stop").await
}

/// The stop gave up saving and killed the VM (`g5.lifecycle.stop.force`).
fn assert_killed(commands: &[Value]) {
    let last = commands.last().expect("the stop sends the host a vm.stop");
    assert_eq!(last["mode"], "kill", "{commands:?}");
    assert_eq!(last["grace_secs"], 0, "{commands:?}");
    assert!(
        last["idempotency_key"]
            .as_str()
            .is_some_and(|key| key.ends_with(":host-kill")),
        "a kill has its own idempotency key: {commands:?}"
    );
}

#[tokio::test]
async fn deterministic_flush_failure_gives_up_saving_and_kills_the_vm() {
    assert_deterministic_flush_failure_kills_the_vm("FLUSH_TOO_LARGE").await;
}

#[tokio::test]
async fn ignored_native_memory_gives_up_saving_and_kills_the_vm() {
    assert_deterministic_flush_failure_kills_the_vm("FLUSH_MEMORY_IGNORED").await;
}

/// A save failure no retry fixes ends the save window at once
/// (`g5.git.flush-failure.block`): the owner's Stop records the loss and
/// kills the VM instead of reopening it, and leaves no block behind.
async fn assert_deterministic_flush_failure_kills_the_vm(error_code: &str) {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime =
        TestWorkspaceRuntimePeer::attach_with_flush_failure(&app, &workspace, owner.id, error_code)
            .await;
    let operation_id = Uuid::new_v4();
    let delivery_unknown_task_id = Uuid::new_v4();
    let response_timeout_task_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO tasks(
             id, org_id, workspace_id, name, kind, status, spec, attempt,
             hold_reason, created_by
         ) VALUES
             ($1,$3,$4,'Ambiguous delivery','command','waiting',
              jsonb_build_object('lifecycle',jsonb_build_object(
                  'interrupt_operation_id',$5::text)),1,'delivery_unknown',$6),
             ($2,$3,$4,'Pending response','command','waiting',
              jsonb_build_object('lifecycle',jsonb_build_object(
                  'interrupt_operation_id',$5::text)),1,'response_timeout',$6)",
    )
    .bind(delivery_unknown_task_id)
    .bind(response_timeout_task_id)
    .bind(org.id)
    .bind(workspace.id)
    .bind(operation_id)
    .bind(owner.id)
    .execute(&app.db)
    .await
    .expect("insert protected task holds");

    let response = app
        .authenticated_client(&owner)
        .post_json(
            &format!("/api/v1/workspaces/{}/stop", workspace.id),
            &json!({"operationId": operation_id}),
        )
        .await;
    let status = response.status();
    let body: Value = response.json().await.expect("parse stop response");
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data(&body)["status"], "stopped", "{body}");
    assert_eq!(data(&body)["unflushedLoss"], true, "{body}");

    let state: (String, Option<chrono::DateTime<chrono::Utc>>, Value) = sqlx::query_as(
        "SELECT status, unflushed_loss_at, last_error
         FROM workspace_vms WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .expect("load forced stop state");
    assert_eq!(state.0, "stopped");
    assert!(state.1.is_some(), "the loss is recorded");
    assert_eq!(state.2["reason"], "deadline_unflushed_loss");
    assert_eq!(state.2["detail"], error_code);
    assert!(
        state.2.get("flush_block").is_none(),
        "no block is kept: {}",
        state.2
    );
    let flush_notifications: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM notifications
         WHERE workspace_id = $1 AND category = 'workspace.flush_blocked'",
    )
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .expect("count flush block notifications");
    assert_eq!(flush_notifications, 0);

    let rpc = runtime.received_rpc_methods().await;
    assert_eq!(
        rpc.iter()
            .filter(|method| method.as_str() == "admission.close")
            .count(),
        2,
        "stop must first refuse new writers and then drain admitted writers: {rpc:?}"
    );
    assert!(rpc.iter().any(|method| method == "workspace.flush"));
    assert!(
        rpc.iter().all(|method| method != "admission.open"),
        "a stop never reopens the VM: {rpc:?}"
    );
    let commands = vm_stop_commands(&runtime).await;
    assert_eq!(commands.len(), 1, "{commands:?}");
    assert_killed(&commands);
    let protected_holds: Vec<(Uuid, String, Option<String>)> = sqlx::query_as(
        "SELECT id, status, hold_reason
         FROM tasks WHERE id IN ($1,$2) ORDER BY id",
    )
    .bind(delivery_unknown_task_id)
    .bind(response_timeout_task_id)
    .fetch_all(&app.db)
    .await
    .expect("load task holds after the forced stop");
    assert!(protected_holds.contains(&(
        delivery_unknown_task_id,
        "waiting".to_string(),
        Some("delivery_unknown".to_string()),
    )));
    assert!(protected_holds.contains(&(
        response_timeout_task_id,
        "waiting".to_string(),
        Some("response_timeout".to_string()),
    )));

    app.cleanup().await;
}

/// A save failure a retry may fix is retried inside the owner's two-minute
/// save window (`g5.lifecycle.stop.window`), with the VM still closed to new
/// work. Once the window ends, the stop records the loss and kills the VM
/// without calling the guest again.
#[tokio::test]
async fn transient_flush_failure_is_retried_until_the_save_window_ends() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach_with_flush_failure(
        &app,
        &workspace,
        owner.id,
        "FLUSH_API_UNAVAILABLE",
    )
    .await;
    // Host control sets a running VM's start time when it reports ready; the
    // scripted peer's row has none.
    sqlx::query("UPDATE workspace_vms SET started_at = now() WHERE workspace_id = $1")
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .expect("record the VM start");

    let admitted_after = chrono::Utc::now();
    let response = app
        .authenticated_client(&owner)
        .post_json(
            &format!("/api/v1/workspaces/{}/stop", workspace.id),
            &json!({"operationId": Uuid::new_v4()}),
        )
        .await;
    let status = response.status();
    let body: Value = response.json().await.expect("parse stop response");
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data(&body)["status"], "stopping", "{body}");
    let (state, deadline, retry_at): (
        String,
        chrono::DateTime<chrono::Utc>,
        chrono::DateTime<chrono::Utc>,
    ) = sqlx::query_as(
        "SELECT status, stop_deadline_at, stop_retry_at
         FROM workspace_vms WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .expect("load retried stop");
    assert_eq!(state, "stopping");
    let window = deadline - admitted_after;
    assert!(
        window > chrono::TimeDelta::seconds(100) && window <= chrono::TimeDelta::seconds(121),
        "the owner's Stop saves for two minutes, not ten: {window}"
    );
    assert!(retry_at <= chrono::Utc::now() + chrono::TimeDelta::seconds(10));
    let rpc = runtime.received_rpc_methods().await;
    assert!(rpc.iter().any(|method| method == "workspace.flush"));
    assert!(
        rpc.iter().all(|method| method != "admission.open"),
        "{rpc:?}"
    );
    assert!(vm_stop_commands(&runtime).await.is_empty());

    sqlx::query(
        "UPDATE workspace_vms
         SET stop_deadline_at = now() - interval '1 second', stop_retry_at = now()
         WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .execute(&app.db)
    .await
    .expect("end the save window");
    let guest_calls = rpc.len();
    assert_eq!(
        workspace_vm_service(&app, owner.id, org.id)
            .await
            .reconcile_stopping(10)
            .await
            .expect("reconcile the stop after its window"),
        1
    );
    let (state, loss, detail): (String, Option<chrono::DateTime<chrono::Utc>>, Value) =
        sqlx::query_as(
            "SELECT status, unflushed_loss_at, last_error -> 'detail'
             FROM workspace_vms WHERE workspace_id = $1",
        )
        .bind(workspace.id)
        .fetch_one(&app.db)
        .await
        .expect("load stop after its window");
    assert_eq!(state, "stopped");
    assert!(loss.is_some());
    assert_eq!(detail, "DEADLINE_EXCEEDED");
    assert_eq!(
        runtime.received_rpc_methods().await.len(),
        guest_calls,
        "no guest call after the save window"
    );
    assert_killed(&vm_stop_commands(&runtime).await);

    app.cleanup().await;
}

/// Giving up saving ends the window for good (`g5.lifecycle.stop.window`):
/// when the host answers the kill with `Busy` and the reconciler retries,
/// the retry kills again under the same key instead of saving again or
/// shutting the VM down gracefully.
#[tokio::test]
async fn forced_stop_stays_a_kill_when_the_host_command_is_retried() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach_with_flush_failure_and_vm_stop_result(
        &app,
        &workspace,
        owner.id,
        "FLUSH_TOO_LARGE",
        g5_internal_api::host_ctl::CommandResult::Busy,
    )
    .await;
    // Host control sets a running VM's start time when it reports ready; the
    // scripted peer's row has none.
    sqlx::query("UPDATE workspace_vms SET started_at = now() WHERE workspace_id = $1")
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .expect("record the VM start");
    let service = workspace_vm_service(&app, owner.id, org.id).await;
    let operation_id = Uuid::new_v4();

    let first = service
        .stop(
            workspace.id,
            owner.id,
            operation_id,
            g5_api::services::workspace_vm_service::StopReason::User,
            None,
        )
        .await
        .expect("a busy host leaves the stop pending");
    assert_eq!(first.status, "stopping");
    assert!(first.unflushed_loss);
    let window_ended: bool = sqlx::query_scalar(
        "SELECT stop_deadline_at <= now() FROM workspace_vms WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .expect("load the stop deadline");
    assert!(window_ended, "giving up ends the save window");
    let guest_calls = runtime.received_rpc_methods().await.len();

    sqlx::query("UPDATE workspace_vms SET stop_retry_at = now() WHERE workspace_id = $1")
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .expect("make the stop due");
    service
        .reconcile_stopping(10)
        .await
        .expect("retry the pending stop");
    let commands = vm_stop_commands(&runtime).await;
    assert_eq!(commands.len(), 2, "{commands:?}");
    for command in &commands {
        assert_killed(std::slice::from_ref(command));
    }
    assert_eq!(
        commands[0]["idempotency_key"], commands[1]["idempotency_key"],
        "the retry resends the same kill"
    );
    assert_eq!(
        runtime.received_rpc_methods().await.len(),
        guest_calls,
        "the retry does not try to save again"
    );

    app.cleanup().await;
}

/// A stop whose save completed lost nothing, even when it resumes after its
/// window: an API restart between the flush and the host command leaves
/// `stop_command_sent_at` unset, and the resumed stop kills the VM without
/// recording a loss.
#[tokio::test]
async fn stop_resumed_after_a_completed_save_records_no_loss() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach_with_vm_stop_result(
        &app,
        &workspace,
        owner.id,
        g5_internal_api::host_ctl::CommandResult::Busy,
    )
    .await;
    // Host control sets a running VM's start time when it reports ready; the
    // scripted peer's row has none.
    sqlx::query("UPDATE workspace_vms SET started_at = now() WHERE workspace_id = $1")
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .expect("record the VM start");
    let service = workspace_vm_service(&app, owner.id, org.id).await;
    let saved = service
        .stop(
            workspace.id,
            owner.id,
            Uuid::new_v4(),
            g5_api::services::workspace_vm_service::StopReason::User,
            None,
        )
        .await
        .expect("a busy host leaves the saved stop pending");
    assert_eq!(saved.status, "stopping");
    assert!(saved.flush_id.is_some(), "the save completed");
    assert!(!saved.unflushed_loss);

    // An API restart before the host command was marked, then the window ends.
    sqlx::query(
        "UPDATE workspace_vms
         SET stop_command_sent_at = NULL,
             stop_deadline_at = now() - interval '1 second', stop_retry_at = now()
         WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .execute(&app.db)
    .await
    .expect("resume the saved stop after its window");
    let guest_calls = runtime.received_rpc_methods().await.len();
    service
        .reconcile_stopping(10)
        .await
        .expect("reconcile the saved stop");
    let (loss, reason): (Option<chrono::DateTime<chrono::Utc>>, Option<String>) = sqlx::query_as(
        "SELECT unflushed_loss_at, last_error ->> 'reason'
             FROM workspace_vms WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .expect("load the resumed stop");
    assert!(loss.is_none(), "a completed save lost nothing");
    assert_ne!(reason.as_deref(), Some("deadline_unflushed_loss"));
    assert_eq!(
        runtime.received_rpc_methods().await.len(),
        guest_calls,
        "no guest call after the save window"
    );
    assert_killed(&vm_stop_commands(&runtime).await);

    app.cleanup().await;
}

#[tokio::test]
async fn user_stop_resumed_past_its_save_window_kills_without_calling_the_guest() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let operation_id = Uuid::new_v4();
    sqlx::query(
        "UPDATE workspace_vms
         SET status = 'stopping', stop_operation_id = $2, stop_reason = 'user',
             stop_generation = 1, started_at = now() - interval '1 minute',
             stop_deadline_at = now() - interval '1 second',
             stop_command_sent_at = NULL
         WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .bind(operation_id)
    .execute(&app.db)
    .await
    .expect("seed expired user stop");

    let outcome = workspace_vm_service(&app, owner.id, org.id)
        .await
        .stop(
            workspace.id,
            owner.id,
            operation_id,
            g5_api::services::workspace_vm_service::StopReason::User,
            None,
        )
        .await
        .expect("a stop past its window completes");
    assert_eq!(outcome.status, "stopped");
    assert!(outcome.unflushed_loss);
    assert!(
        runtime.received_rpc_methods().await.is_empty(),
        "no guest call after the save window"
    );
    assert_killed(&vm_stop_commands(&runtime).await);

    app.cleanup().await;
}

#[tokio::test]
async fn owner_stop_gives_up_on_a_task_event_it_cannot_store_and_start_boots_fresh() {
    // The QA incident: a journaled event the API could not store (there, NUL
    // bytes from a tool's output) failed every replay, so the Stop retried
    // until its deadline and then reopened the VM. A sequence gap fails at
    // the same commit; a missing payload fails reading the replay.
    for (case, event) in [
        (
            "unstorable event",
            json!({"seq": 2, "kind": "module.output", "payload": {"text": "late"}}),
        ),
        ("invalid event", json!({"seq": 1, "kind": "module.output"})),
    ] {
        assert_unsettleable_task_does_not_block_stop(case, event).await;
    }
}

async fn assert_unsettleable_task_does_not_block_stop(case: &str, event: Value) {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    workspace.attach_parent(&app).await;
    let runtime = TestWorkspaceRuntimePeer::attach_with_journal_events(
        &app,
        &workspace,
        owner.id,
        vec![event],
    )
    .await;
    // Host control sets a running VM's start time when it reports ready; the
    // scripted peer's row has none.
    sqlx::query("UPDATE workspace_vms SET started_at = now() WHERE workspace_id = $1")
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .expect("record the VM start");
    let task_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO tasks(
             id, org_id, workspace_id, name, kind, status, spec,
             attempt, vm_id, epoch, start_confirmed, started_at, created_by
         ) VALUES (
             $1,$2,$3,'Running command','command','running',
             jsonb_build_object('command',jsonb_build_object(
                 'schema_version','g5.command-task-spec/v1',
                 'module_id','workspace.bootstrap','module_version','1.0.0',
                 'operation','run','input_schema','g5.workspace.bootstrap.run/v1',
                 'input','{}'::jsonb)),
             1,$4,1,true,now(),$5)",
    )
    .bind(task_id)
    .bind(org.id)
    .bind(workspace.id)
    .bind(runtime.vm_id().to_string())
    .bind(owner.id)
    .execute(&app.db)
    .await
    .expect("insert running command");
    sqlx::query(
        "INSERT INTO task_attempts(
             org_id, task_id, attempt, vm_id, epoch, start_confirmed, started_at
         ) VALUES ($1,$2,1,$3,1,true,now())",
    )
    .bind(org.id)
    .bind(task_id)
    .bind(runtime.vm_id().to_string())
    .execute(&app.db)
    .await
    .expect("insert running command attempt");
    let client = app.authenticated_client(&owner);

    let response = client
        .post_json(
            &format!("/api/v1/workspaces/{}/stop", workspace.id),
            &json!({"operationId": Uuid::new_v4()}),
        )
        .await;
    let status = response.status();
    let body: Value = response.json().await.expect("parse stop response");
    assert_eq!(status, StatusCode::OK, "{case}: {body}");
    assert_eq!(data(&body)["status"], "stopping", "{case}: {body}");
    let rpc = runtime.received_rpc_methods().await;
    assert!(
        rpc.iter().any(|method| method == "events.replay"),
        "{case}: {rpc:?}"
    );
    assert!(vm_stop_commands(&runtime).await.is_empty(), "{case}");

    sqlx::query(
        "UPDATE workspace_vms
         SET stop_deadline_at = now() - interval '1 second', stop_retry_at = now()
         WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .execute(&app.db)
    .await
    .expect("end the save window");
    workspace_vm_service(&app, owner.id, org.id)
        .await
        .reconcile_stopping(10)
        .await
        .expect("reconcile the stop after its window");
    let (state, loss): (String, Option<chrono::DateTime<chrono::Utc>>) = sqlx::query_as(
        "SELECT status, unflushed_loss_at FROM workspace_vms WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .expect("load stopped workspace");
    assert_eq!(state, "stopped", "{case}");
    assert!(loss.is_some(), "{case}");
    assert_killed(&vm_stop_commands(&runtime).await);

    // The attempt the stop could not settle ends as a lost VM's would.
    g5_api_core::maintenance::agent_tasks::run_once(&app.state)
        .await
        .expect("reconcile agent tasks");
    let (task_status, attempt_ended): (String, Option<chrono::DateTime<chrono::Utc>>) =
        sqlx::query_as(
            "SELECT task.status, attempt.ended_at
             FROM tasks AS task
             JOIN task_attempts AS attempt
               ON attempt.task_id = task.id AND attempt.attempt = 1
             WHERE task.id = $1",
        )
        .bind(task_id)
        .fetch_one(&app.db)
        .await
        .expect("load the settled task");
    assert_ne!(task_status, "running", "{case}");
    assert!(attempt_ended.is_some(), "{case}: the attempt is settled");

    // Start boots a new incarnation.
    let resumed = client
        .post_json(
            &format!("/api/v1/workspaces/{}/resume", workspace.id),
            &json!({"operationId": Uuid::new_v4()}),
        )
        .await;
    let status = resumed.status();
    let resumed: Value = resumed.json().await.expect("parse resume outcome");
    assert_eq!(status, StatusCode::OK, "{case}: {resumed}");
    assert_eq!(data(&resumed)["outcome"], "starting", "{case}");
    assert_ne!(
        data(&resumed)["vm"]["vmId"],
        runtime.vm_id().to_string(),
        "{case}: Start after a forced stop allocates a new incarnation"
    );
    assert!(runtime
        .received_host_command_methods()
        .await
        .iter()
        .any(|method| method == "vm.start"));

    app.cleanup().await;
}

/// Idle stop follows the same rule: a save that fails ends with the VM
/// killed and the loss recorded. A block an earlier version left on the
/// workspace no longer keeps it from idle stopping.
#[tokio::test]
async fn idle_stop_kills_a_workspace_whose_save_fails() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach_with_flush_failure(
        &app,
        &workspace,
        owner.id,
        "FLUSH_TOO_LARGE",
    )
    .await;
    sqlx::query(
        "UPDATE workspace_vms
         SET started_at = now() - interval '25 hours',
             last_activity_at = now() - interval '25 hours',
             last_error = jsonb_build_object(
                 'flush_block', jsonb_build_object('code','FLUSH_TOO_LARGE')),
             updated_at = now()
         WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .execute(&app.db)
    .await
    .expect("age the workspace and leave an old flush block");

    let service = workspace_vm_service(&app, owner.id, org.id).await;
    assert_eq!(
        service.idle_candidates(10).await.unwrap(),
        vec![workspace.id]
    );
    assert_eq!(service.reconcile_idle(10).await.unwrap(), 1);
    let (state, loss): (String, Option<chrono::DateTime<chrono::Utc>>) = sqlx::query_as(
        "SELECT status, unflushed_loss_at FROM workspace_vms WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .expect("load idle-stopped workspace");
    assert_eq!(state, "stopped");
    assert!(loss.is_some());
    assert_killed(&vm_stop_commands(&runtime).await);

    app.cleanup().await;
}

#[tokio::test]
async fn idle_reconciler_flushes_and_stops_an_inactive_workspace() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    sqlx::query(
        "UPDATE workspace_vms
         SET started_at = now() - interval '25 hours',
             last_activity_at = now() - interval '25 hours', updated_at = now()
         WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .execute(&app.db)
    .await
    .expect("age workspace activity");

    let service = workspace_vm_service(&app, owner.id, org.id).await;
    assert_eq!(
        service.idle_candidates(10).await.unwrap(),
        vec![workspace.id]
    );
    assert_eq!(service.reconcile_idle(10).await.unwrap(), 1);
    let state: (String, Option<String>, Option<Uuid>) = sqlx::query_as(
        "SELECT status, stop_reason, last_flush_id
         FROM workspace_vms WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .expect("load idle-stopped workspace");
    assert_eq!(state.0, "stopped");
    assert_eq!(state.1, None, "release clears the completed stop reason");
    assert!(state.2.is_some());
    assert!(runtime
        .received_host_command_methods()
        .await
        .iter()
        .any(|method| method == "vm.stop"));

    app.cleanup().await;
}

#[tokio::test]
async fn idle_reconciler_waits_for_a_starting_preview_but_not_a_ready_one() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    sqlx::query(
        "UPDATE workspace_vms
         SET started_at = now() - interval '25 hours',
             last_activity_at = now() - interval '25 hours', updated_at = now()
         WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .execute(&app.db)
    .await
    .expect("age workspace activity");
    let task_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO tasks(
             id, org_id, workspace_id, name, kind, status, spec, attempt,
             vm_id, epoch, start_confirmed, started_at, created_by
         ) VALUES ($1,$2,$3,'Preview','preview','running','{}'::jsonb,1,
                   $4,1,true,now(),$5)",
    )
    .bind(task_id)
    .bind(org.id)
    .bind(workspace.id)
    .bind(runtime.vm_id().to_string())
    .bind(owner.id)
    .execute(&app.db)
    .await
    .expect("insert a preview still building");

    let service = workspace_vm_service(&app, owner.id, org.id).await;
    assert_eq!(
        service.idle_candidates(10).await.unwrap(),
        Vec::<Uuid>::new(),
        "a preview that has not reported ready is bounded work"
    );

    sqlx::query(
        "INSERT INTO task_events(
             id, org_id, task_id, entry_type, content, metadata, attempt, vm_id, seq)
         VALUES ($1,$2,$3,'checkpoint','Preview ready',
                 jsonb_build_object('kind','module.preview.ready'),1,$4,1)",
    )
    .bind(Uuid::new_v4())
    .bind(org.id)
    .bind(task_id)
    .bind(runtime.vm_id().to_string())
    .execute(&app.db)
    .await
    .expect("record preview readiness");
    assert_eq!(
        service.idle_candidates(10).await.unwrap(),
        vec![workspace.id],
        "a ready preview nobody uses does not keep the workspace up"
    );

    app.cleanup().await;
}

#[tokio::test]
async fn idle_stop_admission_yields_to_a_preview_that_started_after_selection() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    sqlx::query(
        "UPDATE workspace_vms
         SET started_at = now() - interval '25 hours',
             last_activity_at = now() - interval '25 hours', updated_at = now()
         WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .execute(&app.db)
    .await
    .expect("age workspace activity");
    let service = workspace_vm_service(&app, owner.id, org.id).await;
    assert_eq!(
        service.idle_candidates(10).await.unwrap(),
        vec![workspace.id]
    );

    // The owner requests a preview after selection and before admission.
    sqlx::query(
        "INSERT INTO tasks(
             id, org_id, workspace_id, name, kind, status, spec, attempt, created_by
         ) VALUES ($1,$2,$3,'Preview','preview','queued','{}'::jsonb,1,$4)",
    )
    .bind(Uuid::new_v4())
    .bind(org.id)
    .bind(workspace.id)
    .bind(owner.id)
    .execute(&app.db)
    .await
    .expect("queue a preview");

    let outcome = service
        .stop_as_system_for_tests(
            workspace.id,
            Uuid::new_v4(),
            g5_api::services::workspace_vm_service::StopReason::Idle,
            None,
        )
        .await
        .expect("an idle stop that finds work yields without error");
    assert_eq!(outcome.status, "running");
    let (vm_status, preview_status): (String, String) = sqlx::query_as(
        "SELECT vm.status, task.status
         FROM workspace_vms vm
         JOIN tasks task ON task.workspace_id = vm.workspace_id AND task.kind = 'preview'
         WHERE vm.workspace_id = $1",
    )
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .expect("load the workspace and its preview");
    assert_eq!(vm_status, "running", "the idle stop was not admitted");
    assert_eq!(preview_status, "queued", "the preview was not cancelled");
    assert!(!runtime
        .received_host_command_methods()
        .await
        .iter()
        .any(|method| method == "vm.stop"));
    assert_eq!(service.reconcile_idle(10).await.unwrap(), 0);

    app.cleanup().await;
}

#[tokio::test]
async fn expired_drain_deadline_records_flush_loss_and_releases_the_vm() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach_with_flush_failure(
        &app,
        &workspace,
        owner.id,
        "FLUSH_TOO_LARGE",
    )
    .await;
    let operation_id = Uuid::new_v4();
    sqlx::query(
        "UPDATE workspace_vms
         SET status = 'stopping', stop_reason = 'drain', stop_operation_id = $2,
             stop_generation = 1, stop_deadline_at = now() - interval '1 second',
             stop_retry_at = now(), started_at = now() - interval '1 hour'
         WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .bind(operation_id)
    .execute(&app.db)
    .await
    .expect("seed expired drain stop");

    let outcome = workspace_vm_service(&app, owner.id, org.id)
        .await
        .stop_as_system_for_tests(
            workspace.id,
            operation_id,
            g5_api::services::workspace_vm_service::StopReason::Drain,
            None,
        )
        .await
        .expect("finish drain after its flush deadline");
    assert_eq!(outcome.status, "stopped");
    assert!(outcome.unflushed_loss);
    let state: (String, Option<chrono::DateTime<chrono::Utc>>, Value) = sqlx::query_as(
        "SELECT status, unflushed_loss_at, last_error
         FROM workspace_vms WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .expect("load deadline-loss stop");
    assert_eq!(state.0, "stopped");
    assert!(state.1.is_some());
    assert_eq!(state.2["reason"], "deadline_unflushed_loss");
    assert_eq!(state.2["detail"], "DEADLINE_EXCEEDED");
    assert!(
        runtime.received_rpc_methods().await.is_empty(),
        "no guest call after the save window"
    );
    assert_killed(&vm_stop_commands(&runtime).await);

    app.cleanup().await;
}

#[tokio::test]
async fn retired_workspace_restart_route_does_not_mutate_the_vm() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let client = app.authenticated_client(&owner);
    let before: Value =
        sqlx::query_scalar("SELECT to_jsonb(vm) FROM workspace_vms vm WHERE workspace_id = $1")
            .bind(workspace.id)
            .fetch_one(&app.db)
            .await
            .expect("read initial VM");
    let response = client
        .post_json(
            &format!("/api/v1/workspaces/{}/restart", workspace.id),
            &json!({"operationId": Uuid::new_v4()}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let after: Value =
        sqlx::query_scalar("SELECT to_jsonb(vm) FROM workspace_vms vm WHERE workspace_id = $1")
            .bind(workspace.id)
            .fetch_one(&app.db)
            .await
            .expect("read unchanged VM");
    assert_eq!(after, before);
    assert!(runtime.received_host_command_methods().await.is_empty());
    app.cleanup().await;
}

#[tokio::test]
async fn workspace_vm_stop_then_resume_flushes_and_starts_a_new_incarnation() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    workspace.attach_parent(&app).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let client = app.authenticated_client(&owner);
    let replacement_image =
        "us-docker.pkg.dev/g5-test/runtime/workspace@sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    sqlx::query("UPDATE orgs SET default_base_image_ref = $2 WHERE id = $1")
        .bind(org.id)
        .bind(replacement_image)
        .execute(&app.db)
        .await
        .expect("change workspace image before the next start");

    let stopped = client
        .post_json(
            &format!("/api/v1/workspaces/{}/stop", workspace.id),
            &json!({"operationId": Uuid::new_v4()}),
        )
        .await;
    assert_eq!(stopped.status(), StatusCode::OK);
    let stopped: Value = stopped.json().await.expect("parse stop outcome");
    assert_eq!(data(&stopped)["status"], "stopped");
    assert!(data(&stopped)["flushId"].is_string());
    assert_eq!(data(&stopped)["unflushedLoss"], false);
    // Nothing required is left in the guest after a complete save, so the
    // host gets a short graceful shutdown (`g5.lifecycle.stop.release`).
    let stop_commands = vm_stop_commands(&runtime).await;
    assert_eq!(stop_commands.len(), 1, "{stop_commands:?}");
    assert_eq!(stop_commands[0]["mode"], "graceful");
    assert_eq!(stop_commands[0]["grace_secs"], 30);
    assert!(stop_commands[0]["idempotency_key"]
        .as_str()
        .is_some_and(|key| key.ends_with(":host")));
    assert!(
        runtime
            .received_host_command_methods()
            .await
            .iter()
            .all(|method| method != "vm.start"),
        "Stop must not start a replacement VM"
    );

    let resumed = client
        .post_json(
            &format!("/api/v1/workspaces/{}/resume", workspace.id),
            &json!({"operationId": Uuid::new_v4()}),
        )
        .await;
    assert_eq!(resumed.status(), StatusCode::OK);
    let resumed: Value = resumed.json().await.expect("parse resume outcome");
    assert_eq!(data(&resumed)["outcome"], "starting");
    assert_eq!(data(&resumed)["vm"]["status"], "starting");
    assert_ne!(
        data(&resumed)["vm"]["vmId"],
        runtime.vm_id().to_string(),
        "Start after Stop must allocate a new incarnation"
    );
    assert_eq!(data(&resumed)["vm"]["imageRef"], replacement_image);
    assert_eq!(data(&resumed)["vm"]["startIntent"]["kind"], "resume");

    let commands = runtime.received_host_command_methods().await;
    let stop_index = commands
        .iter()
        .position(|method| method == "vm.stop")
        .expect("Stop should stop the old VM");
    let start_index = commands
        .iter()
        .position(|method| method == "vm.start")
        .expect("Start should start a new VM");
    assert!(stop_index < start_index);

    app.cleanup().await;
}

#[tokio::test]
async fn reentered_stop_after_host_command_does_not_require_a_live_runtime_lease() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let operation_id = Uuid::new_v4();
    sqlx::query(
        "UPDATE workspace_vms
         SET status = 'stopping', stop_operation_id = $2, stop_reason = 'user',
             stop_generation = 1,
             stop_deadline_at = now() + interval '5 minutes',
             stop_command_sent_at = now(), lease_expires_at = now() - interval '1 minute'
         WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .bind(operation_id)
    .execute(&app.db)
    .await
    .expect("seed ambiguous post-command stop");

    let outcome = workspace_vm_service(&app, owner.id, org.id)
        .await
        .stop(
            workspace.id,
            owner.id,
            operation_id,
            g5_api::services::workspace_vm_service::StopReason::User,
            None,
        )
        .await
        .expect("retry the durable host command");
    assert_eq!(outcome.status, "stopped");
    assert!(!outcome.unflushed_loss);
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT status FROM workspace_vms WHERE workspace_id = $1",
        )
        .bind(workspace.id)
        .fetch_one(&app.db)
        .await
        .expect("load retried stop state"),
        "stopped"
    );
    assert!(runtime
        .received_host_command_methods()
        .await
        .iter()
        .any(|method| method == "vm.stop"));

    app.cleanup().await;
}

#[tokio::test]
async fn lease_expiry_releases_a_pre_command_stop_with_recorded_loss() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let _runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    sqlx::query(
        "UPDATE workspace_vms
         SET status = 'stopping', stop_operation_id = $2, stop_reason = 'user',
             stop_generation = 1,
             stop_deadline_at = now() + interval '5 minutes', stop_command_sent_at = NULL,
             started_at = now() - interval '5 minutes',
             lease_expires_at = now() - interval '1 minute'
         WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .bind(Uuid::new_v4())
    .execute(&app.db)
    .await
    .expect("seed expired pre-command stop");
    let preview_task_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO tasks(
             id, org_id, workspace_id, name, kind, status, spec, attempt, created_by
         ) VALUES ($1,$2,$3,'Queued preview','preview','queued','{}'::jsonb,1,$4)",
    )
    .bind(preview_task_id)
    .bind(org.id)
    .bind(workspace.id)
    .bind(owner.id)
    .execute(&app.db)
    .await
    .expect("seed queued preview before lease loss");
    sqlx::query(
        "UPDATE workspace_vms
         SET preview_generation = 1,
             preview_intent = jsonb_build_object(
                 'generation',1,'action','start','status','queued',
                 'requested_by',$2::text,'task_id',$3::text)
         WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .bind(owner.id)
    .bind(preview_task_id)
    .execute(&app.db)
    .await
    .expect("seed queued preview intent before lease loss");

    let reconciled = workspace_vm_service(&app, owner.id, org.id)
        .await
        .reconcile_expired_leases()
        .await
        .expect("reconcile expired stop lease");
    assert_eq!(reconciled, 1);
    let row: (String, Option<chrono::DateTime<chrono::Utc>>, Value) = sqlx::query_as(
        "SELECT status, unflushed_loss_at, last_error
         FROM workspace_vms WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .expect("load expired stop state");
    assert_eq!(row.0, "stopped");
    assert!(row.1.is_some());
    assert_eq!(row.2["reason"], "lease_expired_during_stop");
    let preview: (String, String) = sqlx::query_as(
        "SELECT task.status, vm.preview_intent ->> 'status'
         FROM tasks task JOIN workspace_vms vm ON vm.workspace_id = task.workspace_id
         WHERE task.id = $1",
    )
    .bind(preview_task_id)
    .fetch_one(&app.db)
    .await
    .expect("load preview after lease loss");
    assert_eq!(preview, ("cancelled".to_string(), "settled".to_string()));

    app.cleanup().await;
}

#[tokio::test]
async fn workspace_transfer_stops_before_new_owner_may_resume() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let next_owner = TestUser::create_with_scopes(
        &app,
        org.id,
        &[
            Scope::WorkspacesRead,
            Scope::WorkspacesWrite,
            Scope::SessionsRead,
            Scope::SessionsWrite,
            Scope::TasksExecute,
        ],
    )
    .await;
    let workspace = TestWorkspace::create(&app, &org).await;
    workspace.attach_parent(&app).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let owner_client = app.authenticated_client(&owner);
    let session_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO chat_sessions(
             id, org_id, workspace_id, scope_type, title, created_by
         ) VALUES ($1,$2,$3,'workspace','Before transfer',$4)",
    )
    .bind(session_id)
    .bind(workspace.org_id)
    .bind(workspace.id)
    .bind(owner.id)
    .execute(&app.db)
    .await
    .expect("insert transferred session");
    let queued_agent_task_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO tasks(
             id, org_id, workspace_id, name, kind, status, spec, attempt,
             chat_session_id, created_by
         ) VALUES ($1,$2,$3,'Former owner turn','agent','queued','{}'::jsonb,1,$4,$5)",
    )
    .bind(queued_agent_task_id)
    .bind(workspace.org_id)
    .bind(workspace.id)
    .bind(session_id)
    .bind(owner.id)
    .execute(&app.db)
    .await
    .expect("insert former-owner queued agent task");
    sqlx::query(
        "INSERT INTO task_attempts(org_id, task_id, attempt)
         VALUES ($1,$2,1)",
    )
    .bind(workspace.org_id)
    .bind(queued_agent_task_id)
    .execute(&app.db)
    .await
    .expect("insert former-owner queued agent attempt");
    let queued_input_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO session_inputs(
             id, org_id, session_id, input_index, content, input_origin, delivery, task_id,
             attempt
         ) VALUES ($1,$2,$3,0,'Former owner input','human','queued',$4,1)",
    )
    .bind(queued_input_id)
    .bind(workspace.org_id)
    .bind(session_id)
    .bind(queued_agent_task_id)
    .execute(&app.db)
    .await
    .expect("insert former-owner queued input");
    let ambiguous_session_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO chat_sessions(
             id, org_id, workspace_id, scope_type, title, created_by
         ) VALUES ($1,$2,$3,'workspace','Ambiguous delivery before transfer',$4)",
    )
    .bind(ambiguous_session_id)
    .bind(workspace.org_id)
    .bind(workspace.id)
    .bind(owner.id)
    .execute(&app.db)
    .await
    .expect("insert ambiguous-delivery session");
    let delivery_unknown_task_id = Uuid::new_v4();
    let response_timeout_task_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO tasks(
             id, org_id, workspace_id, name, kind, status, spec, attempt,
             hold_reason, created_by, chat_session_id
         ) VALUES
             ($1,$3,$4,'Ambiguous delivery','agent','waiting','{}'::jsonb,1,
              'delivery_unknown',$5,$6),
             ($2,$3,$4,'Pending response','command','waiting','{}'::jsonb,1,
              'response_timeout',$5,NULL)",
    )
    .bind(delivery_unknown_task_id)
    .bind(response_timeout_task_id)
    .bind(workspace.org_id)
    .bind(workspace.id)
    .bind(owner.id)
    .bind(ambiguous_session_id)
    .execute(&app.db)
    .await
    .expect("insert transfer-sensitive task holds");
    let preview_task_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO tasks(
             id, org_id, workspace_id, name, kind, status, spec, attempt, created_by
         ) VALUES ($1,$2,$3,'Former owner preview','preview','queued','{}'::jsonb,1,$4)",
    )
    .bind(preview_task_id)
    .bind(workspace.org_id)
    .bind(workspace.id)
    .bind(owner.id)
    .execute(&app.db)
    .await
    .expect("insert former-owner queued preview");
    sqlx::query(
        "UPDATE workspace_vms
         SET preview_generation = 1,
             preview_intent = jsonb_build_object(
                 'generation',1,'action','start','status','queued',
                 'requested_by',$2::text,'task_id',$3::text)
         WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .bind(owner.id)
    .bind(preview_task_id)
    .execute(&app.db)
    .await
    .expect("insert former-owner preview intent");

    let transfer_key = format!("transfer-{}", Uuid::new_v4());
    let transferred = owner_client
        .post_json(
            &format!("/api/v1/workspaces/{}/transfer", workspace.id),
            &json!({
                "toUserId": next_owner.id,
                "idempotencyKey": transfer_key
            }),
        )
        .await;
    let transferred_status = transferred.status();
    let transferred: Value = transferred.json().await.expect("parse transfer outcome");
    assert_eq!(
        transferred_status,
        StatusCode::OK,
        "transfer failed: {transferred}"
    );
    assert_eq!(data(&transferred)["fromUserId"], owner.id.to_string());
    assert_eq!(data(&transferred)["toUserId"], next_owner.id.to_string());
    assert_eq!(data(&transferred)["status"], "completed");
    let transfer_notifications: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM notifications
         WHERE workspace_id = $1
           AND recipient_user_id IN ($2, $3)
           AND title LIKE 'Workspace ownership transferred%'",
    )
    .bind(workspace.id)
    .bind(owner.id)
    .bind(next_owner.id)
    .fetch_one(&app.db)
    .await
    .expect("count ownership transfer notifications");
    assert_eq!(transfer_notifications, 2);

    let persisted: (Uuid, String) = sqlx::query_as(
        "SELECT workspace.owner_user_id, vm.status \
         FROM workspaces workspace JOIN workspace_vms vm ON vm.workspace_id = workspace.id \
         WHERE workspace.id = $1",
    )
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .expect("load transferred workspace");
    assert_eq!(persisted, (next_owner.id, "stopped".to_string()));
    let transferred_work: (String, String, String, String) = sqlx::query_as(
        "SELECT agent.status, input.delivery, preview.status,
                vm.preview_intent ->> 'status'
         FROM tasks agent
         JOIN session_inputs input ON input.session_id = agent.chat_session_id
         JOIN tasks preview ON preview.id = $3
         JOIN workspace_vms vm ON vm.workspace_id = agent.workspace_id
         WHERE agent.id = $1 AND input.id = $2",
    )
    .bind(queued_agent_task_id)
    .bind(queued_input_id)
    .bind(preview_task_id)
    .fetch_one(&app.db)
    .await
    .expect("load former-owner work after transfer");
    assert_eq!(
        transferred_work,
        (
            "waiting".to_string(),
            "queued".to_string(),
            "cancelled".to_string(),
            "settled".to_string(),
        )
    );
    let protected_holds: Vec<(Uuid, String, Option<String>)> = sqlx::query_as(
        "SELECT id, status, hold_reason
         FROM tasks WHERE id IN ($1,$2) ORDER BY id",
    )
    .bind(delivery_unknown_task_id)
    .bind(response_timeout_task_id)
    .fetch_all(&app.db)
    .await
    .expect("load transfer-sensitive task holds");
    assert!(protected_holds.contains(&(
        delivery_unknown_task_id,
        "waiting".to_string(),
        Some("delivery_unknown".to_string()),
    )));
    assert!(protected_holds.contains(&(
        response_timeout_task_id,
        "waiting".to_string(),
        Some("response_timeout".to_string()),
    )));

    let commands_before_resume = runtime.received_host_command_methods().await;
    assert!(commands_before_resume
        .iter()
        .any(|method| method == "vm.stop"));
    assert!(
        !commands_before_resume
            .iter()
            .any(|method| method == "roots.purge"),
        "the host deletes the root at release; transfer purges nothing"
    );
    assert!(!commands_before_resume
        .iter()
        .any(|method| method == "vm.start"));

    let old_owner_resume = owner_client
        .post_json(
            &format!("/api/v1/workspaces/{}/resume", workspace.id),
            &json!({}),
        )
        .await;
    assert_eq!(old_owner_resume.status(), StatusCode::FORBIDDEN);

    let old_owner_session_write = owner_client
        .patch_json(
            &format!("/api/v1/sessions/{session_id}"),
            &json!({"title":"Former owner write"}),
        )
        .await;
    assert_eq!(old_owner_session_write.status(), StatusCode::FORBIDDEN);

    let next_owner_client = app.authenticated_client(&next_owner);
    let new_owner_session_write = next_owner_client
        .patch_json(
            &format!("/api/v1/sessions/{session_id}"),
            &json!({"title":"After transfer"}),
        )
        .await;
    assert_eq!(new_owner_session_write.status(), StatusCode::OK);
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT title FROM chat_sessions WHERE id = $1")
            .bind(session_id)
            .fetch_one(&app.db)
            .await
            .expect("load transferred session title"),
        "After transfer"
    );
    let continued = next_owner_client
        .post_json(
            &format!("/api/v1/tasks/{queued_agent_task_id}/continue"),
            &json!({}),
        )
        .await;
    assert_eq!(continued.status(), StatusCode::OK);
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT status FROM tasks WHERE id = $1")
            .bind(queued_agent_task_id)
            .fetch_one(&app.db)
            .await
            .expect("load transferred task after new-owner Continue"),
        "queued"
    );
    let resumed = next_owner_client
        .post_json(
            &format!("/api/v1/workspaces/{}/resume", workspace.id),
            &json!({"operationId": Uuid::new_v4()}),
        )
        .await;
    assert_eq!(resumed.status(), StatusCode::OK);
    let resumed: Value = resumed.json().await.expect("parse new-owner resume");
    assert_eq!(data(&resumed)["outcome"], "starting");

    app.cleanup().await;
}

/// A transfer's stop follows the same rule (`g5.lifecycle.transfer.failure`):
/// a failed save costs the recorded unsaved work but does not fail the
/// transfer, and the recipient starts from the last complete receipt.
#[tokio::test]
async fn transfer_completes_when_the_former_owners_save_fails() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let next_owner = TestUser::create_with_scopes(
        &app,
        org.id,
        &[
            Scope::WorkspacesRead,
            Scope::WorkspacesWrite,
            Scope::SessionsRead,
            Scope::SessionsWrite,
            Scope::TasksExecute,
        ],
    )
    .await;
    let workspace = TestWorkspace::create(&app, &org).await;
    workspace.attach_parent(&app).await;
    let runtime = TestWorkspaceRuntimePeer::attach_with_flush_failure(
        &app,
        &workspace,
        owner.id,
        "FLUSH_TOO_LARGE",
    )
    .await;

    let transferred = app
        .authenticated_client(&owner)
        .post_json(
            &format!("/api/v1/workspaces/{}/transfer", workspace.id),
            &json!({
                "toUserId": next_owner.id,
                "idempotencyKey": format!("transfer-{}", Uuid::new_v4())
            }),
        )
        .await;
    let status = transferred.status();
    let transferred: Value = transferred.json().await.expect("parse transfer outcome");
    assert_eq!(status, StatusCode::OK, "transfer failed: {transferred}");
    assert_eq!(data(&transferred)["status"], "completed", "{transferred}");

    let persisted: (Uuid, String, Option<chrono::DateTime<chrono::Utc>>) = sqlx::query_as(
        "SELECT workspace.owner_user_id, vm.status, vm.unflushed_loss_at
         FROM workspaces workspace
         JOIN workspace_vms vm ON vm.workspace_id = workspace.id
         WHERE workspace.id = $1",
    )
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .expect("load transferred workspace");
    assert_eq!(persisted.0, next_owner.id);
    assert_eq!(persisted.1, "stopped");
    assert!(persisted.2.is_some(), "the former owner's loss is recorded");
    assert_killed(&vm_stop_commands(&runtime).await);

    app.cleanup().await;
}

#[tokio::test]
async fn transfer_reports_pending_while_the_durable_stop_waits_for_transport() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let next_owner = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let _runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let host_id: Uuid =
        sqlx::query_scalar("SELECT host_id FROM workspace_vms WHERE workspace_id = $1")
            .bind(workspace.id)
            .fetch_one(&app.db)
            .await
            .expect("load runtime host");
    app.state.preview_tunnels.terminate(&host_id).await;

    let response = app
        .authenticated_client(&owner)
        .post_json(
            &format!("/api/v1/workspaces/{}/transfer", workspace.id),
            &json!({
                "toUserId": next_owner.id,
                "idempotencyKey": format!("transfer-{}", Uuid::new_v4())
            }),
        )
        .await;
    let status = response.status();
    let body: Value = response.json().await.expect("parse pending transfer");
    assert_eq!(status, StatusCode::OK, "pending transfer failed: {body}");
    assert_eq!(data(&body)["status"], "pending");
    assert_eq!(data(&body)["completedAt"], Value::Null);
    let persisted: (Uuid, String, String) = sqlx::query_as(
        "SELECT workspace.owner_user_id, transfer.status, vm.status
         FROM workspaces workspace
         JOIN workspace_ownership_transfers transfer ON transfer.workspace_id = workspace.id
         JOIN workspace_vms vm ON vm.workspace_id = workspace.id
         WHERE workspace.id = $1",
    )
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .expect("load pending transfer");
    assert_eq!(
        persisted,
        (owner.id, "pending".to_string(), "stopping".to_string())
    );

    app.cleanup().await;
}

#[tokio::test]
async fn completed_transfer_replay_survives_requester_demotion_and_owner_change() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let requester = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let next_owner = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let transfer_id = Uuid::new_v4();
    let idempotency_key = format!("transfer-{}", Uuid::new_v4());
    let digest = hex::encode(Sha256::digest(
        format!("{}:{}:{}", workspace.id, requester.id, next_owner.id).as_bytes(),
    ));
    sqlx::query(
        "INSERT INTO workspace_ownership_transfers(
             id, org_id, workspace_id, from_user_id, to_user_id, requested_by,
             idempotency_key, request_digest, status, completed_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,'completed',now())",
    )
    .bind(transfer_id)
    .bind(org.id)
    .bind(workspace.id)
    .bind(owner.id)
    .bind(next_owner.id)
    .bind(requester.id)
    .bind(&idempotency_key)
    .bind(digest)
    .execute(&app.db)
    .await
    .expect("seed completed transfer");
    sqlx::query("UPDATE workspaces SET owner_user_id = $2 WHERE id = $1")
        .bind(workspace.id)
        .bind(next_owner.id)
        .execute(&app.db)
        .await
        .expect("apply completed owner change");

    let replay = workspace_vm_service(&app, requester.id, org.id)
        .await
        .transfer(workspace.id, requester.id, next_owner.id, &idempotency_key)
        .await
        .expect("replay completed transfer after requester demotion");
    assert_eq!(replay.transfer_id, transfer_id);
    assert_eq!(replay.status, "completed");
    assert_eq!(replay.from_user_id, owner.id);
    assert_eq!(replay.to_user_id, next_owner.id);

    app.cleanup().await;
}

#[tokio::test]
async fn unfinished_transfer_replay_rechecks_requester_authority() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let requester = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let next_owner = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    sqlx::query("UPDATE org_memberships SET role = 'admin' WHERE org_id = $1 AND user_id = $2")
        .bind(org.id)
        .bind(requester.id)
        .execute(&app.db)
        .await
        .expect("promote transfer requester");
    let workspace = TestWorkspace::create(&app, &org).await;
    let transfer_id = Uuid::new_v4();
    let idempotency_key = format!("transfer-{}", Uuid::new_v4());
    let digest = hex::encode(Sha256::digest(
        format!("{}:{}:{}", workspace.id, requester.id, next_owner.id).as_bytes(),
    ));
    sqlx::query(
        "INSERT INTO workspace_ownership_transfers(
             id, org_id, workspace_id, from_user_id, to_user_id, requested_by,
             idempotency_key, request_digest)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
    )
    .bind(transfer_id)
    .bind(org.id)
    .bind(workspace.id)
    .bind(owner.id)
    .bind(next_owner.id)
    .bind(requester.id)
    .bind(&idempotency_key)
    .bind(digest)
    .execute(&app.db)
    .await
    .expect("seed pending transfer");
    sqlx::query("UPDATE org_memberships SET role = 'member' WHERE org_id = $1 AND user_id = $2")
        .bind(org.id)
        .bind(requester.id)
        .execute(&app.db)
        .await
        .expect("demote transfer requester");

    let error = workspace_vm_service(&app, requester.id, org.id)
        .await
        .transfer(workspace.id, requester.id, next_owner.id, &idempotency_key)
        .await
        .expect_err("unfinished replay must recheck requester role");
    assert!(matches!(
        error,
        g5_contracts::error::G5Error::Forbidden { .. }
    ));
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT status FROM workspace_ownership_transfers WHERE id = $1",
        )
        .bind(transfer_id)
        .fetch_one(&app.db)
        .await
        .expect("load rejected transfer"),
        "aborted"
    );

    app.cleanup().await;
}

#[tokio::test]
async fn pending_transfer_blocks_a_queued_start_intent() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let next_owner = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let intent_id = Uuid::new_v4();
    let now = chrono::Utc::now();
    let intent = json!({
        "id": intent_id,
        "kind": "resume",
        "owner_user_id": owner.id,
        "lifecycle_generation": 0,
        "status": "queued_for_capacity",
        "created_at": now,
        "expires_at": now + chrono::Duration::minutes(15),
        "attempts": 0,
    });
    sqlx::query(
        "INSERT INTO workspace_vms(org_id, workspace_id, status, start_intent)
         VALUES ($1, $2, 'stopped', $3)",
    )
    .bind(org.id)
    .bind(workspace.id)
    .bind(&intent)
    .execute(&app.db)
    .await
    .expect("seed queued start intent");
    let transfer_id = Uuid::new_v4();
    let idempotency_key = format!("transfer-{}", Uuid::new_v4());
    let digest = hex::encode(Sha256::digest(
        format!("{}:{}:{}", workspace.id, owner.id, next_owner.id).as_bytes(),
    ));
    sqlx::query(
        "INSERT INTO workspace_ownership_transfers(
             id, org_id, workspace_id, from_user_id, to_user_id, requested_by,
             idempotency_key, request_digest)
         VALUES ($1,$2,$3,$4,$5,$4,$6,$7)",
    )
    .bind(transfer_id)
    .bind(org.id)
    .bind(workspace.id)
    .bind(owner.id)
    .bind(next_owner.id)
    .bind(idempotency_key)
    .bind(digest)
    .execute(&app.db)
    .await
    .expect("seed pending transfer");

    let service = workspace_vm_service(&app, owner.id, org.id).await;
    let error = service
        .resume(
            workspace.id,
            owner.id,
            intent_id,
            g5_api::services::workspace_vm_service::StartKind::Resume,
        )
        .await
        .expect_err("pending transfer must fence direct resume");
    assert!(matches!(
        error,
        g5_contracts::error::G5Error::WorkspaceStopping
    ));
    assert_eq!(
        service
            .reconcile_start_queue(10)
            .await
            .expect("reconcile start queue"),
        0
    );
    let vm: (String, String) = sqlx::query_as(
        "SELECT status, start_intent ->> 'status'
         FROM workspace_vms WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .expect("load fenced start intent");
    assert_eq!(
        vm,
        ("stopped".to_string(), "queued_for_capacity".to_string())
    );

    app.cleanup().await;
}

#[tokio::test]
async fn expired_start_deadline_keeps_an_incarnation_the_host_still_reports() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let now = chrono::Utc::now();
    let intent = json!({
        "id": Uuid::new_v4(),
        "kind": "resume",
        "owner_user_id": owner.id,
        "lifecycle_generation": 0,
        "status": "starting",
        "created_at": now - chrono::Duration::minutes(1),
        "expires_at": now + chrono::Duration::minutes(14),
        "attempts": 1,
    });
    sqlx::query(
        "UPDATE workspace_vms
         SET status = 'starting', start_intent = $2,
             start_deadline_at = now() - interval '1 second'
         WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .bind(intent)
    .execute(&app.db)
    .await
    .expect("expire the start acquisition deadline");

    let reconciled = workspace_vm_service(&app, owner.id, org.id)
        .await
        .reconcile_start_queue(10)
        .await
        .expect("reconcile observed start");
    assert_eq!(reconciled, 0);
    let state: (
        String,
        Option<Uuid>,
        Option<String>,
        chrono::DateTime<chrono::Utc>,
    ) = sqlx::query_as(
        "SELECT status, host_id, vm_id, start_deadline_at
         FROM workspace_vms WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .expect("load reconciled start");
    assert_eq!(state.0, "starting");
    assert_eq!(state.1, Some(runtime.host_id()));
    assert_eq!(state.2, Some(runtime.vm_id().to_string()));
    assert!(state.3 > chrono::Utc::now());
    assert!(runtime
        .received_host_command_methods()
        .await
        .iter()
        .any(|method| method == g5_internal_api::host_ctl::method::VM_STATUS));

    app.cleanup().await;
}

#[tokio::test]
async fn workspace_delete_stops_and_removes_live_git_refs() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let client = app.authenticated_client(&owner);

    let deleted = client
        .post_json(
            &format!("/api/v1/workspaces/{}/delete", workspace.id),
            &json!({"operationId": Uuid::new_v4()}),
        )
        .await;
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);

    let row: (Option<chrono::DateTime<chrono::Utc>>, i64) = sqlx::query_as(
        "SELECT deleted_at, (SELECT COUNT(*) FROM workspace_vms WHERE workspace_id = $1) \
         FROM workspaces WHERE id = $1",
    )
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .expect("load workspace tombstone");
    assert!(row.0.is_some());
    assert_eq!(row.1, 0);

    let commands = runtime.received_host_command_methods().await;
    assert!(commands.iter().any(|method| method == "vm.stop"));
    assert!(
        !commands.iter().any(|method| method == "roots.purge"),
        "the host deletes the root at release; delete purges nothing"
    );

    app.cleanup().await;
}

#[tokio::test]
async fn forced_workspace_delete_does_not_wait_out_a_new_nudge_after_flush_failure() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach_with_flush_failure(
        &app,
        &workspace,
        owner.id,
        "FLUSH_TOO_LARGE",
    )
    .await;
    sqlx::query(
        "UPDATE workspaces
         SET lifecycle_generation = lifecycle_generation + 1
         WHERE id = $1",
    )
    .bind(workspace.id)
    .execute(&app.db)
    .await
    .expect("advance lifecycle for wedged stop");
    sqlx::query(
        "UPDATE workspace_vms
         SET status = 'stopping', stop_reason = 'user',
             stop_operation_id = $2,
             stop_generation = (SELECT lifecycle_generation FROM workspaces WHERE id = $1),
             stop_deadline_at = now() + interval '10 minutes',
             stop_retry_at = now() + interval '5 seconds',
             started_at = now() - interval '1 hour'
         WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .bind(Uuid::new_v4())
    .execute(&app.db)
    .await
    .expect("seed wedged stop operation");

    let deleted = app
        .authenticated_client(&owner)
        .post_json(
            &format!("/api/v1/workspaces/{}/delete", workspace.id),
            &json!({"operationId":Uuid::new_v4(),"force":true}),
        )
        .await;
    let status = deleted.status();
    let body = deleted.text().await.expect("read forced delete response");
    assert_eq!(
        status,
        StatusCode::NO_CONTENT,
        "forced delete failed: {body}"
    );

    let deletion_loss: Option<Value> =
        sqlx::query_scalar("SELECT deletion_loss FROM workspaces WHERE id = $1")
            .bind(workspace.id)
            .fetch_one(&app.db)
            .await
            .expect("load forced-delete loss evidence");
    assert!(deletion_loss.is_some());
    assert!(runtime
        .received_host_command_methods()
        .await
        .iter()
        .any(|method| method == "vm.stop"));

    app.cleanup().await;
}

#[tokio::test]
async fn forced_workspace_delete_supersedes_an_active_pending_nudge() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach_with_flush_failure(
        &app,
        &workspace,
        owner.id,
        "FLUSH_TOO_LARGE",
    )
    .await;
    let service = workspace_vm_service(&app, owner.id, org.id).await;
    service
        .stop_as_system_for_tests(
            workspace.id,
            Uuid::new_v4(),
            g5_api::services::workspace_vm_service::StopReason::Drain,
            Some(chrono::Utc::now() + chrono::Duration::minutes(5)),
        )
        .await
        .expect("schedule operator nudge");

    let deleted = app
        .authenticated_client(&owner)
        .post_json(
            &format!("/api/v1/workspaces/{}/delete", workspace.id),
            &json!({"operationId":Uuid::new_v4(),"force":true}),
        )
        .await;
    let status = deleted.status();
    let body = deleted.text().await.expect("read forced delete response");
    assert_eq!(
        status,
        StatusCode::NO_CONTENT,
        "forced delete failed: {body}"
    );
    assert!(runtime
        .received_host_command_methods()
        .await
        .iter()
        .any(|method| method == "vm.stop"));

    app.cleanup().await;
}

#[tokio::test]
async fn base_workspace_has_no_vm_lifecycle() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let base_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO workspaces \
         (id, org_id, name, is_base, owner_user_id, created_by) \
         VALUES ($1, $2, 'Base', true, $3, $3)",
    )
    .bind(base_id)
    .bind(org.id)
    .bind(owner.id)
    .execute(&app.db)
    .await
    .expect("insert base workspace");
    let client = app.authenticated_client(&owner);

    let view = client
        .get(&format!("/api/v1/workspaces/{base_id}/vm"))
        .await;
    assert_eq!(view.status(), StatusCode::OK);
    let view: Value = view.json().await.expect("parse base VM view");
    assert_eq!(data(&view)["status"], "stopped");
    assert!(data(&view)["vmId"].is_null());

    for action in ["resume", "stop"] {
        let response = client
            .post_json(
                &format!("/api/v1/workspaces/{base_id}/{action}"),
                &json!({}),
            )
            .await;
        assert_eq!(response.status(), StatusCode::CONFLICT, "action {action}");
    }
    let vm_rows: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM workspace_vms WHERE workspace_id = $1")
            .bind(base_id)
            .fetch_one(&app.db)
            .await
            .expect("count base VM rows");
    assert_eq!(vm_rows, 0);

    app.cleanup().await;
}

#[tokio::test]
async fn api_token_principal_matrix() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;

    let user_view = app
        .authenticated_client(&owner)
        .get(&format!("/api/v1/workspaces/{}/vm", workspace.id))
        .await;
    assert_eq!(user_view.status(), StatusCode::OK);

    let host_token = test_jwt_minter()
        .mint_host_jwt(runtime.host_id(), org.id, Duration::from_secs(3600))
        .expect("mint host token");
    let host_on_user_route = app
        .client
        .get(format!(
            "{}/api/v1/workspaces/{}/vm",
            app.base_url, workspace.id
        ))
        .bearer_auth(host_token)
        .send()
        .await
        .expect("host request");
    assert_eq!(host_on_user_route.status(), StatusCode::UNAUTHORIZED);

    let incarnation_minter = g5_auth::JwtMinter::new(TEST_PRIVATE_KEY_PEM, "g5-api-test", "g5")
        .expect("construct incarnation minter");
    let incarnation_token = incarnation_minter
        .mint_workspace_incarnation(org.id, workspace.id, owner.id, runtime.vm_id(), 1, 3600)
        .expect("mint incarnation token");
    let incarnation_on_user_route = app
        .client
        .get(format!(
            "{}/api/v1/workspaces/{}/vm",
            app.base_url, workspace.id
        ))
        .bearer_auth(&incarnation_token)
        .send()
        .await
        .expect("incarnation request");
    assert_eq!(incarnation_on_user_route.status(), StatusCode::UNAUTHORIZED);

    let internal_path = format!(
        "{}/api/internal/workspace-incarnations/{}/flushes/start",
        app.base_url, workspace.id
    );
    let user_on_incarnation_route = app
        .client
        .post(&internal_path)
        .bearer_auth(&owner.jwt)
        .json(&json!({"reason": "shutdown", "idempotency_key": "user-confusion"}))
        .send()
        .await
        .expect("user request to incarnation route");
    let user_incarnation_status = user_on_incarnation_route.status();
    let user_incarnation_body = user_on_incarnation_route
        .text()
        .await
        .expect("read user/incarnation confusion response");
    assert_eq!(
        user_incarnation_status,
        StatusCode::UNAUTHORIZED,
        "user token confusion response: {user_incarnation_body}"
    );

    let correct = app
        .client
        .post(&internal_path)
        .bearer_auth(&incarnation_token)
        .json(&json!({"reason": "shutdown", "idempotency_key": "principal-matrix"}))
        .send()
        .await
        .expect("current incarnation request");
    assert_eq!(correct.status(), StatusCode::OK);

    let stale_vm_token = incarnation_minter
        .mint_workspace_incarnation(org.id, workspace.id, owner.id, Uuid::new_v4(), 1, 3600)
        .expect("mint stale VM token");
    let stale_vm = app
        .client
        .post(&internal_path)
        .bearer_auth(stale_vm_token)
        .json(&json!({"reason": "shutdown", "idempotency_key": "stale-vm"}))
        .send()
        .await
        .expect("stale VM request");
    assert_eq!(stale_vm.status(), StatusCode::CONFLICT);

    let stale_epoch_token = incarnation_minter
        .mint_workspace_incarnation(org.id, workspace.id, owner.id, runtime.vm_id(), 2, 3600)
        .expect("mint stale epoch token");
    let stale_epoch = app
        .client
        .post(&internal_path)
        .bearer_auth(stale_epoch_token)
        .json(&json!({"reason": "shutdown", "idempotency_key": "stale-epoch"}))
        .send()
        .await
        .expect("stale epoch request");
    assert_eq!(stale_epoch.status(), StatusCode::CONFLICT);

    let wrong_workspace_token = incarnation_minter
        .mint_workspace_incarnation(org.id, Uuid::new_v4(), owner.id, runtime.vm_id(), 1, 3600)
        .expect("mint wrong-workspace token");
    let wrong_workspace = app
        .client
        .post(&internal_path)
        .bearer_auth(wrong_workspace_token)
        .json(&json!({"reason": "shutdown", "idempotency_key": "wrong-workspace"}))
        .send()
        .await
        .expect("wrong workspace request");
    assert_eq!(wrong_workspace.status(), StatusCode::UNAUTHORIZED);

    app.cleanup().await;
}

#[tokio::test]
async fn workspace_incarnation_can_clone_and_push_an_authoritative_workspace_repo() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let token = g5_auth::JwtMinter::new(TEST_PRIVATE_KEY_PEM, "g5-api-test", "g5")
        .expect("construct incarnation minter")
        .mint_workspace_incarnation(org.id, workspace.id, owner.id, runtime.vm_id(), 1, 3600)
        .expect("mint incarnation token");

    let response = app
        .client
        .get(format!(
            "{}/git/{}/info/refs?service=git-upload-pack",
            app.base_url, workspace.repo_id
        ))
        .basic_auth("x-access-token", Some(&token))
        .send()
        .await
        .expect("workspace incarnation Git advertisement request");
    let status = response.status();
    let body = response
        .text()
        .await
        .expect("read workspace incarnation Git response");
    assert_eq!(status, StatusCode::OK, "Git response: {body}");
    assert!(
        body.contains("service=git-upload-pack"),
        "Git advertisement must identify upload-pack: {body}"
    );

    let checkout_root = tempfile::tempdir().expect("create workspace Git checkout root");
    let checkout = checkout_root.path().join("repo");
    let remote_url = format!("{}/git/{}", app.base_url, workspace.repo_id);
    let auth = base64::engine::general_purpose::STANDARD.encode(format!("x-access-token:{token}"));
    let auth_header = format!("http.extraHeader=Authorization: Basic {auth}");
    let clone = Command::new("git")
        .args(["-c", &auth_header, "clone", "--branch", &workspace.name])
        .arg(&remote_url)
        .arg(&checkout)
        .output()
        .await
        .expect("run authenticated workspace clone");
    assert!(
        clone.status.success(),
        "workspace clone failed: {}",
        String::from_utf8_lossy(&clone.stderr)
    );
    for args in [
        vec!["config", "user.email", "workspace@g5.test"],
        vec!["config", "user.name", "Workspace Runtime"],
    ] {
        let configured = Command::new("git")
            .args(args)
            .current_dir(&checkout)
            .output()
            .await
            .expect("configure workspace checkout");
        assert!(configured.status.success());
    }
    std::fs::write(
        checkout.join("runtime.txt"),
        "pushed by workspace incarnation\n",
    )
    .expect("write workspace runtime change");
    for args in [
        vec!["add", "runtime.txt"],
        vec!["commit", "-m", "workspace runtime push"],
    ] {
        let committed = Command::new("git")
            .args(args)
            .current_dir(&checkout)
            .output()
            .await
            .expect("commit workspace runtime change");
        assert!(
            committed.status.success(),
            "workspace commit failed: {}",
            String::from_utf8_lossy(&committed.stderr)
        );
    }
    let destination = format!("HEAD:refs/heads/{}", workspace.name);
    let pushed = Command::new("git")
        .args(["-c", &auth_header, "push", "origin", &destination])
        .current_dir(&checkout)
        .output()
        .await
        .expect("push workspace runtime change");
    assert!(
        pushed.status.success(),
        "workspace push failed: {}",
        String::from_utf8_lossy(&pushed.stderr)
    );

    app.cleanup().await;
}

/// A flush pushes its two immutable create-only refs in one receive-pack
/// (design v9 §5.5). The post-receive check used to mark every accepted
/// immutable ref stale, because its success branch was gated on
/// `!immutable_create` and the fallback compared the freshly created ref with
/// the zero old-oid: the first push of every flush created the refs and still
/// answered 409, and the guest only got through when a retry replayed both
/// refs at the same OIDs. The first push and the same-OID replay must both
/// succeed, and the refs must land at the pushed values.
#[tokio::test]
async fn workspace_incarnation_flush_refs_push_once_and_replay_at_the_same_oids() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let token = g5_auth::JwtMinter::new(TEST_PRIVATE_KEY_PEM, "g5-api-test", "g5")
        .expect("construct incarnation minter")
        .mint_workspace_incarnation(org.id, workspace.id, owner.id, runtime.vm_id(), 1, 3600)
        .expect("mint incarnation token");

    // The flush record makes the refs known to the incarnation.
    let started = app
        .client
        .post(format!(
            "{}/api/internal/workspace-incarnations/{}/flushes/start",
            app.base_url, workspace.id
        ))
        .bearer_auth(&token)
        .json(&json!({"reason": "shutdown", "idempotency_key": "flush-refs-push"}))
        .send()
        .await
        .expect("flush start request");
    let started_status = started.status();
    let started_body: Value = started.json().await.unwrap_or(Value::Null);
    assert_eq!(
        started_status,
        StatusCode::OK,
        "flush start: {started_body}"
    );
    let flush_id = started_body["flush_id"]
        .as_str()
        .or_else(|| started_body["flushId"].as_str())
        .expect("flush start carries the flush id")
        .to_string();

    let checkout_root = tempfile::tempdir().expect("create workspace Git checkout root");
    let checkout = checkout_root.path().join("repo");
    let remote_url = format!("{}/git/{}", app.base_url, workspace.repo_id);
    let auth = base64::engine::general_purpose::STANDARD.encode(format!("x-access-token:{token}"));
    let auth_header = format!("http.extraHeader=Authorization: Basic {auth}");
    let clone = Command::new("git")
        .args(["-c", &auth_header, "clone", "--branch", &workspace.name])
        .arg(&remote_url)
        .arg(&checkout)
        .output()
        .await
        .expect("run authenticated workspace clone");
    assert!(
        clone.status.success(),
        "workspace clone failed: {}",
        String::from_utf8_lossy(&clone.stderr)
    );
    let git = |args: &[&str]| {
        let mut command = Command::new("git");
        command
            .args(["-c", &auth_header])
            .args(args)
            .current_dir(&checkout)
            .env("GIT_AUTHOR_NAME", "Workspace Runtime")
            .env("GIT_AUTHOR_EMAIL", "workspace@g5.test")
            .env("GIT_COMMITTER_NAME", "Workspace Runtime")
            .env("GIT_COMMITTER_EMAIL", "workspace@g5.test");
        command
    };
    let branch_oid = String::from_utf8_lossy(
        &git(&["rev-parse", "HEAD"])
            .output()
            .await
            .expect("rev-parse")
            .stdout,
    )
    .trim()
    .to_string();
    std::fs::write(checkout.join("dirty.txt"), "unflushed edit\n").expect("write dirty file");
    for args in [
        vec!["add", "dirty.txt"],
        vec!["commit", "-m", "flush snapshot"],
    ] {
        let done = git(&args).output().await.expect("snapshot commit");
        assert!(
            done.status.success(),
            "{}",
            String::from_utf8_lossy(&done.stderr)
        );
    }
    let snapshot_oid = String::from_utf8_lossy(
        &git(&["rev-parse", "HEAD"])
            .output()
            .await
            .expect("rev-parse")
            .stdout,
    )
    .trim()
    .to_string();

    let prefix = format!("refs/g5/flush/{}/{flush_id}", workspace.id);
    let snapshot_spec = format!("{snapshot_oid}:{prefix}/snapshot");
    let branch_spec = format!("{branch_oid}:{prefix}/branch");
    for attempt in ["first push", "same-OID replay"] {
        let pushed = git(&[
            "push",
            "--porcelain",
            "origin",
            &snapshot_spec,
            &branch_spec,
        ])
        .output()
        .await
        .expect("push flush refs");
        assert!(
            pushed.status.success(),
            "{attempt} of the flush refs must succeed: {}{}",
            String::from_utf8_lossy(&pushed.stdout),
            String::from_utf8_lossy(&pushed.stderr)
        );
    }

    let repo_path = app.repo_path(workspace.repo_id).await;
    for (leaf, expected) in [("snapshot", &snapshot_oid), ("branch", &branch_oid)] {
        let tip = Command::new("git")
            .args(["-C"])
            .arg(&repo_path)
            .args(["rev-parse", "--verify", &format!("{prefix}/{leaf}")])
            .output()
            .await
            .expect("read the pushed flush ref");
        assert_eq!(
            String::from_utf8_lossy(&tip.stdout).trim(),
            expected.as_str(),
            "{leaf} ref must land at the pushed OID: {}",
            String::from_utf8_lossy(&tip.stderr)
        );
    }

    app.cleanup().await;
}

/// A second API replica: same database, no tunnel to the host.
async fn tunnel_less_replica_service(
    app: &TestApp,
    actor_user_id: Uuid,
    org_id: Uuid,
) -> g5_api::services::WorkspaceVmService {
    use g5_api::services::{WorkspaceRuntime, WorkspaceVmService, WorkspaceVmServiceConfig};
    use g5_api::ws::preview_tunnel::TunnelRegistry;

    let registry = std::sync::Arc::new(TunnelRegistry::new());
    let db = app.scoped_db_for_user_org(actor_user_id, org_id).await;
    let scope = db.reopener();
    db.rollback().await.expect("close captured lifecycle scope");
    WorkspaceVmService::new(WorkspaceVmServiceConfig {
        system_pool: app.state.system_pool.clone(),
        scope,
        hosts: registry.clone(),
        minter: app
            .state
            .jwt_minter
            .clone()
            .expect("test app configures a lifecycle JWT minter"),
        runtime: std::sync::Arc::new(WorkspaceRuntime::new(
            app.state.system_pool.clone(),
            registry,
            app.state.repo_storage.clone(),
        )),
        agent_work: app.state.agent_work.clone(),
        repo_storage: app.state.repo_storage.clone(),
        git_external_url: app.state.git_external_url.clone(),
        ws_hub: app.state.event_bus.ws_hub().clone(),
    })
}

/// Past the save window only the tunnel owner can send the kill, so a
/// replica without the tunnel still yields and leaves the stop due at once.
/// Before, it recorded the loss itself, failed to reach the host and pushed
/// the shared retry time five seconds out on each of its reconciler ticks,
/// which could keep the owner's ticks from ever claiming the stop.
#[tokio::test]
async fn expired_stop_on_a_replica_without_the_host_tunnel_still_reaches_the_owner() {
    use g5_api::services::workspace_vm_service::StopReason;

    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    // Host control sets a running VM's start time when it reports ready; the
    // scripted peer's row has none.
    sqlx::query("UPDATE workspace_vms SET started_at = now() WHERE workspace_id = $1")
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .expect("record the VM start");
    let foreign = tunnel_less_replica_service(&app, owner.id, org.id).await;
    let connected = workspace_vm_service(&app, owner.id, org.id).await;

    let admitted = foreign
        .stop(
            workspace.id,
            owner.id,
            Uuid::new_v4(),
            StopReason::User,
            Some(chrono::Utc::now() - chrono::TimeDelta::seconds(1)),
        )
        .await
        .expect("a replica without the tunnel accepts the expired stop");
    assert_eq!(admitted.status, "stopping");

    // Both replicas' reconcilers tick; the tunnel-less one goes first.
    for _ in 0..3 {
        foreign
            .reconcile_stopping(16)
            .await
            .expect("tunnel-less replica reconciles");
        connected
            .reconcile_stopping(16)
            .await
            .expect("tunnel owner reconciles");
    }
    let view = connected.view(workspace.id).await.expect("view");
    assert_eq!(view.status, "stopped", "{view:?}");
    assert!(view.unflushed_loss_at.is_some(), "{view:?}");
    assert!(
        runtime.received_rpc_methods().await.is_empty(),
        "no guest call after the save window"
    );
    assert_killed(&vm_stop_commands(&runtime).await);

    app.cleanup().await;
}

/// Host tunnels are process-local while the lifecycle reconciler runs on
/// every API replica. A replica that does not hold the host's tunnel must
/// leave the durable stop to the owner instead of driving it into transport
/// backoff (observed in Cloud E2E build 820b0a62: `stop_transport_retry` /
/// `HOST_TUNNEL_UNAVAILABLE` for three minutes while the owning replica's
/// tunnel stayed connected).
#[tokio::test]
async fn stop_on_a_replica_without_the_host_tunnel_yields_to_the_tunnel_owner() {
    use g5_api::services::workspace_vm_service::StopReason;

    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let foreign = tunnel_less_replica_service(&app, owner.id, org.id).await;

    let operation_id = Uuid::new_v4();
    let outcome = foreign
        .stop(workspace.id, owner.id, operation_id, StopReason::User, None)
        .await
        .expect("a replica without the tunnel accepts the durable stop");
    assert_eq!(outcome.status, "stopping");
    assert!(
        runtime.received_rpc_methods().await.is_empty(),
        "the tunnel-less replica must not reach the dispatcher"
    );
    assert!(
        !runtime
            .received_host_command_methods()
            .await
            .iter()
            .any(|method| method == "vm.stop"),
        "the tunnel-less replica must not send vm.stop"
    );
    let (status, reason, retry_soon): (String, Option<String>, bool) = sqlx::query_as(
        "SELECT status, last_error ->> 'reason',
                stop_retry_at <= now() + interval '3 seconds'
         FROM workspace_vms WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .expect("read durable stop state");
    assert_eq!(status, "stopping");
    assert_eq!(reason.as_deref(), Some("stop_awaiting_tunnel_owner"));
    assert!(
        retry_soon,
        "the owner must be able to claim it on its next tick"
    );

    // A second attempt on the same replica stays a yield, not a transport retry.
    let again = foreign
        .stop(workspace.id, owner.id, operation_id, StopReason::User, None)
        .await
        .expect("replayed stop on the tunnel-less replica");
    assert_eq!(again.status, "stopping");

    // The replica holding the tunnel completes it through the reconciler.
    sqlx::query("UPDATE workspace_vms SET stop_retry_at = now() WHERE workspace_id = $1")
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .expect("make the stop immediately eligible");
    let owner_replica = workspace_vm_service(&app, owner.id, org.id).await;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        owner_replica
            .reconcile_stopping(16)
            .await
            .expect("owner replica reconciles stopping rows");
        let view = owner_replica.view(workspace.id).await.expect("view");
        if view.status == "stopped" {
            assert!(
                view.unflushed_loss_at.is_none(),
                "no loss recorded: {view:?}"
            );
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "owner replica did not complete the stop: {view:?}"
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    let commands = runtime.received_host_command_methods().await;
    assert!(commands.iter().any(|method| method == "vm.stop"));

    app.cleanup().await;
}

/// A VM checks out one committed cut (`g5.fork.source.cut`): start reads the
/// member heads under the commit fence and refuses retryably while another
/// holder keeps it past the bound.
#[tokio::test]
async fn resume_reads_one_committed_cut() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let url = format!("/api/v1/workspaces/{}/resume", workspace.id);
    let holder = helpers::hold_commit_fence(&app, workspace.id).await;
    let response = app
        .authenticated_client(&owner)
        .post_json(&url, &json!({"operationId": Uuid::new_v4()}))
        .await;
    let status = response.status();
    let body = response.text().await.unwrap();
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(body.contains("WORKSPACE_COMMIT_FENCE_BUSY"), "{body}");
    holder.rollback().await.unwrap();
    let response = app
        .authenticated_client(&owner)
        .post_json(&url, &json!({"operationId": Uuid::new_v4()}))
        .await;
    let body = response.text().await.unwrap();
    // This fixture has no workspace image; the retry only has to pass the fence.
    assert!(!body.contains("WORKSPACE_COMMIT_FENCE_BUSY"), "{body}");
    app.cleanup().await;
}

/// A start that waits for capacity says why: no ready host, or the org's VM
/// quota. The VM view reads `queued_for_capacity` while it waits, and Stop
/// cancels the queued start instead of leaving it to start later.
#[tokio::test]
async fn queued_start_reports_why_it_waits_and_stop_cancels_it() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    workspace.attach_parent(&app).await;
    let image = format!(
        "mirror.gcr.io/library/buildpack-deps@sha256:{}",
        "a".repeat(64)
    );
    g5_api::services::worker_host_service::WorkerHostService::new()
        .reconcile_unconfigured_org_defaults(&app.db, "local-dev", &image)
        .await
        .expect("configure the org image");
    let resume = |operation: Uuid| {
        let client = app.authenticated_client(&owner);
        let url = format!("/api/v1/workspaces/{}/resume", workspace.id);
        async move {
            let response = client
                .post_json(&url, &json!({"operationId": operation}))
                .await;
            let status = response.status();
            let body: Value = response.json().await.expect("parse resume response");
            assert_eq!(status, StatusCode::OK, "resume failed: {body}");
            body
        }
    };

    // No host is ready.
    let body = resume(Uuid::new_v4()).await;
    assert_eq!(data(&body)["outcome"], "queued_for_capacity");
    assert_eq!(data(&body)["vm"]["status"], "queued_for_capacity", "{body}");
    assert_eq!(
        data(&body)["vm"]["lastError"]["reason"],
        "no_ready_host",
        "{body}"
    );

    // Stop cancels the queued start.
    let response = app
        .authenticated_client(&owner)
        .post_json(
            &format!("/api/v1/workspaces/{}/stop", workspace.id),
            &json!({"operationId": Uuid::new_v4()}),
        )
        .await;
    let stop_status = response.status();
    let stop_body: Value = response.json().await.unwrap_or(Value::Null);
    assert!(stop_status.is_success(), "stop failed: {stop_body}");
    let intent: Value =
        sqlx::query_scalar("SELECT start_intent FROM workspace_vms WHERE workspace_id = $1")
            .bind(workspace.id)
            .fetch_one(&app.db)
            .await
            .expect("load cancelled start intent");
    assert_eq!(intent["status"], "cancelled");
    let view = app
        .authenticated_client(&owner)
        .get(&format!("/api/v1/workspaces/{}/vm", workspace.id))
        .await;
    let view: Value = view.json().await.expect("parse vm view");
    assert_eq!(data(&view)["status"], "stopped", "{view}");

    // At the org's VM quota, the wait names the quota.
    sqlx::query(
        "INSERT INTO configs(org_id, scope, scope_id, key, value)
         VALUES ($1, 'org', $1, 'max_running_vms', '0'::jsonb)
         ON CONFLICT (org_id, scope, scope_id, key)
         DO UPDATE SET value = EXCLUDED.value",
    )
    .bind(org.id)
    .execute(&app.db)
    .await
    .expect("set the org VM quota to zero");
    let body = resume(Uuid::new_v4()).await;
    assert_eq!(data(&body)["outcome"], "queued_for_capacity");
    assert_eq!(data(&body)["vm"]["status"], "queued_for_capacity", "{body}");
    assert_eq!(
        data(&body)["vm"]["lastError"]["reason"],
        "org_vm_quota",
        "{body}"
    );
    assert_eq!(data(&body)["vm"]["lastError"]["limit"], 0, "{body}");

    // A failed VM's new start also reads as waiting, not as the old failure.
    sqlx::query(
        "UPDATE workspace_vms SET status = 'failed', start_intent = NULL WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .execute(&app.db)
    .await
    .expect("mark the VM failed");
    let body = resume(Uuid::new_v4()).await;
    assert_eq!(data(&body)["outcome"], "queued_for_capacity");
    assert_eq!(data(&body)["vm"]["status"], "queued_for_capacity", "{body}");
    app.cleanup().await;
}

/// A host that still holds another VM of the workspace refuses the start as
/// workspace-busy. The start queues and says so, rather than failing.
#[tokio::test]
async fn host_workspace_busy_queues_the_start_and_says_why() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    workspace.attach_parent(&app).await;
    let runtime = TestWorkspaceRuntimePeer::attach_with_vm_start_result(
        &app,
        &workspace,
        owner.id,
        g5_internal_api::host_ctl::CommandResult::WorkspaceBusy,
    )
    .await;
    let client = app.authenticated_client(&owner);
    let stopped = client
        .post_json(
            &format!("/api/v1/workspaces/{}/stop", workspace.id),
            &json!({"operationId": Uuid::new_v4()}),
        )
        .await;
    assert_eq!(stopped.status(), StatusCode::OK);

    let resumed = client
        .post_json(
            &format!("/api/v1/workspaces/{}/resume", workspace.id),
            &json!({"operationId": Uuid::new_v4()}),
        )
        .await;
    let resumed_status = resumed.status();
    let resumed: Value = resumed.json().await.expect("parse resume outcome");
    assert_eq!(resumed_status, StatusCode::OK, "resume failed: {resumed}");
    assert_eq!(
        data(&resumed)["outcome"],
        "queued_for_capacity",
        "{resumed}"
    );
    assert_eq!(
        data(&resumed)["vm"]["status"],
        "queued_for_capacity",
        "{resumed}"
    );
    assert_eq!(
        data(&resumed)["vm"]["lastError"],
        json!({"reason": "host_capacity_refused", "host_reason": "workspace_busy"}),
        "{resumed}"
    );
    let intent_status: String = sqlx::query_scalar(
        "SELECT start_intent ->> 'status' FROM workspace_vms WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .expect("load start intent status");
    assert_eq!(intent_status, "queued_for_capacity");
    let excluded: bool = sqlx::query_scalar(
        "SELECT (start_intent -> 'host_refusals' ->> $2)::timestamptz > now()
         FROM workspace_vms WHERE workspace_id=$1",
    )
    .bind(workspace.id)
    .bind(runtime.host_id().to_string())
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(excluded, "a genuine host refusal must keep its cooldown");
    app.cleanup().await;
}

/// Reconciliation replays only the queued start it captured. After Stop
/// cancels it and a newer start is created and cancelled too, the delayed
/// retry must not bring the old start back.
#[tokio::test]
async fn delayed_reconciliation_cannot_revive_a_superseded_start() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    workspace.attach_parent(&app).await;
    let image = format!(
        "mirror.gcr.io/library/buildpack-deps@sha256:{}",
        "a".repeat(64)
    );
    g5_api::services::worker_host_service::WorkerHostService::new()
        .reconcile_unconfigured_org_defaults(&app.db, "local-dev", &image)
        .await
        .expect("configure the org image");
    let resume = |operation: Uuid| {
        let client = app.authenticated_client(&owner);
        let url = format!("/api/v1/workspaces/{}/resume", workspace.id);
        async move {
            let response = client
                .post_json(&url, &json!({"operationId": operation}))
                .await;
            assert_eq!(response.status(), StatusCode::OK);
        }
    };
    let stop = || {
        let client = app.authenticated_client(&owner);
        let url = format!("/api/v1/workspaces/{}/stop", workspace.id);
        async move {
            let response = client
                .post_json(&url, &json!({"operationId": Uuid::new_v4()}))
                .await;
            assert!(response.status().is_success());
        }
    };
    let intent_id = |label: &'static str| {
        let db = app.db.clone();
        let workspace_id = workspace.id;
        async move {
            let row: (String, String) = sqlx::query_as(
                "SELECT start_intent ->> 'id', start_intent ->> 'status' FROM workspace_vms WHERE workspace_id = $1",
            )
            .bind(workspace_id)
            .fetch_one(&db)
            .await
            .expect(label);
            row
        }
    };

    // Reconciliation captures queued start I; Stop cancels it; a new start J
    // is queued and cancelled in turn.
    resume(Uuid::new_v4()).await;
    let (captured, status) = intent_id("load captured start").await;
    assert_eq!(status, "queued_for_capacity");
    stop().await;
    resume(Uuid::new_v4()).await;
    stop().await;
    let (newer, status) = intent_id("load newer start").await;
    assert_ne!(newer, captured);
    assert_eq!(status, "cancelled");

    let service = workspace_vm_service(&app, owner.id, org.id).await;
    let error = service
        .resume_queued(
            workspace.id,
            owner.id,
            captured.parse().unwrap(),
            g5_api::services::workspace_vm_service::StartKind::Resume,
        )
        .await
        .expect_err("the superseded start is not replayed");
    assert!(error.to_string().contains("superseded"), "{error}");
    let (current, status) = intent_id("load current start").await;
    assert_eq!(current, newer, "the newer intent stays current");
    assert_eq!(status, "cancelled", "and stays cancelled");
    app.cleanup().await;
}

/// The stop's row state while its execution is somewhere between admission
/// and release.
async fn stop_claim(app: &TestApp, workspace_id: Uuid) -> (String, Option<f64>) {
    sqlx::query_as(
        "SELECT status, EXTRACT(EPOCH FROM (stop_retry_at - now()))::float8
         FROM workspace_vms WHERE workspace_id = $1",
    )
    .bind(workspace_id)
    .fetch_one(&app.db)
    .await
    .expect("load the stop's claim")
}

async fn wait_for_guest_rpc(runtime: &TestWorkspaceRuntimePeer, method: &str) {
    for _ in 0..200 {
        if runtime
            .received_rpc_methods()
            .await
            .iter()
            .any(|received| received == method)
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the guest never received {method}");
}

async fn wait_for_vm_status(app: &TestApp, workspace_id: Uuid, expected: &str) {
    for _ in 0..400 {
        let (status, _) = stop_claim(app, workspace_id).await;
        if status == expected {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the workspace VM never reached {expected}");
}

fn assert_clean_release(stop_commands: &[Value]) {
    assert_eq!(stop_commands.len(), 1, "one release: {stop_commands:?}");
    assert_eq!(stop_commands[0]["mode"], "graceful", "{stop_commands:?}");
}

/// An admitted Stop belongs to the workspace, not to the request that asked
/// for it (`g5.lifecycle.stop.force`): the owner's browser going away while
/// the guest is still saving must neither interrupt the save nor leave the
/// workspace `stopping` until a recovery timer notices (G5-Lean#566).
#[tokio::test]
async fn owner_stop_outlives_the_connection_that_requested_it() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach_with_rpc_barrier(
        &app,
        &workspace,
        owner.id,
        "workspace.flush",
    )
    .await;
    sqlx::query("UPDATE workspace_vms SET started_at = now() WHERE workspace_id = $1")
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .expect("record the VM start");

    let operation_id = Uuid::new_v4();
    let client = app.authenticated_client(&owner);
    let path = format!("/api/v1/workspaces/{}/stop", workspace.id);
    let request = tokio::spawn(async move {
        client
            .post_json(&path, &json!({"operationId": operation_id}))
            .await
    });
    wait_for_guest_rpc(&runtime, "workspace.flush").await;
    // The browser reloads: the in-flight request is dropped and its
    // connection closed while the guest still holds the flush.
    request.abort();
    assert!(request.await.is_err(), "the request was abandoned");

    // While the save is held the row carries a live executor claim, not a
    // minutes-long recovery timer.
    let (status, retry_in) = stop_claim(&app, workspace.id).await;
    assert_eq!(status, "stopping");
    let retry_in = retry_in.expect("an admitted stop is claimed");
    assert!(
        retry_in > 0.0 && retry_in <= 30.0,
        "the claim is renewed, not a five-minute wait: {retry_in}s"
    );
    // ... and the detached execution keeps renewing it past the renewal
    // interval, which proves it outlived the request.
    tokio::time::sleep(Duration::from_secs(11)).await;
    let (status, renewed_in) = stop_claim(&app, workspace.id).await;
    assert_eq!(status, "stopping");
    let renewed_in = renewed_in.expect("the stop is still claimed");
    assert!(
        renewed_in > retry_in - 11.0 + 5.0,
        "the claim was renewed: {retry_in}s before, {renewed_in}s after 11s"
    );

    // No reconciler runs in this app: only the detached execution can finish.
    runtime.release_rpc_barrier();
    wait_for_vm_status(&app, workspace.id, "stopped").await;
    let (flush_id, loss, error): (
        Option<Uuid>,
        Option<chrono::DateTime<chrono::Utc>>,
        Option<Value>,
    ) = sqlx::query_as(
        "SELECT last_flush_id, unflushed_loss_at, last_error
             FROM workspace_vms WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .expect("load the released stop");
    assert!(flush_id.is_some(), "the save completed and was kept");
    assert!(loss.is_none(), "{error:?}");
    assert!(error.is_none(), "{error:?}");
    assert_clean_release(&vm_stop_commands(&runtime).await);

    // Once released, the owner's retry of the same Stop is idempotent.
    let repeated = app
        .authenticated_client(&owner)
        .post_json(
            &format!("/api/v1/workspaces/{}/stop", workspace.id),
            &json!({"operationId": operation_id}),
        )
        .await;
    assert_eq!(repeated.status(), StatusCode::OK);
    let repeated: Value = repeated.json().await.expect("parse repeated stop");
    assert_eq!(data(&repeated)["status"], "stopped");
    assert_eq!(
        data(&repeated)["flushId"].as_str().map(str::to_string),
        flush_id.map(|id| id.to_string())
    );
    assert_clean_release(&vm_stop_commands(&runtime).await);

    app.cleanup().await;
}

/// A stop whose executor dies after admission (an API restart) is resumed by
/// the lifecycle reconciler as soon as its claim lapses, not after a fixed
/// recovery timer, and the resumption completes the save and releases the VM
/// once (G5-Lean#566).
#[tokio::test]
async fn abandoned_stop_is_resumed_once_its_claim_lapses() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    sqlx::query("UPDATE workspace_vms SET started_at = now() WHERE workspace_id = $1")
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .expect("record the VM start");
    let service = workspace_vm_service(&app, owner.id, org.id).await;
    let operation_id = Uuid::new_v4();
    assert!(service
        .admit_stop_and_abandon_for_tests(
            workspace.id,
            owner.id,
            operation_id,
            g5_api::services::workspace_vm_service::StopReason::User,
        )
        .await
        .expect("admit the owner's stop"));

    let (status, retry_in) = stop_claim(&app, workspace.id).await;
    assert_eq!(status, "stopping");
    let retry_in = retry_in.expect("an admitted stop is claimed");
    assert!(
        retry_in > 20.0 && retry_in <= 30.0,
        "the claim lapses within half a minute: {retry_in}s"
    );
    // A live claim keeps the reconciler away from the operation.
    assert_eq!(
        service
            .reconcile_stopping(10)
            .await
            .expect("reconcile while the stop is claimed"),
        0
    );
    assert!(runtime.received_rpc_methods().await.is_empty());

    // Nobody renews the dead executor's claim; once it lapses, the
    // reconciler resumes the same operation.
    sqlx::query("UPDATE workspace_vms SET stop_retry_at = now() WHERE workspace_id = $1")
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .expect("let the claim lapse");
    assert_eq!(
        service
            .reconcile_stopping(10)
            .await
            .expect("resume the abandoned stop"),
        1
    );
    let (status, flush_id, loss, stored_operation): (
        String,
        Option<Uuid>,
        Option<chrono::DateTime<chrono::Utc>>,
        Option<Uuid>,
    ) = sqlx::query_as(
        "SELECT status, last_flush_id, unflushed_loss_at, stop_operation_id
         FROM workspace_vms WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .expect("load the resumed stop");
    assert_eq!(status, "stopped");
    assert!(flush_id.is_some(), "the resumed stop saved the workspace");
    assert!(loss.is_none());
    assert!(
        stored_operation.is_none() || stored_operation == Some(operation_id),
        "the reconciler reused the admitted operation: {stored_operation:?}"
    );
    let rpc = runtime.received_rpc_methods().await;
    assert_eq!(
        rpc.iter()
            .filter(|method| *method == "workspace.flush")
            .count(),
        1,
        "{rpc:?}"
    );
    assert_clean_release(&vm_stop_commands(&runtime).await);

    app.cleanup().await;
}

/// Repeated Stop requests while the first execution is live join it instead
/// of running the operation again: the same operation reports progress with
/// no further guest calls, a different operation is refused, and the VM is
/// released exactly once with its saved state (G5-Lean#566).
#[tokio::test]
async fn repeated_stop_requests_join_the_live_execution_and_release_once() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach_with_rpc_barrier(
        &app,
        &workspace,
        owner.id,
        "workspace.flush",
    )
    .await;
    sqlx::query("UPDATE workspace_vms SET started_at = now() WHERE workspace_id = $1")
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .expect("record the VM start");

    let operation_id = Uuid::new_v4();
    let client = app.authenticated_client(&owner);
    let path = format!("/api/v1/workspaces/{}/stop", workspace.id);
    let first = tokio::spawn(async move {
        client
            .post_json(&path, &json!({"operationId": operation_id}))
            .await
    });
    wait_for_guest_rpc(&runtime, "workspace.flush").await;
    let guest_calls = runtime.received_rpc_methods().await.len();

    let client = app.authenticated_client(&owner);
    let repeated = client
        .post_json(
            &format!("/api/v1/workspaces/{}/stop", workspace.id),
            &json!({"operationId": operation_id}),
        )
        .await;
    assert_eq!(repeated.status(), StatusCode::OK);
    let repeated: Value = repeated.json().await.expect("parse repeated stop");
    assert_eq!(data(&repeated)["status"], "stopping", "{repeated}");
    assert_eq!(data(&repeated)["operationId"], operation_id.to_string());

    let other = client
        .post_json(
            &format!("/api/v1/workspaces/{}/stop", workspace.id),
            &json!({"operationId": Uuid::new_v4()}),
        )
        .await;
    assert_eq!(other.status(), StatusCode::CONFLICT);

    // The reconciler also leaves the claimed operation to its executor.
    assert_eq!(
        workspace_vm_service(&app, owner.id, org.id)
            .await
            .reconcile_stopping(10)
            .await
            .expect("reconcile while the stop is claimed"),
        0
    );
    assert_eq!(
        runtime.received_rpc_methods().await.len(),
        guest_calls,
        "joining a live stop makes no guest call"
    );
    assert!(vm_stop_commands(&runtime).await.is_empty());

    runtime.release_rpc_barrier();
    let first = first.await.expect("the first request completes");
    assert_eq!(first.status(), StatusCode::OK);
    let first: Value = first.json().await.expect("parse first stop");
    assert_eq!(data(&first)["status"], "stopped", "{first}");
    assert!(data(&first)["flushId"].is_string());
    assert_eq!(data(&first)["unflushedLoss"], false);
    let rpc = runtime.received_rpc_methods().await;
    assert_eq!(
        rpc.iter()
            .filter(|method| *method == "workspace.flush")
            .count(),
        1,
        "{rpc:?}"
    );
    assert_clean_release(&vm_stop_commands(&runtime).await);

    app.cleanup().await;
}

/// An executor whose claim is gone (it lapsed and the reconciler, or a
/// repeated request, claimed the operation) stops working at once and
/// reports the row as it stands; the claim holder finishes the same
/// operation with the same receipt and releases the VM once.
#[tokio::test]
async fn an_executor_that_loses_its_claim_stops_and_the_claim_holder_finishes() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach_with_rpc_barrier(
        &app,
        &workspace,
        owner.id,
        "workspace.flush",
    )
    .await;
    sqlx::query("UPDATE workspace_vms SET started_at = now() WHERE workspace_id = $1")
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .expect("record the VM start");

    let operation_id = Uuid::new_v4();
    let client = app.authenticated_client(&owner);
    let path = format!("/api/v1/workspaces/{}/stop", workspace.id);
    let request = tokio::spawn(async move {
        client
            .post_json(&path, &json!({"operationId": operation_id}))
            .await
    });
    wait_for_guest_rpc(&runtime, "workspace.flush").await;
    // The claim is taken away while the guest still holds the flush.
    sqlx::query("UPDATE workspace_vms SET stop_retry_at = now() WHERE workspace_id = $1")
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .expect("take the claim away");

    // The next renewal notices and the execution stops without releasing.
    let response = tokio::time::timeout(Duration::from_secs(20), request)
        .await
        .expect("the executor stops within its renewal interval")
        .expect("the request completes");
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("parse stop response");
    assert_eq!(data(&body)["status"], "stopping", "{body}");
    assert!(vm_stop_commands(&runtime).await.is_empty());
    let (status, retry_in) = stop_claim(&app, workspace.id).await;
    assert_eq!(status, "stopping");
    assert!(
        retry_in.is_some_and(|secs| secs <= 0.0),
        "the lost claim was not renewed: {retry_in:?}"
    );

    // The claim holder (here the reconciler) finishes the operation.
    runtime.release_rpc_barrier();
    assert_eq!(
        workspace_vm_service(&app, owner.id, org.id)
            .await
            .reconcile_stopping(10)
            .await
            .expect("resume the stop"),
        1
    );
    let (status, flush_id, loss): (String, Option<Uuid>, Option<chrono::DateTime<chrono::Utc>>) =
        sqlx::query_as(
            "SELECT status, last_flush_id, unflushed_loss_at
         FROM workspace_vms WHERE workspace_id = $1",
        )
        .bind(workspace.id)
        .fetch_one(&app.db)
        .await
        .expect("load the finished stop");
    assert_eq!(status, "stopped");
    assert!(flush_id.is_some());
    assert!(loss.is_none());
    assert_clean_release(&vm_stop_commands(&runtime).await);

    app.cleanup().await;
}

/// A renewal that never comes back (here: the row is locked by another
/// transaction, so the renewal's update waits) must not keep the executor
/// working past its lease: the claim counts as lost when the lease ends,
/// and the execution stops without a host command.
#[tokio::test]
async fn a_renewal_that_never_answers_loses_the_claim_when_the_lease_ends() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach_with_rpc_barrier(
        &app,
        &workspace,
        owner.id,
        "workspace.flush",
    )
    .await;
    sqlx::query("UPDATE workspace_vms SET started_at = now() WHERE workspace_id = $1")
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .expect("record the VM start");

    let operation_id = Uuid::new_v4();
    let client = app.authenticated_client(&owner);
    let path = format!("/api/v1/workspaces/{}/stop", workspace.id);
    let request = tokio::spawn(async move {
        client
            .post_json(&path, &json!({"operationId": operation_id}))
            .await
    });
    wait_for_guest_rpc(&runtime, "workspace.flush").await;

    // Another transaction holds the row, so every renewal from here on
    // waits on it; only then is the remaining lease sampled, so no renewal
    // can extend what was measured.
    let mut lock = app.db.begin().await.expect("open the locking transaction");
    sqlx::query("SELECT 1 FROM workspace_vms WHERE workspace_id = $1 FOR UPDATE")
        .bind(workspace.id)
        .execute(&mut *lock)
        .await
        .expect("lock the VM row");
    let sampled_at = tokio::time::Instant::now();
    let (_, claimed_for): (String, Option<f64>) = sqlx::query_as(
        "SELECT status, EXTRACT(EPOCH FROM (stop_retry_at - now()))::float8
         FROM workspace_vms WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .fetch_one(&mut *lock)
    .await
    .expect("sample the claim under the lock");
    let claimed_for = claimed_for.expect("the stop is claimed");

    let response = tokio::time::timeout(Duration::from_secs(45), request)
        .await
        .expect("the executor gives up by the end of its lease")
        .expect("the request completes");
    let stopped_after = sampled_at.elapsed();
    assert!(
        stopped_after >= Duration::from_secs_f64((claimed_for - 2.0).max(0.0))
            && stopped_after <= Duration::from_secs_f64(claimed_for + 5.0),
        "the executor stops when its lease ends, not before or long after: {stopped_after:?} with {claimed_for}s of lease left"
    );
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("parse stop response");
    assert_eq!(data(&body)["status"], "stopping", "{body}");
    assert!(vm_stop_commands(&runtime).await.is_empty());
    lock.rollback().await.expect("release the row");

    // The executor measured its lease from before it asked for the claim,
    // so it stopped no later than the database lease ends, and at most the
    // admission latency before it. Once the row is due, the reconciler takes
    // the claim and finishes.
    let (status, retry_in) = stop_claim(&app, workspace.id).await;
    assert_eq!(status, "stopping");
    assert!(
        retry_in.is_some_and(|secs| secs <= 5.0),
        "the unrenewed claim is about to lapse: {retry_in:?}"
    );
    for _ in 0..200 {
        let (_, retry_in) = stop_claim(&app, workspace.id).await;
        if retry_in.is_some_and(|secs| secs <= 0.0) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    runtime.release_rpc_barrier();
    assert_eq!(
        workspace_vm_service(&app, owner.id, org.id)
            .await
            .reconcile_stopping(10)
            .await
            .expect("resume the stop"),
        1
    );
    wait_for_vm_status(&app, workspace.id, "stopped").await;
    assert_clean_release(&vm_stop_commands(&runtime).await);

    app.cleanup().await;
}

fn unix_ms() -> u64 {
    u64::try_from(chrono::Utc::now().timestamp_millis()).expect("clock after the epoch")
}

/// G5Labs/G5-Lean#643: a runtime call whose dispatcher stream closes before
/// any reply (the host lost or replaced the dispatcher connection carrying
/// it) is resent with the same idempotency key, so the owner's write lands
/// instead of failing with `HOST_TUNNEL_CLOSED`.
#[tokio::test]
async fn a_runtime_call_whose_stream_closes_without_a_reply_is_resent_under_its_key() {
    use g5_api::services::workspace_runtime::RuntimeRequest;

    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let peer = TestWorkspaceRuntimePeer::attach_dropping_replies(
        &app, &workspace, owner.id, "fs.write", 2,
    )
    .await;
    let handle = app
        .state
        .workspace_runtime
        .owner_handle(workspace.id, owner.id)
        .await
        .expect("the owner's running workspace");
    let repo_id = Uuid::new_v4();
    let request = RuntimeRequest::owner(
        &handle,
        owner.id,
        unix_ms() + 30_000,
        "workspace-file:write:resent",
    );

    let written = app
        .state
        .workspace_runtime
        .file_write(
            &handle,
            &request,
            repo_id,
            "resent.txt",
            "resent\n",
            "utf-8",
            None,
        )
        .await
        .expect("the write lands after its stream closed twice");

    assert!(written["hash"].is_string(), "{written}");
    assert_eq!(
        peer.file_content(repo_id, "resent.txt").await.as_deref(),
        Some("resent\n")
    );
    assert_eq!(
        peer.dropped_method_keys().await,
        vec!["workspace-file:write:resent"; 3],
        "every attempt carries the caller's idempotency key"
    );
    app.cleanup().await;
}

/// The resends are bounded: a dispatcher stream that keeps closing without a
/// reply still fails the call with `HOST_TUNNEL_CLOSED`, after a few seconds.
#[tokio::test]
async fn a_runtime_call_whose_stream_keeps_closing_fails_after_bounded_resends() {
    use g5_api::services::workspace_runtime::RuntimeRequest;

    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let peer = TestWorkspaceRuntimePeer::attach_dropping_replies(
        &app,
        &workspace,
        owner.id,
        "fs.write",
        usize::MAX,
    )
    .await;
    let handle = app
        .state
        .workspace_runtime
        .owner_handle(workspace.id, owner.id)
        .await
        .expect("the owner's running workspace");
    let request = RuntimeRequest::owner(
        &handle,
        owner.id,
        unix_ms() + 30_000,
        "workspace-file:write:lost",
    );

    let started = std::time::Instant::now();
    let error = app
        .state
        .workspace_runtime
        .file_write(
            &handle,
            &request,
            Uuid::new_v4(),
            "lost.txt",
            "lost\n",
            "utf-8",
            None,
        )
        .await
        .expect_err("a dispatcher that never answers fails the call");

    assert_eq!(
        (error.code, error.message.as_str()),
        (503, "HOST_TUNNEL_CLOSED")
    );
    assert_eq!(peer.dropped_method_keys().await.len(), 5);
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "resends stay bounded: {:?}",
        started.elapsed()
    );
    app.cleanup().await;
}

/// Lifecycle gate calls are never resent by the facade: stop, fork and
/// transfer own their keyed retry and its timing.
#[tokio::test]
async fn a_lifecycle_gate_call_whose_stream_closes_is_not_resent() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let peer = TestWorkspaceRuntimePeer::attach_dropping_replies(
        &app,
        &workspace,
        owner.id,
        "admission.close",
        1,
    )
    .await;
    let handle = app
        .state
        .workspace_runtime
        .get(workspace.id)
        .await
        .expect("load runtime")
        .expect("runtime is attached");

    let error = app
        .state
        .workspace_runtime
        .lifecycle_call_for_test(
            &handle,
            "admission.close",
            json!({"prepare_only": true, "operation_id": Uuid::new_v4()}),
            "gate-not-resent",
        )
        .await
        .expect_err("the closed stream fails the gate call");

    assert_eq!(
        (error.code, error.message.as_str()),
        (503, "HOST_TUNNEL_CLOSED")
    );
    assert_eq!(
        peer.dropped_method_keys().await,
        vec!["gate-not-resent".to_string()]
    );
    app.cleanup().await;
}

/// At most one ownership transfer is pending per workspace: a second request
/// is refused rather than queued behind the first
/// (`g5.lifecycle.transfer.admission`, G5-Lean#626 D7).
#[tokio::test]
async fn a_second_pending_transfer_is_refused() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let first_target = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let second_target = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let _runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let host_id: Uuid =
        sqlx::query_scalar("SELECT host_id FROM workspace_vms WHERE workspace_id = $1")
            .bind(workspace.id)
            .fetch_one(&app.db)
            .await
            .expect("load runtime host");
    // With the host unreachable, the first transfer's stop cannot complete, so
    // it stays pending.
    app.state.preview_tunnels.terminate(&host_id).await;

    let first = app
        .authenticated_client(&owner)
        .post_json(
            &format!("/api/v1/workspaces/{}/transfer", workspace.id),
            &json!({
                "toUserId": first_target.id,
                "idempotencyKey": format!("transfer-{}", Uuid::new_v4())
            }),
        )
        .await;
    assert_eq!(first.status(), StatusCode::OK);
    assert_eq!(data(&first.json().await.unwrap())["status"], "pending");

    // A second, distinct transfer request is refused while the first is pending.
    let second = app
        .authenticated_client(&owner)
        .post_json(
            &format!("/api/v1/workspaces/{}/transfer", workspace.id),
            &json!({
                "toUserId": second_target.id,
                "idempotencyKey": format!("transfer-{}", Uuid::new_v4())
            }),
        )
        .await;
    assert_eq!(
        second.status(),
        StatusCode::CONFLICT,
        "second transfer refused"
    );
    let pending_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM workspace_ownership_transfers
         WHERE workspace_id = $1 AND status = 'pending'",
    )
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .expect("count pending transfers");
    assert_eq!(pending_count, 1, "only one transfer is pending");
    app.cleanup().await;
}

/// A Start that waits for the workspace lock re-reads the pending-transfer
/// predicate after acquiring it, so a transfer admitted meanwhile fences the
/// Start (`g5.lifecycle.transfer.admission`, G5-Lean#626 S14). The pre-fix
/// read was a subquery evaluated on the snapshot taken before the lock.
#[tokio::test]
async fn a_transfer_admitted_while_a_start_waits_for_the_lock_fences_it() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let next_owner = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let workspace = TestWorkspace::create(&app, &org).await;

    // Hold the workspace row so the Start blocks on `FOR UPDATE OF workspace`.
    let mut holder = app.db.begin().await.expect("begin the row holder");
    sqlx::query("SELECT id FROM workspaces WHERE id = $1 FOR UPDATE")
        .bind(workspace.id)
        .execute(&mut *holder)
        .await
        .expect("hold the workspace row");

    let service = workspace_vm_service(&app, owner.id, org.id).await;
    let start = tokio::spawn({
        let service = service;
        let workspace_id = workspace.id;
        let owner_id = owner.id;
        async move {
            service
                .resume(
                    workspace_id,
                    owner_id,
                    Uuid::new_v4(),
                    g5_api::services::workspace_vm_service::StartKind::Resume,
                )
                .await
        }
    });

    // Once the Start is waiting for the workspace row, admit a transfer and
    // release the row.
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let waiting: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM pg_stat_activity
                 WHERE datname = current_database() AND wait_event_type = 'Lock'
                   AND query LIKE 'SELECT workspace.org_id%')",
            )
            .fetch_one(&app.db)
            .await
            .expect("read lock waits");
            if waiting {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the start reaches the workspace-row lock");
    let digest = hex::encode(Sha256::digest(
        format!("{}:{}:{}", workspace.id, owner.id, next_owner.id).as_bytes(),
    ));
    sqlx::query(
        "INSERT INTO workspace_ownership_transfers(
             id, org_id, workspace_id, from_user_id, to_user_id, requested_by,
             idempotency_key, request_digest)
         VALUES ($1,$2,$3,$4,$5,$4,$6,$7)",
    )
    .bind(Uuid::new_v4())
    .bind(org.id)
    .bind(workspace.id)
    .bind(owner.id)
    .bind(next_owner.id)
    .bind(format!("transfer-{}", Uuid::new_v4()))
    .bind(digest)
    .execute(&mut *holder)
    .await
    .expect("admit a transfer while the start waits");
    holder.commit().await.expect("release the workspace row");

    let outcome = start.await.expect("the start task joins");
    assert!(
        matches!(
            outcome,
            Err(g5_contracts::error::G5Error::WorkspaceStopping)
        ),
        "a transfer admitted while the start waited fences it: {outcome:?}"
    );
    app.cleanup().await;
}

/// A Start whose reply never arrived keeps its incarnation, and with it its
/// quota slot: on a quota-one organization the deadline reconciler defers the
/// incarnation the host still reports, and a second workspace's Start waits on
/// the quota instead of admitting a second live incarnation
/// (`g5.runtime.start.settlement`, `g5.runtime.placement.capacity`, G5-Lean#626 D2/H1).
#[tokio::test]
async fn a_delayed_start_holds_its_quota_slot_against_a_second_incarnation() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let delayed = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &delayed, owner.id).await;
    // The Start was recorded as sent and its acquisition deadline has passed,
    // but the host still holds the incarnation.
    let now = chrono::Utc::now();
    let intent = json!({
        "id": Uuid::new_v4(),
        "kind": "resume",
        "owner_user_id": owner.id,
        "lifecycle_generation": 0,
        "status": "starting",
        "created_at": now - chrono::Duration::minutes(1),
        "expires_at": now + chrono::Duration::minutes(14),
        "attempts": 1,
    });
    sqlx::query(
        "UPDATE workspace_vms
         SET status = 'starting', start_intent = $2, started_at = NULL,
             start_command_sent_at = now(),
             start_deadline_at = now() - interval '1 second'
         WHERE workspace_id = $1",
    )
    .bind(delayed.id)
    .bind(intent)
    .execute(&app.db)
    .await
    .expect("age the delayed start past its deadline");
    sqlx::query(
        "INSERT INTO configs(org_id, scope, scope_id, key, value)
         VALUES ($1, 'org', $1, 'max_running_vms', '1'::jsonb)
         ON CONFLICT (org_id, scope, scope_id, key)
         DO UPDATE SET value = EXCLUDED.value",
    )
    .bind(org.id)
    .execute(&app.db)
    .await
    .expect("set the org VM quota to one");

    // The reconciler asks the host, which reports the incarnation, so custody
    // is retained rather than released on absence.
    let reconciled = workspace_vm_service(&app, owner.id, org.id)
        .await
        .reconcile_start_queue(10)
        .await
        .expect("reconcile the delayed start");
    assert_eq!(reconciled, 0);
    let delayed_row: (String, Option<Uuid>, Option<String>) =
        sqlx::query_as("SELECT status, host_id, vm_id FROM workspace_vms WHERE workspace_id = $1")
            .bind(delayed.id)
            .fetch_one(&app.db)
            .await
            .expect("load the delayed incarnation");
    assert_eq!(delayed_row.0, "starting");
    assert_eq!(delayed_row.1, Some(runtime.host_id()));
    assert_eq!(delayed_row.2, Some(runtime.vm_id().to_string()));

    // A second workspace's Start finds the quota taken by the delayed
    // incarnation and waits; it is not admitted beside it.
    let second = TestWorkspace::create(&app, &org).await;
    second.attach_parent(&app).await;
    let response = app
        .authenticated_client(&owner)
        .post_json(
            &format!("/api/v1/workspaces/{}/resume", second.id),
            &json!({"operationId": Uuid::new_v4()}),
        )
        .await;
    let status = response.status();
    let body: Value = response.json().await.expect("parse the second resume");
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data(&body)["outcome"], "queued_for_capacity", "{body}");
    assert_eq!(
        data(&body)["vm"]["lastError"]["reason"],
        "org_vm_quota",
        "{body}"
    );
    assert_eq!(data(&body)["vm"]["lastError"]["limit"], 1, "{body}");
    let admitted: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM workspace_vms
         WHERE org_id = $1 AND status IN ('starting', 'running', 'stopping')",
    )
    .bind(org.id)
    .fetch_one(&app.db)
    .await
    .expect("count admitted incarnations");
    assert_eq!(
        admitted, 1,
        "the delayed Start is the only admitted incarnation"
    );
    app.cleanup().await;
}

/// A refusal that arrives for a placement whose Start intent Stop already
/// cancelled releases the incarnation but leaves the intent cancelled, and a
/// release fences on the Start-delivery state its caller observed
/// (`g5.runtime.start.settlement`, `g5.lifecycle.start`, G5-Lean#626 D5).
#[tokio::test]
async fn a_late_refusal_does_not_revive_a_cancelled_start_intent() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
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
    let seed = |workspace_id: Uuid, intent_status: &'static str| {
        let pool = app.db.clone();
        let owner_id = owner.id;
        let org_id = org.id;
        async move {
            let vm_id = Uuid::new_v4();
            let now = chrono::Utc::now();
            let intent = json!({
                "id": Uuid::new_v4(),
                "kind": "resume",
                "owner_user_id": owner_id,
                "lifecycle_generation": 0,
                "status": intent_status,
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
            .bind(org_id)
            .bind(workspace_id)
            .bind(host_id)
            .bind(vm_id.to_string())
            .bind(intent)
            .execute(&pool)
            .await
            .expect("seed a sent, starting incarnation");
            vm_id
        }
    };
    let reservation = |vm_id: Uuid| g5_api::services::worker_host_service::PlacementReservation {
        host_id,
        vm_id,
        epoch: 1,
        occupancy_before: 0,
        previous_activity_at: None,
    };
    let row = |workspace_id: Uuid| {
        let pool = app.db.clone();
        async move {
            sqlx::query_as::<_, (String, Option<Uuid>, String, bool)>(
                "SELECT status, host_id, start_intent ->> 'status',
                        COALESCE(start_intent -> 'host_refusals' ? $2::text, false)
                 FROM workspace_vms WHERE workspace_id = $1",
            )
            .bind(workspace_id)
            .bind(host_id.to_string())
            .fetch_one(&pool)
            .await
            .expect("load the incarnation")
        }
    };
    let service = &app.state.worker_host_service;

    // Stop cancelled the intent while the Start was in flight; the host's
    // late refusal releases the slot but must not requeue the cancelled intent.
    let cancelled = TestWorkspace::create(&app, &org).await;
    let cancelled_vm = seed(cancelled.id, "cancelled").await;
    let mut db = app.scoped_db_for_org(org.id).await;
    assert!(service
        .release_placement(
            &mut db,
            &reservation(cancelled_vm),
            org.id,
            cancelled.id,
            true,
            "host_capacity_refused",
            Some("slots"),
        )
        .await
        .expect("release the refused placement"));
    db.commit().await.expect("commit the release");
    let (status, host, intent, refused_here) = row(cancelled.id).await;
    assert_eq!(status, "stopped", "the refused placement is released");
    assert_eq!(host, None);
    assert_eq!(intent, "cancelled", "a cancelled intent stays cancelled");
    assert!(
        refused_here,
        "the host refusal is still recorded for placement"
    );

    // A live intent returns to the capacity queue.
    let live = TestWorkspace::create(&app, &org).await;
    let live_vm = seed(live.id, "starting").await;
    let mut db = app.scoped_db_for_org(org.id).await;
    assert!(service
        .release_placement(
            &mut db,
            &reservation(live_vm),
            org.id,
            live.id,
            true,
            "host_capacity_refused",
            Some("slots"),
        )
        .await
        .expect("release the refused placement"));
    db.commit().await.expect("commit the release");
    let (status, _, intent, _) = row(live.id).await;
    assert_eq!(
        (status.as_str(), intent.as_str()),
        ("stopped", "queued_for_capacity")
    );

    // A release that observed the Start unsent cannot release a sent one: the
    // sender's stamp and the releaser's fence serialize on the row.
    let fenced = TestWorkspace::create(&app, &org).await;
    let fenced_vm = seed(fenced.id, "starting").await;
    let mut db = app.scoped_db_for_org(org.id).await;
    assert!(!service
        .release_placement(
            &mut db,
            &reservation(fenced_vm),
            org.id,
            fenced.id,
            false,
            "start_absent_on_host",
            None,
        )
        .await
        .expect("attempt the fenced release"));
    db.commit().await.expect("commit the no-op");
    let (status, host, intent, _) = row(fenced.id).await;
    assert_eq!(
        status, "starting",
        "a mismatched delivery fence releases nothing"
    );
    assert_eq!(host, Some(host_id));
    assert_eq!(intent, "starting");
    app.cleanup().await;
}
