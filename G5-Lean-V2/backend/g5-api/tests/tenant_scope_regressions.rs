//! Multi-org selected-org regression coverage.
//!
//! These tests exercise the production route paths that previously derived
//! tenant scope from the selected org embedded in the JWT instead of the target
//! workspace/entity. The fixture intentionally uses a JWT selected to a primary
//! org while every target entity belongs to a second org that the same user is
//! authorized to access.

use crate::helpers;

use g5_api::ws::channels::{authorize_subscribe, Channel};
use g5_auth::{JwtClaims, Scope};
use helpers::{AuthenticatedClient, TestApp, TestOrg, TestTask, TestUser, TestWorkspace};
use reqwest::StatusCode;
use serde_json::{json, Value};
use uuid::Uuid;

struct MultiOrgFixture {
    primary_org: TestOrg,
    secondary_org: TestOrg,
    user: TestUser,
    workspace: TestWorkspace,
    client: AuthenticatedClient,
    scopes: Vec<Scope>,
}

async fn add_org_membership(app: &TestApp, org_id: Uuid, user_id: Uuid, role: &str) {
    sqlx::query(
        r#"
        INSERT INTO org_memberships (org_id, user_id, role)
        VALUES ($1, $2, $3)
        ON CONFLICT (org_id, user_id) DO UPDATE SET role = EXCLUDED.role
        "#,
    )
    .bind(org_id)
    .bind(user_id)
    .bind(role)
    .execute(&app.db)
    .await
    .expect("failed to add org membership");
}

async fn revoke_org_membership(app: &TestApp, org_id: Uuid, user_id: Uuid) {
    sqlx::query(
        r#"
        DELETE FROM org_memberships
        WHERE org_id = $1 AND user_id = $2
        "#,
    )
    .bind(org_id)
    .bind(user_id)
    .execute(&app.db)
    .await
    .expect("failed to revoke org membership");
}

fn test_scopes() -> Vec<Scope> {
    vec![
        Scope::WorkspacesRead,
        Scope::WorkspacesWrite,
        Scope::WorkspacesAdmin,
        Scope::TasksRead,
        Scope::TasksExecute,
        Scope::SessionsRead,
        Scope::SessionsWrite,
        Scope::BillingAdmin,
        Scope::OrgRead,
        Scope::OrgAdmin,
        Scope::SettingsRead,
        Scope::SettingsWrite,
    ]
}

fn claims_for(user: &TestUser, selected_org_id: Uuid, scopes: &[Scope]) -> JwtClaims {
    JwtClaims {
        sub: user.id.to_string(),
        org: selected_org_id.to_string(),
        scopes: scopes.to_vec(),
        iat: 0,
        exp: 4_102_444_800,
        iss: None,
        aud: None,
        jti: None,
        token_use: None,
        client: None,
    }
}

fn constraint_name(err: &sqlx::Error) -> Option<&str> {
    err.as_database_error()
        .and_then(|database_error| database_error.constraint())
}

async fn setup_multiorg_fixture(app: &TestApp) -> MultiOrgFixture {
    let primary_org = TestOrg::create(app).await;
    let secondary_org = TestOrg::create(app).await;
    let scopes = test_scopes();
    let user = TestUser::create_with_scopes(app, primary_org.id, &scopes).await;

    add_org_membership(app, secondary_org.id, user.id, "admin").await;

    let workspace = TestWorkspace::create(app, &secondary_org).await;
    let client = app.authenticated_client(&user);

    MultiOrgFixture {
        primary_org,
        secondary_org,
        user,
        workspace,
        client,
        scopes,
    }
}

async fn create_line_comment(
    client: &AuthenticatedClient,
    workspace_id: Uuid,
    repo_id: Uuid,
    content: &str,
) -> Uuid {
    use sha2::{Digest, Sha256};
    let read = client
        .get(&format!(
            "/api/v1/workspaces/{workspace_id}/repos/{repo_id}/files/README.md"
        ))
        .await;
    assert_eq!(read.status(), StatusCode::OK);
    let read: Value = read.json().await.unwrap();
    let text = read["data"]["content"].as_str().unwrap();
    let line = text.lines().next().unwrap();
    let response = client
        .post_json(
            &format!("/api/v1/workspaces/{workspace_id}/comments"),
            &json!({
                "scope": "line",
                "content": content,
                "repoId": repo_id,
                "filePath": "README.md",
                "lineNumber": 1,
                "commitSha": read["data"]["repoOid"],
                "lineHash": format!("{:x}", Sha256::digest(line.as_bytes())),
                "contentHash": format!("{:x}", Sha256::digest(text.as_bytes())),
                "anchorContext": json!({"before":[],"line":line,"after":[]}).to_string()
            }),
        )
        .await;
    let status = response.status();
    let response_text = response.text().await.expect("comment response body");
    assert_eq!(status, StatusCode::CREATED, "{response_text}");

    let body: Value = serde_json::from_str(&response_text).expect("comment response JSON");
    body["data"]["id"]
        .as_str()
        .expect("comment id should be present")
        .parse()
        .expect("comment id should be a UUID")
}

async fn create_chat_session(client: &AuthenticatedClient, workspace_id: Uuid) -> Uuid {
    let response = client
        .post_json(
            &format!("/api/v1/workspaces/{workspace_id}/sessions"),
            &json!({
                "scopeType": "workspace",
                "title": "Selected org mismatch regression"
            }),
        )
        .await;
    let status = response.status();
    let response_text = response.text().await.expect("session response body");
    assert_eq!(status, StatusCode::CREATED, "{response_text}");

    let body: Value = serde_json::from_str(&response_text).expect("session response JSON");
    body["data"]["id"]
        .as_str()
        .expect("session id should be present")
        .parse()
        .expect("session id should be a UUID")
}

async fn insert_notification(app: &TestApp, workspace_id: Uuid, recipient_user_id: Uuid) -> Uuid {
    sqlx::query_scalar(
        r#"
        INSERT INTO notifications (
            workspace_id, recipient_user_id, category, severity, title, message, actions
        )
        VALUES ($1, $2, 'mention', 'info', 'Mentioned', 'You were mentioned', '[]'::jsonb)
        RETURNING id
        "#,
    )
    .bind(workspace_id)
    .bind(recipient_user_id)
    .fetch_one(&app.db)
    .await
    .expect("failed to insert notification")
}

async fn grant_workspace_override(
    app: &TestApp,
    workspace_id: Uuid,
    user_id: Uuid,
    granted_by: Uuid,
    scope: &str,
) {
    sqlx::query(
        r#"
        INSERT INTO oauth_scopes (scope, description, category)
        VALUES ($1, $2, 'test')
        ON CONFLICT (scope) DO NOTHING
        "#,
    )
    .bind(scope)
    .bind(format!("Test scope for {scope}"))
    .execute(&app.db)
    .await
    .expect("failed to seed oauth scope");

    sqlx::query(
        r#"
        INSERT INTO workspace_permission_overrides (workspace_id, user_id, scope, granted_by)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (workspace_id, user_id, scope) DO NOTHING
        "#,
    )
    .bind(workspace_id)
    .bind(user_id)
    .bind(scope)
    .bind(granted_by)
    .execute(&app.db)
    .await
    .expect("failed to insert workspace permission override");
}

fn assert_denied_after_revocation(status: StatusCode) {
    assert!(
        status == StatusCode::FORBIDDEN || status == StatusCode::NOT_FOUND,
        "expected access to be denied after revocation, got {status}"
    );
}

#[tokio::test]
async fn selected_org_mismatch_workspace_routes_authorize_against_target_entity() {
    let app = TestApp::spawn().await;
    let f = setup_multiorg_fixture(&app).await;

    let comment_id = create_line_comment(
        &f.client,
        f.workspace.id,
        f.workspace.repo_id,
        "target-org comment",
    )
    .await;
    let comments = f
        .client
        .get(&format!("/api/v1/workspaces/{}/comments", f.workspace.id))
        .await;
    assert_eq!(comments.status(), StatusCode::OK);
    let comment_body: Value = comments.json().await.expect("comments response JSON");
    assert!(comment_body["data"]
        .as_array()
        .expect("comments data array")
        .iter()
        .any(|comment| comment["id"].as_str() == Some(&comment_id.to_string())));

    let resolve_comment = f
        .client
        .patch_json(
            &format!("/api/v1/comments/{comment_id}"),
            &json!({ "resolved": true }),
        )
        .await;
    assert_eq!(resolve_comment.status(), StatusCode::OK);

    let task = TestTask::create(&app, &f.workspace).await;
    let response = f
        .client
        .get(&format!("/api/v1/workspaces/{}/tasks", f.workspace.id))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    assert!(body["data"]["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["id"] == task.id.to_string()));

    let session_id = create_chat_session(&f.client, f.workspace.id).await;
    let sessions = f
        .client
        .get(&format!("/api/v1/workspaces/{}/sessions", f.workspace.id))
        .await;
    assert_eq!(sessions.status(), StatusCode::OK);

    let session_detail = f
        .client
        .get(&format!("/api/v1/sessions/{session_id}"))
        .await;
    assert_eq!(session_detail.status(), StatusCode::OK);

    let notification_id = insert_notification(&app, f.workspace.id, f.user.id).await;
    let notifications = f
        .client
        .get(&format!(
            "/api/v1/workspaces/{}/notifications",
            f.workspace.id
        ))
        .await;
    assert_eq!(notifications.status(), StatusCode::OK);

    let mark_notification = f
        .client
        .patch_json(
            &format!("/api/v1/notifications/{notification_id}"),
            &json!({}),
        )
        .await;
    assert_eq!(mark_notification.status(), StatusCode::OK);

    let spend_history = f
        .client
        .get(&format!(
            "/api/v1/workspaces/{}/settings/spend/history",
            f.workspace.id
        ))
        .await;
    assert_eq!(spend_history.status(), StatusCode::OK);

    grant_workspace_override(
        &app,
        f.workspace.id,
        f.user.id,
        f.user.id,
        Scope::TasksExecute.as_str(),
    )
    .await;
    let scopes = f
        .client
        .get(&format!(
            "/api/v1/auth/me/scopes?workspaceId={}",
            f.workspace.id
        ))
        .await;
    assert_eq!(scopes.status(), StatusCode::OK);
    let scopes_body: Value = scopes.json().await.expect("scopes response JSON");
    assert!(scopes_body["data"]["workspaceOverrides"]
        .as_array()
        .expect("workspaceOverrides should be an array")
        .iter()
        .any(|scope| scope.as_str() == Some(Scope::TasksExecute.as_str())));

    let claims = claims_for(&f.user, f.primary_org.id, &f.scopes);
    authorize_subscribe(
        &app.state,
        f.user.id,
        &claims,
        &Channel::Workspace(f.workspace.id),
    )
    .await
    .expect("workspace WS channel should authorize from target workspace org");
    authorize_subscribe(&app.state, f.user.id, &claims, &Channel::Task(task.id))
        .await
        .expect("task WS channel should authorize from target task org");
    authorize_subscribe(&app.state, f.user.id, &claims, &Channel::Chat(session_id))
        .await
        .expect("chat WS channel should authorize from target session org");
    app.cleanup().await;
}

#[tokio::test]
async fn membership_revocation_blocks_warmed_rest_and_ws_access() {
    let app = TestApp::spawn().await;
    let f = setup_multiorg_fixture(&app).await;
    let comment_id = create_line_comment(
        &f.client,
        f.workspace.id,
        f.workspace.repo_id,
        "before revocation",
    )
    .await;

    let warm_rest = f
        .client
        .get(&format!("/api/v1/workspaces/{}/comments", f.workspace.id))
        .await;
    assert_eq!(warm_rest.status(), StatusCode::OK);

    let claims = claims_for(&f.user, f.primary_org.id, &f.scopes);
    authorize_subscribe(
        &app.state,
        f.user.id,
        &claims,
        &Channel::Workspace(f.workspace.id),
    )
    .await
    .expect("warm WS authorization should pass before membership revocation");

    // The current owner cannot leave the org. Transfer the fixture workspace
    // to another eligible member before exercising ordinary read revocation.
    let replacement_owner = TestUser::create_with_scopes(
        &app,
        f.secondary_org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    sqlx::query("UPDATE workspaces SET owner_user_id = $2 WHERE id = $1")
        .bind(f.workspace.id)
        .bind(replacement_owner.id)
        .execute(&app.db)
        .await
        .expect("transfer fixture workspace before membership revocation");
    revoke_org_membership(&app, f.secondary_org.id, f.user.id).await;

    let list_after_revoke = f
        .client
        .get(&format!("/api/v1/workspaces/{}/comments", f.workspace.id))
        .await;
    assert_denied_after_revocation(list_after_revoke.status());

    let patch_after_revoke = f
        .client
        .patch_json(
            &format!("/api/v1/comments/{comment_id}"),
            &json!({ "resolved": false }),
        )
        .await;
    assert_denied_after_revocation(patch_after_revoke.status());

    let ws_after_revoke = authorize_subscribe(
        &app.state,
        f.user.id,
        &claims,
        &Channel::Workspace(f.workspace.id),
    )
    .await;
    assert!(
        ws_after_revoke.is_err(),
        "WS authorization should re-check current membership after revocation"
    );

    app.cleanup().await;
}

#[tokio::test]
async fn workspace_pull_request_tenant_fks_reject_cross_org_rows() {
    let app = TestApp::spawn().await;
    let org_a = TestOrg::create(&app).await;
    let org_b = TestOrg::create(&app).await;
    let source = TestWorkspace::create(&app, &org_a).await;
    let same_org_target = TestWorkspace::create(&app, &org_a).await;
    let other_org_target = TestWorkspace::create(&app, &org_b).await;

    let pull_request_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO workspace_pull_requests (
            org_id, source_workspace_id, target_workspace_id, local_merge_status,
            title, source_heads, target_heads, preflight, last_preflight_fingerprint
        )
        VALUES ($1, $2, $3, 'ready', 'same org PR', '[]'::jsonb, '[]'::jsonb, '{}'::jsonb, 'fp')
        RETURNING id
        "#,
    )
    .bind(org_a.id)
    .bind(source.id)
    .bind(same_org_target.id)
    .fetch_one(&app.db)
    .await
    .expect("same-org workspace PR should insert");

    sqlx::query(
        r#"
        INSERT INTO workspace_pull_request_events (
            org_id, pull_request_id, event_type, source_heads, target_heads, preflight
        )
        VALUES ($1, $2, 'created', '[]'::jsonb, '[]'::jsonb, '{}'::jsonb)
        "#,
    )
    .bind(org_a.id)
    .bind(pull_request_id)
    .execute(&app.db)
    .await
    .expect("same-org workspace PR event should insert");

    let cross_workspace_err = sqlx::query(
        r#"
        INSERT INTO workspace_pull_requests (
            org_id, source_workspace_id, target_workspace_id, local_merge_status,
            title, source_heads, target_heads, preflight, last_preflight_fingerprint
        )
        VALUES ($1, $2, $3, 'ready', 'cross org PR', '[]'::jsonb, '[]'::jsonb, '{}'::jsonb, 'fp')
        "#,
    )
    .bind(org_a.id)
    .bind(source.id)
    .bind(other_org_target.id)
    .execute(&app.db)
    .await
    .expect_err("cross-org target workspace should fail the composite FK");
    assert_eq!(
        constraint_name(&cross_workspace_err),
        Some("fk_wpr_target_workspace_same_org")
    );

    let cross_event_err = sqlx::query(
        r#"
        INSERT INTO workspace_pull_request_events (
            org_id, pull_request_id, event_type, source_heads, target_heads, preflight
        )
        VALUES ($1, $2, 'created', '[]'::jsonb, '[]'::jsonb, '{}'::jsonb)
        "#,
    )
    .bind(org_b.id)
    .bind(pull_request_id)
    .execute(&app.db)
    .await
    .expect_err("cross-org PR event should fail the composite FK");
    assert_eq!(
        constraint_name(&cross_event_err),
        Some("fk_wpre_pull_request_same_org")
    );

    app.cleanup().await;
}
