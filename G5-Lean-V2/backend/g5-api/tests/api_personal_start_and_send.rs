//! Start and send (`g5.personal.start-and-send`) for personal workspaces
//! (G5-Lean#624).
//!
//! Each test spawns a full `TestApp` with an isolated Postgres database and
//! drives the real HTTP endpoints, with the scripted runtime peer standing in
//! for the host where a VM is needed.

use crate::helpers;

use helpers::{TestApp, TestOrg, TestUser, TestWorkspace, TestWorkspaceRuntimePeer};
use reqwest::StatusCode;
use serde_json::{json, Value};
use uuid::Uuid;

async fn body_of(response: reqwest::Response) -> (StatusCode, Value) {
    let status = response.status();
    (status, response.json().await.unwrap_or(Value::Null))
}

async fn input_state(app: &TestApp, input_id: Uuid) -> (String, Option<Uuid>, Value) {
    sqlx::query_as::<_, (String, Option<Uuid>, Value)>(
        "SELECT delivery, start_consent_id, delivery_audit FROM session_inputs WHERE id = $1",
    )
    .bind(input_id)
    .fetch_one(&app.db)
    .await
    .expect("input state")
}

async fn stop(client: &helpers::AuthenticatedClient, workspace_id: Uuid) -> Value {
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
    body
}

/// Start and send on a stopped VM records `compound_start` consent, binds the
/// queued message to it, and a Stop that cancels the Start withdraws the
/// message, so nothing stays queued for a later Start.
#[tokio::test]
async fn start_and_send_binds_the_message_and_cancelling_the_start_withdraws_it() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    workspace.attach_parent(&app).await;
    let _runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let client = app.authenticated_client(&owner);
    stop(&client, workspace.id).await;

    let (status, started) = body_of(
        client
            .post_json(
                &format!("/api/v1/workspaces/{}/sessions/start", workspace.id),
                &json!({ "content": "summarize the codebase", "startCompute": true }),
            )
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{started}");
    let start = &started["data"]["message"]["startCompute"];
    assert_eq!(start["status"], json!("starting"), "{started}");
    let intent_id = Uuid::parse_str(start["intentId"].as_str().expect("intent")).expect("uuid");
    let input_id = Uuid::parse_str(
        started["data"]["message"]["inputId"]
            .as_str()
            .expect("input id"),
    )
    .expect("uuid");
    let task_id =
        Uuid::parse_str(started["data"]["taskId"].as_str().expect("task id")).expect("uuid");

    let kind: String = sqlx::query_scalar(
        "SELECT start_intent ->> 'kind' FROM workspace_vms WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .expect("intent kind");
    assert_eq!(kind, "compound_start", "a clearly labelled compound Start");
    let (delivery, bound, _) = input_state(&app, input_id).await;
    assert_eq!(delivery, "queued");
    assert_eq!(
        bound,
        Some(intent_id),
        "the message is bound to its Start consent"
    );

    // Stop while starting cancels that consent; the message goes with it.
    stop(&client, workspace.id).await;
    let (delivery, bound, audit) = input_state(&app, input_id).await;
    assert_eq!(delivery, "withdrawn", "{audit}");
    assert_eq!(bound, None);
    assert!(
        audit
            .as_array()
            .expect("audit")
            .iter()
            .any(|event| event["event"] == json!("start_ended")),
        "{audit}"
    );

    // The task left without input is settled.
    g5_api::maintenance::agent_tasks::cancel_tasks_left_by_ended_starts(&app.state, 32)
        .await
        .expect("sweep");
    let task_status: String = sqlx::query_scalar("SELECT status FROM tasks WHERE id = $1")
        .bind(task_id)
        .fetch_one(&app.db)
        .await
        .expect("task status");
    assert_eq!(task_status, "cancelled");

    app.cleanup().await;
}

/// Ready releases the binding; expiry withdraws.
#[tokio::test]
async fn consent_reaching_ready_releases_the_message_and_expiry_withdraws_it() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let client = app.authenticated_client(&owner);

    for (index, (terminal, expect_delivery)) in [("consumed", "queued"), ("expired", "withdrawn")]
        .into_iter()
        .enumerate()
    {
        let (status, started) = body_of(
            client
                .post_json(
                    &format!("/api/v1/workspaces/{}/sessions/start", workspace.id),
                    &json!({ "content": format!("message {index}") }),
                )
                .await,
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{started}");
        let input_id = Uuid::parse_str(
            started["data"]["message"]["inputId"]
                .as_str()
                .expect("input id"),
        )
        .expect("uuid");
        let intent_id = Uuid::new_v4();
        let now = chrono::Utc::now();
        let intent = json!({
            "id": intent_id, "kind": "compound_start", "owner_user_id": owner.id,
            "lifecycle_generation": 0, "status": "pending", "created_at": now,
            "expires_at": now + chrono::Duration::minutes(15), "attempts": 0,
        });
        sqlx::query(
            "INSERT INTO workspace_vms(org_id, workspace_id, status, start_intent)
             VALUES ($1, $2, 'stopped', $3)
             ON CONFLICT (workspace_id) DO UPDATE SET start_intent = EXCLUDED.start_intent",
        )
        .bind(org.id)
        .bind(workspace.id)
        .bind(&intent)
        .execute(&app.db)
        .await
        .expect("seed intent");
        sqlx::query("UPDATE session_inputs SET start_consent_id = $2 WHERE id = $1")
            .bind(input_id)
            .bind(intent_id)
            .execute(&app.db)
            .await
            .expect("bind");
        sqlx::query(
            "UPDATE workspace_vms SET start_intent = jsonb_set(start_intent, '{status}', to_jsonb($2::text))
             WHERE workspace_id = $1",
        )
        .bind(workspace.id)
        .bind(terminal)
        .execute(&app.db)
        .await
        .expect("end consent");
        let (delivery, bound, _) = input_state(&app, input_id).await;
        assert_eq!(delivery, expect_delivery, "{terminal}");
        assert_eq!(bound, None, "{terminal} settles the binding");
    }

    app.cleanup().await;
}

/// Start and send needs the owner, like Start.
#[tokio::test]
async fn start_and_send_is_refused_for_a_non_owner() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let other = TestUser::create_with_scopes(
        &app,
        org.id,
        &[
            g5_auth::Scope::WorkspacesRead,
            g5_auth::Scope::WorkspacesWrite,
            g5_auth::Scope::SessionsWrite,
        ],
    )
    .await;
    let (status, body) = body_of(
        app.authenticated_client(&other)
            .post_json(
                &format!("/api/v1/workspaces/{}/sessions/start", workspace.id),
                &json!({ "content": "not mine", "startCompute": true }),
            )
            .await,
    )
    .await;
    assert!(
        matches!(status, StatusCode::FORBIDDEN | StatusCode::NOT_FOUND),
        "{status} {body}"
    );
    let starts: i64 =
        sqlx::query_scalar("SELECT count(*) FROM workspace_vms WHERE workspace_id = $1")
            .bind(workspace.id)
            .fetch_one(&app.db)
            .await
            .expect("vm rows");
    assert_eq!(starts, 0, "no consent was recorded");

    app.cleanup().await;
}
