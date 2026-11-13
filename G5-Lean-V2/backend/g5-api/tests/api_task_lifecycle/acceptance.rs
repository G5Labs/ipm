//! Issue #629: acceptance and delivery facts survive later lifecycle changes.
use super::*;

#[path = "confirmation.rs"]
mod confirmation;

async fn wait_for_steer(r: &Running) {
    while !r
        .peer
        .as_ref()
        .unwrap()
        .received_rpc_methods()
        .await
        .iter()
        .any(|m| m == "modules.input.steer")
    {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
}

async fn lose_vm(app: &TestApp, r: &Running) {
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
}

#[tokio::test]
async fn send_returns_its_committed_identity_after_ownership_changes_during_steering() {
    let app = TestApp::spawn().await;
    let r = running_with_peer(&app, "claude_code", true).await;
    let new_owner = TestUser::create_with_scopes(&app, r.org.id, &[Scope::WorkspacesRead]).await;
    let client = app.authenticated_client(&r.owner);
    let path = format!("/api/v1/sessions/{}/messages", r.session);
    let (response, input) = tokio::join!(
        async {
            client
                .post_json(&path, &json!({"content":"accepted before transfer"}))
                .await
        },
        async {
            wait_for_steer(&r).await;
            let input: Uuid =
                sqlx::query_scalar("SELECT id FROM session_inputs WHERE session_id=$1 AND steered")
                    .bind(r.session)
                    .fetch_one(&app.db)
                    .await
                    .unwrap();
            sqlx::query("UPDATE workspaces SET owner_user_id=$2 WHERE id=$1")
                .bind(r.workspace.id)
                .bind(new_owner.id)
                .execute(&app.db)
                .await
                .unwrap();
            r.peer.as_ref().unwrap().release_rpc_barrier();
            input
        }
    );
    assert_eq!(response.status(), StatusCode::CREATED);
    let response: Value = response.json().await.unwrap();
    assert_eq!(response["data"]["inputId"], input.to_string());
    assert_eq!(
        response["data"]["delivery"], "queued",
        "only the acceptance snapshot is returned"
    );
    // The call holds no lock the ownership change waits for
    // (`g5.runtime.guest-call-admission`): the guest's acceptance, admitted
    // under the owner of the time, is recorded as the positive receipt it is,
    // by the steer's completion after the request answered. A real transfer
    // drains the owner's calls in flight before it moves ownership; this test
    // moves it by fiat while the call is held.
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let flushed: bool = sqlx::query_scalar(
                "SELECT steering_flushed_at IS NOT NULL FROM session_inputs WHERE id=$1",
            )
            .bind(input)
            .fetch_one(&app.db)
            .await
            .unwrap();
            if flushed {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("the receipt is recorded once the held call answers");
    lose_vm(&app, &r).await;
    assert_eq!(delivery(&app, input).await, "delivered");
    app.cleanup().await;
}

#[tokio::test]
async fn positive_receipt_is_recorded_under_tenant_authority_after_transfer() {
    let app = TestApp::spawn().await;
    let r = running(&app, "claude_code").await;
    let input = enqueue(&app, &r, "accepted before transfer", json!([])).await;
    let claim = reserve(&app, &r, input).await.unwrap();
    let new_owner = TestUser::create_with_scopes(&app, r.org.id, &[Scope::WorkspacesRead]).await;
    sqlx::query("UPDATE workspaces SET owner_user_id=$2 WHERE id=$1")
        .bind(r.workspace.id)
        .bind(new_owner.id)
        .execute(&app.db)
        .await
        .unwrap();
    let deps = g5_api::db::maintenance::TenantMaintenanceDeps {
        system_pool: app.state.system_pool.clone(),
        tenant_pool: app.state.tenant_pool.clone(),
        authz: app.state.authz.clone(),
        repo_storage: app.state.repo_storage.clone(),
    };
    let mut db = g5_api::db::maintenance::scoped_db_for_maintenance_org(&deps, r.org.id)
        .await
        .unwrap();
    app.state
        .task_service
        .finish_agent_steer(&mut db, &claim, SteerDisposition::Shipped)
        .await
        .unwrap();
    db.commit().await.unwrap();
    lose_vm(&app, &r).await;
    assert_eq!(delivery(&app, input).await, "delivered");
    app.cleanup().await;
}

#[tokio::test]
async fn steering_receipt_completes_while_send_holds_workspace_authority() {
    for ended in [false, true] {
        let app = TestApp::spawn().await;
        let r = running(&app, "claude_code").await;
        let input = enqueue(&app, &r, "receipt beside another Send", json!([])).await;
        let claim = reserve(&app, &r, input).await.unwrap();
        if ended {
            lose_vm(&app, &r).await;
        }
        // Send's scoped authorization holds this lock before waiting for
        // session turn allocation. Receipt completion already owns that
        // session lock, so it must not wait to upgrade the workspace lock.
        let mut send = app.db.begin().await.unwrap();
        sqlx::query("SELECT id FROM workspaces WHERE id=$1 FOR SHARE")
            .bind(r.workspace.id)
            .execute(&mut *send)
            .await
            .unwrap();
        let mut db = app.scoped_db_for_org(r.org.id).await;
        // Bound only database lock waits, not wall time or task scheduling.
        sqlx::query("SET LOCAL lock_timeout = '1s'")
            .execute(db.conn())
            .await
            .unwrap();
        app.state
            .task_service
            .finish_agent_steer(&mut db, &claim, SteerDisposition::Shipped)
            .await
            .expect("receipt persistence must not wait for Send's workspace lock");
        db.commit().await.unwrap();
        let known: bool = sqlx::query_scalar(
            "SELECT steering_flushed_at IS NOT NULL FROM session_inputs WHERE id=$1",
        )
        .bind(input)
        .fetch_one(&app.db)
        .await
        .unwrap();
        assert!(known);
        assert_eq!(
            delivery(&app, input).await,
            if ended { "delivered" } else { "queued" }
        );
        send.rollback().await.unwrap();
        if !ended {
            lose_vm(&app, &r).await;
            assert_eq!(delivery(&app, input).await, "delivered");
        }
        app.cleanup().await;
    }
}

#[tokio::test]
async fn late_flush_receipt_preserves_history_but_never_claims_a_newer_revision() {
    for action in ["unchanged", "edited", "withdrawn"] {
        let app = TestApp::spawn().await;
        let r = running(&app, "claude_code").await;
        let input = enqueue(&app, &r, "late flush", json!([])).await;
        let c = reserve(&app, &r, input).await.unwrap();
        lose_vm(&app, &r).await;
        let mut db = app.scoped_db_for_org(r.org.id).await;
        if action == "edited" {
            app.state
                .task_service
                .edit_queued_session_input(
                    &mut db,
                    r.owner.id,
                    r.session,
                    input,
                    1,
                    "new revision".into(),
                )
                .await
                .unwrap();
        } else if action == "withdrawn" {
            app.state
                .task_service
                .withdraw_queued_session_input(&mut db, r.owner.id, r.session, input, 1)
                .await
                .unwrap();
        }
        app.state
            .task_service
            .finish_agent_steer(&mut db, &c, SteerDisposition::Shipped)
            .await
            .unwrap();
        app.state
            .task_service
            .finish_agent_steer(&mut db, &c, SteerDisposition::Unknown)
            .await
            .unwrap();
        db.commit().await.unwrap();
        assert_eq!(
            delivery(&app, input).await,
            if action == "edited" {
                "queued"
            } else {
                "delivered"
            }
        );
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM chat_messages WHERE input_id=$1")
            .bind(input)
            .fetch_one(&app.db)
            .await
            .unwrap();
        assert_eq!(count, if action == "edited" { 0 } else { 1 });
        app.cleanup().await;
    }
}

#[tokio::test]
async fn queued_followups_dispatch_their_own_accounts_models_and_effort() {
    let app = TestApp::spawn().await;
    let r = running(&app, "claude_code").await;
    let client = app.authenticated_client(&r.owner);
    // No live peer: both sends take the ordinary queue path after the bounded RPC.
    let mut choices = Vec::new();
    for (index, model, effort) in [
        (0, "claude-sonnet-4-6", "medium"),
        (1, "claude-opus-4-8[1m]", "high"),
    ] {
        let response = client
            .post_json(
                &format!("/api/v1/orgs/{}/provider-accounts", r.org.id),
                &json!({"name":format!("choice-{index}"),"kind":"anthropic_api_key","scope":"user",
                "secret":format!("sk-ant-test-choice-{index}-0123456789abcdef")}),
            )
            .await;
        assert_eq!(response.status(), StatusCode::CREATED);
        let account: Value = response.json().await.unwrap();
        let account = account["data"]["id"].as_str().unwrap().to_string();
        let response = client
            .post_json(
                &format!("/api/v1/sessions/{}/messages", r.session),
                &json!({"content":format!("queued-{index}"),"model":model,"effort":effort,
                "account":{"kind":"account","accountId":account}}),
            )
            .await;
        assert_eq!(response.status(), StatusCode::CREATED);
        let input: Value = response.json().await.unwrap();
        choices.push((
            input["data"]["inputId"].as_str().unwrap().to_string(),
            account,
            model,
            effort,
        ));
    }
    apply_agent_event(
        &app,
        r.org.id,
        (r.task, r.vm),
        2,
        "invocation.outcome",
        json!({"status":"succeeded","result":{"status":"succeeded","code":"exit_code"}}),
        true,
    )
    .await;
    for (input, account, model, effort) in choices {
        let mut db = app.scoped_db_for_org(r.org.id).await;
        let dispatch = app
            .state
            .task_service
            .reserve_agent_dispatch(&mut db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            dispatch.invoke_params["input"]["task"]["initial_input"]["input_id"],
            input
        );
        let launch:Value=sqlx::query_scalar("SELECT policy_snapshot_ref->'agent_launch'->'resolved' FROM task_attempts WHERE task_id=$1 AND attempt=$2")
            .bind(dispatch.task_id).bind(dispatch.attempt).fetch_one(db.conn()).await.unwrap();
        assert_eq!(launch["accountId"], account);
        assert_eq!(launch["model"]["value"], model);
        assert_eq!(launch["effort"]["value"], effort);
        db.commit().await.unwrap();
        sqlx::query("UPDATE tasks SET updated_at=now()-interval '3 seconds' WHERE id=$1")
            .bind(dispatch.task_id)
            .execute(&app.db)
            .await
            .unwrap();
        let mut db = app.scoped_db_for_org(r.org.id).await;
        let retry = app
            .state
            .task_service
            .reserve_agent_dispatch(&mut db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(retry.invoke_params, dispatch.invoke_params);
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
            &app,
            r.org.id,
            (dispatch.task_id, r.vm),
            1,
            "input.accepted",
            dispatch.invoke_params["input"]["task"]["initial_input"].clone(),
            false,
        )
        .await;
        apply_agent_event(
            &app,
            r.org.id,
            (dispatch.task_id, r.vm),
            2,
            "invocation.outcome",
            json!({"status":"succeeded","result":{"status":"succeeded","code":"exit_code"}}),
            true,
        )
        .await;
    }
    app.cleanup().await;
}

#[tokio::test]
async fn confirmed_queued_launch_refuses_changed_defaults_before_first_dispatch() {
    for changed in [false, true] {
        let app = TestApp::spawn().await;
        let mut owner = TestUser::create(&app).await;
        let org = TestOrg::create_with_admin(&app, &mut owner).await;
        let workspace = TestWorkspace::create(&app, &org).await;
        let client = app.authenticated_client(&owner);
        let config = format!("/api/v1/orgs/{}/config", org.id);
        let result = client
            .put_json(
                &config,
                &json!({"settings":{"ANTHROPIC_MODEL":"claude-sonnet-4-6"}}),
            )
            .await;
        assert_eq!(result.status(), StatusCode::OK);
        let preferences = json!({"schemaVersion":1,"provider":{"source":"explicit","value":"claude_code"},
            "permissionMode":{"source":"explicit","value":"bypassPermissions"},
            "effort":{"source":"explicit","value":"high"}});
        let preview = client
            .post_json(
                &format!(
                    "/api/v1/workspaces/{}/config/agent-launch/resolve",
                    workspace.id
                ),
                &json!({"preferences":preferences}),
            )
            .await;
        assert_eq!(preview.status(), StatusCode::OK);
        let preview: Value = preview.json().await.unwrap();
        let start = client
            .post_json(
                &format!("/api/v1/workspaces/{}/sessions/start", workspace.id),
                &json!({"content":"confirmed queued input","agentLaunch":{"preferences":preferences,
                "confirmedResolutionId":preview["data"]["resolved"]["resolutionId"]}}),
            )
            .await;
        assert_eq!(start.status(), StatusCode::CREATED);
        let start: Value = start.json().await.unwrap();
        let session: Uuid = start["data"]["session"]["id"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap();
        if changed {
            let result = client
                .put_json(
                    &config,
                    &json!({"settings":{"ANTHROPIC_MODEL":"claude-opus-4-8[1m]"}}),
                )
                .await;
            assert_eq!(result.status(), StatusCode::OK);
        }
        attach_running_vm_without_runtime(&app, &workspace).await;
        let mut db = app.scoped_db_for_org(org.id).await;
        let dispatch = app
            .state
            .task_service
            .reserve_agent_dispatch(&mut db)
            .await
            .unwrap();
        assert_eq!(dispatch.is_some(), !changed);
        db.commit().await.unwrap();
        if changed {
            let row:(String,i64)=sqlx::query_as("SELECT status,(SELECT count(*) FROM task_attempts WHERE task_id=tasks.id) FROM tasks WHERE chat_session_id=$1")
                .bind(session).fetch_one(&app.db).await.unwrap();
            assert_eq!(row, ("failed".into(), 0));
        } else {
            let dispatch = dispatch.unwrap();
            let input: Uuid = dispatch.invoke_params["input"]["task"]["initial_input"]["input_id"]
                .as_str()
                .unwrap()
                .parse()
                .unwrap();
            let mut db = app.scoped_db_for_org(org.id).await;
            app.state
                .task_service
                .apply(
                    &mut db,
                    TaskLifecycleEvent::VmLost {
                        task_id: dispatch.task_id,
                        attempt: dispatch.attempt,
                        vm_id: dispatch.vm_id,
                        epoch: dispatch.epoch,
                        evidence: json!({"reason":"lost_after_first_admission"}),
                    },
                )
                .await
                .unwrap();
            db.commit().await.unwrap();
            let result = client
                .put_json(
                    &config,
                    &json!({"settings":{"ANTHROPIC_MODEL":"claude-opus-4-8[1m]"}}),
                )
                .await;
            assert_eq!(result.status(), StatusCode::OK);
            let mut db = app.scoped_db_for_org(org.id).await;
            app.state
                .task_service
                .authorize_unknown_input_redelivery(&mut db, owner.id, session, input)
                .await
                .unwrap();
            let retry = app
                .state
                .task_service
                .reserve_agent_dispatch(&mut db)
                .await
                .unwrap()
                .unwrap();
            let launch:Value=sqlx::query_scalar("SELECT policy_snapshot_ref->'agent_launch'->'resolved' FROM task_attempts WHERE task_id=$1 AND attempt=$2")
                .bind(retry.task_id).bind(retry.attempt).fetch_one(db.conn()).await.unwrap();
            assert_eq!(launch["model"]["value"], "claude-sonnet-4-6");
            db.commit().await.unwrap();
        }
        app.cleanup().await;
    }
}

#[tokio::test]
async fn input_targeted_stop_refuses_a_native_steering_reservation() {
    let app = TestApp::spawn().await;
    let r = running(&app, "claude_code").await;
    let input = enqueue(&app, &r, "reserved correction", json!([])).await;
    let c = reserve(&app, &r, input).await.unwrap();
    let response = app
        .authenticated_client(&r.owner)
        .post_json(
            &format!("/api/v1/sessions/{}/messages/{input}/stop", r.session),
            &json!({}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
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
    assert_eq!(delivery(&app, input).await, "delivered");
    app.cleanup().await;
}

#[tokio::test]
async fn known_rpc_flush_survives_vm_loss_before_journal_projection() {
    let app = TestApp::spawn().await;
    let r = running(&app, "claude_code").await;
    let input = enqueue(&app, &r, "known flushed correction", json!([])).await;
    let c = reserve(&app, &r, input).await.unwrap();
    let mut db = app.scoped_db_for_org(r.org.id).await;
    app.state
        .task_service
        .finish_agent_steer(&mut db, &c, SteerDisposition::Shipped)
        .await
        .unwrap();
    app.state
        .task_service
        .finish_agent_steer(&mut db, &c, SteerDisposition::Unknown)
        .await
        .unwrap();
    db.commit().await.unwrap();
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
                evidence: json!({"reason":"fixture_loss_before_replay"}),
            },
        )
        .await
        .unwrap();
    db.commit().await.unwrap();
    assert_eq!(delivery(&app, input).await, "delivered");
    let messages: i64 =
        sqlx::query_scalar("SELECT count(*) FROM chat_messages WHERE input_id=$1 AND role='user'")
            .bind(input)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(messages, 1);
    let binding: (Uuid, Option<i32>) =
        sqlx::query_as("SELECT task_id,attempt FROM session_inputs WHERE id=$1")
            .bind(input)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(binding, (r.task, Some(1)));
    app.cleanup().await;
}

#[tokio::test]
async fn send_returns_acceptance_while_steering_persistence_waits_for_a_lock() {
    let app = TestApp::spawn().await;
    let r = running_with_peer(&app, "claude_code", true).await;
    let client = app.authenticated_client(&r.owner);
    let path = format!("/api/v1/sessions/{}/messages", r.session);
    let request = tokio::spawn(async move {
        client
            .post_json(&path, &json!({"content":"accepted before persistence"}))
            .await
    });
    wait_for_steer(&r).await;
    let input: Uuid =
        sqlx::query_scalar("SELECT id FROM session_inputs WHERE session_id=$1 AND steered")
            .bind(r.session)
            .fetch_one(&app.db)
            .await
            .unwrap();
    // Acceptance has committed. Hold the lock that receipt persistence needs
    // before releasing the successful RPC; the response must not wait for it.
    let mut blocker = app.db.begin().await.unwrap();
    let (class, key) = chat_session_turn_lock_key(r.session);
    sqlx::query("SELECT pg_advisory_xact_lock($1, $2)")
        .bind(class)
        .bind(key)
        .execute(&mut *blocker)
        .await
        .unwrap();
    r.peer.as_ref().unwrap().release_rpc_barrier();
    helpers::wait_for_lock_wait(&app).await;
    let response = tokio::time::timeout(std::time::Duration::from_secs(5), request)
        .await
        .expect("committed acceptance must not wait for receipt persistence")
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["data"]["inputId"], input.to_string());
    assert_eq!(body["data"]["delivery"], "queued");
    blocker.rollback().await.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let known: bool = sqlx::query_scalar(
                "SELECT steering_flushed_at IS NOT NULL FROM session_inputs WHERE id=$1",
            )
            .bind(input)
            .fetch_one(&app.db)
            .await
            .unwrap();
            if known {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("background receipt persistence resumes after the lock releases");
    lose_vm(&app, &r).await;
    assert_eq!(delivery(&app, input).await, "delivered");
    app.cleanup().await;
}

#[tokio::test]
async fn a_cancelled_http_request_does_not_abandon_its_committed_steer() {
    let app = TestApp::spawn().await;
    let r = running_with_peer(&app, "claude_code", true).await;
    let client = app.authenticated_client(&r.owner);
    let path = format!("/api/v1/sessions/{}/messages", r.session);
    let request = tokio::spawn(async move {
        client
            .post_json(&path, &json!({"content":"accepted before disconnect"}))
            .await
    });
    wait_for_steer(&r).await;
    let input: Uuid =
        sqlx::query_scalar("SELECT id FROM session_inputs WHERE session_id=$1 AND steered")
            .bind(r.session)
            .fetch_one(&app.db)
            .await
            .unwrap();
    request.abort();
    match request.await {
        Err(error) => assert!(error.is_cancelled()),
        // Send can return its acceptance before the client disconnects. Either
        // way, the RPC is still blocked and must complete without this caller.
        Ok(response) => assert_eq!(response.status(), StatusCode::CREATED),
    }
    r.peer.as_ref().unwrap().release_rpc_barrier();
    loop {
        let known: bool = sqlx::query_scalar(
            "SELECT steering_flushed_at IS NOT NULL FROM session_inputs WHERE id=$1",
        )
        .bind(input)
        .fetch_one(&app.db)
        .await
        .unwrap();
        if known {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    lose_vm(&app, &r).await;
    assert_eq!(delivery(&app, input).await, "delivered");
    app.cleanup().await;
}

#[tokio::test]
async fn a_stale_positive_receipt_cannot_claim_a_new_dispatch_generation() {
    let app = TestApp::spawn().await;
    let r = running(&app, "claude_code").await;
    let input = enqueue(&app, &r, "new generation", json!([])).await;
    let claim = reserve(&app, &r, input).await.unwrap();
    lose_vm(&app, &r).await;
    let mut db = app.scoped_db_for_org(r.org.id).await;
    let next = app
        .state
        .task_service
        .reserve_agent_dispatch(&mut db)
        .await
        .unwrap()
        .unwrap();
    db.commit().await.unwrap();
    let mut db = app.scoped_db_for_org(r.org.id).await;
    app.state
        .task_service
        .finish_agent_steer(&mut db, &claim, SteerDisposition::Shipped)
        .await
        .unwrap();
    db.commit().await.unwrap();
    let row: (Uuid,i32,String,bool) = sqlx::query_as("SELECT task_id,delivery_generation,delivery,steering_flushed_at IS NOT NULL FROM session_inputs WHERE id=$1")
        .bind(input).fetch_one(&app.db).await.unwrap();
    assert_eq!(
        row,
        (
            next.task_id,
            claim.delivery_generation + 1,
            "reserved".into(),
            false
        )
    );
    app.cleanup().await;
}
