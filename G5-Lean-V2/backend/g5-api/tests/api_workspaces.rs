//! Integration tests for G5 API workspace endpoints.
//!
//! Covers the complete request→route→middleware→service→database→response cycle
//! for the workspace endpoints across the org/workspace path prefixes:
//!
//! 1. **`GET /api/v1/orgs/:orgId/workspaces`** — List workspaces
//! 4. **`GET /api/v1/workspaces/:id`** — Get workspace detail
//! 5. **`PATCH /api/v1/workspaces/:id`** — Update workspace fields
//! 6. **`POST /api/v1/workspaces/:id/delete`** — Delete workspace
//! 7. **`GET /api/v1/workspaces/:id/stats`** — Get workspace statistics
//! 10. **`GET /api/v1/workspaces/:id/branches`** — List branches
//! 11. **`POST /api/v1/workspaces/:id/branches`** — Add branch
//! 12. The legacy branch-removal endpoint is retired
//! 13. **`POST /api/v1/workspaces/:id/sync`** — Sync workspace
//! 14. **`POST /api/v1/workspaces/:id/merge`** — Merge workspace
//! 15. **`POST /api/v1/workspaces/:id/clone`** — Clone workspace
//! 16. Legacy workspace-member routes are retired.
//! 19. **`POST /api/v1/orgs/:orgId/workspaces/:workspaceId/repos`** — Add a repository
//!
//! Each test spawns a full `TestApp` instance with an isolated Postgres database,
//! makes real HTTP requests via `AuthenticatedClient` (or raw `reqwest::Client`
//! for unauthenticated tests), and asserts at the HTTP response level (status codes,
//! JSON body shapes) and the database level (verifying rows were created/updated/deleted).

use crate::helpers;
use g5_api::ws::host_ctl::ControlPlaneHostState;
use g5_auth::Scope;
use g5_scm::GitRepo;

use g5_contracts::types::WorkspaceRepoKey;
use helpers::{
    assert_json_error, seed_workspace_authority_head, TestApp, TestOrg, TestUser, TestWorkspace,
    TEST_WORKER_IMAGE_REF,
};
use reqwest::StatusCode;
use serde_json::{json, Value};
use sqlx::Row;
use std::collections::HashSet;
use std::io::Read;
use std::process::Command;
use uuid::Uuid;

/// Workspace lifecycle queries must run through the caller's reusable tenant
/// scope. The restricted system pool intentionally sees no tenant workspace or
/// worker-host rows under forced RLS.
#[tokio::test]
async fn workspace_vm_routes_use_tenant_scope_for_view_and_placement() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    workspace.attach_parent(&app).await;
    sqlx::query("UPDATE orgs SET default_base_image_ref = $1 WHERE id = $2")
        .bind(TEST_WORKER_IMAGE_REF)
        .bind(org.id)
        .execute(&app.db)
        .await
        .expect("set the workspace image policy");

    let system_can_see_workspace: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM workspaces WHERE id = $1)")
            .bind(workspace.id)
            .fetch_one(&app.system_pool)
            .await
            .expect("restricted system-pool workspace lookup should execute");
    assert!(
        !system_can_see_workspace,
        "the regression requires forced RLS to hide tenant workspaces from the system pool"
    );

    let client = app.authenticated_client(&user);
    let view_response = client
        .get(&format!("/api/v1/workspaces/{}/vm", workspace.id))
        .await;
    assert_eq!(view_response.status(), StatusCode::OK);
    let view: Value = view_response.json().await.expect("workspace VM view JSON");
    assert_eq!(view["data"]["workspaceId"], workspace.id.to_string());
    assert_eq!(view["data"]["ownerUserId"], user.id.to_string());
    assert_eq!(view["data"]["status"], "stopped");

    let operation_id = Uuid::new_v4();
    let resume_response = client
        .post_json(
            &format!("/api/v1/workspaces/{}/resume", workspace.id),
            &json!({"operationId": operation_id}),
        )
        .await;
    let resume_status = resume_response.status();
    let resume: Value = resume_response.json().await.expect("workspace resume JSON");
    assert_eq!(
        resume_status,
        StatusCode::OK,
        "owner-authorized placement should reach the no-capacity result: {resume}"
    );
    assert_eq!(resume["data"]["outcome"], "queued_for_capacity");
    // A queued start reads as waiting, with the reason it waits.
    assert_eq!(resume["data"]["vm"]["status"], "queued_for_capacity");
    assert_eq!(resume["data"]["vm"]["lastError"]["reason"], "no_ready_host");

    let intent_status: String = sqlx::query_scalar(
        "SELECT start_intent ->> 'status' FROM workspace_vms WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .expect("durable workspace start intent");
    assert_eq!(intent_status, "queued_for_capacity");

    app.cleanup().await;
}

// ===========================================================================
// Helper: Create Test Repo
// ===========================================================================

async fn assert_json_error_message(
    response: reqwest::Response,
    expected_status: StatusCode,
    expected_code: &str,
    expected_message: &str,
) {
    let status = response.status();
    assert_eq!(
        status, expected_status,
        "Expected status {expected_status}, got {status}"
    );
    let body: Value = response
        .json()
        .await
        .expect("Failed to parse error response as JSON");
    assert_eq!(body["error"]["code"].as_str(), Some(expected_code));
    assert_eq!(body["error"]["message"].as_str(), Some(expected_message));
}

/// Inserts a repo row directly into the database for use in branch management tests.
/// This helper is specific to workspace tests (not in the shared helpers module).
async fn create_test_repo(app: &TestApp, org: &TestOrg) -> Uuid {
    create_named_test_repo(app, org, &format!("test-repo-{}", Uuid::new_v4())).await
}

async fn create_named_test_repo(app: &TestApp, org: &TestOrg, name: &str) -> Uuid {
    let repo_id = Uuid::new_v4();
    let unique = Uuid::new_v4();
    sqlx::query!(
        "INSERT INTO repos (id, org_id, name, slug, git_url, default_branch, provider) VALUES ($1, $2, $3, $4, $5, $6, $7)",
        repo_id,
        org.id,
        name,
        format!("{}-{}", name, unique),
        format!("https://github.com/test/{name}-{unique}.git"),
        "main",
        "github"
    )
    .execute(&app.db)
    .await
    .expect("Failed to insert test repo");
    repo_id
}

fn run_git(repo_path: &std::path::Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo_path)
        .output()
        .expect("failed to execute git");
    assert!(
        output.status.success(),
        "git {:?} failed: stdout={} stderr={}",
        args,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn git_stdout(repo_path: &std::path::Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo_path)
        .output()
        .expect("failed to execute git");
    assert!(
        output.status.success(),
        "git {:?} failed: stdout={} stderr={}",
        args,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("stdout should be utf-8")
        .trim()
        .to_string()
}

async fn set_workspace_branch_head(app: &TestApp, member: WorkspaceRepoKey, head_commit: &str) {
    seed_workspace_authority_head(app, member.workspace_id, member.repo_id, head_commit).await;
}

async fn seed_materialized_repo(app: &TestApp, repo_id: Uuid, readme: &str) -> String {
    let repo_path = app.init_fixture_repo(repo_id).await;
    run_git(&repo_path, &["init"]);
    run_git(&repo_path, &["config", "user.email", "g5@test.local"]);
    run_git(&repo_path, &["config", "user.name", "G5 Test"]);
    run_git(&repo_path, &["checkout", "-B", "main"]);
    std::fs::write(repo_path.join("README.md"), readme).expect("failed to seed README");
    run_git(&repo_path, &["add", "README.md"]);
    run_git(&repo_path, &["commit", "-m", "seed repo"]);
    git_stdout(&repo_path, &["rev-parse", "HEAD"])
}

async fn insert_workspace(
    app: &TestApp,
    org_id: Uuid,
    name: &str,
    cloned_from_id: Option<Uuid>,
) -> Uuid {
    let workspace_id = Uuid::new_v4();
    let owner_user_id = if let Some(user_id) = sqlx::query_scalar::<_, Uuid>(
        "SELECT m.user_id FROM org_memberships m JOIN users u ON u.id=m.user_id \
         WHERE m.org_id = $1 AND m.role IN ('admin', 'member') \
         ORDER BY (u.email LIKE 'fixture-admin-%@g5.test'), \
                  CASE m.role WHEN 'admin' THEN 1 ELSE 2 END, m.created_at, m.user_id \
         LIMIT 1",
    )
    .bind(org_id)
    .fetch_optional(&app.db)
    .await
    .expect("failed to resolve fixture workspace owner")
    {
        user_id
    } else {
        let user_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO users (id, email, display_name) VALUES ($1, $2, 'Workspace Fixture Owner')",
        )
        .bind(user_id)
        .bind(format!("workspace-owner-{user_id}@example.test"))
        .execute(&app.db)
        .await
        .expect("failed to insert fixture workspace owner");
        sqlx::query(
            "INSERT INTO org_memberships (org_id, user_id, role) VALUES ($1, $2, 'member')",
        )
        .bind(org_id)
        .bind(user_id)
        .execute(&app.db)
        .await
        .expect("failed to insert fixture workspace owner membership");
        user_id
    };
    sqlx::query(
        r#"
        INSERT INTO workspaces (
            id, org_id, name, is_base, cloned_from_id,
            owner_user_id, created_by
        )
        VALUES ($1, $2, $3, false, $4, $5, $5)
        "#,
    )
    .bind(workspace_id)
    .bind(org_id)
    .bind(name)
    .bind(cloned_from_id)
    .bind(owner_user_id)
    .execute(&app.db)
    .await
    .expect("failed to insert workspace");

    sqlx::query!(
        r#"
        INSERT INTO workspace_settings (org_id, workspace_id)
        VALUES ($1, $2)
        "#,
        org_id,
        workspace_id,
    )
    .execute(&app.db)
    .await
    .expect("failed to insert workspace settings");

    workspace_id
}

async fn insert_workspace_repo(
    app: &TestApp,
    org_id: Uuid,
    workspace_id: Uuid,
    repo_id: Uuid,
    head_commit: &str,
) -> WorkspaceRepoKey {
    sqlx::query("INSERT INTO workspace_repos (org_id,workspace_id,repo_id,root_path,role) SELECT $1,$2,$3,CASE WHEN name='docs' THEN 'docs' ELSE slug END,CASE WHEN name='docs' THEN 'docs' ELSE 'code' END FROM repos WHERE id=$3")
        .bind(org_id).bind(workspace_id).bind(repo_id).execute(&app.db).await.unwrap();
    seed_workspace_authority_head(app, workspace_id, repo_id, head_commit).await;
    WorkspaceRepoKey {
        org_id,
        workspace_id,
        repo_id,
    }
}
async fn member_head(app: &TestApp, member: WorkspaceRepoKey) -> String {
    let branch: String = sqlx::query_scalar("SELECT CASE WHEN w.is_base THEN r.default_branch ELSE w.name END FROM workspace_repos m JOIN workspaces w ON w.id=m.workspace_id JOIN repos r ON r.id=m.repo_id WHERE m.workspace_id=$1 AND m.repo_id=$2")
        .bind(member.workspace_id).bind(member.repo_id).fetch_one(&app.db).await.unwrap();
    let path = app.repo_path(member.repo_id).await;
    git_stdout(&path, &["rev-parse", &format!("refs/heads/{branch}")])
}

#[tokio::test]
async fn source_download_streams_all_non_doc_workspace_repos() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let client = app.authenticated_client(&user);

    let docs_repo_id = create_named_test_repo(&app, &org, "docs").await;
    let api_repo_id = create_named_test_repo(&app, &org, "api").await;
    let worker_repo_id = create_named_test_repo(&app, &org, "worker").await;
    let docs_head = seed_materialized_repo(&app, docs_repo_id, "docs readme").await;
    let api_head = seed_materialized_repo(&app, api_repo_id, "api readme").await;
    let worker_head = seed_materialized_repo(&app, worker_repo_id, "worker readme").await;
    let workspace_id = insert_workspace(&app, org.id, "download-source", None).await;
    let workspace_branch: String = sqlx::query_scalar("SELECT name FROM workspaces WHERE id = $1")
        .bind(workspace_id)
        .fetch_one(&app.db)
        .await
        .expect("load exact workspace branch name");
    helpers::seed_branch_ref(&app, docs_repo_id, &workspace_branch, &docs_head).await;
    helpers::seed_branch_ref(&app, api_repo_id, &workspace_branch, &api_head).await;
    helpers::seed_branch_ref(&app, worker_repo_id, &workspace_branch, &worker_head).await;

    sqlx::query(
        r#"
        UPDATE workspaces
        SET docs_repo_id = $1
        WHERE id = $2
        "#,
    )
    .bind(docs_repo_id)
    .bind(workspace_id)
    .execute(&app.db)
    .await
    .expect("failed to configure workspace docs repo");

    insert_workspace_repo(&app, org.id, workspace_id, docs_repo_id, &docs_head).await;
    insert_workspace_repo(&app, org.id, workspace_id, api_repo_id, &api_head).await;
    insert_workspace_repo(&app, org.id, workspace_id, worker_repo_id, &worker_head).await;

    let response = client
        .get(&format!(
            "/api/v1/workspaces/{workspace_id}/source-download"
        ))
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("application/zip")
    );
    let disposition = response
        .headers()
        .get(reqwest::header::CONTENT_DISPOSITION)
        .and_then(|value| value.to_str().ok())
        .expect("content disposition header should be present")
        .to_string();
    assert!(disposition.contains("attachment"));
    assert!(disposition.contains(&format!("{workspace_branch}-source.zip")));

    let bytes = response.bytes().await.expect("zip body should download");
    let reader = std::io::Cursor::new(bytes);
    let mut zip = zip::ZipArchive::new(reader).expect("response should be a zip archive");
    let names: Vec<String> = zip
        .file_names()
        .map(|name| name.expect("archive filename should be valid").into_owned())
        .collect();

    assert!(names.contains(&"api/README.md".to_string()));
    assert!(names.contains(&"worker/README.md".to_string()));
    assert!(!names.contains(&"docs/README.md".to_string()));

    let mut api_readme = String::new();
    zip.by_name("api/README.md")
        .expect("api readme should be archived")
        .read_to_string(&mut api_readme)
        .expect("api readme should be utf-8");
    assert_eq!(api_readme, "api readme");

    sqlx::query("UPDATE workspaces SET deleted_at = now() WHERE id = $1")
        .bind(workspace_id)
        .execute(&app.db)
        .await
        .expect("failed to tombstone workspace");

    let deleted_response = client
        .get(&format!(
            "/api/v1/workspaces/{workspace_id}/source-download"
        ))
        .await;
    assert_json_error(deleted_response, StatusCode::NOT_FOUND, "NOT_FOUND").await;

    app.cleanup().await;
}

// ===========================================================================
// Group 1: List Workspaces — GET /api/v1/orgs/:orgId/workspaces
// ===========================================================================

/// Verifies that `GET /api/v1/orgs/:orgId/workspaces` returns the complete,
/// unpaginated list of workspaces with computed stats and per-branch details.
#[tokio::test]
async fn test_list_workspaces_returns_complete_list() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let client = app.authenticated_client(&user);

    insert_workspace(&app, org.id, "workspace-alpha", None).await;
    insert_workspace(&app, org.id, "workspace-beta", None).await;

    let response = client
        .get(&format!("/api/v1/orgs/{}/workspaces", org.id))
        .await;

    assert_eq!(response.status(), StatusCode::OK);

    let body: Value = response.json().await.expect("Failed to parse JSON");
    let data = body["data"].as_array().expect("data should be an array");
    assert!(data.len() >= 2, "Should have at least 2 workspaces");

    // Verify workspace list item shape
    let ws = &data[0];
    assert!(ws["id"].is_string(), "id should be a string UUID");
    assert!(ws["name"].is_string(), "name should be a string");
    assert!(
        ws.get("status").is_none(),
        "retired workspace status must be absent"
    );
    assert!(
        ws["ownerUserId"].is_string(),
        "current owner must be explicit on list items"
    );
    assert!(ws["branches"].is_array(), "branches should be an array");
    assert!(
        ws["tasksCompleted"].is_number(),
        "tasksCompleted should be a number"
    );
    assert!(
        ws["totalTasks"].is_number(),
        "totalTasks should be a number"
    );
    assert!(ws["cost"].is_number(), "cost should be a number");
    assert!(
        ws["concurrentTasks"].is_number(),
        "concurrentTasks should be a number"
    );
    assert!(
        ws["lastUpdated"].is_string(),
        "lastUpdated should be a string"
    );
    assert!(ws["createdAt"].is_string(), "createdAt should be a string");

    // The endpoint is unpaginated by design: `meta` must be absent entirely,
    // not merely empty. Its presence would mean the paginated envelope — and
    // with it the truncation defect of issue #1232 — had returned.
    assert!(
        body.get("meta").is_none(),
        "meta must be absent from the unpaginated list response, got: {:?}",
        body.get("meta")
    );

    app.cleanup().await;
}

/// Regression test for issue #1232.
///
/// The endpoint used to be offset-paginated with `perPage` defaulting to 50 and
/// clamped to `[1, 100]`, so an org with more workspaces than that ceiling
/// silently returned a truncated list — no error, no indicator. Seed an org
/// past the old ceiling and assert the whole set comes back in one response,
/// both for a default request and for one still sending the retired params.
#[tokio::test]
async fn test_list_workspaces_returns_all_beyond_legacy_page_size() {
    const WORKSPACE_COUNT: usize = 105; // > the retired `perPage` clamp of 100

    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let client = app.authenticated_client(&user);

    let mut expected_ids = HashSet::new();
    for i in 0..WORKSPACE_COUNT {
        let id = insert_workspace(&app, org.id, &format!("ws-{i:03}"), None).await;
        expected_ids.insert(id.to_string());
    }

    // A default request — no query params — must return every workspace.
    let response = client
        .get(&format!("/api/v1/orgs/{}/workspaces", org.id))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("Failed to parse JSON");

    assert!(
        body.get("meta").is_none(),
        "meta must be absent from the unpaginated list response, got: {:?}",
        body.get("meta")
    );

    let data = body["data"].as_array().expect("data should be an array");
    assert_eq!(
        data.len(),
        WORKSPACE_COUNT,
        "expected all {WORKSPACE_COUNT} workspaces in one response, got {}",
        data.len()
    );

    // Set-equality, not just a count: a truncated page plus duplicates could
    // otherwise satisfy a length check.
    let returned_ids: HashSet<String> = data
        .iter()
        .map(|ws| {
            ws["id"]
                .as_str()
                .expect("id should be a string")
                .to_string()
        })
        .collect();
    assert_eq!(
        returned_ids, expected_ids,
        "returned workspace ids must be exactly the seeded set"
    );

    // A stale caller still sending the retired params must not be able to
    // induce truncation — the params are simply ignored.
    let legacy = client
        .get(&format!(
            "/api/v1/orgs/{}/workspaces?page=2&perPage=50",
            org.id
        ))
        .await;
    assert_eq!(legacy.status(), StatusCode::OK);
    let legacy_body: Value = legacy.json().await.expect("Failed to parse JSON");
    let legacy_ids: HashSet<String> = legacy_body["data"]
        .as_array()
        .expect("data should be an array")
        .iter()
        .map(|ws| {
            ws["id"]
                .as_str()
                .expect("id should be a string")
                .to_string()
        })
        .collect();
    assert_eq!(
        legacy_ids, expected_ids,
        "retired page/perPage params must not truncate the response"
    );

    app.cleanup().await;
}

#[tokio::test]
async fn test_list_workspaces_hides_personal_workspaces_from_other_members() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let outsider = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let client = app.authenticated_client(&outsider);

    let visible_workspace = TestWorkspace::create(&app, &org).await;
    let hidden_workspace = TestWorkspace::create_personal(&app, &org, owner.id).await;
    sqlx::query("UPDATE workspaces SET created_by = $2 WHERE id = $1")
        .bind(hidden_workspace.id)
        .bind(outsider.id)
        .execute(&app.db)
        .await
        .expect("mark outsider as historical creator");

    let response = client
        .get(&format!("/api/v1/orgs/{}/workspaces", org.id))
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("Failed to parse JSON");
    let data = body["data"].as_array().expect("data should be an array");
    let workspace_ids: Vec<&str> = data.iter().filter_map(|ws| ws["id"].as_str()).collect();
    let visible_workspace_id = visible_workspace.id.to_string();
    let hidden_workspace_id = hidden_workspace.id.to_string();

    assert!(
        workspace_ids.contains(&visible_workspace_id.as_str()),
        "org-visible workspace should remain visible"
    );
    assert!(
        !workspace_ids.contains(&hidden_workspace_id.as_str()),
        "historical creator identity must not reveal a personal workspace"
    );

    app.cleanup().await;
}

#[tokio::test]
async fn test_get_personal_workspace_requires_owner_or_admin() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let outsider = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let client = app.authenticated_client(&outsider);

    let workspace = TestWorkspace::create_personal(&app, &org, owner.id).await;

    let response = client
        .get(&format!("/api/v1/workspaces/{}", workspace.id))
        .await;

    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    app.cleanup().await;
}

#[tokio::test]
async fn test_removed_workspace_members_route_is_not_found() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let outsider = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let client = app.authenticated_client(&outsider);

    let workspace = TestWorkspace::create_personal(&app, &org, owner.id).await;

    let response = client
        .get(&format!("/api/v1/workspaces/{}/members", workspace.id))
        .await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    app.cleanup().await;
}

#[tokio::test]
async fn test_merge_clone_workspace_advances_parent_branch_head() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let repo_id = create_test_repo(&app, &org).await;

    let repo_path = app.init_fixture_repo(repo_id).await;
    run_git(&repo_path, &["init"]);
    run_git(&repo_path, &["config", "user.email", "g5@test.local"]);
    run_git(&repo_path, &["config", "user.name", "G5 Test"]);
    run_git(&repo_path, &["checkout", "-b", "main"]);
    std::fs::write(repo_path.join("README.md"), "base\n").expect("failed to write base file");
    run_git(&repo_path, &["add", "README.md"]);
    run_git(&repo_path, &["commit", "-m", "base"]);
    let base_sha = git_stdout(&repo_path, &["rev-parse", "HEAD"]);

    let clone_branch_name = "main-clone-merge";
    run_git(&repo_path, &["checkout", "-b", clone_branch_name]);
    std::fs::write(repo_path.join("README.md"), "base\nclone change\n")
        .expect("failed to write clone file");
    run_git(&repo_path, &["commit", "-am", "clone change"]);
    let clone_sha = git_stdout(&repo_path, &["rev-parse", "HEAD"]);
    run_git(&repo_path, &["checkout", "main"]);

    let parent_workspace_id = insert_workspace(&app, org.id, "main", None).await;
    let clone_workspace_id =
        insert_workspace(&app, org.id, clone_branch_name, Some(parent_workspace_id)).await;

    let parent_branch_id =
        insert_workspace_repo(&app, org.id, parent_workspace_id, repo_id, &base_sha).await;
    insert_workspace_repo(&app, org.id, clone_workspace_id, repo_id, &clone_sha).await;

    // Hold the destination commit fence so the request waits at its authority
    // landing. Every landing takes the fence before the workspace row, so the
    // waiting merge holds no destination row lock; it rechecks ownership after
    // acquiring the fence (see
    // `transfer_during_clone_merge_preparation_refuses_the_old_owners_landing`).
    let mut fence = app.db.begin().await.expect("begin merge fence transaction");
    sqlx::query(
        "SELECT pg_advisory_xact_lock(\
             hashtextextended('g5:workspace-commit:' || $1::text, 0)\
         )",
    )
    .bind(parent_workspace_id)
    .execute(&mut *fence)
    .await
    .expect("hold destination workspace commit fence");

    let request_client = app.client.clone();
    let request_url = format!(
        "{}/api/v1/workspaces/{clone_workspace_id}/merge",
        app.base_url
    );
    let request_token = user.jwt.clone();
    let merge = tokio::spawn(async move {
        request_client
            .post(request_url)
            .bearer_auth(request_token)
            .json(&json!({}))
            .send()
            .await
            .expect("send merge request")
    });
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let waiting: bool = sqlx::query_scalar(
                "SELECT EXISTS(
                     SELECT 1 FROM pg_stat_activity
                     WHERE datname = current_database()
                       AND wait_event_type = 'Lock'
                       AND wait_event = 'advisory')",
            )
            .fetch_one(&app.db)
            .await
            .expect("observe merge fence waiter");
            if waiting {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("merge must reach the destination commit fence");

    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        sqlx::query("UPDATE workspaces SET updated_at = now() WHERE id = $1")
            .bind(parent_workspace_id)
            .execute(&app.db),
    )
    .await
    .expect("a merge waiting at the fence holds no destination row lock")
    .expect("update the destination row");

    fence.commit().await.expect("release merge commit fence");
    let response = merge.await.expect("join merge request");
    assert_eq!(response.status(), StatusCode::OK);

    let merged_parent_head = member_head(&app, parent_branch_id).await;
    assert_eq!(merged_parent_head, clone_sha);

    let repo = GitRepo::open(&repo_path).expect("failed to open git repo");
    let main_head = repo
        .resolve_branch_head("main")
        .expect("failed to resolve main branch");
    assert_eq!(main_head, clone_sha);

    app.cleanup().await;
}

#[tokio::test]
async fn transfer_during_clone_merge_preparation_refuses_the_old_owners_landing() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let successor = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let repo_id = create_test_repo(&app, &org).await;
    let repo_path = app.init_fixture_repo(repo_id).await;
    run_git(&repo_path, &["init"]);
    run_git(&repo_path, &["config", "user.email", "g5@test.local"]);
    run_git(&repo_path, &["config", "user.name", "G5 Test"]);
    run_git(&repo_path, &["checkout", "-b", "main"]);
    std::fs::write(repo_path.join("README.md"), "base\n").expect("write base file");
    run_git(&repo_path, &["add", "README.md"]);
    run_git(&repo_path, &["commit", "-m", "base"]);
    let base_sha = git_stdout(&repo_path, &["rev-parse", "HEAD"]);
    let clone_branch_name = "main-clone-transfer-race";
    run_git(&repo_path, &["checkout", "-b", clone_branch_name]);
    std::fs::write(repo_path.join("README.md"), "base\nclone change\n").expect("write clone file");
    run_git(&repo_path, &["commit", "-am", "clone change"]);
    let clone_sha = git_stdout(&repo_path, &["rev-parse", "HEAD"]);
    run_git(&repo_path, &["checkout", "main"]);

    let parent_id = insert_workspace(&app, org.id, "main", None).await;
    let clone_id = insert_workspace(&app, org.id, clone_branch_name, Some(parent_id)).await;
    insert_workspace_repo(&app, org.id, parent_id, repo_id, &base_sha).await;
    insert_workspace_repo(&app, org.id, clone_id, repo_id, &clone_sha).await;
    sqlx::query("INSERT INTO workspace_vms(org_id, workspace_id, status) VALUES ($1,$2,'stopped')")
        .bind(org.id)
        .bind(parent_id)
        .execute(&app.db)
        .await
        .expect("seed stopped parent incarnation for transfer");

    let barrier =
        g5_api::services::workspace_service::install_clone_merge_preparation_barrier(parent_id);
    let merge_client = app.client.clone();
    let merge_url = format!("{}/api/v1/workspaces/{clone_id}/merge", app.base_url);
    let merge_token = owner.jwt.clone();
    let merge = tokio::spawn(async move {
        merge_client
            .post(merge_url)
            .bearer_auth(merge_token)
            .json(&json!({}))
            .send()
            .await
            .expect("send clone merge")
    });
    tokio::time::timeout(
        std::time::Duration::from_secs(10),
        barrier.wait_until_prepared(),
    )
    .await
    .expect("merge reaches the post-Git, pre-authorization boundary");
    assert_eq!(
        git_stdout(&repo_path, &["rev-parse", "refs/heads/main"]),
        base_sha
    );

    let transfer = app
        .authenticated_client(&owner)
        .post_json(
            &format!("/api/v1/workspaces/{parent_id}/transfer"),
            &json!({
                "toUserId": successor.id,
                "idempotencyKey": format!("merge-preparation-transfer-{}", Uuid::new_v4())
            }),
        )
        .await;
    let transfer_status = transfer.status();
    let transfer_body: Value = transfer.json().await.expect("parse transfer response");
    assert_eq!(transfer_status, StatusCode::OK, "{transfer_body}");
    assert_eq!(transfer_body["data"]["status"], "completed");
    barrier.release();

    let merge_response = merge.await.expect("join clone merge");
    let merge_status = merge_response.status();
    let merge_body: Value = merge_response.json().await.expect("parse refused merge");
    assert_eq!(merge_status, StatusCode::FORBIDDEN, "{merge_body}");
    assert_eq!(
        git_stdout(&repo_path, &["rev-parse", "refs/heads/main"]),
        base_sha
    );

    assert_eq!(
        sqlx::query_scalar::<_, Uuid>("SELECT owner_user_id FROM workspaces WHERE id = $1")
            .bind(parent_id)
            .fetch_one(&app.db)
            .await
            .expect("load new parent owner"),
        successor.id
    );
    app.cleanup().await;
}

#[tokio::test]
async fn merge_clone_rejects_a_destination_owned_by_another_member() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let source_owner = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let destination_owner = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let destination_id = insert_workspace(&app, org.id, "merge-destination", None).await;
    let source_id = insert_workspace(&app, org.id, "merge-source", Some(destination_id)).await;
    sqlx::query("UPDATE workspaces SET owner_user_id = $2 WHERE id = $1")
        .bind(destination_id)
        .bind(destination_owner.id)
        .execute(&app.db)
        .await
        .expect("assign distinct destination owner");

    let response = app
        .authenticated_client(&source_owner)
        .post_json(&format!("/api/v1/workspaces/{source_id}/merge"), &json!({}))
        .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    app.cleanup().await;
}

#[tokio::test]
async fn test_sync_clone_workspace_does_not_merge_parent_and_enqueues_remote_cycle() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let client = app.authenticated_client(&user);
    let repo_id = create_test_repo(&app, &org).await;

    let repo_path = app.init_fixture_repo(repo_id).await;
    run_git(&repo_path, &["init"]);
    run_git(&repo_path, &["config", "user.email", "g5@test.local"]);
    run_git(&repo_path, &["config", "user.name", "G5 Test"]);
    run_git(&repo_path, &["checkout", "-b", "main"]);
    std::fs::write(repo_path.join("README.md"), "base\n").expect("failed to write base file");
    run_git(&repo_path, &["add", "README.md"]);
    run_git(&repo_path, &["commit", "-m", "base"]);
    let base_sha = git_stdout(&repo_path, &["rev-parse", "HEAD"]);

    std::fs::write(repo_path.join("README.md"), "base\nparent update\n")
        .expect("failed to write updated file");
    run_git(&repo_path, &["commit", "-am", "parent update"]);
    let advanced_sha = git_stdout(&repo_path, &["rev-parse", "HEAD"]);

    let clone_branch_name = "main-clone-sync";
    let parent_workspace_id = insert_workspace(&app, org.id, "main", None).await;
    let clone_workspace_id =
        insert_workspace(&app, org.id, clone_branch_name, Some(parent_workspace_id)).await;
    run_git(
        &repo_path,
        &[
            "update-ref",
            &format!("refs/heads/{clone_branch_name}"),
            &base_sha,
        ],
    );

    insert_workspace_repo(&app, org.id, parent_workspace_id, repo_id, &advanced_sha).await;
    let clone_branch_id =
        insert_workspace_repo(&app, org.id, clone_workspace_id, repo_id, &base_sha).await;
    sqlx::query("INSERT INTO remote_integrations(org_id,workspace_id,repo_id,remote_branch,auto_pull_enabled) VALUES ($1,$2,$3,'main',true)")
        .bind(org.id).bind(clone_workspace_id).bind(repo_id).execute(&app.db).await.unwrap();
    g5_scm::GitRepo::open(&repo_path)
        .unwrap()
        .write_baseline_ref(clone_branch_name, &base_sha)
        .unwrap();

    let response = client
        .post_json(
            &format!("/api/v1/workspaces/{clone_workspace_id}/sync"),
            &json!({}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);

    let body: Value = response
        .json()
        .await
        .expect("sync response should be valid JSON");
    assert_eq!(body["data"]["branchesUpdated"], 0);
    assert_eq!(body["data"]["branchesSkipped"], 0);
    assert_eq!(body["data"]["details"][0]["status"], "queued");

    let queued_trigger: String = sqlx::query_scalar(
        r#"
        SELECT trigger_kind::text
          FROM remote_integration_queue
         WHERE workspace_id = $1 AND repo_id = $2
        "#,
    )
    .bind(clone_workspace_id)
    .bind(repo_id)
    .fetch_one(&app.db)
    .await
    .expect("sync should enqueue a remote-tracking cycle for the clone");
    assert_eq!(queued_trigger, "manual");

    let clone_head = member_head(&app, clone_branch_id).await;
    assert_eq!(clone_head, base_sha);
    assert_ne!(clone_head, advanced_sha);

    app.cleanup().await;
}

#[tokio::test]
async fn test_sync_fetches_captured_repo_url_despite_stale_mirror_origin() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let mirror = app.repo_path(workspace.repo_id).await;
    let remotes = tempfile::tempdir().unwrap();
    let mut tips = Vec::new();
    for name in ["stale", "authoritative"] {
        let path = remotes.path().join(name);
        run_git(
            remotes.path(),
            &[
                "clone",
                "--branch",
                "main",
                mirror.to_str().unwrap(),
                path.to_str().unwrap(),
            ],
        );
        run_git(&path, &["config", "user.email", "g5@test.local"]);
        run_git(&path, &["config", "user.name", "G5 Test"]);
        std::fs::write(path.join(format!("{name}.txt")), name).unwrap();
        run_git(&path, &["add", "."]);
        run_git(&path, &["commit", "-m", name]);
        tips.push(git_stdout(&path, &["rev-parse", "HEAD"]));
    }
    let stale_url = remotes.path().join("stale");
    let authoritative_url = remotes.path().join("authoritative");
    run_git(
        &mirror,
        &["remote", "add", "origin", stale_url.to_str().unwrap()],
    );
    sqlx::query("UPDATE repos SET git_url = $2 WHERE id = $1")
        .bind(workspace.repo_id)
        .bind(authoritative_url.to_str().unwrap())
        .execute(&app.db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO remote_integrations(org_id,workspace_id,repo_id,remote_branch,auto_pull_enabled) VALUES ($1,$2,$3,'main',true)")
        .bind(org.id).bind(workspace.id).bind(workspace.repo_id).execute(&app.db).await.unwrap();
    let git = GitRepo::open(&mirror).unwrap();
    git.write_baseline_ref(&workspace.name, &workspace.head_commit)
        .unwrap();

    // A retry must stay on the captured repository destination too.
    for expected_status in ["imported", "noop"] {
        let response = app
            .authenticated_client(&user)
            .post_json(
                &format!("/api/v1/workspaces/{}/sync", workspace.id),
                &json!({}),
            )
            .await;
        let status = response.status();
        let body: Value = response.json().await.unwrap();
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["data"]["details"][0]["status"], "queued", "{body}");
        let cancel = tokio_util::sync::CancellationToken::new();
        let worker = tokio::spawn(
            g5_api::maintenance::branch_integration_worker::run_branch_integration_worker(
                g5_api::maintenance::branch_integration_worker::BranchIntegrationWorkerDeps {
                    landed_commit_notifier: std::sync::Arc::new(
                        g5_api::services::review_source_notifications::ReviewSourceNotifier::from_state(
                            &app.state,
                        ),
                    ),
                    system_pool: app.system_pool.clone(),
                    tenant_pool: app.state.tenant_pool.clone(),
                    authz: app.state.authz.clone(),
                    repo_storage: app.repo_storage.clone(),
                    repository_credentials: app.state.repository_credentials.clone(),
                    event_bus: app.state.event_bus.clone(),
                },
                cancel.clone(),
            ),
        );
        let drained = tokio::time::timeout(std::time::Duration::from_secs(45), async {
            loop {
                let pending: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM remote_integration_queue WHERE workspace_id=$1 AND repo_id=$2)")
                    .bind(workspace.id).bind(workspace.repo_id).fetch_one(&app.db).await.unwrap();
                if !pending { break; }
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        }).await;
        cancel.cancel();
        worker.await.unwrap();
        drained.expect("sync queue must drain");
        let outcome: String = sqlx::query_scalar("SELECT status FROM remote_sync_runs WHERE workspace_id=$1 AND repo_id=$2 ORDER BY started_at DESC LIMIT 1")
            .bind(workspace.id).bind(workspace.repo_id).fetch_one(&app.db).await.unwrap();
        assert_eq!(outcome, expected_status);
        let published = git.read_branch_oid(&workspace.name).unwrap().unwrap();
        assert_eq!(
            published, tips[1],
            "sync must publish the authoritative remote, never stale origin"
        );
        assert_ne!(published, tips[0]);
        assert_eq!(
            git_stdout(&mirror, &["remote", "get-url", "origin"]),
            stale_url.to_str().unwrap(),
            "fetch must not race other operations by mutating shared remote configuration"
        );
    }
    app.cleanup().await;
}

#[tokio::test]
async fn test_sync_non_clone_workspace_without_branches_returns_empty_result() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let client = app.authenticated_client(&user);
    let workspace_id = insert_workspace(&app, org.id, "sync-empty", None).await;

    let response = client
        .post_json(
            &format!("/api/v1/workspaces/{workspace_id}/sync"),
            &json!({}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);

    let body: serde_json::Value = response
        .json()
        .await
        .expect("sync response should be valid JSON");
    let data = &body["data"];
    assert_eq!(data["branchesUpdated"], 0);
    assert_eq!(data["branchesSkipped"], 0);
    assert_eq!(
        data["details"]
            .as_array()
            .expect("details should be an array")
            .len(),
        0
    );

    app.cleanup().await;
}

#[tokio::test]
async fn test_sync_workspace_does_not_duplicate_repo_level_skips() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let client = app.authenticated_client(&user);
    let workspace_id = insert_workspace(&app, org.id, "sync-skip", None).await;
    for _ in 0..2 {
        let repo_id = create_test_repo(&app, &org).await;
        let head = seed_materialized_repo(&app, repo_id, "local-only repo\n").await;
        insert_workspace_repo(&app, org.id, workspace_id, repo_id, &head).await;
    }

    let response = client
        .post_json(
            &format!("/api/v1/workspaces/{workspace_id}/sync"),
            &json!({}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);

    let body: Value = response
        .json()
        .await
        .expect("sync response should be valid JSON");
    let details = body["data"]["details"]
        .as_array()
        .expect("details should be an array");
    assert_eq!(details.len(), 2);
    assert!(details
        .iter()
        .all(|detail| { detail["status"].as_str() == Some("skipped") }));

    app.cleanup().await;
}

/// Verifies that `GET /api/v1/orgs/:orgId/workspaces` without a JWT returns 401.
#[tokio::test]
async fn test_list_workspaces_without_auth_returns_401() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;

    let response = app
        .client
        .get(format!(
            "{}/api/v1/orgs/{}/workspaces",
            app.base_url, org.id
        ))
        .send()
        .await
        .expect("Failed to send request");

    assert_json_error(response, StatusCode::UNAUTHORIZED, "UNAUTHORIZED").await;

    app.cleanup().await;
}

// ===========================================================================
// Group 3: Get Workspace — GET /api/v1/workspaces/:id
// ===========================================================================

/// Verifies that `GET /api/v1/workspaces/:id` returns the full workspace
/// detail with branches and dependencies.
#[tokio::test]
async fn test_get_workspace_returns_detail() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let client = app.authenticated_client(&user);

    let ws_id = insert_workspace(&app, org.id, "detail-test", None).await;
    sqlx::query("UPDATE workspaces SET description = 'Test description' WHERE id = $1")
        .bind(ws_id)
        .execute(&app.db)
        .await
        .expect("set fixture workspace description");

    // Get workspace detail
    let response = client.get(&format!("/api/v1/workspaces/{ws_id}")).await;

    assert_eq!(response.status(), StatusCode::OK);

    let body: Value = response.json().await.expect("Failed to parse JSON");
    let data = &body["data"];

    assert_eq!(data["id"].as_str().unwrap(), ws_id.to_string());
    assert_eq!(data["name"].as_str().unwrap(), "detail-test");
    assert_eq!(data["description"].as_str().unwrap(), "Test description");
    assert_eq!(data["orgId"].as_str().unwrap(), org.id.to_string());
    assert!(data["branches"].is_array());
    assert!(data["createdAt"].is_string());
    assert!(data["updatedAt"].is_string());

    app.cleanup().await;
}

/// Verifies workspace list/detail responses expose only durable workspace
/// members and carry the workspace docs binding explicitly. Disposable
/// transport rows must never become duplicate roots in the Files tree.
#[tokio::test]
async fn test_workspace_detail_and_list_hide_transient_transport_branches() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    let workspace_id = insert_workspace(&app, org.id, "canonical-branches", None).await;
    let code_repo_id = create_test_repo(&app, &org).await;
    let docs_repo_id = create_test_repo(&app, &org).await;
    let workspace_id_string = workspace_id.to_string();
    let docs_repo_id_string = docs_repo_id.to_string();
    let code_head = "1111111111111111111111111111111111111111";
    let docs_head = "2222222222222222222222222222222222222222";
    insert_workspace_repo(&app, org.id, workspace_id, code_repo_id, code_head).await;
    insert_workspace_repo(&app, org.id, workspace_id, docs_repo_id, docs_head).await;
    let code_repo_id_string = code_repo_id.to_string();
    sqlx::query("UPDATE workspaces SET docs_repo_id=$1 WHERE id=$2")
        .bind(docs_repo_id)
        .bind(workspace_id)
        .execute(&app.db)
        .await
        .unwrap();
    helpers::seed_branch_ref(
        &app,
        docs_repo_id,
        "remote-tracking-base-materialized-docs",
        docs_head,
    )
    .await;
    helpers::seed_branch_ref(
        &app,
        code_repo_id,
        "remote-tracking-base-materialized-code",
        code_head,
    )
    .await;

    let detail_response = client
        .get(&format!("/api/v1/workspaces/{workspace_id}"))
        .await;
    assert_eq!(detail_response.status(), StatusCode::OK);

    let detail_body: Value = detail_response
        .json()
        .await
        .expect("failed to parse workspace detail JSON");
    let detail = &detail_body["data"];
    assert_eq!(
        detail["docsRepoId"].as_str(),
        Some(docs_repo_id_string.as_str())
    );

    let branches = detail["branches"]
        .as_array()
        .expect("detail branches should be an array");
    let mut branch_ids: Vec<String> = branches
        .iter()
        .map(|branch| {
            branch["repoId"]
                .as_str()
                .expect("repository id")
                .to_string()
        })
        .collect();
    branch_ids.sort();
    let mut expected_branch_ids = vec![code_repo_id_string.clone(), docs_repo_id_string.clone()];
    expected_branch_ids.sort();
    assert_eq!(
        branch_ids, expected_branch_ids,
        "workspace detail should expose only materialized workspace-authority branches"
    );
    assert!(
        branches
            .iter()
            .all(|branch| branch.get("materializationStatus").is_none()),
        "canonical branch summaries should not expose materialization status"
    );

    let list_response = client
        .get(&format!("/api/v1/orgs/{}/workspaces", org.id))
        .await;
    assert_eq!(list_response.status(), StatusCode::OK);

    let list_body: Value = list_response
        .json()
        .await
        .expect("failed to parse workspace list JSON");
    let list_data = list_body["data"].as_array().expect("list data array");
    let listed_workspace = list_data
        .iter()
        .find(|workspace| workspace["id"].as_str() == Some(workspace_id_string.as_str()))
        .expect("created workspace should be listed");
    let listed_branches = listed_workspace["branches"]
        .as_array()
        .expect("list item branches should be an array");
    let mut listed_branch_ids: Vec<String> = listed_branches
        .iter()
        .map(|branch| {
            branch["repoId"]
                .as_str()
                .expect("repository id")
                .to_string()
        })
        .collect();
    listed_branch_ids.sort();
    assert_eq!(
        listed_branch_ids, expected_branch_ids,
        "workspace list should expose the same canonical executable branches"
    );

    app.cleanup().await;
}

/// Verifies that `GET /api/v1/workspaces/:id` with a non-existent UUID returns 404.
#[tokio::test]
async fn test_get_workspace_nonexistent_returns_404() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let client = app.authenticated_client(&user);

    let fake_id = Uuid::new_v4();
    let response = client.get(&format!("/api/v1/workspaces/{}", fake_id)).await;

    assert_json_error(response, StatusCode::NOT_FOUND, "NOT_FOUND").await;

    app.cleanup().await;
}

/// Verifies that a known workspace in an org the caller does not belong to is
/// indistinguishable from an absent workspace.
#[tokio::test]
async fn test_get_cross_org_workspace_matches_nonexistent_404() {
    let app = TestApp::spawn().await;
    let caller_org = TestOrg::create(&app).await;
    let caller = TestUser::create_with_scopes(&app, caller_org.id, &[Scope::WorkspacesRead]).await;
    let client = app.authenticated_client(&caller);

    let foreign_org = TestOrg::create(&app).await;
    let foreign_workspace = TestWorkspace::create(&app, &foreign_org).await;

    let known_foreign = client
        .get(&format!("/api/v1/workspaces/{}", foreign_workspace.id))
        .await;
    assert_json_error(known_foreign, StatusCode::NOT_FOUND, "NOT_FOUND").await;

    let absent = client
        .get(&format!("/api/v1/workspaces/{}", Uuid::new_v4()))
        .await;
    assert_json_error(absent, StatusCode::NOT_FOUND, "NOT_FOUND").await;

    app.cleanup().await;
}

/// Verifies that workspace-scoped routes authorize against the workspace's org,
/// not only the JWT's currently selected org.
#[tokio::test]
async fn test_get_workspace_in_non_selected_member_org_returns_detail() {
    let app = TestApp::spawn().await;
    let primary_org = TestOrg::create(&app).await;
    let secondary_org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(
        &app,
        primary_org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;

    sqlx::query!(
        r#"
        INSERT INTO org_memberships (org_id, user_id, role)
        VALUES ($1, $2, $3)
        ON CONFLICT (org_id, user_id) DO NOTHING
        "#,
        secondary_org.id,
        user.id,
        "member"
    )
    .execute(&app.db)
    .await
    .expect("Failed to insert secondary org membership");

    let client = app.authenticated_client(&user);
    let workspace_id = insert_workspace(&app, secondary_org.id, "secondary-org-ws", None).await;

    let response = client
        .get(&format!("/api/v1/workspaces/{workspace_id}"))
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    assert_eq!(
        body["data"]["id"].as_str().unwrap(),
        workspace_id.to_string()
    );
    assert_eq!(body["data"]["name"].as_str().unwrap(), "secondary-org-ws");

    app.cleanup().await;
}

// ===========================================================================
// Group 5: Delete Workspace — POST /api/v1/workspaces/:id/delete
// ===========================================================================

/// Verifies that the lifecycle delete action tombstones the workspace, hides
/// it from user-facing reads, and keeps its name reserved.
#[tokio::test]
async fn test_delete_workspace_tombstones_and_reserves_name() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let client = app.authenticated_client(&user);

    let ws_uuid = insert_workspace(&app, org.id, "to-delete", None).await;
    let ws_id = ws_uuid.to_string();

    let retired_response = client
        .delete(&format!("/api/v1/workspaces/{}", ws_id))
        .await;
    assert_eq!(retired_response.status(), StatusCode::METHOD_NOT_ALLOWED);

    let response = client
        .post_json(&format!("/api/v1/workspaces/{}/delete", ws_id), &json!({}))
        .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let tombstone = sqlx::query(
        "SELECT name, deleted_at IS NOT NULL AS deleted, deleted_by FROM workspaces WHERE id = $1",
    )
    .bind(ws_uuid)
    .fetch_one(&app.db)
    .await
    .expect("Failed to query workspace tombstone");
    let name: String = tombstone.get("name");
    let deleted: bool = tombstone.get("deleted");
    let deleted_by: Option<Uuid> = tombstone.get("deleted_by");
    assert_eq!(name, "to-delete");
    assert!(deleted, "Workspace row should be tombstoned");
    assert_eq!(deleted_by, Some(user.id));

    let list_response = client
        .get(&format!("/api/v1/orgs/{}/workspaces", org.id))
        .await;
    assert_eq!(list_response.status(), StatusCode::OK);
    let list_body = list_response
        .json::<Value>()
        .await
        .expect("Failed to parse list JSON");
    let listed_ids: Vec<&str> = list_body["data"]
        .as_array()
        .expect("data should be an array")
        .iter()
        .filter_map(|ws| ws["id"].as_str())
        .collect();
    assert!(
        !listed_ids.contains(&ws_id.as_str()),
        "Tombstoned workspace should be absent from list"
    );

    let response = client.get(&format!("/api/v1/workspaces/{}", ws_id)).await;
    assert_json_error(response, StatusCode::NOT_FOUND, "NOT_FOUND").await;

    let response = client
        .get(&format!("/api/v1/workspaces/{}/stats", ws_id))
        .await;
    assert_json_error(response, StatusCode::NOT_FOUND, "NOT_FOUND").await;

    app.cleanup().await;
}

/// The lifecycle delete boundary owns the exact workspace branch, immutable
/// flush namespace, receipt rows, repository mapping, and stopped VM state.
/// It must remove that mutable state while preserving unrelated refs and the
/// workspace audit tombstone.
#[tokio::test]
async fn test_delete_workspace_removes_branch_flush_receipts_and_runtime_rows() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let branch_name = workspace.name.clone();

    let repo_path = app.repo_path(workspace.repo_id).await;
    run_git(
        &repo_path,
        &[
            "update-ref",
            &format!("refs/heads/{branch_name}"),
            &workspace.head_commit,
        ],
    );
    let flush_id = Uuid::new_v4();
    let flush_prefix = format!("refs/g5/flush/{}/{}", workspace.id, flush_id);
    run_git(
        &repo_path,
        &[
            "update-ref",
            &format!("{flush_prefix}/branch"),
            &workspace.head_commit,
        ],
    );
    run_git(
        &repo_path,
        &[
            "update-ref",
            &format!("{flush_prefix}/snapshot"),
            &workspace.head_commit,
        ],
    );

    let vm_id = Uuid::new_v4();
    let receipt = json!([{
        "repo_id": workspace.repo_id,
        "branch_oid": workspace.head_commit,
        "local_branch_oid": workspace.head_commit,
        "local_head_oid": workspace.head_commit,
        "head_ref": branch_name,
        "sequencer": "none",
        "snapshot_oid": workspace.head_commit,
        "dirty": false,
        "dirty_paths": []
    }]);
    sqlx::query(
        "INSERT INTO workspace_flushes(
             flush_id, org_id, workspace_id, origin, vm_id, epoch, reason,
             required_repo_ids, repos, complete_at, request_key)
         VALUES ($1,$2,$3,'flush',$4,1,'shutdown',$5,$6,now(),'delete-test')",
    )
    .bind(flush_id)
    .bind(org.id)
    .bind(workspace.id)
    .bind(vm_id.to_string())
    .bind(vec![workspace.repo_id])
    .bind(receipt)
    .execute(&app.db)
    .await
    .expect("seed completed flush receipt");
    let loss_at = chrono::Utc::now();
    sqlx::query(
        "INSERT INTO workspace_vms(
             org_id, workspace_id, status, last_flush_id,
             unflushed_loss_at, last_error)
         VALUES ($1,$2,'stopped',$3,$4,$5)",
    )
    .bind(org.id)
    .bind(workspace.id)
    .bind(flush_id)
    .bind(loss_at)
    .bind(json!({"reason":"deadline_unflushed_loss","detail":"forced test loss"}))
    .execute(&app.db)
    .await
    .expect("seed stopped workspace VM state");

    let operation_id = Uuid::new_v4();
    let response = app
        .authenticated_client(&user)
        .post_json(
            &format!("/api/v1/workspaces/{}/delete", workspace.id),
            &json!({"operationId": operation_id, "force": true}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let mutable_rows: (i64, i64, i64) = sqlx::query_as(
        "SELECT
             (SELECT count(*) FROM workspace_repos WHERE workspace_id = $1),
             (SELECT count(*) FROM workspace_flushes WHERE workspace_id = $1),
             (SELECT count(*) FROM workspace_vms WHERE workspace_id = $1)",
    )
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .expect("query deleted workspace mutable state");
    assert_eq!(mutable_rows, (0, 0, 0));
    let (deleted_at, recorded_operation, deletion_loss): (
        Option<chrono::DateTime<chrono::Utc>>,
        Option<Uuid>,
        Option<Value>,
    ) = sqlx::query_as(
        "SELECT deleted_at, deletion_operation_id, deletion_loss
         FROM workspaces WHERE id = $1",
    )
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .expect("query workspace audit tombstone");
    assert!(deleted_at.is_some());
    assert_eq!(recorded_operation, Some(operation_id));
    assert_eq!(
        deletion_loss
            .as_ref()
            .and_then(|loss| loss["lastError"]["reason"].as_str()),
        Some("deadline_unflushed_loss")
    );
    assert!(deletion_loss
        .as_ref()
        .and_then(|loss| loss["unflushedLossAt"].as_str())
        .is_some());

    assert!(!Command::new("git")
        .args(["show-ref", "--verify", &format!("refs/heads/{branch_name}")])
        .current_dir(&repo_path)
        .output()
        .expect("inspect deleted workspace branch")
        .status
        .success());
    for suffix in ["branch", "snapshot"] {
        assert!(!Command::new("git")
            .args(["show-ref", "--verify", &format!("{flush_prefix}/{suffix}")])
            .current_dir(&repo_path)
            .output()
            .expect("inspect deleted flush ref")
            .status
            .success());
    }
    assert_eq!(
        git_stdout(&repo_path, &["rev-parse", "refs/heads/main"]),
        workspace.head_commit
    );

    app.cleanup().await;
}

/// Git ref deletion and database cleanup cannot share one atomic transaction.
/// A retry must therefore finish a deletion whose Git mutation succeeded but
/// whose database transaction rolled back before receipt cleanup committed.
#[tokio::test]
async fn test_delete_workspace_recovers_after_post_git_database_failure() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let branch_name = workspace.name.clone();
    let repo_path = app.repo_path(workspace.repo_id).await;
    run_git(
        &repo_path,
        &[
            "update-ref",
            &format!("refs/heads/{branch_name}"),
            &workspace.head_commit,
        ],
    );

    let flush_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO workspace_flushes(
             flush_id, org_id, workspace_id, origin, vm_id, epoch, reason,
             required_repo_ids, repos, complete_at, request_key)
         VALUES ($1,$2,$3,'flush',$4,1,'shutdown',$5,$6,now(),'delete-retry-test')",
    )
    .bind(flush_id)
    .bind(org.id)
    .bind(workspace.id)
    .bind(Uuid::new_v4().to_string())
    .bind(vec![workspace.repo_id])
    .bind(json!([{
        "repo_id": workspace.repo_id,
        "branch_oid": workspace.head_commit,
        "local_branch_oid": workspace.head_commit,
        "local_head_oid": workspace.head_commit,
        "head_ref": branch_name,
        "sequencer": "none",
        "snapshot_oid": workspace.head_commit,
        "dirty": false,
        "dirty_paths": []
    }]))
    .execute(&app.db)
    .await
    .expect("seed retry receipt");
    sqlx::query(
        "INSERT INTO workspace_vms(org_id, workspace_id, status, last_flush_id)
         VALUES ($1,$2,'stopped',$3)",
    )
    .bind(org.id)
    .bind(workspace.id)
    .bind(flush_id)
    .execute(&app.db)
    .await
    .expect("seed retry VM state");
    sqlx::query(
        "CREATE FUNCTION test_fail_workspace_flush_delete() RETURNS trigger
         LANGUAGE plpgsql AS $$
         BEGIN
             RAISE EXCEPTION 'injected receipt cleanup failure';
         END
         $$",
    )
    .execute(&app.db)
    .await
    .expect("create deletion failure function");
    sqlx::query(
        "CREATE TRIGGER test_fail_workspace_flush_delete
         BEFORE DELETE ON workspace_flushes
         FOR EACH STATEMENT EXECUTE FUNCTION test_fail_workspace_flush_delete()",
    )
    .execute(&app.db)
    .await
    .expect("create deletion failure trigger");

    let operation_id = Uuid::new_v4();
    let client = app.authenticated_client(&user);
    let failed = client
        .post_json(
            &format!("/api/v1/workspaces/{}/delete", workspace.id),
            &json!({"operationId": operation_id}),
        )
        .await;
    assert_json_error(failed, StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR").await;
    let retained: (Option<chrono::DateTime<chrono::Utc>>, i64) = sqlx::query_as(
        "SELECT deleted_at,
                (SELECT count(*) FROM workspace_flushes WHERE workspace_id = $1)
         FROM workspaces WHERE id = $1",
    )
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .expect("query rolled-back workspace deletion");
    assert_eq!(retained, (None, 1));
    assert!(!Command::new("git")
        .args(["show-ref", "--verify", &format!("refs/heads/{branch_name}")])
        .current_dir(&repo_path)
        .output()
        .expect("inspect first deletion Git effect")
        .status
        .success());

    sqlx::query("DROP TRIGGER test_fail_workspace_flush_delete ON workspace_flushes")
        .execute(&app.db)
        .await
        .expect("remove deletion failure trigger");
    sqlx::query("DROP FUNCTION test_fail_workspace_flush_delete()")
        .execute(&app.db)
        .await
        .expect("remove deletion failure function");
    let retried = client
        .post_json(
            &format!("/api/v1/workspaces/{}/delete", workspace.id),
            &json!({"operationId": Uuid::new_v4()}),
        )
        .await;
    assert_eq!(retried.status(), StatusCode::NO_CONTENT);
    let retained_identity: Option<Uuid> =
        sqlx::query_scalar("SELECT deletion_operation_id FROM workspaces WHERE id=$1")
            .bind(workspace.id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(
        retained_identity,
        Some(operation_id),
        "a fresh client request joins the interrupted deletion"
    );

    let completed: (Option<chrono::DateTime<chrono::Utc>>, i64, i64) = sqlx::query_as(
        "SELECT deleted_at,
                (SELECT count(*) FROM workspace_flushes WHERE workspace_id = $1),
                (SELECT count(*) FROM workspace_repos WHERE workspace_id = $1)
         FROM workspaces WHERE id = $1",
    )
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .expect("query retried workspace deletion");
    assert!(completed.0.is_some());
    assert_eq!((completed.1, completed.2), (0, 0));

    app.cleanup().await;
}

/// Hosts delete a workspace's root when its VM is released, so Delete does
/// not wait for a host that still reports a kept root (as hosts from before
/// that change did). Historical SQL values are covered by the upgrade fixture.
#[tokio::test]
async fn test_delete_workspace_does_not_wait_for_a_reported_root() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let workspace = TestWorkspace::create(&app, &org).await;

    let host_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO worker_hosts(host_id, org_id, region, status, last_heartbeat)
         VALUES ($1,$2,'test','ready',now())",
    )
    .bind(host_id)
    .bind(org.id)
    .execute(&app.db)
    .await
    .unwrap();
    app.host_control_backend()
        .accept_hello(
            host_id,
            &g5_internal_api::host_ctl::HostHello {
                host_id,
                protocol_version: g5_internal_api::host_ctl::PROTOCOL_VERSION,
                capabilities: Vec::new(),
                slots_total: 1,
                slot_size: Some(g5_internal_api::host_ctl::VmSizeSpec {
                    cpus: 2,
                    mem_mib: 4096,
                    scratch_disk_mib: 10240,
                }),
                inventory: Vec::new(),
                retained_roots: vec![g5_internal_api::host_ctl::RetainedRoot {
                    workspace_id: workspace.id,
                    cache_generation: 7,
                    expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
                }],
                runtime: Some(g5_internal_api::host_ctl::HostRuntimeIdentity::bundled()),
            },
            &g5_api::ws::host_ctl::HostConnection::new(true),
        )
        .await
        .expect("accept the legacy root report without storing it");
    sqlx::query("INSERT INTO workspace_vms(org_id, workspace_id, status) VALUES ($1,$2,'stopped')")
        .bind(org.id)
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .expect("seed stopped VM");

    let client = app.authenticated_client(&user);
    let deleted = client
        .post_json(
            &format!("/api/v1/workspaces/{}/delete", workspace.id),
            &json!({"operationId": Uuid::new_v4()}),
        )
        .await;
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
    let deleted_at: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT deleted_at FROM workspaces WHERE id = $1")
            .bind(workspace.id)
            .fetch_one(&app.db)
            .await
            .expect("query workspace deletion");
    assert!(deleted_at.is_some());

    app.cleanup().await;
}

/// Verifies that deleting a live parent workspace is rejected instead of
/// promoting or re-rooting its children in the visible workspace tree.
#[tokio::test]
async fn test_delete_workspace_rejects_live_parent_workspace() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let client = app.authenticated_client(&user);

    let parent_id = insert_workspace(&app, org.id, "parent-workspace", None).await;
    let _child_id = insert_workspace(&app, org.id, "child-workspace", Some(parent_id)).await;

    let response = client
        .post_json(
            &format!("/api/v1/workspaces/{}/delete", parent_id),
            &json!({}),
        )
        .await;
    assert_json_error_message(
        response,
        StatusCode::CONFLICT,
        "CONFLICT",
        "Conflict: Workspace has child workspaces and cannot be removed",
    )
    .await;

    let deleted_at: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT deleted_at FROM workspaces WHERE id = $1")
            .bind(parent_id)
            .fetch_one(&app.db)
            .await
            .expect("Failed to query parent workspace tombstone");
    assert!(
        deleted_at.is_none(),
        "Parent workspace should remain live when removal is rejected"
    );

    app.cleanup().await;
}

/// Verifies that deletion is refused while an active merge workflow references
/// the workspace, so hidden work is not left running under a tombstone.
#[tokio::test]
async fn test_delete_workspace_rejects_active_merge_workflow() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let source = TestWorkspace::create(&app, &org).await;
    let target = TestWorkspace::create(&app, &org).await;
    let client = app.authenticated_client(&owner);

    sqlx::query(
        r#"
        INSERT INTO workspace_merge_runs (
            id, org_id, workspace_id, source_kind,
            source_workspace_id, target_workspace_id, source_branch_name,
            status, phase, source_heads, target_heads, pre_start_source_heads,
            started_at
        )
        VALUES (
            $1, $2, $3, 'workspace',
            $3, $4, 'main',
            'running', 'workspace_rebase',
            '[]'::jsonb, '[]'::jsonb, '[]'::jsonb,
            now()
        )
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(org.id)
    .bind(source.id)
    .bind(target.id)
    .execute(&app.db)
    .await
    .expect("seed active workspace merge");

    let response = client
        .post_json(
            &format!("/api/v1/workspaces/{}/delete", source.id),
            &json!({}),
        )
        .await;
    assert_json_error_message(
        response,
        StatusCode::CONFLICT,
        "CONFLICT",
        "Conflict: Workspace has active merge workflows and cannot be removed",
    )
    .await;

    let deleted_at: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT deleted_at FROM workspaces WHERE id = $1")
            .bind(source.id)
            .fetch_one(&app.db)
            .await
            .expect("query source workspace tombstone");
    assert!(
        deleted_at.is_none(),
        "workspace should remain live while an active merge references it"
    );

    app.cleanup().await;
}

/// Verifies that a non-owner direct API delete returns 403.
#[tokio::test]
async fn test_delete_workspace_rejects_non_creator_member() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let member = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;

    let workspace = TestWorkspace::create(&app, &org).await;
    let ws_id = workspace.id.to_string();

    let member_client = app.authenticated_client(&member);
    let response = member_client
        .post_json(&format!("/api/v1/workspaces/{}/delete", ws_id), &json!({}))
        .await;

    assert_json_error(response, StatusCode::FORBIDDEN, "FORBIDDEN").await;

    let count = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM workspaces WHERE id = $1",
        workspace.id
    )
    .fetch_one(&app.db)
    .await
    .expect("Failed to query database");
    assert_eq!(
        count.unwrap_or(0),
        1,
        "Unauthorized delete should preserve the workspace row"
    );

    app.cleanup().await;
}

/// Verifies that lifecycle deletion rejects a Base workspace before VM work.
#[tokio::test]
async fn test_delete_base_workspace_is_rejected() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let client = app.authenticated_client(&user);

    // Insert a base workspace directly (base workspaces are created during org setup)
    let base_ws_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO workspaces \
         (id, org_id, name, is_base, owner_user_id, created_by) \
         VALUES ($1, $2, 'Base', true, $3, $3)",
    )
    .bind(base_ws_id)
    .bind(org.id)
    .bind(user.id)
    .execute(&app.db)
    .await
    .expect("Failed to insert base workspace");

    // Try to delete the base workspace
    let response = client
        .post_json(
            &format!("/api/v1/workspaces/{}/delete", base_ws_id),
            &json!({}),
        )
        .await;

    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body: Value = response.json().await.expect("base delete error JSON");
    assert_eq!(body["error"]["code"].as_str().unwrap(), "WORKSPACE_IS_BASE");
    assert_eq!(
        body["error"]["message"].as_str().unwrap(),
        "Base workspaces do not have a runtime"
    );

    app.cleanup().await;
}

// ===========================================================================
// Group 6: Branch Management — GET /api/v1/workspaces/:id/branches
// ===========================================================================

/// Working-workspace membership is immutable. The legacy branch-removal route
/// must reject the request and preserve the authoritative member mapping.
#[tokio::test]
async fn test_remove_branch_endpoint_is_retired() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let client = app.authenticated_client(&user);
    let repo_id = create_test_repo(&app, &org).await;
    let head = seed_materialized_repo(&app, repo_id, "remove branch seed").await;
    let ws_id = insert_workspace(&app, org.id, "remove-branch-test", None).await;
    insert_workspace_repo(&app, org.id, ws_id, repo_id, &head).await;

    let response = client
        .delete(&format!(
            "/api/v1/workspaces/{}/branches/{}",
            ws_id, repo_id
        ))
        .await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let list_resp = client
        .get(&format!("/api/v1/workspaces/{}/branches", ws_id))
        .await;
    let list_body: Value = list_resp.json().await.unwrap();
    let branches = list_body["data"].as_array().unwrap();
    assert!(
        branches
            .iter()
            .any(|branch| branch["repoId"] == repo_id.to_string()),
        "retired member removal must preserve the branch"
    );

    app.cleanup().await;
}

// ===========================================================================
// ===========================================================================

// ===========================================================================
// Group 8: Clone Workspace — POST /api/v1/workspaces/:id/clone
// ===========================================================================

#[tokio::test]
async fn test_clone_base_workspace_inherits_docs_and_code_repos() {
    let app = TestApp::spawn().await;
    let user = TestUser::create(&app).await;
    let client = app.authenticated_client(&user);

    let create_response = helpers::platform::create_org_via_operator(
        &app,
        &user,
        json!({ "name": "Base Clone Inheritance" }),
    )
    .await;
    let create_status = create_response.status();
    let create_body: Value = create_response
        .json()
        .await
        .expect("parse organization creation response");
    assert_eq!(
        create_status,
        StatusCode::CREATED,
        "organization creation failed: {create_body}"
    );
    let base_workspace_id = create_body["data"]["baseWorkspace"]["id"]
        .as_str()
        .expect("Base workspace id should be present");
    let docs_repo_id = create_body["data"]["docsRepo"]["id"]
        .as_str()
        .expect("docs repo id should be present");
    let code_repo_id = create_body["data"]["codeRepo"]["id"]
        .as_str()
        .expect("code repo id should be present");

    let clone_response = client
        .post_json(
            &format!("/api/v1/workspaces/{base_workspace_id}/clone"),
            &json!({
                "name": "base-docs-and-code-clone",
                "idempotencyKey": "base-docs-and-code-clone-test"
            }),
        )
        .await;
    let clone_status = clone_response.status();
    let clone_body: Value = clone_response.json().await.expect("parse clone response");
    assert_eq!(
        clone_status,
        StatusCode::CREATED,
        "Base clone failed: {clone_body}"
    );
    let clone_data = &clone_body["data"];
    assert_eq!(clone_data["clonedBranches"].as_i64(), Some(2));
    let clone_repo_ids: std::collections::HashSet<&str> = clone_data["workspace"]["branches"]
        .as_array()
        .expect("clone workspace should return branches")
        .iter()
        .map(|branch| {
            branch["repoId"]
                .as_str()
                .expect("branch repo id should be present")
        })
        .collect();
    let expected_repo_ids: std::collections::HashSet<&str> =
        [docs_repo_id, code_repo_id].into_iter().collect();
    assert_eq!(
        clone_repo_ids, expected_repo_ids,
        "forking Base must preserve exactly its docs and initial code repos"
    );

    let clone_workspace_id = Uuid::parse_str(
        clone_data["workspace"]["id"]
            .as_str()
            .expect("clone workspace id should be present"),
    )
    .expect("clone workspace id should be a UUID");

    let replay = client
        .post_json(
            &format!("/api/v1/workspaces/{base_workspace_id}/clone"),
            &json!({
                "name": "base-docs-and-code-clone",
                "idempotencyKey": "base-docs-and-code-clone-test"
            }),
        )
        .await;
    let replay_status = replay.status();
    let replay_body: Value = replay.json().await.expect("parse replay response");
    assert_eq!(
        replay_status,
        StatusCode::CREATED,
        "an identical retry must return its completed fork: {replay_body}"
    );
    assert_eq!(
        replay_body["data"]["workspace"]["id"],
        clone_workspace_id.to_string(),
        "an identical retry must not create a second workspace"
    );

    let collision = client
        .post_json(
            &format!("/api/v1/workspaces/{base_workspace_id}/clone"),
            &json!({
                "name": "base-docs-and-code-clone",
                "idempotencyKey": "base-docs-and-code-clone-collision"
            }),
        )
        .await;
    assert_json_error(
        collision,
        StatusCode::UNPROCESSABLE_ENTITY,
        "VALIDATION_ERROR",
    )
    .await;

    let members: Vec<Uuid> =
        sqlx::query_scalar("SELECT repo_id FROM workspace_repos WHERE workspace_id=$1")
            .bind(clone_workspace_id)
            .fetch_all(&app.db)
            .await
            .unwrap();
    assert_eq!(members.len(), 2);
    for repo_id in members {
        let path = app.repo_path(repo_id).await;
        let child_head = git_stdout(&path, &["rev-parse", "refs/heads/base-docs-and-code-clone"]);
        assert!(
            !child_head.is_empty(),
            "fork materializes every member's Git ref"
        );
    }

    app.cleanup().await;
}

#[tokio::test]
async fn durable_fork_pins_the_git_ref_when_head_projections_lag() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let repo_path = app.repo_path(workspace.repo_id).await;
    run_git(&repo_path, &["config", "user.email", "g5@test.local"]);
    run_git(&repo_path, &["config", "user.name", "G5 Test"]);
    let tree = git_stdout(
        &repo_path,
        &["rev-parse", &format!("{}^{{tree}}", workspace.head_commit)],
    );
    let advanced = git_stdout(
        &repo_path,
        &[
            "commit-tree",
            &tree,
            "-p",
            &workspace.head_commit,
            "-m",
            "advance ref without projection",
        ],
    );
    run_git(
        &repo_path,
        &[
            "update-ref",
            &format!("refs/heads/{}", workspace.name),
            &advanced,
        ],
    );
    assert_ne!(advanced, workspace.head_commit);

    let child_name = format!("ref-authority-{}", Uuid::new_v4());
    let response = app
        .authenticated_client(&owner)
        .post_json(
            &format!("/api/v1/workspaces/{}/clone", workspace.id),
            &json!({
                "name": child_name,
                "idempotencyKey": format!("fork-ref-authority-{}", Uuid::new_v4()),
                "source": "durable"
            }),
        )
        .await;
    let status = response.status();
    let body: Value = response.json().await.expect("parse durable fork response");
    assert_eq!(status, StatusCode::CREATED, "durable fork failed: {body}");
    assert_eq!(
        body["data"]["workspace"]["branches"][0]["headCommit"],
        advanced
    );
    assert_eq!(
        git_stdout(
            &repo_path,
            &["rev-parse", &format!("refs/heads/{child_name}")],
        ),
        advanced
    );

    app.cleanup().await;
}

#[tokio::test]
async fn providerless_legacy_remote_still_requires_fork_collision_preflight() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    sqlx::query("UPDATE repos SET provider = NULL, git_url = 'http://127.0.0.1:1/github.com/unavailable.git' WHERE id = $1")
        .bind(workspace.repo_id).execute(&app.db).await.unwrap();
    let response = app
        .authenticated_client(&owner)
        .post_json(
            &format!("/api/v1/workspaces/{}/clone", workspace.id),
            &json!({"name": format!("legacy-remote-{}", Uuid::new_v4()), "idempotencyKey": Uuid::new_v4().to_string(), "source": "durable"}),
        )
        .await;
    assert_json_error(
        response,
        StatusCode::SERVICE_UNAVAILABLE,
        "SERVICE_UNAVAILABLE",
    )
    .await;
    app.cleanup().await;
}

#[tokio::test]
async fn identical_fork_retry_with_a_new_key_resumes_the_pending_operation() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    sqlx::query(
        "UPDATE repos
         SET provider = 'github', git_url = 'https://127.0.0.1:1/unavailable.git'
         WHERE id = $1",
    )
    .bind(workspace.repo_id)
    .execute(&app.db)
    .await
    .expect("make provider verification retryably unavailable");

    // Reach the unavailable provider after credential resolution, so this
    // exercises retryable Git verification rather than missing credentials.
    let encrypted = app
        .state
        .scm_token_manager
        .as_ref()
        .expect("test SCM token manager")
        .encrypt_token("fork-provider-test-token")
        .expect("encrypt test SCM token");
    sqlx::query(
        "INSERT INTO user_scm_connections
         (id, user_id, provider, provider_user_id, provider_username, access_token_enc, scopes)
         VALUES ($1, $2, 'github', 'fork-owner', 'fork-owner', $3, ARRAY['repo'])",
    )
    .bind(Uuid::new_v4())
    .bind(owner.id)
    .bind(encrypted)
    .execute(&app.db)
    .await
    .expect("seed provider verification credentials");

    let client = app.authenticated_client(&owner);
    let path = format!("/api/v1/workspaces/{}/clone", workspace.id);
    let child_name = format!("retryable-fork-{}", Uuid::new_v4());
    let first_key = format!("retryable-first-{}", Uuid::new_v4());
    let first = client
        .post_json(
            &path,
            &json!({
                "name": child_name,
                "idempotencyKey": first_key,
                "source": "durable"
            }),
        )
        .await;
    assert_json_error(
        first,
        StatusCode::SERVICE_UNAVAILABLE,
        "SERVICE_UNAVAILABLE",
    )
    .await;

    let operation_before: (Uuid, String, String) = sqlx::query_as(
        "SELECT id, idempotency_key, status
         FROM workspace_forks
         WHERE org_id = $1 AND proposed_name = $2",
    )
    .bind(org.id)
    .bind(&child_name)
    .fetch_one(&app.db)
    .await
    .expect("load retryable fork operation");
    assert_eq!(operation_before.1, first_key);
    assert_eq!(operation_before.2, "pending");

    let second = client
        .post_json(
            &path,
            &json!({
                "name": child_name,
                "idempotencyKey": format!("retryable-second-{}", Uuid::new_v4()),
                "source": "durable"
            }),
        )
        .await;
    assert_json_error(
        second,
        StatusCode::SERVICE_UNAVAILABLE,
        "SERVICE_UNAVAILABLE",
    )
    .await;

    let operations = sqlx::query_as::<_, (Uuid, String, String)>(
        "SELECT id, idempotency_key, status
         FROM workspace_forks
         WHERE org_id = $1 AND proposed_name = $2",
    )
    .bind(org.id)
    .bind(&child_name)
    .fetch_all(&app.db)
    .await
    .expect("reload retryable fork operations");
    assert_eq!(operations, vec![operation_before]);

    app.cleanup().await;
}

#[tokio::test]
async fn completed_fork_seen_after_repo_lock_keeps_the_published_child_and_refs() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let source = TestWorkspace::create(&app, &org).await;
    let name = format!("completed-lock-replay-{}", Uuid::new_v4());
    let key = format!("completed-lock-key-{}", Uuid::new_v4());
    let response = app
        .authenticated_client(&owner)
        .post_json(
            &format!("/api/v1/workspaces/{}/clone", source.id),
            &json!({"name": name, "idempotencyKey": key, "source": "durable"}),
        )
        .await;
    let status = response.status();
    let body: Value = response.json().await.expect("parse completed fork");
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let child_id = Uuid::parse_str(
        body["data"]["workspace"]["id"]
            .as_str()
            .expect("completed child UUID"),
    )
    .expect("valid child UUID");
    let repo_path = app.repo_path(source.repo_id).await;
    let published_head = git_stdout(&repo_path, &["rev-parse", &format!("refs/heads/{name}")]);

    // Make the already published fixture look pending to the first read. The
    // test-only probe must then observe a fresh completed status after it has
    // waited for the canonical repository row lock.
    sqlx::query("UPDATE workspace_forks SET status = 'pending', completed_at = NULL WHERE id = $1")
        .bind(child_id)
        .execute(&app.db)
        .await
        .expect("stage pending fork status for post-lock completion race");

    let mut fence = app.db.begin().await.expect("begin repo lock fence");
    sqlx::query("SELECT id FROM repos WHERE org_id = $1 AND id = $2 FOR UPDATE")
        .bind(org.id)
        .bind(source.repo_id)
        .execute(&mut *fence)
        .await
        .expect("hold fork repository row");
    let scoped_db = app.scoped_db_for_user_org(owner.id, org.id).await;
    let scope = scoped_db.reopener();
    scoped_db.rollback().await.expect("close fork replay scope");
    let fork_service = app.state.workspace_fork_service.scoped(scope);
    let owner_id = owner.id;
    let org_id = org.id;
    let replay_key = key.clone();
    let replay = tokio::spawn(async move {
        fork_service
            .probe_completed_member_refs_after_repo_lock(org_id, owner_id, &replay_key)
            .await
    });
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let waiting: bool = sqlx::query_scalar(
                "SELECT EXISTS(
                     SELECT 1 FROM pg_stat_activity
                     WHERE datname = current_database()
                       AND wait_event_type = 'Lock'
                       AND query LIKE '%SELECT id FROM repos%')",
            )
            .fetch_one(&app.db)
            .await
            .expect("observe completed fork repo-lock waiter");
            if waiting {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("completed fork replay reaches canonical repository lock");
    sqlx::query(
        "UPDATE workspace_forks SET status = 'completed', completed_at = now() WHERE id = $1",
    )
    .bind(child_id)
    .execute(&app.db)
    .await
    .expect("complete fork while replay waits for repository row");
    fence.commit().await.expect("release fork repository row");
    assert!(
        replay
            .await
            .expect("join completed fork replay")
            .expect("completed fork is already applied"),
        "completed operation must return AlreadyApplied after repo locks"
    );
    assert_eq!(
        git_stdout(&repo_path, &["rev-parse", &format!("refs/heads/{name}")]),
        published_head
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM workspaces WHERE org_id = $1 AND name = $2 AND deleted_at IS NULL",
        )
        .bind(org.id)
        .bind(&name)
        .fetch_one(&app.db)
        .await
        .expect("count published fork workspace"),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT status FROM workspace_forks WHERE id = $1")
            .bind(child_id)
            .fetch_one(&app.db)
            .await
            .expect("load completed fork status"),
        "completed"
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT status FROM workspace_vms WHERE workspace_id = $1")
            .bind(child_id)
            .fetch_one(&app.db)
            .await
            .expect("load published child lifecycle"),
        "stopped"
    );
    app.cleanup().await;
}

#[tokio::test]
async fn concurrent_case_variant_name_forks_have_one_operation_winner() {
    let app = TestApp::spawn().await;
    let user = TestUser::create(&app).await;
    let client = app.authenticated_client(&user);
    let create = helpers::platform::create_org_via_operator(
        &app,
        &user,
        json!({ "name": format!("Concurrent Fork {}", Uuid::new_v4()) }),
    )
    .await;
    assert_eq!(create.status(), StatusCode::CREATED);
    let created: Value = create.json().await.expect("parse created organization");
    let org_id =
        Uuid::parse_str(created["data"]["org"]["id"].as_str().unwrap()).expect("organization id");
    let source_id = Uuid::parse_str(created["data"]["baseWorkspace"]["id"].as_str().unwrap())
        .expect("base workspace id");
    let path = format!("/api/v1/workspaces/{source_id}/clone");
    let name = format!("concurrent-fork-{}", Uuid::new_v4());

    let first_body = json!({"name": name, "idempotencyKey": format!("fork-a-{}", Uuid::new_v4())});
    let second_body = json!({
        "name": name.to_ascii_uppercase(),
        "idempotencyKey": format!("fork-b-{}", Uuid::new_v4())
    });
    let first = client.post_json(&path, &first_body);
    let second = client.post_json(&path, &second_body);
    let (first, second) = tokio::join!(first, second);
    let statuses = [first.status(), second.status()];
    assert_eq!(
        statuses
            .iter()
            .filter(|status| **status == StatusCode::CREATED)
            .count(),
        1,
        "exactly one case-variant fork name may publish: {statuses:?}"
    );
    assert_eq!(
        statuses
            .iter()
            .filter(|status| **status == StatusCode::UNPROCESSABLE_ENTITY)
            .count(),
        1,
        "the losing fork must receive the generic name-unavailable result: {statuses:?}"
    );

    let workspace_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM workspaces
         WHERE org_id = $1 AND lower(name) = lower($2) AND deleted_at IS NULL",
    )
    .bind(org_id)
    .bind(&name)
    .fetch_one(&app.db)
    .await
    .expect("count fork workspaces");
    assert_eq!(workspace_count, 1);
    let operation_counts: (i64, i64) = sqlx::query_as(
        "SELECT count(*) FILTER (WHERE status = 'completed'),
                count(*) FILTER (WHERE status IN ('pending','aborting'))
         FROM workspace_forks
         WHERE org_id = $1 AND lower(proposed_name) = lower($2)",
    )
    .bind(org_id)
    .bind(&name)
    .fetch_one(&app.db)
    .await
    .expect("count fork operations");
    assert_eq!(operation_counts, (1, 0));

    app.cleanup().await;
}

#[tokio::test]
async fn concurrent_same_name_forks_from_disjoint_sources_publish_only_one_workspace() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let first_source = TestWorkspace::create(&app, &org).await;
    let second_source = TestWorkspace::create(&app, &org).await;
    assert_ne!(first_source.repo_id, second_source.repo_id);
    let client = app.authenticated_client(&owner);
    let name = format!("disjoint-fork-{}", Uuid::new_v4());

    let first_path = format!("/api/v1/workspaces/{}/clone", first_source.id);
    let first_body = json!({
        "name": name,
        "idempotencyKey": format!("disjoint-first-{}", Uuid::new_v4())
    });
    let second_path = format!("/api/v1/workspaces/{}/clone", second_source.id);
    let second_body = json!({
        "name": name.to_ascii_uppercase(),
        "idempotencyKey": format!("disjoint-second-{}", Uuid::new_v4())
    });

    let first = client.post_json(&first_path, &first_body);
    let second = client.post_json(&second_path, &second_body);
    let (first, second) = tokio::join!(first, second);
    let statuses = [first.status(), second.status()];
    assert_eq!(
        statuses
            .iter()
            .filter(|status| **status == StatusCode::CREATED)
            .count(),
        1,
        "exactly one disjoint-source fork may publish a case-insensitive workspace name: {statuses:?}"
    );
    assert_eq!(
        statuses
            .iter()
            .filter(|status| **status == StatusCode::UNPROCESSABLE_ENTITY)
            .count(),
        1,
        "the losing disjoint-source fork must report name unavailable: {statuses:?}"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM workspaces
             WHERE org_id = $1 AND lower(name) = lower($2) AND deleted_at IS NULL",
        )
        .bind(org.id)
        .bind(&name)
        .fetch_one(&app.db)
        .await
        .expect("count disjoint-source fork winners"),
        1
    );

    app.cleanup().await;
}

#[tokio::test]
async fn organization_member_can_fork_base_into_a_stopped_child_they_own() {
    let app = TestApp::spawn().await;
    let owner = TestUser::create(&app).await;
    let created = helpers::platform::create_org_via_operator(
        &app,
        &owner,
        json!({"name": format!("Reader Fork {}", Uuid::new_v4())}),
    )
    .await;
    assert_eq!(created.status(), StatusCode::CREATED);
    let created: Value = created.json().await.expect("parse created organization");
    let org_id = Uuid::parse_str(
        created["data"]["org"]["id"]
            .as_str()
            .expect("organization id"),
    )
    .expect("organization UUID");
    let base_id = Uuid::parse_str(
        created["data"]["baseWorkspace"]["id"]
            .as_str()
            .expect("base workspace id"),
    )
    .expect("base workspace UUID");
    let member = TestUser::create_with_scopes(
        &app,
        org_id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let child_name = format!("member-base-fork-{}", Uuid::new_v4());

    let response = app
        .authenticated_client(&member)
        .post_json(
            &format!("/api/v1/workspaces/{base_id}/clone"),
            &json!({
                "name": child_name,
                "idempotencyKey": format!("member-base-fork-{}", Uuid::new_v4())
            }),
        )
        .await;
    let status = response.status();
    let body: Value = response.json().await.expect("parse member fork response");
    assert_eq!(status, StatusCode::CREATED, "member fork failed: {body}");
    let child_id = Uuid::parse_str(
        body["data"]["workspace"]["id"]
            .as_str()
            .expect("child workspace id"),
    )
    .expect("child workspace UUID");
    let child: (Uuid, String) =
        sqlx::query_as("SELECT owner_user_id, name FROM workspaces WHERE id = $1")
            .bind(child_id)
            .fetch_one(&app.db)
            .await
            .expect("load member-owned fork");
    assert_eq!(child.0, member.id);
    assert_eq!(child.1, child_name);
    let repo_ids: Vec<Uuid> =
        sqlx::query_scalar("SELECT repo_id FROM workspace_repos WHERE workspace_id=$1")
            .bind(child_id)
            .fetch_all(&app.db)
            .await
            .unwrap();
    assert!(!repo_ids.is_empty());
    for repo_id in repo_ids {
        let path = app.repo_path(repo_id).await;
        assert!(!git_stdout(&path, &["rev-parse", &format!("refs/heads/{child_name}")]).is_empty());
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM workspace_vms
             WHERE workspace_id IN ($1, $2) AND status IN ('starting','running','stopping')",
        )
        .bind(base_id)
        .bind(child_id)
        .fetch_one(&app.db)
        .await
        .expect("check passive base fork VM state"),
        0
    );

    app.cleanup().await;
}

#[tokio::test]
async fn active_fork_name_claim_is_serialized_even_for_disjoint_repo_manifests() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let source = TestWorkspace::create(&app, &org).await;
    let other_source = TestWorkspace::create(&app, &org).await;
    let requester =
        sqlx::query_scalar::<_, Uuid>("SELECT owner_user_id FROM workspaces WHERE id = $1")
            .bind(source.id)
            .fetch_one(&app.db)
            .await
            .expect("load source owner");
    let proposed_name = format!("disjoint-race-{}", Uuid::new_v4());
    let mut first = app.db.begin().await.expect("begin first name claim");
    sqlx::query(
        "INSERT INTO workspace_forks(
             id, org_id, source_workspace_id, requested_by, owner_user_id,
             idempotency_key, request_digest, proposed_name, repo_manifest)
         VALUES ($1,$2,$3,$4,$4,$5,$6,$7,$8)",
    )
    .bind(Uuid::new_v4())
    .bind(org.id)
    .bind(source.id)
    .bind(requester)
    .bind("disjoint-first")
    .bind("a".repeat(64))
    .bind(&proposed_name)
    .bind(json!([{"repo_id":source.repo_id}]))
    .execute(&mut *first)
    .await
    .expect("insert first active name claim");
    let second_pool = app.db.clone();
    let second_name = proposed_name.to_ascii_uppercase();
    let second_source = other_source.id;
    let second_repo = other_source.repo_id;
    let second_org = org.id;
    let second = tokio::spawn(async move {
        sqlx::query(
            "INSERT INTO workspace_forks(
                 id, org_id, source_workspace_id, requested_by, owner_user_id,
                 idempotency_key, request_digest, proposed_name, repo_manifest)
             VALUES ($1,$2,$3,$4,$4,$5,$6,$7,$8)",
        )
        .bind(Uuid::new_v4())
        .bind(second_org)
        .bind(second_source)
        .bind(requester)
        .bind("disjoint-second")
        .bind("b".repeat(64))
        .bind(second_name)
        .bind(json!([{"repo_id":second_repo}]))
        .execute(&second_pool)
        .await
    });
    tokio::task::yield_now().await;
    first.commit().await.expect("commit winning name claim");
    let error = second
        .await
        .expect("join losing name claim")
        .expect_err("unique active-name claim must reject the loser");
    assert_eq!(
        error
            .as_database_error()
            .and_then(|error| error.code())
            .as_deref(),
        Some("23505")
    );

    app.cleanup().await;
}

#[tokio::test]
async fn fork_lock_waiter_sees_fresh_private_prefix_and_exact_name_claims() {
    let app = TestApp::spawn().await;
    let user = TestUser::create(&app).await;
    let create = helpers::platform::create_org_via_operator(
        &app,
        &user,
        json!({ "name": format!("Fork Snapshot {}", Uuid::new_v4()) }),
    )
    .await;
    assert_eq!(create.status(), StatusCode::CREATED);
    let created: Value = create.json().await.expect("parse created organization");
    let org_id =
        Uuid::parse_str(created["data"]["org"]["id"].as_str().unwrap()).expect("organization id");
    let source_id = Uuid::parse_str(created["data"]["baseWorkspace"]["id"].as_str().unwrap())
        .expect("base workspace id");
    let repo_ids = ["codeRepo", "docsRepo"]
        .map(|key| {
            Uuid::parse_str(created["data"][key]["id"].as_str().unwrap()).expect("repository id")
        })
        .to_vec();
    let hidden_owner = TestUser::create_with_scopes(
        &app,
        org_id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let hidden_workspace_id = Uuid::new_v4();
    let hidden_workspace_name = format!("hidden-private-{}", Uuid::new_v4());
    sqlx::query(
        "INSERT INTO workspaces(
             id, org_id, name, is_base, personal_user_id, owner_user_id, created_by)
         VALUES ($1, $2, $3, false, $4, $4, $4)",
    )
    .bind(hidden_workspace_id)
    .bind(org_id)
    .bind(&hidden_workspace_name)
    .bind(hidden_owner.id)
    .execute(&app.db)
    .await
    .expect("insert hidden private workspace");

    for exact_name in [false, true] {
        let mut lock = app.db.begin().await.expect("begin repository lock");
        sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM repos WHERE org_id = $1 AND id = ANY($2) ORDER BY id FOR UPDATE",
        )
        .bind(org_id)
        .bind(&repo_ids)
        .fetch_all(&mut *lock)
        .await
        .expect("lock fork repository rows");

        let requested_name = if exact_name {
            format!("fresh-private-exact-{}", Uuid::new_v4())
        } else {
            format!("fresh-private-{}/child", Uuid::new_v4())
        };
        let claimed_name = if exact_name {
            requested_name.clone()
        } else {
            requested_name
                .strip_suffix("/child")
                .expect("test name has child suffix")
                .to_string()
        };
        let request_client = app.client.clone();
        let request_url = format!("{}/api/v1/workspaces/{source_id}/clone", app.base_url);
        let request_token = user.jwt.clone();
        let request_name = requested_name.clone();
        let waiter = tokio::spawn(async move {
            request_client
                .post(request_url)
                .bearer_auth(request_token)
                .json(&json!({
                    "name": request_name,
                    "idempotencyKey": format!("fresh-snapshot-{}", Uuid::new_v4())
                }))
                .send()
                .await
                .expect("send blocked fork request")
        });

        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                let waiting: bool = sqlx::query_scalar(
                    "SELECT EXISTS(
                     SELECT 1 FROM pg_stat_activity
                     WHERE datname = current_database()
                       AND wait_event_type = 'Lock'
                       AND query LIKE '%SELECT id FROM repos%')",
                )
                .fetch_one(&app.db)
                .await
                .expect("observe fork lock waiter");
                if waiting {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("fork request should wait on canonical repository locks");

        let manifest = repo_ids
            .iter()
            .map(|repo_id| json!({"repo_id": repo_id}))
            .collect::<Vec<_>>();
        let hidden_operation_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO workspace_forks(
             id, org_id, source_workspace_id, requested_by, owner_user_id,
             idempotency_key, request_digest, proposed_name, repo_manifest)
         VALUES ($1, $2, $3, $4, $4, $5, $6, $7, $8)",
        )
        .bind(hidden_operation_id)
        .bind(org_id)
        .bind(hidden_workspace_id)
        .bind(hidden_owner.id)
        .bind(format!("hidden-claim-{}", Uuid::new_v4()))
        .bind("a".repeat(64))
        .bind(&claimed_name)
        .bind(json!(manifest))
        .execute(&app.db)
        .await
        .expect("commit hidden private name claim while waiter is blocked");
        lock.commit().await.expect("release repository locks");

        let response = waiter.await.expect("join fork waiter");
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body: Value = response
            .json()
            .await
            .expect("parse name-unavailable response");
        assert_eq!(body["error"]["code"], "VALIDATION_ERROR");
        let disclosed = body.to_string();
        assert!(!disclosed.contains(&hidden_workspace_name));
        assert!(!disclosed.contains(&hidden_owner.id.to_string()));
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM workspaces WHERE org_id = $1 AND name = $2"
            )
            .bind(org_id)
            .bind(&requested_name)
            .fetch_one(&app.db)
            .await
            .expect("count refused fork workspaces"),
            0
        );
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT status FROM workspace_forks WHERE id = $1")
                .bind(hidden_operation_id)
                .fetch_one(&app.db)
                .await
                .expect("load winning private name claim"),
            "pending",
            "refused fork must not clean up the other operation"
        );
    }

    app.cleanup().await;
}

#[tokio::test]
async fn fork_collision_cleanup_preserves_another_operations_branch_and_proof() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let source = TestWorkspace::create(&app, &org).await;
    let name = format!("foreign-fork-proof-{}", Uuid::new_v4());
    let key = format!("foreign-fork-collision-{}", Uuid::new_v4());
    let foreign_operation_id = Uuid::new_v4();
    let repo_path = app.repo_path(source.repo_id).await;
    GitRepo::open(&repo_path)
        .expect("open fork collision repository")
        .create_workspace_fork_refs(&name, foreign_operation_id, &source.head_commit)
        .expect("seed foreign fork branch and proof");

    let response = app
        .authenticated_client(&owner)
        .post_json(
            &format!("/api/v1/workspaces/{}/clone", source.id),
            &json!({"name": name, "idempotencyKey": key, "source": "durable"}),
        )
        .await;
    let status = response.status();
    let body: Value = response
        .json()
        .await
        .expect("parse fork collision response");
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["error"]["code"], "VALIDATION_ERROR");
    assert!(!body.to_string().contains(&foreign_operation_id.to_string()));
    let (refused_operation_id, refused_status): (Uuid, String) = sqlx::query_as(
        "SELECT id, status FROM workspace_forks WHERE org_id = $1 AND requested_by = $2 AND idempotency_key = $3",
    )
    .bind(org.id)
    .bind(owner.id)
    .bind(&key)
    .fetch_one(&app.db)
    .await
    .expect("admitted fork reaches abort cleanup after local ref collision");
    assert_ne!(refused_operation_id, foreign_operation_id);
    assert_eq!(refused_status, "aborted");
    assert_eq!(
        git_stdout(&repo_path, &["rev-parse", &format!("refs/heads/{name}")]),
        source.head_commit,
        "abort cleanup must preserve the foreign branch"
    );
    assert_eq!(
        git_stdout(
            &repo_path,
            &[
                "rev-parse",
                &format!("refs/g5/forks/{foreign_operation_id}")
            ]
        ),
        source.head_commit,
        "abort cleanup must preserve the foreign proof"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM workspaces WHERE org_id = $1 AND name = $2 AND deleted_at IS NULL"
        )
        .bind(org.id)
        .bind(&name)
        .fetch_one(&app.db)
        .await
        .expect("count refused fork workspaces"),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM workspace_vms WHERE workspace_id = $1")
            .bind(refused_operation_id)
            .fetch_one(&app.db)
            .await
            .expect("count refused fork VM rows"),
        0
    );
    app.cleanup().await;
}

#[tokio::test]
async fn test_clone_workspace_rejects_tombstoned_source() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let client = app.authenticated_client(&user);

    let source_id = insert_workspace(&app, org.id, "tombstoned-clone-source", None).await;
    sqlx::query("UPDATE workspaces SET deleted_at = now(), updated_at = now() WHERE id = $1")
        .bind(source_id)
        .execute(&app.db)
        .await
        .expect("tombstone source workspace");

    let response = client
        .post_json(
            &format!("/api/v1/workspaces/{source_id}/clone"),
            &json!({
                "name": "clone-should-fail",
                "idempotencyKey": "tombstoned-source-clone-test"
            }),
        )
        .await;
    assert_json_error(response, StatusCode::NOT_FOUND, "NOT_FOUND").await;

    let clone_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM workspaces WHERE cloned_from_id = $1")
            .bind(source_id)
            .fetch_one(&app.db)
            .await
            .expect("count clone rows");
    assert_eq!(clone_count, 0, "tombstoned source must not create a clone");

    app.cleanup().await;
}

// ===========================================================================
// Group 9: Workspace Members — GET/POST/DELETE /api/v1/workspaces/:id/members
// ===========================================================================

/// Retired sharing operations never appear successful or change other metadata.
#[tokio::test]
async fn removed_sharing_requests_fail_without_changing_metadata() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let member = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let ws = TestWorkspace::create(&app, &org).await;
    for actor in [&owner, &member] {
        let client = app.authenticated_client(actor);
        let path = format!("/api/v1/workspaces/{}", ws.id);
        for response in [
            client.get(&format!("{path}/members")).await,
            client
                .post_json(
                    &format!("{path}/members"),
                    &json!({"userId": member.id, "role": "viewer"}),
                )
                .await,
            client
                .delete(&format!("{path}/members/{}", member.id))
                .await,
        ] {
            assert_eq!(response.status(), StatusCode::NOT_FOUND);
        }
        for visibility in [json!("org"), json!("invite_only"), Value::Null] {
            let response = client
                .patch_json(
                    &path,
                    &json!({"visibility": visibility, "description": "must not land"}),
                )
                .await;
            assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
        let response = client.get(&path).await;
        assert_eq!(response.status(), StatusCode::OK);
        let body: Value = response.json().await.unwrap();
        assert!(body["data"].get("visibility").is_none());
        assert_ne!(body["data"]["description"], "must not land");
    }
    let response = app
        .authenticated_client(&owner)
        .patch_json(
            &format!("/api/v1/workspaces/{}", ws.id),
            &json!({"description": "metadata still works", "phase": "se"}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["data"]["description"], "metadata still works");
    assert_eq!(body["data"]["phase"], "se");
    app.cleanup().await;
}

// ===========================================================================
// Issue #1995 — real behind/ahead in workspace-detail branch summaries
// (ontology: workspace_service D50; scenario: workspace_pull_requests_phase1 I13)
// ===========================================================================

/// Creates one commit object (plus a branch ref for debuggability) in the
/// non-bare seeded test repo without touching its checkout, so it composes
/// with `seed_materialized_repo` fixtures.
fn commit_file_on_ref(
    repo_path: &std::path::Path,
    ref_branch: &str,
    parent_head: &str,
    file_path: &str,
    contents: &str,
    message: &str,
) -> String {
    let repo = git2::Repository::open(repo_path).expect("seeded test repo should open");
    let signature =
        git2::Signature::now("G5 Test", "g5@test.local").expect("signature should be valid");
    let parent_oid = git2::Oid::from_str(parent_head).expect("parent head should be a git oid");
    let parent_commit = repo
        .find_commit(parent_oid)
        .expect("parent commit should exist");
    let parent_tree = parent_commit.tree().expect("parent tree should load");
    let mut tree_builder = repo
        .treebuilder(Some(&parent_tree))
        .expect("tree builder should load parent tree");
    let blob_oid = repo
        .blob(contents.as_bytes())
        .expect("file blob should be created");
    tree_builder
        .insert(file_path, blob_oid, 0o100644)
        .expect("file should be inserted into tree");
    let tree_oid = tree_builder.write().expect("tree should be written");
    let tree = repo.find_tree(tree_oid).expect("new tree should load");
    repo.commit(
        Some(&format!("refs/heads/{ref_branch}")),
        &signature,
        &signature,
        message,
        &tree,
        &[&parent_commit],
    )
    .expect("commit should be created")
    .to_string()
}

fn branch_summary_for_repo(body: &Value, repo_id: Uuid) -> &Value {
    body["data"]["branches"]
        .as_array()
        .expect("branches should be an array")
        .iter()
        .find(|branch| branch["repoId"].as_str() == Some(repo_id.to_string().as_str()))
        .expect("workspace detail should carry the repo's branch summary")
}

/// RED regression for issue #1995: the Prepare-to-Ship badge reads
/// `GET /api/v1/workspaces/:id` branch summaries, which historically hardcoded
/// `behindBase: 0` / `divergence: 0`, so the "Up to date with {target}" badge
/// could never report divergence. The detail path must compute real git
/// ancestry against the clone parent (merge-target-first per D50): a parent
/// advance is `behindBase > 0`, a clone advance is `divergence > 0`, and a
/// both-sides advance reports both axes independently.
#[tokio::test]
async fn get_workspace_reports_real_behind_base_and_divergence_against_clone_parent() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    // One shared repo whose object store carries a real history.
    let repo_id = create_test_repo(&app, &org).await;
    let base_head = seed_materialized_repo(&app, repo_id, "issue-1995 shared repo\n").await;
    let repo_path = app.repo_path(repo_id).await;

    // Parent (comparison target) and its clone, both at the seed commit.
    let parent_ws = insert_workspace(&app, org.id, "1995-parent", None).await;
    let parent_branch = insert_workspace_repo(&app, org.id, parent_ws, repo_id, &base_head).await;

    let clone_ws = insert_workspace(&app, org.id, "1995-clone", Some(parent_ws)).await;
    let clone_branch = insert_workspace_repo(&app, org.id, clone_ws, repo_id, &base_head).await;

    // Scenario 1 (the dev observation): the PARENT advances; the clone must
    // report one commit behind, zero ahead. Pre-fix stub reported 0/0.
    let parent_advance = commit_file_on_ref(
        &repo_path,
        "main",
        &base_head,
        "parent.txt",
        "parent advance\n",
        "parent advance",
    );
    set_workspace_branch_head(&app, parent_branch, &parent_advance).await;

    let response = client.get(&format!("/api/v1/workspaces/{clone_ws}")).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("detail should parse");
    let branch = branch_summary_for_repo(&body, repo_id);
    assert_eq!(
        branch["behindBase"].as_i64(),
        Some(1),
        "a clone whose parent advanced one commit must report behindBase == 1 \
         (hardcoded-stub regression, issue #1995); got {branch}"
    );
    assert_eq!(
        branch["divergence"].as_i64(),
        Some(0),
        "the clone itself did not advance, so divergence (ahead) must be a known 0; got {branch}"
    );

    // Scenario 1 companion: the CLONE advances too — both axes report
    // independently (ahead 1, behind 1), never collapsed into one boolean.
    let clone_advance = commit_file_on_ref(
        &repo_path,
        "clone-main",
        &base_head,
        "clone.txt",
        "clone advance\n",
        "clone advance",
    );
    set_workspace_branch_head(&app, clone_branch, &clone_advance).await;

    let response = client.get(&format!("/api/v1/workspaces/{clone_ws}")).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("detail should parse");
    let branch = branch_summary_for_repo(&body, repo_id);
    assert_eq!(
        branch["divergence"].as_i64(),
        Some(1),
        "clone advance is ahead-by-1"
    );
    assert_eq!(
        branch["behindBase"].as_i64(),
        Some(1),
        "parent advance stays behind-by-1"
    );

    // Companion pair: a second clone advances while ITS parent stays put —
    // divergence > 0 with a known behindBase == 0.
    let ahead_parent_ws = insert_workspace(&app, org.id, "1995-ahead-parent", None).await;
    insert_workspace_repo(&app, org.id, ahead_parent_ws, repo_id, &base_head).await;
    let ahead_clone_ws =
        insert_workspace(&app, org.id, "1995-ahead-clone", Some(ahead_parent_ws)).await;
    insert_workspace_repo(&app, org.id, ahead_clone_ws, repo_id, &clone_advance).await;

    let response = client
        .get(&format!("/api/v1/workspaces/{ahead_clone_ws}"))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("detail should parse");
    let branch = branch_summary_for_repo(&body, repo_id);
    assert_eq!(
        branch["divergence"].as_i64(),
        Some(1),
        "clone-side advance must report divergence == 1; got {branch}"
    );
    assert_eq!(
        branch["behindBase"].as_i64(),
        Some(0),
        "the parent did not move, so behindBase must be a known 0; got {branch}"
    );

    app.cleanup().await;
}

/// Issue #1995 nullable-contract half (D50): a surface that cannot compute
/// divergence reports `null`, never a fabricated `0`, and the endpoint still
/// returns 200. Covers a standalone workspace (no merge target, no clone
/// parent) and a clone whose parent lacks the repo's branch.
#[tokio::test]
async fn get_workspace_reports_null_divergence_when_no_comparison_workspace() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    let repo_id = create_test_repo(&app, &org).await;
    let base_head = seed_materialized_repo(&app, repo_id, "issue-1995 null case\n").await;

    // Standalone workspace: neither merge_target_workspace_id nor cloned_from_id.
    let standalone_ws = insert_workspace(&app, org.id, "1995-standalone", None).await;
    insert_workspace_repo(&app, org.id, standalone_ws, repo_id, &base_head).await;

    let response = client
        .get(&format!("/api/v1/workspaces/{standalone_ws}"))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("detail should parse");
    let branch = branch_summary_for_repo(&body, repo_id);
    assert!(
        branch["behindBase"].is_null(),
        "no comparison workspace: behindBase must serialize as JSON null, not a fake 0; got {branch}"
    );
    assert!(
        branch["divergence"].is_null(),
        "no comparison workspace: divergence must serialize as JSON null, not a fake 0; got {branch}"
    );

    // Clone whose parent has no branch for a second repo: that branch degrades
    // to null while the endpoint stays 200.
    let second_repo_id = create_test_repo(&app, &org).await;
    let second_head = seed_materialized_repo(&app, second_repo_id, "issue-1995 unpaired\n").await;
    let parent_ws = insert_workspace(&app, org.id, "1995-sparse-parent", None).await;
    insert_workspace_repo(&app, org.id, parent_ws, repo_id, &base_head).await;

    let clone_ws = insert_workspace(&app, org.id, "1995-sparse-clone", Some(parent_ws)).await;
    insert_workspace_repo(&app, org.id, clone_ws, repo_id, &base_head).await;
    insert_workspace_repo(&app, org.id, clone_ws, second_repo_id, &second_head).await;

    let response = client.get(&format!("/api/v1/workspaces/{clone_ws}")).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("detail should parse");
    let paired = branch_summary_for_repo(&body, repo_id);
    assert!(
        paired["behindBase"].is_null() && paired["divergence"].is_null(),
        "incomplete parent membership makes the entire comparison unavailable; got {paired}"
    );
    let unpaired = branch_summary_for_repo(&body, second_repo_id);
    assert!(
        unpaired["behindBase"].is_null() && unpaired["divergence"].is_null(),
        "a repo the parent lacks must degrade to null, not fail or fake 0; got {unpaired}"
    );

    app.cleanup().await;
}

/// Detail comparisons follow the parent independently of a different owner
/// holding the merge target, and remain stable after that target is deleted.
#[tokio::test]
async fn workspace_detail_compares_to_parent_regardless_of_merge_target_access() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let caller = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let target_owner = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let client = app.authenticated_client(&caller);

    let repo_id = create_test_repo(&app, &org).await;
    let base_head = seed_materialized_repo(&app, repo_id, "comparison authorization\n").await;
    let repo_path = app.repo_path(repo_id).await;
    let target_head = commit_file_on_ref(
        &repo_path,
        "other-target-main",
        &base_head,
        "target.txt",
        "other owner target advance\n",
        "other owner target advance",
    );

    let clone_parent = insert_workspace(&app, org.id, "accessible-clone-parent", None).await;
    insert_workspace_repo(&app, org.id, clone_parent, repo_id, &base_head).await;

    let merge_target = insert_workspace(&app, org.id, "other-merge-target", None).await;
    sqlx::query("UPDATE workspaces SET owner_user_id=$2 WHERE id=$1")
        .bind(merge_target)
        .bind(target_owner.id)
        .execute(&app.db)
        .await
        .unwrap();
    insert_workspace_repo(&app, org.id, merge_target, repo_id, &target_head).await;

    let source = insert_workspace(&app, org.id, "comparison-source", Some(clone_parent)).await;
    sqlx::query("UPDATE workspaces SET merge_target_workspace_id = $2 WHERE id = $1")
        .bind(source)
        .bind(merge_target)
        .execute(&app.db)
        .await
        .expect("failed to set explicit merge target");
    insert_workspace_repo(&app, org.id, source, repo_id, &base_head).await;

    let response = client.get(&format!("/api/v1/workspaces/{source}")).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("GET detail should parse");
    let branch = branch_summary_for_repo(&body, repo_id);
    assert!(
        branch["divergence"] == json!(0) && branch["behindBase"] == json!(0),
        "comparison uses the readable parent even when a merge target has another owner; got {branch}"
    );

    let response = client
        .patch_json(
            &format!("/api/v1/workspaces/{source}"),
            &json!({ "description": "authorized source update" }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("PATCH detail should parse");
    let branch = branch_summary_for_repo(&body, repo_id);
    assert!(
        branch["divergence"] == json!(0) && branch["behindBase"] == json!(0),
        "PATCH must apply the same comparison authorization as GET; got {branch}"
    );

    let response = client.get(&format!("/api/v1/workspaces/{source}")).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response
        .json()
        .await
        .expect("authorized detail should parse");
    let branch = branch_summary_for_repo(&body, repo_id);
    assert_eq!(branch["divergence"].as_i64(), Some(0));
    assert_eq!(
        branch["behindBase"].as_i64(),
        Some(0),
        "merge target membership does not change the parent comparison; got {branch}"
    );

    sqlx::query("UPDATE workspaces SET deleted_at = NOW(), updated_at = NOW() WHERE id = $1")
        .bind(merge_target)
        .execute(&app.db)
        .await
        .expect("failed to tombstone merge target");
    let response = client.get(&format!("/api/v1/workspaces/{source}")).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("fallback detail should parse");
    let branch = branch_summary_for_repo(&body, repo_id);
    assert_eq!(
        branch["divergence"].as_i64(),
        Some(0),
        "a tombstoned merge target does not change the parent comparison; got {branch}"
    );
    assert_eq!(branch["behindBase"].as_i64(), Some(0));

    app.cleanup().await;
}

/// Ordinary clone parents are readable across owners in the same organization.
#[tokio::test]
async fn workspace_detail_reads_other_owners_clone_parent_divergence() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let caller = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let parent_owner = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let client = app.authenticated_client(&caller);

    let repo_id = create_test_repo(&app, &org).await;
    let base_head = seed_materialized_repo(&app, repo_id, "other owner clone parent\n").await;
    let repo_path = app.repo_path(repo_id).await;
    let parent_head = commit_file_on_ref(
        &repo_path,
        "other-parent-main",
        &base_head,
        "parent.txt",
        "other owner parent advance\n",
        "other owner parent advance",
    );

    let parent = insert_workspace(&app, org.id, "other-clone-parent", None).await;
    sqlx::query("UPDATE workspaces SET owner_user_id=$2 WHERE id=$1")
        .bind(parent)
        .bind(parent_owner.id)
        .execute(&app.db)
        .await
        .unwrap();
    insert_workspace_repo(&app, org.id, parent, repo_id, &parent_head).await;

    let source = insert_workspace(&app, org.id, "other-parent-clone", Some(parent)).await;
    insert_workspace_repo(&app, org.id, source, repo_id, &base_head).await;

    let response = client.get(&format!("/api/v1/workspaces/{source}")).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("detail should parse");
    let branch = branch_summary_for_repo(&body, repo_id);
    assert!(
        branch["divergence"] == json!(0) && branch["behindBase"] == json!(1),
        "an ordinary parent remains readable across owners; got {branch}"
    );

    app.cleanup().await;
}

#[tokio::test]
async fn remote_binding_during_clone_merge_preparation_refuses_local_landing() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let repo_id = create_test_repo(&app, &org).await;
    let repo_path = app.init_fixture_repo(repo_id).await;
    run_git(&repo_path, &["init"]);
    run_git(&repo_path, &["config", "user.email", "g5@test.local"]);
    run_git(&repo_path, &["config", "user.name", "G5 Test"]);
    run_git(&repo_path, &["checkout", "-b", "main"]);
    std::fs::write(repo_path.join("README.md"), "base\n").expect("write base file");
    run_git(&repo_path, &["add", "README.md"]);
    run_git(&repo_path, &["commit", "-m", "base"]);
    let base_sha = git_stdout(&repo_path, &["rev-parse", "HEAD"]);
    let clone_branch_name = "main-clone-transfer-race";
    run_git(&repo_path, &["checkout", "-b", clone_branch_name]);
    std::fs::write(repo_path.join("README.md"), "base\nclone change\n").expect("write clone file");
    run_git(&repo_path, &["commit", "-am", "clone change"]);
    let clone_sha = git_stdout(&repo_path, &["rev-parse", "HEAD"]);
    run_git(&repo_path, &["checkout", "main"]);

    let parent_id = insert_workspace(&app, org.id, "main", None).await;
    let clone_id = insert_workspace(&app, org.id, clone_branch_name, Some(parent_id)).await;
    insert_workspace_repo(&app, org.id, parent_id, repo_id, &base_sha).await;
    insert_workspace_repo(&app, org.id, clone_id, repo_id, &clone_sha).await;
    let barrier =
        g5_api::services::workspace_service::install_clone_merge_preparation_barrier(parent_id);
    let merge_client = app.client.clone();
    let merge_url = format!("{}/api/v1/workspaces/{clone_id}/merge", app.base_url);
    let merge_token = owner.jwt.clone();
    let merge = tokio::spawn(async move {
        merge_client
            .post(merge_url)
            .bearer_auth(merge_token)
            .json(&json!({}))
            .send()
            .await
            .expect("send clone merge")
    });
    tokio::time::timeout(
        std::time::Duration::from_secs(10),
        barrier.wait_until_prepared(),
    )
    .await
    .expect("merge reaches the post-Git, pre-authorization boundary");
    assert_eq!(
        git_stdout(&repo_path, &["rev-parse", "refs/heads/main"]),
        base_sha
    );

    let mut binding_db = app.scoped_db_for_org(org.id).await;
    g5_api::services::branch_commit::acquire_workspace_commit_fence(binding_db.conn(), parent_id)
        .await
        .unwrap();
    g5_api::services::remote_integrations::bind_published(
        binding_db.conn(),
        WorkspaceRepoKey {
            org_id: org.id,
            workspace_id: parent_id,
            repo_id,
        },
        "main",
    )
    .await
    .unwrap();
    binding_db.commit().await.unwrap();
    barrier.release();

    let merge_response = merge.await.expect("join clone merge");
    let merge_status = merge_response.status();
    let merge_body: Value = merge_response.json().await.expect("parse refused merge");
    let landed_head = git_stdout(&repo_path, &["rev-parse", "refs/heads/main"]);
    app.cleanup().await;
    assert_eq!(merge_status, StatusCode::CONFLICT,
        "a remote binding added during preparation must require provider acceptance; target advanced={}, response={merge_body}", landed_head != base_sha);
    assert_eq!(landed_head, base_sha);
}

struct ConcurrentMergeFixture {
    org: Uuid,
    parent: Uuid,
    source: Uuid,
    owner: TestUser,
    // Repository order is also the fork's capture order.
    members: Vec<(Uuid, std::path::PathBuf, String, String)>,
}

async fn concurrent_merge_fixture(app: &TestApp) -> ConcurrentMergeFixture {
    let mut owner = TestUser::create(app).await;
    let org = TestOrg::create_with_admin(app, &mut owner).await;
    let parent = insert_workspace(app, org.id, "main", None).await;
    let source = insert_workspace(app, org.id, "merge-source", Some(parent)).await;
    let mut members = Vec::new();
    for name in ["code", "docs"] {
        let repo_id = create_named_test_repo(app, &org, name).await;
        sqlx::query("UPDATE repos SET provider=NULL, git_url=$2 WHERE id=$1")
            .bind(repo_id)
            .bind(format!("g5-local://{repo_id}"))
            .execute(&app.db)
            .await
            .unwrap();
        let old = seed_materialized_repo(app, repo_id, "before merge\n").await;
        let path = app.repo_path(repo_id).await;
        run_git(&path, &["checkout", "-b", "merge-source"]);
        std::fs::write(path.join("README.md"), format!("after merge: {name}\n")).unwrap();
        run_git(&path, &["commit", "-am", "source change"]);
        let new = git_stdout(&path, &["rev-parse", "HEAD"]);
        run_git(&path, &["checkout", "main"]);
        insert_workspace_repo(app, org.id, parent, repo_id, &old).await;
        insert_workspace_repo(app, org.id, source, repo_id, &new).await;
        if name == "docs" {
            sqlx::query("UPDATE workspaces SET docs_repo_id=$1 WHERE id=ANY($2)")
                .bind(repo_id)
                .bind(vec![parent, source])
                .execute(&app.db)
                .await
                .unwrap();
        }
        members.push((repo_id, path, old, new));
    }
    members.sort_by_key(|member| member.0);
    ConcurrentMergeFixture {
        org: org.id,
        parent,
        source,
        owner,
        members,
    }
}

async fn wait_for_workspace_settlement_waiter(app: &TestApp) {
    // This fixture has its own database and two concurrent operations. The
    // paused one holds settlement authority; the competing request may first
    // wait on an earlier SQL phase, whose statement text is not the contract.
    let waited = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let waiting: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database()
                 AND wait_event_type='Lock' AND cardinality(pg_blocking_pids(pid)) > 0)",
            )
            .fetch_one(&app.db)
            .await
            .unwrap();
            if waiting {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await;
    if waited.is_err() {
        type ConnectionActivity = (i32, Option<String>, Option<String>, String, Vec<i32>);
        let activity: Vec<ConnectionActivity> = sqlx::query_as(
            "SELECT pid, wait_event_type, wait_event, left(query, 500), pg_blocking_pids(pid)
             FROM pg_stat_activity WHERE datname=current_database() AND pid<>pg_backend_pid()",
        )
        .fetch_all(&app.db)
        .await
        .unwrap();
        panic!("the competing platform operation did not reach a settlement lock: {activity:?}");
    }
}

fn start_fixture_merge(
    app: &TestApp,
    fixture: &ConcurrentMergeFixture,
) -> tokio::task::JoinHandle<reqwest::Response> {
    let client = app.client.clone();
    let url = format!(
        "{}/api/v1/workspaces/{}/merge",
        app.base_url, fixture.source
    );
    let token = fixture.owner.jwt.clone();
    tokio::spawn(async move {
        client
            .post(url)
            .bearer_auth(token)
            .json(&json!({}))
            .send()
            .await
            .unwrap()
    })
}

fn start_fixture_fork(
    app: &TestApp,
    fixture: &ConcurrentMergeFixture,
    source: &str,
    name: &str,
) -> tokio::task::JoinHandle<reqwest::Response> {
    let client = app.client.clone();
    let url = format!(
        "{}/api/v1/workspaces/{}/clone",
        app.base_url, fixture.parent
    );
    let token = fixture.owner.jwt.clone();
    let body = json!({"name":name,"idempotencyKey":name,"source":source});
    tokio::spawn(async move {
        client
            .post(url)
            .bearer_auth(token)
            .json(&body)
            .send()
            .await
            .unwrap()
    })
}

async fn assert_concurrent_fork_cut(
    app: &TestApp,
    fixture: &ConcurrentMergeFixture,
    response: reqwest::Response,
    name: &str,
    after_merge: bool,
    parent_after_merge: bool,
) -> Uuid {
    let status = response.status();
    let body: Value = response.json().await.unwrap();
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let child: Uuid = body["data"]["workspace"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let (operation, manifest, state): (Uuid, Value, String) = sqlx::query_as(
        "SELECT id, repo_manifest, status FROM workspace_forks WHERE target_workspace_id=$1",
    )
    .bind(child)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(state, "completed");
    assert_eq!(manifest.as_array().unwrap().len(), 2);
    let rows: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT repo_id, role FROM workspace_repos WHERE workspace_id=$1 ORDER BY repo_id",
    )
    .bind(child)
    .fetch_all(&app.db)
    .await
    .unwrap();
    assert_eq!(
        rows.iter().map(|row| row.0).collect::<Vec<_>>(),
        fixture
            .members
            .iter()
            .map(|member| member.0)
            .collect::<Vec<_>>()
    );
    assert!(rows.iter().any(|row| row.1 == "code") && rows.iter().any(|row| row.1 == "docs"));
    for (repo_id, path, old, new) in &fixture.members {
        let expected = if after_merge { new } else { old };
        let member = manifest
            .as_array()
            .unwrap()
            .iter()
            .find(|member| member["repo_id"] == repo_id.to_string())
            .unwrap();
        assert_eq!(
            member["source_oid"], *expected,
            "fork must freeze a complete cut: {manifest}"
        );
        let repo = GitRepo::open(path).unwrap();
        assert_eq!(repo.read_branch_oid(name).unwrap().as_ref(), Some(expected));
        repo.verify_workspace_fork_refs(name, operation, expected)
            .unwrap();
        assert_eq!(
            repo.read_branch_oid("main").unwrap().as_ref(),
            Some(if parent_after_merge { new } else { old })
        );
    }
    child
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_fork_capture_and_merge_preserve_the_complete_pre_merge_cut() {
    use g5_api::services::git_test_barrier::GitTestBarrier;
    for source in ["branch", "durable"] {
        let app = TestApp::spawn().await;
        let fixture = concurrent_merge_fixture(&app).await;
        let name = format!("fork-before-{source}");
        let barrier = GitTestBarrier::after_member_read(fixture.members[0].0, "main");
        let fork = start_fixture_fork(&app, &fixture, source, &name);
        barrier.reached().await;
        // Prove the partial capture holds the real fence even if the competing
        // request first waits on the caller's SQL authority lock.
        let mut probe = app.db.begin().await.unwrap();
        assert!(
            !g5_api::services::branch_commit::try_acquire_workspace_commit_fence(
                &mut probe,
                fixture.parent,
            )
            .await
            .unwrap()
        );
        probe.rollback().await.unwrap();
        let mut merge = start_fixture_merge(&app, &fixture);
        // The partial capture owns the fence. The competing public request
        // must wait, either there or at its preceding SQL authority check.
        let merged = tokio::select! {
            result = &mut merge => Some(result.unwrap()),
            () = wait_for_workspace_settlement_waiter(&app) => None,
        };
        barrier.release();
        let merged = match merged {
            Some(response) => response,
            None => merge.await.unwrap(),
        };
        let status = merged.status();
        let body: Value = merged.json().await.unwrap();
        assert_eq!(status, StatusCode::OK, "{body}");
        let child =
            assert_concurrent_fork_cut(&app, &fixture, fork.await.unwrap(), &name, false, true)
                .await;
        let replay = start_fixture_fork(&app, &fixture, source, &name)
            .await
            .unwrap();
        assert_eq!(
            assert_concurrent_fork_cut(&app, &fixture, replay, &name, false, true).await,
            child
        );
        app.cleanup().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_fork_waits_for_merge_commit_and_captures_the_complete_new_cut() {
    use g5_api::services::workspace_service::install_refs_applied_landing_barrier;
    let app = TestApp::spawn().await;
    let fixture = concurrent_merge_fixture(&app).await;
    let barrier = install_refs_applied_landing_barrier(fixture.parent);
    let merge = start_fixture_merge(&app, &fixture);
    barrier.wait_until_prepared().await;
    let name = "fork-after-merge";
    let mut fork = start_fixture_fork(&app, &fixture, "durable", name);
    // Git refs have moved, but the actual merge still owns its SQL fence and
    // has not committed. Fork may not freeze that intermediate outcome.
    // The public route can wait first on its authority row lock (FOR SHARE),
    // which also conflicts with merge settlement, before reaching capture.
    tokio::select! {
        result = &mut fork => panic!("fork completed before merge settled: {:?}", result.unwrap().status()),
        () = wait_for_workspace_settlement_waiter(&app) => {},
    }
    let frozen: i64 =
        sqlx::query_scalar("SELECT count(*) FROM workspace_forks WHERE source_workspace_id=$1")
            .bind(fixture.parent)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(
        frozen, 0,
        "an in-progress merge must not become a frozen fork source"
    );
    barrier.release();
    assert_eq!(merge.await.unwrap().status(), StatusCode::OK);
    assert_concurrent_fork_cut(&app, &fixture, fork.await.unwrap(), name, true, true).await;
    app.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_fork_waits_for_rejected_landing_and_captures_unchanged_cut() {
    use g5_api::services::branch_commit::{prepare_branch_commit, FenceMode};
    use g5_contracts::types::{AuthorityTarget, WorkspaceHeadRepoInput};

    let app = TestApp::spawn().await;
    let fixture = concurrent_merge_fixture(&app).await;
    let scope = app.scoped_db_for_org(fixture.org).await;
    let reopener = scope.reopener();
    scope.rollback().await.unwrap();
    let prepared = prepare_branch_commit(
        &reopener,
        &app.repo_storage,
        AuthorityTarget::Workspace {
            workspace_id: fixture.parent,
        },
        fixture
            .members
            .iter()
            .map(|(repo_id, _, old, new)| WorkspaceHeadRepoInput {
                repo_id: *repo_id,
                expected_old_head: old.clone(),
                proposed_new_head: new.clone(),
            })
            .collect(),
        "refused_landing_capture_regression",
    )
    .await
    .unwrap();
    let (reached_tx, reached_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = tokio::sync::oneshot::channel();
    let db = reopener.open().await.unwrap();
    let state = app.state.clone();
    let storage = app.repo_storage.clone();
    let landing = tokio::spawn(async move {
        prepared.land_with(db, &state.event_bus, &storage, FenceMode::Blocking,
            move |db, _| Box::pin(async move {
                // The real deferred constraint must refuse before Git moves refs.
                sqlx::query("CREATE TEMP TABLE refused_landing (id int, UNIQUE(id) DEFERRABLE INITIALLY DEFERRED) ON COMMIT DROP")
                    .execute(db.conn()).await?;
                sqlx::query("INSERT INTO refused_landing VALUES (1),(1)").execute(db.conn()).await?;
                reached_tx.send(()).unwrap();
                let _ = release_rx.await;
                Ok(())
            }),
        ).await
    });
    tokio::time::timeout(std::time::Duration::from_secs(10), reached_rx)
        .await
        .unwrap()
        .unwrap();
    let name = "fork-after-refused-landing";
    let mut fork = start_fixture_fork(&app, &fixture, "branch", name);
    tokio::select! {
        response = &mut fork => panic!("fork completed before landing settled: {:?}", response.unwrap().status()),
        () = wait_for_workspace_settlement_waiter(&app) => {},
    }
    release_tx.send(()).unwrap();
    let error = landing.await.unwrap().unwrap_err();
    assert!(error.to_string().contains("duplicate key"), "{error}");
    assert_concurrent_fork_cut(&app, &fixture, fork.await.unwrap(), name, false, false).await;
    app.cleanup().await;
}

/// Readers that persist or act on a whole multi-repository cut read it under
/// the workspace commit fence (`g5.fork.source.cut`): while another holder
/// keeps the fence past their bound they refuse with the retryable busy error
/// instead of reading refs a landing may be moving.
#[tokio::test]
async fn manifest_and_source_download_read_one_committed_cut() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    // The runtime pool has two connections. Reserve one for a concurrent
    // request: each reader must release admission before its fenced capture,
    // both when the fence refuses it and when the read succeeds.
    assert_eq!(app.state.tenant_pool.options().get_max_connections(), 2);
    let reserved = app.state.tenant_pool.acquire().await.unwrap();
    for route in ["manifest", "source-download"] {
        let url = format!("/api/v1/workspaces/{}/{route}", workspace.id);
        let holder = helpers::hold_commit_fence(&app, workspace.id).await;
        let response = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            app.authenticated_client(&owner).get(&url),
        )
        .await
        .expect("fenced read must not wait for a second tenant connection");
        let status = response.status();
        let body = response.text().await.unwrap();
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{route}: {body}");
        assert!(
            body.contains("WORKSPACE_COMMIT_FENCE_BUSY"),
            "{route}: {body}"
        );
        holder.rollback().await.unwrap();
        let response = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            app.authenticated_client(&owner).get(&url),
        )
        .await
        .expect("successful read must not wait for a second tenant connection");
        assert_eq!(response.status(), StatusCode::OK, "{route} after the fence");
    }
    drop(reserved);
    app.cleanup().await;
}

/// The clone-merge applies the child's heads to the parent without checking
/// them against the child again, so it reads the child as one committed cut.
#[tokio::test]
async fn merge_clone_reads_the_child_as_one_committed_cut() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let repo_id = create_test_repo(&app, &org).await;
    let repo_path = app.init_fixture_repo(repo_id).await;
    run_git(&repo_path, &["init"]);
    run_git(&repo_path, &["config", "user.email", "g5@test.local"]);
    run_git(&repo_path, &["config", "user.name", "G5 Test"]);
    run_git(&repo_path, &["checkout", "-b", "main"]);
    std::fs::write(repo_path.join("README.md"), "base\n").unwrap();
    run_git(&repo_path, &["add", "README.md"]);
    run_git(&repo_path, &["commit", "-m", "base"]);
    let base_sha = git_stdout(&repo_path, &["rev-parse", "HEAD"]);
    let clone_branch_name = "main-clone-cut";
    run_git(&repo_path, &["checkout", "-b", clone_branch_name]);
    std::fs::write(repo_path.join("README.md"), "base\nclone change\n").unwrap();
    run_git(&repo_path, &["commit", "-am", "clone change"]);
    let clone_sha = git_stdout(&repo_path, &["rev-parse", "HEAD"]);
    run_git(&repo_path, &["checkout", "main"]);
    let parent_id = insert_workspace(&app, org.id, "main", None).await;
    let clone_id = insert_workspace(&app, org.id, clone_branch_name, Some(parent_id)).await;
    let parent = insert_workspace_repo(&app, org.id, parent_id, repo_id, &base_sha).await;
    insert_workspace_repo(&app, org.id, clone_id, repo_id, &clone_sha).await;
    sqlx::query("UPDATE workspaces SET owner_user_id = $2 WHERE id = ANY($1)")
        .bind(vec![parent_id, clone_id])
        .bind(user.id)
        .execute(&app.db)
        .await
        .unwrap();

    let url = format!("/api/v1/workspaces/{clone_id}/merge");
    let holder = helpers::hold_commit_fence(&app, clone_id).await;
    let response = app
        .authenticated_client(&user)
        .post_json(&url, &json!({}))
        .await;
    let status = response.status();
    let body = response.text().await.unwrap();
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(body.contains("WORKSPACE_COMMIT_FENCE_BUSY"), "{body}");
    assert_eq!(member_head(&app, parent).await, base_sha, "nothing merged");
    holder.rollback().await.unwrap();

    let response = app
        .authenticated_client(&user)
        .post_json(&url, &json!({}))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(member_head(&app, parent).await, clone_sha);
    app.cleanup().await;
}

/// G5Labs/G5-Lean#463: the direct-clean merge checks the target's owner before
/// the Git work in a transaction it rolls back, so a transfer of the target
/// during preparation must still be noticed at the landing, where the owner
/// is re-read under the landing's row locks.
#[tokio::test]
async fn transfer_during_direct_clean_merge_preparation_refuses_the_old_owners_landing() {
    let app = TestApp::spawn().await;
    let fixture = DirectCleanMergeFixture::create(&app).await;
    let successor = TestUser::create_with_scopes(
        &app,
        fixture.org_id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;

    let barrier =
        g5_api::services::workspace_service::install_prepared_landing_barrier(fixture.parent_id);
    let merge = fixture.spawn_direct_clean_merge(&app);
    tokio::time::timeout(
        std::time::Duration::from_secs(10),
        barrier.wait_until_prepared(),
    )
    .await
    .expect("merge reaches the post-Git, pre-landing boundary");
    assert_eq!(
        git_stdout(&fixture.repo_path, &["rev-parse", "refs/heads/main"]),
        fixture.main_sha
    );

    let transfer = app
        .authenticated_client(&fixture.owner)
        .post_json(
            &format!("/api/v1/workspaces/{}/transfer", fixture.parent_id),
            &json!({
                "toUserId": successor.id,
                "idempotencyKey": format!("direct-clean-transfer-{}", Uuid::new_v4())
            }),
        )
        .await;
    let transfer_status = transfer.status();
    let transfer_body: Value = transfer.json().await.expect("parse transfer response");
    assert_eq!(transfer_status, StatusCode::OK, "{transfer_body}");
    assert_eq!(transfer_body["data"]["status"], "completed");
    barrier.release();

    let merge_response = merge.await.expect("join direct-clean merge");
    let merge_status = merge_response.status();
    let merge_body: Value = merge_response.json().await.expect("parse refused merge");
    assert_eq!(merge_status, StatusCode::FORBIDDEN, "{merge_body}");
    assert_eq!(
        git_stdout(&fixture.repo_path, &["rev-parse", "refs/heads/main"]),
        fixture.main_sha,
        "the target branch must not advance for an owner the target no longer has"
    );
    assert_eq!(
        sqlx::query_scalar::<_, Uuid>("SELECT owner_user_id FROM workspaces WHERE id = $1")
            .bind(fixture.parent_id)
            .fetch_one(&app.db)
            .await
            .expect("load new parent owner"),
        successor.id
    );
    app.cleanup().await;
}

/// Control for the transfer race: the same merge with the owner unchanged
/// lands a merge commit on the target.
#[tokio::test]
async fn direct_clean_merge_lands_when_the_target_owner_is_unchanged() {
    let app = TestApp::spawn().await;
    let fixture = DirectCleanMergeFixture::create(&app).await;

    let merge_response = fixture
        .spawn_direct_clean_merge(&app)
        .await
        .expect("join direct-clean merge");
    let merge_status = merge_response.status();
    let merge_body: Value = merge_response.json().await.expect("parse merge response");
    assert_eq!(merge_status, StatusCode::OK, "{merge_body}");
    assert!(merge_body["data"]["run"].is_object(), "{merge_body}");
    let main_head = git_stdout(&fixture.repo_path, &["rev-parse", "refs/heads/main"]);
    assert_ne!(main_head, fixture.main_sha, "the target advanced");
    for parent in [&fixture.clone_sha, &fixture.main_sha] {
        run_git(
            &fixture.repo_path,
            &["merge-base", "--is-ancestor", parent, "refs/heads/main"],
        );
    }
    app.cleanup().await;
}

struct DirectCleanMergeFixture {
    org_id: Uuid,
    owner: TestUser,
    parent_id: Uuid,
    clone_id: Uuid,
    repo_path: std::path::PathBuf,
    main_sha: String,
    clone_sha: String,
    preflight_fingerprint: String,
}

impl DirectCleanMergeFixture {
    /// A parent on `main` and a clone branch that both moved since they
    /// diverged, in different files: a clean three-way merge, the only
    /// preflight decision the direct-clean route accepts.
    async fn create(app: &TestApp) -> Self {
        let mut owner = TestUser::create(app).await;
        let org = TestOrg::create_with_admin(app, &mut owner).await;
        let repo_id = create_test_repo(app, &org).await;
        let repo_path = app.init_fixture_repo(repo_id).await;
        run_git(&repo_path, &["init"]);
        run_git(&repo_path, &["config", "user.email", "g5@test.local"]);
        run_git(&repo_path, &["config", "user.name", "G5 Test"]);
        run_git(&repo_path, &["checkout", "-b", "main"]);
        std::fs::write(repo_path.join("README.md"), "base\n").expect("write base file");
        run_git(&repo_path, &["add", "README.md"]);
        run_git(&repo_path, &["commit", "-m", "base"]);
        let clone_branch_name = "main-direct-clean";
        run_git(&repo_path, &["checkout", "-b", clone_branch_name]);
        std::fs::write(repo_path.join("README.md"), "base\nclone change\n")
            .expect("write clone file");
        run_git(&repo_path, &["commit", "-am", "clone change"]);
        let clone_sha = git_stdout(&repo_path, &["rev-parse", "HEAD"]);
        run_git(&repo_path, &["checkout", "main"]);
        std::fs::write(repo_path.join("NOTES.md"), "parent moved\n").expect("write parent file");
        run_git(&repo_path, &["add", "NOTES.md"]);
        run_git(&repo_path, &["commit", "-m", "parent change"]);
        let main_sha = git_stdout(&repo_path, &["rev-parse", "HEAD"]);

        let parent_id = insert_workspace(app, org.id, "main", None).await;
        let clone_id = insert_workspace(app, org.id, clone_branch_name, Some(parent_id)).await;
        insert_workspace_repo(app, org.id, parent_id, repo_id, &main_sha).await;
        insert_workspace_repo(app, org.id, clone_id, repo_id, &clone_sha).await;
        sqlx::query(
            "INSERT INTO workspace_vms(org_id, workspace_id, status) VALUES ($1,$2,'stopped')",
        )
        .bind(org.id)
        .bind(parent_id)
        .execute(&app.db)
        .await
        .expect("seed stopped parent incarnation for transfer");

        let preflight = app
            .authenticated_client(&owner)
            .post_json(
                &format!("/api/v1/workspaces/{clone_id}/rebase-merge/preflight"),
                &json!({ "targetWorkspaceId": parent_id }),
            )
            .await;
        let preflight_status = preflight.status();
        let preflight_body: Value = preflight.json().await.expect("parse preflight");
        assert_eq!(preflight_status, StatusCode::OK, "{preflight_body}");
        assert_eq!(
            preflight_body["data"]["preflight"]["decision"], "cleanMergeNoGitConflicts",
            "{preflight_body}"
        );
        let preflight_fingerprint = preflight_body["data"]["preflight"]["preflightFingerprint"]
            .as_str()
            .expect("preflight fingerprint")
            .to_string();
        Self {
            org_id: org.id,
            owner,
            parent_id,
            clone_id,
            repo_path,
            main_sha,
            clone_sha,
            preflight_fingerprint,
        }
    }

    fn spawn_direct_clean_merge(
        &self,
        app: &TestApp,
    ) -> tokio::task::JoinHandle<reqwest::Response> {
        let client = app.client.clone();
        let url = format!(
            "{}/api/v1/workspaces/{}/rebase-merge/direct-clean",
            app.base_url, self.clone_id
        );
        let token = self.owner.jwt.clone();
        let body = json!({
            "targetWorkspaceId": self.parent_id,
            "preflightFingerprint": self.preflight_fingerprint,
        });
        tokio::spawn(async move {
            client
                .post(url)
                .bearer_auth(token)
                .json(&body)
                .send()
                .await
                .expect("send direct-clean merge")
        })
    }
}

/// The legacy merge endpoint can target a workspace other than the clone's
/// parent. Such a target has no live-child blocker, so a deletion reservation
/// admitted while Git preparation is paused must fence the later landing.
#[tokio::test]
async fn deletion_during_clone_merge_preparation_refuses_the_reserved_targets_landing() {
    use g5_api::services::workspace_service::operation_test_barrier::{self, Stage};
    let app = TestApp::spawn().await;
    let fixture = DirectCleanMergeFixture::create(&app).await;
    let actual_parent = insert_workspace(&app, fixture.org_id, "other-parent", None).await;
    sqlx::query("UPDATE workspaces SET cloned_from_id=$2,merge_target_workspace_id=$3 WHERE id=$1")
        .bind(fixture.clone_id)
        .bind(actual_parent)
        .bind(fixture.parent_id)
        .execute(&app.db)
        .await
        .unwrap();
    let merge_gate = g5_api::services::workspace_service::install_clone_merge_preparation_barrier(
        fixture.parent_id,
    );
    let client = app.authenticated_client(&fixture.owner);
    let path = format!("/api/v1/workspaces/{}/merge", fixture.clone_id);
    let merge = tokio::spawn(async move { client.post_json(&path, &json!({})).await });
    tokio::time::timeout(
        std::time::Duration::from_secs(10),
        merge_gate.wait_until_prepared(),
    )
    .await
    .expect("clone merge reaches its application boundary");

    let delete_gate = operation_test_barrier::install(fixture.parent_id, Stage::DeleteAdmitted);
    let client = app.authenticated_client(&fixture.owner);
    let path = format!("/api/v1/workspaces/{}/delete", fixture.parent_id);
    let deletion = tokio::spawn(async move { client.post_json(&path, &json!({})).await });
    delete_gate.reached().await;
    merge_gate.release();
    let response = merge.await.unwrap();
    let status = response.status();
    let body = response.text().await.unwrap();
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(
        git_stdout(&fixture.repo_path, &["rev-parse", "refs/heads/main"]),
        fixture.main_sha
    );
    delete_gate.release();
    assert_eq!(deletion.await.unwrap().status(), StatusCode::NO_CONTENT);
    app.cleanup().await;
}
