//! Persona chat starts and "follow view" forks (workbench
//! `g5.workbench.persona`, `g5.workbench.actions`).
//!
//! `POST /workspaces/:id/sessions/start` with `persona`, `actionId`,
//! `workbenchContext` and `configRevision` records the persona and its frozen
//! instructions on the session and queues one first input: instructions,
//! template, context, user text. The workbench fields join the request
//! identity, the configuration is resolved only after the request key is
//! claimed and against the pinned revision, and the expanded content stays
//! under the frozen-prompt cap. A withdrawn first input is bootstrapped at
//! delivery; a fork inherits the persona; a follow-view fork commits the new
//! persona with its first input.
//!
//! Each test spawns a full `TestApp` with an isolated Postgres database and
//! drives the real HTTP endpoints. No mocks.

use crate::helpers;

use g5_api::services::chat_service::ChatService;
use helpers::{TestApp, TestOrg, TestUser, TestWorkspace};
use reqwest::StatusCode;
use serde_json::{json, Value};
use uuid::Uuid;

const SE_OVERRIDE: &str = "se:\n  - name: Design\n    actions:\n      - id: create-implementation-plan\n        label: Plan it\n";

async fn fixture() -> (TestApp, TestUser, TestOrg, TestWorkspace) {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    (app, owner, org, workspace)
}

async fn workbench(client: &helpers::AuthenticatedClient, workspace_id: Uuid) -> Value {
    let response = client
        .get(&format!("/api/v1/workspaces/{workspace_id}/workbench"))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    response.json::<Value>().await.expect("workbench JSON")["data"].clone()
}

async fn start(
    client: &helpers::AuthenticatedClient,
    workspace_id: Uuid,
    body: Value,
) -> (StatusCode, Value) {
    let response = client
        .post_json(
            &format!("/api/v1/workspaces/{workspace_id}/sessions/start"),
            &body,
        )
        .await;
    let status = response.status();
    (status, response.json().await.unwrap_or(Value::Null))
}

fn session_id(body: &Value) -> Uuid {
    Uuid::parse_str(body["data"]["session"]["id"].as_str().expect("session id")).unwrap()
}

async fn session_persona(app: &TestApp, session_id: Uuid) -> (Option<String>, Option<String>) {
    sqlx::query_as::<_, (Option<String>, Option<String>)>(
        "SELECT persona, persona_instructions FROM chat_sessions WHERE id = $1",
    )
    .bind(session_id)
    .fetch_one(&app.db)
    .await
    .expect("session persona")
}

/// `(content, delivery)` of the session's inputs, oldest first.
async fn inputs(app: &TestApp, session_id: Uuid) -> Vec<(String, String)> {
    sqlx::query_as::<_, (String, String)>(
        "SELECT content, delivery FROM session_inputs WHERE session_id = $1 ORDER BY input_index",
    )
    .bind(session_id)
    .fetch_all(&app.db)
    .await
    .expect("session inputs")
}

async fn seed_transcript(
    app: &TestApp,
    org_id: Uuid,
    session_id: Uuid,
    rows: &[(&str, &str, &str)],
) {
    for (index, (role, content, status)) in rows.iter().enumerate() {
        sqlx::query(
            "INSERT INTO chat_messages (id, org_id, session_id, role, content, status, message_index)
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(Uuid::new_v4())
        .bind(org_id)
        .bind(session_id)
        .bind(role)
        .bind(content)
        .bind(status)
        .bind(index as i32)
        .execute(&app.db)
        .await
        .expect("seed transcript row");
    }
    sqlx::query("UPDATE chat_sessions SET message_count = $2 WHERE id = $1")
        .bind(session_id)
        .bind(rows.len() as i32)
        .execute(&app.db)
        .await
        .expect("update message count");
}

#[tokio::test]
async fn a_persona_start_freezes_the_persona_and_prefixes_the_template() {
    let (app, owner, _org, workspace) = fixture().await;
    let client = app.authenticated_client(&owner);
    let config = workbench(&client, workspace.id).await;
    let revision = config["revision"].as_str().unwrap().to_string();
    let be_text = config["personas"]["be"]["text"]
        .as_str()
        .unwrap()
        .to_string();

    let (status, body) = start(
        &client,
        workspace.id,
        json!({
            "content": "Focus on the onboarding screens.",
            "persona": "be",
            "phase": "be",
            "actionId": "generate-mock",
            "workbenchContext": { "kind": "phase" },
            "configRevision": revision,
            "requestKey": "generate-mock-1",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["data"]["session"]["persona"], json!("be"));
    let session = session_id(&body);
    let (persona, instructions) = session_persona(&app, session).await;
    assert_eq!(persona.as_deref(), Some("be"));
    assert_eq!(instructions.as_deref(), Some(be_text.as_str()));

    let queued = inputs(&app, session).await;
    assert_eq!(queued.len(), 1);
    let content = &queued[0].0;
    assert!(
        content.starts_with(&be_text),
        "first message starts with the persona"
    );
    assert!(content.contains("Build a clickable mock"), "{content}");
    assert!(content.contains("Context: the workspace is in the Business Engineering phase."));
    assert!(content.ends_with("Focus on the onboarding screens."));
    // The session detail read reports the persona too.
    let detail = client
        .get(&format!("/api/v1/sessions/{session}"))
        .await
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(detail["data"]["persona"], json!("be"));

    app.cleanup().await;
}

#[tokio::test]
async fn an_identical_retry_replays_the_session_without_rereading_configuration() {
    let (app, owner, _org, mut workspace) = fixture().await;
    let client = app.authenticated_client(&owner);
    let config = workbench(&client, workspace.id).await;
    let revision = config["revision"].as_str().unwrap().to_string();
    let request = json!({
        "content": "",
        "persona": "se",
        "phase": "se",
        "actionId": "create-implementation-plan",
        "configRevision": revision,
        "requestKey": "plan-retry",
    });

    let (status, first) = start(&client, workspace.id, request.clone()).await;
    assert_eq!(status, StatusCode::CREATED, "{first}");
    let first_id = session_id(&first);
    let first_inputs = inputs(&app, first_id).await;
    assert!(first_inputs[0]
        .0
        .contains("Create the implementation plan for this workspace's change."));

    // The committed configuration changes (and with it the current
    // revision); the retry with the same key and body still replays the
    // original session and never re-reads the template.
    workspace
        .commit_file(&app, ".g5/workbench/actions.yaml", SE_OVERRIDE)
        .await;
    let current = workbench(&client, workspace.id).await;
    assert_ne!(current["revision"], json!(revision));

    let (status, retry) = start(&client, workspace.id, request.clone()).await;
    assert_eq!(status, StatusCode::OK, "{retry}");
    assert_eq!(session_id(&retry), first_id);
    assert_eq!(inputs(&app, first_id).await, first_inputs);
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM chat_sessions WHERE workspace_id = $1")
            .bind(workspace.id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(count, 1);

    // The same key with a different persona is a different message.
    let mut other = request.clone();
    other["persona"] = json!("be");
    let (status, body) = start(&client, workspace.id, other).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(body
        .to_string()
        .contains("already used for a different message"));

    app.cleanup().await;
}

#[tokio::test]
async fn a_stale_revision_is_refused_before_any_session_exists() {
    let (app, owner, _org, workspace) = fixture().await;
    let client = app.authenticated_client(&owner);
    let config = workbench(&client, workspace.id).await;
    let revision = config["revision"].as_str().unwrap().to_string();

    let (status, body) = start(
        &client,
        workspace.id,
        json!({
            "content": "",
            "persona": "be",
            "phase": "be",
            "actionId": "generate-mock",
            "configRevision": "deadbeef.000000000000",
            "requestKey": "stale-start",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(
        body["error"]["details"]["code"],
        json!("WORKBENCH_CONFIG_STALE")
    );
    assert_eq!(body["error"]["details"]["currentRevision"], json!(revision));
    let sessions: i64 =
        sqlx::query_scalar("SELECT count(*) FROM chat_sessions WHERE workspace_id = $1")
            .bind(workspace.id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(sessions, 0, "no session exists after a refused start");
    // The claim rolled back with the request: the same key works with the
    // current revision.
    let (status, body) = start(
        &client,
        workspace.id,
        json!({
            "content": "",
            "persona": "be",
            "phase": "be",
            "actionId": "generate-mock",
            "configRevision": revision,
            "requestKey": "stale-start",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    // Persona without a revision, or an unknown persona/action, is refused.
    for (body, expected) in [
        (
            json!({ "content": "x", "persona": "be" }),
            "required together",
        ),
        (
            json!({ "content": "x", "persona": "qa", "configRevision": revision }),
            "one of be, se, ie",
        ),
        (
            json!({ "content": "x", "persona": "be", "phase": "be", "actionId": "nope", "configRevision": revision }),
            "unknown workbench action",
        ),
        (
            json!({ "content": "x", "persona": "be", "phase": "be", "actionId": "new-doc", "configRevision": revision }),
            "does not start a chat",
        ),
        (
            json!({ "content": "x", "persona": "be", "actionId": "generate-mock", "configRevision": revision }),
            "needs the phase",
        ),
        (
            json!({ "content": "x", "persona": "be", "phase": "qa", "actionId": "generate-mock", "configRevision": revision }),
            "phase must be one of",
        ),
        (
            json!({ "content": "x", "actionId": "generate-mock" }),
            "need persona",
        ),
    ] {
        let (status, response) = start(&client, workspace.id, body.clone()).await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{body}: {response}"
        );
        assert!(
            response.to_string().contains(expected),
            "{body}: {response}"
        );
    }

    app.cleanup().await;
}

#[tokio::test]
async fn oversized_expanded_content_is_rejected_with_the_prompt_limit_error() {
    let (app, owner, _org, workspace) = fixture().await;
    let client = app.authenticated_client(&owner);
    let config = workbench(&client, workspace.id).await;
    let revision = config["revision"].as_str().unwrap().to_string();

    // Just under the cap on its own; the persona prefix pushes it over.
    let typed = "x".repeat(512 * 1024 - 64);
    let (status, body) = start(
        &client,
        workspace.id,
        json!({
            "content": typed,
            "persona": "be",
            "phase": "be",
            "actionId": "generate-mock",
            "configRevision": revision,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{}", body["error"]["code"]);
    assert!(
        body.to_string().contains("exceeds the prompt byte limit"),
        "{body}"
    );

    app.cleanup().await;
}

#[tokio::test]
async fn a_withdrawn_first_input_is_bootstrapped_from_the_frozen_instructions() {
    let (app, owner, org, workspace) = fixture().await;
    let client = app.authenticated_client(&owner);
    let config = workbench(&client, workspace.id).await;
    let revision = config["revision"].as_str().unwrap().to_string();
    let be_text = config["personas"]["be"]["text"]
        .as_str()
        .unwrap()
        .to_string();

    let (status, body) = start(
        &client,
        workspace.id,
        json!({
            "content": "",
            "persona": "be",
            "phase": "be",
            "actionId": "import-user-stories",
            "configRevision": revision,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let session = session_id(&body);
    let input_id = body["data"]["message"]["inputId"]
        .as_str()
        .unwrap()
        .to_string();

    // Withdraw the prefixed first input before delivery.
    let response = client
        .post_json(
            &format!("/api/v1/sessions/{session}/inputs/{input_id}/withdraw"),
            &json!({ "expectedRevision": 1 }),
        )
        .await;
    let status = response.status();
    let withdrawn: Value = response.json().await.unwrap_or(Value::Null);
    assert_eq!(status, StatusCode::OK, "{withdrawn}");

    // The next message is an ordinary send without any prefix.
    let response = client
        .post_json(
            &format!("/api/v1/sessions/{session}/messages"),
            &json!({ "content": "Start with the login story." }),
        )
        .await;
    let status = response.status();
    let sent: Value = response.json().await.unwrap_or(Value::Null);
    assert!(
        matches!(
            status,
            StatusCode::OK | StatusCode::CREATED | StatusCode::ACCEPTED
        ),
        "{status}: {sent}"
    );
    let queued = inputs(&app, session).await;
    let plain = queued
        .iter()
        .find(|(_, delivery)| delivery == "queued")
        .expect("a queued input");
    assert_eq!(plain.0, "Start with the login story.");

    // Delivery: the dispatcher prefixes the first delivered input with the
    // frozen instructions and persists the bytes it reserved.
    attach_running_vm(&app, &workspace).await;
    let mut db = app.scoped_db_for_org(org.id).await;
    let dispatch = app
        .state
        .task_service
        .reserve_agent_dispatch(&mut db)
        .await
        .expect("reserve dispatch")
        .expect("a dispatch is available");
    db.commit().await.unwrap();
    assert_eq!(dispatch.workspace_id, workspace.id);
    let reserved = inputs(&app, session)
        .await
        .into_iter()
        .find(|(_, delivery)| delivery == "reserved")
        .expect("reserved input");
    assert!(
        reserved.0.starts_with(&be_text),
        "{}",
        &reserved.0[..80.min(reserved.0.len())]
    );
    assert!(reserved.0.ends_with("Start with the login story."));
    let (persona, _) = session_persona(&app, session).await;
    assert_eq!(persona.as_deref(), Some("be"));

    app.cleanup().await;
}

#[tokio::test]
async fn a_follow_view_fork_commits_the_persona_with_its_first_input() {
    let (app, owner, org, workspace) = fixture().await;
    let client = app.authenticated_client(&owner);
    let config = workbench(&client, workspace.id).await;
    let revision = config["revision"].as_str().unwrap().to_string();
    let se_text = config["personas"]["se"]["text"]
        .as_str()
        .unwrap()
        .to_string();

    // A BE session with a delivered exchange, then an active turn.
    let (status, body) = start(
        &client,
        workspace.id,
        json!({ "content": "", "persona": "be", "phase": "be", "actionId": "generate-mock", "configRevision": revision }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let source = session_id(&body);
    let be_prefix = inputs(&app, source).await[0].0.clone();
    seed_transcript(
        &app,
        org.id,
        source,
        &[
            ("user", &be_prefix, "complete"),
            ("assistant", "Here is the mock.", "complete"),
            ("user", "Now plan it.", "complete"),
            ("assistant", "", "streaming"),
        ],
    )
    .await;

    let response = client
        .post_json(
            &format!("/api/v1/sessions/{source}/fork"),
            &json!({
                "fromMessageIndex": 3,
                "persona": "se",
                "configRevision": revision,
                "requestKey": "follow-view-1",
            }),
        )
        .await;
    let status = response.status();
    let forked: Value = response.json().await.unwrap_or(Value::Null);
    assert_eq!(status, StatusCode::CREATED, "{forked}");
    let fork = Uuid::parse_str(forked["data"]["sessionId"].as_str().unwrap()).unwrap();
    // The boundary is the last completed assistant unit (index 1), not the
    // streaming one.
    assert_eq!(forked["data"]["messagesCopied"], json!(2));
    let copied: i64 =
        sqlx::query_scalar("SELECT count(*) FROM chat_messages WHERE session_id = $1")
            .bind(fork)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(copied, 2);
    let (persona, instructions) = session_persona(&app, fork).await;
    assert_eq!(persona.as_deref(), Some("se"));
    assert_eq!(instructions.as_deref(), Some(se_text.as_str()));
    let queued = inputs(&app, fork).await;
    assert_eq!(queued.len(), 1, "exactly one first input");
    assert!(queued[0].0.starts_with(&se_text));
    assert!(queued[0]
        .0
        .contains("taking over this conversation as the System Engineering persona"));
    let forked_from: Option<Uuid> =
        sqlx::query_scalar("SELECT forked_from_id FROM chat_sessions WHERE id = $1")
            .bind(fork)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(forked_from, Some(source));

    // A retry with the same key returns the same fork.
    let response = client
        .post_json(
            &format!("/api/v1/sessions/{source}/fork"),
            &json!({
                "fromMessageIndex": 3,
                "persona": "se",
                "configRevision": revision,
                "requestKey": "follow-view-1",
            }),
        )
        .await;
    let retry: Value = response.json().await.unwrap_or(Value::Null);
    assert_eq!(retry["data"]["sessionId"], forked["data"]["sessionId"]);

    // A stale pinned revision is refused before anything is created.
    let response = client
        .post_json(
            &format!("/api/v1/sessions/{source}/fork"),
            &json!({ "fromMessageIndex": 1, "persona": "ie", "configRevision": "stale.0" }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);

    app.cleanup().await;
}

#[tokio::test]
async fn an_ordinary_fork_inherits_the_persona_and_an_empty_source_starts_fresh() {
    let (app, owner, org, workspace) = fixture().await;
    let client = app.authenticated_client(&owner);
    let config = workbench(&client, workspace.id).await;
    let revision = config["revision"].as_str().unwrap().to_string();
    let ie_text = config["personas"]["ie"]["text"]
        .as_str()
        .unwrap()
        .to_string();

    let (status, body) = start(
        &client,
        workspace.id,
        json!({ "content": "", "persona": "be", "phase": "be", "actionId": "generate-mock", "configRevision": revision }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let source = session_id(&body);

    // Queued-only source: "follow view" starts a new session under the
    // persona instead of forking an empty transcript.
    let response = client
        .post_json(
            &format!("/api/v1/sessions/{source}/fork"),
            &json!({ "fromMessageIndex": 0, "persona": "ie" }),
        )
        .await;
    let status = response.status();
    let fresh: Value = response.json().await.unwrap_or(Value::Null);
    assert_eq!(status, StatusCode::CREATED, "{fresh}");
    let fresh_id = Uuid::parse_str(fresh["data"]["sessionId"].as_str().unwrap()).unwrap();
    assert_eq!(fresh["data"]["messagesCopied"], json!(0));
    let forked_from: Option<Uuid> =
        sqlx::query_scalar("SELECT forked_from_id FROM chat_sessions WHERE id = $1")
            .bind(fresh_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(forked_from, None);
    let (persona, _) = session_persona(&app, fresh_id).await;
    assert_eq!(persona.as_deref(), Some("ie"));
    let queued = inputs(&app, fresh_id).await;
    assert!(queued[0].0.starts_with(&ie_text));
    assert!(queued[0]
        .0
        .contains("Start this conversation as the Integration Engineering persona"));

    // Ordinary fork of a delivered BE transcript: persona inherited, no new
    // input, no second prefix.
    let be_prefix = inputs(&app, source).await[0].0.clone();
    seed_transcript(
        &app,
        org.id,
        source,
        &[
            ("user", &be_prefix, "complete"),
            ("assistant", "Done.", "complete"),
        ],
    )
    .await;
    let response = client
        .post_json(
            &format!("/api/v1/sessions/{source}/fork"),
            &json!({ "fromMessageIndex": 1 }),
        )
        .await;
    let status = response.status();
    let forked: Value = response.json().await.unwrap_or(Value::Null);
    assert_eq!(status, StatusCode::CREATED, "{forked}");
    let fork = Uuid::parse_str(forked["data"]["sessionId"].as_str().unwrap()).unwrap();
    let (persona, instructions) = session_persona(&app, fork).await;
    assert_eq!(persona.as_deref(), Some("be"));
    assert_eq!(
        instructions,
        session_persona(&app, source).await.1,
        "the frozen instructions are copied as they are"
    );
    assert!(inputs(&app, fork).await.is_empty(), "no input is injected");
    let prefixed_rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM chat_messages WHERE session_id = $1 AND role = 'user' AND content = $2",
    )
    .bind(fork)
    .bind(&be_prefix)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(
        prefixed_rows, 1,
        "the copied transcript carries the prefix once"
    );

    // Its next message is not prefixed again at delivery: the copied
    // transcript already carries the instructions.
    let response = client
        .post_json(
            &format!("/api/v1/sessions/{fork}/messages"),
            &json!({ "content": "Continue." }),
        )
        .await;
    assert!(response.status().is_success(), "{}", response.status());
    attach_running_vm(&app, &workspace).await;
    let mut db = app.scoped_db_for_org(org.id).await;
    let dispatch = app
        .state
        .task_service
        .reserve_agent_dispatch(&mut db)
        .await
        .expect("reserve dispatch")
        .expect("a dispatch is available");
    db.commit().await.unwrap();
    // Several sessions queue a turn; find the fork's reserved input.
    let _ = dispatch;
    let fork_inputs = inputs(&app, fork).await;
    assert_eq!(fork_inputs.len(), 1);
    assert_eq!(fork_inputs[0].0, "Continue.", "{:?}", fork_inputs[0]);

    app.cleanup().await;
}

#[tokio::test]
async fn the_clicked_phase_selects_the_template_whatever_the_persona_override() {
    let (app, owner, _org, mut workspace) = fixture().await;
    let client = app.authenticated_client(&owner);
    // BE's `import-user-stories` (the Import menu's item) is overridden to the SE persona with its own
    // prompt; SE keeps the built-in action of the same id.
    workspace
        .commit_file(
            &app,
            ".g5/workbench/actions.yaml",
            "be:\n  - name: Author\n    actions:\n      - id: import-user-stories\n        persona: se\n        prompt: BE PLAN PROMPT ONLY\n",
        )
        .await;
    let config = workbench(&client, workspace.id).await;
    assert_eq!(config["config"]["status"], json!("committed"), "{config}");
    let revision = config["revision"].as_str().unwrap().to_string();
    let se_text = config["personas"]["se"]["text"]
        .as_str()
        .unwrap()
        .to_string();

    let (status, body) = start(
        &client,
        workspace.id,
        json!({ "content": "", "persona": "se", "phase": "be", "actionId": "import-user-stories", "configRevision": revision }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let be_click = inputs(&app, session_id(&body)).await[0].0.clone();
    assert!(be_click.starts_with(&se_text), "the SE persona is applied");
    assert!(be_click.contains("BE PLAN PROMPT ONLY"), "{be_click}");
    assert!(
        !be_click.contains("Import user stories from an uploaded backlog export"),
        "not SE's template"
    );

    let (status, body) = start(
        &client,
        workspace.id,
        json!({ "content": "", "persona": "se", "phase": "se", "actionId": "import-user-stories", "configRevision": revision }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let se_click = inputs(&app, session_id(&body)).await[0].0.clone();
    assert!(
        se_click.contains("Import user stories from an uploaded backlog export"),
        "{se_click}"
    );
    assert!(!se_click.contains("BE PLAN PROMPT ONLY"));

    // An id that exists in another phase only is unknown for the clicked one.
    let (status, body) = start(
        &client,
        workspace.id,
        json!({ "content": "", "persona": "ie", "phase": "ie", "actionId": "generate-code", "configRevision": revision }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(body.to_string().contains("in phase `ie`"));

    app.cleanup().await;
}

#[tokio::test]
async fn an_oversized_persona_input_fails_visibly_while_other_sessions_dispatch() {
    let (app, owner, org, workspace) = fixture().await;
    let client = app.authenticated_client(&owner);
    let config = workbench(&client, workspace.id).await;
    let revision = config["revision"].as_str().unwrap().to_string();

    // Enqueue and edit cap the expanded size up front.
    let (status, body) = start(
        &client,
        workspace.id,
        json!({ "content": "", "persona": "be", "phase": "be", "actionId": "generate-mock", "configRevision": revision }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let oversized = session_id(&body);
    let input_id = body["data"]["message"]["inputId"]
        .as_str()
        .unwrap()
        .to_string();
    let response = client
        .patch_json(
            &format!("/api/v1/sessions/{oversized}/inputs/{input_id}"),
            &json!({ "expectedRevision": 1, "content": "x".repeat(512 * 1024 - 64) }),
        )
        .await;
    let status = response.status();
    let refused: Value = response.json().await.unwrap_or(Value::Null);
    assert_eq!(status, StatusCode::CONFLICT, "{refused}");
    assert!(
        refused
            .to_string()
            .contains("exceeds the prompt byte limit"),
        "{refused}"
    );
    let response = client
        .post_json(
            &format!("/api/v1/sessions/{oversized}/messages"),
            &json!({ "content": "y".repeat(512 * 1024 - 64) }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);

    // A row that slipped past them (seeded directly) must not stall the pass.
    sqlx::query("UPDATE session_inputs SET content = $2 WHERE id = $1::uuid")
        .bind(&input_id)
        .bind("z".repeat(512 * 1024 - 64))
        .execute(&app.db)
        .await
        .unwrap();
    let (status, body) = start(
        &client,
        workspace.id,
        json!({ "content": "", "persona": "se", "phase": "se", "actionId": "create-implementation-plan", "configRevision": revision }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let healthy = session_id(&body);

    attach_running_vm(&app, &workspace).await;
    for _ in 0..3 {
        let mut db = app.scoped_db_for_org(org.id).await;
        let _ = app
            .state
            .task_service
            .reserve_agent_dispatch(&mut db)
            .await
            .expect("the pass never errors on an oversized input");
        db.commit().await.unwrap();
    }
    let (status, outcome): (String, Option<Value>) = sqlx::query_as(
        "SELECT task.status, task.outcome FROM tasks AS task
         JOIN session_inputs AS input ON input.task_id = task.id
         WHERE input.id = $1::uuid",
    )
    .bind(&input_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(status, "failed");
    let outcome = outcome.unwrap_or(Value::Null);
    assert_eq!(outcome["reason"], json!("dispatch_admission_failed"));
    assert!(
        outcome["detail"]
            .as_str()
            .unwrap()
            .contains("persona instructions"),
        "{outcome}"
    );
    let healthy_inputs = inputs(&app, healthy).await;
    assert_eq!(
        healthy_inputs[0].1, "reserved",
        "the other session dispatched: {healthy_inputs:?}"
    );

    app.cleanup().await;
}

#[tokio::test]
async fn an_edited_first_input_is_delivered_with_the_prefix_exactly_once() {
    let (app, owner, org, workspace) = fixture().await;
    let client = app.authenticated_client(&owner);
    let config = workbench(&client, workspace.id).await;
    let revision = config["revision"].as_str().unwrap().to_string();
    let be_text = config["personas"]["be"]["text"]
        .as_str()
        .unwrap()
        .to_string();

    let (status, body) = start(
        &client,
        workspace.id,
        json!({ "content": "", "persona": "be", "phase": "be", "actionId": "generate-mock", "configRevision": revision }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let session = session_id(&body);
    let input_id = body["data"]["message"]["inputId"]
        .as_str()
        .unwrap()
        .to_string();
    let response = client
        .patch_json(
            &format!("/api/v1/sessions/{session}/inputs/{input_id}"),
            &json!({ "expectedRevision": 1, "content": "Only the dashboard, please." }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);

    attach_running_vm(&app, &workspace).await;
    let mut db = app.scoped_db_for_org(org.id).await;
    app.state
        .task_service
        .reserve_agent_dispatch(&mut db)
        .await
        .expect("reserve")
        .expect("a dispatch");
    db.commit().await.unwrap();
    let reserved = inputs(&app, session).await;
    assert_eq!(reserved.len(), 1);
    assert_eq!(reserved[0].1, "reserved");
    assert_eq!(
        reserved[0].0,
        format!("{be_text}\n\nOnly the dashboard, please."),
        "prefixed exactly once"
    );
    assert_eq!(reserved[0].0.matches(&be_text).count(), 1);

    app.cleanup().await;
}

#[tokio::test]
async fn a_carried_session_keeps_its_persona_without_reinjection() {
    let (app, owner, org, workspace) = fixture().await;
    let client = app.authenticated_client(&owner);
    let config = workbench(&client, workspace.id).await;
    let revision = config["revision"].as_str().unwrap().to_string();

    let (status, body) = start(
        &client,
        workspace.id,
        json!({ "content": "", "persona": "ie", "phase": "ie", "actionId": "merge", "configRevision": revision }),
    )
    .await;
    // `merge` is an open action: a bare persona start with text instead.
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    let (status, body) = start(
        &client,
        workspace.id,
        json!({ "content": "Ship it.", "persona": "ie", "phase": "ie", "configRevision": revision }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let source = session_id(&body);
    let prefix = inputs(&app, source).await[0].0.clone();
    seed_transcript(
        &app,
        org.id,
        source,
        &[
            ("user", &prefix, "complete"),
            ("assistant", "Done.", "complete"),
        ],
    )
    .await;

    // The production fork copy boundary (`workspace_fork_service` carry).
    let target = TestWorkspace::create(&app, &org).await;
    let mut connection = app.db.acquire().await.unwrap();
    let carried = ChatService::copy_sessions_to_workspace(
        &mut connection,
        workspace.id,
        target.id,
        &[source],
        owner.id,
    )
    .await
    .expect("carry");
    assert_eq!(carried, 1);
    drop(connection);
    let (carried_id, persona, instructions): (Uuid, Option<String>, Option<String>) = sqlx::query_as(
        "SELECT id, persona, persona_instructions FROM chat_sessions WHERE workspace_id = $1 AND forked_from_id = $2",
    )
    .bind(target.id)
    .bind(source)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(persona.as_deref(), Some("ie"));
    assert_eq!(instructions, session_persona(&app, source).await.1);
    assert!(
        inputs(&app, carried_id).await.is_empty(),
        "no input is injected by the carry"
    );
    let user_rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM chat_messages WHERE session_id = $1 AND role = 'user' AND content = $2",
    )
    .bind(carried_id)
    .bind(&prefix)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(
        user_rows, 1,
        "the copied transcript carries the prefix once"
    );

    app.cleanup().await;
}

#[tokio::test]
async fn a_follow_view_retry_replays_after_the_source_turn_completes() {
    let (app, owner, org, workspace) = fixture().await;
    let client = app.authenticated_client(&owner);
    let config = workbench(&client, workspace.id).await;
    let revision = config["revision"].as_str().unwrap().to_string();

    let (status, body) = start(
        &client,
        workspace.id,
        json!({ "content": "", "persona": "be", "phase": "be", "actionId": "generate-mock", "configRevision": revision }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let source = session_id(&body);
    let prefix = inputs(&app, source).await[0].0.clone();
    seed_transcript(
        &app,
        org.id,
        source,
        &[
            ("user", &prefix, "complete"),
            ("assistant", "Mock done.", "complete"),
            ("user", "Plan it.", "complete"),
            ("assistant", "", "streaming"),
        ],
    )
    .await;
    let request = json!({
        "fromMessageIndex": 3,
        "persona": "se",
        "configRevision": revision,
        "requestKey": "follow-retry",
    });
    let first: Value = client
        .post_json(&format!("/api/v1/sessions/{source}/fork"), &request)
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(first["data"]["messagesCopied"], json!(2), "{first}");

    // The source's active turn completes between the first call and the
    // retry: the identity hashes the submitted fields only, so the retry
    // replays the frozen decision instead of recomputing (or refusing).
    sqlx::query("UPDATE chat_messages SET status = 'complete', content = 'Planned.' WHERE session_id = $1 AND message_index = 3")
        .bind(source)
        .execute(&app.db)
        .await
        .unwrap();
    let retry: Value = client
        .post_json(&format!("/api/v1/sessions/{source}/fork"), &request)
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(retry["data"]["sessionId"], first["data"]["sessionId"]);
    assert_eq!(retry["data"]["messagesCopied"], json!(2));
    let forks: i64 =
        sqlx::query_scalar("SELECT count(*) FROM chat_sessions WHERE forked_from_id = $1")
            .bind(source)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(forks, 1);

    // Two different empty sources never share an identity under one key.
    let (_, a) = start(
        &client,
        workspace.id,
        json!({ "content": "A", "persona": "be", "phase": "be", "configRevision": revision }),
    )
    .await;
    let (_, b) = start(
        &client,
        workspace.id,
        json!({ "content": "B", "persona": "be", "phase": "be", "configRevision": revision }),
    )
    .await;
    let empty = json!({ "fromMessageIndex": 0, "persona": "ie", "requestKey": "empty-key" });
    let first = client
        .post_json(&format!("/api/v1/sessions/{}/fork", session_id(&a)), &empty)
        .await;
    assert_eq!(first.status(), StatusCode::CREATED);
    let second = client
        .post_json(&format!("/api/v1/sessions/{}/fork", session_id(&b)), &empty)
        .await;
    assert_eq!(second.status(), StatusCode::UNPROCESSABLE_ENTITY);

    app.cleanup().await;
}

/// A running VM lease so `reserve_agent_dispatch` can pick the workspace's
/// queued turn (copied from `api_task_lifecycle.rs`).
async fn attach_running_vm(app: &TestApp, workspace: &TestWorkspace) {
    let host_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO worker_hosts(host_id, org_id, region, status, last_heartbeat, slot_size_reported)
         VALUES ($1,$2,'us-central1','ready',now(),true)",
    )
    .bind(host_id)
    .bind(workspace.org_id)
    .execute(&app.db)
    .await
    .expect("insert dispatch test host");
    sqlx::query(
        "INSERT INTO workspace_vms(
             org_id, workspace_id, host_id, vm_id, epoch, status, lease_expires_at
         ) VALUES ($1,$2,$3,$4,1,'running',now() + interval '1 hour')",
    )
    .bind(workspace.org_id)
    .bind(workspace.id)
    .bind(host_id)
    .bind(Uuid::new_v4().to_string())
    .execute(&app.db)
    .await
    .expect("insert running dispatch test VM");
    app.attach_dispatch_host(host_id).await;
}
