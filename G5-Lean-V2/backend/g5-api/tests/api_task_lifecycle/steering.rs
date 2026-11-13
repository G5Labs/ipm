use super::*;
use g5_api::services::agents::agent_steering::{ClaimedSteer, SteerDisposition};

#[path = "acceptance.rs"]
mod acceptance;

struct Running {
    owner: TestUser,
    org: TestOrg,
    workspace: TestWorkspace,
    task: Uuid,
    session: Uuid,
    vm: Uuid,
    peer: Option<helpers::runtime_peer::TestWorkspaceRuntimePeer>,
}

async fn running(app: &TestApp, provider: &str) -> Running {
    running_with_peer(app, provider, false).await
}

async fn running_with_peer(app: &TestApp, provider: &str, connected: bool) -> Running {
    let mut owner = TestUser::create(app).await;
    let org = TestOrg::create_with_admin(app, &mut owner).await;
    helpers::seed_org_llm_credentials_all_providers(app, org.id).await;
    let workspace = TestWorkspace::create(app, &org).await;
    let peer = if connected {
        Some(
            helpers::runtime_peer::TestWorkspaceRuntimePeer::attach_with_rpc_barrier(
                app,
                &workspace,
                owner.id,
                "modules.input.steer",
            )
            .await,
        )
    } else {
        None
    };
    let vm = if let Some(peer) = &peer {
        peer.vm_id()
    } else {
        attach_running_vm_without_runtime(app, &workspace).await
    };
    let (task, session) = create_agent_task(app, &workspace, owner.id, "queued", None, None).await;
    let mut launch = json!({"schemaVersion":1,"provider":{"source":"explicit","value":provider}});
    if provider == "codex" {
        launch["permissionMode"] = json!({"source":"explicit","value":"never/read-only"});
    }
    sqlx::query("UPDATE tasks SET spec=jsonb_set(spec,'{agent,agentLaunch}', $2) WHERE id=$1")
        .bind(task)
        .bind(launch)
        .execute(&app.db)
        .await
        .unwrap();
    queue_session_input(app, org.id, session, task, "initial input").await;
    let mut db = app.scoped_db_for_org(org.id).await;
    let dispatch = app
        .state
        .task_service
        .reserve_agent_dispatch(&mut db)
        .await
        .unwrap()
        .expect("reserve initial input");
    assert_eq!(
        dispatch.invoke_params["input"]["provider"]["options"]["native_mid_turn_steering"],
        provider == "claude_code"
    );
    app.state
        .task_service
        .apply(
            &mut db,
            dispatch.start_disposition(json!({"disposition":"started"})),
        )
        .await
        .unwrap();
    db.commit().await.unwrap();
    apply_agent_event(
        app,
        org.id,
        (task, vm),
        1,
        "input.accepted",
        dispatch.invoke_params["input"]["task"]["initial_input"].clone(),
        false,
    )
    .await;
    Running {
        owner,
        org,
        workspace,
        task,
        session,
        vm,
        peer,
    }
}

async fn enqueue(app: &TestApp, r: &Running, content: &str, artifacts: Value) -> Uuid {
    let mut db = app.scoped_db_for_org(r.org.id).await;
    let input = app
        .state
        .task_service
        .enqueue_agent_input(
            &mut db,
            EnqueueAgentInput {
                session_id: r.session,
                actor_user_id: r.owner.id,
                task_name: None,
                content: content.into(),
                artifacts,
                input_origin: "human".into(),
                template_ref: None,
                template_revision: None,
                confirmed_launch: None,
                agent_launch: Default::default(),
            },
        )
        .await
        .unwrap();
    db.commit().await.unwrap();
    input.input_id
}

async fn reserve(app: &TestApp, r: &Running, input: Uuid) -> Option<ClaimedSteer> {
    let mut db = app.scoped_db_for_org(r.org.id).await;
    let claim = app
        .state
        .task_service
        .reserve_agent_steer(&mut db, r.owner.id, r.session, input)
        .await
        .unwrap();
    db.commit().await.unwrap();
    claim
}

fn evidence(c: &ClaimedSteer) -> Value {
    json!({"input_id":c.input_id,"reservation_id":c.reservation_id,"revision":c.revision,
        "delivery_generation":c.delivery_generation,"request_id":format!("{}:{}",c.input_id,c.delivery_generation)})
}

async fn delivery(app: &TestApp, input: Uuid) -> String {
    sqlx::query_scalar("SELECT delivery FROM session_inputs WHERE id=$1")
        .bind(input)
        .fetch_one(&app.db)
        .await
        .unwrap()
}

#[tokio::test]
async fn native_steering_http_commits_queue_before_one_rpc_and_preserves_owner_authority() {
    let app = TestApp::spawn().await;
    let mut r = running_with_peer(&app, "claude_code", true).await;
    let frozen: Value = sqlx::query_scalar(
        "SELECT policy_snapshot_ref FROM task_attempts WHERE task_id=$1 AND attempt=1",
    )
    .bind(r.task)
    .fetch_one(&app.db)
    .await
    .unwrap();
    let model = if frozen["agent_launch"]["resolved"]["model"]["value"] == "claude-sonnet-4-6" {
        "claude-opus-4-8[1m]"
    } else {
        "claude-sonnet-4-6"
    };
    let path = format!("/api/v1/sessions/{}/messages", r.session);
    let reader = TestUser::create_with_scopes(
        &app,
        r.org.id,
        &[
            Scope::SessionsWrite,
            Scope::WorkspacesRead,
            Scope::WorkspacesWrite,
        ],
    )
    .await;
    let denied = app
        .authenticated_client(&reader)
        .post_json(&path, &json!({"content":"not the owner"}))
        .await;
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
    let content = "  HTTP steering stays exact\n  ";
    let (response, input) = tokio::join!(
        async {
            app.authenticated_client(&r.owner)
                .post_json(
                    &path,
                    &json!({"content":content,"agentProvider":"claude_code","model":model,"effort":"high"}),
                )
                .await
        },
        async {
            tokio::time::timeout(std::time::Duration::from_secs(10), async {
                while !r
                    .peer
                    .as_ref()
                    .unwrap()
                    .received_rpc_methods()
                    .await
                    .iter()
                    .any(|method| method == "modules.input.steer")
                {
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();
            let row:(Uuid,String,Uuid,i32,i32)=sqlx::query_as(
            "SELECT id,content,reservation_id,revision,delivery_generation FROM session_inputs WHERE session_id=$1 AND steered")
            .bind(r.session).fetch_one(&app.db).await.unwrap();
            assert_eq!(row.1, content);
            assert_eq!(delivery(&app, row.0).await, "queued");
            // The journaled write is the delivery fact even before the RPC replies.
            apply_agent_event(&app,r.org.id,(r.task,r.vm),2,"input.shipped",
            json!({"input_id":row.0,"reservation_id":row.2,"revision":row.3,"delivery_generation":row.4}),false).await;
            r.peer.as_ref().unwrap().release_rpc_barrier();
            row.0
        }
    );
    assert_eq!(response.status(), StatusCode::CREATED);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["data"]["inputId"], input.to_string());
    assert_eq!(
        body["data"]["delivery"], "queued",
        "Send returns its committed acceptance snapshot"
    );
    assert_eq!(delivery(&app, input).await, "delivered");
    let after: Value = sqlx::query_scalar(
        "SELECT policy_snapshot_ref FROM task_attempts WHERE task_id=$1 AND attempt=1",
    )
    .bind(r.task)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(
        after, frozen,
        "steering retains the running attempt's choices"
    );
    let preferences: Value =
        sqlx::query_scalar("SELECT launch_preferences FROM session_inputs WHERE id=$1")
            .bind(input)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(preferences["model"]["value"], model);
    let methods = r.peer.as_ref().unwrap().received_rpc_methods().await;
    assert_eq!(
        methods
            .iter()
            .filter(|m| *m == "modules.input.steer")
            .count(),
        1
    );
    assert!(!methods.iter().any(|m| m == "modules.cancel"));
    drop(r.peer.take());
    app.cleanup().await;
}

#[tokio::test]
async fn native_steering_deadline_leaves_one_queued_input_without_background_retry() {
    let app = TestApp::spawn().await;
    let mut r = running_with_peer(&app, "claude_code", true).await;
    let path = format!("/api/v1/sessions/{}/messages", r.session);
    let started = std::time::Instant::now();
    let response = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        app.authenticated_client(&r.owner)
            .post_json(&path, &json!({"content":"deadline fallback"}))
            .await
    })
    .await
    .expect("acceptance does not wait for the steering deadline");
    assert_eq!(response.status(), StatusCode::CREATED);
    let body: Value = response.json().await.unwrap();
    let input = Uuid::parse_str(body["data"]["inputId"].as_str().unwrap()).unwrap();
    assert_eq!(body["data"]["delivery"], "queued");
    let binding: (bool, Option<Uuid>) =
        sqlx::query_as("SELECT steered,reservation_id FROM session_inputs WHERE id=$1")
            .bind(input)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(binding.0 && binding.1.is_some());
    // The accepted response has returned, but the guest RPC is still blocked.
    // Its deadline must independently release the input to the ordinary queue.
    tokio::time::timeout(std::time::Duration::from_secs(55), async {
        loop {
            let binding: (bool, Option<Uuid>) =
                sqlx::query_as("SELECT steered,reservation_id FROM session_inputs WHERE id=$1")
                    .bind(input)
                    .fetch_one(&app.db)
                    .await
                    .unwrap();
            if binding == (false, None) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the steering deadline releases the reservation");
    assert!(started.elapsed() >= std::time::Duration::from_secs(44));
    let methods = r.peer.as_ref().unwrap().received_rpc_methods().await;
    assert_eq!(
        methods
            .iter()
            .filter(|m| *m == "modules.input.steer")
            .count(),
        1,
        "the deadline expires while the one delivery RPC is held"
    );
    r.peer.as_ref().unwrap().release_rpc_barrier();
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert_eq!(delivery(&app, input).await, "queued");
    let methods = r.peer.as_ref().unwrap().received_rpc_methods().await;
    assert_eq!(
        methods
            .iter()
            .filter(|m| *m == "modules.input.steer")
            .count(),
        1
    );
    drop(r.peer.take());
    app.cleanup().await;
}

#[tokio::test]
async fn native_steering_flush_splits_persisted_transcript_once_in_the_same_attempt() {
    let app = TestApp::spawn().await;
    let r = running(&app, "claude_code").await;
    apply_agent_event(
        &app,
        r.org.id,
        (r.task, r.vm),
        2,
        "message.final",
        json!({"block_id":"before","content":"Before steering"}),
        false,
    )
    .await;
    let content = "  new instruction\nwith whitespace  ";
    let input = enqueue(&app, &r, content, json!([])).await;
    let c = reserve(&app, &r, input)
        .await
        .expect("eligible Claude input");
    assert_eq!(delivery(&app, input).await, "queued");
    // Neither edit nor withdraw may race a frozen delivery generation.
    let response = app
        .authenticated_client(&r.owner)
        .post_json(
            &format!("/api/v1/sessions/{}/inputs/{input}/withdraw", r.session),
            &json!({"expectedRevision":1}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let mut wrong = evidence(&c);
    wrong["reservation_id"] = json!(Uuid::new_v4());
    apply_agent_event(
        &app,
        r.org.id,
        (r.task, r.vm),
        3,
        "input.shipped",
        wrong,
        false,
    )
    .await;
    assert_eq!(delivery(&app, input).await, "queued");
    apply_agent_event(
        &app,
        r.org.id,
        (r.task, r.vm),
        4,
        "input.shipped",
        evidence(&c),
        false,
    )
    .await;
    apply_agent_event(
        &app,
        r.org.id,
        (r.task, r.vm),
        5,
        "input.shipped",
        evidence(&c),
        false,
    )
    .await;
    apply_agent_event(
        &app,
        r.org.id,
        (r.task, r.vm),
        6,
        "message.final",
        json!({"block_id":"after","content":"After steering"}),
        false,
    )
    .await;
    apply_agent_event(
        &app,
        r.org.id,
        (r.task, r.vm),
        7,
        "invocation.outcome",
        json!({"status":"succeeded","result":{"status":"succeeded","code":"exit_code"}}),
        true,
    )
    .await;
    let rows: Vec<(String,String,String,i32)> = sqlx::query_as("SELECT role,content,status,attempt FROM chat_messages WHERE session_id=$1 ORDER BY message_index").bind(r.session).fetch_all(&app.db).await.unwrap();
    assert_eq!(
        rows,
        vec![
            ("user".into(), "initial input".into(), "complete".into(), 1),
            (
                "assistant".into(),
                "Before steering".into(),
                "complete".into(),
                1
            ),
            ("user".into(), content.into(), "complete".into(), 1),
            (
                "assistant".into(),
                "After steering".into(),
                "complete".into(),
                1
            )
        ]
    );
    assert_eq!(delivery(&app, input).await, "delivered");
    let tasks: i64 = sqlx::query_scalar("SELECT count(*) FROM tasks WHERE chat_session_id=$1")
        .bind(r.session)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(tasks, 1);
    // Rebuilt continuation history must contain the flushed input.
    enqueue(&app, &r, "what did I ask?", json!([])).await;
    let mut db = app.scoped_db_for_org(r.org.id).await;
    let next = app
        .state
        .task_service
        .reserve_agent_dispatch(&mut db)
        .await
        .unwrap()
        .unwrap();
    let history = next.invoke_params["input"]["prompt"]["text"]
        .as_str()
        .unwrap();
    assert!(
        history
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .any(|message| message["role"] == "user" && message["content"] == content),
        "{history}"
    );
    db.commit().await.unwrap();
    app.cleanup().await;
}

#[tokio::test]
async fn native_steering_queues_new_input_after_cancel_or_pause_intent() {
    for pause in [false, true] {
        let app = TestApp::spawn().await;
        let r = running(&app, "claude_code").await;
        let mut db = app.scoped_db_for_org(r.org.id).await;
        assert!(app
            .state
            .task_service
            .native_steering_available(&mut db, r.task)
            .await
            .unwrap());
        if pause {
            app.state
                .task_service
                .request_workspace_task_pause(&mut db, r.owner.id, r.task)
                .await
                .unwrap();
        } else {
            app.state
                .task_service
                .request_workspace_task_cancel(&mut db, r.owner.id, r.task)
                .await
                .unwrap();
        }
        assert!(!app
            .state
            .task_service
            .native_steering_available(&mut db, r.task)
            .await
            .unwrap());
        db.commit().await.unwrap();
        let response = app
            .authenticated_client(&r.owner)
            .post_json(
                &format!("/api/v1/sessions/{}/messages", r.session),
                &json!({"content":"the next request","agentProvider":"claude_code"}),
            )
            .await;
        assert_eq!(response.status(), StatusCode::CREATED);
        let body: Value = response.json().await.unwrap();
        let input = Uuid::parse_str(body["data"]["inputId"].as_str().unwrap()).unwrap();
        assert_eq!(body["data"]["delivery"], "queued");
        assert_eq!(delivery(&app, input).await, "queued");
        let row:(String,bool)=sqlx::query_as("SELECT task.status,input.steered FROM tasks task JOIN session_inputs input ON input.task_id=task.id WHERE input.id=$1")
            .bind(input).fetch_one(&app.db).await.unwrap();
        assert_eq!(
            row,
            ("running".into(), false),
            "pending cancellation must not ship a new input"
        );
        app.cleanup().await;
    }
}

#[tokio::test]
async fn native_steering_stop_or_vm_loss_preserves_flushed_input_without_recovery() {
    for loss in [false, true] {
        let app = TestApp::spawn().await;
        let r = running(&app, "claude_code").await;
        apply_agent_event(
            &app,
            r.org.id,
            (r.task, r.vm),
            2,
            "message.delta",
            json!({"block_id":"before","text_delta":"Partial "}),
            false,
        )
        .await;
        let input = enqueue(&app, &r, "flushed correction", json!([])).await;
        let c = reserve(&app, &r, input).await.unwrap();
        apply_agent_event(
            &app,
            r.org.id,
            (r.task, r.vm),
            3,
            "input.shipped",
            evidence(&c),
            false,
        )
        .await;
        apply_agent_event(
            &app,
            r.org.id,
            (r.task, r.vm),
            4,
            "message.delta",
            json!({"block_id":"before","text_delta":"text after flush"}),
            false,
        )
        .await;
        if loss {
            let mut db = app.scoped_db_for_org(r.org.id).await;
            app.state
                .task_service
                .apply(
                    &mut db,
                    TaskLifecycleEvent::VmLost {
                        task_id: r.task,
                        attempt: 1,
                        vm_id: r.vm,
                        epoch: 1,
                        evidence: json!({"reason":"fixture_loss"}),
                    },
                )
                .await
                .unwrap();
            db.commit().await.unwrap();
        } else {
            let mut db = app.scoped_db_for_org(r.org.id).await;
            app.state
                .task_service
                .end_chat_turn(
                    &mut db,
                    r.owner.id,
                    r.session,
                    canonical_agent_message_id(r.task, 1),
                )
                .await
                .unwrap();
            db.commit().await.unwrap();
            apply_agent_event(
                &app,
                r.org.id,
                (r.task, r.vm),
                5,
                "invocation.outcome",
                json!({"status":"cancelled","result":{"status":"cancelled","code":"cancelled"}}),
                true,
            )
            .await;
        }
        assert_eq!(delivery(&app, input).await, "delivered");
        let row: (Option<String>, bool) =
            sqlx::query_as("SELECT hold_reason,ended_at IS NOT NULL FROM tasks WHERE id=$1")
                .bind(r.task)
                .fetch_one(&app.db)
                .await
                .unwrap();
        assert_ne!(row.0.as_deref(), Some("delivery_unknown"));
        let states:Vec<String>=sqlx::query_scalar("SELECT status FROM chat_messages WHERE task_id=$1 AND role='assistant' ORDER BY message_index")
            .bind(r.task).fetch_all(&app.db).await.unwrap();
        assert_eq!(states[0], "complete");
        let partial: String = sqlx::query_scalar("SELECT content FROM chat_messages WHERE id=$1")
            .bind(canonical_agent_message_id(r.task, 1))
            .fetch_one(&app.db)
            .await
            .unwrap();
        assert_eq!(partial, "Partial text after flush");
        assert_ne!(states[1], "pending");
        let content: String =
            sqlx::query_scalar("SELECT content FROM chat_messages WHERE input_id=$1")
                .bind(input)
                .fetch_one(&app.db)
                .await
                .unwrap();
        assert_eq!(content, "flushed correction");
        app.cleanup().await;
    }
}

#[tokio::test]
async fn native_steering_rejection_ambiguity_and_lost_request_use_normal_fifo_dispatch() {
    for disposition in [
        Some(SteerDisposition::NotSteerable),
        Some(SteerDisposition::Unknown),
        None,
    ] {
        let app = TestApp::spawn().await;
        let r = running(&app, "claude_code").await;
        let first = enqueue(&app, &r, "first correction", json!([])).await;
        let c = reserve(&app, &r, first).await.unwrap();
        let later = enqueue(&app, &r, "later correction", json!([])).await;
        assert!(reserve(&app, &r, later).await.is_none());
        if let Some(disposition) = disposition {
            let mut db = app.scoped_db_for_org(r.org.id).await;
            app.state
                .task_service
                .finish_agent_steer(&mut db, &c, disposition)
                .await
                .unwrap();
            db.commit().await.unwrap();
            // A delayed write from the abandoned generation cannot claim it.
            apply_agent_event(
                &app,
                r.org.id,
                (r.task, r.vm),
                2,
                "input.shipped",
                evidence(&c),
                false,
            )
            .await;
        }
        assert_eq!(delivery(&app, first).await, "queued");
        apply_agent_event(
            &app,
            r.org.id,
            (r.task, r.vm),
            if disposition.is_some() { 3 } else { 2 },
            "invocation.outcome",
            json!({"status":"succeeded","result":{"status":"succeeded","code":"exit_code"}}),
            true,
        )
        .await;
        let mut db = app.scoped_db_for_org(r.org.id).await;
        let next = app
            .state
            .task_service
            .reserve_agent_dispatch(&mut db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            next.invoke_params["input"]["task"]["initial_input"]["input_id"],
            first.to_string()
        );
        assert_ne!(
            next.invoke_params["input"]["task"]["initial_input"]["reservation_id"],
            c.reservation_id.to_string()
        );
        db.commit().await.unwrap();
        assert_eq!(delivery(&app, first).await, "reserved");
        assert_eq!(delivery(&app, later).await, "queued");
        app.cleanup().await;
    }
}

#[tokio::test]
async fn native_steering_ignores_the_removed_organization_rollout_setting() {
    let app = TestApp::spawn().await;
    let r = running(&app, "claude_code").await;
    sqlx::query("UPDATE org_configs SET settings_json=settings_json || jsonb_build_object('claude_native_steering',false) WHERE org_id=$1")
        .bind(r.org.id).execute(&app.db).await.unwrap();
    let mut db = app.scoped_db_for_org(r.org.id).await;
    assert!(app
        .state
        .task_service
        .native_steering_available(&mut db, r.task)
        .await
        .unwrap());
    db.commit().await.unwrap();
    let input = enqueue(&app, &r, "steering needs no rollout switch", json!([])).await;
    assert!(reserve(&app, &r, input).await.is_some());
    assert_eq!(delivery(&app, input).await, "queued");

    // The obsolete setting must not disable newly launched attempts either.
    let (task, session) =
        create_agent_task(&app, &r.workspace, r.owner.id, "queued", None, None).await;
    queue_session_input(&app, r.org.id, session, task, "another initial input").await;
    let mut db = app.scoped_db_for_org(r.org.id).await;
    let dispatch = app
        .state
        .task_service
        .reserve_agent_dispatch(&mut db)
        .await
        .unwrap()
        .expect("reserve another Claude attempt");
    assert_eq!(dispatch.task_id, task);
    assert_eq!(
        dispatch.invoke_params["input"]["provider"]["options"]["native_mid_turn_steering"],
        true
    );
    db.commit().await.unwrap();
    app.cleanup().await;
}

#[tokio::test]
async fn native_steering_admission_keeps_older_attempts_codex_attachments_and_older_queue_on_queue()
{
    for (steering_support, provider, attachments) in [
        (false, "claude_code", false),
        (true, "codex", false),
        (true, "claude_code", true),
    ] {
        let app = TestApp::spawn().await;
        let r = running(&app, provider).await;
        if !steering_support {
            // Seed the historical snapshot of a process launched before this
            // release. Production correctly prevents changing this identity.
            let mut fixture = app.db.begin().await.unwrap();
            sqlx::query(
                "ALTER TABLE task_attempts DISABLE TRIGGER task_attempts_immutable_identity",
            )
            .execute(&mut *fixture)
            .await
            .unwrap();
            sqlx::query("UPDATE task_attempts SET policy_snapshot_ref=policy_snapshot_ref-'native_mid_turn_steering' WHERE task_id=$1")
                .bind(r.task).execute(&mut *fixture).await.unwrap();
            sqlx::query(
                "ALTER TABLE task_attempts ENABLE TRIGGER task_attempts_immutable_identity",
            )
            .execute(&mut *fixture)
            .await
            .unwrap();
            fixture.commit().await.unwrap();
            let mut db = app.scoped_db_for_org(r.org.id).await;
            assert!(!app
                .state
                .task_service
                .native_steering_available(&mut db, r.task)
                .await
                .unwrap());
            db.commit().await.unwrap();
        }
        let input = enqueue(
            &app,
            &r,
            "queue fallback",
            if attachments {
                json!([{"kind":"file"}])
            } else {
                json!([])
            },
        )
        .await;
        assert!(reserve(&app, &r, input).await.is_none());
        assert_eq!(delivery(&app, input).await, "queued");
        let later = enqueue(&app, &r, "must not pass older input", json!([])).await;
        assert!(reserve(&app, &r, later).await.is_none());
        app.cleanup().await;
    }
}

#[tokio::test]
async fn native_steering_admission_respects_native_requests_and_background_waits() {
    for background in [false, true] {
        let app = TestApp::spawn().await;
        let r = running(&app, "claude_code").await;
        let (kind, payload) = if background {
            (
                "background_tasks.replaced",
                json!({"tasks":[{"task_id":"bg","kind":"shell","description":"waiting"}],"count":1,"truncated":false,"tasks_ref":"sha256:fixture"}),
            )
        } else {
            (
                "provider_request.opened",
                json!({"provider_request_id":"permission","request_kind":"permission","tool_name":"Bash","payload":{"command":"ls"},"display":{"command":"ls"},"deadline_unix_ms":chrono::Utc::now().timestamp_millis()+60_000}),
            )
        };
        apply_agent_event(&app, r.org.id, (r.task, r.vm), 2, kind, payload, false).await;
        let input = enqueue(&app, &r, "must queue", json!([])).await;
        assert!(reserve(&app, &r, input).await.is_none());
        assert_eq!(delivery(&app, input).await, "queued");
        app.cleanup().await;
    }
}

#[tokio::test]
async fn native_steering_flush_keeps_in_flight_text_and_tools_in_their_original_segment() {
    let app = TestApp::spawn().await;
    let r = running(&app, "claude_code").await;
    let prior = canonical_agent_message_id(r.task, 1);
    apply_agent_event(
        &app,
        r.org.id,
        (r.task, r.vm),
        2,
        "message.delta",
        json!({"block_id":"text","text_delta":"Before"}),
        false,
    )
    .await;
    apply_agent_event(
        &app,
        r.org.id,
        (r.task, r.vm),
        3,
        "tool.started",
        json!({"call_id":"tool","name":"Bash","input":{"command":"sleep 1"}}),
        false,
    )
    .await;
    let input = enqueue(&app, &r, "correction", json!([])).await;
    let c = reserve(&app, &r, input).await.unwrap();
    apply_agent_event(
        &app,
        r.org.id,
        (r.task, r.vm),
        4,
        "input.shipped",
        evidence(&c),
        false,
    )
    .await;
    apply_agent_event(
        &app,
        r.org.id,
        (r.task, r.vm),
        5,
        "message.delta",
        json!({"block_id":"text","text_delta":" and after"}),
        false,
    )
    .await;
    apply_agent_event(
        &app,
        r.org.id,
        (r.task, r.vm),
        6,
        "message.final",
        json!({"block_id":"text","content":"Before and after"}),
        false,
    )
    .await;
    apply_agent_event(
        &app,
        r.org.id,
        (r.task, r.vm),
        7,
        "tool.completed",
        json!({"call_id":"tool","name":"Bash","status":"success","output":"done"}),
        false,
    )
    .await;
    apply_agent_event(
        &app,
        r.org.id,
        (r.task, r.vm),
        8,
        "message.final",
        json!({"block_id":"next","content":"Addressed correction"}),
        false,
    )
    .await;
    let rows:Vec<(i64,String)>=sqlx::query_as("SELECT seq,metadata->>'messageId' FROM task_events WHERE task_id=$1 AND seq>=5 ORDER BY seq")
        .bind(r.task).fetch_all(&app.db).await.unwrap();
    assert!(
        rows[..3].iter().all(|(_, id)| *id == prior.to_string()),
        "{rows:?}"
    );
    assert_ne!(rows[3].1, prior.to_string());
    let text:Vec<String>=sqlx::query_scalar("SELECT content FROM chat_messages WHERE task_id=$1 AND role='assistant' ORDER BY message_index")
        .bind(r.task).fetch_all(&app.db).await.unwrap();
    assert_eq!(text, vec!["Before and after", "Addressed correction"]);
    app.cleanup().await;
}
