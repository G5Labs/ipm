use crate::helpers::{TestApp, TestOrg, TestTask, TestUser, TestWorkspace};
use reqwest::StatusCode;
use serde_json::Value;
use uuid::Uuid;

#[tokio::test]
async fn workspace_task_pages_are_bounded_stable_and_authorized_without_jobs() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let other = TestWorkspace::create(&app, &org).await;
    let foreign = TestTask::create(&app, &other).await;
    let client = app.authenticated_client(&owner);
    let mut expected = Vec::new();
    for _ in 0..5 {
        expected.push(TestTask::create(&app, &workspace).await.id);
    }
    sqlx::query("UPDATE tasks SET created_at='2026-01-01T00:00:00Z',status='succeeded' WHERE workspace_id=$1").bind(workspace.id).execute(&app.db).await.unwrap();
    expected.sort_by(|a, b| b.cmp(a));
    let first = client
        .get(&format!(
            "/api/v1/workspaces/{}/tasks?limit=2",
            workspace.id
        ))
        .await;
    assert_eq!(first.status(), StatusCode::OK);
    let first: Value = first.json().await.unwrap();
    assert_eq!(first["data"]["tasks"].as_array().unwrap().len(), 2);
    assert_eq!(first["data"]["hasMore"], true);
    let newer = TestTask::create(&app, &workspace).await;
    let mut found: Vec<Uuid> = first["data"]["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["id"].as_str().unwrap().parse().unwrap())
        .collect();
    let mut cursor = first["data"]["cursor"].as_str().unwrap().to_owned();
    loop {
        let encoded: String = url::form_urlencoded::byte_serialize(cursor.as_bytes()).collect();
        let response = client
            .get(&format!(
                "/api/v1/workspaces/{}/tasks?limit=2&cursor={encoded}",
                workspace.id
            ))
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        let page: Value = response.json().await.unwrap();
        let tasks = page["data"]["tasks"].as_array().unwrap();
        assert!(tasks.len() <= 2);
        found.extend(
            tasks
                .iter()
                .map(|t| t["id"].as_str().unwrap().parse::<Uuid>().unwrap()),
        );
        if page["data"]["hasMore"] == false {
            assert!(page["data"]["cursor"].is_null());
            break;
        }
        cursor = page["data"]["cursor"].as_str().unwrap().to_owned();
    }
    assert_eq!(
        found, expected,
        "ties and a newly inserted task must not skip or repeat older tasks"
    );
    assert!(!found.contains(&foreign.id));
    let response = client
        .get(&format!(
            "/api/v1/workspaces/{}/tasks?activeOnly=true",
            workspace.id
        ))
        .await;
    let active: Value = response.json().await.unwrap();
    assert_eq!(active["data"]["tasks"].as_array().unwrap().len(), 1);
    assert_eq!(active["data"]["tasks"][0]["id"], newer.id.to_string());
    let oldest = expected.last().unwrap();
    let response = client.get(&format!("/api/v1/tasks/{oldest}")).await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "detail does not depend on the loaded page"
    );
    let detail: Value = response.json().await.unwrap();
    assert_eq!(detail["data"]["workspaceId"], workspace.id.to_string());
    assert!(detail["data"].get("jobId").is_none());
    assert_eq!(
        client
            .get(&format!(
                "/api/v1/workspaces/{}/tasks?cursor=invalid",
                workspace.id
            ))
            .await
            .status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let outsider = TestUser::create(&app).await;
    let response = app
        .authenticated_client(&outsider)
        .get(&format!("/api/v1/workspaces/{}/tasks", workspace.id))
        .await;
    assert!(matches!(
        response.status(),
        StatusCode::FORBIDDEN | StatusCode::NOT_FOUND
    ));
    assert_eq!(
        client
            .get(&format!("/api/v1/workspaces/{}/jobs", workspace.id))
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    app.cleanup().await;
}
