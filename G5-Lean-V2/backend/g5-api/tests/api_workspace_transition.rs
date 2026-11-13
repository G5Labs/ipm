//! Integration tests for the live read-write ↔ read-only *transition* endpoint
//! (`POST /api/v1/workspaces/:id/mutation-policy`).
//!
//! Each test spawns a full `TestApp` with an isolated Postgres database and
//! drives the real HTTP endpoint + `workspace_service` transition orchestration,
//! asserting at the HTTP-response and database levels. No mocks, stubs, or fakes.
//!
//! Grounds upward to `services/workspace_mutation_policy` [I10] (the `freezing`
//! drain-window semantics and the resolved restart-durability contract — crash-safe,
//! not crash-resuming; a stranded `freezing` is recovered by re-issuing "Make
//! read/write") and [I11] (the transition endpoint is `AdminWorkspaceAdministration`,
//! never self-blocked, and a `read_write` request reverts an in-progress/stranded
//! drain), and `routes/workspaces` [I5c] (the endpoint contract).

use crate::helpers;
use g5_auth::Scope;

use helpers::{TestApp, TestOrg, TestUser, TestWorkspace};
use reqwest::StatusCode;
use serde_json::{json, Value};
use tokio::time::{sleep, Duration, Instant};
use uuid::Uuid;

/// Give the actor Admin privileges for these transition scenarios.
/// `TestWorkspace::create` chooses this explicit member before the fixture-only
/// administrator, so the actor also holds current workspace ownership.
async fn promote_to_org_admin(app: &TestApp, org_id: Uuid, user_id: Uuid) {
    sqlx::query("UPDATE org_memberships SET role = 'admin' WHERE org_id = $1 AND user_id = $2")
        .bind(org_id)
        .bind(user_id)
        .execute(&app.db)
        .await
        .expect("Failed to promote test user to org Admin");
}

/// Read the persisted `(mutation_policy, mutation_policy_transition)` pair.
async fn load_policy_state(app: &TestApp, ws_id: Uuid) -> (String, Option<String>) {
    let policy: String =
        sqlx::query_scalar::<_, String>("SELECT mutation_policy FROM workspaces WHERE id = $1")
            .bind(ws_id)
            .fetch_one(&app.db)
            .await
            .expect("Failed to load mutation_policy");
    let transition: Option<String> = sqlx::query_scalar::<_, Option<String>>(
        "SELECT mutation_policy_transition FROM workspaces WHERE id = $1",
    )
    .bind(ws_id)
    .fetch_one(&app.db)
    .await
    .expect("Failed to load mutation_policy_transition");
    (policy, transition)
}

/// A workspace stranded in `freezing` by a mid-drain API restart is recovered by
/// the operator pressing **"Make read/write"** — a `read_write` transition request
/// reverts `freezing → read_write` and clears the marker. This is the resolved
/// durability contract (crash-safe, not crash-resuming): there is no boot reconcile,
/// so the manual RW-button revert is the recovery path (spec [I10]/[I11]).
#[tokio::test]
async fn revert_from_stranded_freezing_via_make_read_write() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    promote_to_org_admin(&app, org.id, user.id).await;
    let client = app.authenticated_client(&user);

    let workspace = TestWorkspace::create(&app, &org).await;
    let ws_uuid = workspace.id;
    let ws_id = ws_uuid.to_string();

    // Simulate an API process that died mid-drain, leaving the workspace stranded
    // in `freezing` (no auto-resume on restart — [I10]).
    sqlx::query("UPDATE workspaces SET mutation_policy_transition = 'freezing' WHERE id = $1")
        .bind(ws_uuid)
        .execute(&app.db)
        .await
        .expect("Failed to strand workspace in freezing");
    let (before_policy, before_transition) = load_policy_state(&app, ws_uuid).await;
    assert_eq!(before_policy, "read_write");
    assert_eq!(before_transition.as_deref(), Some("freezing"));

    // "Make read/write" reverts the stuck transition.
    let resp = client
        .post_json(
            &format!("/api/v1/workspaces/{ws_id}/mutation-policy"),
            &json!({ "policy": "read_write" }),
        )
        .await;
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "reverting a stranded freezing must succeed with 200"
    );
    let body: Value = resp.json().await.expect("json");
    assert_eq!(body["data"]["policy"], json!("read_write"));
    assert!(
        body["data"]["transition"].is_null(),
        "the freezing marker must be cleared in the response"
    );

    // The database reflects the cleared transition.
    let (after_policy, after_transition) = load_policy_state(&app, ws_uuid).await;
    assert_eq!(after_policy, "read_write");
    assert_eq!(
        after_transition, None,
        "the stranded freezing marker must be cleared in the database"
    );

    app.cleanup().await;
}

/// A `require_quiescent` read-write → read-only transition on a workspace with no
/// in-flight work commits synchronously (200, no blockers) and read-only
/// enforcement is immediately active. (Spec [I10]/[I11], routes [I5c].)
#[tokio::test]
async fn transition_read_write_to_read_only_require_quiescent_commits_and_enforces() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    promote_to_org_admin(&app, org.id, user.id).await;
    let client = app.authenticated_client(&user);

    let workspace = TestWorkspace::create(&app, &org).await;
    let ws_uuid = workspace.id;
    let ws_id = ws_uuid.to_string();

    // The fixture has no non-terminal tasks, so `require_quiescent` commits
    // directly to read_only without entering `freezing`.
    let resp = client
        .post_json(
            &format!("/api/v1/workspaces/{ws_id}/mutation-policy"),
            &json!({ "policy": "read_only", "mode": "require_quiescent" }),
        )
        .await;
    let status = resp.status();
    let body_text = resp.text().await.expect("transition response body");
    assert_eq!(
        status,
        StatusCode::OK,
        "a quiescent read_write → read_only transition commits synchronously (200): {body_text}"
    );
    let body: Value = serde_json::from_str(&body_text).expect("json");
    assert_eq!(body["data"]["policy"], json!("read_only"));
    assert!(
        body["data"]["transition"].is_null(),
        "a synchronous commit carries no freezing marker"
    );
    assert!(
        body["data"]["blockers"]
            .as_array()
            .map(|b| b.is_empty())
            .unwrap_or(true),
        "a clean commit carries no blockers"
    );

    // The database committed to read_only with no lingering transition.
    let (policy, transition) = load_policy_state(&app, ws_uuid).await;
    assert_eq!(policy, "read_only");
    assert_eq!(transition, None);

    // Read-only enforcement is now active: a direct workspace write is blocked
    // with the structured `WORKSPACE_READ_ONLY` 403.
    let upd = client
        .patch_json(
            &format!("/api/v1/workspaces/{ws_id}"),
            &json!({ "description": "nope" }),
        )
        .await;
    assert_eq!(upd.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        upd.json::<Value>().await.expect("json")["error"]["code"],
        json!("WORKSPACE_READ_ONLY"),
        "the committed read_only policy must be enforced on direct writes"
    );

    app.cleanup().await;
}

/// A `drain` read-write → read-only transition with in-flight work enters
/// `freezing` (202) rather than committing immediately, blocks new user
/// admission while it drains, then the real background driver commits to
/// read-only once the already-admitted task terminalizes. This exercises the
/// asynchronous freeze/drain/commit path rather than the quiescent fast path.
/// (Spec [I5]/[I10], routes [I5c].)
#[tokio::test]
async fn transition_read_write_to_read_only_drain_commits_after_in_flight_task_terminalizes() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(
        &app,
        org.id,
        &[
            Scope::WorkspacesRead,
            Scope::WorkspacesWrite,
            Scope::SessionsWrite,
        ],
    )
    .await;
    promote_to_org_admin(&app, org.id, user.id).await;
    let client = app.authenticated_client(&user);

    let workspace = TestWorkspace::create(&app, &org).await;
    let ws_uuid = workspace.id;
    let ws_id = ws_uuid.to_string();

    let task_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO tasks (id, org_id, workspace_id, name, kind, status, created_at, created_by) \
         VALUES ($1, $2, $3, 'drain-test-task', 'command', 'running', now(), $4)",
    )
    .bind(task_id)
    .bind(org.id)
    .bind(ws_uuid)
    .bind(user.id)
    .execute(&app.db)
    .await
    .expect("task should be seeded as non-terminal");
    let resp = client
        .post_json(
            &format!("/api/v1/workspaces/{ws_id}/mutation-policy"),
            &json!({ "policy": "read_only", "mode": "drain", "drainTimeoutSec": 10 }),
        )
        .await;
    let status = resp.status();
    let body_text = resp.text().await.expect("transition response body");
    assert_eq!(
        status,
        StatusCode::ACCEPTED,
        "drain with in-flight work must return 202/freezing: {body_text}"
    );
    let body: Value = serde_json::from_str(&body_text).expect("json");
    assert_eq!(body["data"]["policy"], json!("read_write"));
    assert_eq!(body["data"]["transition"], json!("freezing"));
    assert_eq!(body["data"]["blockers"][0]["kind"], json!("task"));
    assert_eq!(
        body["data"]["blockers"][0]["id"],
        json!(task_id.to_string())
    );

    let (freezing_policy, freezing_transition) = load_policy_state(&app, ws_uuid).await;
    assert_eq!(freezing_policy, "read_write");
    assert_eq!(freezing_transition.as_deref(), Some("freezing"));

    let blocked = client
        .post_json(
            &format!("/api/v1/workspaces/{ws_id}/sessions/start"),
            &json!({ "content": "Run the compiler." }),
        )
        .await;
    assert_eq!(
        blocked.status(),
        StatusCode::FORBIDDEN,
        "freezing must stop new agent admission"
    );
    assert_eq!(
        blocked.json::<Value>().await.expect("json")["error"]["code"],
        json!("WORKSPACE_READ_ONLY")
    );

    sqlx::query(
        "UPDATE tasks \
         SET status = 'succeeded', ended_at = now() \
         WHERE id = $1",
    )
    .bind(task_id)
    .execute(&app.db)
    .await
    .expect("task should terminalize");

    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let (policy, transition) = load_policy_state(&app, ws_uuid).await;
        if policy == "read_only" && transition.is_none() {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "background drain did not commit after terminalization; last state: policy={policy}, transition={transition:?}"
        );
        sleep(Duration::from_millis(250)).await;
    }

    let upd = client
        .patch_json(
            &format!("/api/v1/workspaces/{ws_id}"),
            &json!({ "description": "still blocked after drain" }),
        )
        .await;
    assert_eq!(upd.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        upd.json::<Value>().await.expect("json")["error"]["code"],
        json!("WORKSPACE_READ_ONLY"),
        "the drained commit must enforce steady read-only"
    );

    app.cleanup().await;
}

/// `mode` / `drainTimeoutSec` are meaningful only for a read_write → read_only
/// drain; supplying them on a read_write (RO→RW) request is rejected with a
/// `VALIDATION_ERROR` (422). The reverse transition is instantaneous. (Spec [I5c].)
#[tokio::test]
async fn make_read_write_rejects_drain_mode_params() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    promote_to_org_admin(&app, org.id, user.id).await;
    let client = app.authenticated_client(&user);

    let workspace = TestWorkspace::create(&app, &org).await;
    let ws_id = workspace.id.to_string();

    let resp = client
        .post_json(
            &format!("/api/v1/workspaces/{ws_id}/mutation-policy"),
            &json!({ "policy": "read_write", "mode": "drain" }),
        )
        .await;
    assert_eq!(
        resp.status(),
        StatusCode::UNPROCESSABLE_ENTITY,
        "mode/drainTimeoutSec apply only to a read_write → read_only transition"
    );
    assert_eq!(
        resp.json::<Value>().await.expect("json")["error"]["code"],
        json!("VALIDATION_ERROR")
    );

    app.cleanup().await;
}

#[tokio::test]
async fn taskless_agent_tasks_block_quiescence_in_every_active_state() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let client = app.authenticated_client(&owner);
    let started = client
        .post_json(
            &format!("/api/v1/workspaces/{}/sessions/start", workspace.id),
            &json!({"title":"An arbitrary action", "content":"Do some work."}),
        )
        .await;
    assert_eq!(started.status(), StatusCode::CREATED);
    let started: Value = started.json().await.unwrap();
    let task_id: Uuid = started["data"]["taskId"].as_str().unwrap().parse().unwrap();
    for state in ["queued", "running", "waiting"] {
        sqlx::query("UPDATE tasks SET status=$2 WHERE id=$1")
            .bind(task_id)
            .bind(state)
            .execute(&app.db)
            .await
            .unwrap();
        let response = client
            .post_json(
                &format!("/api/v1/workspaces/{}/mutation-policy", workspace.id),
                &json!({"policy":"read_only", "mode":"require_quiescent"}),
            )
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        let body: Value = response.json().await.unwrap();
        assert_eq!(body["data"]["policy"], "read_write", "{body}");
        assert_eq!(body["data"]["blockers"][0]["kind"], "task", "{body}");
        assert_eq!(body["data"]["blockers"][0]["id"], task_id.to_string());
        assert_eq!(load_policy_state(&app, workspace.id).await.0, "read_write");
    }
    app.cleanup().await;
}

#[tokio::test]
async fn drain_waits_for_taskless_command_cancellation_but_excludes_preview() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let client = app.authenticated_client(&owner);
    let command_id = Uuid::new_v4();
    let preview_id = Uuid::new_v4();
    for (id, kind) in [(command_id, "command"), (preview_id, "preview")] {
        sqlx::query(
            "INSERT INTO tasks (id,org_id,workspace_id,name,kind,status,created_by)
                     VALUES ($1,$2,$3,$4,$4,'running',$5)",
        )
        .bind(id)
        .bind(org.id)
        .bind(workspace.id)
        .bind(kind)
        .bind(owner.id)
        .execute(&app.db)
        .await
        .unwrap();
    }
    let response = client
        .post_json(
            &format!("/api/v1/workspaces/{}/mutation-policy", workspace.id),
            &json!({"policy":"read_only", "mode":"drain", "drainTimeoutSec":10}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["data"]["blockers"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"]["blockers"][0]["id"], command_id.to_string());
    let blocked = client
        .post_json(
            &format!("/api/v1/workspaces/{}/sessions/start", workspace.id),
            &json!({"content":"Cannot admit while draining."}),
        )
        .await;
    assert_eq!(blocked.status(), StatusCode::FORBIDDEN);
    sqlx::query("UPDATE tasks SET status='cancelled' WHERE id=$1")
        .bind(command_id)
        .execute(&app.db)
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            if load_policy_state(&app, workspace.id).await == ("read_only".into(), None) {
                break;
            }
            sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("drain should commit once the command is cancelled");
    let preview_status: String = sqlx::query_scalar("SELECT status FROM tasks WHERE id=$1")
        .bind(preview_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(preview_status, "running");
    app.cleanup().await;
}

#[tokio::test]
async fn read_only_commit_rechecks_task_admitted_under_workspace_share_lock() {
    use g5_api::services::workspace_messages::{MessageTarget, WorkspaceMessage};
    use g5_api::services::workspace_mutation_policy::ensure_workspace_mutable_by;

    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let client = app.authenticated_client(&owner);
    let mut admission = app.scoped_db_for_org(org.id).await;
    ensure_workspace_mutable_by(&mut admission, workspace.id, owner.id, "chat.start_session")
        .await
        .unwrap();
    let admission_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(admission.conn())
        .await
        .unwrap();
    let sent = app
        .state
        .workspace_messenger
        .send(
            &mut admission,
            WorkspaceMessage {
                org_id: org.id,
                workspace_id: workspace.id,
                actor_user_id: owner.id,
                target: MessageTarget::NewSession {
                    title: None,
                    scope_type: None,
                    scope_ref: None,
                },
                content: "Admitted concurrently with read-only.".into(),
                agent_launch: None,
                legacy_preferences: Default::default(),
                attachment_digest: None,
                request_key: None,
                workbench: None,
            },
        )
        .await
        .unwrap();
    let task_id = sent.queued.task_id;
    // The queued task is uncommitted, so the initial quiescence read cannot
    // see it. Wait until the real transition reaches the workspace lock, then
    // publish the admission. The final check must now see this task.
    let path = format!("/api/v1/workspaces/{}/mutation-policy", workspace.id);
    let request = json!({"policy":"read_only", "mode":"require_quiescent"});
    let (response, ()) = tokio::time::timeout(Duration::from_secs(10), async {
        tokio::join!(client.post_json(&path, &request), async {
            loop {
                let waiting: bool = sqlx::query_scalar(
                    "SELECT EXISTS (SELECT 1 FROM pg_stat_activity WHERE $1 = ANY(pg_blocking_pids(pid)))",
                ).bind(admission_pid).fetch_one(&app.db).await.unwrap();
                if waiting { break; }
                sleep(Duration::from_millis(10)).await;
            }
            admission.commit().await.unwrap();
        })
    }).await.expect("transition should serialize with admission");
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["data"]["policy"], "read_write", "{body}");
    assert_eq!(
        body["data"]["blockers"][0]["id"],
        task_id.to_string(),
        "{body}"
    );
    assert_eq!(
        load_policy_state(&app, workspace.id).await,
        ("read_write".into(), None)
    );
    app.cleanup().await;
}
