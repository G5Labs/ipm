//! Ontology changes cards in workspace chats (`g5.chat.ontology-card`): the
//! running turn's record kept for its own VM, the card settled onto a
//! succeeded turn's reply, and the owner's edits on the newest card
//! (`g5.chat.ontology-card.edit`).

use reqwest::StatusCode;
use serde_json::{json, Value};
use std::time::Duration;
use uuid::Uuid;

use crate::helpers;
use g5_api::services::task_lifecycle::{canonical_agent_message_id, canonical_exec_id};
use helpers::{TestApp, TestOrg, TestUser, TestWorkspace};

const FILE: &str = ".g5/ontology/features/chat.g5.md";

fn page(body: &str) -> String {
    format!(
        "# Chat\n\n<a id=\"x.a\"></a>\n[I · x.a]\n\n{body}\n\n<a id=\"x.b\"></a>\n[D · x.b]\n\nBeta.\n\n## Metadata\n\n### x.a\n\n- agent\n\n### x.b\n\n- agent\n"
    )
}

fn statement(id: &str, kind: &str, body: &str) -> Value {
    json!({
        "source": format!("<a id=\"{id}\"></a>\n[{kind} · {id}]\n\n{body}\n\n"),
        "metadata": "\n\n- agent\n\n",
    })
}

fn card(repo_id: Uuid) -> Value {
    json!({
        "schema": "g5.ontology-card.v1",
        "files": [{
            "repoId": repo_id,
            "rootPath": "code",
            "path": FILE,
            "changes": [{
                "id": "x.a",
                "change": "modified",
                "before": statement("x.a", "I", "Alpha."),
                "after": statement("x.a", "I", "Alpha, changed by the agent."),
            }],
        }],
        "governing": [],
    })
}

/// A chat session and its running agent turn on `vm_id`, with the attempt's
/// assistant placeholder, as dispatch leaves them once the module started.
async fn seed_turn(
    app: &TestApp,
    org: &TestOrg,
    workspace: &TestWorkspace,
    owner: &TestUser,
    vm_id: Uuid,
    session_id: Option<Uuid>,
    message_index: i32,
) -> (Uuid, Uuid) {
    let session_id = match session_id {
        Some(id) => id,
        None => {
            let id = Uuid::new_v4();
            sqlx::query(
                "INSERT INTO chat_sessions(id, org_id, workspace_id, scope_type, title, created_by)
                 VALUES ($1,$2,$3,'workspace','Cards',$4)",
            )
            .bind(id)
            .bind(org.id)
            .bind(workspace.id)
            .bind(owner.id)
            .execute(&app.db)
            .await
            .expect("insert session");
            id
        }
    };
    let task_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO tasks(
             id, org_id, workspace_id, name, kind, status, spec, attempt,
             vm_id, epoch, start_confirmed, started_at, chat_session_id, created_by
         ) VALUES ($1,$2,$3,'Card turn','agent','running','{}'::jsonb,1,
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
        "INSERT INTO task_attempts(org_id, task_id, attempt, vm_id, epoch, start_confirmed, started_at)
         VALUES ($1,$2,1,$3,1,true,now())",
    )
    .bind(org.id)
    .bind(task_id)
    .bind(vm_id.to_string())
    .execute(&app.db)
    .await
    .expect("insert running attempt");
    sqlx::query(
        "INSERT INTO chat_messages(
             id, org_id, session_id, role, content, citations, status,
             message_index, created_at, task_id, attempt)
         VALUES ($1,$2,$3,'assistant','','[]'::jsonb,'pending',$5,now(),$4,1)",
    )
    .bind(canonical_agent_message_id(task_id, 1))
    .bind(org.id)
    .bind(session_id)
    .bind(task_id)
    .bind(message_index)
    .execute(&app.db)
    .await
    .expect("insert the attempt's assistant placeholder");
    (session_id, task_id)
}

fn outcome(seq: u64, status: &str) -> Value {
    json!({
        "seq": seq,
        "kind": "invocation.outcome",
        "payload": {
            "status": "succeeded",
            "result": {
                "status": status, "code": if status == "succeeded" { "exit_code" } else { "provider_error" },
                "retryable": false, "provider_started": true,
            },
        },
        "terminal": true,
    })
}

fn journal(repo_id: Uuid, status: &str) -> Vec<Value> {
    vec![
        json!({"seq": 1, "kind": "module.message.delta", "payload": {"text_delta": "Changed the ontology."}}),
        json!({"seq": 2, "kind": "module.ontology.card", "payload": card(repo_id)}),
        outcome(3, status),
    ]
}

async fn reply_artifacts(app: &TestApp, task_id: Uuid) -> Option<Value> {
    sqlx::query_scalar::<_, Option<Value>>("SELECT artifacts FROM chat_messages WHERE id = $1")
        .bind(canonical_agent_message_id(task_id, 1))
        .fetch_one(&app.db)
        .await
        .expect("load the reply")
}

async fn reconcile_turn(app: &TestApp, task_id: Uuid, vm_id: Uuid) {
    g5_api_core::maintenance::agent_tasks::run_once(&app.state)
        .await
        .expect("run one coordinator pass");
    // The coordinator starts a follower; returning from the pass does not
    // mean its journal has committed. Keep the assertions after that boundary.
    assert!(
        app.state
            .followers
            .wait_finished(
                &canonical_exec_id(task_id, 1, vm_id),
                Duration::from_secs(10)
            )
            .await,
        "the turn's follower must finish before its result is inspected"
    );
}

/// A succeeded turn's card is copied onto its reply, the event is stored
/// under its own kind, and the turn's record is dropped.
#[tokio::test]
async fn a_succeeded_turn_ends_its_reply_with_its_card() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = helpers::TestWorkspaceRuntimePeer::attach_with_journal_events(
        &app,
        &workspace,
        owner.id,
        journal(workspace.repo_id, "succeeded"),
    )
    .await;
    let (session_id, task_id) =
        seed_turn(&app, &org, &workspace, &owner, runtime.vm_id(), None, 1).await;
    sqlx::query(
        "INSERT INTO chat_turn_ontology(org_id, workspace_id, task_id, baseline, baseline_attempt)
         VALUES ($1,$2,$3,'{\"members\":[]}'::jsonb,1)",
    )
    .bind(org.id)
    .bind(workspace.id)
    .bind(task_id)
    .execute(&app.db)
    .await
    .expect("record the turn's start");

    reconcile_turn(&app, task_id, runtime.vm_id()).await;

    let status: String = sqlx::query_scalar("SELECT status FROM tasks WHERE id = $1")
        .bind(task_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(status, "succeeded");
    let artifacts = reply_artifacts(&app, task_id)
        .await
        .expect("the reply carries its card");
    assert_eq!(artifacts["ontology_card"]["schema"], "g5.ontology-card.v1");
    assert_eq!(
        artifacts["ontology_card"]["files"][0]["changes"][0]["id"],
        "x.a"
    );
    assert_eq!(artifacts["ontology_card"]["edits"], json!([]));
    let kind: String = sqlx::query_scalar(
        "SELECT metadata ->> 'kind' FROM task_events WHERE task_id = $1 AND seq = 2",
    )
    .bind(task_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(kind, "ontology.card");
    let kept: i64 =
        sqlx::query_scalar("SELECT count(*) FROM chat_turn_ontology WHERE task_id = $1")
            .bind(task_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(kept, 0, "an ended turn's record is dropped");

    // Reload returns the card on the same message.
    let response = app
        .authenticated_client(&owner)
        .get(&format!("/api/v1/sessions/{session_id}/messages"))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    let reply = body["data"]["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == canonical_agent_message_id(task_id, 1).to_string())
        .expect("the reply is listed");
    assert_eq!(
        reply["artifacts"]["ontology_card"]["files"][0]["path"],
        FILE
    );
    app.cleanup().await;
}

/// A failed turn has no card, even when its runner emitted one.
#[tokio::test]
async fn a_failed_turn_has_no_card() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = helpers::TestWorkspaceRuntimePeer::attach_with_journal_events(
        &app,
        &workspace,
        owner.id,
        journal(workspace.repo_id, "failed"),
    )
    .await;
    let (_, task_id) = seed_turn(&app, &org, &workspace, &owner, runtime.vm_id(), None, 1).await;
    reconcile_turn(&app, task_id, runtime.vm_id()).await;
    let status: String = sqlx::query_scalar("SELECT status FROM tasks WHERE id = $1")
        .bind(task_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(status, "failed");
    let artifacts = reply_artifacts(&app, task_id).await;
    assert!(
        artifacts
            .as_ref()
            .is_none_or(|a| a.get("ontology_card").is_none()),
        "{artifacts:?}"
    );
    app.cleanup().await;
}

/// The workspace VM's credential, for a running VM row of the workspace.
async fn agent_token(
    app: &TestApp,
    org_id: Uuid,
    workspace_id: Uuid,
    owner: Uuid,
) -> (String, Uuid) {
    let host_id = Uuid::new_v4();
    let vm_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO worker_hosts (host_id, org_id, region, status, last_heartbeat, slot_size_reported)
         VALUES ($1, $2, 'us-central1', 'ready', now(), true)",
    )
    .bind(host_id)
    .bind(org_id)
    .execute(&app.db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO workspace_vms (org_id, workspace_id, host_id, vm_id, epoch, status, lease_expires_at)
         VALUES ($1, $2, $3, $4, 1, 'running', now() + interval '1 hour')",
    )
    .bind(org_id)
    .bind(workspace_id)
    .bind(host_id)
    .bind(vm_id.to_string())
    .execute(&app.db)
    .await
    .unwrap();
    let token = app
        .state
        .jwt_minter
        .as_ref()
        .expect("test jwt minter")
        .mint_workspace_incarnation(org_id, workspace_id, owner, vm_id, 1, 3600)
        .unwrap();
    (token, vm_id)
}

async fn agent_call(
    app: &TestApp,
    token: &str,
    method: reqwest::Method,
    workspace_id: Uuid,
    rest: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut request = app
        .client
        .request(
            method,
            format!(
                "{}/api/internal/workspace-agent/{workspace_id}/{rest}",
                app.base_url
            ),
        )
        .bearer_auth(token);
    if let Some(body) = body {
        request = request.json(&body);
    }
    let response = request.send().await.unwrap();
    let status = response.status();
    (status, response.json().await.unwrap_or(Value::Null))
}

fn governing(id: &str, repo_id: Uuid, reason: &str) -> Value {
    json!({"statementId": id, "repoId": repo_id, "path": FILE, "reason": reason})
}

/// The running turn's own VM reads and records its start once, and names up
/// to eight governing statements; ended turns, other VMs and other
/// workspaces are refused.
#[tokio::test]
async fn only_the_running_turns_vm_keeps_its_ontology_record() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let (token, vm_id) = agent_token(&app, org.id, workspace.id, owner.id).await;
    let (_, task_id) = seed_turn(&app, &org, &workspace, &owner, vm_id, None, 1).await;
    let ws = workspace.id;
    let get = reqwest::Method::GET;
    let post = reqwest::Method::POST;
    let path = format!("chat-tasks/{task_id}/ontology");

    let (status, body) = agent_call(&app, &token, get.clone(), ws, &path, None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"], json!({"baseline": null, "governing": []}));

    let first = json!({"members": [{"repo_id": "r", "base_commit": "abc"}]});
    let (status, body) = agent_call(
        &app,
        &token,
        post.clone(),
        ws,
        &format!("{path}/baseline"),
        Some(json!({"attempt": 1, "baseline": first})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["baseline"], first);
    let (_, body) = agent_call(
        &app,
        &token,
        post.clone(),
        ws,
        &format!("{path}/baseline"),
        Some(json!({"attempt": 1, "baseline": {"members": [{"repo_id": "r", "base_commit": "later"}]}})),
    )
    .await;
    assert_eq!(body["data"]["baseline"], first, "the first start stands");
    let (status, _) = agent_call(
        &app,
        &token,
        post.clone(),
        ws,
        &format!("{path}/baseline"),
        Some(json!({"attempt": 2, "baseline": first})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "another attempt is refused");

    for i in 0..8 {
        let (status, body) = agent_call(
            &app,
            &token,
            post.clone(),
            ws,
            &format!("{path}/governing"),
            Some(governing(
                &format!("g.s{i}"),
                workspace.repo_id,
                "It governs.",
            )),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
    }
    let (status, _) = agent_call(
        &app,
        &token,
        post.clone(),
        ws,
        &format!("{path}/governing"),
        Some(governing("g.s9", workspace.repo_id, "One too many.")),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "a ninth statement is refused");
    let (status, body) = agent_call(
        &app,
        &token,
        post.clone(),
        ws,
        &format!("{path}/governing"),
        Some(governing("g.s0", workspace.repo_id, "A better reason.")),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let list = body["data"]["governing"].as_array().unwrap();
    assert_eq!(list.len(), 8);
    assert_eq!(list[0]["reason"], "A better reason.");
    let (status, _) = agent_call(
        &app,
        &token,
        post.clone(),
        ws,
        &format!("{path}/governing"),
        Some(json!({"statementId": "g.x", "repoId": workspace.repo_id, "path": "../etc/passwd", "reason": "x"})),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    // Another chat's task on another VM of the workspace is not this VM's.
    let other_vm = Uuid::new_v4();
    let (_, other_task) = seed_turn(&app, &org, &workspace, &owner, other_vm, None, 1).await;
    let (status, _) = agent_call(
        &app,
        &token,
        get.clone(),
        ws,
        &format!("chat-tasks/{other_task}/ontology"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    // Another workspace's caller cannot name this workspace.
    let other_workspace = TestWorkspace::create(&app, &org).await;
    let (other_token, _) = agent_token(&app, org.id, other_workspace.id, owner.id).await;
    let (status, _) = agent_call(&app, &other_token, get.clone(), ws, &path, None).await;
    assert!(
        status == StatusCode::FORBIDDEN || status == StatusCode::UNAUTHORIZED,
        "{status}"
    );
    let (status, _) = agent_call(
        &app,
        &other_token,
        get.clone(),
        other_workspace.id,
        &path,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "a task of another workspace");
    // An ended turn is refused, as a background process of an earlier turn.
    sqlx::query("UPDATE tasks SET status = 'succeeded' WHERE id = $1")
        .bind(task_id)
        .execute(&app.db)
        .await
        .unwrap();
    let (status, _) = agent_call(
        &app,
        &token,
        post.clone(),
        ws,
        &format!("{path}/governing"),
        Some(governing("g.late", workspace.repo_id, "Too late.")),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    // Without the VM's credential, nothing is reachable.
    let response = app
        .client
        .get(format!(
            "{}/api/internal/workspace-agent/{ws}/{path}",
            app.base_url
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    app.cleanup().await;
}

/// Settle a seeded turn with a card through the coordinator.
async fn settle_with_card(
    app: &TestApp,
    org: &TestOrg,
    workspace: &TestWorkspace,
    owner: &TestUser,
    runtime: &helpers::TestWorkspaceRuntimePeer,
    session_id: Option<Uuid>,
    message_index: i32,
) -> (Uuid, Uuid) {
    let (session_id, task_id) = seed_turn(
        app,
        org,
        workspace,
        owner,
        runtime.vm_id(),
        session_id,
        message_index,
    )
    .await;
    reconcile_turn(app, task_id, runtime.vm_id()).await;
    (session_id, task_id)
}

fn edit(repo_id: Uuid, expected: &str, text: &str, key: &str) -> Value {
    json!({
        "repoId": repo_id,
        "path": FILE,
        "statementId": "x.a",
        "expectedText": expected,
        "text": text,
        "idempotencyKey": key,
    })
}

async fn notices(app: &TestApp, session_id: Uuid) -> Vec<(String, Value)> {
    sqlx::query_as(
        "SELECT content, artifacts FROM chat_messages
         WHERE session_id = $1 AND role = 'system' ORDER BY message_index",
    )
    .bind(session_id)
    .fetch_all(&app.db)
    .await
    .unwrap()
}

/// The owner's edit on the newest card writes only that statement under the
/// editor's version check, is recorded once per key, and queues one notice
/// for the next turn that survives a workspace Stop.
#[tokio::test]
async fn the_owner_edits_a_statement_on_the_newest_card() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = helpers::TestWorkspaceRuntimePeer::attach_with_journal_events(
        &app,
        &workspace,
        owner.id,
        journal(workspace.repo_id, "succeeded"),
    )
    .await;
    runtime
        .seed_file(
            workspace.repo_id,
            FILE,
            &page("Alpha, changed by the agent."),
        )
        .await;
    let (session_id, task_id) =
        settle_with_card(&app, &org, &workspace, &owner, &runtime, None, 1).await;
    let message_id = canonical_agent_message_id(task_id, 1);
    let client = app.authenticated_client(&owner);
    let url = format!("/api/v1/sessions/{session_id}/messages/{message_id}/ontology-card/edits");

    // The card's text must match what the browser showed.
    let response = client
        .post_json(
            &url,
            &edit(workspace.repo_id, "Stale text.", "The owner's text.", "k1"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(
        runtime.file_content(workspace.repo_id, FILE).await.unwrap(),
        page("Alpha, changed by the agent.")
    );

    let response = client
        .post_json(
            &url,
            &edit(
                workspace.repo_id,
                "Alpha, changed by the agent.",
                "The owner's text.",
                "k1",
            ),
        )
        .await;
    let status = response.status();
    let body: Value = response.json().await.unwrap();
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["text"], "The owner's text.");
    assert_eq!(
        runtime.file_content(workspace.repo_id, FILE).await.unwrap(),
        page("The owner's text."),
        "only the statement's body changes"
    );
    let artifacts = reply_artifacts(&app, task_id).await.unwrap();
    assert_eq!(
        artifacts["ontology_card"]["edits"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        artifacts["ontology_card"]["files"][0]["changes"][0]["after"],
        statement("x.a", "I", "Alpha, changed by the agent."),
        "the card keeps the agent's text"
    );
    let queued = notices(&app, session_id).await;
    assert_eq!(queued.len(), 1);
    assert!(
        queued[0].0.contains("`x.a`") && queued[0].0.contains(FILE),
        "{}",
        queued[0].0
    );
    assert_eq!(queued[0].1["platform_notice"]["delivery"], "queued");
    assert_eq!(queued[0].1["platform_notice"]["kind"], "ontology_card_edit");

    // A retry of the same key replays the edit: no second write or notice.
    let response = client
        .post_json(
            &url,
            &edit(
                workspace.repo_id,
                "Alpha, changed by the agent.",
                "The owner's text.",
                "k1",
            ),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(notices(&app, session_id).await.len(), 1);
    assert_eq!(
        reply_artifacts(&app, task_id).await.unwrap()["ontology_card"]["edits"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    // The same key for a different edit is refused, not answered with k1's.
    let response = client
        .post_json(
            &url,
            &edit(
                workspace.repo_id,
                "Alpha, changed by the agent.",
                "Other text.",
                "k1",
            ),
        )
        .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);

    // A second edit compares with the owner's text the card now shows.
    let response = client
        .post_json(
            &url,
            &edit(
                workspace.repo_id,
                "Alpha, changed by the agent.",
                "Third.",
                "k2",
            ),
        )
        .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);

    // Malformed text is refused and nothing is written.
    let response = client
        .post_json(
            &url,
            &edit(
                workspace.repo_id,
                "The owner's text.",
                "New.\n\n<a id=\"x.c\"></a>\n[I · x.c]\n\nSmuggled.",
                "k3",
            ),
        )
        .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        runtime.file_content(workspace.repo_id, FILE).await.unwrap(),
        page("The owner's text.")
    );

    // A file another writer changed since it was read is refused by the
    // version check, and nothing is recorded.
    runtime.refuse_writes(workspace.repo_id, FILE).await;
    let response = client
        .post_json(
            &url,
            &edit(workspace.repo_id, "The owner's text.", "Fourth.", "k4"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    runtime.allow_writes(workspace.repo_id, FILE).await;
    assert_eq!(notices(&app, session_id).await.len(), 1);

    // The notice is kept, not withdrawn, when the workspace's VM stops.
    let mut tx = app.db.begin().await.unwrap();
    g5_api_core::services::chat_service::withdraw_workspace_notices_for_test(
        &mut tx,
        workspace.id,
        "workspace_stopped",
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(
        notices(&app, session_id).await[0].1["platform_notice"]["delivery"],
        "queued"
    );

    // A recorded edit still replays after a turn has started: its reply was
    // lost, not the edit.
    seed_turn(
        &app,
        &org,
        &workspace,
        &owner,
        Uuid::new_v4(),
        Some(session_id),
        10,
    )
    .await;
    let response = client
        .post_json(
            &url,
            &edit(
                workspace.repo_id,
                "Alpha, changed by the agent.",
                "The owner's text.",
                "k1",
            ),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(notices(&app, session_id).await.len(), 1);
    app.cleanup().await;
}

/// Only the owner edits, only on the newest card, and only between turns.
#[tokio::test]
async fn card_edits_are_refused_to_readers_older_cards_and_running_turns() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = helpers::TestWorkspaceRuntimePeer::attach_with_journal_events(
        &app,
        &workspace,
        owner.id,
        journal(workspace.repo_id, "succeeded"),
    )
    .await;
    runtime
        .seed_file(
            workspace.repo_id,
            FILE,
            &page("Alpha, changed by the agent."),
        )
        .await;
    let (session_id, first_task) =
        settle_with_card(&app, &org, &workspace, &owner, &runtime, None, 1).await;
    let first_message = canonical_agent_message_id(first_task, 1);
    let owner_client = app.authenticated_client(&owner);
    let body = edit(
        workspace.repo_id,
        "Alpha, changed by the agent.",
        "Edited.",
        "r1",
    );

    // A member who is not the owner is refused.
    let member = TestUser::create_with_scopes(
        &app,
        org.id,
        &[
            g5_auth::Scope::SessionsRead,
            g5_auth::Scope::SessionsWrite,
            g5_auth::Scope::WorkspacesRead,
            g5_auth::Scope::WorkspacesWrite,
        ],
    )
    .await;
    let response = app
        .authenticated_client(&member)
        .post_json(
            &format!("/api/v1/sessions/{session_id}/messages/{first_message}/ontology-card/edits"),
            &body,
        )
        .await;
    assert!(
        response.status() == StatusCode::FORBIDDEN || response.status() == StatusCode::NOT_FOUND,
        "{}",
        response.status()
    );

    // While a turn of the chat runs, the owner is refused too.
    let (_, running) = seed_turn(
        &app,
        &org,
        &workspace,
        &owner,
        Uuid::new_v4(),
        Some(session_id),
        3,
    )
    .await;
    let response = owner_client
        .post_json(
            &format!("/api/v1/sessions/{session_id}/messages/{first_message}/ontology-card/edits"),
            &body,
        )
        .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    sqlx::query("UPDATE tasks SET status = 'cancelled' WHERE id = $1")
        .bind(running)
        .execute(&app.db)
        .await
        .unwrap();

    // A later card makes the first one an earlier card.
    sqlx::query(
        "UPDATE chat_messages SET artifacts = jsonb_build_object('ontology_card', $2::jsonb), status = 'complete'
         WHERE id = $1",
    )
    .bind(canonical_agent_message_id(running, 1))
    .bind(card(workspace.repo_id))
    .execute(&app.db)
    .await
    .unwrap();
    let response = owner_client
        .post_json(
            &format!("/api/v1/sessions/{session_id}/messages/{first_message}/ontology-card/edits"),
            &body,
        )
        .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(
        runtime.file_content(workspace.repo_id, FILE).await.unwrap(),
        page("Alpha, changed by the agent."),
        "no refused edit writes the file"
    );
    app.cleanup().await;
}
