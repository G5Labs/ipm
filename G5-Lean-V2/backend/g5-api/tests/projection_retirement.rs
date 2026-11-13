//! Runtime boundaries retained after retiring the projection producers.
use crate::helpers::{self, TestApp};
use g5_api::config::MaintenanceConfig;
use std::{sync::Arc, time::Duration};

/// Scoped transaction setup, context cleanup and commit preserve the same
/// SQLSTATE classification as statement errors; transport errors remain opaque.
#[tokio::test]
async fn test_scoped_database_errors_preserve_retryability() {
    use g5_api::errors::AppError;
    use g5_authz::ScopedDbError;
    let app = TestApp::spawn().await;
    let wrappers: [fn(sqlx::Error) -> ScopedDbError; 3] = [
        ScopedDbError::InitFailed,
        ScopedDbError::ClearFailed,
        ScopedDbError::CommitFailed,
    ];
    for wrap in wrappers {
        for (state, expected) in [
            ("40P01", "DATABASE_RETRYABLE"),
            ("40001", "DATABASE_RETRYABLE"),
            ("P0001", "INTERNAL_ERROR"),
        ] {
            let error = sqlx::raw_sql(&format!(
                "DO $$ BEGIN RAISE EXCEPTION 'private database failure' USING ERRCODE = '{state}'; END $$;"
            )).execute(&app.db).await.unwrap_err();
            let original = error.to_string();
            let converted = AppError::from(wrap(error));
            assert_eq!(converted.inner.error_code(), expected);
            assert!(!converted
                .inner
                .to_error_response(String::new())
                .message
                .contains("private database failure"));
            if let g5_api::errors::G5Error::Internal { message } = converted.inner {
                assert_eq!(message, original);
            }
        }
        for error in [
            sqlx::Error::PoolTimedOut,
            sqlx::Error::Protocol("deadlock detected".into()),
        ] {
            let original = error.to_string();
            match AppError::from(wrap(error)).inner {
                g5_api::errors::G5Error::Internal { message } => assert_eq!(message, original),
                other => panic!("transport/protocol failure must not be retried: {other:?}"),
            }
        }
    }
    assert!(matches!(
        AppError::from(ScopedDbError::InvalidSession {
            session_id: g5_contracts::types::ids::SessionId::new(),
        })
        .inner,
        g5_api::errors::G5Error::Unauthorized
    ));
    match AppError::from(ScopedDbError::TransactionConsumed).inner {
        g5_api::errors::G5Error::Internal { message } => {
            assert_eq!(message, "Database transaction already consumed")
        }
        other => panic!("consumed transaction mapping changed: {other:?}"),
    }
    app.cleanup().await;
}

async fn retirement_snapshot(app: &TestApp) -> serde_json::Value {
    sqlx::query_scalar("SELECT jsonb_build_object('nodes',to_regclass('public.ontology_nodes'),'docs',(SELECT jsonb_agg(to_jsonb(d) ORDER BY id) FROM docs d),'links',to_regclass('public.doc_node_links'),'state',to_regclass('public.projection_state'))")
        .fetch_one(&app.db).await.unwrap()
}

/// Real Tokio loops, PostgreSQL, and the retained branch-event subscriber. The
/// clock advances explicitly; an invitation transition is the progress barrier.
/// There is no elapsed-time sleep that could mistake an unstarted loop for absence.
#[tokio::test]
async fn maintenance_startup_ticks_hints_and_restart_never_project() {
    use g5_api::ws::{Channel, WsEvent};
    use helpers::{TestOrg, TestWorkspace};
    use uuid::Uuid;
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let owner: Uuid = sqlx::query_scalar("SELECT owner_user_id FROM workspaces WHERE id=$1")
        .bind(workspace.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    sqlx::query("UPDATE workspace_repos SET role='docs',root_path='docs' WHERE workspace_id=$1 AND repo_id=$2")
        .bind(workspace.id).bind(workspace.repo_id).execute(&app.db).await.unwrap();
    sqlx::query("UPDATE workspaces SET docs_repo_id=$2 WHERE id=$1")
        .bind(workspace.id)
        .bind(workspace.repo_id)
        .execute(&app.db)
        .await
        .unwrap();
    let doc = Uuid::new_v4();
    sqlx::query("INSERT INTO docs(id,org_id,workspace_id,repo_id,name,doc_type,file_path,content) VALUES($1,$2,$3,$4,'Legacy doc','design','docs/legacy.md','Retained metadata')")
        .bind(doc).bind(org.id).bind(workspace.id).bind(workspace.repo_id).execute(&app.db).await.unwrap();
    let before = retirement_snapshot(&app).await;
    for key in ["nodes", "links", "state"] {
        assert!(
            before[key].is_null(),
            "retired storage must be absent: {key}"
        );
    }
    let mut scoped = app.scoped_db_for_org(org.id).await;
    let visible: i64 = sqlx::query_scalar("SELECT count(*) FROM docs WHERE id=$1")
        .bind(doc)
        .fetch_one(scoped.conn())
        .await
        .unwrap();
    assert_eq!(
        visible, 1,
        "the fixture is visible to the actual tenant role"
    );
    scoped.rollback().await.unwrap();
    let mut locks = app.db.acquire().await.unwrap();
    sqlx::query("SELECT pg_advisory_lock($1)")
        .bind(g5_api::maintenance::repo_gc::ADVISORY_LOCK_KEY)
        .execute(&mut *locks)
        .await
        .unwrap();
    let projection_key: i64 = sqlx::query_scalar(
        "SELECT hashtextextended('projection:' || $1 || ':refs/heads/' || $2,0)",
    )
    .bind(workspace.repo_id.to_string())
    .bind(&workspace.name)
    .fetch_one(&app.db)
    .await
    .unwrap();
    sqlx::query("SELECT pg_advisory_lock($1)")
        .bind(projection_key)
        .execute(&mut *locks)
        .await
        .unwrap();
    // Every repository access made by the production maintenance runner is
    // observed. Repo GC has its independent replica lock held below, and the
    // fixture has no queued provider work or flush receipts. A projection pass
    // would still open the populated workspace mirror, even if its advisory
    // lock subsequently blocks.
    let accesses = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counted = accesses.clone();
    let storage = app.repo_storage.observed_for_test(Arc::new(move |_| {
        counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }));

    // Keep the runtime runnable while PostgreSQL responds so paused Tokio time
    // cannot auto-jump across connection timeouts. Only advance() moves it.
    let clock_guard_cancel = tokio_util::sync::CancellationToken::new();
    let clock_guard_token = clock_guard_cancel.clone();
    let clock_guard = tokio::spawn(async move {
        while !clock_guard_token.is_cancelled() {
            tokio::task::yield_now().await;
        }
    });
    tokio::time::pause();
    for restart in 0..2 {
        let cancel = tokio_util::sync::CancellationToken::new();
        let runner = g5_api::maintenance::MaintenanceRunner::new(
            g5_api::maintenance::MaintenanceRunnerDeps {
                system_pool: app.system_pool.clone(),
                authz_pool: app.authz_pool.clone(),
                tenant_pool: app.state.tenant_pool.clone(),
                authz: app.state.authz.clone(),
                repo_storage: storage.clone(),
                event_bus: app.state.event_bus.clone(),
                git_bin: app.state.git_bin.clone(),
                repository_credentials: app.state.repository_credentials.clone(),
            },
            MaintenanceConfig {
                invitation_gc_interval: Duration::from_secs(1),
                stale_draft_timeout: Duration::from_secs(86400),
                draft_stale_retention: Duration::from_secs(604800),
                repo_gc_interval: Duration::from_secs(3600),
                repo_gc_loose_threshold: 500,
                remote_tracking_scheduler_interval: Duration::from_secs(60),
                remote_tracking_advisory_interval: Duration::from_secs(60),
                passive_auto_pull_enabled: true,
            },
            cancel.clone(),
            helpers::test_config(),
        );
        let handles = runner.spawn_all();
        assert_eq!(handles.len(), 13);
        let pr_refresh = tokio::spawn(g5_api::maintenance::workspace_pull_requests::run_loop(
            app.state.clone(),
            cancel.clone(),
        ));
        for tick in 0..3 {
            let invitation = Uuid::new_v4();
            sqlx::query("INSERT INTO org_invitations(id,org_id,email,role,invited_by,status,expires_at) VALUES($1,$2,$1::text || '@test.invalid','member',$3,'pending',now()-interval '1 hour')")
                .bind(invitation).bind(org.id).bind(owner).execute(&app.db).await.unwrap();
            // Duplicate stale OIDs are hints, including after a restart.
            for _ in 0..3 {
                app.state.event_bus.publish(
                    &Channel::Workspace(workspace.id),
                    &WsEvent::WorkspaceBranchUpdated {
                        workspace_id: workspace.id,
                        repo_id: workspace.repo_id,
                        branch: workspace.name.clone(),
                        head_commit: "stale-hint".into(),
                        hint: None,
                    },
                );
            }
            tokio::time::advance(Duration::from_secs(61)).await;
            let deadline = std::time::Instant::now() + Duration::from_secs(10);
            loop {
                let status: String =
                    sqlx::query_scalar("SELECT status FROM org_invitations WHERE id=$1")
                        .bind(invitation)
                        .fetch_one(&app.db)
                        .await
                        .unwrap();
                if status == "expired" {
                    break;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "retained invitation GC did not progress: restart {restart}, tick {tick}"
                );
                tokio::task::yield_now().await;
            }
            let blocked: i64 = sqlx::query_scalar("SELECT count(*) FROM pg_locks WHERE locktype='advisory' AND NOT granted AND database=(SELECT oid FROM pg_database WHERE datname=current_database())")
                .fetch_one(&app.db).await.unwrap();
            assert_eq!(
                blocked, 0,
                "no maintenance task may wait for the held projection fence"
            );
            assert_eq!(retirement_snapshot(&app).await, before);
            assert_eq!(
                accesses.load(std::sync::atomic::Ordering::SeqCst),
                0,
                "no projection tree scan or read may open the mirror"
            );
        }
        cancel.cancel();
        tokio::time::resume();
        for handle in handles {
            tokio::time::timeout(Duration::from_secs(15), handle)
                .await
                .unwrap()
                .unwrap();
        }
        tokio::time::timeout(Duration::from_secs(15), pr_refresh)
            .await
            .unwrap()
            .unwrap();
        tokio::time::pause();
    }
    tokio::time::resume();
    clock_guard_cancel.cancel();
    clock_guard.await.unwrap();
    sqlx::query("SELECT pg_advisory_unlock_all()")
        .execute(&mut *locks)
        .await
        .unwrap();
    drop(locks);
    app.cleanup().await;
}

#[tokio::test]
async fn retired_routes_and_node_inputs_cannot_recreate_projection_data() {
    use helpers::{TestOrg, TestUser, TestWorkspace};
    use reqwest::StatusCode;
    use serde_json::json;
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let client = app.authenticated_client(&owner);
    let root = format!("/api/v1/workspaces/{}", workspace.id);
    let node = uuid::Uuid::new_v4();
    for suffix in [
        "ontology".to_string(),
        format!("ontology/{node}"),
        format!("ontology/{node}/children"),
        format!("ontology/{node}/content"),
        format!("ontology/{node}/read-ontology"),
    ] {
        assert_eq!(
            client.get(&format!("{root}/{suffix}")).await.status(),
            StatusCode::NOT_FOUND,
            "{suffix}"
        );
    }
    for suffix in ["ontology/sync-specs", "docs/sync-committed"] {
        let response = client
            .post_json(&format!("{root}/{suffix}"), &json!({}))
            .await;
        assert!(
            matches!(
                response.status(),
                StatusCode::NOT_FOUND | StatusCode::METHOD_NOT_ALLOWED
            ),
            "{suffix}: {}",
            response.status()
        );
    }
    for suffix in [
        format!("ontology/{node}/content"),
        format!("docs/{node}/links"),
    ] {
        assert_eq!(
            client
                .put_json(
                    &format!("{root}/{suffix}"),
                    &json!({"content":"Forbidden","nodeIds":[node]})
                )
                .await
                .status(),
            StatusCode::NOT_FOUND
        );
    }
    let peer =
        helpers::runtime_peer::TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let token = app
        .state
        .jwt_minter
        .as_ref()
        .unwrap()
        .mint_workspace_incarnation(org.id, workspace.id, owner.id, peer.vm_id(), 1, 3600)
        .unwrap();
    let internal = reqwest::Client::new()
        .post(format!(
            "{}/api/internal/workspaces/{}/ontology/upsert",
            app.base_url, workspace.id
        ))
        .bearer_auth(token)
        .json(&json!({"nodes":[]}))
        .send()
        .await
        .unwrap();
    // Unknown internal paths are denied by the middleware even for a valid
    // incarnation token. The router itself must also have no handler left.
    assert_eq!(internal.status(), StatusCode::UNAUTHORIZED);
    use tower::ServiceExt;
    let route = g5_api::routes::build_router(app.state.clone())
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri(format!(
                    "/api/internal/workspaces/{}/ontology/upsert",
                    workspace.id
                ))
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(route.status(), StatusCode::NOT_FOUND);
    let request = client
        .post_json(
            &format!("{root}/docs"),
            &json!({"name":"No node input","docType":"design","linkedNodeIds":[node]}),
        )
        .await;
    assert_eq!(request.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let count:i64=sqlx::query_scalar("SELECT (SELECT count(*) FROM docs)+(SELECT count(*) FROM pg_tables WHERE schemaname='public' AND tablename IN ('ontology_nodes','doc_node_links','projection_state'))").fetch_one(&app.db).await.unwrap();
    assert_eq!(count, 0);
    app.cleanup().await;
}
