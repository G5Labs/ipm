//! A first message in an existing chat must carry its selected permission mode
//! into the launch resolver when the org has no configured default.

use crate::helpers;

use g5_api::services::chat_service::ChatService;
use g5_api::services::git_conflict_resolution::{
    ConflictResolutionLaunch, ConflictResolutionLauncher,
};
use helpers::{TestApp, TestOrg, TestUser, TestWorkspace};
use reqwest::StatusCode;
use serde_json::{json, Value};
use uuid::Uuid;

fn data(body: &Value) -> &Value {
    body.get("data").unwrap_or(body)
}

#[tokio::test]
async fn first_message_uses_its_explicit_permission_mode() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let client = app.authenticated_client(&owner);

    let config = client
        .put_json(
            &format!("/api/v1/orgs/{}/config", org.id),
            &json!({"setSecrets":{"ANTHROPIC_API_KEY":"sk-ant-ci-test-key"}}),
        )
        .await;
    assert_eq!(config.status(), StatusCode::OK);
    let created = client
        .post_json(
            &format!("/api/v1/workspaces/{}/sessions", workspace.id),
            &json!({
                "sessionType":"chat",
                "title":"Explicit permission mode",
                "agentProvider":"claude_code"
            }),
        )
        .await;
    let created_status = created.status();
    let created: Value = created.json().await.expect("session response");
    assert_eq!(created_status, StatusCode::CREATED, "{created}");
    let session_id: Uuid = data(&created)["id"]
        .as_str()
        .expect("session id")
        .parse()
        .expect("valid session id");

    let sent = client
        .post_json(
            &format!("/api/v1/sessions/{session_id}/messages"),
            &json!({
                "content":"Run the first turn",
                "permissionMode":"default"
            }),
        )
        .await;
    let sent_status = sent.status();
    let sent: Value = sent.json().await.expect("message response");
    assert_eq!(sent_status, StatusCode::CREATED, "{sent}");
    let task_id: Uuid = data(&sent)["taskId"]
        .as_str()
        .expect("task id")
        .parse()
        .expect("valid task id");
    let spec: Value = sqlx::query_scalar("SELECT spec FROM tasks WHERE id = $1")
        .bind(task_id)
        .fetch_one(&app.db)
        .await
        .expect("persisted task spec");
    assert_eq!(
        spec["agent"]["agentLaunch"]["permissionMode"],
        json!({"source":"explicit","value":"default"}),
        "the first message's selected mode must be durable: {spec}"
    );

    let reopened: Value = client
        .get(&format!("/api/v1/sessions/{session_id}"))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(
        data(&reopened)["providerState"]["claudeCode"]["settings"]["permissionMode"],
        "default"
    );
    let changed = client.post_json(&format!("/api/v1/sessions/{session_id}/messages"), &json!({
        "content": "Cannot change the queued first turn's mode", "permissionMode": "bypassPermissions",
    })).await;
    assert_eq!(changed.status(), StatusCode::UNPROCESSABLE_ENTITY);

    app.cleanup().await;
}

#[tokio::test]
async fn claude_conflict_chat_keeps_its_launch_mode_on_reopen_and_fork() {
    conflict_chat_mode("claude_code", "claudeCode", "bypassPermissions", "default").await;
}

#[tokio::test]
async fn codex_conflict_chat_keeps_its_launch_mode_on_reopen_and_fork() {
    conflict_chat_mode(
        "codex",
        "codex",
        "never/danger-full-access",
        "never/workspace-write",
    )
    .await;
}

async fn conflict_chat_mode(provider: &str, namespace: &str, mode: &str, wrong_mode: &str) {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    helpers::seed_org_llm_credentials_all_providers(&app, org.id).await;
    let client = app.authenticated_client(&owner);
    let preferences = json!({ "schemaVersion": 1,
        "provider": { "source": "explicit", "value": provider },
        "permissionMode": { "source": "explicit", "value": mode },
    });
    let preview = client
        .post_json(
            &format!(
                "/api/v1/workspaces/{}/config/agent-launch/resolve",
                workspace.id
            ),
            &json!({ "preferences": preferences }),
        )
        .await;
    let status = preview.status();
    let preview: Value = preview.json().await.unwrap();
    assert_eq!(status, StatusCode::OK, "{preview}");
    let selection = serde_json::from_value(json!({ "preferences": preferences,
        "confirmedResolutionId": data(&preview)["resolved"]["resolutionId"],
    }))
    .unwrap();
    let _runtime = helpers::TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let mut db = app.scoped_db_for_org(org.id).await;
    let launched = app
        .state
        .workspace_messenger
        .launch(
            &mut db,
            ConflictResolutionLaunch {
                workspace_id: workspace.id,
                actor_user_id: owner.id,
                agent_launch: Some(&selection),
                title: "Resolve Git conflict",
                scope_ref: None,
                template_ref: "test-conflict",
                template_revision: "v1",
                prompt: "Resolve the conflict".into(),
            },
        )
        .await
        .expect("conflict launcher");
    db.commit().await.unwrap();
    let session_id = launched.session_id;
    let path = format!("/api/v1/sessions/{session_id}");
    let saved: Value = sqlx::query_scalar("SELECT provider_state FROM chat_sessions WHERE id=$1")
        .bind(session_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(
        saved[namespace]["settings"]["permissionMode"], mode,
        "conflict launcher initializes settings before dispatch"
    );
    assert!(saved[namespace]["settings"]["account"].is_object());

    let mut db = app.scoped_db_for_org(org.id).await;
    let dispatch = app
        .state
        .task_service
        .reserve_agent_dispatch(&mut db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(dispatch.task_id, launched.task_id);
    db.commit().await.unwrap();
    // Add a historical fixture without rewriting the immutable dispatch row:
    // old Codex snapshots were flat; old Claude snapshots could spell Bypass
    // as `auto`. Its earlier timestamp makes it the frozen first launch.
    let historical = sqlx::query("INSERT INTO task_attempts(org_id,task_id,attempt,vm_id,epoch,start_confirmed,started_at,policy_snapshot_ref)
        SELECT org_id,task_id,attempt+1,vm_id,epoch,true,started_at - interval '1 second',
            CASE WHEN $2 THEN jsonb_set(policy_snapshot_ref, '{agent_launch}', policy_snapshot_ref->'agent_launch'->'resolved')
            ELSE jsonb_set(policy_snapshot_ref, '{agent_launch,resolved,permissionMode,value}', '\"auto\"') END
        FROM task_attempts WHERE task_id=$1 AND attempt=1")
        .bind(launched.task_id).bind(provider == "codex").execute(&app.db).await.unwrap();
    assert_eq!(historical.rows_affected(), 1);
    // A later attempt and saved settings must never override the first mode.
    let later_attempt = sqlx::query("INSERT INTO task_attempts(org_id,task_id,attempt,vm_id,epoch,start_confirmed,started_at,policy_snapshot_ref)
        SELECT org_id,task_id,attempt+2,vm_id,epoch,true,started_at + interval '1 second',
            jsonb_set(policy_snapshot_ref, $2::text[], to_jsonb($3::text))
        FROM task_attempts WHERE task_id=$1 AND attempt=1")
        .bind(launched.task_id)
        .bind(vec!["agent_launch", "resolved", "permissionMode", "value"])
        .bind(wrong_mode).execute(&app.db).await.unwrap();
    assert_eq!(later_attempt.rows_affected(), 1);
    for state in [
        json!({}),
        json!({ namespace: { "settings": { "permissionMode": wrong_mode } } }),
    ] {
        sqlx::query("UPDATE chat_sessions SET provider_state=$2 WHERE id=$1")
            .bind(session_id)
            .bind(state)
            .execute(&app.db)
            .await
            .unwrap();
        let detail: Value = client.get(&path).await.json().await.unwrap();
        assert_eq!(
            data(&detail)["providerState"][namespace]["settings"]["permissionMode"],
            mode,
            "{detail}"
        );
        let list: Value = client
            .get(&format!("/api/v1/workspaces/{}/sessions", workspace.id))
            .await
            .json()
            .await
            .unwrap();
        assert_eq!(
            data(&list)[0]["providerState"][namespace]["settings"]["permissionMode"],
            mode,
            "{list}"
        );
        let sent = client
            .post_json(
                &format!("{path}/messages"),
                &json!({ "content": "Continue", "permissionMode": mode }),
            )
            .await;
        let status = sent.status();
        let sent: Value = sent.json().await.unwrap();
        assert_eq!(status, StatusCode::CREATED, "{sent}");
        let changed = client
            .post_json(
                &format!("{path}/messages"),
                &json!({ "content": "Wrong mode", "permissionMode": wrong_mode }),
            )
            .await;
        let status = changed.status();
        let changed: Value = changed.json().await.unwrap();
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{changed}");
        assert_eq!(
            changed["error"]["details"]["code"],
            "immutable_chat_permission_mode"
        );
    }
    // A plain history fork and workspace carry have no attempts of their own.
    // They must materialize the source's effective mode in their saved state.
    sqlx::query("INSERT INTO chat_messages(id,org_id,session_id,role,content,status,message_index) VALUES ($1,$2,$3,'user','Original request','complete',0)")
        .bind(Uuid::new_v4()).bind(org.id).bind(session_id).execute(&app.db).await.unwrap();
    sqlx::query("UPDATE chat_sessions SET message_count=1, provider_state=$2 WHERE id=$1")
        .bind(session_id).bind(json!({ namespace: { "sessionId": "native-do-not-copy", "settings": { "effort": "high" } } }))
        .execute(&app.db).await.unwrap();
    let forked = client
        .post_json(&format!("{path}/fork"), &json!({ "fromMessageIndex": 0 }))
        .await;
    let status = forked.status();
    let forked: Value = forked.json().await.unwrap();
    assert_eq!(status, StatusCode::CREATED, "{forked}");
    let fork_id: Uuid = data(&forked)["sessionId"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let fork_state: Value =
        sqlx::query_scalar("SELECT provider_state FROM chat_sessions WHERE id=$1")
            .bind(fork_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(fork_state[namespace]["settings"]["permissionMode"], mode);
    assert_eq!(fork_state[namespace]["settings"]["effort"], "high");
    assert!(fork_state[namespace]["sessionId"].is_null());
    let fork_send = client
        .post_json(
            &format!("/api/v1/sessions/{fork_id}/messages"),
            &json!({ "content": "Continue fork", "permissionMode": mode }),
        )
        .await;
    let status = fork_send.status();
    let fork_send: Value = fork_send.json().await.unwrap();
    assert_eq!(status, StatusCode::CREATED, "{fork_send}");

    // A malformed legacy launch in another chat must not hide the healthy
    // chat or prevent a workspace carry. Sending that chat remains strict.
    let fork_task_id: Uuid = data(&fork_send)["taskId"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    sqlx::query("INSERT INTO task_attempts(org_id,task_id,attempt,vm_id,epoch,start_confirmed,started_at,policy_snapshot_ref)
        SELECT org_id,$2,1,vm_id,epoch,true,started_at,
            jsonb_set(policy_snapshot_ref, '{agent_launch}', '{\"broken\":true}')
        FROM task_attempts WHERE task_id=$1 AND attempt=1")
        .bind(launched.task_id).bind(fork_task_id).execute(&app.db).await.unwrap();
    let detail = client.get(&format!("/api/v1/sessions/{fork_id}")).await;
    assert_eq!(detail.status(), StatusCode::OK);
    let detail: Value = detail.json().await.unwrap();
    assert_eq!(
        data(&detail)["providerState"][namespace]["settings"]["permissionMode"],
        mode
    );
    let list = client
        .get(&format!("/api/v1/workspaces/{}/sessions", workspace.id))
        .await;
    assert_eq!(list.status(), StatusCode::OK);
    let list: Value = list.json().await.unwrap();
    assert_eq!(data(&list).as_array().unwrap().len(), 2);
    for item in data(&list).as_array().unwrap() {
        assert_eq!(
            item["providerState"][namespace]["settings"]["permissionMode"],
            mode
        );
    }
    let bad_send = client
        .post_json(
            &format!("/api/v1/sessions/{fork_id}/messages"),
            &json!({ "content": "Must validate the launch", "permissionMode": mode }),
        )
        .await;
    assert_eq!(bad_send.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let bad_send: Value = bad_send.json().await.unwrap();
    assert_eq!(
        bad_send["error"]["details"]["code"],
        "invalid_persisted_agent_launch"
    );
    let target = TestWorkspace::create(&app, &org).await;
    let mut conn = app.db.acquire().await.unwrap();
    assert_eq!(
        ChatService::copy_sessions_to_workspace(
            &mut conn,
            workspace.id,
            target.id,
            &[session_id, fork_id],
            owner.id
        )
        .await
        .unwrap(),
        2
    );
    drop(conn);
    let carried: Vec<Value> =
        sqlx::query_scalar("SELECT provider_state FROM chat_sessions WHERE workspace_id=$1")
            .bind(target.id)
            .fetch_all(&app.db)
            .await
            .unwrap();
    assert_eq!(carried.len(), 2);
    for state in carried {
        assert_eq!(state[namespace]["settings"]["permissionMode"], mode);
    }
    app.cleanup().await;
}

#[tokio::test]
async fn saved_legacy_claude_auto_is_displayed_and_sent_as_bypass() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let client = app.authenticated_client(&owner);
    let created: Value = client
        .post_json(
            &format!("/api/v1/workspaces/{}/sessions", workspace.id),
            &json!({ "title": "Legacy auto" }),
        )
        .await
        .json()
        .await
        .unwrap();
    let session_id: Uuid = data(&created)["id"].as_str().unwrap().parse().unwrap();
    sqlx::query("UPDATE chat_sessions SET provider_state=$2 WHERE id=$1")
        .bind(session_id)
        .bind(json!({ "claudeCode": { "settings": { "permissionMode": "auto" } } }))
        .execute(&app.db)
        .await
        .unwrap();
    let detail: Value = client
        .get(&format!("/api/v1/sessions/{session_id}"))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(
        data(&detail)["providerState"]["claudeCode"]["settings"]["permissionMode"],
        "bypassPermissions"
    );
    let sent = client
        .post_json(
            &format!("/api/v1/sessions/{session_id}/messages"),
            &json!({ "content": "Continue", "permissionMode": "bypassPermissions" }),
        )
        .await;
    let status = sent.status();
    let body: Value = sent.json().await.unwrap();
    assert_eq!(status, StatusCode::CREATED, "{body}");
    app.cleanup().await;
}

#[tokio::test]
async fn competing_first_sends_cannot_freeze_different_modes() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let client = app.authenticated_client(&owner);
    let created: Value = client
        .post_json(
            &format!("/api/v1/workspaces/{}/sessions", workspace.id),
            &json!({ "title": "Competing first sends" }),
        )
        .await
        .json()
        .await
        .unwrap();
    let session_id: Uuid = data(&created)["id"].as_str().unwrap().parse().unwrap();
    let path = format!("/api/v1/sessions/{session_id}/messages");
    let ask = json!({ "content": "Ask first", "permissionMode": "default" });
    let bypass = json!({ "content": "Bypass first", "permissionMode": "bypassPermissions" });
    let (ask, bypass) = tokio::join!(
        client.post_json(&path, &ask),
        client.post_json(&path, &bypass)
    );
    let winner = match (ask.status(), bypass.status()) {
        (StatusCode::CREATED, StatusCode::UNPROCESSABLE_ENTITY) => "default",
        (StatusCode::UNPROCESSABLE_ENTITY, StatusCode::CREATED) => "bypassPermissions",
        statuses => panic!("exactly one mode must win: {statuses:?}"),
    };
    let state: Value = sqlx::query_scalar("SELECT provider_state FROM chat_sessions WHERE id=$1")
        .bind(session_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(state["claudeCode"]["settings"]["permissionMode"], winner);
    let inputs: i64 = sqlx::query_scalar("SELECT count(*) FROM session_inputs WHERE session_id=$1")
        .bind(session_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(inputs, 1, "a rejected send must not leave an input");
    app.cleanup().await;
}
