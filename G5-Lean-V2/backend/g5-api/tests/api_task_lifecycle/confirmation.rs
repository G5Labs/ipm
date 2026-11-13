//! First-admission confirmation and explicit recovery through the owner's next Send.
use super::*;

struct ConfirmedInput {
    owner: TestUser,
    org: TestOrg,
    workspace: TestWorkspace,
    session: Uuid,
    task: Uuid,
    input: Uuid,
    resolved: Value,
    preferences: Value,
}

async fn confirmed_input(app: &TestApp) -> ConfirmedInput {
    let mut owner = TestUser::create(app).await;
    let org = TestOrg::create_with_admin(app, &mut owner).await;
    let workspace = TestWorkspace::create(app, &org).await;
    let client = app.authenticated_client(&owner);
    let configured = client
        .put_json(
            &format!("/api/v1/orgs/{}/config", org.id),
            &json!({"settings":{"ANTHROPIC_MODEL":"claude-sonnet-4-6"},
                "setSecrets":{"REVIEW_VALIDATION_SECRET":"before"}}),
        )
        .await;
    assert_eq!(configured.status(), StatusCode::OK);
    let preferences = json!({"schemaVersion":1,
        "provider":{"source":"explicit","value":"claude_code"},
        "model":{"source":"inherit_configured"},
        "account":{"source":"inherit_configured"},
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
    let resolved = preview["data"]["resolved"].clone();
    let start = client
        .post_json(
            &format!("/api/v1/workspaces/{}/sessions/start", workspace.id),
            &json!({"content":"confirmed first message","agentLaunch":{
                "preferences":preferences,"confirmedResolutionId":resolved["resolutionId"]}}),
        )
        .await;
    assert_eq!(start.status(), StatusCode::CREATED);
    let start: Value = start.json().await.unwrap();
    ConfirmedInput {
        owner,
        org,
        workspace,
        session: start["data"]["session"]["id"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap(),
        task: start["data"]["taskId"].as_str().unwrap().parse().unwrap(),
        input: start["data"]["message"]["inputId"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap(),
        resolved,
        preferences,
    }
}

#[tokio::test]
async fn refused_confirmation_recovers_on_next_send_and_keeps_edit_and_withdraw() {
    for action in ["send", "edit_then_send", "withdraw"] {
        let app = TestApp::spawn().await;
        let queued = confirmed_input(&app).await;
        let client = app.authenticated_client(&queued.owner);
        let changed = client
            .put_json(
                &format!("/api/v1/orgs/{}/config", queued.org.id),
                &json!({"settings":{"ANTHROPIC_MODEL":"claude-opus-4-8[1m]"}}),
            )
            .await;
        assert_eq!(changed.status(), StatusCode::OK);
        let vm = attach_running_vm_without_runtime(&app, &queued.workspace).await;
        let mut db = app.scoped_db_for_org(queued.org.id).await;
        assert!(app
            .state
            .task_service
            .reserve_agent_dispatch(&mut db)
            .await
            .unwrap()
            .is_none());
        db.commit().await.unwrap();
        let status: (String, i64) = sqlx::query_as(
            "SELECT status,(SELECT count(*) FROM task_attempts WHERE task_id=tasks.id)
             FROM tasks WHERE id=$1",
        )
        .bind(queued.task)
        .fetch_one(&app.db)
        .await
        .unwrap();
        assert_eq!(status, ("failed".into(), 0));
        let notice: String = sqlx::query_scalar("SELECT content FROM chat_messages WHERE id=$1")
            .bind(canonical_agent_message_id(queued.task, 0))
            .fetch_one(&app.db)
            .await
            .unwrap();
        for expected in [
            "model",
            "claude-sonnet-4-6",
            "claude-opus-4-8[1m]",
            "next message",
            "current values",
            "withdraw",
        ] {
            assert!(notice.contains(expected), "missing {expected}: {notice}");
        }
        let input_path = format!(
            "/api/v1/sessions/{}/inputs/{}",
            queued.session, queued.input
        );
        if action == "withdraw" {
            let response = client
                .post_json(
                    &format!("{input_path}/withdraw"),
                    &json!({"expectedRevision":1}),
                )
                .await;
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(delivery(&app, queued.input).await, "withdrawn");
            app.cleanup().await;
            continue;
        }
        if action == "edit_then_send" {
            let response = client
                .patch_json(
                    &input_path,
                    &json!({"expectedRevision":1,"content":"edited first message"}),
                )
                .await;
            assert_eq!(response.status(), StatusCode::OK);
        }
        let before: Value =
            sqlx::query_scalar("SELECT confirmed_launch FROM session_inputs WHERE id=$1")
                .bind(queued.input)
                .fetch_one(&app.db)
                .await
                .unwrap();
        assert_eq!(before["resolved"], queued.resolved);
        let response = client
            .post_json(
                &format!("/api/v1/sessions/{}/messages", queued.session),
                &json!({"content":"second message","model":"claude-sonnet-4-6","effort":"medium"}),
            )
            .await;
        assert_eq!(response.status(), StatusCode::CREATED);
        let response: Value = response.json().await.unwrap();
        let second: Uuid = response["data"]["inputId"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap();
        let (preferences, confirmed): (Value, Option<Value>) = sqlx::query_as(
            "SELECT launch_preferences,confirmed_launch FROM session_inputs WHERE id=$1",
        )
        .bind(queued.input)
        .fetch_one(&app.db)
        .await
        .unwrap();
        assert_eq!(preferences, queued.preferences);
        assert!(
            confirmed.is_none(),
            "next Send discards only the stale confirmation"
        );
        for (input, model, effort) in [
            (queued.input, "claude-opus-4-8[1m]", "high"),
            (second, "claude-sonnet-4-6", "medium"),
        ] {
            let mut db = app.scoped_db_for_org(queued.org.id).await;
            let dispatch = app
                .state
                .task_service
                .reserve_agent_dispatch(&mut db)
                .await
                .unwrap()
                .expect("the refused input must not refuse again or block the next message");
            assert_eq!(
                dispatch.invoke_params["input"]["task"]["initial_input"]["input_id"],
                input.to_string()
            );
            let launch: Value = sqlx::query_scalar(
                "SELECT policy_snapshot_ref->'agent_launch'->'resolved' FROM task_attempts WHERE task_id=$1 AND attempt=$2",
            ).bind(dispatch.task_id).bind(dispatch.attempt).fetch_one(db.conn()).await.unwrap();
            assert_eq!(launch["model"]["value"], model);
            assert_eq!(launch["effort"]["value"], effort);
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
                queued.org.id,
                (dispatch.task_id, vm),
                1,
                "input.accepted",
                dispatch.invoke_params["input"]["task"]["initial_input"].clone(),
                false,
            )
            .await;
            apply_agent_event(
                &app,
                queued.org.id,
                (dispatch.task_id, vm),
                2,
                "invocation.outcome",
                json!({"status":"succeeded","result":{"status":"succeeded","code":"exit_code"}}),
                true,
            )
            .await;
        }
        assert_eq!(delivery(&app, queued.input).await, "delivered");
        assert_eq!(delivery(&app, second).await, "delivered");
        app.cleanup().await;
    }
}

#[tokio::test]
async fn unrelated_config_changes_admit_confirmed_choices_and_freeze_the_admitted_retry() {
    for change in [
        json!({"settings":{"EDITOR_THEME":"dark"}}),
        json!({"setSecrets":{"REVIEW_VALIDATION_SECRET":"after"}}),
    ] {
        let app = TestApp::spawn().await;
        let queued = confirmed_input(&app).await;
        let client = app.authenticated_client(&queued.owner);
        let changed = client
            .put_json(&format!("/api/v1/orgs/{}/config", queued.org.id), &change)
            .await;
        assert_eq!(changed.status(), StatusCode::OK);
        let preview = client
            .post_json(
                &format!(
                    "/api/v1/workspaces/{}/config/agent-launch/resolve",
                    queued.workspace.id
                ),
                &json!({"preferences":queued.preferences}),
            )
            .await;
        assert_eq!(preview.status(), StatusCode::OK);
        let preview: Value = preview.json().await.unwrap();
        assert_ne!(
            preview["data"]["resolved"]["resolutionId"],
            queued.resolved["resolutionId"]
        );
        let stale_start = client
            .post_json(
                &format!("/api/v1/workspaces/{}/sessions/start", queued.workspace.id),
                &json!({"content":"a separate stale Start","agentLaunch":{
                    "preferences":queued.preferences,
                    "confirmedResolutionId":queued.resolved["resolutionId"]}}),
            )
            .await;
        assert_eq!(
            stale_start.status(),
            StatusCode::CONFLICT,
            "Start retains its full resolution-id check"
        );
        attach_running_vm_without_runtime(&app, &queued.workspace).await;
        let mut db = app.scoped_db_for_org(queued.org.id).await;
        let dispatch = app
            .state
            .task_service
            .reserve_agent_dispatch(&mut db)
            .await
            .unwrap()
            .expect("unrelated settings and secret rotations do not change confirmed choices");
        let admitted: Value = sqlx::query_scalar(
            "SELECT policy_snapshot_ref->'agent_launch'->'resolved' FROM task_attempts WHERE task_id=$1 AND attempt=$2",
        ).bind(dispatch.task_id).bind(dispatch.attempt).fetch_one(db.conn()).await.unwrap();
        for field in [
            "provider",
            "model",
            "effort",
            "permissionMode",
            "accountKind",
            "accountScope",
            "accountId",
        ] {
            assert_eq!(admitted[field], queued.resolved[field], "confirmed {field}");
        }
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
        let changed = client
            .put_json(
                &format!("/api/v1/orgs/{}/config", queued.org.id),
                &json!({"settings":{"ANTHROPIC_MODEL":"claude-opus-4-8[1m]"}}),
            )
            .await;
        assert_eq!(changed.status(), StatusCode::OK);
        let mut db = app.scoped_db_for_org(queued.org.id).await;
        app.state
            .task_service
            .authorize_unknown_input_redelivery(
                &mut db,
                queued.owner.id,
                queued.session,
                queued.input,
            )
            .await
            .unwrap();
        let retry = app
            .state
            .task_service
            .reserve_agent_dispatch(&mut db)
            .await
            .unwrap()
            .unwrap();
        let retained: Value = sqlx::query_scalar(
            "SELECT policy_snapshot_ref->'agent_launch'->'resolved' FROM task_attempts WHERE task_id=$1 AND attempt=$2",
        ).bind(retry.task_id).bind(retry.attempt).fetch_one(db.conn()).await.unwrap();
        assert_eq!(
            retained, admitted,
            "retry preserves the exact admitted resolution"
        );
        db.commit().await.unwrap();
        app.cleanup().await;
    }
}
