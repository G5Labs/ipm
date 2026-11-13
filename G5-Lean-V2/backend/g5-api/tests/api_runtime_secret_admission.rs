//! Runtime Config secret admission (`g5.chat.secret-admission`,
//! `g5.terminal.secret-admission`; issue #695).
//!
//! One exportable-environment policy governs the configuration writer, agent
//! dispatch admission and the owner terminal. A stored invalid configuration
//! (seeded past the writer, as a pre-policy row would be) settles its own
//! turn visibly without stalling a later independent session, reconciles an
//! already reserved attempt with a preflight error instead of ending the
//! pass, and refuses a terminal with a structured reason; a configuration
//! at every limit opens and dispatches.
//!
//! Runs against live Postgres like every file in this directory:
//! `cargo nextest run -p g5-api --test integration -E 'test(/^api_runtime_secret_admission::/)'`.

use std::collections::HashMap;
use std::time::Duration;

use crate::helpers;
use futures_util::StreamExt;
use g5_contracts::terminal::TERMINAL_OPEN_FRAME_MAX_BYTES;
use g5_contracts::types::task_env::{MAX_TASK_ENV_SECRETS, MAX_TASK_ENV_VALUE_BYTES};
use helpers::{TestApp, TestOrg, TestUser, TestWorkspace, TestWorkspaceRuntimePeer};
use reqwest::StatusCode;
use serde_json::{json, Value};
use tokio_tungstenite::connect_async;
use uuid::Uuid;

fn data(body: &Value) -> &Value {
    &body["data"]
}

fn uuid(value: &Value) -> Uuid {
    Uuid::parse_str(value.as_str().expect("uuid string")).expect("uuid")
}

/// Store a secret map for a workspace exactly as an older writer would have,
/// bypassing the policy.
async fn seed_workspace_secrets(
    app: &TestApp,
    org_id: Uuid,
    workspace_id: Uuid,
    owner_id: Uuid,
    secrets: &[(&str, &str)],
) {
    let manager = app
        .state
        .scm_token_manager
        .as_ref()
        .expect("test app configures the secret encryption key");
    let map: HashMap<&str, &str> = secrets.iter().copied().collect();
    let encrypted = manager
        .encrypt_token(&serde_json::to_string(&map).unwrap())
        .unwrap();
    let mut keys: Vec<String> = map.keys().map(|key| key.to_string()).collect();
    keys.sort();
    sqlx::query(
        "INSERT INTO workspace_configs \
             (workspace_id, org_id, settings_json, secrets_enc, secret_keys, updated_by, updated_at) \
         VALUES ($1, $2, '{}'::jsonb, $3, $4, $5, now()) \
         ON CONFLICT (workspace_id) DO UPDATE SET \
             secrets_enc = EXCLUDED.secrets_enc, secret_keys = EXCLUDED.secret_keys, \
             updated_at = now()",
    )
    .bind(workspace_id)
    .bind(org_id)
    .bind(encrypted)
    .bind(&keys)
    .bind(owner_id)
    .execute(&app.db)
    .await
    .expect("seed stored workspace secrets");
}

async fn set_org_api_key(app: &TestApp, owner: &TestUser, org_id: Uuid) {
    let response = app
        .authenticated_client(owner)
        .put_json(
            &format!("/api/v1/orgs/{org_id}/config"),
            &json!({"setSecrets":{"ANTHROPIC_API_KEY":"sk-ant-ci-test-key"}}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
}

async fn open_chat(app: &TestApp, owner: &TestUser, workspace_id: Uuid, title: &str) -> Uuid {
    let created = app
        .authenticated_client(owner)
        .post_json(
            &format!("/api/v1/workspaces/{workspace_id}/sessions"),
            &json!({"sessionType":"chat","title":title,"agentProvider":"claude_code"}),
        )
        .await;
    let status = created.status();
    let created: Value = created.json().await.expect("session response");
    assert_eq!(status, StatusCode::CREATED, "{created}");
    uuid(&data(&created)["id"])
}

/// Sends a message: the input id and its turn's task id.
async fn send(app: &TestApp, owner: &TestUser, session_id: Uuid, content: &str) -> (Uuid, Uuid) {
    let sent = app
        .authenticated_client(owner)
        .post_json(
            &format!("/api/v1/sessions/{session_id}/messages"),
            &json!({ "content": content }),
        )
        .await;
    let status = sent.status();
    let sent: Value = sent.json().await.expect("send response");
    assert_eq!(status, StatusCode::CREATED, "{sent}");
    (
        uuid(&data(&sent)["messageId"]),
        uuid(&data(&sent)["taskId"]),
    )
}

async fn dispatch_pass(app: &TestApp) {
    g5_api_core::maintenance::agent_tasks::run_once(&app.state)
        .await
        .expect("run the dispatch pass");
}

/// An unconfirmed attempt is retried after a short backoff; skip it.
async fn backdate(app: &TestApp, task_id: Uuid) {
    sqlx::query("UPDATE tasks SET updated_at = now() - interval '3 seconds' WHERE id = $1")
        .bind(task_id)
        .execute(&app.db)
        .await
        .expect("backdate the attempt");
}

async fn task_state(app: &TestApp, task_id: Uuid) -> (String, bool, Option<Value>) {
    sqlx::query_as("SELECT status, start_confirmed, outcome FROM tasks WHERE id = $1")
        .bind(task_id)
        .fetch_one(&app.db)
        .await
        .expect("load task")
}

async fn assistant_error_notices(app: &TestApp, session_id: Uuid) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT content FROM chat_messages \
         WHERE session_id = $1 AND role = 'assistant' AND status = 'error' \
         ORDER BY message_index",
    )
    .bind(session_id)
    .fetch_all(&app.db)
    .await
    .expect("load notices")
}

async fn invokes(peer: &TestWorkspaceRuntimePeer) -> usize {
    peer.received_rpc_methods()
        .await
        .iter()
        .filter(|method| method.starts_with("modules.invoke:agent.turn"))
        .count()
}

async fn terminal_ticket(app: &TestApp, owner: &TestUser, workspace_id: Uuid) -> String {
    let response = app
        .authenticated_client(owner)
        .post_json(
            "/api/v1/auth/ws-ticket",
            &json!({"workspaceId": workspace_id}),
        )
        .await;
    let status = response.status();
    let body: Value = response.json().await.expect("ticket response");
    assert_eq!(status, StatusCode::OK, "{body}");
    body["data"]["ticket"]
        .as_str()
        .expect("ticket string")
        .to_string()
}

/// Every scope's writer refuses a policy violation with the sentence that
/// names the key and the limit, accepts the boundaries, and always lets a
/// stored pre-policy key be removed.
#[tokio::test]
async fn config_writes_refuse_policy_violations_with_the_reason() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let client = app.authenticated_client(&owner);
    let paths = [
        format!("/api/v1/orgs/{}/config", org.id),
        format!(
            "/api/v1/workspaces/{}/config?orgId={}",
            workspace.id, org.id
        ),
        format!("/api/v1/users/me/config?orgId={}", org.id),
    ];

    let over_value = "x".repeat(MAX_TASK_ENV_VALUE_BYTES + 1);
    let too_long = "A".repeat(129);
    let refusals: Vec<(Value, &str, &str)> = vec![
        (
            json!({"lower":"v"}),
            "secret_name_invalid",
            "`lower` is not an environment name",
        ),
        (
            json!({"HOME":"/x"}),
            "secret_name_reserved",
            "`HOME` is reserved",
        ),
        (
            json!({"GIT_AUTHOR_EMAIL":"a@b"}),
            "secret_name_reserved",
            "`GIT_AUTHOR_EMAIL` is reserved",
        ),
        (
            json!({"CLAUDE_CONFIG_DIR":"/x"}),
            "secret_name_reserved",
            "`CLAUDE_CONFIG_DIR` is reserved",
        ),
        (
            json!({"G5D_WORKSPACE_ID":"x"}),
            "secret_name_reserved",
            "`G5D_WORKSPACE_ID` is reserved",
        ),
        (
            json!({too_long.clone():"v"}),
            "secret_name_invalid",
            "is not an environment name",
        ),
        (
            json!({"BIG":over_value}),
            "secret_value_too_large",
            "the limit is 65536 bytes",
        ),
        (
            json!({"NUL":"a\u{0}b"}),
            "secret_value_nul",
            "contains a NUL character",
        ),
    ];
    for path in &paths {
        for (secrets, code, sentence) in &refusals {
            let response = client.put_json(path, &json!({"setSecrets": secrets})).await;
            let status = response.status();
            let text = response.text().await.expect("refusal body");
            let body: Value = serde_json::from_str(&text)
                .unwrap_or_else(|_| panic!("{path} {status} for {code}: {text}"));
            assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{path}: {body}");
            assert_eq!(body["error"]["details"]["code"], *code, "{path}: {body}");
            let message = body["error"]["message"].as_str().unwrap_or_default();
            assert!(message.contains(sentence), "{path}: {body}");
        }
    }

    // Boundaries pass: the longest name, the largest value, the most keys.
    let longest = "A".repeat(128);
    let mut full = serde_json::Map::new();
    for index in 0..MAX_TASK_ENV_SECRETS - 1 {
        full.insert(format!("K{index}"), json!("v"));
    }
    full.insert(longest.clone(), json!("x".repeat(MAX_TASK_ENV_VALUE_BYTES)));
    let response = client
        .put_json(&paths[1], &json!({"setSecrets": Value::Object(full)}))
        .await;
    let status = response.status();
    let body: Value = response.json().await.expect("boundary body");
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        data(&body)["secretKeys"].as_array().unwrap().len(),
        MAX_TASK_ENV_SECRETS
    );
    // One more is refused; the editor's message names the count limit.
    let response = client
        .put_json(&paths[1], &json!({"setSecrets": {"ONE_MORE":"v"}}))
        .await;
    let status = response.status();
    let body: Value = response.json().await.expect("over count body");
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["error"]["details"]["code"], "secrets_too_many");
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("limit is 128"),
        "{body}"
    );
    // Removing and adding in one write stays at the bound.
    let response = client
        .put_json(
            &paths[1],
            &json!({"setSecrets": {"ONE_MORE":"v"}, "removeSecretKeys": ["K0"]}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);

    // A stored pre-policy key does not block other writes and can be removed.
    seed_workspace_secrets(
        &app,
        org.id,
        workspace.id,
        owner.id,
        &[("lower", "old"), ("GOOD", "v")],
    )
    .await;
    let response = client
        .put_json(&paths[1], &json!({"setSecrets": {"OTHER":"v"}}))
        .await;
    let status = response.status();
    let body: Value = response.json().await.expect("write beside legacy key");
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data(&body)["secretKeys"], json!(["GOOD", "OTHER", "lower"]));
    let response = client
        .put_json(&paths[1], &json!({"removeSecretKeys": ["lower"]}))
        .await;
    let status = response.status();
    let body: Value = response.json().await.expect("remove legacy key");
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data(&body)["secretKeys"], json!(["GOOD", "OTHER"]));

    app.cleanup().await;
}

/// The organization's oldest candidate carries a stored invalid workspace
/// configuration. The pass settles it as failed with the reason, records the
/// reason in its chat, keeps its queued input, and still dispatches the later
/// independent session's turn. Once corrected, the owner's next message runs
/// the kept input.
#[tokio::test]
async fn a_stored_invalid_secret_settles_its_turn_visibly_and_later_sessions_dispatch() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    set_org_api_key(&app, &owner, org.id).await;
    let first = TestWorkspace::create(&app, &org).await;
    let second = TestWorkspace::create(&app, &org).await;
    let first_peer = TestWorkspaceRuntimePeer::attach(&app, &first, owner.id).await;
    let second_peer = TestWorkspaceRuntimePeer::attach(&app, &second, owner.id).await;

    let first_session = open_chat(&app, &owner, first.id, "Stored invalid config").await;
    let (first_input, first_task) = send(&app, &owner, first_session, "Oldest turn").await;
    seed_workspace_secrets(
        &app,
        org.id,
        first.id,
        owner.id,
        &[("lower", "x"), ("GOOD", "v")],
    )
    .await;
    let second_session = open_chat(&app, &owner, second.id, "Valid config").await;
    let (_, second_task) = send(&app, &owner, second_session, "Later turn").await;

    dispatch_pass(&app).await;

    let (status, _, outcome) = task_state(&app, first_task).await;
    assert_eq!(status, "failed");
    let outcome = outcome.expect("failed task outcome");
    assert_eq!(outcome["reason"], "dispatch_admission_failed", "{outcome}");
    assert!(
        outcome["detail"]
            .as_str()
            .unwrap()
            .contains("`lower` is not an environment name"),
        "{outcome}"
    );
    let notices = assistant_error_notices(&app, first_session).await;
    assert_eq!(notices.len(), 1, "{notices:?}");
    assert!(
        notices[0].contains("`lower` is not an environment name")
            && notices[0].contains("not sent to the agent"),
        "{notices:?}"
    );
    let (delivery, task_id): (String, Option<Uuid>) =
        sqlx::query_as("SELECT delivery, task_id FROM session_inputs WHERE id = $1")
            .bind(first_input)
            .fetch_one(&app.db)
            .await
            .expect("load kept input");
    assert_eq!((delivery.as_str(), task_id), ("queued", Some(first_task)));
    assert_eq!(
        invokes(&first_peer).await,
        0,
        "the invalid configuration never reached the guest"
    );

    let (status, confirmed, _) = task_state(&app, second_task).await;
    assert_eq!((status.as_str(), confirmed), ("running", true));
    assert_eq!(invokes(&second_peer).await, 1);

    // The second pass has nothing new for the first workspace: no fresh
    // failed task is minted for the kept input.
    dispatch_pass(&app).await;
    let failed_tasks: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM tasks WHERE chat_session_id = $1 AND status = 'failed'",
    )
    .bind(first_session)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(failed_tasks, 1);

    // Correct the configuration through the writer, then the next message
    // runs both inputs in one turn.
    let fixed = app
        .authenticated_client(&owner)
        .put_json(
            &format!("/api/v1/workspaces/{}/config?orgId={}", first.id, org.id),
            &json!({"removeSecretKeys": ["lower"]}),
        )
        .await;
    assert_eq!(fixed.status(), StatusCode::OK);
    let (_, next_task) = send(&app, &owner, first_session, "After the fix").await;
    assert_ne!(next_task, first_task);
    dispatch_pass(&app).await;
    let (status, confirmed, _) = task_state(&app, next_task).await;
    assert_eq!((status.as_str(), confirmed), ("running", true));
    let (delivery, task_id): (String, Option<Uuid>) =
        sqlx::query_as("SELECT delivery, task_id FROM session_inputs WHERE id = $1")
            .bind(first_input)
            .fetch_one(&app.db)
            .await
            .expect("load rebound input");
    assert_eq!(task_id, Some(next_task));
    assert_ne!(delivery, "queued");
    assert_eq!(invokes(&first_peer).await, 1);
    let params = first_peer.invoke_params().await;
    let secrets = params.last().expect("the fixed turn was invoked")["secrets"]
        .as_object()
        .expect("secrets object")
        .clone();
    assert_eq!(secrets.get("GOOD").and_then(Value::as_str), Some("v"));
    assert!(!secrets.contains_key("lower"));

    app.cleanup().await;
}

/// An attempt reserved under a valid configuration whose invoke was never
/// confirmed is rebuilt on the next pass. If the stored set turned invalid in
/// between, the rebuild withholds what no longer passes and carries a
/// preflight error instead of ending the tenant's pass; a fresh start then
/// records the reason in the chat.
#[tokio::test]
async fn an_unconfirmed_attempt_whose_secrets_turned_invalid_reconciles_with_a_preflight() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    set_org_api_key(&app, &owner, org.id).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let _peer = TestWorkspaceRuntimePeer::attach_with_invoke_failure(
        &app,
        &workspace,
        owner.id,
        "DISPATCH_TEST_INVOKE_LOST",
    )
    .await;
    let session = open_chat(&app, &owner, workspace.id, "Unconfirmed attempt").await;
    let (_, task) = send(&app, &owner, session, "Reserve me").await;
    dispatch_pass(&app).await;
    let (status, confirmed, _) = task_state(&app, task).await;
    assert_eq!((status.as_str(), confirmed), ("running", false));

    seed_workspace_secrets(
        &app,
        org.id,
        workspace.id,
        owner.id,
        &[("lower", "x"), ("GOOD", "v")],
    )
    .await;
    backdate(&app, task).await;
    let mut db = app.scoped_db_for_org(org.id).await;
    let dispatch = app
        .state
        .task_service
        .reserve_agent_dispatch(&mut db)
        .await
        .expect("rebuild the unconfirmed attempt")
        .expect("the unconfirmed attempt is rebuilt, not skipped");
    assert_eq!(dispatch.task_id, task);
    let detail = dispatch
        .secret_preflight_error
        .clone()
        .expect("the rebuild names what it withheld");
    assert!(
        detail.contains("`lower` is not an environment name"),
        "{detail}"
    );
    assert_eq!(
        dispatch.invoke_params["input"]["secret_preflight_error"],
        json!(detail)
    );
    let carried = dispatch.invoke_params["secrets"].as_object().unwrap();
    assert!(carried.contains_key("GOOD") && carried.contains_key("ANTHROPIC_API_KEY"));
    assert!(!carried.contains_key("lower"));
    app.state
        .task_service
        .record_secret_preflight_notice(&mut db, &dispatch)
        .await
        .expect("record the notice");
    db.commit().await.expect("commit");
    let notices = assistant_error_notices(&app, session).await;
    assert_eq!(notices.len(), 1, "{notices:?}");
    assert!(
        notices[0].contains("`lower` is not an environment name"),
        "{notices:?}"
    );

    // A corrected configuration rebuilds without a preflight.
    seed_workspace_secrets(&app, org.id, workspace.id, owner.id, &[("GOOD", "v")]).await;
    backdate(&app, task).await;
    let mut db = app.scoped_db_for_org(org.id).await;
    let dispatch = app
        .state
        .task_service
        .reserve_agent_dispatch(&mut db)
        .await
        .expect("rebuild again")
        .expect("still unconfirmed");
    assert_eq!(dispatch.secret_preflight_error, None);
    assert_eq!(
        dispatch.invoke_params["input"]["secret_preflight_error"],
        Value::Null
    );
    db.commit().await.expect("commit");

    app.cleanup().await;
}

/// The owner terminal: a stored invalid set is refused by the API with the
/// guest's control line before any PTY stream reaches the guest; once
/// corrected, a set at the limits travels in one open frame past the old
/// 64 KiB line and the guest's answer reaches the browser.
#[tokio::test]
async fn the_terminal_refuses_a_stored_invalid_set_with_its_reason_and_opens_a_valid_one() {
    use tokio_tungstenite::tungstenite::Message;
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let (_peer, mut streams) =
        TestWorkspaceRuntimePeer::attach_with_streams(&app, &workspace, owner.id).await;
    seed_workspace_secrets(
        &app,
        org.id,
        workspace.id,
        owner.id,
        &[("GIT_CONFIG_GLOBAL", "/x"), ("GOOD", "v")],
    )
    .await;

    let ticket = terminal_ticket(&app, &owner, workspace.id).await;
    let (mut browser, _) = connect_async(format!(
        "{}/api/v1/workspaces/{}/terminal?token={ticket}&cols=80&rows=24",
        app.ws_url, workspace.id
    ))
    .await
    .expect("the upgrade succeeds so the reason can be delivered");
    let first = tokio::time::timeout(Duration::from_secs(5), browser.next())
        .await
        .expect("control line")
        .expect("socket open")
        .expect("message");
    let control: Value =
        serde_json::from_slice(first.into_data().strip_suffix(b"\n").unwrap()).unwrap();
    assert_eq!(control["type"], "rejected", "{control}");
    assert_eq!(control["reason"], "secrets_invalid", "{control}");
    assert!(
        control["detail"]
            .as_str()
            .unwrap()
            .contains("`GIT_CONFIG_GLOBAL` is reserved"),
        "{control}"
    );
    let closed = tokio::time::timeout(Duration::from_secs(5), browser.next())
        .await
        .expect("close");
    assert!(
        matches!(closed, None | Some(Ok(Message::Close(_))) | Some(Err(_))),
        "{closed:?}"
    );
    // The guest's PTY stream, if it was opened at all, closes before any
    // open frame: the refusal never reaches the dispatcher as a request.
    if let Ok(Some(mut guest)) = tokio::time::timeout(Duration::from_secs(2), streams.recv()).await
    {
        // Mux incarnation header, if sent; bounded so a silent stream cannot
        // hang the test.
        let _ = tokio::time::timeout(Duration::from_secs(5), guest.recv())
            .await
            .expect("mux incarnation header or guest stream close");
        let line = tokio::time::timeout(
            Duration::from_secs(5),
            guest.read_line_with_limit(TERMINAL_OPEN_FRAME_MAX_BYTES),
        )
        .await
        .expect("the refused stream closes promptly");
        assert_eq!(
            line, None,
            "a refused terminal sends the guest no open frame"
        );
    }

    // Correct it, then a boundary set opens: three values at the value limit
    // make an open line far beyond 64 KiB.
    let big = "x".repeat(MAX_TASK_ENV_VALUE_BYTES);
    let fixed = app
        .authenticated_client(&owner)
        .put_json(
            &format!(
                "/api/v1/workspaces/{}/config?orgId={}",
                workspace.id, org.id
            ),
            &json!({
                "removeSecretKeys": ["GIT_CONFIG_GLOBAL"],
                "setSecrets": {"BIG_A": big, "BIG_B": big, "BIG_C": big, "MARK": "boundary"}
            }),
        )
        .await;
    let fixed_status = fixed.status();
    let fixed_text = fixed.text().await.unwrap_or_default();
    assert_eq!(fixed_status, StatusCode::OK, "{fixed_text}");
    let ticket = terminal_ticket(&app, &owner, workspace.id).await;
    let (mut browser, _) = connect_async(format!(
        "{}/api/v1/workspaces/{}/terminal?token={ticket}&cols=80&rows=24",
        app.ws_url, workspace.id
    ))
    .await
    .expect("open the owner terminal");
    let mut guest = tokio::time::timeout(Duration::from_secs(5), streams.recv())
        .await
        .expect("guest stream")
        .expect("guest stream open");
    guest.recv().await.expect("mux incarnation header");
    let open = guest
        .read_line_with_limit(TERMINAL_OPEN_FRAME_MAX_BYTES)
        .await
        .expect("open line");
    assert!(open.len() > 64 * 1024, "{} bytes", open.len());
    let open: Value = serde_json::from_str(&open).unwrap();
    assert_eq!(open["type"], "open");
    let secrets = open["secrets"].as_object().unwrap();
    // The fixture's organization key rides along; the refused key is gone.
    assert!(
        !secrets.contains_key("GIT_CONFIG_GLOBAL"),
        "{:?}",
        secrets.keys().collect::<Vec<_>>()
    );
    assert_eq!(secrets["MARK"], "boundary");
    assert_eq!(secrets["GOOD"], "v");
    assert_eq!(
        secrets["BIG_C"].as_str().unwrap().len(),
        MAX_TASK_ENV_VALUE_BYTES
    );
    guest
        .send(b"{\"type\":\"opened\",\"terminal_id\":\"t-1\",\"reattached\":false,\"offset\":0,\"reset\":false}\n")
        .await
        .unwrap();
    let answer = tokio::time::timeout(Duration::from_secs(5), browser.next())
        .await
        .expect("opened line")
        .expect("socket open")
        .expect("message");
    let control: Value =
        serde_json::from_slice(answer.into_data().strip_suffix(b"\n").unwrap()).unwrap();
    assert_eq!(control["type"], "opened", "{control}");
    browser.close(None).await.ok();

    app.cleanup().await;
}
