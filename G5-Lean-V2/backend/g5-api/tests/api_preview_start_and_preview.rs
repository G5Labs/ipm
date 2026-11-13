//! Start workspace and preview (`g5.preview.start-and-preview`): one owner
//! request records Start consent and the preview task bound to it; the
//! consent trigger (migration 152) releases or cancels the task when the
//! Start settles, as it does for Start-and-send inputs.
//!
//! Each test spawns a full `TestApp` with an isolated Postgres database and
//! drives the real HTTP endpoints, with the scripted runtime peer standing in
//! for the host where a VM is needed.

use crate::helpers;

use helpers::{TestApp, TestOrg, TestUser, TestWorkspace, TestWorkspaceRuntimePeer};
use reqwest::StatusCode;
use serde_json::{json, Value};
use uuid::Uuid;

const WORKSPACE_DEFAULT: &str = "preview:\n  command: node server.js\n  port: 4173\n";

async fn body_of(response: reqwest::Response) -> (StatusCode, Value) {
    let status = response.status();
    (status, response.json().await.unwrap_or(Value::Null))
}

async fn task_state(app: &TestApp, task_id: Uuid) -> (String, Option<Uuid>, Value) {
    sqlx::query_as::<_, (String, Option<Uuid>, Value)>(
        "SELECT status, start_consent_id, COALESCE(outcome, 'null'::jsonb) FROM tasks WHERE id = $1",
    )
    .bind(task_id)
    .fetch_one(&app.db)
    .await
    .expect("task state")
}

async fn stop(client: &helpers::AuthenticatedClient, workspace_id: Uuid) {
    let (status, body) = body_of(
        client
            .post_json(
                &format!("/api/v1/workspaces/{workspace_id}/stop"),
                &json!({ "operationId": Uuid::new_v4() }),
            )
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "stop failed: {body}");
}

async fn start_preview(
    client: &helpers::AuthenticatedClient,
    workspace_id: Uuid,
    body: Value,
) -> (StatusCode, Value) {
    body_of(
        client
            .post_json(&format!("/api/v1/workspaces/{workspace_id}/preview"), &body)
            .await,
    )
    .await
}

fn task_id_of(body: &Value) -> Uuid {
    Uuid::parse_str(body["data"]["taskId"].as_str().expect("task id")).expect("uuid")
}

/// A stopped workspace: Start workspace and preview records `compound_start`
/// consent and a queued preview bound to it, from the committed
/// configuration; a Stop that cancels the Start cancels the preview with a
/// reason, so nothing stays queued for a later Start.
#[tokio::test]
async fn start_and_preview_binds_the_preview_and_cancelling_the_start_cancels_it() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let mut workspace = TestWorkspace::create(&app, &org).await;
    workspace
        .commit_file(&app, ".g5/workspace.yaml", WORKSPACE_DEFAULT)
        .await;
    workspace.attach_parent(&app).await;
    let _runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let client = app.authenticated_client(&owner);
    stop(&client, workspace.id).await;

    let (status, started) = start_preview(
        &client,
        workspace.id,
        json!({
            "repoId": workspace.repo_id,
            "idempotencyKey": "preview-start-1",
            "startCompute": true,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{started}");
    let start = &started["data"]["startCompute"];
    assert_eq!(start["status"], json!("starting"), "{started}");
    let intent_id = Uuid::parse_str(start["intentId"].as_str().expect("intent")).expect("uuid");
    assert_eq!(started["data"]["status"], json!("queued"));
    assert_eq!(started["data"]["pendingStart"], json!(true), "{started}");
    assert_eq!(started["data"]["port"], json!(4173), "the committed port");
    let task_id = task_id_of(&started);

    let kind: String = sqlx::query_scalar(
        "SELECT start_intent ->> 'kind' FROM workspace_vms WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .expect("intent kind");
    assert_eq!(kind, "compound_start", "a clearly labelled compound Start");
    let (task_status, bound, _) = task_state(&app, task_id).await;
    assert_eq!(task_status, "queued");
    assert_eq!(bound, Some(intent_id), "the preview is bound to its Start");

    // The pending preview is the workspace's current preview for every reader.
    let (status, current) = body_of(
        client
            .get(&format!("/api/v1/workspaces/{}/preview", workspace.id))
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{current}");
    assert_eq!(current["data"]["taskId"], json!(task_id.to_string()));
    assert_eq!(current["data"]["pendingStart"], json!(true), "{current}");

    // One preview at a time: a second start is refused while this one waits.
    let (status, second) = start_preview(
        &client,
        workspace.id,
        json!({
            "repoId": workspace.repo_id,
            "idempotencyKey": "preview-start-2",
            "startCompute": true,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{second}");
    // A replay of the first request (a lost response retried) returns the
    // same preview and records no second Start.
    let (status, replay) = start_preview(
        &client,
        workspace.id,
        json!({
            "repoId": workspace.repo_id,
            "idempotencyKey": "preview-start-1",
            "startCompute": true,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{replay}");
    assert_eq!(task_id_of(&replay), task_id);
    let same_intent: String = sqlx::query_scalar(
        "SELECT start_intent ->> 'id' FROM workspace_vms WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .expect("intent id");
    assert_eq!(
        same_intent,
        intent_id.to_string(),
        "the first Start carries the preview"
    );
    let intents: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM tasks WHERE workspace_id = $1 AND kind = 'preview'",
    )
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .expect("task count");
    assert_eq!(intents, 1);

    // Stop while starting cancels that consent; the preview goes with it.
    stop(&client, workspace.id).await;
    let (task_status, bound, outcome) = task_state(&app, task_id).await;
    assert_eq!(task_status, "cancelled", "{outcome}");
    assert_eq!(bound, None);
    assert_eq!(outcome["reason"], json!("start_ended"), "{outcome}");
    let (status, ended) = body_of(
        client
            .get(&format!("/api/v1/workspaces/{}/preview", workspace.id))
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{ended}");
    assert_eq!(ended["data"]["status"], json!("cancelled"));
    assert_eq!(ended["data"]["pendingStart"], json!(false));
    assert_eq!(
        ended["data"]["ending"]["kind"],
        json!("did_not_start"),
        "{ended}"
    );
    assert_eq!(ended["data"]["ending"]["reason"], json!("start_ended"));
    let intent_status: String = sqlx::query_scalar(
        "SELECT preview_intent ->> 'status' FROM workspace_vms WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .expect("preview intent");
    assert_eq!(intent_status, "settled");

    // After the Start ended, the launcher is open again.
    let (status, again) = start_preview(
        &client,
        workspace.id,
        json!({
            "repoId": workspace.repo_id,
            "idempotencyKey": "preview-start-3",
            "startCompute": true,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{again}");
    assert_ne!(task_id_of(&again), task_id);

    app.cleanup().await;
}

/// Close on a pending preview cancels the preview and leaves the Start under
/// way; a retry of the original request after its Start ended replays the
/// recorded (cancelled) preview without recording another Start; and a chat
/// Start and send while the preview's Start is pending joins that Start
/// instead of superseding it, so the preview stays bound.
#[tokio::test]
async fn close_retry_and_a_second_compound_action_keep_the_pending_start_whole() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let mut workspace = TestWorkspace::create(&app, &org).await;
    workspace
        .commit_file(&app, ".g5/workspace.yaml", WORKSPACE_DEFAULT)
        .await;
    workspace.attach_parent(&app).await;
    let _runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let client = app.authenticated_client(&owner);
    stop(&client, workspace.id).await;

    let (status, started) = start_preview(
        &client,
        workspace.id,
        json!({
            "repoId": workspace.repo_id,
            "idempotencyKey": "preview-close-1",
            "startCompute": true,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{started}");
    let intent_id = Uuid::parse_str(
        started["data"]["startCompute"]["intentId"]
            .as_str()
            .expect("intent"),
    )
    .expect("uuid");
    let task_id = task_id_of(&started);

    // Start and send while the preview's Start is pending joins it.
    let (status, sent) = body_of(
        client
            .post_json(
                &format!("/api/v1/workspaces/{}/sessions/start", workspace.id),
                &json!({ "content": "and tell me when it serves", "startCompute": true }),
            )
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{sent}");
    assert_eq!(
        sent["data"]["message"]["startCompute"]["intentId"],
        json!(intent_id.to_string()),
        "the message joins the preview's Start: {sent}"
    );
    let input_id = Uuid::parse_str(
        sent["data"]["message"]["inputId"]
            .as_str()
            .expect("input id"),
    )
    .expect("uuid");
    let bound_input: Option<Uuid> =
        sqlx::query_scalar("SELECT start_consent_id FROM session_inputs WHERE id = $1")
            .bind(input_id)
            .fetch_one(&app.db)
            .await
            .expect("input binding");
    assert_eq!(bound_input, Some(intent_id));
    let (task_status, bound, _) = task_state(&app, task_id).await;
    assert_eq!((task_status.as_str(), bound), ("queued", Some(intent_id)));
    let live_intent: String = sqlx::query_scalar(
        "SELECT start_intent ->> 'id' FROM workspace_vms WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .expect("intent id");
    assert_eq!(
        live_intent,
        intent_id.to_string(),
        "no second Start was recorded"
    );
    let joined: Option<bool> = sqlx::query_scalar(
        "SELECT (start_intent ->> 'joined')::boolean FROM workspace_vms WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .expect("joined marker");
    assert_eq!(
        joined,
        Some(true),
        "a joined consent is marked, so no abandon cancels it"
    );

    // The header's Start (a plain Resume) during the pending compound Start
    // joins it too: the owner's own Start never supersedes and cancels the
    // work bound to it.
    let (status, resumed) = body_of(
        client
            .post_json(
                &format!("/api/v1/workspaces/{}/resume", workspace.id),
                &json!({ "operationId": Uuid::new_v4() }),
            )
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{resumed}");
    let live_intent: String = sqlx::query_scalar(
        "SELECT start_intent ->> 'id' FROM workspace_vms WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .expect("intent id");
    assert_eq!(
        live_intent,
        intent_id.to_string(),
        "Resume joined the pending Start"
    );
    let (task_status, bound, _) = task_state(&app, task_id).await;
    assert_eq!((task_status.as_str(), bound), ("queued", Some(intent_id)));

    // Close cancels the preview only; the Start and the message stay.
    let (status, closed) = body_of(
        client
            .post_json(
                &format!("/api/v1/workspaces/{}/preview/cancel", workspace.id),
                &json!({}),
            )
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{closed}");
    let (task_status, bound, outcome) = task_state(&app, task_id).await;
    assert_eq!(task_status, "cancelled", "{outcome}");
    assert_eq!(bound, None, "Close clears the binding");
    let (status, current) = body_of(
        client
            .get(&format!("/api/v1/workspaces/{}/preview", workspace.id))
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{current}");
    assert_eq!(current["data"]["pendingStart"], json!(false), "{current}");
    assert_eq!(
        current["data"]["ending"]["kind"],
        json!("closed"),
        "{current}"
    );
    let intent_status: String = sqlx::query_scalar(
        "SELECT start_intent ->> 'status' FROM workspace_vms WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .expect("intent status");
    assert!(
        matches!(
            intent_status.as_str(),
            "pending" | "queued_for_capacity" | "starting"
        ),
        "the Start stays under way: {intent_status}"
    );
    let delivery: String = sqlx::query_scalar("SELECT delivery FROM session_inputs WHERE id = $1")
        .bind(input_id)
        .fetch_one(&app.db)
        .await
        .expect("delivery");
    assert_eq!(delivery, "queued", "the message is unaffected by Close");

    // A retry of the original request replays the recorded preview as it is
    // now, and records no Start.
    stop(&client, workspace.id).await;
    let (status, replay) = start_preview(
        &client,
        workspace.id,
        json!({
            "repoId": workspace.repo_id,
            "idempotencyKey": "preview-close-1",
            "startCompute": true,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{replay}");
    assert_eq!(task_id_of(&replay), task_id);
    assert_eq!(replay["data"]["status"], json!("cancelled"));
    assert!(replay["data"].get("startCompute").is_none(), "{replay}");
    let intent_status: String = sqlx::query_scalar(
        "SELECT start_intent ->> 'status' FROM workspace_vms WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .expect("intent status");
    assert_eq!(intent_status, "cancelled", "the replay recorded no Start");

    app.cleanup().await;
}

/// Ready releases the binding, so the dispatcher may admit the task; expiry
/// and failure cancel it, a failed Start with the existing `vm_start_failed`
/// ending. Without `startCompute`, a start while a Start is already under
/// way binds to that Start (Preview once running).
#[tokio::test]
async fn consent_reaching_ready_releases_the_preview_and_an_ended_consent_cancels_it() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let mut workspace = TestWorkspace::create(&app, &org).await;
    workspace
        .commit_file(&app, ".g5/workspace.yaml", WORKSPACE_DEFAULT)
        .await;
    workspace
        .commit_file(
            &app,
            ".g5/preview_profiles/admin.yaml",
            "command: npm run admin\nport: 5173\ndescription: Admin UI\n",
        )
        .await;
    let client = app.authenticated_client(&owner);

    for (index, (terminal, expect_status, expect_reason)) in [
        ("consumed", "queued", None),
        ("expired", "cancelled", Some("start_ended")),
        ("failed", "cancelled", Some("vm_start_failed")),
    ]
    .into_iter()
    .enumerate()
    {
        // A Start already under way, as the header's Start records it.
        let intent_id = Uuid::new_v4();
        let now = chrono::Utc::now();
        let intent = json!({
            "id": intent_id, "kind": "resume", "owner_user_id": owner.id,
            "lifecycle_generation": 0, "status": "pending", "created_at": now,
            "expires_at": now + chrono::Duration::minutes(15), "attempts": 0,
        });
        sqlx::query(
            "INSERT INTO workspace_vms(org_id, workspace_id, status, start_intent)
             VALUES ($1, $2, 'stopped', $3)
             ON CONFLICT (workspace_id) DO UPDATE
             SET start_intent = EXCLUDED.start_intent, status = 'stopped'",
        )
        .bind(org.id)
        .bind(workspace.id)
        .bind(&intent)
        .execute(&app.db)
        .await
        .expect("seed intent");

        let profile = if index == 0 {
            Value::Null
        } else {
            json!("admin")
        };
        let (status, started) = start_preview(
            &client,
            workspace.id,
            json!({
                "repoId": workspace.repo_id,
                "profile": profile,
                "idempotencyKey": format!("preview-once-running-{index}"),
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{terminal}: {started}");
        assert!(
            started["data"].get("startCompute").is_none(),
            "no new Start: {started}"
        );
        assert_eq!(started["data"]["pendingStart"], json!(true), "{started}");
        assert_eq!(
            started["data"]["port"],
            json!(if index == 0 { 4173 } else { 5173 }),
            "{started}"
        );
        let task_id = task_id_of(&started);
        let (_, bound, _) = task_state(&app, task_id).await;
        assert_eq!(
            bound,
            Some(intent_id),
            "{terminal}: bound to the Start under way"
        );

        sqlx::query(
            "UPDATE workspace_vms
             SET start_intent = jsonb_set(start_intent, '{status}', to_jsonb($2::text))
             WHERE workspace_id = $1",
        )
        .bind(workspace.id)
        .bind(terminal)
        .execute(&app.db)
        .await
        .expect("end consent");
        let (task_status, bound, outcome) = task_state(&app, task_id).await;
        assert_eq!(task_status, expect_status, "{terminal}: {outcome}");
        assert_eq!(bound, None, "{terminal} settles the binding");
        match expect_reason {
            Some(reason) => assert_eq!(outcome["reason"], json!(reason), "{terminal}: {outcome}"),
            None => assert_eq!(outcome, Value::Null, "{terminal}: {outcome}"),
        }
        if terminal == "consumed" {
            // The released task is an ordinary queued preview; clear it for
            // the next round the way an owner's Close would.
            let (status, cancelled) = body_of(
                client
                    .post_json(
                        &format!("/api/v1/workspaces/{}/preview/cancel", workspace.id),
                        &json!({}),
                    )
                    .await,
            )
            .await;
            assert_eq!(status, StatusCode::OK, "{cancelled}");
        }
    }

    app.cleanup().await;
}

/// Without a running VM and without a Start under way, a plain start is
/// refused as before; a start whose consent ended before the preview was
/// recorded is refused and leaves nothing queued; a non-owner cannot start.
#[tokio::test]
async fn pending_previews_need_a_live_start_and_the_owner() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let mut workspace = TestWorkspace::create(&app, &org).await;
    workspace
        .commit_file(&app, ".g5/workspace.yaml", WORKSPACE_DEFAULT)
        .await;
    let client = app.authenticated_client(&owner);

    sqlx::query(
        "INSERT INTO workspace_vms(org_id, workspace_id, status)
         VALUES ($1, $2, 'stopped')
         ON CONFLICT (workspace_id) DO UPDATE SET status = 'stopped', start_intent = NULL",
    )
    .bind(org.id)
    .bind(workspace.id)
    .execute(&app.db)
    .await
    .expect("seed stopped vm");
    let (status, refused) = start_preview(
        &client,
        workspace.id,
        json!({ "repoId": workspace.repo_id, "idempotencyKey": "plain-start" }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{refused}");
    assert_eq!(
        refused["error"]["code"],
        json!("WORKSPACE_NOT_RUNNING"),
        "{refused}"
    );

    // A non-owner may not Start-and-preview.
    let other = TestUser::create_with_scopes(
        &app,
        org.id,
        &[
            g5_auth::Scope::WorkspacesRead,
            g5_auth::Scope::WorkspacesWrite,
        ],
    )
    .await;
    let (status, forbidden) = start_preview(
        &app.authenticated_client(&other),
        workspace.id,
        json!({
            "repoId": workspace.repo_id,
            "idempotencyKey": "not-owner",
            "startCompute": true,
        }),
    )
    .await;
    assert!(
        status == StatusCode::FORBIDDEN || status == StatusCode::NOT_FOUND,
        "{status}: {forbidden}"
    );
    let previews: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM tasks WHERE workspace_id = $1 AND kind = 'preview'",
    )
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .expect("task count");
    assert_eq!(previews, 0, "refused starts record no preview");

    app.cleanup().await;
}

/// A workspace that has never started has no VM row: Start workspace and
/// preview still records the Start (queued for capacity without a host) and
/// the bound preview; a request whose key admission would refuse records no
/// Start at all.
#[tokio::test]
async fn a_never_started_workspace_records_the_start_and_an_invalid_key_records_nothing() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let mut workspace = TestWorkspace::create(&app, &org).await;
    workspace
        .commit_file(&app, ".g5/workspace.yaml", WORKSPACE_DEFAULT)
        .await;
    workspace.attach_parent(&app).await;
    // A Start needs the organization's default image, which the runtime peer
    // would otherwise set; no host is attached, so placement queues.
    sqlx::query("UPDATE orgs SET default_base_image_ref = $2 WHERE id = $1")
        .bind(org.id)
        .bind(helpers::TEST_WORKER_IMAGE_REF)
        .execute(&app.db)
        .await
        .expect("org default image");
    let client = app.authenticated_client(&owner);
    let rows: i64 =
        sqlx::query_scalar("SELECT count(*) FROM workspace_vms WHERE workspace_id = $1")
            .bind(workspace.id)
            .fetch_one(&app.db)
            .await
            .expect("vm rows");
    assert_eq!(rows, 0, "a fresh workspace has no VM row");

    let (status, refused) = start_preview(
        &client,
        workspace.id,
        json!({
            "repoId": workspace.repo_id,
            "idempotencyKey": " padded ",
            "startCompute": true,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{refused}");
    let rows: i64 =
        sqlx::query_scalar("SELECT count(*) FROM workspace_vms WHERE workspace_id = $1")
            .bind(workspace.id)
            .fetch_one(&app.db)
            .await
            .expect("vm rows");
    assert_eq!(rows, 0, "an invalid key records no Start");

    let (status, started) = start_preview(
        &client,
        workspace.id,
        json!({
            "repoId": workspace.repo_id,
            "idempotencyKey": "first-ever-start",
            "startCompute": true,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{started}");
    assert_eq!(started["data"]["pendingStart"], json!(true), "{started}");
    let intent_id = Uuid::parse_str(
        started["data"]["startCompute"]["intentId"]
            .as_str()
            .expect("intent"),
    )
    .expect("uuid");
    let (task_status, bound, _) = task_state(&app, task_id_of(&started)).await;
    assert_eq!((task_status.as_str(), bound), ("queued", Some(intent_id)));

    app.cleanup().await;
}

/// Start workspace and preview sent to a VM that came up meanwhile (its
/// consent consumed) attaches to the running compute: no new Start is
/// recorded and the preview is queued unbound for the dispatcher.
#[tokio::test]
async fn a_compound_start_on_a_running_vm_attaches_and_records_no_start() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    runtime
        .seed_file(workspace.repo_id, ".g5/workspace.yaml", WORKSPACE_DEFAULT)
        .await;
    let client = app.authenticated_client(&owner);
    let consumed = Uuid::new_v4();
    let now = chrono::Utc::now();
    sqlx::query("UPDATE workspace_vms SET start_intent = $2 WHERE workspace_id = $1")
        .bind(workspace.id)
        .bind(json!({
            "id": consumed, "kind": "compound_start", "owner_user_id": owner.id,
            "lifecycle_generation": 0, "status": "consumed", "created_at": now,
            "expires_at": now + chrono::Duration::minutes(15), "attempts": 0,
        }))
        .execute(&app.db)
        .await
        .expect("seed consumed consent");

    let (status, started) = start_preview(
        &client,
        workspace.id,
        json!({
            "repoId": workspace.repo_id,
            "idempotencyKey": "running-compound",
            "startCompute": true,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{started}");
    assert_eq!(
        started["data"]["startCompute"]["status"],
        json!("running"),
        "{started}"
    );
    assert_eq!(started["data"]["pendingStart"], json!(false), "{started}");
    let (task_status, bound, _) = task_state(&app, task_id_of(&started)).await;
    assert_eq!((task_status.as_str(), bound), ("queued", None));
    let intent: String = sqlx::query_scalar(
        "SELECT start_intent ->> 'id' FROM workspace_vms WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .expect("intent id");
    assert_eq!(intent, consumed.to_string(), "no new Start was recorded");

    app.cleanup().await;
}

/// While the VM is starting under a Start the owner cannot bind to (the
/// platform's automatic continuation), Preview once running and Start
/// workspace and preview are both refused with the reason, nothing is
/// recorded and the running Start is left alone.
#[tokio::test]
async fn a_boot_under_a_start_the_owner_cannot_bind_to_refuses_with_the_reason() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let mut workspace = TestWorkspace::create(&app, &org).await;
    workspace
        .commit_file(&app, ".g5/workspace.yaml", WORKSPACE_DEFAULT)
        .await;
    workspace.attach_parent(&app).await;
    let _runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let client = app.authenticated_client(&owner);
    stop(&client, workspace.id).await;
    let (status, resumed) = body_of(
        client
            .post_json(
                &format!("/api/v1/workspaces/{}/resume", workspace.id),
                &json!({ "operationId": Uuid::new_v4() }),
            )
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{resumed}");
    let (vm_status, intent_id): (String, String) = sqlx::query_as(
        "SELECT status, start_intent ->> 'id' FROM workspace_vms WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .expect("vm row");
    assert_eq!(vm_status, "starting", "the Resume placed the VM");
    // The boot continues under the platform's own consent.
    sqlx::query(
        "UPDATE workspace_vms
         SET start_intent = jsonb_set(start_intent, '{kind}', '\"automatic_continuation\"')
         WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .execute(&app.db)
    .await
    .expect("platform consent");

    let (status, refused) = start_preview(
        &client,
        workspace.id,
        json!({ "repoId": workspace.repo_id, "idempotencyKey": "once-running" }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{refused}");
    assert!(
        refused["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("without a Start of yours"),
        "{refused}"
    );

    let (status, refused) = start_preview(
        &client,
        workspace.id,
        json!({
            "repoId": workspace.repo_id,
            "idempotencyKey": "start-and-preview",
            "startCompute": true,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{refused}");
    assert!(
        refused["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("cannot wait for"),
        "{refused}"
    );
    let (vm_status, live_intent, previews): (String, String, i64) = sqlx::query_as(
        "SELECT vm.status, vm.start_intent ->> 'id',
                (SELECT count(*) FROM tasks WHERE workspace_id = vm.workspace_id AND kind = 'preview')
         FROM workspace_vms AS vm WHERE vm.workspace_id = $1",
    )
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .expect("vm row");
    assert_eq!(
        (vm_status.as_str(), live_intent, previews),
        ("starting", intent_id, 0),
        "the running Start is untouched and nothing was recorded"
    );

    app.cleanup().await;
}

/// A committed configuration the launch would refuse is reported as that
/// refusal, not as missing, and records no Start.
#[tokio::test]
async fn a_malformed_committed_configuration_is_reported_and_records_no_start() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let mut workspace = TestWorkspace::create(&app, &org).await;
    workspace
        .commit_file(
            &app,
            ".g5/workspace.yaml",
            "preview:\n  command: 42\n  port: here\n",
        )
        .await;
    workspace.attach_parent(&app).await;
    let _runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let client = app.authenticated_client(&owner);
    stop(&client, workspace.id).await;
    let before: Option<String> = sqlx::query_scalar(
        "SELECT start_intent ->> 'id' FROM workspace_vms WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .expect("intent before");

    let (status, refused) = start_preview(
        &client,
        workspace.id,
        json!({
            "repoId": workspace.repo_id,
            "idempotencyKey": "malformed-config",
            "startCompute": true,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{refused}");
    let message = refused["error"]["message"].as_str().unwrap_or_default();
    assert!(
        message.contains(".g5/workspace.yaml is refused") && message.contains("port"),
        "the committed file is refused for its content: {refused}"
    );
    let after: Option<String> = sqlx::query_scalar(
        "SELECT start_intent ->> 'id' FROM workspace_vms WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .expect("intent after");
    assert_eq!(after, before, "no Start was recorded");

    app.cleanup().await;
}

/// A preview intent recorded before this change carries the older digest
/// (repository, profile and port); a replay of it still matches.
#[tokio::test]
async fn a_replay_matches_an_intent_recorded_with_the_previous_digest_shape() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let mut workspace = TestWorkspace::create(&app, &org).await;
    workspace
        .commit_file(&app, ".g5/workspace.yaml", WORKSPACE_DEFAULT)
        .await;
    workspace.attach_parent(&app).await;
    let _runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let client = app.authenticated_client(&owner);
    stop(&client, workspace.id).await;

    let (status, started) = start_preview(
        &client,
        workspace.id,
        json!({
            "repoId": workspace.repo_id,
            "idempotencyKey": "legacy-digest",
            "startCompute": true,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{started}");
    let task_id = task_id_of(&started);
    let legacy = g5_api_core::services::task_lifecycle::legacy_preview_request_digest(
        workspace.repo_id,
        None,
        4173,
    );
    sqlx::query(
        "UPDATE workspace_vms
         SET preview_intent = jsonb_set(preview_intent, '{request_digest}', to_jsonb($2::text))
         WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .bind(&legacy)
    .execute(&app.db)
    .await
    .expect("age the digest");

    let (status, replay) = start_preview(
        &client,
        workspace.id,
        json!({
            "repoId": workspace.repo_id,
            "idempotencyKey": "legacy-digest",
            "startCompute": true,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{replay}");
    assert_eq!(task_id_of(&replay), task_id);
    // A different request under the same key is still refused.
    let (status, reused) = start_preview(
        &client,
        workspace.id,
        json!({
            "repoId": workspace.repo_id,
            "profile": "admin",
            "idempotencyKey": "legacy-digest",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{reused}");

    app.cleanup().await;
}

/// The committed listing offers the workspace default and the named profiles
/// the branch commits, in the launch's shape; the live listing is unchanged.
#[tokio::test]
async fn committed_preview_applications_list_the_branch_configuration() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let mut workspace = TestWorkspace::create(&app, &org).await;
    let client = app.authenticated_client(&owner);
    let path = format!(
        "/api/v1/workspaces/{}/repos/{}/preview-applications?source=committed",
        workspace.id, workspace.repo_id
    );

    let (status, empty) = body_of(client.get(&path).await).await;
    assert_eq!(status, StatusCode::OK, "{empty}");
    assert_eq!(empty["data"], json!([]), "nothing committed yet");

    workspace
        .commit_file(&app, ".g5/workspace.yaml", WORKSPACE_DEFAULT)
        .await;
    workspace
        .commit_file(
            &app,
            ".g5/preview_profiles/admin.yaml",
            "command: npm run admin\nport: 5173\ndescription: Admin UI\n",
        )
        .await;
    workspace
        .commit_file(&app, ".g5/preview_profiles/broken.yaml", "port: nope\n")
        .await;
    let (status, listed) = body_of(client.get(&path).await).await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    let applications = listed["data"].as_array().expect("array");
    assert_eq!(applications.len(), 2, "{listed}");
    assert_eq!(applications[0]["source"], json!("workspaceDefault"));
    assert_eq!(applications[0]["configPath"], json!(".g5/workspace.yaml"));
    assert_eq!(applications[0]["port"], json!(4173));
    assert_eq!(applications[1]["profile"], json!("admin"));
    assert_eq!(
        applications[1]["configPath"],
        json!(".g5/preview_profiles/admin.yaml")
    );
    assert_eq!(applications[1]["description"], json!("Admin UI"));

    let (status, unknown) = body_of(
        client
            .get(&format!(
                "/api/v1/workspaces/{}/repos/{}/preview-applications?source=other",
                workspace.id, workspace.repo_id
            ))
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{unknown}");

    app.cleanup().await;
}
