//! Ordinary fork/VM HTTP behavior after a real uncertain COMMIT outcome.
use crate::helpers::{
    self, commit_ack_proxy::CommitAckProxy, TestApp, TestOrg, TestUser, TestWorkspace,
};
use reqwest::StatusCode;
use serde_json::{json, Value};
use uuid::Uuid;

async fn app_with_proxy() -> (TestApp, std::sync::Arc<CommitAckProxy>) {
    let proxy = CommitAckProxy::start().await;
    let copy = proxy.clone();
    let mut config = helpers::test_config();
    config.tenant_database.max_connections = 8;
    let app = TestApp::spawn_with_tenant_transport(
        config,
        std::sync::Arc::new(move |url| copy.rewrite(url)),
    )
    .await;
    (app, proxy)
}
async fn seed(app: &TestApp) -> (TestUser, TestOrg, TestWorkspace) {
    let mut owner = TestUser::create(app).await;
    let org = TestOrg::create_with_admin(app, &mut owner).await;
    let source = TestWorkspace::create(app, &org).await;
    source.attach_parent(app).await;
    g5_api::services::worker_host_service::WorkerHostService::new()
        .reconcile_unconfigured_org_defaults(
            &app.db,
            "local-dev",
            &format!(
                "mirror.gcr.io/library/buildpack-deps@sha256:{}",
                "a".repeat(64)
            ),
        )
        .await
        .unwrap();
    (owner, org, source)
}
async fn repeat_and_assert_one(
    app: &TestApp,
    owner: &TestUser,
    source: &TestWorkspace,
    vm: bool,
    key: Uuid,
    path: &str,
    body: &Value,
) {
    let repeated = app.authenticated_client(owner).post_json(path, body).await;
    let status = repeated.status();
    let result: Value = repeated.json().await.unwrap();
    assert_eq!(
        status,
        if vm {
            StatusCode::OK
        } else {
            StatusCode::CREATED
        },
        "{result}"
    );
    if vm {
        let (count, intent): (i64, Value) = sqlx::query_as(
            "SELECT count(*) OVER(), start_intent FROM workspace_vms WHERE workspace_id=$1",
        )
        .bind(source.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
        assert_eq!(count, 1);
        assert_eq!(intent["id"], key.to_string());
    } else {
        let operations: i64 =
            sqlx::query_scalar("SELECT count(*) FROM workspace_forks WHERE idempotency_key=$1")
                .bind(key.to_string())
                .fetch_one(&app.db)
                .await
                .unwrap();
        let children: i64 = sqlx::query_scalar("SELECT count(*) FROM workspaces WHERE name=$1")
            .bind(body["name"].as_str().unwrap())
            .fetch_one(&app.db)
            .await
            .unwrap();
        assert_eq!((operations, children), (1, 1));
    }
}

#[tokio::test]
async fn committed_fork_and_vm_start_with_lost_ack_return_503_and_retry_one_durable_identity() {
    for vm in [false, true] {
        let (app, proxy) = app_with_proxy().await;
        let (owner, _org, source) = seed(&app).await;
        let key = Uuid::new_v4();
        let (path, body, pattern) = if vm {
            (
                format!("/api/v1/workspaces/{}/resume", source.id),
                json!({"operationId":key}),
                "UPDATE workspace_vms SET start_intent",
            )
        } else {
            (
                format!("/api/v1/workspaces/{}/clone", source.id),
                json!({"name":format!("ack-{key}"),"idempotencyKey":key,"source":"branch"}),
                "INSERT INTO workspace_forks",
            )
        };
        let prior = proxy.drop_count();
        proxy.arm(pattern);
        let response = app
            .authenticated_client(&owner)
            .post_json(&path, &body)
            .await;
        let status = response.status();
        let result: Value = response.json().await.unwrap();
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "vm={vm}: {result}");
        assert!(
            result.to_string().contains("retry its durable identity"),
            "{result}"
        );
        proxy.wait_for_drop(prior).await;
        repeat_and_assert_one(&app, &owner, &source, vm, key, &path, &body).await;
        app.cleanup().await;
        drop(proxy);
    }
}

#[tokio::test]
async fn terminated_fork_and_vm_commit_roll_back_return_503_and_retry_one_durable_identity() {
    for vm in [false, true] {
        let app = TestApp::spawn().await;
        let (owner, _org, source) = seed(&app).await;
        let key = Uuid::new_v4();
        let lock = (key.as_u128() & i64::MAX as u128) as i64;
        let mut holder = app.db.begin().await.unwrap();
        let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
            .fetch_one(&mut *holder)
            .await
            .unwrap();
        sqlx::query("SELECT pg_advisory_xact_lock($1)")
            .bind(lock)
            .execute(&mut *holder)
            .await
            .unwrap();
        let (table, events, condition) = if vm {
            (
                "workspace_vms",
                "INSERT OR UPDATE",
                "WHEN (NEW.start_intent IS NOT NULL)",
            )
        } else {
            ("workspace_forks", "INSERT", "")
        };
        sqlx::raw_sql(&format!(
            "CREATE FUNCTION test_commit_hold() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_advisory_xact_lock({lock}); RETURN NEW; END $$; CREATE CONSTRAINT TRIGGER test_commit_hold AFTER {events} ON {table} DEFERRABLE INITIALLY DEFERRED FOR EACH ROW {condition} EXECUTE FUNCTION test_commit_hold();"
        )).execute(&app.db).await.unwrap();
        let (path, body) = if vm {
            (
                format!("/api/v1/workspaces/{}/resume", source.id),
                json!({"operationId":key}),
            )
        } else {
            (
                format!("/api/v1/workspaces/{}/clone", source.id),
                json!({"name":format!("rollback-{key}"),"idempotencyKey":key,"source":"branch"}),
            )
        };
        let request = {
            let client = app.authenticated_client(&owner);
            let path = path.clone();
            let body = body.clone();
            tokio::spawn(async move { client.post_json(&path, &body).await })
        };
        let blocked=tokio::time::timeout(std::time::Duration::from_secs(10),async {
            loop {
                let candidate:Option<i32>=sqlx::query_scalar(
                    "SELECT pid FROM pg_stat_activity WHERE datname=current_database() AND wait_event='advisory' AND $1=ANY(pg_blocking_pids(pid)) AND query='COMMIT'",
                ).bind(pid).fetch_optional(&app.db).await.unwrap();
                if let Some(pid)=candidate{return pid;}
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        }).await.expect("operation must reach its actual deferred COMMIT trigger");
        assert!(
            sqlx::query_scalar::<_, bool>("SELECT pg_terminate_backend($1)")
                .bind(blocked)
                .fetch_one(&app.db)
                .await
                .unwrap()
        );
        let response = tokio::time::timeout(std::time::Duration::from_secs(10), request)
            .await
            .unwrap()
            .unwrap();
        let status = response.status();
        let result: Value = response.json().await.unwrap();
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "vm={vm}: {result}");
        if vm {
            let intents:i64=sqlx::query_scalar("SELECT count(*) FROM workspace_vms WHERE workspace_id=$1 AND start_intent IS NOT NULL")
                .bind(source.id).fetch_one(&app.db).await.unwrap();
            assert_eq!(intents, 0);
        } else {
            let count: i64 =
                sqlx::query_scalar("SELECT count(*) FROM workspace_forks WHERE idempotency_key=$1")
                    .bind(key.to_string())
                    .fetch_one(&app.db)
                    .await
                    .unwrap();
            assert_eq!(count, 0);
        }
        holder.rollback().await.unwrap();
        sqlx::raw_sql(&format!(
            "DROP TRIGGER test_commit_hold ON {table}; DROP FUNCTION test_commit_hold();"
        ))
        .execute(&app.db)
        .await
        .unwrap();
        repeat_and_assert_one(&app, &owner, &source, vm, key, &path, &body).await;
        app.cleanup().await;
    }
}
