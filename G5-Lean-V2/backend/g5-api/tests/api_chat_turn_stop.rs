//! Stop in a chat ends the turn (`g5.agents.lifecycle.chat-stop`), and the
//! owner's next message ends a chat turn held for Continue
//! (`g5.agents.lifecycle.chat-held-turn`); Start sends a message a workspace
//! Stop held (`g5.lifecycle.start.held-work`). Messages go through the chat routes
//! and dispatch through the maintenance pass to the test runtime peer; each
//! attempt's events are applied with `TaskService::apply`, as host control
//! applies them.

use crate::helpers;

use helpers::{TestApp, TestOrg, TestUser, TestWorkspace, TestWorkspaceRuntimePeer};
use reqwest::StatusCode;
use serde_json::{json, Value};
use uuid::Uuid;

use g5_api::services::agent_dispatch::EnqueueAgentInput;
use g5_api::services::task_lifecycle::{
    canonical_agent_message_id, canonical_exec_id, JournalTaskEvent, TaskLifecycleEvent,
};
use g5_contracts::types::AgentLaunchPreferencesV1;

fn data(body: &Value) -> &Value {
    body.get("data").unwrap_or(body)
}

fn uuid(value: &Value) -> Uuid {
    value
        .as_str()
        .and_then(|text| text.parse().ok())
        .unwrap_or_else(|| panic!("expected a uuid, got {value}"))
}

/// How the chat's guest answers.
enum Guest {
    Plain,
    /// The VM cancels an invoked turn before the agent starts it.
    UnstartedCancellation,
    /// `modules.cancel` is held until the test releases it.
    HeldCancelAnswer,
    /// The first `modules.cancel`, with its resends, closes without an answer.
    DroppedCancelAnswer,
}

/// An owner's chat on a running workspace whose turns can dispatch.
struct Chat {
    owner: TestUser,
    org_id: Uuid,
    workspace_id: Uuid,
    runtime: TestWorkspaceRuntimePeer,
    session_id: Uuid,
}

impl Chat {
    async fn open(app: &TestApp) -> Self {
        Self::open_with(app, Guest::Plain).await
    }

    /// A chat whose VM cancels an invoked turn before the agent starts it.
    async fn open_with_unstarted_cancellation(app: &TestApp) -> Self {
        Self::open_with(app, Guest::UnstartedCancellation).await
    }

    async fn open_with(app: &TestApp, guest: Guest) -> Self {
        let mut owner = TestUser::create(app).await;
        let org = TestOrg::create_with_admin(app, &mut owner).await;
        let workspace = TestWorkspace::create(app, &org).await;
        let runtime = match guest {
            Guest::Plain => TestWorkspaceRuntimePeer::attach(app, &workspace, owner.id).await,
            Guest::UnstartedCancellation => {
                TestWorkspaceRuntimePeer::attach_with_unstarted_cancellation(
                    app, &workspace, owner.id,
                )
                .await
            }
            Guest::HeldCancelAnswer => {
                TestWorkspaceRuntimePeer::attach_with_rpc_barrier(
                    app,
                    &workspace,
                    owner.id,
                    "modules.cancel",
                )
                .await
            }
            Guest::DroppedCancelAnswer => {
                // The runtime resends a closed stream four times before it
                // gives up, so the guest drops the request and every resend.
                TestWorkspaceRuntimePeer::attach_dropping_replies(
                    app,
                    &workspace,
                    owner.id,
                    "modules.cancel",
                    5,
                )
                .await
            }
        };
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
                &json!({"sessionType":"chat","title":"Chat stop","agentProvider":"claude_code"}),
            )
            .await;
        let status = created.status();
        let created: Value = created.json().await.expect("session response");
        assert_eq!(status, StatusCode::CREATED, "{created}");
        Self {
            session_id: uuid(&data(&created)["id"]),
            org_id: org.id,
            workspace_id: workspace.id,
            owner,
            runtime,
        }
    }

    /// Sends a message: the input id the client holds, and the input's turn.
    async fn send(&self, app: &TestApp, content: &str) -> (Uuid, Uuid) {
        self.send_in(app, self.session_id, content).await
    }

    /// Sends a message in another of the workspace's chats.
    async fn send_in(&self, app: &TestApp, session_id: Uuid, content: &str) -> (Uuid, Uuid) {
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
        (
            uuid(&data(&sent)["messageId"]),
            uuid(&data(&sent)["taskId"]),
        )
    }

    /// Opens another chat in the same workspace.
    async fn open_another(&self, app: &TestApp) -> Uuid {
        let created = app
            .authenticated_client(&self.owner)
            .post_json(
                &format!("/api/v1/workspaces/{}/sessions", self.workspace_id),
                &json!({"sessionType":"chat","title":"Another chat","agentProvider":"claude_code"}),
            )
            .await;
        let status = created.status();
        let created: Value = created.json().await.expect("session response");
        assert_eq!(status, StatusCode::CREATED, "{created}");
        uuid(&data(&created)["id"])
    }

    /// The chat's Stop naming `turn_id`; whether it stopped anything.
    async fn stop(&self, app: &TestApp, turn_id: Uuid) -> bool {
        let stopped = app
            .authenticated_client(&self.owner)
            .post_json(
                &format!(
                    "/api/v1/sessions/{}/messages/{turn_id}/stop",
                    self.session_id
                ),
                &json!({}),
            )
            .await;
        let status = stopped.status();
        let body: Value = stopped.json().await.expect("stop response");
        assert_eq!(status, StatusCode::OK, "{body}");
        data(&body)["stopped"]
            .as_bool()
            .expect("stop answers whether it stopped")
    }

    async fn cancels_sent(&self) -> usize {
        self.runtime
            .received_rpc_methods()
            .await
            .iter()
            .filter(|method| method.as_str() == "modules.cancel")
            .count()
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

async fn running_attempt(app: &TestApp, chat: &Chat, task_id: Uuid) -> Attempt {
    let (status, attempt, vm_id, epoch): (String, i32, Option<String>, Option<i64>) =
        sqlx::query_as("SELECT status, attempt, vm_id, epoch FROM tasks WHERE id = $1")
            .bind(task_id)
            .fetch_one(&app.db)
            .await
            .expect("load the turn");
    assert_eq!(status, "running", "the dispatch pass started the turn");
    Attempt {
        org_id: chat.org_id,
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

    /// The agent received the turn's message; the reply exists from here on.
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

    fn reply_id(&self) -> Uuid {
        canonical_agent_message_id(self.task_id, self.attempt)
    }
}

/// The turn's status, hold reason and whether its attempt was asked to stop.
async fn turn(app: &TestApp, task_id: Uuid) -> (String, Option<String>, bool) {
    sqlx::query_as(
        "SELECT status, hold_reason,
                COALESCE((spec #>> '{lifecycle,cancel_requested}')::boolean, false)
         FROM tasks WHERE id = $1",
    )
    .bind(task_id)
    .fetch_one(&app.db)
    .await
    .expect("load the turn")
}

async fn input(app: &TestApp, input_id: Uuid) -> (Option<Uuid>, String) {
    sqlx::query_as("SELECT task_id, delivery FROM session_inputs WHERE id = $1")
        .bind(input_id)
        .fetch_one(&app.db)
        .await
        .expect("load the message")
}

async fn reply(app: &TestApp, reply_id: Uuid) -> (String, String) {
    sqlx::query_as("SELECT status, content FROM chat_messages WHERE id = $1")
        .bind(reply_id)
        .fetch_one(&app.db)
        .await
        .expect("load the reply")
}

fn held(hold_reason: &str) -> (String, Option<String>, bool) {
    ("waiting".into(), Some(hold_reason.into()), false)
}

/// Stop ends a running turn: its attempt is cancelled, the reply keeps its
/// partial text marked stopped, and the message queued behind the turn runs
/// next as a new turn, with no Continue.
#[tokio::test]
async fn stop_ends_a_running_turn_and_the_queued_message_runs_next() {
    let app = TestApp::spawn().await;
    let chat = Chat::open(&app).await;
    let (_, first_task) = chat.send(&app, "Summarize the repository").await;
    dispatch(&app).await;
    let mut first = running_attempt(&app, &chat, first_task).await;
    first.accept(&app).await;
    first.reply(&app, "The repository holds").await;
    let (queued, queued_task) = chat.send(&app, "Then list the tests").await;
    assert_eq!(
        queued_task, first_task,
        "a message sent during a turn waits behind it"
    );

    // The page names the live reply.
    assert!(chat.stop(&app, first.reply_id()).await);
    assert_eq!(
        turn(&app, first_task).await,
        ("running".into(), None, true),
        "the attempt is asked to stop, not held"
    );
    assert_eq!(chat.cancels_sent().await, 1);
    // The dispatcher reports the cancelled attempt.
    first.end(&app, json!({"status":"cancelled"})).await;

    assert_eq!(turn(&app, first_task).await.0, "cancelled");
    let (status, text) = reply(&app, first.reply_id()).await;
    assert_eq!(status, "stopped");
    assert!(text.contains("The repository holds"), "{text}");
    let (next_task, delivery) = input(&app, queued).await;
    let next_task = next_task.expect("the queued message has a turn");
    assert_ne!(
        next_task, first_task,
        "the queued message starts a new turn"
    );
    assert_eq!(delivery, "queued");

    dispatch(&app).await;
    running_attempt(&app, &chat, next_task).await;
    assert_eq!(
        input(&app, queued).await,
        (Some(next_task), "reserved".into())
    );
    app.cleanup().await;
}

/// The owner's next message after Stop starts a new turn whose prompt is
/// rebuilt from the chat, which keeps the stopped reply.
#[tokio::test]
async fn the_next_message_after_stop_carries_the_stopped_reply() {
    let app = TestApp::spawn().await;
    let chat = Chat::open(&app).await;
    let (_, first_task) = chat.send(&app, "Summarize the repository").await;
    dispatch(&app).await;
    let mut first = running_attempt(&app, &chat, first_task).await;
    first.accept(&app).await;
    first.reply(&app, "Half an answer").await;
    assert!(chat.stop(&app, first.reply_id()).await);
    first.end(&app, json!({"status":"cancelled"})).await;

    let (_, next_task) = chat.send(&app, "Go on").await;
    assert_ne!(next_task, first_task);
    dispatch(&app).await;
    let next = running_attempt(&app, &chat, next_task).await;
    let prompt: String = sqlx::query_scalar(
        "SELECT policy_snapshot_ref #>> '{frozen_recovery_prompt,text}'
         FROM task_attempts WHERE task_id = $1 AND attempt = $2",
    )
    .bind(next_task)
    .bind(next.attempt)
    .fetch_one(&app.db)
    .await
    .expect("the new turn's prompt");
    for part in ["Summarize the repository", "Half an answer", "Go on"] {
        assert!(prompt.contains(part), "{part:?} missing from {prompt}");
    }
    app.cleanup().await;
}

/// Stop on a message still waiting behind the running turn withdraws just
/// that message; the running turn and the other queued message go on.
#[tokio::test]
async fn stop_on_a_waiting_message_withdraws_only_that_message() {
    let app = TestApp::spawn().await;
    let chat = Chat::open(&app).await;
    let (_, first_task) = chat.send(&app, "Summarize the repository").await;
    dispatch(&app).await;
    let mut first = running_attempt(&app, &chat, first_task).await;
    first.accept(&app).await;
    let (second, _) = chat.send(&app, "Then list the tests").await;
    let (third, _) = chat.send(&app, "Then the docs").await;

    assert!(chat.stop(&app, second).await);
    assert_eq!(input(&app, second).await.1, "withdrawn");
    assert_eq!(
        input(&app, third).await,
        (Some(first_task), "queued".into())
    );
    assert_eq!(
        turn(&app, first_task).await,
        ("running".into(), None, false)
    );
    assert_eq!(chat.cancels_sent().await, 0);
    app.cleanup().await;
}

/// A Stop for a turn that already ended changes nothing, so it never reaches
/// the turn that runs after it.
#[tokio::test]
async fn a_late_stop_never_reaches_a_later_turn() {
    let app = TestApp::spawn().await;
    let chat = Chat::open(&app).await;
    let (first_input, first_task) = chat.send(&app, "Summarize the repository").await;
    dispatch(&app).await;
    let mut first = running_attempt(&app, &chat, first_task).await;
    first.accept(&app).await;
    first
        .end(
            &app,
            json!({"status":"succeeded","result":{"status":"succeeded"}}),
        )
        .await;
    assert_eq!(turn(&app, first_task).await.0, "succeeded");
    let (_, second_task) = chat.send(&app, "Then list the tests").await;
    dispatch(&app).await;
    running_attempt(&app, &chat, second_task).await;

    assert!(!chat.stop(&app, first.reply_id()).await);
    assert!(!chat.stop(&app, first_input).await);
    assert_eq!(
        turn(&app, second_task).await,
        ("running".into(), None, false)
    );
    assert_eq!(chat.cancels_sent().await, 0);
    app.cleanup().await;
}

/// Stop on a held turn ends it at once, and its message, which the agent
/// never received, is withdrawn. (A message the owner sends behind a held
/// turn ends that turn itself, so none is left to run next here.)
#[tokio::test]
async fn stop_ends_a_held_turn_and_withdraws_its_undelivered_message() {
    let app = TestApp::spawn().await;
    let chat = Chat::open(&app).await;
    let (first_input, first_task) = chat.send(&app, "Summarize the repository").await;
    dispatch(&app).await;
    let mut first = running_attempt(&app, &chat, first_task).await;
    idle_stop(&app, &mut first).await;
    assert_eq!(turn(&app, first_task).await, held("idle_stop"));

    assert!(chat.stop(&app, first_input).await);
    assert_eq!(turn(&app, first_task).await.0, "cancelled");
    assert_eq!(input(&app, first_input).await.1, "withdrawn");
    assert_eq!(chat.cancels_sent().await, 0, "nothing was running");
    app.cleanup().await;
}

/// The idle stop interrupts the attempt before the agent received its
/// message, as the workspace's idle stop does.
async fn idle_stop(app: &TestApp, attempt: &mut Attempt) {
    request_idle_stop(app, attempt.task_id).await;
    attempt.end(app, json!({"status":"cancelled"})).await;
}

/// The idle stop has asked the turn's attempt to stop; it has not settled.
async fn request_idle_stop(app: &TestApp, task_id: Uuid) {
    sqlx::query(
        "UPDATE tasks
         SET spec = jsonb_set(spec, '{lifecycle}',
                 COALESCE(spec -> 'lifecycle', '{}'::jsonb)
                     || jsonb_build_object('interrupt_reason','idle'), true)
         WHERE id = $1",
    )
    .bind(task_id)
    .execute(&app.db)
    .await
    .expect("mark the idle interruption");
}

/// A new message ends a turn an idle stop held. The turn's message, which the
/// agent never received, stays queued and runs first, then the new one.
#[tokio::test]
async fn a_new_message_ends_an_idle_held_turn_and_its_undelivered_message_runs_first() {
    let app = TestApp::spawn().await;
    let chat = Chat::open(&app).await;
    let (first_input, first_task) = chat.send(&app, "Summarize the repository").await;
    dispatch(&app).await;
    let mut first = running_attempt(&app, &chat, first_task).await;
    idle_stop(&app, &mut first).await;
    assert_eq!(turn(&app, first_task).await, held("idle_stop"));
    assert_eq!(
        input(&app, first_input).await,
        (Some(first_task), "queued".into())
    );

    let (second_input, second_task) = chat.send(&app, "Then list the tests").await;
    assert_ne!(second_task, first_task, "the new message starts a new turn");
    assert_eq!(turn(&app, first_task).await.0, "cancelled");
    assert_eq!(
        input(&app, first_input).await,
        (Some(second_task), "queued".into()),
        "the undelivered message joins the new turn"
    );

    dispatch(&app).await;
    let mut second = running_attempt(&app, &chat, second_task).await;
    assert_eq!(
        input(&app, first_input).await,
        (Some(second_task), "reserved".into()),
        "the undelivered message runs first"
    );
    assert_eq!(
        input(&app, second_input).await,
        (Some(second_task), "queued".into())
    );
    second.accept(&app).await;
    second
        .end(
            &app,
            json!({"status":"succeeded","result":{"status":"succeeded"}}),
        )
        .await;
    let (third_task, _) = input(&app, second_input).await;
    let third_task = third_task.expect("the new message has a turn");
    assert_ne!(third_task, second_task);
    dispatch(&app).await;
    running_attempt(&app, &chat, third_task).await;
    assert_eq!(
        input(&app, second_input).await,
        (Some(third_task), "reserved".into())
    );
    app.cleanup().await;
}

/// A new message ends a turn held after the agent's question went
/// unanswered, and runs as the next turn.
#[tokio::test]
async fn a_new_message_ends_a_turn_held_after_an_unanswered_question() {
    let app = TestApp::spawn().await;
    let chat = Chat::open(&app).await;
    let (_, first_task) = chat.send(&app, "Summarize the repository").await;
    dispatch(&app).await;
    let mut first = running_attempt(&app, &chat, first_task).await;
    first.accept(&app).await;
    first.reply(&app, "Which branch?").await;
    first
        .end(
            &app,
            json!({"status":"succeeded","result":{"status":"suspended","code":"native_response_timeout"}}),
        )
        .await;
    assert_eq!(turn(&app, first_task).await, held("response_timeout"));

    let (second_input, second_task) = chat.send(&app, "Use main").await;
    assert_ne!(second_task, first_task);
    assert_eq!(turn(&app, first_task).await.0, "cancelled");
    dispatch(&app).await;
    running_attempt(&app, &chat, second_task).await;
    assert_eq!(
        input(&app, second_input).await,
        (Some(second_task), "reserved".into())
    );
    app.cleanup().await;
}

/// A turn whose message may or may not have reached the agent keeps the
/// chat's Redeliver choice: a new message waits behind it.
#[tokio::test]
async fn a_turn_of_unknown_delivery_still_waits_for_redelivery() {
    let app = TestApp::spawn().await;
    let chat = Chat::open(&app).await;
    let (first_input, first_task) = chat.send(&app, "Summarize the repository").await;
    dispatch(&app).await;
    let mut first = running_attempt(&app, &chat, first_task).await;
    first
        .end(
            &app,
            json!({"status":"interrupted","detail":{"input_delivery_unknown":true}}),
        )
        .await;
    assert_eq!(turn(&app, first_task).await, held("delivery_unknown"));

    let (_, second_task) = chat.send(&app, "Then list the tests").await;
    assert_eq!(second_task, first_task, "the new message waits behind it");
    assert_eq!(turn(&app, first_task).await, held("delivery_unknown"));
    assert_eq!(input(&app, first_input).await.1, "delivery_unknown");
    app.cleanup().await;
}

/// Claude's End that has to stop Claude ends a chat turn as Stop does, and
/// the message queued behind it runs next.
#[tokio::test]
async fn claude_end_that_stops_claude_ends_a_chat_turn() {
    let app = TestApp::spawn().await;
    let chat = Chat::open(&app).await;
    let (_, first_task) = chat.send(&app, "Run the build in the background").await;
    dispatch(&app).await;
    let mut first = running_attempt(&app, &chat, first_task).await;
    first.accept(&app).await;
    first.reply(&app, "Waiting on the build").await;
    let (queued, _) = chat.send(&app, "Then list the tests").await;

    first.end(&app, background_wait_ended()).await;
    assert_eq!(turn(&app, first_task).await.0, "cancelled");
    assert_eq!(reply(&app, first.reply_id()).await.0, "stopped");
    let (next_task, delivery) = input(&app, queued).await;
    assert_ne!(
        next_task,
        Some(first_task),
        "the queued message starts a new turn"
    );
    assert_eq!(delivery, "queued");
    app.cleanup().await;
}

/// What the agent turn reports when Claude's End had to stop Claude.
fn background_wait_ended() -> Value {
    json!({"status":"succeeded","result":{"status":"interrupted","code":"background_wait_ended"}})
}

/// Stop is final even when the attempt ends another way before the stop
/// reaches it: an unanswered question or an interruption still ends the
/// turn, and the message queued behind it runs next.
#[tokio::test]
async fn stop_is_final_whatever_ends_the_attempt_first() {
    let app = TestApp::spawn().await;
    let chat = Chat::open(&app).await;
    let (_, first_task) = chat.send(&app, "Summarize the repository").await;
    let mut task_id = first_task;
    for (raced_by, next) in [
        (
            json!({"status":"succeeded","result":{"status":"suspended","code":"native_response_timeout"}}),
            "Then list the tests",
        ),
        (json!({"status":"interrupted"}), "Then the docs"),
    ] {
        dispatch(&app).await;
        let mut attempt = running_attempt(&app, &chat, task_id).await;
        attempt.accept(&app).await;
        let (queued, _) = chat.send(&app, next).await;
        assert!(chat.stop(&app, attempt.reply_id()).await);
        attempt.end(&app, raced_by.clone()).await;

        assert_eq!(
            turn(&app, task_id).await.0,
            "cancelled",
            "{raced_by} reached the attempt before the stop"
        );
        let (next_task, delivery) = input(&app, queued).await;
        let next_task = next_task.expect("the queued message has a turn");
        assert_ne!(next_task, task_id, "the queued message starts a new turn");
        assert_eq!(delivery, "queued");
        task_id = next_task;
    }
    app.cleanup().await;
}

/// Stop ends a turn held before any attempt reserved its message: the
/// message is withdrawn and the message queued behind it runs next.
#[tokio::test]
async fn stop_ends_a_turn_held_before_its_first_attempt() {
    let app = TestApp::spawn().await;
    let chat = Chat::open(&app).await;
    let (first_input, first_task) = chat.send(&app, "Summarize the repository").await;
    let (queued, queued_task) = chat.send(&app, "Then list the tests").await;
    assert_eq!(queued_task, first_task);
    let mut db = app.scoped_db_for_org(chat.org_id).await;
    app.state
        .task_service
        .request_workspace_task_pause(&mut db, chat.owner.id, first_task)
        .await
        .expect("hold the turn before its first attempt");
    db.commit().await.expect("commit the hold");
    assert_eq!(turn(&app, first_task).await, held("owner_stop"));

    assert!(chat.stop(&app, first_input).await);
    assert_eq!(turn(&app, first_task).await.0, "cancelled");
    assert_eq!(input(&app, first_input).await.1, "withdrawn");
    let (next_task, delivery) = input(&app, queued).await;
    let next_task = next_task.expect("the queued message has a turn");
    assert_ne!(
        next_task, first_task,
        "the queued message starts a new turn"
    );
    assert_eq!(delivery, "queued");
    dispatch(&app).await;
    running_attempt(&app, &chat, next_task).await;
    app.cleanup().await;
}

/// A message the agent never received is carried into the turn that a new
/// message starts, and runs first there: a Stop naming it ends that run, and
/// the newer message still runs next.
#[tokio::test]
async fn stop_naming_a_carried_message_ends_the_turn_that_runs_it() {
    let app = TestApp::spawn().await;
    let chat = Chat::open(&app).await;
    let (first_input, first_task) = chat.send(&app, "Summarize the repository").await;
    dispatch(&app).await;
    let mut first = running_attempt(&app, &chat, first_task).await;
    idle_stop(&app, &mut first).await;
    let (second_input, second_task) = chat.send(&app, "Then list the tests").await;
    dispatch(&app).await;
    let mut second = running_attempt(&app, &chat, second_task).await;
    second.accept(&app).await;
    assert_eq!(
        input(&app, first_input).await,
        (Some(second_task), "delivered".into())
    );

    assert!(chat.stop(&app, first_input).await);
    assert_eq!(
        turn(&app, second_task).await,
        ("running".into(), None, true)
    );
    second.end(&app, json!({"status":"cancelled"})).await;
    assert_eq!(turn(&app, second_task).await.0, "cancelled");
    let (third_task, delivery) = input(&app, second_input).await;
    let third_task = third_task.expect("the newer message has a turn");
    assert_ne!(third_task, second_task, "the newer message runs next");
    assert_eq!(delivery, "queued");
    app.cleanup().await;
}

/// A session's messages run in the order they were sent, even while another
/// transaction, such as an edit, holds the earliest one: dispatch waits for
/// it rather than run a later message first. A turn's own message is its
/// earliest, which Stop relies on.
#[tokio::test]
async fn messages_run_in_order_while_the_earliest_is_being_edited() {
    let app = TestApp::spawn().await;
    let chat = Chat::open(&app).await;
    let (first_input, first_task) = chat.send(&app, "Summarize the repository").await;
    dispatch(&app).await;
    let mut first = running_attempt(&app, &chat, first_task).await;
    idle_stop(&app, &mut first).await;
    let (second_input, second_task) = chat.send(&app, "Then list the tests").await;

    let mut edit = app.db.begin().await.expect("begin the edit");
    sqlx::query("SELECT 1 FROM session_inputs WHERE id = $1 FOR UPDATE")
        .bind(first_input)
        .execute(&mut *edit)
        .await
        .expect("hold the carried message");
    dispatch(&app).await;
    assert_eq!(
        turn(&app, second_task).await.0,
        "queued",
        "the later message does not run first"
    );
    assert_eq!(input(&app, second_input).await.1, "queued");
    edit.rollback().await.expect("end the edit");

    dispatch(&app).await;
    running_attempt(&app, &chat, second_task).await;
    assert_eq!(
        input(&app, first_input).await,
        (Some(second_task), "reserved".into())
    );
    assert_eq!(input(&app, second_input).await.1, "queued");
    app.cleanup().await;
}

/// While another transaction holds one chat's earliest message, the dispatch
/// pass leaves that turn for a later pass and runs the next chat's turn.
#[tokio::test]
async fn a_held_message_does_not_stall_other_chats() {
    let app = TestApp::spawn().await;
    let chat = Chat::open(&app).await;
    let (held_input, held_task) = chat.send(&app, "Summarize the repository").await;
    let other_chat = chat.open_another(&app).await;
    let (_, other_task) = chat.send_in(&app, other_chat, "List the tests").await;

    let mut edit = app.db.begin().await.expect("begin the edit");
    sqlx::query("SELECT 1 FROM session_inputs WHERE id = $1 FOR UPDATE")
        .bind(held_input)
        .execute(&mut *edit)
        .await
        .expect("hold the message");
    dispatch(&app).await;
    running_attempt(&app, &chat, other_task).await;
    assert_eq!(
        turn(&app, held_task).await.0,
        "queued",
        "the held message waits"
    );
    edit.rollback().await.expect("end the edit");

    dispatch(&app).await;
    running_attempt(&app, &chat, held_task).await;
    assert_eq!(input(&app, held_input).await.1, "reserved");
    app.cleanup().await;
}

/// The dispatch pass lets go of a turn it passes over before it goes on. A
/// withdrawal holds that turn's message and the workspace, then needs the
/// turn's row, while the next turn's failed admission starts a follow-up turn
/// whose insert needs the workspace row: holding the passed-over turn would
/// deadlock the two.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_passed_over_turn_is_released_before_the_next_one() {
    let app = TestApp::spawn().await;
    let chat = Chat::open(&app).await;
    let (held_input, held_task) = chat.send(&app, "Summarize the repository").await;
    let other_chat = chat.open_another(&app).await;
    let (_, other_task) = chat.send_in(&app, other_chat, "List the tests").await;
    sqlx::query(
        "UPDATE tasks
         SET spec = jsonb_set(spec, '{agent,schemaVersion}', '\"unsupported\"')
         WHERE id = $1",
    )
    .bind(other_task)
    .execute(&app.db)
    .await
    .expect("make the other turn fail admission");

    // The withdrawal holds the message, then the workspace, as
    // `withdraw_queued_session_input` and the cancellation it calls do (its
    // session allocation lock, taken first, is for another chat than the
    // failing turn's and changes nothing here).
    let mut withdrawal = app.db.begin().await.expect("begin the withdrawal");
    sqlx::query("SELECT 1 FROM session_inputs WHERE id = $1 FOR UPDATE")
        .bind(held_input)
        .execute(&mut *withdrawal)
        .await
        .expect("hold the message");
    sqlx::query("SELECT 1 FROM workspaces WHERE id = $1 FOR UPDATE")
        .bind(chat.workspace_id)
        .execute(&mut *withdrawal)
        .await
        .expect("hold the workspace");
    let pass = g5_api_core::maintenance::agent_tasks::run_once(&app.state);
    let withdraw = async {
        tokio::time::timeout(std::time::Duration::from_secs(15), async {
            loop {
                let waiting: bool = sqlx::query_scalar(
                    "SELECT EXISTS (SELECT 1 FROM pg_stat_activity
                     WHERE datname = current_database() AND wait_event_type = 'Lock'
                       AND query LIKE 'INSERT INTO tasks(%')",
                )
                .fetch_one(&app.db)
                .await
                .expect("read lock waits");
                if waiting {
                    return;
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("the follow-up turn waits for the workspace row");
        // The cancellation then takes the turn the pass passed over.
        let taken = sqlx::query("SELECT 1 FROM tasks WHERE id = $1 FOR UPDATE")
            .bind(held_task)
            .execute(&mut *withdrawal)
            .await
            .map(|_| ());
        withdrawal.rollback().await.expect("end the withdrawal");
        taken
    };
    let (ran, taken) = tokio::join!(pass, withdraw);
    ran.expect("run the dispatch pass");
    taken.expect("the withdrawal takes the passed-over turn");
    assert_eq!(turn(&app, other_task).await.0, "failed");

    // The settled admission does not end the pass: once the withdrawal let
    // go of the message, the same pass reserved and started the turn it had
    // passed over (`g5.chat.secret-admission`).
    running_attempt(&app, &chat, held_task).await;
    app.cleanup().await;
}

/// Stop holds a turn the platform started, as a merge-conflict chat's first
/// turn is, for Continue, as it holds a workflow turn: its merge run follows
/// that task, so cancelling it would strand the merge.
#[tokio::test]
async fn stop_holds_a_turn_the_platform_started() {
    let app = TestApp::spawn().await;
    let chat = Chat::open(&app).await;
    let (message, task) = send_platform_message(&app, &chat).await;

    assert!(chat.stop(&app, message).await);
    assert_eq!(turn(&app, task).await, held("owner_stop"));
    assert_eq!(input(&app, message).await, (Some(task), "queued".into()));
    app.cleanup().await;
}

/// Claude's End that has to stop Claude also holds a turn the platform
/// started.
#[tokio::test]
async fn claude_end_that_stops_claude_holds_a_platform_turn() {
    let app = TestApp::spawn().await;
    let chat = Chat::open(&app).await;
    let (_, task) = send_platform_message(&app, &chat).await;
    dispatch(&app).await;
    let mut first = running_attempt(&app, &chat, task).await;
    first.accept(&app).await;

    first.end(&app, background_wait_ended()).await;
    assert_eq!(turn(&app, task).await, held("owner_stop"));
    app.cleanup().await;
}

/// A message the owner sends while the turn is still stopping waits behind
/// it; once the stop settles and holds the turn, that message ends it and
/// starts the next turn.
#[tokio::test]
async fn a_message_sent_while_the_turn_stops_ends_it_once_held() {
    let app = TestApp::spawn().await;
    let chat = Chat::open(&app).await;
    let (_, first_task) = chat.send(&app, "Summarize the repository").await;
    dispatch(&app).await;
    let mut first = running_attempt(&app, &chat, first_task).await;
    first.accept(&app).await;
    request_idle_stop(&app, first_task).await;
    let (second, second_task) = chat.send(&app, "Then list the tests").await;
    assert_eq!(second_task, first_task, "it waits behind the stopping turn");

    first.end(&app, json!({"status":"cancelled"})).await;
    assert_eq!(turn(&app, first_task).await.0, "cancelled");
    let (next_task, delivery) = input(&app, second).await;
    let next_task = next_task.expect("the message has a turn");
    assert_ne!(next_task, first_task, "the message starts a new turn");
    assert_eq!(delivery, "queued");
    app.cleanup().await;
}

/// The platform's first message in this chat, as a merge-conflict
/// resolution sends it: the input and its turn.
async fn send_platform_message(app: &TestApp, chat: &Chat) -> (Uuid, Uuid) {
    let mut db = app.scoped_db_for_org(chat.org_id).await;
    let queued = app
        .state
        .task_service
        .enqueue_agent_input(
            &mut db,
            EnqueueAgentInput {
                task_name: Some("Resolve conflicts".into()),
                session_id: chat.session_id,
                actor_user_id: chat.owner.id,
                content: "Resolve the merge conflicts".into(),
                artifacts: json!([]),
                input_origin: "workflow".into(),
                template_ref: None,
                template_revision: None,
                confirmed_launch: None,
                agent_launch: AgentLaunchPreferencesV1::default(),
            },
        )
        .await
        .expect("enqueue the platform's message");
    db.commit().await.expect("commit the message");
    (queued.input_id, queued.task_id)
}

/// Stop naming a queued turn's own message ends that turn, and the message
/// queued behind it starts the next turn.
#[tokio::test]
async fn stop_ends_a_queued_turn_and_its_follow_up_starts_the_next() {
    let app = TestApp::spawn().await;
    let chat = Chat::open(&app).await;
    let (first_input, first_task) = chat.send(&app, "Summarize the repository").await;
    let (queued, queued_task) = chat.send(&app, "Then list the tests").await;
    assert_eq!(
        queued_task, first_task,
        "both wait in the turn that has not started"
    );

    assert!(chat.stop(&app, first_input).await);
    assert_eq!(turn(&app, first_task).await.0, "cancelled");
    assert_eq!(input(&app, first_input).await.1, "withdrawn");
    let (next_task, delivery) = input(&app, queued).await;
    let next_task = next_task.expect("the queued message has a turn");
    assert_ne!(
        next_task, first_task,
        "the queued message starts a new turn"
    );
    assert_eq!(delivery, "queued");
    app.cleanup().await;
}

/// A held turn the platform started, such as a merge-conflict resolution its
/// merge run follows by task, is not ended by the owner's next message: the
/// message waits behind it, as for a workflow turn.
#[tokio::test]
async fn a_held_platform_turn_waits_for_continue() {
    let app = TestApp::spawn().await;
    let chat = Chat::open(&app).await;
    let (_, task) = send_platform_message(&app, &chat).await;
    dispatch(&app).await;
    let mut first = running_attempt(&app, &chat, task).await;
    idle_stop(&app, &mut first).await;
    assert_eq!(turn(&app, task).await, held("idle_stop"));

    let (_, second_task) = chat.send(&app, "Prefer our side").await;
    assert_eq!(second_task, task, "the message waits behind it");
    assert_eq!(turn(&app, task).await, held("idle_stop"));
    app.cleanup().await;
}

/// Start sends a message queued before a workspace Stop, with no new message
/// (`g5.lifecycle.start.held-work`). The Stop cancels a turn the agent never
/// started, which holds it and returns its message to the queue; the host's
/// Ready for the resumed VM returns the turn to the queue, and the next
/// dispatch pass sends the same message (QA 2026-10-08).
#[tokio::test]
async fn start_sends_a_message_queued_before_a_workspace_stop() {
    use g5_api::ws::host_ctl::ControlPlaneHostState;
    use g5_internal_api::host_ctl::{VmEvent, VmEventKind};

    let app = TestApp::spawn().await;
    let chat = Chat::open_with_unstarted_cancellation(&app).await;
    let (message, task) = chat.send(&app, "Keep an eye on the CI").await;
    dispatch(&app).await;
    let first = running_attempt(&app, &chat, task).await;

    let client = app.authenticated_client(&chat.owner);
    let stopped = client
        .post_json(
            &format!("/api/v1/workspaces/{}/stop", chat.workspace_id),
            &json!({"operationId": Uuid::new_v4()}),
        )
        .await;
    let status = stopped.status();
    let stopped: Value = stopped.json().await.expect("stop response");
    assert_eq!(status, StatusCode::OK, "{stopped}");
    assert_eq!(data(&stopped)["status"], "stopped");
    assert_eq!(turn(&app, task).await, held("owner_stop"));
    assert_eq!(input(&app, message).await, (Some(task), "queued".into()));

    let resumed = client
        .post_json(
            &format!("/api/v1/workspaces/{}/resume", chat.workspace_id),
            &json!({"operationId": Uuid::new_v4()}),
        )
        .await;
    let status = resumed.status();
    let resumed: Value = resumed.json().await.expect("resume response");
    assert_eq!(status, StatusCode::OK, "{resumed}");
    assert_eq!(data(&resumed)["outcome"], "starting");
    let (vm_id, epoch, catalog, contract): (String, i64, String, i64) = sqlx::query_as(
        "SELECT vm_id, epoch, module_catalog_version,
                (boot_spec ->> 'dispatcher_contract_version')::bigint
         FROM workspace_vms WHERE workspace_id = $1",
    )
    .bind(chat.workspace_id)
    .fetch_one(&app.db)
    .await
    .expect("load the starting VM");
    app.host_control_backend()
        .ingest_vm_event(
            chat.runtime.host_id(),
            &VmEvent {
                workspace_id: chat.workspace_id,
                vm_id: vm_id.parse().expect("VM id"),
                seq: 1,
                kind: VmEventKind::Ready {
                    epoch: u64::try_from(epoch).expect("epoch"),
                    module_catalog_version: catalog,
                    dispatcher_contract_version: u32::try_from(contract).expect("contract"),
                },
                at: chrono::Utc::now(),
                log_tail: None,
            },
        )
        .await
        .expect("ingest the resumed VM's Ready");
    assert_eq!(turn(&app, task).await, ("queued".into(), None, false));

    dispatch(&app).await;
    let second = running_attempt(&app, &chat, task).await;
    assert_eq!(second.attempt, first.attempt + 1);
    assert_eq!(second.vm_id.to_string(), vm_id);
    let delivery: (String, Option<i32>) =
        sqlx::query_as("SELECT delivery, attempt FROM session_inputs WHERE id = $1")
            .bind(message)
            .fetch_one(&app.db)
            .await
            .expect("load the message");
    assert_eq!(delivery, ("reserved".into(), Some(second.attempt)));
    app.cleanup().await;
}

/// The inputs list and its mutations serialize the published camelCase
/// contract the frontend schema validates (issue #629 S16), including the
/// server-evaluated queued-input permissions.
#[tokio::test]
async fn inputs_endpoint_serializes_the_published_camel_case_contract() {
    let app = TestApp::spawn().await;
    let chat = Chat::open(&app).await;
    let (message, _) = chat.send(&app, "Summarize the repository").await;

    let client = app.authenticated_client(&chat.owner);
    let listed = client
        .get(&format!("/api/v1/sessions/{}/inputs", chat.session_id))
        .await;
    let status = listed.status();
    let listed: Value = listed.json().await.expect("inputs response");
    assert_eq!(status, StatusCode::OK, "{listed}");
    let inputs = data(&listed).as_array().expect("inputs array");
    let queued = inputs
        .iter()
        .find(|input| input["id"] == message.to_string().as_str())
        .expect("the sent message is listed");
    let object = queued.as_object().expect("input object");
    for field in [
        "id",
        "sessionId",
        "inputIndex",
        "content",
        "artifacts",
        "revision",
        "inputOrigin",
        "delivery",
        "deliveryGeneration",
        "taskId",
        "attempt",
        "createdAt",
        "deliveredAt",
        "withdrawnAt",
        "editable",
        "withdrawable",
    ] {
        assert!(
            object.contains_key(field),
            "missing field {field}: {queued}"
        );
    }
    assert!(
        !object.keys().any(|key| key.contains('_')),
        "snake_case key leaked into the wire contract: {queued}"
    );
    assert_eq!(queued["inputOrigin"], "human");
    assert_eq!(queued["delivery"], "queued");
    assert_eq!(queued["editable"], true);
    assert_eq!(queued["withdrawable"], true);

    let edited = client
        .patch_json(
            &format!("/api/v1/sessions/{}/inputs/{message}", chat.session_id),
            &json!({"expectedRevision": 1, "content": "Summarize the README"}),
        )
        .await;
    let status = edited.status();
    let edited: Value = edited.json().await.expect("edit response");
    assert_eq!(status, StatusCode::OK, "{edited}");
    assert_eq!(data(&edited)["revision"], 2);
    assert_eq!(data(&edited)["inputOrigin"], "human");
    assert_eq!(data(&edited)["editable"], true);

    let withdrawn = client
        .post_json(
            &format!(
                "/api/v1/sessions/{}/inputs/{message}/withdraw",
                chat.session_id
            ),
            &json!({"expectedRevision": 2}),
        )
        .await;
    let status = withdrawn.status();
    let withdrawn: Value = withdrawn.json().await.expect("withdraw response");
    assert_eq!(status, StatusCode::OK, "{withdrawn}");
    assert_eq!(data(&withdrawn)["delivery"], "withdrawn");
    assert_eq!(data(&withdrawn)["withdrawable"], false);
    assert_eq!(data(&withdrawn)["editable"], false);
    app.cleanup().await;
}

/// Withdrawing a turn's only message cancels the turn, and a message sent
/// while that withdrawal is uncommitted survives it on a fresh turn: the
/// withdrawal holds the session's turn-allocation lock across its
/// check-then-cancel, so the send cannot join the task it is cancelling
/// (issue #629 S24).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_send_during_an_uncommitted_withdrawal_survives_on_a_new_turn() {
    let app = TestApp::spawn().await;
    let chat = Chat::open(&app).await;
    let (message, task) = chat.send(&app, "Summarize the repository").await;

    let mut withdrawal = app.scoped_db_for_org(chat.org_id).await;
    app.state
        .task_service
        .withdraw_queued_session_input(&mut withdrawal, chat.owner.id, chat.session_id, message, 1)
        .await
        .expect("withdraw the only message");

    let send = tokio::spawn({
        let client = app.authenticated_client(&chat.owner);
        let session_id = chat.session_id;
        async move {
            client
                .post_json(
                    &format!("/api/v1/sessions/{session_id}/messages"),
                    &json!({ "content": "Then list the tests" }),
                )
                .await
        }
    });
    // The send waits for the withdrawal's allocation lock instead of joining
    // the task the withdrawal is about to cancel.
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    assert!(!send.is_finished(), "the send waits for the withdrawal");
    withdrawal.commit().await.expect("commit the withdrawal");

    let sent = send.await.expect("join the send");
    let status = sent.status();
    let sent: Value = sent.json().await.expect("send response");
    assert_eq!(status, StatusCode::CREATED, "{sent}");
    let new_task = uuid(&data(&sent)["taskId"]);
    assert_ne!(
        new_task, task,
        "the new message does not join the cancelled turn"
    );
    assert_eq!(turn(&app, task).await.0, "cancelled");
    assert_eq!(input(&app, message).await.1, "withdrawn");
    let new_message = uuid(&data(&sent)["messageId"]);
    assert_eq!(
        input(&app, new_message).await,
        (Some(new_task), "queued".into())
    );

    dispatch(&app).await;
    running_attempt(&app, &chat, new_task).await;
    app.cleanup().await;
}

/// The reverse order: while a send is uncommitted, the withdrawal of the
/// older message waits for it, then sees the newly attached message and
/// leaves the shared turn running instead of cancelling it (issue #629 S24).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_withdrawal_during_an_uncommitted_send_spares_the_joined_turn() {
    let app = TestApp::spawn().await;
    let chat = Chat::open(&app).await;
    let (message, task) = chat.send(&app, "Summarize the repository").await;

    let mut send = app.scoped_db_for_org(chat.org_id).await;
    let joined = app
        .state
        .task_service
        .enqueue_agent_input(
            &mut send,
            EnqueueAgentInput {
                session_id: chat.session_id,
                actor_user_id: chat.owner.id,
                task_name: None,
                content: "Then list the tests".to_string(),
                artifacts: json!([]),
                input_origin: "human".to_string(),
                template_ref: None,
                template_revision: None,
                confirmed_launch: None,
                agent_launch: AgentLaunchPreferencesV1::default(),
            },
        )
        .await
        .expect("enqueue the joining message");
    assert_eq!(
        joined.task_id, task,
        "the second message joins the open turn"
    );

    let withdraw = tokio::spawn({
        let client = app.authenticated_client(&chat.owner);
        let session_id = chat.session_id;
        async move {
            client
                .post_json(
                    &format!("/api/v1/sessions/{session_id}/inputs/{message}/withdraw"),
                    &json!({"expectedRevision": 1}),
                )
                .await
        }
    });
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    assert!(!withdraw.is_finished(), "the withdrawal waits for the send");
    send.commit().await.expect("commit the send");

    let withdrawn = withdraw.await.expect("join the withdrawal");
    let status = withdrawn.status();
    let withdrawn: Value = withdrawn.json().await.expect("withdraw response");
    assert_eq!(status, StatusCode::OK, "{withdrawn}");
    assert_eq!(input(&app, message).await.1, "withdrawn");
    assert_eq!(
        input(&app, joined.input_id).await,
        (Some(task), "queued".into()),
        "the joined message survives the older withdrawal"
    );
    assert_eq!(turn(&app, task).await.0, "queued");

    dispatch(&app).await;
    running_attempt(&app, &chat, task).await;
    app.cleanup().await;
}

/// Sequential controls stay intact around the race fix: withdrawing the last
/// message cancels the turn, and withdrawing one of two leaves the turn with
/// the survivor.
#[tokio::test]
async fn sequential_withdrawals_cancel_only_an_emptied_turn() {
    let app = TestApp::spawn().await;
    let chat = Chat::open(&app).await;
    let (first, task) = chat.send(&app, "Summarize the repository").await;
    let (second, second_task) = chat.send(&app, "Then list the tests").await;
    assert_eq!(second_task, task);

    let client = app.authenticated_client(&chat.owner);
    let withdrawn = client
        .post_json(
            &format!(
                "/api/v1/sessions/{}/inputs/{first}/withdraw",
                chat.session_id
            ),
            &json!({"expectedRevision": 1}),
        )
        .await;
    assert_eq!(withdrawn.status(), StatusCode::OK);
    assert_eq!(turn(&app, task).await.0, "queued", "a message remains");

    let withdrawn = client
        .post_json(
            &format!(
                "/api/v1/sessions/{}/inputs/{second}/withdraw",
                chat.session_id
            ),
            &json!({"expectedRevision": 1}),
        )
        .await;
    assert_eq!(withdrawn.status(), StatusCode::OK);
    assert_eq!(turn(&app, task).await.0, "cancelled", "nothing left to run");
    app.cleanup().await;
}

/// Withdrawal's task cancellation must not upgrade the route's shared
/// workspace authority lock: a Send holding its own share lock while waiting
/// for the withdrawal's turn-allocation lock would deadlock against that
/// upgrade (issue #629 review). The withdrawal completes while another
/// transaction keeps the workspace share lock.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn withdrawal_cancels_the_emptied_turn_without_the_workspace_row_lock() {
    let app = TestApp::spawn().await;
    let chat = Chat::open(&app).await;
    let (message, task) = chat.send(&app, "Summarize the repository").await;

    let mut share_holder = app.db.begin().await.expect("begin the share holder");
    sqlx::query("SELECT 1 FROM workspaces WHERE id = $1 FOR SHARE")
        .bind(chat.workspace_id)
        .execute(&mut *share_holder)
        .await
        .expect("hold the workspace share lock");

    let withdrawn = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        app.authenticated_client(&chat.owner)
            .post_json(
                &format!(
                    "/api/v1/sessions/{}/inputs/{message}/withdraw",
                    chat.session_id
                ),
                &json!({"expectedRevision": 1}),
            )
            .await
    })
    .await
    .expect("withdrawal must not wait for an exclusive workspace lock");
    assert_eq!(withdrawn.status(), StatusCode::OK);
    assert_eq!(turn(&app, task).await.0, "cancelled");
    share_holder
        .rollback()
        .await
        .expect("release the share holder");
    app.cleanup().await;
}

// ---------------------------------------------------------------------------
// Repeated Stop requests converge without a lock cycle
// (`g5.chat.control-locks`, `g5.chat.controls`; #966)
// ---------------------------------------------------------------------------

/// A Stop's raw response: status and body, without asserting either.
async fn stop_response(app: &TestApp, chat: &Chat, turn_id: Uuid) -> (StatusCode, Value) {
    let response = app
        .authenticated_client(&chat.owner)
        .post_json(
            &format!(
                "/api/v1/sessions/{}/messages/{turn_id}/stop",
                chat.session_id
            ),
            &json!({}),
        )
        .await;
    let status = response.status();
    let body: Value = response.json().await.expect("stop response");
    (status, body)
}

/// Holds the session's turn-allocation advisory lock, which every Stop takes
/// first, so concurrent Stops queue at the lock boundary the incident's
/// deadlock formed behind.
async fn hold_session_lock(
    app: &TestApp,
    session_id: Uuid,
) -> sqlx::Transaction<'static, sqlx::Postgres> {
    let (class, key) = helpers::chat_session_turn_lock_key(session_id);
    let mut holder = app.db.begin().await.expect("begin the lock holder");
    sqlx::query("SELECT pg_advisory_xact_lock($1, $2)")
        .bind(class)
        .bind(key)
        .execute(&mut *holder)
        .await
        .expect("hold the session's turn-allocation lock");
    holder
}

/// Waits until at least `count` backends wait on an advisory lock.
async fn wait_for_advisory_waiters(app: &TestApp, count: i64) {
    tokio::time::timeout(std::time::Duration::from_secs(15), async {
        loop {
            let waiting: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM pg_stat_activity
                 WHERE datname = current_database() AND wait_event_type = 'Lock'
                   AND wait_event = 'advisory'",
            )
            .fetch_one(&app.db)
            .await
            .expect("read advisory lock waits");
            if waiting >= count {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("{count} requests reach the session lock boundary"));
}

async fn cancel_intent(app: &TestApp, task_id: Uuid) -> (bool, Option<String>) {
    sqlx::query_as::<_, (bool, Option<String>)>(
        "SELECT COALESCE((spec #>> '{lifecycle,cancel_requested}')::boolean, false),
                spec #>> '{lifecycle,cancel_requested_at}'
         FROM tasks WHERE id = $1",
    )
    .bind(task_id)
    .fetch_one(&app.db)
    .await
    .expect("load the cancel intent")
}

/// Two Stops that both reach the session lock while the first's workspace
/// share is held no longer upgrade that share: neither deadlocks, neither
/// answers a deadlock-derived 503, and the turn keeps one cancel intent
/// (`g5.chat.control-locks`).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_stops_behind_the_session_lock_both_commit_without_a_deadlock() {
    let app = TestApp::spawn().await;
    let chat = Chat::open(&app).await;
    let (turn_id, task_id) = chat.send(&app, "Write the report").await;
    dispatch(&app).await;
    running_attempt(&app, &chat, task_id).await;

    let holder = hold_session_lock(&app, chat.session_id).await;
    let first = stop_response(&app, &chat, turn_id);
    let second = stop_response(&app, &chat, turn_id);
    let release = async {
        wait_for_advisory_waiters(&app, 2).await;
        holder.rollback().await.expect("release the session lock");
    };
    let ((first_status, first_body), (second_status, second_body), ()) =
        tokio::join!(first, second, release);
    for (name, status, body) in [
        ("first", first_status, &first_body),
        ("second", second_status, &second_body),
    ] {
        assert_eq!(status, StatusCode::OK, "{name}: {body}");
        assert_eq!(data(body)["stopped"], true, "{name}: {body}");
        assert!(
            matches!(
                data(body)["disposition"].as_str(),
                Some("signalling" | "already_finished" | "cancelled")
            ),
            "{name}: {body}"
        );
    }
    let (requested, _) = cancel_intent(&app, task_id).await;
    assert!(
        requested,
        "the running turn carries one durable cancel intent"
    );
    let status: String = sqlx::query_scalar("SELECT status FROM tasks WHERE id = $1")
        .bind(task_id)
        .fetch_one(&app.db)
        .await
        .expect("load the turn");
    assert_eq!(
        status, "running",
        "the attempt settles only on the guest's outcome"
    );
    assert!(
        chat.cancels_sent().await >= 1,
        "the committed intent reached the guest"
    );
    app.cleanup().await;
}

/// Twenty Stops race a reconciliation batch for the same session lock
/// (the incident's third participant): every request answers 200, the
/// batch commits once the lock is free, and the turn ends with exactly one
/// cancel intent and no queue transition beyond it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_stop_burst_with_a_reconciliation_waiter_converges_on_one_intent() {
    let app = TestApp::spawn().await;
    let chat = Chat::open(&app).await;
    let (turn_id, task_id) = chat.send(&app, "Refactor the parser").await;
    dispatch(&app).await;
    let attempt = running_attempt(&app, &chat, task_id).await;
    // Output the follower stores as soon as it sees the tool boundary: its
    // batch waits on the session lock like the Stops do.
    chat.runtime
        .journal()
        .extend([
            json!({"seq":1,"kind":"module.message.delta","payload":{"block_id":"b1","text_delta":"Looking"}}),
            json!({"seq":2,"kind":"module.message.delta","payload":{"block_id":"b1","text_delta":" around"}}),
            json!({"seq":3,"kind":"module.tool.started","payload":{"call_id":"c1","name":"Bash","content":"ls"}}),
        ])
        .await;
    let mut live = app
        .ws_connect(&chat.owner, &format!("chat:{}", chat.session_id))
        .await;
    live.expect_subscribed().await;

    let holder = hold_session_lock(&app, chat.session_id).await;
    dispatch(&app).await; // the follower starts, publishes live and queues its batch
    for _ in 0..3 {
        let row = live
            .expect_event("chat.timeline", std::time::Duration::from_secs(10))
            .await;
        assert_eq!(row["entry"]["metadata"]["live"], true);
    }
    let exec_id = g5_api::services::task_lifecycle::canonical_exec_id(
        task_id,
        attempt.attempt,
        attempt.vm_id,
    );
    let burst = futures::future::join_all((0..20).map(|_| stop_response(&app, &chat, turn_id)));
    let release = async {
        // The batch and the first Stops queue at the lock (the test server's
        // connection pool bounds how many wait at once).
        wait_for_advisory_waiters(&app, 2).await;
        holder.rollback().await.expect("release the session lock");
    };
    let (responses, ()) = tokio::join!(burst, release);
    for (index, (status, body)) in responses.iter().enumerate() {
        assert_eq!(*status, StatusCode::OK, "stop {index}: {body}");
        assert!(
            matches!(
                data(body)["disposition"].as_str(),
                Some("signalling" | "already_finished" | "cancelled" | "undelivered")
            ),
            "stop {index}: {body}"
        );
    }
    assert!(
        app.state
            .followers
            .wait_persisted(&exec_id, 3, std::time::Duration::from_secs(15))
            .await,
        "the reconciliation batch commits once the lock is released"
    );
    let (requested, requested_at) = cancel_intent(&app, task_id).await;
    assert!(requested);
    assert!(requested_at.is_some(), "one cancel intent, recorded once");
    let (status, attempts): (String, i64) = sqlx::query_as(
        "SELECT status, (SELECT count(*) FROM task_attempts WHERE task_id = $1)
         FROM tasks WHERE id = $1",
    )
    .bind(task_id)
    .fetch_one(&app.db)
    .await
    .expect("load the turn");
    assert_eq!(status, "running");
    assert_eq!(
        attempts, 1,
        "no queue transition beyond the one cancel intent"
    );
    let stored: i64 = sqlx::query_scalar("SELECT count(*) FROM task_events WHERE task_id = $1")
        .bind(task_id)
        .fetch_one(&app.db)
        .await
        .expect("count the stored rows");
    assert_eq!(stored, 3);
    app.cleanup().await;
}

/// A Stop commits while an unrelated transaction keeps the workspace shared:
/// the control never needs the exclusive workspace lock.
#[tokio::test]
async fn a_stop_commits_while_another_transaction_holds_the_workspace_share() {
    let app = TestApp::spawn().await;
    let chat = Chat::open(&app).await;
    let (turn_id, task_id) = chat.send(&app, "Count the files").await;
    dispatch(&app).await;
    running_attempt(&app, &chat, task_id).await;
    let mut holder = app.db.begin().await.expect("begin the share holder");
    sqlx::query("SELECT 1 FROM workspaces WHERE id = $1 FOR SHARE")
        .bind(chat.workspace_id)
        .execute(&mut *holder)
        .await
        .expect("hold the workspace shared");
    let (status, body) = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        stop_response(&app, &chat, turn_id),
    )
    .await
    .expect("Stop must not wait for an exclusive workspace lock");
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data(&body)["disposition"], "signalling");
    holder.rollback().await.expect("release the share");
    app.cleanup().await;
}

/// A repeated Stop on a turn that already ended reports so and never reaches
/// the successor; a queued turn ends once (`g5.chat.controls`).
#[tokio::test]
async fn a_repeated_stop_reports_already_ended_and_leaves_the_successor_alone() {
    let app = TestApp::spawn().await;
    let chat = Chat::open(&app).await;
    let (first_turn, first_task) = chat.send(&app, "First").await;
    let (status, body) = stop_response(&app, &chat, first_turn).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data(&body)["stopped"], true);
    assert_eq!(
        data(&body)["disposition"],
        "ended",
        "a queued turn ends now"
    );
    let (status, body) = stop_response(&app, &chat, first_turn).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data(&body)["stopped"], false);
    assert_eq!(data(&body)["disposition"], "already_ended");
    let (_second_turn, second_task) = chat.send(&app, "Second").await;
    let (status, body) = stop_response(&app, &chat, first_turn).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data(&body)["disposition"], "already_ended");
    let statuses: Vec<(Uuid, String)> =
        sqlx::query_as("SELECT id, status FROM tasks WHERE id = ANY($1) ORDER BY created_at")
            .bind(vec![first_task, second_task])
            .fetch_all(&app.db)
            .await
            .expect("load both turns");
    assert_eq!(statuses[0], (first_task, "cancelled".to_string()));
    assert_eq!(
        statuses[1].1, "queued",
        "the late Stop never reached the successor"
    );
    app.cleanup().await;
}

/// Polls `check` (an expression that may `.await`) until it holds, failing
/// after `timeout`.
macro_rules! wait_until {
    ($what:expr, $timeout:expr, $check:expr) => {{
        let deadline = std::time::Instant::now() + $timeout;
        loop {
            if $check {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "{} within {:?}",
                $what,
                $timeout
            );
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    }};
}

async fn task_status(app: &TestApp, task_id: Uuid) -> String {
    sqlx::query_scalar("SELECT status FROM tasks WHERE id = $1")
        .bind(task_id)
        .fetch_one(&app.db)
        .await
        .expect("load the task")
}

/// A Stop whose guest answers late (the cancel call held at the guest) has
/// committed its intent before asking: the turn ends on the guest's actual
/// outcome, the message queued behind it starts the next turn, and the late
/// answer reaches neither that successor nor the session's lock
/// (`g5.chat.controls`, `g5.chat.control-locks`).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_delayed_stop_answer_leaves_the_successor_alone() {
    let app = TestApp::spawn().await;
    let chat = Chat::open_with(&app, Guest::HeldCancelAnswer).await;
    let (first_turn, first_task) = chat.send(&app, "First").await;
    dispatch(&app).await;
    let mut first = running_attempt(&app, &chat, first_task).await;
    first.accept(&app).await;
    // The next pass claims the started attempt for a follower.
    dispatch(&app).await;
    let first_exec = canonical_exec_id(first.task_id, first.attempt, first.vm_id);
    assert!(app.state.followers.is_following(&first_exec));

    let stop = stop_response(&app, &chat, first_turn);
    let drive = async {
        // The intent is durable before the guest is asked; the guest holds
        // its answer while the conversation moves on.
        wait_until!(
            "the Stop reaches the guest",
            std::time::Duration::from_secs(10),
            chat.cancels_sent().await >= 1
        );
        let (requested, _) = cancel_intent(&app, first_task).await;
        assert!(
            requested,
            "the Stop committed its intent before asking the guest"
        );
        // A message sent while the turn runs waits behind it; the turn's
        // end moves it to the successor.
        let (second_input, joined_task) = chat.send(&app, "Second").await;
        assert_eq!(
            joined_task, first_task,
            "the new message waits behind the running turn"
        );
        first
            .end(
                &app,
                json!({"status":"succeeded","result":{"status":"succeeded"}}),
            )
            .await;
        let second_task = input(&app, second_input)
            .await
            .0
            .expect("the queued message moved to the successor turn");
        assert_ne!(second_task, first_task);
        dispatch(&app).await;
        running_attempt(&app, &chat, second_task).await;
        chat.runtime.release_rpc_barrier();
        second_task
    };
    let ((status, body), second_task) = tokio::join!(stop, drive);
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data(&body)["stopped"], true, "{body}");
    assert_eq!(data(&body)["disposition"], "signalling", "{body}");

    assert_eq!(task_status(&app, first_task).await, "succeeded");
    assert_eq!(task_status(&app, second_task).await, "running");
    let (second_requested, _) = cancel_intent(&app, second_task).await;
    assert!(
        !second_requested,
        "the late Stop never reached the successor"
    );
    assert!(
        app.state
            .followers
            .wait_finished(&first_exec, std::time::Duration::from_secs(10))
            .await
    );
    let second = running_attempt(&app, &chat, second_task).await;
    let second_exec = canonical_exec_id(second.task_id, second.attempt, second.vm_id);
    // The next pass claims the successor's attempt for its own follower.
    dispatch(&app).await;
    assert!(app.state.followers.is_following(&second_exec));
    assert_eq!(
        chat.runtime.cancelled_execs().await,
        vec![first_exec],
        "only the stopped execution was cancelled"
    );
    app.cleanup().await;
}

/// A Stop whose delivery fails answers `undelivered` with its intent
/// committed; the follower, whose next read of that intent fails too, keeps
/// the read pending and delivers the cancel on its next pass. A repeated
/// Stop reports the delivered state without a second intent, and the
/// successor is never cancelled (`g5.chat.controls`).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_failed_delivery_and_a_failed_intent_read_still_end_in_cancellation() {
    let app = TestApp::spawn().await;
    let chat = Chat::open_with(&app, Guest::DroppedCancelAnswer).await;
    let (first_turn, first_task) = chat.send(&app, "First").await;
    dispatch(&app).await;
    let mut first = running_attempt(&app, &chat, first_task).await;
    first.accept(&app).await;
    // The next pass claims the started attempt for a follower.
    dispatch(&app).await;
    let first_exec = canonical_exec_id(first.task_id, first.attempt, first.vm_id);
    assert!(app.state.followers.is_following(&first_exec));
    app.state.followers.fail_control_reads(&first_exec, 1);

    let (status, body) = stop_response(&app, &chat, first_turn).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data(&body)["stopped"], true, "{body}");
    assert_eq!(data(&body)["disposition"], "undelivered", "{body}");
    let (requested, requested_at) = cancel_intent(&app, first_task).await;
    assert!(
        requested,
        "the intent is durable although the guest did not answer"
    );
    assert_eq!(
        chat.cancels_sent().await,
        5,
        "the Stop's request and its four resends all went unanswered"
    );

    // The follower's poked read failed; it re-reads on its next pass and
    // delivers the committed intent.
    wait_until!(
        "the follower delivers the cancel",
        std::time::Duration::from_secs(15),
        chat.runtime.cancelled_execs().await == vec![first_exec.clone()]
    );
    assert!(chat.cancels_sent().await >= 6);

    // A repeated click reports the delivered state and adds no intent.
    let (status, body) = stop_response(&app, &chat, first_turn).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data(&body)["stopped"], true, "{body}");
    assert_eq!(data(&body)["disposition"], "signalling", "{body}");
    assert_eq!(cancel_intent(&app, first_task).await, (true, requested_at));
    assert_eq!(task_status(&app, first_task).await, "running");

    // The successor runs untouched once the guest's outcome ends the turn.
    let (second_input, _) = chat.send(&app, "Second").await;
    first
        .end(
            &app,
            json!({"status":"succeeded","result":{"status":"succeeded"}}),
        )
        .await;
    assert_eq!(task_status(&app, first_task).await, "succeeded");
    let second_task = input(&app, second_input)
        .await
        .0
        .expect("the queued message moved to the successor turn");
    assert_ne!(second_task, first_task);
    dispatch(&app).await;
    let second = running_attempt(&app, &chat, second_task).await;
    let second_exec = canonical_exec_id(second.task_id, second.attempt, second.vm_id);
    // The next pass claims the successor's attempt for its own follower.
    dispatch(&app).await;
    assert!(
        app.state
            .followers
            .wait_finished(&first_exec, std::time::Duration::from_secs(10))
            .await
    );
    assert!(app.state.followers.is_following(&second_exec));
    // Longer than the follower's redelivery cadence: nothing reaches the successor.
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    assert_eq!(chat.runtime.cancelled_execs().await, vec![first_exec]);
    assert!(!cancel_intent(&app, second_task).await.0);
    assert_eq!(task_status(&app, second_task).await, "running");
    app.cleanup().await;
}
