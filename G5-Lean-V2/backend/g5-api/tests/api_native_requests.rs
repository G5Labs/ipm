//! Native requests over HTTP (design v9 §8.4).
//!
//! An agent's native question or permission prompt reaches the owner as a
//! `provider_requests` row journaled by the attempt, and the owner's answer
//! travels back through `POST /tasks/:id/provider-requests/:rid/reply`,
//! which accepts it durably and then delivers it to the dispatcher as
//! `modules.input.respond`. The scripted runtime peer answers that RPC with
//! `disposition: delivered`, so these tests cover the API side of the
//! contract:
//!
//! - only the current workspace owner may answer; another org member is
//!   refused even with write scope;
//! - a reply identity must be well formed, name an open request and carry a
//!   response of the request's kind;
//! - the first accepted reply is the only one: a replay with the same
//!   `requestId` and body is redelivered, a different body for the same
//!   request conflicts;
//! - an elapsed deadline or an execution that is no longer running refuses
//!   the reply;
//! - delivery is fenced on the current incarnation: the accepted reply
//!   stays durable for redelivery while a stale incarnation gets a conflict.

use crate::helpers;

use reqwest::StatusCode;
use serde_json::{json, Value};
use uuid::Uuid;

use g5_api::services::task_lifecycle::{canonical_exec_id, JournalTaskEvent, TaskLifecycleEvent};
use g5_auth::Scope;
use helpers::{
    AuthenticatedClient, TestApp, TestOrg, TestUser, TestWorkspace, TestWorkspaceRuntimePeer,
};

/// A running agent attempt bound to the attached runtime.
struct RunningAttempt {
    task_id: Uuid,
    session_id: Uuid,
    org_id: Uuid,
    vm_id: Uuid,
    next_seq: u64,
}

impl RunningAttempt {
    async fn create(
        app: &TestApp,
        workspace: &TestWorkspace,
        owner_id: Uuid,
        runtime: &TestWorkspaceRuntimePeer,
    ) -> Self {
        Self::create_for(app, workspace, owner_id, runtime, "claude_code").await
    }

    async fn create_for(
        app: &TestApp,
        workspace: &TestWorkspace,
        owner_id: Uuid,
        runtime: &TestWorkspaceRuntimePeer,
        provider: &str,
    ) -> Self {
        let snapshot = json!({"agent_launch": {"resolved": {"provider": {"value": provider}}}});
        Self::create_with_snapshot(app, workspace, owner_id, runtime, Some(snapshot)).await
    }

    /// A running attempt whose policy snapshot is `snapshot` (`None`: absent).
    async fn create_with_snapshot(
        app: &TestApp,
        workspace: &TestWorkspace,
        owner_id: Uuid,
        runtime: &TestWorkspaceRuntimePeer,
        snapshot: Option<Value>,
    ) -> Self {
        let session_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO chat_sessions (
                 id, org_id, workspace_id, scope_type, title, created_by
             ) VALUES ($1, $2, $3, 'workspace', 'Native request test', $4)",
        )
        .bind(session_id)
        .bind(workspace.org_id)
        .bind(workspace.id)
        .bind(owner_id)
        .execute(&app.db)
        .await
        .expect("insert chat session");
        let task_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO tasks (
                 id, org_id, workspace_id, name, kind, status, spec, attempt,
                 chat_session_id, created_by, vm_id, epoch, start_confirmed, started_at
             ) VALUES (
                 $1, $2, $3, 'Native request test', 'agent', 'running',
                 jsonb_build_object(
                     'agent', jsonb_build_object(
                         'schemaVersion', 'g5.agent-task-spec/v1',
                         'agentLaunch', jsonb_build_object('schemaVersion', 1)
                     )
                 ), 1, $4, $5, $6, 1, true, now()
             )",
        )
        .bind(task_id)
        .bind(workspace.org_id)
        .bind(workspace.id)
        .bind(session_id)
        .bind(owner_id)
        .bind(runtime.vm_id().to_string())
        .execute(&app.db)
        .await
        .expect("insert running agent task");
        sqlx::query(
            "INSERT INTO task_attempts(
                 org_id, task_id, attempt, vm_id, epoch, start_confirmed, started_at,
                 policy_snapshot_ref
             ) VALUES ($1,$2,1,$3,1,true,now(),$4)",
        )
        .bind(workspace.org_id)
        .bind(task_id)
        .bind(runtime.vm_id().to_string())
        .bind(snapshot)
        .execute(&app.db)
        .await
        .expect("insert running attempt");
        Self {
            task_id,
            session_id,
            org_id: workspace.org_id,
            vm_id: runtime.vm_id(),
            next_seq: 1,
        }
    }

    /// Journal a `provider_request.opened` event for `kind` with the given
    /// deadline, as the attempt's transport would.
    async fn open_request(
        &mut self,
        app: &TestApp,
        provider_request_id: &str,
        kind: &str,
        deadline_unix_ms: i64,
    ) {
        let display = match kind {
            "question" => {
                json!({"questions":[{"question":"Choose","options":[{"label":"A"},{"label":"B"}]}]})
            }
            _ => json!({"tool_name":"Bash","command":"touch allowed"}),
        };
        let seq = self.next_seq;
        self.next_seq += 1;
        let mut db = app.scoped_db_for_org(self.org_id).await;
        app.state
            .task_service
            .apply(
                &mut db,
                TaskLifecycleEvent::Journal(JournalTaskEvent {
                    task_id: self.task_id,
                    attempt: 1,
                    vm_id: self.vm_id,
                    epoch: 1,
                    exec_id: canonical_exec_id(self.task_id, 1, self.vm_id),
                    seq,
                    kind: "provider_request.opened".to_string(),
                    payload: json!({
                        "provider_request_id": provider_request_id,
                        "request_kind": kind,
                        "tool_name": (kind == "permission").then_some("Bash"),
                        "payload": display,
                        "display": display,
                        "deadline_unix_ms": deadline_unix_ms,
                    }),
                    terminal: false,
                }),
            )
            .await
            .expect("journal provider_request.opened");
        db.commit().await.expect("commit journal event");
    }

    /// Journal one event of the attempt, as its transport would.
    async fn journal(&mut self, app: &TestApp, kind: &str, payload: Value) {
        let seq = self.next_seq;
        self.next_seq += 1;
        let mut db = app.scoped_db_for_org(self.org_id).await;
        app.state
            .task_service
            .apply(
                &mut db,
                TaskLifecycleEvent::Journal(JournalTaskEvent {
                    task_id: self.task_id,
                    attempt: 1,
                    vm_id: self.vm_id,
                    epoch: 1,
                    exec_id: canonical_exec_id(self.task_id, 1, self.vm_id),
                    seq,
                    kind: kind.to_string(),
                    payload,
                    terminal: kind == "invocation.outcome",
                }),
            )
            .await
            .expect("journal event");
        db.commit().await.expect("commit journal event");
    }

    async fn reply(
        &self,
        client: &AuthenticatedClient,
        provider_request_id: &str,
        body: &Value,
    ) -> (StatusCode, Value) {
        let response = client
            .post_json(
                &format!(
                    "/api/v1/tasks/{}/provider-requests/{provider_request_id}/reply",
                    self.task_id
                ),
                body,
            )
            .await;
        let status = response.status();
        let body = response.json().await.unwrap_or(Value::Null);
        (status, body)
    }

    async fn request_state(
        &self,
        app: &TestApp,
        provider_request_id: &str,
    ) -> (String, Option<Uuid>, Option<String>) {
        sqlx::query_as(
            "SELECT status, response_author_id, response ->> 'digest'
             FROM provider_requests
             WHERE task_id = $1 AND attempt = 1 AND provider_request_id = $2",
        )
        .bind(self.task_id)
        .bind(provider_request_id)
        .fetch_one(&app.db)
        .await
        .expect("load provider request")
    }
}

fn reply_body(request_id: &str, response: Value) -> Value {
    json!({
        "attempt": 1,
        "requestId": request_id,
        "validatedResponse": response,
    })
}

fn answer() -> Value {
    json!({"g5AnswersByIndex":{"0":{"selectedOptionIndexes":[1]}}})
}

fn in_a_minute() -> i64 {
    chrono::Utc::now().timestamp_millis() + 60_000
}

/// A refused reply is a 409 whose message names the rule that refused it;
/// the status alone would not tell one guard from another.
fn expect_conflict(case: &str, (status, body): (StatusCode, Value), reason: &str) {
    assert_eq!(status, StatusCode::CONFLICT, "{case}: {body}");
    assert_eq!(body["error"]["code"], "CONFLICT", "{case}: {body}");
    let message = body["error"]["message"].as_str().unwrap_or("");
    assert!(
        message.contains(reason),
        "{case}: expected the refusal {reason:?}, got {message:?}"
    );
}

async fn respond_rpc_count(runtime: &TestWorkspaceRuntimePeer) -> usize {
    runtime
        .received_rpc_methods()
        .await
        .iter()
        .filter(|method| method.as_str() == "modules.input.respond")
        .count()
}

#[tokio::test]
async fn native_replies_are_accepted_once_from_the_owner_only() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let member = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let mut attempt = RunningAttempt::create(&app, &workspace, owner.id, &runtime).await;
    attempt
        .open_request(&app, "q-1", "question", in_a_minute())
        .await;
    let owner_client = app.authenticated_client(&owner);
    let member_client = app.authenticated_client(&member);

    let (status, body) = attempt
        .reply(&member_client, "q-1", &reply_body("member-reply", answer()))
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "member reply: {body}");
    assert_eq!(
        attempt.request_state(&app, "q-1").await.0,
        "pending",
        "a refused caller leaves the request open"
    );

    for (case, body, reason) in [
        (
            "attempt zero",
            json!({"attempt": 0, "requestId": "r", "validatedResponse": answer()}),
            "invalid native reply identity",
        ),
        (
            "empty request id",
            reply_body("", answer()),
            "invalid native reply identity",
        ),
        (
            "a permission body for a question",
            reply_body("wrong-kind", json!({"behavior":"allow"})),
            "question response must contain exactly one g5AnswersByIndex object",
        ),
        (
            "a non-object body",
            reply_body("scalar", json!("B")),
            "validated response must be an object",
        ),
        (
            "missing answer",
            reply_body("missing", json!({"g5AnswersByIndex":{}})),
            "answers must cover exactly",
        ),
        (
            "unknown question",
            reply_body(
                "unknown-question",
                json!({"g5AnswersByIndex":{"0":{"selectedOptionIndexes":[1]},"1":{"selectedOptionIndexes":[0]}}}),
            ),
            "answers must cover exactly",
        ),
        (
            "wrong question",
            reply_body(
                "wrong-question",
                json!({"g5AnswersByIndex":{"1":{"selectedOptionIndexes":[0]}}}),
            ),
            "answer question position is missing",
        ),
        (
            "nonstring free text",
            reply_body(
                "array",
                json!({"g5AnswersByIndex":{"0":{"selectedOptionIndexes":[],"otherText":["A","B"]}}}),
            ),
            "native answer must contain",
        ),
        (
            "empty answer",
            reply_body(
                "empty",
                json!({"g5AnswersByIndex":{"0":{"selectedOptionIndexes":[],"otherText":"  "}}}),
            ),
            "native answer text must be nonempty",
        ),
        (
            "out-of-bounds option",
            reply_body(
                "option",
                json!({"g5AnswersByIndex":{"0":{"selectedOptionIndexes":[2]}}}),
            ),
            "native answer option position is invalid",
        ),
        (
            "single-select mixture",
            reply_body(
                "mixture",
                json!({"g5AnswersByIndex":{"0":{"selectedOptionIndexes":[0],"otherText":"custom"}}}),
            ),
            "matching its selection mode",
        ),
    ] {
        let refused = attempt.reply(&owner_client, "q-1", &body).await;
        expect_conflict(case, refused, reason);
        assert_eq!(
            attempt.request_state(&app, "q-1").await.0,
            "pending",
            "{case}"
        );
    }
    let refused = attempt
        .reply(&owner_client, "q-unknown", &reply_body("unknown", answer()))
        .await;
    expect_conflict("unknown request", refused, "provider request is absent");
    assert_eq!(respond_rpc_count(&runtime).await, 0);

    let (status, first) = attempt
        .reply(&owner_client, "q-1", &reply_body("reply-1", answer()))
        .await;
    assert_eq!(status, StatusCode::OK, "owner reply: {first}");
    assert_eq!(first["data"]["deliveryDisposition"], "delivered");
    let digest = first["data"]["responseDigest"]
        .as_str()
        .expect("response digest")
        .to_string();
    assert_eq!(
        attempt.request_state(&app, "q-1").await,
        (
            "reply_pending".to_string(),
            Some(owner.id),
            Some(digest.clone())
        ),
        "the accepted reply is durable before the journal settles it"
    );
    assert_eq!(respond_rpc_count(&runtime).await, 1);

    // The same reply again is a redelivery of the accepted response.
    let (status, replay) = attempt
        .reply(&owner_client, "q-1", &reply_body("reply-1", answer()))
        .await;
    assert_eq!(status, StatusCode::OK, "replayed reply: {replay}");
    assert_eq!(replay["data"], first["data"]);
    assert_eq!(respond_rpc_count(&runtime).await, 2);

    // Any other reply for the request is refused: another body under the
    // same request id, or the same body under another request id.
    for (case, body) in [
        (
            "another answer",
            reply_body(
                "reply-1",
                json!({"g5AnswersByIndex":{"0":{"selectedOptionIndexes":[0]}}}),
            ),
        ),
        ("another request id", reply_body("reply-2", answer())),
    ] {
        let refused = attempt.reply(&owner_client, "q-1", &body).await;
        expect_conflict(
            case,
            refused,
            "provider request already accepted a different reply",
        );
    }
    let (status, body) = attempt
        .reply(&member_client, "q-1", &reply_body("reply-1", answer()))
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "member replay: {body}");
    assert_eq!(
        attempt.request_state(&app, "q-1").await,
        ("reply_pending".to_string(), Some(owner.id), Some(digest)),
    );
    assert_eq!(respond_rpc_count(&runtime).await, 2);

    // Free text is a native string, not constrained to the suggested options.
    attempt
        .open_request(&app, "free-text", "question", in_a_minute())
        .await;
    let free_text = json!({"g5AnswersByIndex":{"0":{"selectedOptionIndexes":[],"otherText":"Custom blue 🟦, with details"}}});
    let (status, body) = attempt
        .reply(
            &owner_client,
            "free-text",
            &reply_body("free", free_text.clone()),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "free text: {body}");
    let stored: Value = sqlx::query_scalar("SELECT response->'body' FROM provider_requests WHERE task_id = $1 AND provider_request_id = 'free-text'")
        .bind(attempt.task_id).fetch_one(&app.db).await.unwrap();
    assert_eq!(stored, free_text);
    assert_eq!(respond_rpc_count(&runtime).await, 3);

    app.cleanup().await;
}

#[tokio::test]
async fn redacted_native_question_collisions_keep_distinct_answer_positions() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let mut attempt = RunningAttempt::create(&app, &workspace, owner.id, &runtime).await;
    let question = json!({"question":"Choose [REDACTED]","options":[{"label":"[REDACTED]"},{"label":"[REDACTED]"}]});
    let display = json!({"questions":[question.clone(),question]});
    attempt.journal(&app, "provider_request.opened", json!({
        "provider_request_id":"collision", "request_kind":"question", "tool_name":"AskUserQuestion",
        "payload":display, "display":display, "deadline_unix_ms":in_a_minute(),
    })).await;
    let client = app.authenticated_client(&owner);
    expect_conflict(
        "incomplete colliding questions",
        attempt
            .reply(&client, "collision", &reply_body("missing", answer()))
            .await,
        "answers must cover exactly",
    );
    assert_eq!(attempt.request_state(&app, "collision").await.0, "pending");
    assert_eq!(respond_rpc_count(&runtime).await, 0);
    let answers = json!({"g5AnswersByIndex":{
        "0":{"selectedOptionIndexes":[1]},
        "1":{"selectedOptionIndexes":[],"otherText":"custom note"},
    }});
    let (status, body) = attempt
        .reply(&client, "collision", &reply_body("both", answers.clone()))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let stored: Value = sqlx::query_scalar("SELECT response->'body' FROM provider_requests WHERE task_id = $1 AND provider_request_id = 'collision'")
        .bind(attempt.task_id).fetch_one(&app.db).await.unwrap();
    assert_eq!(stored, answers);
    assert_eq!(respond_rpc_count(&runtime).await, 1);
    app.cleanup().await;
}

async fn end_wait_rpc_count(runtime: &TestWorkspaceRuntimePeer) -> usize {
    runtime
        .received_rpc_methods()
        .await
        .iter()
        .filter(|method| method.as_str() == "modules.background.end")
        .count()
}

/// The owner's End for a background wait reaches the session's running
/// attempt through `modules.background.end`; nobody else can send it and a
/// session without a running attempt refuses it.
#[tokio::test]
async fn owner_end_wait_reaches_only_the_running_attempt() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let member = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let attempt = RunningAttempt::create(&app, &workspace, owner.id, &runtime).await;
    let path = format!("/api/v1/sessions/{}/background/end", attempt.session_id);
    let this_wait = json!({"taskId": attempt.task_id, "attempt": 1});

    let refused = app
        .authenticated_client(&member)
        .post_json(&path, &this_wait)
        .await;
    assert!(
        matches!(
            refused.status(),
            StatusCode::FORBIDDEN | StatusCode::NOT_FOUND
        ),
        "member end: {}",
        refused.status()
    );
    assert_eq!(end_wait_rpc_count(&runtime).await, 0);

    let owner_client = app.authenticated_client(&owner);
    // An End for a wait the owner saw on an earlier attempt, or on another
    // task, never reaches the running attempt.
    for stale in [
        json!({"taskId": attempt.task_id, "attempt": 2}),
        json!({"taskId": Uuid::new_v4(), "attempt": 1}),
    ] {
        let response = owner_client.post_json(&path, &stale).await;
        expect_conflict(
            "stale wait",
            (
                response.status(),
                response.json().await.unwrap_or(Value::Null),
            ),
            "that background wait has already ended",
        );
    }
    assert_eq!(end_wait_rpc_count(&runtime).await, 0);
    let ended = owner_client.post_json(&path, &this_wait).await;
    let status = ended.status();
    let body: Value = ended.json().await.unwrap_or(Value::Null);
    assert_eq!(status, StatusCode::OK, "owner end: {body}");
    assert_eq!(body["data"]["delivered"], true, "{body}");
    assert_eq!(end_wait_rpc_count(&runtime).await, 1);
    // End changes no durable task state: the attempt keeps running until
    // its own outcome settles it.
    let task_status: String = sqlx::query_scalar("SELECT status FROM tasks WHERE id = $1")
        .bind(attempt.task_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(task_status, "running");

    // Codex attempts have no background window to end.
    let codex_attempt =
        RunningAttempt::create_for(&app, &workspace, owner.id, &runtime, "codex").await;
    let codex = owner_client
        .post_json(
            &format!(
                "/api/v1/sessions/{}/background/end",
                codex_attempt.session_id
            ),
            &json!({"taskId": codex_attempt.task_id, "attempt": 1}),
        )
        .await;
    expect_conflict(
        "codex attempt",
        (codex.status(), codex.json().await.unwrap_or(Value::Null)),
        "the running agent attempt has no background wait to end",
    );
    assert_eq!(end_wait_rpc_count(&runtime).await, 1);

    // Neither does an attempt whose snapshot does not name Claude.
    let unnamed_attempt =
        RunningAttempt::create_with_snapshot(&app, &workspace, owner.id, &runtime, None).await;
    let unnamed = owner_client
        .post_json(
            &format!(
                "/api/v1/sessions/{}/background/end",
                unnamed_attempt.session_id
            ),
            &json!({"taskId": unnamed_attempt.task_id, "attempt": 1}),
        )
        .await;
    expect_conflict(
        "attempt without a provider",
        (
            unnamed.status(),
            unnamed.json().await.unwrap_or(Value::Null),
        ),
        "the running agent attempt has no background wait to end",
    );
    assert_eq!(end_wait_rpc_count(&runtime).await, 1);

    sqlx::query("UPDATE tasks SET status = 'succeeded', ended_at = now() WHERE id = $1")
        .bind(attempt.task_id)
        .execute(&app.db)
        .await
        .unwrap();
    let late = owner_client.post_json(&path, &this_wait).await;
    expect_conflict(
        "no running attempt",
        (late.status(), late.json().await.unwrap_or(Value::Null)),
        "session has no running agent attempt",
    );
    assert_eq!(end_wait_rpc_count(&runtime).await, 1);

    app.cleanup().await;
}

/// The session's live task carries Claude's current background set, so a
/// reload during a long wait keeps the waiting panel even after the set's
/// own row has left the newest timeline page.
#[tokio::test]
async fn session_task_carries_the_live_background_set() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let mut attempt = RunningAttempt::create(&app, &workspace, owner.id, &runtime).await;
    let client = app.authenticated_client(&owner);
    let task_path = format!("/api/v1/sessions/{}/task", attempt.session_id);
    let live_wait = |body: Value| body["data"]["backgroundWait"].clone();

    let body: Value = client.get(&task_path).await.json().await.unwrap();
    assert_eq!(live_wait(body), Value::Null, "no background work yet");

    attempt
        .journal(
            &app,
            "background_tasks.replaced",
            json!({
                "tasks_ref": "sha256:00",
                "tasks": [{"task_id":"bg1","kind":"shell","description":"docker-ci","started_at_unix_ms":1_000}],
                "count": 1,
                "truncated": false,
                "wait_started_at_unix_ms": 1_000,
                "content": "Waiting on 1 background task",
            }),
        )
        .await;
    for index in 0..210 {
        attempt
            .journal(
                &app,
                "subtask.progress",
                json!({"task_id":"bg1","progress_ref":"sha256:01","description":format!("step {index}"),"content":"step"}),
            )
            .await;
    }
    let body: Value = client.get(&task_path).await.json().await.unwrap();
    let wait = live_wait(body);
    assert_eq!(wait["count"], 1, "{wait}");
    assert_eq!(wait["tasks"][0]["description"], "docker-ci");
    assert_eq!(wait["wait_started_at_unix_ms"], 1_000);

    attempt
        .journal(
            &app,
            "background_tasks.replaced",
            json!({"tasks_ref":"sha256:02","tasks":[],"count":0,"truncated":false,"content":"No background work remains"}),
        )
        .await;
    let body: Value = client.get(&task_path).await.json().await.unwrap();
    assert_eq!(live_wait(body), Value::Null, "an emptied set ends the wait");

    app.cleanup().await;
}

#[tokio::test]
async fn native_replies_need_an_open_deadline_and_a_running_execution() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let mut attempt = RunningAttempt::create(&app, &workspace, owner.id, &runtime).await;
    let owner_client = app.authenticated_client(&owner);

    attempt
        .open_request(
            &app,
            "late",
            "permission",
            chrono::Utc::now().timestamp_millis() - 1_000,
        )
        .await;
    let refused = attempt
        .reply(
            &owner_client,
            "late",
            &reply_body("late-reply", json!({"behavior":"allow"})),
        )
        .await;
    expect_conflict(
        "elapsed deadline",
        refused,
        "provider request response deadline elapsed",
    );
    assert_eq!(attempt.request_state(&app, "late").await.0, "pending");

    attempt
        .open_request(&app, "open", "permission", in_a_minute())
        .await;
    sqlx::query("UPDATE tasks SET status = 'waiting' WHERE id = $1")
        .bind(attempt.task_id)
        .execute(&app.db)
        .await
        .expect("park the execution");
    let refused = attempt
        .reply(
            &owner_client,
            "open",
            &reply_body("open-reply", json!({"behavior":"allow"})),
        )
        .await;
    expect_conflict(
        "execution not running",
        refused,
        "provider request execution is not running",
    );
    assert_eq!(attempt.request_state(&app, "open").await.0, "pending");
    assert_eq!(respond_rpc_count(&runtime).await, 0);

    sqlx::query("UPDATE tasks SET status = 'running' WHERE id = $1")
        .bind(attempt.task_id)
        .execute(&app.db)
        .await
        .expect("resume the execution");
    let (status, body) = attempt
        .reply(
            &owner_client,
            "open",
            &reply_body("open-reply", json!({"behavior":"allow"})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "running again: {body}");
    assert_eq!(body["data"]["deliveryDisposition"], "delivered");
    assert_eq!(respond_rpc_count(&runtime).await, 1);

    app.cleanup().await;
}

#[tokio::test]
async fn native_reply_delivery_is_fenced_on_the_current_incarnation() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let mut attempt = RunningAttempt::create(&app, &workspace, owner.id, &runtime).await;
    let owner_client = app.authenticated_client(&owner);
    attempt
        .open_request(&app, "q-1", "question", in_a_minute())
        .await;

    let (status, accepted) = attempt
        .reply(&owner_client, "q-1", &reply_body("reply-1", answer()))
        .await;
    assert_eq!(status, StatusCode::OK, "accepted: {accepted}");
    let before = attempt.request_state(&app, "q-1").await;
    assert_eq!(before.0, "reply_pending");

    // The workspace VM moves on (a stop/start cycle bumps the epoch) while the
    // attempt still names the old incarnation.
    sqlx::query("UPDATE workspace_vms SET epoch = epoch + 1 WHERE workspace_id = $1")
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .expect("advance the incarnation");
    let refused = attempt
        .reply(&owner_client, "q-1", &reply_body("reply-1", answer()))
        .await;
    expect_conflict(
        "stale incarnation",
        refused,
        "provider request execution is stale",
    );
    assert_eq!(
        attempt.request_state(&app, "q-1").await,
        before,
        "the accepted reply stays durable for the reconciler to redeliver"
    );
    assert_eq!(
        respond_rpc_count(&runtime).await,
        1,
        "nothing is sent to a VM the attempt does not belong to"
    );

    app.cleanup().await;
}

/// The original question survives settlement and a replacement incarnation.
/// Answer-before-harvest and answer-after-reattachment use the same immutable row.
#[tokio::test]
async fn deferred_native_question_waits_without_expiry_and_reserves_its_exact_answer() {
    for scenario in [
        "restored",
        "saving",
        "cancel",
        "transfer",
        "retry",
        "unknown",
        "restore_failed",
    ] {
        let answer_while_saving = scenario == "saving";
        let provider_cancels = scenario == "cancel";
        let app = TestApp::spawn().await;
        let mut owner = TestUser::create(&app).await;
        let org = TestOrg::create_with_admin(&app, &mut owner).await;
        let workspace = TestWorkspace::create(&app, &org).await;
        let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
        let mut attempt = RunningAttempt::create(&app, &workspace, owner.id, &runtime).await;
        let native_session = Uuid::new_v4().to_string();
        attempt
            .journal(
                &app,
                "provider.session",
                json!({"provider_id":"claude_code", "provider_session_id":native_session}),
            )
            .await;
        let display =
            json!({"questions":[{"question":"Choose","options":[{"label":"A"},{"label":"B"}]}]});
        attempt.journal(&app, "provider_request.opened", json!({
            "provider_request_id":"toolu-deferred", "request_kind":"question", "tool_name":"AskUserQuestion",
            "payload":display, "display":display, "deadline_unix_ms":null, "resume_session_id":native_session,
        })).await;
        let mut db = app.scoped_db_for_org(org.id).await;
        assert!(app
            .state
            .task_service
            .claim_expired_native_requests(&mut db, 10)
            .await
            .unwrap()
            .is_empty());
        db.commit().await.unwrap();
        let client = app.authenticated_client(&owner);
        let body = reply_body("durable-owner-answer", answer());
        if answer_while_saving {
            let (status, result) = attempt.reply(&client, "toolu-deferred", &body).await;
            assert_eq!(status, StatusCode::OK, "{result}");
            assert_eq!(result["data"]["deliveryDisposition"], "queued");
        }
        attempt.journal(&app, "invocation.outcome", json!({
            "status":"succeeded", "result":{"schema_version":"g5.agent.turn.outcome/v1",
                "status":"suspended","code":"native_question_deferred", "retryable":false,
                "provider_session_id":native_session,"session_files_state":"present","session_manifest_generation":1}
        })).await;
        let state: (String, Option<String>) =
            sqlx::query_as("SELECT status, hold_reason FROM tasks WHERE id = $1")
                .bind(attempt.task_id)
                .fetch_one(&app.db)
                .await
                .unwrap();
        if !answer_while_saving {
            assert_eq!(state, ("waiting".into(), Some("awaiting_answer".into())));
            assert_eq!(
                attempt.request_state(&app, "toolu-deferred").await.0,
                "pending"
            );
            // Stop/reattach changes only execution authority; the old attempt and question stay immutable.
            sqlx::query("UPDATE workspace_vms SET status = 'stopping', lease_expires_at = NULL WHERE workspace_id = $1")
                .bind(workspace.id).execute(&app.db).await.unwrap();
            let refused = attempt.reply(&client, "toolu-deferred", &body).await;
            expect_conflict("no attached compute", refused, "attach ready compute");
            sqlx::query(
                "UPDATE workspace_vms SET status = 'running', vm_id = $2, epoch = epoch + 1,
                lease_expires_at = now() + interval '1 hour' WHERE workspace_id = $1",
            )
            .bind(workspace.id)
            .bind(Uuid::new_v4().to_string())
            .execute(&app.db)
            .await
            .unwrap();
            let (status, result) = attempt.reply(&client, "toolu-deferred", &body).await;
            assert_eq!(status, StatusCode::OK, "{result}");
            assert_eq!(result["data"]["deliveryDisposition"], "queued");
        } else {
            assert_eq!(state, ("queued".into(), None));
        }
        // Losing the HTTP acknowledgement does not require a live VM to
        // confirm the already-frozen answer, and must not dispatch it twice.
        sqlx::query("UPDATE workspace_vms SET status = 'stopping', lease_expires_at = NULL WHERE workspace_id = $1")
            .bind(workspace.id).execute(&app.db).await.unwrap();
        let (status, retry) = attempt.reply(&client, "toolu-deferred", &body).await;
        assert_eq!(status, StatusCode::OK, "{retry}");
        assert_eq!(retry["data"]["deliveryDisposition"], "queued");
        let rejected = attempt
            .reply(
                &client,
                "toolu-deferred",
                &reply_body("competing", answer()),
            )
            .await;
        expect_conflict("immutable first answer", rejected, "different reply");
        sqlx::query("UPDATE workspace_vms SET status = 'running', lease_expires_at = now() + interval '1 hour' WHERE workspace_id = $1")
            .bind(workspace.id).execute(&app.db).await.unwrap();
        assert_eq!(
            respond_rpc_count(&runtime).await,
            0,
            "old process must never receive this answer"
        );
        if scenario == "transfer" {
            let new_owner = TestUser::create(&app).await;
            sqlx::query(
                "INSERT INTO org_memberships (org_id, user_id, role) VALUES ($1,$2,'member')",
            )
            .bind(org.id)
            .bind(new_owner.id)
            .execute(&app.db)
            .await
            .unwrap();
            sqlx::query("UPDATE workspaces SET owner_user_id = $2 WHERE id = $1")
                .bind(workspace.id)
                .bind(new_owner.id)
                .execute(&app.db)
                .await
                .unwrap();
            let mut db = app.scoped_db_for_org(org.id).await;
            assert!(
                app.state
                    .task_service
                    .reserve_agent_dispatch(&mut db)
                    .await
                    .unwrap()
                    .is_none(),
                "a previous owner's accepted answer must not reserve native delivery"
            );
            db.commit().await.unwrap();
            let status: String = sqlx::query_scalar("SELECT status FROM tasks WHERE id = $1")
                .bind(attempt.task_id)
                .fetch_one(&app.db)
                .await
                .unwrap();
            assert_eq!(
                status, "failed",
                "stale author is a visible admission failure"
            );
            assert_eq!(
                attempt.request_state(&app, "toolu-deferred").await.0,
                "invalidated"
            );
            app.cleanup().await;
            continue;
        }
        let mut db = app.scoped_db_for_org(org.id).await;
        let dispatch = app
            .state
            .task_service
            .reserve_agent_dispatch(&mut db)
            .await
            .unwrap()
            .expect("native resume reservation");
        db.commit().await.unwrap();
        let input = &dispatch.invoke_params["input"];
        assert_eq!(input["task"]["attempt"], 2);
        assert_eq!(input["prompt"]["text"], "");
        assert_eq!(input["resume_prompt"]["text"], "");
        assert_eq!(input["native_question_reply"]["source_attempt"], 1);
        assert_eq!(
            input["native_question_reply"]["provider_request_id"],
            "toolu-deferred"
        );
        assert_eq!(
            input["native_question_reply"]["provider_session_id"],
            native_session
        );
        assert_eq!(
            input["native_question_reply"]["validated_response"],
            answer()
        );
        if scenario == "restore_failed" {
            // Attempt 2 carried the frozen answer but its resumed session did
            // not reissue the question. It ends `native_question_restore_failed`
            // with no transport-write; TaskService labels the request, keeps
            // the accepted answer, and an identical retry reports it
            // undeliverable (`g5.chat.native-question-recovery`).
            let mut db = app.scoped_db_for_org(org.id).await;
            app.state.task_service.apply(&mut db, TaskLifecycleEvent::Journal(JournalTaskEvent {
                task_id:attempt.task_id, attempt:2, vm_id:dispatch.vm_id, epoch:dispatch.epoch,
                exec_id:canonical_exec_id(attempt.task_id,2,dispatch.vm_id), seq:1,
                kind:"invocation.outcome".into(), terminal:true,
                payload:json!({"status":"succeeded", "result":{"schema_version":"g5.agent.turn.outcome/v1",
                    "status":"failed","code":"native_question_restore_failed","retryable":false,
                    "provider_session_id":native_session,"session_files_state":"present"}}),
            })).await.unwrap();
            db.commit().await.unwrap();
            let (task_status, hold): (String, Option<String>) =
                sqlx::query_as("SELECT status, hold_reason FROM tasks WHERE id = $1")
                    .bind(attempt.task_id)
                    .fetch_one(&app.db)
                    .await
                    .unwrap();
            assert_eq!(task_status, "failed", "restore failure ends the attempt");
            assert_eq!(hold, None);
            let (request_status, _author, _digest) =
                attempt.request_state(&app, "toolu-deferred").await;
            assert_eq!(request_status, "invalidated");
            let end_reason: Option<String> = sqlx::query_scalar(
                "SELECT end_reason FROM provider_requests WHERE task_id = $1 AND provider_request_id = $2",
            )
            .bind(attempt.task_id)
            .bind("toolu-deferred")
            .fetch_one(&app.db)
            .await
            .unwrap();
            assert_eq!(
                end_reason.as_deref(),
                Some("native_question_restore_failed")
            );
            // The accepted answer stays recorded; an identical retry is
            // reported undeliverable, never newly queued.
            let (status, retry) = attempt.reply(&client, "toolu-deferred", &body).await;
            assert_eq!(status, StatusCode::OK, "{retry}");
            assert_eq!(retry["data"]["deliveryDisposition"], "undeliverable");
            app.cleanup().await;
            continue;
        }
        if matches!(scenario, "retry" | "unknown") {
            let uncertain = scenario == "unknown";
            let mut db = app.scoped_db_for_org(org.id).await;
            app.state.task_service.apply(&mut db, TaskLifecycleEvent::Journal(JournalTaskEvent {
                task_id:attempt.task_id, attempt:2, vm_id:dispatch.vm_id, epoch:dispatch.epoch,
                exec_id:canonical_exec_id(attempt.task_id,2,dispatch.vm_id), seq:1,
                kind:"invocation.outcome".into(), terminal:true,
                payload:json!({"status":"succeeded", "result":{"schema_version":"g5.agent.turn.outcome/v1",
                    "status":"suspended","code":"native_response_undeliverable","retryable":false,
                    "provider_session_id":native_session,"session_files_state":"present",
                    "input_delivery_unknown":uncertain}}),
            })).await.unwrap();
            db.commit().await.unwrap();
            let response = client
                .post_json(
                    &format!("/api/v1/tasks/{}/continue", attempt.task_id),
                    &json!({}),
                )
                .await;
            assert_eq!(
                response.status(),
                if uncertain {
                    StatusCode::CONFLICT
                } else {
                    StatusCode::OK
                },
                "scenario {scenario}"
            );
            let hold: Option<String> =
                sqlx::query_scalar("SELECT hold_reason FROM tasks WHERE id = $1")
                    .bind(attempt.task_id)
                    .fetch_one(&app.db)
                    .await
                    .unwrap();
            assert_eq!(
                hold.as_deref(),
                if uncertain {
                    Some("delivery_unknown")
                } else {
                    None
                }
            );
            if !uncertain {
                let mut db = app.scoped_db_for_org(org.id).await;
                let retry = app
                    .state
                    .task_service
                    .reserve_agent_dispatch(&mut db)
                    .await
                    .unwrap()
                    .unwrap();
                assert_eq!(retry.invoke_params["input"]["task"]["attempt"], 3);
                assert_eq!(
                    retry.invoke_params["input"]["native_question_reply"],
                    input["native_question_reply"]
                );
                assert_eq!(retry.invoke_params["input"]["prompt"]["text"], "");
                db.commit().await.unwrap();
            }
            app.cleanup().await;
            continue;
        }
        let mut db = app.scoped_db_for_org(org.id).await;
        let evidence = JournalTaskEvent {
            task_id: attempt.task_id,
            attempt: 2,
            vm_id: dispatch.vm_id,
            epoch: dispatch.epoch,
            exec_id: canonical_exec_id(attempt.task_id, 2, dispatch.vm_id),
            seq: 1,
            kind: "provider_request.replied".into(),
            terminal: false,
            payload: json!({"provider_request_id":"toolu-deferred", "source_attempt":1,
                "request_id":"durable-owner-answer", "response_digest":input["native_question_reply"]["response_digest"],
                "disposition":"answered", "evidence":"transport_write", "input_acceptance":input["task"]["initial_input"]}),
        };
        if provider_cancels {
            let cancelled = JournalTaskEvent {
                kind: "provider_request.invalidated".into(),
                payload: json!({"provider_request_id":"toolu-deferred", "source_attempt":1,
                    "reason":"provider_cancelled"}),
                ..evidence
            };
            app.state
                .task_service
                .apply(&mut db, TaskLifecycleEvent::Journal(cancelled))
                .await
                .unwrap();
            db.commit().await.unwrap();
            assert_eq!(
                attempt.request_state(&app, "toolu-deferred").await.0,
                "invalidated"
            );
            app.cleanup().await;
            continue;
        }
        // Delivery evidence whose input acceptance is missing or does not
        // match the frozen reservation is state inconsistency, not a
        // rejectable provider fact: it must stall unconsumed (nothing
        // demoted, nothing acknowledged), and the correct fact must still
        // apply afterwards.
        let mut tampered = evidence.clone();
        tampered.payload["input_acceptance"]["reservation_id"] = json!(Uuid::new_v4());
        app.state
            .task_service
            .apply(&mut db, TaskLifecycleEvent::Journal(tampered))
            .await
            .expect_err("a reservation mismatch must stall, not consume");
        drop(db);
        let mut db = app.scoped_db_for_org(org.id).await;
        let mut stripped = evidence.clone();
        stripped
            .payload
            .as_object_mut()
            .unwrap()
            .remove("input_acceptance");
        app.state
            .task_service
            .apply(&mut db, TaskLifecycleEvent::Journal(stripped))
            .await
            .expect_err("absent acceptance evidence must stall, not consume");
        drop(db);
        assert_eq!(
            attempt.request_state(&app, "toolu-deferred").await.0,
            "reply_pending",
            "the stalled facts must leave the accepted answer untouched"
        );
        let mut db = app.scoped_db_for_org(org.id).await;
        app.state
            .task_service
            .apply(&mut db, TaskLifecycleEvent::Journal(evidence))
            .await
            .unwrap();
        db.commit().await.unwrap();
        assert_eq!(
            attempt.request_state(&app, "toolu-deferred").await.0,
            "responded"
        );
        let delivery: String =
            sqlx::query_scalar("SELECT delivery FROM session_inputs WHERE id = $1")
                .bind(
                    Uuid::parse_str(input["task"]["initial_input"]["input_id"].as_str().unwrap())
                        .unwrap(),
                )
                .fetch_one(&app.db)
                .await
                .unwrap();
        assert_eq!(
            delivery, "delivered",
            "one native write atomically settles the reservation"
        );
        // A lost HTTP response may be retried after native delivery and final completion.
        sqlx::query("UPDATE tasks SET status = 'succeeded', ended_at = now() WHERE id = $1")
            .bind(attempt.task_id)
            .execute(&app.db)
            .await
            .unwrap();
        let (status, retry) = attempt.reply(&client, "toolu-deferred", &body).await;
        assert_eq!(status, StatusCode::OK, "{retry}");
        assert_eq!(retry["data"]["deliveryDisposition"], "already_delivered");
        let timeline = client
            .get(&format!("/api/v1/tasks/{}/timeline", attempt.task_id))
            .await;
        assert_eq!(timeline.status(), StatusCode::OK);
        let timeline: Value = timeline.json().await.unwrap();
        assert!(timeline["data"]["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(
                |entry| entry["metadata"]["kind"] == "provider_request.opened"
                    && entry["metadata"]["nativeRequestStatus"] == "responded"
            ));
        app.cleanup().await;
    }
}

async fn journal_evidence(app: &TestApp, task_id: Uuid, seq: i64) -> (String, String, i64) {
    let (entry_type, content): (String, String) = sqlx::query_as(
        "SELECT entry_type, content FROM task_events
         WHERE task_id = $1 AND attempt = 1 AND seq = $2",
    )
    .bind(task_id)
    .bind(seq)
    .fetch_one(&app.db)
    .await
    .expect("load journal evidence row");
    let acked: i64 = sqlx::query_scalar(
        "SELECT acked_event_seq FROM task_attempts WHERE task_id = $1 AND attempt = 1",
    )
    .bind(task_id)
    .fetch_one(&app.db)
    .await
    .expect("load acknowledgement watermark");
    (entry_type, content, acked)
}

/// H2: the provider reopens a settled request id. The rejected fact becomes
/// bounded error evidence, consumes its sequence, and the terminal outcome
/// behind it still settles the attempt instead of wedging every replay.
#[tokio::test]
async fn reused_provider_request_id_is_consumed_as_error_evidence() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let mut attempt = RunningAttempt::create(&app, &workspace, owner.id, &runtime).await;
    let owner_client = app.authenticated_client(&owner);

    attempt
        .open_request(&app, "req-1", "permission", in_a_minute())
        .await;
    let (status, accepted) = attempt
        .reply(
            &owner_client,
            "req-1",
            &reply_body("first-reply", json!({"behavior":"allow"})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{accepted}");
    attempt
        .journal(
            &app,
            "provider_request.replied",
            json!({
                "provider_request_id":"req-1",
                "request_id":"first-reply",
                "response_digest":accepted["data"]["responseDigest"],
                "disposition":"allowed",
                "evidence":"transport_write",
            }),
        )
        .await;
    assert_eq!(attempt.request_state(&app, "req-1").await.0, "responded");

    attempt
        .open_request(&app, "req-1", "permission", in_a_minute())
        .await;
    let (entry_type, content, acked) = journal_evidence(&app, attempt.task_id, 3).await;
    assert_eq!(entry_type, "error");
    assert!(content.contains("already used"), "{content}");
    assert_eq!(acked, 3, "the rejected fact must consume its sequence");
    let rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM provider_requests
         WHERE task_id = $1 AND provider_request_id = 'req-1'",
    )
    .bind(attempt.task_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(rows, 1, "distinct uses must not merge into one request");

    attempt
        .journal(
            &app,
            "invocation.outcome",
            json!({
                "status":"succeeded",
                "result":{"schema_version":"g5.agent.turn.outcome/v1",
                    "status":"succeeded","retryable":false}
            }),
        )
        .await;
    let settled: (String, Option<chrono::DateTime<chrono::Utc>>) = sqlx::query_as(
        "SELECT task.status, attempt.ended_at FROM tasks AS task
         JOIN task_attempts AS attempt ON attempt.task_id = task.id AND attempt.attempt = 1
         WHERE task.id = $1",
    )
    .bind(attempt.task_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(settled.0, "succeeded");
    assert!(
        settled.1.is_some(),
        "the terminal outcome must still settle"
    );

    app.cleanup().await;
}

/// Sibling malformed identities: untrimmed and overlong ids are consumed as
/// evidence, a sanitization collision cannot merge two distinct raw ids into
/// one request, and a valid request afterwards keeps its full guarantees.
#[tokio::test]
async fn malformed_native_identities_are_consumed_without_merging() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let mut attempt = RunningAttempt::create(&app, &workspace, owner.id, &runtime).await;
    let owner_client = app.authenticated_client(&owner);

    attempt
        .open_request(&app, " padded ", "permission", in_a_minute())
        .await;
    let (entry_type, content, _) = journal_evidence(&app, attempt.task_id, 1).await;
    assert_eq!(entry_type, "error");
    assert!(content.contains("provider_request_id"), "{content}");

    attempt
        .open_request(&app, &"x".repeat(513), "permission", in_a_minute())
        .await;
    let (entry_type, _, _) = journal_evidence(&app, attempt.task_id, 2).await;
    assert_eq!(entry_type, "error");

    // Storage sanitization maps U+0000 to U+FFFD; the raw twin that collides
    // with the stored id is rejected rather than silently becoming one row.
    attempt
        .open_request(&app, "nul\u{0}id", "permission", in_a_minute())
        .await;
    attempt
        .open_request(&app, "nul\u{FFFD}id", "permission", in_a_minute())
        .await;
    let (entry_type, content, acked) = journal_evidence(&app, attempt.task_id, 4).await;
    assert_eq!(entry_type, "error");
    assert!(content.contains("already used"), "{content}");
    assert_eq!(acked, 4);
    let rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM provider_requests
         WHERE task_id = $1 AND provider_request_id = 'nul\u{FFFD}id'",
    )
    .bind(attempt.task_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(rows, 1);

    attempt
        .open_request(&app, "req-ok", "permission", in_a_minute())
        .await;
    let (status, body) = attempt
        .reply(
            &owner_client,
            "req-ok",
            &reply_body("ok-reply", json!({"behavior":"allow"})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["deliveryDisposition"], "delivered");

    app.cleanup().await;
}

/// D6: after owner authorization an identical accepted answer replays from
/// its stored key, body and digest — after response settlement, after the
/// invalidation that ends an attempt, and after the task completes — without
/// reopening the request. Changed answers stay rejected.
#[tokio::test]
async fn identical_native_reply_replays_after_settlement_and_task_end() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let mut attempt = RunningAttempt::create(&app, &workspace, owner.id, &runtime).await;
    let owner_client = app.authenticated_client(&owner);

    attempt
        .open_request(&app, "perm-1", "permission", in_a_minute())
        .await;
    let allow = reply_body("reply-1", json!({"behavior":"allow"}));
    let (status, first) = attempt.reply(&owner_client, "perm-1", &allow).await;
    assert_eq!(status, StatusCode::OK, "{first}");
    assert_eq!(first["data"]["deliveryDisposition"], "delivered");

    attempt
        .journal(
            &app,
            "provider_request.replied",
            json!({
                "provider_request_id":"perm-1",
                "request_id":"reply-1",
                "response_digest":first["data"]["responseDigest"],
                "disposition":"allowed",
                "evidence":"transport_write",
            }),
        )
        .await;
    assert_eq!(attempt.request_state(&app, "perm-1").await.0, "responded");
    let (status, replayed) = attempt.reply(&owner_client, "perm-1", &allow).await;
    assert_eq!(status, StatusCode::OK, "{replayed}");
    assert_eq!(replayed["data"]["deliveryDisposition"], "already_delivered");
    assert_eq!(
        respond_rpc_count(&runtime).await,
        1,
        "a settled request is never redelivered"
    );
    let changed = attempt
        .reply(
            &owner_client,
            "perm-1",
            &reply_body("reply-2", json!({"behavior":"deny"})),
        )
        .await;
    expect_conflict(
        "changed answer after settlement",
        changed,
        "different reply",
    );

    attempt
        .open_request(&app, "perm-2", "permission", in_a_minute())
        .await;
    let deny = reply_body("reply-3", json!({"behavior":"deny"}));
    let (status, second) = attempt.reply(&owner_client, "perm-2", &deny).await;
    assert_eq!(status, StatusCode::OK, "{second}");
    assert_eq!(respond_rpc_count(&runtime).await, 2);
    attempt
        .journal(
            &app,
            "invocation.outcome",
            json!({
                "status":"succeeded",
                "result":{"schema_version":"g5.agent.turn.outcome/v1",
                    "status":"succeeded","retryable":false}
            }),
        )
        .await;
    let task_status: String = sqlx::query_scalar("SELECT status FROM tasks WHERE id = $1")
        .bind(attempt.task_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(task_status, "succeeded");
    assert_eq!(
        attempt.request_state(&app, "perm-2").await.0,
        "invalidated",
        "settlement invalidates the accepted-but-undelivered reply"
    );

    // Identical retries after the execution and task ended replay the frozen
    // acceptance; the request is not reopened and nothing reaches a VM.
    let (status, after_end) = attempt.reply(&owner_client, "perm-2", &deny).await;
    assert_eq!(status, StatusCode::OK, "{after_end}");
    assert_eq!(after_end["data"]["deliveryDisposition"], "queued");
    assert_eq!(
        after_end["data"]["responseDigest"],
        second["data"]["responseDigest"]
    );
    let (status, settled_retry) = attempt.reply(&owner_client, "perm-1", &allow).await;
    assert_eq!(status, StatusCode::OK, "{settled_retry}");
    assert_eq!(
        settled_retry["data"]["deliveryDisposition"],
        "already_delivered"
    );
    assert_eq!(respond_rpc_count(&runtime).await, 2);
    assert_eq!(attempt.request_state(&app, "perm-2").await.0, "invalidated");
    let changed = attempt
        .reply(
            &owner_client,
            "perm-2",
            &reply_body("reply-4", json!({"behavior":"allow"})),
        )
        .await;
    expect_conflict("changed answer after task end", changed, "different reply");

    app.cleanup().await;
}

/// S29: the owner's pause invalidates a deferred question before its
/// journaled suspension applies. The task must hold for the owner — nothing
/// can answer the invalidated question — and Continue releases it.
#[tokio::test]
async fn pause_invalidated_deferred_question_holds_for_the_owner() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let mut attempt = RunningAttempt::create(&app, &workspace, owner.id, &runtime).await;
    let native_session = Uuid::new_v4().to_string();
    attempt
        .journal(
            &app,
            "provider.session",
            json!({"provider_id":"claude_code","provider_session_id":native_session}),
        )
        .await;
    let display =
        json!({"questions":[{"question":"Choose","options":[{"label":"A"},{"label":"B"}]}]});
    attempt
        .journal(
            &app,
            "provider_request.opened",
            json!({
                "provider_request_id":"toolu-paused","request_kind":"question",
                "tool_name":"AskUserQuestion","payload":display,"display":display,
                "deadline_unix_ms":null,"resume_session_id":native_session,
            }),
        )
        .await;

    let mut db = app.scoped_db_for_org(org.id).await;
    let paused = app
        .state
        .task_service
        .request_workspace_task_pause(&mut db, owner.id, attempt.task_id)
        .await
        .expect("pause the running task");
    db.commit().await.unwrap();
    assert_eq!(paused.status, "pausing");
    assert_eq!(
        attempt.request_state(&app, "toolu-paused").await.0,
        "invalidated"
    );

    attempt
        .journal(
            &app,
            "invocation.outcome",
            json!({
                "status":"succeeded",
                "result":{"schema_version":"g5.agent.turn.outcome/v1",
                    "status":"suspended","code":"native_question_deferred","retryable":false,
                    "provider_session_id":native_session,"session_files_state":"present",
                    "session_manifest_generation":1}
            }),
        )
        .await;
    let state: (String, Option<String>) =
        sqlx::query_as("SELECT status, hold_reason FROM tasks WHERE id = $1")
            .bind(attempt.task_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(
        state,
        ("waiting".into(), Some("owner_stop".into())),
        "no live question remains, so the hold must be the owner's"
    );

    let mut db = app.scoped_db_for_org(org.id).await;
    let resumed = app
        .state
        .task_service
        .continue_workspace_task(&mut db, owner.id, attempt.task_id)
        .await
        .expect("continue the held task");
    db.commit().await.unwrap();
    assert_eq!(resumed, "queued");

    app.cleanup().await;
}

/// A `native_question_lost` outcome, after the adapter invalidated the
/// deferred question whose saved session no longer offers it, holds the turn
/// for the owner — not for an answer nobody can deliver
/// (`g5.chat.native-question-recovery`).
#[tokio::test]
async fn native_question_lost_outcome_holds_for_the_owner() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let mut attempt = RunningAttempt::create(&app, &workspace, owner.id, &runtime).await;
    let native_session = Uuid::new_v4().to_string();
    attempt
        .journal(
            &app,
            "provider.session",
            json!({"provider_id":"claude_code", "provider_session_id":native_session}),
        )
        .await;
    let display =
        json!({"questions":[{"question":"Choose","options":[{"label":"A"},{"label":"B"}]}]});
    attempt
        .journal(
            &app,
            "provider_request.opened",
            json!({
                "provider_request_id":"toolu-lost","request_kind":"question",
                "tool_name":"AskUserQuestion","payload":display,"display":display,
                "deadline_unix_ms":null,"resume_session_id":native_session,
            }),
        )
        .await;
    // The owner answers while the process is still running; the answer is
    // accepted durably before the adapter learns the session advanced.
    let client = app.authenticated_client(&owner);
    let body = reply_body("lost-answer", answer());
    let (status, accepted) = attempt.reply(&client, "toolu-lost", &body).await;
    assert_eq!(status, StatusCode::OK, "{accepted}");
    assert_eq!(accepted["data"]["deliveryDisposition"], "queued");
    // The adapter invalidates the question on the same attempt because the
    // session advanced past it, then settles `native_question_lost`.
    attempt
        .journal(
            &app,
            "provider_request.invalidated",
            json!({"provider_request_id":"toolu-lost","reason":"native_session_advanced"}),
        )
        .await;
    assert_eq!(
        attempt.request_state(&app, "toolu-lost").await.0,
        "invalidated"
    );
    attempt
        .journal(
            &app,
            "invocation.outcome",
            json!({
                "status":"succeeded",
                "result":{"schema_version":"g5.agent.turn.outcome/v1",
                    "status":"suspended","code":"native_question_lost","retryable":false,
                    "provider_session_id":native_session,"session_files_state":"present",
                    "session_manifest_generation":1}
            }),
        )
        .await;
    let state: (String, Option<String>) =
        sqlx::query_as("SELECT status, hold_reason FROM tasks WHERE id = $1")
            .bind(attempt.task_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(
        state,
        ("waiting".into(), Some("owner_stop".into())),
        "a lost question holds for the owner, not for an answer"
    );
    // The accepted answer stays recorded; an identical retry says it cannot
    // be delivered rather than that it was newly queued
    // (`g5.chat.native-question-recovery`).
    let (status, retry) = attempt.reply(&client, "toolu-lost", &body).await;
    assert_eq!(status, StatusCode::OK, "{retry}");
    assert_eq!(retry["data"]["deliveryDisposition"], "undeliverable");
    // Continue releases the held turn through the ordinary path.
    let mut db = app.scoped_db_for_org(org.id).await;
    let resumed = app
        .state
        .task_service
        .continue_workspace_task(&mut db, owner.id, attempt.task_id)
        .await
        .expect("continue the held task");
    db.commit().await.unwrap();
    assert_eq!(resumed, "queued");
    app.cleanup().await;
}

/// Two owner reply requests race with the authorization `FOR SHARE` workspace
/// locks already held, as two in-flight HTTP requests hold them after
/// `require_current_workspace_owner`. The claim must not upgrade those shared
/// locks in place — that deadlocks — but converge both replies on the frozen
/// acceptance without database errors.
#[tokio::test]
async fn concurrent_owner_replies_do_not_deadlock_on_authorization_locks() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let mut attempt = RunningAttempt::create(&app, &workspace, owner.id, &runtime).await;
    let owner_client = app.authenticated_client(&owner);

    attempt
        .open_request(&app, "perm-share", "permission", in_a_minute())
        .await;
    let allow = reply_body("share-reply", json!({"behavior":"allow"}));
    let (status, first) = attempt.reply(&owner_client, "perm-share", &allow).await;
    assert_eq!(status, StatusCode::OK, "{first}");

    let mut authorized = Vec::new();
    for _ in 0..2 {
        let mut db = app.scoped_db_for_org(org.id).await;
        sqlx::query(
            "SELECT owner_user_id FROM workspaces
             WHERE id = $1 AND deleted_at IS NULL FOR SHARE OF workspaces",
        )
        .bind(workspace.id)
        .fetch_one(db.conn())
        .await
        .expect("authorization share lock");
        authorized.push(db);
    }
    let db2 = authorized.pop().expect("second authorized transaction");
    let db1 = authorized.pop().expect("first authorized transaction");
    let reply_input = || g5_api::services::task_lifecycle::NativeReplyInput {
        task_id: attempt.task_id,
        attempt: 1,
        provider_request_id: "perm-share".to_string(),
        request_id: "share-reply".to_string(),
        validated_response: json!({"behavior":"allow"}),
        pending_start: None,
    };
    let (one, two) = tokio::join!(
        app.state
            .task_service
            .reply_to_native_request(db1, owner.id, reply_input()),
        app.state
            .task_service
            .reply_to_native_request(db2, owner.id, reply_input()),
    );
    for (name, outcome) in [("first", one), ("second", two)] {
        let outcome = outcome.unwrap_or_else(|error| {
            panic!("{name} concurrent reply must converge, not abort: {error:?}")
        });
        assert!(
            matches!(
                outcome.delivery_disposition.as_str(),
                "delivered" | "already_delivered" | "queued"
            ),
            "{name}: {}",
            outcome.delivery_disposition
        );
    }
    let (request_status, author, _) = attempt.request_state(&app, "perm-share").await;
    assert_eq!(request_status, "reply_pending");
    assert_eq!(author, Some(owner.id));

    app.cleanup().await;
}

/// A transfer or deletion admitted after route authorization committed (its
/// shared lock must not be upgraded in place) is seen by the claim's repeated
/// admission under the exclusive workspace lock: the reply is refused before
/// anything durable changes, and succeeds again once the window closes.
#[tokio::test]
async fn admission_repeated_in_the_claim_refuses_a_reply_unchanged() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let member = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let mut attempt = RunningAttempt::create(&app, &workspace, owner.id, &runtime).await;
    let owner_client = app.authenticated_client(&owner);
    attempt
        .open_request(&app, "perm-adm", "permission", in_a_minute())
        .await;
    let reply_input = || g5_api::services::task_lifecycle::NativeReplyInput {
        task_id: attempt.task_id,
        attempt: 1,
        provider_request_id: "perm-adm".to_string(),
        request_id: "adm-reply".to_string(),
        validated_response: json!({"behavior":"allow"}),
        pending_start: None,
    };

    // Authorization passed with no pending transfer; the transfer then admits
    // in the window before the claim transaction opens.
    let db = app.scoped_db_for_org(org.id).await;
    let transfer_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO workspace_ownership_transfers(
             id,org_id,workspace_id,from_user_id,to_user_id,requested_by,
             idempotency_key,request_digest,status)
         VALUES($1,$2,$3,$4,$5,$4,'admission-window',repeat('a',64),'pending')",
    )
    .bind(transfer_id)
    .bind(org.id)
    .bind(workspace.id)
    .bind(owner.id)
    .bind(member.id)
    .execute(&app.db)
    .await
    .expect("admit a pending transfer in the window");
    let refused = app
        .state
        .task_service
        .reply_to_native_request(db, owner.id, reply_input())
        .await
        .expect_err("a pending transfer must refuse the reply");
    assert!(
        format!("{refused:?}").contains("WorkspaceStopping"),
        "{refused:?}"
    );
    assert_eq!(
        attempt.request_state(&app, "perm-adm").await,
        ("pending".to_string(), None, None),
        "a refused reply must leave nothing durable behind"
    );

    sqlx::query("UPDATE workspace_ownership_transfers SET status='aborted' WHERE id = $1")
        .bind(transfer_id)
        .execute(&app.db)
        .await
        .unwrap();
    sqlx::query("UPDATE workspaces SET deleted_at = now() WHERE id = $1")
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .unwrap();
    let db = app.scoped_db_for_org(org.id).await;
    let refused = app
        .state
        .task_service
        .reply_to_native_request(db, owner.id, reply_input())
        .await
        .expect_err("a deleted workspace must refuse the reply");
    assert!(format!("{refused:?}").contains("NotFound"), "{refused:?}");
    assert_eq!(
        attempt.request_state(&app, "perm-adm").await.0,
        "pending".to_string()
    );

    sqlx::query("UPDATE workspaces SET deleted_at = NULL WHERE id = $1")
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .unwrap();
    let (status, accepted) = attempt
        .reply(
            &owner_client,
            "perm-adm",
            &reply_body("adm-reply", json!({"behavior":"allow"})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{accepted}");
    assert_eq!(accepted["data"]["deliveryDisposition"], "delivered");

    app.cleanup().await;
}

/// Owner replies race attempt settlement on separate PostgreSQL connections.
/// Every identical retry succeeds before, during and after the settlement
/// that invalidates the accepted reply, the connections never deadlock, and
/// the recovered state converges on the frozen acceptance.
#[tokio::test]
async fn identical_replies_race_settlement_across_connections() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let mut attempt = RunningAttempt::create(&app, &workspace, owner.id, &runtime).await;
    let owner_client = app.authenticated_client(&owner);

    attempt
        .open_request(&app, "perm-race", "permission", in_a_minute())
        .await;
    let allow = reply_body("race-reply", json!({"behavior":"allow"}));
    let (status, first) = attempt.reply(&owner_client, "perm-race", &allow).await;
    assert_eq!(status, StatusCode::OK, "{first}");

    let task_id = attempt.task_id;
    let vm_id = attempt.vm_id;
    let retries = async {
        for round in 0..10 {
            let (status, body) = attempt.reply(&owner_client, "perm-race", &allow).await;
            assert_eq!(status, StatusCode::OK, "round {round}: {body}");
            let disposition = body["data"]["deliveryDisposition"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            assert!(
                matches!(
                    disposition.as_str(),
                    "delivered" | "already_delivered" | "queued"
                ),
                "round {round}: {disposition}"
            );
        }
    };
    let settlement = async {
        let mut db = app.scoped_db_for_org(org.id).await;
        app.state
            .task_service
            .apply(
                &mut db,
                TaskLifecycleEvent::Journal(JournalTaskEvent {
                    task_id,
                    attempt: 1,
                    vm_id,
                    epoch: 1,
                    exec_id: canonical_exec_id(task_id, 1, vm_id),
                    seq: 2,
                    kind: "invocation.outcome".to_string(),
                    payload: json!({
                        "status":"succeeded",
                        "result":{"schema_version":"g5.agent.turn.outcome/v1",
                            "status":"succeeded","retryable":false}
                    }),
                    terminal: true,
                }),
            )
            .await
            .expect("settle the attempt mid-race");
        db.commit().await.expect("commit the settlement");
    };
    tokio::join!(retries, settlement);

    let (request_status, author, digest) = attempt.request_state(&app, "perm-race").await;
    assert_eq!(request_status, "invalidated");
    assert_eq!(author, Some(owner.id));
    assert_eq!(
        digest,
        first["data"]["responseDigest"].as_str().map(str::to_string)
    );
    let task_status: String = sqlx::query_scalar("SELECT status FROM tasks WHERE id = $1")
        .bind(task_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(task_status, "succeeded");
    let (status, after) = attempt.reply(&owner_client, "perm-race", &allow).await;
    assert_eq!(status, StatusCode::OK, "{after}");
    assert_eq!(after["data"]["deliveryDisposition"], "queued");

    app.cleanup().await;
}

/// Start and answer (`g5.chat.start-and-send`): while the VM is stopped, a
/// held question's answer carrying `startCompute` records a clearly labelled
/// compound Start, is accepted for the saved provider session, and re-queues
/// the turn so it runs once the VM is up; without the flag the stopped
/// workspace still refuses the first acceptance.
#[tokio::test]
async fn start_and_answer_records_consent_and_requeues_the_held_question() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    workspace.attach_parent(&app).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let mut attempt = RunningAttempt::create(&app, &workspace, owner.id, &runtime).await;
    let native_session = Uuid::new_v4().to_string();
    attempt
        .journal(
            &app,
            "provider.session",
            json!({"provider_id":"claude_code", "provider_session_id":native_session}),
        )
        .await;
    let display =
        json!({"questions":[{"question":"Choose","options":[{"label":"A"},{"label":"B"}]}]});
    attempt.journal(&app, "provider_request.opened", json!({
        "provider_request_id":"toolu-held", "request_kind":"question", "tool_name":"AskUserQuestion",
        "payload":display, "display":display, "deadline_unix_ms":null, "resume_session_id":native_session,
    })).await;
    attempt.journal(&app, "invocation.outcome", json!({
        "status":"succeeded", "result":{"schema_version":"g5.agent.turn.outcome/v1",
            "status":"suspended","code":"native_question_deferred", "retryable":false,
            "provider_session_id":native_session,"session_files_state":"present","session_manifest_generation":1}
    })).await;
    let state: (String, Option<String>) =
        sqlx::query_as("SELECT status, hold_reason FROM tasks WHERE id = $1")
            .bind(attempt.task_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(state, ("waiting".into(), Some("awaiting_answer".into())));
    let client = app.authenticated_client(&owner);
    // The owner stops the workspace with the question still open.
    let stopped = client
        .post_json(
            &format!("/api/v1/workspaces/{}/stop", workspace.id),
            &json!({ "operationId": Uuid::new_v4() }),
        )
        .await;
    assert_eq!(stopped.status(), StatusCode::OK);
    // A plain answer still needs ready compute.
    let refused = attempt
        .reply(&client, "toolu-held", &reply_body("held-answer", answer()))
        .await;
    expect_conflict("no attached compute", refused, "attach ready compute");
    // Start and answer: one request records the compound Start and accepts
    // the answer for the held turn.
    let mut body = reply_body("held-answer", answer());
    body["startCompute"] = json!(true);
    let (status, result) = attempt.reply(&client, "toolu-held", &body).await;
    assert_eq!(status, StatusCode::OK, "{result}");
    assert_eq!(result["data"]["deliveryDisposition"], "queued", "{result}");
    assert_eq!(
        result["data"]["startCompute"]["status"], "starting",
        "{result}"
    );
    let kind: String = sqlx::query_scalar(
        "SELECT start_intent ->> 'kind' FROM workspace_vms WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .expect("intent kind");
    assert_eq!(kind, "compound_start", "a clearly labelled compound Start");
    assert_eq!(
        attempt.request_state(&app, "toolu-held").await.0,
        "reply_pending",
        "the answer is accepted and frozen"
    );
    let state: (String, Option<String>) =
        sqlx::query_as("SELECT status, hold_reason FROM tasks WHERE id = $1")
            .bind(attempt.task_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(
        state,
        ("queued".into(), None),
        "the held turn is re-queued for the starting VM"
    );
    // A retry of the same answer without the flag confirms the frozen reply.
    let (status, again) = attempt
        .reply(&client, "toolu-held", &reply_body("held-answer", answer()))
        .await;
    assert_eq!(status, StatusCode::OK, "{again}");
    assert_eq!(again["data"]["deliveryDisposition"], "queued", "{again}");
    app.cleanup().await;
}
