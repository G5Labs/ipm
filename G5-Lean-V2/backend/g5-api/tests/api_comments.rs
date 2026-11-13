//! Integration tests for G5 API comment endpoints.
//!
//! Covers the complete request→route→middleware→service→database→response cycle
//! for the 8 endpoints across 2 path prefixes defined in the comments routes spec (§3.5):
//!
//! **Workspace-Scoped Comment Endpoints:**
//! 1. `GET /api/v1/workspaces/:workspaceId/comments` — List comments (filterable by repoId, filePath, branch; paginated)
//! 3. `POST /api/v1/workspaces/:workspaceId/comments` — Create comment (line-scoped or node-scoped)
//!
//! **Comment-Scoped Endpoints:**
//! 4. `PATCH /api/v1/comments/:id` — Update / resolve comment
//! 5. `POST /api/v1/comments/:id/replies` — Add reply
//! 6. `DELETE /api/v1/comments/:id` — Delete comment
//!
//! Each test spawns a full `TestApp` instance with an isolated Postgres database,
//! makes real HTTP requests via `AuthenticatedClient` (or raw `reqwest::Client`
//! for unauthenticated tests), and asserts at the HTTP response level (status codes,
//! JSON body shapes) and the database level (verifying rows were created/updated/deleted).
#![allow(dead_code, unused_imports, clippy::bool_assert_comparison)]

use crate::helpers;
use g5_auth::Scope;

use helpers::{
    assert_json_error, assert_paginated_response, test_jwt_minter, AuthenticatedClient, TestApp,
    TestOrg, TestUser, TestWorkspace,
};
use reqwest::StatusCode;
use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;

const TEST_LINE_HASH: &str = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";
const TEST_CONTENT_HASH: &str = "4d6b44473c77bedb07e4593f278ab6ad63b5a701fad94a2590a4c06f30c2121d";
const TEST_ANCHOR_CONTEXT: &str = "{\"before\":[],\"line\":\"test\",\"after\":[]}";

// ===========================================================================
// Module-Level Helper Functions (specific to comment tests)
// ===========================================================================

async fn comment_workspace(app: &TestApp, org: &TestOrg) -> TestWorkspace {
    seeded_comment_workspace(app, org, None).await
}

async fn seeded_comment_workspace(
    app: &TestApp,
    org: &TestOrg,
    personal_owner: Option<Uuid>,
) -> TestWorkspace {
    let mut workspace = match personal_owner {
        Some(owner) => TestWorkspace::create_personal(app, org, owner).await,
        None => TestWorkspace::create(app, org).await,
    };
    for path in ["src/auth.rs", "docs/plan.md"] {
        workspace
            .commit_file(app, path, &"test\n".repeat(100))
            .await;
    }
    workspace
}

/// Creates a line-scoped comment via the API, returning the comment ID.
async fn create_line_comment(
    client: &AuthenticatedClient,
    workspace_id: &str,
    repo_id: Uuid,
    file_path: &str,
    line_number: i32,
    content: &str,
) -> String {
    let branches: Value = client
        .get(&format!("/api/v1/workspaces/{workspace_id}/branches"))
        .await
        .json()
        .await
        .expect("pushed vector");
    let sha = branches["data"]
        .as_array()
        .expect("repository members")
        .iter()
        .find(|h| h["repoId"] == repo_id.to_string())
        .expect("member")["headCommit"]
        .clone();
    let resp = client
        .post_json(
            &format!("/api/v1/workspaces/{}/comments", workspace_id),
            &json!({
                "scope": "line",
                "content": content,
                "repoId": repo_id,
                "filePath": file_path,
                "lineNumber": line_number,
                "commitSha": sha,
                "lineHash": TEST_LINE_HASH,
                "contentHash": TEST_CONTENT_HASH,
                "anchorContext": TEST_ANCHOR_CONTEXT
            }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::CREATED);

    let body: Value = resp.json().await.unwrap();
    body["data"]["id"].as_str().unwrap().to_string()
}

/// Inserts a line comment directly into the database (for testing cross-branch
/// discovery where we need comments on specific branches).
struct DirectLineComment<'a> {
    workspace_id: Uuid,
    repo_id: Uuid,
    author_id: Uuid,
    file_path: &'a str,
    line_number: i32,
    origin_branch: &'a str,
    content: &'a str,
}

async fn insert_line_comment_directly(app: &TestApp, comment: DirectLineComment<'_>) -> Uuid {
    let comment_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO comment_index
           (id, workspace_id, repo_id, author_id, scope, file_path, line_number,
            commit_sha, content, origin_branch, org_id)
           VALUES ($1, $2, $3, $4, 'line', $5, $6, 'abc1234', $7, $8,
                   (SELECT org_id FROM workspaces WHERE id = $2))"#,
    )
    .bind(comment_id)
    .bind(comment.workspace_id)
    .bind(comment.repo_id)
    .bind(comment.author_id)
    .bind(comment.file_path)
    .bind(comment.line_number)
    .bind(comment.content)
    .bind(comment.origin_branch)
    .execute(&app.db)
    .await
    .expect("Failed to insert comment");
    comment_id
}

async fn add_workspace_repo(app: &TestApp, workspace: &TestWorkspace, root_path: &str) -> Uuid {
    let repo_id = Uuid::new_v4();
    let slug = format!("comment-repo-{}", nanoid::nanoid!(8));
    sqlx::query(
        "INSERT INTO repos (id, org_id, name, slug, git_url, default_branch) \
         VALUES ($1, $2, $3, $3, $4, 'main')",
    )
    .bind(repo_id)
    .bind(workspace.org_id)
    .bind(&slug)
    .bind(format!("file:///tmp/{repo_id}"))
    .execute(&app.db)
    .await
    .expect("insert secondary comment repo");

    sqlx::query(
        r#"
        INSERT INTO workspace_repos (
            org_id, workspace_id, repo_id, root_path, role
        )
        VALUES ($1, $2, $3, $4, 'code')
        "#,
    )
    .bind(workspace.org_id)
    .bind(workspace.id)
    .bind(repo_id)
    .bind(root_path)
    .execute(&app.db)
    .await
    .expect("insert secondary authoritative workspace repo mapping");
    // Reads resolve the member's head from its branch ref (P1.1); the
    // secondary repository only needs a ref, its content is never read here.
    let source_path = app.repo_path(workspace.repo_id).await;
    let destination = app.repo_path(repo_id).await;
    git2::Repository::clone(source_path.to_str().unwrap(), &destination).unwrap();
    helpers::seed_branch_ref(app, repo_id, &workspace.name, &workspace.head_commit).await;
    repo_id
}

async fn update_user_display_name(app: &TestApp, user_id: Uuid, display_name: &str) {
    sqlx::query(
        r#"
        UPDATE users
        SET display_name = $2
        WHERE id = $1
        "#,
    )
    .bind(user_id)
    .bind(display_name)
    .execute(&app.db)
    .await
    .expect("Failed to update user display name");
}

async fn count_mention_notifications(
    app: &TestApp,
    workspace_id: Uuid,
    recipient_user_id: Uuid,
) -> i64 {
    sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*)
        FROM notifications
        WHERE workspace_id = $1
          AND recipient_user_id = $2
          AND category = 'mention'
        "#,
    )
    .bind(workspace_id)
    .bind(recipient_user_id)
    .fetch_one(&app.db)
    .await
    .expect("failed to count mention notifications")
}

// ===========================================================================
// Group 1: Create Comment — POST /api/v1/workspaces/:workspaceId/comments
// ===========================================================================

/// Verifies that creating a line-scoped comment returns 201 with the full
/// `CommentDTO` shape.
#[tokio::test]
async fn test_create_line_comment_returns_201_with_comment_detail() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let client = app.authenticated_client(&user);
    let workspace = comment_workspace(&app, &org).await;

    let response = client
        .post_json(
            &format!("/api/v1/workspaces/{}/comments", workspace.id),
            &json!({
                "scope": "line",
                "content": "This should use bcrypt instead of SHA256",
                "repoId": workspace.repo_id,
                "filePath": "src/auth.rs",
                "lineNumber": 42,
                "commitSha": workspace.head_commit,
                "lineHash": TEST_LINE_HASH,
                "contentHash": TEST_CONTENT_HASH,
                "anchorContext": TEST_ANCHOR_CONTEXT
            }),
        )
        .await;

    assert_eq!(response.status(), StatusCode::CREATED);

    let body: Value = response.json().await.expect("Failed to parse JSON");
    let data = &body["data"];

    // Verify CommentDTO shape
    assert!(data["id"].is_string(), "id should be a string UUID");
    assert!(
        data["workspaceId"].is_string(),
        "workspaceId should be a string"
    );
    assert!(data["authorId"].is_string(), "authorId should be a string");
    assert!(
        data["authorName"].is_string(),
        "authorName should be a string"
    );
    assert_eq!(data["scope"].as_str().unwrap(), "line");
    assert_eq!(data["repoId"], workspace.repo_id.to_string());
    assert!(
        data.get("nodeId").is_none(),
        "retired node identity must not appear in comment responses"
    );
    assert_eq!(data["filePath"].as_str().unwrap(), "src/auth.rs");
    assert_eq!(data["lineNumber"].as_i64().unwrap(), 42);
    assert_eq!(data["commitSha"], workspace.head_commit);
    assert_eq!(
        data["content"].as_str().unwrap(),
        "This should use bcrypt instead of SHA256"
    );
    assert_eq!(data["resolved"].as_bool().unwrap(), false);
    assert!(
        data["resolvedAt"].is_null(),
        "resolvedAt should be null initially"
    );
    assert!(
        data["originBranch"].is_string(),
        "originBranch should be set"
    );
    assert_eq!(data["replyCount"].as_i64().unwrap(), 0);
    assert!(data["createdAt"].is_string());
    assert!(data["updatedAt"].is_string());

    // Verify database row was created
    let comment_id: Uuid = data["id"].as_str().unwrap().parse().unwrap();
    let row = sqlx::query(
        "SELECT scope, repo_id, file_path, line_number, resolved, content FROM comment_index WHERE id = $1",
    )
    .bind(comment_id)
    .fetch_one(&app.db)
    .await
    .expect("Comment should exist in database");
    assert_eq!(row.get::<String, _>("scope"), "line");
    assert_eq!(
        row.get::<Option<Uuid>, _>("repo_id"),
        Some(workspace.repo_id)
    );
    assert_eq!(
        row.get::<Option<String>, _>("file_path").as_deref(),
        Some("src/auth.rs")
    );
    assert_eq!(row.get::<Option<i32>, _>("line_number"), Some(42));
    assert!(!row.get::<bool, _>("resolved"));
    assert_eq!(
        row.get::<String, _>("content"),
        "This should use bcrypt instead of SHA256"
    );

    app.cleanup().await;
}

/// Verifies that explicit picker mentions still create notifications for
/// ordinary org members in org-visible workspaces.
#[tokio::test]
async fn test_create_comment_explicit_mention_notifies_org_member() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let author = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let mentioned = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let client = app.authenticated_client(&author);
    let workspace = comment_workspace(&app, &org).await;

    let response = client
        .post_json(
            &format!("/api/v1/workspaces/{}/comments", workspace.id),
            &json!({
                "scope": "line",
                "content": "Please take a look",
                "repoId": workspace.repo_id,
                "filePath": "src/auth.rs",
                "lineNumber": 7,
                "commitSha": workspace.head_commit,
                "lineHash": TEST_LINE_HASH,
                "contentHash": TEST_CONTENT_HASH,
                "anchorContext": TEST_ANCHOR_CONTEXT,
                "mentionUserIds": [mentioned.id]
            }),
        )
        .await;

    let status = response.status();
    let response_body = response
        .text()
        .await
        .expect("comment response body should be readable");
    assert_eq!(
        status,
        StatusCode::CREATED,
        "unexpected response body: {response_body}"
    );

    let notification_count = count_mention_notifications(&app, workspace.id, mentioned.id).await;
    assert_eq!(notification_count, 1);

    app.cleanup().await;
}

/// A mention recipient who only reads the workspace (read scope, not the
/// owner) marks their own notifications read: a chosen set, one, and all at
/// once. Read state belongs to the recipient and is not a workspace write;
/// a set leaves the rows outside it unread (g5.user.notifications).
#[tokio::test]
async fn test_mention_recipient_marks_notifications_read_with_read_access_only() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let author = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let mentioned = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let workspace = comment_workspace(&app, &org).await;
    let client = app.authenticated_client(&author);

    for line_number in [7, 8, 9] {
        let response = client
            .post_json(
                &format!("/api/v1/workspaces/{}/comments", workspace.id),
                &json!({
                    "scope": "line",
                    "content": "Please take a look",
                    "repoId": workspace.repo_id,
                    "filePath": "src/auth.rs",
                    "lineNumber": line_number,
                    "commitSha": workspace.head_commit,
                    "lineHash": TEST_LINE_HASH,
                    "contentHash": TEST_CONTENT_HASH,
                    "anchorContext": TEST_ANCHOR_CONTEXT,
                    "mentionUserIds": [mentioned.id]
                }),
            )
            .await;
        assert_eq!(response.status(), StatusCode::CREATED);
    }
    assert_eq!(
        count_mention_notifications(&app, workspace.id, mentioned.id).await,
        3
    );

    let reader = app.authenticated_client(&mentioned);
    let listed = reader
        .get(&format!(
            "/api/v1/workspaces/{}/notifications",
            workspace.id
        ))
        .await;
    assert_eq!(listed.status(), StatusCode::OK);
    let listed: Value = listed.json().await.expect("notification list body");
    let rows = listed["data"].as_array().expect("data array");
    assert_eq!(rows.len(), 3);
    let first_id = rows[0]["id"].as_str().expect("notification id");
    let second_id = rows[1]["id"].as_str().expect("notification id");
    let unread = || async {
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM notifications
             WHERE workspace_id = $1 AND recipient_user_id = $2 AND read_at IS NULL",
        )
        .bind(workspace.id)
        .bind(mentioned.id)
        .fetch_one(&app.db)
        .await
        .expect("unread count")
    };
    let read_all_path = format!("/api/v1/workspaces/{}/notifications/read-all", workspace.id);

    // A selection that is present but not valid is refused and changes
    // nothing: it never widens into "all".
    let malformed = reader
        .post_json(&read_all_path, &json!({ "ids": ["not-a-uuid"] }))
        .await;
    assert_eq!(malformed.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(unread().await, 3);

    // A chosen set: only the listed row changes.
    let some = reader
        .post_json(&read_all_path, &json!({ "ids": [first_id] }))
        .await;
    assert_eq!(
        some.status(),
        StatusCode::OK,
        "a recipient marks a set read"
    );
    let some: Value = some.json().await.expect("read-all body");
    assert_eq!(some["data"]["updated"].as_u64(), Some(1));
    assert_eq!(unread().await, 2);

    // One row.
    let one = reader
        .patch_json(&format!("/api/v1/notifications/{second_id}"), &json!({}))
        .await;
    assert_eq!(one.status(), StatusCode::OK, "a recipient marks one read");
    assert_eq!(unread().await, 1);

    // A missing body is refused, never read as "all".
    let bodiless = reader
        .post_body(&read_all_path, "application/json", Vec::new())
        .await;
    assert!(bodiless.status().is_client_error(), "{}", bodiless.status());
    assert_eq!(unread().await, 1);

    // `{}` means all; then nothing is left.
    let all = reader.post_json(&read_all_path, &json!({})).await;
    assert_eq!(all.status(), StatusCode::OK, "a recipient marks all read");
    let all: Value = all.json().await.expect("read-all body");
    assert_eq!(all["data"]["updated"].as_u64(), Some(1));
    assert_eq!(unread().await, 0);

    let again = reader.post_json(&read_all_path, &json!({})).await;
    assert_eq!(again.status(), StatusCode::OK);
    let again: Value = again.json().await.expect("read-all body");
    assert_eq!(again["data"]["updated"].as_u64(), Some(0));

    app.cleanup().await;
}

/// Verifies that parsed @handle mentions still create notifications for
/// ordinary org members in org-visible workspaces.
#[tokio::test]
async fn test_create_comment_handle_mention_notifies_org_member() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let author = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let mentioned = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let client = app.authenticated_client(&author);
    let workspace = comment_workspace(&app, &org).await;

    update_user_display_name(&app, mentioned.id, "reviewer").await;

    let response = client
        .post_json(
            &format!("/api/v1/workspaces/{}/comments", workspace.id),
            &json!({
                "scope": "line",
                "content": "Please take a look @reviewer",
                "repoId": workspace.repo_id,
                "filePath": "src/auth.rs",
                "lineNumber": 9,
                "commitSha": workspace.head_commit,
                "lineHash": TEST_LINE_HASH,
                "contentHash": TEST_CONTENT_HASH,
                "anchorContext": TEST_ANCHOR_CONTEXT
            }),
        )
        .await;

    let status = response.status();
    let response_body = response
        .text()
        .await
        .expect("comment response body should be readable");
    assert_eq!(
        status,
        StatusCode::CREATED,
        "unexpected response body: {response_body}"
    );
    assert_eq!(
        count_mention_notifications(&app, workspace.id, mentioned.id).await,
        1
    );

    app.cleanup().await;
}

/// Personal workspace mentions reach admitted administrators.
#[tokio::test]
async fn test_personal_comment_explicit_mention_notifies_admin() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let author = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let workspace = seeded_comment_workspace(&app, &org, Some(owner.id)).await;
    let client = app.authenticated_client(&owner);

    sqlx::query("UPDATE org_memberships SET role='admin' WHERE org_id=$1 AND user_id=$2")
        .bind(org.id)
        .bind(author.id)
        .execute(&app.db)
        .await
        .unwrap();

    let response = client
        .post_json(
            &format!("/api/v1/workspaces/{}/comments", workspace.id),
            &json!({
                "scope": "line",
                "content": "Please take a look",
                "repoId": workspace.repo_id,
                "filePath": "src/auth.rs",
                "lineNumber": 11,
                "commitSha": workspace.head_commit,
                "lineHash": TEST_LINE_HASH,
                "contentHash": TEST_CONTENT_HASH,
                "anchorContext": TEST_ANCHOR_CONTEXT,
                "mentionUserIds": [author.id]
            }),
        )
        .await;

    let status = response.status();
    let response_body = response
        .text()
        .await
        .expect("comment response body should be readable");
    assert_eq!(
        status,
        StatusCode::CREATED,
        "unexpected response body: {response_body}"
    );
    assert_eq!(
        count_mention_notifications(&app, workspace.id, author.id).await,
        1
    );

    app.cleanup().await;
}

/// Verifies that creating a comment without authentication returns 401.
#[tokio::test]
async fn test_create_comment_without_auth_returns_401() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let _user = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let workspace = comment_workspace(&app, &org).await;

    // Send request without auth
    let response = app
        .client
        .post(format!(
            "{}/api/v1/workspaces/{}/comments",
            app.base_url, workspace.id
        ))
        .header("Content-Type", "application/json")
        .json(&json!({
            "scope": "line",
            "content": "test",
            "filePath": "src/test.rs",
            "lineNumber": 1
        }))
        .send()
        .await
        .expect("Failed to send request");

    assert_json_error(response, StatusCode::UNAUTHORIZED, "UNAUTHORIZED").await;

    app.cleanup().await;
}

/// Verifies that creating a comment with an invalid scope value returns 422
/// VALIDATION_ERROR.
#[tokio::test]
async fn test_create_comment_invalid_scope_returns_422() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let client = app.authenticated_client(&user);
    let workspace = comment_workspace(&app, &org).await;

    let response = client
        .post_json(
            &format!("/api/v1/workspaces/{}/comments", workspace.id),
            &json!({
                "scope": "invalid_scope",
                "content": "test comment"
            }),
        )
        .await;

    assert_json_error(
        response,
        StatusCode::UNPROCESSABLE_ENTITY,
        "VALIDATION_ERROR",
    )
    .await;

    app.cleanup().await;
}

/// Verifies that creating a line-scoped comment without `filePath` returns 422
/// VALIDATION_ERROR.
#[tokio::test]
async fn test_create_line_comment_missing_file_path_returns_422() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let client = app.authenticated_client(&user);
    let workspace = comment_workspace(&app, &org).await;

    let response = client
        .post_json(
            &format!("/api/v1/workspaces/{}/comments", workspace.id),
            &json!({
                "scope": "line",
                "content": "test comment",
                "repoId": workspace.repo_id,
                "lineNumber": 42
                // Missing filePath
            }),
        )
        .await;

    assert_json_error(
        response,
        StatusCode::UNPROCESSABLE_ENTITY,
        "VALIDATION_ERROR",
    )
    .await;

    app.cleanup().await;
}

// ===========================================================================
// Group 2: List Comments — GET /api/v1/workspaces/:workspaceId/comments
// ===========================================================================

/// Verifies that listing comments returns paginated results with the correct
/// response shape, including `meta` pagination metadata.
#[tokio::test]
async fn test_list_comments_returns_paginated_comments() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let client = app.authenticated_client(&user);
    let workspace = comment_workspace(&app, &org).await;
    let ws_id = workspace.id.to_string();

    // Create two line comments
    create_line_comment(
        &client,
        &ws_id,
        workspace.repo_id,
        "src/auth.rs",
        10,
        "Comment 1",
    )
    .await;
    create_line_comment(
        &client,
        &ws_id,
        workspace.repo_id,
        "src/auth.rs",
        20,
        "Comment 2",
    )
    .await;

    let response = client
        .get(&format!("/api/v1/workspaces/{}/comments", ws_id))
        .await;

    assert_eq!(response.status(), StatusCode::OK);

    let body: Value = response.json().await.expect("Failed to parse JSON");
    let data = body["data"].as_array().expect("data should be an array");
    assert_eq!(data.len(), 2, "Should have exactly 2 comments");

    // Verify CommentDTO shape
    let comment = &data[0];
    assert!(comment["id"].is_string());
    assert!(comment["workspaceId"].is_string());
    assert!(comment["authorId"].is_string());
    assert!(comment["authorName"].is_string());
    assert!(comment["scope"].is_string());
    assert_eq!(comment["repoId"], workspace.repo_id.to_string());
    assert!(comment["content"].is_string());
    assert!(comment["resolved"].is_boolean());
    assert!(comment["originBranch"].is_string());
    assert!(comment["replyCount"].is_number());
    assert!(comment["createdAt"].is_string());
    assert!(comment["updatedAt"].is_string());

    // Verify pagination meta
    let meta = &body["meta"];
    assert!(meta["page"].is_number(), "meta should have page");
    assert!(meta["perPage"].is_number(), "meta should have perPage");
    assert!(meta["total"].is_number(), "meta should have total");
    assert_eq!(meta["total"].as_i64().unwrap(), 2);

    app.cleanup().await;
}

/// Verifies that listing comments for a workspace with no comments returns an
/// empty array (not an error).
#[tokio::test]
async fn test_list_comments_empty_returns_empty_array() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let client = app.authenticated_client(&user);
    let workspace = comment_workspace(&app, &org).await;

    let response = client
        .get(&format!("/api/v1/workspaces/{}/comments", workspace.id))
        .await;

    assert_eq!(response.status(), StatusCode::OK);

    let body: Value = response.json().await.expect("Failed to parse JSON");
    let data = body["data"].as_array().expect("data should be an array");
    assert!(
        data.is_empty(),
        "Should return empty array when no comments exist"
    );

    app.cleanup().await;
}

#[tokio::test]
async fn test_list_line_comments_are_isolated_by_repository() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let client = app.authenticated_client(&user);
    let workspace = comment_workspace(&app, &org).await;
    let second_repo_id = add_workspace_repo(&app, &workspace, "secondary-list").await;

    insert_line_comment_directly(
        &app,
        DirectLineComment {
            workspace_id: workspace.id,
            repo_id: workspace.repo_id,
            author_id: user.id,
            file_path: "src/shared.rs",
            line_number: 10,
            origin_branch: "main",
            content: "first repo",
        },
    )
    .await;
    insert_line_comment_directly(
        &app,
        DirectLineComment {
            workspace_id: workspace.id,
            repo_id: second_repo_id,
            author_id: user.id,
            file_path: "src/shared.rs",
            line_number: 20,
            origin_branch: "main",
            content: "second repo",
        },
    )
    .await;

    let response = client
        .get(&format!(
            "/api/v1/workspaces/{}/comments?repoId={}&filePath=src/shared.rs&branch=main",
            workspace.id, second_repo_id
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("line comment list");
    let comments = body["data"].as_array().expect("comments array");
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0]["repoId"], second_repo_id.to_string());
    assert_eq!(comments[0]["content"], "second repo");

    app.cleanup().await;
}

// ===========================================================================
// Group 3: Cross-Branch Comment Discovery
//   GET /api/v1/workspaces/:workspaceId/comments/cross-branch?repoId=R
// ===========================================================================

#[tokio::test]
async fn test_line_comment_requires_workspace_member_repo() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let client = app.authenticated_client(&user);
    let workspace = comment_workspace(&app, &org).await;

    let missing_repo = client
        .post_json(
            &format!("/api/v1/workspaces/{}/comments", workspace.id),
            &json!({
                "scope": "line",
                "content": "missing repo",
                "filePath": "src/auth.rs",
                "lineNumber": 1,
            }),
        )
        .await;
    assert_eq!(missing_repo.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let foreign_repo = client
        .post_json(
            &format!("/api/v1/workspaces/{}/comments", workspace.id),
            &json!({
                "scope": "line",
                "content": "foreign repo",
                "repoId": Uuid::new_v4(),
                "filePath": "src/auth.rs",
                "lineNumber": 1,
            }),
        )
        .await;
    assert_eq!(foreign_repo.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let unqualified_list = client
        .get(&format!(
            "/api/v1/workspaces/{}/comments?filePath=src/auth.rs&branch=main",
            workspace.id
        ))
        .await;
    assert_eq!(unqualified_list.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let foreign_list = client
        .get(&format!(
            "/api/v1/workspaces/{}/comments?repoId={}&filePath=src/auth.rs&branch=main",
            workspace.id,
            Uuid::new_v4()
        ))
        .await;
    assert_eq!(foreign_list.status(), StatusCode::UNPROCESSABLE_ENTITY);

    app.cleanup().await;
}

// ===========================================================================
// Group 4: Update / Resolve Comment — PATCH /api/v1/comments/:id
// ===========================================================================

/// Verifies that resolving a comment sets `resolved = true` and `resolvedAt`
/// to a timestamp.
#[tokio::test]
async fn test_resolve_comment_sets_resolved_and_timestamp() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let client = app.authenticated_client(&user);
    let workspace = comment_workspace(&app, &org).await;
    let comment_id = create_line_comment(
        &client,
        &workspace.id.to_string(),
        workspace.repo_id,
        "src/auth.rs",
        42,
        "This needs fixing",
    )
    .await;

    // Resolve the comment
    let response = client
        .patch_json(
            &format!("/api/v1/comments/{}", comment_id),
            &json!({ "resolved": true }),
        )
        .await;

    assert_eq!(response.status(), StatusCode::OK);

    let body: Value = response.json().await.expect("Failed to parse JSON");
    let data = &body["data"];

    assert_eq!(data["resolved"].as_bool().unwrap(), true);
    assert!(
        data["resolvedAt"].is_string(),
        "resolvedAt should be set when resolved"
    );

    // Verify database state
    let comment_uuid: Uuid = comment_id.parse().unwrap();
    let row = sqlx::query!(
        "SELECT resolved, resolved_at FROM comment_index WHERE id = $1",
        comment_uuid
    )
    .fetch_one(&app.db)
    .await
    .expect("Comment should exist");
    assert_eq!(row.resolved, true);
    assert!(
        row.resolved_at.is_some(),
        "resolved_at should be set in database"
    );

    app.cleanup().await;
}

/// Verifies that updating a non-existent comment returns 404 NOT_FOUND.
#[tokio::test]
async fn test_update_nonexistent_comment_returns_404() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let client = app.authenticated_client(&user);

    let fake_id = Uuid::new_v4();
    let response = client
        .patch_json(
            &format!("/api/v1/comments/{}", fake_id),
            &json!({ "resolved": true }),
        )
        .await;

    assert_json_error(response, StatusCode::NOT_FOUND, "NOT_FOUND").await;

    app.cleanup().await;
}

// ===========================================================================
// Group 5: Add Reply — POST /api/v1/comments/:id/replies
// ===========================================================================

/// Verifies that adding a reply returns 201 with the `ReplyDTO` shape and
/// increments the parent comment's reply count.
#[tokio::test]
async fn test_add_reply_returns_201_with_reply_detail() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let client = app.authenticated_client(&user);
    let workspace = comment_workspace(&app, &org).await;
    let comment_id = create_line_comment(
        &client,
        &workspace.id.to_string(),
        workspace.repo_id,
        "src/auth.rs",
        42,
        "This needs fixing",
    )
    .await;

    // Add a reply
    let response = client
        .post_json(
            &format!("/api/v1/comments/{}/replies", comment_id),
            &json!({ "content": "Good catch, I'll fix this." }),
        )
        .await;

    assert_eq!(response.status(), StatusCode::CREATED);

    let body: Value = response.json().await.expect("Failed to parse JSON");
    let data = &body["data"];

    // Verify ReplyDTO shape
    assert!(data["id"].is_string(), "id should be a string UUID");
    assert_eq!(data["commentId"].as_str().unwrap(), comment_id);
    assert!(data["authorId"].is_string());
    assert!(data["authorName"].is_string());
    assert_eq!(
        data["content"].as_str().unwrap(),
        "Good catch, I'll fix this."
    );
    assert!(data["createdAt"].is_string());

    // Verify reply exists in database
    let reply_id: Uuid = data["id"].as_str().unwrap().parse().unwrap();
    let row = sqlx::query!(
        "SELECT content, author_id FROM comment_replies WHERE id = $1",
        reply_id
    )
    .fetch_one(&app.db)
    .await
    .expect("Reply should exist in database");
    assert_eq!(row.content, "Good catch, I'll fix this.");

    // Verify parent comment's updated_at was bumped (reply activity updates parent)
    let comment_uuid: Uuid = comment_id.parse().unwrap();
    let parent_row = sqlx::query!(
        "SELECT updated_at FROM comment_index WHERE id = $1",
        comment_uuid
    )
    .fetch_one(&app.db)
    .await
    .expect("Parent comment should exist");
    // updated_at should be recent (within last few seconds)
    assert!(
        parent_row.updated_at > chrono::Utc::now() - chrono::Duration::seconds(10),
        "Parent comment updated_at should be recent after reply"
    );

    app.cleanup().await;
}

// ===========================================================================
// Group 6: Delete Comment — DELETE /api/v1/comments/:id
// ===========================================================================

/// Verifies that deleting a comment returns success and cascade-deletes all replies.
#[tokio::test]
async fn test_delete_comment_returns_success_and_cascades_replies() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let client = app.authenticated_client(&user);
    let workspace = comment_workspace(&app, &org).await;
    let comment_id = create_line_comment(
        &client,
        &workspace.id.to_string(),
        workspace.repo_id,
        "src/auth.rs",
        42,
        "This needs fixing",
    )
    .await;

    // Add a reply first
    client
        .post_json(
            &format!("/api/v1/comments/{}/replies", comment_id),
            &json!({ "content": "I'll fix this." }),
        )
        .await;

    // Delete the comment
    let response = client
        .delete(&format!("/api/v1/comments/{}", comment_id))
        .await;

    assert_eq!(response.status(), StatusCode::OK);

    let body: Value = response.json().await.expect("Failed to parse JSON");
    assert_eq!(body["data"]["deleted"].as_bool().unwrap(), true);

    // Verify comment was deleted from database
    let comment_uuid: Uuid = comment_id.parse().unwrap();
    let count = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM comment_index WHERE id = $1",
        comment_uuid
    )
    .fetch_one(&app.db)
    .await
    .expect("Query should succeed");
    assert_eq!(count.unwrap_or(0), 0, "Comment should be deleted");

    // Verify replies were cascade-deleted
    let reply_count = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM comment_replies WHERE comment_id = $1",
        comment_uuid
    )
    .fetch_one(&app.db)
    .await
    .expect("Query should succeed");
    assert_eq!(
        reply_count.unwrap_or(0),
        0,
        "Replies should be cascade-deleted"
    );

    app.cleanup().await;
}

// ===========================================================================
// Group 9: Thread identity, authority, replies and events
// (`.g5/ontology-plan-comments.md`)
// ===========================================================================

async fn org_visible_reader(app: &TestApp, org_id: Uuid) -> TestUser {
    TestUser::create_with_scopes(app, org_id, &[Scope::WorkspacesRead]).await
}

async fn comment_row_exists(app: &TestApp, comment_id: &str) -> bool {
    sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM comment_index WHERE id = $1::uuid)")
        .bind(comment_id)
        .fetch_one(&app.db)
        .await
        .expect("comment existence")
}

fn line_comment_request(workspace: &TestWorkspace, content: &str) -> Value {
    json!({
        "scope": "line",
        "content": content,
        "repoId": workspace.repo_id,
        "commitSha": workspace.head_commit,
        "filePath": "docs/plan.md",
        "lineNumber": 3,
        "lineHash": TEST_LINE_HASH,
        "contentHash": TEST_CONTENT_HASH,
        "anchorContext": TEST_ANCHOR_CONTEXT
    })
}

/// g5.documents.comments.authority: anyone who can read the workspace
/// starts a thread and replies (read scope, no VM); only the current owner
/// resolves, reopens or deletes a thread, including one a reader started.
#[tokio::test]
async fn test_reader_starts_and_replies_but_cannot_resolve_or_delete_threads() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let owner = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let workspace = comment_workspace(&app, &org).await;
    let reader = org_visible_reader(&app, org.id).await;
    let owner_client = app.authenticated_client(&owner);
    let reader_client = app.authenticated_client(&reader);

    let comment_id = create_line_comment(
        &owner_client,
        &workspace.id.to_string(),
        workspace.repo_id,
        "docs/plan.md",
        3,
        "Please check this step",
    )
    .await;

    // A reader with only read scope answers in the thread, and the teammate
    // it mentions hears about it.
    let reply = reader_client
        .post_json(
            &format!("/api/v1/comments/{comment_id}/replies"),
            &json!({
                "content": "Done, see the next section",
                "mentionUserIds": [owner.id],
            }),
        )
        .await;
    assert_eq!(reply.status(), StatusCode::CREATED, "reader reply");
    let mention_notices: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM notifications WHERE recipient_user_id = $1 AND category = 'mention'",
    )
    .bind(owner.id)
    .fetch_one(&app.db)
    .await
    .expect("mention notices");
    assert_eq!(mention_notices, 1, "the mentioned owner gets one notice");
    let reply_body: Value = reply.json().await.unwrap();
    assert!(reply_body["data"].get("authorKind").is_none());
    assert_eq!(reply_body["data"]["authorId"], reader.id.to_string());

    let replies: Value = owner_client
        .get(&format!("/api/v1/comments/{comment_id}/replies"))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(replies["data"].as_array().unwrap().len(), 1);

    // A reader with only read scope starts a thread of their own.
    let start = reader_client
        .post_json(
            &format!("/api/v1/workspaces/{}/comments", workspace.id),
            &line_comment_request(&workspace, "reader thread"),
        )
        .await;
    assert_eq!(
        start.status(),
        StatusCode::CREATED,
        "a reader starts a thread"
    );
    let started: Value = start.json().await.unwrap();
    assert_eq!(started["data"]["authorId"], reader.id.to_string());
    let reader_thread_id = started["data"]["id"].as_str().unwrap().to_string();
    let listed: Value = owner_client
        .get(&format!(
            "/api/v1/workspaces/{}/comments?repoId={}&filePath=docs/plan.md",
            workspace.id, workspace.repo_id
        ))
        .await
        .json()
        .await
        .unwrap();
    assert!(
        listed["data"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["id"] == reader_thread_id.as_str()),
        "the owner sees the reader's thread: {listed}"
    );

    // The reader edits the text of their own thread with read scope; nobody
    // else edits it, the owner included.
    let edited = reader_client
        .patch_json(
            &format!("/api/v1/comments/{reader_thread_id}"),
            &json!({"content": "reader thread, corrected"}),
        )
        .await;
    assert_eq!(edited.status(), StatusCode::OK, "author edits own text");
    let edited: Value = edited.json().await.unwrap();
    assert_eq!(edited["data"]["content"], "reader thread, corrected");
    let owner_edit = owner_client
        .patch_json(
            &format!("/api/v1/comments/{reader_thread_id}"),
            &json!({"content": "rewritten by the owner"}),
        )
        .await;
    assert_eq!(
        owner_edit.status(),
        StatusCode::FORBIDDEN,
        "owner edits text"
    );
    let other_reader = org_visible_reader(&app, org.id).await;
    let other_edit = app
        .authenticated_client(&other_reader)
        .patch_json(
            &format!("/api/v1/comments/{reader_thread_id}"),
            &json!({"content": "rewritten by another reader"}),
        )
        .await;
    assert_eq!(
        other_edit.status(),
        StatusCode::FORBIDDEN,
        "other edits text"
    );

    // Resolving and deleting stay with the owner, even for the reader's own
    // thread, and even for a non-owner holding write scope.
    let resolve_own = reader_client
        .patch_json(
            &format!("/api/v1/comments/{reader_thread_id}"),
            &json!({"resolved": true}),
        )
        .await;
    assert_eq!(
        resolve_own.status(),
        StatusCode::FORBIDDEN,
        "reader resolves own"
    );
    let delete_own = reader_client
        .delete(&format!("/api/v1/comments/{reader_thread_id}"))
        .await;
    assert_eq!(
        delete_own.status(),
        StatusCode::FORBIDDEN,
        "reader deletes own"
    );
    let writer = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let writer_client = app.authenticated_client(&writer);
    let writer_resolves = writer_client
        .patch_json(
            &format!("/api/v1/comments/{reader_thread_id}"),
            &json!({"resolved": true}),
        )
        .await;
    assert_eq!(
        writer_resolves.status(),
        StatusCode::FORBIDDEN,
        "a non-owner with write scope cannot resolve"
    );
    let writer_deletes = writer_client
        .delete(&format!("/api/v1/comments/{reader_thread_id}"))
        .await;
    assert_eq!(
        writer_deletes.status(),
        StatusCode::FORBIDDEN,
        "a non-owner with write scope cannot delete"
    );
    let writer_starts = writer_client
        .post_json(
            &format!("/api/v1/workspaces/{}/comments", workspace.id),
            &line_comment_request(&workspace, "writer thread"),
        )
        .await;
    assert_eq!(
        writer_starts.status(),
        StatusCode::CREATED,
        "but starts one"
    );

    // The owner's credential must itself carry write scope to decide a
    // thread: a read-only token of the owner edits the owner's own text but
    // neither resolves nor deletes, whatever the database would admit.
    let owner_read_only = AuthenticatedClient::new(
        app.client.clone(),
        app.base_url.clone(),
        test_jwt_minter()
            .mint(owner.id, org.id, vec![Scope::WorkspacesRead], 3600)
            .expect("owner read-only token"),
    );
    let read_only_resolve = owner_read_only
        .patch_json(
            &format!("/api/v1/comments/{reader_thread_id}"),
            &json!({"resolved": true}),
        )
        .await;
    let status = read_only_resolve.status();
    let body: Value = read_only_resolve.json().await.unwrap();
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["error"]["code"], "FORBIDDEN", "{body}");
    assert!(
        body["error"]["details"]["missing_scopes"]
            .as_array()
            .map(|scopes| scopes.iter().any(|s| s == "workspaces:write"))
            .unwrap_or(false),
        "names the missing write scope: {body}"
    );
    let read_only_reopen_and_edit = owner_read_only
        .patch_json(
            &format!("/api/v1/comments/{comment_id}"),
            &json!({"content": "owner's text, edited", "resolved": false}),
        )
        .await;
    assert_eq!(
        read_only_reopen_and_edit.status(),
        StatusCode::FORBIDDEN,
        "a decision beside an edit still needs write scope"
    );
    let read_only_delete = owner_read_only
        .delete(&format!("/api/v1/comments/{reader_thread_id}"))
        .await;
    assert_eq!(read_only_delete.status(), StatusCode::FORBIDDEN);
    assert!(comment_row_exists(&app, &reader_thread_id).await);
    let read_only_edit = owner_read_only
        .patch_json(
            &format!("/api/v1/comments/{comment_id}"),
            &json!({"content": "owner's text, edited"}),
        )
        .await;
    assert_eq!(read_only_edit.status(), StatusCode::OK, "author edits");
    let read_only_edit: Value = read_only_edit.json().await.unwrap();
    assert_eq!(read_only_edit["data"]["content"], "owner's text, edited");
    assert_eq!(read_only_edit["data"]["resolved"], false);
    let owner_resolves = owner_client
        .patch_json(
            &format!("/api/v1/comments/{reader_thread_id}"),
            &json!({"resolved": true}),
        )
        .await;
    assert_eq!(owner_resolves.status(), StatusCode::OK, "owner resolves it");
    let resolve = reader_client
        .patch_json(
            &format!("/api/v1/comments/{comment_id}"),
            &json!({"resolved": true}),
        )
        .await;
    assert_eq!(resolve.status(), StatusCode::FORBIDDEN, "reader resolve");
    let delete = reader_client
        .delete(&format!("/api/v1/comments/{comment_id}"))
        .await;
    assert_eq!(delete.status(), StatusCode::FORBIDDEN, "reader delete");
    assert!(comment_row_exists(&app, &comment_id).await);

    // An empty reply is refused.
    let blank = reader_client
        .post_json(
            &format!("/api/v1/comments/{comment_id}/replies"),
            &json!({"content": "   "}),
        )
        .await;
    assert_eq!(blank.status(), StatusCode::UNPROCESSABLE_ENTITY);

    app.cleanup().await;
}

/// g5.documents.comments.authority: the current owner acts on any thread in
/// the workspace, whoever wrote it; a former owner no longer resolves.
#[tokio::test]
async fn test_new_owner_resolves_and_deletes_the_previous_owners_threads() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let first_owner = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let workspace = comment_workspace(&app, &org).await;
    let next_owner = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let first_client = app.authenticated_client(&first_owner);
    let next_client = app.authenticated_client(&next_owner);

    let resolved_id = create_line_comment(
        &first_client,
        &workspace.id.to_string(),
        workspace.repo_id,
        "docs/plan.md",
        1,
        "first owner's note",
    )
    .await;
    let deleted_id = create_line_comment(
        &first_client,
        &workspace.id.to_string(),
        workspace.repo_id,
        "docs/plan.md",
        2,
        "another note",
    )
    .await;

    // The real transfer: the workspace has no attached VM, so it completes
    // at once and moves authority to the new owner (g5.lifecycle.transfer).
    let transferred = first_client
        .post_json(
            &format!("/api/v1/workspaces/{}/transfer", workspace.id),
            &json!({
                "toUserId": next_owner.id,
                "idempotencyKey": format!("transfer-{}", Uuid::new_v4()),
            }),
        )
        .await;
    let transfer_status = transferred.status();
    let transferred: Value = transferred.json().await.unwrap();
    assert_eq!(transfer_status, StatusCode::OK, "transfer: {transferred}");
    assert_eq!(transferred["data"]["status"], "completed");

    let former = first_client
        .patch_json(
            &format!("/api/v1/comments/{resolved_id}"),
            &json!({"resolved": true}),
        )
        .await;
    assert_eq!(former.status(), StatusCode::FORBIDDEN, "former owner");

    let resolved = next_client
        .patch_json(
            &format!("/api/v1/comments/{resolved_id}"),
            &json!({"resolved": true}),
        )
        .await;
    assert_eq!(resolved.status(), StatusCode::OK, "new owner resolves");
    let resolved_body: Value = resolved.json().await.unwrap();
    assert_eq!(resolved_body["data"]["resolved"], true);

    // Text stays with its author.
    let edit = next_client
        .patch_json(
            &format!("/api/v1/comments/{resolved_id}"),
            &json!({"content": "rewritten by someone else"}),
        )
        .await;
    assert_eq!(
        edit.status(),
        StatusCode::FORBIDDEN,
        "only the author edits"
    );

    let deleted = next_client
        .delete(&format!("/api/v1/comments/{deleted_id}"))
        .await;
    assert_eq!(deleted.status(), StatusCode::OK, "new owner deletes");
    assert!(!comment_row_exists(&app, &deleted_id).await);

    app.cleanup().await;
}

/// Current activity, not creator provenance, governs mention eligibility.
#[tokio::test]
async fn test_a_deactivated_former_owner_is_mentionable_only_after_reactivation() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    // The first member becomes the fixture workspace's owner and creator.
    let creator = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let workspace = comment_workspace(&app, &org).await;
    let next_owner = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let creator_client = app.authenticated_client(&creator);
    let next_client = app.authenticated_client(&next_owner);

    // The real transfer: no VM is attached, so it completes at once.
    let transferred = creator_client
        .post_json(
            &format!("/api/v1/workspaces/{}/transfer", workspace.id),
            &json!({
                "toUserId": next_owner.id,
                "idempotencyKey": format!("transfer-{}", Uuid::new_v4()),
            }),
        )
        .await;
    let transfer_status = transferred.status();
    let transferred: Value = transferred.json().await.unwrap();
    assert_eq!(transfer_status, StatusCode::OK, "transfer: {transferred}");
    assert_eq!(transferred["data"]["status"], "completed");

    sqlx::query("UPDATE org_memberships SET deactivated_at=now() WHERE org_id=$1 AND user_id=$2")
        .bind(org.id)
        .bind(creator.id)
        .execute(&app.db)
        .await
        .unwrap();
    let mut request = line_comment_request(&workspace, "please look");
    request["mentionUserIds"] = json!([creator.id]);
    let path = format!("/api/v1/workspaces/{}/comments", workspace.id);
    let refused = next_client.post_json(&path, &request).await;
    let status = refused.status();
    let body: Value = refused.json().await.unwrap();
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["error"]["code"], "VALIDATION_ERROR", "{body}");
    assert_eq!(
        body["error"]["details"]["field"], "mentionUserIds",
        "{body}"
    );
    assert_eq!(
        count_mention_notifications(&app, workspace.id, creator.id).await,
        0
    );

    sqlx::query("UPDATE org_memberships SET deactivated_at=NULL WHERE org_id=$1 AND user_id=$2")
        .bind(org.id)
        .bind(creator.id)
        .execute(&app.db)
        .await
        .unwrap();
    let accepted = next_client.post_json(&path, &request).await;
    assert_eq!(
        accepted.status(),
        StatusCode::CREATED,
        "reactivated creator"
    );
    assert_eq!(
        count_mention_notifications(&app, workspace.id, creator.id).await,
        1
    );

    app.cleanup().await;
}

/// g5.documents.comments.authority lives in CommentService, not only in the
/// routes' scope gate: called directly on a transaction scoped to a former
/// owner, the service still lets them start a thread (they can read the
/// workspace) but refuses to resolve, reopen or delete one; for the current
/// owner it does each.
#[tokio::test]
async fn test_comment_service_decides_thread_authority_itself() {
    use g5_api::services::comment_service::{CreateCommentRequest, UpdateCommentRequest};
    use g5_contracts::G5Error;

    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let former = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let workspace = comment_workspace(&app, &org).await;
    let current = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let thread_id: Uuid = create_line_comment(
        &app.authenticated_client(&former),
        &workspace.id.to_string(),
        workspace.repo_id,
        "docs/plan.md",
        1,
        "written while owner",
    )
    .await
    .parse()
    .expect("thread id");
    sqlx::query("UPDATE workspaces SET owner_user_id = $2 WHERE id = $1")
        .bind(workspace.id)
        .bind(current.id)
        .execute(&app.db)
        .await
        .expect("transfer ownership");

    let service = &app.state.comment_service;
    let new_thread = || CreateCommentRequest {
        id: None,
        scope: "line".to_string(),
        content: "a new thread".to_string(),

        repo_id: Some(workspace.repo_id),
        file_path: Some("docs/plan.md".to_string()),
        line_number: Some(2),
        line_hash: Some(TEST_LINE_HASH.to_string()),
        content_hash: Some(TEST_CONTENT_HASH.to_string()),
        anchor_context: Some(TEST_ANCHOR_CONTEXT.to_string()),
        commit_sha: Some(workspace.head_commit.clone()),
        origin_branch: None,
        commit_range_base: None,
        commit_range_head: None,
    };
    let set_resolved = |resolved| UpdateCommentRequest {
        content: None,
        resolved: Some(resolved),
    };
    let refused = |error: g5_api::errors::AppError| {
        assert!(
            matches!(error.inner, G5Error::Forbidden { .. }),
            "refused as not the owner: {:?}",
            error.inner
        );
    };

    let mut db = app.scoped_db_for_user_org(former.id, org.id).await;
    let readers_thread = service
        .create_comment(&mut db, workspace.id, former.id, new_thread())
        .await
        .expect("a former owner still reads the workspace, so starts a thread");
    assert_eq!(readers_thread.author_id, former.id);
    db.commit().await.expect("commit");
    for resolved in [true, false] {
        let mut db = app.scoped_db_for_user_org(former.id, org.id).await;
        refused(
            service
                .update_comment(&mut db, thread_id, former.id, set_resolved(resolved))
                .await
                .expect_err("a former owner cannot resolve or reopen"),
        );
    }
    let mut db = app.scoped_db_for_user_org(former.id, org.id).await;
    refused(
        service
            .delete_comment(&mut db, thread_id, former.id)
            .await
            .expect_err("a former owner cannot delete"),
    );
    drop(db);
    assert!(comment_row_exists(&app, &thread_id.to_string()).await);

    let mut db = app.scoped_db_for_user_org(current.id, org.id).await;
    service
        .create_comment(&mut db, workspace.id, current.id, new_thread())
        .await
        .expect("the current owner starts a thread");
    for resolved in [true, false] {
        service
            .update_comment(&mut db, thread_id, current.id, set_resolved(resolved))
            .await
            .expect("the current owner resolves and reopens");
    }
    service
        .delete_comment(&mut db, thread_id, current.id)
        .await
        .expect("the current owner deletes the former owner's thread");
    db.commit().await.expect("commit");
    assert!(!comment_row_exists(&app, &thread_id.to_string()).await);

    app.cleanup().await;
}

/// g5.documents.comments.authority lives in CommentService: tenant row
/// security admits every org member, so the service itself refuses a member
/// who cannot read a personal workspace, for starting a thread and for
/// replying, and admits them after organization promotion.
#[tokio::test]
async fn test_comment_service_checks_a_readers_access_to_start_and_reply() {
    use g5_contracts::G5Error;

    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let owner = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let workspace = seeded_comment_workspace(&app, &org, Some(owner.id)).await;
    let member = org_visible_reader(&app, org.id).await;
    let thread_id: Uuid = create_line_comment(
        &app.authenticated_client(&owner),
        &workspace.id.to_string(),
        workspace.repo_id,
        "docs/plan.md",
        1,
        "private note",
    )
    .await
    .parse()
    .expect("thread id");

    let service = &app.state.comment_service;
    let new_thread = || g5_api::services::comment_service::CreateCommentRequest {
        id: None,
        scope: "line".to_string(),
        content: "a private thread".to_string(),

        repo_id: Some(workspace.repo_id),
        file_path: Some("docs/plan.md".to_string()),
        line_number: Some(2),
        line_hash: Some(TEST_LINE_HASH.to_string()),
        content_hash: Some(TEST_CONTENT_HASH.to_string()),
        anchor_context: Some(TEST_ANCHOR_CONTEXT.to_string()),
        commit_sha: Some(workspace.head_commit.clone()),
        origin_branch: None,
        commit_range_base: None,
        commit_range_head: None,
    };
    let mut db = app.scoped_db_for_user_org(member.id, org.id).await;
    let refused = service
        .add_reply(&mut db, thread_id, member.id, "let me in".to_string())
        .await
        .expect_err("an ordinary member cannot reply");
    assert!(
        matches!(refused.inner, G5Error::Forbidden { .. }),
        "refused for lack of read access: {:?}",
        refused.inner
    );
    drop(db);
    let mut db = app.scoped_db_for_user_org(member.id, org.id).await;
    let refused = service
        .create_comment(&mut db, workspace.id, member.id, new_thread())
        .await
        .expect_err("an ordinary member cannot start a thread");
    assert!(
        matches!(refused.inner, G5Error::Forbidden { .. }),
        "refused for lack of read access: {:?}",
        refused.inner
    );
    drop(db);

    sqlx::query("UPDATE org_memberships SET role='admin' WHERE org_id=$1 AND user_id=$2")
        .bind(org.id)
        .bind(member.id)
        .execute(&app.db)
        .await
        .unwrap();
    let mut db = app.scoped_db_for_user_org(member.id, org.id).await;
    service
        .add_reply(
            &mut db,
            thread_id,
            member.id,
            "administrator reply".to_string(),
        )
        .await
        .expect("an organization administrator replies");
    service
        .create_comment(&mut db, workspace.id, member.id, new_thread())
        .await
        .expect("an organization administrator starts a thread");
    db.commit().await.expect("commit");

    app.cleanup().await;
}

/// g5.security.current-authority for reader thread writes: the membership
/// and audience a reader's thread or reply was admitted on stay held until
/// the write commits, so a revocation or audience change that would withdraw
/// them waits behind the write instead of landing between admission and
/// commit; once the write commits the revocation lands and the next write
/// is refused.
#[tokio::test]
async fn test_a_readers_thread_write_holds_its_access_until_commit() {
    use g5_api::services::comment_service::CreateCommentRequest;
    use g5_contracts::G5Error;

    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let owner = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let workspace = comment_workspace(&app, &org).await;
    let member = org_visible_reader(&app, org.id).await;
    let thread_id: Uuid = create_line_comment(
        &app.authenticated_client(&owner),
        &workspace.id.to_string(),
        workspace.repo_id,
        "docs/plan.md",
        1,
        "owner's note",
    )
    .await
    .parse()
    .expect("thread id");
    let new_thread = || CreateCommentRequest {
        id: None,
        scope: "line".to_string(),
        content: "while active".to_string(),

        repo_id: Some(workspace.repo_id),
        file_path: Some("docs/plan.md".to_string()),
        line_number: Some(2),
        line_hash: Some(TEST_LINE_HASH.to_string()),
        content_hash: Some(TEST_CONTENT_HASH.to_string()),
        anchor_context: Some(TEST_ANCHOR_CONTEXT.to_string()),
        commit_sha: Some(workspace.head_commit.clone()),
        origin_branch: None,
        commit_range_base: None,
        commit_range_head: None,
    };
    let service = &app.state.comment_service;

    // A revocation attempted while the admitted write is open must wait for
    // it (here: time out), for both a new thread and a reply.
    async fn revocation_waits(app: &TestApp, workspace_id: Uuid, member_id: Uuid) {
        let mut tx = app.db.begin().await.expect("revoker transaction");
        sqlx::query("SET LOCAL lock_timeout = '300ms'")
            .execute(&mut *tx)
            .await
            .expect("lock timeout");
        let revoked =
            sqlx::query("UPDATE org_memberships SET deactivated_at=now() WHERE org_id = (SELECT org_id FROM workspaces WHERE id = $1) AND user_id = $2")
                .bind(workspace_id)
                .bind(member_id)
                .execute(&mut *tx)
                .await
                .expect_err("the membership stays held until the write commits");
        assert_eq!(
            revoked
                .as_database_error()
                .and_then(|e| e.code())
                .as_deref(),
            Some("55P03"),
            "lock timeout, not another failure: {revoked:?}"
        );
        tx.rollback().await.expect("rollback");
        let mut tx = app.db.begin().await.expect("audience transaction");
        sqlx::query("SET LOCAL lock_timeout = '300ms'")
            .execute(&mut *tx)
            .await
            .expect("lock timeout");
        let closed = sqlx::query("UPDATE workspaces SET deleted_at=now() WHERE id = $1")
            .bind(workspace_id)
            .execute(&mut *tx)
            .await
            .expect_err("the audience stays held until the write commits");
        assert_eq!(
            closed.as_database_error().and_then(|e| e.code()).as_deref(),
            Some("55P03"),
            "lock timeout, not another failure: {closed:?}"
        );
        tx.rollback().await.expect("rollback");
        // The content policy is read under the same hold, so a read-only
        // transition waits too.
        let mut tx = app.db.begin().await.expect("policy transaction");
        sqlx::query("SET LOCAL lock_timeout = '300ms'")
            .execute(&mut *tx)
            .await
            .expect("lock timeout");
        let frozen =
            sqlx::query("UPDATE workspaces SET mutation_policy = 'read_only' WHERE id = $1")
                .bind(workspace_id)
                .execute(&mut *tx)
                .await
                .expect_err("the policy stays held until the write commits");
        assert_eq!(
            frozen.as_database_error().and_then(|e| e.code()).as_deref(),
            Some("55P03"),
            "lock timeout, not another failure: {frozen:?}"
        );
        tx.rollback().await.expect("rollback");
    }

    let mut db = app.scoped_db_for_user_org(member.id, org.id).await;
    let started = service
        .create_comment(&mut db, workspace.id, member.id, new_thread())
        .await
        .expect("an active member starts a thread");
    revocation_waits(&app, workspace.id, member.id).await;
    db.commit().await.expect("the admitted thread commits");
    assert!(comment_row_exists(&app, &started.id.to_string()).await);

    let mut db = app.scoped_db_for_user_org(member.id, org.id).await;
    service
        .add_reply(&mut db, thread_id, member.id, "while active".to_string())
        .await
        .expect("an active member replies");
    revocation_waits(&app, workspace.id, member.id).await;
    db.commit().await.expect("the admitted reply commits");

    // With no write open, the revocation lands and the next write is refused.
    let revoked =
        sqlx::query("UPDATE org_memberships SET deactivated_at=now() WHERE org_id = (SELECT org_id FROM workspaces WHERE id = $1) AND user_id = $2")
            .bind(workspace.id)
            .bind(member.id)
            .execute(&app.db)
            .await
            .expect("revoke the membership");
    assert_eq!(revoked.rows_affected(), 1);
    let mut db = app.scoped_db_for_org(org.id).await;
    let refused = service
        .create_comment(&mut db, workspace.id, member.id, new_thread())
        .await
        .expect_err("a revoked member cannot start a thread");
    assert!(
        matches!(refused.inner, G5Error::NotFound { .. }),
        "refused for lack of read access: {:?}",
        refused.inner
    );
    let refused = service
        .add_reply(
            &mut db,
            thread_id,
            member.id,
            "after revocation".to_string(),
        )
        .await
        .expect_err("a revoked member cannot reply");
    assert!(
        matches!(refused.inner, G5Error::NotFound { .. }),
        "refused for lack of read access: {:?}",
        refused.inner
    );
    drop(db);

    app.cleanup().await;
}

/// g5.workspace.content-policy under g5.security.current-authority: the
/// read-only policy is read under the reader's access hold, so a transition
/// that is already in flight when a reader's write starts makes the write
/// wait and then refuses it; the old order, which read the policy before
/// taking the hold, would have proceeded on the stale read.
#[tokio::test]
async fn test_a_readers_thread_write_sees_a_transition_committed_while_it_waited() {
    use g5_api::services::comment_service::{CreateCommentRequest, UpdateCommentRequest};
    use g5_contracts::G5Error;
    use std::time::Duration;

    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let owner = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let workspace = comment_workspace(&app, &org).await;
    let reader = org_visible_reader(&app, org.id).await;
    let owners_thread: Uuid = create_line_comment(
        &app.authenticated_client(&owner),
        &workspace.id.to_string(),
        workspace.repo_id,
        "docs/plan.md",
        1,
        "owner's note",
    )
    .await
    .parse()
    .expect("thread id");
    let service = &app.state.comment_service;
    let new_thread = || CreateCommentRequest {
        id: None,
        scope: "line".to_string(),
        content: "started during the transition".to_string(),

        repo_id: Some(workspace.repo_id),
        file_path: Some("docs/plan.md".to_string()),
        line_number: Some(2),
        line_hash: Some(TEST_LINE_HASH.to_string()),
        content_hash: Some(TEST_CONTENT_HASH.to_string()),
        anchor_context: Some(TEST_ANCHOR_CONTEXT.to_string()),
        commit_sha: Some(workspace.head_commit.clone()),
        origin_branch: None,
        commit_range_base: None,
        commit_range_head: None,
    };

    /// Starts a read-only transition and leaves it uncommitted, holding the
    /// workspace row, so a write started afterwards must wait on it.
    async fn hold_transition(
        app: &TestApp,
        workspace_id: Uuid,
    ) -> sqlx::Transaction<'static, sqlx::Postgres> {
        let mut tx = app.db.begin().await.expect("transition transaction");
        let updated =
            sqlx::query("UPDATE workspaces SET mutation_policy = 'read_only' WHERE id = $1")
                .bind(workspace_id)
                .execute(&mut *tx)
                .await
                .expect("hold the uncommitted transition");
        assert_eq!(
            updated.rows_affected(),
            1,
            "the transition locks the workspace row"
        );
        tx
    }

    /// Commits the held transition once another session of this test's
    /// database is seen waiting on a lock: the server is idle, so that
    /// session is the write under test, blocked on the workspace row.
    async fn commit_once_write_waits(
        app: &TestApp,
        tx: sqlx::Transaction<'static, sqlx::Postgres>,
    ) {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        loop {
            let waiting: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM pg_stat_activity
                 WHERE datname = current_database()
                   AND wait_event_type = 'Lock'
                   AND pid <> pg_backend_pid()",
            )
            .fetch_one(&app.db)
            .await
            .expect("activity");
            if waiting >= 1 {
                break;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "the write never waited on the workspace row"
            );
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        tx.commit().await.expect("the transition commits");
    }

    // A new thread: refused as read-only, nothing inserted.
    let held = hold_transition(&app, workspace.id).await;
    let mut db = app.scoped_db_for_user_org(reader.id, org.id).await;
    let (started, ()) = tokio::join!(
        service.create_comment(&mut db, workspace.id, reader.id, new_thread()),
        commit_once_write_waits(&app, held),
    );
    let refused = started.expect_err("the write that waited sees the committed transition");
    assert!(
        matches!(refused.inner, G5Error::WorkspaceReadOnly { .. }),
        "refused as read-only: {:?}",
        refused.inner
    );
    // The refused write still holds its share lock until the transaction
    // ends; dropping only queues the rollback for the connection's next use,
    // so roll back now or the policy restore below waits on that lock.
    db.rollback().await.expect("release the reader's hold");
    let threads: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM comment_index WHERE workspace_id = $1 AND content = $2",
    )
    .bind(workspace.id)
    .bind("started during the transition")
    .fetch_one(&app.db)
    .await
    .expect("count");
    assert_eq!(threads, 0, "nothing was written");

    // Back to read-write; the author's text edit takes the same path.
    sqlx::query("UPDATE workspaces SET mutation_policy = 'read_write' WHERE id = $1")
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .expect("restore editing");
    let held = hold_transition(&app, workspace.id).await;
    let mut db = app.scoped_db_for_user_org(owner.id, org.id).await;
    let (edited, ()) = tokio::join!(
        service.update_comment(
            &mut db,
            owners_thread,
            owner.id,
            UpdateCommentRequest {
                content: Some("edited during the transition".to_string()),
                resolved: None,
            },
        ),
        commit_once_write_waits(&app, held),
    );
    let refused = edited.expect_err("the edit that waited sees the committed transition");
    assert!(
        matches!(refused.inner, G5Error::WorkspaceReadOnly { .. }),
        "refused as read-only: {:?}",
        refused.inner
    );
    db.rollback().await.expect("release the author's hold");
    let content: String = sqlx::query_scalar("SELECT content FROM comment_index WHERE id = $1")
        .bind(owners_thread)
        .fetch_one(&app.db)
        .await
        .expect("content");
    assert_eq!(content, "owner's note", "the text was not changed");

    app.cleanup().await;
}

/// g5.documents.comments.replies and g5.workspace.identity.base: the shared
/// thread writers refuse Base for every entry path, review or anchored, so
/// no caller can skip the rule.
#[tokio::test]
async fn test_thread_writers_refuse_base_for_every_caller() {
    use g5_api::services::comment_service::{append_thread_reply, open_suggestion_thread};
    use g5_contracts::G5Error;

    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let base = TestWorkspace::create_base(&app, &org).await;
    let thread = insert_line_comment_directly(
        &app,
        DirectLineComment {
            workspace_id: base.id,
            repo_id: base.repo_id,
            author_id: user.id,
            file_path: "docs/plan.md",
            line_number: 1,
            origin_branch: "main",
            content: "written before protection",
        },
    )
    .await;

    let mut db = app.scoped_db_for_org(org.id).await;
    let reply = append_thread_reply(&mut db, thread, user.id, "an agent's note")
        .await
        .expect_err("Base refuses a thread message");
    assert!(
        matches!(reply, G5Error::WorkspaceReadOnly { .. }),
        "{reply:?}"
    );
    let opened = open_suggestion_thread(
        &mut db,
        org.id,
        base.id,
        "main",
        user.id,
        Some(base.repo_id),
        Some("docs/plan.md"),
        Some(1),
        "a review note",
    )
    .await
    .expect_err("Base refuses a new review thread");
    assert!(
        matches!(opened, G5Error::WorkspaceReadOnly { .. }),
        "{opened:?}"
    );
    drop(db);

    app.cleanup().await;
}

/// g5.documents.comments.identity: a file's threads are identified by
/// workspace, repository and path; the branch label never hides one.
#[tokio::test]
async fn test_branch_label_never_hides_a_files_threads() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let client = app.authenticated_client(&user);
    let workspace = comment_workspace(&app, &org).await;

    let mut request = line_comment_request(&workspace, "labelled elsewhere");
    request["originBranch"] = json!("workspace/feature-x");
    let created = client
        .post_json(
            &format!("/api/v1/workspaces/{}/comments", workspace.id),
            &request,
        )
        .await;
    assert_eq!(created.status(), StatusCode::CREATED);
    let created: Value = created.json().await.unwrap();
    assert_eq!(created["data"]["originBranch"], "workspace/feature-x");
    let id = created["data"]["id"].as_str().unwrap().to_string();

    for query in [
        format!("repoId={}&filePath=docs/plan.md", workspace.repo_id),
        format!(
            "repoId={}&filePath=docs/plan.md&branch=main",
            workspace.repo_id
        ),
        format!(
            "repoId={}&filePath=docs/plan.md&branch=workspace/feature-x",
            workspace.repo_id
        ),
    ] {
        let listed: Value = client
            .get(&format!(
                "/api/v1/workspaces/{}/comments?{query}",
                workspace.id
            ))
            .await
            .json()
            .await
            .unwrap();
        let ids: Vec<&str> = listed["data"]
            .as_array()
            .unwrap()
            .iter()
            .map(|comment| comment["id"].as_str().unwrap())
            .collect();
        assert_eq!(ids, vec![id.as_str()], "listing with {query}");
    }

    // An unlabelled comment is labelled with the workspace's own branch,
    // which is named after the workspace, not with a fixed default.
    let unlabelled = client
        .post_json(
            &format!("/api/v1/workspaces/{}/comments", workspace.id),
            &line_comment_request(&workspace, "no label sent"),
        )
        .await;
    assert_eq!(unlabelled.status(), StatusCode::CREATED);
    let unlabelled: Value = unlabelled.json().await.unwrap();
    assert_eq!(unlabelled["data"]["originBranch"], workspace.name.as_str());

    app.cleanup().await;
}

/// Retired request fields fail explicitly instead of selecting a different scope.
/// Existing node data is rejected by migration 090, before readers are removed.
#[tokio::test]
async fn test_retired_node_comment_inputs_are_refused() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let client = app.authenticated_client(&user);
    let workspace = comment_workspace(&app, &org).await;
    let path = format!("/api/v1/workspaces/{}/comments", workspace.id);

    let refused = client
        .post_json(
            &path,
            &json!({"scope":"node", "content":"old input", "nodeId":Uuid::new_v4()}),
        )
        .await;
    assert_eq!(refused.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let retired_filter = client
        .get(&format!("{path}?nodeId={}", Uuid::new_v4()))
        .await;
    assert_eq!(retired_filter.status(), StatusCode::BAD_REQUEST);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM comment_index WHERE workspace_id=$1")
        .bind(workspace.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(count, 0);
    app.cleanup().await;
}

/// Blank comment text is refused at the service boundary.
#[tokio::test]
async fn test_blank_comment_text_is_refused() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let client = app.authenticated_client(&user);
    let workspace = comment_workspace(&app, &org).await;
    let response = client
        .post_json(
            &format!("/api/v1/workspaces/{}/comments", workspace.id),
            &line_comment_request(&workspace, " \n "),
        )
        .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

    // An update that changes nothing is refused rather than written.
    let comment_id = create_line_comment(
        &client,
        &workspace.id.to_string(),
        workspace.repo_id,
        "docs/plan.md",
        1,
        "keep me",
    )
    .await;
    let empty = client
        .patch_json(&format!("/api/v1/comments/{comment_id}"), &json!({}))
        .await;
    assert_eq!(empty.status(), StatusCode::UNPROCESSABLE_ENTITY);
    app.cleanup().await;
}

/// g5.documents.comments.authority / .replies: Base is read-only for every
/// user, so it accepts no thread write, even from its owner.
#[tokio::test]
async fn test_base_workspace_refuses_comment_writes() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let client = app.authenticated_client(&user);
    let base = TestWorkspace::create_base(&app, &org).await;
    let existing = insert_line_comment_directly(
        &app,
        DirectLineComment {
            workspace_id: base.id,
            repo_id: base.repo_id,
            author_id: user.id,
            file_path: "docs/plan.md",
            line_number: 1,
            origin_branch: "main",
            content: "written before protection",
        },
    )
    .await;

    let create = client
        .post_json(
            &format!("/api/v1/workspaces/{}/comments", base.id),
            &line_comment_request(&base, "on base"),
        )
        .await;
    assert_eq!(create.status(), StatusCode::FORBIDDEN, "create on base");
    let reply = client
        .post_json(
            &format!("/api/v1/comments/{existing}/replies"),
            &json!({"content": "reply on base"}),
        )
        .await;
    assert_eq!(reply.status(), StatusCode::FORBIDDEN, "reply on base");
    let resolve = client
        .patch_json(
            &format!("/api/v1/comments/{existing}"),
            &json!({"resolved": true}),
        )
        .await;
    assert_eq!(resolve.status(), StatusCode::FORBIDDEN, "resolve on base");

    app.cleanup().await;
}

/// g5.documents.comments.identity / g5.governance.review-thread: an
/// anchored-comment request never reads or changes a review thread.
#[tokio::test]
async fn test_anchored_comment_routes_do_not_reach_review_threads() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let client = app.authenticated_client(&user);
    let workspace = comment_workspace(&app, &org).await;

    let thread_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO comment_index (id, org_id, workspace_id, author_id, scope, repo_id, file_path, line_number, content, origin_branch)
         VALUES ($1, $2, $3, $4, 'suggestion', $5, 'README.md', 1, 'review note', 'main')",
    )
    .bind(thread_id)
    .bind(org.id)
    .bind(workspace.id)
    .bind(user.id)
    .bind(workspace.repo_id)
    .execute(&app.db)
    .await
    .expect("review thread row");

    assert_eq!(
        client
            .get(&format!("/api/v1/comments/{thread_id}/replies"))
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        client
            .post_json(
                &format!("/api/v1/comments/{thread_id}/replies"),
                &json!({"content": "around the review"}),
            )
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        client
            .patch_json(
                &format!("/api/v1/comments/{thread_id}"),
                &json!({"resolved": true}),
            )
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        client
            .delete(&format!("/api/v1/comments/{thread_id}"))
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    assert!(comment_row_exists(&app, &thread_id.to_string()).await);
    let listed: Value = client
        .get(&format!("/api/v1/workspaces/{}/comments", workspace.id))
        .await
        .json()
        .await
        .unwrap();
    assert!(listed["data"].as_array().unwrap().is_empty());

    app.cleanup().await;
}

/// g5.documents.comments.live: other viewers see every thread change, and
/// no event goes out for a change that was not committed.
#[tokio::test]
async fn test_thread_changes_reach_other_viewers_only_after_commit() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let owner = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let workspace = comment_workspace(&app, &org).await;
    let viewer = org_visible_reader(&app, org.id).await;
    let owner_client = app.authenticated_client(&owner);
    let mut socket = app
        .ws_connect(&viewer, &format!("workspace:{}", workspace.id))
        .await;
    let wait = std::time::Duration::from_secs(5);

    // A change whose transaction never commits announces nothing.
    {
        let mut db = app.scoped_db_for_user_org(owner.id, org.id).await;
        app.state
            .comment_service
            .create_comment(
                &mut db,
                workspace.id,
                owner.id,
                g5_api::services::comment_service::CreateCommentRequest {
                    id: None,
                    scope: "line".to_string(),
                    content: "rolled back".to_string(),

                    repo_id: Some(workspace.repo_id),
                    file_path: Some("docs/plan.md".to_string()),
                    line_number: Some(1),
                    line_hash: Some(TEST_LINE_HASH.to_string()),
                    content_hash: Some(TEST_CONTENT_HASH.to_string()),
                    anchor_context: Some(TEST_ANCHOR_CONTEXT.to_string()),
                    commit_sha: None,
                    origin_branch: None,
                    commit_range_base: None,
                    commit_range_head: None,
                },
            )
            .await
            .expect("service create");
        drop(db);
    }
    socket
        .expect_no_event("comment.new", std::time::Duration::from_millis(700))
        .await;

    let comment_id = create_line_comment(
        &owner_client,
        &workspace.id.to_string(),
        workspace.repo_id,
        "docs/plan.md",
        2,
        "committed",
    )
    .await;
    let event = socket.expect_event("comment.new", wait).await;
    assert_eq!(event["commentId"], comment_id);
    assert!(
        comment_row_exists(&app, &comment_id).await,
        "the row is stored when the event arrives"
    );

    let viewer_client = app.authenticated_client(&viewer);
    let reply = viewer_client
        .post_json(
            &format!("/api/v1/comments/{comment_id}/replies"),
            &json!({"content": "noted"}),
        )
        .await;
    assert_eq!(reply.status(), StatusCode::CREATED);
    socket.expect_event("comment.reply", wait).await;

    for resolved in [true, false] {
        let response = owner_client
            .patch_json(
                &format!("/api/v1/comments/{comment_id}"),
                &json!({"resolved": resolved}),
            )
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        let event = socket.expect_event("comment.updated", wait).await;
        assert_eq!(event["resolved"], resolved);
    }

    // The author edits the text: viewers refetch it from the same event.
    let edited = owner_client
        .patch_json(
            &format!("/api/v1/comments/{comment_id}"),
            &json!({"content": "committed, reworded"}),
        )
        .await;
    assert_eq!(edited.status(), StatusCode::OK);
    let event = socket.expect_event("comment.updated", wait).await;
    assert_eq!(event["commentId"], comment_id);

    let deleted = owner_client
        .delete(&format!("/api/v1/comments/{comment_id}"))
        .await;
    assert_eq!(deleted.status(), StatusCode::OK);
    let event = socket.expect_event("comment.deleted", wait).await;
    assert_eq!(event["commentId"], comment_id);

    app.cleanup().await;
}

/// The shared after-commit hook behind g5.design.commit-notify: actions run
/// only after a successful commit, never after a rollback or a drop, and a
/// savepoint rollback withdraws what it had queued.
#[tokio::test]
async fn test_scoped_db_runs_commit_actions_only_after_commit() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let ran = Arc::new(AtomicUsize::new(0));
    let counter = |ran: &Arc<AtomicUsize>, weight: usize| {
        let ran = ran.clone();
        move || {
            ran.fetch_add(weight, Ordering::SeqCst);
        }
    };

    let mut dropped = app.scoped_db_for_org(org.id).await;
    dropped.on_commit(counter(&ran, 1));
    drop(dropped);

    let mut rolled_back = app.scoped_db_for_org(org.id).await;
    rolled_back.on_commit(counter(&ran, 10));
    rolled_back.rollback().await.expect("rollback");
    assert_eq!(ran.load(Ordering::SeqCst), 0, "no commit, no action");

    let mut committed = app.scoped_db_for_org(org.id).await;
    committed.on_commit(counter(&ran, 100));
    let mark = committed.commit_actions_mark();
    committed.on_commit(counter(&ran, 1000));
    committed.discard_commit_actions_since(mark);
    assert_eq!(ran.load(Ordering::SeqCst), 0, "queued until commit");
    committed.commit().await.expect("commit");
    assert_eq!(
        ran.load(Ordering::SeqCst),
        100,
        "the kept action ran once; the discarded one never"
    );

    app.cleanup().await;
}

/// g5.design.commit-notify for the shared notification owner: a notice is
/// delivered only once its transaction commits.
#[tokio::test]
async fn test_notifications_are_delivered_only_after_commit() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let actor = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let workspace = comment_workspace(&app, &org).await;
    let recipient = org_visible_reader(&app, org.id).await;
    let mut socket = app
        .ws_connect(&recipient, &format!("user:{}", recipient.id))
        .await;
    let request = |title: &str| g5_api::services::notification_service::CreateNotificationRequest {
        workspace_id: workspace.id,
        recipient_user_id: recipient.id,
        actor_user_id: Some(actor.id),
        category: "review".to_string(),
        severity: "info".to_string(),
        title: title.to_string(),
        message: String::new(),
        task_id: None,
        comment_id: None,
        reply_id: None,
        actions: vec![],
        expires_at: None,
    };

    let mut db = app.scoped_db_for_user_org(actor.id, org.id).await;
    app.state
        .notification_service
        .create_notification(&mut db, request("rolled back"))
        .await
        .expect("create notification");
    drop(db);
    socket
        .expect_no_event("notification", std::time::Duration::from_millis(700))
        .await;

    let mut db = app.scoped_db_for_user_org(actor.id, org.id).await;
    app.state
        .notification_service
        .create_notification(&mut db, request("committed"))
        .await
        .expect("create notification");
    socket
        .expect_no_event("notification", std::time::Duration::from_millis(300))
        .await;
    db.commit().await.expect("commit");
    let event = socket
        .expect_event("notification", std::time::Duration::from_secs(5))
        .await;
    assert_eq!(event["notification"]["title"], "committed");

    app.cleanup().await;
}

/// g5.security.tenant.rls: mention edges carry their workspace's
/// organization and the table forces row-level security.
#[tokio::test]
async fn test_mention_edges_are_tenant_scoped() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let author = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let workspace = comment_workspace(&app, &org).await;
    let mentioned = org_visible_reader(&app, org.id).await;
    let client = app.authenticated_client(&author);

    let mut request = line_comment_request(&workspace, "please look");
    request["mentionUserIds"] = json!([mentioned.id]);
    let created = client
        .post_json(
            &format!("/api/v1/workspaces/{}/comments", workspace.id),
            &request,
        )
        .await;
    assert_eq!(created.status(), StatusCode::CREATED);

    let org_ids: Vec<Uuid> = sqlx::query_scalar("SELECT org_id FROM comment_mentions")
        .fetch_all(&app.db)
        .await
        .expect("mention rows");
    assert_eq!(org_ids, vec![org.id]);
    let (enabled, forced): (bool, bool) = sqlx::query_as(
        "SELECT relrowsecurity, relforcerowsecurity FROM pg_class WHERE oid = 'public.comment_mentions'::regclass",
    )
    .fetch_one(&app.db)
    .await
    .expect("row security flags");
    assert!(
        enabled && forced,
        "comment_mentions forces row-level security"
    );

    let other_org = TestOrg::create(&app).await;
    let mut other_db = app.scoped_db_for_org(other_org.id).await;
    let visible: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM comment_mentions")
        .fetch_one(other_db.conn())
        .await
        .expect("tenant count");
    assert_eq!(visible, 0, "another organization sees no mention edges");
    drop(other_db);

    app.cleanup().await;
}

#[tokio::test]
async fn owner_can_comment_on_unpushed_commit_but_reader_cannot() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let owner = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let workspace = comment_workspace(&app, &org).await;
    let reader = org_visible_reader(&app, org.id).await;
    let mut request = line_comment_request(&workspace, "Private current work");
    request["commitSha"] = json!("f".repeat(40));
    let path = format!("/api/v1/workspaces/{}/comments", workspace.id);
    let response = app
        .authenticated_client(&owner)
        .post_json(&path, &request)
        .await;
    let status = response.status();
    assert_eq!(
        status,
        StatusCode::CREATED,
        "{}",
        response.text().await.unwrap()
    );
    let response = app
        .authenticated_client(&reader)
        .post_json(&path, &request)
        .await;
    assert!(!response.status().is_success());
    app.cleanup().await;
}
