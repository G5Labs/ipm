//! Production-boundary regressions for owner task lifecycle actions.

use reqwest::StatusCode;
use serde_json::{json, Value};
use uuid::Uuid;

use g5_api::services::agent_dispatch::EnqueueAgentInput;
use g5_api::services::task_lifecycle::{
    canonical_agent_message_id, canonical_exec_id, JournalTaskEvent, TaskLifecycleEvent,
};
use g5_auth::Scope;
use g5_contracts::types::AgentLaunchPreferencesV1;

use crate::helpers;
#[path = "api_task_lifecycle/steering.rs"]
mod steering;
use helpers::{
    chat_session_turn_lock_key, hold_while_the_lease_ends, wait_for_lock_wait_past_the_lease,
    TestApp, TestOrg, TestUser, TestWorkspace,
};

/// The start of the statement that authorizes a request and locks its
/// workspace row (`authz.lock_user_scope_authority`).
const AUTHORIZATION_LOCK: &str = "SELECT role, granted_scopes FROM authz.lock_user_scope_authority";

#[tokio::test]
async fn terminal_provider_journal_makes_its_reported_bundle_pending_for_the_same_vm() {
    use g5_api::services::chat_session_file_service::{
        ChatSessionFileService, SessionFileIncarnation,
    };
    use g5_internal_api::dtos::SessionFileBundleBegin;

    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let vm_id = attach_running_vm_without_runtime(&app, &workspace).await;
    let (task_id, session_id) =
        create_agent_task(&app, &workspace, owner.id, "queued", None, None).await;
    let input_id = queue_session_input(&app, org.id, session_id, task_id, "Continue").await;
    let reservation_id = bind_running_attempt(&app, org.id, task_id, vm_id, input_id).await;
    let incarnation = SessionFileIncarnation { vm_id, epoch: 1 };
    let generation = Uuid::new_v4();
    let store = ChatSessionFileService::new();
    let mut db = app.scoped_db_for_org(org.id).await;
    store
        .begin_bundle(
            db.conn(),
            session_id,
            &SessionFileBundleBegin {
                generation,
                task_id,
                attempt: 1,
                reservation_id,
                cwd_relative_path: "code".into(),
            },
            incarnation,
        )
        .await
        .expect("begin the running attempt's bundle");
    db.commit().await.unwrap();

    let provider_session_id = Uuid::new_v4().to_string();
    apply_agent_event(
        &app,
        org.id,
        (task_id, vm_id),
        1,
        "input.accepted",
        json!({
            "input_id": input_id, "reservation_id": reservation_id,
            "revision": 1, "delivery_generation": 1, "system_notice_ids": []
        }),
        false,
    )
    .await;
    apply_agent_event(
        &app,
        org.id,
        (task_id, vm_id),
        2,
        "provider.session",
        json!({
            "provider_id": "codex", "provider_session_id": provider_session_id,
            "phase": "started", "model_id": "fixture-model"
        }),
        false,
    )
    .await;
    let mut db = app.scoped_db_for_org(org.id).await;
    assert!(store
        .pending_bundles(db.conn(), workspace.id, incarnation, Some(session_id))
        .await
        .expect("running attempt is not completable")
        .is_empty());
    db.commit().await.unwrap();

    apply_agent_event(
        &app,
        org.id,
        (task_id, vm_id),
        3,
        "invocation.outcome",
        json!({
            "status": "succeeded", "result": {
                "status": "interrupted", "provider_started": true,
                "provider_session_id": provider_session_id,
                "session_files_state": "persist_failed"
            }
        }),
        true,
    )
    .await;
    let mut db = app.scoped_db_for_org(org.id).await;
    let pending = store
        .pending_bundles(db.conn(), workspace.id, incarnation, Some(session_id))
        .await
        .expect("terminal journal authorizes pending completion");
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].generation, generation);
    assert_eq!(pending[0].provider_session_id, provider_session_id);
    assert_eq!(pending[0].cwd_relative_path, "code");
    assert!(store
        .pending_bundles(
            db.conn(),
            workspace.id,
            SessionFileIncarnation {
                vm_id: Uuid::new_v4(),
                epoch: 1
            },
            Some(session_id)
        )
        .await
        .expect("replacement VM cannot complete it")
        .is_empty());
    db.commit().await.unwrap();
    app.cleanup().await;
}

mod codex_device_auth {
    use super::*;
    use g5_contracts::types::{AgentLaunchFieldPreferenceV1, ChatAgentProvider};

    fn session_id(dispatch: &g5_api::services::agent_dispatch::AgentDispatch) -> Uuid {
        dispatch.invoke_params["input"]["task"]["session_id"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap()
    }

    /// The original prompt's launch choices: Codex runs read-only.
    fn launch_preferences(provider: ChatAgentProvider) -> AgentLaunchPreferencesV1 {
        AgentLaunchPreferencesV1 {
            provider: AgentLaunchFieldPreferenceV1::Explicit(provider),
            permission_mode: AgentLaunchFieldPreferenceV1::Explicit(
                if provider == ChatAgentProvider::Codex {
                    "never/read-only"
                } else {
                    "default"
                }
                .into(),
            ),
            ..Default::default()
        }
    }

    /// A follow-up's launch choices: the chat's permission mode is frozen
    /// per chat, so a follow-up varies what the composer lets it vary, its
    /// effort, and each one gets its own so that a turn reusing another
    /// message's choices would be told apart (`g5.chat.queued-launch`).
    fn follow_up_preferences(effort: &str) -> AgentLaunchPreferencesV1 {
        AgentLaunchPreferencesV1 {
            effort: AgentLaunchFieldPreferenceV1::Explicit(effort.into()),
            ..launch_preferences(ChatAgentProvider::Codex)
        }
    }

    fn effort(dispatch: &g5_api::services::agent_dispatch::AgentDispatch) -> &str {
        dispatch.invoke_params["input"]["provider"]["options"]["effort"]
            .as_str()
            .unwrap_or_default()
    }

    #[tokio::test]
    async fn live_subscription_usage_is_ephemeral_and_attempt_fenced() {
        use std::time::Duration;
        let app = TestApp::spawn().await;
        for auth in [
            g5_contracts::types::AgentAuthModeV1::ChatGptSubscription,
            g5_contracts::types::AgentAuthModeV1::AnthropicOauth,
            g5_contracts::types::AgentAuthModeV1::AnthropicApiKey,
        ] {
            let (owner, _org, workspace, dispatch) = fixture_with_auth(&app, None, auth).await;
            let sid = session_id(&dispatch);
            let token = g5_auth::JwtMinter::new(helpers::TEST_PRIVATE_KEY_PEM, "g5-api-test", "g5")
                .unwrap()
                .mint_workspace_incarnation(
                    workspace.org_id,
                    workspace.id,
                    owner.id,
                    dispatch.vm_id,
                    dispatch.epoch,
                    3600,
                )
                .unwrap();
            let mut socket = app.ws_connect(&owner, &format!("chat:{sid}")).await;
            socket.expect_subscribed().await;
            let url = format!(
                "{}/api/internal/chat-sessions/{sid}/subscription-usage",
                app.base_url
            );
            let http = reqwest::Client::new();
            let report = json!({"taskId":dispatch.task_id,"attempt":dispatch.attempt,"usage":{
                "fiveHourUtilization":0.23,"fiveHourResetAt":null,
                "sevenDayUtilization":0.45,"sevenDayResetAt":null,
                "capturedAt":chrono::Utc::now()
            }});
            let before: i64 =
                sqlx::query_scalar("SELECT count(*) FROM task_events WHERE task_id=$1")
                    .bind(dispatch.task_id)
                    .fetch_one(&app.db)
                    .await
                    .unwrap();
            let response = http
                .post(&url)
                .bearer_auth(&token)
                .json(&report)
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            if auth == g5_contracts::types::AgentAuthModeV1::AnthropicApiKey {
                assert_eq!(response.json::<Value>().await.unwrap()["accepted"], false);
                socket
                    .expect_no_event("chat.subscription_usage", Duration::from_millis(100))
                    .await;
                socket.close().await;
                continue;
            }
            assert_eq!(response.json::<Value>().await.unwrap()["accepted"], true);
            let event = socket
                .expect_event("chat.subscription_usage", Duration::from_secs(3))
                .await;
            assert_eq!(event["sessionId"], sid.to_string());
            assert_eq!(event["authMode"], serde_json::to_value(auth).unwrap());
            assert_eq!(event["usage"]["fiveHourUtilization"], 0.23);
            assert_eq!(event["usage"]["sevenDayUtilization"], 0.45);

            // No quota snapshot or execution row is written by either report.
            let after: i64 =
                sqlx::query_scalar("SELECT count(*) FROM task_events WHERE task_id=$1")
                    .bind(dispatch.task_id)
                    .fetch_one(&app.db)
                    .await
                    .unwrap();
            assert_eq!(before, after);
            let quotas: i64 = sqlx::query_scalar("SELECT count(*) FROM provider_quota_snapshots")
                .fetch_one(&app.db)
                .await
                .unwrap();
            assert_eq!(quotas, 0);
            for patch in [
                json!({"attempt":dispatch.attempt+1}),
                json!({"taskId":Uuid::new_v4()}),
            ] {
                let mut stale = report.clone();
                stale
                    .as_object_mut()
                    .unwrap()
                    .extend(patch.as_object().unwrap().clone());
                let response = http
                    .post(&url)
                    .bearer_auth(&token)
                    .json(&stale)
                    .send()
                    .await
                    .unwrap();
                assert_eq!(response.json::<Value>().await.unwrap()["accepted"], false);
            }
            let mut malformed = report.clone();
            malformed["usage"]["fiveHourUtilization"] = json!(12);
            let response = http
                .post(&url)
                .bearer_auth(&token)
                .json(&malformed)
                .send()
                .await
                .unwrap();
            assert_eq!(response.json::<Value>().await.unwrap()["accepted"], false);
            // Cancelling the attempt revokes reporting before the worker exits.
            sqlx::query("UPDATE tasks SET spec=jsonb_set(spec,'{lifecycle}',COALESCE(spec->'lifecycle','{}'::jsonb)||'{\"cancel_requested\":true}'::jsonb) WHERE id=$1")
            .bind(dispatch.task_id).execute(&app.db).await.unwrap();
            let response = http
                .post(&url)
                .bearer_auth(&token)
                .json(&report)
                .send()
                .await
                .unwrap();
            assert_eq!(response.json::<Value>().await.unwrap()["accepted"], false);
            let stale_vm_token =
                g5_auth::JwtMinter::new(helpers::TEST_PRIVATE_KEY_PEM, "g5-api-test", "g5")
                    .unwrap()
                    .mint_workspace_incarnation(
                        workspace.org_id,
                        workspace.id,
                        owner.id,
                        Uuid::new_v4(),
                        dispatch.epoch,
                        3600,
                    )
                    .unwrap();
            assert_eq!(
                http.post(&url)
                    .bearer_auth(stale_vm_token)
                    .json(&report)
                    .send()
                    .await
                    .unwrap()
                    .status(),
                StatusCode::FORBIDDEN
            );
            assert_eq!(
                http.post(&url).json(&report).send().await.unwrap().status(),
                StatusCode::UNAUTHORIZED
            );
            let foreign = format!(
                "{}/api/internal/chat-sessions/{}/subscription-usage",
                app.base_url,
                Uuid::new_v4()
            );
            assert_eq!(
                http.post(foreign)
                    .bearer_auth(&token)
                    .json(&report)
                    .send()
                    .await
                    .unwrap()
                    .status(),
                StatusCode::FORBIDDEN
            );
            socket
                .expect_no_event("chat.subscription_usage", Duration::from_millis(100))
                .await;
            socket.close().await;
        }
        app.cleanup().await;
    }

    async fn fixture(
        app: &TestApp,
    ) -> (
        TestUser,
        TestOrg,
        TestWorkspace,
        g5_api::services::agent_dispatch::AgentDispatch,
    ) {
        fixture_with_notice(app, None).await
    }

    async fn fixture_with_notice(
        app: &TestApp,
        notice_id: Option<Uuid>,
    ) -> (
        TestUser,
        TestOrg,
        TestWorkspace,
        g5_api::services::agent_dispatch::AgentDispatch,
    ) {
        fixture_with_auth(
            app,
            notice_id,
            g5_contracts::types::AgentAuthModeV1::ChatGptSubscription,
        )
        .await
    }

    async fn fixture_with_auth(
        app: &TestApp,
        notice_id: Option<Uuid>,
        auth: g5_contracts::types::AgentAuthModeV1,
    ) -> (
        TestUser,
        TestOrg,
        TestWorkspace,
        g5_api::services::agent_dispatch::AgentDispatch,
    ) {
        let mut owner = TestUser::create(app).await;
        let org = TestOrg::create_with_admin(app, &mut owner).await;
        if auth == g5_contracts::types::AgentAuthModeV1::AnthropicOauth {
            // Configure the route before reservation; frozen attempts are immutable.
            let secrets = app
                .state
                .scm_token_manager
                .as_ref()
                .unwrap()
                .encrypt_token(r#"{"CLAUDE_CODE_OAUTH_TOKEN":"synthetic-oauth-token"}"#)
                .unwrap();
            sqlx::query("UPDATE org_configs SET secrets_enc=$2,secret_keys=ARRAY['CLAUDE_CODE_OAUTH_TOKEN'] WHERE org_id=$1")
                .bind(org.id).bind(secrets).execute(&app.db).await.unwrap();
        }
        let provider = if auth == g5_contracts::types::AgentAuthModeV1::ChatGptSubscription {
            ChatAgentProvider::Codex
        } else {
            ChatAgentProvider::ClaudeCode
        };
        let workspace = TestWorkspace::create(app, &org).await;
        attach_running_vm_without_runtime(app, &workspace).await;
        let (task_id, session_id) =
            create_agent_task(app, &workspace, owner.id, "queued", None, None).await;
        sqlx::query("UPDATE tasks SET spec=jsonb_set(spec,'{agent,agentLaunch}',$2) WHERE id=$1")
            .bind(task_id)
            .bind(serde_json::to_value(launch_preferences(provider)).unwrap())
            .execute(&app.db)
            .await
            .unwrap();
        queue_session_input(
            app,
            org.id,
            session_id,
            task_id,
            "original subscription prompt",
        )
        .await;
        if let Some(notice_id) = notice_id {
            sqlx::query(
                "INSERT INTO chat_messages(id,org_id,session_id,role,content,artifacts,citations,status,message_index)
                 VALUES ($1,$2,$3,'system','Workspace shutdown is scheduled.',
                     jsonb_build_object('platform_notice',jsonb_build_object(
                         'schema_version',1,'operation_id',$4::text,'stop_reason','drain','delivery','queued')),
                     '[]'::jsonb,'complete',0)",
            ).bind(notice_id).bind(org.id).bind(session_id).bind(Uuid::new_v4())
                .execute(&app.db).await.unwrap();
        }
        let mut db = app.scoped_db_for_org(org.id).await;
        let dispatch = app
            .state
            .task_service
            .reserve_agent_dispatch(&mut db)
            .await
            .unwrap()
            .expect("subscription must reach the VM without preexisting credentials");
        db.commit().await.unwrap();
        assert_eq!(
            dispatch.invoke_params["input"]["provider"]["auth_mode"],
            serde_json::to_value(auth).unwrap()
        );
        (owner, org, workspace, dispatch)
    }

    async fn update(
        app: &TestApp,
        owner: &TestUser,
        workspace: &TestWorkspace,
        dispatch: &g5_api::services::agent_dispatch::AgentDispatch,
        mut body: Value,
    ) -> Value {
        let token = g5_auth::JwtMinter::new(helpers::TEST_PRIVATE_KEY_PEM, "g5-api-test", "g5")
            .unwrap()
            .mint_workspace_incarnation(
                workspace.org_id,
                workspace.id,
                owner.id,
                dispatch.vm_id,
                dispatch.epoch,
                3600,
            )
            .unwrap();
        body["task_id"] = json!(dispatch.task_id);
        body["attempt"] = json!(dispatch.attempt);
        let response = reqwest::Client::new()
            .post(format!(
                "{}/api/internal/chat-sessions/{}/codex-auth",
                app.base_url,
                session_id(dispatch)
            ))
            .bearer_auth(token)
            .json(&body)
            .send()
            .await
            .unwrap();
        let status = response.status();
        let body: Value = response.json().await.unwrap();
        assert_eq!(status, StatusCode::OK, "{body}");
        body
    }

    async fn wait(
        app: &TestApp,
        owner: &TestUser,
        workspace: &TestWorkspace,
        dispatch: &g5_api::services::agent_dispatch::AgentDispatch,
    ) {
        assert_eq!(
            update(app, owner, workspace, dispatch, json!({"phase":"checking"})).await["proceed"],
            true
        );
        assert_eq!(update(app, owner, workspace, dispatch, json!({"phase":"waiting", "user_code":"TEST-12345", "deadline_unix_ms":chrono::Utc::now().timestamp_millis()+300_000})).await["proceed"], true);
    }

    /// A human follow-up with its own Codex launch choices, as the composer
    /// sends one while the owner waits for sign-in.
    fn follow_up_input(
        session_id: Uuid,
        actor_user_id: Uuid,
        content: &str,
        effort: &str,
    ) -> EnqueueAgentInput {
        EnqueueAgentInput {
            task_name: None,
            session_id,
            actor_user_id,
            content: content.into(),
            artifacts: json!([]),
            input_origin: "human".into(),
            template_ref: None,
            template_revision: None,
            confirmed_launch: None,
            agent_launch: follow_up_preferences(effort),
        }
    }

    async fn enqueue_message(
        app: &TestApp,
        owner: &TestUser,
        org: &TestOrg,
        dispatch: &g5_api::services::agent_dispatch::AgentDispatch,
        content: &str,
        effort: &str,
    ) -> Uuid {
        let mut db = app.scoped_db_for_org(org.id).await;
        let queued = app
            .state
            .task_service
            .enqueue_agent_input(
                &mut db,
                follow_up_input(session_id(dispatch), owner.id, content, effort),
            )
            .await
            .unwrap();
        db.commit().await.unwrap();
        queued.input_id
    }

    async fn enqueue(
        app: &TestApp,
        owner: &TestUser,
        org: &TestOrg,
        dispatch: &g5_api::services::agent_dispatch::AgentDispatch,
    ) -> Uuid {
        enqueue_message(app, owner, org, dispatch, "new message", "high").await
    }

    /// The provider accepts the turn's reserved input and the turn succeeds.
    async fn accept_and_finish(
        app: &TestApp,
        org: &TestOrg,
        dispatch: &g5_api::services::agent_dispatch::AgentDispatch,
    ) {
        let (input_id, reservation_id, revision, generation): (Uuid, Uuid, i32, i32) =
            sqlx::query_as(
                "SELECT id, reservation_id, revision, delivery_generation FROM session_inputs
                 WHERE task_id=$1 AND attempt=$2 AND delivery='reserved'",
            )
            .bind(dispatch.task_id)
            .bind(dispatch.attempt)
            .fetch_one(&app.db)
            .await
            .unwrap();
        for (seq, kind, payload, terminal) in [
            (
                1,
                "input.accepted",
                json!({"input_id":input_id,"reservation_id":reservation_id,"revision":revision,
                    "delivery_generation":generation,"system_notice_ids":[]}),
                false,
            ),
            (
                2,
                "message.final",
                json!({"block_id":"codex-message-1:0","content":"Done."}),
                false,
            ),
            (
                3,
                "invocation.outcome",
                json!({"status":"succeeded","result":{"status":"succeeded",
                    "usage":{"input_tokens":1,"output_tokens":1}}}),
                true,
            ),
        ] {
            apply_agent_attempt_event(
                app,
                org.id,
                dispatch.task_id,
                dispatch.attempt,
                dispatch.vm_id,
                dispatch.epoch,
                seq,
                kind,
                payload,
                terminal,
            )
            .await;
        }
    }

    async fn reserve_next(
        app: &TestApp,
        org: &TestOrg,
    ) -> g5_api::services::agent_dispatch::AgentDispatch {
        let mut db = app.scoped_db_for_org(org.id).await;
        let next = app
            .state
            .task_service
            .reserve_agent_dispatch(&mut db)
            .await
            .unwrap()
            .expect("a queued follow-up dispatches");
        db.commit().await.unwrap();
        next
    }

    #[tokio::test]
    async fn challenge_is_owner_only_encrypted_and_ready_clears_it() {
        let app = TestApp::spawn().await;
        let (owner, org, workspace, dispatch) = fixture(&app).await;
        wait(&app, &owner, &workspace, &dispatch).await;
        let path = format!("/api/v1/sessions/{}/codex-auth", session_id(&dispatch));
        let response = app.authenticated_client(&owner).get(&path).await;
        assert_eq!(response.headers()["cache-control"], "no-store");
        let body: Value = response.json().await.unwrap();
        assert_eq!(body["data"]["userCode"], "TEST-12345");
        let reader = TestUser::create_with_scopes(
            &app,
            org.id,
            &[Scope::SessionsRead, Scope::WorkspacesRead],
        )
        .await;
        sqlx::query("UPDATE org_memberships SET role='admin' WHERE org_id=$1 AND user_id=$2")
            .bind(org.id)
            .bind(reader.id)
            .execute(&app.db)
            .await
            .unwrap();
        assert_eq!(
            app.authenticated_client(&reader).get(&path).await.status(),
            StatusCode::FORBIDDEN
        );
        let (state, encrypted): (Value, Vec<u8>) = sqlx::query_as("SELECT codex_auth_state,codex_auth_challenge_enc FROM task_attempts WHERE task_id=$1 AND attempt=1")
            .bind(dispatch.task_id).fetch_one(&app.db).await.unwrap();
        assert!(!state.to_string().contains("TEST-12345"));
        assert!(!encrypted.windows(10).any(|part| part == b"TEST-12345"));
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM task_events WHERE task_id=$1 AND (metadata::text LIKE '%TEST-12345%' OR content LIKE '%TEST-12345%')")
            .bind(dispatch.task_id).fetch_one(&app.db).await.unwrap();
        assert_eq!(count, 0);
        assert_eq!(
            update(
                &app,
                &owner,
                &workspace,
                &dispatch,
                json!({"phase":"ready"})
            )
            .await["proceed"],
            true
        );
        let body: Value = app
            .authenticated_client(&owner)
            .get(&path)
            .await
            .json()
            .await
            .unwrap();
        assert!(body["data"].is_null());
        let cleared: bool = sqlx::query_scalar("SELECT codex_auth_challenge_enc IS NULL FROM task_attempts WHERE task_id=$1 AND attempt=1")
            .bind(dispatch.task_id).fetch_one(&app.db).await.unwrap();
        assert!(cleared);
        enqueue(&app, &owner, &org, &dispatch).await;
        let reason: Option<String> = sqlx::query_scalar(
            "SELECT spec #>> '{lifecycle,interrupt_reason}' FROM tasks WHERE id=$1",
        )
        .bind(dispatch.task_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
        assert!(
            reason.is_none(),
            "new input after readiness must queue normally"
        );
        app.cleanup().await;
    }

    #[tokio::test]
    async fn challenge_read_fences_ownership_until_the_read_transaction_closes() {
        let app = TestApp::spawn().await;
        let (owner, _org, workspace, dispatch) = fixture(&app).await;
        wait(&app, &owner, &workspace, &dispatch).await;
        let path = format!("/api/v1/sessions/{}/codex-auth", session_id(&dispatch));

        // Pause the real HTTP handler's second query, after its owner check.
        let mut blocker = app.db.begin().await.unwrap();
        let blocker_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
            .fetch_one(&mut *blocker)
            .await
            .unwrap();
        sqlx::query("LOCK TABLE task_attempts IN ACCESS EXCLUSIVE MODE")
            .execute(&mut *blocker)
            .await
            .unwrap();
        let client = app.authenticated_client(&owner);
        let read = tokio::spawn(async move { client.get(&path).await });
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                let waiting: bool = sqlx::query_scalar(
                    "SELECT EXISTS(SELECT 1 FROM pg_stat_activity
                     WHERE datname=current_database() AND $1=ANY(pg_blocking_pids(pid))
                       AND query LIKE '%attempt.codex_auth_challenge_enc%')",
                )
                .bind(blocker_pid)
                .fetch_one(&app.db)
                .await
                .unwrap();
                if waiting {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("challenge read must reach its second query");

        // Ownership transfer starts with this same row lock. It cannot pass
        // the owner check while the private challenge read is still pending.
        let mut transfer = app.db.begin().await.unwrap();
        let conflict = sqlx::query("SELECT id FROM workspaces WHERE id=$1 FOR UPDATE NOWAIT")
            .bind(workspace.id)
            .execute(&mut *transfer)
            .await
            .expect_err("ownership must stay fenced throughout the challenge read");
        assert_eq!(
            conflict.as_database_error().unwrap().code().as_deref(),
            Some("55P03")
        );
        transfer.rollback().await.unwrap();
        blocker.rollback().await.unwrap();

        let response = tokio::time::timeout(std::time::Duration::from_secs(5), read)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: Value = response.json().await.unwrap();
        assert_eq!(body["data"]["userCode"], "TEST-12345");

        // A completed poll must not leave the owner/enqueue row lock behind.
        let mut transfer = app.db.begin().await.unwrap();
        sqlx::query("SELECT id FROM workspaces WHERE id=$1 FOR UPDATE NOWAIT")
            .bind(workspace.id)
            .execute(&mut *transfer)
            .await
            .expect("poll must close its transaction before returning");
        transfer.rollback().await.unwrap();
        app.cleanup().await;
    }

    /// The challenge read checks the VM lease with the clock at the time of
    /// the read, not when its transaction began: when the read's transaction
    /// waits for the workspace row (in its authorization) while the lease
    /// ends, the challenge is shown as interrupted, without its code.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn challenge_read_checks_the_lease_after_its_transaction_waits() {
        let app = TestApp::spawn().await;
        let (owner, _org, workspace, dispatch) = fixture(&app).await;
        wait(&app, &owner, &workspace, &dispatch).await;
        let path = format!("/api/v1/sessions/{}/codex-auth", session_id(&dispatch));

        let holder = hold_while_the_lease_ends(
            &app,
            workspace.id,
            "SELECT 1 FROM workspaces WHERE id = $1 FOR UPDATE",
        )
        .await;
        let client = app.authenticated_client(&owner);
        let release = async {
            wait_for_lock_wait_past_the_lease(&app, workspace.id, AUTHORIZATION_LOCK).await;
            holder.rollback().await.unwrap();
        };
        let (response, ()) = tokio::join!(client.get(&path), release);
        let body: Value = response.json().await.unwrap();
        assert_eq!(body["data"]["phase"], "ended", "{body}");
        assert!(body["data"]["userCode"].is_null(), "{body}");
        app.cleanup().await;
    }

    /// Messages sent during sign-in queue behind the original prompt without
    /// touching the login (`g5.chat.login-before-prompt`, `g5.chat.queue`):
    /// one before the CLI reports `checking`, one while the challenge shows.
    /// After login the original prompt runs, then the follow-ups in order,
    /// each with its own launch choices.
    #[tokio::test]
    async fn follow_ups_queue_during_login_and_run_in_order_after_the_original_prompt() {
        let app = TestApp::spawn().await;
        let (owner, org, workspace, dispatch) = fixture(&app).await;
        let sid = session_id(&dispatch);
        let second = enqueue_message(&app, &owner, &org, &dispatch, "second message", "high").await;
        let response = update(
            &app,
            &owner,
            &workspace,
            &dispatch,
            json!({"phase":"checking"}),
        )
        .await;
        assert_eq!(response["proceed"], true, "{response}");
        assert!(response.get("code").is_none(), "{response}");
        let deadline = chrono::Utc::now().timestamp_millis() + 300_000;
        assert_eq!(
            update(
                &app,
                &owner,
                &workspace,
                &dispatch,
                json!({"phase":"waiting", "user_code":"TEST-12345", "deadline_unix_ms":deadline})
            )
            .await["proceed"],
            true
        );
        let third = enqueue_message(&app, &owner, &org, &dispatch, "third message", "low").await;
        // Queued follow-ups stay editable and withdrawable until reserved.
        let client = app.authenticated_client(&owner);
        let edit = client
            .patch_json(
                &format!("/api/v1/sessions/{sid}/inputs/{third}"),
                &json!({"expectedRevision": 1, "content": "third message, edited"}),
            )
            .await;
        assert_eq!(edit.status(), StatusCode::OK);
        let fourth =
            enqueue_message(&app, &owner, &org, &dispatch, "withdrawn message", "high").await;
        let withdraw = client
            .post_json(
                &format!("/api/v1/sessions/{sid}/inputs/{fourth}/withdraw"),
                &json!({"expectedRevision": 1}),
            )
            .await;
        assert_eq!(withdraw.status(), StatusCode::OK);

        // The login is untouched: same phase, deadline and code; the original
        // reservation stands; no interruption and no notice.
        let (phase, stored_deadline, reason, status): (String, i64, Option<String>, String) =
            sqlx::query_as(
                "SELECT attempt.codex_auth_state->>'phase',
                        (attempt.codex_auth_state->>'deadline_unix_ms')::bigint,
                        task.spec #>> '{lifecycle,interrupt_reason}', task.status
                 FROM tasks task JOIN task_attempts attempt
                   ON attempt.task_id=task.id AND attempt.attempt=task.attempt
                 WHERE task.id=$1",
            )
            .bind(dispatch.task_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
        assert_eq!(
            (phase.as_str(), stored_deadline, reason, status.as_str()),
            ("waiting", deadline, None, "running")
        );
        let challenge: Value = client
            .get(&format!("/api/v1/sessions/{sid}/codex-auth"))
            .await
            .json()
            .await
            .unwrap();
        assert_eq!(challenge["data"]["userCode"], "TEST-12345");
        assert_eq!(challenge["data"]["deadlineUnixMs"], deadline);
        assert_eq!(challenge["data"]["taskId"], dispatch.task_id.to_string());
        let inputs: Vec<(Uuid, String, String, Option<Uuid>)> = sqlx::query_as(
            "SELECT id, content, delivery, task_id FROM session_inputs
             WHERE session_id=$1 ORDER BY input_index, id",
        )
        .bind(sid)
        .fetch_all(&app.db)
        .await
        .unwrap();
        assert_eq!(inputs.len(), 4, "{inputs:?}");
        assert_eq!(
            (inputs[0].1.as_str(), inputs[0].2.as_str(), inputs[0].3),
            (
                "original subscription prompt",
                "reserved",
                Some(dispatch.task_id)
            )
        );
        assert_eq!(
            (inputs[1].0, inputs[1].2.as_str(), inputs[1].3),
            (second, "queued", Some(dispatch.task_id))
        );
        assert_eq!(
            (
                inputs[2].0,
                inputs[2].1.as_str(),
                inputs[2].2.as_str(),
                inputs[2].3
            ),
            (
                third,
                "third message, edited",
                "queued",
                Some(dispatch.task_id)
            )
        );
        assert_eq!((inputs[3].0, inputs[3].2.as_str()), (fourth, "withdrawn"));
        let notices: i64 =
            sqlx::query_scalar("SELECT count(*) FROM chat_messages WHERE session_id=$1")
                .bind(sid)
                .fetch_one(&app.db)
                .await
                .unwrap();
        assert_eq!(notices, 0, "no cancellation or failed-turn notice");

        // Login completes; the original prompt is delivered and the turn ends.
        assert_eq!(
            update(
                &app,
                &owner,
                &workspace,
                &dispatch,
                json!({"phase":"ready"})
            )
            .await["proceed"],
            true
        );
        accept_and_finish(&app, &org, &dispatch).await;
        let (status, original_delivery): (String, String) = sqlx::query_as(
            "SELECT task.status, input.delivery FROM tasks task
             JOIN session_inputs input ON input.id=$2 WHERE task.id=$1",
        )
        .bind(dispatch.task_id)
        .bind(inputs[0].0)
        .fetch_one(&app.db)
        .await
        .unwrap();
        assert_eq!(
            (status.as_str(), original_delivery.as_str()),
            ("succeeded", "delivered")
        );

        // The follow-ups run next, in order, on a new turn with their own
        // Codex launch choices; nothing replays the original prompt.
        let next = reserve_next(&app, &org).await;
        assert_ne!(next.task_id, dispatch.task_id);
        assert_eq!(
            next.invoke_params["input"]["task"]["initial_input"]["input_id"],
            second.to_string()
        );
        // The new turn carries the delivered conversation as historical
        // context only; the original prompt is not executed again.
        let prompt = next.invoke_params["input"]["prompt"]["text"]
            .as_str()
            .unwrap();
        assert!(
            prompt.ends_with("Current input follows:\nsecond message"),
            "{prompt}"
        );
        assert!(
            prompt.contains("do not execute earlier user requests again"),
            "{prompt}"
        );
        assert_eq!(
            next.invoke_params["input"]["provider"]["auth_mode"],
            json!("chat_gpt_subscription")
        );
        // Each turn runs with its own message's choices, not the original's
        // or an earlier follow-up's.
        assert_ne!(effort(&dispatch), "high");
        assert_eq!(effort(&next), "high");
        let (third_task, third_delivery): (Option<Uuid>, String) =
            sqlx::query_as("SELECT task_id, delivery FROM session_inputs WHERE id=$1")
                .bind(third)
                .fetch_one(&app.db)
                .await
                .unwrap();
        assert_eq!(
            (third_task, third_delivery.as_str()),
            (Some(next.task_id), "queued")
        );
        accept_and_finish(&app, &org, &next).await;
        let last = reserve_next(&app, &org).await;
        assert_ne!(last.task_id, next.task_id);
        assert_eq!(
            last.invoke_params["input"]["task"]["initial_input"]["input_id"],
            third.to_string()
        );
        let prompt = last.invoke_params["input"]["prompt"]["text"]
            .as_str()
            .unwrap();
        assert!(
            prompt.ends_with("Current input follows:\nthird message, edited"),
            "{prompt}"
        );
        assert_eq!(effort(&last), "low");
        app.cleanup().await;
    }

    /// Readiness locks the task and attempt rows; a follow-up locks the
    /// session, then the task row. Whichever commits first, the other waits
    /// and then succeeds: the login proceeds and the message stays queued.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn readiness_and_a_follow_up_succeed_in_either_lock_order() {
        use g5_internal_api::codex_auth::{CodexAuthPhase, CodexAuthUpdate};
        let app = TestApp::spawn().await;
        for ready_first in [true, false] {
            let (owner, org, workspace, dispatch) = fixture(&app).await;
            wait(&app, &owner, &workspace, &dispatch).await;
            let sid = session_id(&dispatch);
            let service = app.state.task_service.clone();
            let ready_update = || CodexAuthUpdate {
                task_id: dispatch.task_id,
                attempt: dispatch.attempt,
                update: CodexAuthPhase::Ready,
            };
            let mut first = app.scoped_db_for_org(org.id).await;
            if ready_first {
                let ready = service
                    .update_codex_auth(
                        &mut first,
                        sid,
                        owner.id,
                        dispatch.vm_id,
                        dispatch.epoch,
                        ready_update(),
                    )
                    .await
                    .unwrap();
                assert!(ready.proceed);
            } else {
                service
                    .enqueue_agent_input(
                        &mut first,
                        follow_up_input(sid, owner.id, "follow-up", "high"),
                    )
                    .await
                    .unwrap();
            }
            let mut second_db = app.scoped_db_for_org(org.id).await;
            let second_service = service.clone();
            let (actor, vm_id, epoch) = (owner.id, dispatch.vm_id, dispatch.epoch);
            let second_update = ready_update();
            let mut second = tokio::spawn(async move {
                let proceeded = if ready_first {
                    second_service
                        .enqueue_agent_input(
                            &mut second_db,
                            follow_up_input(sid, actor, "follow-up", "high"),
                        )
                        .await
                        .map(|_| true)
                } else {
                    second_service
                        .update_codex_auth(&mut second_db, sid, actor, vm_id, epoch, second_update)
                        .await
                        .map(|disposition| disposition.proceed)
                };
                second_db.commit().await.unwrap();
                proceeded
            });
            assert!(
                tokio::time::timeout(std::time::Duration::from_millis(500), &mut second)
                    .await
                    .is_err(),
                "ready_first={ready_first}: the second request waits for the first's task lock"
            );
            first.commit().await.unwrap();
            let proceeded = tokio::time::timeout(std::time::Duration::from_secs(5), second)
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            assert!(proceeded, "ready_first={ready_first}");
            let (phase, reason, queued): (String, Option<String>, i64) = sqlx::query_as(
                "SELECT attempt.codex_auth_state->>'phase', task.spec #>> '{lifecycle,interrupt_reason}',
                        (SELECT count(*) FROM session_inputs input
                         WHERE input.task_id=task.id AND input.delivery='queued')
                 FROM tasks task JOIN task_attempts attempt
                   ON attempt.task_id=task.id AND attempt.attempt=task.attempt
                 WHERE task.id=$1",
            )
            .bind(dispatch.task_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
            assert_eq!(
                (phase.as_str(), reason, queued),
                ("ready", None, 1),
                "ready_first={ready_first}"
            );
        }
        app.cleanup().await;
    }

    /// The owner's Stop is what ends a pending login. It ends that prompt's
    /// turn alone: the prompt is withdrawn with the interrupted notice, and
    /// the follow-up queued behind it starts the next turn.
    #[tokio::test]
    async fn stop_during_login_ends_only_the_prompt_and_the_queued_follow_up_starts_the_next_turn()
    {
        let app = TestApp::spawn().await;
        let (owner, org, workspace, dispatch) = fixture(&app).await;
        wait(&app, &owner, &workspace, &dispatch).await;
        let sid = session_id(&dispatch);
        let follow_up =
            enqueue_message(&app, &owner, &org, &dispatch, "after the stop", "high").await;
        let original: Uuid = sqlx::query_scalar(
            "SELECT id FROM session_inputs WHERE task_id=$1 AND delivery='reserved'",
        )
        .bind(dispatch.task_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
        let mut db = app.scoped_db_for_org(org.id).await;
        let stopped = app
            .state
            .task_service
            .end_chat_turn(&mut db, owner.id, sid, original)
            .await
            .unwrap();
        db.commit().await.unwrap();
        assert!(format!("{stopped:?}").starts_with("Stopped"), "{stopped:?}");
        assert_eq!(
            update(
                &app,
                &owner,
                &workspace,
                &dispatch,
                json!({"phase":"ready"})
            )
            .await["proceed"],
            false,
            "a login finishing after Stop cannot deliver the stopped prompt"
        );
        apply_agent_event(
            &app,
            org.id,
            (dispatch.task_id, dispatch.vm_id),
            1,
            "invocation.outcome",
            json!({"status":"cancelled"}),
            true,
        )
        .await;
        let (status, original_delivery, notice): (String, String, String) = sqlx::query_as(
            "SELECT task.status, input.delivery,
                    (SELECT content FROM chat_messages WHERE task_id=task.id AND role='assistant')
             FROM tasks task JOIN session_inputs input ON input.id=$2 WHERE task.id=$1",
        )
        .bind(dispatch.task_id)
        .bind(original)
        .fetch_one(&app.db)
        .await
        .unwrap();
        // No prompt crossed the provider boundary, so the turn fails as every
        // ended login does (`g5.chat.failed-login`), and its prompt is withdrawn.
        assert_eq!(
            (status.as_str(), original_delivery.as_str()),
            ("failed", "withdrawn")
        );
        assert_eq!(
            notice,
            g5_internal_api::codex_auth::auth_notice("codex_auth_interrupted")
        );
        let next = reserve_next(&app, &org).await;
        assert_ne!(next.task_id, dispatch.task_id);
        assert_eq!(
            next.invoke_params["input"]["task"]["initial_input"]["input_id"],
            follow_up.to_string()
        );
        assert_eq!(
            next.invoke_params["input"]["prompt"]["text"],
            "after the stop"
        );
        assert_eq!(effort(&next), "high");
        app.cleanup().await;
    }

    #[tokio::test]
    async fn deadline_and_vm_loss_cannot_resume_the_original_prompt() {
        for lose_vm in [false, true] {
            let app = TestApp::spawn().await;
            let (owner, org, workspace, dispatch) = fixture(&app).await;
            wait(&app, &owner, &workspace, &dispatch).await;
            let follow_up = enqueue_message(
                &app,
                &owner,
                &org,
                &dispatch,
                "queued during the login",
                "high",
            )
            .await;
            if lose_vm {
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
                            evidence: json!({"reason":"test_vm_loss"}),
                        },
                    )
                    .await
                    .unwrap();
                db.commit().await.unwrap();
            } else {
                sqlx::query("UPDATE task_attempts SET codex_auth_state=jsonb_set(codex_auth_state,'{deadline_unix_ms}','1'::jsonb) WHERE task_id=$1")
                    .bind(dispatch.task_id).execute(&app.db).await.unwrap();
                assert_eq!(
                    update(
                        &app,
                        &owner,
                        &workspace,
                        &dispatch,
                        json!({"phase":"ready"})
                    )
                    .await["proceed"],
                    false
                );
                apply_agent_event(&app, org.id, (dispatch.task_id, dispatch.vm_id), 1, "invocation.outcome", json!({"status":"succeeded","result":{"status":"failed","code":"codex_auth_timeout"}}), true).await;
            }
            let (status, secret_clear): (String, bool) = sqlx::query_as("SELECT task.status,attempt.codex_auth_challenge_enc IS NULL FROM tasks task JOIN task_attempts attempt ON attempt.task_id=task.id WHERE task.id=$1")
                .bind(dispatch.task_id).fetch_one(&app.db).await.unwrap();
            assert_eq!(status, "failed");
            assert!(secret_clear);
            // The failed prompt is withdrawn, not replayed; the follow-up
            // queued behind it starts the next turn.
            let original_delivery: String =
                sqlx::query_scalar("SELECT delivery FROM session_inputs WHERE task_id=$1")
                    .bind(dispatch.task_id)
                    .fetch_one(&app.db)
                    .await
                    .unwrap();
            assert_eq!(original_delivery, "withdrawn", "lose_vm={lose_vm}");
            let (delivery, next_task, next_status): (String, Option<Uuid>, Option<String>) =
                sqlx::query_as(
                    "SELECT input.delivery, input.task_id, task.status FROM session_inputs input
                     LEFT JOIN tasks task ON task.id=input.task_id WHERE input.id=$1",
                )
                .bind(follow_up)
                .fetch_one(&app.db)
                .await
                .unwrap();
            assert_eq!(delivery, "queued", "lose_vm={lose_vm}");
            assert_ne!(next_task, Some(dispatch.task_id), "lose_vm={lose_vm}");
            assert_eq!(next_status.as_deref(), Some("queued"), "lose_vm={lose_vm}");
            if !lose_vm {
                let next = reserve_next(&app, &org).await;
                assert_eq!(
                    next.invoke_params["input"]["task"]["initial_input"]["input_id"],
                    follow_up.to_string()
                );
                assert_eq!(
                    next.invoke_params["input"]["prompt"]["text"],
                    "queued during the login"
                );
                assert_eq!(effort(&next), "high");
            }
            app.cleanup().await;
        }
    }

    #[tokio::test]
    async fn owner_actions_do_not_invert_the_auth_workspace_task_lock_order() {
        use g5_internal_api::codex_auth::{CodexAuthPhase, CodexAuthUpdate};
        let app = TestApp::spawn().await;
        for action in ["cancel", "pause", "continue"] {
            let (owner, org, workspace, dispatch) = fixture(&app).await;
            let mut auth_db = app.scoped_db_for_org(org.id).await;
            // Stage the first lock held by the real incarnation-auth HTTP route.
            sqlx::query("SELECT id FROM workspaces WHERE id=$1 FOR SHARE")
                .bind(workspace.id)
                .execute(auth_db.conn())
                .await
                .unwrap();
            let auth_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
                .fetch_one(auth_db.conn())
                .await
                .unwrap();
            let mut action_db = app.scoped_db_for_org(org.id).await;
            let task_id = dispatch.task_id;
            let actor_id = owner.id;
            // An owner control keeps the workspace shared and never upgrades
            // it (`g5.chat.control-locks`): it completes while the login
            // request holds its share, instead of queueing behind it.
            let action_result = tokio::time::timeout(std::time::Duration::from_secs(3), async {
                match action {
                    "cancel" => app
                        .state
                        .task_service
                        .request_workspace_task_cancel(&mut action_db, actor_id, task_id)
                        .await
                        .map(|result| result.status),
                    "pause" => app
                        .state
                        .task_service
                        .request_workspace_task_pause(&mut action_db, actor_id, task_id)
                        .await
                        .map(|result| result.status),
                    _ => {
                        app.state
                            .task_service
                            .continue_workspace_task(&mut action_db, actor_id, task_id)
                            .await
                    }
                }
                .map_err(|error| format!("{error:?}"))
            })
            .await
            .expect("an owner action must not wait for the login's workspace share");
            // The action holds the task row; the login's update then waits
            // for the action to end, and the order share → task on both sides
            // leaves no cycle.
            let service = app.state.task_service.clone();
            let session = session_id(&dispatch);
            let (vm_id, epoch, attempt) = (dispatch.vm_id, dispatch.epoch, dispatch.attempt);
            let ready = tokio::spawn(async move {
                let ready = service
                    .update_codex_auth(
                        &mut auth_db,
                        session,
                        actor_id,
                        vm_id,
                        epoch,
                        CodexAuthUpdate {
                            task_id,
                            attempt,
                            update: CodexAuthPhase::Ready,
                        },
                    )
                    .await;
                auth_db.rollback().await.unwrap();
                ready
            });
            tokio::time::timeout(std::time::Duration::from_secs(3), async {
                loop {
                    let waiting: bool = sqlx::query_scalar(
                        "SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE pid=$1 AND wait_event_type='Lock')",
                    ).bind(auth_pid).fetch_one(&app.db).await.unwrap();
                    if waiting { break; }
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("the login's task update waits for the owner action's task lock");
            action_db.rollback().await.unwrap();
            let ready = tokio::time::timeout(std::time::Duration::from_secs(3), ready)
                .await
                .unwrap()
                .unwrap();
            assert!(ready.is_ok(), "{action} deadlocked auth update: {ready:?}");
            match action {
                "cancel" => assert_eq!(action_result.unwrap(), "cancelling"),
                "pause" => assert_eq!(action_result.unwrap(), "pausing"),
                _ => assert!(action_result
                    .unwrap_err()
                    .contains("no held continuation from running")),
            }
        }
        app.cleanup().await;
    }
    #[tokio::test]
    async fn durable_outcome_recovers_the_auth_reason_when_ended_post_was_lost() {
        let app = TestApp::spawn().await;
        for (code, interruption, expected) in [
            ("codex_auth_timeout", None, "codex_auth_timeout"),
            ("codex_auth_disabled", None, "codex_auth_disabled"),
            ("untrusted_code", None, "codex_auth_interrupted"),
            (
                "codex_auth_timeout",
                Some("historical_superseded"),
                "codex_auth_superseded",
            ),
            (
                "codex_auth_timeout",
                Some("cancel"),
                "codex_auth_interrupted",
            ),
        ] {
            let (owner, org, workspace, dispatch) = fixture(&app).await;
            wait(&app, &owner, &workspace, &dispatch).await;
            if interruption == Some("historical_superseded") {
                // Compatibility fixture: an attempt that a new message cancelled
                // before follow-ups queued during login
                // (`g5.chat.login-before-prompt`); its record and notice stay
                // readable, and nothing writes this phase for new attempts.
                sqlx::query("UPDATE task_attempts SET codex_auth_challenge_enc=NULL,
                        codex_auth_state=jsonb_build_object('phase','superseded','code','codex_auth_superseded')
                     WHERE task_id=$1 AND attempt=$2")
                    .bind(dispatch.task_id).bind(dispatch.attempt).execute(&app.db).await.unwrap();
                sqlx::query("UPDATE tasks SET spec=jsonb_set(spec,'{lifecycle}',COALESCE(spec->'lifecycle','{}'::jsonb)
                        || jsonb_build_object('interrupt_reason','codex_auth_superseded','interrupt_requested_at',now()),true)
                     WHERE id=$1")
                    .bind(dispatch.task_id).execute(&app.db).await.unwrap();
            } else if interruption == Some("cancel") {
                let mut db = app.scoped_db_for_org(org.id).await;
                app.state
                    .task_service
                    .request_workspace_task_cancel(&mut db, owner.id, dispatch.task_id)
                    .await
                    .unwrap();
                db.commit().await.unwrap();
            }
            // The best-effort private Ended POST was lost; journal replay is
            // the only durable source of the subprocess failure reason.
            apply_agent_event(
                &app,
                org.id,
                (dispatch.task_id, dispatch.vm_id),
                1,
                "invocation.outcome",
                json!({"status":"succeeded","result":{"status":"failed","code":code}}),
                true,
            )
            .await;
            let reason: String = sqlx::query_scalar(
                "SELECT codex_auth_state->>'code' FROM task_attempts WHERE task_id=$1 AND attempt=$2",
            ).bind(dispatch.task_id).bind(dispatch.attempt).fetch_one(&app.db).await.unwrap();
            assert_eq!(reason, expected);
            let notice: String = sqlx::query_scalar(
                "SELECT content FROM chat_messages WHERE task_id=$1 AND role='assistant'",
            )
            .bind(dispatch.task_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
            assert_eq!(notice, g5_internal_api::codex_auth::auth_notice(expected));
            // Shared transcript readers can reconcile the notice even when
            // the journal page and private owner auth card are absent.
            let reader = TestUser::create_with_scopes(
                &app,
                org.id,
                &[Scope::SessionsRead, Scope::WorkspacesRead],
            )
            .await;
            sqlx::query("UPDATE org_memberships SET role='admin' WHERE org_id=$1 AND user_id=$2")
                .bind(org.id)
                .bind(reader.id)
                .execute(&app.db)
                .await
                .unwrap();
            for actor in [&owner, &reader] {
                for query in ["", "?tail=true&limit=10"] {
                    let response = app
                        .authenticated_client(actor)
                        .get(&format!(
                            "/api/v1/sessions/{}/messages{query}",
                            session_id(&dispatch)
                        ))
                        .await;
                    assert_eq!(response.status(), StatusCode::OK);
                    let body: Value = response.json().await.unwrap();
                    let messages = body["data"]["messages"].as_array().unwrap();
                    assert_eq!(messages.len(), 2);
                    for message in messages {
                        assert_eq!(message["status"], "error");
                        assert_eq!(message["isErrorNotice"], message["role"] == "assistant");
                        assert_eq!(
                            message["codexAuthFailure"],
                            json!({
                                "taskId": dispatch.task_id, "attempt": dispatch.attempt,
                            })
                        );
                        assert!(message.get("userCode").is_none());
                    }
                }
            }
        }
        app.cleanup().await;
    }

    #[tokio::test]
    async fn terminal_auth_failure_withdraws_reserved_platform_notices() {
        let app = TestApp::spawn().await;
        for terminal_path in ["outcome", "vm_loss", "before_start"] {
            let notice_id = Uuid::new_v4();
            let (_, org, _, dispatch) = fixture_with_notice(&app, Some(notice_id)).await;
            assert_eq!(
                dispatch.invoke_params["input"]["task"]["initial_input"]["system_notice_ids"],
                json!([notice_id])
            );
            let mut db = app.scoped_db_for_org(org.id).await;
            let event = match terminal_path {
                "vm_loss" => TaskLifecycleEvent::VmLost {
                    task_id: dispatch.task_id,
                    attempt: dispatch.attempt,
                    vm_id: dispatch.vm_id,
                    epoch: dispatch.epoch,
                    evidence: json!({"reason":"test"}),
                },
                "before_start" => TaskLifecycleEvent::StartDisposition {
                    task_id: dispatch.task_id,
                    attempt: dispatch.attempt,
                    vm_id: dispatch.vm_id,
                    epoch: dispatch.epoch,
                    disposition: json!({"disposition":"cancelled"}),
                },
                _ => TaskLifecycleEvent::RecoveredOutcome {
                    task_id: dispatch.task_id,
                    attempt: dispatch.attempt,
                    vm_id: dispatch.vm_id,
                    epoch: dispatch.epoch,
                    outcome: json!({"status":"failed"}),
                },
            };
            app.state.task_service.apply(&mut db, event).await.unwrap();
            db.commit().await.unwrap();
            let delivery: String = sqlx::query_scalar(
                "SELECT artifacts #>> '{platform_notice,delivery}' FROM chat_messages WHERE id=$1",
            )
            .bind(notice_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
            assert_eq!(
                delivery, "withdrawn",
                "{terminal_path} stranded its reserved notice"
            );
        }
        app.cleanup().await;
    }
}

async fn attach_running_vm_without_runtime(app: &TestApp, workspace: &TestWorkspace) -> Uuid {
    let host_id = Uuid::new_v4();
    let vm_id = Uuid::new_v4();
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
             org_id, workspace_id, host_id, vm_id, epoch, status, lease_expires_at,
             module_catalog_version
         ) VALUES ($1,$2,$3,$4,1,'running',now() + interval '1 hour',$5)",
    )
    .bind(workspace.org_id)
    .bind(workspace.id)
    .bind(host_id)
    .bind(vm_id.to_string())
    // A running incarnation records the catalog its host verified at Ready.
    .bind(g5_internal_api::host_ctl::MODULE_CATALOG_VERSION)
    .execute(&app.db)
    .await
    .expect("insert running dispatch test VM");
    app.attach_dispatch_host(host_id).await;
    vm_id
}

async fn create_agent_task(
    app: &TestApp,
    workspace: &TestWorkspace,
    owner_id: Uuid,
    status: &str,
    continuation_reason: Option<&str>,
    hold_reason: Option<&str>,
) -> (Uuid, Uuid) {
    let session_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO chat_sessions (
             id, org_id, workspace_id, scope_type, title, created_by
         ) VALUES ($1, $2, $3, 'workspace', 'Task lifecycle test', $4)",
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
             chat_session_id, continuation_reason, hold_reason, created_by
         ) VALUES (
             $1, $2, $3, 'Task lifecycle test', 'agent', $4,
             jsonb_build_object(
                 'agent', jsonb_build_object(
                     'schemaVersion', 'g5.agent-task-spec/v1',
                     'agentLaunch', jsonb_build_object('schemaVersion', 1)
                 )
             ), 1,
             $5, $6, $7, $8
         )",
    )
    .bind(task_id)
    .bind(workspace.org_id)
    .bind(workspace.id)
    .bind(status)
    .bind(session_id)
    .bind(continuation_reason)
    .bind(hold_reason)
    .bind(owner_id)
    .execute(&app.db)
    .await
    .expect("insert agent task");

    (task_id, session_id)
}

async fn queue_session_input(
    app: &TestApp,
    org_id: Uuid,
    session_id: Uuid,
    task_id: Uuid,
    content: &str,
) -> Uuid {
    queue_session_input_with_origin(
        app, org_id, session_id, task_id, content, "human", None, None,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn queue_session_input_with_origin(
    app: &TestApp,
    org_id: Uuid,
    session_id: Uuid,
    task_id: Uuid,
    content: &str,
    input_origin: &str,
    template_ref: Option<&str>,
    template_revision: Option<&str>,
) -> Uuid {
    let input_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO session_inputs(
             id, org_id, session_id, input_index, content, input_origin,
             template_ref, template_revision, delivery, task_id
         ) VALUES (
             $1,$2,$3,0,$4,$5,$6,$7,'queued',$8
         )",
    )
    .bind(input_id)
    .bind(org_id)
    .bind(session_id)
    .bind(content)
    .bind(input_origin)
    .bind(template_ref)
    .bind(template_revision)
    .bind(task_id)
    .execute(&app.db)
    .await
    .expect("queue session input");
    input_id
}

#[tokio::test]
async fn stale_native_generation_uses_frozen_delivered_history() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    attach_running_vm_without_runtime(&app, &workspace).await;
    let (task_id, session_id) =
        create_agent_task(&app, &workspace, owner.id, "queued", None, None).await;
    let old_provider_id = Uuid::new_v4().to_string();
    sqlx::query(
        "UPDATE chat_sessions
         SET provider_session_id = $2, session_manifest_generation = 7,
             message_count = 3
         WHERE id = $1",
    )
    .bind(session_id)
    .bind(&old_provider_id)
    .execute(&app.db)
    .await
    .unwrap();
    let user_message_id = Uuid::new_v4();
    for (index, id, role, content, status) in [
        (
            0,
            user_message_id,
            "user",
            "The project code is RAVEN-42.",
            "complete",
        ),
        (
            1,
            Uuid::new_v4(),
            "assistant",
            "I will remember it.",
            "complete",
        ),
        (
            2,
            Uuid::new_v4(),
            "assistant",
            "unfinished output",
            "pending",
        ),
    ] {
        sqlx::query(
            "INSERT INTO chat_messages(
                 id,org_id,session_id,role,content,status,message_index)
             VALUES ($1,$2,$3,$4,$5,$6,$7)",
        )
        .bind(id)
        .bind(org.id)
        .bind(session_id)
        .bind(role)
        .bind(content)
        .bind(status)
        .bind(index)
        .execute(&app.db)
        .await
        .unwrap();
    }
    for attachment_index in 0..17 {
        let file_name = format!("brief-{attachment_index:02}.pdf");
        sqlx::query(
            "INSERT INTO chat_attachments(
                 org_id,session_id,message_id,created_by,file_name,size_bytes,
                 sha256,storage_key,provider_mount_path,content_bytes)
             VALUES ($1,$2,$3,$4,$5,3,$6,$7,$8,$9)",
        )
        .bind(org.id)
        .bind(session_id)
        .bind(user_message_id)
        .bind(owner.id)
        .bind(&file_name)
        .bind("a".repeat(64))
        .bind(format!(
            "chat-attachments/{session_id}/{user_message_id}/{file_name}"
        ))
        .bind(format!(
            "/mnt/g5/chat-attachments/{session_id}/{user_message_id}/{file_name}"
        ))
        .bind(b"pdf".to_vec())
        .execute(&app.db)
        .await
        .unwrap();
    }
    queue_session_input(
        &app,
        org.id,
        session_id,
        task_id,
        "What is the project code?",
    )
    .await;

    let mut db = app.scoped_db_for_org(org.id).await;
    let dispatch = app
        .state
        .task_service
        .reserve_agent_dispatch(&mut db)
        .await
        .unwrap()
        .expect("new turn is reserved");
    db.commit().await.unwrap();
    let input = &dispatch.invoke_params["input"];
    assert_eq!(dispatch.invoke_params["version"], "1.0.5");
    // No limit was requested: the default bounds only setup and delivery.
    assert_eq!(input["limits"]["setup_timeout_ms"], 300_000);
    assert!(input["limits"].get("wall_clock_ms").is_none());
    let prompt = input["prompt"]["text"].as_str().unwrap();
    assert!(prompt.contains("RAVEN-42"));
    assert!(prompt.contains("brief-00.pdf"));
    assert!(!prompt.contains("brief-16.pdf"));
    assert!(prompt.contains("\"omitted_attachment_references\":1"));
    assert!(!prompt.contains("unfinished output"));
    assert!(prompt.ends_with("Current input follows:\nWhat is the project code?"));
    assert_eq!(
        input["session_binding"]["provider_session_id"],
        old_provider_id
    );
    assert_eq!(input["resume_prompt"]["text"], "What is the project code?");
    assert_eq!(
        input["session_binding"]["expected_manifest_generation"],
        Value::Null
    );
    assert_eq!(input["recovery"]["included_messages"], 2);
    assert_eq!(input["recovery"]["attachment_references"], 16);
    assert_eq!(input["recovery"]["omitted_attachment_references"], 1);
    let frozen: Value = sqlx::query_scalar(
        "SELECT policy_snapshot_ref -> 'frozen_recovery_prompt'
         FROM task_attempts WHERE task_id = $1 AND attempt = 1",
    )
    .bind(task_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(frozen["text"], prompt);
    assert_eq!(frozen["evidence_cutoff"], 1);
    app.cleanup().await;
}

/// Dispatch always requires the current contract; rollout compatibility is operator-managed.
#[tokio::test]
async fn agent_dispatch_never_falls_back_to_a_lifetime_limited_contract() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    for catalog in [
        "2026.09.29-agent-turn-recovery",
        "2026.09.30-agent-turn-background",
        "2026.10.03-agent-turn-native-resume",
    ] {
        let workspace = TestWorkspace::create(&app, &org).await;
        attach_running_vm_without_runtime(&app, &workspace).await;
        sqlx::query("UPDATE workspace_vms SET module_catalog_version = $2 WHERE workspace_id = $1")
            .bind(workspace.id)
            .bind(catalog)
            .execute(&app.db)
            .await
            .unwrap();
        let (task_id, session_id) =
            create_agent_task(&app, &workspace, owner.id, "queued", None, None).await;
        let prior_session_id = Uuid::new_v4().to_string();
        sqlx::query("UPDATE chat_sessions SET provider_session_id = $2 WHERE id = $1")
            .bind(session_id)
            .bind(&prior_session_id)
            .execute(&app.db)
            .await
            .unwrap();
        queue_session_input(&app, org.id, session_id, task_id, "Hello").await;
        let mut db = app.scoped_db_for_org(org.id).await;
        let dispatch = app
            .state
            .task_service
            .reserve_agent_dispatch(&mut db)
            .await
            .unwrap()
            .expect("turn is reserved");
        db.commit().await.unwrap();
        assert_eq!(dispatch.task_id, task_id, "{catalog}");
        assert_eq!(dispatch.invoke_params["version"], "1.0.5", "{catalog}");
        let input = &dispatch.invoke_params["input"];
        assert!(input.get("resume_prompt").is_some(), "{catalog}: {input}");
        assert_eq!(
            input["session_binding"]["provider_session_id"],
            serde_json::json!(prior_session_id),
            "{catalog}: {input}"
        );
        let limits = &dispatch.invoke_params["input"]["limits"];
        assert_eq!(limits["setup_timeout_ms"], 300_000, "{catalog}");
        assert!(
            limits.get("wall_clock_scope").is_none(),
            "{catalog}: {limits}"
        );
    }
    app.cleanup().await;
}

#[tokio::test]
async fn undelivered_auth_notice_does_not_block_the_next_first_turn() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    attach_running_vm_without_runtime(&app, &workspace).await;
    let (task_id, session_id) =
        create_agent_task(&app, &workspace, owner.id, "queued", None, None).await;
    for (index, role, content) in [
        (0, "user", "Undelivered login-gated input"),
        (
            1,
            "assistant",
            "Codex authorization expired before prompt delivery",
        ),
    ] {
        sqlx::query(
            "INSERT INTO chat_messages(org_id,session_id,role,content,status,message_index)
             VALUES ($1,$2,$3,$4,'error',$5)",
        )
        .bind(org.id)
        .bind(session_id)
        .bind(role)
        .bind(content)
        .bind(index)
        .execute(&app.db)
        .await
        .unwrap();
    }
    sqlx::query("UPDATE chat_sessions SET message_count = 2 WHERE id = $1")
        .bind(session_id)
        .execute(&app.db)
        .await
        .unwrap();
    queue_session_input(&app, org.id, session_id, task_id, "The new current input").await;
    let mut db = app.scoped_db_for_org(org.id).await;
    let dispatch = app
        .state
        .task_service
        .reserve_agent_dispatch(&mut db)
        .await
        .unwrap()
        .expect("new input remains dispatchable");
    db.commit().await.unwrap();
    let input = &dispatch.invoke_params["input"];
    assert_eq!(input["prompt"]["text"], "The new current input");
    assert!(input.get("recovery").is_none());
    app.cleanup().await;
}

#[tokio::test]
async fn continuation_without_a_delivered_objective_freezes_visible_failure() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    attach_running_vm_without_runtime(&app, &workspace).await;
    let (task_id, session_id) = create_agent_task(
        &app,
        &workspace,
        owner.id,
        "queued",
        Some("interrupted"),
        None,
    )
    .await;
    sqlx::query(
        "INSERT INTO task_attempts(
             org_id,task_id,attempt,vm_id,epoch,start_confirmed,started_at,ended_at,reason)
         VALUES ($1,$2,1,$3,1,true,now() - interval '1 minute',now(),'interrupted')",
    )
    .bind(org.id)
    .bind(task_id)
    .bind(Uuid::new_v4().to_string())
    .execute(&app.db)
    .await
    .unwrap();
    let mut db = app.scoped_db_for_org(org.id).await;
    let dispatch = app
        .state
        .task_service
        .reserve_agent_dispatch(&mut db)
        .await
        .unwrap()
        .expect("unavailable context dispatches a visible failure");
    db.commit().await.unwrap();
    let input = &dispatch.invoke_params["input"];
    assert_eq!(dispatch.attempt, 2);
    assert_eq!(input["task"]["session_id"], session_id.to_string());
    assert!(input["recovery"]["unavailable_reason"].as_str().is_some());
    assert_eq!(input["session_binding"]["provider_session_id"], Value::Null);
    let frozen: Value = sqlx::query_scalar(
        "SELECT policy_snapshot_ref -> 'frozen_recovery_prompt'
         FROM task_attempts WHERE task_id = $1 AND attempt = 2",
    )
    .bind(task_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(frozen["recovery"], input["recovery"]);
    app.cleanup().await;
}

#[tokio::test]
async fn chat_fork_reconstructs_copied_fact_without_a_native_identity() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    attach_running_vm_without_runtime(&app, &workspace).await;
    let (_, source_session_id) = create_agent_task(
        &app,
        &workspace,
        owner.id,
        "waiting",
        None,
        Some("owner_stop"),
    )
    .await;
    sqlx::query(
        "UPDATE chat_sessions SET message_count = 2, provider_session_id = $2 WHERE id = $1",
    )
    .bind(source_session_id)
    .bind(Uuid::new_v4().to_string())
    .execute(&app.db)
    .await
    .unwrap();
    let source_user_id = Uuid::new_v4();
    let source_assistant_id = Uuid::new_v4();
    for (index, id, role, content) in [
        (0, source_user_id, "user", "The secret code is LANTERN-9."),
        (
            1,
            source_assistant_id,
            "assistant",
            "I have recorded LANTERN-9.",
        ),
    ] {
        sqlx::query(
            "INSERT INTO chat_messages(id,org_id,session_id,role,content,status,message_index)
             VALUES ($1,$2,$3,$4,$5,'complete',$6)",
        )
        .bind(id)
        .bind(org.id)
        .bind(source_session_id)
        .bind(role)
        .bind(content)
        .bind(index)
        .execute(&app.db)
        .await
        .unwrap();
    }
    sqlx::query(
        "INSERT INTO chat_attachments(
             org_id,session_id,message_id,created_by,file_name,size_bytes,
             sha256,storage_key,provider_mount_path,content_bytes)
         VALUES ($1,$2,$3,$4,'reference.txt',4,$5,$6,$7,$8)",
    )
    .bind(org.id)
    .bind(source_session_id)
    .bind(source_user_id)
    .bind(owner.id)
    .bind("b".repeat(64))
    .bind(format!(
        "chat-attachments/{source_session_id}/{source_user_id}/reference.txt"
    ))
    .bind(format!(
        "/mnt/g5/chat-attachments/{source_session_id}/{source_user_id}/reference.txt"
    ))
    .bind(b"note".to_vec())
    .execute(&app.db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO chat_attachments(
             org_id,session_id,message_id,file_name,size_bytes,
             sha256,storage_key,provider_mount_path,content_bytes)
         VALUES ($1,$2,$3,'screenshot.png',3,$4,$5,$6,$7)",
    )
    .bind(org.id)
    .bind(source_session_id)
    .bind(source_assistant_id)
    .bind("c".repeat(64))
    .bind(format!(
        "agent-screenshots/{source_session_id}/{source_assistant_id}/screenshot.png"
    ))
    .bind(format!(
        "/mnt/g5/agent-screenshots/{source_session_id}/{source_assistant_id}/screenshot.png"
    ))
    .bind(b"png".to_vec())
    .execute(&app.db)
    .await
    .unwrap();
    let response = app
        .authenticated_client(&owner)
        .post_json(
            &format!("/api/v1/sessions/{source_session_id}/fork"),
            &json!({"fromMessageIndex":1}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let body: Value = response.json().await.unwrap();
    let fork_id: Uuid = body["data"]["sessionId"].as_str().unwrap().parse().unwrap();
    assert_eq!(body["data"]["messagesCopied"], 2);
    let fork_native_id: Option<String> =
        sqlx::query_scalar("SELECT provider_session_id FROM chat_sessions WHERE id = $1")
            .bind(fork_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(fork_native_id.is_none());
    let (screenshot_key, screenshot_mount): (String, String) = sqlx::query_as(
        "SELECT storage_key,provider_mount_path FROM chat_attachments
         WHERE session_id = $1 AND file_name = 'screenshot.png'",
    )
    .bind(fork_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(screenshot_key.starts_with("agent-screenshots/"));
    assert!(screenshot_mount.starts_with("/mnt/g5/agent-screenshots/"));

    let (task_id, _) = create_agent_task(&app, &workspace, owner.id, "queued", None, None).await;
    sqlx::query("UPDATE tasks SET chat_session_id = $2 WHERE id = $1")
        .bind(task_id)
        .bind(fork_id)
        .execute(&app.db)
        .await
        .unwrap();
    queue_session_input(&app, org.id, fork_id, task_id, "What is the secret code?").await;
    let mut db = app.scoped_db_for_org(org.id).await;
    let dispatch = app
        .state
        .task_service
        .reserve_agent_dispatch(&mut db)
        .await
        .unwrap()
        .expect("forked turn dispatches");
    db.commit().await.unwrap();
    assert_eq!(dispatch.task_id, task_id);
    let input = &dispatch.invoke_params["input"];
    assert!(input["prompt"]["text"]
        .as_str()
        .unwrap()
        .contains("LANTERN-9"));
    assert!(input["prompt"]["text"]
        .as_str()
        .unwrap()
        .contains("reference.txt"));
    assert_eq!(input["session_binding"]["provider_session_id"], Value::Null);
    assert_eq!(input["recovery"]["included_messages"], 2);
    assert_eq!(input["recovery"]["attachment_references"], 2);
    app.cleanup().await;
}

#[tokio::test]
async fn interrupted_task_continues_original_objective_without_resending_its_input() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    attach_running_vm_without_runtime(&app, &workspace).await;
    let (task_id, session_id) = create_agent_task(
        &app,
        &workspace,
        owner.id,
        "queued",
        Some("interrupted"),
        None,
    )
    .await;
    sqlx::query(
        "INSERT INTO task_attempts(
             org_id,task_id,attempt,vm_id,epoch,start_confirmed,
             session_files_state,session_manifest_generation,started_at,ended_at,reason)
         VALUES ($1,$2,1,$3,1,true,'persist_failed',7,
                 now() - interval '1 minute',now(),'interrupted')",
    )
    .bind(org.id)
    .bind(task_id)
    .bind(Uuid::new_v4().to_string())
    .execute(&app.db)
    .await
    .unwrap();
    let original_input_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO session_inputs(
             id,org_id,session_id,input_index,content,input_origin,delivery,
             reservation_id,reservation_revision,delivery_generation,task_id,
             attempt,delivered_at,delivery_audit)
         VALUES ($1,$2,$3,0,'Create exactly one issue named RAVEN-42.',
                 'human','delivered',$4,1,1,$5,1,now(),
                 '[{\"event\":\"input.accepted\"}]'::jsonb)",
    )
    .bind(original_input_id)
    .bind(org.id)
    .bind(session_id)
    .bind(Uuid::new_v4())
    .bind(task_id)
    .execute(&app.db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO chat_messages(
             id,org_id,session_id,role,content,status,message_index,
             input_id,task_id,attempt)
         VALUES ($1,$2,$3,'user','Create exactly one issue named RAVEN-42.',
                 'complete',0,$1,$4,1)",
    )
    .bind(original_input_id)
    .bind(org.id)
    .bind(session_id)
    .bind(task_id)
    .execute(&app.db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO chat_messages(
             org_id,session_id,role,content,status,message_index,task_id,attempt)
         VALUES ($1,$2,'assistant','Created issue #123.','stopped',1,$3,1)",
    )
    .bind(org.id)
    .bind(session_id)
    .bind(task_id)
    .execute(&app.db)
    .await
    .unwrap();
    let old_provider_id = Uuid::new_v4().to_string();
    sqlx::query(
        "UPDATE chat_sessions
         SET message_count = 2, provider_session_id = $2,
             session_manifest_generation = 7 WHERE id = $1",
    )
    .bind(session_id)
    .bind(&old_provider_id)
    .execute(&app.db)
    .await
    .unwrap();

    let mut db = app.scoped_db_for_org(org.id).await;
    let dispatch = app
        .state
        .task_service
        .reserve_agent_dispatch(&mut db)
        .await
        .unwrap()
        .expect("continuation dispatches");
    db.commit().await.unwrap();
    assert_eq!(dispatch.task_id, task_id);
    assert_eq!(dispatch.attempt, 2);
    let input = &dispatch.invoke_params["input"];
    let prompt = input["prompt"]["text"].as_str().unwrap();
    assert!(prompt.contains("Create exactly one issue named RAVEN-42."));
    assert!(prompt.contains("Created issue #123."));
    assert!(prompt.contains("do not replay previously delivered input."));
    assert!(prompt.ends_with(
        "Background tasks and sub-agents from the earlier attempt are no longer running."
    ));
    assert_ne!(
        input["task"]["initial_input"]["input_id"],
        original_input_id.to_string()
    );
    assert_eq!(input["task"]["input_origin"], "continuation");
    assert_eq!(
        input["session_binding"]["provider_session_id"],
        old_provider_id
    );
    assert!(!input["resume_prompt"]["text"]
        .as_str()
        .unwrap()
        .contains("Prior delivered conversation"));
    assert_eq!(
        input["session_binding"]["expected_manifest_generation"],
        Value::Null
    );
    let repeated_original_inputs: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM session_inputs
         WHERE session_id = $1 AND content = 'Create exactly one issue named RAVEN-42.'",
    )
    .bind(session_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(repeated_original_inputs, 1);
    app.cleanup().await;
}

pub(super) async fn insert_queued_preview(
    app: &TestApp,
    workspace: &TestWorkspace,
    owner_id: Uuid,
) -> Uuid {
    let task_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO tasks(
             id, org_id, workspace_id, name, kind, status, spec, attempt, created_by
         ) VALUES ($1,$2,$3,'Preview','preview','queued',$4,1,$5)",
    )
    .bind(task_id)
    .bind(workspace.org_id)
    .bind(workspace.id)
    .bind(json!({
        "command": {
            "schema_version": "g5.preview-task-spec/v1",
            "module_id": "workspace.preview",
            "module_version": "1.0.0",
            "operation": "start",
            "input_schema": "g5.workspace.preview.start/v1",
            "input": {"profile":"default"}
        }
    }))
    .bind(owner_id)
    .execute(&app.db)
    .await
    .expect("insert queued preview");
    task_id
}

async fn apply_agent_event(
    app: &TestApp,
    org_id: Uuid,
    attempt: (Uuid, Uuid),
    seq: u64,
    kind: &str,
    payload: Value,
    terminal: bool,
) {
    let (task_id, vm_id) = attempt;
    apply_agent_attempt_event(
        app, org_id, task_id, 1, vm_id, 1, seq, kind, payload, terminal,
    )
    .await;
}

#[allow(clippy::too_many_arguments)]
async fn apply_agent_attempt_event(
    app: &TestApp,
    org_id: Uuid,
    task_id: Uuid,
    attempt: i32,
    vm_id: Uuid,
    epoch: u64,
    seq: u64,
    kind: &str,
    payload: Value,
    terminal: bool,
) {
    let mut db = app.scoped_db_for_org(org_id).await;
    app.state
        .task_service
        .apply(
            &mut db,
            TaskLifecycleEvent::Journal(JournalTaskEvent {
                task_id,
                attempt,
                vm_id,
                epoch,
                exec_id: canonical_exec_id(task_id, attempt, vm_id),
                seq,
                kind: kind.to_string(),
                payload,
                terminal,
            }),
        )
        .await
        .expect("apply agent journal event");
    db.commit().await.expect("commit agent journal event");
}

/// Chat Stop holds a job's turn with the owner-stop hold
/// (`request_workspace_task_pause`); Continue releases the same task.
#[tokio::test]
async fn owner_stop_holds_queued_task_and_continue_releases_same_task() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let client = app.authenticated_client(&owner);
    let (task_id, session_id) =
        create_agent_task(&app, &workspace, owner.id, "queued", None, None).await;

    let mut db = app.scoped_db_for_org(org.id).await;
    let hold = app
        .state
        .task_service
        .request_workspace_task_pause(&mut db, owner.id, task_id)
        .await
        .expect("hold the queued task");
    db.commit().await.expect("commit the hold");
    assert_eq!(hold.task_id, task_id);
    assert_eq!(hold.status, "waiting");

    let held = sqlx::query_as::<_, (String, Option<String>, Option<String>, Uuid)>(
        "SELECT status, continuation_reason, hold_reason, chat_session_id
         FROM tasks WHERE id = $1",
    )
    .bind(task_id)
    .fetch_one(&app.db)
    .await
    .expect("load held task");
    assert_eq!(held.0, "waiting");
    assert_eq!(
        held.1, None,
        "an unstarted task has no prior attempt reason"
    );
    assert_eq!(held.2.as_deref(), Some("owner_stop"));
    assert_eq!(held.3, session_id);

    let resume = client
        .post_json(&format!("/api/v1/tasks/{task_id}/continue"), &json!({}))
        .await;
    assert_eq!(resume.status(), StatusCode::OK);
    let resume_body: Value = resume.json().await.expect("continue response json");
    assert_eq!(resume_body["data"]["taskId"], task_id.to_string());
    assert_eq!(resume_body["data"]["status"], "queued");

    let resumed = sqlx::query_as::<_, (String, Option<String>, Option<String>, Uuid)>(
        "SELECT status, continuation_reason, hold_reason, chat_session_id
         FROM tasks WHERE id = $1",
    )
    .bind(task_id)
    .fetch_one(&app.db)
    .await
    .expect("load resumed task");
    assert_eq!(resumed.0, "queued");
    assert_eq!(resumed.1, None);
    assert_eq!(resumed.2, None);
    assert_eq!(resumed.3, session_id, "Continue preserves logical identity");

    app.cleanup().await;
}

/// A preview has no owner-stop hold: the service rejects it and leaves the
/// queued preview untouched.
#[tokio::test]
async fn owner_stop_rejects_a_queued_preview_without_stranding_it() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let task_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO tasks(
             id, org_id, workspace_id, name, kind, status, spec, attempt, created_by
         ) VALUES ($1,$2,$3,'Preview','preview','queued','{}'::jsonb,1,$4)",
    )
    .bind(task_id)
    .bind(org.id)
    .bind(workspace.id)
    .bind(owner.id)
    .execute(&app.db)
    .await
    .expect("insert queued preview");

    let mut db = app.scoped_db_for_org(org.id).await;
    let rejected = app
        .state
        .task_service
        .request_workspace_task_pause(&mut db, owner.id, task_id)
        .await;
    db.rollback().await.expect("release the workspace lock");
    let error = rejected.expect_err("a queued preview takes no owner-stop hold");
    assert_eq!(
        error.to_string(),
        "Conflict: only agent tasks can be paused"
    );
    let state: (String, Option<String>) =
        sqlx::query_as("SELECT status, hold_reason FROM tasks WHERE id = $1")
            .bind(task_id)
            .fetch_one(&app.db)
            .await
            .expect("load preview after rejected pause");
    assert_eq!(state, ("queued".to_string(), None));

    app.cleanup().await;
}

#[tokio::test]
async fn vm_loss_honors_an_explicit_owner_cancel() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let vm_id = attach_running_vm_without_runtime(&app, &workspace).await;
    let (task_id, _) = create_agent_task(&app, &workspace, owner.id, "queued", None, None).await;
    sqlx::query(
        "UPDATE tasks
         SET status = 'running', vm_id = $2, epoch = 1,
             start_confirmed = true, started_at = now(),
             spec = jsonb_set(spec, '{lifecycle}',
                 '{\"cancel_requested\":true}'::jsonb, true)
         WHERE id = $1",
    )
    .bind(task_id)
    .bind(vm_id.to_string())
    .execute(&app.db)
    .await
    .expect("mark agent attempt cancelling");
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
    .expect("insert cancelling attempt");

    let mut db = app.scoped_db_for_org(org.id).await;
    app.state
        .task_service
        .apply(
            &mut db,
            TaskLifecycleEvent::VmLost {
                task_id,
                attempt: 1,
                vm_id,
                epoch: 1,
                evidence: json!({"reason":"test_vm_loss"}),
            },
        )
        .await
        .expect("apply VM loss after owner cancel");
    db.commit().await.expect("commit VM loss");

    let settled: (String, Option<String>) = sqlx::query_as(
        "SELECT task.status, attempt.reason
         FROM tasks AS task
         JOIN task_attempts AS attempt
           ON attempt.task_id = task.id AND attempt.attempt = task.attempt
         WHERE task.id = $1",
    )
    .bind(task_id)
    .fetch_one(&app.db)
    .await
    .expect("load cancelled VM-loss attempt");
    assert_eq!(
        settled,
        ("cancelled".to_string(), Some("cancelled".to_string()))
    );

    app.cleanup().await;
}

#[tokio::test]
async fn typed_and_templated_sessions_are_admitted_concurrently_to_one_workspace_vm() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = helpers::TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let (first_task, first_session) =
        create_agent_task(&app, &workspace, owner.id, "queued", None, None).await;
    let (second_task, second_session) =
        create_agent_task(&app, &workspace, owner.id, "queued", None, None).await;
    queue_session_input_with_origin(
        &app,
        org.id,
        first_session,
        first_task,
        "first concurrent turn",
        "human",
        None,
        None,
    )
    .await;
    queue_session_input_with_origin(
        &app,
        org.id,
        second_session,
        second_task,
        "second concurrent turn",
        "workflow",
        Some("workflow:test-template"),
        Some("revision-1"),
    )
    .await;

    let mut reserved = Vec::new();
    let mut input_origins = Vec::new();
    for _ in 0..2 {
        let mut db = app.scoped_db_for_org(org.id).await;
        let dispatch = app
            .state
            .task_service
            .reserve_agent_dispatch(&mut db)
            .await
            .expect("reserve agent dispatch")
            .expect("both agent sessions must fit the workspace execution slots");
        assert_eq!(dispatch.vm_id, runtime.vm_id());
        reserved.push(dispatch.task_id);
        input_origins.push(
            dispatch.invoke_params["input"]["task"]["input_origin"]
                .as_str()
                .expect("agent input origin")
                .to_string(),
        );
        db.commit().await.expect("commit agent reservation");

        let disposition = app
            .state
            .task_service
            .invoke_agent_dispatch(&dispatch)
            .await
            .expect("invoke fake agent turn");
        let mut confirmation = app.scoped_db_for_org(org.id).await;
        app.state
            .task_service
            .apply(&mut confirmation, dispatch.start_disposition(disposition))
            .await
            .expect("confirm fake agent start");
        confirmation
            .commit()
            .await
            .expect("commit fake agent start confirmation");
    }

    reserved.sort_unstable();
    let mut expected = vec![first_task, second_task];
    expected.sort_unstable();
    assert_eq!(reserved, expected);
    input_origins.sort();
    assert_eq!(input_origins, vec!["human", "workflow"]);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM tasks
             WHERE workspace_id = $1 AND kind = 'agent'
               AND status = 'running' AND start_confirmed",
        )
        .bind(workspace.id)
        .fetch_one(&app.db)
        .await
        .expect("count concurrent agent turns"),
        2
    );
    assert_eq!(
        runtime
            .received_rpc_methods()
            .await
            .iter()
            .filter(|method| method.as_str() == "modules.invoke")
            .count(),
        2
    );

    app.cleanup().await;
}

/// `g5.agents.admission.default-slots`: a workspace whose owner never chose a
/// slot count runs sixteen chat turns at once. Every reserved turn is invoked
/// and confirmed, so it holds its slot the way a live turn does, and the
/// seventeenth waits queued for a free slot. Under the earlier default of two,
/// the third reservation already came back empty.
#[tokio::test]
async fn a_default_workspace_runs_sixteen_chat_turns_and_queues_the_next() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = helpers::TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let slots: i32 = sqlx::query_scalar(
        "SELECT bounded_execution_slots FROM workspace_settings WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .expect("load the workspace's slot count");
    assert_eq!(slots, 16, "a new settings row takes the default slot count");

    let mut queued = Vec::new();
    for turn in 0..17 {
        let (task_id, session_id) =
            create_agent_task(&app, &workspace, owner.id, "queued", None, None).await;
        queue_session_input(
            &app,
            org.id,
            session_id,
            task_id,
            &format!("concurrent chat turn {turn}"),
        )
        .await;
        queued.push(task_id);
    }

    let mut reserved = Vec::new();
    for _ in 0..16 {
        let mut db = app.scoped_db_for_org(org.id).await;
        let dispatch = app
            .state
            .task_service
            .reserve_agent_dispatch(&mut db)
            .await
            .expect("reserve agent dispatch")
            .expect("sixteen chat turns fit the default workspace slots");
        assert_eq!(dispatch.vm_id, runtime.vm_id());
        reserved.push(dispatch.task_id);
        db.commit().await.expect("commit agent reservation");

        let disposition = app
            .state
            .task_service
            .invoke_agent_dispatch(&dispatch)
            .await
            .expect("invoke fake agent turn");
        let mut confirmation = app.scoped_db_for_org(org.id).await;
        app.state
            .task_service
            .apply(&mut confirmation, dispatch.start_disposition(disposition))
            .await
            .expect("confirm fake agent start");
        confirmation
            .commit()
            .await
            .expect("commit fake agent start confirmation");
    }

    let mut db = app.scoped_db_for_org(org.id).await;
    let next = app
        .state
        .task_service
        .reserve_agent_dispatch(&mut db)
        .await
        .expect("reserve agent dispatch with every slot held");
    assert!(
        next.is_none(),
        "the seventeenth chat turn waits for a free slot"
    );
    db.commit()
        .await
        .expect("commit the empty reservation pass");

    reserved.sort_unstable();
    reserved.dedup();
    assert_eq!(reserved.len(), 16, "sixteen distinct turns were reserved");
    let waiting: Vec<Uuid> = queued
        .iter()
        .copied()
        .filter(|task_id| reserved.binary_search(task_id).is_err())
        .collect();
    assert_eq!(waiting.len(), 1, "exactly one turn is left over");
    let waiting_state: (String, String) = sqlx::query_as(
        "SELECT task.status, input.delivery
         FROM tasks AS task
         JOIN session_inputs AS input ON input.task_id = task.id
         WHERE task.id = $1",
    )
    .bind(waiting[0])
    .fetch_one(&app.db)
    .await
    .expect("load the waiting turn");
    assert_eq!(
        waiting_state,
        ("queued".to_string(), "queued".to_string()),
        "the left-over turn and its message stay queued"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM tasks
             WHERE workspace_id = $1 AND kind = 'agent'
               AND status = 'running' AND start_confirmed",
        )
        .bind(workspace.id)
        .fetch_one(&app.db)
        .await
        .expect("count concurrent agent turns"),
        16
    );
    assert_eq!(
        runtime
            .received_rpc_methods()
            .await
            .iter()
            .filter(|method| method.as_str() == "modules.invoke")
            .count(),
        16
    );

    app.cleanup().await;
}

/// `g5.agents.admission.vm-slots`: a VM started from a catalog shipped before
/// sixteen-turn support has four bounded slots in all, so until it is replaced
/// a default workspace admits only two turns there, agent or command, and
/// Changes keeps two slots. The same queued work is admitted once the VM
/// records a later catalog.
#[tokio::test]
async fn a_four_slot_catalog_vm_admits_at_most_two_turns() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = helpers::TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let record_vm_catalog = |version: &'static str| {
        sqlx::query("UPDATE workspace_vms SET module_catalog_version = $2 WHERE workspace_id = $1")
            .bind(workspace.id)
            .bind(version)
            .execute(&app.db)
    };
    record_vm_catalog("2026.10.06-workspace-changes-rebase")
        .await
        .expect("record the VM's four-slot catalog");

    let mut agent_tasks = Vec::new();
    for turn in 0..3 {
        let (task_id, session_id) =
            create_agent_task(&app, &workspace, owner.id, "queued", None, None).await;
        queue_session_input(
            &app,
            org.id,
            session_id,
            task_id,
            &format!("turn on a four-slot VM {turn}"),
        )
        .await;
        agent_tasks.push(task_id);
    }
    let command_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO tasks(
             id, org_id, workspace_id, name, kind, status, spec, attempt, created_by
         ) VALUES ($1,$2,$3,'Four-slot VM bootstrap','command','queued',$4,1,$5)",
    )
    .bind(command_id)
    .bind(workspace.org_id)
    .bind(workspace.id)
    .bind(json!({
        "command": {
            "schema_version": "g5.command-task-spec/v1",
            "module_id": "workspace.bootstrap",
            "module_version": "1.0.0",
            "operation": "run",
            "input_schema": "g5.workspace.bootstrap.run/v1",
            "input": {"profile":"uplift"}
        }
    }))
    .bind(owner.id)
    .execute(&app.db)
    .await
    .expect("insert queued bootstrap command");

    for _ in 0..2 {
        let mut db = app.scoped_db_for_org(org.id).await;
        let dispatch = app
            .state
            .task_service
            .reserve_agent_dispatch(&mut db)
            .await
            .expect("reserve agent dispatch")
            .expect("two turns fit the previous ceiling");
        assert_eq!(dispatch.vm_id, runtime.vm_id());
        db.commit().await.expect("commit agent reservation");
        let disposition = app
            .state
            .task_service
            .invoke_agent_dispatch(&dispatch)
            .await
            .expect("invoke fake agent turn");
        let mut confirmation = app.scoped_db_for_org(org.id).await;
        app.state
            .task_service
            .apply(&mut confirmation, dispatch.start_disposition(disposition))
            .await
            .expect("confirm fake agent start");
        confirmation
            .commit()
            .await
            .expect("commit fake agent start confirmation");
    }

    let mut db = app.scoped_db_for_org(org.id).await;
    let third = app
        .state
        .task_service
        .reserve_agent_dispatch(&mut db)
        .await
        .expect("reserve a third turn on the four-slot VM");
    assert!(
        third.is_none(),
        "a third agent turn waits on a four-slot VM"
    );
    let command = app
        .state
        .task_service
        .reserve_command_dispatch(&mut db)
        .await
        .expect("reserve a command on the four-slot VM");
    assert!(command.is_none(), "a command waits on a four-slot VM too");
    db.rollback()
        .await
        .expect("roll back the empty reservations");

    record_vm_catalog("2026.10.11-agent-execution")
        .await
        .expect("record a sixteen-turn catalog");
    let mut db = app.scoped_db_for_org(org.id).await;
    let third = app
        .state
        .task_service
        .reserve_agent_dispatch(&mut db)
        .await
        .expect("reserve the third turn")
        .expect("a sixteen-turn catalog admits the third turn");
    assert!(agent_tasks.contains(&third.task_id));
    db.commit().await.expect("commit the third reservation");
    let mut db = app.scoped_db_for_org(org.id).await;
    let command = app
        .state
        .task_service
        .reserve_command_dispatch(&mut db)
        .await
        .expect("reserve the command");
    assert!(
        command.is_none(),
        "generic commands have no task admission path"
    );
    db.commit().await.expect("commit the command reservation");

    app.cleanup().await;
}

#[tokio::test]
async fn native_question_and_permission_settle_only_from_journaled_transport_evidence() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = helpers::TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let (task_id, _) = create_agent_task(&app, &workspace, owner.id, "queued", None, None).await;
    sqlx::query(
        "UPDATE tasks
         SET status = 'running', vm_id = $2, epoch = 1,
             start_confirmed = true, started_at = now()
         WHERE id = $1",
    )
    .bind(task_id)
    .bind(runtime.vm_id().to_string())
    .execute(&app.db)
    .await
    .expect("bind native-request task to runtime");
    sqlx::query(
        "INSERT INTO task_attempts(
             org_id, task_id, attempt, vm_id, epoch, start_confirmed, started_at
         ) VALUES ($1,$2,1,$3,1,true,now())",
    )
    .bind(org.id)
    .bind(task_id)
    .bind(runtime.vm_id().to_string())
    .execute(&app.db)
    .await
    .expect("insert native-request attempt");

    let client = app.authenticated_client(&owner);
    let cases = [
        (
            1,
            "native-question",
            "question",
            json!({"questions":[{"question":"Choose","options":[{"label":"A"},{"label":"B"}]}]}),
            json!({"g5AnswersByIndex":{"0":{"selectedOptionIndexes":[1]}}}),
            "answered",
        ),
        (
            3,
            "native-permission",
            "permission",
            json!({"tool_name":"Bash","command":"touch allowed"}),
            json!({"behavior":"allow"}),
            "allowed",
        ),
    ];

    for (opened_seq, provider_request_id, request_kind, display, answer, disposition) in cases {
        apply_agent_attempt_event(
            &app,
            org.id,
            task_id,
            1,
            runtime.vm_id(),
            1,
            opened_seq,
            "provider_request.opened",
            json!({
                "provider_request_id": provider_request_id,
                "request_kind": request_kind,
                "tool_name": (request_kind == "permission").then_some("Bash"),
                "payload": display,
                "display": display,
                "deadline_unix_ms": chrono::Utc::now().timestamp_millis() + 60_000,
            }),
            false,
        )
        .await;

        let request_id = format!("reply-{provider_request_id}");
        let response = client
            .post_json(
                &format!("/api/v1/tasks/{task_id}/provider-requests/{provider_request_id}/reply"),
                &json!({
                    "attempt": 1,
                    "requestId": request_id,
                    "validatedResponse": answer,
                }),
            )
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        let body: Value = response.json().await.expect("native reply response json");
        assert_eq!(body["data"]["deliveryDisposition"], "delivered");
        let response_digest = body["data"]["responseDigest"]
            .as_str()
            .expect("native response digest")
            .to_string();
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT status FROM provider_requests
                 WHERE task_id = $1 AND attempt = 1 AND provider_request_id = $2",
            )
            .bind(task_id)
            .bind(provider_request_id)
            .fetch_one(&app.db)
            .await
            .expect("load transport-accepted native request"),
            "reply_pending",
            "RPC acknowledgement alone must not settle the durable request",
        );

        apply_agent_attempt_event(
            &app,
            org.id,
            task_id,
            1,
            runtime.vm_id(),
            1,
            opened_seq + 1,
            "provider_request.replied",
            json!({
                "provider_request_id": provider_request_id,
                "request_id": request_id,
                "response_digest": response_digest,
                "disposition": disposition,
                "evidence": "transport_write",
            }),
            false,
        )
        .await;
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT status FROM provider_requests
                 WHERE task_id = $1 AND attempt = 1 AND provider_request_id = $2",
            )
            .bind(task_id)
            .bind(provider_request_id)
            .fetch_one(&app.db)
            .await
            .expect("load journal-settled native request"),
            "responded",
        );
    }

    apply_agent_attempt_event(
        &app,
        org.id,
        task_id,
        1,
        runtime.vm_id(),
        1,
        4,
        "provider_request.replied",
        json!({
            "provider_request_id": "native-permission",
            "request_id": "reply-native-permission",
            "response_digest": sqlx::query_scalar::<_, String>(
                "SELECT response ->> 'digest' FROM provider_requests
                 WHERE task_id = $1 AND provider_request_id = 'native-permission'",
            )
            .bind(task_id)
            .fetch_one(&app.db)
            .await
            .expect("load permission response digest"),
            "disposition": "allowed",
            "evidence": "transport_write",
        }),
        false,
    )
    .await;
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM provider_requests
             WHERE task_id = $1 AND status = 'responded'",
        )
        .bind(task_id)
        .fetch_one(&app.db)
        .await
        .expect("count durable native replies"),
        2,
        "journal replay must not create a second durable result",
    );
    assert_eq!(
        runtime
            .received_rpc_methods()
            .await
            .iter()
            .filter(|method| method.as_str() == "modules.input.respond")
            .count(),
        2,
    );

    app.cleanup().await;
}

#[tokio::test]
async fn vm_loss_continuation_preserves_identity_and_blocks_dependents_until_final_success() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let first_vm = attach_running_vm_without_runtime(&app, &workspace).await;
    let (prerequisite, prerequisite_session) =
        create_agent_task(&app, &workspace, owner.id, "queued", None, None).await;
    let (dependent, dependent_session) =
        create_agent_task(&app, &workspace, owner.id, "queued", None, None).await;
    sqlx::query(
        "INSERT INTO task_dependencies(org_id, workspace_id, task_id, depends_on_task_id)
         VALUES ($1,$2,$3,$4)",
    )
    .bind(org.id)
    .bind(workspace.id)
    .bind(dependent)
    .bind(prerequisite)
    .execute(&app.db)
    .await
    .expect("insert agent dependency edge");
    queue_session_input(
        &app,
        org.id,
        prerequisite_session,
        prerequisite,
        "complete the prerequisite",
    )
    .await;
    queue_session_input_with_origin(
        &app,
        org.id,
        dependent_session,
        dependent,
        "run after the prerequisite",
        "workflow",
        Some("workflow:dependent"),
        Some("revision-1"),
    )
    .await;

    let mut first_reservation = app.scoped_db_for_org(org.id).await;
    let first = app
        .state
        .task_service
        .reserve_agent_dispatch(&mut first_reservation)
        .await
        .expect("reserve prerequisite attempt")
        .expect("dependency-ready prerequisite");
    assert_eq!(first.task_id, prerequisite);
    assert_eq!(first.vm_id, first_vm);
    assert_eq!(
        first.invoke_params["input"]["provider"]["auth_mode"], "anthropic_api_key",
        "direct provider dispatch must carry the admitted authentication mode"
    );
    first_reservation
        .commit()
        .await
        .expect("commit prerequisite attempt");
    let initial = &first.invoke_params["input"]["task"]["initial_input"];
    apply_agent_attempt_event(
        &app,
        org.id,
        prerequisite,
        1,
        first_vm,
        1,
        1,
        "input.accepted",
        json!({
            "reservation_id": initial["reservation_id"],
            "input_id": initial["input_id"],
            "revision": initial["revision"],
            "delivery_generation": initial["delivery_generation"],
            "system_notice_ids": initial["system_notice_ids"],
        }),
        false,
    )
    .await;
    let mut loss = app.scoped_db_for_org(org.id).await;
    app.state
        .task_service
        .apply(
            &mut loss,
            TaskLifecycleEvent::VmLost {
                task_id: prerequisite,
                attempt: 1,
                vm_id: first_vm,
                epoch: 1,
                evidence: json!({"reason":"fixture_vm_loss"}),
            },
        )
        .await
        .expect("settle first attempt as VM-lost");
    loss.commit().await.expect("commit VM-loss settlement");
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT status FROM tasks WHERE id = $1")
            .bind(prerequisite)
            .fetch_one(&app.db)
            .await
            .expect("load held prerequisite"),
        "waiting",
    );
    let mut blocked = app.scoped_db_for_org(org.id).await;
    assert!(
        app.state
            .task_service
            .reserve_agent_dispatch(&mut blocked)
            .await
            .expect("check dependent while prerequisite waits")
            .is_none(),
        "an ended attempt must not release the logical dependency",
    );
    blocked.commit().await.expect("commit blocked scan");

    let second_vm = Uuid::new_v4();
    sqlx::query(
        "UPDATE workspace_vms
         SET vm_id = $2, epoch = 2, status = 'running', lease_expires_at = now() + interval '1 hour'
         WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .bind(second_vm.to_string())
    .execute(&app.db)
    .await
    .expect("rotate workspace incarnation");
    let continued = app
        .authenticated_client(&owner)
        .post_json(
            &format!("/api/v1/tasks/{prerequisite}/continue"),
            &json!({}),
        )
        .await;
    assert_eq!(continued.status(), StatusCode::OK);
    let mut second_reservation = app.scoped_db_for_org(org.id).await;
    let second = app
        .state
        .task_service
        .reserve_agent_dispatch(&mut second_reservation)
        .await
        .expect("reserve continued attempt")
        .expect("continued prerequisite must become dispatchable");
    assert_eq!(second.task_id, prerequisite);
    assert_eq!(second.attempt, 2);
    assert_eq!(second.vm_id, second_vm);
    assert_eq!(
        second.invoke_params["input"]["task"]["input_origin"],
        "continuation"
    );
    second_reservation
        .commit()
        .await
        .expect("commit continued attempt");
    let mut confirmed = app.scoped_db_for_org(org.id).await;
    app.state
        .task_service
        .apply(
            &mut confirmed,
            second.start_disposition(json!({"disposition":"started"})),
        )
        .await
        .expect("confirm continued attempt");
    confirmed.commit().await.expect("commit continued start");
    let continued_input = &second.invoke_params["input"]["task"]["initial_input"];
    apply_agent_attempt_event(
        &app,
        org.id,
        prerequisite,
        2,
        second_vm,
        2,
        1,
        "input.accepted",
        json!({
            "reservation_id": continued_input["reservation_id"],
            "input_id": continued_input["input_id"],
            "revision": continued_input["revision"],
            "delivery_generation": continued_input["delivery_generation"],
            "system_notice_ids": continued_input["system_notice_ids"],
        }),
        false,
    )
    .await;
    apply_agent_attempt_event(
        &app,
        org.id,
        prerequisite,
        2,
        second_vm,
        2,
        2,
        "invocation.outcome",
        json!({"status":"succeeded","result":{"status":"succeeded","code":"exit_code"}}),
        true,
    )
    .await;

    let identity: (Uuid, Uuid, i32, String) = sqlx::query_as(
        "SELECT task.id, task.chat_session_id, task.attempt, task.status
         FROM tasks task WHERE task.id = $1",
    )
    .bind(prerequisite)
    .fetch_one(&app.db)
    .await
    .expect("load continued logical identity");
    assert_eq!(
        identity,
        (prerequisite, prerequisite_session, 2, "succeeded".into())
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM task_dependencies
             WHERE task_id = $1 AND depends_on_task_id = $2",
        )
        .bind(dependent)
        .bind(prerequisite)
        .fetch_one(&app.db)
        .await
        .expect("load preserved dependency edge"),
        1,
    );
    let mut released = app.scoped_db_for_org(org.id).await;
    let downstream = app
        .state
        .task_service
        .reserve_agent_dispatch(&mut released)
        .await
        .expect("reserve released dependent")
        .expect("final logical success must release the dependent");
    assert_eq!(downstream.task_id, dependent);
    assert_eq!(downstream.vm_id, second_vm);
    released
        .commit()
        .await
        .expect("commit dependent reservation");

    app.cleanup().await;
}

#[tokio::test]
async fn dispatch_clears_stale_workspace_interrupt_metadata_before_a_new_attempt() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    attach_running_vm_without_runtime(&app, &workspace).await;
    let (task_id, session_id) =
        create_agent_task(&app, &workspace, owner.id, "queued", None, None).await;
    queue_session_input(
        &app,
        org.id,
        session_id,
        task_id,
        "Run after the workspace drain",
    )
    .await;
    sqlx::query(
        "UPDATE tasks
         SET spec = jsonb_set(
             spec, '{lifecycle}',
             jsonb_build_object(
                 'interrupt_operation_id', $2::text,
                 'interrupt_reason', 'drain',
                 'interrupt_requested_at', now()),
             true)
         WHERE id = $1",
    )
    .bind(task_id)
    .bind(Uuid::new_v4())
    .execute(&app.db)
    .await
    .expect("seed stale interruption metadata");

    let mut db = app.scoped_db_for_org(org.id).await;
    let dispatch = app
        .state
        .task_service
        .reserve_agent_dispatch(&mut db)
        .await
        .expect("reserve resumed task")
        .expect("resumed task must dispatch");
    assert_eq!(dispatch.task_id, task_id);
    db.commit().await.expect("commit resumed task reservation");

    let lifecycle: Option<Value> =
        sqlx::query_scalar("SELECT spec -> 'lifecycle' FROM tasks WHERE id = $1")
            .bind(task_id)
            .fetch_one(&app.db)
            .await
            .expect("load task lifecycle metadata");
    let lifecycle = lifecycle.unwrap_or_else(|| json!({}));
    assert!(lifecycle.get("interrupt_operation_id").is_none());
    assert!(lifecycle.get("interrupt_reason").is_none());
    assert!(lifecycle.get("interrupt_requested_at").is_none());

    app.cleanup().await;
}

#[tokio::test]
async fn cancelled_continuation_start_preserves_the_retry_reason() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    attach_running_vm_without_runtime(&app, &workspace).await;
    let (task_id, _session_id) = create_agent_task(
        &app,
        &workspace,
        owner.id,
        "queued",
        Some("interrupted"),
        None,
    )
    .await;
    let prior_vm_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO task_attempts(
             org_id, task_id, attempt, vm_id, epoch, start_confirmed,
             started_at, ended_at, reason)
         VALUES ($1,$2,1,$3,1,true,now() - interval '1 minute',now(),'interrupted')",
    )
    .bind(org.id)
    .bind(task_id)
    .bind(prior_vm_id.to_string())
    .execute(&app.db)
    .await
    .expect("insert prior continuation attempt");

    let mut db = app.scoped_db_for_org(org.id).await;
    let dispatch = app
        .state
        .task_service
        .reserve_agent_dispatch(&mut db)
        .await
        .expect("reserve continuation")
        .expect("continuation must dispatch");
    assert_eq!(dispatch.task_id, task_id);
    assert_eq!(dispatch.attempt, 2);
    db.commit().await.expect("commit continuation reservation");

    let mut db = app.scoped_db_for_org(org.id).await;
    let result = app
        .state
        .task_service
        .apply(
            &mut db,
            dispatch.start_disposition(json!({"disposition":"cancelled"})),
        )
        .await
        .expect("settle cancelled continuation start");
    assert!(!result.terminal);
    assert_eq!(result.task_status, "queued");
    db.commit()
        .await
        .expect("commit cancelled continuation start");

    let task: (String, Option<String>) =
        sqlx::query_as("SELECT status, continuation_reason FROM tasks WHERE id = $1")
            .bind(task_id)
            .fetch_one(&app.db)
            .await
            .expect("load retriable continuation task");
    assert_eq!(task.0, "queued");
    assert_eq!(task.1.as_deref(), Some("interrupted"));
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT delivery FROM session_inputs
             WHERE task_id = $1 AND attempt = 2 AND input_origin = 'continuation'",
        )
        .bind(task_id)
        .fetch_one(&app.db)
        .await
        .expect("load withdrawn continuation input"),
        "withdrawn"
    );

    let mut retry_db = app.scoped_db_for_org(org.id).await;
    let retry = app
        .state
        .task_service
        .reserve_agent_dispatch(&mut retry_db)
        .await
        .expect("reserve retried continuation")
        .expect("continuation reason must make the task dispatchable again");
    assert_eq!(retry.task_id, task_id);
    assert_eq!(retry.attempt, 3);
    retry_db
        .commit()
        .await
        .expect("commit retried continuation reservation");

    app.cleanup().await;
}

#[tokio::test]
async fn withdrawing_human_input_keeps_a_pending_continuation_dispatchable() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let (task_id, session_id) = create_agent_task(
        &app,
        &workspace,
        owner.id,
        "queued",
        Some("interrupted"),
        None,
    )
    .await;
    let input_id = queue_session_input(
        &app,
        org.id,
        session_id,
        task_id,
        "A later instruction that may be withdrawn",
    )
    .await;

    let mut db = app.scoped_db_for_org(org.id).await;
    app.state
        .task_service
        .withdraw_queued_session_input(&mut db, owner.id, session_id, input_id, 1)
        .await
        .expect("withdraw queued human input");
    db.commit().await.expect("commit input withdrawal");

    let task: (String, Option<String>) =
        sqlx::query_as("SELECT status, continuation_reason FROM tasks WHERE id = $1")
            .bind(task_id)
            .fetch_one(&app.db)
            .await
            .expect("load continuation after withdrawal");
    assert_eq!(task.0, "queued");
    assert_eq!(task.1.as_deref(), Some("interrupted"));

    app.cleanup().await;
}

#[tokio::test]
async fn tool_selection_admission_rejects_saved_policies_before_reserving_attempts() {
    use g5_contracts::types::{AgentLaunchFieldPreferenceV1, ChatAgentProvider};

    let app = TestApp::spawn().await;
    for mode in ["default", "bypassPermissions"] {
        for (tools, error) in [
            (vec!["mcp__docs__search"], Some("UNSUPPORTED_ALLOWED_TOOL")),
            (vec!["Read"], Some("QUESTION_TOOL_EXCLUDED")),
            (vec!["Read", "AskUserQuestion"], None),
            (vec![], None),
        ] {
            let accepted = error.is_none();
            let mut owner = TestUser::create(&app).await;
            let org = TestOrg::create_with_admin(&app, &mut owner).await;
            let workspace = TestWorkspace::create(&app, &org).await;
            attach_running_vm_without_runtime(&app, &workspace).await;
            let (task_id, session_id) =
                create_agent_task(&app, &workspace, owner.id, "queued", None, None).await;
            let preferences = AgentLaunchPreferencesV1 {
                provider: AgentLaunchFieldPreferenceV1::Explicit(ChatAgentProvider::ClaudeCode),
                permission_mode: AgentLaunchFieldPreferenceV1::Explicit(mode.into()),
                ..Default::default()
            };
            sqlx::query(
            "UPDATE tasks SET spec=jsonb_set(jsonb_set(spec,'{agent,agentLaunch}',$2),'{agent,allowedTools}',$3) WHERE id=$1",
        )
        .bind(task_id)
        .bind(serde_json::to_value(preferences).unwrap())
        .bind(json!(tools))
        .execute(&app.db).await.unwrap();
            let original: Value = sqlx::query_scalar("SELECT spec FROM tasks WHERE id=$1")
                .bind(task_id)
                .fetch_one(&app.db)
                .await
                .unwrap();
            queue_session_input(&app, org.id, session_id, task_id, "Read the repository").await;

            let mut db = app.scoped_db_for_org(org.id).await;
            let dispatch = app
                .state
                .task_service
                .reserve_agent_dispatch(&mut db)
                .await
                .unwrap();
            db.commit().await.unwrap();
            assert_eq!(dispatch.is_some(), accepted, "{tools:?}");
            let attempts: i64 =
                sqlx::query_scalar("SELECT count(*) FROM task_attempts WHERE task_id=$1")
                    .bind(task_id)
                    .fetch_one(&app.db)
                    .await
                    .unwrap();
            assert_eq!(attempts, if accepted { 1 } else { 0 });
            if let Some(dispatch) = dispatch {
                let policy = &dispatch.invoke_params["input"]["provider"]["policy_snapshot"];
                assert_eq!(policy["allowed_tools"], json!(tools));
                assert_eq!(
                    policy["tool_approval"],
                    if mode == "default" {
                        "prompt"
                    } else {
                        "auto_allow"
                    }
                );
                assert_eq!(policy["questions"], "enabled");
            } else {
                let status: String = sqlx::query_scalar("SELECT status FROM tasks WHERE id=$1")
                    .bind(task_id)
                    .fetch_one(&app.db)
                    .await
                    .unwrap();
                assert_eq!(status, "failed");
                let retained: Value = sqlx::query_scalar("SELECT spec FROM tasks WHERE id=$1")
                    .bind(task_id)
                    .fetch_one(&app.db)
                    .await
                    .unwrap();
                assert_eq!(
                    retained, original,
                    "admission must not rewrite saved restrictions"
                );
                let outcome: Value = sqlx::query_scalar("SELECT outcome FROM tasks WHERE id=$1")
                    .bind(task_id)
                    .fetch_one(&app.db)
                    .await
                    .unwrap();
                assert!(outcome["detail"].as_str().unwrap().contains(error.unwrap()));
                let mut retry_db = app.scoped_db_for_org(org.id).await;
                assert!(app
                    .state
                    .task_service
                    .reserve_agent_dispatch(&mut retry_db)
                    .await
                    .unwrap()
                    .is_none());
                retry_db.commit().await.unwrap();
                let tasks: i64 =
                    sqlx::query_scalar("SELECT count(*) FROM tasks WHERE chat_session_id=$1")
                        .bind(session_id)
                        .fetch_one(&app.db)
                        .await
                        .unwrap();
                assert_eq!(
                    tasks, 1,
                    "unsupported policy must not create an automatic retry loop"
                );
                let retained_input: (Uuid, String) = sqlx::query_as(
                    "SELECT task_id, delivery FROM session_inputs WHERE session_id=$1",
                )
                .bind(session_id)
                .fetch_one(&app.db)
                .await
                .unwrap();
                assert_eq!(retained_input, (task_id, "queued".to_owned()));

                // A refusal has no attempt or provider journal. Its saved
                // content must still identify itself as the error explanation
                // on both page shapes, rather than becoming assistant prose.
                let client = app.authenticated_client(&owner);
                for suffix in ["", "?tail=true&limit=20"] {
                    let response = client
                        .get(&format!("/api/v1/sessions/{session_id}/messages{suffix}"))
                        .await;
                    assert_eq!(response.status(), StatusCode::OK);
                    let body: Value = response.json().await.unwrap();
                    let messages = body["data"]["messages"].as_array().unwrap();
                    assert_eq!(messages.len(), 1, "{body}");
                    assert_eq!(messages[0]["role"], "assistant");
                    assert_eq!(messages[0]["status"], "error");
                    assert_eq!(messages[0]["isErrorNotice"], true);
                    assert!(messages[0]["content"]
                        .as_str()
                        .unwrap()
                        .contains(error.unwrap()));
                    assert!(messages[0].get("codexAuthFailure").is_none());
                }

                if mode == "default" && tools == ["Read"] {
                    let response = client
                        .post_json(
                            &format!("/api/v1/sessions/{session_id}/fork"),
                            &json!({ "fromMessageIndex": 0 }),
                        )
                        .await;
                    let status = response.status();
                    let body: Value = response.json().await.unwrap();
                    assert_eq!(status, StatusCode::CREATED, "{body}");
                    let fork = body["data"]["sessionId"].as_str().unwrap();
                    let response = client
                        .get(&format!("/api/v1/sessions/{fork}/messages"))
                        .await;
                    let body: Value = response.json().await.unwrap();
                    assert_eq!(body["data"]["messages"][0]["isErrorNotice"], true);

                    let target = TestWorkspace::create(&app, &org).await;
                    let mut connection = app.db.acquire().await.unwrap();
                    g5_api::services::chat_service::ChatService::copy_sessions_to_workspace(
                        &mut connection,
                        workspace.id,
                        target.id,
                        &[session_id],
                        owner.id,
                    )
                    .await
                    .unwrap();
                    let marker: bool = sqlx::query_scalar(
                        "SELECT m.is_error_notice FROM chat_messages m
                         JOIN chat_sessions s ON s.id=m.session_id
                         WHERE s.workspace_id=$1 AND s.forked_from_id=$2",
                    )
                    .bind(target.id)
                    .bind(session_id)
                    .fetch_one(&app.db)
                    .await
                    .unwrap();
                    assert!(
                        marker,
                        "workspace carry preserves diagnostic content semantics"
                    );
                }
            }
        }
    }
    app.cleanup().await;
}

#[tokio::test]
async fn redispatch_preserves_the_frozen_bypass_question_snapshot() {
    use g5_contracts::types::{AgentLaunchFieldPreferenceV1, ChatAgentProvider};
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    attach_running_vm_without_runtime(&app, &workspace).await;
    let (task_id, session_id) =
        create_agent_task(&app, &workspace, owner.id, "queued", None, None).await;
    let preferences = AgentLaunchPreferencesV1 {
        provider: AgentLaunchFieldPreferenceV1::Explicit(ChatAgentProvider::ClaudeCode),
        permission_mode: AgentLaunchFieldPreferenceV1::Explicit("bypassPermissions".into()),
        ..Default::default()
    };
    sqlx::query("UPDATE tasks SET spec=jsonb_set(spec,'{agent,agentLaunch}',$2) WHERE id=$1")
        .bind(task_id)
        .bind(serde_json::to_value(preferences).unwrap())
        .execute(&app.db)
        .await
        .unwrap();
    queue_session_input(&app, org.id, session_id, task_id, "Ask a clarification").await;
    let mut db = app.scoped_db_for_org(org.id).await;
    let dispatch = app
        .state
        .task_service
        .reserve_agent_dispatch(&mut db)
        .await
        .unwrap()
        .unwrap();
    db.commit().await.unwrap();
    assert_eq!(
        dispatch.invoke_params["input"]["provider"]["policy_snapshot"]["questions"],
        "enabled"
    );

    let frozen: Value = sqlx::query_scalar(
        "SELECT policy_snapshot_ref FROM task_attempts WHERE task_id=$1 AND attempt=1",
    )
    .bind(task_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    // The database also refuses rewriting frozen permission evidence.
    let rejected = sqlx::query(
        "UPDATE task_attempts SET policy_snapshot_ref=jsonb_set(policy_snapshot_ref,'{permission_policy,questions}','\"disabled\"')
         WHERE task_id=$1 AND attempt=1",
    ).bind(task_id).execute(&app.db).await.unwrap_err();
    assert!(rejected
        .to_string()
        .contains("execution identity is immutable"));
    sqlx::query("UPDATE tasks SET updated_at=now()-interval '3 seconds' WHERE id=$1")
        .bind(task_id)
        .execute(&app.db)
        .await
        .unwrap();
    let mut db = app.scoped_db_for_org(org.id).await;
    let retry = app
        .state
        .task_service
        .reserve_agent_dispatch(&mut db)
        .await
        .unwrap()
        .unwrap();
    db.commit().await.unwrap();
    assert_eq!(
        retry.invoke_params["input"]["provider"]["policy_snapshot"],
        frozen["permission_policy"]
    );
    let retained: Value = sqlx::query_scalar(
        "SELECT policy_snapshot_ref FROM task_attempts WHERE task_id=$1 AND attempt=1",
    )
    .bind(task_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(retained, frozen);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM task_attempts WHERE task_id=$1")
        .bind(task_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(count, 1);
    app.cleanup().await;
}

#[tokio::test]
async fn dispatch_admission_failure_rebinds_queued_input_to_a_followup_task() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    attach_running_vm_without_runtime(&app, &workspace).await;
    let (task_id, session_id) =
        create_agent_task(&app, &workspace, owner.id, "queued", None, None).await;
    let input_id = queue_session_input(
        &app,
        org.id,
        session_id,
        task_id,
        "Keep this input after admission fails",
    )
    .await;
    sqlx::query("UPDATE tasks SET spec = '{}'::jsonb WHERE id = $1")
        .bind(task_id)
        .execute(&app.db)
        .await
        .expect("make agent dispatch admission fail");

    // Enqueue takes this lock before it requests a task row. Dispatch already
    // selected the task by the time it knows the session, so it must yield
    // instead of waiting in the inverse order and deadlocking with enqueue.
    let mut session_lock = app.db.begin().await.expect("begin session lock holder");
    let (lock_class, lock_key) = chat_session_turn_lock_key(session_id);
    sqlx::query("SELECT pg_advisory_xact_lock($1, $2)")
        .bind(lock_class)
        .bind(lock_key)
        .execute(&mut *session_lock)
        .await
        .expect("hold session turn allocation lock");
    let mut skipped_db = app.scoped_db_for_org(org.id).await;
    let skipped = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        app.state
            .task_service
            .reserve_agent_dispatch(&mut skipped_db),
    )
    .await
    .expect("dispatch must not wait while enqueue owns the session lock")
    .expect("skip contended dispatch reservation");
    assert!(skipped.is_none());
    skipped_db
        .commit()
        .await
        .expect("commit skipped dispatch reservation");
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT status FROM tasks WHERE id = $1")
            .bind(task_id)
            .fetch_one(&app.db)
            .await
            .expect("load task after contended dispatch"),
        "queued"
    );
    session_lock
        .commit()
        .await
        .expect("release session turn allocation lock");

    let mut db = app.scoped_db_for_org(org.id).await;
    assert!(app
        .state
        .task_service
        .reserve_agent_dispatch(&mut db)
        .await
        .expect("settle failed dispatch admission")
        .is_none());
    db.commit()
        .await
        .expect("commit failed dispatch admission settlement");

    let original_status: String = sqlx::query_scalar("SELECT status FROM tasks WHERE id = $1")
        .bind(task_id)
        .fetch_one(&app.db)
        .await
        .expect("load failed original task");
    assert_eq!(original_status, "failed");
    let rebound: (Uuid, String, String) = sqlx::query_as(
        "SELECT input.task_id, input.delivery, task.status
         FROM session_inputs AS input
         JOIN tasks AS task ON task.id = input.task_id
         WHERE input.id = $1",
    )
    .bind(input_id)
    .fetch_one(&app.db)
    .await
    .expect("load rebound queued input");
    assert_ne!(rebound.0, task_id);
    assert_eq!(rebound.1, "queued");
    assert_eq!(rebound.2, "queued");

    app.cleanup().await;
}

#[tokio::test]
async fn cancelling_an_enqueued_agent_task_withdraws_its_queued_input() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let session_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO chat_sessions(
             id, org_id, workspace_id, scope_type, title, created_by
         ) VALUES ($1,$2,$3,'workspace','Queued cancellation',$4)",
    )
    .bind(session_id)
    .bind(org.id)
    .bind(workspace.id)
    .bind(owner.id)
    .execute(&app.db)
    .await
    .expect("insert queued cancellation session");

    let mut db = app.scoped_db_for_org(org.id).await;
    let queued = app
        .state
        .task_service
        .enqueue_agent_input(
            &mut db,
            EnqueueAgentInput {
                task_name: None,
                session_id,
                actor_user_id: owner.id,
                content: "Do not run this prompt".into(),
                artifacts: json!([]),
                input_origin: "human".into(),
                template_ref: None,
                template_revision: None,
                confirmed_launch: None,
                agent_launch: AgentLaunchPreferencesV1::default(),
            },
        )
        .await
        .expect("enqueue agent input");
    let cancelled = app
        .state
        .task_service
        .request_workspace_task_cancel(&mut db, owner.id, queued.task_id)
        .await
        .expect("cancel queued task");
    assert_eq!(cancelled.status, "cancelled");
    db.commit().await.expect("commit queued cancellation");

    let input: (Option<Uuid>, String) =
        sqlx::query_as("SELECT task_id, delivery FROM session_inputs WHERE id = $1")
            .bind(queued.input_id)
            .fetch_one(&app.db)
            .await
            .expect("load cancelled queued input");
    assert_eq!(input, (Some(queued.task_id), "withdrawn".to_string()));

    app.cleanup().await;
}

#[tokio::test]
async fn queued_platform_notice_is_frozen_into_input_and_delivered_by_acceptance() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = helpers::TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let (task_id, session_id) =
        create_agent_task(&app, &workspace, owner.id, "queued", None, None).await;
    let input_id =
        queue_session_input(&app, org.id, session_id, task_id, "Continue the review").await;
    let notice_id = Uuid::new_v4();
    let operation_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO chat_messages(
             id, org_id, session_id, role, content, artifacts, citations,
             status, message_index)
         VALUES ($1,$2,$3,'system','Workspace shutdown is scheduled.',
                 jsonb_build_object('platform_notice',jsonb_build_object(
                     'schema_version',1,'operation_id',$4::text,
                     'stop_reason','drain','delivery','queued')),
                 '[]'::jsonb,'complete',0)",
    )
    .bind(notice_id)
    .bind(org.id)
    .bind(session_id)
    .bind(operation_id)
    .execute(&app.db)
    .await
    .expect("insert queued platform notice");

    let mut db = app.scoped_db_for_org(org.id).await;
    let dispatch = app
        .state
        .task_service
        .reserve_agent_dispatch(&mut db)
        .await
        .expect("reserve agent dispatch")
        .expect("agent task must be dispatchable");
    let initial = &dispatch.invoke_params["input"]["task"]["initial_input"];
    assert_eq!(initial["system_notice_ids"], json!([notice_id]));
    assert!(dispatch.invoke_params["input"]["prompt"]["text"]
        .as_str()
        .is_some_and(|prompt| prompt.contains("Workspace shutdown is scheduled.")
            && prompt.contains("Continue the review")));
    db.commit().await.expect("commit notice reservation");

    let reservation: (Uuid, i32, i32) = sqlx::query_as(
        "SELECT reservation_id, revision, delivery_generation
         FROM session_inputs WHERE id = $1",
    )
    .bind(input_id)
    .fetch_one(&app.db)
    .await
    .expect("load frozen input reservation");
    apply_agent_event(
        &app,
        org.id,
        (task_id, runtime.vm_id()),
        1,
        "input.accepted",
        json!({
            "input_id": input_id,
            "reservation_id": reservation.0,
            "revision": reservation.1,
            "delivery_generation": reservation.2,
            "system_notice_ids": [notice_id],
        }),
        false,
    )
    .await;
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT artifacts #>> '{platform_notice,delivery}'
             FROM chat_messages WHERE id = $1",
        )
        .bind(notice_id)
        .fetch_one(&app.db)
        .await
        .expect("load delivered platform notice"),
        "delivered"
    );

    app.cleanup().await;
}

#[tokio::test]
async fn agent_limit_settlement_preserves_delivery_evidence_and_attempt_reason() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let vm_id = attach_running_vm_without_runtime(&app, &workspace).await;

    for case in [
        "unknown",
        "unknown_cancelled",
        "prewrite",
        "limited",
        "accepted_dispatch_timeout",
    ] {
        let unknown = matches!(case, "unknown" | "unknown_cancelled");
        let (task_id, session_id) =
            create_agent_task(&app, &workspace, owner.id, "queued", None, None).await;
        let input_id =
            queue_session_input(&app, org.id, session_id, task_id, "bounded prompt").await;
        let reservation_id = Uuid::new_v4();
        sqlx::query(
            "UPDATE tasks
             SET status = 'running', vm_id = $2, epoch = 1,
                 start_confirmed = true, started_at = now(), updated_at = now()
             WHERE id = $1",
        )
        .bind(task_id)
        .bind(vm_id.to_string())
        .execute(&app.db)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO task_attempts(
                 org_id, task_id, attempt, vm_id, epoch, start_confirmed
             ) VALUES ($1,$2,1,$3,1,true)",
        )
        .bind(org.id)
        .bind(task_id)
        .bind(vm_id.to_string())
        .execute(&app.db)
        .await
        .unwrap();
        sqlx::query(
            "UPDATE session_inputs
             SET delivery = 'reserved', reservation_id = $2,
                 reservation_revision = revision, delivery_generation = 1,
                 task_id = $3, attempt = 1
             WHERE id = $1",
        )
        .bind(input_id)
        .bind(reservation_id)
        .bind(task_id)
        .execute(&app.db)
        .await
        .unwrap();
        if case == "unknown_cancelled" {
            sqlx::query(
                "UPDATE tasks SET spec = jsonb_set(spec, '{lifecycle}',
                     '{\"cancel_requested\":true}'::jsonb, true) WHERE id = $1",
            )
            .bind(task_id)
            .execute(&app.db)
            .await
            .unwrap();
        }

        if matches!(case, "limited" | "accepted_dispatch_timeout") {
            apply_agent_event(
                &app,
                org.id,
                (task_id, vm_id),
                1,
                "input.accepted",
                json!({
                    "input_id":input_id,"reservation_id":reservation_id,
                    "revision":1,"delivery_generation":1,"system_notice_ids":[]
                }),
                false,
            )
            .await;
        }
        let inner = if case == "limited" {
            json!({"status":"limit_exceeded","code":"max_output_bytes"})
        } else {
            json!({
                "status":"timed_out","code":"wall_clock_exceeded",
                "input_delivery_unknown":unknown
            })
        };
        let outcome = if case == "accepted_dispatch_timeout" {
            json!({"status":"timed_out","detail":{"input_delivery_unknown":true}})
        } else {
            json!({"status":"succeeded","result":inner})
        };
        apply_agent_event(
            &app,
            org.id,
            (task_id, vm_id),
            if matches!(case, "limited" | "accepted_dispatch_timeout") {
                2
            } else {
                1
            },
            "invocation.outcome",
            outcome,
            true,
        )
        .await;
        let task: (String, Option<String>) =
            sqlx::query_as("SELECT status, hold_reason FROM tasks WHERE id = $1")
                .bind(task_id)
                .fetch_one(&app.db)
                .await
                .unwrap();
        let delivery: String =
            sqlx::query_scalar("SELECT delivery FROM session_inputs WHERE id = $1")
                .bind(input_id)
                .fetch_one(&app.db)
                .await
                .unwrap();
        let reason: String = sqlx::query_scalar(
            "SELECT reason FROM task_attempts WHERE task_id = $1 AND attempt = 1",
        )
        .bind(task_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
        assert_eq!(
            reason,
            if case == "limited" {
                "limit_exceeded"
            } else if case == "unknown_cancelled" {
                "cancelled"
            } else {
                "timed_out"
            }
        );
        if case == "unknown" {
            assert_eq!(task, ("waiting".into(), Some("delivery_unknown".into())));
            assert_eq!(delivery, "delivery_unknown");
        } else if case == "unknown_cancelled" {
            assert_eq!(task, ("cancelled".into(), None));
            assert_eq!(delivery, "withdrawn");
            let audit: serde_json::Value =
                sqlx::query_scalar("SELECT delivery_audit FROM session_inputs WHERE id = $1")
                    .bind(input_id)
                    .fetch_one(&app.db)
                    .await
                    .unwrap();
            assert!(audit.to_string().contains("delivery_unknown"), "{audit}");
        } else if matches!(case, "limited" | "accepted_dispatch_timeout") {
            assert_eq!(task, ("failed".into(), None));
            assert_eq!(delivery, "delivered");
        } else {
            assert_eq!(task, ("failed".into(), None));
            assert_eq!(delivery, "withdrawn");
        }
    }
    app.cleanup().await;
}

#[tokio::test]
async fn accepted_agent_input_materializes_one_durable_assistant_message_until_final_outcome() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let vm_id = attach_running_vm_without_runtime(&app, &workspace).await;
    let (task_id, session_id) =
        create_agent_task(&app, &workspace, owner.id, "queued", None, None).await;
    let input_id = queue_session_input(&app, org.id, session_id, task_id, "What changed?").await;
    let reservation_id = Uuid::new_v4();
    sqlx::query(
        "UPDATE tasks
         SET status = 'running', vm_id = $2, epoch = 1,
             start_confirmed = true, started_at = now(), updated_at = now()
         WHERE id = $1",
    )
    .bind(task_id)
    .bind(vm_id.to_string())
    .execute(&app.db)
    .await
    .expect("bind running agent task");
    sqlx::query(
        "INSERT INTO task_attempts(
             org_id, task_id, attempt, vm_id, epoch, start_confirmed
         ) VALUES ($1,$2,1,$3,1,true)",
    )
    .bind(org.id)
    .bind(task_id)
    .bind(vm_id.to_string())
    .execute(&app.db)
    .await
    .expect("insert running agent attempt");
    sqlx::query(
        "UPDATE session_inputs
         SET delivery = 'reserved', reservation_id = $2,
             reservation_revision = revision, delivery_generation = 1,
             task_id = $3, attempt = 1
         WHERE id = $1",
    )
    .bind(input_id)
    .bind(reservation_id)
    .bind(task_id)
    .execute(&app.db)
    .await
    .expect("reserve input for running attempt");

    apply_agent_event(
        &app,
        org.id,
        (task_id, vm_id),
        1,
        "input.accepted",
        json!({
            "input_id": input_id,
            "reservation_id": reservation_id,
            "revision": 1,
            "delivery_generation": 1,
            "system_notice_ids": [],
        }),
        false,
    )
    .await;

    let assistant_message_id = canonical_agent_message_id(task_id, 1);
    let placeholder: (String, String, i32, i32) = sqlx::query_as(
        "SELECT role, status, message_index,
                (SELECT message_count FROM chat_sessions WHERE id = $2)
         FROM chat_messages WHERE id = $1",
    )
    .bind(assistant_message_id)
    .bind(session_id)
    .fetch_one(&app.db)
    .await
    .expect("load durable assistant placeholder");
    assert_eq!(placeholder, ("assistant".into(), "pending".into(), 1, 2));

    apply_agent_event(
        &app,
        org.id,
        (task_id, vm_id),
        2,
        "message.delta",
        json!({"block_id":"claude-message-1:0","text_delta":"The workspace is ready"}),
        false,
    )
    .await;
    let streaming: (String, String) =
        sqlx::query_as("SELECT content, status FROM chat_messages WHERE id = $1")
            .bind(assistant_message_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(streaming, ("".into(), "streaming".into()));
    apply_agent_event(
        &app,
        org.id,
        (task_id, vm_id),
        3,
        "message.final",
        json!({"block_id":"claude-message-1:0","content":"The workspace is ready."}),
        false,
    )
    .await;
    apply_agent_event(
        &app,
        org.id,
        (task_id, vm_id),
        4,
        "tool.started",
        json!({"call_id":"tool-1","name":"Bash","input_ref":"sha256:tool"}),
        false,
    )
    .await;
    apply_agent_event(
        &app,
        org.id,
        (task_id, vm_id),
        5,
        "message.final",
        json!({"block_id":"claude-message-2:0","content":"Final answer."}),
        false,
    )
    .await;
    apply_agent_event(
        &app,
        org.id,
        (task_id, vm_id),
        5,
        "message.final",
        json!({"block_id":"claude-message-2:0","content":"Final answer."}),
        false,
    )
    .await;
    apply_agent_event(
        &app,
        org.id,
        (task_id, vm_id),
        6,
        "invocation.outcome",
        json!({
            "status":"succeeded",
            "result":{
                "status":"succeeded",
                "final_message_ref":assistant_message_id,
                "usage":{"input_tokens":4,"output_tokens":5}
            }
        }),
        true,
    )
    .await;

    let materialized: (String, String, i64, i32) = sqlx::query_as(
        "SELECT content, status,
                (SELECT count(*) FROM chat_messages WHERE session_id = $2),
                (SELECT message_count FROM chat_sessions WHERE id = $2)
         FROM chat_messages WHERE id = $1",
    )
    .bind(assistant_message_id)
    .bind(session_id)
    .fetch_one(&app.db)
    .await
    .expect("load finalized assistant message");
    assert_eq!(
        materialized,
        (
            "The workspace is ready.\n\nFinal answer.".into(),
            "complete".into(),
            2,
            2
        )
    );

    let timeline: Vec<String> = sqlx::query_scalar(
        "SELECT metadata #>> '{payload,content}' FROM task_events
         WHERE task_id = $1 AND metadata ->> 'kind' = 'message.final'
         ORDER BY seq",
    )
    .bind(task_id)
    .fetch_all(&app.db)
    .await
    .unwrap();
    assert_eq!(timeline, ["The workspace is ready.", "Final answer."]);

    let replayed: (String, i64) = sqlx::query_as(
        "SELECT content,
                (SELECT count(*) FROM task_events WHERE task_id = $2 AND metadata ->> 'kind' = 'message.final')
         FROM chat_messages WHERE id = $1",
    )
    .bind(assistant_message_id)
    .bind(task_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(
        replayed,
        ("The workspace is ready.\n\nFinal answer.".into(), 2)
    );

    app.cleanup().await;
}

#[tokio::test]
async fn assistant_projection_settles_result_only_repetition_and_interrupted_partial_output() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let vm_id = attach_running_vm_without_runtime(&app, &workspace).await;
    for (scenario, event_kind, payload, result_status, expected_content, expected_status) in [
        (
            "result-only",
            "message.final",
            json!({"block_id":"g5:terminal-result","content":"An answer."}),
            "succeeded",
            "An answer.",
            "complete",
        ),
        (
            "partial-error",
            "message.delta",
            json!({"block_id":"native-1:0","text_delta":"Working"}),
            "failed",
            "Working",
            "error",
        ),
        (
            "partial-stop",
            "message.delta",
            json!({"block_id":"native-2:0","text_delta":"Still working"}),
            "interrupted",
            "Still working",
            "stopped",
        ),
        (
            "vm-loss",
            "message.delta",
            json!({"block_id":"native-3:0","text_delta":"Before loss"}),
            "vm_lost",
            "Before loss",
            "stopped",
        ),
    ] {
        let (task_id, session_id) =
            create_agent_task(&app, &workspace, owner.id, "queued", None, None).await;
        let input_id = queue_session_input(&app, org.id, session_id, task_id, scenario).await;
        let reservation_id = bind_running_attempt(&app, org.id, task_id, vm_id, input_id).await;
        apply_agent_event(
            &app,
            org.id,
            (task_id, vm_id),
            1,
            "input.accepted",
            json!({
                "input_id":input_id,"reservation_id":reservation_id,"revision":1,
                "delivery_generation":1,"system_notice_ids":[]
            }),
            false,
        )
        .await;
        apply_agent_event(
            &app,
            org.id,
            (task_id, vm_id),
            2,
            event_kind,
            payload.clone(),
            false,
        )
        .await;
        if scenario == "result-only" {
            // A repeated completed unit is the same native block, not another answer.
            apply_agent_event(
                &app,
                org.id,
                (task_id, vm_id),
                3,
                event_kind,
                payload,
                false,
            )
            .await;
        }
        let outcome_seq = if scenario == "result-only" { 4 } else { 3 };
        if scenario == "vm-loss" {
            let mut db = app.scoped_db_for_org(org.id).await;
            app.state
                .task_service
                .apply(
                    &mut db,
                    TaskLifecycleEvent::VmLost {
                        task_id,
                        attempt: 1,
                        vm_id,
                        epoch: 1,
                        evidence: json!({"reason":"confirmed_execution_missing"}),
                    },
                )
                .await
                .unwrap();
            db.commit().await.unwrap();
        } else {
            apply_agent_event(
                &app,
                org.id,
                (task_id, vm_id),
                outcome_seq,
                "invocation.outcome",
                json!({
                    "status":"succeeded","result":{"status":result_status}
                }),
                true,
            )
            .await;
        }
        let materialized: (String, String) =
            sqlx::query_as("SELECT content, status FROM chat_messages WHERE id = $1")
                .bind(canonical_agent_message_id(task_id, 1))
                .fetch_one(&app.db)
                .await
                .unwrap();
        assert_eq!(
            materialized,
            (expected_content.into(), expected_status.into()),
            "{scenario}"
        );
        let timeline_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM task_events WHERE task_id = $1 AND metadata ->> 'kind' = $2",
        )
        .bind(task_id)
        .bind(event_kind)
        .fetch_one(&app.db)
        .await
        .unwrap();
        assert_eq!(
            timeline_count,
            if scenario == "result-only" { 2 } else { 1 }
        );
    }
    app.cleanup().await;
}

#[tokio::test]
async fn dependency_failure_settles_every_blocked_task() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let prerequisite = Uuid::new_v4();
    let dependent = Uuid::new_v4();
    for (task_id, name) in [(prerequisite, "Prerequisite"), (dependent, "Dependent")] {
        sqlx::query(
            "INSERT INTO tasks(
                 id, org_id, workspace_id, name, kind, status, spec,
                 attempt, created_by
             ) VALUES ($1,$2,$3,$4,'command','queued','{}'::jsonb,1,$5)",
        )
        .bind(task_id)
        .bind(org.id)
        .bind(workspace.id)
        .bind(name)
        .bind(owner.id)
        .execute(&app.db)
        .await
        .expect("insert dependency task");
    }
    sqlx::query(
        "INSERT INTO task_dependencies(org_id, workspace_id, task_id, depends_on_task_id)
         VALUES ($1,$2,$3,$4)",
    )
    .bind(org.id)
    .bind(workspace.id)
    .bind(dependent)
    .bind(prerequisite)
    .execute(&app.db)
    .await
    .expect("insert task dependency");

    let response = app
        .authenticated_client(&owner)
        .post_json(&format!("/api/v1/tasks/{prerequisite}/cancel"), &json!({}))
        .await;
    assert_eq!(response.status(), StatusCode::OK);

    let task_statuses: Vec<(Uuid, String)> =
        sqlx::query_as("SELECT id, status FROM tasks WHERE id IN ($1,$2) ORDER BY id")
            .bind(prerequisite)
            .bind(dependent)
            .fetch_all(&app.db)
            .await
            .expect("load dependency task statuses");
    assert!(task_statuses.contains(&(prerequisite, "cancelled".to_string())));
    assert!(task_statuses.contains(&(dependent, "failed".to_string())));
    app.cleanup().await;
}

#[tokio::test]
async fn ordinary_continue_rejects_delivery_unknown_without_mutating_evidence() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let client = app.authenticated_client(&owner);
    let (task_id, session_id) = create_agent_task(
        &app,
        &workspace,
        owner.id,
        "waiting",
        Some("interrupted"),
        Some("delivery_unknown"),
    )
    .await;

    sqlx::query(
        "INSERT INTO task_attempts (
             org_id, task_id, attempt, started_at, ended_at, reason
         ) VALUES ($1, $2, 1, now() - interval '1 second', now(), 'interrupted')",
    )
    .bind(workspace.org_id)
    .bind(task_id)
    .execute(&app.db)
    .await
    .expect("insert ended task attempt");

    let input_id = Uuid::new_v4();
    let reservation_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO session_inputs (
             id, org_id, session_id, input_index, content, input_origin,
             delivery, reservation_id, reservation_revision, task_id, attempt
         ) VALUES (
             $1, $2, $3, 0, 'possibly handed off', 'human',
             'delivery_unknown', $4, 1, $5, 1
         )",
    )
    .bind(input_id)
    .bind(workspace.org_id)
    .bind(session_id)
    .bind(reservation_id)
    .bind(task_id)
    .execute(&app.db)
    .await
    .expect("insert ambiguous input evidence");

    let response = client
        .post_json(&format!("/api/v1/tasks/{task_id}/continue"), &json!({}))
        .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);

    let task = sqlx::query_as::<_, (String, Option<String>, Option<String>)>(
        "SELECT status, continuation_reason, hold_reason FROM tasks WHERE id = $1",
    )
    .bind(task_id)
    .fetch_one(&app.db)
    .await
    .expect("load held task");
    assert_eq!(task.0, "waiting");
    assert_eq!(task.1.as_deref(), Some("interrupted"));
    assert_eq!(task.2.as_deref(), Some("delivery_unknown"));
    let delivery: String = sqlx::query_scalar("SELECT delivery FROM session_inputs WHERE id = $1")
        .bind(input_id)
        .fetch_one(&app.db)
        .await
        .expect("load input evidence");
    assert_eq!(delivery, "delivery_unknown");

    let cancel = client
        .post_json(
            &format!("/api/v1/sessions/{session_id}/inputs/{input_id}/cancel"),
            &json!({}),
        )
        .await;
    assert_eq!(cancel.status(), StatusCode::OK);
    let cancelled: Value = cancel.json().await.expect("cancel response json");
    assert_eq!(cancelled["data"]["delivery"], "withdrawn");
    let settled = sqlx::query_as::<_, (String, String)>(
        "SELECT task.status, input.delivery
         FROM tasks task JOIN session_inputs input ON input.task_id = task.id
         WHERE task.id = $1 AND input.id = $2",
    )
    .bind(task_id)
    .bind(input_id)
    .fetch_one(&app.db)
    .await
    .expect("load explicit uncertain-delivery cancellation");
    assert_eq!(settled, ("cancelled".to_string(), "withdrawn".to_string()));

    app.cleanup().await;
}

#[tokio::test]
async fn explicit_redelivery_clears_stale_cancel_and_interrupt_lifecycle_state() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let (task_id, session_id) = create_agent_task(
        &app,
        &workspace,
        owner.id,
        "waiting",
        Some("interrupted"),
        Some("delivery_unknown"),
    )
    .await;
    sqlx::query(
        "UPDATE tasks
         SET spec = jsonb_set(spec, '{lifecycle}',
             '{\"cancel_requested\":true,\"interrupt_reason\":\"drain\"}'::jsonb, true)
         WHERE id = $1",
    )
    .bind(task_id)
    .execute(&app.db)
    .await
    .expect("seed stale lifecycle state");
    sqlx::query(
        "INSERT INTO task_attempts(
             org_id, task_id, attempt, started_at, ended_at, reason
         ) VALUES ($1,$2,1,now() - interval '1 second',now(),'vm_lost')",
    )
    .bind(org.id)
    .bind(task_id)
    .execute(&app.db)
    .await
    .expect("insert prior attempt");
    let input_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO session_inputs(
             id, org_id, session_id, input_index, content, input_origin,
             delivery, reservation_id, reservation_revision, task_id, attempt
         ) VALUES ($1,$2,$3,0,'retry me','human','delivery_unknown',$4,1,$5,1)",
    )
    .bind(input_id)
    .bind(org.id)
    .bind(session_id)
    .bind(Uuid::new_v4())
    .bind(task_id)
    .execute(&app.db)
    .await
    .expect("insert delivery-unknown input");

    let response = app
        .authenticated_client(&owner)
        .post_json(
            &format!("/api/v1/sessions/{session_id}/inputs/{input_id}/redeliver"),
            &json!({}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let state: (String, Value) =
        sqlx::query_as("SELECT status, spec -> 'lifecycle' FROM tasks WHERE id = $1")
            .bind(task_id)
            .fetch_one(&app.db)
            .await
            .expect("load redelivered task");
    assert_eq!(state.0, "queued");
    assert!(state.1.get("redelivery").is_some());
    assert!(state.1.get("cancel_requested").is_none());
    assert!(state.1.get("interrupt_reason").is_none());
    let rebound: (Option<Uuid>, Option<i32>, String) =
        sqlx::query_as("SELECT task_id, attempt, delivery FROM session_inputs WHERE id = $1")
            .bind(input_id)
            .fetch_one(&app.db)
            .await
            .expect("load redelivered input binding");
    assert_eq!(rebound, (Some(task_id), None, "queued".to_string()));

    app.cleanup().await;
}

/// An owner who authorizes redelivery authorizes that content as a possible
/// duplicate: the revision stays frozen, so editing the re-queued input is
/// refused while withdrawing it still works, and an ordinary never-delivered
/// queued input stays editable (issue #629 S12).
#[tokio::test]
async fn redelivery_authorized_input_refuses_edits_but_not_withdrawal() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let (task_id, session_id) = create_agent_task(
        &app,
        &workspace,
        owner.id,
        "waiting",
        Some("interrupted"),
        Some("delivery_unknown"),
    )
    .await;
    sqlx::query(
        "INSERT INTO task_attempts(
             org_id, task_id, attempt, started_at, ended_at, reason
         ) VALUES ($1,$2,1,now() - interval '1 second',now(),'vm_lost')",
    )
    .bind(org.id)
    .bind(task_id)
    .execute(&app.db)
    .await
    .expect("insert prior attempt");
    let uncertain_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO session_inputs(
             id, org_id, session_id, input_index, content, input_origin,
             delivery, reservation_id, reservation_revision, task_id, attempt
         ) VALUES ($1,$2,$3,0,'possibly delivered once','human',
                   'delivery_unknown',$4,1,$5,1)",
    )
    .bind(uncertain_id)
    .bind(org.id)
    .bind(session_id)
    .bind(Uuid::new_v4())
    .bind(task_id)
    .execute(&app.db)
    .await
    .expect("insert delivery-unknown input");
    let ordinary_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO session_inputs(
             id, org_id, session_id, input_index, content, input_origin,
             delivery, task_id
         ) VALUES ($1,$2,$3,1,'never delivered','human','queued',$4)",
    )
    .bind(ordinary_id)
    .bind(org.id)
    .bind(session_id)
    .bind(task_id)
    .execute(&app.db)
    .await
    .expect("insert ordinary queued input");

    let client = app.authenticated_client(&owner);
    let redelivered = client
        .post_json(
            &format!("/api/v1/sessions/{session_id}/inputs/{uncertain_id}/redeliver"),
            &json!({}),
        )
        .await;
    let status = redelivered.status();
    let redelivered: Value = redelivered.json().await.expect("redeliver response");
    assert_eq!(status, StatusCode::OK, "{redelivered}");
    assert_eq!(redelivered["data"]["delivery"], "queued");
    assert_eq!(
        redelivered["data"]["editable"], false,
        "the possible duplicate's content is frozen"
    );
    assert_eq!(redelivered["data"]["withdrawable"], true);

    let edit = client
        .patch_json(
            &format!("/api/v1/sessions/{session_id}/inputs/{uncertain_id}"),
            &json!({"expectedRevision": 1, "content": "something else entirely"}),
        )
        .await;
    assert_eq!(edit.status(), StatusCode::CONFLICT);
    let frozen: (String, i32) =
        sqlx::query_as("SELECT content, revision FROM session_inputs WHERE id = $1")
            .bind(uncertain_id)
            .fetch_one(&app.db)
            .await
            .expect("load frozen input");
    assert_eq!(frozen, ("possibly delivered once".to_string(), 1));

    let edit = client
        .patch_json(
            &format!("/api/v1/sessions/{session_id}/inputs/{ordinary_id}"),
            &json!({"expectedRevision": 1, "content": "never delivered, edited"}),
        )
        .await;
    assert_eq!(
        edit.status(),
        StatusCode::OK,
        "an ordinary queued input stays editable"
    );

    let withdraw = client
        .post_json(
            &format!("/api/v1/sessions/{session_id}/inputs/{uncertain_id}/withdraw"),
            &json!({"expectedRevision": 1}),
        )
        .await;
    let status = withdraw.status();
    let withdraw: Value = withdraw.json().await.expect("withdraw response");
    assert_eq!(status, StatusCode::OK, "{withdraw}");
    assert_eq!(withdraw["data"]["delivery"], "withdrawn");

    app.cleanup().await;
}

#[tokio::test]
async fn preview_without_chat_input_is_dispatchable() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let vm_id = attach_running_vm_without_runtime(&app, &workspace).await;
    let task_id = insert_queued_preview(&app, &workspace, owner.id).await;

    let mut db = app.scoped_db_for_org(workspace.org_id).await;
    let dispatch = app
        .state
        .task_service
        .reserve_command_dispatch(&mut db)
        .await
        .expect("reserve preview dispatch")
        .expect("preview must be dispatchable without a chat input");
    assert_eq!(dispatch.task_id, task_id);
    assert_eq!(dispatch.vm_id, vm_id);
    db.commit().await.expect("commit preview reservation");

    app.cleanup().await;
}

/// A guest from an older module catalog refuses unknown preview input, so the
/// isolated origin (`g5.workspace.preview-origin-security`) reaches only VMs
/// whose catalog accepts it; the others still start the preview.
#[tokio::test]
async fn preview_origin_reaches_only_guests_whose_catalog_accepts_it() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    for (catalog, carries_origin) in [
        ("2026.10.26-workspace-preview-origin", true),
        ("2026.10.24-codex-app-server", false),
    ] {
        let workspace = TestWorkspace::create(&app, &org).await;
        attach_running_vm_without_runtime(&app, &workspace).await;
        sqlx::query("UPDATE workspace_vms SET module_catalog_version=$2 WHERE workspace_id=$1")
            .bind(workspace.id)
            .bind(catalog)
            .execute(&app.db)
            .await
            .unwrap();
        let task_id = insert_queued_preview(&app, &workspace, owner.id).await;
        let origin = format!("https://{task_id}.preview.g5.example");
        sqlx::query(
            "UPDATE tasks SET spec=jsonb_set(spec,'{command,input,origin}',to_jsonb($2::text))
             WHERE id=$1",
        )
        .bind(task_id)
        .bind(&origin)
        .execute(&app.db)
        .await
        .unwrap();

        let mut db = app.scoped_db_for_org(workspace.org_id).await;
        let dispatch = app
            .state
            .task_service
            .reserve_command_dispatch(&mut db)
            .await
            .expect("reserve preview dispatch")
            .expect("preview is dispatchable");
        db.commit().await.expect("commit preview reservation");
        assert_eq!(dispatch.task_id, task_id);
        let input = &dispatch.invoke_params["input"];
        assert_eq!(input["profile"], "default", "{catalog}");
        if carries_origin {
            assert_eq!(input["origin"], origin.as_str(), "{catalog}");
        } else {
            assert!(input.get("origin").is_none(), "{catalog}: {input}");
        }
    }

    app.cleanup().await;
}

#[tokio::test]
async fn failed_unconfirmed_invoke_does_not_block_another_workspace_in_the_org() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let failing_workspace = TestWorkspace::create(&app, &org).await;
    let healthy_workspace = TestWorkspace::create(&app, &org).await;
    let failing_runtime = helpers::TestWorkspaceRuntimePeer::attach_with_invoke_failure(
        &app,
        &failing_workspace,
        owner.id,
        "scripted invoke failure",
    )
    .await;
    let healthy_runtime =
        helpers::TestWorkspaceRuntimePeer::attach(&app, &healthy_workspace, owner.id).await;
    let failing_task = insert_queued_preview(&app, &failing_workspace, owner.id).await;

    let mut db = app.scoped_db_for_org(org.id).await;
    let reservation = app
        .state
        .task_service
        .reserve_command_dispatch(&mut db)
        .await
        .expect("reserve failing preview")
        .expect("failing preview must be dispatchable");
    assert_eq!(reservation.task_id, failing_task);
    db.commit().await.expect("commit failing reservation");

    sqlx::query("UPDATE tasks SET updated_at = now() - interval '3 seconds' WHERE id = $1")
        .bind(failing_task)
        .execute(&app.db)
        .await
        .expect("make unconfirmed reservation eligible for retry");
    let healthy_task = insert_queued_preview(&app, &healthy_workspace, owner.id).await;

    g5_api_core::maintenance::agent_tasks::run_once(&app.state)
        .await
        .expect("run task maintenance");

    let states = sqlx::query_as::<_, (Uuid, String, bool)>(
        "SELECT id, status, start_confirmed
         FROM tasks WHERE id = ANY($1) ORDER BY id",
    )
    .bind(vec![failing_task, healthy_task])
    .fetch_all(&app.db)
    .await
    .expect("load dispatch outcomes");
    assert_eq!(states.len(), 2);
    assert_eq!(
        states
            .iter()
            .find(|state| state.0 == failing_task)
            .map(|state| (state.1.as_str(), state.2)),
        Some(("running", false))
    );
    assert_eq!(
        states
            .iter()
            .find(|state| state.0 == healthy_task)
            .map(|state| (state.1.as_str(), state.2)),
        Some(("running", true))
    );
    assert_eq!(
        failing_runtime
            .received_rpc_methods()
            .await
            .iter()
            .filter(|method| method.as_str() == "modules.invoke")
            .count(),
        1
    );
    assert_eq!(
        healthy_runtime
            .received_rpc_methods()
            .await
            .iter()
            .filter(|method| method.as_str() == "modules.invoke")
            .count(),
        1
    );

    app.cleanup().await;
}

pub(super) async fn running_preview(
    app: &TestApp,
    workspace: &TestWorkspace,
    owner_id: Uuid,
    vm_id: Uuid,
) -> Uuid {
    let task_id = insert_queued_preview(app, workspace, owner_id).await;
    sqlx::query(
        "UPDATE tasks
         SET status = 'running', vm_id = $2, epoch = 1, start_confirmed = true,
             spec = spec || jsonb_build_object('preview',jsonb_build_object(
                 'port',4173,'repo_id',$3::text,'profile','default','generation',1))
         WHERE id = $1",
    )
    .bind(task_id)
    .bind(vm_id.to_string())
    .bind(workspace.repo_id)
    .execute(&app.db)
    .await
    .expect("mark preview running");
    sqlx::query(
        "UPDATE workspace_vms
         SET preview_generation = 1,
             preview_intent = jsonb_build_object(
                 'generation',1,'action','start','status','running',
                 'requested_by',$2::text,'task_id',$3::text)
         WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .bind(owner_id)
    .bind(task_id)
    .execute(&app.db)
    .await
    .expect("select current preview");
    sqlx::query(
        "INSERT INTO task_events(
             id, org_id, task_id, entry_type, content, metadata, attempt, vm_id, seq)
         VALUES ($1,$2,$3,'checkpoint','Preview ready',
                 jsonb_build_object('kind','module.preview.ready'),1,$4,1)",
    )
    .bind(Uuid::new_v4())
    .bind(workspace.org_id)
    .bind(task_id)
    .bind(vm_id.to_string())
    .execute(&app.db)
    .await
    .expect("record preview readiness");

    task_id
}

#[tokio::test]
async fn current_owner_alone_can_mint_a_live_preview_ticket() {
    let mut config = helpers::test_config();
    config.server.external_url = "https://app.g5.example".into();
    config.server.preview_host_suffix = Some("preview.g5.example".into());
    let app = TestApp::spawn_with_config(config).await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    // Satisfy route scopes so rejection must reach workspace ownership.
    let reader = TestUser::create_with_scopes(
        &app,
        org.id,
        &[
            Scope::TasksRead,
            Scope::WorkspacesRead,
            Scope::WorkspacesWrite,
        ],
    )
    .await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = helpers::TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let task_id = running_preview(&app, &workspace, owner.id, runtime.vm_id()).await;

    let reader_response = app
        .authenticated_client(&reader)
        .post_json(&format!("/api/v1/preview/session/{task_id}"), &json!({}))
        .await;
    assert_eq!(reader_response.status(), StatusCode::FORBIDDEN);
    let reader_head = app
        .client
        .head(format!("{}/api/v1/preview/{task_id}/", app.base_url))
        .header("Authorization", format!("Bearer {}", reader.jwt))
        .send()
        .await
        .expect("send unauthorized preview HEAD");
    assert_eq!(
        reader_head.status(),
        StatusCode::FORBIDDEN,
        "HEAD must enforce the same owner/task/port authority before opening a tunnel"
    );
    let owner_response = app
        .authenticated_client(&owner)
        .post_json(&format!("/api/v1/preview/session/{task_id}"), &json!({}))
        .await;
    assert_eq!(owner_response.status(), StatusCode::OK);
    assert!(
        owner_response.headers().get("set-cookie").is_none(),
        "no platform-origin preview cookie is ever set"
    );
    let minted: Value = owner_response.json().await.unwrap();
    assert!(minted["data"]["previewUrl"]
        .as_str()
        .unwrap()
        .starts_with(&format!("https://{task_id}.preview.g5.example/")));
    assert_eq!(reader_head.headers()["cache-control"], "private, no-store");

    // Host loss can mark the VM failed before it settles the Preview task.
    // A stale task/ready event must never mint a fresh ticket or serve data.
    sqlx::query(
        "UPDATE workspace_vms
         SET status = 'failed', host_id = NULL, vm_id = NULL,
             lease_expires_at = NULL
         WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .execute(&app.db)
    .await
    .expect("mark Preview VM lost before task settlement");
    let dead_mint = app
        .authenticated_client(&owner)
        .post_json(&format!("/api/v1/preview/session/{task_id}"), &json!({}))
        .await;
    assert_eq!(dead_mint.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert!(
        dead_mint
            .text()
            .await
            .unwrap()
            .contains("PREVIEW_NOT_READY"),
        "lost VM must fence a stale Preview ticket"
    );
    let dead_head = app
        .client
        .head(format!("{}/api/v1/preview/{task_id}/", app.base_url))
        .header("Authorization", format!("Bearer {}", owner.jwt))
        .send()
        .await
        .expect("probe dead Preview route");
    assert_eq!(dead_head.status(), StatusCode::SERVICE_UNAVAILABLE);

    app.cleanup().await;
}

#[tokio::test]
async fn subdomain_preview_mint_and_legacy_routes_cannot_serve_platform_origin_html() {
    let mut config = helpers::test_config();
    config.server.external_url = "https://app.g5.example".into();
    config.server.preview_host_suffix = Some("preview.g5.example".into());
    let app = TestApp::spawn_with_config(config).await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = helpers::TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let task_id = running_preview(&app, &workspace, owner.id, runtime.vm_id()).await;
    let response = app
        .authenticated_client(&owner)
        .post_json(&format!("/api/v1/preview/session/{task_id}"), &json!({}))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        response.headers().get("set-cookie").is_none(),
        "subdomain mint must not grant a platform-origin cookie"
    );
    assert_eq!(response.headers()["cache-control"], "private, no-store");
    let body: Value = response.json().await.unwrap();
    assert!(body["data"]["previewUrl"]
        .as_str()
        .unwrap()
        .starts_with(&format!(
            "https://{task_id}.preview.g5.example/?__g5_preview_ticket="
        )));

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    // A valid job ticket is a credential only on the preview host: presented
    // on the platform path, under either cookie name, it authenticates
    // nothing (`g5.security.mvp-exposure.preview`).
    let ticket = app
        .state
        .auth_service
        .mint_preview_ticket(owner.id, org.id, task_id)
        .unwrap();
    for method in ["GET", "HEAD", "POST", "PUT", "PATCH", "DELETE", "TRACE"] {
        for suffix in ["", "/", "/login"] {
            let response = client
                .request(
                    method.parse().unwrap(),
                    format!("{}/api/v1/preview/{task_id}{suffix}", app.base_url),
                )
                .bearer_auth(&owner.jwt)
                .send()
                .await
                .unwrap();
            assert_eq!(response.headers()["cache-control"], "private, no-store");
            assert!(response.headers().get("set-cookie").is_none());
            if matches!(method, "GET" | "HEAD") {
                assert_eq!(response.status(), StatusCode::FOUND);
                assert!(response.headers()["location"]
                    .to_str()
                    .unwrap()
                    .starts_with(&format!("https://{task_id}.preview.g5.example/")));
            } else {
                assert_eq!(
                    response.status(),
                    StatusCode::METHOD_NOT_ALLOWED,
                    "{method} {suffix}"
                );
                assert_eq!(response.headers()["allow"], "GET, HEAD");
            }
            for cookie_name in ["g5_preview", "__Host-g5_preview"] {
                let cookie_only = client
                    .request(
                        method.parse().unwrap(),
                        format!("{}/api/v1/preview/{task_id}{suffix}", app.base_url),
                    )
                    .header("cookie", format!("{cookie_name}={ticket}"))
                    .send()
                    .await
                    .unwrap();
                assert_eq!(
                    cookie_only.status(),
                    StatusCode::UNAUTHORIZED,
                    "{method} {suffix} {cookie_name}"
                );
            }
        }
    }
    // A cookie-only browser navigation is sent to re-auth, which mints a
    // preview-host handoff; the platform path itself still serves nothing.
    let navigation = client
        .get(format!("{}/api/v1/preview/{task_id}/", app.base_url))
        .header("cookie", format!("__Host-g5_preview={ticket}"))
        .header("accept", "text/html")
        .header("sec-fetch-dest", "document")
        .header("sec-fetch-mode", "navigate")
        .header("sec-fetch-site", "none")
        .send()
        .await
        .unwrap();
    assert_eq!(navigation.status(), StatusCode::FOUND);
    assert!(navigation.headers()["location"]
        .to_str()
        .unwrap()
        .contains("/api/v1/auth/preview/reauth/"));
    let refresh = helpers::insert_refresh_token(&app, owner.id).await;
    let reauth_url = format!(
        "{}/api/v1/auth/preview/reauth/{task_id}?return=/",
        app.base_url
    );
    let expiry_before: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT expires_at FROM refresh_tokens WHERE token_hash=$1")
            .bind(helpers::refresh_token_hash(&refresh))
            .fetch_one(&app.db)
            .await
            .unwrap();
    for cookie in [
        format!("g5_refresh={refresh}"),
        format!("__Host-g5_refresh={refresh}; __Host-g5_refresh={refresh}"),
        format!("__Host-g5_refresh=; __Host-g5_refresh={refresh}"),
    ] {
        let response = client
            .get(&reauth_url)
            .header("cookie", cookie)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert!(!response.headers().contains_key("location"));
    }
    let reauth = client
        .get(&reauth_url)
        .header(
            "cookie",
            format!("g5_refresh=planted; __Host-g5_refresh={refresh}"),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(reauth.status(), StatusCode::FOUND);
    assert!(reauth.headers()["location"]
        .to_str()
        .unwrap()
        .starts_with(&format!("https://{task_id}.preview.g5.example/")));
    let expiry_after: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT expires_at FROM refresh_tokens WHERE token_hash=$1")
            .bind(helpers::refresh_token_hash(&refresh))
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(
        expiry_after, expiry_before,
        "preview re-authentication remains read-only"
    );
    let attack = client
        .post(format!("{}/api/v1/preview/{task_id}/login", app.base_url))
        .bearer_auth(&owner.jwt)
        .header("sec-fetch-site", "same-site")
        .header("sec-fetch-mode", "navigate")
        .body("attack=1")
        .send()
        .await
        .unwrap();
    assert_eq!(attack.status(), StatusCode::FORBIDDEN);
    let upgrade = client
        .get(format!("{}/api/v1/preview/{task_id}/ws", app.base_url))
        .bearer_auth(&owner.jwt)
        .header("connection", "Upgrade")
        .header("upgrade", "websocket")
        .header("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ==")
        .header("sec-websocket-version", "13")
        .send()
        .await
        .unwrap();
    assert_eq!(
        upgrade.status(),
        StatusCode::FOUND,
        "platform-path upgrades must redirect before tunneling"
    );

    // Satisfy route scopes so rejection must reach workspace ownership.
    let reader = TestUser::create_with_scopes(
        &app,
        org.id,
        &[
            Scope::TasksRead,
            Scope::WorkspacesRead,
            Scope::WorkspacesWrite,
        ],
    )
    .await;
    for method in ["GET", "POST"] {
        let denied = client
            .request(
                method.parse().unwrap(),
                format!("{}/api/v1/preview/{task_id}/", app.base_url),
            )
            .bearer_auth(&reader.jwt)
            .send()
            .await
            .unwrap();
        assert_eq!(
            denied.status(),
            StatusCode::FORBIDDEN,
            "authorize before redirect or method fence"
        );
    }
    sqlx::query("UPDATE workspace_vms SET status = 'failed', host_id = NULL, vm_id = NULL, lease_expires_at = NULL WHERE workspace_id = $1")
        .bind(workspace.id).execute(&app.db).await.unwrap();
    for method in ["GET", "POST"] {
        let lost = client
            .request(
                method.parse().unwrap(),
                format!("{}/api/v1/preview/{task_id}/", app.base_url),
            )
            .bearer_auth(&owner.jwt)
            .send()
            .await
            .unwrap();
        assert_eq!(
            lost.status(),
            StatusCode::SERVICE_UNAVAILABLE,
            "readiness is checked before the redirect"
        );
    }
    app.cleanup().await;
}

/// Without a preview suffix the platform runs with previews unavailable
/// (`g5.security.mvp-exposure.preview`): the task still starts, but the
/// session endpoint, the platform path and re-auth all answer
/// `PREVIEW_ORIGIN_UNCONFIGURED` and nothing is served or minted.
#[tokio::test]
async fn previews_are_unavailable_without_a_preview_origin() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = helpers::TestWorkspaceRuntimePeer::attach_with_preview_response(
        &app,
        &workspace,
        owner.id,
        b"HTTP/1.1 200 OK\r\ncontent-type: text/html\r\ncontent-length: 5\r\n\r\nhello",
    )
    .await;
    let task_id = running_preview(&app, &workspace, owner.id, runtime.vm_id()).await;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();

    let mint = app
        .authenticated_client(&owner)
        .post_json(&format!("/api/v1/preview/session/{task_id}"), &json!({}))
        .await;
    assert_eq!(mint.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert!(mint.headers().get("set-cookie").is_none());
    let body = mint.text().await.unwrap();
    assert!(body.contains("PREVIEW_ORIGIN_UNCONFIGURED"), "{body}");

    for method in ["GET", "HEAD", "POST"] {
        let response = client
            .request(
                method.parse().unwrap(),
                format!("{}/api/v1/preview/{task_id}/", app.base_url),
            )
            .bearer_auth(&owner.jwt)
            .send()
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::SERVICE_UNAVAILABLE,
            "{method}"
        );
        assert!(response.headers().get("set-cookie").is_none());
        assert_eq!(response.headers()["cache-control"], "private, no-store");
    }
    let refresh = helpers::insert_refresh_token(&app, owner.id).await;
    let reauth = client
        .get(format!(
            "{}/api/v1/auth/preview/reauth/{task_id}?return=/",
            app.base_url
        ))
        .header("cookie", format!("__Host-g5_refresh={refresh}"))
        .send()
        .await
        .unwrap();
    assert_eq!(reauth.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert!(reauth.headers().get("set-cookie").is_none());
    assert!(reauth
        .text()
        .await
        .unwrap()
        .contains("PREVIEW_ORIGIN_UNCONFIGURED"));
    assert_eq!(runtime.preview_stream_count(), 0, "nothing was relayed");

    // The preview task itself is unaffected: it is a visible service that
    // the owner can still close.
    let task = app
        .authenticated_client(&owner)
        .get(&format!("/api/v1/tasks/{task_id}"))
        .await;
    assert_eq!(task.status(), StatusCode::OK);
    app.cleanup().await;
}

/// A preview stream opens only on the incarnation its task runs on
/// (`g5.preview.transport`): the target names that VM and epoch, the stream
/// carries the port in the shared header, and after the workspace moves to
/// a newer epoch the same target is refused instead of served there.
#[tokio::test]
async fn preview_stream_opens_only_on_the_preview_incarnation() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = helpers::TestWorkspaceRuntimePeer::attach_with_preview_response(
        &app,
        &workspace,
        owner.id,
        b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\nok",
    )
    .await;
    let task_id = running_preview(&app, &workspace, owner.id, runtime.vm_id()).await;
    let mut db = app.scoped_db_for_org(workspace.org_id).await;
    let target = app
        .state
        .task_service
        .workspace_preview_target(&mut db, task_id, true)
        .await
        .expect("resolve the running preview");
    db.commit().await.expect("commit the target read");
    assert_eq!(
        (target.vm_id, target.epoch, target.port),
        (runtime.vm_id(), 1, 4173)
    );

    let (mut stream, _activity) = app
        .state
        .workspace_runtime
        .open_preview_stream(
            target.workspace_id,
            target.vm_id,
            target.epoch,
            target.port,
            owner.id,
        )
        .await
        .expect("the preview's own incarnation opens");
    stream
        .send(b"GET / HTTP/1.1\r\nhost: preview\r\n\r\n")
        .await
        .expect("send the request");
    let response = stream.recv().await.expect("the preview answers");
    assert!(response.starts_with(b"HTTP/1.1 200"), "{response:?}");
    assert_eq!(runtime.preview_stream_count(), 1);

    sqlx::query("UPDATE workspace_vms SET epoch = epoch + 1 WHERE workspace_id = $1")
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .expect("replace the incarnation");
    let refused = app
        .state
        .workspace_runtime
        .open_preview_stream(
            target.workspace_id,
            target.vm_id,
            target.epoch,
            target.port,
            owner.id,
        )
        .await;
    assert!(
        matches!(
            refused,
            Err(g5_contracts::error::G5Error::ServiceUnavailable { ref message })
                if message == "PREVIEW_NOT_READY"
        ),
        "a stale target is refused: {:?}",
        refused.map(|_| ())
    );
    assert_eq!(
        runtime.preview_stream_count(),
        1,
        "no stream opens on the newer incarnation"
    );
    app.cleanup().await;
}

/// Minting a preview ticket reads the preview's incarnation with the clock at
/// the time of the read, not when the request's transaction began: when that
/// transaction waits for the workspace row (in its authorization) while the
/// VM lease ends, the preview is not ready and no ticket is minted.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn preview_ticket_checks_the_lease_after_its_transaction_waits() {
    let mut config = helpers::test_config();
    config.server.external_url = "https://app.g5.example".into();
    config.server.preview_host_suffix = Some("preview.g5.example".into());
    let app = TestApp::spawn_with_config(config).await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = helpers::TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let task_id = running_preview(&app, &workspace, owner.id, runtime.vm_id()).await;

    let holder = hold_while_the_lease_ends(
        &app,
        workspace.id,
        "SELECT 1 FROM workspaces WHERE id = $1 FOR UPDATE",
    )
    .await;
    let client = app.authenticated_client(&owner);
    let path = format!("/api/v1/preview/session/{task_id}");
    let request = json!({});
    let mint = client.post_json(&path, &request);
    let release = async {
        wait_for_lock_wait_past_the_lease(&app, workspace.id, AUTHORIZATION_LOCK).await;
        holder.rollback().await.expect("release the workspace row");
    };
    let (response, ()) = tokio::join!(mint, release);
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert!(
        response.headers().get("set-cookie").is_none(),
        "no preview ticket is minted"
    );
    let body = response.text().await.unwrap();
    assert!(body.contains("PREVIEW_NOT_READY"), "{body}");
    app.cleanup().await;
}

#[tokio::test]
async fn preview_http_relay_rewrites_isolated_app_cookies_and_renews_g5_ticket() {
    {
        let mut config = helpers::test_config();
        config.server.external_url = "https://app.g5.example".into();
        config.server.preview_host_suffix = Some("preview.g5.example".into());
        let app = TestApp::spawn_with_config(config).await;
        let mut owner = TestUser::create(&app).await;
        let org = TestOrg::create_with_admin(&app, &mut owner).await;
        let workspace = TestWorkspace::create(&app, &org).await;
        let runtime = helpers::TestWorkspaceRuntimePeer::attach_with_preview_response(
            &app, &workspace, owner.id,
            b"HTTP/1.1 200 OK\r\ncontent-type: text/html\r\ncontent-length: 5\r\nset-cookie: sid=abc; Path=/; HttpOnly\r\nset-cookie: domain_cookie=parent; Domain=g5.example; Path=/; SameSite=Lax\r\nset-cookie: utf8=caf\xc3\xa9; Domain=g5.example; Path=/\r\nset-cookie: g5_refresh=evil; Path=/\r\nset-cookie: __Host-g5_refresh=evil; Secure; Path=/\r\nset-cookie: __Host-g5_github_app_pkce=evil; Secure; Path=/\r\nset-cookie: G5_REFRESH=evil; Path=/\r\nset-cookie: g5_preview=evil; Path=/\r\nset-cookie: __Host-g5_preview=evil; Secure; Path=/\r\nset-cookie: __Secure-g5_x=evil; Secure\r\nset-cookie: broken\r\nclear-site-data: \"cookies\", \"storage\"\r\ncontent-security-policy: sandbox allow-scripts allow-same-origin\r\nnel: {\"max_age\":86400,\"report_to\":\"attacker\"}\r\nreport-to: {\"group\":\"attacker\"}\r\nstrict-transport-security: max-age=0\r\n\r\nhello",
        ).await;
        let task_id = running_preview(&app, &workspace, owner.id, runtime.vm_id()).await;
        // Exercise the production renewal emission, not only its decision:
        // a correctly signed job ticket inside the renewal window reaches HTTP.
        let now = chrono::Utc::now().timestamp();
        let claims = g5_auth::JwtClaims {
            sub: owner.id.to_string(),
            org: org.id.to_string(),
            scopes: vec![Scope::TasksRead, Scope::WorkspacesRead],
            iat: now - 1200,
            exp: now + 600,
            iss: Some("g5-api-test".into()),
            aud: Some(g5_auth::preview_ticket_audience(task_id)),
            jti: Some(Uuid::new_v4().to_string()),
            token_use: Some(g5_auth::PREVIEW_TOKEN_USE.into()),
            client: None,
        };
        let ticket = jsonwebtoken::encode(
            &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::ES256),
            &claims,
            &jsonwebtoken::EncodingKey::from_ec_pem(helpers::TEST_PRIVATE_KEY_PEM.as_bytes())
                .unwrap(),
        )
        .unwrap();
        let host = format!("{task_id}.preview.g5.example");
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        let handoff = app
            .state
            .auth_service
            .mint_preview_handoff_ticket(owner.id, org.id, task_id)
            .unwrap();
        let exchange = client
            .get(format!(
                "{}/login?__g5_preview_ticket={handoff}",
                app.base_url
            ))
            .header("host", &host)
            .header("sec-fetch-site", "same-site")
            .send()
            .await
            .unwrap();
        assert_eq!(exchange.status(), StatusCode::SEE_OTHER);
        assert!(exchange.headers()["set-cookie"]
            .to_str()
            .unwrap()
            .starts_with("__Host-g5_preview="));
        assert_eq!(exchange.headers()["location"], "/login");
        let cookie_name = "__Host-g5_preview";
        let response = client
            .post(format!("{}/login", app.base_url))
            .header("cookie", format!("{cookie_name}={ticket}"))
            .header("sec-fetch-site", "same-site")
            .header("host", &host)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert!(!response.headers().contains_key("clear-site-data"));
        // Upstream policy headers reach the isolated host untouched; only
        // the platform path ever needed containment.
        assert!(!response
            .headers()
            .get_all("content-security-policy")
            .iter()
            .any(|v| v == "sandbox"));
        assert_eq!(
            response.headers()["nel"],
            r#"{"max_age":86400,"report_to":"attacker"}"#
        );
        for name in ["report-to", "strict-transport-security"] {
            assert!(response.headers().contains_key(name), "{name}");
        }
        let cookie_values: Vec<_> = response.headers().get_all("set-cookie").iter().collect();
        assert_eq!(cookie_values.len(), 4);
        let cookies: Vec<_> = cookie_values
            .iter()
            .filter_map(|value| value.to_str().ok())
            .collect();
        assert!(cookies.contains(&"sid=abc; Path=/; HttpOnly"));
        assert!(cookies.contains(&"domain_cookie=parent; Path=/; SameSite=Lax"));
        assert!(cookie_values
            .iter()
            .any(|value| value.as_bytes() == b"utf8=caf\xc3\xa9; Path=/"));
        assert!(cookies
            .iter()
            .all(|cookie| !cookie.to_ascii_lowercase().contains("domain=")));
        let renewed_cookies: Vec<_> = cookies
            .iter()
            .filter(|cookie| cookie.starts_with(&format!("{cookie_name}=")))
            .collect();
        assert_eq!(
            renewed_cookies.len(),
            1,
            "G5's renewed ticket must appear once"
        );
        let cookie = renewed_cookies[0];
        assert!(cookie.starts_with(&format!("{cookie_name}=")), "{cookie}");
        assert!(cookie.contains("HttpOnly; Secure;"));
        assert!(cookie.contains("Path=/;"));
        let renewed = cookie
            .split_once('=')
            .unwrap()
            .1
            .split(';')
            .next()
            .unwrap()
            .to_string();
        let renewed_claims = app
            .state
            .jwt_validator
            .as_ref()
            .unwrap()
            .validate_preview_ticket(&renewed, task_id)
            .unwrap();
        assert_eq!(renewed_claims.sub, claims.sub);
        assert_eq!(renewed_claims.org, claims.org);
        assert!(renewed_claims.exp > claims.exp + 1000);
        assert_eq!(response.headers()["cache-control"], "private, no-store");
        assert_eq!(response.text().await.unwrap(), "hello");
        {
            // The browser's next request carries the app session back to the
            // same upstream while the relay still hides G5's preview ticket.
            let round_trip = client
                .get(format!("{}/account", app.base_url))
                .header("host", &host)
                .header("sec-fetch-site", "same-site")
                .header(
                    "cookie",
                    format!("{cookie_name}={renewed}; sid=abc; utf8=café"),
                )
                .send()
                .await
                .unwrap();
            assert_eq!(round_trip.status(), StatusCode::OK);

            let websocket_request = || {
                client
                    .get(format!("{}/socket", app.base_url))
                    .header("host", &host)
                    .header("cookie", format!("{cookie_name}={renewed}; sid=abc"))
                    .header("connection", "Upgrade")
                    .header("upgrade", "websocket")
                    .header("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ==")
                    .header("sec-websocket-version", "13")
            };
            for foreign_origin in [None, Some("null"), Some("https://other.preview.g5.example")] {
                let mut request = websocket_request();
                if let Some(origin) = foreign_origin {
                    request = request.header("origin", origin);
                }
                let blocked = request.send().await.unwrap();
                assert_eq!(blocked.status(), StatusCode::FORBIDDEN);
            }
            assert_eq!(runtime.preview_requests().await.len(), 2);
            assert_eq!(runtime.preview_stream_count(), 2);
            let allowed = websocket_request()
                .header("origin", format!("https://{host}"))
                .send()
                .await
                .unwrap();
            assert_eq!(allowed.status(), StatusCode::OK);
        }
        let requests = runtime.preview_requests().await;
        assert_eq!(requests.len(), 3);
        assert_eq!(runtime.preview_stream_count(), 3);
        assert!(requests[0].starts_with("POST /login HTTP/1.1\r\n"));
        assert!(!requests[0].contains(&ticket));
        {
            assert!(requests[1].starts_with("GET /account HTTP/1.1\r\n"));
            assert!(requests[1].contains("cookie: sid=abc; utf8=café\r\n"));
            assert!(!requests[1].contains(&renewed));
            assert!(requests[2].starts_with("GET /socket HTTP/1.1\r\n"));
            let websocket_head = requests[2].to_ascii_lowercase();
            assert!(websocket_head.contains("connection: upgrade\r\n"));
            assert!(websocket_head.contains("upgrade: websocket\r\n"));
            assert!(requests[2].contains(&format!("origin: https://{host}\r\n")));
            assert!(!requests[2].contains(&renewed));
        }
        app.cleanup().await;
    }
}

/// Creates a `g5k_` organization API key for `user` through the public route
/// and returns the key row id (for revocation) with its plaintext.
async fn create_api_key(
    app: &TestApp,
    user: &TestUser,
    org_id: Uuid,
    scopes: &[&str],
) -> (Uuid, String) {
    let response = app
        .authenticated_client(user)
        .post_json(
            &format!("/api/v1/orgs/{org_id}/api-keys"),
            &json!({ "name": "delegated key", "scopes": scopes }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("parse api key body");
    let key_id = body["data"]["apiKey"]["id"]
        .as_str()
        .expect("api key id")
        .parse()
        .expect("api key id is a uuid");
    let key = body["data"]["key"].as_str().expect("plaintext key");
    assert!(key.starts_with("g5k_"));
    (key_id, key.to_string())
}

/// A refusal at the preview issuance boundary: `403` naming
/// `user_session_required`, with no handoff URL in the body, no `Location`
/// and no `Set-Cookie`. A `HEAD` response carries no body.
async fn assert_no_browser_credential_issued(response: reqwest::Response, context: &str) {
    assert_eq!(response.status(), StatusCode::FORBIDDEN, "{context}");
    assert!(
        response.headers().get("location").is_none(),
        "{context}: a refusal must not redirect"
    );
    assert!(
        response.headers().get("set-cookie").is_none(),
        "{context}: a refusal must not set a cookie"
    );
    assert_eq!(
        response.headers()["cache-control"],
        "private, no-store",
        "{context}"
    );
    let body = response.text().await.unwrap();
    assert!(
        !body.contains("__g5_preview_ticket") && !body.contains("previewUrl"),
        "{context}: credential-bearing body {body}"
    );
    if !body.is_empty() {
        let error: Value =
            serde_json::from_str(&body).unwrap_or_else(|e| panic!("{context}: {e}: {body}"));
        assert_eq!(error["error"]["code"], "FORBIDDEN", "{context}: {body}");
        assert_eq!(
            error["error"]["details"]["missing_scopes"],
            json!(["user_session_required"]),
            "{context}: {body}"
        );
    }
}

/// Organization API keys cannot turn themselves into a browser preview
/// credential (G5Labs/G5-Lean#661; `g5.security.credential-classes`,
/// `g5.workspace.preview-origin-security`). An owner-attributed key — narrow
/// (`tasks:read`) or broad (every user scope) — still reads the task and
/// relays on the isolated preview host, but the explicit mint, every
/// platform-path GET/HEAD form and relay renewal refuse it with
/// `user_session_required` and produce no handoff URL, redirect or cookie,
/// and its relay access ends with its revocation. On the same fixture the
/// owner's session mints from the POST and the platform path, the handoff
/// exchanges once at the preview host into a ticket cookie that relays, and
/// refresh-cookie re-authentication still mints a handoff.
#[tokio::test]
async fn api_keys_cannot_mint_preview_browser_handoffs() {
    let mut config = helpers::test_config();
    config.server.external_url = "https://app.g5.example".into();
    config.server.preview_host_suffix = Some("preview.g5.example".into());
    let app = TestApp::spawn_with_config(config).await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = helpers::TestWorkspaceRuntimePeer::attach_with_preview_response(
        &app,
        &workspace,
        owner.id,
        b"HTTP/1.1 200 OK\r\ncontent-type: text/plain\r\ncontent-length: 5\r\n\r\nhello",
    )
    .await;
    let task_id = running_preview(&app, &workspace, owner.id, runtime.vm_id()).await;
    let host = format!("{task_id}.preview.g5.example");
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let all_scopes: Vec<&str> = Scope::all()
        .iter()
        .filter(|scope| **scope != Scope::WorkerHost)
        .map(|scope| scope.as_str())
        .collect();
    let keys = [
        (
            "narrow tasks:read key",
            create_api_key(&app, &owner, org.id, &["tasks:read"]).await,
        ),
        (
            "broad every-scope key",
            create_api_key(&app, &owner, org.id, &all_scopes).await,
        ),
    ];
    let platform_forms = [
        format!("/api/v1/preview/{task_id}"),
        format!("/api/v1/preview/{task_id}/"),
        format!("/api/v1/preview/{task_id}/index.html"),
    ];

    for (label, (_, key)) in &keys {
        // The key is live, owner-attributed and in scope — it reads the task —
        // so every refusal below is the issuance boundary, not the key.
        let read = client
            .get(format!("{}/api/v1/tasks/{task_id}", app.base_url))
            .bearer_auth(key)
            .send()
            .await
            .unwrap();
        assert_eq!(read.status(), StatusCode::OK, "{label} reads the task");

        let mint = client
            .post(format!("{}/api/v1/preview/session/{task_id}", app.base_url))
            .bearer_auth(key)
            .json(&json!({}))
            .send()
            .await
            .unwrap();
        assert_no_browser_credential_issued(mint, &format!("{label}: POST mint")).await;

        for method in [reqwest::Method::GET, reqwest::Method::HEAD] {
            for path in &platform_forms {
                let response = client
                    .request(method.clone(), format!("{}{path}", app.base_url))
                    .bearer_auth(key)
                    .send()
                    .await
                    .unwrap();
                assert_no_browser_credential_issued(response, &format!("{label}: {method} {path}"))
                    .await;
            }
        }

        // Relay on the isolated preview host is preserved, but renewal never
        // hands the key a ticket cookie and the key is stripped before relay.
        // Also cover curl: it sends no Sec-Fetch-* metadata (#1120).
        for fetch_site in [Some("same-site"), None] {
            let label = format!("{label} (sec-fetch-site={fetch_site:?})");
            let relayed_before = runtime.preview_requests().await.len();
            let mut request = client
                .get(format!("{}/account", app.base_url))
                .header("host", &host)
                .bearer_auth(key);
            if let Some(site) = fetch_site {
                request = request.header("sec-fetch-site", site);
            }
            let relay = request.send().await.unwrap();
            assert_eq!(
                relay.status(),
                StatusCode::OK,
                "{label} relays on the preview host"
            );
            assert!(
                relay.headers().get("set-cookie").is_none(),
                "{label}: relay must not renew or mint a preview ticket for a key"
            );
            assert!(relay.headers().get("location").is_none(), "{label}");
            assert_eq!(relay.text().await.unwrap(), "hello", "{label}");
            let requests = runtime.preview_requests().await;
            assert_eq!(requests.len(), relayed_before + 1, "{label}");
            let relayed = requests.last().unwrap();
            assert!(relayed.starts_with("GET /account HTTP/1.1\r\n"), "{label}");
            assert!(
                !relayed.contains(key.as_str()),
                "{label}: the key must be stripped before relay"
            );
        }
    }

    // Positive controls on the same fixture: the owner's session mints a
    // handoff from the POST and from both platform-path methods, the handoff
    // exchanges exactly once into the ticket cookie, and the cookie relays.
    let minted = app
        .authenticated_client(&owner)
        .post_json(&format!("/api/v1/preview/session/{task_id}"), &json!({}))
        .await;
    assert_eq!(minted.status(), StatusCode::OK);
    let minted: Value = minted.json().await.unwrap();
    let preview_url = minted["data"]["previewUrl"].as_str().unwrap();
    let handoff = preview_url
        .strip_prefix(&format!("https://{host}/?__g5_preview_ticket="))
        .expect("owner handoff URL");
    let exchange_url = format!("{}/?__g5_preview_ticket={handoff}", app.base_url);
    let exchange = client
        .get(&exchange_url)
        .header("host", &host)
        .header("sec-fetch-site", "same-site")
        .send()
        .await
        .unwrap();
    assert_eq!(exchange.status(), StatusCode::SEE_OTHER);
    let cookie = exchange.headers()["set-cookie"].to_str().unwrap();
    assert!(cookie.starts_with("__Host-g5_preview="), "{cookie}");
    let ticket = cookie
        .split_once('=')
        .unwrap()
        .1
        .split(';')
        .next()
        .unwrap()
        .to_string();
    let exchanged = app
        .state
        .jwt_validator
        .as_ref()
        .unwrap()
        .validate_preview_ticket(&ticket, task_id)
        .expect("the exchanged cookie is this preview's ticket");
    assert_eq!(exchanged.sub, owner.id.to_string());
    assert_eq!(exchanged.org, org.id.to_string());
    let replay = client
        .get(&exchange_url)
        .header("host", &host)
        .header("sec-fetch-site", "same-site")
        .send()
        .await
        .unwrap();
    assert_eq!(replay.status(), StatusCode::UNAUTHORIZED);
    assert!(replay.headers().get("set-cookie").is_none());
    let relay = client
        .get(format!("{}/account", app.base_url))
        .header("host", &host)
        .header("sec-fetch-site", "same-site")
        .header("cookie", format!("__Host-g5_preview={ticket}"))
        .send()
        .await
        .unwrap();
    assert_eq!(relay.status(), StatusCode::OK);
    assert_eq!(relay.text().await.unwrap(), "hello");
    for method in [reqwest::Method::GET, reqwest::Method::HEAD] {
        for path in &platform_forms {
            let redirect = client
                .request(method.clone(), format!("{}{path}", app.base_url))
                .bearer_auth(&owner.jwt)
                .send()
                .await
                .unwrap();
            assert_eq!(redirect.status(), StatusCode::FOUND, "{method} {path}");
            assert!(
                redirect.headers()["location"]
                    .to_str()
                    .unwrap()
                    .starts_with(&format!("https://{host}/")),
                "{method} {path}"
            );
            assert!(
                redirect.headers()["location"]
                    .to_str()
                    .unwrap()
                    .contains("__g5_preview_ticket="),
                "{method} {path}"
            );
        }
    }
    let refresh = helpers::insert_refresh_token(&app, owner.id).await;
    let reauth = client
        .get(format!(
            "{}/api/v1/auth/preview/reauth/{task_id}?return=/",
            app.base_url
        ))
        .header("cookie", format!("__Host-g5_refresh={refresh}"))
        .send()
        .await
        .unwrap();
    assert_eq!(reauth.status(), StatusCode::FOUND);
    assert!(reauth.headers()["location"]
        .to_str()
        .unwrap()
        .starts_with(&format!("https://{host}/?__g5_preview_ticket=")));

    // Revocation ends the key's relay access; nothing it did outlives it.
    for (label, (key_id, key)) in &keys {
        let revoke = app
            .authenticated_client(&owner)
            .delete(&format!("/api/v1/orgs/{}/api-keys/{key_id}", org.id))
            .await;
        assert_eq!(revoke.status(), StatusCode::OK, "{label}");
        let relay = client
            .get(format!("{}/account", app.base_url))
            .header("host", &host)
            .header("sec-fetch-site", "same-site")
            .bearer_auth(key)
            .send()
            .await
            .unwrap();
        assert_eq!(
            relay.status(),
            StatusCode::UNAUTHORIZED,
            "{label}: a revoked key no longer relays"
        );
        assert!(relay.headers().get("set-cookie").is_none(), "{label}");
        let mint = client
            .post(format!("{}/api/v1/preview/session/{task_id}", app.base_url))
            .bearer_auth(key)
            .json(&json!({}))
            .send()
            .await
            .unwrap();
        assert_eq!(mint.status(), StatusCode::UNAUTHORIZED, "{label}");
        assert!(mint.headers().get("set-cookie").is_none(), "{label}");
    }
    app.cleanup().await;
}

/// One preview at a time (`g5.preview.service.replace`, amended 2026-10-05):
/// a second start is refused with a conflict while the current preview is
/// active, the same request key replays its own outcome, and non-owners may
/// read the durable preview status but can neither start nor close one.
#[tokio::test]
async fn second_preview_start_is_refused_while_one_is_active_and_non_owners_cannot_start_or_cancel()
{
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let reader = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = helpers::TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    runtime
        .seed_file(
            workspace.repo_id,
            ".g5/workspace.yaml",
            "preview:\n  command: node server.js\n  port: 4173\n  cleanup_command: node down.js\n",
        )
        .await;
    let owner_client = app.authenticated_client(&owner);
    let start = |key: &str| json!({"repoId": workspace.repo_id, "idempotencyKey": key});

    let first = owner_client
        .post_json(
            &format!("/api/v1/workspaces/{}/preview", workspace.id),
            &start("preview-1"),
        )
        .await;
    let first_status = first.status();
    let first: Value = first.json().await.unwrap_or(Value::Null);
    assert_eq!(first_status, StatusCode::OK, "first start: {first}");
    let first_task = first["data"]["taskId"]
        .as_str()
        .expect("first preview task id")
        .to_string();
    assert_eq!(first["data"]["ready"], false);
    assert_eq!(first["data"]["closing"], false);
    assert!(first["data"]["close"].is_null());

    // One preview at a time (`g5.preview.service.replace`): a second start
    // with a new key is refused while the first is active, and nothing
    // about the first changes.
    let second = owner_client
        .post_json(
            &format!("/api/v1/workspaces/{}/preview", workspace.id),
            &start("preview-2"),
        )
        .await;
    let second_status = second.status();
    let second: Value = second.json().await.unwrap_or(Value::Null);
    assert_eq!(
        second_status,
        StatusCode::CONFLICT,
        "second start: {second}"
    );
    assert!(
        second
            .to_string()
            .contains("close it before starting another"),
        "the refusal names Close as the next step: {second}"
    );
    let still_queued: String = sqlx::query_scalar("SELECT status FROM tasks WHERE id = $1::uuid")
        .bind(&first_task)
        .fetch_one(&app.db)
        .await
        .expect("first task status");
    assert_eq!(still_queued, "queued", "a refused start cancels nothing");
    let current = owner_client
        .get(&format!("/api/v1/workspaces/{}/preview", workspace.id))
        .await;
    let current: Value = current.json().await.unwrap_or(Value::Null);
    assert_eq!(current["data"]["taskId"], first_task.as_str());

    // The same key replays its own outcome instead of being refused.
    let replay = owner_client
        .post_json(
            &format!("/api/v1/workspaces/{}/preview", workspace.id),
            &start("preview-1"),
        )
        .await;
    let replay_status = replay.status();
    let replay: Value = replay.json().await.unwrap_or(Value::Null);
    assert_eq!(replay_status, StatusCode::OK, "replay: {replay}");
    assert_eq!(replay["data"]["taskId"], first_task.as_str());
    let second_task = first_task.clone();

    // Non-owners: durable status is readable; start and cancel are refused.
    let reader_client = app.authenticated_client(&reader);
    let reader_view = reader_client
        .get(&format!("/api/v1/workspaces/{}/preview", workspace.id))
        .await;
    assert_eq!(reader_view.status(), StatusCode::OK);
    let reader_start = reader_client
        .post_json(
            &format!("/api/v1/workspaces/{}/preview", workspace.id),
            &start("reader-1"),
        )
        .await;
    assert_eq!(reader_start.status(), StatusCode::FORBIDDEN);
    let reader_cancel = reader_client
        .post_json(
            &format!("/api/v1/workspaces/{}/preview/cancel", workspace.id),
            &json!({}),
        )
        .await;
    assert_eq!(reader_cancel.status(), StatusCode::FORBIDDEN);
    let still_current: String = sqlx::query_scalar("SELECT status FROM tasks WHERE id = $1::uuid")
        .bind(&second_task)
        .fetch_one(&app.db)
        .await
        .expect("current preview status");
    assert_ne!(
        still_current, "cancelled",
        "a reader cannot cancel the owner's preview"
    );

    app.cleanup().await;
}

/// Close (`g5.preview.service.close`): the view shows `closing` while the
/// durable cancel intent waits for the guest, a start is still refused, and
/// the guest's cancelled outcome classifies the close for the owner.
#[tokio::test]
async fn closing_preview_is_visible_until_the_guest_reports_how_it_closed() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = helpers::TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    runtime
        .seed_file(
            workspace.repo_id,
            ".g5/workspace.yaml",
            "preview:\n  command: node server.js\n  port: 4173\n",
        )
        .await;
    let vm_id = runtime.vm_id();
    let task_id = running_preview(&app, &workspace, owner.id, vm_id).await;
    // The helper journaled the readiness event as seq 1; the attempt row
    // acknowledges it so the guest's outcome can follow as seq 2.
    sqlx::query(
        "INSERT INTO task_attempts(
             org_id, task_id, attempt, vm_id, epoch, start_confirmed, started_at,
             acked_event_seq
         ) VALUES ($1,$2,1,$3,1,true,now(),1)",
    )
    .bind(org.id)
    .bind(task_id)
    .bind(vm_id.to_string())
    .execute(&app.db)
    .await
    .expect("insert running preview attempt");
    let client = app.authenticated_client(&owner);
    let preview_url = format!("/api/v1/workspaces/{}/preview", workspace.id);

    let before = client.get(&preview_url).await;
    let before: Value = before.json().await.unwrap_or(Value::Null);
    assert_eq!(before["data"]["status"], "running");
    assert_eq!(before["data"]["closing"], false);

    let close = client
        .post_json(&format!("{preview_url}/cancel"), &json!({}))
        .await;
    assert_eq!(close.status(), StatusCode::OK);
    let closing = client.get(&preview_url).await;
    let closing: Value = closing.json().await.unwrap_or(Value::Null);
    assert_eq!(closing["data"]["status"], "running", "{closing}");
    assert_eq!(closing["data"]["closing"], true, "{closing}");
    assert!(closing["data"]["close"].is_null());
    let mint = client
        .post_json(&format!("/api/v1/preview/session/{task_id}"), &json!({}))
        .await;
    assert_eq!(
        mint.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "a closing preview admits no new stream"
    );

    let during_close = client
        .post_json(
            &preview_url,
            &json!({"repoId": workspace.repo_id, "idempotencyKey": "preview-while-closing"}),
        )
        .await;
    assert_eq!(
        during_close.status(),
        StatusCode::CONFLICT,
        "a closing preview still refuses a new start"
    );

    apply_agent_attempt_event(
        &app,
        org.id,
        task_id,
        1,
        vm_id,
        1,
        2,
        "invocation.outcome",
        json!({"status":"cancelled","detail":{"close":"failed"}}),
        true,
    )
    .await;
    let closed = client.get(&preview_url).await;
    let closed: Value = closed.json().await.unwrap_or(Value::Null);
    assert_eq!(closed["data"]["status"], "cancelled", "{closed}");
    assert_eq!(closed["data"]["closing"], false);
    assert_eq!(
        closed["data"]["close"], "failed",
        "the guest's close classification reaches the owner: {closed}"
    );

    let after = client
        .post_json(
            &preview_url,
            &json!({"repoId": workspace.repo_id, "idempotencyKey": "preview-after-close"}),
        )
        .await;
    assert_eq!(
        after.status(),
        StatusCode::OK,
        "a settled preview frees the slot"
    );
    app.cleanup().await;
}

#[tokio::test]
async fn active_attempt_reconciliation_rotates_past_the_first_batch() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = helpers::TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    for index in 0..33 {
        let task_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO tasks(
                 id, org_id, workspace_id, name, kind, status, spec, attempt,
                 vm_id, epoch, start_confirmed, started_at, created_by
             ) VALUES ($1,$2,$3,$4,'command','running','{}'::jsonb,1,
                       $5,1,true,now(),$6)",
        )
        .bind(task_id)
        .bind(org.id)
        .bind(workspace.id)
        .bind(format!("Active attempt {index}"))
        .bind(runtime.vm_id().to_string())
        .bind(owner.id)
        .execute(&app.db)
        .await
        .expect("insert active task");
        sqlx::query(
            "INSERT INTO task_attempts(
                 org_id, task_id, attempt, vm_id, epoch, start_confirmed, started_at
             ) VALUES ($1,$2,1,$3,1,true,now())",
        )
        .bind(org.id)
        .bind(task_id)
        .bind(runtime.vm_id().to_string())
        .execute(&app.db)
        .await
        .expect("insert active task attempt");
    }

    g5_api_core::maintenance::agent_tasks::run_once(&app.state)
        .await
        .expect("run first reconciliation batch");
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM task_attempts
             WHERE last_reconciled_at > '-infinity'::timestamptz",
        )
        .fetch_one(&app.db)
        .await
        .expect("count first reconciliation batch"),
        32
    );

    g5_api_core::maintenance::agent_tasks::run_once(&app.state)
        .await
        .expect("run rotating reconciliation batch");
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM task_attempts
             WHERE last_reconciled_at > '-infinity'::timestamptz",
        )
        .fetch_one(&app.db)
        .await
        .expect("count rotated reconciliation batch"),
        33
    );

    app.cleanup().await;
}

/// A queued command whose spec dispatch cannot run fails on its own; it no
/// longer ends the dispatch pass and so blocks no other work in the
/// organization.
#[tokio::test]
async fn unrunnable_command_spec_fails_its_task_without_blocking_agent_dispatch() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = helpers::TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let bad_command = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO tasks(
             id, org_id, workspace_id, name, kind, status, spec, attempt, created_by
         ) VALUES ($1,$2,$3,'Unrunnable command','command','queued',$4,1,$5)",
    )
    .bind(bad_command)
    .bind(workspace.org_id)
    .bind(workspace.id)
    .bind(json!({
        "command": {
            "schema_version": "g5.command-task-spec/v1",
            "module_id": "shell.exec",
            "module_version": "1.0.0",
            "operation": "run",
            "input_schema": "g5.workspace.bootstrap.run/v1",
            "input": {}
        }
    }))
    .bind(owner.id)
    .execute(&app.db)
    .await
    .expect("insert unrunnable command");
    let (agent_task, session_id) =
        create_agent_task(&app, &workspace, owner.id, "queued", None, None).await;
    queue_session_input(&app, workspace.org_id, session_id, agent_task, "Hello").await;

    g5_api_core::maintenance::agent_tasks::run_once(&app.state)
        .await
        .expect("run task maintenance");

    let command = sqlx::query_as::<_, (String, Option<String>)>(
        "SELECT status, outcome ->> 'reason' FROM tasks WHERE id = $1",
    )
    .bind(bad_command)
    .fetch_one(&app.db)
    .await
    .expect("load the command");
    assert_eq!(
        command,
        (
            "failed".to_string(),
            Some("dispatch_admission_failed".to_string())
        )
    );
    let agent_status: String = sqlx::query_scalar("SELECT status FROM tasks WHERE id = $1")
        .bind(agent_task)
        .fetch_one(&app.db)
        .await
        .expect("load the agent task");
    assert_eq!(agent_status, "running", "the agent task was dispatched");
    assert!(runtime
        .received_rpc_methods()
        .await
        .iter()
        .any(|method| method.as_str() == "modules.invoke"));

    app.cleanup().await;
}

/// The dispatch pass settles an unrunnable command and then reserves an agent
/// turn in the same transaction, and the settlement can wait for a dependent
/// task's row. The reservation compares the VM lease with the clock, so a
/// lease that ended during that wait reserves nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn dispatch_checks_the_lease_after_its_pass_waits() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = helpers::TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let unrunnable = json!({
        "command": {
            "schema_version": "g5.command-task-spec/v1",
            "module_id": "shell.exec",
            "module_version": "1.0.0",
            "operation": "run",
            "input_schema": "g5.workspace.bootstrap.run/v1",
            "input": {}
        }
    });
    let (bad_command, dependent) = (Uuid::new_v4(), Uuid::new_v4());
    for (task_id, name) in [
        (bad_command, "Unrunnable command"),
        (dependent, "Waits on the command"),
    ] {
        sqlx::query(
            "INSERT INTO tasks(
                 id, org_id, workspace_id, name, kind, status, spec, attempt, created_by
             ) VALUES ($1,$2,$3,$4,'command','queued',$5,1,$6)",
        )
        .bind(task_id)
        .bind(workspace.org_id)
        .bind(workspace.id)
        .bind(name)
        .bind(&unrunnable)
        .bind(owner.id)
        .execute(&app.db)
        .await
        .expect("insert command");
    }
    sqlx::query(
        "INSERT INTO task_dependencies(org_id, workspace_id, task_id, depends_on_task_id)
         VALUES ($1,$2,$3,$4)",
    )
    .bind(workspace.org_id)
    .bind(workspace.id)
    .bind(dependent)
    .bind(bad_command)
    .execute(&app.db)
    .await
    .expect("insert dependency");
    let (agent_task, session_id) =
        create_agent_task(&app, &workspace, owner.id, "queued", None, None).await;
    queue_session_input(&app, workspace.org_id, session_id, agent_task, "Hello").await;

    let holder = hold_while_the_lease_ends(
        &app,
        workspace.id,
        "SELECT 1 FROM tasks
         WHERE workspace_id = $1 AND name = 'Waits on the command' FOR UPDATE",
    )
    .await;
    let pass = g5_api_core::maintenance::agent_tasks::run_once(&app.state);
    let release = async {
        // The settlement's update of the dependent tasks.
        wait_for_lock_wait_past_the_lease(&app, workspace.id, "WITH RECURSIVE blocked").await;
        holder.rollback().await.expect("release the dependent task");
    };
    let (ran, ()) = tokio::join!(pass, release);
    ran.expect("run task maintenance");

    let statuses = sqlx::query_as::<_, (String, String)>(
        "SELECT
             (SELECT status FROM tasks WHERE id = $1),
             (SELECT status FROM tasks WHERE id = $2)",
    )
    .bind(bad_command)
    .bind(agent_task)
    .fetch_one(&app.db)
    .await
    .expect("load the tasks");
    assert_eq!(
        statuses,
        ("failed".to_string(), "queued".to_string()),
        "the command failed and the agent turn was not reserved"
    );
    assert!(!runtime
        .received_rpc_methods()
        .await
        .iter()
        .any(|method| method.as_str() == "modules.invoke"));
    app.cleanup().await;
}

/// A completed agent turn records its usage once: a `usage_records` row and
/// the task and session counters, with the provider session state on the
/// session. Written before usage and transcript writes moved to their owners,
/// so it pins what the move must keep.
#[tokio::test]
async fn completed_turn_records_usage_counters_and_provider_session_state() {
    use sqlx::Row;

    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let vm_id = attach_running_vm_without_runtime(&app, &workspace).await;
    let (task_id, session_id) =
        create_agent_task(&app, &workspace, owner.id, "queued", None, None).await;
    let input_id = queue_session_input(&app, org.id, session_id, task_id, "Summarize").await;
    let reservation_id = bind_running_attempt(&app, org.id, task_id, vm_id, input_id).await;
    apply_agent_event(
        &app,
        org.id,
        (task_id, vm_id),
        1,
        "input.accepted",
        json!({
            "input_id": input_id,
            "reservation_id": reservation_id,
            "revision": 1,
            "delivery_generation": 1,
            "system_notice_ids": [],
        }),
        false,
    )
    .await;
    apply_agent_event(
        &app,
        org.id,
        (task_id, vm_id),
        2,
        "invocation.outcome",
        json!({
            "status": "succeeded",
            "result": {
                "status": "succeeded",
                "provider_session_id": "provider-session-1",
                "session_manifest_generation": 3,
                "usage": {
                    "input_tokens": 120,
                    "output_tokens": 45,
                    "cache_creation_input_tokens": 7,
                    "cache_read_input_tokens": 11,
                    "total_cost_usd": 0.25,
                    "model": "claude-test",
                    "finish_reason": "end_turn",
                    "duration_ms": 1500
                }
            }
        }),
        true,
    )
    .await;

    let usage = sqlx::query(
        "SELECT org_id, workspace_id, session_id, provider, model,
                input_tokens, output_tokens, total_tokens, cache_creation_tokens,
                cache_read_tokens, cost_usd::text AS cost, request_type,
                finish_reason, duration_ms
         FROM usage_records WHERE task_id = $1",
    )
    .bind(task_id)
    .fetch_all(&app.db)
    .await
    .expect("usage rows");
    assert_eq!(usage.len(), 1, "one usage row per completed turn");
    let row = &usage[0];
    assert_eq!(row.get::<Uuid, _>("org_id"), org.id);
    assert_eq!(
        row.get::<Option<Uuid>, _>("workspace_id"),
        Some(workspace.id)
    );
    assert_eq!(row.get::<Option<Uuid>, _>("session_id"), Some(session_id));
    assert_eq!(row.get::<String, _>("provider"), "unknown");
    assert_eq!(row.get::<String, _>("model"), "claude-test");
    assert_eq!(row.get::<i64, _>("input_tokens"), 120);
    assert_eq!(row.get::<i64, _>("output_tokens"), 45);
    assert_eq!(row.get::<i64, _>("total_tokens"), 183);
    assert_eq!(row.get::<i64, _>("cache_creation_tokens"), 7);
    assert_eq!(row.get::<i64, _>("cache_read_tokens"), 11);
    assert_eq!(row.get::<String, _>("cost").parse::<f64>().unwrap(), 0.25);
    assert_eq!(row.get::<String, _>("request_type"), "chat");
    assert_eq!(
        row.get::<Option<String>, _>("finish_reason").as_deref(),
        Some("end_turn")
    );
    assert_eq!(row.get::<Option<i32>, _>("duration_ms"), Some(1500));

    for (table, id) in [("tasks", task_id)] {
        let counters: (i64, i64, String) = sqlx::query_as(&format!(
            "SELECT input_tokens::bigint, output_tokens::bigint, cost_usd::text FROM {table} WHERE id = $1"
        ))
        .bind(id)
        .fetch_one(&app.db)
        .await
        .expect("counters");
        assert_eq!(
            (counters.0, counters.1, counters.2.parse::<f64>().unwrap()),
            (120, 45, 0.25),
            "{table} counters"
        );
    }
    let session: (Option<String>, i32, i64, i64, String) = sqlx::query_as(
        "SELECT provider_session_id, session_manifest_generation,
                total_input_tokens::bigint, total_output_tokens::bigint,
                total_cost_usd::text
         FROM chat_sessions WHERE id = $1",
    )
    .bind(session_id)
    .fetch_one(&app.db)
    .await
    .expect("session counters");
    assert_eq!(session.0.as_deref(), Some("provider-session-1"));
    assert_eq!(session.1, 3);
    assert_eq!((session.2, session.3), (120, 45));
    assert_eq!(
        session.4.parse::<f64>().unwrap(),
        0.0,
        "the turn writer has never added cost to the session"
    );

    app.cleanup().await;
}

/// Bind a queued agent task to the running VM and reserve its input the way
/// dispatch does, so an `input.accepted` event is admissible.
async fn bind_running_attempt(
    app: &TestApp,
    org_id: Uuid,
    task_id: Uuid,
    vm_id: Uuid,
    input_id: Uuid,
) -> Uuid {
    let reservation_id = Uuid::new_v4();
    sqlx::query(
        "UPDATE tasks
         SET status = 'running', vm_id = $2, epoch = 1,
             start_confirmed = true, started_at = now(), updated_at = now()
         WHERE id = $1",
    )
    .bind(task_id)
    .bind(vm_id.to_string())
    .execute(&app.db)
    .await
    .expect("bind running agent task");
    sqlx::query(
        "INSERT INTO task_attempts(
             org_id, task_id, attempt, vm_id, epoch, start_confirmed
         ) VALUES ($1,$2,1,$3,1,true)",
    )
    .bind(org_id)
    .bind(task_id)
    .bind(vm_id.to_string())
    .execute(&app.db)
    .await
    .expect("insert running agent attempt");
    sqlx::query(
        "UPDATE session_inputs
         SET delivery = 'reserved', reservation_id = $2,
             reservation_revision = revision, delivery_generation = 1,
             task_id = $3, attempt = 1
         WHERE id = $1",
    )
    .bind(input_id)
    .bind(reservation_id)
    .bind(task_id)
    .execute(&app.db)
    .await
    .expect("reserve input for running attempt");
    reservation_id
}

async fn session_title(app: &TestApp, session_id: Uuid) -> Option<String> {
    sqlx::query_scalar("SELECT title FROM chat_sessions WHERE id = $1")
        .bind(session_id)
        .fetch_one(&app.db)
        .await
        .expect("load session title")
}

/// Sessions created bare (no `/start`) are named when their first user
/// message is delivered, and the turn's `session.title` summary then replaces
/// that provisional name. A title the user chose is never overwritten.
#[tokio::test]
async fn first_delivery_names_an_unnamed_session_and_the_turn_summary_replaces_it() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let vm_id = attach_running_vm_without_runtime(&app, &workspace).await;
    let (task_id, session_id) =
        create_agent_task(&app, &workspace, owner.id, "queued", None, None).await;
    sqlx::query("UPDATE chat_sessions SET title = NULL WHERE id = $1")
        .bind(session_id)
        .execute(&app.db)
        .await
        .expect("clear the fixture title");
    let input_id = queue_session_input(
        &app,
        org.id,
        session_id,
        task_id,
        "Why does the preview tab open blank? It worked yesterday.",
    )
    .await;
    let reservation_id = bind_running_attempt(&app, org.id, task_id, vm_id, input_id).await;

    apply_agent_event(
        &app,
        org.id,
        (task_id, vm_id),
        1,
        "input.accepted",
        json!({
            "input_id": input_id,
            "reservation_id": reservation_id,
            "revision": 1,
            "delivery_generation": 1,
            "system_notice_ids": [],
        }),
        false,
    )
    .await;
    assert_eq!(
        session_title(&app, session_id).await.as_deref(),
        Some("Why does the preview tab open blank?"),
        "delivery names an unnamed session from its first user message"
    );

    // Unusable summaries are ignored, not applied and not fatal.
    apply_agent_event(
        &app,
        org.id,
        (task_id, vm_id),
        2,
        "session.title",
        json!({"title": "  \"\"  ", "source": "provider_summary"}),
        false,
    )
    .await;
    assert_eq!(
        session_title(&app, session_id).await.as_deref(),
        Some("Why does the preview tab open blank?")
    );

    apply_agent_event(
        &app,
        org.id,
        (task_id, vm_id),
        3,
        "session.title",
        json!({"title": "\"Diagnose blank preview tab.\"\n", "source": "provider_summary"}),
        false,
    )
    .await;
    assert_eq!(
        session_title(&app, session_id).await.as_deref(),
        Some("Diagnose blank preview tab"),
        "the provider summary replaces the provisional title"
    );

    // A later summary must not clobber a title the user chose meanwhile.
    sqlx::query("UPDATE chat_sessions SET title = 'My renamed session' WHERE id = $1")
        .bind(session_id)
        .execute(&app.db)
        .await
        .expect("user rename");
    apply_agent_event(
        &app,
        org.id,
        (task_id, vm_id),
        4,
        "session.title",
        json!({"title": "Another summary", "source": "provider_summary"}),
        false,
    )
    .await;
    assert_eq!(
        session_title(&app, session_id).await.as_deref(),
        Some("My renamed session")
    );

    // A title the user cleared is an explicit choice too: neither the
    // summary nor a later delivery may fill it back in.
    sqlx::query("UPDATE chat_sessions SET title = NULL, title_source = 'user' WHERE id = $1")
        .bind(session_id)
        .execute(&app.db)
        .await
        .expect("user clears the title");
    apply_agent_event(
        &app,
        org.id,
        (task_id, vm_id),
        5,
        "session.title",
        json!({"title": "Yet another summary", "source": "provider_summary"}),
        false,
    )
    .await;
    assert_eq!(session_title(&app, session_id).await, None);

    app.cleanup().await;
}

/// A session that already carries a user-chosen title keeps it through
/// delivery; only `NULL` is filled in.
#[tokio::test]
async fn first_delivery_leaves_a_named_session_alone() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let vm_id = attach_running_vm_without_runtime(&app, &workspace).await;
    let (task_id, session_id) =
        create_agent_task(&app, &workspace, owner.id, "queued", None, None).await;
    let input_id = queue_session_input(&app, org.id, session_id, task_id, "What changed?").await;
    let reservation_id = bind_running_attempt(&app, org.id, task_id, vm_id, input_id).await;
    apply_agent_event(
        &app,
        org.id,
        (task_id, vm_id),
        1,
        "input.accepted",
        json!({
            "input_id": input_id,
            "reservation_id": reservation_id,
            "revision": 1,
            "delivery_generation": 1,
            "system_notice_ids": [],
        }),
        false,
    )
    .await;
    assert_eq!(
        session_title(&app, session_id).await.as_deref(),
        Some("Task lifecycle test")
    );

    app.cleanup().await;
}

/// Applying a running preview's readiness and the owner cancelling it both
/// lock the preview task before the VM row, so they cannot deadlock. The task
/// row is held while the readiness and then the cancellation queue on it.
#[tokio::test]
async fn idle_interrupted_preview_records_why_it_ended() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = helpers::TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let vm_id = runtime.vm_id();
    let task_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO tasks(
             id, org_id, workspace_id, name, kind, status, spec, attempt,
             vm_id, epoch, start_confirmed, started_at, created_by
         ) VALUES ($1,$2,$3,'Preview','preview','running',
                   jsonb_build_object('lifecycle', jsonb_build_object(
                       'interrupt_reason','idle',
                       'interrupt_operation_id',$6::text,
                       'interrupt_requested_at',now())),
                   1,$4,1,true,now(),$5)",
    )
    .bind(task_id)
    .bind(org.id)
    .bind(workspace.id)
    .bind(vm_id.to_string())
    .bind(owner.id)
    .bind(Uuid::new_v4())
    .execute(&app.db)
    .await
    .expect("insert an idle-interrupted preview");
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
    .expect("insert the preview attempt");
    sqlx::query(
        "UPDATE workspace_vms
         SET preview_generation = 1,
             preview_intent = jsonb_build_object(
                 'generation',1,'action','start','status','queued',
                 'requested_by',$2::text,'task_id',$3::text)
         WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .bind(owner.id)
    .bind(task_id)
    .execute(&app.db)
    .await
    .expect("bind the preview intent");

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
                seq: 1,
                kind: "invocation.outcome".to_string(),
                // A null reason is as absent as no reason at all.
                payload: json!({"status": "cancelled", "detail": {}, "reason": null}),
                terminal: true,
            }),
        )
        .await
        .expect("settle the interrupted preview");
    db.commit().await.expect("commit settlement");

    let (status, outcome): (String, Value) =
        sqlx::query_as("SELECT status, outcome FROM tasks WHERE id = $1")
            .bind(task_id)
            .fetch_one(&app.db)
            .await
            .expect("load the settled preview");
    assert_eq!(status, "cancelled");
    assert_eq!(outcome["status"], "cancelled");
    assert_eq!(
        outcome["reason"], "idle_stop",
        "the task outcome says the idle stop ended the preview"
    );
    let attempt_outcome: Value =
        sqlx::query_scalar("SELECT outcome FROM task_attempts WHERE task_id = $1 AND attempt = 1")
            .bind(task_id)
            .fetch_one(&app.db)
            .await
            .expect("load the attempt outcome");
    assert!(
        attempt_outcome["reason"].is_null() && attempt_outcome["detail"].is_object(),
        "the attempt keeps the module's raw payload"
    );
    let intent_status: String = sqlx::query_scalar(
        "SELECT preview_intent ->> 'status' FROM workspace_vms WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .expect("load the preview intent");
    assert_eq!(intent_status, "settled");

    let activity: Value = app
        .authenticated_client(&owner)
        .get(&format!("/api/v1/workspaces/{}/activity", workspace.id))
        .await
        .json()
        .await
        .expect("activity response body");
    let entries = activity["data"]["entries"].clone();
    let entry = entries
        .as_array()
        .expect("activity entries")
        .iter()
        .find(|entry| entry["key"] == format!("task:{task_id}"))
        .expect("the preview appears in the activity view");
    assert_eq!(entry["status"], "cancelled");
    assert_eq!(entry["reason"], "idle_stop");

    app.cleanup().await;
}

/// `g5.preview.service.exit-diagnostics`: the guest's failure outcome (the
/// app's status and stderr tail in `result`) is persisted verbatim on the
/// attempt, the task fails, and the activity view reports the failure.
#[tokio::test]
async fn failed_preview_app_outcome_keeps_its_stderr_tail() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = helpers::TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let vm_id = runtime.vm_id();
    let task_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO tasks(
             id, org_id, workspace_id, name, kind, status, spec, attempt,
             vm_id, epoch, start_confirmed, started_at, created_by
         ) VALUES ($1,$2,$3,'Preview','preview','running','{}'::jsonb,
                   1,$4,1,true,now(),$5)",
    )
    .bind(task_id)
    .bind(org.id)
    .bind(workspace.id)
    .bind(vm_id.to_string())
    .bind(owner.id)
    .execute(&app.db)
    .await
    .expect("insert a running preview");
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
    .expect("insert the preview attempt");
    sqlx::query(
        "UPDATE workspace_vms
         SET preview_generation = 1,
             preview_intent = jsonb_build_object(
                 'generation',1,'action','start','status','queued',
                 'requested_by',$2::text,'task_id',$3::text)
         WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .bind(owner.id)
    .bind(task_id)
    .execute(&app.db)
    .await
    .expect("bind the preview intent");

    let tail = "error: failed to open registry/cache/rustls-0.23.46.crate\nCaused by: Permission denied (os error 13)\n";
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
                seq: 1,
                kind: "invocation.outcome".to_string(),
                payload: json!({
                    "status": "failed",
                    "exit_code": 1,
                    "result": {
                        "status": "app_exited",
                        "app_exit_code": 7,
                        "app_signal": null,
                        "stderr_tail": tail,
                        "stderr_tail_truncated": false,
                        "message": "preview app exited with status Some(7)",
                    }
                }),
                terminal: true,
            }),
        )
        .await
        .expect("settle the failed preview");
    db.commit().await.expect("commit settlement");

    let (status, outcome): (String, Value) =
        sqlx::query_as("SELECT status, outcome FROM tasks WHERE id = $1")
            .bind(task_id)
            .fetch_one(&app.db)
            .await
            .expect("load the settled preview");
    assert_eq!(status, "failed");
    assert_eq!(outcome["status"], "failed");
    let attempt_outcome: Value =
        sqlx::query_scalar("SELECT outcome FROM task_attempts WHERE task_id = $1 AND attempt = 1")
            .bind(task_id)
            .fetch_one(&app.db)
            .await
            .expect("load the attempt outcome");
    assert_eq!(attempt_outcome["status"], "failed");
    assert_eq!(attempt_outcome["exit_code"], 1);
    assert_eq!(attempt_outcome["result"]["status"], "app_exited");
    assert_eq!(attempt_outcome["result"]["app_exit_code"], 7);
    assert_eq!(
        attempt_outcome["result"]["stderr_tail"], tail,
        "the guest's stderr tail is persisted verbatim on the attempt"
    );
    assert_eq!(attempt_outcome["result"]["stderr_tail_truncated"], false);
    let intent_status: String = sqlx::query_scalar(
        "SELECT preview_intent ->> 'status' FROM workspace_vms WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .expect("load the preview intent");
    assert_eq!(intent_status, "settled");

    let activity: Value = app
        .authenticated_client(&owner)
        .get(&format!("/api/v1/workspaces/{}/activity", workspace.id))
        .await
        .json()
        .await
        .expect("activity response body");
    let entry = activity["data"]["entries"]
        .as_array()
        .expect("activity entries")
        .iter()
        .find(|entry| entry["key"] == format!("task:{task_id}"))
        .cloned()
        .expect("the preview appears in the activity view");
    assert_eq!(entry["status"], "failed");

    app.cleanup().await;
}

#[tokio::test]
async fn preview_cancelled_before_its_start_records_why_it_ended() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = helpers::TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let vm_id = runtime.vm_id();
    let task_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO tasks(
             id, org_id, workspace_id, name, kind, status, spec, attempt,
             vm_id, epoch, start_confirmed, started_at, created_by
         ) VALUES ($1,$2,$3,'Preview','preview','running',
                   jsonb_build_object('lifecycle', jsonb_build_object(
                       'interrupt_reason','idle','interrupt_requested_at',now())),
                   1,$4,1,false,now(),$5)",
    )
    .bind(task_id)
    .bind(org.id)
    .bind(workspace.id)
    .bind(vm_id.to_string())
    .bind(owner.id)
    .execute(&app.db)
    .await
    .expect("insert an unconfirmed preview interrupted by an idle stop");
    sqlx::query(
        "INSERT INTO task_attempts(
             org_id, task_id, attempt, vm_id, epoch, start_confirmed, started_at
         ) VALUES ($1,$2,1,$3,1,false,now())",
    )
    .bind(org.id)
    .bind(task_id)
    .bind(vm_id.to_string())
    .execute(&app.db)
    .await
    .expect("insert the preview attempt");

    let mut db = app.scoped_db_for_org(org.id).await;
    app.state
        .task_service
        .apply(
            &mut db,
            TaskLifecycleEvent::StartDisposition {
                task_id,
                attempt: 1,
                vm_id,
                epoch: 1,
                disposition: json!({"disposition": "cancelled"}),
            },
        )
        .await
        .expect("settle the cancelled start");
    db.commit().await.expect("commit settlement");

    let (status, outcome): (String, Value) =
        sqlx::query_as("SELECT status, outcome FROM tasks WHERE id = $1")
            .bind(task_id)
            .fetch_one(&app.db)
            .await
            .expect("load the settled preview");
    assert_eq!(status, "cancelled");
    assert_eq!(outcome["status"], "cancelled");
    assert_eq!(outcome["reason"], "idle_stop");
    let attempt_outcome: Value =
        sqlx::query_scalar("SELECT outcome FROM task_attempts WHERE task_id = $1 AND attempt = 1")
            .bind(task_id)
            .fetch_one(&app.db)
            .await
            .expect("load the attempt outcome");
    assert_eq!(attempt_outcome["disposition"], "cancelled");

    app.cleanup().await;
}

#[tokio::test]
async fn preview_lost_with_its_vm_records_why_it_ended() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = helpers::TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let vm_id = runtime.vm_id();
    let mut seeded = Vec::new();
    for lifecycle in [
        json!({}),
        json!({"lifecycle": {"interrupt_reason": "idle", "interrupt_requested_at": "2026-10-06T19:33:39Z"}}),
        json!({"lifecycle": {"cancel_requested": true}}),
    ] {
        let task_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO tasks(
                 id, org_id, workspace_id, name, kind, status, spec, attempt,
                 vm_id, epoch, start_confirmed, started_at, created_by
             ) VALUES ($1,$2,$3,'Preview','preview','running',$6,1,$4,1,true,now(),$5)",
        )
        .bind(task_id)
        .bind(org.id)
        .bind(workspace.id)
        .bind(vm_id.to_string())
        .bind(owner.id)
        .bind(&lifecycle)
        .execute(&app.db)
        .await
        .expect("insert a running preview");
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
        .expect("insert the preview attempt");
        seeded.push(task_id);
    }

    for task_id in &seeded {
        let mut db = app.scoped_db_for_org(org.id).await;
        app.state
            .task_service
            .apply(
                &mut db,
                TaskLifecycleEvent::VmLost {
                    task_id: *task_id,
                    attempt: 1,
                    vm_id,
                    epoch: 1,
                    evidence: json!({"kind": "exited"}),
                },
            )
            .await
            .expect("settle the lost preview");
        db.commit().await.expect("commit the loss");
    }

    for (task_id, expected_reason) in seeded.iter().zip(["vm_lost", "idle_stop"]) {
        let (status, outcome): (String, Value) =
            sqlx::query_as("SELECT status, outcome FROM tasks WHERE id = $1")
                .bind(task_id)
                .fetch_one(&app.db)
                .await
                .expect("load the lost preview");
        assert_eq!(status, "failed");
        assert_eq!(outcome["status"], "failed");
        assert_eq!(
            outcome["reason"], expected_reason,
            "a lost preview's task outcome says why it ended"
        );
    }
    // A Close under way ends cancelled, and the outcome records that the VM
    // was lost before cleanup finished (`g5.ui.activity.preview-row`).
    let (status, outcome): (String, Value) =
        sqlx::query_as("SELECT status, outcome FROM tasks WHERE id = $1")
            .bind(seeded[2])
            .fetch_one(&app.db)
            .await
            .expect("load the lost closing preview");
    assert_eq!(status, "cancelled");
    assert_eq!(outcome["status"], "cancelled");
    assert_eq!(outcome["reason"], "owner_cancel");
    assert_eq!(outcome["detail"]["close"], "lost");

    app.cleanup().await;
}

#[tokio::test]
async fn running_preview_cancellation_and_its_readiness_do_not_deadlock() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = helpers::TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let vm_id = runtime.vm_id();
    let task_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO tasks(
             id, org_id, workspace_id, name, kind, status, spec, attempt,
             vm_id, epoch, start_confirmed, started_at, created_by
         ) VALUES ($1,$2,$3,'Running preview','preview','running','{}'::jsonb,1,
                   $4,1,true,now(),$5)",
    )
    .bind(task_id)
    .bind(org.id)
    .bind(workspace.id)
    .bind(vm_id.to_string())
    .bind(owner.id)
    .execute(&app.db)
    .await
    .expect("insert running preview");
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
    .expect("insert running preview attempt");
    sqlx::query(
        "UPDATE workspace_vms
         SET preview_generation = 1,
             preview_intent = jsonb_build_object(
                 'generation',1,'action','start','status','starting',
                 'requested_by',$2::text,'task_id',$3::text)
         WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .bind(owner.id)
    .bind(task_id)
    .execute(&app.db)
    .await
    .expect("bind the preview intent");
    let lock_waiters = |expected: i64| {
        let db = app.db.clone();
        async move {
            tokio::time::timeout(std::time::Duration::from_secs(10), async {
                while sqlx::query_scalar::<_, i64>(
                    "SELECT count(*) FROM pg_stat_activity
                     WHERE datname = current_database() AND wait_event_type = 'Lock'",
                )
                .fetch_one(&db)
                .await
                .unwrap()
                    < expected
                {
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap_or_else(|_| panic!("{expected} transactions wait for the task row"));
        }
    };

    let mut holder = app.db.begin().await.expect("begin task row holder");
    sqlx::query("SELECT 1 FROM tasks WHERE id = $1 FOR UPDATE")
        .bind(task_id)
        .execute(&mut *holder)
        .await
        .expect("hold the preview task row");
    let mut db = app.scoped_db_for_org(org.id).await;
    let service = app.state.task_service.clone();
    let readiness = tokio::spawn(async move {
        service
            .apply(
                &mut db,
                TaskLifecycleEvent::Journal(JournalTaskEvent {
                    task_id,
                    attempt: 1,
                    vm_id,
                    epoch: 1,
                    exec_id: canonical_exec_id(task_id, 1, vm_id),
                    seq: 1,
                    kind: "module.preview.ready".to_string(),
                    payload: json!({}),
                    terminal: false,
                }),
            )
            .await
            .map_err(|error| format!("{error:?}"))?;
        db.commit().await.map_err(|error| format!("{error:?}"))
    });
    lock_waiters(1).await;
    let client = app.authenticated_client(&owner);
    let cancel = tokio::spawn(async move {
        let response = client
            .post_json(&format!("/api/v1/tasks/{task_id}/cancel"), &json!({}))
            .await;
        let status = response.status();
        (status, response.text().await.unwrap_or_default())
    });
    lock_waiters(2).await;
    holder.rollback().await.expect("free the preview task row");

    tokio::time::timeout(std::time::Duration::from_secs(30), readiness)
        .await
        .expect("the readiness finishes")
        .expect("join the readiness")
        .expect("the readiness is not a deadlock victim");
    let (status, body) = tokio::time::timeout(std::time::Duration::from_secs(30), cancel)
        .await
        .expect("the cancellation finishes")
        .expect("join the cancellation");
    assert_eq!(
        status,
        StatusCode::OK,
        "the cancellation is not a deadlock victim: {body}"
    );
    app.cleanup().await;
}

/// The dispatch coordinator publishes a journal event's live updates from the
/// organization's own rows: the live timeline row names its placeholder and
/// carries the id the stored row gets, and the chat channel sees the same
/// row (`g5.chat.live-delivery`, `g5.chat.replay`).
#[tokio::test]
async fn live_agent_events_are_published_before_storage_with_the_stored_identity() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = helpers::TestWorkspaceRuntimePeer::attach_with_journal_events(
        &app,
        &workspace,
        owner.id,
        vec![json!({
            "seq": 1,
            "kind": "module.message.delta",
            "payload": {"text_delta": "Hello"},
        })],
    )
    .await;
    let session_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO chat_sessions(id, org_id, workspace_id, scope_type, title, created_by)
         VALUES ($1,$2,$3,'workspace','Live',$4)",
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
         ) VALUES ($1,$2,$3,'Live turn','agent','running','{}'::jsonb,1,
                   $4,1,false,now(),$5,$6)",
    )
    .bind(task_id)
    .bind(org.id)
    .bind(workspace.id)
    .bind(runtime.vm_id().to_string())
    .bind(session_id)
    .bind(owner.id)
    .execute(&app.db)
    .await
    .expect("insert running agent task");
    sqlx::query(
        "INSERT INTO task_attempts(
             org_id, task_id, attempt, vm_id, epoch, start_confirmed, started_at
         ) VALUES ($1,$2,1,$3,1,false,now())",
    )
    .bind(org.id)
    .bind(task_id)
    .bind(runtime.vm_id().to_string())
    .execute(&app.db)
    .await
    .expect("insert running agent attempt");
    let message_id = canonical_agent_message_id(task_id, 1);
    sqlx::query(
        "INSERT INTO chat_messages(
             id, org_id, session_id, role, content, citations, status,
             message_index, created_at, task_id, attempt)
         VALUES ($1,$2,$3,'assistant','','[]'::jsonb,'pending',4,now(),$4,1)",
    )
    .bind(message_id)
    .bind(org.id)
    .bind(session_id)
    .bind(task_id)
    .execute(&app.db)
    .await
    .expect("insert the attempt's assistant placeholder");
    let mut chat = app.ws_connect(&owner, &format!("chat:{session_id}")).await;
    chat.expect_subscribed().await;
    let mut task = app.ws_connect(&owner, &format!("task:{task_id}")).await;
    task.expect_subscribed().await;

    g5_api_core::maintenance::agent_tasks::run_once(&app.state)
        .await
        .expect("run one coordinator pass");

    // The live copy is published before the row is stored
    // (`g5.chat.live-delivery`): it carries the execution identity, the
    // placeholder it belongs to and the id the stored row will have.
    let timeline = task
        .expect_event("task.timeline", std::time::Duration::from_secs(10))
        .await;
    assert_eq!(timeline["entry"]["metadata"]["live"], true);
    assert_eq!(timeline["entry"]["metadata"]["seq"], 1);
    assert_eq!(timeline["entry"]["metadata"]["attempt"], 1);
    assert_eq!(
        timeline["entry"]["metadata"]["messageId"],
        message_id.to_string()
    );
    assert_eq!(
        timeline["entry"]["metadata"]["payload"]["text_delta"],
        "Hello"
    );
    let live = chat
        .expect_event("chat.timeline", std::time::Duration::from_secs(10))
        .await;
    assert_eq!(live["sessionId"], session_id.to_string());
    assert_eq!(live["taskId"], task_id.to_string());
    assert_eq!(live["entry"]["id"], timeline["entry"]["id"]);
    assert_eq!(live["entry"]["content"], "Hello");
    let exec_id = canonical_exec_id(task_id, 1, runtime.vm_id());
    assert!(
        app.state
            .followers
            .wait_persisted(&exec_id, 1, std::time::Duration::from_secs(15))
            .await,
        "the routine delta is stored within the batch interval"
    );
    let (stored, metadata): (Uuid, Value) =
        sqlx::query_as("SELECT id, metadata FROM task_events WHERE task_id = $1")
            .bind(task_id)
            .fetch_one(&app.db)
            .await
            .expect("load the stored timeline entry");
    assert_eq!(
        timeline["entry"]["id"],
        stored.to_string(),
        "the live copy and the stored row are one fact (`g5.chat.replay`)"
    );
    assert_eq!(metadata["seq"], 1);
    assert_eq!(metadata["messageId"], message_id.to_string());
    assert!(
        metadata.get("live").is_none(),
        "the stored row is not marked live"
    );
    let status: String = sqlx::query_scalar("SELECT status FROM chat_messages WHERE id = $1")
        .bind(message_id)
        .fetch_one(&app.db)
        .await
        .expect("load the placeholder");
    assert_eq!(
        status, "streaming",
        "the batch marks the placeholder streaming"
    );
    app.cleanup().await;
}

/// A running conversation turn on `vm_id` with its assistant placeholder, as
/// dispatch leaves it once the module has started.
async fn seed_running_agent_turn(
    app: &TestApp,
    org: &TestOrg,
    workspace: &TestWorkspace,
    owner: &TestUser,
    vm_id: Uuid,
) -> (Uuid, Uuid) {
    let session_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO chat_sessions(id, org_id, workspace_id, scope_type, title, created_by)
         VALUES ($1,$2,$3,'workspace','Binary output',$4)",
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
         ) VALUES ($1,$2,$3,'Binary output turn','agent','running','{}'::jsonb,1,
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

/// A tool that prints a binary file reports U+0000, which PostgreSQL text and
/// jsonb reject. The coordinator stores and streams such events with U+FFFD
/// and reaches the outcome journaled after them, instead of retrying the
/// rejected event on every pass while the chat shows the tool running
/// (QA 2026-10-06: a Bash command printed an ELF header).
#[tokio::test]
async fn journal_events_carrying_nul_are_stored_and_the_turn_settles() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let elf = "\u{7f}ELF\u{2}\u{1}\u{1}\0\0\0";
    let runtime = helpers::TestWorkspaceRuntimePeer::attach_with_journal_events(
        &app,
        &workspace,
        owner.id,
        vec![
            json!({
                "seq": 1,
                "kind": "module.tool.started",
                "payload": {
                    "call_id": "toolu_1", "name": "Bash", "content": "Used Bash",
                    "input": {"command": "head -c 16 /usr/bin/gcloud"},
                },
            }),
            json!({
                "seq": 2,
                "kind": "module.tool.completed",
                "payload": {
                    "call_id": "toolu_1", "name": "Bash", "status": "ok",
                    "output": elf, "content": elf, "output_truncated": false,
                    "meta\0": ["x\0"],
                },
            }),
            json!({
                "seq": 3,
                "kind": "module.message.delta",
                "payload": {"text_delta": "It starts with \0"},
            }),
            json!({
                "seq": 4,
                "kind": "invocation.outcome",
                "payload": {
                    "status": "succeeded",
                    "result": {
                        "status": "succeeded", "code": "exit_code",
                        "provider_started": true, "detail": "stderr \0",
                    },
                },
                "terminal": true,
            }),
        ],
    )
    .await;
    let (session_id, task_id) =
        seed_running_agent_turn(&app, &org, &workspace, &owner, runtime.vm_id()).await;
    let mut chat = app.ws_connect(&owner, &format!("chat:{session_id}")).await;
    chat.expect_subscribed().await;

    g5_api_core::maintenance::agent_tasks::run_once(&app.state)
        .await
        .expect("run one coordinator pass");
    let exec_id = canonical_exec_id(task_id, 1, runtime.vm_id());
    assert!(
        app.state
            .followers
            .wait_finished(&exec_id, std::time::Duration::from_secs(15))
            .await,
        "the terminal outcome flushes the batch at once and ends the follower"
    );

    let (acked, ended): (i64, bool) = sqlx::query_as(
        "SELECT acked_event_seq, ended_at IS NOT NULL
         FROM task_attempts WHERE task_id = $1 AND attempt = 1",
    )
    .bind(task_id)
    .fetch_one(&app.db)
    .await
    .expect("load the attempt");
    assert_eq!(acked, 4, "every journaled event is stored and acknowledged");
    assert!(
        ended,
        "the outcome behind the binary output settles the attempt"
    );
    let status: String = sqlx::query_scalar("SELECT status FROM tasks WHERE id = $1")
        .bind(task_id)
        .fetch_one(&app.db)
        .await
        .expect("load the task");
    assert_eq!(status, "succeeded");

    let (content, metadata): (String, Value) =
        sqlx::query_as("SELECT content, metadata FROM task_events WHERE task_id = $1 AND seq = 2")
            .bind(task_id)
            .fetch_one(&app.db)
            .await
            .expect("load the tool result");
    let replaced = "\u{7f}ELF\u{2}\u{1}\u{1}\u{FFFD}\u{FFFD}\u{FFFD}";
    assert_eq!(content, replaced);
    assert_eq!(metadata["payload"]["output"], replaced);
    assert_eq!(metadata["payload"]["meta\u{FFFD}"], json!(["x\u{FFFD}"]));
    let detail: Value = sqlx::query_scalar(
        "SELECT metadata -> 'payload' -> 'result' -> 'detail'
         FROM task_events WHERE task_id = $1 AND seq = 4",
    )
    .bind(task_id)
    .fetch_one(&app.db)
    .await
    .expect("load the outcome");
    assert_eq!(detail, "stderr \u{FFFD}");

    let mut delta = chat
        .expect_event("chat.timeline", std::time::Duration::from_secs(10))
        .await;
    while delta["entry"]["metadata"]["kind"] != "message.delta" {
        delta = chat
            .expect_event("chat.timeline", std::time::Duration::from_secs(10))
            .await;
    }
    assert_eq!(
        delta["entry"]["metadata"]["payload"]["text_delta"], "It starts with \u{FFFD}",
        "the live copy matches the stored one"
    );
    assert_eq!(
        runtime.acked_seqs().await.last().copied(),
        Some(4),
        "the committed prefix is acknowledged cumulatively, once per batch"
    );
    app.cleanup().await;
}

/// Dispatcher values outside the journal reach the same tables: a cancelled
/// start disposition stores its dispatcher-supplied reason.
#[tokio::test]
async fn start_disposition_carrying_nul_settles_the_attempt() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let vm_id = Uuid::new_v4();
    let (_, task_id) = seed_running_agent_turn(&app, &org, &workspace, &owner, vm_id).await;

    let mut db = app.scoped_db_for_org(org.id).await;
    app.state
        .task_service
        .apply(
            &mut db,
            TaskLifecycleEvent::StartDisposition {
                task_id,
                attempt: 1,
                vm_id,
                epoch: 1,
                disposition: json!({"disposition": "cancelled", "reason": "spawn \0 failed"}),
            },
        )
        .await
        .expect("apply the disposition");
    db.commit().await.expect("commit the disposition");

    let (outcome, ended): (Value, bool) = sqlx::query_as(
        "SELECT outcome, ended_at IS NOT NULL FROM task_attempts
         WHERE task_id = $1 AND attempt = 1",
    )
    .bind(task_id)
    .fetch_one(&app.db)
    .await
    .expect("load the attempt");
    assert!(ended, "the cancelled start settles the attempt");
    assert_eq!(outcome["reason"], "spawn \u{FFFD} failed");
    app.cleanup().await;
}

/// One coordinator pass reconciles each active attempt once: the top-up
/// collection must not re-claim the attempts the fair-share loop already
/// claimed in the same pass (S9).
#[tokio::test]
async fn one_reconciliation_pass_claims_each_attempt_once() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = helpers::TestWorkspaceRuntimePeer::attach_with_journal_events(
        &app,
        &workspace,
        owner.id,
        vec![json!({
            "seq": 1,
            "kind": "invocation.outcome",
            "payload": {
                "status": "succeeded",
                "result": {"status": "succeeded", "code": "exit_code", "provider_started": true},
            },
            "terminal": true,
        })],
    )
    .await;
    let (_session, task_id) =
        seed_running_agent_turn(&app, &org, &workspace, &owner, runtime.vm_id()).await;

    g5_api_core::maintenance::agent_tasks::run_once(&app.state)
        .await
        .expect("run one coordinator pass");
    let exec_id = canonical_exec_id(task_id, 1, runtime.vm_id());
    assert!(
        app.state
            .followers
            .wait_finished(&exec_id, std::time::Duration::from_secs(15))
            .await,
        "the follower stores the outcome and ends"
    );

    let replays = runtime
        .received_rpc_methods()
        .await
        .iter()
        .filter(|method| method.as_str() == "events.replay")
        .count();
    assert_eq!(replays, 1, "the pass must collect the attempt exactly once");
    let status: String = sqlx::query_scalar("SELECT status FROM tasks WHERE id = $1")
        .bind(task_id)
        .fetch_one(&app.db)
        .await
        .expect("load the task");
    assert_eq!(status, "succeeded");
    app.cleanup().await;
}

/// Catalog negotiation is frozen at reservation, including reconstruction after
/// the available guest changes. This exercises the database reservation owner.
/// A VM whose catalog lacks App Server refuses Codex rather than selecting the
/// retired exec contract (`g5.chat.codex-runtime-refusal`).
#[tokio::test]
async fn codex_app_server_module_and_transport_remain_frozen_on_redispatch() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    for (catalog, expected) in [
        (
            Some("2026.11.10-agent-turn-output-unbounded"),
            Some("1.0.8"),
        ),
        (Some("2026.11.05-approval-report-order"), Some("1.0.8")),
        (Some("2026.10.30-agent-turn-question-gate"), Some("1.0.8")),
        (Some("2026.10.30-approval-summary"), Some("1.0.8")),
        (Some("2026.10.28-codex-formal-app-server"), Some("1.0.8")),
        (Some("2026.10.27-merge-base"), Some("1.0.8")),
        (Some("2026.10.26-workspace-preview-origin"), Some("1.0.8")),
        (Some("2026.10.27-merge-base-preview-origin"), Some("1.0.8")),
        (Some("2026.10.24-codex-app-server"), Some("1.0.8")),
        (Some("2026.10.22-workspace-base-reviews"), None),
        (Some("2099.01.01-unknown"), None),
        (None, None),
    ] {
        let workspace = TestWorkspace::create(&app, &org).await;
        attach_running_vm_without_runtime(&app, &workspace).await;
        sqlx::query("UPDATE workspace_vms SET module_catalog_version=$2 WHERE workspace_id=$1")
            .bind(workspace.id)
            .bind(catalog)
            .execute(&app.db)
            .await
            .unwrap();
        let (task, session) = queue_codex_turn(&app, &org, &workspace, owner.id).await;
        let mut db = app.scoped_db_for_org(org.id).await;
        let first = app
            .state
            .task_service
            .reserve_agent_dispatch(&mut db)
            .await
            .unwrap();
        db.commit().await.unwrap();
        let Some(expected) = expected else {
            assert!(first.is_none(), "{catalog:?} must not dispatch Codex");
            let (status, reason, detail): (String, String, String) = sqlx::query_as(
                "SELECT status, outcome->>'reason', outcome->>'detail' FROM tasks WHERE id=$1",
            )
            .bind(task)
            .fetch_one(&app.db)
            .await
            .unwrap();
            assert_eq!(
                (status.as_str(), reason.as_str()),
                ("failed", "dispatch_admission_failed")
            );
            assert!(detail.contains("Stop and start the workspace"), "{detail}");
            let attempts: i64 =
                sqlx::query_scalar("SELECT count(*) FROM task_attempts WHERE task_id=$1")
                    .bind(task)
                    .fetch_one(&app.db)
                    .await
                    .unwrap();
            assert_eq!(attempts, 0, "{catalog:?} reserved an attempt");
            let notice: String = sqlx::query_scalar(
                "SELECT content FROM chat_messages WHERE session_id=$1 AND is_error_notice",
            )
            .bind(session)
            .fetch_one(&app.db)
            .await
            .unwrap();
            assert!(
                notice.starts_with(
                    "This message was not sent to the agent: this workspace's runtime predates"
                ) && notice.contains("Once corrected, your next message runs it."),
                "{notice}"
            );
            continue;
        };
        let first = first.unwrap();
        assert_eq!(first.invoke_params["version"], expected);
        assert_eq!(
            first.invoke_params["input"]["provider"]["options"]["codex_transport"],
            "app_server_stdio_v1"
        );
        let frozen: Value = sqlx::query_scalar(
            "SELECT policy_snapshot_ref FROM task_attempts WHERE task_id=$1 AND attempt=1",
        )
        .bind(task)
        .fetch_one(&app.db)
        .await
        .unwrap();
        // The VM's recorded catalog changing cannot alter the reservation.
        let retry = redispatch_after_catalog(
            &app,
            &org,
            &workspace,
            task,
            "2026.10.22-workspace-base-reviews",
        )
        .await;
        assert_eq!(
            retry.invoke_params, first.invoke_params,
            "reconstruction cannot upgrade or downgrade a reservation"
        );
        let retained: Value = sqlx::query_scalar(
            "SELECT policy_snapshot_ref FROM task_attempts WHERE task_id=$1 AND attempt=1",
        )
        .bind(task)
        .fetch_one(&app.db)
        .await
        .unwrap();
        assert_eq!(retained, frozen);
        // Stop this attempt so the next workspace's case is selected independently.
        apply_agent_event(&app,org.id,(task,first.vm_id),1,"invocation.outcome",
            json!({"status":"succeeded","result":{"status":"failed","code":"provider_error","input_delivery_unknown":false}}),true).await;
    }
    app.cleanup().await;
}

/// An unconfirmed reservation frozen on exec before retirement re-reaches only
/// its own incarnation, exactly as reserved: it is neither converted to App
/// Server nor refused by the new admission rule (`g5.chat.codex-exec-retirement`).
#[tokio::test]
async fn historical_exec_reservation_reconstructs_without_conversion() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    attach_running_vm_without_runtime(&app, &workspace).await;
    sqlx::query("UPDATE workspace_vms SET module_catalog_version=$2 WHERE workspace_id=$1")
        .bind(workspace.id)
        .bind("2026.10.24-codex-app-server")
        .execute(&app.db)
        .await
        .unwrap();
    let (task, _session) = queue_codex_turn(&app, &org, &workspace, owner.id).await;
    let mut db = app.scoped_db_for_org(org.id).await;
    let first = app
        .state
        .task_service
        .reserve_agent_dispatch(&mut db)
        .await
        .unwrap()
        .unwrap();
    db.commit().await.unwrap();
    // The shape a pre-retirement API froze for an older guest: no transport.
    // Frozen identities are immutable, so seed that historical row with the
    // identity trigger bypassed on this one connection only.
    let mut conn = app.db.acquire().await.unwrap();
    sqlx::query("SET session_replication_role = replica")
        .execute(&mut *conn)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE task_attempts SET policy_snapshot_ref = (policy_snapshot_ref - 'codex_transport')
             || jsonb_build_object('agent_module_version', '1.0.5')
         WHERE task_id=$1 AND attempt=1",
    )
    .bind(task)
    .execute(&mut *conn)
    .await
    .unwrap();
    sqlx::query("SET session_replication_role = origin")
        .execute(&mut *conn)
        .await
        .unwrap();
    drop(conn);
    let retry = redispatch_after_catalog(
        &app,
        &org,
        &workspace,
        task,
        "2026.10.22-workspace-base-reviews",
    )
    .await;
    assert_eq!(retry.exec_id, first.exec_id);
    assert_eq!(retry.invoke_params["version"], "1.0.5");
    assert!(retry.invoke_params["input"]["provider"]["options"]["codex_transport"].is_null());
    app.cleanup().await;
}

async fn queue_codex_turn(
    app: &TestApp,
    org: &TestOrg,
    workspace: &TestWorkspace,
    owner: Uuid,
) -> (Uuid, Uuid) {
    use g5_contracts::types::{AgentLaunchFieldPreferenceV1, ChatAgentProvider};
    let (task, session) = create_agent_task(app, workspace, owner, "queued", None, None).await;
    let preferences = AgentLaunchPreferencesV1 {
        provider: AgentLaunchFieldPreferenceV1::Explicit(ChatAgentProvider::Codex),
        permission_mode: AgentLaunchFieldPreferenceV1::Explicit("never/read-only".into()),
        ..Default::default()
    };
    sqlx::query("UPDATE tasks SET spec=jsonb_set(spec,'{agent,agentLaunch}',$2) WHERE id=$1")
        .bind(task)
        .bind(serde_json::to_value(preferences).unwrap())
        .execute(&app.db)
        .await
        .unwrap();
    queue_session_input(app, org.id, session, task, "frozen input").await;
    (task, session)
}

/// Rebuild the task's unconfirmed reservation after the VM's recorded
/// catalog changed.
async fn redispatch_after_catalog(
    app: &TestApp,
    org: &TestOrg,
    workspace: &TestWorkspace,
    task: Uuid,
    catalog: &str,
) -> g5_api_core::services::agents::agent_dispatch::AgentDispatch {
    sqlx::query("UPDATE workspace_vms SET module_catalog_version=$2 WHERE workspace_id=$1")
        .bind(workspace.id)
        .bind(catalog)
        .execute(&app.db)
        .await
        .unwrap();
    sqlx::query("UPDATE tasks SET updated_at=now()-interval '3 seconds' WHERE id=$1")
        .bind(task)
        .execute(&app.db)
        .await
        .unwrap();
    let mut db = app.scoped_db_for_org(org.id).await;
    let retry = app
        .state
        .task_service
        .reserve_agent_dispatch(&mut db)
        .await
        .unwrap()
        .unwrap();
    db.commit().await.unwrap();
    retry
}
