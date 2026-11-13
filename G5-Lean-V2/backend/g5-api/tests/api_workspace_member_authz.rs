//! Integration tests for current-owner workspace administration.
//!
//! Mutation-policy writes retain owner authority. Retired membership routes
//! refuse everyone, including workspace owners and organization administrators.

use crate::helpers;
use g5_auth::Scope;

use helpers::{TestApp, TestOrg, TestUser, TestWorkspace};
use reqwest::StatusCode;
use serde_json::json;
use uuid::Uuid;

async fn seed_org_workspace(app: &TestApp, org: &TestOrg, owner_user_id: Uuid) -> Uuid {
    let workspace = TestWorkspace::create(app, org).await;
    sqlx::query("UPDATE workspaces SET owner_user_id = $2, created_by = $2 WHERE id = $1")
        .bind(workspace.id)
        .bind(owner_user_id)
        .execute(&app.db)
        .await
        .expect("assign workspace owner");
    workspace.id
}

async fn promote_org_role(app: &TestApp, org_id: Uuid, user_id: Uuid, role: &str) {
    sqlx::query("UPDATE org_memberships SET role = $1 WHERE org_id = $2 AND user_id = $3")
        .bind(role)
        .bind(org_id)
        .bind(user_id)
        .execute(&app.db)
        .await
        .expect("promote org membership role");
}

async fn workspace_mutation_policy(app: &TestApp, ws_id: Uuid) -> String {
    sqlx::query_scalar::<_, String>("SELECT mutation_policy FROM workspaces WHERE id = $1")
        .bind(ws_id)
        .fetch_one(&app.db)
        .await
        .expect("load workspace mutation_policy")
}

#[tokio::test]
async fn former_creator_cannot_self_grant_workspace_admin_or_freeze() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let owner = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let former_creator = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let ws = seed_org_workspace(&app, &org, owner.id).await;
    sqlx::query("UPDATE workspaces SET created_by = $2 WHERE id = $1")
        .bind(ws)
        .bind(former_creator.id)
        .execute(&app.db)
        .await
        .expect("record former creator provenance");
    let client = app.authenticated_client(&former_creator);

    let control = client
        .post_json(
            &format!("/api/v1/workspaces/{ws}/mutation-policy"),
            &json!({ "policy": "read_only", "mode": "require_quiescent" }),
        )
        .await;
    assert_eq!(control.status(), StatusCode::FORBIDDEN);

    let self_grant = client
        .post_json(
            &format!("/api/v1/workspaces/{ws}/members"),
            &json!({ "userId": former_creator.id.to_string(), "role": "admin" }),
        )
        .await;
    assert_eq!(self_grant.status(), StatusCode::NOT_FOUND);
    assert_eq!(workspace_mutation_policy(&app, ws).await, "read_write");

    app.cleanup().await;
}

#[tokio::test]
async fn non_owner_cannot_remove_workspace_member() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let owner = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let member = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let victim = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let ws = seed_org_workspace(&app, &org, owner.id).await;

    let response = app
        .authenticated_client(&member)
        .delete(&format!("/api/v1/workspaces/{ws}/members/{}", victim.id))
        .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    app.cleanup().await;
}

#[tokio::test]
async fn org_admin_cannot_restore_removed_workspace_invitations() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let org_admin = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    promote_org_role(&app, org.id, org_admin.id, "admin").await;
    let workspace_owner =
        TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let target = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let ws = seed_org_workspace(&app, &org, workspace_owner.id).await;

    let response = app
        .authenticated_client(&org_admin)
        .post_json(
            &format!("/api/v1/workspaces/{ws}/members"),
            &json!({ "userId": target.id.to_string(), "role": "editor" }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    app.cleanup().await;
}

#[tokio::test]
async fn current_owner_cannot_invite_but_can_freeze_workspace() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let owner = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let target = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let ws = seed_org_workspace(&app, &org, owner.id).await;
    let client = app.authenticated_client(&owner);

    let add = client
        .post_json(
            &format!("/api/v1/workspaces/{ws}/members"),
            &json!({ "userId": target.id.to_string(), "role": "editor" }),
        )
        .await;
    assert_eq!(add.status(), StatusCode::NOT_FOUND);

    let freeze = client
        .post_json(
            &format!("/api/v1/workspaces/{ws}/mutation-policy"),
            &json!({ "policy": "read_only", "mode": "require_quiescent" }),
        )
        .await;
    assert_eq!(freeze.status(), StatusCode::OK);
    assert_eq!(workspace_mutation_policy(&app, ws).await, "read_only");

    app.cleanup().await;
}
