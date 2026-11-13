//! Retired workflow launch endpoints cannot create work. Current UI presets use sessions/start.
use crate::helpers;
use helpers::{TestApp, TestOrg, TestUser, TestWorkspace};
use reqwest::StatusCode;
use serde_json::json;

#[tokio::test]
async fn removed_workflow_endpoints_cannot_create_work() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let client = app.authenticated_client(&owner);
    let response = client
        .post_json(
            &format!("/api/v1/workspaces/{}/jobs", workspace.id),
            &json!({"jobType":"sync"}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    for suffix in ["docs/explainers", "docs/views", "docs/any-document/sync"] {
        let response = client
            .post_json(
                &format!("/api/v1/workspaces/{}/{suffix}", workspace.id),
                &json!({}),
            )
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
    for table in ["tasks", "chat_sessions", "docs"] {
        let count: i64 = sqlx::query_scalar(&format!(
            "SELECT count(*) FROM {table} WHERE workspace_id=$1"
        ))
        .bind(workspace.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
        assert_eq!(count, 0, "{table}");
    }
    app.cleanup().await;
}
