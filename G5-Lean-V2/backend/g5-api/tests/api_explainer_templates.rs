//! Integration tests for `/api/v1/workspaces/:workspaceId/repos/:repoId/explainer-templates`.
//!
//! Reads merge the six built-ins with the durable Git projection. Saves use
//! the attached owner's live dispatcher and remain uncommitted until the
//! workspace lifecycle flushes them.

use crate::helpers;
use g5_auth::Scope;

use helpers::{
    assert_json_error, TestApp, TestOrg, TestUser, TestWorkspace, TestWorkspaceRuntimePeer,
};
use reqwest::StatusCode;
use serde_json::{json, Value};

const BUILTIN_SLUGS: [&str; 6] = [
    "big-picture",
    "balanced",
    "deep-technical",
    "corner-cases",
    "demo-walkthrough",
    "new-hire-onboarding",
];

async fn owned_workspace(app: &TestApp, org: &TestOrg) -> TestWorkspace {
    TestWorkspace::create(app, org).await
}

#[tokio::test]
async fn list_contains_the_six_builtin_templates() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let workspace = owned_workspace(&app, &org).await;
    let client = app.authenticated_client(&user);

    let response = client
        .get(&format!(
            "/api/v1/workspaces/{}/repos/{}/explainer-templates",
            workspace.id, workspace.repo_id
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("template list JSON");
    let templates = body["data"].as_array().expect("template array");
    assert_eq!(templates.len(), BUILTIN_SLUGS.len());
    for (template, expected_slug) in templates.iter().zip(BUILTIN_SLUGS) {
        assert_eq!(template["slug"].as_str(), Some(expected_slug));
        assert_eq!(template["source"].as_str(), Some("builtin"));
        assert!(!template["name"].as_str().unwrap_or_default().is_empty());
        assert!(template["prompt"]
            .as_str()
            .unwrap_or_default()
            .starts_with("Objective:"));
    }

    app.cleanup().await;
}

#[tokio::test]
async fn save_writes_live_checkout_without_committing() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let workspace = owned_workspace(&app, &org).await;
    let base_sha = workspace.head_commit.clone();
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, user.id).await;
    let client = app.authenticated_client(&user);

    let response = client
        .post_json(
            &format!(
                "/api/v1/workspaces/{}/repos/{}/explainer-templates",
                workspace.id, workspace.repo_id
            ),
            &json!({
                "name": "My Style",
                "slug": "my-style",
                "prompt": "Objective: my custom brief.\n- Length: brief."
            }),
        )
        .await;
    let status = response.status();
    let body: Value = response.json().await.expect("save response JSON");
    assert_eq!(status, StatusCode::OK, "save response: {body}");
    assert_eq!(body["data"]["headCommit"], base_sha);
    assert_eq!(body["data"]["template"]["slug"], "my-style");

    let live_file = runtime
        .file_content(workspace.repo_id, ".g5/explainer_templates/my-style.yaml")
        .await
        .expect("live template file");
    assert!(live_file.contains("Objective: my custom brief."));

    let durable_head: String = workspace.git_head(&app);
    assert_eq!(
        durable_head, base_sha,
        "Save must not commit or advance Git"
    );

    let response = client
        .get(&format!(
            "/api/v1/workspaces/{}/repos/{}/explainer-templates",
            workspace.id, workspace.repo_id
        ))
        .await;
    let body: Value = response.json().await.expect("durable list JSON");
    let templates = body["data"].as_array().expect("template array");
    assert_eq!(templates.len(), BUILTIN_SLUGS.len());
    assert!(
        templates
            .iter()
            .all(|template| template["slug"].as_str() != Some("my-style")),
        "an unflushed live file must not appear in the durable list"
    );

    drop(runtime);
    app.cleanup().await;
}

#[tokio::test]
async fn overlapping_creates_cannot_silently_overwrite_each_other() {
    let mut config = helpers::test_config();
    config.database.max_connections = 8;
    config.tenant_database.max_connections = 8;
    let app = TestApp::spawn_with_config(config).await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let workspace = owned_workspace(&app, &org).await;
    let rel_path = ".g5/explainer_templates/race-style.yaml";
    let runtime =
        TestWorkspaceRuntimePeer::attach_with_read_barrier(&app, &workspace, user.id, rel_path, 2)
            .await;
    let path = format!(
        "/api/v1/workspaces/{}/repos/{}/explainer-templates",
        workspace.id, workspace.repo_id
    );
    let client_a = app.authenticated_client(&user);
    let client_b = app.authenticated_client(&user);
    let body_a = json!({
        "name": "Race Style",
        "slug": "race-style",
        "prompt": "Objective: prompt A."
    });
    let body_b = json!({
        "name": "Race Style",
        "slug": "race-style",
        "prompt": "Objective: prompt B."
    });
    let request_a = client_a.post_json(&path, &body_a);
    let request_b = client_b.post_json(&path, &body_b);
    let (response_a, response_b) = tokio::join!(request_a, request_b);
    let status_a = response_a.status();
    let status_b = response_b.status();
    let body_a: Value = response_a.json().await.expect("response A JSON");
    let body_b: Value = response_b.json().await.expect("response B JSON");

    let successes = [status_a, status_b]
        .into_iter()
        .filter(|status| *status == StatusCode::OK)
        .count();
    let conflicts = [status_a, status_b]
        .into_iter()
        .filter(|status| *status == StatusCode::CONFLICT)
        .count();
    assert_eq!(
        (successes, conflicts),
        (1, 1),
        "exactly one create must win: A={status_a} {body_a}, B={status_b} {body_b}"
    );
    let conflict = if status_a == StatusCode::CONFLICT {
        &body_a
    } else {
        &body_b
    };
    assert_eq!(conflict["error"]["code"], "CONFLICT");
    let stored = runtime
        .file_content(workspace.repo_id, rel_path)
        .await
        .expect("one winning live file");
    assert!(
        stored.contains("Objective: prompt A.") ^ stored.contains("Objective: prompt B."),
        "the live file must contain exactly one complete winner: {stored}"
    );

    drop(runtime);
    app.cleanup().await;
}

#[tokio::test]
async fn duplicate_requires_overwrite_and_replaces_live_file() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let workspace = owned_workspace(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, user.id).await;
    let client = app.authenticated_client(&user);
    let path = format!(
        "/api/v1/workspaces/{}/repos/{}/explainer-templates",
        workspace.id, workspace.repo_id
    );

    let response = client
        .post_json(
            &path,
            &json!({"name": "First", "slug": "dup-style", "prompt": "Objective: first."}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);

    let response = client
        .post_json(
            &path,
            &json!({"name": "Second", "slug": "dup-style", "prompt": "Objective: second."}),
        )
        .await;
    assert_json_error(response, StatusCode::CONFLICT, "CONFLICT").await;

    let response = client
        .post_json(
            &path,
            &json!({
                "name": "Second",
                "slug": "dup-style",
                "prompt": "Objective: second.",
                "overwrite": true
            }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let stored = runtime
        .file_content(workspace.repo_id, ".g5/explainer_templates/dup-style.yaml")
        .await
        .expect("overwritten live file");
    assert!(stored.contains("name: Second"));
    assert!(stored.contains("Objective: second."));
    assert!(!stored.contains("Objective: first."));

    drop(runtime);
    app.cleanup().await;
}

#[tokio::test]
async fn reserved_or_invalid_slugs_fail_before_runtime_io() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let workspace = owned_workspace(&app, &org).await;
    let base_sha = workspace.head_commit.clone();
    let client = app.authenticated_client(&user);
    let path = format!(
        "/api/v1/workspaces/{}/repos/{}/explainer-templates",
        workspace.id, workspace.repo_id
    );

    for slug in BUILTIN_SLUGS {
        for overwrite in [false, true] {
            let response = client
                .post_json(
                    &path,
                    &json!({
                        "name": "Shadow",
                        "slug": slug,
                        "prompt": "Objective: shadow.",
                        "overwrite": overwrite
                    }),
                )
                .await;
            assert_json_error(
                response,
                StatusCode::UNPROCESSABLE_ENTITY,
                "VALIDATION_ERROR",
            )
            .await;
        }
    }
    for slug in ["", "Has-Upper", "has_underscore", "has.dot", "has/slash"] {
        let response = client
            .post_json(
                &path,
                &json!({"name": "Bad", "slug": slug, "prompt": "Objective: bad."}),
            )
            .await;
        assert_json_error(
            response,
            StatusCode::UNPROCESSABLE_ENTITY,
            "VALIDATION_ERROR",
        )
        .await;
    }

    let branch_head: String = workspace.git_head(&app);
    assert_eq!(branch_head, base_sha);
    let vm_rows: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM workspace_vms WHERE workspace_id = $1")
            .bind(workspace.id)
            .fetch_one(&app.db)
            .await
            .expect("workspace VM count");
    assert_eq!(vm_rows, 0, "validation failures must not start a workspace");

    app.cleanup().await;
}
