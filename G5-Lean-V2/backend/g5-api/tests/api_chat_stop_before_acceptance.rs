//! Regression guard from the qa provider-lane e2e failures after #117: the
//! id that `POST /workspaces/{id}/sessions/start` (and send) returns as the
//! turn's message id is the session input's id. No `chat_messages` row exists
//! for it until the runtime accepts the turn, and stop validated the id
//! against `chat_messages` only, so stopping a still-queued turn, which is
//! what the UI's Stop button and the e2e cleanup do, was refused with 422
//! "Chat message ID is not linked to this session".

use crate::helpers;

use helpers::{TestApp, TestOrg, TestUser, TestWorkspace};
use reqwest::StatusCode;
use serde_json::{json, Value};

fn data(body: &Value) -> &Value {
    body.get("data").unwrap_or(body)
}

#[tokio::test]
async fn owner_can_stop_a_queued_turn_by_the_id_start_returned() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let client = app.authenticated_client(&owner);

    let started = client
        .post_json(
            &format!("/api/v1/workspaces/{}/sessions/start", workspace.id),
            &json!({
                "content": "stop me before the runtime accepts this turn",
                "title": "stop before acceptance",
            }),
        )
        .await;
    let started_status = started.status();
    let started: Value = started.json().await.expect("parse start response");
    assert_eq!(
        started_status,
        StatusCode::CREATED,
        "session start failed: {started}"
    );
    let session_id = data(&started)["session"]["id"]
        .as_str()
        .expect("start response carries the session id")
        .to_string();
    let assistant_message_id = data(&started)["assistantMessageId"]
        .as_str()
        .expect("start response carries assistantMessageId")
        .to_string();
    assert_eq!(
        data(&started)["message"]["inputId"].as_str(),
        Some(assistant_message_id.as_str()),
        "the turn's message id is its session input id"
    );

    // Nothing has accepted the turn: no chat_messages row carries this id.
    let persisted: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM chat_messages WHERE id = $1::uuid)")
            .bind(&assistant_message_id)
            .fetch_one(&app.db)
            .await
            .expect("check chat_messages");
    assert!(
        !persisted,
        "queued turn must not have a chat_messages row yet"
    );

    let stopped = client
        .post_json(
            &format!("/api/v1/sessions/{session_id}/messages/{assistant_message_id}/stop"),
            &json!({}),
        )
        .await;
    let stopped_status = stopped.status();
    let stopped: Value = stopped.json().await.unwrap_or(Value::Null);
    assert_eq!(
        stopped_status,
        StatusCode::OK,
        "stop of a queued turn refused: {stopped}"
    );
    assert_eq!(data(&stopped)["stopped"], true);

    // A foreign id is still refused.
    let foreign = client
        .post_json(
            &format!(
                "/api/v1/sessions/{session_id}/messages/{}/stop",
                uuid::Uuid::new_v4()
            ),
            &json!({}),
        )
        .await;
    assert_eq!(foreign.status(), StatusCode::UNPROCESSABLE_ENTITY);

    app.cleanup().await;
}
