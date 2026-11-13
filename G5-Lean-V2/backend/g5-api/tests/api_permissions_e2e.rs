//! Integration tests for the Permissions API under the split runtime-role
//! setup used by prod-parity CI.

use crate::helpers;

use g5_auth::Scope;
use helpers::{TestApp, TestOrg, TestUser, TestWorkspace};
use reqwest::StatusCode;
use serde_json::{json, Value};
use uuid::Uuid;

async fn seed_org_default_scope(app: &TestApp, org_id: Uuid, scope: &str) {
    sqlx::query(
        "INSERT INTO oauth_scopes (scope, description, category) VALUES ($1, $2, $3) ON CONFLICT (scope) DO NOTHING",
    )
    .bind(scope)
    .bind(format!("Test scope for {scope}"))
    .bind("test")
    .execute(&app.db)
    .await
    .expect("Failed to seed oauth scope");

    sqlx::query("INSERT INTO org_default_scopes (org_id, scope) VALUES ($1, $2)")
        .bind(org_id)
        .bind(scope)
        .execute(&app.db)
        .await
        .expect("Failed to seed org default scope");
}

#[tokio::test]
async fn test_get_my_scopes_returns_effective_scopes() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    seed_org_default_scope(&app, org.id, "workspaces:read").await;
    let client = app.authenticated_client(&user);

    let response = client.get("/api/v1/auth/me/scopes").await;

    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    let scopes = body["data"]["scopes"]
        .as_array()
        .expect("scopes should be an array")
        .iter()
        .filter_map(|scope| scope.as_str())
        .collect::<Vec<_>>();
    assert!(scopes.contains(&"workspaces:read"));

    app.cleanup().await;
}

#[tokio::test]
async fn test_check_my_scopes_returns_allowed_result() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    seed_org_default_scope(&app, org.id, "workspaces:read").await;
    let client = app.authenticated_client(&user);

    let response = client
        .post_json(
            "/api/v1/auth/me/scopes/check",
            &json!({ "scopes": ["workspaces:read"] }),
        )
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["data"]["allowed"].as_bool(), Some(true));

    app.cleanup().await;
}

#[tokio::test]
async fn test_list_workspace_overrides_for_workspace_in_non_selected_owner_org() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let _primary_org = TestOrg::create_with_admin(&app, &mut user).await;
    let secondary_org = TestOrg::create(&app).await;
    let workspace = TestWorkspace::create(&app, &secondary_org).await;

    sqlx::query(
        r#"
        INSERT INTO org_memberships (org_id, user_id, role)
        VALUES ($1, $2, 'admin')
        ON CONFLICT (org_id, user_id) DO UPDATE SET role = 'admin'
        "#,
    )
    .bind(secondary_org.id)
    .bind(user.id)
    .execute(&app.db)
    .await
    .expect("Failed to insert secondary owner membership");

    let client = app.authenticated_client(&user);
    let response = client
        .get(&format!("/api/v1/workspaces/{}/permissions", workspace.id))
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    assert_eq!(
        body["data"]
            .as_array()
            .expect("data should be an array")
            .len(),
        0
    );

    app.cleanup().await;
}

#[tokio::test]
async fn test_list_workspace_overrides_rejects_tombstoned_workspace() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;

    sqlx::query("UPDATE workspaces SET deleted_at = now(), updated_at = now() WHERE id = $1")
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .expect("tombstone workspace");

    let client = app.authenticated_client(&user);
    let response = client
        .get(&format!("/api/v1/workspaces/{}/permissions", workspace.id))
        .await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    app.cleanup().await;
}
