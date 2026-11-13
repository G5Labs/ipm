//! Agent actions share conversation admission; descriptive names do not select execution policy.
use crate::helpers;

use chrono::{Duration, Utc};
use g5_api::services::approval_review_service::CreateApprovalReviewRequest;
use g5_api::services::spend_service::{RecordUsageRequest, SpendLimitStatus};
use helpers::{TestApp, TestOrg, TestUser, TestWorkspace};
use reqwest::StatusCode;
use serde_json::{json, Value};
use uuid::Uuid;

#[tokio::test]
async fn workspace_statistics_include_all_tasks_for_owners_and_members() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let member =
        TestUser::create_with_scopes(&app, org.id, &[g5_auth::Scope::WorkspacesRead]).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let other = TestWorkspace::create(&app, &org).await;
    let client = app.authenticated_client(&owner);
    for state in [
        "queued",
        "running",
        "waiting",
        "succeeded",
        "failed",
        "cancelled",
    ] {
        let response = client
            .post_json(
                &format!("/api/v1/workspaces/{}/sessions/start", workspace.id),
                &json!({"content": state}),
            )
            .await;
        assert_eq!(response.status(), StatusCode::CREATED);
        let response: Value = response.json().await.unwrap();
        let task_id: Uuid = response["data"]["taskId"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap();
        sqlx::query(
            "UPDATE tasks SET status=$2, cost_usd=1, input_tokens=11, output_tokens=7 WHERE id=$1",
        )
        .bind(task_id)
        .bind(state)
        .execute(&app.db)
        .await
        .unwrap();
    }
    // Every task contributes once; another workspace remains isolated.
    for (ws, kind, state, cost) in [
        (workspace.id, "command", "succeeded", 4),
        (workspace.id, "command", "running", 0),
        (workspace.id, "preview", "running", 0),
        (other.id, "command", "running", 99),
    ] {
        sqlx::query("INSERT INTO tasks (id,org_id,workspace_id,name,kind,status,cost_usd,created_by) VALUES ($1,$2,$3,'Count me',$4,$5,$6,$7)")
            .bind(Uuid::new_v4()).bind(org.id).bind(ws).bind(kind).bind(state).bind(cost).bind(owner.id)
            .execute(&app.db).await.unwrap();
    }
    // Recorded spend survives every task state, including ordinary agent tasks.
    sqlx::query("INSERT INTO usage_records (id,org_id,workspace_id,task_id,provider,model,input_tokens,output_tokens,total_tokens,cost_usd,request_type)
                 SELECT gen_random_uuid(),org_id,workspace_id,id,'codex','test',input_tokens,output_tokens,input_tokens+output_tokens,cost_usd,'chat'
                 FROM tasks WHERE workspace_id IN ($1,$2)")
        .bind(workspace.id).bind(other.id).execute(&app.db).await.unwrap();
    let mut counts = Vec::new();
    for user in [&owner, &member] {
        let response = app
            .authenticated_client(user)
            .get(&format!("/api/v1/orgs/{}/workspaces", org.id))
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        let response: Value = response.json().await.unwrap();
        let row = response["data"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["id"] == workspace.id.to_string())
            .unwrap();
        counts.push(json!([
            row["totalTasks"],
            row["tasksCompleted"],
            row["concurrentTasks"],
            row["cost"]
        ]));
    }
    let response = client
        .get(&format!("/api/v1/workspaces/{}/stats", workspace.id))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let response: Value = response.json().await.unwrap();
    let stats = &response["data"];
    counts.push(json!([
        stats["totalTasks"],
        stats["tasksCompleted"],
        stats["concurrentTasks"],
        stats["totalCostUsd"]
    ]));
    assert_eq!(counts, vec![json!([9, 2, 3, 10.0]); 3]);
    assert_eq!(stats["tasksStopped"], 1);
    // Only the seven tasks with recorded positive spend appear in the breakdown.
    assert_eq!(stats["byTask"].as_array().unwrap().len(), 7);
    assert_eq!(stats["byTask"][0]["costUsd"], 4.0);

    let response = client
        .get(&format!(
            "/api/v1/workspaces/{}/settings/spend",
            workspace.id
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let response: Value = response.json().await.unwrap();
    assert_eq!(response["data"]["totalSpendUsd"], 10.0);
    assert_eq!(response["data"]["totalInputTokens"], 66);
    assert_eq!(response["data"]["totalOutputTokens"], 42);
    for (spend, input, output, expected_reason) in [
        (Some(8.0), None, None, "max_spend_usd"),
        (None, Some(60_i64), None, "max_input_tokens"),
        (None, None, Some(40_i64), "max_output_tokens"),
    ] {
        sqlx::query("INSERT INTO workspace_settings (workspace_id,org_id,max_spend_usd,max_input_tokens,max_output_tokens)
                     VALUES ($1,$2,$3::double precision::numeric,$4,$5)
                     ON CONFLICT (workspace_id) DO UPDATE SET max_spend_usd=EXCLUDED.max_spend_usd,
                       max_input_tokens=EXCLUDED.max_input_tokens,max_output_tokens=EXCLUDED.max_output_tokens")
            .bind(workspace.id).bind(org.id).bind(spend).bind(input).bind(output)
            .execute(&app.db).await.unwrap();
        let mut db = app.scoped_db_for_org(org.id).await;
        let status = app
            .state
            .spend_service
            .check_spend_limit(&mut db, workspace.id)
            .await
            .unwrap();
        assert!(
            matches!(status, g5_api::services::spend_service::SpendLimitStatus::Exceeded { reason } if reason == expected_reason)
        );
        db.rollback().await.unwrap();
    }
    app.cleanup().await;
}

#[tokio::test]
async fn spend_period_counts_new_usage_from_tasks_started_in_a_prior_period() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let client = app.authenticated_client(&owner);
    let start = chrono::DateTime::from_timestamp(Utc::now().timestamp() - 3600, 0).unwrap();
    let end = start + Duration::days(1);
    let old = start - Duration::days(2);
    sqlx::query("INSERT INTO workspace_settings (workspace_id,org_id,spend_period,spend_period_start,max_spend_usd)
                 VALUES ($1,$2,'daily',$3,5) ON CONFLICT (workspace_id) DO UPDATE
                 SET spend_period='daily',spend_period_start=$3,max_spend_usd=5")
        .bind(workspace.id).bind(org.id).bind(start).execute(&app.db).await.unwrap();
    for _ in 0..2 {
        let response = client
            .post_json(
                &format!("/api/v1/workspaces/{}/sessions/start", workspace.id),
                &json!({"content":"Keep working across the period boundary."}),
            )
            .await;
        assert_eq!(response.status(), StatusCode::CREATED);
        let body: Value = response.json().await.unwrap();
        let task_id: Uuid = body["data"]["taskId"].as_str().unwrap().parse().unwrap();
        sqlx::query("UPDATE tasks SET created_at=$2, status='running' WHERE id=$1")
            .bind(task_id)
            .bind(old)
            .execute(&app.db)
            .await
            .unwrap();
        for (model, at, cost) in [
            ("before", start - Duration::seconds(1), 100_i64),
            ("at-start", start, 1),
            ("during", start + Duration::seconds(1), 2),
            ("at-end", end, 8),
        ] {
            let mut db = app.scoped_db_for_org(org.id).await;
            app.state
                .spend_service
                .record_usage(
                    &mut db,
                    vec![RecordUsageRequest {
                        org_id: org.id,
                        workspace_id: workspace.id,
                        task_id: Some(task_id),
                        session_id: None,
                        provider: "codex".into(),
                        model: model.into(),
                        input_tokens: cost * 10,
                        output_tokens: cost * 5,
                        total_tokens: None,
                        cache_creation_tokens: None,
                        cache_read_tokens: None,
                        cost_usd: cost.into(),
                        request_type: "chat".into(),
                        finish_reason: None,
                        duration_ms: None,
                    }],
                )
                .await
                .unwrap();
            db.commit().await.unwrap();
            sqlx::query("UPDATE usage_records SET created_at=$3 WHERE task_id=$1 AND model=$2")
                .bind(task_id)
                .bind(model)
                .bind(at)
                .execute(&app.db)
                .await
                .unwrap();
        }
    }
    let response = client
        .get(&format!(
            "/api/v1/workspaces/{}/settings/spend",
            workspace.id
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    let summary = &body["data"];
    assert_eq!(summary["totalSpendUsd"], 6.0);
    assert_eq!(summary["totalInputTokens"], 60);
    assert_eq!(summary["totalOutputTokens"], 30);
    assert_eq!(summary["isSpendLimitReached"], true);
    assert_eq!(summary["byTask"].as_array().unwrap().len(), 2);
    assert_eq!(summary["byTask"][0]["spendUsd"], 3.0);
    assert_eq!(summary["byTask"][0]["inputTokens"], 30);
    assert_eq!(summary["byTask"][0]["outputTokens"], 15);
    let mut db = app.scoped_db_for_org(org.id).await;
    assert!(
        matches!(app.state.spend_service.check_spend_limit(&mut db, workspace.id).await.unwrap(),
        SpendLimitStatus::Exceeded { reason } if reason == "max_spend_usd")
    );
    db.rollback().await.unwrap();

    // Recorded usage remains chargeable while waiting and after failure/Stop.
    for status in ["waiting", "failed", "cancelled"] {
        sqlx::query("UPDATE tasks SET status=$2 WHERE workspace_id=$1")
            .bind(workspace.id)
            .bind(status)
            .execute(&app.db)
            .await
            .unwrap();
        let response = client
            .get(&format!(
                "/api/v1/workspaces/{}/settings/spend",
                workspace.id
            ))
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        let body: Value = response.json().await.unwrap();
        assert_eq!(body["data"]["totalSpendUsd"], 6.0, "{status}");
        assert_eq!(body["data"]["byTask"][0]["spendUsd"], 3.0, "{status}");
        assert_eq!(body["data"]["isSpendLimitReached"], true, "{status}");
    }

    // Unattributed, session-only and task usage each contribute exactly once.
    let (task_id, session_id): (Uuid, Uuid) =
        sqlx::query_as("SELECT t.id, i.session_id FROM tasks t JOIN session_inputs i ON i.task_id=t.id WHERE t.workspace_id=$1 ORDER BY t.id LIMIT 1")
            .bind(workspace.id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    for (task, session, cost) in [
        (None, None, 4_i64),
        (None, Some(session_id), 2),
        (Some(task_id), None, 8),
    ] {
        let mut db = app.scoped_db_for_org(org.id).await;
        app.state
            .spend_service
            .record_usage(
                &mut db,
                vec![RecordUsageRequest {
                    org_id: org.id,
                    workspace_id: workspace.id,
                    task_id: task,
                    session_id: session,
                    provider: "codex".into(),
                    model: "partial-association".into(),
                    input_tokens: cost * 10,
                    output_tokens: cost * 5,
                    total_tokens: None,
                    cache_creation_tokens: None,
                    cache_read_tokens: None,
                    cost_usd: cost.into(),
                    request_type: "chat".into(),
                    finish_reason: None,
                    duration_ms: None,
                }],
            )
            .await
            .unwrap();
        db.commit().await.unwrap();
    }
    let response = client
        .get(&format!(
            "/api/v1/workspaces/{}/settings/spend",
            workspace.id
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["data"]["totalSpendUsd"], 20.0);
    assert_eq!(body["data"]["unattributedSpendUsd"], 6.0);
    assert_eq!(body["data"]["totalInputTokens"], 200);
    assert_eq!(body["data"]["totalOutputTokens"], 100);
    assert_eq!(body["data"]["byTask"][0]["spendUsd"], 11.0);
    assert_eq!(body["data"]["byTask"][0]["inputTokens"], 110);
    assert_eq!(body["data"]["byTask"][0]["outputTokens"], 55);
    app.cleanup().await;
}

#[tokio::test]
async fn every_prefilled_action_is_an_ordinary_unlimited_agent_task() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let client = app.authenticated_client(&owner);
    for label in ["chat", "sync", "explainer", "review", "My arbitrary action"] {
        let response = client.post_json(
            &format!("/api/v1/workspaces/{}/sessions/start", workspace.id),
            &json!({"title": label, "content": format!("Please perform {label}."), "requestKey": label}),
        ).await;
        let status = response.status();
        let body: Value = response.json().await.unwrap();
        assert_eq!(status, StatusCode::CREATED, "{body}");
        let task_id: Uuid = body["data"]["taskId"].as_str().unwrap().parse().unwrap();
        let (kind, spec, content): (String, Value, String) = sqlx::query_as(
            "SELECT task.kind, task.spec, input.content FROM tasks task
             JOIN session_inputs input ON input.task_id = task.id WHERE task.id = $1",
        )
        .bind(task_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
        assert_eq!(kind, "agent");
        assert_eq!(content, format!("Please perform {label}."));
        assert!(spec["agent"]["limits"].is_object());
        assert!(spec["agent"]["limits"].get("wallClockMs").is_none());
        let session_id = body["data"]["session"]["id"].as_str().unwrap();
        let inputs = client
            .get(&format!("/api/v1/sessions/{session_id}/inputs"))
            .await;
        assert_eq!(inputs.status(), StatusCode::OK);
        let inputs: Value = inputs.json().await.unwrap();
        assert_eq!(
            inputs["data"][0]["content"],
            format!("Please perform {label}.")
        );
        // Before dispatch, the inbox is authoritative; transcript messages
        // only appear after the provider acknowledges delivery.
        let messages = client
            .get(&format!("/api/v1/sessions/{session_id}/messages"))
            .await;
        assert_eq!(messages.status(), StatusCode::OK);
        let messages: Value = messages.json().await.unwrap();
        assert!(messages["data"]["messages"].as_array().unwrap().is_empty());
    }
    // Upgrade queued work too: a retained legacy cap cannot survive in task specs.
    let seeded = sqlx::query("UPDATE tasks SET spec=jsonb_set(spec, '{agent,limits,wallClockMs}', '3600000'::jsonb) WHERE workspace_id=$1")
        .bind(workspace.id).execute(&app.db).await.unwrap();
    assert_eq!(seeded.rows_affected(), 5);
    let capped: i64 = sqlx::query_scalar("SELECT count(*) FROM tasks WHERE workspace_id=$1 AND spec #> '{agent,limits,wallClockMs}' = '3600000'::jsonb")
        .bind(workspace.id).fetch_one(&app.db).await.unwrap();
    assert_eq!(capped, 5, "all legacy caps must exist before migration");
    sqlx::raw_sql(include_str!(
        "../src/db/migrations/078_remove_agent_lifetime_limits.sql"
    ))
    .execute(&app.db)
    .await
    .unwrap();
    let capped: i64 = sqlx::query_scalar("SELECT count(*) FROM tasks WHERE workspace_id=$1 AND spec #> '{agent,limits,wallClockMs}' IS NOT NULL")
        .bind(workspace.id).fetch_one(&app.db).await.unwrap();
    assert_eq!(capped, 0);
    app.cleanup().await;
}

#[tokio::test]
async fn approval_review_admission_retries_share_one_durable_operation_and_exact_request() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let source = TestWorkspace::create(&app, &org).await;
    let target = TestWorkspace::create(&app, &org).await;
    sqlx::query("UPDATE workspaces SET merge_target_workspace_id=$2 WHERE id=$1")
        .bind(source.id)
        .bind(target.id)
        .execute(&app.db)
        .await
        .unwrap();
    let client = app.authenticated_client(&owner);
    let definition = client.post_json(
        &format!("/api/v1/orgs/{}/approval-definitions", org.id),
        &json!({"name":"Review renamed freely", "agentType":"simple", "agentPrompt":"Check the authorization boundary.", "phases":["ie"]}),
    ).await;
    let status = definition.status();
    let definition: Value = definition.json().await.unwrap();
    assert_eq!(status, StatusCode::CREATED, "{definition}");
    let definition_id = definition["data"]["id"].as_str().unwrap();
    let attached = client
        .post_json(
            &format!("/api/v1/workspaces/{}/approvals/requirements", target.id),
            &json!({"definitionId":definition_id}),
        )
        .await;
    assert_eq!(attached.status(), StatusCode::OK);
    let path = format!("/api/v1/workspaces/{}/approvals/reviews", source.id);
    let request = json!({"definitionId":definition_id,"definitionVersion":1,"phase":"ie"});
    let review_request = || CreateApprovalReviewRequest {
        client_request_id: None,
        retry_review_id: None,
        definition_id: definition_id.parse().unwrap(),
        definition_version: 1,
        agent_launch: None,
        phase: "ie".into(),
    };
    let db = app.scoped_db_for_org(org.id).await;
    assert!(app
        .state
        .approval_review_service
        .admit(
            db,
            &app.state.system_pool,
            source.id,
            Uuid::new_v4(),
            review_request(),
        )
        .await
        .is_err());
    // If admission rolls back, recovery must not report a durable task or request.
    let db = app.scoped_db_for_org(org.id).await;
    let (db, rolled_back) = app
        .state
        .approval_review_service
        .admit(
            db,
            &app.state.system_pool,
            source.id,
            owner.id,
            review_request(),
        )
        .await
        .unwrap();
    db.rollback().await.unwrap();
    let mut db = app.scoped_db_for_org(org.id).await;
    assert!(app
        .state
        .approval_review_service
        .confirm_admission_after_commit_error(&mut db, source.id, &rolled_back,)
        .await
        .unwrap()
        .is_none());
    db.rollback().await.unwrap();
    // Both routes hold the ordinary workspace authority SHARE lock before
    // admission. Acquiring UPDATE here would deadlock concurrent requests.
    let mut first_db = app.scoped_db_for_org(org.id).await;
    let mut second_db = app.scoped_db_for_org(org.id).await;
    for db in [&mut first_db, &mut second_db] {
        sqlx::query("SELECT id FROM workspaces WHERE id=$1 FOR SHARE")
            .bind(source.id)
            .execute(db.conn())
            .await
            .unwrap();
    }
    let admit_and_commit = |db| {
        let request = review_request();
        let app = &app;
        let source_id = source.id;
        let owner_id = owner.id;
        async move {
            let (db, receipt) = app
                .state
                .approval_review_service
                .admit(db, &app.state.system_pool, source_id, owner_id, request)
                .await
                .unwrap();
            db.commit().await.unwrap();
            receipt
        }
    };
    let (first_receipt, second_receipt) =
        tokio::join!(admit_and_commit(first_db), admit_and_commit(second_db),);
    assert_eq!(first_receipt.request_id, second_receipt.request_id);
    assert_eq!(first_receipt.review_id, second_receipt.review_id);
    assert_ne!(first_receipt.request_id, first_receipt.review_id);
    assert_eq!(first_receipt.task_id, second_receipt.task_id);
    let (first, retry) = tokio::join!(
        client.post_json(&path, &request),
        client.post_json(&path, &request)
    );
    let first_status = first.status();
    let retry_status = retry.status();
    let first: Value = first.json().await.unwrap();
    let retry: Value = retry.json().await.unwrap();
    assert!(first_status.is_success(), "{first_status}: {first}");
    assert!(retry_status.is_success(), "{retry_status}: {retry}");
    assert_eq!(first["data"]["requestId"], retry["data"]["requestId"]);
    assert_eq!(first["data"]["reviewId"], retry["data"]["reviewId"]);
    assert_eq!(
        first["data"]["reviewId"],
        first_receipt.review_id.to_string()
    );
    assert!(first["data"]["reused"] == true || retry["data"]["reused"] == true);
    let request_id: Uuid = first["data"]["requestId"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let rows: Vec<(Uuid,Uuid,Option<Uuid>,String)> = sqlx::query_as("SELECT id,review_workspace_id,task_id,stage FROM agent_approval_reviews WHERE approval_request_id=$1")
        .bind(request_id).fetch_all(&app.db).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, first_receipt.review_id);
    assert_ne!(rows[0].0, request_id);
    assert_eq!(
        rows[0].2, None,
        "no task before checked fork and VM readiness"
    );
    assert_eq!(rows[0].3, "capture");
    let db = app.scoped_db_for_org(org.id).await;
    let (db, receipt) = app
        .state
        .approval_review_service
        .admit(
            db,
            &app.state.system_pool,
            source.id,
            owner.id,
            review_request(),
        )
        .await
        .unwrap();
    db.commit().await.unwrap();
    assert_eq!(receipt.review_id, first_receipt.review_id);
    assert_eq!(receipt.request_id, request_id);
    let mut db = app.scoped_db_for_org(org.id).await;
    assert!(app
        .state
        .approval_review_service
        .confirm_admission_after_commit_error(&mut db, source.id, &receipt)
        .await
        .unwrap()
        .is_some());
    db.rollback().await.unwrap();
    let mut wrong = receipt.clone();
    wrong.review_id = Uuid::new_v4();
    let mut db = app.scoped_db_for_org(org.id).await;
    assert!(app
        .state
        .approval_review_service
        .confirm_admission_after_commit_error(&mut db, source.id, &wrong)
        .await
        .unwrap()
        .is_none());
    db.rollback().await.unwrap();
    let response = client
        .get(&format!("/api/v1/workspaces/{}/approvals", source.id))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let history: Value = response.json().await.unwrap();
    assert!(
        history.to_string().contains(&request_id.to_string()),
        "{history}"
    );
    // The formal child exception must not allow unrelated task links, on
    // either insertion or repointing an existing source link.
    let source_task = Uuid::new_v4();
    let unrelated_task = Uuid::new_v4();
    for (task, workspace) in [(source_task, source.id), (unrelated_task, target.id)] {
        sqlx::query(
            r#"
            INSERT INTO tasks (id, org_id, workspace_id, name, kind, status, created_by)
            VALUES ($1, $2, $3, 'Link guard', 'command', 'succeeded', $4)
            "#,
        )
        .bind(task)
        .bind(org.id)
        .bind(workspace)
        .bind(owner.id)
        .execute(&app.db)
        .await
        .unwrap();
    }
    let error = sqlx::query(
        r#"
        INSERT INTO approval_request_tasks (org_id, approval_request_id, source_workspace_id, task_id)
        VALUES ($1, $2, $3, $4)
        "#,
    )
    .bind(org.id)
    .bind(request_id)
    .bind(source.id)
    .bind(unrelated_task)
    .execute(&app.db)
    .await
    .unwrap_err();
    let database_error = error.as_database_error().unwrap();
    assert_eq!(database_error.code().as_deref(), Some("23514"));
    assert_eq!(
        database_error.message(),
        "approval task does not belong to its source or assigned review workspace"
    );
    sqlx::query(
        r#"
        INSERT INTO approval_request_tasks (org_id, approval_request_id, source_workspace_id, task_id)
        VALUES ($1, $2, $3, $4)
        "#,
    )
    .bind(org.id)
    .bind(request_id)
    .bind(source.id)
    .bind(source_task)
    .execute(&app.db)
    .await
    .unwrap();
    let error = sqlx::query(
        "UPDATE approval_request_tasks SET source_workspace_id = $2 WHERE task_id = $1",
    )
    .bind(source_task)
    .bind(target.id)
    .execute(&app.db)
    .await
    .unwrap_err();
    let database_error = error.as_database_error().unwrap();
    assert_eq!(database_error.code().as_deref(), Some("23514"));
    assert_eq!(
        database_error.message(),
        "approval task does not belong to its source or assigned review workspace"
    );
    // Retention must keep the request while its ordinary task/report is retained.
    sqlx::query("UPDATE approval_requests SET state='expired', closed_at=now(), updated_at=now() WHERE id=$1")
        .bind(request_id).execute(&app.db).await.unwrap();
    let retention =
        g5_api::maintenance::approval_request_retention::run_approval_request_retention_sweep(
            &g5_api::maintenance::TenantMaintenanceDeps {
                system_pool: app.system_pool.clone(),
                tenant_pool: app.state.tenant_pool.clone(),
                authz: app.state.authz.clone(),
                repo_storage: app.state.repo_storage.clone(),
            },
            &g5_api::maintenance::approval_request_retention::ApprovalRequestRetentionConfig {
                retention_days: 0,
                ..Default::default()
            },
            &tokio_util::sync::CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(retention.orgs_failed, 0);
    assert_eq!(retention.deleted, 0);
    let retained: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM approval_requests WHERE id=$1)")
            .bind(request_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(retained);
    app.cleanup().await;
}

#[tokio::test]
async fn bounded_task_spend_keeps_overflow_and_deleted_task_usage_in_the_total() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    sqlx::query("INSERT INTO tasks (id,org_id,workspace_id,name,kind,status,cost_usd,created_by)
                 SELECT gen_random_uuid(),$1,$2,'Task ' || n,'command','succeeded',n,$3 FROM generate_series(1,101) n")
        .bind(org.id).bind(workspace.id).bind(owner.id).execute(&app.db).await.unwrap();
    sqlx::query("INSERT INTO usage_records (id,org_id,workspace_id,task_id,provider,model,input_tokens,output_tokens,total_tokens,cost_usd,request_type)
                 SELECT gen_random_uuid(),org_id,workspace_id,id,'codex','test',0,0,0,cost_usd,'chat' FROM tasks WHERE workspace_id=$1")
        .bind(workspace.id).execute(&app.db).await.unwrap();
    sqlx::query("INSERT INTO usage_records (id,org_id,workspace_id,provider,model,input_tokens,output_tokens,total_tokens,cost_usd,request_type)
                 VALUES (gen_random_uuid(),$1,$2,'codex','test',0,0,0,7,'chat')")
        .bind(org.id).bind(workspace.id).execute(&app.db).await.unwrap();
    let client = app.authenticated_client(&owner);
    for (removed, remainder, unattributed, other_tasks) in
        [(false, 8.0, 7.0, 1.0), (true, 108.0, 108.0, 0.0)]
    {
        if removed {
            sqlx::query("DELETE FROM tasks WHERE workspace_id=$1 AND name='Task 101'")
                .bind(workspace.id)
                .execute(&app.db)
                .await
                .unwrap();
        }
        let response = client
            .get(&format!("/api/v1/workspaces/{}/stats", workspace.id))
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        let body: Value = response.json().await.unwrap();
        let stats = &body["data"];
        assert_eq!(stats["totalCostUsd"], 5158.0);
        let tasks = stats["byTask"].as_array().unwrap();
        assert_eq!(tasks.len(), 100);
        let listed: f64 = tasks
            .iter()
            .map(|task| task["costUsd"].as_f64().unwrap())
            .sum();
        assert_eq!(5158.0 - listed, remainder);
        let response = client
            .get(&format!(
                "/api/v1/workspaces/{}/settings/spend",
                workspace.id
            ))
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        let body: Value = response.json().await.unwrap();
        let spend = &body["data"];
        assert_eq!(spend["totalSpendUsd"], 5158.0);
        assert_eq!(spend["byTask"].as_array().unwrap().len(), 100);
        assert_eq!(spend["unattributedSpendUsd"], unattributed);
        assert_eq!(spend["otherTaskSpendUsd"], other_tasks);
        let displayed: f64 = spend["byTask"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["spendUsd"].as_f64().unwrap())
            .sum();
        assert_eq!(displayed + unattributed + other_tasks, 5158.0);
    }
    app.cleanup().await;
}
