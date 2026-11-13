//! Sending a chat message to a workspace for its agent to work on: a person
//! starting a chat (with a request key that makes retries safe), and a fork
//! that starts a chat in the workspace it creates.

use crate::helpers;

use helpers::{TestApp, TestOrg, TestUser, TestWorkspace};
use reqwest::StatusCode;
use serde_json::{json, Value};
use uuid::Uuid;

async fn json_of(response: reqwest::Response) -> (StatusCode, Value) {
    let status = response.status();
    let body = response.json::<Value>().await.unwrap_or(Value::Null);
    (status, body)
}

async fn count(app: &TestApp, sql: &str, id: Uuid) -> i64 {
    sqlx::query_scalar(sql)
        .bind(id)
        .fetch_one(&app.db)
        .await
        .unwrap()
}

#[tokio::test]
async fn a_request_key_returns_the_chat_the_first_request_started() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let client = app.authenticated_client(&owner);
    let start_path = format!("/api/v1/workspaces/{}/sessions/start", workspace.id);
    let start = |content: &str, key: &str| json!({"content": content, "requestKey": key});

    let (status, first) = json_of(
        client
            .post_json(&start_path, &start("Fix the login redirect.", "start-1"))
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{first}");
    // A retry of the same request replays it.
    let (status, replay) = json_of(
        client
            .post_json(&start_path, &start("Fix the login redirect.", "start-1"))
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{replay}");
    assert_eq!(
        replay["data"]["session"]["id"],
        first["data"]["session"]["id"]
    );
    assert_eq!(
        replay["data"]["message"]["inputId"],
        first["data"]["message"]["inputId"]
    );
    assert_eq!(replay["data"]["taskId"], first["data"]["taskId"]);
    assert_eq!(
        count(
            &app,
            "SELECT COUNT(*) FROM chat_sessions WHERE workspace_id = $1",
            workspace.id
        )
        .await,
        1
    );
    // The key names that message; another message needs another key.
    let (status, body) = json_of(
        client
            .post_json(&start_path, &start("Fix the signup page.", "start-1"))
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["error"]["details"]["field"], "requestKey", "{body}");
    // Without a key, each request starts its own chat.
    for _ in 0..2 {
        let (status, body) = json_of(
            client
                .post_json(&start_path, &json!({"content": "Another task."}))
                .await,
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
    }
    assert_eq!(
        count(
            &app,
            "SELECT COUNT(*) FROM chat_sessions WHERE workspace_id = $1",
            workspace.id
        )
        .await,
        3
    );

    app.cleanup().await;
}

async fn fork(
    app: &TestApp,
    user: &TestUser,
    source_id: Uuid,
    name: &str,
    key: &str,
    start_chat: Value,
) -> (StatusCode, Value) {
    json_of(
        app.authenticated_client(user)
            .post_json(
                &format!("/api/v1/workspaces/{source_id}/clone"),
                &json!({"name": name, "idempotencyKey": key, "startChat": start_chat}),
            )
            .await,
    )
    .await
}

#[tokio::test]
async fn a_fork_starts_a_chat_in_the_workspace_it_creates() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let base = TestWorkspace::create(&app, &org).await;
    let instruction = json!({"content": "Work on acme/web#12: fix the login redirect."});

    // An empty first message is refused before any fork work.
    let (status, body) = fork(
        &app,
        &owner,
        base.id,
        "fix/empty",
        "fork-chat-0",
        json!({"content": "   "}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(
        body["error"]["details"]["field"], "startChat.content",
        "{body}"
    );

    let (status, body) = fork(
        &app,
        &owner,
        base.id,
        "fix/login-redirect",
        "fork-chat-1",
        instruction.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let child: Uuid = body["data"]["workspace"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let started = &body["data"]["startedChat"];
    assert_eq!(started["error"], Value::Null, "{body}");
    let session_id: Uuid = started["sessionId"]
        .as_str()
        .expect("the fork started a chat")
        .parse()
        .unwrap();

    // The chat belongs to the new workspace, is named from its message and
    // has the message queued for the owner's agent.
    let (workspace_id, title, created_by): (Uuid, Option<String>, Uuid) =
        sqlx::query_as("SELECT workspace_id, title, created_by FROM chat_sessions WHERE id = $1")
            .bind(session_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(workspace_id, child);
    assert_eq!(created_by, owner.id);
    assert!(title.is_some_and(|title| title.contains("acme/web#12")));
    let (content, delivery, origin): (String, String, String) = sqlx::query_as(
        "SELECT content, delivery, input_origin FROM session_inputs WHERE session_id = $1",
    )
    .bind(session_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(content, "Work on acme/web#12: fix the login redirect.");
    assert_eq!((delivery.as_str(), origin.as_str()), ("queued", "human"));
    assert_eq!(
        count(
            &app,
            "SELECT COUNT(*) FROM tasks WHERE chat_session_id = $1 AND kind = 'agent' \
             AND status = 'queued'",
            session_id
        )
        .await,
        1
    );

    // A replay returns the same fork and chat; nothing is sent again.
    let (status, replay) = fork(
        &app,
        &owner,
        base.id,
        "fix/login-redirect",
        "fork-chat-1",
        instruction,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{replay}");
    assert_eq!(replay["data"]["workspace"]["id"], child.to_string());
    assert_eq!(
        replay["data"]["startedChat"]["sessionId"],
        session_id.to_string()
    );
    assert_eq!(
        count(
            &app,
            "SELECT COUNT(*) FROM chat_sessions WHERE workspace_id = $1",
            child
        )
        .await,
        1
    );
    // The message is part of the request the key names.
    let (status, body) = fork(
        &app,
        &owner,
        base.id,
        "fix/login-redirect",
        "fork-chat-1",
        json!({"content": "Something else."}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(
        body["error"]["details"]["field"], "idempotencyKey",
        "{body}"
    );

    // A fork without a message starts no chat.
    let (status, body) = json_of(
        app.authenticated_client(&owner)
            .post_json(
                &format!("/api/v1/workspaces/{}/clone", base.id),
                &json!({"name": "fix/plain", "idempotencyKey": "fork-plain"}),
            )
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["data"]["startedChat"], Value::Null);

    app.cleanup().await;
}

#[tokio::test]
async fn a_fork_s_chat_runs_with_the_requester_s_launch_choices() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let base = TestWorkspace::create(&app, &org).await;
    let client = app.authenticated_client(&owner);
    let config = client
        .put_json(
            &format!("/api/v1/orgs/{}/config", org.id),
            &json!({"setSecrets":{"ANTHROPIC_API_KEY":"sk-ant-ci-test-key"}}),
        )
        .await;
    assert_eq!(config.status(), StatusCode::OK);

    // The choices the chat composer would send win over configured defaults.
    let (status, body) = fork(
        &app,
        &owner,
        base.id,
        "fix/with-choices",
        "fork-launch-1",
        json!({
            "content": "Fix the login redirect.",
            "launchPreferences": {
                "schemaVersion": 1,
                "permissionMode": {"source": "explicit", "value": "default"}
            }
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let session_id: Uuid = body["data"]["startedChat"]["sessionId"]
        .as_str()
        .unwrap_or_else(|| panic!("the chat started: {body}"))
        .parse()
        .unwrap();
    let spec: Value =
        sqlx::query_scalar("SELECT spec FROM tasks WHERE chat_session_id = $1 AND kind = 'agent'")
            .bind(session_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(
        spec["agent"]["agentLaunch"]["permissionMode"],
        json!({"source": "explicit", "value": "default"}),
        "{spec}"
    );

    // Choices that cannot launch in the new workspace leave the fork
    // complete, say why, and create no chat.
    let (status, body) = fork(
        &app,
        &owner,
        base.id,
        "fix/unknown-account",
        "fork-launch-2",
        json!({
            "content": "Fix the login redirect.",
            "launchPreferences": {
                "schemaVersion": 1,
                "permissionMode": {"source": "explicit", "value": "default"},
                "account": {
                    "source": "explicit",
                    "value": {"kind": "account", "accountId": Uuid::new_v4()}
                }
            }
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let child: Uuid = body["data"]["workspace"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let started = &body["data"]["startedChat"];
    assert_eq!(started["sessionId"], Value::Null, "{body}");
    assert!(started["error"].is_string(), "{body}");
    assert_eq!(
        count(
            &app,
            "SELECT COUNT(*) FROM chat_sessions WHERE workspace_id = $1",
            child
        )
        .await,
        0,
        "the failed chat is rolled back, not left half-created"
    );

    // Invalid choices are refused before any fork work.
    let (status, body) = fork(
        &app,
        &owner,
        base.id,
        "fix/bad-choices",
        "fork-launch-3",
        json!({
            "content": "Fix the login redirect.",
            "launchPreferences": {"schemaVersion": 9}
        }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");

    app.cleanup().await;
}
