//! Security regression tests for remote-tracking write routes.
//!
//! These tests exercise the live HTTP route/middleware/service path against an
//! isolated Postgres database. They specifically prove that read-only workspace
//! members cannot publish to the customer remote or change publication mode.

use crate::helpers;

use g5_auth::Scope;
use g5_contracts::types::WorkspaceRepoKey;
use helpers::{TestApp, TestOrg, TestUser, TestWorkspace};
use reqwest::StatusCode;
use serde_json::{json, Value};

async fn enable_remote(
    app: &TestApp,
    workspace: &TestWorkspace,
    remote_branch: &str,
) -> WorkspaceRepoKey {
    let key = WorkspaceRepoKey {
        org_id: workspace.org_id,
        workspace_id: workspace.id,
        repo_id: workspace.repo_id,
    };
    sqlx::query("INSERT INTO remote_integrations (org_id,workspace_id,repo_id,remote_branch,last_sync_at) VALUES ($1,$2,$3,$4,now())")
        .bind(key.org_id).bind(key.workspace_id).bind(key.repo_id).bind(remote_branch).execute(&app.db).await.unwrap();
    let path = app.repo_path(key.repo_id).await;
    git2::Repository::open(path)
        .unwrap()
        .reference(
            &format!("refs/g5-tracking/{}/baseline", workspace.name),
            git2::Oid::from_str(&workspace.head_commit).unwrap(),
            true,
            "fixture watermark",
        )
        .unwrap();
    key
}

#[tokio::test]
async fn read_only_member_cannot_push_to_remote_or_enqueue() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let key = enable_remote(&app, &workspace, "main").await;

    let read_only = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let client = app.authenticated_client(&read_only);

    let response = client
        .post_json::<Value>(
            &format!("/api/v1/workspaces/{}/push_to_remote", workspace.id),
            &json!({}),
        )
        .await;

    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let queued: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM remote_integration_queue WHERE workspace_id = $1 AND repo_id = $2",
    )
    .bind(key.workspace_id)
    .bind(key.repo_id)
    .fetch_one(&app.db)
    .await
    .expect("queue count should be readable");
    assert_eq!(
        queued, 0,
        "read-only push_to_remote must not enqueue integration work"
    );

    app.cleanup().await;
}

#[tokio::test]
async fn approved_rebase_publish_rejects_read_only_and_cross_org_users_without_enqueueing() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let key = enable_remote(&app, &workspace, "main").await;
    let path = format!(
        "/api/v1/workspaces/{}/remote_tracking/approved_rebase_publish",
        workspace.id
    );

    let read_only = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let read_only_response = app
        .authenticated_client(&read_only)
        .post_json::<Value>(&path, &json!({}))
        .await;
    assert_eq!(read_only_response.status(), StatusCode::FORBIDDEN);

    let other_org = TestOrg::create(&app).await;
    let cross_org =
        TestUser::create_with_scopes(&app, other_org.id, &[Scope::WorkspacesWrite]).await;
    let cross_org_response = app
        .authenticated_client(&cross_org)
        .post_json::<Value>(&path, &json!({}))
        .await;
    assert_eq!(cross_org_response.status(), StatusCode::NOT_FOUND);

    let queued: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM remote_integration_queue WHERE workspace_id = $1 AND repo_id = $2",
    )
    .bind(key.workspace_id)
    .bind(key.repo_id)
    .fetch_one(&app.db)
    .await
    .expect("queue count should be readable");
    assert_eq!(
        queued, 0,
        "authorization failures must not enqueue approved publication work"
    );

    app.cleanup().await;
}

#[tokio::test]
async fn read_only_member_cannot_change_push_mode() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    enable_remote(&app, &workspace, "main").await;

    sqlx::query("UPDATE workspaces SET push_mode = 'manual' WHERE id = $1")
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .expect("seed the publication mode the read-only user will try to change");

    let read_only = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let client = app.authenticated_client(&read_only);

    let response = client
        .put_json(
            &format!("/api/v1/workspaces/{}/push_mode", workspace.id),
            &json!({ "pushMode": "auto" }),
        )
        .await;

    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let push_mode: String = sqlx::query_scalar("SELECT push_mode FROM workspaces WHERE id = $1")
        .bind(workspace.id)
        .fetch_one(&app.db)
        .await
        .expect("workspace push_mode should be readable");
    assert_eq!(
        push_mode, "manual",
        "read-only push_mode update must leave workspace state unchanged"
    );

    app.cleanup().await;
}

/// Queue D8: explicit pull/push fan-out must not resurrect a soft-deleted
/// workspace. The bulk helper is shared by both paths, so test it directly
/// against its real SQL and database constraints.
#[tokio::test]
async fn bulk_enqueue_skips_soft_deleted_workspace() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let key = enable_remote(&app, &workspace, "main").await;

    sqlx::query("UPDATE workspaces SET deleted_at = now() WHERE id = $1")
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .expect("soft-delete workspace");

    let mut conn = app.db.acquire().await.expect("acquire system connection");
    let enqueued =
        g5_api::models::branch_integration_queue::enqueue_branch_integration_for_workspace(
            &mut conn,
            workspace.id,
            g5_api::models::branch_integration_queue::trigger_kind::MANUAL,
            g5_api::models::branch_integration_queue::WorkspaceEnqueueConflict::Precedence,
        )
        .await
        .expect("soft-deleted workspace is a safe no-op");
    assert_eq!(enqueued, 0);

    let queued: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM remote_integration_queue WHERE workspace_id = $1 AND repo_id = $2",
    )
    .bind(key.workspace_id)
    .bind(key.repo_id)
    .fetch_one(&app.db)
    .await
    .expect("queue count");
    assert_eq!(queued, 0, "a deleted workspace must not receive queue work");

    app.cleanup().await;
}
