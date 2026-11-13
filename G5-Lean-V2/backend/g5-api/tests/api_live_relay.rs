//! Live delivery from the guest journal, five-second history batches and
//! tail recovery (`g5.chat.live-delivery`, `g5.chat.history-batches`,
//! `g5.chat.live-tail-recovery`, `g5.chat.journal`; #966).
//!
//! Every test drives the production reconciler, follower, TaskService batch,
//! WebSocket hub and routes against PostgreSQL, with the scripted runtime
//! peer standing in for the guest dispatcher's journal.

use std::collections::HashSet;
use std::time::{Duration, Instant};

use reqwest::StatusCode;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::helpers;
use helpers::{TestApp, TestOrg, TestUser, TestWorkspace, TestWorkspaceRuntimePeer, WsTestClient};

use g5_api::services::task_lifecycle::{canonical_agent_message_id, canonical_exec_id};

fn data(body: &Value) -> &Value {
    body.get("data").unwrap_or(body)
}

fn delta(seq: u64, block: &str, text: &str) -> Value {
    json!({"seq": seq, "kind": "module.message.delta",
           "payload": {"block_id": block, "text_delta": text}})
}

fn final_block(seq: u64, block: &str, content: &str) -> Value {
    json!({"seq": seq, "kind": "module.message.final",
           "payload": {"block_id": block, "content": content}})
}

fn tool_started(seq: u64, call: &str) -> Value {
    json!({"seq": seq, "kind": "module.tool.started",
           "payload": {"call_id": call, "name": "Bash", "content": "ls", "input": {"command": "ls"}}})
}

fn tool_completed(seq: u64, call: &str) -> Value {
    json!({"seq": seq, "kind": "module.tool.completed",
           "payload": {"call_id": call, "name": "Bash", "status": "ok", "output": "ok", "content": "ok"}})
}

fn outcome(seq: u64) -> Value {
    json!({"seq": seq, "kind": "invocation.outcome", "terminal": true,
           "payload": {"status": "succeeded",
                       "result": {"status": "succeeded", "code": "exit_code", "provider_started": true}}})
}

/// A running conversation turn on `vm_id` with its assistant placeholder, as
/// dispatch leaves it once the module has started.
async fn seed_running_turn(
    app: &TestApp,
    org: &TestOrg,
    workspace: &TestWorkspace,
    owner: &TestUser,
    vm_id: Uuid,
) -> (Uuid, Uuid) {
    let session_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO chat_sessions(id, org_id, workspace_id, scope_type, title, created_by)
         VALUES ($1,$2,$3,'workspace','Live relay',$4)",
    )
    .bind(session_id)
    .bind(org.id)
    .bind(workspace.id)
    .bind(owner.id)
    .execute(&app.db)
    .await
    .expect("insert session");
    let task_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO tasks(
             id, org_id, workspace_id, name, kind, status, spec, attempt,
             vm_id, epoch, start_confirmed, started_at, chat_session_id, created_by
         ) VALUES ($1,$2,$3,'Live relay turn','agent','running','{}'::jsonb,1,
                   $4,1,true,now(),$5,$6)",
    )
    .bind(task_id)
    .bind(org.id)
    .bind(workspace.id)
    .bind(vm_id.to_string())
    .bind(session_id)
    .bind(owner.id)
    .execute(&app.db)
    .await
    .expect("insert running agent task");
    sqlx::query(
        "INSERT INTO task_attempts(
             org_id, task_id, attempt, vm_id, epoch, start_confirmed, started_at
         ) VALUES ($1,$2,1,$3,1,true,now())",
    )
    .bind(org.id)
    .bind(task_id)
    .bind(vm_id.to_string())
    .execute(&app.db)
    .await
    .expect("insert running agent attempt");
    sqlx::query(
        "INSERT INTO chat_messages(
             id, org_id, session_id, role, content, citations, status,
             message_index, created_at, task_id, attempt)
         VALUES ($1,$2,$3,'assistant','','[]'::jsonb,'pending',1,now(),$4,1)",
    )
    .bind(canonical_agent_message_id(task_id, 1))
    .bind(org.id)
    .bind(session_id)
    .bind(task_id)
    .execute(&app.db)
    .await
    .expect("insert the attempt's assistant placeholder");
    (session_id, task_id)
}

struct Turn {
    owner: TestUser,
    org: TestOrg,
    workspace: TestWorkspace,
    runtime: TestWorkspaceRuntimePeer,
    session_id: Uuid,
    task_id: Uuid,
    exec_id: String,
}

impl Turn {
    async fn seed(app: &TestApp, journal: Vec<Value>) -> Self {
        let mut owner = TestUser::create(app).await;
        let org = TestOrg::create_with_admin(app, &mut owner).await;
        let workspace = TestWorkspace::create(app, &org).await;
        let runtime = TestWorkspaceRuntimePeer::attach_with_journal_events(
            app, &workspace, owner.id, journal,
        )
        .await;
        let (session_id, task_id) =
            seed_running_turn(app, &org, &workspace, &owner, runtime.vm_id()).await;
        let exec_id = canonical_exec_id(task_id, 1, runtime.vm_id());
        Self {
            owner,
            org,
            workspace,
            runtime,
            session_id,
            task_id,
            exec_id,
        }
    }

    /// A turn whose guest holds every request carrying one of `methods`
    /// until the test releases it; the journal starts empty.
    async fn seed_holding(app: &TestApp, methods: &[&'static str]) -> Self {
        let mut owner = TestUser::create(app).await;
        let org = TestOrg::create_with_admin(app, &mut owner).await;
        let workspace = TestWorkspace::create(app, &org).await;
        let runtime =
            TestWorkspaceRuntimePeer::attach_with_rpc_barriers(app, &workspace, owner.id, methods)
                .await;
        let (session_id, task_id) =
            seed_running_turn(app, &org, &workspace, &owner, runtime.vm_id()).await;
        let exec_id = canonical_exec_id(task_id, 1, runtime.vm_id());
        Self {
            owner,
            org,
            workspace,
            runtime,
            session_id,
            task_id,
            exec_id,
        }
    }

    async fn chat_socket(&self, app: &TestApp) -> WsTestClient {
        let mut socket = app
            .ws_connect(&self.owner, &format!("chat:{}", self.session_id))
            .await;
        socket.expect_subscribed().await;
        socket
    }

    async fn persisted_through(&self, app: &TestApp) -> i64 {
        sqlx::query_scalar(
            "SELECT acked_event_seq FROM task_attempts WHERE task_id = $1 AND attempt = 1",
        )
        .bind(self.task_id)
        .fetch_one(&app.db)
        .await
        .expect("load the persisted prefix")
    }

    async fn stored_rows(&self, app: &TestApp) -> Vec<(Uuid, i64, Value)> {
        sqlx::query_as("SELECT id, seq, metadata FROM task_events WHERE task_id = $1 ORDER BY seq")
            .bind(self.task_id)
            .fetch_all(&app.db)
            .await
            .expect("load the stored rows")
    }

    async fn task_status(&self, app: &TestApp) -> String {
        sqlx::query_scalar("SELECT status FROM tasks WHERE id = $1")
            .bind(self.task_id)
            .fetch_one(&app.db)
            .await
            .expect("load the task")
    }

    async fn reply_content(&self, app: &TestApp) -> (String, String) {
        sqlx::query_as("SELECT content, status FROM chat_messages WHERE id = $1")
            .bind(canonical_agent_message_id(self.task_id, 1))
            .fetch_one(&app.db)
            .await
            .expect("load the reply")
    }

    async fn live_tail(&self, app: &TestApp) -> Value {
        let response = app
            .authenticated_client(&self.owner)
            .get(&format!("/api/v1/sessions/{}/live-tail", self.session_id))
            .await;
        let status = response.status();
        let body: Value = response.json().await.expect("live tail response");
        assert_eq!(status, StatusCode::OK, "{body}");
        data(&body).clone()
    }

    async fn stop(&self, app: &TestApp) -> Value {
        let response = app
            .authenticated_client(&self.owner)
            .post_json(
                &format!(
                    "/api/v1/sessions/{}/messages/{}/stop",
                    self.session_id,
                    canonical_agent_message_id(self.task_id, 1)
                ),
                &json!({}),
            )
            .await;
        let status = response.status();
        let body: Value = response.json().await.expect("stop response");
        assert_eq!(status, StatusCode::OK, "{body}");
        data(&body).clone()
    }
}

async fn reconcile(app: &TestApp) {
    g5_api_core::maintenance::agent_tasks::run_once(&app.state)
        .await
        .expect("run one reconciliation pass");
}

/// Collects `expected` live chat timeline rows, failing on the timeout.
async fn collect_live(socket: &mut WsTestClient, expected: usize, timeout: Duration) -> Vec<Value> {
    let deadline = Instant::now() + timeout;
    let mut rows = Vec::with_capacity(expected);
    while rows.len() < expected {
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(
            !remaining.is_zero(),
            "only {} of {expected} live rows arrived within {timeout:?}",
            rows.len()
        );
        let row = socket.expect_event("chat.timeline", remaining).await;
        rows.push(row["entry"].clone());
    }
    rows
}

fn seqs(rows: &[Value]) -> Vec<u64> {
    rows.iter()
        .map(|row| row["metadata"]["seq"].as_u64().expect("seq"))
        .collect()
}

/// Polls `check` (an expression that may `.await`) until it holds, failing
/// after `timeout`.
macro_rules! wait_until {
    ($what:expr, $timeout:expr, $check:expr) => {{
        let deadline = Instant::now() + $timeout;
        loop {
            if $check {
                break;
            }
            assert!(Instant::now() < deadline, "{} within {:?}", $what, $timeout);
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }};
}

/// The guest acknowledgements once the follower's acknowledgement task,
/// which runs beside the follower, has sent the last one expected.
async fn acks_after(runtime: &TestWorkspaceRuntimePeer, last: u64) -> Vec<u64> {
    wait_until!(
        format!("the guest is acknowledged through {last}"),
        Duration::from_secs(10),
        runtime.acked_seqs().await.last().copied() == Some(last)
    );
    runtime.acked_seqs().await
}

/// How many `events.ack` calls reached the peer, answered or held.
async fn acks_sent(runtime: &TestWorkspaceRuntimePeer) -> usize {
    rpc_count(runtime, "events.ack").await
}

/// How many calls of `method` reached the peer, answered or held.
async fn rpc_count(runtime: &TestWorkspaceRuntimePeer, method: &str) -> usize {
    runtime
        .received_rpc_methods()
        .await
        .iter()
        .filter(|received| received.as_str() == method)
        .count()
}

/// Connections of this database idle inside a transaction: what a guest
/// call holds while it waits for its answer (its incarnation fence).
async fn connections_idle_in_transaction(app: &TestApp) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM pg_stat_activity
         WHERE datname = current_database() AND state = 'idle in transaction'",
    )
    .fetch_one(&app.db)
    .await
    .expect("read idle transactions")
}

/// Waits until a backend waits on a lock inside the batch's attempt-lock
/// statement, the evidence that a history write is blocked on the row.
async fn wait_for_attempt_lock_waiter(app: &TestApp) {
    wait_until!(
        "a history batch waits on the attempt row",
        Duration::from_secs(10),
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM pg_stat_activity
             WHERE datname = current_database() AND wait_event_type = 'Lock'
               AND query LIKE '%FOR UPDATE OF task, attempt_row%'",
        )
        .fetch_one(&app.db)
        .await
        .expect("read lock waits")
            >= 1
    );
}

async fn journal_confirmed(app: &TestApp, task_id: Uuid) -> i64 {
    sqlx::query_scalar("SELECT journal_acked_seq FROM task_attempts WHERE task_id = $1")
        .bind(task_id)
        .fetch_one(&app.db)
        .await
        .expect("load the confirmation")
}

async fn settlements(app: &TestApp, task_id: Uuid) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM task_attempts WHERE task_id = $1 AND ended_at IS NOT NULL",
    )
    .bind(task_id)
    .fetch_one(&app.db)
    .await
    .expect("count settlements")
}

/// Under ordinary low-volume output, half a second does not flush history
/// and five seconds does; a terminal boundary flushes at once; the
/// committed prefix is acknowledged once per batch (`g5.chat.history-batches`).
#[tokio::test]
async fn routine_output_waits_five_seconds_and_a_boundary_flushes_at_once() {
    let app = TestApp::spawn().await;
    let turn = Turn::seed(
        &app,
        vec![
            delta(1, "b1", "Hello"),
            delta(2, "b1", " there"),
            delta(3, "b1", ", world"),
        ],
    )
    .await;
    let mut socket = turn.chat_socket(&app).await;
    reconcile(&app).await;
    let live = collect_live(&mut socket, 3, Duration::from_secs(5)).await;
    let received_at = Instant::now();
    assert_eq!(seqs(&live), vec![1, 2, 3]);
    assert!(live.iter().all(|row| row["metadata"]["live"] == true));

    tokio::time::sleep(Duration::from_millis(600)).await;
    assert_eq!(
        turn.persisted_through(&app).await,
        0,
        "no flush after 500 ms"
    );
    assert!(turn.stored_rows(&app).await.is_empty());
    assert!(turn.runtime.acked_seqs().await.is_empty());

    assert!(
        app.state
            .followers
            .wait_persisted(&turn.exec_id, 3, Duration::from_secs(10))
            .await,
        "the batch is stored once the interval elapses"
    );
    let waited = received_at.elapsed();
    assert!(
        waited >= Duration::from_millis(3500),
        "routine output waited for the five-second interval, not {waited:?}"
    );
    let stats = app
        .state
        .followers
        .stats(&turn.exec_id)
        .expect("the follower is still running");
    assert_eq!(stats.flushes, 1);
    assert_eq!(stats.flushed_events, 3);
    assert_eq!(acks_after(&turn.runtime, 3).await, vec![3]);
    let rows = turn.stored_rows(&app).await;
    assert_eq!(rows.len(), 3);
    assert_eq!(
        rows.iter()
            .map(|(id, _, _)| id.to_string())
            .collect::<Vec<_>>(),
        live.iter()
            .map(|row| row["id"].as_str().unwrap().to_string())
            .collect::<Vec<_>>(),
        "stored rows carry the ids the live copies announced"
    );

    turn.runtime
        .journal()
        .extend([final_block(4, "b1", "Hello there, world"), outcome(5)])
        .await;
    app.state.followers.wake(&turn.exec_id);
    let appended_at = Instant::now();
    assert!(
        app.state
            .followers
            .wait_finished(&turn.exec_id, Duration::from_secs(10))
            .await
    );
    assert!(
        appended_at.elapsed() < Duration::from_secs(4),
        "the terminal boundary flushed without waiting for the interval"
    );
    assert_eq!(acks_after(&turn.runtime, 5).await, vec![3, 5]);
    assert_eq!(turn.task_status(&app).await, "succeeded");
    assert_eq!(
        turn.reply_content(&app).await,
        ("Hello there, world".to_string(), "complete".to_string())
    );
    let ended: bool =
        sqlx::query_scalar("SELECT ended_at IS NOT NULL FROM task_attempts WHERE task_id = $1")
            .bind(turn.task_id)
            .fetch_one(&app.db)
            .await
            .expect("load the attempt");
    assert!(ended);
    wait_until!(
        "the guest's confirmation is recorded",
        Duration::from_secs(10),
        journal_confirmed(&app, turn.task_id).await == 5
    );
    app.cleanup().await;
}

/// A ready backlog drains in bounded batches without a five-second wait
/// between them: 600 deltas become batches of 256, 256 and 88.
#[tokio::test]
async fn capacity_closes_batches_early_and_a_backlog_drains_without_the_interval() {
    let app = TestApp::spawn().await;
    let journal: Vec<Value> = (1..=600)
        .map(|seq| delta(seq, "b1", &format!("w{seq} ")))
        .collect();
    let turn = Turn::seed(&app, journal).await;
    let started = Instant::now();
    reconcile(&app).await;
    assert!(
        app.state
            .followers
            .wait_persisted(&turn.exec_id, 600, Duration::from_secs(10))
            .await
    );
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "the backlog drained before one interval: {:?}",
        started.elapsed()
    );
    let stats = app.state.followers.stats(&turn.exec_id).expect("follower");
    assert_eq!(stats.flushes, 3);
    assert_eq!(stats.flushed_events, 600);
    assert_eq!(acks_after(&turn.runtime, 600).await, vec![256, 512, 600]);
    assert_eq!(turn.stored_rows(&app).await.len(), 600);
    app.cleanup().await;
}

/// The incident's shape: 370 journaled events (362 deltas) finished before
/// the API caught up, then a Stop. Every event reaches the browser once,
/// the stored text and timeline are exact, the actual success is retained
/// and the attempt settles once (`g5.chat.cancel-outcomes`).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_incident_shape_keeps_exact_output_and_the_actual_outcome_after_stop() {
    let app = TestApp::spawn().await;
    let mut journal = vec![tool_started(1, "c1"), tool_completed(2, "c1")];
    let mut text = String::new();
    for i in 0..362u64 {
        let piece = format!("w{i} ");
        text.push_str(&piece);
        journal.push(delta(3 + i, "b1", &piece));
    }
    journal.extend([
        tool_started(365, "c2"),
        tool_completed(366, "c2"),
        tool_started(367, "c3"),
        tool_completed(368, "c3"),
        final_block(369, "b1", text.trim_end()),
        outcome(370),
    ]);
    assert_eq!(journal.len(), 370);
    let turn = Turn::seed(&app, journal).await;
    let mut socket = turn.chat_socket(&app).await;
    // The Stop reaches the API after the guest finished and before the API
    // caught up, as in the incident.
    let stop = turn.stop(&app).await;
    assert_eq!(stop["stopped"], true);
    assert_eq!(
        stop["disposition"], "already_finished",
        "the guest had already finished: the owner sees finishing, not stopping"
    );
    reconcile(&app).await;
    let live = collect_live(&mut socket, 370, Duration::from_secs(20)).await;
    let ids: HashSet<&str> = live.iter().map(|row| row["id"].as_str().unwrap()).collect();
    assert_eq!(ids.len(), 370, "no live row was published twice");
    assert_eq!(seqs(&live), (1..=370).collect::<Vec<_>>());
    assert!(
        app.state
            .followers
            .wait_finished(&turn.exec_id, Duration::from_secs(20))
            .await
    );
    assert_eq!(
        turn.task_status(&app).await,
        "succeeded",
        "Stop did not overwrite success"
    );
    let (content, status) = turn.reply_content(&app).await;
    assert_eq!(content, text.trim_end());
    assert_eq!(status, "complete");
    let rows = turn.stored_rows(&app).await;
    assert_eq!(rows.len(), 370);
    let stored_ids: HashSet<String> = rows.iter().map(|(id, _, _)| id.to_string()).collect();
    assert_eq!(stored_ids.len(), 370);
    assert!(
        ids.iter().all(|id| stored_ids.contains(*id)),
        "every live row is the stored row"
    );
    let settlements: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM task_attempts WHERE task_id = $1 AND ended_at IS NOT NULL",
    )
    .bind(turn.task_id)
    .fetch_one(&app.db)
    .await
    .expect("count settlements");
    assert_eq!(settlements, 1);
    let batches = acks_after(&turn.runtime, 370).await;
    assert!(
        batches.len() <= 3,
        "acknowledgements scale with batches, not deltas: {batches:?}"
    );
    assert_eq!(batches.last().copied(), Some(370));
    app.cleanup().await;
}

/// A history write blocked for longer than its batch interval: the attempt
/// row is held by another transaction for ten seconds while the batch is
/// observed waiting on it. Live rows still reach the browser within the
/// second and the tail read serves them; a Stop sent meanwhile answers 200
/// because the batch yields the session's locks after its bounded wait
/// instead of holding them; the persisted prefix stays put and nothing is
/// acknowledged until release, after which history converges once with one
/// settlement (`g5.chat.live-delivery`, `g5.chat.live-tail-recovery`,
/// `g5.chat.history-batches`, `g5.chat.controls`).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_blocked_history_write_delays_neither_live_text_nor_stop_and_converges_once() {
    let app = TestApp::spawn().await;
    let turn = Turn::seed(
        &app,
        vec![
            delta(1, "b1", "The "),
            delta(2, "b1", "quick "),
            delta(3, "b1", "brown "),
            delta(4, "b1", "fox"),
            delta(5, "b1", "."),
        ],
    )
    .await;
    assert_eq!(
        turn.live_tail(&app).await["state"],
        "catching_up",
        "a running turn no follower streams yet is catching up"
    );
    let mut socket = turn.chat_socket(&app).await;
    reconcile(&app).await;
    let mut live = collect_live(&mut socket, 5, Duration::from_secs(5)).await;

    // Hold the attempt row as a stalled write would, then let the guest
    // produce a boundary that asks for an immediate batch.
    let mut stall = app.db.begin().await.expect("begin the stall");
    sqlx::query("SELECT 1 FROM task_attempts WHERE task_id = $1 AND attempt = 1 FOR UPDATE")
        .bind(turn.task_id)
        .execute(&mut *stall)
        .await
        .expect("hold the attempt row");
    let stalled_at = Instant::now();
    turn.runtime.journal().append(tool_started(6, "c1")).await;
    app.state.followers.wake(&turn.exec_id);
    live.extend(collect_live(&mut socket, 1, Duration::from_secs(5)).await);
    wait_for_attempt_lock_waiter(&app).await;

    // Ten seconds of output while the write stays blocked.
    let mut latencies = Vec::new();
    let mut stop_latency = None;
    for i in 0..10u64 {
        let seq = 7 + i;
        let sent = Instant::now();
        turn.runtime
            .journal()
            .append(delta(seq, "b1", &format!(" w{i}")))
            .await;
        app.state.followers.wake(&turn.exec_id);
        live.extend(collect_live(&mut socket, 1, Duration::from_secs(3)).await);
        latencies.push(sent.elapsed());
        let tail = turn.live_tail(&app).await;
        assert_eq!(tail["state"], "live");
        assert_eq!(tail["persistedThrough"], 0);
        assert_eq!(tail["fetchedThrough"], seq);
        assert_eq!(
            tail["entries"].as_array().map(Vec::len),
            Some(seq as usize),
            "the tail holds every unpersisted row"
        );
        if i == 4 {
            // Stop in the middle of the stall: the blocked batch holds the
            // session's turn lock for at most its bounded wait.
            let asked = Instant::now();
            let response = turn.stop(&app).await;
            assert_eq!(response["stopped"], true, "{response}");
            assert_eq!(response["disposition"], "signalling", "{response}");
            stop_latency = Some(asked.elapsed());
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    assert!(stalled_at.elapsed() >= Duration::from_secs(10));
    assert_eq!(seqs(&live), (1..=16).collect::<Vec<_>>());
    let slowest = latencies.iter().max().copied().unwrap_or_default();
    assert!(
        slowest < Duration::from_secs(3),
        "live rows kept arriving during the stall (slowest {slowest:?})"
    );
    let stop_latency = stop_latency.expect("a Stop was sent during the stall");
    assert!(
        stop_latency < Duration::from_secs(5),
        "Stop answered in {stop_latency:?} while the write was blocked"
    );
    let (cancel_requested,): (bool,) = sqlx::query_as(
        "SELECT COALESCE((spec #>> '{lifecycle,cancel_requested}')::boolean, false)
         FROM tasks WHERE id = $1",
    )
    .bind(turn.task_id)
    .fetch_one(&app.db)
    .await
    .expect("load the cancel intent");
    assert!(cancel_requested, "the Stop committed its intent");
    assert_eq!(turn.persisted_through(&app).await, 0);
    assert!(turn.stored_rows(&app).await.is_empty());
    assert!(turn.runtime.acked_seqs().await.is_empty());
    assert_eq!(turn.task_status(&app).await, "running");

    stall.rollback().await.expect("release the stall");
    // The guest finishes after the cancel: its actual outcome settles the turn.
    turn.runtime.journal().append(outcome(17)).await;
    app.state.followers.wake(&turn.exec_id);
    live.extend(collect_live(&mut socket, 1, Duration::from_secs(10)).await);
    assert!(
        app.state
            .followers
            .wait_finished(&turn.exec_id, Duration::from_secs(20))
            .await
    );
    let rows = turn.stored_rows(&app).await;
    assert_eq!(
        rows.iter().map(|(_, seq, _)| *seq).collect::<Vec<_>>(),
        (1..=17).collect::<Vec<i64>>(),
        "history converged once, without replay duplicates"
    );
    assert_eq!(
        rows.iter()
            .map(|(id, _, _)| id.to_string())
            .collect::<Vec<_>>(),
        live.iter()
            .map(|row| row["id"].as_str().unwrap().to_string())
            .collect::<Vec<_>>(),
        "every live row is the stored row"
    );
    assert_eq!(turn.persisted_through(&app).await, 17);
    assert_eq!(
        acks_after(&turn.runtime, 17).await.last().copied(),
        Some(17)
    );
    assert_eq!(turn.task_status(&app).await, "succeeded");
    assert_eq!(settlements(&app, turn.task_id).await, 1);
    assert_eq!(turn.live_tail(&app).await["state"], "none");
    eprintln!("blocked write: live latencies {latencies:?}, Stop answered in {stop_latency:?}");
    app.cleanup().await;
}

/// No running turn: the tail read says so.
#[tokio::test]
async fn the_tail_read_reports_no_live_attempt_after_settlement() {
    let app = TestApp::spawn().await;
    let turn = Turn::seed(&app, vec![delta(1, "b1", "Hi"), outcome(2)]).await;
    reconcile(&app).await;
    assert!(
        app.state
            .followers
            .wait_finished(&turn.exec_id, Duration::from_secs(10))
            .await
    );
    assert_eq!(turn.live_tail(&app).await["state"], "none");
    app.cleanup().await;
}

/// An acknowledgement that failed after the attempt ended is retried by the
/// reconciler while the incarnation lives (`g5.chat.journal`).
#[tokio::test]
async fn a_failed_acknowledgement_is_retried_after_the_attempt_ended() {
    let app = TestApp::spawn().await;
    let turn = Turn::seed(&app, vec![delta(1, "b1", "Hi"), outcome(2)]).await;
    turn.runtime.set_ack_failure(true);
    reconcile(&app).await;
    assert!(
        app.state
            .followers
            .wait_finished(&turn.exec_id, Duration::from_secs(10))
            .await
    );
    assert_eq!(turn.task_status(&app).await, "succeeded");
    let (persisted, confirmed): (i64, i64) = sqlx::query_as(
        "SELECT acked_event_seq, journal_acked_seq FROM task_attempts WHERE task_id = $1",
    )
    .bind(turn.task_id)
    .fetch_one(&app.db)
    .await
    .expect("load the attempt");
    assert_eq!(
        (persisted, confirmed),
        (2, 0),
        "stored but not confirmed by the guest"
    );
    assert!(turn.runtime.acked_seqs().await.is_empty());

    turn.runtime.set_ack_failure(false);
    // The retry scan runs on its own task, off the tick.
    reconcile(&app).await;
    wait_until!(
        "the guest confirms the retried prefix",
        Duration::from_secs(10),
        journal_confirmed(&app, turn.task_id).await == 2
    );
    assert_eq!(turn.runtime.acked_seqs().await, vec![2]);
    reconcile(&app).await;
    wait_until!(
        "the next scan ends",
        Duration::from_secs(10),
        !app.state.followers.ack_retry_running()
    );
    assert_eq!(
        turn.runtime.acked_seqs().await,
        vec![2],
        "a confirmed prefix is not acknowledged again"
    );
    app.cleanup().await;
}

/// A guest that never answers `events.ack` holds neither the reconciler
/// tick nor another workspace's turn: the follower's acknowledgement and
/// the retry scan run on their own tasks, and acknowledgements in flight
/// are bounded so a held one cannot exhaust the connections every guest
/// call fences with (`g5.chat.journal`). The test pool allows one
/// acknowledgement at a time.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_stalled_acknowledgement_holds_neither_the_tick_nor_another_turn() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let stalled_workspace = TestWorkspace::create(&app, &org).await;
    let stalled_runtime = TestWorkspaceRuntimePeer::attach_with_rpc_barrier(
        &app,
        &stalled_workspace,
        owner.id,
        "events.ack",
    )
    .await;
    stalled_runtime
        .journal()
        .extend([delta(1, "b1", "Hi"), outcome(2)])
        .await;
    let (_, stalled_task) = seed_running_turn(
        &app,
        &org,
        &stalled_workspace,
        &owner,
        stalled_runtime.vm_id(),
    )
    .await;
    let stalled_exec = canonical_exec_id(stalled_task, 1, stalled_runtime.vm_id());
    reconcile(&app).await;
    assert!(
        app.state
            .followers
            .wait_finished(&stalled_exec, Duration::from_secs(10))
            .await,
        "the turn settles while its acknowledgement waits on the guest"
    );
    let status: String = sqlx::query_scalar("SELECT status FROM tasks WHERE id = $1")
        .bind(stalled_task)
        .fetch_one(&app.db)
        .await
        .expect("load the stalled task");
    assert_eq!(status, "succeeded");
    let (persisted, confirmed): (i64, i64) = sqlx::query_as(
        "SELECT acked_event_seq, journal_acked_seq FROM task_attempts WHERE task_id = $1",
    )
    .bind(stalled_task)
    .fetch_one(&app.db)
    .await
    .expect("load the attempt");
    assert_eq!((persisted, confirmed), (2, 0));
    wait_until!(
        "the follower's acknowledgement reaches the guest",
        Duration::from_secs(5),
        acks_sent(&stalled_runtime).await == 1
    );

    // Another workspace's turn, reconciled while the arrears stand.
    let healthy_workspace = TestWorkspace::create(&app, &org).await;
    let healthy_runtime = TestWorkspaceRuntimePeer::attach_with_journal_events(
        &app,
        &healthy_workspace,
        owner.id,
        vec![delta(1, "b1", "Hi"), outcome(2)],
    )
    .await;
    let (_, healthy_task) = seed_running_turn(
        &app,
        &org,
        &healthy_workspace,
        &owner,
        healthy_runtime.vm_id(),
    )
    .await;
    let tick = Instant::now();
    reconcile(&app).await;
    let tick_elapsed = tick.elapsed();
    // The scan claimed the arrears and is still waiting when the tick
    // returns: the tick never waited for it (a tick that did would last the
    // guest call's fifteen-second budget).
    assert!(
        app.state.followers.ack_retry_running(),
        "the retry scan runs on while the tick has returned"
    );
    assert!(
        tick_elapsed < Duration::from_secs(5),
        "the tick returned in {tick_elapsed:?} with an acknowledgement stalled"
    );
    let healthy_exec = canonical_exec_id(healthy_task, 1, healthy_runtime.vm_id());
    assert!(
        app.state
            .followers
            .wait_finished(&healthy_exec, Duration::from_secs(10))
            .await,
        "the healthy turn settled while the other guest held its acknowledgement"
    );
    let status: String = sqlx::query_scalar("SELECT status FROM tasks WHERE id = $1")
        .bind(healthy_task)
        .fetch_one(&app.db)
        .await
        .expect("load the healthy task");
    assert_eq!(status, "succeeded");
    // Acknowledgements wait behind the held one; nothing else does.
    assert_eq!(journal_confirmed(&app, stalled_task).await, 0);
    assert_eq!(acks_sent(&stalled_runtime).await, 1);
    let tick = Instant::now();
    reconcile(&app).await;
    assert!(tick.elapsed() < Duration::from_secs(5));

    stalled_runtime.release_rpc_barrier();
    wait_until!(
        "the guest confirms the prefix",
        Duration::from_secs(15),
        journal_confirmed(&app, stalled_task).await == 2
    );
    wait_until!(
        "the healthy turn's acknowledgement follows",
        Duration::from_secs(15),
        healthy_runtime.acked_seqs().await == vec![2]
    );
    wait_until!(
        "the retry scan ends",
        Duration::from_secs(15),
        !app.state.followers.ack_retry_running()
    );
    assert!(stalled_runtime.acked_seqs().await.contains(&2));
    app.cleanup().await;
}

/// Status probes and cancel redeliveries are guest calls, and every guest
/// call pins a pooled connection for its round trip. With six idle
/// followers whose guests accept those calls and never answer, the calls in
/// flight stay within the side-call bound (two for this test's pool of
/// eight, which is also what makes the test fail without the permits: the
/// pool alone would admit seven), the rest wait without a connection, and
/// another workspace's turn proceeds meanwhile (`g5.chat.journal`,
/// `g5.chat.controls`).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stalled_probes_and_redeliveries_pin_a_bounded_number_of_connections() {
    let mut config = helpers::test_config();
    config.database.max_connections = 8;
    let app = TestApp::spawn_with_config(config).await;
    let limit = app.state.followers.side_call_limit();
    assert_eq!(limit, 2, "a pool of eight allows two side calls at a time");
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let mut stalled = Vec::new();
    for _ in 0..6 {
        let workspace = TestWorkspace::create(&app, &org).await;
        let runtime = TestWorkspaceRuntimePeer::attach_with_rpc_barriers(
            &app,
            &workspace,
            owner.id,
            &["modules.status", "modules.cancel"],
        )
        .await;
        let (_, task_id) = seed_running_turn(&app, &org, &workspace, &owner, runtime.vm_id()).await;
        let exec_id = canonical_exec_id(task_id, 1, runtime.vm_id());
        stalled.push((runtime, task_id, exec_id));
    }
    reconcile(&app).await;
    for (_, _, exec_id) in &stalled {
        assert!(app.state.followers.is_following(exec_id));
    }
    // One idle turn carries a committed Stop whose direct delivery failed:
    // its follower redelivers the intent, which the guest then holds.
    let (first_runtime, first_task, first_exec) = &stalled[0];
    sqlx::query(
        "UPDATE tasks
         SET spec = jsonb_set(spec, '{lifecycle}',
             COALESCE(spec -> 'lifecycle', '{}'::jsonb) || jsonb_build_object(
                 'cancel_requested', true,
                 'cancel_requested_by', $2::text,
                 'cancel_requested_at', to_jsonb(now())), true)
         WHERE id = $1",
    )
    .bind(first_task)
    .bind(owner.id.to_string())
    .execute(&app.db)
    .await
    .expect("commit the cancel intent");
    app.state.followers.poke(first_exec);
    wait_until!(
        "the redelivery reaches the guest",
        Duration::from_secs(10),
        rpc_count(first_runtime, "modules.cancel").await >= 1
    );

    // Another workspace's turn, started while the redelivery is held.
    let healthy_workspace = TestWorkspace::create(&app, &org).await;
    let healthy_runtime = TestWorkspaceRuntimePeer::attach_with_journal_events(
        &app,
        &healthy_workspace,
        owner.id,
        vec![delta(1, "b1", "Hi"), outcome(2)],
    )
    .await;
    let (_, healthy_task) = seed_running_turn(
        &app,
        &org,
        &healthy_workspace,
        &owner,
        healthy_runtime.vm_id(),
    )
    .await;
    reconcile(&app).await;
    let healthy_exec = canonical_exec_id(healthy_task, 1, healthy_runtime.vm_id());
    assert!(
        app.state
            .followers
            .wait_finished(&healthy_exec, Duration::from_secs(10))
            .await,
        "the healthy turn settled while the other guests held their calls"
    );
    let status: String = sqlx::query_scalar("SELECT status FROM tasks WHERE id = $1")
        .bind(healthy_task)
        .fetch_one(&app.db)
        .await
        .expect("load the healthy task");
    assert_eq!(status, "succeeded");

    // Past the idle interval every follower wants a status probe; the
    // permit is held, so the probes wait without pinning anything. Sample
    // the connections idle inside a transaction across that window.
    let mut peak = 0;
    let window = Instant::now() + Duration::from_secs(12);
    while Instant::now() < window {
        peak = peak.max(connections_idle_in_transaction(&app).await);
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    let arrived: usize = {
        let mut total = 0;
        for (runtime, _, _) in &stalled {
            total += rpc_count(runtime, "modules.status").await
                + rpc_count(runtime, "modules.cancel").await;
        }
        total
    };
    assert!(
        arrived <= limit + 1,
        "guest calls in flight stay within the bound (a call that ran out its budget may be followed by one more): {arrived}"
    );
    assert!(
        peak <= limit as i64 + 1,
        "connections held by waiting guest calls stay bounded (peak {peak})"
    );
    for (_, _, exec_id) in &stalled {
        assert!(app.state.followers.is_following(exec_id));
    }

    // Released, every held call answers. The redelivered Stop reaches the
    // first guest, whose run (with nothing journaled) ends cancelled, so that
    // attempt settles once; the other probes report live executions and
    // their followers go on.
    for (runtime, _, _) in &stalled {
        runtime.release_rpc_barrier_permits(16);
    }
    wait_until!(
        "the idle followers' probes reach their guests",
        Duration::from_secs(30),
        {
            let mut probed = 0;
            for (runtime, _, _) in &stalled[1..] {
                probed += usize::from(rpc_count(runtime, "modules.status").await >= 1);
            }
            probed == 5
        }
    );
    assert!(first_runtime.cancelled_execs().await.contains(first_exec));
    assert!(
        app.state
            .followers
            .wait_finished(first_exec, Duration::from_secs(20))
            .await,
        "the stopped attempt settles on the guest's cancelled outcome"
    );
    let status: String = sqlx::query_scalar("SELECT status FROM tasks WHERE id = $1")
        .bind(first_task)
        .fetch_one(&app.db)
        .await
        .expect("load the stopped task");
    assert_eq!(status, "cancelled");
    assert_eq!(settlements(&app, *first_task).await, 1);
    for (_, task_id, exec_id) in &stalled[1..] {
        assert!(app.state.followers.is_following(exec_id));
        let status: String = sqlx::query_scalar("SELECT status FROM tasks WHERE id = $1")
            .bind(task_id)
            .fetch_one(&app.db)
            .await
            .expect("load the stalled task");
        assert_eq!(
            status, "running",
            "a live execution is not settled by its probe"
        );
    }
    app.cleanup().await;
}

/// A guest call holds no database connection while the guest answers: three
/// Stops whose cancel calls the guests hold proceed at once on the
/// two-connection test pool, nothing sits idle in a transaction meanwhile,
/// and an ordinary request still answers (`g5.runtime.guest-call-admission`).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn held_guest_calls_pin_no_database_connection() {
    let app = TestApp::spawn().await;
    let mut turns = Vec::new();
    for _ in 0..3 {
        turns.push(Turn::seed_holding(&app, &["modules.cancel"]).await);
    }
    let stops = async {
        let (a, b, c) = tokio::join!(
            turns[0].stop(&app),
            turns[1].stop(&app),
            turns[2].stop(&app)
        );
        vec![a, b, c]
    };
    let observe = async {
        wait_until!(
            "every Stop's cancel call reaches its guest",
            Duration::from_secs(10),
            {
                let mut held = 0;
                for turn in &turns {
                    held += usize::from(rpc_count(&turn.runtime, "modules.cancel").await >= 1);
                }
                held == 3
            }
        );
        let mut peak = 0;
        for _ in 0..10 {
            peak = peak.max(connections_idle_in_transaction(&app).await);
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        for turn in &turns {
            assert_eq!(
                app.state
                    .workspace_runtime
                    .calls_in_flight(turn.workspace.id),
                1
            );
        }
        // The pool has two connections and three calls are held: a request
        // that needs one must still answer promptly.
        let asked = Instant::now();
        let tail = turns[0].live_tail(&app).await;
        assert!(tail["state"].is_string(), "{tail}");
        let latency = asked.elapsed();
        assert!(
            latency < Duration::from_secs(3),
            "a request answered in {latency:?} while three guest calls were held"
        );
        for turn in &turns {
            turn.runtime.release_rpc_barrier_permits(16);
        }
        peak
    };
    let (responses, peak) = tokio::join!(stops, observe);
    assert_eq!(peak, 0, "no held guest call keeps a transaction open");
    for response in responses {
        assert_eq!(response["stopped"], true, "{response}");
        assert_eq!(response["disposition"], "signalling", "{response}");
    }
    for turn in &turns {
        assert_eq!(
            app.state
                .workspace_runtime
                .calls_in_flight(turn.workspace.id),
            0
        );
    }
    app.cleanup().await;
}

/// A stop waits for the incarnation's calls in flight before it takes the
/// incarnation: the guest sees the held call end before `admission.close`
/// (`g5.runtime.guest-call-admission`).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_stop_waits_for_the_incarnations_calls_in_flight() {
    let app = TestApp::spawn().await;
    let turn = Turn::seed_holding(&app, &["modules.cancel"]).await;
    let db = app.scoped_db_for_user_org(turn.owner.id, turn.org.id).await;
    let scope = db.reopener();
    db.rollback().await.expect("close captured lifecycle scope");
    let service =
        g5_api::services::WorkspaceVmService::new(g5_api::services::WorkspaceVmServiceConfig {
            system_pool: app.state.system_pool.clone(),
            scope,
            hosts: app.state.preview_tunnels.clone(),
            minter: app
                .state
                .jwt_minter
                .clone()
                .expect("test app configures a lifecycle JWT minter"),
            runtime: app.state.workspace_runtime.clone(),
            agent_work: app.state.agent_work.clone(),
            repo_storage: app.state.repo_storage.clone(),
            git_external_url: app.state.git_external_url.clone(),
            ws_hub: app.state.event_bus.ws_hub().clone(),
        });
    let stop_request = turn.stop(&app);
    let drive = async {
        wait_until!(
            "the Stop's cancel call reaches the guest",
            Duration::from_secs(10),
            rpc_count(&turn.runtime, "modules.cancel").await >= 1
        );
        let stop = service.stop(
            turn.workspace.id,
            turn.owner.id,
            Uuid::new_v4(),
            g5_api::services::workspace_vm_service::StopReason::User,
            None,
        );
        tokio::pin!(stop);
        // While the call is held, the stop waits ahead of the incarnation.
        assert!(
            tokio::time::timeout(Duration::from_secs(2), &mut stop)
                .await
                .is_err(),
            "the stop admission waits for the held call"
        );
        assert!(!turn
            .runtime
            .received_rpc_methods()
            .await
            .iter()
            .any(|method| method == "admission.close"));
        assert_eq!(
            app.state
                .workspace_runtime
                .calls_in_flight(turn.workspace.id),
            1
        );
        turn.runtime.release_rpc_barrier_permits(16);
        let stopped = stop.await.expect("the stop completes once the call ended");
        assert_eq!(stopped.status, "stopped");
    };
    let (response, ()) = tokio::join!(stop_request, drive);
    assert_eq!(response["disposition"], "signalling", "{response}");
    let methods = turn.runtime.received_rpc_methods().await;
    let cancel = methods
        .iter()
        .position(|m| m == "modules.cancel")
        .expect("the held cancel");
    let close = methods
        .iter()
        .position(|m| m == "admission.close")
        .expect("the stop's close");
    assert!(
        cancel < close,
        "the held call ended before the stop took the incarnation: {methods:?}"
    );
    app.cleanup().await;
}

/// A provider request's live row is provisional: the row the browser can
/// act on is the stored one, published again under the same id after the
/// batch commits, with the request's status
/// (`g5.chat.live-delivery`, `g5.chat.native-request-boundary`).
#[tokio::test]
async fn a_provider_request_row_is_published_again_once_stored() {
    let app = TestApp::spawn().await;
    let display =
        json!({"questions":[{"question":"Choose","options":[{"label":"A"},{"label":"B"}]}]});
    let deadline = chrono::Utc::now().timestamp_millis() + 60_000;
    let turn = Turn::seed(
        &app,
        vec![
            delta(1, "b1", "Choose:"),
            json!({"seq": 2, "kind": "provider_request.opened", "payload": {
                "provider_request_id": "req-1",
                "request_kind": "question",
                "tool_name": null,
                "payload": display,
                "display": display,
                "deadline_unix_ms": deadline,
            }}),
        ],
    )
    .await;
    let mut socket = turn.chat_socket(&app).await;
    reconcile(&app).await;
    let live = collect_live(&mut socket, 2, Duration::from_secs(5)).await;
    assert_eq!(seqs(&live), vec![1, 2]);
    assert_eq!(live[1]["metadata"]["kind"], "provider_request.opened");
    assert_eq!(
        live[1]["metadata"]["live"], true,
        "the live copy is provisional"
    );
    assert!(live[1]["metadata"]["nativeRequestStatus"].is_null());
    // The request is a boundary: its batch commits at once and the stored
    // row follows under the same id, actionable.
    let stored = collect_live(&mut socket, 1, Duration::from_secs(5)).await;
    assert_eq!(stored[0]["id"], live[1]["id"]);
    assert_eq!(stored[0]["metadata"]["seq"], 2);
    assert!(stored[0]["metadata"]["live"].is_null(), "{}", stored[0]);
    assert_eq!(stored[0]["metadata"]["nativeRequestStatus"], "pending");
    assert_eq!(stored[0]["type"], live[1]["type"]);
    let status: String = sqlx::query_scalar(
        "SELECT status FROM provider_requests WHERE task_id = $1 AND provider_request_id = 'req-1'",
    )
    .bind(turn.task_id)
    .fetch_one(&app.db)
    .await
    .expect("load the request");
    assert_eq!(status, "pending");
    let rows = turn.stored_rows(&app).await;
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1].0.to_string(), stored[0]["id"].as_str().unwrap());
    app.cleanup().await;
}

/// A follower lost before its batch (an API restart) is replaced by one
/// that rebuilds the tail from the journal: the same ids, no duplicate rows.
#[tokio::test]
async fn a_restarted_follower_rebuilds_the_tail_from_the_journal_without_duplicates() {
    let app = TestApp::spawn().await;
    let turn = Turn::seed(
        &app,
        vec![
            delta(1, "b1", "a"),
            delta(2, "b1", "b"),
            delta(3, "b1", "c"),
            delta(4, "b1", "d"),
        ],
    )
    .await;
    let mut socket = turn.chat_socket(&app).await;
    reconcile(&app).await;
    let first = collect_live(&mut socket, 4, Duration::from_secs(5)).await;
    app.state.followers.retire(&turn.exec_id).await;
    assert!(
        app.state
            .followers
            .wait_finished(&turn.exec_id, Duration::from_secs(5))
            .await
    );
    assert_eq!(turn.persisted_through(&app).await, 0, "the batch never ran");
    reconcile(&app).await;
    let again = collect_live(&mut socket, 4, Duration::from_secs(5)).await;
    assert_eq!(
        first.iter().map(|r| r["id"].clone()).collect::<Vec<_>>(),
        again.iter().map(|r| r["id"].clone()).collect::<Vec<_>>(),
        "the rebuilt tail carries the same identities (`g5.chat.replay`)"
    );
    assert!(
        app.state
            .followers
            .wait_persisted(&turn.exec_id, 4, Duration::from_secs(10))
            .await
    );
    let rows = turn.stored_rows(&app).await;
    assert_eq!(rows.len(), 4);
    assert_eq!(acks_after(&turn.runtime, 4).await, vec![4]);
    app.cleanup().await;
}

/// A replaced incarnation ends its follower; the reconciler settles the
/// attempt from the database alone, as before.
#[tokio::test]
async fn a_replaced_incarnation_ends_the_follower_and_the_attempt_settles_as_lost() {
    let app = TestApp::spawn().await;
    let turn = Turn::seed(&app, vec![delta(1, "b1", "Hi")]).await;
    let mut socket = turn.chat_socket(&app).await;
    reconcile(&app).await;
    collect_live(&mut socket, 1, Duration::from_secs(5)).await;
    sqlx::query("UPDATE workspace_vms SET epoch = epoch + 1 WHERE workspace_id = $1")
        .bind(turn.workspace.id)
        .execute(&app.db)
        .await
        .expect("replace the incarnation");
    app.state.followers.wake(&turn.exec_id);
    assert!(
        app.state
            .followers
            .wait_finished(&turn.exec_id, Duration::from_secs(10))
            .await,
        "the follower ends on an incarnation mismatch"
    );
    reconcile(&app).await;
    let (status, reason): (String, Option<String>) = sqlx::query_as(
        "SELECT task.status, attempt.reason FROM tasks AS task
         JOIN task_attempts AS attempt ON attempt.task_id = task.id AND attempt.attempt = 1
         WHERE task.id = $1",
    )
    .bind(turn.task_id)
    .fetch_one(&app.db)
    .await
    .expect("load the settled task");
    assert_ne!(status, "running");
    assert_eq!(reason.as_deref(), Some("vm_lost"));
    app.cleanup().await;
}

/// One runtime that never answers its journal fetch does not hold another
/// workspace's turn: followers are independent (`g5.chat.live-delivery`).
/// Fetches are bounded to half the pool, so the two-connection test pool
/// would admit one fetch at a time; a pool of eight admits four.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_stalled_runtime_does_not_hold_another_workspaces_turn() {
    let mut config = helpers::test_config();
    config.database.max_connections = 8;
    let app = TestApp::spawn_with_config(config).await;
    assert!(app.state.followers.fetch_limit() >= 2);
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let stalled_workspace = TestWorkspace::create(&app, &org).await;
    let stalled_runtime = TestWorkspaceRuntimePeer::attach_with_rpc_barrier(
        &app,
        &stalled_workspace,
        owner.id,
        "events.replay",
    )
    .await;
    let (_, stalled_task) = seed_running_turn(
        &app,
        &org,
        &stalled_workspace,
        &owner,
        stalled_runtime.vm_id(),
    )
    .await;
    let healthy_workspace = TestWorkspace::create(&app, &org).await;
    let healthy_runtime = TestWorkspaceRuntimePeer::attach_with_journal_events(
        &app,
        &healthy_workspace,
        owner.id,
        vec![delta(1, "b1", "Hi"), outcome(2)],
    )
    .await;
    let (_, healthy_task) = seed_running_turn(
        &app,
        &org,
        &healthy_workspace,
        &owner,
        healthy_runtime.vm_id(),
    )
    .await;
    reconcile(&app).await;
    let healthy_exec = canonical_exec_id(healthy_task, 1, healthy_runtime.vm_id());
    assert!(
        app.state
            .followers
            .wait_finished(&healthy_exec, Duration::from_secs(10))
            .await,
        "the healthy turn settled while the other runtime stalled"
    );
    let status: String = sqlx::query_scalar("SELECT status FROM tasks WHERE id = $1")
        .bind(healthy_task)
        .fetch_one(&app.db)
        .await
        .expect("load the healthy task");
    assert_eq!(status, "succeeded");
    let stalled_exec = canonical_exec_id(stalled_task, 1, stalled_runtime.vm_id());
    assert!(app.state.followers.is_following(&stalled_exec));
    stalled_runtime.release_rpc_barrier();
    app.cleanup().await;
}

/// The VM stop drain, the other journal consumer, reads every page of a
/// backlog larger than one reply, stores each page as one batch and
/// acknowledges the prefix before the incarnation is released
/// (`g5.chat.history-batches`, `g5.workspace.module-execution`).
#[tokio::test]
async fn the_vm_stop_drain_consumes_every_page_and_acknowledges_the_prefix() {
    let app = TestApp::spawn().await;
    let mut journal: Vec<Value> = (1..=1100)
        .map(|seq| delta(seq, "b1", &format!("w{seq} ")))
        .collect();
    journal.push(outcome(1101));
    let turn = Turn::seed(&app, journal).await;
    let db = app.scoped_db_for_user_org(turn.owner.id, turn.org.id).await;
    let scope = db.reopener();
    db.rollback().await.expect("close captured lifecycle scope");
    let service =
        g5_api::services::WorkspaceVmService::new(g5_api::services::WorkspaceVmServiceConfig {
            system_pool: app.state.system_pool.clone(),
            scope,
            hosts: app.state.preview_tunnels.clone(),
            minter: app
                .state
                .jwt_minter
                .clone()
                .expect("test app configures a lifecycle JWT minter"),
            runtime: app.state.workspace_runtime.clone(),
            agent_work: app.state.agent_work.clone(),
            repo_storage: app.state.repo_storage.clone(),
            git_external_url: app.state.git_external_url.clone(),
            ws_hub: app.state.event_bus.ws_hub().clone(),
        });
    let stopped = service
        .stop(
            turn.workspace.id,
            turn.owner.id,
            Uuid::new_v4(),
            g5_api::services::workspace_vm_service::StopReason::User,
            None,
        )
        .await
        .expect("the paged replay must not wedge the stop");
    assert_eq!(stopped.status, "stopped");
    assert_eq!(turn.persisted_through(&app).await, 1101);
    assert_eq!(turn.stored_rows(&app).await.len(), 1101);
    assert_eq!(turn.task_status(&app).await, "succeeded");
    let acks = turn.runtime.acked_seqs().await;
    assert_eq!(
        acks.last().copied(),
        Some(1101),
        "the whole prefix is acknowledged before release: {acks:?}"
    );
    assert!(
        acks.len() <= 3,
        "one acknowledgement per page, not per event: {acks:?}"
    );
    let replays = turn
        .runtime
        .received_rpc_methods()
        .await
        .iter()
        .filter(|method| method.as_str() == "events.replay")
        .count();
    assert!(
        replays >= 2,
        "a backlog over one page takes several pages: {replays}"
    );
    app.cleanup().await;
}

/// A task's resumed attempt (a native question answered after the first
/// attempt suspended) is a new execution: the reconciler claims it while
/// the first attempt's follower, which has nothing more to store, ends.
#[tokio::test]
async fn a_resumed_attempt_is_followed_while_the_first_attempts_follower_ends() {
    let app = TestApp::spawn().await;
    let turn = Turn::seed(&app, Vec::new()).await;
    reconcile(&app).await;
    assert!(app.state.followers.is_following(&turn.exec_id));
    // The first attempt suspends on a deferred question; the task moves on
    // to attempt 2 on the same incarnation.
    sqlx::query(
        "UPDATE task_attempts SET ended_at = now(), reason = 'suspended',
             outcome = '{\"status\":\"suspended\"}'::jsonb
         WHERE task_id = $1 AND attempt = 1",
    )
    .bind(turn.task_id)
    .execute(&app.db)
    .await
    .expect("suspend the first attempt");
    sqlx::query(
        "INSERT INTO task_attempts(org_id, task_id, attempt, vm_id, epoch, start_confirmed, started_at)
         VALUES ($1,$2,2,$3,1,true,now())",
    )
    .bind(turn.org.id)
    .bind(turn.task_id)
    .bind(turn.runtime.vm_id().to_string())
    .execute(&app.db)
    .await
    .expect("insert the resumed attempt");
    sqlx::query("UPDATE tasks SET attempt = 2, updated_at = now() WHERE id = $1")
        .bind(turn.task_id)
        .execute(&app.db)
        .await
        .expect("advance the task to its resumed attempt");
    turn.runtime
        .journal()
        .extend([delta(1, "b1", "Native question answered Blue."), outcome(2)])
        .await;
    app.state.followers.wake(&turn.exec_id);
    assert!(
        app.state
            .followers
            .wait_finished(&turn.exec_id, Duration::from_secs(10))
            .await,
        "the first attempt's follower ends once its attempt is superseded"
    );
    let exec_2 = canonical_exec_id(turn.task_id, 2, turn.runtime.vm_id());
    reconcile(&app).await;
    assert!(
        app.state
            .followers
            .wait_finished(&exec_2, Duration::from_secs(10))
            .await
    );
    let (status, persisted): (String, i64) = sqlx::query_as(
        "SELECT task.status, a.acked_event_seq FROM tasks AS task
         JOIN task_attempts AS a ON a.task_id = task.id AND a.attempt = 2
         WHERE task.id = $1",
    )
    .bind(turn.task_id)
    .fetch_one(&app.db)
    .await
    .expect("load the resumed attempt");
    assert_eq!(status, "succeeded");
    assert_eq!(persisted, 2, "the resumed attempt's journal was stored");
    app.cleanup().await;
}
