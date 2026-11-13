//! Chat turn activity, activity ordering and personal result inspection
//! (chat `g5.chat.activity`, `g5.chat.activity.turns`,
//! `g5.chat.activity.order`, `g5.chat.inspection`,
//! `g5.chat.inspection.acknowledge`, `g5.chat.activity.refresh`).
//!
//! Messages go through the chat routes and dispatch through the maintenance
//! pass to the test runtime peer; each attempt's events are applied with
//! `TaskService::apply`, as host control applies them.

use crate::helpers;

use std::time::Duration;

use helpers::{TestApp, TestOrg, TestUser, TestWorkspace, TestWorkspaceRuntimePeer};
use reqwest::StatusCode;
use serde_json::{json, Value};
use uuid::Uuid;

use g5_api::services::task_lifecycle::{canonical_exec_id, JournalTaskEvent, TaskLifecycleEvent};

fn data(body: &Value) -> &Value {
    body.get("data").unwrap_or(body)
}

fn uuid(value: &Value) -> Uuid {
    value
        .as_str()
        .and_then(|text| text.parse().ok())
        .unwrap_or_else(|| panic!("expected a uuid, got {value}"))
}

/// An owner's workspace on a running VM whose chat turns can dispatch.
struct Workspace {
    owner: TestUser,
    org: TestOrg,
    workspace_id: Uuid,
    _runtime: TestWorkspaceRuntimePeer,
}

impl Workspace {
    async fn open(app: &TestApp) -> Self {
        let mut owner = TestUser::create(app).await;
        let org = TestOrg::create_with_admin(app, &mut owner).await;
        let workspace = TestWorkspace::create(app, &org).await;
        let runtime = TestWorkspaceRuntimePeer::attach(app, &workspace, owner.id).await;
        let config = app
            .authenticated_client(&owner)
            .put_json(
                &format!("/api/v1/orgs/{}/config", org.id),
                &json!({"setSecrets":{"ANTHROPIC_API_KEY":"sk-ant-ci-test-key"}}),
            )
            .await;
        assert_eq!(config.status(), StatusCode::OK);
        Self {
            owner,
            org,
            workspace_id: workspace.id,
            _runtime: runtime,
        }
    }

    async fn chat(&self, app: &TestApp, title: &str) -> Uuid {
        let created = app
            .authenticated_client(&self.owner)
            .post_json(
                &format!("/api/v1/workspaces/{}/sessions", self.workspace_id),
                &json!({"sessionType":"chat","title":title,"agentProvider":"claude_code"}),
            )
            .await;
        let status = created.status();
        let created: Value = created.json().await.expect("session response");
        assert_eq!(status, StatusCode::CREATED, "{created}");
        uuid(&data(&created)["id"])
    }

    /// Sends a message; returns the message's turn.
    async fn send(&self, app: &TestApp, session_id: Uuid, content: &str) -> Uuid {
        let sent = app
            .authenticated_client(&self.owner)
            .post_json(
                &format!("/api/v1/sessions/{session_id}/messages"),
                &json!({ "content": content }),
            )
            .await;
        let status = sent.status();
        let sent: Value = sent.json().await.expect("send response");
        assert_eq!(status, StatusCode::CREATED, "{sent}");
        uuid(&data(&sent)["taskId"])
    }

    async fn list_as(&self, app: &TestApp, user: &TestUser, query: &str) -> Vec<Value> {
        let listed = app
            .authenticated_client(user)
            .get(&format!(
                "/api/v1/workspaces/{}/sessions{query}",
                self.workspace_id
            ))
            .await;
        let status = listed.status();
        let body: Value = listed.json().await.expect("list response");
        assert_eq!(status, StatusCode::OK, "{body}");
        data(&body).as_array().expect("session list").clone()
    }

    async fn list(&self, app: &TestApp) -> Vec<Value> {
        self.list_as(app, &self.owner, "").await
    }

    async fn order(&self, app: &TestApp) -> Vec<Uuid> {
        self.list(app)
            .await
            .iter()
            .map(|session| uuid(&session["id"]))
            .collect()
    }

    /// One chat's list entry as `user` sees it.
    async fn entry_as(&self, app: &TestApp, user: &TestUser, session_id: Uuid) -> Value {
        self.list_as(app, user, "")
            .await
            .into_iter()
            .find(|session| uuid(&session["id"]) == session_id)
            .expect("the chat is listed")
    }

    async fn inspect(
        &self,
        app: &TestApp,
        user: &TestUser,
        session_id: Uuid,
        task_id: Uuid,
        attempt: Option<i32>,
    ) -> (StatusCode, Value) {
        let response = app
            .authenticated_client(user)
            .put_json(
                &format!("/api/v1/sessions/{session_id}/inspection"),
                &json!({"taskId": task_id, "attempt": attempt}),
            )
            .await;
        let status = response.status();
        let body: Value = response.json().await.expect("inspection response");
        (status, body)
    }

    async fn rename(&self, app: &TestApp, session_id: Uuid, title: &str) {
        let renamed = app
            .authenticated_client(&self.owner)
            .patch_json(
                &format!("/api/v1/sessions/{session_id}"),
                &json!({ "title": title }),
            )
            .await;
        assert_eq!(renamed.status(), StatusCode::OK);
    }
}

async fn dispatch(app: &TestApp) {
    g5_api_core::maintenance::agent_tasks::run_once(&app.state)
        .await
        .expect("run the dispatch pass");
}

/// A turn's attempt, as the dispatch pass started it.
struct Attempt {
    org_id: Uuid,
    task_id: Uuid,
    attempt: i32,
    vm_id: Uuid,
    epoch: u64,
    seq: u64,
}

async fn running_attempt(app: &TestApp, org_id: Uuid, task_id: Uuid) -> Attempt {
    let (status, attempt, vm_id, epoch): (String, i32, Option<String>, Option<i64>) =
        sqlx::query_as("SELECT status, attempt, vm_id, epoch FROM tasks WHERE id = $1")
            .bind(task_id)
            .fetch_one(&app.db)
            .await
            .expect("load the turn");
    assert_eq!(status, "running", "the dispatch pass started the turn");
    Attempt {
        org_id,
        task_id,
        attempt,
        vm_id: vm_id.expect("a VM").parse().expect("VM id"),
        epoch: u64::try_from(epoch.expect("an epoch")).expect("epoch"),
        seq: 0,
    }
}

impl Attempt {
    async fn event(&mut self, app: &TestApp, kind: &str, payload: Value, terminal: bool) {
        self.seq += 1;
        let mut db = app.scoped_db_for_org(self.org_id).await;
        app.state
            .task_service
            .apply(
                &mut db,
                TaskLifecycleEvent::Journal(JournalTaskEvent {
                    task_id: self.task_id,
                    attempt: self.attempt,
                    vm_id: self.vm_id,
                    epoch: self.epoch,
                    exec_id: canonical_exec_id(self.task_id, self.attempt, self.vm_id),
                    seq: self.seq,
                    kind: kind.to_string(),
                    payload,
                    terminal,
                }),
            )
            .await
            .expect("apply the attempt's event");
        db.commit().await.expect("commit the event");
    }

    async fn accept(&mut self, app: &TestApp) {
        let (input_id, reservation_id, revision, generation): (Uuid, Uuid, i32, i32) =
            sqlx::query_as(
                "SELECT id, reservation_id, reservation_revision, delivery_generation
                 FROM session_inputs
                 WHERE task_id = $1 AND attempt = $2 AND delivery = 'reserved'",
            )
            .bind(self.task_id)
            .bind(self.attempt)
            .fetch_one(&app.db)
            .await
            .expect("the attempt's reserved message");
        self.event(
            app,
            "input.accepted",
            json!({
                "input_id": input_id,
                "reservation_id": reservation_id,
                "revision": revision,
                "delivery_generation": generation,
                "system_notice_ids": [],
            }),
            false,
        )
        .await;
    }

    async fn reply(&mut self, app: &TestApp, text: &str) {
        self.event(
            app,
            "message.delta",
            json!({"block_id":"claude-message-1:0","text_delta":text}),
            false,
        )
        .await;
    }

    async fn end(&mut self, app: &TestApp, outcome: Value) {
        self.event(app, "invocation.outcome", outcome, true).await;
    }
}

/// Runs one whole successful turn in `session_id`; returns its task.
async fn finish_turn(app: &TestApp, ws: &Workspace, session_id: Uuid, text: &str) -> Uuid {
    let task_id = ws.send(app, session_id, text).await;
    dispatch(app).await;
    let mut attempt = running_attempt(app, ws.org.id, task_id).await;
    attempt.accept(app).await;
    attempt.reply(app, "Done.").await;
    attempt
        .end(app, json!({"status":"succeeded","exit_code":0}))
        .await;
    let status: String = sqlx::query_scalar("SELECT status FROM tasks WHERE id = $1")
        .bind(task_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(status, "succeeded");
    task_id
}

/// A workspace member who can read the workspace's chats but not write them.
async fn reader(app: &TestApp, ws: &Workspace) -> TestUser {
    TestUser::create_with_scopes(
        app,
        ws.org.id,
        &[g5_auth::Scope::WorkspacesRead, g5_auth::Scope::SessionsRead],
    )
    .await
}

/// Active chats come first by turn start; the others by their last finished
/// turn, falling back to creation; renaming moves nothing.
#[tokio::test]
async fn chats_order_by_turn_activity_and_rename_does_not_reorder() {
    let app = TestApp::spawn().await;
    let ws = Workspace::open(&app).await;
    let finished = ws.chat(&app, "Finished").await;
    let running = ws.chat(&app, "Running").await;
    let idle = ws.chat(&app, "Never ran").await;

    // Before any turn, creation order decides, newest first.
    assert_eq!(ws.order(&app).await, vec![idle, running, finished]);

    let finished_task = finish_turn(&app, &ws, finished, "Summarize the repository").await;
    // A finished turn outranks a chat created earlier.
    assert_eq!(ws.order(&app).await, vec![finished, idle, running]);

    let running_task = ws.send(&app, running, "List the tests").await;
    dispatch(&app).await;
    running_attempt(&app, ws.org.id, running_task).await;
    assert_eq!(ws.order(&app).await, vec![running, finished, idle]);

    let sessions = ws.list(&app).await;
    let active = &sessions[0]["activity"];
    assert!(active["activeTurnStartedAt"].is_string(), "{active}");
    assert!(active["lastFinishedAt"].is_null(), "{active}");
    let done = &sessions[1]["activity"];
    assert!(done["activeTurnStartedAt"].is_null(), "{done}");
    assert!(done["lastFinishedAt"].is_string(), "{done}");
    assert_eq!(done["latestResult"]["taskId"], finished_task.to_string());
    assert_eq!(done["latestResult"]["attempt"], 1);
    assert_eq!(done["latestResult"]["hasOutput"], true);
    assert_eq!(done["unseenResult"], true);
    let never = &sessions[2]["activity"];
    assert!(never["latestResult"].is_null(), "{never}");
    assert_eq!(never["unseenResult"], false);

    // Renaming bumps updated_at but is not turn activity.
    let updated_before = sessions[2]["updatedAt"].clone();
    ws.rename(&app, idle, "Renamed").await;
    ws.rename(&app, finished, "").await;
    let after = ws.list(&app).await;
    assert_ne!(after[2]["updatedAt"], updated_before);
    assert_eq!(ws.order(&app).await, vec![running, finished, idle]);

    // Inspecting is not activity either.
    let (status, _) = ws
        .inspect(&app, &ws.owner, finished, finished_task, Some(1))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ws.order(&app).await, vec![running, finished, idle]);

    app.cleanup().await;
}

/// Two running chats order by when their turns started, newest first.
#[tokio::test]
async fn active_chats_order_by_turn_start() {
    let app = TestApp::spawn().await;
    let ws = Workspace::open(&app).await;
    let first = ws.chat(&app, "First").await;
    let second = ws.chat(&app, "Second").await;
    let older_turn = ws.send(&app, second, "Older turn").await;
    let newer_turn = ws.send(&app, first, "Newer turn").await;
    dispatch(&app).await;
    for task in [older_turn, newer_turn] {
        let status: String = sqlx::query_scalar("SELECT status FROM tasks WHERE id = $1")
            .bind(task)
            .fetch_one(&app.db)
            .await
            .unwrap();
        assert!(matches!(status.as_str(), "queued" | "running"), "{status}");
    }
    assert_eq!(ws.order(&app).await, vec![first, second]);
    app.cleanup().await;
}

/// The ordering covers the whole workspace before the 50-chat page limit.
#[tokio::test]
async fn ordering_applies_before_the_page_limit() {
    let app = TestApp::spawn().await;
    let ws = Workspace::open(&app).await;
    let oldest = ws.chat(&app, "Oldest").await;
    for index in 0..50 {
        ws.chat(&app, &format!("Chat {index}")).await;
    }
    assert!(!ws.order(&app).await.contains(&oldest));
    ws.send(&app, oldest, "Wake up").await;
    let page = ws.order(&app).await;
    assert_eq!(page.len(), 50);
    assert_eq!(page[0], oldest, "the active chat leads the first page");
    app.cleanup().await;
}

/// Held and resumable work does not spin; its settled attempt is a result.
#[tokio::test]
async fn a_held_turn_is_not_active_and_its_result_is_unseen() {
    let app = TestApp::spawn().await;
    let ws = Workspace::open(&app).await;
    let chat = ws.chat(&app, "Held").await;
    let task = ws.send(&app, chat, "Start").await;
    dispatch(&app).await;
    let mut attempt = running_attempt(&app, ws.org.id, task).await;
    attempt.accept(&app).await;
    attempt.reply(&app, "Partial").await;
    // The attempt was interrupted: the task waits for Continue.
    attempt
        .end(&app, json!({"status":"interrupted","reason":"idle"}))
        .await;
    let (status, hold): (String, Option<String>) =
        sqlx::query_as("SELECT status, hold_reason FROM tasks WHERE id = $1")
            .bind(task)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(status, "waiting", "hold {hold:?}");
    let entry = ws.entry_as(&app, &ws.owner, chat).await;
    let activity = &entry["activity"];
    assert!(activity["activeTurnStartedAt"].is_null(), "{activity}");
    assert_eq!(activity["latestResult"]["taskId"], task.to_string());
    assert_eq!(activity["unseenResult"], true);
    app.cleanup().await;
}

/// A pending question or permission request of the running attempt marks the
/// turn as waiting for the person and hints the list; the person's answer
/// clears it. A deferred question that outlives its attempt holds the turn
/// and still waits (chat g5.chat.activity.awaiting-input).
#[tokio::test]
async fn a_pending_request_marks_the_turn_awaiting_input() {
    let app = TestApp::spawn().await;
    let ws = Workspace::open(&app).await;
    let chat = ws.chat(&app, "Asking").await;
    let channel = format!("workspace:{}", ws.workspace_id);
    let mut socket = app.ws_connect(&ws.owner, &channel).await;
    let wait = Duration::from_secs(10);

    let task = ws.send(&app, chat, "Ask me").await;
    socket.expect_event("chat.activity", wait).await;
    dispatch(&app).await;
    let mut attempt = running_attempt(&app, ws.org.id, task).await;
    attempt.accept(&app).await;
    let activity = ws.entry_as(&app, &ws.owner, chat).await["activity"].clone();
    assert_eq!(activity["awaitingInput"], false, "{activity}");

    let display = json!({"tool_name":"Bash","command":"touch allowed"});
    attempt
        .event(
            &app,
            "provider_request.opened",
            json!({
                "provider_request_id": "perm-1",
                "request_kind": "permission",
                "tool_name": "Bash",
                "payload": display,
                "display": display,
                "deadline_unix_ms": chrono::Utc::now().timestamp_millis() + 60_000,
            }),
            false,
        )
        .await;
    let hint = socket.expect_event("chat.activity", wait).await;
    assert_eq!(hint["sessionId"], chat.to_string(), "{hint}");
    let activity = ws.entry_as(&app, &ws.owner, chat).await["activity"].clone();
    assert_eq!(activity["awaitingInput"], true, "{activity}");
    assert!(activity["activeTurnStartedAt"].is_string(), "{activity}");

    // The owner's answer is on its way: the turn no longer waits for them.
    let replied = app
        .authenticated_client(&ws.owner)
        .post_json(
            &format!("/api/v1/tasks/{task}/provider-requests/perm-1/reply"),
            &json!({
                "attempt": attempt.attempt,
                "requestId": "perm-1",
                "validatedResponse": {"behavior": "allow"},
            }),
        )
        .await;
    let status = replied.status();
    let replied: Value = replied.json().await.expect("reply response");
    assert_eq!(status, StatusCode::OK, "{replied}");
    let hint = socket.expect_event("chat.activity", wait).await;
    assert_eq!(hint["sessionId"], chat.to_string(), "{hint}");
    let activity = ws.entry_as(&app, &ws.owner, chat).await["activity"].clone();
    assert_eq!(activity["awaitingInput"], false, "{activity}");
    assert!(activity["activeTurnStartedAt"].is_string(), "{activity}");

    // A deferred question outlives its attempt: the held turn still waits.
    let deferred = ws.chat(&app, "Deferred").await;
    let task = ws.send(&app, deferred, "Ask later").await;
    dispatch(&app).await;
    let mut attempt = running_attempt(&app, ws.org.id, task).await;
    attempt.accept(&app).await;
    let native_session = Uuid::new_v4().to_string();
    attempt
        .event(
            &app,
            "provider.session",
            json!({"provider_id":"claude_code","provider_session_id":native_session}),
            false,
        )
        .await;
    let question =
        json!({"questions":[{"question":"Choose","options":[{"label":"A"},{"label":"B"}]}]});
    attempt
        .event(
            &app,
            "provider_request.opened",
            json!({
                "provider_request_id": "toolu-deferred",
                "request_kind": "question",
                "tool_name": "AskUserQuestion",
                "payload": question,
                "display": question,
                "deadline_unix_ms": null,
                "resume_session_id": native_session,
            }),
            false,
        )
        .await;
    attempt
        .end(
            &app,
            json!({
                "status": "succeeded",
                "result": {
                    "schema_version": "g5.agent.turn.outcome/v1",
                    "status": "suspended",
                    "code": "native_question_deferred",
                    "retryable": false,
                    "provider_session_id": native_session,
                    "session_files_state": "present",
                    "session_manifest_generation": 1,
                },
            }),
        )
        .await;
    let (status, hold): (String, Option<String>) =
        sqlx::query_as("SELECT status, hold_reason FROM tasks WHERE id = $1")
            .bind(task)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(
        (status.as_str(), hold.as_deref()),
        ("waiting", Some("awaiting_answer"))
    );
    let activity = ws.entry_as(&app, &ws.owner, deferred).await["activity"].clone();
    assert_eq!(activity["awaitingInput"], true, "{activity}");
    assert!(activity["activeTurnStartedAt"].is_null(), "{activity}");
    app.cleanup().await;
}

/// A turn queued behind stopped compute waits for the owner: it does not spin
/// until the workspace has compute running or starting.
#[tokio::test]
async fn a_queued_turn_without_compute_is_not_active() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let client = app.authenticated_client(&owner);
    let created = client
        .post_json(
            &format!("/api/v1/workspaces/{}/sessions", workspace.id),
            &json!({"sessionType":"chat","title":"Stopped","agentProvider":"claude_code"}),
        )
        .await;
    let created: Value = created.json().await.unwrap();
    let chat = uuid(&data(&created)["id"]);
    let sent = client
        .post_json(
            &format!("/api/v1/sessions/{chat}/messages"),
            &json!({ "content": "Waits for a start" }),
        )
        .await;
    let status = sent.status();
    let sent: Value = sent.json().await.unwrap();
    assert_eq!(status, StatusCode::CREATED, "{sent}");
    let task = uuid(&data(&sent)["taskId"]);
    let turn_status: String = sqlx::query_scalar("SELECT status FROM tasks WHERE id = $1")
        .bind(task)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(turn_status, "queued");
    let listed = client
        .get(&format!("/api/v1/workspaces/{}/sessions", workspace.id))
        .await;
    let listed: Value = listed.json().await.unwrap();
    let activity = &data(&listed)[0]["activity"];
    assert!(activity["activeTurnStartedAt"].is_null(), "{activity}");

    // Once compute runs, the same queued turn is active.
    let _runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let listed = client
        .get(&format!("/api/v1/workspaces/{}/sessions", workspace.id))
        .await;
    let listed: Value = listed.json().await.unwrap();
    let activity = &data(&listed)[0]["activity"];
    assert!(activity["activeTurnStartedAt"].is_string(), "{activity}");
    app.cleanup().await;
}

/// A turn that ends without any attempt still finishes with a result.
#[tokio::test]
async fn a_turn_refused_before_any_attempt_is_a_finished_result() {
    let app = TestApp::spawn().await;
    let ws = Workspace::open(&app).await;
    let chat = ws.chat(&app, "Refused").await;
    let task = ws.send(&app, chat, "Never runs").await;
    // As settle_dispatch_admission_failure leaves a queued turn.
    sqlx::query(
        "UPDATE tasks SET status = 'failed', ended_at = now(), updated_at = now()
         WHERE id = $1 AND status = 'queued'",
    )
    .bind(task)
    .execute(&app.db)
    .await
    .unwrap();
    let activity = ws.entry_as(&app, &ws.owner, chat).await["activity"].clone();
    assert_eq!(activity["latestResult"]["taskId"], task.to_string());
    assert!(activity["latestResult"]["attempt"].is_null(), "{activity}");
    assert_eq!(activity["latestResult"]["hasOutput"], false);
    assert_eq!(activity["unseenResult"], true);
    let (status, body) = ws.inspect(&app, &ws.owner, chat, task, None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data(&body)["unseenResult"], false);
    app.cleanup().await;
}

/// Inspection is personal, reader-writable and only moves forward; a stale
/// acknowledgement cannot clear a newer result, and only finished results of
/// the chat are accepted.
#[tokio::test]
async fn inspection_is_personal_monotonic_and_names_a_finished_result() {
    let app = TestApp::spawn().await;
    let ws = Workspace::open(&app).await;
    let reader = reader(&app, &ws).await;
    let chat = ws.chat(&app, "Shared").await;
    let other = ws.chat(&app, "Other").await;
    let first = finish_turn(&app, &ws, chat, "First").await;

    assert_eq!(
        ws.entry_as(&app, &ws.owner, chat).await["activity"]["unseenResult"],
        true
    );
    assert_eq!(
        ws.entry_as(&app, &reader, chat).await["activity"]["unseenResult"],
        true
    );

    // The owner's viewing does not clear the reader's indicator.
    let (status, body) = ws.inspect(&app, &ws.owner, chat, first, Some(1)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data(&body)["unseenResult"], false);
    assert_eq!(
        ws.entry_as(&app, &ws.owner, chat).await["activity"]["unseenResult"],
        false
    );
    assert_eq!(
        ws.entry_as(&app, &reader, chat).await["activity"]["unseenResult"],
        true
    );

    // A reader records their own inspection with read access alone.
    let (status, body) = ws.inspect(&app, &reader, chat, first, Some(1)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        ws.entry_as(&app, &reader, chat).await["activity"]["unseenResult"],
        false
    );
    let reader_rows: i64 =
        sqlx::query_scalar("SELECT count(*) FROM chat_session_inspections WHERE session_id = $1")
            .bind(chat)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(reader_rows, 2);

    // A newer result is unseen; acknowledging the older one again keeps it so.
    let second = finish_turn(&app, &ws, chat, "Second").await;
    assert_eq!(
        ws.entry_as(&app, &ws.owner, chat).await["activity"]["unseenResult"],
        true
    );
    let (status, body) = ws.inspect(&app, &ws.owner, chat, first, Some(1)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(data(&body)["unseenResult"], true, "{body}");
    assert_eq!(
        data(&body)["latestResult"]["taskId"],
        second.to_string(),
        "{body}"
    );

    // Results of another chat, unknown attempts and unsettled work are refused.
    let (status, _) = ws.inspect(&app, &ws.owner, other, second, Some(1)).await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, _) = ws.inspect(&app, &ws.owner, chat, second, Some(7)).await;
    assert_eq!(status, StatusCode::CONFLICT);
    let running = ws.send(&app, chat, "Third").await;
    dispatch(&app).await;
    running_attempt(&app, ws.org.id, running).await;
    let (status, _) = ws.inspect(&app, &ws.owner, chat, running, Some(1)).await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, _) = ws.inspect(&app, &ws.owner, chat, running, None).await;
    assert_eq!(status, StatusCode::CONFLICT);

    // The spinner takes precedence, but the unseen second result stays recorded.
    let activity = ws.entry_as(&app, &ws.owner, chat).await["activity"].clone();
    assert!(activity["activeTurnStartedAt"].is_string(), "{activity}");
    assert_eq!(activity["unseenResult"], true);
    assert_eq!(activity["latestResult"]["taskId"], second.to_string());

    // Readers see the same activity, including the running turn.
    assert!(ws.entry_as(&app, &reader, chat).await["activity"]["activeTurnStartedAt"].is_string());

    // A non-member cannot record inspection at all.
    let stranger = TestUser::create(&app).await;
    let (status, _) = ws.inspect(&app, &stranger, chat, second, Some(1)).await;
    assert!(
        matches!(status, StatusCode::FORBIDDEN | StatusCode::NOT_FOUND),
        "{status}"
    );

    // Session detail carries the same activity for the caller.
    let detail = app
        .authenticated_client(&reader)
        .get(&format!("/api/v1/sessions/{chat}"))
        .await;
    let detail: Value = detail.json().await.unwrap();
    assert!(
        data(&detail)["activity"]["activeTurnStartedAt"].is_string(),
        "{detail}"
    );
    assert_eq!(data(&detail)["activity"]["unseenResult"], true);

    app.cleanup().await;
}

/// Results that ended before a chat's floor count as inspected, as historical
/// results do at rollout.
#[tokio::test]
async fn results_before_the_rollout_floor_count_as_inspected() {
    let app = TestApp::spawn().await;
    let ws = Workspace::open(&app).await;
    let chat = ws.chat(&app, "Legacy").await;
    finish_turn(&app, &ws, chat, "Before rollout").await;
    sqlx::query("UPDATE chat_sessions SET inspection_floor_at = now() WHERE id = $1")
        .bind(chat)
        .execute(&app.db)
        .await
        .unwrap();
    let activity = ws.entry_as(&app, &ws.owner, chat).await["activity"].clone();
    assert!(activity["latestResult"].is_object(), "{activity}");
    assert_eq!(activity["unseenResult"], false);
    finish_turn(&app, &ws, chat, "After rollout").await;
    assert_eq!(
        ws.entry_as(&app, &ws.owner, chat).await["activity"]["unseenResult"],
        true
    );
    app.cleanup().await;
}

/// Messages name the attempt that produced them, so a reader can tell which
/// result the transcript displays.
#[tokio::test]
async fn messages_name_their_attempt() {
    let app = TestApp::spawn().await;
    let ws = Workspace::open(&app).await;
    let chat = ws.chat(&app, "Attempts").await;
    let task = finish_turn(&app, &ws, chat, "Name it").await;
    let messages = app
        .authenticated_client(&ws.owner)
        .get(&format!("/api/v1/sessions/{chat}/messages?tail=true"))
        .await;
    let messages: Value = messages.json().await.unwrap();
    let assistant = data(&messages)["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|message| message["role"] == "assistant")
        .cloned()
        .expect("the reply");
    assert_eq!(assistant["taskId"], task.to_string());
    assert_eq!(assistant["attempt"], 1);
    app.cleanup().await;
}

/// Turn start and settlement hint the workspace channel; streamed output does
/// not; an acknowledgement reaches only the acknowledging person.
#[tokio::test]
async fn activity_changes_hint_the_workspace_channel() {
    let app = TestApp::spawn().await;
    let ws = Workspace::open(&app).await;
    let reader = reader(&app, &ws).await;
    let chat = ws.chat(&app, "Hinted").await;
    let channel = format!("workspace:{}", ws.workspace_id);
    let mut owner_socket = app.ws_connect(&ws.owner, &channel).await;
    let mut reader_socket = app.ws_connect(&reader, &channel).await;
    let wait = Duration::from_secs(10);

    let task = ws.send(&app, chat, "Hint me").await;
    let hint = owner_socket.expect_event("chat.activity", wait).await;
    assert_eq!(hint["sessionId"], chat.to_string(), "{hint}");
    reader_socket.expect_event("chat.activity", wait).await;

    dispatch(&app).await;
    let mut attempt = running_attempt(&app, ws.org.id, task).await;
    attempt.accept(&app).await;
    attempt.reply(&app, "Streaming").await;
    owner_socket
        .expect_no_event("chat.activity", Duration::from_millis(500))
        .await;
    attempt
        .end(&app, json!({"status":"succeeded","exit_code":0}))
        .await;
    let hint = owner_socket.expect_event("chat.activity", wait).await;
    assert_eq!(hint["sessionId"], chat.to_string(), "{hint}");
    reader_socket.expect_event("chat.activity", wait).await;

    let (status, _) = ws.inspect(&app, &ws.owner, chat, task, Some(1)).await;
    assert_eq!(status, StatusCode::OK);
    owner_socket.expect_event("chat.activity", wait).await;
    reader_socket
        .expect_no_event("chat.activity", Duration::from_millis(500))
        .await;
    // Repeating it advances nothing and says nothing.
    ws.inspect(&app, &ws.owner, chat, task, Some(1)).await;
    owner_socket
        .expect_no_event("chat.activity", Duration::from_millis(500))
        .await;
    app.cleanup().await;
}
