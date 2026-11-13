//! Sessions are named the moment they start: `POST /workspaces/{id}/sessions/start`
//! without a `title` derives a provisional one from the first request (the
//! frontend showed "New Session" for every session before this, because the
//! column was never set). An explicit title is kept verbatim.

use crate::helpers;

use helpers::{TestApp, TestOrg, TestUser, TestWorkspace};
use reqwest::StatusCode;
use serde_json::{json, Value};

fn data(body: &Value) -> &Value {
    body.get("data").unwrap_or(body)
}

async fn start_session(app: &TestApp, body: Value) -> (String, Value) {
    let mut owner = TestUser::create(app).await;
    let org = TestOrg::create_with_admin(app, &mut owner).await;
    let workspace = TestWorkspace::create(app, &org).await;
    let client = app.authenticated_client(&owner);
    let started = client
        .post_json(
            &format!("/api/v1/workspaces/{}/sessions/start", workspace.id),
            &body,
        )
        .await;
    let status = started.status();
    let started: Value = started.json().await.expect("parse start response");
    assert_eq!(
        status,
        StatusCode::CREATED,
        "session start failed: {started}"
    );
    let session_id = data(&started)["session"]["id"]
        .as_str()
        .expect("start response carries the session id")
        .to_string();
    (session_id, started)
}

#[tokio::test]
async fn a_session_started_without_a_title_is_named_from_its_first_request() {
    let app = TestApp::spawn().await;
    let (session_id, started) = start_session(
        &app,
        json!({
            "content": "can you check the preview? the tab it opens is blank",
        }),
    )
    .await;

    // The start response already carries the provisional title, so the
    // session list shows it before the first turn runs.
    assert_eq!(
        data(&started)["session"]["title"].as_str(),
        Some("can you check the preview?"),
        "start response: {started}"
    );
    let stored: Option<String> =
        sqlx::query_scalar("SELECT title FROM chat_sessions WHERE id = $1::uuid")
            .bind(&session_id)
            .fetch_one(&app.db)
            .await
            .expect("load session title");
    assert_eq!(stored.as_deref(), Some("can you check the preview?"));

    app.cleanup().await;
}

#[tokio::test]
async fn an_explicit_title_is_kept_verbatim() {
    let app = TestApp::spawn().await;
    let (session_id, started) = start_session(
        &app,
        json!({
            "content": "can you check the preview? the tab it opens is blank",
            "title": "  Preview triage  ",
        }),
    )
    .await;
    assert_eq!(
        data(&started)["session"]["title"].as_str(),
        Some("Preview triage")
    );
    let stored: Option<String> =
        sqlx::query_scalar("SELECT title FROM chat_sessions WHERE id = $1::uuid")
            .bind(&session_id)
            .fetch_one(&app.db)
            .await
            .expect("load session title");
    assert_eq!(stored.as_deref(), Some("Preview triage"));

    app.cleanup().await;
}
