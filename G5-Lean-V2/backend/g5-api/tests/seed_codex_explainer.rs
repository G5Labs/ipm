//! Exercise the Codex seed against real HTTP, PostgreSQL, and Git boundaries.
use crate::helpers;

use helpers::TestApp;
use serde_json::Value;
use sqlx::Row;
use uuid::Uuid;

/// Execute fixture SQL on the isolated database; only Docker/psql transport
/// is replaced, so membership constraints and production HTTP reads are real.
async fn fixture_sql(
    axum::extract::State(db): axum::extract::State<sqlx::PgPool>,
    sql: String,
) -> Result<String, (axum::http::StatusCode, String)> {
    let rows = sqlx::raw_sql(&sql)
        .fetch_all(&db)
        .await
        .map_err(|error| (axum::http::StatusCode::BAD_REQUEST, error.to_string()))?;
    rows.into_iter()
        .map(|row| {
            row.try_get::<String, _>(0)
                .or_else(|_| row.try_get::<Uuid, _>(0).map(|id| id.to_string()))
                .or_else(|_| row.try_get::<i64, _>(0).map(|count| count.to_string()))
                .map_err(|error| (axum::http::StatusCode::BAD_REQUEST, error.to_string()))
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|values| values.join("\n"))
}

/// The real seed must provision the docs write boundary before advertising
/// its curl command, and reruns must preserve the already-created explainers.
#[tokio::test]
#[serial_test::serial(e2e_auth)]
async fn test_codex_seed_provisions_docs_and_prints_a_working_explainer_command() {
    let _env = helpers::EnvVarGuard::set_many(&[("G5_E2E_AUTH", "true")]);
    let app = TestApp::spawn().await;
    let token = app
        .client
        .post(format!("{}/api/v1/auth/test-token", app.base_url))
        .json(&serde_json::json!({"email": "demo@g5labs.ai"}))
        .send()
        .await
        .unwrap();
    assert!(token.status().is_success());
    let org_ids: Vec<Uuid> =
        sqlx::query_scalar("SELECT id FROM orgs WHERE slug IN ('g5labs', 'g5labs-member')")
            .fetch_all(&app.db)
            .await
            .unwrap();
    for org_id in org_ids {
        helpers::seed_org_llm_credential(&app, org_id).await;
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let sql_url = format!("http://{}", listener.local_addr().unwrap());
    let sql_router = axum::Router::new()
        .route("/", axum::routing::post(fixture_sql))
        .with_state(app.db.clone());
    let sql_server = tokio::spawn(async move {
        axum::serve(listener, sql_router).await.unwrap();
    });
    // The seed writes `<base>/<org-id>/<repo-id>`: hand it the storage base,
    // two levels above any keyed repository path.
    let repo_path = app.repo_storage.test_path(Uuid::new_v4(), Uuid::new_v4());
    let storage_base = repo_path.parent().unwrap().parent().unwrap();
    let output = tokio::process::Command::new("python3")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/fixtures/seed-codex/run.py"
        ))
        .env("SEED_TEST_API", &app.base_url)
        .env("SEED_TEST_DB", sql_url)
        .env("SEED_TEST_REPOS", storage_base)
        .output()
        .await
        .expect("run actual seed against isolated API/Postgres/Git");
    sql_server.abort();
    assert!(
        output.status.success(),
        "seed failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["requests"].as_array().unwrap().len(), 3);
    let manifest: g5d::cli::config::WorkspaceConfig =
        serde_yaml::from_str(result["manifest"].as_str().unwrap()).unwrap();
    let workspace_id: Uuid = sqlx::query_scalar("SELECT id FROM workspaces WHERE name = 'codex'")
        .fetch_one(&app.db)
        .await
        .unwrap();
    let docs_repo_id: Uuid = sqlx::query_scalar(
        "SELECT repo_id FROM workspace_repos WHERE workspace_id = $1 AND role = 'docs'",
    )
    .bind(workspace_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    let docs = manifest
        .docs
        .expect("manifest must declare the docs member");
    assert_eq!(
        docs.repo_id.as_deref(),
        Some(docs_repo_id.to_string().as_str())
    );
    assert_eq!(docs.branch, "codex");
    assert_eq!(docs.path, "repos/codex-docs");
    assert_eq!(manifest.repos.len(), 1);
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM docs WHERE workspace_id = $1 AND doc_type = 'explainer' AND repo_id = $2",
    ).bind(workspace_id).bind(docs_repo_id).fetch_one(&app.db).await.unwrap();
    assert_eq!(count, 1, "reruns preserve the pre-existing explainer");
    for request in result["requests"].as_array().unwrap() {
        let task_id: Uuid = request["taskId"].as_str().unwrap().parse().unwrap();
        let (kind, session_id, content): (String, Uuid, String) =
            sqlx::query_as("SELECT t.kind, t.chat_session_id, i.content FROM tasks t
                            JOIN session_inputs i ON i.task_id=t.id WHERE t.id=$1 AND t.workspace_id=$2")
                .bind(task_id).bind(workspace_id).fetch_one(&app.db).await.unwrap();
        assert_eq!(kind, "agent");
        assert_eq!(
            session_id.to_string(),
            request["session"]["id"].as_str().unwrap()
        );
        assert!(content.contains("Create an explainer:"));
        assert!(content.contains(&format!("docs/{workspace_id}/explainers/")));
    }
    let docs_path = app.repo_path(docs_repo_id).await;
    assert_eq!(
        String::from_utf8(
            std::process::Command::new("git")
                .args(["show", "codex:docs/tool_system_refactor.md"])
                .current_dir(&docs_path)
                .output()
                .unwrap()
                .stdout
        )
        .unwrap()
        .trim(),
        "# Tool refactor\nPreserve this instruction content."
    );
    app.cleanup().await;
}
