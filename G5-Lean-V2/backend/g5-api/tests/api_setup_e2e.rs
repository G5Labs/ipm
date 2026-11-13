//! Integration tests for the workspace setup flow — the "last mile" stubs
//! that were implemented to enable the e2e demo.
//!
//! Covers:
//! 1. `GET /api/v1/resolve?org=...&repo=...&workspace=...` — Slug resolution
//! 2. `GET /api/v1/workspaces/:id/settings` — Get workspace settings
//! 3. `PUT /api/v1/workspaces/:id/settings` — Update workspace settings
//! 4. `GET /api/v1/branches/:branchId/files` — List file tree
//! 5. `GET /api/v1/branches/:branchId/files/*path` — Get file content
//! 6. `GET /api/v1/instance/chat/models` — Get available LLM models

use crate::helpers;
use g5_auth::Scope;

use helpers::{TestApp, TestOrg, TestUser, TestWorkspace};
use reqwest::StatusCode;
use serde_json::{json, Value};
use std::process::Command;
use uuid::Uuid;

// ===========================================================================
// Helper: create a repo with a known slug
// ===========================================================================

async fn create_test_repo(app: &TestApp, org: &TestOrg, slug: &str) -> Uuid {
    let repo_id = Uuid::new_v4();
    let repo_path = app.init_repo(org.id, repo_id).await;
    run_git(&repo_path, &["init"]);
    run_git(&repo_path, &["config", "user.email", "g5@test.local"]);
    run_git(&repo_path, &["config", "user.name", "G5 Test"]);
    run_git(&repo_path, &["checkout", "-B", "main"]);
    std::fs::create_dir_all(repo_path.join("src")).expect("Failed to seed src dir");
    std::fs::write(repo_path.join("Cargo.toml"), "[package]\nname = \"demo\"\n")
        .expect("Failed to seed Cargo.toml");
    std::fs::write(repo_path.join("src/main.rs"), "fn main() {}\n")
        .expect("Failed to seed main.rs");
    run_git(&repo_path, &["add", "Cargo.toml"]);
    run_git(&repo_path, &["add", "src/main.rs"]);
    run_git(&repo_path, &["commit", "-m", "seed setup e2e repo"]);
    let git_url = repo_path.display().to_string();

    sqlx::query(
        r#"INSERT INTO repos (id, org_id, name, slug, git_url, default_branch, provider)
           VALUES ($1, $2, $3, $4, $5, $6, $7)"#,
    )
    .bind(repo_id)
    .bind(org.id)
    .bind(slug)
    .bind(slug)
    .bind(git_url)
    .bind("main")
    .bind(None::<&str>)
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

fn git_ok(cwd: &std::path::Path, args: &[&str]) {
    run_git(cwd, args);
}

fn git_stdout(cwd: &std::path::Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("git command should start");
    assert!(
        output.status.success(),
        "git {:?} failed: stdout={} stderr={}",
        args,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("git stdout should be utf-8")
        .trim()
        .to_string()
}

fn init_bare_repo_with_files(
    repo_path: &std::path::Path,
    branch: &str,
    files: &[(&str, &str)],
) -> String {
    if repo_path.exists() {
        std::fs::remove_dir_all(repo_path).expect("should remove existing bare repo");
    }
    if let Some(parent) = repo_path.parent() {
        std::fs::create_dir_all(parent).expect("should create bare repo parent");
    }

    let temp = tempfile::tempdir().expect("should create temp worktree");
    git_ok(temp.path(), &["init"]);
    git_ok(temp.path(), &["config", "user.name", "G5 Test"]);
    git_ok(temp.path(), &["config", "user.email", "g5@test.local"]);

    for (rel_path, content) in files {
        let file_path = temp.path().join(rel_path);
        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent).expect("should create repo file parent");
        }
        std::fs::write(&file_path, content).expect("should write repo file");
    }

    git_ok(temp.path(), &["add", "."]);
    git_ok(temp.path(), &["commit", "-m", "Initial commit"]);
    git_ok(temp.path(), &["branch", "-M", branch]);

    let status = std::process::Command::new("git")
        .args(["init", "--bare", repo_path.to_string_lossy().as_ref()])
        .status()
        .expect("git init --bare should start");
    assert!(status.success(), "git init --bare should succeed");

    git_ok(
        temp.path(),
        &[
            "remote",
            "add",
            "origin",
            repo_path.to_string_lossy().as_ref(),
        ],
    );
    git_ok(
        temp.path(),
        &["push", "origin", &format!("HEAD:refs/heads/{branch}")],
    );
    git_stdout(temp.path(), &["rev-parse", "HEAD"])
}

/// Helper: insert a workspace_branches row and optionally set up a repo
/// directory on disk with some files for file-tree tests.
async fn replace_workspace_repo_authority(
    app: &TestApp,
    workspace: &TestWorkspace,
    repo_id: Uuid,
    head_commit: &str,
    materialization_status: &str,
) -> String {
    sqlx::query("DELETE FROM workspace_repos WHERE workspace_id = $1")
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .expect("remove private fixture repo mappings");
    sqlx::query(
        "INSERT INTO workspace_repos \
         (org_id, workspace_id, repo_id, root_path, role) \
         VALUES ($1, $2, $3, '', 'code')",
    )
    .bind(workspace.org_id)
    .bind(workspace.id)
    .bind(repo_id)
    .execute(&app.db)
    .await
    .expect("insert authoritative workspace repo mapping");
    if materialization_status == "materialized" {
        helpers::seed_branch_ref(app, repo_id, &workspace.name, head_commit).await;
    }
    format!("/api/v1/workspaces/{}/repos/{repo_id}", workspace.id)
}

async fn create_repo_with_files(app: &TestApp, workspace: &TestWorkspace, repo_id: Uuid) -> String {
    let repo_path = sqlx::query_scalar::<_, String>("SELECT git_url FROM repos WHERE id = $1")
        .bind(repo_id)
        .fetch_one(&app.db)
        .await
        .expect("test repository should have an exact storage path");
    let repo_path = std::path::PathBuf::from(repo_path);
    std::fs::write(repo_path.join("README.md"), "# Test Repo\nHello!")
        .expect("Failed to write README");
    std::fs::write(
        repo_path.join("src/main.rs"),
        "fn main() {\n    println!(\"hello\");\n}\n",
    )
    .expect("Failed to write main.rs");
    std::fs::write(repo_path.join("src/lib.rs"), "pub mod utils;\n")
        .expect("Failed to write lib.rs");
    run_git(
        &repo_path,
        &["add", "README.md", "src/main.rs", "src/lib.rs"],
    );
    run_git(&repo_path, &["commit", "-m", "seed file API fixture"]);
    let head_commit = git_stdout(&repo_path, &["rev-parse", "HEAD"]);

    replace_workspace_repo_authority(app, workspace, repo_id, &head_commit, "materialized").await
}

async fn create_materialized_history_repo(
    app: &TestApp,
    workspace: &TestWorkspace,
    repo_id: Uuid,
) -> (String, std::path::PathBuf, String) {
    let repo_path = app.repo_path(repo_id).await;
    let head = git_stdout(&repo_path, &["rev-parse", "HEAD"]);
    let repo_route =
        replace_workspace_repo_authority(app, workspace, repo_id, &head, "materialized").await;
    (repo_route, repo_path, head)
}

/// Cleanup repo directory after test
async fn cleanup_repo_dir(repo_id: Uuid) {
    let repo_dir = format!("/tmp/g5-test-repos/{}", repo_id);
    let _ = tokio::fs::remove_dir_all(&repo_dir).await;
}

// ===========================================================================
// Group 1: Resolve Slugs — GET /api/v1/resolve
// ===========================================================================

#[tokio::test]
async fn test_resolve_slugs_org_only() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let client = app.authenticated_client(&user);

    let response = client
        .get(&format!("/api/v1/resolve?org={}", org.slug))
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.headers().get("location").is_none());
    let body: Value = response.json().await.unwrap();
    assert_eq!(body, json!({ "data": { "orgId": org.id.to_string() } }));

    app.cleanup().await;
}

#[tokio::test]
async fn test_resolve_slugs_org_and_repo() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let repo_id = create_test_repo(&app, &org, "my-repo").await;
    let user = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let client = app.authenticated_client(&user);

    let response = client
        .get(&format!("/api/v1/resolve?org={}&repo=my-repo", org.slug))
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    let data = &body["data"];
    assert_eq!(data["orgId"].as_str().unwrap(), org.id.to_string());
    assert_eq!(data["repoId"].as_str().unwrap(), repo_id.to_string());

    app.cleanup().await;
}

#[tokio::test]
async fn test_resolve_slugs_org_and_workspace() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let user = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let client = app.authenticated_client(&user);

    // Resolve by workspace name (case-insensitive)
    let response = client
        .get(&format!(
            "/api/v1/resolve?org={}&workspace={}",
            org.slug, workspace.name
        ))
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    let data = &body["data"];
    assert_eq!(data["orgId"].as_str().unwrap(), org.id.to_string());
    assert_eq!(
        data["workspaceId"].as_str().unwrap(),
        workspace.id.to_string()
    );

    app.cleanup().await;
}

#[tokio::test]
async fn test_resolve_slugs_workspace_in_non_selected_member_org() {
    let app = TestApp::spawn().await;
    let primary_org = TestOrg::create(&app).await;
    let secondary_org = TestOrg::create(&app).await;
    let workspace = TestWorkspace::create(&app, &secondary_org).await;
    let user = TestUser::create_with_scopes(&app, primary_org.id, &[Scope::WorkspacesRead]).await;

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
    let response = client
        .get(&format!(
            "/api/v1/resolve?org={}&workspace={}",
            secondary_org.slug, workspace.name
        ))
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    let data = &body["data"];
    assert_eq!(
        data["orgId"].as_str().unwrap(),
        secondary_org.id.to_string()
    );
    assert_eq!(
        data["workspaceId"].as_str().unwrap(),
        workspace.id.to_string()
    );

    app.cleanup().await;
}

#[tokio::test]
async fn test_resolve_slugs_not_found() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let client = resolve_redirect_client();

    for params in [
        vec![("org", "nonexistent-org")],
        vec![("org", org.slug.as_str()), ("repo", "nonexistent-repo")],
        vec![
            ("org", org.slug.as_str()),
            ("workspace", "nonexistent-workspace"),
        ],
    ] {
        // Slug resolution keeps precedence, including for an unsafe redirect.
        for redirect in [
            None,
            Some("/app/{orgId}"),
            Some("https://evil.example/path"),
        ] {
            let mut request = client
                .get(format!("{}/api/v1/resolve", app.base_url))
                .bearer_auth(&user.jwt)
                .query(&params);
            if let Some(redirect) = redirect {
                request = request.query(&[("redirect", redirect)]);
            }
            let response = request.send().await.unwrap();
            assert_eq!(response.status(), StatusCode::NOT_FOUND);
            assert!(response.headers().get("location").is_none());
            let body: Value = response.json().await.unwrap();
            assert_eq!(body["error"]["code"], "NOT_FOUND");
        }
    }

    app.cleanup().await;
}

#[tokio::test]
async fn test_resolve_slugs_with_redirect() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let repo_id = create_test_repo(&app, &org, "my-repo").await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let user = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let client = resolve_redirect_client();

    for (template, expected) in [
        ("/", "/".to_string()),
        (
            "/app/{orgId}/dashboard",
            format!("/app/{}/dashboard", org.id),
        ),
        (
            "/app/{orgId}/repos/{repoId}/workspaces/{workspaceId}?org={orgId}&view=files#L10",
            format!(
                "/app/{}/repos/{repo_id}/workspaces/{}?org={}&view=files#L10",
                org.id, workspace.id, org.id
            ),
        ),
        // Query decoding happens once. Encoded path characters in the final
        // Location remain local and must not be recursively decoded.
        ("/docs/a%20b", "/docs/a%20b".to_string()),
        ("/%5Cevil.example/path", "/%5Cevil.example/path".to_string()),
        ("/%2Fevil.example/path", "/%2Fevil.example/path".to_string()),
    ] {
        let response = client
            .get(format!("{}/api/v1/resolve", app.base_url))
            .bearer_auth(&user.jwt)
            .query(&[
                ("org", org.slug.as_str()),
                ("repo", "my-repo"),
                ("workspace", workspace.name.as_str()),
                ("redirect", template),
            ])
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::FOUND, "{template}");
        assert_eq!(response.headers()["location"], expected, "{template}");
    }

    app.cleanup().await;
}

fn resolve_redirect_client() -> reqwest::Client {
    // Inspect the redirect response itself; the shared test client follows
    // redirects and would otherwise report the SPA fallback.
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("build no-redirect client")
}

async fn assert_resolve_redirect_rejected(response: reqwest::Response, case: &str) {
    assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{case:?}");
    assert!(response.headers().get("location").is_none(), "{case:?}");
    let body: Value = response.json().await.expect("structured error response");
    assert_eq!(body["error"]["code"], "VALIDATION_ERROR", "{case:?}");
    assert!(body["error"]["details"]["redirect"].is_string(), "{body}");
}

#[tokio::test]
async fn test_resolve_slugs_redirect_rejects_unsafe_destinations() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let client = resolve_redirect_client();

    // Raw query values exercise Axum's decoding at the production route.
    // Do not pass these through .query(), which would encode '%' again.
    for redirect in [
        "",
        "settings",
        "https://evil.example/path",
        "http://evil.example/path",
        "//evil.example/path",
        "/\\evil.example/path",
        "\\\\evil.example/path",
        "%2F%2Fevil.example/path",
        "/%5Cevil.example/path",
        "%2f%5cevil.example/path",
        "%5C%5Cevil.example/path",
        "/a%5Cb",
        "/%09/evil.example/path",
        "/a%0D%0ASet-Cookie:%20x=y",
        "/a%00b",
        "/a%7Fb",
        "/a%C2%85b",
    ] {
        let response = client
            .get(format!(
                "{}/api/v1/resolve?org={}&redirect={redirect}",
                app.base_url, org.slug
            ))
            .bearer_auth(&user.jwt)
            .send()
            .await
            .unwrap();
        assert_resolve_redirect_rejected(response, redirect).await;
    }

    app.cleanup().await;
}

async fn check_resolve_redirect_missing_id(placeholder: &str) {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    // The template is local, but removing the missing ID creates //evil.example.
    let template = format!("/{placeholder}/evil.example/path");
    let response = resolve_redirect_client()
        .get(format!("{}/api/v1/resolve", app.base_url))
        .bearer_auth(&user.jwt)
        .query(&[("org", org.slug.as_str()), ("redirect", template.as_str())])
        .send()
        .await
        .unwrap();
    assert_resolve_redirect_rejected(response, &template).await;

    app.cleanup().await;
}

#[tokio::test]
async fn test_resolve_slugs_redirect_rejects_missing_repo_id() {
    check_resolve_redirect_missing_id("{repoId}").await;
}

#[tokio::test]
async fn test_resolve_slugs_redirect_rejects_missing_workspace_id() {
    check_resolve_redirect_missing_id("{workspaceId}").await;
}

#[tokio::test]
async fn test_resolve_slugs_redirect_checks_final_byte_length() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let client = resolve_redirect_client();

    let at_limit = format!("/{}", "a".repeat(2047));
    let interpolated_suffix = "a".repeat(2048 - 1 - org.id.to_string().len());
    for (template, expected) in [
        (at_limit.clone(), at_limit.clone()),
        (
            format!("/{{orgId}}{interpolated_suffix}"),
            format!("/{}{interpolated_suffix}", org.id),
        ),
        // A template longer than the limit is valid if removing absent IDs
        // brings the final destination within the shared byte limit.
        (
            format!("{at_limit}{{repoId}}{{workspaceId}}"),
            at_limit.clone(),
        ),
        (
            "/app/{orgId}/{repoId}/{workspaceId}".to_string(),
            format!("/app/{}//", org.id),
        ),
    ] {
        let response = client
            .get(format!("{}/api/v1/resolve", app.base_url))
            .bearer_auth(&user.jwt)
            .query(&[("org", org.slug.as_str()), ("redirect", template.as_str())])
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FOUND);
        assert_eq!(response.headers()["location"], expected);
    }

    for template in [
        format!("{at_limit}a"),
        format!("/{{orgId}}{interpolated_suffix}a"),
        // Fewer than 2048 characters, but 2049 UTF-8 bytes.
        format!("/{}", "é".repeat(1024)),
    ] {
        let response = client
            .get(format!("{}/api/v1/resolve", app.base_url))
            .bearer_auth(&user.jwt)
            .query(&[("org", org.slug.as_str()), ("redirect", template.as_str())])
            .send()
            .await
            .unwrap();
        assert_resolve_redirect_rejected(response, &template).await;
    }

    app.cleanup().await;
}

#[tokio::test]
async fn test_resolve_slugs_requires_auth_and_scope() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(&app, org.id, &[]).await;
    let client = resolve_redirect_client();

    for (token, status, code) in [
        (None, StatusCode::UNAUTHORIZED, "UNAUTHORIZED"),
        (Some(&user.jwt), StatusCode::FORBIDDEN, "FORBIDDEN"),
    ] {
        for redirect in [
            None,
            Some("/app/{orgId}"),
            Some("https://evil.example/path"),
        ] {
            let mut request = client
                .get(format!("{}/api/v1/resolve", app.base_url))
                .query(&[("org", org.slug.as_str())]);
            if let Some(redirect) = redirect {
                request = request.query(&[("redirect", redirect)]);
            }
            if let Some(token) = token {
                request = request.bearer_auth(token);
            }
            let response = request.send().await.unwrap();
            assert_eq!(response.status(), status);
            assert!(response.headers().get("location").is_none());
            let body: Value = response.json().await.unwrap();
            assert_eq!(body["error"]["code"], code);
        }
    }

    app.cleanup().await;
}

// ===========================================================================
// Group 2: Workspace Settings — GET/PUT /api/v1/workspaces/:id/settings
// ===========================================================================

#[tokio::test]
async fn test_get_workspace_settings() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let user = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let client = app.authenticated_client(&user);

    let response = client
        .get(&format!("/api/v1/workspaces/{}/settings", workspace.id))
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    let data = &body["data"];

    // Verify default settings shape
    assert_eq!(
        data["workspaceId"].as_str().unwrap(),
        workspace.id.to_string()
    );
    assert_eq!(data["spendPeriod"].as_str().unwrap(), "monthly");
    assert_eq!(data["idleTimeoutSecs"].as_i64().unwrap(), 28800);
    assert_eq!(data["responseDeadlineSecs"].as_i64().unwrap(), 900);
    assert!(data.get("retainHours").is_none(), "roots are not retained");
    assert_eq!(data["boundedExecutionSlots"].as_i64().unwrap(), 16);
    assert!(data["permissionPolicy"].is_null());

    app.cleanup().await;
}

#[tokio::test]
async fn test_get_workspace_settings_for_workspace_in_non_selected_member_org() {
    let app = TestApp::spawn().await;
    let primary_org = TestOrg::create(&app).await;
    let secondary_org = TestOrg::create(&app).await;
    let workspace = TestWorkspace::create(&app, &secondary_org).await;
    let user = TestUser::create_with_scopes(&app, primary_org.id, &[Scope::WorkspacesRead]).await;

    sqlx::query(
        r#"
        INSERT INTO org_memberships (org_id, user_id, role)
        VALUES ($1, $2, 'member')
        ON CONFLICT (org_id, user_id) DO NOTHING
        "#,
    )
    .bind(secondary_org.id)
    .bind(user.id)
    .execute(&app.db)
    .await
    .expect("Failed to insert secondary org membership");

    let client = app.authenticated_client(&user);
    let response = client
        .get(&format!("/api/v1/workspaces/{}/settings", workspace.id))
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    assert_eq!(
        body["data"]["workspaceId"].as_str().unwrap(),
        workspace.id.to_string()
    );

    app.cleanup().await;
}

#[tokio::test]
async fn test_get_workspace_settings_bootstraps_missing_row() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let user = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;

    sqlx::query("DELETE FROM workspace_settings WHERE workspace_id = $1")
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .expect("Failed to delete workspace settings row");

    let client = app.authenticated_client(&user);
    let response = client
        .get(&format!("/api/v1/workspaces/{}/settings", workspace.id))
        .await;

    assert_eq!(response.status(), StatusCode::OK);

    let recreated = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM workspace_settings WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .expect("Failed to verify recreated workspace settings row");

    assert_eq!(recreated, 1);

    app.cleanup().await;
}

#[tokio::test]
async fn test_get_workspace_settings_not_found() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let client = app.authenticated_client(&user);

    let response = client
        .get(&format!("/api/v1/workspaces/{}/settings", Uuid::new_v4()))
        .await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    app.cleanup().await;
}

#[tokio::test]
async fn test_update_workspace_settings_partial() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let client = app.authenticated_client(&user);

    // Update only the retained runtime admission controls.
    let response = client
        .put_json(
            &format!("/api/v1/workspaces/{}/settings", workspace.id),
            &json!({
                "boundedExecutionSlots": 8,
                "idleTimeoutSecs": 3600,
            }),
        )
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    let data = &body["data"];

    // Updated fields
    assert_eq!(data["boundedExecutionSlots"].as_i64().unwrap(), 8);
    assert_eq!(data["idleTimeoutSecs"].as_i64().unwrap(), 3600);

    // Unchanged defaults
    assert_eq!(data["spendPeriod"].as_str().unwrap(), "monthly");
    assert_eq!(data["responseDeadlineSecs"].as_i64().unwrap(), 900);

    // updatedBy should be the user who made the change
    assert_eq!(data["updatedBy"].as_str().unwrap(), user.id.to_string());

    app.cleanup().await;
}

#[tokio::test]
async fn test_update_workspace_settings_nullable_field() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let client = app.authenticated_client(&user);

    // Set maxSpendUsd to a value
    let response = client
        .put_json(
            &format!("/api/v1/workspaces/{}/settings", workspace.id),
            &json!({ "maxSpendUsd": 100.0 }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    assert!(body["data"]["maxSpendUsd"].as_f64().is_some());

    // Now set it to null (explicit null clears)
    let response = client
        .put_json(
            &format!("/api/v1/workspaces/{}/settings", workspace.id),
            &json!({ "maxSpendUsd": null }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    assert!(
        body["data"]["maxSpendUsd"].is_null(),
        "maxSpendUsd should be null after explicit null update"
    );

    app.cleanup().await;
}

#[tokio::test]
async fn test_update_workspace_settings_validation() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let client = app.authenticated_client(&user);

    // boundedExecutionSlots must be >= 1
    let response = client
        .put_json(
            &format!("/api/v1/workspaces/{}/settings", workspace.id),
            &json!({ "boundedExecutionSlots": 0 }),
        )
        .await;

    // Should fail validation (400 Bad Request)
    assert!(
        response.status() == StatusCode::BAD_REQUEST
            || response.status() == StatusCode::UNPROCESSABLE_ENTITY,
        "invalid settings should be rejected, got {}",
        response.status()
    );

    // idleTimeoutSecs is bounded 60..=86400.
    // Both boundary values are accepted and round-trip...
    for boundary in [60_i64, 86_400] {
        let response = client
            .put_json(
                &format!("/api/v1/workspaces/{}/settings", workspace.id),
                &json!({ "idleTimeoutSecs": boundary }),
            )
            .await;
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "idleTimeoutSecs = {boundary} (boundary) should be accepted, got {}",
            response.status()
        );
        let body: Value = response.json().await.expect("settings json");
        assert_eq!(
            body["data"]["idleTimeoutSecs"].as_i64(),
            Some(boundary),
            "the accepted boundary value must round-trip in the response DTO"
        );
    }

    // ...and a value above the maximum is rejected.
    let response = client
        .put_json(
            &format!("/api/v1/workspaces/{}/settings", workspace.id),
            &json!({ "idleTimeoutSecs": 86_401 }),
        )
        .await;
    assert!(
        response.status() == StatusCode::BAD_REQUEST
            || response.status() == StatusCode::UNPROCESSABLE_ENTITY,
        "idleTimeoutSecs above 86400 should be rejected, got {}",
        response.status()
    );

    app.cleanup().await;
}

// ===========================================================================
// Group 3: File Tree — GET /api/v1/branches/:branchId/files
// ===========================================================================

#[tokio::test]
async fn test_list_file_tree_root() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let repo_id = create_test_repo(&app, &org, "tree-test-repo").await;
    let repo_route = create_repo_with_files(&app, &workspace, repo_id).await;
    let user = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let client = app.authenticated_client(&user);

    let response = client.get(&format!("{}/files", repo_route)).await;

    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    let data = body["data"]["entries"]
        .as_array()
        .expect("data.entries should be an array");
    assert_eq!(body["data"]["source"], "committed");
    assert!(body["data"]["repoOid"].as_str().is_some());
    assert!(body["data"]["snapshotAt"].as_str().is_some());

    // Should have "src" directory and "README.md" file
    let names: Vec<&str> = data.iter().filter_map(|e| e["name"].as_str()).collect();
    assert!(
        names.contains(&"src"),
        "Should contain 'src' directory. Got: {names:?}"
    );
    assert!(
        names.contains(&"README.md"),
        "Should contain 'README.md'. Got: {names:?}"
    );

    // Verify directory entry shape
    let src_entry = data.iter().find(|e| e["name"] == "src").unwrap();
    assert_eq!(src_entry["type"].as_str().unwrap(), "directory");

    // Verify file entry shape
    let readme = data.iter().find(|e| e["name"] == "README.md").unwrap();
    assert_eq!(readme["type"].as_str().unwrap(), "file");
    assert!(readme["size"].as_i64().is_some());

    // Directories should sort before files
    let first_entry = &data[0];
    assert_eq!(
        first_entry["type"].as_str().unwrap(),
        "directory",
        "Directories should come before files"
    );

    cleanup_repo_dir(repo_id).await;
    app.cleanup().await;
}

#[tokio::test]
async fn test_list_file_tree_materialized_git_subdirectory_shows_durable_g5_content() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let repo_id = create_test_repo(&app, &org, "tree-materialized-subdir-repo").await;

    let repo_path = app.repo_path(repo_id).await;
    let head = init_bare_repo_with_files(
        &repo_path,
        "main",
        &[
            ("designs/README.md", "# Design\n"),
            (".g5/workspace.yaml", "version: 1\n"),
            (".g5/auth_state.json", "{}\n"),
            (".semlift/specs/auth.semlift.md", "# Auth Spec\n"),
        ],
    );

    let repo_route =
        replace_workspace_repo_authority(&app, &workspace, repo_id, &head, "materialized").await;

    let user = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let client = app.authenticated_client(&user);

    let root_response = client.get(&format!("{}/files?depth=-1", repo_route)).await;
    assert_eq!(root_response.status(), StatusCode::OK);
    let root_body: Value = root_response.json().await.unwrap();
    let root_data = root_body["data"]["entries"]
        .as_array()
        .expect("root data entries array");
    let root_json = serde_json::to_string(root_data).expect("root JSON");
    assert!(
        root_json.contains(".g5/workspace.yaml"),
        "normal file tree should show durable .g5 repository content: {root_json}"
    );
    assert!(
        !root_json.contains(".g5/auth_state.json"),
        "normal file tree should hide runtime .g5 metadata: {root_json}"
    );
    assert!(
        root_json.contains(".semlift"),
        "normal file tree should keep public .semlift specs: {root_json}"
    );

    let subdir_response = client
        .get(&format!("{}/files?path=designs&depth=-1", repo_route))
        .await;
    assert_eq!(subdir_response.status(), StatusCode::OK);
    let subdir_body: Value = subdir_response.json().await.unwrap();
    let subdir_data = subdir_body["data"]["entries"]
        .as_array()
        .expect("subdir data entries array");
    let names: Vec<&str> = subdir_data
        .iter()
        .filter_map(|entry| entry["name"].as_str())
        .collect();
    assert!(
        names.contains(&"README.md"),
        "path-scoped git tree should include direct children of designs/: {subdir_data:?}"
    );

    cleanup_repo_dir(repo_id).await;
    app.cleanup().await;
}

#[tokio::test]
async fn test_list_file_tree_rejects_unborn_storage_fallback() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let repo_id = create_test_repo(&app, &org, "tree-storage-fallback-repo").await;

    let repo_path = app.repo_path(repo_id).await;
    std::fs::create_dir_all(repo_path.join(".g5/experience"))
        .expect("should create durable .g5 directory");
    std::fs::create_dir_all(repo_path.join(".github/workflows"))
        .expect("should create hidden control directory");
    std::fs::write(repo_path.join(".g5/workspace.yaml"), "version: 1\n")
        .expect("should write durable .g5 content");
    std::fs::write(repo_path.join(".g5/experience/fallback.md"), "# Fallback\n")
        .expect("should write durable .g5 experience");
    std::fs::write(repo_path.join(".g5/auth_state.json"), "{}\n")
        .expect("should write runtime .g5 metadata");
    std::fs::write(repo_path.join(".github/workflows/ci.yml"), "name: ci\n")
        .expect("should write hidden control content");

    let repo_route =
        replace_workspace_repo_authority(&app, &workspace, repo_id, "", "unborn").await;

    let user = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let client = app.authenticated_client(&user);
    let response = client.get(&format!("{}/files?depth=-1", repo_route)).await;

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

    cleanup_repo_dir(repo_id).await;
    app.cleanup().await;
}

#[tokio::test]
async fn test_list_file_tree_subdirectory() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let repo_id = create_test_repo(&app, &org, "tree-sub-repo").await;
    let repo_route = create_repo_with_files(&app, &workspace, repo_id).await;
    let user = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let client = app.authenticated_client(&user);

    let response = client.get(&format!("{}/files?path=src", repo_route)).await;

    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    let data = body["data"]["entries"]
        .as_array()
        .expect("data.entries should be an array");

    let names: Vec<&str> = data.iter().filter_map(|e| e["name"].as_str()).collect();
    assert!(
        names.contains(&"main.rs"),
        "Should contain main.rs: {names:?}"
    );
    assert!(
        names.contains(&"lib.rs"),
        "Should contain lib.rs: {names:?}"
    );

    cleanup_repo_dir(repo_id).await;
    app.cleanup().await;
}

#[tokio::test]
async fn test_list_file_tree_nonexistent_path() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let repo_id = create_test_repo(&app, &org, "tree-nopath-repo").await;
    let repo_route = create_repo_with_files(&app, &workspace, repo_id).await;
    let user = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let client = app.authenticated_client(&user);

    let response = client
        .get(&format!("{}/files?path=nonexistent", repo_route))
        .await;

    // Non-existent path returns empty array, not an error
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    let data = body["data"]["entries"]
        .as_array()
        .expect("data.entries should be an array");
    assert!(
        data.is_empty(),
        "Non-existent path should return empty list"
    );

    cleanup_repo_dir(repo_id).await;
    app.cleanup().await;
}

#[tokio::test]
async fn test_list_file_tree_invalid_branch() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let client = app.authenticated_client(&user);

    let response = client
        .get(&format!(
            "/api/v1/workspaces/{}/repos/{}/files",
            Uuid::new_v4(),
            Uuid::new_v4()
        ))
        .await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    app.cleanup().await;
}

#[tokio::test]
async fn test_list_file_tree_personal_workspace_requires_owner_or_admin() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create_personal(&app, &org, owner.id).await;
    let repo_id = create_test_repo(&app, &org, "tree-private-repo").await;
    let repo_route = create_repo_with_files(&app, &workspace, repo_id).await;
    let outsider = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let client = app.authenticated_client(&outsider);

    let response = client.get(&format!("{}/files", repo_route)).await;

    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    cleanup_repo_dir(repo_id).await;
    app.cleanup().await;
}

#[tokio::test]
async fn test_list_file_tree_for_file_path_returns_entry() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let repo_id = create_test_repo(&app, &org, "tree-file-path-repo").await;
    let repo_route = create_repo_with_files(&app, &workspace, repo_id).await;
    let user = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let client = app.authenticated_client(&user);

    let response = client
        .get(&format!("{}/files?path=README.md", repo_route))
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    let data = body["data"]["entries"]
        .as_array()
        .expect("data.entries should be an array");
    assert_eq!(data.len(), 1);
    assert_eq!(data[0]["name"], "README.md");
    assert_eq!(data[0]["type"], "file");

    cleanup_repo_dir(repo_id).await;
    app.cleanup().await;
}

// ===========================================================================
// Group 4: File Content — GET /api/v1/branches/:branchId/files/*path
// ===========================================================================

#[tokio::test]
async fn test_get_file_content_text() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let repo_id = create_test_repo(&app, &org, "content-txt-repo").await;
    let repo_route = create_repo_with_files(&app, &workspace, repo_id).await;
    let user = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let client = app.authenticated_client(&user);

    let response = client.get(&format!("{}/files/README.md", repo_route)).await;

    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    let data = &body["data"];

    assert_eq!(data["path"].as_str().unwrap(), "README.md");
    assert_eq!(data["encoding"].as_str().unwrap(), "utf-8");
    let content = data["content"].as_str().unwrap();
    assert!(
        content.contains("Test Repo"),
        "Should contain file content: {content}"
    );
    assert!(data["size"].as_i64().unwrap() > 0);
    assert!(!data["draftDirty"].as_bool().unwrap());

    cleanup_repo_dir(repo_id).await;
    app.cleanup().await;
}

#[tokio::test]
async fn test_get_file_content_nested() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let repo_id = create_test_repo(&app, &org, "content-nested-repo").await;
    let repo_route = create_repo_with_files(&app, &workspace, repo_id).await;
    let user = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let client = app.authenticated_client(&user);

    let response = client
        .get(&format!("{}/files/src/main.rs", repo_route))
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    let data = &body["data"];

    assert_eq!(data["path"].as_str().unwrap(), "src/main.rs");
    assert_eq!(data["encoding"].as_str().unwrap(), "utf-8");
    let content = data["content"].as_str().unwrap();
    assert!(
        content.contains("fn main()"),
        "Should contain Rust main fn: {content}"
    );

    cleanup_repo_dir(repo_id).await;
    app.cleanup().await;
}

#[tokio::test]
async fn test_get_file_content_not_found() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let repo_id = create_test_repo(&app, &org, "content-404-repo").await;
    let repo_route = create_repo_with_files(&app, &workspace, repo_id).await;
    let user = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let client = app.authenticated_client(&user);

    let response = client
        .get(&format!("{}/files/nonexistent.txt", repo_route))
        .await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    cleanup_repo_dir(repo_id).await;
    app.cleanup().await;
}

#[tokio::test]
async fn test_get_file_content_path_traversal_rejected() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let repo_id = create_test_repo(&app, &org, "content-traverse-repo").await;
    let repo_route = create_repo_with_files(&app, &workspace, repo_id).await;
    let user = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let client = app.authenticated_client(&user);

    let response = client
        .get(&format!("{}/files/src/../../etc/passwd", repo_route))
        .await;

    // Path traversal should be rejected (400, 404, or 422)
    let status = response.status();
    assert!(
        status == StatusCode::BAD_REQUEST
            || status == StatusCode::NOT_FOUND
            || status == StatusCode::UNPROCESSABLE_ENTITY,
        "Path traversal should be rejected, got {status}"
    );

    cleanup_repo_dir(repo_id).await;
    app.cleanup().await;
}

#[tokio::test]
async fn test_get_file_content_personal_workspace_requires_owner_or_admin() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create_personal(&app, &org, owner.id).await;
    let repo_id = create_test_repo(&app, &org, "content-private-repo").await;
    let repo_route = create_repo_with_files(&app, &workspace, repo_id).await;
    let outsider = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let client = app.authenticated_client(&outsider);

    let response = client.get(&format!("{}/files/README.md", repo_route)).await;

    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    cleanup_repo_dir(repo_id).await;
    app.cleanup().await;
}

// ===========================================================================
// Group 5: Chat Models — GET /api/v1/instance/chat/models
// ===========================================================================

#[tokio::test]
async fn test_get_available_models() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let client = app.authenticated_client(&user);

    let response = client.get("/api/v1/instance/chat/models").await;

    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    let data = body["data"].as_array().expect("data should be an array");

    // Match the actual HTTP response to every native-tested choice. No extra
    // model (including the retired hard-coded MiniMax list) can slip through.
    let matrix: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/native-provider-catalog.json"
    ))
    .unwrap();
    let providers = matrix["providers"].as_object().unwrap();
    let expected_count: usize = providers
        .values()
        .map(|provider| provider["models"].as_array().unwrap().len())
        .sum();
    assert_eq!(data.len(), expected_count);
    for (provider, catalog) in providers {
        let actual: Vec<_> = data
            .iter()
            .filter(|model| model["agentProvider"] == provider.as_str())
            .collect();
        let expected = catalog["models"].as_array().unwrap();
        assert_eq!(actual.len(), expected.len());
        for (model, expected) in actual.iter().zip(expected) {
            assert_eq!(model["id"], expected["id"]);
            assert_eq!(model["route"], expected["route"]);
            assert_eq!(model["efforts"], expected["efforts"]);
            assert_eq!(model["defaultEffort"], expected["defaultEffort"]);
            assert_eq!(model["modes"], catalog["modes"]);
            assert_eq!(model["cliVersion"], catalog["version"]);
            assert!(model["name"].as_str().is_some_and(|name| !name.is_empty()));
            assert_eq!(model["supportsStreaming"], true);
            for field in [
                "contextWindow",
                "maxOutputTokens",
                "costPer1kInput",
                "costPer1kOutput",
            ] {
                assert!(model[field].is_null(), "catalog must not invent {field}");
            }
        }
    }

    app.cleanup().await;
}

// ===========================================================================
// Group 6: Full Setup Flow — E2E Composite Test
// ===========================================================================

/// Simulates the full workspace setup flow:
/// 1. Create org → 2. Add repo → 3. Fork workspace → 4. Resolve slugs →
/// 5. Get settings → 6. Update settings → 7. Browse files → 8. Read file
#[tokio::test]
async fn test_full_setup_flow() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    // 1. Org exists (created above)

    // 2. Create repo
    let repo_id = create_test_repo(&app, &org, "demo-repo").await;

    // 3. Seed an existing source authority and use the sole public workspace
    // creation path: fork. Direct POST /orgs/:id/workspaces is retired.
    let source_workspace = TestWorkspace::create(&app, &org).await;
    let source_head = git_stdout(&app.repo_path(repo_id).await, &["rev-parse", "HEAD"]);
    replace_workspace_repo_authority(
        &app,
        &source_workspace,
        repo_id,
        &source_head,
        "materialized",
    )
    .await;

    let response = client
        .post_json(
            &format!("/api/v1/workspaces/{}/clone", source_workspace.id),
            &json!({
                "name": "demo-workspace",
                "idempotencyKey": "api-setup-e2e-demo-workspace"
            }),
        )
        .await;
    let fork_status = response.status();
    let fork_body = response.text().await.expect("fork response body");
    assert_eq!(fork_status, StatusCode::CREATED, "{fork_body}");
    let ws_body: Value = serde_json::from_str(&fork_body).expect("fork response json");
    let workspace_id = ws_body["data"]["workspace"]["id"].as_str().unwrap();
    let repo_route = format!("/api/v1/workspaces/{workspace_id}/repos/{repo_id}");

    // 4. Resolve slugs (org + workspace name)
    let response = client
        .get(&format!(
            "/api/v1/resolve?org={}&workspace=demo-workspace",
            org.slug
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let resolve_body: Value = response.json().await.unwrap();
    assert_eq!(
        resolve_body["data"]["orgId"].as_str().unwrap(),
        org.id.to_string()
    );
    assert_eq!(
        resolve_body["data"]["workspaceId"].as_str().unwrap(),
        workspace_id
    );

    // 5. Get settings
    let response = client
        .get(&format!("/api/v1/workspaces/{}/settings", workspace_id))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let settings: Value = response.json().await.unwrap();
    assert_eq!(
        settings["data"]["boundedExecutionSlots"].as_i64().unwrap(),
        16
    );

    // 6. Update settings
    let response = client
        .put_json(
            &format!("/api/v1/workspaces/{}/settings", workspace_id),
            &json!({
                "boundedExecutionSlots": 12,
                "maxSpendUsd": 50.0,
                "idleTimeoutSecs": 3600,
            }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let updated: Value = response.json().await.unwrap();
    assert_eq!(
        updated["data"]["boundedExecutionSlots"].as_i64().unwrap(),
        12
    );
    assert_eq!(updated["data"]["maxSpendUsd"].as_f64().unwrap(), 50.0);
    assert_eq!(updated["data"]["idleTimeoutSecs"].as_i64().unwrap(), 3600);

    // 7. Browse files
    let response = client.get(&format!("{}/files", repo_route)).await;
    assert_eq!(response.status(), StatusCode::OK);
    let tree: Value = response.json().await.unwrap();
    let entries = tree["data"]["entries"].as_array().unwrap();
    let names: Vec<&str> = entries.iter().filter_map(|e| e["name"].as_str()).collect();
    assert!(names.contains(&"src"), "Should list 'src' dir");
    assert!(names.contains(&"Cargo.toml"), "Should list Cargo.toml");

    // 8. Read file content
    let response = client
        .get(&format!("{}/files/src/main.rs", repo_route))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let file: Value = response.json().await.unwrap();
    assert!(file["data"]["content"]
        .as_str()
        .unwrap()
        .contains("fn main"));

    // 9. Get chat models
    let response = client.get("/api/v1/instance/chat/models").await;
    assert_eq!(response.status(), StatusCode::OK);

    cleanup_repo_dir(repo_id).await;
    app.cleanup().await;
}

#[tokio::test]
async fn test_source_control_history_api_contract_and_repository_isolation() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let repo_id = create_test_repo(&app, &org, "history-api-repo").await;
    let repo_path = app.repo_path(repo_id).await;

    for index in 1..=12 {
        std::fs::write(
            repo_path.join("history.txt"),
            format!("history revision {index}\n"),
        )
        .expect("should update history fixture");
        git_ok(&repo_path, &["add", "history.txt"]);
        git_ok(
            &repo_path,
            &["commit", "-m", &format!("history commit {index}")],
        );
    }
    let (repo_route, _repo_path, head) =
        create_materialized_history_repo(&app, &workspace, repo_id).await;

    let user = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let client = app.authenticated_client(&user);
    let no_scope_user = TestUser::create_with_scopes(&app, org.id, &[]).await;
    let no_scope_client = app.authenticated_client(&no_scope_user);

    let unauthorized = no_scope_client
        .get(&format!("{repo_route}/commits/history?limit=10"))
        .await;
    assert_eq!(unauthorized.status(), StatusCode::FORBIDDEN);

    let first_page_response = client
        .get(&format!("{repo_route}/commits/history?limit=10"))
        .await;
    assert_eq!(first_page_response.status(), StatusCode::OK);
    let first_page: Value = first_page_response.json().await.expect("history json");
    let first_commits = first_page["data"]["commits"]
        .as_array()
        .expect("commits array");
    assert_eq!(first_commits.len(), 10);
    assert_eq!(first_commits[0]["sha"].as_str(), Some(head.as_str()));
    assert_eq!(
        first_commits[0]["summary"].as_str(),
        Some("history commit 12")
    );
    let cursor = first_page["data"]["nextCursor"]
        .as_str()
        .expect("first page cursor")
        .to_string();
    assert_eq!(cursor.len(), 40);

    let second_page_response = client
        .get(&format!(
            "{repo_route}/commits/history?limit=10&cursor={cursor}"
        ))
        .await;
    assert_eq!(second_page_response.status(), StatusCode::OK);
    let second_page: Value = second_page_response.json().await.expect("history json");
    let second_commits = second_page["data"]["commits"]
        .as_array()
        .expect("commits array");
    assert_eq!(second_commits.len(), 3);
    let first_shas: std::collections::HashSet<&str> = first_commits
        .iter()
        .filter_map(|entry| entry["sha"].as_str())
        .collect();
    assert!(second_commits
        .iter()
        .filter_map(|entry| entry["sha"].as_str())
        .all(|sha| !first_shas.contains(sha)));
    assert!(second_page["data"]["nextCursor"].is_null());

    let diff_response = client
        .get(&format!("{repo_route}/commits/{head}/diff"))
        .await;
    assert_eq!(diff_response.status(), StatusCode::OK);
    let diff: Value = diff_response.json().await.expect("commit diff json");
    assert_eq!(diff["data"]["commit"]["sha"].as_str(), Some(head.as_str()));
    assert_eq!(diff["data"]["comparisonBase"]["kind"], "parent");
    let history_file = diff["data"]["files"]
        .as_array()
        .expect("file summaries")
        .iter()
        .find(|file| file["newPath"] == "history.txt")
        .expect("history.txt summary");
    assert_eq!(history_file["status"], "modified");

    let file_response = client
        .get(&format!(
            "{repo_route}/commits/{head}/diff/files/history.txt"
        ))
        .await;
    assert_eq!(file_response.status(), StatusCode::OK);
    let file_diff: Value = file_response.json().await.expect("file diff json");
    assert_eq!(file_diff["data"]["status"], "text");
    assert!(!file_diff["data"]["hunks"]
        .as_array()
        .expect("hunks")
        .is_empty());

    let invalid_sha = client
        .get(&format!("{repo_route}/commits/abc123/diff"))
        .await;
    assert!(
        invalid_sha.status() == StatusCode::BAD_REQUEST
            || invalid_sha.status() == StatusCode::UNPROCESSABLE_ENTITY
    );

    let other_repo_id = create_test_repo(&app, &org, "history-other-repo").await;
    let other_repo_path = app.repo_path(other_repo_id).await;
    std::fs::write(
        other_repo_path.join("wrong-repository.txt"),
        "commit that exists only in the wrong repository\n",
    )
    .expect("should create wrong-repository fixture");
    git_ok(&other_repo_path, &["add", "wrong-repository.txt"]);
    git_ok(
        &other_repo_path,
        &["commit", "-m", "wrong repository commit"],
    );
    let other_head = git_stdout(&other_repo_path, &["rev-parse", "HEAD"]);
    let wrong_repo = client
        .get(&format!("{repo_route}/commits/{other_head}/diff"))
        .await;
    assert_eq!(wrong_repo.status(), StatusCode::NOT_FOUND);

    let stale_cursor = client
        .get(&format!(
            "{repo_route}/commits/history?cursor={other_head}&limit=10"
        ))
        .await;
    assert_eq!(stale_cursor.status(), StatusCode::CONFLICT);
    let stale_body: Value = stale_cursor.json().await.expect("stale error json");
    assert_eq!(stale_body["error"]["code"], "STALE_HISTORY");

    let pending_workspace = TestWorkspace::create(&app, &org).await;
    sqlx::query("DELETE FROM workspace_repos WHERE workspace_id = $1")
        .bind(pending_workspace.id)
        .execute(&app.db)
        .await
        .expect("remove pending fixture repo mappings");
    let pending_repo_route = replace_workspace_repo_authority(
        &app,
        &pending_workspace,
        other_repo_id,
        &other_head,
        "unborn",
    )
    .await;
    let pending_history = client
        .get(&format!("{pending_repo_route}/commits/history"))
        .await;
    assert!(
        pending_history.status() == StatusCode::BAD_REQUEST
            || pending_history.status() == StatusCode::UNPROCESSABLE_ENTITY
    );

    workspace.attach_parent(&app).await;
    let preview_commits = client.get(&format!("{repo_route}/commits?limit=10")).await;
    assert_eq!(preview_commits.status(), StatusCode::OK);

    app.cleanup().await;
}
