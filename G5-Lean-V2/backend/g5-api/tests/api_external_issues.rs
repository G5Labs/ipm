//! External issue domains, GitHub issue reads and workspace issue links.
//!
//! The real router, auth middleware and Postgres (RLS under `g5_tenant`) are
//! exercised; only GitHub is replaced by a wiremock server. GitHub App
//! bindings are seeded as recorded snapshots (their connection flow is
//! covered by `api_github_app_bindings`).

use crate::helpers;

use g5_api::services::external_issues::{link_forked_workspace, IssueLinkTarget, LinkRole};
use g5_auth::Scope;
use helpers::{test_config, TestApp, TestOrg, TestUser, TestWorkspace};
use reqwest::StatusCode;
use serde_json::{json, Value};
use uuid::Uuid;
use wiremock::matchers::{body_json, body_string_contains, header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

// Test-only RSA key (not a real credential); GitHub App JWTs are RS256.
const TEST_GITHUB_APP_PRIVATE_KEY: &str = "-----BEGIN PRIVATE KEY-----\nMIIEvwIBADANBgkqhkiG9w0BAQEFAASCBKkwggSlAgEAAoIBAQCqiVnFPs/rD57C\nOPHD6xoZZE9vn5sa05AFNwj/dwq7NTzF7gTlZ6HVBQm9CJrzOA6emE6F2TPL3JWG\nKS5BddbkxwAlUnOK+gSFonZwGJV4ddZQtYpxeueWG/Ah5DJRvWCr3XHM4nt/09ss\nmMPNxOZ4Mr/deEBNkpneOR8VFW+opqukxNJSCw/Q5ZWfI19M+KJi6GG1pgFir00a\nQavMgBm4Mv9Kx2J79329XCQpYWT4OqnE6LpYJ61AycDHUvm6xEbJ9FXifYxM3af0\nn5nKXzM/a3VrjUjGwDjGU+akYdHwWxsaupJu72QgB1orY6DMou3ZA+dCTQBh1uZ0\nK26K+1M5AgMBAAECggEACNM9zYrQGLKAvT1UKZbfjWPXBohEbxF2uQZdlVbUO9bn\n87g7EaIYylzN4bXxytdiTkvEeIHCf/DE3iIAbkvKWKGBvOy05QLGPCIprehfx9GV\nSyzx4qiBFpAQj7hu8DL0L0Cw9HDeAkH07CxlqZ7gbGT3796auUj2ZH0hQiaPVjNg\nHqaZvcTsMCF9Yg09q03iruNxinpzfIIASS4CLJNhH8Eqa7BQZwoaNMfN8cvrP/aC\n2v6A/etHfc1cZby1zSDGf6LBZ7F8t1rCjiBpqWQEI+WaNBou0kSb2xjhu8s3Bjzh\n1ilbrZNYUwLcrkV+moKWquz7+o9MIhJFkpmXYfz1jwKBgQDnV8UUjutPDdKsJ3td\n+jiHfzhn3eAzbA1583A9pLfvhSOK4QIJJ/odXCseHC6Bder1fEiBw2VCjqNkJ4Kq\nrAWzMoWOb23HDW031HR6e39WBF/npKpwRV8Df6db5htgyecFQxhGUmkwjPJc+/7a\ndNKUtn4VK1pzDS+YwcPzHINOqwKBgQC8tng4UYKQL02F+lht+jR2d5Yg35Xze0Gj\nNNXuu+mzrmpI+9mlnVIps4E1CDCB6yzOoHXzoiMd9iIepT2HkZaPoqkWJOhDFvHI\n3UFeMzv9F0kj5ZQVzRghoo5v5Vdm0Ahdn4kZILY3UmEPqxjF48hCEuLl8DQ1I48j\nXFdn1nVVqwKBgQCE9ZrdX+65trYYH3D16DPJHEMs3wZWR4nPRyyBIURdsuVkkkF7\nRbCoYyqc2ZGY8k6h3FZHzidKg3RAwbyFnux/u0+rNzuepYbaCY8QPh5HL6h3F9ql\nQBmFhz6GwM+HZ0MRcShockDmmHtLGcG1iGLm+aOpQwtvCGsMnoCiHwcQlQKBgQC0\ngNxREjYOcBB43FKFL1PLFOu2meY2NGk+EKWoanlJcsUa9jriXK5gUOB/t0uemTIM\nmhL4bNemKu3L3kYSTEF1NiMBmV/H/ERc2rVRwzPYC9Pb0BZrG/e/BFu1z+pf3xCB\nbL2YGV/xXoyUjhVuA0hjWulJAw8M3Oxm6EZ6fflRbQKBgQCMsAdlFb3PV56gh14V\nYtr+VG/wSj9rvsR7BgQ/sma3mY5Deqj2+Lw7bOBVZTfXQkBzOk6g9UvYtY1hr2Af\ne0IuUQQoAHYjz5t//9wYhkXYfvII4apAbfwGYFLhSbTCVXfymCZAGn7gUCkXZR4l\nigpSb+iNKO+xB1posjWgU4iFZA==\n-----END PRIVATE KEY-----\n";

const INSTALLATION_ID: i64 = 555;
/// The org's issue repository (becomes the issue domain).
const ISSUES_REPO: i64 = 1001;
/// Covered by the binding but not configured as a domain at first.
const DOCS_REPO: i64 = 1002;
/// Not covered by the binding.
const UNCOVERED_REPO: i64 = 1003;
/// An issue repository whose GitHub metadata is failing.
const BROKEN_REPO: i64 = 2001;

fn github_app_config(github: &MockServer) -> g5_api::config::GithubAppConfig {
    g5_api::config::GithubAppConfig {
        app_id: "123456".to_string(),
        private_key_pem: TEST_GITHUB_APP_PRIVATE_KEY.to_string(),
        app_slug: "g5-test-app".to_string(),
        oauth_client_id: "Iv1.test".to_string(),
        oauth_client_secret: "test-oauth-secret".to_string(),
        oauth_callback_url: "http://localhost/api/v1/github-app/oauth/callback".to_string(),
        connection_state_secret: "integration-state-secret-of-at-least-32-bytes".to_string(),
        api_base_url: github.uri(),
        web_base_url: github.uri(),
    }
}

async fn spawn_with_github(github: &MockServer) -> TestApp {
    let mut config = test_config();
    config.github_app = Some(github_app_config(github));
    TestApp::spawn_with_config(config).await
}

// ─── GitHub fixtures ─────────────────────────────────────────────────────────

/// REST issue id for issue `number` of the issue repository. Deliberately
/// above 2^53 so any float round trip would corrupt it.
fn issue_id(number: u64) -> u64 {
    9_007_199_254_740_000 + number
}

fn issue_json(github: &MockServer, number: u64, title: &str) -> Value {
    json!({
        "id": issue_id(number),
        "number": number,
        "title": title,
        "body": format!("Body of {number}"),
        "state": "open",
        "state_reason": null,
        "html_url": format!("{}/acme/issues/issues/{number}", github.uri()),
        "repository_url": format!("{}/repos/acme/issues", github.uri()),
        "user": {"login": "reporter", "avatar_url": "https://avatars.example/reporter"},
        "assignees": [{"login": "fixer", "avatar_url": null}],
        "labels": [{"name": "bug", "color": "d73a4a"}],
        "comments": 3,
        "created_at": "2026-01-01T00:00:00Z",
        "updated_at": "2026-01-05T00:00:00Z",
        "closed_at": null
    })
}

async fn mount_issue(github: &MockServer, number: u64) {
    Mock::given(method("GET"))
        .and(path(format!("/repositories/{ISSUES_REPO}/issues/{number}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(issue_json(
            github,
            number,
            &format!("Issue {number}"),
        )))
        .mount(github)
        .await;
    // Issue #12 has its own comment page below; others have none.
    if number != 12 {
        Mock::given(method("GET"))
            .and(path(format!(
                "/repositories/{ISSUES_REPO}/issues/{number}/comments"
            )))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
            .mount(github)
            .await;
    }
}

async fn mount_github(github: &MockServer) {
    for repository in [ISSUES_REPO, DOCS_REPO, BROKEN_REPO] {
        Mock::given(method("POST"))
            .and(path(format!(
                "/app/installations/{INSTALLATION_ID}/access_tokens"
            )))
            .and(body_json(json!({
                "repository_ids": [repository],
                "permissions": {"issues": "read"}
            })))
            .respond_with(
                ResponseTemplate::new(201)
                    .set_body_json(json!({"token": format!("ghs_issues_{repository}")})),
            )
            .mount(github)
            .await;
    }
    Mock::given(method("GET"))
        .and(path(format!("/repositories/{ISSUES_REPO}")))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"id": ISSUES_REPO, "full_name": "acme/issues"})),
        )
        .mount(github)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/repositories/{DOCS_REPO}")))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"id": DOCS_REPO, "full_name": "acme/docs"})),
        )
        .mount(github)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/repositories/{BROKEN_REPO}")))
        .respond_with(ResponseTemplate::new(503))
        .mount(github)
        .await;
    for number in [12, 15, 16, 17, 20, 21] {
        mount_issue(github, number).await;
    }
    // #13 is a pull request.
    let mut pull = issue_json(github, 13, "A pull request");
    pull["pull_request"] = json!({"url": "https://api.github.com/repos/acme/issues/pulls/13"});
    Mock::given(method("GET"))
        .and(path(format!("/repositories/{ISSUES_REPO}/issues/13")))
        .respond_with(ResponseTemplate::new(200).set_body_json(pull.clone()))
        .mount(github)
        .await;
    // #14 fails upstream.
    Mock::given(method("GET"))
        .and(path(format!("/repositories/{ISSUES_REPO}/issues/14")))
        .respond_with(ResponseTemplate::new(502))
        .mount(github)
        .await;
    // #18 resolves once, then GitHub fails.
    Mock::given(method("GET"))
        .and(path(format!("/repositories/{ISSUES_REPO}/issues/18")))
        .respond_with(ResponseTemplate::new(200).set_body_json(issue_json(github, 18, "Flaky")))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(github)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/repositories/{ISSUES_REPO}/issues/18")))
        .respond_with(ResponseTemplate::new(500))
        .with_priority(2)
        .mount(github)
        .await;
    // Comments of #12: GitHub reports three, the first page holds two.
    Mock::given(method("GET"))
        .and(path(format!(
            "/repositories/{ISSUES_REPO}/issues/12/comments"
        )))
        .and(query_param("per_page", "100"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"id": 7001, "user": {"login": "reporter", "avatar_url": null}, "body": "first",
             "created_at": "2026-01-02T00:00:00Z", "updated_at": "2026-01-02T00:00:00Z",
             "html_url": format!("{}/acme/issues/issues/12#issuecomment-7001", github.uri())},
            {"id": 7002, "user": null, "body": "second",
             "created_at": "2026-01-03T00:00:00Z", "updated_at": "2026-01-03T00:00:00Z",
             "html_url": format!("{}/acme/issues/issues/12#issuecomment-7002", github.uri())}
        ])))
        .mount(github)
        .await;
    // Search: one issue, one pull request and one result from another
    // repository; only the issue may be returned.
    let mut other_repo = issue_json(github, 99, "Elsewhere");
    other_repo["repository_url"] = json!(format!("{}/repos/other/public", github.uri()));
    Mock::given(method("GET"))
        .and(path("/search/issues"))
        .and(query_param(
            "q",
            "repo:acme/issues is:issue crash state:open",
        ))
        .and(query_param("sort", "updated"))
        .and(query_param("order", "desc"))
        .and(query_param("per_page", "30"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count": 3,
            "incomplete_results": false,
            "items": [issue_json(github, 12, "Crash on save"), pull, other_repo]
        })))
        .mount(github)
        .await;
    // Search where GitHub matched more issues than it returned.
    Mock::given(method("GET"))
        .and(path("/search/issues"))
        .and(query_param(
            "q",
            "repo:acme/issues is:issue flood state:open",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count": 250,
            "incomplete_results": false,
            "items": [issue_json(github, 12, "Crash on save")]
        })))
        .mount(github)
        .await;
}

// ─── Database fixtures ───────────────────────────────────────────────────────

/// Append a recorded binding snapshot with `permissions` and `coverage`.
async fn seed_binding(
    app: &TestApp,
    org_id: Uuid,
    created_by: Uuid,
    disposition: &str,
    permissions: Value,
    coverage: &[(i64, &str)],
) {
    sqlx::query(
        "INSERT INTO github_app_identities (org_id, kind) VALUES ($1, 'platform_managed') \
         ON CONFLICT (org_id, kind) DO NOTHING",
    )
    .bind(org_id)
    .execute(&app.db)
    .await
    .unwrap();
    let identity: Uuid = sqlx::query_scalar(
        "SELECT id FROM github_app_identities WHERE org_id = $1 AND kind = 'platform_managed'",
    )
    .bind(org_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    let version: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(version), 0) + 1 FROM github_app_installation_bindings WHERE org_id = $1",
    )
    .bind(org_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    let provenance = if disposition == "active" {
        "coverage_change"
    } else {
        "disconnect"
    };
    let binding: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO github_app_installation_bindings
            (org_id, app_identity_id, version, disposition, installation_id, account_id,
             account_login, permissions, provenance, created_by)
        VALUES ($1, $2, $3, $4, $5, 44, 'acme', $6, $7, $8)
        RETURNING id
        "#,
    )
    .bind(org_id)
    .bind(identity)
    .bind(version)
    .bind(disposition)
    .bind(INSTALLATION_ID)
    .bind(permissions)
    .bind(provenance)
    .bind(created_by)
    .fetch_one(&app.db)
    .await
    .unwrap();
    for (repository, full_name) in coverage {
        sqlx::query(
            "INSERT INTO github_app_installation_binding_repositories \
             (org_id, binding_id, github_repo_id, full_name) VALUES ($1, $2, $3, $4)",
        )
        .bind(org_id)
        .bind(binding)
        .bind(repository)
        .bind(full_name)
        .execute(&app.db)
        .await
        .unwrap();
    }
}

async fn seed_active_binding(app: &TestApp, org_id: Uuid, created_by: Uuid) {
    seed_binding(
        app,
        org_id,
        created_by,
        "active",
        json!({"issues": "read", "metadata": "read"}),
        &[
            (ISSUES_REPO, "acme/issues"),
            (DOCS_REPO, "acme/docs"),
            (BROKEN_REPO, "acme/broken"),
        ],
    )
    .await;
}

async fn insert_workspace(
    app: &TestApp,
    org_id: Uuid,
    owner: Uuid,
    name: &str,
    personal: bool,
) -> Uuid {
    insert_workspace_row(app, org_id, owner, name, personal, false).await
}

async fn insert_workspace_row(
    app: &TestApp,
    org_id: Uuid,
    owner: Uuid,
    name: &str,
    personal: bool,
    is_base: bool,
) -> Uuid {
    let workspace_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO workspaces
            (id, org_id, name, is_base, owner_user_id, created_by, personal_user_id)
        VALUES ($1, $2, $3, $6, $4, $4, $5)
        "#,
    )
    .bind(workspace_id)
    .bind(org_id)
    .bind(name)
    .bind(owner)
    .bind(personal.then_some(owner))
    .bind(is_base)
    .execute(&app.db)
    .await
    .unwrap();
    sqlx::query("INSERT INTO workspace_settings (org_id, workspace_id) VALUES ($1, $2)")
        .bind(org_id)
        .bind(workspace_id)
        .execute(&app.db)
        .await
        .unwrap();
    workspace_id
}

// ─── Request helpers ─────────────────────────────────────────────────────────

fn domains_path(org_id: Uuid) -> String {
    format!("/api/v1/orgs/{org_id}/issue-domains")
}

fn links_path(workspace_id: Uuid) -> String {
    format!("/api/v1/workspaces/{workspace_id}/issue-links")
}

async fn json_of(response: reqwest::Response) -> (StatusCode, Value) {
    let status = response.status();
    let body = response.json::<Value>().await.unwrap_or(Value::Null);
    (status, body)
}

async fn add_domain(
    app: &TestApp,
    user: &TestUser,
    org_id: Uuid,
    repo: i64,
) -> (StatusCode, Value) {
    json_of(
        app.authenticated_client(user)
            .post_json(
                &domains_path(org_id),
                &json!({"githubRepoId": repo.to_string(), "contentTrusted": false}),
            )
            .await,
    )
    .await
}

async fn link(
    app: &TestApp,
    user: &TestUser,
    workspace_id: Uuid,
    reference: &str,
    role: &str,
) -> (StatusCode, Value) {
    json_of(
        app.authenticated_client(user)
            .post_json(
                &links_path(workspace_id),
                &json!({"ref": reference, "role": role}),
            )
            .await,
    )
    .await
}

fn assert_issue_error(status: StatusCode, body: &Value, expected: StatusCode, kind: &str) {
    assert_eq!(status, expected, "{body}");
    assert_eq!(body["error"]["details"]["issueError"], kind, "{body}");
}

fn assert_conflict_code(status: StatusCode, body: &Value, code: &str) {
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "CONFLICT", "{body}");
    assert_eq!(body["error"]["details"]["code"], code, "{body}");
}

/// Turn external issues on for an organization, as its admin does in
/// organization settings.
async fn enable_external_issues(app: &TestApp, org_id: Uuid) {
    sqlx::query(
        "INSERT INTO org_configs (org_id, settings_json, secret_keys) \
         VALUES ($1, '{\"external_issues_enabled\": true}'::jsonb, '{}') \
         ON CONFLICT (org_id) DO UPDATE \
         SET settings_json = org_configs.settings_json || EXCLUDED.settings_json",
    )
    .bind(org_id)
    .execute(&app.db)
    .await
    .unwrap();
}

/// An org with external issues on, an active binding and the issue
/// repository as its domain.
async fn org_with_domain(app: &TestApp) -> (TestUser, TestOrg) {
    let mut owner = TestUser::create(app).await;
    let org = TestOrg::create_with_admin(app, &mut owner).await;
    enable_external_issues(app, org.id).await;
    seed_active_binding(app, org.id, owner.id).await;
    let (status, body) = add_domain(app, &owner, org.id, ISSUES_REPO).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    (owner, org)
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn domain_administration_requires_binding_coverage_and_issues_permission() {
    let github = MockServer::start().await;
    mount_github(&github).await;
    let app = spawn_with_github(&github).await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    enable_external_issues(&app, org.id).await;

    let info: Value = app
        .client
        .get(format!("{}/api/v1/instance/info", app.base_url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(info["data"]["features"]
        .as_array()
        .unwrap()
        .contains(&json!("external_issues")));

    // No binding.
    let (status, body) = add_domain(&app, &owner, org.id, ISSUES_REPO).await;
    assert_issue_error(
        status,
        &body,
        StatusCode::CONFLICT,
        "binding_needs_attention",
    );

    // Covered, but the snapshot does not grant Issues.
    seed_binding(
        &app,
        org.id,
        owner.id,
        "active",
        json!({"metadata": "read"}),
        &[(ISSUES_REPO, "acme/issues")],
    )
    .await;
    let (status, body) = add_domain(&app, &owner, org.id, ISSUES_REPO).await;
    assert_issue_error(
        status,
        &body,
        StatusCode::CONFLICT,
        "binding_needs_attention",
    );
    assert!(body["error"]["message"]
        .as_str()
        .unwrap()
        .contains("refresh the binding"));

    seed_active_binding(&app, org.id, owner.id).await;
    // Not covered.
    let (status, body) = add_domain(&app, &owner, org.id, UNCOVERED_REPO).await;
    assert_issue_error(
        status,
        &body,
        StatusCode::CONFLICT,
        "binding_needs_attention",
    );
    // Not a canonical repository id.
    let response = app
        .authenticated_client(&owner)
        .post_json(
            &domains_path(org.id),
            &json!({"githubRepoId": "01001", "contentTrusted": false}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let no_trackers: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM issue_trackers WHERE org_id = $1")
            .bind(org.id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(no_trackers, 0);

    // The first domain gets-or-creates the tracker and is the default.
    let (status, first) = add_domain(&app, &owner, org.id, ISSUES_REPO).await;
    assert_eq!(status, StatusCode::CREATED, "{first}");
    let first = &first["data"];
    assert_eq!(first["provider"], "github");
    assert_eq!(first["containerKind"], "github_repository");
    assert_eq!(first["externalContainerId"], ISSUES_REPO.to_string());
    assert_eq!(first["displayKey"], "acme/issues");
    assert_eq!(first["isDefaultCreateTarget"], true);
    assert_eq!(first["contentTrusted"], false);
    assert_eq!(first["available"], true);
    assert_eq!(first["unavailableReason"], Value::Null);
    let (status, body) = add_domain(&app, &owner, org.id, ISSUES_REPO).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");

    let (status, second) = add_domain(&app, &owner, org.id, DOCS_REPO).await;
    assert_eq!(status, StatusCode::CREATED, "{second}");
    assert_eq!(second["data"]["isDefaultCreateTarget"], false);
    assert_eq!(second["data"]["trackerId"], first["trackerId"]);
    let tracker: (i64, String, String) = sqlx::query_as(
        "SELECT COUNT(*) OVER (), instance_key, web_base_url FROM issue_trackers WHERE org_id = $1",
    )
    .bind(org.id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(tracker.0, 1);
    assert_eq!(tracker.1, "github.com");
    assert_eq!(tracker.2, github.uri());

    let second_id = second["data"]["id"].as_str().unwrap();
    let (status, patched) = json_of(
        app.authenticated_client(&owner)
            .patch_json(
                &format!("{}/{second_id}", domains_path(org.id)),
                &json!({"contentTrusted": true}),
            )
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{patched}");
    assert_eq!(patched["data"]["contentTrusted"], true);

    let response = app
        .authenticated_client(&owner)
        .delete(&format!("{}/{second_id}", domains_path(org.id)))
        .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let response = app
        .authenticated_client(&owner)
        .delete(&format!("{}/{second_id}", domains_path(org.id)))
        .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    // Members list domains; availability is a database-only binding check.
    let member = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let (status, listed) = json_of(
        app.authenticated_client(&member)
            .get(&domains_path(org.id))
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    let domains = listed["data"]["domains"].as_array().unwrap();
    assert_eq!(domains.len(), 1);
    assert_eq!(domains[0]["available"], true);

    seed_binding(&app, org.id, owner.id, "disconnected", json!({}), &[]).await;
    let (_, listed) = json_of(
        app.authenticated_client(&member)
            .get(&domains_path(org.id))
            .await,
    )
    .await;
    assert_eq!(listed["data"]["domains"][0]["available"], false);
    assert_eq!(
        listed["data"]["domains"][0]["unavailableReason"],
        "binding_inactive"
    );
    // Reads of an ineligible domain report the binding, not empty results.
    let (status, search) = json_of(
        app.authenticated_client(&member)
            .get(&format!("/api/v1/orgs/{}/issues/search?q=crash", org.id))
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{search}");
    assert_eq!(search["data"]["results"], json!([]));
    assert_eq!(
        search["data"]["domainErrors"][0]["issueError"],
        "binding_needs_attention"
    );
    assert_eq!(search["data"]["domainCount"], 1);

    app.cleanup().await;
}

#[tokio::test]
async fn non_admins_cannot_manage_domains() {
    let github = MockServer::start().await;
    mount_github(&github).await;
    let app = spawn_with_github(&github).await;
    let (owner, org) = org_with_domain(&app).await;
    let domain_id = {
        let (_, listed) = json_of(
            app.authenticated_client(&owner)
                .get(&domains_path(org.id))
                .await,
        )
        .await;
        listed["data"]["domains"][0]["id"]
            .as_str()
            .unwrap()
            .to_string()
    };

    // Missing OrgAdmin scope: stopped by the route scope.
    let member = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let (status, _) = add_domain(&app, &member, org.id, DOCS_REPO).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // OrgAdmin scope without the path-org Admin role: refused.
    let scoped_member = TestUser::create_with_scopes(&app, org.id, &[Scope::OrgAdmin]).await;
    let client = app.authenticated_client(&scoped_member);
    for response in [
        client
            .post_json(
                &domains_path(org.id),
                &json!({"githubRepoId": DOCS_REPO.to_string(), "contentTrusted": true}),
            )
            .await,
        client
            .patch_json(
                &format!("{}/{domain_id}", domains_path(org.id)),
                &json!({"contentTrusted": true}),
            )
            .await,
        client
            .delete(&format!("{}/{domain_id}", domains_path(org.id)))
            .await,
    ] {
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }
    let unchanged: (i64, bool) = sqlx::query_as(
        "SELECT COUNT(*) OVER (), bool_or(content_trusted) OVER () FROM issue_domains WHERE org_id = $1",
    )
    .bind(org.id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(unchanged, (1, false));

    app.cleanup().await;
}

#[tokio::test]
async fn search_resolve_and_detail_read_github_within_the_domain() {
    let github = MockServer::start().await;
    mount_github(&github).await;
    let app = spawn_with_github(&github).await;
    let (owner, org) = org_with_domain(&app).await;
    let member = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let client = app.authenticated_client(&member);

    // Search excludes pull requests and other repositories.
    let (status, search) = json_of(
        client
            .get(&format!(
                "/api/v1/orgs/{}/issues/search?q=crash&state=open",
                org.id
            ))
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{search}");
    let data = &search["data"];
    assert_eq!(data["domainCount"], 1);
    assert_eq!(data["domainErrors"], json!([]));
    assert_eq!(data["truncated"], false);
    let results = data["results"].as_array().unwrap();
    assert_eq!(results.len(), 1, "{search}");
    let result = &results[0];
    assert_eq!(result["provider"], "github");
    assert_eq!(result["externalIssueId"], issue_id(12).to_string());
    assert_eq!(
        result["locator"],
        json!({"repositoryId": ISSUES_REPO.to_string(), "number": 12})
    );
    assert_eq!(result["displayKey"], "acme/issues#12");
    assert_eq!(
        result["canonicalUrl"],
        format!("{}/acme/issues/issues/12", github.uri())
    );
    assert_eq!(result["state"], "open");
    assert_eq!(
        result["assignees"],
        json!([{"login": "fixer", "avatarUrl": null}])
    );
    assert_eq!(
        result["labels"],
        json!([{"name": "bug", "color": "d73a4a"}])
    );
    assert_eq!(result["contentTrusted"], false);
    assert_eq!(result["primaryWorkspace"], Value::Null);

    // Matches GitHub held back mark the page truncated even though it is
    // shorter than the page size.
    let (status, flood) = json_of(
        client
            .get(&format!("/api/v1/orgs/{}/issues/search?q=flood", org.id))
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{flood}");
    assert_eq!(flood["data"]["results"].as_array().unwrap().len(), 1);
    assert_eq!(flood["data"]["truncated"], true, "{flood}");

    let response = client
        .get(&format!(
            "/api/v1/orgs/{}/issues/search?state=bogus",
            org.id
        ))
        .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

    // Resolution by URL and by owner/name#n (case-insensitive).
    for reference in [
        format!("{}/acme/issues/issues/12", github.uri()),
        "ACME/Issues#12".to_string(),
    ] {
        let (status, body) = json_of(
            client
                .get(&format!(
                    "/api/v1/orgs/{}/issues/resolve?ref={}",
                    org.id,
                    urlencode(&reference)
                ))
                .await,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{reference}: {body}");
        assert_eq!(body["data"]["externalIssueId"], issue_id(12).to_string());
        assert_eq!(body["data"]["domainId"], result["domainId"]);
    }
    // Search finds a pasted reference without GitHub search
    // (g5.issue.search.one-path); filters still apply, and a reference to
    // another repository finds nothing.
    for (reference, state, expected) in [
        (format!("{}/acme/issues/issues/12", github.uri()), "all", 1),
        ("ACME/Issues#12".to_string(), "open", 1),
        ("acme/issues#12".to_string(), "closed", 0),
        ("other/repo#1".to_string(), "all", 0),
        ("acme/issues#13".to_string(), "all", 0),
    ] {
        let (status, body) = json_of(
            client
                .get(&format!(
                    "/api/v1/orgs/{}/issues/search?q={}&state={state}",
                    org.id,
                    urlencode(&reference)
                ))
                .await,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{reference}: {body}");
        assert_eq!(body["data"]["domainErrors"], json!([]), "{reference}");
        let found = body["data"]["results"].as_array().unwrap();
        assert_eq!(found.len(), expected, "{reference}: {body}");
        if expected == 1 {
            assert_eq!(found[0]["externalIssueId"], issue_id(12).to_string());
        }
    }
    for (reference, expected_status, kind) in [
        (
            "https://github.com/acme/issues/issues/12",
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid_reference",
        ),
        (
            "other/repo#1",
            StatusCode::UNPROCESSABLE_ENTITY,
            "outside_domain",
        ),
        (
            &format!("{}/acme/issues/pull/13", github.uri()),
            StatusCode::UNPROCESSABLE_ENTITY,
            "not_an_issue",
        ),
        (
            "acme/issues#13",
            StatusCode::UNPROCESSABLE_ENTITY,
            "not_an_issue",
        ),
        (
            "garbage",
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid_reference",
        ),
        ("acme/issues#404", StatusCode::NOT_FOUND, "not_found"),
        (
            "acme/issues#14",
            StatusCode::SERVICE_UNAVAILABLE,
            "unavailable",
        ),
    ] {
        let (status, body) = json_of(
            client
                .get(&format!(
                    "/api/v1/orgs/{}/issues/resolve?ref={}",
                    org.id,
                    urlencode(reference)
                ))
                .await,
        )
        .await;
        assert_issue_error(status, &body, expected_status, kind);
    }

    // Detail: comments oldest first, truncated against GitHub's count.
    let workspace = insert_workspace(&app, org.id, owner.id, "detail-ws", false).await;
    let (status, body) = link(&app, &owner, workspace, "acme/issues#12", "primary").await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let (status, detail) = json_of(
        client
            .get(&format!(
                "/api/v1/orgs/{}/issues/github/{ISSUES_REPO}/12",
                org.id
            ))
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{detail}");
    let detail = &detail["data"];
    assert_eq!(detail["externalIssueId"], issue_id(12).to_string());
    assert_eq!(detail["body"], "Body of 12");
    assert_eq!(detail["author"]["login"], "reporter");
    assert_eq!(detail["commentCount"], 3);
    assert_eq!(detail["commentsTruncated"], true);
    let bodies: Vec<&str> = detail["comments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|comment| comment["body"].as_str().unwrap())
        .collect();
    assert_eq!(bodies, vec!["first", "second"]);
    assert_eq!(detail["comments"][0]["id"], "7001");
    assert_eq!(detail["comments"][1]["author"], Value::Null);
    assert_eq!(
        detail["links"],
        json!([{"workspaceId": workspace, "workspaceName": "detail-ws", "role": "primary"}])
    );
    assert_eq!(
        detail["primaryWorkspace"],
        json!({"visible": true, "workspaceId": workspace, "name": "detail-ws"})
    );

    // A repository that is not a domain is refused before any GitHub read.
    let (status, body) = json_of(
        client
            .get(&format!(
                "/api/v1/orgs/{}/issues/github/{DOCS_REPO}/1",
                org.id
            ))
            .await,
    )
    .await;
    assert_issue_error(
        status,
        &body,
        StatusCode::UNPROCESSABLE_ENTITY,
        "outside_domain",
    );
    // GitHub 5xx is unavailable, never an empty issue.
    let (status, body) = json_of(
        client
            .get(&format!(
                "/api/v1/orgs/{}/issues/github/{ISSUES_REPO}/14",
                org.id
            ))
            .await,
    )
    .await;
    assert_issue_error(
        status,
        &body,
        StatusCode::SERVICE_UNAVAILABLE,
        "unavailable",
    );

    app.cleanup().await;
}

fn urlencode(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}

#[tokio::test]
async fn search_reports_failing_domains_and_empty_configuration() {
    let github = MockServer::start().await;
    mount_github(&github).await;
    let app = spawn_with_github(&github).await;

    // No domain configured.
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    enable_external_issues(&app, org.id).await;
    let (status, search) = json_of(
        app.authenticated_client(&owner)
            .get(&format!("/api/v1/orgs/{}/issues/search?q=crash", org.id))
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{search}");
    assert_eq!(
        search["data"],
        json!({"results": [], "domainErrors": [], "domainCount": 0, "truncated": false})
    );
    // With nothing to search, "me" needs no connected GitHub account.
    let (status, search) = json_of(
        app.authenticated_client(&owner)
            .get(&format!(
                "/api/v1/orgs/{}/issues/search?assignee=me",
                org.id
            ))
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{search}");
    assert_eq!(search["data"]["domainCount"], 0);

    // A domain whose GitHub metadata fails is reported, not empty.
    seed_active_binding(&app, org.id, owner.id).await;
    let (status, body) = add_domain(&app, &owner, org.id, BROKEN_REPO).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let (status, search) = json_of(
        app.authenticated_client(&owner)
            .get(&format!("/api/v1/orgs/{}/issues/search?q=crash", org.id))
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{search}");
    assert_eq!(search["data"]["results"], json!([]));
    assert_eq!(search["data"]["domainCount"], 1);
    let errors = search["data"]["domainErrors"].as_array().unwrap();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0]["issueError"], "unavailable");
    assert_eq!(errors[0]["displayKey"], "acme/broken");
    assert_eq!(errors[0]["domainId"], body["data"]["id"]);

    // With a healthy domain too, its results are returned alongside the error.
    let (status, _) = add_domain(&app, &owner, org.id, ISSUES_REPO).await;
    assert_eq!(status, StatusCode::CREATED);
    let (_, search) = json_of(
        app.authenticated_client(&owner)
            .get(&format!("/api/v1/orgs/{}/issues/search?q=crash", org.id))
            .await,
    )
    .await;
    assert_eq!(search["data"]["results"].as_array().unwrap().len(), 1);
    assert_eq!(search["data"]["domainErrors"].as_array().unwrap().len(), 1);
    assert_eq!(search["data"]["domainCount"], 2);

    // A query GitHub refuses is reported with GitHub's reason, not as an
    // outage.
    Mock::given(method("GET"))
        .and(path("/search/issues"))
        .and(query_param(
            "q",
            "repo:acme/issues is:issue author:nobody state:open",
        ))
        .respond_with(ResponseTemplate::new(422).set_body_json(json!({
            "message": "Validation Failed",
            "errors": [{"message": "The listed users cannot be searched either because the users do not exist or you do not have permission to view the users."}]
        })))
        .mount(&github)
        .await;
    let (status, search) = json_of(
        app.authenticated_client(&owner)
            .get(&format!(
                "/api/v1/orgs/{}/issues/search?q=author:nobody",
                org.id
            ))
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{search}");
    let refused = search["data"]["domainErrors"]
        .as_array()
        .unwrap()
        .iter()
        .find(|error| error["displayKey"] == "acme/issues")
        .unwrap_or_else(|| panic!("{search}"));
    assert_eq!(refused["issueError"], "invalid_query");
    assert!(
        refused["message"]
            .as_str()
            .unwrap()
            .contains("The listed users cannot be searched"),
        "{search}"
    );
    assert!(refused["message"].as_str().unwrap().contains("HTTP 422"));
    assert!(refused["message"]
        .as_str()
        .unwrap()
        .contains("made no provider changes"));

    app.cleanup().await;
}

#[tokio::test]
async fn assignee_search_uses_the_connected_account_or_finds_unassigned_issues() {
    let github = MockServer::start().await;
    mount_github(&github).await;
    // GitHub matches the login; only the connected account's issue is kept,
    // not one assigned to another account now holding that login.
    let assigned = |number: u64, assignees: Value| {
        let mut issue = issue_json(&github, number, &format!("Issue {number}"));
        issue["assignees"] = assignees;
        issue
    };
    Mock::given(method("GET"))
        .and(path("/search/issues"))
        .and(query_param(
            "q",
            format!("repo:acme/issues is:issue state:open assignee:{GITHUB_LOGIN}"),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count": 3,
            "incomplete_results": false,
            "items": [
                assigned(12, json!([{"id": GITHUB_USER_ID, "login": GITHUB_LOGIN}])),
                assigned(13, json!([{"id": 1, "login": GITHUB_LOGIN}])),
                assigned(14, json!([])),
            ]
        })))
        .mount(&github)
        .await;
    // Unassigned: an assigned issue GitHub returned anyway is dropped, and
    // the rest are ordered most recently updated first.
    let updated = |number: u64, at: &str| {
        let mut issue = assigned(number, json!([]));
        issue["updated_at"] = json!(at);
        issue
    };
    Mock::given(method("GET"))
        .and(path("/search/issues"))
        .and(query_param(
            "q",
            "repo:acme/issues is:issue state:open no:assignee",
        ))
        .and(query_param("sort", "updated"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count": 3,
            "incomplete_results": false,
            "items": [
                updated(14, "2026-01-03T00:00:00Z"),
                issue_json(&github, 12, "Assigned"),
                updated(15, "2026-01-09T00:00:00Z"),
            ]
        })))
        .mount(&github)
        .await;
    let app = spawn_with_github(&github).await;
    let (owner, org) = org_with_domain(&app).await;
    let search = |query: &'static str| {
        let client = app.authenticated_client(&owner);
        let url = format!("/api/v1/orgs/{}/issues/search?{query}", org.id);
        async move { json_of(client.get(&url).await).await }
    };

    let (status, body) = search("assignee=someone").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");

    let (status, body) = search("assignee=none").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let keys: Vec<&str> = body["data"]["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|issue| issue["displayKey"].as_str().unwrap())
        .collect();
    assert_eq!(keys, ["acme/issues#15", "acme/issues#14"], "{body}");

    // Without a connected GitHub account there is no "me" to search for.
    let (status, body) = search("assignee=me").await;
    assert_conflict_code(status, &body, "github_connection_required");

    connect_github(&app, &github, &owner, "ghu_person", 28_800).await;
    let (status, body) = search("assignee=me").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let results = body["data"]["results"].as_array().unwrap();
    assert_eq!(results.len(), 1, "{body}");
    assert_eq!(results[0]["displayKey"], "acme/issues#12");
    assert_eq!(body["data"]["domainErrors"], json!([]));

    app.cleanup().await;
}

#[tokio::test]
async fn link_invariants_and_primary_owner_visibility() {
    let github = MockServer::start().await;
    mount_github(&github).await;
    let app = spawn_with_github(&github).await;
    let (owner, org) = org_with_domain(&app).await;
    let first = insert_workspace(&app, org.id, owner.id, "first", false).await;
    let second = insert_workspace(&app, org.id, owner.id, "second", false).await;

    let (status, created) = link(
        &app,
        &owner,
        first,
        &format!("{}/acme/issues/issues/12", github.uri()),
        "primary",
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let created = &created["data"];
    assert_eq!(created["workspaceId"], first.to_string());
    assert_eq!(created["role"], "primary");
    assert_eq!(created["externalIssueId"], issue_id(12).to_string());
    assert_eq!(created["displayKey"], "acme/issues#12");
    assert_eq!(
        created["locator"],
        json!({"repositoryId": ISSUES_REPO.to_string(), "number": 12})
    );
    assert_eq!(created["linkedBy"], owner.id.to_string());
    assert_eq!(created["issueError"], Value::Null);
    assert_eq!(
        created["issue"]["primaryWorkspace"],
        json!({"visible": true, "workspaceId": first, "name": "first"})
    );

    let (status, body) = link(&app, &owner, first, "acme/issues#12", "related").await;
    assert_conflict_code(status, &body, "already_linked");
    let (status, body) = link(&app, &owner, first, "acme/issues#15", "primary").await;
    assert_conflict_code(status, &body, "workspace_has_primary");
    let (status, body) = link(&app, &owner, second, "acme/issues#12", "primary").await;
    assert_conflict_code(status, &body, "issue_has_primary");
    assert_eq!(
        body["error"]["details"]["ownerWorkspace"],
        json!({"visible": true, "workspaceId": first, "name": "first"})
    );
    let (status, body) = link(&app, &owner, second, "acme/issues#12", "related").await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let (status, body) = link(&app, &owner, first, "acme/issues#15", "related").await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let (status, body) = link(&app, &owner, first, "other/repo#1", "related").await;
    assert_issue_error(
        status,
        &body,
        StatusCode::UNPROCESSABLE_ENTITY,
        "outside_domain",
    );
    let (status, body) = link(&app, &owner, first, "acme/issues#16", "owner").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");

    // Listing: active links, primary first, with live summaries.
    let (status, listed) = json_of(
        app.authenticated_client(&owner)
            .get(&links_path(first))
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    let links = listed["data"]["links"].as_array().unwrap();
    assert_eq!(links.len(), 2);
    assert_eq!(links[0]["role"], "primary");
    assert_eq!(links[0]["issue"]["title"], "Issue 12");
    assert_eq!(links[1]["role"], "related");
    assert_eq!(links[1]["issue"]["displayKey"], "acme/issues#15");

    // An owner hidden from the viewer is not disclosed.
    let member = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let private = insert_workspace(&app, org.id, owner.id, "private", true).await;
    let mine = insert_workspace(&app, org.id, member.id, "mine", false).await;
    let (status, body) = link(&app, &owner, private, "acme/issues#16", "primary").await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let (status, body) = link(&app, &member, mine, "acme/issues#16", "primary").await;
    assert_conflict_code(status, &body, "issue_has_primary");
    assert_eq!(
        body["error"]["details"]["ownerWorkspace"],
        json!({"visible": false})
    );
    let (status, resolved) = json_of(
        app.authenticated_client(&member)
            .get(&format!(
                "/api/v1/orgs/{}/issues/resolve?ref={}",
                org.id,
                urlencode("acme/issues#16")
            ))
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{resolved}");
    assert_eq!(
        resolved["data"]["primaryWorkspace"],
        json!({"visible": false})
    );
    let (_, detail) = json_of(
        app.authenticated_client(&member)
            .get(&format!(
                "/api/v1/orgs/{}/issues/github/{ISSUES_REPO}/16",
                org.id
            ))
            .await,
    )
    .await;
    assert_eq!(
        detail["data"]["links"],
        // Neither the name nor the id of an inaccessible workspace leaks.
        json!([{"workspaceName": null, "role": "primary"}])
    );
    // Only the workspace owner may link.
    let (status, _) = link(&app, &member, first, "acme/issues#17", "related").await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Unlink releases the primary claim; the issue itself is untouched.
    let link_id = created["id"].as_str().unwrap();
    let (status, body) = json_of(
        app.authenticated_client(&owner)
            .post_json(
                &format!("{}/{link_id}/unlink", links_path(first)),
                &json!({}),
            )
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"], json!({"unlinked": true}));
    let unlinked_by: Option<Uuid> =
        sqlx::query_scalar("SELECT unlinked_by FROM workspace_issue_links WHERE id = $1")
            .bind(Uuid::parse_str(link_id).unwrap())
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(unlinked_by, Some(owner.id));
    let response = app
        .authenticated_client(&owner)
        .post_json(
            &format!("{}/{}/unlink", links_path(first), Uuid::new_v4()),
            &json!({}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    // `second` holds a related link to #12, so a third workspace claims it.
    let third = insert_workspace(&app, org.id, owner.id, "third", false).await;
    let (status, body) = link(&app, &owner, third, "acme/issues#12", "primary").await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    // And `first` may take a new primary.
    let (status, body) = link(&app, &owner, first, "acme/issues#17", "primary").await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    // The tenant role cannot delete association history.
    let mut db = app.scoped_db_for_org(org.id).await;
    let denied = sqlx::query("DELETE FROM workspace_issue_links WHERE workspace_id = $1")
        .bind(first)
        .execute(db.conn())
        .await;
    assert!(denied.is_err(), "tenant DELETE must be refused");
    drop(db);

    app.cleanup().await;
}

/// Base is read-only and not a unit of work: it takes no issue links, as
/// primary or related, while the same issue still links to an ordinary
/// workspace (`g5.issue.base-links`).
#[tokio::test]
async fn base_takes_no_issue_links() {
    let github = MockServer::start().await;
    mount_github(&github).await;
    let app = spawn_with_github(&github).await;
    let (owner, org) = org_with_domain(&app).await;
    let base = insert_workspace_row(&app, org.id, owner.id, "base", false, true).await;

    for role in ["primary", "related"] {
        let (status, body) = link(&app, &owner, base, "acme/issues#12", role).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{role}: {body}");
        assert_eq!(body["error"]["code"], "WORKSPACE_READ_ONLY", "{body}");
        assert_eq!(
            body["error"]["details"]["operation"], "issue.link",
            "{body}"
        );
    }
    let (status, body) = json_of(
        app.authenticated_client(&owner)
            .get(&links_path(base))
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["links"], json!([]), "{body}");

    let fork = insert_workspace(&app, org.id, owner.id, "fork", false).await;
    let (status, body) = link(&app, &owner, fork, "acme/issues#12", "primary").await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    app.cleanup().await;
}

#[tokio::test]
async fn workspace_deletion_releases_links_and_a_new_workspace_can_claim() {
    let github = MockServer::start().await;
    mount_github(&github).await;
    let app = spawn_with_github(&github).await;
    let (owner, org) = org_with_domain(&app).await;
    let doomed = insert_workspace(&app, org.id, owner.id, "doomed", false).await;
    let (status, body) = link(&app, &owner, doomed, "acme/issues#20", "primary").await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let (status, body) = link(&app, &owner, doomed, "acme/issues#21", "related").await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    let response = app
        .authenticated_client(&owner)
        .post_json(&format!("/api/v1/workspaces/{doomed}/delete"), &json!({}))
        .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let released: Vec<(Option<Uuid>, bool)> = sqlx::query_as(
        "SELECT unlinked_by, unlinked_at IS NOT NULL FROM workspace_issue_links WHERE workspace_id = $1",
    )
    .bind(doomed)
    .fetch_all(&app.db)
    .await
    .unwrap();
    assert_eq!(released.len(), 2);
    assert!(released
        .iter()
        .all(|(by, unlinked)| *by == Some(owner.id) && *unlinked));

    // The deleted workspace no longer accepts links.
    let (status, _) = link(&app, &owner, doomed, "acme/issues#21", "related").await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let successor = insert_workspace(&app, org.id, owner.id, "successor", false).await;
    let (status, body) = link(&app, &owner, successor, "acme/issues#20", "primary").await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let (_, detail) = json_of(
        app.authenticated_client(&owner)
            .get(&format!(
                "/api/v1/orgs/{}/issues/github/{ISSUES_REPO}/20",
                org.id
            ))
            .await,
    )
    .await;
    assert_eq!(
        detail["data"]["links"],
        json!([{"workspaceId": successor, "workspaceName": "successor", "role": "primary"}])
    );

    app.cleanup().await;
}

#[tokio::test]
async fn concurrent_primary_claims_have_exactly_one_winner() {
    let github = MockServer::start().await;
    mount_github(&github).await;
    let app = spawn_with_github(&github).await;
    let (owner, org) = org_with_domain(&app).await;

    for round in 0..3 {
        let left = insert_workspace(&app, org.id, owner.id, &format!("left-{round}"), false).await;
        let right =
            insert_workspace(&app, org.id, owner.id, &format!("right-{round}"), false).await;
        let number = [12, 15, 17][round];
        let reference = format!("acme/issues#{number}");
        let (a, b) = tokio::join!(
            link(&app, &owner, left, &reference, "primary"),
            link(&app, &owner, right, &reference, "primary"),
        );
        let statuses = [a.0, b.0];
        assert_eq!(
            statuses
                .iter()
                .filter(|status| **status == StatusCode::CREATED)
                .count(),
            1,
            "{a:?} {b:?}"
        );
        let loser = if a.0 == StatusCode::CREATED { &b } else { &a };
        assert_conflict_code(loser.0, &loser.1, "issue_has_primary");
        assert_eq!(
            loser.1["error"]["details"]["ownerWorkspace"]["visible"],
            true
        );
        let primaries: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM workspace_issue_links \
             WHERE external_issue_id = $1 AND is_primary AND unlinked_at IS NULL",
        )
        .bind(issue_id(number).to_string())
        .fetch_one(&app.db)
        .await
        .unwrap();
        assert_eq!(primaries, 1);
        // Release for the next round's issue independence.
        sqlx::query(
            "UPDATE workspace_issue_links SET unlinked_at = now(), unlinked_by = $2 \
             WHERE external_issue_id = $1 AND unlinked_at IS NULL",
        )
        .bind(issue_id(number).to_string())
        .bind(owner.id)
        .execute(&app.db)
        .await
        .unwrap();
    }

    app.cleanup().await;
}

#[tokio::test]
async fn link_list_reports_unavailable_github_per_link() {
    let github = MockServer::start().await;
    mount_github(&github).await;
    let app = spawn_with_github(&github).await;
    let (owner, org) = org_with_domain(&app).await;
    let workspace = insert_workspace(&app, org.id, owner.id, "flaky", false).await;
    // #18 resolves once for linking, then GitHub fails.
    let (status, body) = link(&app, &owner, workspace, "acme/issues#18", "primary").await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let (status, body) = link(&app, &owner, workspace, "acme/issues#12", "related").await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    let (status, listed) = json_of(
        app.authenticated_client(&owner)
            .get(&links_path(workspace))
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    let links = listed["data"]["links"].as_array().unwrap();
    assert_eq!(links.len(), 2);
    assert_eq!(links[0]["displayKey"], "acme/issues#18");
    assert_eq!(links[0]["issue"], Value::Null);
    assert_eq!(links[0]["issueError"]["issueError"], "unavailable");
    assert_eq!(
        links[1]["issue"]["externalIssueId"],
        issue_id(12).to_string()
    );
    assert_eq!(links[1]["issueError"], Value::Null);

    app.cleanup().await;
}

#[tokio::test]
async fn other_organizations_cannot_see_domains_issues_or_links() {
    let github = MockServer::start().await;
    mount_github(&github).await;
    let app = spawn_with_github(&github).await;
    let (owner, org) = org_with_domain(&app).await;
    let workspace = insert_workspace(&app, org.id, owner.id, "org-a", false).await;
    let (status, body) = link(&app, &owner, workspace, "acme/issues#12", "primary").await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    let mut outsider = TestUser::create(&app).await;
    let other = TestOrg::create_with_admin(&app, &mut outsider).await;
    enable_external_issues(&app, other.id).await;
    let client = app.authenticated_client(&outsider);
    for path in [
        domains_path(org.id),
        format!("/api/v1/orgs/{}/issues/search?q=crash", org.id),
        format!(
            "/api/v1/orgs/{}/issues/resolve?ref=acme%2Fissues%2312",
            org.id
        ),
        format!("/api/v1/orgs/{}/issues/github/{ISSUES_REPO}/12", org.id),
        links_path(workspace),
    ] {
        let response = client.get(&path).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}");
    }
    let (status, _) = link(&app, &outsider, workspace, "acme/issues#15", "related").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let response = client
        .post_json(
            &domains_path(org.id),
            &json!({"githubRepoId": DOCS_REPO.to_string(), "contentTrusted": false}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    // Org B's own view is empty, and its RLS scope cannot read org A's rows.
    let (status, listed) = json_of(client.get(&domains_path(other.id)).await).await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    assert_eq!(listed["data"]["domains"], json!([]));
    let mut db = app.scoped_db_for_org(other.id).await;
    for table in ["issue_trackers", "issue_domains", "workspace_issue_links"] {
        let visible: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
            .fetch_one(db.conn())
            .await
            .unwrap();
        assert_eq!(visible, 0, "{table} leaked across organizations");
    }
    db.commit().await.unwrap();

    app.cleanup().await;
}

// ─── Fork with an issue ──────────────────────────────────────────────────────

async fn fork_with_issue(
    app: &TestApp,
    user: &TestUser,
    source_id: Uuid,
    name: &str,
    key: &str,
    issue: Value,
) -> (StatusCode, Value) {
    json_of(
        app.authenticated_client(user)
            .post_json(
                &format!("/api/v1/workspaces/{source_id}/clone"),
                &json!({"name": name, "idempotencyKey": key, "issue": issue}),
            )
            .await,
    )
    .await
}

#[tokio::test]
async fn fork_links_an_existing_issue_when_it_completes() {
    let github = MockServer::start().await;
    mount_github(&github).await;
    let app = spawn_with_github(&github).await;
    let (owner, org) = org_with_domain(&app).await;
    let base = TestWorkspace::create(&app, &org).await;
    let primary = json!({"ref": "acme/issues#12", "role": "primary"});

    let (status, body) = fork_with_issue(
        &app,
        &owner,
        base.id,
        "fix/issue-12",
        "fork-issue-1",
        primary.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let child: Uuid = body["data"]["workspace"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let outcome = &body["data"]["issueLink"];
    assert_eq!(outcome["displayKey"], "acme/issues#12", "{body}");
    assert_eq!(
        outcome["canonicalUrl"],
        format!("{}/acme/issues/issues/12", github.uri())
    );
    assert_eq!(outcome["requestedRole"], "primary");
    assert_eq!(outcome["role"], "primary");

    let (status, links) = json_of(
        app.authenticated_client(&owner)
            .get(&links_path(child))
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{links}");
    let links = links["data"]["links"].as_array().unwrap().clone();
    assert_eq!(links.len(), 1);
    assert_eq!(links[0]["id"], outcome["linkId"]);
    assert_eq!(links[0]["role"], "primary");
    assert_eq!(links[0]["externalIssueId"], issue_id(12).to_string());
    assert_eq!(links[0]["linkedBy"], owner.id.to_string());

    // A replay returns the same fork and association without asking GitHub.
    let github_calls = github.received_requests().await.unwrap().len();
    let (status, replay) = fork_with_issue(
        &app,
        &owner,
        base.id,
        "fix/issue-12",
        "fork-issue-1",
        primary.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{replay}");
    assert_eq!(replay["data"]["workspace"]["id"], child.to_string());
    assert_eq!(replay["data"]["issueLink"], *outcome);
    assert_eq!(
        github.received_requests().await.unwrap().len(),
        github_calls
    );

    // The issue and role are part of the request the key names.
    for changed in [
        json!({"ref": "acme/issues#12", "role": "related"}),
        json!({"ref": "acme/issues#15", "role": "primary"}),
        Value::Null,
    ] {
        let (status, body) = fork_with_issue(
            &app,
            &owner,
            base.id,
            "fix/issue-12",
            "fork-issue-1",
            changed,
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
        assert_eq!(
            body["error"]["details"]["field"], "idempotencyKey",
            "{body}"
        );
    }

    // A second primary claim is refused before any fork work: no fork
    // record and no workspace.
    let (status, body) = fork_with_issue(
        &app,
        &owner,
        base.id,
        "fix/issue-12-b",
        "fork-issue-2",
        primary,
    )
    .await;
    assert_conflict_code(status, &body, "issue_has_primary");
    assert_eq!(
        body["error"]["details"]["ownerWorkspace"],
        json!({"visible": true, "workspaceId": child, "name": "fix/issue-12"})
    );
    let (forks, workspaces): (i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM workspace_forks WHERE idempotency_key = 'fork-issue-2'), \
                (SELECT count(*) FROM workspaces WHERE name = 'fix/issue-12-b')",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!((forks, workspaces), (0, 0));

    // A related link to the same issue is fine.
    let (status, body) = fork_with_issue(
        &app,
        &owner,
        base.id,
        "fix/issue-12-c",
        "fork-issue-3",
        json!({"ref": format!("{}/acme/issues/issues/12", github.uri()), "role": "related"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["data"]["issueLink"]["role"], "related");
    assert_eq!(body["data"]["issueLink"]["requestedRole"], "related");

    // Issues outside the domains and unknown roles are refused.
    let (status, body) = fork_with_issue(
        &app,
        &owner,
        base.id,
        "fix/elsewhere",
        "fork-issue-4",
        json!({"ref": "other/repo#1", "role": "related"}),
    )
    .await;
    assert_issue_error(
        status,
        &body,
        StatusCode::UNPROCESSABLE_ENTITY,
        "outside_domain",
    );
    let (status, body) = fork_with_issue(
        &app,
        &owner,
        base.id,
        "fix/bad-role",
        "fork-issue-5",
        json!({"ref": "acme/issues#12", "role": "owner"}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");

    // A fork without an issue reports no association.
    let (status, body) = fork_with_issue(
        &app,
        &owner,
        base.id,
        "fix/no-issue",
        "fork-issue-6",
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["data"]["issueLink"], Value::Null);

    app.cleanup().await;
}

/// Leave an issue-linked fork pending (its repository's provider cannot be
/// reached), then let a retry with the same key resume and complete it.
async fn pending_issue_fork_then_retry(
    app: &TestApp,
    owner: &TestUser,
    base: &TestWorkspace,
    name: &str,
    between: impl std::future::Future<Output = ()>,
) -> (StatusCode, Value) {
    let (provider, git_url): (Option<String>, String) =
        sqlx::query_as("SELECT provider, git_url FROM repos WHERE id = $1")
            .bind(base.repo_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    sqlx::query(
        "UPDATE repos SET provider = 'github', git_url = 'https://127.0.0.1:1/unavailable.git' \
         WHERE id = $1",
    )
    .bind(base.repo_id)
    .execute(&app.db)
    .await
    .unwrap();
    let issue = json!({"ref": "acme/issues#12", "role": "related"});
    let (status, body) = fork_with_issue(app, owner, base.id, name, name, issue.clone()).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    let pending: String =
        sqlx::query_scalar("SELECT status FROM workspace_forks WHERE idempotency_key = $1")
            .bind(name)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(pending, "pending");

    between.await;
    // The provider recovers. The fork froze each repository's remote in its
    // manifest at admission, so restore it there as well.
    sqlx::query("UPDATE repos SET provider = $2, git_url = $3 WHERE id = $1")
        .bind(base.repo_id)
        .bind(&provider)
        .bind(&git_url)
        .execute(&app.db)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE workspace_forks SET repo_manifest = ( \
             SELECT jsonb_agg(member || jsonb_build_object('git_url', $2::text, 'provider', $3::text)) \
             FROM jsonb_array_elements(repo_manifest) AS member) \
         WHERE idempotency_key = $1",
    )
    .bind(name)
    .bind(&git_url)
    .bind(&provider)
    .execute(&app.db)
    .await
    .unwrap();
    fork_with_issue(app, owner, base.id, name, name, issue).await
}

/// A retry resumes an admitted fork without resolving its issue again, so
/// the organization switch is checked where the link is created: a fork
/// admitted before an admin turns external issues off completes without it.
#[tokio::test]
async fn fork_admitted_before_the_switch_turns_off_completes_without_its_issue() {
    let github = MockServer::start().await;
    mount_github(&github).await;
    let app = spawn_with_github(&github).await;
    let (owner, org) = org_with_domain(&app).await;
    let base = TestWorkspace::create(&app, &org).await;

    // Still on: the resumed fork links its issue.
    let (status, body) =
        pending_issue_fork_then_retry(&app, &owner, &base, "fix/issue-12-on", async {}).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["data"]["issueLink"]["displayKey"], "acme/issues#12");

    // Turned off between admission and the retry: the fork completes and
    // publishes its workspace, with no link.
    let (status, body) = pending_issue_fork_then_retry(&app, &owner, &base, "fix/issue-12-off", {
        let db = app.db.clone();
        async move {
            sqlx::query(
                "UPDATE org_configs SET settings_json = settings_json || \
                 '{\"external_issues_enabled\": false}'::jsonb WHERE org_id = $1",
            )
            .bind(org.id)
            .execute(&db)
            .await
            .unwrap();
        }
    })
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["data"]["issueLink"], Value::Null, "{body}");
    let child: Uuid = body["data"]["workspace"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let (links, fork_status): (i64, String) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM workspace_issue_links WHERE workspace_id = $1), \
                (SELECT status FROM workspace_forks WHERE idempotency_key = 'fix/issue-12-off')",
    )
    .bind(child)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!((links, fork_status.as_str()), (0, "completed"));

    app.cleanup().await;
}

#[tokio::test]
async fn fork_naming_a_github_issue_is_unavailable_without_the_github_app() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    // Issues are on (G5's own tracker needs no App), but a GitHub reference
    // cannot be resolved on this deployment.
    enable_external_issues(&app, org.id).await;
    let base = TestWorkspace::create(&app, &org).await;

    let (status, body) = fork_with_issue(
        &app,
        &owner,
        base.id,
        "fix/no-app",
        "fork-no-app",
        json!({"ref": "acme/issues#12", "role": "primary"}),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    let (forks,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM workspace_forks WHERE idempotency_key = 'fork-no-app'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(forks, 0);

    app.cleanup().await;
}

/// A fork whose primary claim is taken while it runs still completes, with
/// the issue linked as related. The competing claim is held uncommitted so
/// the fork's insert waits on the unique index and loses to it.
#[tokio::test]
async fn forked_workspace_links_as_related_when_the_primary_is_claimed_meanwhile() {
    let github = MockServer::start().await;
    mount_github(&github).await;
    let app = spawn_with_github(&github).await;
    let (owner, org) = org_with_domain(&app).await;
    let winner = insert_workspace(&app, org.id, owner.id, "winner", false).await;
    let forked = insert_workspace(&app, org.id, owner.id, "forked", false).await;
    let later = insert_workspace(&app, org.id, owner.id, "later", false).await;
    let tracker_id: Uuid = sqlx::query_scalar("SELECT id FROM issue_trackers WHERE org_id = $1")
        .bind(org.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    let target = IssueLinkTarget {
        tracker_id,
        external_issue_id: issue_id(12).to_string(),
        display_key: "acme/issues#12".to_string(),
        canonical_url: format!("{}/acme/issues/issues/12", github.uri()),
        locator: json!({"repositoryId": ISSUES_REPO.to_string(), "number": 12}),
    };

    let mut claim = app.scoped_db_for_user_org(owner.id, org.id).await;
    let claimed = link_forked_workspace(
        &mut claim,
        org.id,
        winner,
        owner.id,
        &target,
        LinkRole::Primary,
    )
    .await
    .unwrap();
    assert!(claimed.is_primary);

    let mut fork_db = app.scoped_db_for_user_org(owner.id, org.id).await;
    let fork_target = target.clone();
    let (org_id, owner_id) = (org.id, owner.id);
    let fork_link = tokio::spawn(async move {
        let link = link_forked_workspace(
            &mut fork_db,
            org_id,
            forked,
            owner_id,
            &fork_target,
            LinkRole::Primary,
        )
        .await;
        fork_db.commit().await.unwrap();
        link
    });
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    assert!(
        !fork_link.is_finished(),
        "the fork's insert should wait on the uncommitted claim"
    );
    claim.commit().await.unwrap();
    let link = fork_link.await.unwrap().unwrap();
    assert!(!link.is_primary, "the losing claim is linked as related");

    // With the claim committed, the up-front check takes the same path.
    let mut db = app.scoped_db_for_user_org(owner.id, org.id).await;
    let link = link_forked_workspace(&mut db, org.id, later, owner.id, &target, LinkRole::Primary)
        .await
        .unwrap();
    db.commit().await.unwrap();
    assert!(!link.is_primary);

    let primaries: Vec<Uuid> = sqlx::query_scalar(
        "SELECT workspace_id FROM workspace_issue_links \
         WHERE tracker_id = $1 AND is_primary AND unlinked_at IS NULL",
    )
    .bind(tracker_id)
    .fetch_all(&app.db)
    .await
    .unwrap();
    assert_eq!(primaries, vec![winner]);

    app.cleanup().await;
}

#[tokio::test]
async fn workspace_list_reports_each_workspace_primary_issue() {
    let github = MockServer::start().await;
    mount_github(&github).await;
    let app = spawn_with_github(&github).await;
    let (owner, org) = org_with_domain(&app).await;
    let with_primary = insert_workspace(&app, org.id, owner.id, "with-primary", false).await;
    let related_only = insert_workspace(&app, org.id, owner.id, "related-only", false).await;
    let unlinked = insert_workspace(&app, org.id, owner.id, "unlinked", false).await;
    let (status, body) = link(&app, &owner, with_primary, "acme/issues#12", "primary").await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let primary_link = body["data"]["id"].clone();
    let (status, body) = link(&app, &owner, related_only, "acme/issues#15", "related").await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let (status, body) = link(&app, &owner, unlinked, "acme/issues#16", "primary").await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let unlink = app
        .authenticated_client(&owner)
        .post_json(
            &format!(
                "{}/{}/unlink",
                links_path(unlinked),
                body["data"]["id"].as_str().unwrap()
            ),
            &json!({}),
        )
        .await;
    assert_eq!(unlink.status(), StatusCode::OK);

    let (status, list) = json_of(
        app.authenticated_client(&owner)
            .get(&format!("/api/v1/orgs/{}/workspaces", org.id))
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{list}");
    let primary_of = |workspace_id: Uuid| {
        list["data"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["id"] == workspace_id.to_string())
            .map(|item| item["primaryIssue"].clone())
            .expect("workspace listed")
    };
    assert_eq!(
        primary_of(with_primary),
        json!({
            "linkId": primary_link,
            "displayKey": "acme/issues#12",
            "canonicalUrl": format!("{}/acme/issues/issues/12", github.uri()),
            "locator": {"repositoryId": ISSUES_REPO.to_string(), "number": 12},
        })
    );
    assert_eq!(primary_of(related_only), Value::Null);
    assert_eq!(primary_of(unlinked), Value::Null);

    app.cleanup().await;
}

// ─── Activity ────────────────────────────────────────────────────────────────

fn activity_path(workspace_id: Uuid) -> String {
    format!("/api/v1/workspaces/{workspace_id}/activity")
}

async fn insert_chat(app: &TestApp, workspace: &TestWorkspace, owner: Uuid, at: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO chat_sessions \
         (id, org_id, workspace_id, created_by, scope_type, title, message_count, created_at, updated_at) \
         VALUES ($1, $2, $3, $4, 'workspace', 'Implementation chat', 4, $5::timestamptz, $5::timestamptz)",
    )
    .bind(id)
    .bind(workspace.org_id)
    .bind(workspace.id)
    .bind(owner)
    .bind(at)
    .execute(&app.db)
    .await
    .expect("insert chat");
    id
}

async fn insert_task(
    app: &TestApp,
    workspace: &TestWorkspace,
    owner: Uuid,
    name: &str,
    at: &str,
) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO tasks \
         (id, org_id, workspace_id, name, kind, status, created_by, created_at, started_at, ended_at) \
         VALUES ($1, $2, $3, $4, 'command', 'succeeded', $5, \
                 $6::timestamptz - interval '1 minute', $6::timestamptz - interval '1 minute', $6::timestamptz)",
    )
    .bind(id)
    .bind(workspace.org_id)
    .bind(workspace.id)
    .bind(name)
    .bind(owner)
    .bind(at)
    .execute(&app.db)
    .await
    .expect("insert validation_task");
    id
}

#[tokio::test]
async fn workspace_activity_interleaves_issue_discussion_with_workspace_context() {
    let github = MockServer::start().await;
    mount_github(&github).await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/repositories/{ISSUES_REPO}/issues/12/timeline"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"event": "commented", "id": 7001, "user": {"login": "reviewer"},
             "body": "Please also cover two rapid disconnects.",
             "created_at": "2026-01-02T10:00:00Z", "updated_at": "2026-01-02T10:00:00Z",
             "html_url": format!("{}/acme/issues/issues/12#issuecomment-7001", github.uri())},
            {"event": "labeled", "id": 42, "actor": {"login": "reviewer"},
             "label": {"name": "bug"}, "created_at": "2026-01-03T10:00:00Z"},
            {"event": "subscribed", "id": 43, "created_at": "2026-01-03T11:00:00Z"}
        ])))
        .mount(&github)
        .await;
    let app = spawn_with_github(&github).await;
    let (owner, org) = org_with_domain(&app).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let owner_id: Uuid = sqlx::query_scalar("SELECT owner_user_id FROM workspaces WHERE id = $1")
        .bind(workspace.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    let (status, body) = link(&app, &owner, workspace.id, "acme/issues#12", "primary").await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    let chat = insert_chat(&app, &workspace, owner_id, "2026-01-01T10:00:00Z").await;
    let validation_task = insert_task(
        &app,
        &workspace,
        owner_id,
        "Validation",
        "2026-01-04T10:00:00Z",
    )
    .await;
    let task = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO tasks \
         (id, org_id, workspace_id, name, kind, status, spec, attempt, created_by, created_at, started_at, ended_at) \
         VALUES ($1, $2, $3, 'Preview', 'preview', 'succeeded', $4, 1, $5, \
                 '2026-01-05T09:00:00Z', '2026-01-05T09:00:00Z', '2026-01-05T10:00:00Z')",
    )
    .bind(task)
    .bind(workspace.org_id)
    .bind(workspace.id)
    .bind(json!({"command": {
        "schema_version": "g5.preview-task-spec/v1",
        "module_id": "workspace.preview",
        "module_version": "1.0.0",
        "operation": "start",
        "input_schema": "g5.workspace.preview.start/v1",
        "input": {"profile": "default"}
    }}))
    .bind(owner_id)
    .execute(&app.db)
    .await
    .expect("insert preview task");
    let definition: Uuid = {
        let response = app
            .authenticated_client(&owner)
            .post_json(
                &format!("/api/v1/orgs/{}/approval-definitions", org.id),
                &json!({"name": "Final review", "humanUserIds": [owner.id]}),
            )
            .await;
        assert_eq!(response.status(), StatusCode::CREATED);
        let body: Value = response.json().await.unwrap();
        body["data"]["id"].as_str().unwrap().parse().unwrap()
    };
    let heads = json!([{
        "repoId": workspace.repo_id.to_string(),
        "branchName": "main",
        "workspaceId": workspace.id,
        "headCommit": workspace.head_commit,
    }]);
    let decision: Uuid = sqlx::query_scalar(
        "INSERT INTO approval_statuses \
         (org_id, source_workspace_id, approval_definition_id, approval_definition_version, \
          outcome, comment, actor_type, actor_user_id, reviewed_git_heads, created_at, phase) \
         VALUES ($1, $2, $3, 1, 'approved', 'Looks right', 'human', $4, $5, '2026-01-06T10:00:00Z', 'ie') \
         RETURNING id",
    )
    .bind(org.id)
    .bind(workspace.id)
    .bind(definition)
    .bind(owner.id)
    .bind(&heads)
    .fetch_one(&app.db)
    .await
    .expect("insert approval decision");

    let (status, body) = json_of(
        app.authenticated_client(&owner)
            .get(&activity_path(workspace.id))
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let data = &body["data"];
    assert_eq!(data["primaryIssue"]["displayKey"], "acme/issues#12");
    assert_eq!(data["primaryIssue"]["contentTrusted"], false);
    assert_eq!(data["issueError"], Value::Null);
    assert_eq!(data["issueHistoryTruncated"], false);
    assert_eq!(data["earlierCursor"], Value::Null);
    let all_entries = data["entries"].as_array().unwrap();
    assert_eq!(all_entries.len(), 8);
    assert_eq!(
        all_entries
            .iter()
            .filter(|e| e["kind"] == "workspace_created")
            .count(),
        1
    );
    let entries: Vec<_> = all_entries
        .iter()
        .filter(|e| e["kind"] != "workspace_created")
        .collect();
    let summary: Vec<(String, String, String)> = entries
        .iter()
        .map(|entry| {
            (
                entry["kind"].as_str().unwrap().to_string(),
                entry["source"].as_str().unwrap().to_string(),
                entry["key"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    // Newest first (ui g5.workbench.landing).
    assert_eq!(
        summary,
        vec![
            (
                "approval_decision".into(),
                "workspace".into(),
                format!("approval:{decision}")
            ),
            ("task".into(), "workspace".into(), format!("task:{task}")),
            (
                "task".into(),
                "workspace".into(),
                format!("task:{validation_task}")
            ),
            (
                "issue_event".into(),
                "github".into(),
                "github-event:42".into()
            ),
            (
                "issue_comment".into(),
                "github".into(),
                "github-comment:7001".into()
            ),
            ("chat".into(), "workspace".into(), format!("chat:{chat}")),
            (
                "issue_opened".into(),
                "github".into(),
                format!("github-issue:{}", issue_id(12))
            ),
        ]
    );
    assert_eq!(entries[6]["author"]["login"], "reporter");
    assert_eq!(entries[6]["body"], "Body of 12");
    assert_eq!(
        entries[6]["url"],
        format!("{}/acme/issues/issues/12", github.uri())
    );
    assert_eq!(entries[5]["messageCount"], 4);
    assert_eq!(
        entries[5].get("taskStatus"),
        None,
        "a record chat has no task status"
    );
    assert_eq!(entries[4]["author"]["login"], "reviewer");
    assert_eq!(
        entries[4]["body"],
        "Please also cover two rapid disconnects."
    );
    assert_eq!(entries[3]["event"], "labeled");
    assert_eq!(entries[3]["detail"], "bug");
    assert_eq!(entries[2]["name"], "Validation");
    assert_eq!(entries[1]["taskKind"], "preview");
    assert_eq!(entries[1]["reason"], Value::Null);
    assert_eq!(entries[0]["definitionName"], "Final review");
    assert_eq!(entries[0]["outcome"], "approved");
    assert_eq!(entries[0]["actorName"], owner.display_name);

    let (status, body) = json_of(
        app.authenticated_client(&owner)
            .get(&format!("{}?before=yesterday", activity_path(workspace.id)))
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");

    app.cleanup().await;
}

#[tokio::test]
async fn workspace_activity_pages_and_survives_issue_read_failures() {
    let github = MockServer::start().await;
    mount_github(&github).await;
    // #15's timeline fails; its workspace context must still load.
    Mock::given(method("GET"))
        .and(path(format!(
            "/repositories/{ISSUES_REPO}/issues/15/timeline"
        )))
        .respond_with(ResponseTemplate::new(503))
        .mount(&github)
        .await;
    let app = spawn_with_github(&github).await;
    let (owner, org) = org_with_domain(&app).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let plain = TestWorkspace::create(&app, &org).await;
    let owner_id: Uuid = sqlx::query_scalar("SELECT owner_user_id FROM workspaces WHERE id = $1")
        .bind(workspace.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    let (status, body) = link(&app, &owner, workspace.id, "acme/issues#15", "primary").await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    // 60 tasks, two sharing each minute so ties cross page boundaries.
    for index in 0..60 {
        let at = format!("2026-02-01T10:{:02}:00Z", index / 2);
        insert_task(&app, &workspace, owner_id, &format!("Task {index:02}"), &at).await;
    }
    insert_chat(&app, &plain, owner_id, "2026-02-01T09:00:00Z").await;

    let client = app.authenticated_client(&owner);
    let (status, first) = json_of(client.get(&activity_path(workspace.id)).await).await;
    assert_eq!(status, StatusCode::OK, "{first}");
    assert_eq!(
        first["data"]["issueError"]["issueError"], "unavailable",
        "{first}"
    );
    assert_eq!(
        first["data"]["primaryIssue"]["displayKey"],
        "acme/issues#15"
    );
    let first_entries = first["data"]["entries"].as_array().unwrap().clone();
    assert_eq!(first_entries.len(), 50);
    let cursor = first["data"]["earlierCursor"]
        .as_str()
        .expect("older entries")
        .to_string();

    let (status, second) = json_of(
        client
            .get(&format!(
                "{}?before={}",
                activity_path(workspace.id),
                urlencode(&cursor)
            ))
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{second}");
    assert_eq!(second["data"]["earlierCursor"], Value::Null);
    let second_entries = second["data"]["entries"].as_array().unwrap().clone();
    assert_eq!(second_entries.len(), 11);

    // Newest first within a page and across pages (the later-fetched page is
    // the older one).
    let mut keys: Vec<String> = first_entries
        .iter()
        .chain(second_entries.iter())
        .map(|entry| entry["key"].as_str().unwrap().to_string())
        .collect();
    let in_order = keys.clone();
    keys.sort();
    keys.dedup();
    assert_eq!(
        keys.len(),
        61,
        "jobs and workspace creation appear once across pages"
    );
    let times: Vec<&str> = first_entries
        .iter()
        .chain(second_entries.iter())
        .map(|entry| entry["at"].as_str().unwrap())
        .collect();
    assert!(
        times.windows(2).all(|pair| pair[0] >= pair[1]),
        "{in_order:?}"
    );

    // Without a primary issue the page is workspace context only.
    let (status, body) = json_of(client.get(&activity_path(plain.id)).await).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["primaryIssue"], Value::Null);
    assert_eq!(body["data"]["issueError"], Value::Null);
    assert_eq!(body["data"]["entries"].as_array().unwrap().len(), 2);
    assert_eq!(body["data"]["entries"][0]["kind"], "workspace_created");
    assert_eq!(body["data"]["entries"][1]["kind"], "chat");

    app.cleanup().await;
}

fn git_at(repo_path: &std::path::Path, date: &str, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(repo_path)
        .env("GIT_AUTHOR_DATE", date)
        .env("GIT_COMMITTER_DATE", date)
        .env("GIT_AUTHOR_NAME", "Ada Dev")
        .env("GIT_AUTHOR_EMAIL", "ada@example.test")
        .env("GIT_COMMITTER_NAME", "Ada Dev")
        .env("GIT_COMMITTER_EMAIL", "ada@example.test")
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

/// Commits on the workspace branch since its parent merge base appear in
/// Activity at their commit time, read from G5's git.
#[tokio::test]
async fn workspace_activity_lists_branch_commits_since_the_base() {
    let github = MockServer::start().await;
    mount_github(&github).await;
    let app = spawn_with_github(&github).await;
    let (owner, org) = org_with_domain(&app).await;
    let workspace = TestWorkspace::create(&app, &org).await;

    // A repository whose workspace branch starts at the seed commit.
    let repo_id = Uuid::new_v4();
    let repo_path = app.init_repo(org.id, repo_id).await;
    let seed_date = "2026-03-01T09:00:00Z";
    git_at(&repo_path, seed_date, &["init"]);
    git_at(&repo_path, seed_date, &["checkout", "-B", "main"]);
    std::fs::write(repo_path.join("README.md"), "seed\n").unwrap();
    git_at(&repo_path, seed_date, &["add", "README.md"]);
    git_at(&repo_path, seed_date, &["commit", "-m", "Seed"]);
    let base = git_at(&repo_path, seed_date, &["rev-parse", "HEAD"]);
    sqlx::query(
        "INSERT INTO repos (id, org_id, name, slug, git_url, default_branch, provider) \
         VALUES ($1, $2, 'web', 'web', $3, 'main', 'github')",
    )
    .bind(repo_id)
    .bind(org.id)
    .bind(repo_path.display().to_string())
    .execute(&app.db)
    .await
    .unwrap();
    let parent_id = insert_workspace(&app, org.id, owner.id, "activity-parent", false).await;
    helpers::seed_branch_ref(&app, repo_id, "activity-parent", &base).await;
    sqlx::query("UPDATE workspaces SET cloned_from_id = $2 WHERE id = $1")
        .bind(workspace.id)
        .bind(parent_id)
        .execute(&app.db)
        .await
        .unwrap();
    sqlx::query("DELETE FROM workspace_repos WHERE workspace_id = $1")
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .unwrap();
    for workspace_id in [parent_id, workspace.id] {
        sqlx::query(
            "INSERT INTO workspace_repos (org_id, workspace_id, repo_id, root_path, role) \
             VALUES ($1, $2, $3, '', 'code')",
        )
        .bind(org.id)
        .bind(workspace_id)
        .bind(repo_id)
        .execute(&app.db)
        .await
        .unwrap();
    }

    // Two commits reach the branch after it was created.
    std::fs::write(repo_path.join("README.md"), "one\n").unwrap();
    git_at(
        &repo_path,
        "2026-03-02T10:00:00Z",
        &["commit", "-am", "Fix reconnect"],
    );
    std::fs::write(repo_path.join("README.md"), "two\n").unwrap();
    git_at(
        &repo_path,
        "2026-03-03T10:00:00Z",
        &["commit", "-am", "Add regression test"],
    );
    let head = git_at(&repo_path, seed_date, &["rev-parse", "HEAD"]);
    let first = git_at(&repo_path, seed_date, &["rev-parse", "HEAD~1"]);
    helpers::seed_branch_ref(&app, repo_id, &workspace.name, &head).await;
    let (status, body) = json_of(
        app.authenticated_client(&owner)
            .get(&activity_path(workspace.id))
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let commits: Vec<&Value> = body["data"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|entry| entry["kind"] == "commit")
        .collect();
    assert_eq!(commits.len(), 2, "{body}");
    // Newest first.
    assert_eq!(commits[1]["key"], format!("commit:{repo_id}:{first}"));
    assert_eq!(commits[1]["summary"], "Fix reconnect");
    assert_eq!(commits[1]["at"], "2026-03-02T10:00:00Z");
    assert_eq!(commits[0]["summary"], "Add regression test");
    assert_eq!(commits[0]["sha"], head);
    assert_eq!(commits[0]["shortSha"], head[..7]);
    assert_eq!(commits[0]["authorName"], "Ada Dev");
    assert_eq!(commits[0]["repoName"], "web");
    assert_eq!(commits[0]["branchName"], workspace.name);
    assert_eq!(commits[0]["repoId"], repo_id.to_string());
    assert!(commits[0].get("branchId").is_none());
    assert_eq!(commits[0]["source"], "workspace");

    app.cleanup().await;
}

// ─── Comments from Activity ──────────────────────────────────────────────────

fn no_redirect_client() -> reqwest::Client {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("client")
}

fn redirect_location(response: &reqwest::Response) -> String {
    response
        .headers()
        .get(reqwest::header::LOCATION)
        .expect("redirect location")
        .to_str()
        .unwrap()
        .to_string()
}

fn pkce_cookie(response: &reqwest::Response) -> String {
    let cookie = response
        .headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find(|value| value.starts_with("__Host-g5_github_app_pkce="))
        .expect("PKCE cookie");
    helpers::assert_host_cookie(cookie);
    cookie.split(';').next().unwrap().to_string()
}

fn url_query_param(url: &str, name: &str) -> String {
    url::Url::parse(url)
        .expect("absolute URL")
        .query_pairs()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.into_owned())
        .unwrap_or_else(|| panic!("{name} missing from {url}"))
}

const GITHUB_USER_ID: u64 = 4242;
const GITHUB_LOGIN: &str = "ada-gh";

/// GitHub's side of "Connect GitHub" for `code`: the token exchange and the
/// authenticated user.
async fn mount_user_authorization(github: &MockServer, code: &str, token: &str, expires_in: i64) {
    Mock::given(method("POST"))
        .and(path("/login/oauth/access_token"))
        .and(body_string_contains(format!("code={code}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": token,
            "expires_in": expires_in,
            "refresh_token": "ghr_refresh_1",
            "refresh_token_expires_in": 15_897_600,
            "token_type": "bearer",
        })))
        .mount(github)
        .await;
    Mock::given(method("GET"))
        .and(path("/user"))
        .and(header("authorization", format!("Bearer {token}").as_str()))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"id": GITHUB_USER_ID, "login": GITHUB_LOGIN})),
        )
        .mount(github)
        .await;
}

/// Start "Connect GitHub" as `user`; returns the signed state and the PKCE
/// cookie.
async fn start_user_connection(
    app: &TestApp,
    user: &TestUser,
    return_path: &str,
) -> (String, String) {
    let response = app
        .authenticated_client(user)
        .post_json(
            "/api/v1/github-app/user-connection",
            &json!({"returnPath": return_path}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let cookie = pkce_cookie(&response);
    let body: Value = response.json().await.unwrap();
    let authorize = body["data"]["redirectUrl"].as_str().unwrap().to_string();
    assert!(authorize.contains("/login/oauth/authorize"), "{authorize}");
    assert_eq!(url_query_param(&authorize, "code_challenge_method"), "S256");
    (url_query_param(&authorize, "state"), cookie)
}

async fn user_callback(app: &TestApp, state: &str, code: &str, cookie: &str) -> reqwest::Response {
    let mut request = no_redirect_client()
        .get(format!("{}/api/v1/github-app/oauth/callback", app.base_url))
        .query(&[("code", code), ("state", state)]);
    if !cookie.is_empty() {
        request = request.header(reqwest::header::COOKIE, cookie);
    }
    request.send().await.unwrap()
}

async fn connect_github(
    app: &TestApp,
    github: &MockServer,
    user: &TestUser,
    token: &str,
    expires_in: i64,
) {
    let code = format!("code-{}", Uuid::new_v4());
    mount_user_authorization(github, &code, token, expires_in).await;
    let (state, cookie) = start_user_connection(app, user, "/acme/ws?view=activity").await;
    let response = user_callback(app, &state, &code, &cookie).await;
    assert_eq!(
        redirect_location(&response),
        "/acme/ws?view=activity&githubConnection=connected"
    );
}

/// The after-login offer (g5.settings.github-connection) is decided for the
/// named organization: a disconnected owner of an organization with a GitHub
/// issue domain is offered it; a viewer, a connected person and a non-member
/// are not. A status read that names no organization stays unchanged.
#[tokio::test]
async fn after_login_offer_follows_connection_role_and_membership() {
    let github = MockServer::start().await;
    mount_github(&github).await;
    let app = spawn_with_github(&github).await;
    let (owner, org) = org_with_domain(&app).await;
    let viewer = TestUser::create_with_scopes(&app, org.id, &[Scope::UsersRead]).await;
    sqlx::query("UPDATE org_memberships SET role = 'viewer' WHERE org_id = $1 AND user_id = $2")
        .bind(org.id)
        .bind(viewer.id)
        .execute(&app.db)
        .await
        .unwrap();
    let mut outsider = TestUser::create(&app).await;
    TestOrg::create_with_admin(&app, &mut outsider).await;
    let offer_path = format!(
        "/api/v1/github-app/user-connection?afterLoginOrgId={}",
        org.id
    );

    let (status, body) = json_of(app.authenticated_client(&owner).get(&offer_path).await).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["data"],
        json!({"connected": false, "githubLogin": null, "offerAfterLogin": true})
    );
    let (_, body) = json_of(
        app.authenticated_client(&owner)
            .get("/api/v1/github-app/user-connection")
            .await,
    )
    .await;
    assert_eq!(
        body["data"],
        json!({"connected": false, "githubLogin": null})
    );

    let (status, body) = json_of(app.authenticated_client(&viewer).get(&offer_path).await).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["offerAfterLogin"], json!(false));

    let (status, body) = json_of(app.authenticated_client(&outsider).get(&offer_path).await).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");

    connect_github(&app, &github, &owner, "ghu_person", 28_800).await;
    let (status, body) = json_of(app.authenticated_client(&owner).get(&offer_path).await).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["offerAfterLogin"], json!(false));

    app.cleanup().await;
}

/// A repository makes its organization use GitHub only on the configured
/// GitHub origin (g5.settings.github-connection), as publication decides it:
/// a manually added repository without a stored provider counts from its URL,
/// while GitLab, G5-local and provider/URL-mismatched repositories do not.
#[tokio::test]
async fn after_login_offer_counts_repositories_on_the_github_origin() {
    let github = MockServer::start().await;
    let mut config = test_config();
    let mut github_app = github_app_config(&github);
    github_app.web_base_url = "https://github.com".into();
    config.github_app = Some(github_app);
    let app = TestApp::spawn_with_config(config).await;
    let mut owner = TestUser::create(&app).await;
    let github_org = TestOrg::create_with_admin(&app, &mut owner).await;
    let other_org = TestOrg::create_with_admin(&app, &mut owner).await;
    for (org_id, slug, git_url, provider) in [
        (
            github_org.id,
            "web",
            "https://github.com/acme/web.git",
            None,
        ),
        (
            other_org.id,
            "lab",
            "https://gitlab.com/acme/lab.git",
            Some("gitlab"),
        ),
        (other_org.id, "local", "g5-local://workspaces/x.git", None),
        (
            other_org.id,
            "odd",
            "https://gitlab.com/acme/odd.git",
            Some("github"),
        ),
    ] {
        sqlx::query(
            "INSERT INTO repos (org_id, name, slug, git_url, provider) VALUES ($1, $2, $2, $3, $4)",
        )
        .bind(org_id)
        .bind(slug)
        .bind(git_url)
        .bind(provider)
        .execute(&app.db)
        .await
        .unwrap();
    }

    for (org_id, expected) in [(github_org.id, true), (other_org.id, false)] {
        let (status, body) = json_of(
            app.authenticated_client(&owner)
                .get(&format!(
                    "/api/v1/github-app/user-connection?afterLoginOrgId={org_id}"
                ))
                .await,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["data"]["offerAfterLogin"], json!(expected), "{org_id}");
    }

    app.cleanup().await;
}

#[tokio::test]
async fn connect_github_stores_the_person_s_grant_encrypted() {
    let github = MockServer::start().await;
    mount_github(&github).await;
    mount_user_authorization(&github, "good-code", "ghu_person", 28_800).await;
    let app = spawn_with_github(&github).await;
    let (owner, _org) = org_with_domain(&app).await;
    let client = app.authenticated_client(&owner);

    let (status, body) = json_of(client.get("/api/v1/github-app/user-connection").await).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["data"],
        json!({"connected": false, "githubLogin": null})
    );

    // A cookie from another attempt, or none, cannot finish it.
    let (state, cookie) = start_user_connection(&app, &owner, "/acme/ws?view=activity").await;
    let (_, other_cookie) = start_user_connection(&app, &owner, "/acme/ws").await;
    let before = github_connection_snapshot(&app, owner.id).await;
    let provider_calls = github.received_requests().await.unwrap().len();
    let legacy = cookie.trim_start_matches("__Host-");
    for bad_cookie in [
        String::new(),
        other_cookie.clone(),
        legacy.to_string(),
        format!("{cookie}; {other_cookie}"),
        format!("{other_cookie}; {cookie}"),
        format!("{cookie}; {cookie}"),
        format!("__Host-g5_github_app_pkce=; {cookie}"),
    ] {
        let response = user_callback(&app, &state, "good-code", &bad_cookie).await;
        assert_eq!(
            redirect_location(&response),
            "/acme/ws?view=activity&githubConnection=failed"
        );
        helpers::assert_host_cookie(response.headers()["set-cookie"].to_str().unwrap());
        assert_eq!(github_connection_snapshot(&app, owner.id).await, before);
        assert_eq!(
            github.received_requests().await.unwrap().len(),
            provider_calls,
            "ambiguous or legacy PKCE cookies must fail before provider exchange"
        );
    }
    let response = user_callback(&app, &state, "good-code", &format!("{legacy}; {cookie}")).await;
    assert_eq!(
        redirect_location(&response),
        "/acme/ws?view=activity&githubConnection=connected"
    );
    let expirations: Vec<_> = response
        .headers()
        .get_all("set-cookie")
        .iter()
        .map(|v| v.to_str().unwrap())
        .collect();
    assert!(expirations
        .iter()
        .any(|v| v.starts_with("__Host-g5_github_app_pkce=;") && v.contains("Max-Age=0")));
    assert!(expirations
        .iter()
        .any(|v| v.starts_with("g5_github_app_pkce=;")
            && v.contains("Path=/api/v1/github-app;")
            && v.contains("Max-Age=0")));
    // The attempt is consumed.
    let replay = user_callback(&app, &state, "good-code", &cookie).await;
    assert_eq!(redirect_location(&replay), "/?githubConnection=failed");

    let (_, body) = json_of(client.get("/api/v1/github-app/user-connection").await).await;
    assert_eq!(
        body["data"],
        json!({"connected": true, "githubLogin": GITHUB_LOGIN})
    );
    let (stored, refresh, expires): (
        Vec<u8>,
        Option<Vec<u8>>,
        Option<chrono::DateTime<chrono::Utc>>,
    ) = sqlx::query_as(
        "SELECT access_token_enc, refresh_token_enc, access_token_expires_at \
             FROM github_app_user_grants WHERE user_id = $1",
    )
    .bind(owner.id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(!stored.windows(10).any(|window| window == b"ghu_person"));
    assert!(refresh.is_some());
    assert!(expires.is_some());

    // Return paths must stay inside G5.
    let response = client
        .post_json(
            "/api/v1/github-app/user-connection",
            &json!({"returnPath": "//evil.example/x"}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let (status, body) = json_of(
        client
            .post_json("/api/v1/github-app/user-connection/disconnect", &json!({}))
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["connected"], false);

    app.cleanup().await;
}

/// Creates a `g5k_` API key in `org_id` for `user` and returns its plaintext.
async fn create_api_key(app: &TestApp, user: &TestUser, org_id: Uuid, scopes: &[&str]) -> String {
    let (status, body) = json_of(
        app.authenticated_client(user)
            .post_json(
                &format!("/api/v1/orgs/{org_id}/api-keys"),
                &json!({"name": "automation key", "scopes": scopes}),
            )
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["data"]["key"].as_str().unwrap().to_string()
}

/// The person's grant and every connection attempt they have started.
async fn github_connection_snapshot(app: &TestApp, user_id: Uuid) -> Value {
    sqlx::query_scalar(
        "SELECT jsonb_build_object( \
           'grant', (SELECT to_jsonb(g) FROM github_app_user_grants g WHERE user_id = $1), \
           'attempts', (SELECT jsonb_agg(to_jsonb(a) ORDER BY id) \
                        FROM github_app_user_connection_attempts a WHERE user_id = $1))",
    )
    .bind(user_id)
    .fetch_one(&app.db)
    .await
    .unwrap()
}

/// A person's GitHub connection is account-wide, so an organization API key
/// cannot read, start replacing or forget it, whatever its organization or
/// scopes (#579). The person's own session keeps the connection in every org.
#[tokio::test]
async fn api_keys_cannot_touch_the_person_s_github_connection() {
    let github = MockServer::start().await;
    mount_github(&github).await;
    let app = spawn_with_github(&github).await;
    let (mut owner, org_a) = org_with_domain(&app).await;
    connect_github(&app, &github, &owner, "ghu_person", 28_800).await;
    let org_b = TestOrg::create_with_admin(&app, &mut owner).await;
    let all_scopes: Vec<&str> = Scope::all()
        .iter()
        .filter(|scope| **scope != Scope::WorkerHost)
        .map(|scope| scope.as_str())
        .collect();
    let keys = [
        (
            "org A zero-scope",
            create_api_key(&app, &owner, org_a.id, &[]).await,
        ),
        (
            "org A admin",
            create_api_key(&app, &owner, org_a.id, &[Scope::OrgAdmin.as_str()]).await,
        ),
        (
            "org A full-scope",
            create_api_key(&app, &owner, org_a.id, &all_scopes).await,
        ),
        (
            "org B full-scope",
            create_api_key(&app, &owner, org_b.id, &all_scopes).await,
        ),
    ];
    let before = github_connection_snapshot(&app, owner.id).await;
    assert!(!before["grant"].is_null());
    let github_requests = github.received_requests().await.unwrap().len();

    let routes = [
        (
            reqwest::Method::GET,
            "/api/v1/github-app/user-connection",
            json!({}),
        ),
        (
            reqwest::Method::POST,
            "/api/v1/github-app/user-connection",
            json!({"returnPath": "/acme/ws"}),
        ),
        (
            reqwest::Method::POST,
            "/api/v1/github-app/user-connection/disconnect",
            json!({}),
        ),
    ];
    for (label, key) in &keys {
        for (method, route, body) in &routes {
            let response = app
                .client
                .request(method.clone(), format!("{}{route}", app.base_url))
                .bearer_auth(key)
                .json(body)
                .send()
                .await
                .unwrap();
            assert!(
                response
                    .headers()
                    .get_all(reqwest::header::SET_COOKIE)
                    .iter()
                    .all(|value| !value.as_bytes().starts_with(b"__Host-g5_github_app_pkce=")),
                "{label} {method} {route} must not receive a PKCE cookie"
            );
            let (status, error) = json_of(response).await;
            assert_eq!(
                status,
                StatusCode::FORBIDDEN,
                "{label} {method} {route}: {error}"
            );
            assert_eq!(
                error["error"]["details"]["missing_scopes"],
                json!(["user_session_required"]),
                "{label} {method} {route}"
            );
            assert_eq!(
                github_connection_snapshot(&app, owner.id).await,
                before,
                "{label} {method} {route} must leave the grant and attempts unchanged"
            );
        }
    }
    assert_eq!(
        github.received_requests().await.unwrap().len(),
        github_requests,
        "keys must not reach GitHub"
    );

    // The person's session, now selecting org B, still sees the connection.
    let (status, body) = json_of(
        app.authenticated_client(&owner)
            .get("/api/v1/github-app/user-connection")
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["data"],
        json!({"connected": true, "githubLogin": GITHUB_LOGIN})
    );

    app.cleanup().await;
}

/// The issue repository reported as `visibility`.
async fn mount_repository_visibility(github: &MockServer, visibility: &str) {
    Mock::given(method("GET"))
        .and(path(format!("/repositories/{ISSUES_REPO}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": ISSUES_REPO,
            "full_name": "acme/issues",
            "visibility": visibility,
            "private": visibility != "public",
        })))
        .with_priority(1)
        .expect(1..)
        .named(format!("repository visibility {visibility}"))
        .mount(github)
        .await;
}

/// Issue #12 with no timeline activity, so Activity can read it.
async fn mount_empty_timeline(github: &MockServer) {
    Mock::given(method("GET"))
        .and(path(format!(
            "/repositories/{ISSUES_REPO}/issues/12/timeline"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(github)
        .await;
}

async fn enable_comments(app: &TestApp, owner: &TestUser, org_id: Uuid) {
    let domain_id: Uuid = sqlx::query_scalar("SELECT id FROM issue_domains WHERE org_id = $1")
        .bind(org_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    let (status, body) = json_of(
        app.authenticated_client(owner)
            .patch_json(
                &format!("{}/{domain_id}", domains_path(org_id)),
                &json!({"commentsEnabled": true}),
            )
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["commentsEnabled"], true);
    assert_eq!(body["data"]["contentTrusted"], false);
}

fn comments_path(workspace_id: Uuid) -> String {
    format!("/api/v1/workspaces/{workspace_id}/activity/comments")
}

async fn post_comment(
    app: &TestApp,
    user: &TestUser,
    workspace_id: Uuid,
    body: &str,
    key: &str,
    destination: &Value,
) -> (StatusCode, Value) {
    json_of(
        app.authenticated_client(user)
            .post_json(
                &comments_path(workspace_id),
                &json!({"body": body, "idempotencyKey": key, "destination": destination}),
            )
            .await,
    )
    .await
}

#[tokio::test]
async fn comments_post_as_the_person_only_to_approved_non_public_destinations() {
    let github = MockServer::start().await;
    mount_github(&github).await;
    mount_empty_timeline(&github).await;
    let app = spawn_with_github(&github).await;
    let (owner, org) = org_with_domain(&app).await;
    let workspace = insert_workspace(&app, org.id, owner.id, "commenting", false).await;
    let (status, body) = link(&app, &owner, workspace, "acme/issues#12", "primary").await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let tracker_id: Uuid = sqlx::query_scalar("SELECT id FROM issue_trackers WHERE org_id = $1")
        .bind(org.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    let destination = json!({"trackerId": tracker_id, "externalIssueId": issue_id(12).to_string()});

    // Comments are off until an admin enables them for the domain.
    let (status, body) =
        post_comment(&app, &owner, workspace, "Looks right.", "c-1", &destination).await;
    assert_conflict_code(status, &body, "comments_disabled");
    let (_, activity) = json_of(
        app.authenticated_client(&owner)
            .get(&activity_path(workspace))
            .await,
    )
    .await;
    assert_eq!(
        activity["data"]["composer"]["state"], "comments_disabled",
        "{activity}"
    );
    enable_comments(&app, &owner, org.id).await;

    // mount_github's repository has no visibility: unknown blocks writes.
    let (status, body) =
        post_comment(&app, &owner, workspace, "Looks right.", "c-1", &destination).await;
    assert_conflict_code(status, &body, "visibility_unknown");

    // Public repositories never receive G5 comments.
    mount_repository_visibility(&github, "public").await;
    let (status, body) =
        post_comment(&app, &owner, workspace, "Looks right.", "c-1", &destination).await;
    assert_conflict_code(status, &body, "public_destination");
    github.reset().await;
    mount_github(&github).await;
    mount_empty_timeline(&github).await;
    mount_repository_visibility(&github, "private").await;

    // Secrets are refused before anything is recorded or sent.
    let secret = format!("token ghp_{}", "a".repeat(36));
    let (status, body) = post_comment(&app, &owner, workspace, &secret, "c-1", &destination).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["error"]["details"]["code"], "secret_detected");
    assert!(!body.to_string().contains("ghp_"), "{body}");

    // Without a GitHub connection the person is asked to connect.
    let (status, body) =
        post_comment(&app, &owner, workspace, "Looks right.", "c-1", &destination).await;
    assert_conflict_code(status, &body, "github_connection_required");
    let (_, activity) = json_of(
        app.authenticated_client(&owner)
            .get(&activity_path(workspace))
            .await,
    )
    .await;
    assert_eq!(activity["data"]["composer"]["state"], "connection_required");
    assert_eq!(activity["data"]["composer"]["destination"], destination);

    // A token about to expire is renewed before posting.
    connect_github(&app, &github, &owner, "ghu_expiring", 60).await;
    Mock::given(method("POST"))
        .and(path("/login/oauth/access_token"))
        .and(body_string_contains("grant_type=refresh_token"))
        .and(body_string_contains("refresh_token=ghr_refresh_1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "ghu_renewed",
            "expires_in": 28_800,
            "refresh_token": "ghr_refresh_2",
            "refresh_token_expires_in": 15_897_600,
        })))
        .expect(1)
        .mount(&github)
        .await;
    Mock::given(method("POST"))
        .and(path(format!(
            "/repositories/{ISSUES_REPO}/issues/12/comments"
        )))
        .and(header("authorization", "Bearer ghu_renewed"))
        .and(body_string_contains("Looks right."))
        .and(body_string_contains("<!-- g5-comment "))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "id": 9001,
            "user": {"login": GITHUB_LOGIN},
            "body": "Looks right.",
            "created_at": "2026-09-26T20:00:00Z",
            "updated_at": "2026-09-26T20:00:00Z",
            "html_url": format!("{}/acme/issues/issues/12#issuecomment-9001", github.uri()),
        })))
        .expect(1)
        .mount(&github)
        .await;

    // The primary issue moved: the author must review the new destination.
    let stale = json!({"trackerId": tracker_id, "externalIssueId": issue_id(15).to_string()});
    let (status, body) = post_comment(&app, &owner, workspace, "Looks right.", "c-1", &stale).await;
    assert_conflict_code(status, &body, "destination_changed");

    let (status, posted) =
        post_comment(&app, &owner, workspace, "Looks right.", "c-1", &destination).await;
    assert_eq!(status, StatusCode::CREATED, "{posted}");
    let data = &posted["data"];
    assert_eq!(data["state"], "published", "{posted}");
    assert_eq!(data["id"], "9001");
    assert_eq!(data["githubLogin"], GITHUB_LOGIN);
    assert_eq!(data["body"], "Looks right.");
    let payload = data["payload"].as_str().unwrap().to_string();
    assert!(
        payload.starts_with("Looks right.\n\n<!-- g5-comment ") && payload.ends_with(" -->"),
        "{payload}"
    );
    assert_eq!(
        data["externalCommentUrl"],
        format!("{}/acme/issues/issues/12#issuecomment-9001", github.uri())
    );

    // A retry with the same key finds the comment by its marker on the issue
    // and does not send again (the comment mock expects exactly one call).
    mount_issue_comments(
        &github,
        12,
        json!([github_comment(
            &github,
            9001,
            ME,
            &payload,
            12,
            "2026-09-26T20:00:00Z"
        )]),
    )
    .await;
    let (status, replay) =
        post_comment(&app, &owner, workspace, "Looks right.", "c-1", &destination).await;
    assert_eq!(status, StatusCode::OK, "{replay}");
    assert_eq!(replay["data"]["id"], "9001");
    assert_eq!(replay["data"]["state"], "published");
    // The key names that text; other text under it is refused.
    let (status, body) = post_comment(
        &app,
        &owner,
        workspace,
        "Changed my mind.",
        "c-1",
        &destination,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(
        body["error"]["details"]["field"], "idempotencyKey",
        "{body}"
    );
    let (_, activity) = json_of(
        app.authenticated_client(&owner)
            .get(&activity_path(workspace))
            .await,
    )
    .await;
    assert_eq!(activity["data"]["composer"]["state"], "ready");
    assert_eq!(activity["data"]["composer"]["githubLogin"], GITHUB_LOGIN);
    assert!(
        activity["data"].get("myPendingComments").is_none(),
        "{activity}"
    );

    github.verify().await;
    app.cleanup().await;
}

/// No comments on issue `number` yet, where mount_github lists none: what a
/// comment's marker lookup reads before posting.
async fn mount_no_issue_comments(github: &MockServer, number: u64) {
    Mock::given(method("GET"))
        .and(path(format!(
            "/repositories/{ISSUES_REPO}/issues/{number}/comments"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .with_priority(3)
        .mount(github)
        .await;
}

/// Comments on issue `number` GitHub lists (any page), ahead of mount_github's.
async fn mount_issue_comments(github: &MockServer, number: u64, comments: Value) {
    Mock::given(method("GET"))
        .and(path(format!(
            "/repositories/{ISSUES_REPO}/issues/{number}/comments"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(comments))
        .with_priority(2)
        .mount(github)
        .await;
}

/// `g5.issue.comment-recovery`: nothing is recorded. An unclear send is
/// reported as such, and posting again with the same key looks for the
/// marker first, through a busy issue's pages, so it is not sent twice.
#[tokio::test]
async fn an_unclear_comment_is_found_by_its_marker_when_posted_again() {
    let github = MockServer::start().await;
    mount_github(&github).await;
    mount_repository_visibility(&github, "private").await;
    let app = spawn_with_github(&github).await;
    let (owner, org) = org_with_domain(&app).await;
    enable_comments(&app, &owner, org.id).await;
    let workspace = insert_workspace(&app, org.id, owner.id, "ambiguous", false).await;
    let (status, body) = link(&app, &owner, workspace, "acme/issues#12", "primary").await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    // A reviewer who does not own the workspace can comment.
    let reviewer = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    connect_github(&app, &github, &reviewer, "ghu_reviewer", 28_800).await;
    let tracker_id: Uuid = sqlx::query_scalar("SELECT id FROM issue_trackers WHERE org_id = $1")
        .bind(org.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    let destination = json!({"trackerId": tracker_id, "externalIssueId": issue_id(12).to_string()});
    Mock::given(method("POST"))
        .and(path(format!(
            "/repositories/{ISSUES_REPO}/issues/12/comments"
        )))
        .and(header("authorization", "Bearer ghu_reviewer"))
        .respond_with(ResponseTemplate::new(502))
        .expect(1)
        .mount(&github)
        .await;

    let (status, posted) = post_comment(
        &app,
        &reviewer,
        workspace,
        "Please retest.",
        "r-1",
        &destination,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{posted}");
    assert_eq!(posted["data"]["state"], "outcome_unknown", "{posted}");
    assert!(posted["data"]["message"].is_string(), "{posted}");
    let payload = posted["data"]["payload"].as_str().unwrap().to_string();
    // Nothing is kept for later: Activity lists no pending comment.
    let (_, activity) = json_of(
        app.authenticated_client(&reviewer)
            .get(&activity_path(workspace))
            .await,
    )
    .await;
    assert!(
        activity["data"].get("myPendingComments").is_none(),
        "{activity}"
    );

    // GitHub did post it: a busy issue with a full first page of other
    // comments updated in the window, and G5's comment on the second page.
    let unrelated: Vec<Value> = (0..100)
        .map(|index| {
            github_comment(
                &github,
                8000 + index,
                (5000, "someone"),
                "unrelated",
                12,
                "2026-09-26T20:00:00Z",
            )
        })
        .collect();
    Mock::given(method("GET"))
        .and(path(format!(
            "/repositories/{ISSUES_REPO}/issues/12/comments"
        )))
        .and(query_param("page", "1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(Value::Array(unrelated)))
        .with_priority(1)
        .mount(&github)
        .await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/repositories/{ISSUES_REPO}/issues/12/comments"
        )))
        .and(query_param("page", "2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"id": 9101, "user": {"login": GITHUB_LOGIN},
             "body": payload,
             "created_at": "2026-09-26T20:00:01Z", "updated_at": "2026-09-26T20:00:01Z",
             "html_url": format!("{}/acme/issues/issues/12#issuecomment-9101", github.uri())}
        ])))
        .with_priority(1)
        .mount(&github)
        .await;
    // Posting again with the same key finds it and sends nothing (the POST
    // mock expects one call).
    let (status, found) = post_comment(
        &app,
        &reviewer,
        workspace,
        "Please retest.",
        "r-1",
        &destination,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{found}");
    assert_eq!(found["data"]["state"], "published", "{found}");
    assert_eq!(found["data"]["id"], "9101");
    assert_eq!(
        found["data"]["externalCommentUrl"],
        format!("{}/acme/issues/issues/12#issuecomment-9101", github.uri())
    );
    // The marker belongs to the author: another person's same key is a
    // different submission, refused here only by the comment guards.
    let (status, body) = post_comment(
        &app,
        &owner,
        workspace,
        "Please retest.",
        "r-1",
        &destination,
    )
    .await;
    assert_conflict_code(status, &body, "github_connection_required");

    github.verify().await;
    app.cleanup().await;
}

/// The connected person's GitHub account.
const ME: (u64, &str) = (GITHUB_USER_ID, GITHUB_LOGIN);

fn github_comment(
    github: &MockServer,
    id: u64,
    (user_id, login): (u64, &str),
    body: &str,
    issue: u64,
    updated_at: &str,
) -> Value {
    json!({
        "id": id,
        "user": {"id": user_id, "login": login},
        "body": body,
        "created_at": "2026-09-26T20:00:00Z",
        "updated_at": updated_at,
        "html_url": format!("{}/acme/issues/issues/{issue}#issuecomment-{id}", github.uri()),
        "issue_url": format!("{}/repos/acme/issues/issues/{issue}", github.uri()),
    })
}

async fn mount_github_comment(github: &MockServer, comment: Value) {
    let id = comment["id"].as_u64().unwrap();
    Mock::given(method("GET"))
        .and(path(format!(
            "/repositories/{ISSUES_REPO}/issues/comments/{id}"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(comment))
        .mount(github)
        .await;
}

#[tokio::test]
async fn edits_replace_only_the_person_s_own_unchanged_comment() {
    let github = MockServer::start().await;
    mount_github(&github).await;
    mount_empty_timeline(&github).await;
    mount_repository_visibility(&github, "private").await;
    let app = spawn_with_github(&github).await;
    let (owner, org) = org_with_domain(&app).await;
    enable_comments(&app, &owner, org.id).await;
    let workspace = insert_workspace(&app, org.id, owner.id, "editing", false).await;
    let (status, body) = link(&app, &owner, workspace, "acme/issues#12", "primary").await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    connect_github(&app, &github, &owner, "ghu_editor", 28_800).await;
    let tracker_id: Uuid = sqlx::query_scalar("SELECT id FROM issue_trackers WHERE org_id = $1")
        .bind(org.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    let destination = json!({"trackerId": tracker_id, "externalIssueId": issue_id(12).to_string()});
    let reference = "<!-- g5-comment cbc40edc-c5ed-4f83-ba43-85069252d6b6 -->";
    let seen = "2026-09-26T20:00:00Z";
    mount_github_comment(
        &github,
        github_comment(
            &github,
            9001,
            ME,
            &format!("Old text\n\n{reference}"),
            12,
            seen,
        ),
    )
    .await;
    mount_github_comment(
        &github,
        github_comment(&github, 9002, (5001, "someone-else"), "Theirs", 12, seen),
    )
    .await;
    mount_github_comment(
        &github,
        github_comment(&github, 9003, ME, "Elsewhere", 15, seen),
    )
    .await;
    mount_github_comment(
        &github,
        github_comment(&github, 9004, ME, "Flaky", 12, seen),
    )
    .await;
    // The person's old login, renamed and since claimed by another account.
    mount_github_comment(
        &github,
        github_comment(&github, 9005, (5002, GITHUB_LOGIN), "Not mine", 12, seen),
    )
    .await;
    // An earlier save whose answer was lost, but which GitHub applied.
    mount_github_comment(
        &github,
        github_comment(&github, 9006, ME, "New text", 12, "2026-09-26T21:30:00Z"),
    )
    .await;
    Mock::given(method("PATCH"))
        .and(path(format!(
            "/repositories/{ISSUES_REPO}/issues/comments/9006"
        )))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&github)
        .await;
    Mock::given(method("PATCH"))
        .and(path(format!(
            "/repositories/{ISSUES_REPO}/issues/comments/9001"
        )))
        .and(header("authorization", "Bearer ghu_editor"))
        .and(body_json(
            json!({"body": format!("New text\n\n{reference}")}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(github_comment(
            &github,
            9001,
            ME,
            &format!("New text\n\n{reference}"),
            12,
            "2026-09-26T21:00:00Z",
        )))
        .expect(1)
        .mount(&github)
        .await;
    Mock::given(method("PATCH"))
        .and(path(format!(
            "/repositories/{ISSUES_REPO}/issues/comments/9004"
        )))
        .respond_with(ResponseTemplate::new(502))
        .mount(&github)
        .await;
    let client = app.authenticated_client(&owner);
    let edit = |comment: u64, key: &str, expected: &str| {
        let body = json!({
            "body": "New text",
            "idempotencyKey": key,
            "destination": destination.clone(),
            "expectedUpdatedAt": expected,
        });
        let url = format!("{}/github/{comment}", comments_path(workspace));
        let client = &client;
        async move { json_of(client.patch_json(&url, &body).await).await }
    };

    // Changed on GitHub since the editor opened.
    let (status, body) = edit(9001, "e-2", "2026-09-26T19:00:00Z").await;
    assert_conflict_code(status, &body, "comment_changed");

    let (status, edited) = edit(9001, "e-1", seen).await;
    assert_eq!(status, StatusCode::OK, "{edited}");
    assert_eq!(edited["data"]["state"], "published", "{edited}");
    assert_eq!(edited["data"]["kind"], "edit");
    assert_eq!(edited["data"]["body"], "New text");
    assert_eq!(
        edited["data"]["externalCommentUrl"],
        format!("{}/acme/issues/issues/12#issuecomment-9001", github.uri())
    );
    // Saving the same edit again, once GitHub holds it, does not send again
    // (the PATCH mock expects one call).
    Mock::given(method("GET"))
        .and(path(format!(
            "/repositories/{ISSUES_REPO}/issues/comments/9001"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(github_comment(
            &github,
            9001,
            ME,
            &format!("New text\n\n{reference}"),
            12,
            "2026-09-26T21:00:00Z",
        )))
        .with_priority(1)
        .mount(&github)
        .await;
    let (status, replay) = edit(9001, "e-1", seen).await;
    assert_eq!(status, StatusCode::OK, "{replay}");
    assert_eq!(replay["data"]["id"], "9001");
    // Someone else's comment, and a comment on another issue.
    let (status, body) = edit(9002, "e-3", seen).await;
    assert_conflict_code(status, &body, "not_your_comment");
    let (status, body) = edit(9003, "e-4", seen).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    let (status, body) = edit(9005, "e-6", seen).await;
    assert_conflict_code(status, &body, "not_your_comment");

    // An unclear answer is reported as such and says saving again is safe.
    let (status, body) = edit(9004, "e-5", seen).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["state"], "outcome_unknown");
    // The edited comment carries no G5 marker; the result still names it.
    assert_eq!(body["data"]["id"], "9004", "{body}");
    assert!(
        body["data"]["message"]
            .as_str()
            .unwrap()
            .contains("saving again is safe"),
        "{body}"
    );

    // Saving again after an edit GitHub already applied reports it without
    // resending, although the comment's timestamp moved.
    let (status, body) = edit(9006, "e-7", seen).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["state"], "published", "{body}");
    assert_eq!(
        body["data"]["externalCommentUrl"],
        format!("{}/acme/issues/issues/12#issuecomment-9006", github.uri())
    );

    github.verify().await;
    app.cleanup().await;
}

#[tokio::test]
async fn only_a_revoked_grant_asks_the_person_to_reconnect() {
    let github = MockServer::start().await;
    mount_github(&github).await;
    mount_empty_timeline(&github).await;
    mount_repository_visibility(&github, "private").await;
    let app = spawn_with_github(&github).await;
    let (owner, org) = org_with_domain(&app).await;
    enable_comments(&app, &owner, org.id).await;
    let workspace = insert_workspace(&app, org.id, owner.id, "renewal", false).await;
    let (status, body) = link(&app, &owner, workspace, "acme/issues#12", "primary").await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    // An access token about to expire, so posting renews it first.
    connect_github(&app, &github, &owner, "ghu_stale", 60).await;
    let tracker_id: Uuid = sqlx::query_scalar("SELECT id FROM issue_trackers WHERE org_id = $1")
        .bind(org.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    let destination = json!({"trackerId": tracker_id, "externalIssueId": issue_id(12).to_string()});

    // A deployment misconfiguration is not the person's to fix.
    let refresh = Mock::given(method("POST"))
        .and(path("/login/oauth/access_token"))
        .and(body_string_contains("grant_type=refresh_token"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"error": "incorrect_client_credentials"})),
        )
        .mount_as_scoped(&github)
        .await;
    let (status, body) = post_comment(&app, &owner, workspace, "Hello", "k-1", &destination).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("incorrect_client_credentials"),
        "{body}"
    );
    drop(refresh);
    let (_, connection) = json_of(
        app.authenticated_client(&owner)
            .get("/api/v1/github-app/user-connection")
            .await,
    )
    .await;
    assert_eq!(connection["data"]["connected"], true);

    // A revoked or superseded grant is.
    Mock::given(method("POST"))
        .and(path("/login/oauth/access_token"))
        .and(body_string_contains("grant_type=refresh_token"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"error": "bad_refresh_token"})),
        )
        .mount(&github)
        .await;
    let (status, body) = post_comment(&app, &owner, workspace, "Hello", "k-2", &destination).await;
    assert_conflict_code(status, &body, "github_connection_required");

    app.cleanup().await;
}

fn state_path(workspace_id: Uuid, link_id: &str) -> String {
    format!("/api/v1/workspaces/{workspace_id}/issue-links/{link_id}/state")
}

async fn change_state(
    app: &TestApp,
    user: &TestUser,
    workspace_id: Uuid,
    link_id: &str,
    state: &str,
) -> (StatusCode, Value) {
    json_of(
        app.authenticated_client(user)
            .post_json(&state_path(workspace_id, link_id), &json!({"state": state}))
            .await,
    )
    .await
}

/// GitHub received no issue write so far (a refused request sent nothing).
async fn assert_no_patch(github: &MockServer) {
    assert!(
        github
            .received_requests()
            .await
            .unwrap()
            .iter()
            .all(|request| request.method != "PATCH"),
        "a refused request must send nothing to GitHub"
    );
}

/// `g5.external-issues.transition`: a person closes a linked issue as
/// completed and reopens it under their own token, through the comment
/// guards; nothing is recorded and an unclear answer is reported as such.
#[tokio::test]
async fn a_person_closes_and_reopens_a_linked_issue_as_themselves() {
    let github = MockServer::start().await;
    mount_github(&github).await;
    let app = spawn_with_github(&github).await;
    let (owner, org) = org_with_domain(&app).await;
    let workspace = insert_workspace(&app, org.id, owner.id, "closing", false).await;
    let (status, body) = link(&app, &owner, workspace, "acme/issues#12", "primary").await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let link_id = body["data"]["id"].as_str().unwrap().to_string();

    // The request names a state.
    let (status, body) = change_state(&app, &owner, workspace, &link_id, "done").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["error"]["details"]["field"], "state", "{body}");

    // Someone outside the organization cannot reach the link.
    let mut outsider = TestUser::create(&app).await;
    let other_org = TestOrg::create_with_admin(&app, &mut outsider).await;
    enable_external_issues(&app, other_org.id).await;
    let (status, body) = change_state(&app, &outsider, workspace, &link_id, "closed").await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");

    // A review copy's controls are disabled, and the route refuses too.
    let review_copy = insert_workspace(&app, org.id, owner.id, "closing-review", false).await;
    let repo_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO repos (id, org_id, name, slug, git_url, default_branch, provider) \
         VALUES ($1, $2, 'closing', 'closing', 'file:///unused', 'main', 'github')",
    )
    .bind(repo_id)
    .bind(org.id)
    .execute(&app.db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO workspace_forks(
             id, org_id, source_workspace_id, target_workspace_id, requested_by, owner_user_id,
             idempotency_key, request_digest, proposed_name, repo_manifest, status, completed_at)
         VALUES ($1,$2,$3,$4,$5,$5,$6,$7,$8,$9,'completed',now())",
    )
    .bind(Uuid::new_v4())
    .bind(org.id)
    .bind(workspace)
    .bind(review_copy)
    .bind(owner.id)
    .bind("review-workspace:closing:1")
    .bind("c".repeat(64))
    .bind("closing-review")
    .bind(json!([{"repo_id": repo_id}]))
    .execute(&app.db)
    .await
    .unwrap();
    let (status, body) = change_state(&app, &owner, review_copy, &link_id, "closed").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(
        body["error"]["details"]["code"], "REVIEW_WORKSPACE_RESTRICTED",
        "{body}"
    );

    // External issues off for the organization refuses before anything else.
    sqlx::query(
        "UPDATE org_configs SET settings_json = settings_json || '{\"external_issues_enabled\": false}'::jsonb WHERE org_id = $1",
    )
    .bind(org.id)
    .execute(&app.db)
    .await
    .unwrap();
    let (status, body) = change_state(&app, &owner, workspace, &link_id, "closed").await;
    assert_conflict_code(status, &body, "external_issues_disabled");
    enable_external_issues(&app, org.id).await;

    // Comments are off until an admin enables them for the domain.
    let (status, body) = change_state(&app, &owner, workspace, &link_id, "closed").await;
    assert_conflict_code(status, &body, "comments_disabled");
    enable_comments(&app, &owner, org.id).await;

    // mount_github's repository has no visibility: unknown blocks writes.
    let (status, body) = change_state(&app, &owner, workspace, &link_id, "closed").await;
    assert_conflict_code(status, &body, "visibility_unknown");

    // Public repositories never receive G5 writes.
    mount_repository_visibility(&github, "public").await;
    let (status, body) = change_state(&app, &owner, workspace, &link_id, "closed").await;
    assert_conflict_code(status, &body, "public_destination");
    // None of the refusals so far sent anything to GitHub.
    assert_no_patch(&github).await;
    github.reset().await;
    mount_github(&github).await;
    mount_repository_visibility(&github, "private").await;

    // Without a GitHub connection the person is asked to connect.
    let (status, body) = change_state(&app, &owner, workspace, &link_id, "closed").await;
    assert_conflict_code(status, &body, "github_connection_required");
    connect_github(&app, &github, &owner, "ghu_closer", 28_800).await;

    // A link of another workspace is not this workspace's to change.
    let other = insert_workspace(&app, org.id, owner.id, "other", false).await;
    let (status, body) = change_state(&app, &owner, other, &link_id, "closed").await;
    assert_issue_error(status, &body, StatusCode::NOT_FOUND, "not_found");
    assert_no_patch(&github).await;

    // Close as completed with the person's token; the response carries
    // GitHub's answer.
    let mut closed = issue_json(&github, 12, "Issue 12");
    closed["state"] = json!("closed");
    closed["state_reason"] = json!("completed");
    closed["closed_at"] = json!("2026-10-15T12:00:00Z");
    Mock::given(method("PATCH"))
        .and(path(format!("/repositories/{ISSUES_REPO}/issues/12")))
        .and(header("authorization", "Bearer ghu_closer"))
        .and(body_json(
            json!({"state": "closed", "state_reason": "completed"}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(closed))
        .expect(1)
        .named("close as completed")
        .mount(&github)
        .await;
    let (status, body) = change_state(&app, &owner, workspace, &link_id, "closed").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["requested"], "closed");
    assert_eq!(body["data"]["outcome"], "applied");
    assert_eq!(body["data"]["githubLogin"], GITHUB_LOGIN);
    assert_eq!(body["data"]["issue"]["state"], "closed");
    assert_eq!(body["data"]["issue"]["stateReason"], "completed");
    assert_eq!(body["data"]["issue"]["displayKey"], "acme/issues#12");
    github.verify().await;

    // Reopen sends only the state.
    let mut reopened = issue_json(&github, 12, "Issue 12");
    reopened["state_reason"] = json!("reopened");
    Mock::given(method("PATCH"))
        .and(path(format!("/repositories/{ISSUES_REPO}/issues/12")))
        .and(header("authorization", "Bearer ghu_closer"))
        .and(body_json(json!({"state": "open"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(reopened))
        .expect(1)
        .named("reopen")
        .mount(&github)
        .await;
    let (status, body) = change_state(&app, &owner, workspace, &link_id, "open").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["outcome"], "applied");
    assert_eq!(body["data"]["issue"]["state"], "open");
    github.verify().await;

    // GitHub's refusal is reported as it is; a 5xx is an unknown outcome.
    github.reset().await;
    mount_github(&github).await;
    mount_repository_visibility(&github, "private").await;
    Mock::given(method("PATCH"))
        .and(path(format!("/repositories/{ISSUES_REPO}/issues/12")))
        .respond_with(
            ResponseTemplate::new(403).set_body_json(json!({"message": "Must have push access"})),
        )
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&github)
        .await;
    let (status, body) = change_state(&app, &owner, workspace, &link_id, "closed").await;
    assert_issue_error(status, &body, StatusCode::FORBIDDEN, "access_denied");
    Mock::given(method("PATCH"))
        .and(path(format!("/repositories/{ISSUES_REPO}/issues/12")))
        .respond_with(ResponseTemplate::new(502))
        .with_priority(2)
        .mount(&github)
        .await;
    let (status, body) = change_state(&app, &owner, workspace, &link_id, "closed").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["outcome"], "unknown");
    assert!(body["data"]["issue"].is_null(), "{body}");
    assert!(body["data"]["message"].is_string(), "{body}");

    app.cleanup().await;
}

// ─── The owner's agent reading linked issues (internal API) ────────────────

/// A running VM for the workspace and an incarnation token for its owner,
/// as a workspace agent holds it.
async fn agent_token(app: &TestApp, org_id: Uuid, workspace_id: Uuid, owner: Uuid) -> String {
    let host_id = Uuid::new_v4();
    let vm_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO worker_hosts (host_id, org_id, region, status, last_heartbeat, slot_size_reported) \
         VALUES ($1, $2, 'us-central1', 'ready', now(), true)",
    )
    .bind(host_id)
    .bind(org_id)
    .execute(&app.db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO workspace_vms \
         (org_id, workspace_id, host_id, vm_id, epoch, status, lease_expires_at) \
         VALUES ($1, $2, $3, $4, 1, 'running', now() + interval '1 hour')",
    )
    .bind(org_id)
    .bind(workspace_id)
    .bind(host_id)
    .bind(vm_id.to_string())
    .execute(&app.db)
    .await
    .unwrap();
    app.state
        .jwt_minter
        .as_ref()
        .expect("test jwt minter")
        .mint_workspace_incarnation(org_id, workspace_id, owner, vm_id, 1, 3600)
        .unwrap()
}

async fn agent_get(app: &TestApp, token: &str, path: &str) -> (StatusCode, Value) {
    json_of(
        reqwest::Client::new()
            .get(format!("{}{path}", app.base_url))
            .bearer_auth(token)
            .send()
            .await
            .unwrap(),
    )
    .await
}

#[tokio::test]
async fn the_workspace_agent_reads_only_its_own_linked_issues() {
    let github = MockServer::start().await;
    mount_github(&github).await;
    let app = spawn_with_github(&github).await;
    let (owner, org) = org_with_domain(&app).await;
    let workspace = insert_workspace(&app, org.id, owner.id, "agent-reads", false).await;
    let other = insert_workspace(&app, org.id, owner.id, "other-work", false).await;
    for (target, reference, role) in [
        (workspace, "acme/issues#12", "primary"),
        (workspace, "acme/issues#15", "related"),
        (other, "acme/issues#16", "primary"),
    ] {
        let (status, body) = link(&app, &owner, target, reference, role).await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
    }
    let token = agent_token(&app, org.id, workspace, owner.id).await;
    let links_url = format!("/api/internal/workspaces/{workspace}/issue-links");

    // The list: primary first, each with its live summary.
    let (status, body) = agent_get(&app, &token, &links_url).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let links = body["data"]["links"].as_array().unwrap();
    assert_eq!(links.len(), 2, "{body}");
    assert_eq!(links[0]["role"], "primary");
    assert_eq!(links[0]["displayKey"], "acme/issues#12");
    assert_eq!(links[0]["issue"]["title"], "Issue 12");
    assert_eq!(links[0]["issue"]["contentTrusted"], false);
    assert_eq!(links[1]["role"], "related");
    assert_eq!(links[1]["issue"]["title"], "Issue 15");

    // The primary issue with its description and comments.
    let primary = links[0]["id"].as_str().unwrap();
    let (status, body) = agent_get(&app, &token, &format!("{links_url}/{primary}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["link"]["role"], "primary");
    assert_eq!(body["data"]["issue"]["displayKey"], "acme/issues#12");
    assert_eq!(body["data"]["issue"]["body"], "Body of 12");
    let comments = body["data"]["issue"]["comments"].as_array().unwrap();
    assert_eq!(comments.len(), 2, "{body}");
    assert_eq!(comments[0]["body"], "first");

    // Another workspace's link is not found through this workspace.
    let other_link: Uuid = sqlx::query_scalar(
        "SELECT id FROM workspace_issue_links WHERE workspace_id = $1 AND unlinked_at IS NULL",
    )
    .bind(other)
    .fetch_one(&app.db)
    .await
    .unwrap();
    let (status, body) = agent_get(&app, &token, &format!("{links_url}/{other_link}")).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    // Nor can this workspace's token read the other workspace.
    let (status, body) = agent_get(
        &app,
        &token,
        &format!("/api/internal/workspaces/{other}/issue-links"),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");

    // Once the VM's lease lapses, its token no longer reads anything.
    sqlx::query(
        "UPDATE workspace_vms SET lease_expires_at = now() - interval '1 minute' \
         WHERE workspace_id = $1",
    )
    .bind(workspace)
    .execute(&app.db)
    .await
    .unwrap();
    let (status, body) = agent_get(&app, &token, &links_url).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    // A person's session is not an agent token.
    let (status, _) = json_of(app.authenticated_client(&owner).get(&links_url).await).await;
    assert!(
        matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN),
        "{status}"
    );

    app.cleanup().await;
}

/// GitHub requests the mock received at `path`.
async fn requests_to(github: &MockServer, path: &str) -> usize {
    github
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|request| request.url.path() == path)
        .count()
}

async fn set_external_issues(app: &TestApp, org_id: Uuid, enabled: bool) {
    sqlx::query(
        "UPDATE org_configs SET settings_json = settings_json || \
         jsonb_build_object('external_issues_enabled', $2::boolean) WHERE org_id = $1",
    )
    .bind(org_id)
    .bind(enabled)
    .execute(&app.db)
    .await
    .unwrap();
}

#[tokio::test]
async fn the_workspace_agent_views_any_issue_in_the_domains_as_its_owner() {
    let github = MockServer::start().await;
    mount_github(&github).await;
    let app = spawn_with_github(&github).await;
    let (admin, org) = org_with_domain(&app).await;
    // The agent's owner is an ordinary member; another member's private
    // workspace holds #16 as its primary issue.
    let owner = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let workspace = insert_workspace(&app, org.id, owner.id, "agent-views", false).await;
    let private = insert_workspace(&app, org.id, admin.id, "private-work", true).await;
    for (user, target, reference, role) in [
        (&owner, workspace, "acme/issues#12", "primary"),
        (&owner, workspace, "acme/issues#15", "related"),
        (&admin, private, "acme/issues#16", "primary"),
    ] {
        let (status, body) = link(&app, user, target, reference, role).await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
    }
    let token = agent_token(&app, org.id, workspace, owner.id).await;
    let view = |query: String| {
        let token = token.clone();
        let app = &app;
        async move {
            agent_get(
                app,
                &token,
                &format!("/api/internal/workspaces/{workspace}/issues/view?{query}"),
            )
            .await
        }
    };

    // No argument: the primary issue, without comments unless asked.
    let comments_path = format!("/repositories/{ISSUES_REPO}/issues/12/comments");
    let (status, body) = view(String::new()).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["workspaceLink"]["role"], "primary", "{body}");
    assert_eq!(body["data"]["issue"]["displayKey"], "acme/issues#12");
    assert_eq!(body["data"]["issue"]["body"], "Body of 12");
    assert_eq!(body["data"]["issue"]["commentCount"], 3);
    assert_eq!(body["data"]["issue"]["comments"], json!([]));
    assert_eq!(body["data"]["issue"]["commentsTruncated"], false);
    assert_eq!(body["data"]["issue"]["author"]["login"], "reporter");
    assert_eq!(requests_to(&github, &comments_path).await, 0);
    let (status, body) = view("comments=true".into()).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["data"]["issue"]["comments"].as_array().unwrap().len(),
        2
    );
    assert_eq!(body["data"]["issue"]["commentsTruncated"], true);

    // A bare number no link has names the creation target; the owner is
    // the viewer of the workspaces linking it.
    let (status, body) = view("ref=16".into()).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["issue"]["displayKey"], "acme/issues#16");
    assert_eq!(body["data"]["workspaceLink"], Value::Null);
    assert_eq!(
        body["data"]["issue"]["links"],
        json!([{"workspaceName": null, "role": "primary"}]),
        "{body}"
    );
    assert_eq!(body["data"]["issue"]["primaryWorkspace"]["visible"], false);
    // A full reference or URL reads any issue in the domain, and names this
    // workspace's link when there is one.
    let (status, body) = view(format!("ref={}", urlencode("acme/issues#17"))).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["issue"]["title"], "Issue 17");
    assert_eq!(body["data"]["issue"]["links"], json!([]));
    let url = format!("{}/acme/issues/issues/15", github.uri());
    let (status, body) = view(format!("ref={}", urlencode(&url))).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["workspaceLink"]["role"], "related", "{body}");
    assert_eq!(
        body["data"]["issue"]["links"][0]["workspaceName"], "agent-views",
        "{body}"
    );

    // Outside the configured domains, nothing is read.
    let (status, body) = view(format!("ref={}", urlencode("other/public#1"))).await;
    assert_issue_error(
        status,
        &body,
        StatusCode::UNPROCESSABLE_ENTITY,
        "outside_domain",
    );
    // Another workspace's link id is not this workspace's.
    let other_link: Uuid = sqlx::query_scalar(
        "SELECT id FROM workspace_issue_links WHERE workspace_id = $1 AND unlinked_at IS NULL",
    )
    .bind(private)
    .fetch_one(&app.db)
    .await
    .unwrap();
    let (status, body) = view(format!("ref={other_link}")).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    // Without a creation target, an unlinked number names no repository.
    sqlx::query("UPDATE issue_domains SET is_default_create_target = false WHERE org_id = $1")
        .bind(org.id)
        .execute(&app.db)
        .await
        .unwrap();
    let (status, body) = view("ref=16".into()).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(body.to_string().contains("owner/name#16"), "{body}");
    // A linked number still resolves.
    let (status, body) = view("ref=%2315".into()).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // The switch off refuses the read.
    set_external_issues(&app, org.id, false).await;
    let (status, body) = view(String::new()).await;
    assert_conflict_code(status, &body, "external_issues_disabled");
    set_external_issues(&app, org.id, true).await;

    // A lapsed VM lease ends the incarnation's reads.
    sqlx::query(
        "UPDATE workspace_vms SET lease_expires_at = now() - interval '1 minute' \
         WHERE workspace_id = $1",
    )
    .bind(workspace)
    .execute(&app.db)
    .await
    .unwrap();
    let (status, body) = view(String::new()).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    sqlx::query(
        "UPDATE workspace_vms SET lease_expires_at = now() + interval '1 hour' \
         WHERE workspace_id = $1",
    )
    .bind(workspace)
    .execute(&app.db)
    .await
    .unwrap();

    // Once the workspace has another owner, the old owner's agent reads
    // nothing more.
    sqlx::query("UPDATE workspaces SET owner_user_id = $2 WHERE id = $1")
        .bind(workspace)
        .bind(admin.id)
        .execute(&app.db)
        .await
        .unwrap();
    let (status, body) = view(format!("ref={}", urlencode("acme/issues#17"))).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");

    app.cleanup().await;
}

#[tokio::test]
async fn the_workspace_agent_searches_as_its_owner_within_the_agents_budget() {
    let github = MockServer::start().await;
    mount_github(&github).await;
    // Both states (gh's default) with a qualifier passed through.
    Mock::given(method("GET"))
        .and(path("/search/issues"))
        .and(query_param("q", "repo:acme/issues is:issue label:bug"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count": 1,
            "incomplete_results": false,
            "items": [issue_json(&github, 15, "Labelled")]
        })))
        .mount(&github)
        .await;
    let app = spawn_with_github(&github).await;
    let (owner, org) = org_with_domain(&app).await;
    let workspace = insert_workspace(&app, org.id, owner.id, "agent-searches", false).await;
    let (status, body) = link(&app, &owner, workspace, "acme/issues#12", "primary").await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let token = agent_token(&app, org.id, workspace, owner.id).await;
    let search = |token: String, workspace: Uuid, query: &'static str| {
        let app = &app;
        async move {
            agent_get(
                app,
                &token,
                &format!("/api/internal/workspaces/{workspace}/issues/search?{query}"),
            )
            .await
        }
    };

    let (status, body) = search(token.clone(), workspace, "q=crash&state=open").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let results = body["data"]["results"].as_array().unwrap();
    assert_eq!(results.len(), 1, "{body}");
    assert_eq!(results[0]["displayKey"], "acme/issues#12");
    assert_eq!(results[0]["author"]["login"], "reporter");
    assert_eq!(results[0]["commentCount"], 3);
    assert_eq!(
        results[0]["primaryWorkspace"]["name"], "agent-searches",
        "{body}"
    );
    let (status, body) = search(token.clone(), workspace, "q=label%3Abug&repo=ACME%2Fissues").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["data"]["results"][0]["displayKey"], "acme/issues#15",
        "{body}"
    );
    // A limit below GitHub's matches cuts the page and says so.
    let (status, body) = search(token.clone(), workspace, "q=flood&state=open&limit=1").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["results"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"]["truncated"], true);

    // What G5 cannot apply is refused before GitHub is asked.
    let searched = requests_to(&github, "/search/issues").await;
    for query in [
        "q=crash+repo%3Aother%2Fpublic",
        "q=is%3Apr",
        "limit=31",
        "limit=0",
        "assignee=octo",
    ] {
        let (status, body) = search(token.clone(), workspace, query).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{query}: {body}");
    }
    let (status, body) = search(token.clone(), workspace, "repo=other%2Fpublic").await;
    assert_issue_error(
        status,
        &body,
        StatusCode::UNPROCESSABLE_ENTITY,
        "outside_domain",
    );
    // `@me` is the owner's account, which must be connected.
    let (status, body) = search(token.clone(), workspace, "q=crash&assignee=me").await;
    assert_conflict_code(status, &body, "github_connection_required");
    assert_eq!(requests_to(&github, "/search/issues").await, searched);

    // The workspace's window is full: refused with a retry time, unsent.
    let window = |key: String| {
        let app = &app;
        async move {
            sqlx::query(
                "INSERT INTO rate_limit_windows (key, window_start, count) \
                 VALUES ($1, now() + interval '1 hour', 1000) \
                 ON CONFLICT (key) DO UPDATE SET window_start = EXCLUDED.window_start, count = 1000",
            )
            .bind(key)
            .execute(&app.db)
            .await
            .unwrap();
        }
    };
    window(format!(
        "g5:rate-limit:{{agent-issue-search}}:workspace:{workspace}"
    ))
    .await;
    let (status, body) = search(token.clone(), workspace, "q=crash&state=open").await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "{body}");
    assert!(
        body["error"]["details"]["retry_after_secs"]
            .as_u64()
            .unwrap()
            >= 1,
        "{body}"
    );
    assert_eq!(requests_to(&github, "/search/issues").await, searched);
    // Another workspace still searches until the organization's agents
    // have spent theirs.
    let second = insert_workspace(&app, org.id, owner.id, "agent-searches-2", false).await;
    let second_token = agent_token(&app, org.id, second, owner.id).await;
    // One workspace's token does not search as another.
    let (status, body) = search(token.clone(), second, "q=crash&state=open").await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    let (status, body) = search(second_token.clone(), second, "q=crash&state=open").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    window(format!(
        "g5:rate-limit:{{agent-issue-search}}:org:{}",
        org.id
    ))
    .await;
    let (status, body) = search(second_token, second, "q=crash&state=open").await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "{body}");
    // People's searches are not limited by G5.
    let (status, body) = json_of(
        app.authenticated_client(&owner)
            .get(&format!("/api/v1/orgs/{}/issues/search?q=crash", org.id))
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    app.cleanup().await;
}

#[tokio::test]
async fn the_workspace_agent_unlinks_its_own_issues_without_touching_the_tracker() {
    let github = MockServer::start().await;
    mount_github(&github).await;
    let app = spawn_with_github(&github).await;
    let (owner, org) = org_with_domain(&app).await;
    let workspace = insert_workspace(&app, org.id, owner.id, "agent-unlinks", false).await;
    let other = insert_workspace(&app, org.id, owner.id, "other-work", false).await;
    for (target, reference, role) in [
        (workspace, "acme/issues#12", "primary"),
        (workspace, "acme/issues#15", "related"),
        (other, "acme/issues#16", "related"),
    ] {
        let (status, body) = link(&app, &owner, target, reference, role).await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
    }
    let token = agent_token(&app, org.id, workspace, owner.id).await;
    let unlink = |reference: String| {
        let token = token.clone();
        let app = &app;
        async move {
            agent_post(
                app,
                &token,
                &format!("/api/internal/workspaces/{workspace}/issue-links/unlink"),
                &json!({"ref": reference}),
            )
            .await
        }
    };
    let requests = github.received_requests().await.unwrap_or_default().len();

    // A related link by reference ends; the owner is recorded.
    let (status, body) = unlink("ACME/issues#15".into()).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["link"]["role"], "related", "{body}");
    let unlinked_by: Option<Uuid> = sqlx::query_scalar(
        "SELECT unlinked_by FROM workspace_issue_links WHERE workspace_id = $1 \
         AND display_key = 'acme/issues#15'",
    )
    .bind(workspace)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(unlinked_by, Some(owner.id));
    // Again, it is no longer linked.
    let (status, body) = unlink("15".into()).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert!(body.to_string().contains("acme/issues#12"), "{body}");
    // Another workspace's link is not this workspace's to end.
    let other_link: Uuid = sqlx::query_scalar(
        "SELECT id FROM workspace_issue_links WHERE workspace_id = $1 AND unlinked_at IS NULL",
    )
    .bind(other)
    .fetch_one(&app.db)
    .await
    .unwrap();
    for reference in [other_link.to_string(), "acme/issues#16".to_string()] {
        let (status, body) = unlink(reference).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    }
    // Nor can this workspace's token unlink through the other workspace.
    let (status, body) = agent_post(
        &app,
        &token,
        &format!("/api/internal/workspaces/{other}/issue-links/unlink"),
        &json!({"ref": "acme/issues#16"}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");

    // The primary ends too, with the switch off: cleanup stays available.
    set_external_issues(&app, org.id, false).await;
    let (status, body) = unlink("#12".into()).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["link"]["role"], "primary", "{body}");
    assert_eq!(
        github.received_requests().await.unwrap_or_default().len(),
        requests,
        "unlinking reads no tracker"
    );
    set_external_issues(&app, org.id, true).await;
    // The claim is released: another workspace can take it.
    let (status, body) = link(&app, &owner, other, "acme/issues#12", "primary").await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    // A number two links share is refused, never guessed; the full
    // reference ends the one named.
    for reference in ["acme/issues#17", "acme/issues#20"] {
        let (status, body) = link(&app, &owner, workspace, reference, "related").await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
    }
    sqlx::query(
        "UPDATE workspace_issue_links SET display_key = 'acme/docs#17', \
         canonical_url = 'https://github.com/acme/docs/issues/17', \
         locator = jsonb_set(locator, '{number}', '17') \
         WHERE workspace_id = $1 AND display_key = 'acme/issues#20' AND unlinked_at IS NULL",
    )
    .bind(workspace)
    .execute(&app.db)
    .await
    .unwrap();
    let (status, body) = unlink("#17".into()).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(
        body["error"]["details"]["code"], "ambiguous_issue",
        "{body}"
    );
    let (status, body) = unlink("acme/docs#17".into()).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["link"]["displayKey"], "acme/docs#17", "{body}");

    app.cleanup().await;
}

async fn agent_post(app: &TestApp, token: &str, path: &str, body: &Value) -> (StatusCode, Value) {
    json_of(
        reqwest::Client::new()
            .post(format!("{}{path}", app.base_url))
            .bearer_auth(token)
            .json(body)
            .send()
            .await
            .unwrap(),
    )
    .await
}

#[tokio::test]
async fn the_workspace_agent_comments_on_the_primary_issue_as_its_owner() {
    let github = MockServer::start().await;
    mount_github(&github).await;
    mount_empty_timeline(&github).await;
    mount_repository_visibility(&github, "private").await;
    let app = spawn_with_github(&github).await;
    let (owner, org) = org_with_domain(&app).await;
    let workspace = insert_workspace(&app, org.id, owner.id, "agent-comments", false).await;
    let (status, body) = link(&app, &owner, workspace, "acme/issues#12", "primary").await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let token = agent_token(&app, org.id, workspace, owner.id).await;
    let comments_url = format!("/api/internal/workspaces/{workspace}/issue-comments");
    let comment = |text: &str, key: &str| json!({"body": text, "idempotencyKey": key});

    // The organization has not allowed G5 comments on the domain yet.
    let (status, body) = agent_post(&app, &token, &comments_url, &comment("Done.", "a-1")).await;
    assert_conflict_code(status, &body, "comments_disabled");
    enable_comments(&app, &owner, org.id).await;
    // The owner has not connected GitHub.
    let (status, body) = agent_post(&app, &token, &comments_url, &comment("Done.", "a-1")).await;
    assert_conflict_code(status, &body, "github_connection_required");
    connect_github(&app, &github, &owner, "ghu_owner", 28_800).await;
    // Secrets never leave G5.
    let (status, body) = agent_post(
        &app,
        &token,
        &comments_url,
        &comment("token ghp_abcdefghijklmnopqrstuvwxyz0123456789", "a-2"),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");

    Mock::given(method("POST"))
        .and(path(format!(
            "/repositories/{ISSUES_REPO}/issues/12/comments"
        )))
        .and(header("authorization", "Bearer ghu_owner"))
        .and(body_string_contains("Ready for review."))
        .and(body_string_contains("<!-- g5-comment "))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "id": 9301,
            "user": {"id": GITHUB_USER_ID, "login": GITHUB_LOGIN},
            "body": "Ready for review.",
            "created_at": "2026-09-26T20:00:00Z",
            "updated_at": "2026-09-26T20:00:00Z",
            "html_url": format!("{}/acme/issues/issues/12#issuecomment-9301", github.uri()),
        })))
        .expect(1)
        .mount(&github)
        .await;
    let (status, posted) = agent_post(
        &app,
        &token,
        &comments_url,
        &comment("Ready for review.", "a-3"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{posted}");
    assert_eq!(posted["data"]["state"], "published", "{posted}");
    assert_eq!(posted["data"]["origin"], "agent");
    assert_eq!(posted["data"]["githubLogin"], GITHUB_LOGIN);
    assert_eq!(posted["data"]["displayKey"], "acme/issues#12");
    // A retry of the same comment finds it on the issue and does not post
    // again.
    mount_issue_comments(
        &github,
        12,
        json!([github_comment(
            &github,
            9301,
            ME,
            posted["data"]["payload"].as_str().unwrap(),
            12,
            "2026-09-26T20:00:00Z",
        )]),
    )
    .await;
    let (status, replay) = agent_post(
        &app,
        &token,
        &comments_url,
        &comment("Ready for review.", "a-3"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{replay}");
    assert_eq!(replay["data"]["id"], posted["data"]["id"]);
    // The key names that text: another comment is refused, not reported as
    // the first one.
    let (status, body) = agent_post(
        &app,
        &token,
        &comments_url,
        &comment("Something else.", "a-3"),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(
        body["error"]["details"]["field"], "idempotencyKey",
        "{body}"
    );
    // Keys are exact: surrounding whitespace is refused, not trimmed.
    let (status, body) = agent_post(
        &app,
        &token,
        &comments_url,
        &comment("Ready for review.", " a-3"),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");

    github.verify().await;
    app.cleanup().await;
}

#[tokio::test]
async fn the_organization_switch_gates_external_issues() {
    let github = MockServer::start().await;
    mount_github(&github).await;
    mount_empty_timeline(&github).await;
    let app = spawn_with_github(&github).await;
    let (owner, org) = org_with_domain(&app).await;
    let workspace = insert_workspace(&app, org.id, owner.id, "switched", false).await;
    let (status, body) = link(&app, &owner, workspace, "acme/issues#12", "primary").await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let (status, related) = link(&app, &owner, workspace, "acme/issues#15", "related").await;
    assert_eq!(status, StatusCode::CREATED, "{related}");
    let client = app.authenticated_client(&owner);
    let set_switch = |scope_path: String, value: Value| {
        let client = &client;
        async move {
            json_of(
                client
                    .put_json(
                        &scope_path,
                        &json!({"settings": {"external_issues_enabled": value}}),
                    )
                    .await,
            )
            .await
        }
    };

    // Only an org owner or admin can turn it on or off.
    let member = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let (status, body) = json_of(
        app.authenticated_client(&member)
            .put_json(
                &format!("/api/v1/orgs/{}/config", org.id),
                &json!({"settings": {"external_issues_enabled": false}}),
            )
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    let (status, body) = json_of(
        app.authenticated_client(&member)
            .put_json(
                &format!("/api/v1/orgs/{}/config", org.id),
                &json!({"removeSettingKeys": ["external_issues_enabled"]}),
            )
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");

    // Only the organization can turn it on or off, and only with a boolean.
    let (status, body) = set_switch(
        format!("/api/v1/workspaces/{workspace}/config?orgId={}", org.id),
        json!(true),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(
        body["error"]["details"]["code"], "org_only_setting",
        "{body}"
    );
    let (status, body) = set_switch(format!("/api/v1/orgs/{}/config", org.id), json!("yes")).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(
        body["error"]["details"]["code"], "invalid_setting_type",
        "{body}"
    );

    // Turned off: issue work is refused, Activity shows no issue, and the
    // agent cannot read or comment.
    let (status, body) = set_switch(format!("/api/v1/orgs/{}/config", org.id), json!(false)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    for path in [
        format!("/api/v1/orgs/{}/issues/search?q=crash", org.id),
        format!(
            "/api/v1/orgs/{}/issues/resolve?ref=acme%2Fissues%2312",
            org.id
        ),
    ] {
        let (status, body) = json_of(client.get(&path).await).await;
        assert_conflict_code(status, &body, "external_issues_disabled");
    }
    // Cleanup stays available: the links and domains G5 holds are listed
    // without asking GitHub or returning issue content, and a link can be
    // unlinked.
    let github_calls = github.received_requests().await.unwrap().len();
    let (status, body) = json_of(client.get(&links_path(workspace)).await).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let listed = body["data"]["links"].as_array().unwrap();
    assert_eq!(listed.len(), 2, "{body}");
    for listed_link in listed {
        assert_eq!(listed_link["issue"], Value::Null, "{body}");
        assert_eq!(listed_link["issueError"], Value::Null, "{body}");
    }
    let (status, body) = json_of(client.get(&domains_path(org.id)).await).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["domains"].as_array().unwrap().len(), 1);
    assert_eq!(
        github.received_requests().await.unwrap().len(),
        github_calls,
        "cleanup reads must not call GitHub"
    );
    let (status, body) = json_of(
        client
            .post_json(
                &format!(
                    "{}/{}/unlink",
                    links_path(workspace),
                    related["data"]["id"].as_str().unwrap()
                ),
                &json!({}),
            )
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = link(&app, &owner, workspace, "acme/issues#15", "related").await;
    assert_conflict_code(status, &body, "external_issues_disabled");
    let (status, activity) = json_of(client.get(&activity_path(workspace)).await).await;
    assert_eq!(status, StatusCode::OK, "{activity}");
    assert_eq!(activity["data"]["primaryIssue"], Value::Null, "{activity}");
    assert_eq!(activity["data"]["composer"], Value::Null, "{activity}");
    let token = agent_token(&app, org.id, workspace, owner.id).await;
    let (status, body) = agent_get(
        &app,
        &token,
        &format!("/api/internal/workspaces/{workspace}/issue-links"),
    )
    .await;
    assert_conflict_code(status, &body, "external_issues_disabled");
    let base = TestWorkspace::create(&app, &org).await;
    let (status, body) = fork_with_issue(
        &app,
        &owner,
        base.id,
        "fix/switched-off",
        "fork-switched-off",
        json!({"ref": "acme/issues#16", "role": "primary"}),
    )
    .await;
    assert_conflict_code(status, &body, "external_issues_disabled");

    // Turned back on, the same work resumes.
    let (status, body) = set_switch(format!("/api/v1/orgs/{}/config", org.id), json!(true)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = json_of(client.get(&links_path(workspace)).await).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["links"].as_array().unwrap().len(), 1);

    app.cleanup().await;
}

#[tokio::test]
async fn sec_h02_comments_require_target_write_authority_without_workspace_ownership() {
    let github = MockServer::start().await;
    mount_github(&github).await;
    let app = spawn_with_github(&github).await;
    let (owner, org) = org_with_domain(&app).await;
    let workspace = insert_workspace(&app, org.id, owner.id, "collaborative-comments", false).await;
    let (status, body) = link(&app, &owner, workspace, "acme/issues#12", "primary").await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    enable_comments(&app, &owner, org.id).await;
    let tracker: Uuid = sqlx::query_scalar("SELECT id FROM issue_trackers WHERE org_id=$1")
        .bind(org.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    let destination = json!({"trackerId":tracker,"externalIssueId":issue_id(12).to_string()});
    let mut reviewer = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let _source = TestOrg::create_with_admin(&app, &mut reviewer).await;
    connect_github(&app, &github, &reviewer, "ghu_person", 28_800).await;
    mount_repository_visibility(&github, "private").await;
    Mock::given(method("POST"))
        .and(path(format!(
            "/repositories/{ISSUES_REPO}/issues/12/comments"
        )))
        .and(header("authorization", "Bearer ghu_person"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "id": 9010, "user":{"login":GITHUB_LOGIN}, "body":"Collaborator review",
            "created_at":"2026-09-26T20:00:00Z", "updated_at":"2026-09-26T20:00:00Z",
            "html_url":format!("{}/acme/issues/issues/12#issuecomment-9010", github.uri())
        })))
        .expect(1)
        .mount(&github)
        .await;
    sqlx::query("UPDATE org_memberships SET role='viewer' WHERE org_id=$1 AND user_id=$2")
        .bind(org.id)
        .bind(reviewer.id)
        .execute(&app.db)
        .await
        .unwrap();
    let comment_path = comments_path(workspace);
    let payload = json!({"body":"Collaborator review","idempotencyKey":"collaborative-1","destination":destination,"expectedUpdatedAt":"2026-09-26T20:00:00Z"});
    let http = reqwest::Client::new();
    for (method, path) in [
        (reqwest::Method::POST, comment_path.clone()),
        (
            reqwest::Method::PATCH,
            format!("{comment_path}/github/9010"),
        ),
    ] {
        let response = http
            .request(method.clone(), format!("{}{path}", app.base_url))
            .bearer_auth(&reviewer.jwt)
            .json(&payload)
            .send()
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::FORBIDDEN,
            "{method} {path}: {}",
            response.text().await.unwrap()
        );
    }
    // A member who does not own this workspace can legitimately collaborate.
    sqlx::query("UPDATE org_memberships SET role='member' WHERE org_id=$1 AND user_id=$2")
        .bind(org.id)
        .bind(reviewer.id)
        .execute(&app.db)
        .await
        .unwrap();
    let (status, body) = post_comment(
        &app,
        &reviewer,
        workspace,
        "Collaborator review",
        "collaborative-1",
        &destination,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    // GitHub now lists it, so a replay finds it instead of posting again.
    mount_issue_comments(
        &github,
        12,
        json!([github_comment(
            &github,
            9010,
            ME,
            body["data"]["payload"].as_str().unwrap(),
            12,
            "2026-09-26T20:00:00Z",
        )]),
    )
    .await;
    sqlx::query("UPDATE org_memberships SET role='viewer' WHERE org_id=$1 AND user_id=$2")
        .bind(org.id)
        .bind(reviewer.id)
        .execute(&app.db)
        .await
        .unwrap();
    let (status, _) = post_comment(
        &app,
        &reviewer,
        workspace,
        "Collaborator review",
        "collaborative-1",
        &destination,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    // A write grant does not raise a viewer above read-only
    // (`g5.organization.viewer-ceiling`); restored to member, the same
    // request replays idempotently.
    reviewer
        .grant_org_scope(&app, org.id, Scope::WorkspacesWrite)
        .await;
    let (status, _) = post_comment(
        &app,
        &reviewer,
        workspace,
        "Collaborator review",
        "collaborative-1",
        &destination,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    sqlx::query("UPDATE org_memberships SET role='member' WHERE org_id=$1 AND user_id=$2")
        .bind(org.id)
        .bind(reviewer.id)
        .execute(&app.db)
        .await
        .unwrap();
    let (status, replay) = post_comment(
        &app,
        &reviewer,
        workspace,
        "Collaborator review",
        "collaborative-1",
        &destination,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{replay}");
    assert_eq!(replay["data"]["id"], "9010");
    github.verify().await;
    app.cleanup().await;
}

#[tokio::test]
async fn sec_h02_post_answers_after_authority_is_revoked_in_flight() {
    comment_answers_after_revocation_in_flight(false, PublicationRevocation::Membership).await;
}

#[tokio::test]
async fn sec_h02_edit_answers_after_authority_is_revoked_in_flight() {
    comment_answers_after_revocation_in_flight(true, PublicationRevocation::Membership).await;
}

#[tokio::test]
async fn sec_h02_post_answers_after_workspace_deletion_in_flight() {
    comment_answers_after_revocation_in_flight(false, PublicationRevocation::Workspace).await;
}

#[tokio::test]
async fn sec_h02_edit_answers_after_workspace_deletion_in_flight() {
    comment_answers_after_revocation_in_flight(true, PublicationRevocation::Workspace).await;
}

#[tokio::test]
async fn sec_h02_post_answers_after_external_issues_disabled_in_flight() {
    comment_answers_after_revocation_in_flight(false, PublicationRevocation::ExternalIssues).await;
}

#[tokio::test]
async fn sec_h02_edit_answers_after_external_issues_disabled_in_flight() {
    comment_answers_after_revocation_in_flight(true, PublicationRevocation::ExternalIssues).await;
}

#[derive(Clone, Copy)]
enum PublicationRevocation {
    Membership,
    Workspace,
    ExternalIssues,
}

/// No authority lock or transaction is held while GitHub performs a comment
/// write: revocation completes meanwhile, and the request still reports
/// GitHub's answer (nothing is recorded to finalize).
async fn comment_answers_after_revocation_in_flight(edit: bool, revocation: PublicationRevocation) {
    use std::time::Duration;
    let github = MockServer::start().await;
    mount_github(&github).await;
    let app = spawn_with_github(&github).await;
    let (owner, org) = org_with_domain(&app).await;
    let workspace = insert_workspace(&app, org.id, owner.id, "revoked-comment", false).await;
    let (status, body) = link(&app, &owner, workspace, "acme/issues#12", "primary").await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    enable_comments(&app, &owner, org.id).await;
    let tracker: Uuid = sqlx::query_scalar("SELECT id FROM issue_trackers WHERE org_id=$1")
        .bind(org.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    let destination = json!({"trackerId":tracker,"externalIssueId":issue_id(12).to_string()});
    let author = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    connect_github(&app, &github, &author, "ghu_person", 28_800).await;
    mount_repository_visibility(&github, "private").await;
    if edit {
        mount_github_comment(
            &github,
            github_comment(&github, 9001, ME, "Old text", 12, "2026-09-26T20:00:00Z"),
        )
        .await;
    }
    let verb = if edit { "PATCH" } else { "POST" };
    let provider_path = if edit {
        format!("/repositories/{ISSUES_REPO}/issues/comments/9001")
    } else {
        format!("/repositories/{ISSUES_REPO}/issues/12/comments")
    };
    Mock::given(method(verb))
        .and(path(&provider_path))
        .respond_with(
            ResponseTemplate::new(if edit { 200 } else { 201 })
                .set_body_json(github_comment(
                    &github,
                    9001,
                    ME,
                    "Saved text",
                    12,
                    "2026-09-26T20:01:00Z",
                ))
                .set_delay(Duration::from_secs(3)),
        )
        .expect(1)
        .mount(&github)
        .await;
    let route = if edit {
        format!("{}/github/9001", comments_path(workspace))
    } else {
        comments_path(workspace)
    };
    let payload = json!({"body":"Saved text", "idempotencyKey":"revoke-in-flight", "destination":destination, "expectedUpdatedAt":"2026-09-26T20:00:00Z"});
    let request = reqwest::Client::new()
        .request(
            if edit {
                reqwest::Method::PATCH
            } else {
                reqwest::Method::POST
            },
            format!("{}{route}", app.base_url),
        )
        .bearer_auth(&author.jwt)
        .json(&payload)
        .send();
    tokio::pin!(request);
    tokio::select! {
        response = &mut request => panic!("request finished before provider mutation: {:?}", response.unwrap().text().await),
        () = async {
            tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    if github.received_requests().await.unwrap().iter().any(|r| r.method.as_str() == verb && r.url.path() == provider_path) { break; }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            }).await.expect("provider mutation was not sent");
        } => {}
    }
    // Admission has committed and released authority locks. Revocation now
    // completes before the provider response.
    tokio::time::timeout(Duration::from_secs(2), async {
        if matches!(revocation, PublicationRevocation::ExternalIssues) {
            let response = app
                .authenticated_client(&owner)
                .put_json(
                    &format!("/api/v1/orgs/{}/config", org.id),
                    &json!({"settings":{"external_issues_enabled":false}}),
                )
                .await;
            assert_eq!(
                response.status(),
                StatusCode::OK,
                "{}",
                response.text().await.unwrap()
            );
            return Ok::<_, sqlx::Error>(());
        }
        if matches!(revocation, PublicationRevocation::Workspace) {
            sqlx::query(
                "UPDATE workspaces SET deleted_at=clock_timestamp(), deleted_by=$2 WHERE id=$1",
            )
            .bind(workspace)
            .bind(owner.id)
            .execute(&app.db)
            .await
            .map(|_| ())
        } else {
            sqlx::query("DELETE FROM org_memberships WHERE org_id=$1 AND user_id=$2")
                .bind(org.id)
                .bind(author.id)
                .execute(&app.db)
                .await
                .map(|_| ())
        }
    })
    .await
    .expect("admission locks must be released before the provider responds")
    .unwrap();
    let response = request.await.unwrap();
    let status = response.status();
    let body: Value = response.json().await.unwrap();
    let expected = if edit {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };
    assert_eq!(status, expected, "edit={edit}: {body}");
    assert_eq!(body["data"]["state"], "published", "{body}");
    let response = reqwest::Client::new().post(format!("{}{}", app.base_url, comments_path(workspace))).bearer_auth(&author.jwt)
            .json(&json!({"body":"New request", "idempotencyKey":"after-revoke", "destination":destination})).send().await.unwrap();
    if matches!(revocation, PublicationRevocation::ExternalIssues) {
        let (status, body) = json_of(response).await;
        assert_conflict_code(status, &body, "external_issues_disabled");
    } else {
        assert!(matches!(
            response.status(),
            StatusCode::FORBIDDEN | StatusCode::NOT_FOUND
        ));
    }
    github.verify().await;
    app.cleanup().await;
}

// ─── Issue creation ──────────────────────────────────────────────────────────

fn org_issues_path(org_id: Uuid) -> String {
    format!("/api/v1/orgs/{org_id}/issues")
}

async fn create_target(app: &TestApp, user: &TestUser, org_id: Uuid) -> Value {
    let (status, body) = json_of(
        app.authenticated_client(user)
            .get(&format!("{}/create-target", org_issues_path(org_id)))
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["data"].clone()
}

async fn create_issue(
    app: &TestApp,
    user: &TestUser,
    org_id: Uuid,
    request: &Value,
) -> (StatusCode, Value) {
    json_of(
        app.authenticated_client(user)
            .post_json(&org_issues_path(org_id), request)
            .await,
    )
    .await
}

/// A GitHub issue `number` in the issue repository whose description is
/// `body`.
fn created_issue_json(github: &MockServer, number: u64, title: &str, body: &str) -> Value {
    let mut issue = issue_json(github, number, title);
    issue["body"] = json!(body);
    issue["assignees"] = json!([]);
    issue
}

/// The issues `creator` opened recently: `issues`, at most `times` times.
async fn mount_created_by(github: &MockServer, creator: &str, issues: Value, times: Option<u64>) {
    let mock = Mock::given(method("GET"))
        .and(path(format!("/repositories/{ISSUES_REPO}/issues")))
        .and(query_param("creator", creator))
        .and(query_param("state", "all"))
        .respond_with(ResponseTemplate::new(200).set_body_json(issues));
    match times {
        Some(times) => mock.up_to_n_times(times).mount(github).await,
        None => mock.mount(github).await,
    }
}

#[tokio::test]
async fn members_create_issues_as_themselves_only_in_an_approved_non_public_target() {
    use wiremock::matchers::body_partial_json;
    let github = MockServer::start().await;
    mount_github(&github).await;
    Mock::given(method("GET"))
        .and(path(format!("/repositories/{ISSUES_REPO}/assignees")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"id": 7, "login": "fixer"},
            {"id": GITHUB_USER_ID, "login": GITHUB_LOGIN}
        ])))
        .mount(&github)
        .await;
    let app = spawn_with_github(&github).await;
    let (owner, org) = org_with_domain(&app).await;
    // Any member with write authority creates; ownership of a workspace is
    // not needed.
    let member = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let domain_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM issue_domains WHERE org_id = $1 AND is_default_create_target",
    )
    .bind(org.id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    let request = |title: &str, key: &str| {
        json!({
            "title": title,
            "body": "Steps to reproduce",
            "assignees": ["@me", "fixer"],
            "idempotencyKey": key,
            "domainId": domain_id,
        })
    };

    // The first domain is the creation target; creation follows its G5
    // comments switch.
    let target = create_target(&app, &member, org.id).await;
    assert_eq!(target["state"], "comments_disabled", "{target}");
    assert_eq!(target["destination"]["displayKey"], "acme/issues");
    assert_eq!(target["destination"]["domainId"], json!(domain_id));
    let (status, body) = create_issue(&app, &member, org.id, &request("Crash", "i-1")).await;
    assert_conflict_code(status, &body, "comments_disabled");
    enable_comments(&app, &owner, org.id).await;

    // Unknown visibility and public repositories receive nothing.
    assert_eq!(
        create_target(&app, &member, org.id).await["state"],
        "visibility_unknown"
    );
    let (status, body) = create_issue(&app, &member, org.id, &request("Crash", "i-1")).await;
    assert_conflict_code(status, &body, "visibility_unknown");
    mount_repository_visibility(&github, "public").await;
    assert_eq!(
        create_target(&app, &member, org.id).await["state"],
        "public_destination"
    );
    let (status, body) = create_issue(&app, &member, org.id, &request("Crash", "i-1")).await;
    assert_conflict_code(status, &body, "public_destination");
    github.reset().await;
    mount_github(&github).await;
    mount_repository_visibility(&github, "private").await;
    Mock::given(method("GET"))
        .and(path(format!("/repositories/{ISSUES_REPO}/assignees")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{"login": "fixer"}])))
        .mount(&github)
        .await;

    // Secrets in the title or description are refused without echoing them.
    let secret = format!("ghp_{}", "a".repeat(36));
    let (status, body) = create_issue(
        &app,
        &member,
        org.id,
        &request(&format!("leak {secret}"), "i-1"),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["error"]["details"]["code"], "secret_detected");
    assert_eq!(body["error"]["details"]["field"], "title");
    assert!(!body.to_string().contains(&secret), "{body}");

    // Without a GitHub connection the person is asked to connect; there is
    // no bot fallback.
    let target = create_target(&app, &member, org.id).await;
    assert_eq!(target["state"], "connection_required", "{target}");
    assert_eq!(
        target["assignableUsers"],
        json!([{"login": "fixer", "avatarUrl": null}])
    );
    let (status, body) = create_issue(&app, &member, org.id, &request("Crash", "i-1")).await;
    assert_conflict_code(status, &body, "github_connection_required");
    connect_github(&app, &github, &member, "ghu_member", 28_800).await;
    let target = create_target(&app, &member, org.id).await;
    assert_eq!(target["state"], "ready", "{target}");
    assert_eq!(target["githubLogin"], GITHUB_LOGIN);

    // A changed creation target needs a new review.
    let mut stale = request("Crash", "i-1");
    stale["domainId"] = json!(Uuid::new_v4());
    let (status, body) = create_issue(&app, &member, org.id, &stale).await;
    assert_conflict_code(status, &body, "destination_changed");

    // The first attempt finds no earlier issue and sends exactly once.
    let marker = g5_api::services::issue_creations::creation_marker(org.id, member.id, "i-1");
    mount_created_by(&github, GITHUB_LOGIN, json!([]), Some(1)).await;
    Mock::given(method("POST"))
        .and(path(format!("/repositories/{ISSUES_REPO}/issues")))
        .and(header("authorization", "Bearer ghu_member"))
        .and(body_partial_json(json!({
            "title": "Crash",
            "assignees": [GITHUB_LOGIN, "fixer"],
        })))
        .and(body_partial_json(
            json!({"body": format!("Steps to reproduce\n\n{marker}")}),
        ))
        .respond_with(ResponseTemplate::new(201).set_body_json(created_issue_json(
            &github,
            30,
            "Crash",
            &format!("Steps to reproduce\n\n{marker}"),
        )))
        .expect(1)
        .mount(&github)
        .await;
    let (status, created) = create_issue(&app, &member, org.id, &request("Crash", "i-1")).await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let data = &created["data"];
    assert_eq!(data["state"], "created", "{created}");
    assert_eq!(data["sent"], true);
    assert_eq!(data["githubLogin"], GITHUB_LOGIN);
    assert_eq!(data["assignees"], json!([GITHUB_LOGIN, "fixer"]));
    assert_eq!(data["issue"]["number"], 30);
    assert_eq!(data["issue"]["displayKey"], "acme/issues#30");
    assert_eq!(data["issue"]["repositoryId"], ISSUES_REPO.to_string());
    let payload = data["payload"].as_str().unwrap().to_string();
    assert!(
        payload.starts_with("Steps to reproduce\n\n<!-- g5-issue "),
        "{payload}"
    );

    // A retry with the same key finds that issue by its marker and returns
    // it without sending again (the create mock expects one call).
    mount_created_by(
        &github,
        GITHUB_LOGIN,
        json!([created_issue_json(&github, 30, "Crash", &payload)]),
        None,
    )
    .await;
    let (status, replay) = create_issue(&app, &member, org.id, &request("Crash", "i-1")).await;
    assert_eq!(status, StatusCode::OK, "{replay}");
    assert_eq!(replay["data"]["state"], "created");
    assert_eq!(replay["data"]["sent"], false);
    assert_eq!(replay["data"]["issue"]["number"], 30);
    assert_eq!(replay["data"]["payload"], payload);

    github.verify().await;
    app.cleanup().await;
}

#[tokio::test]
async fn an_unclear_creation_is_checked_by_its_marker_and_never_resent_automatically() {
    let github = MockServer::start().await;
    mount_github(&github).await;
    mount_repository_visibility(&github, "private").await;
    let app = spawn_with_github(&github).await;
    let (owner, org) = org_with_domain(&app).await;
    enable_comments(&app, &owner, org.id).await;
    connect_github(&app, &github, &owner, "ghu_owner", 28_800).await;
    let domain_id: Uuid = sqlx::query_scalar("SELECT id FROM issue_domains WHERE org_id = $1")
        .bind(org.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    let request = |check_only: bool| {
        json!({
            "title": "Flaky deploy",
            "idempotencyKey": "unclear-1",
            "domainId": domain_id,
            "checkOnly": check_only,
        })
    };

    // A lookup that fails refuses rather than risk a duplicate.
    Mock::given(method("GET"))
        .and(path(format!("/repositories/{ISSUES_REPO}/issues")))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(1)
        .mount(&github)
        .await;
    let (status, body) = create_issue(&app, &owner, org.id, &request(false)).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");

    // GitHub gives no definite answer: the outcome is unknown.
    mount_created_by(&github, GITHUB_LOGIN, json!([]), Some(2)).await;
    Mock::given(method("POST"))
        .and(path(format!("/repositories/{ISSUES_REPO}/issues")))
        .respond_with(ResponseTemplate::new(502))
        .expect(1)
        .mount(&github)
        .await;
    let (status, body) = create_issue(&app, &owner, org.id, &request(false)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["state"], "outcome_unknown", "{body}");
    assert_eq!(body["data"]["sent"], true);
    assert_eq!(body["data"]["idempotencyKey"], "unclear-1");
    assert!(body["data"]["issue"].is_null());
    let payload = body["data"]["payload"].as_str().unwrap().to_string();
    assert!(payload.starts_with("<!-- g5-issue "), "{payload}");

    // A check only looks: no issue yet, and nothing is sent.
    let (status, body) = create_issue(&app, &owner, org.id, &request(true)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["state"], "not_found", "{body}");
    assert_eq!(body["data"]["sent"], false);

    // Once GitHub lists the issue, the check reports it created.
    mount_created_by(
        &github,
        GITHUB_LOGIN,
        json!([created_issue_json(&github, 31, "Flaky deploy", &payload)]),
        None,
    )
    .await;
    let (status, body) = create_issue(&app, &owner, org.id, &request(true)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["state"], "created", "{body}");
    assert_eq!(body["data"]["sent"], false);
    assert_eq!(body["data"]["issue"]["number"], 31);

    // Organizations with external issues off create nothing.
    sqlx::query(
        "UPDATE org_configs SET settings_json = settings_json || '{\"external_issues_enabled\": false}'::jsonb \
         WHERE org_id = $1",
    )
    .bind(org.id)
    .execute(&app.db)
    .await
    .unwrap();
    let (status, body) = create_issue(&app, &owner, org.id, &request(false)).await;
    assert_conflict_code(status, &body, "external_issues_disabled");

    github.verify().await;
    app.cleanup().await;
}

/// GitHub's parent of issue `number`: `parent`, or none (404).
async fn mount_parent(github: &MockServer, number: u64, parent: Option<Value>) {
    let response = match parent {
        Some(parent) => ResponseTemplate::new(200).set_body_json(parent),
        None => ResponseTemplate::new(404).set_body_json(json!({"message": "Not Found"})),
    };
    Mock::given(method("GET"))
        .and(path(format!(
            "/repositories/{ISSUES_REPO}/issues/{number}/parent"
        )))
        .respond_with(response)
        .mount(github)
        .await;
}

/// GitHub's answer to putting issue `number` under #12, at most `times`
/// times when given.
async fn mount_attach(
    github: &MockServer,
    number: u64,
    status: u16,
    times: Option<u64>,
    priority: u8,
) {
    use wiremock::matchers::body_partial_json;
    let mock = Mock::given(method("POST"))
        .and(path(format!(
            "/repositories/{ISSUES_REPO}/issues/12/sub_issues"
        )))
        .and(header("authorization", "Bearer ghu_owner"))
        .and(body_partial_json(json!({"sub_issue_id": issue_id(number)})))
        .respond_with(
            ResponseTemplate::new(status)
                .set_body_json(json!({"message": format!("status {status}")})),
        )
        .with_priority(priority);
    match times {
        Some(times) => mock.up_to_n_times(times).expect(times).mount(github).await,
        None => mock.mount(github).await,
    }
}

#[tokio::test]
async fn a_new_issue_goes_under_its_parent_once_and_is_never_moved() {
    use g5_api::services::issue_creations::creation_marker;
    use std::sync::{Arc, Mutex};
    use wiremock::matchers::body_partial_json;
    let github = MockServer::start().await;
    mount_github(&github).await;
    mount_repository_visibility(&github, "private").await;
    let app = spawn_with_github(&github).await;
    let (owner, org) = org_with_domain(&app).await;
    enable_comments(&app, &owner, org.id).await;
    connect_github(&app, &github, &owner, "ghu_owner", 28_800).await;
    let domain_id: Uuid = sqlx::query_scalar("SELECT id FROM issue_domains WHERE org_id = $1")
        .bind(org.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    let request = |key: &str, parent: &str, check_only: bool| {
        json!({
            "title": "Sub-task",
            "idempotencyKey": key,
            "domainId": domain_id,
            "parent": parent,
            "checkOnly": check_only,
        })
    };
    // The owner's recent issues, as GitHub lists them now.
    let listed = Arc::new(Mutex::new(Vec::<Value>::new()));
    let listing = listed.clone();
    Mock::given(method("GET"))
        .and(path(format!("/repositories/{ISSUES_REPO}/issues")))
        .and(query_param("creator", GITHUB_LOGIN))
        .respond_with(move |_: &wiremock::Request| {
            ResponseTemplate::new(200).set_body_json(Value::Array(listing.lock().unwrap().clone()))
        })
        .mount(&github)
        .await;
    // Issue `number` created by the submission with `key`, sent once.
    let mount_create = |number: u64, key: &str| {
        let payload = creation_marker(org.id, owner.id, key);
        let issue = created_issue_json(&github, number, "Sub-task", &payload);
        let github = &github;
        async move {
            Mock::given(method("POST"))
                .and(path(format!("/repositories/{ISSUES_REPO}/issues")))
                .and(body_partial_json(json!({"body": payload})))
                .respond_with(ResponseTemplate::new(201).set_body_json(issue.clone()))
                .expect(1)
                .mount(github)
                .await;
            issue
        }
    };

    // A parent that is not an issue of the creation target refuses the
    // creation with nothing sent.
    {
        let _nothing_sent = Mock::given(method("POST"))
            .and(path(format!("/repositories/{ISSUES_REPO}/issues")))
            .respond_with(ResponseTemplate::new(500))
            .expect(0)
            .mount_as_scoped(&github)
            .await;
        for (parent, code) in [
            ("#13", "parent_not_an_issue"),
            ("99", "parent_not_found"),
            ("acme/docs#5", "parent_outside_destination"),
            ("not a reference", "invalid_parent"),
        ] {
            let (status, body) =
                create_issue(&app, &owner, org.id, &request("p-0", parent, false)).await;
            assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{parent}: {body}");
            assert_eq!(body["error"]["details"]["field"], "parent", "{body}");
            assert_eq!(body["error"]["details"]["code"], code, "{parent}: {body}");
        }
    }

    // Created, then put under #12 with the author's token.
    let issue = mount_create(30, "p-1").await;
    mount_attach(&github, 30, 201, Some(1), 1).await;
    let (status, body) = create_issue(&app, &owner, org.id, &request("p-1", "#12", false)).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let data = &body["data"];
    assert_eq!(data["issue"]["number"], 30, "{body}");
    assert_eq!(
        data["parent"],
        json!({
            "externalIssueId": issue_id(12).to_string(), "number": 12,
            "displayKey": "acme/issues#12", "title": "Issue 12",
            "url": format!("{}/acme/issues/issues/12", github.uri()),
            "state": "attached", "message": null,
        })
    );
    // A replay finds the issue already under #12 and sends nothing.
    listed.lock().unwrap().push(issue);
    mount_parent(&github, 30, Some(issue_json(&github, 12, "Issue 12"))).await;
    let (status, body) = create_issue(&app, &owner, org.id, &request("p-1", "12", false)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["sent"], false);
    assert_eq!(body["data"]["parent"]["state"], "attached", "{body}");

    // GitHub refuses the parent: the issue stays created. A check reports it
    // unattached; a retry with the key attaches it without creating again.
    let issue = mount_create(31, "p-2").await;
    mount_attach(&github, 31, 422, Some(1), 1).await;
    let (status, body) = create_issue(&app, &owner, org.id, &request("p-2", "#12", false)).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["data"]["state"], "created");
    assert_eq!(body["data"]["parent"]["state"], "not_attached", "{body}");
    assert!(body["data"]["parent"]["message"].is_string(), "{body}");
    listed.lock().unwrap().push(issue);
    mount_parent(&github, 31, None).await;
    let (_, body) = create_issue(&app, &owner, org.id, &request("p-2", "#12", true)).await;
    assert_eq!(body["data"]["parent"]["state"], "not_attached", "{body}");
    mount_attach(&github, 31, 201, Some(1), 2).await;
    let (status, body) = create_issue(&app, &owner, org.id, &request("p-2", "#12", false)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["sent"], false);
    assert_eq!(body["data"]["issue"]["number"], 31);
    assert_eq!(body["data"]["parent"]["state"], "attached", "{body}");

    // An issue already under another parent is never moved.
    let payload = creation_marker(org.id, owner.id, "p-3");
    listed
        .lock()
        .unwrap()
        .push(created_issue_json(&github, 32, "Sub-task", &payload));
    mount_parent(&github, 32, Some(issue_json(&github, 15, "Issue 15"))).await;
    let (_, body) = create_issue(&app, &owner, org.id, &request("p-3", "#12", false)).await;
    assert_eq!(body["data"]["parent"]["state"], "other_parent", "{body}");
    assert!(
        body["data"]["parent"]["message"]
            .as_str()
            .unwrap()
            .contains("acme/issues#15"),
        "{body}"
    );

    // No answer from GitHub leaves the parent unknown; the issue exists.
    mount_create(33, "p-4").await;
    mount_attach(&github, 33, 502, Some(1), 1).await;
    let (_, body) = create_issue(&app, &owner, org.id, &request("p-4", "#12", false)).await;
    assert_eq!(body["data"]["state"], "created");
    assert_eq!(body["data"]["parent"]["state"], "outcome_unknown", "{body}");

    github.verify().await;
    app.cleanup().await;
}

#[tokio::test]
async fn the_workspace_agent_creates_issues_as_its_owner_and_links_them_explicitly() {
    use g5_api::services::issue_creations::creation_marker;
    use wiremock::matchers::body_partial_json;
    let github = MockServer::start().await;
    mount_github(&github).await;
    mount_repository_visibility(&github, "private").await;
    let app = spawn_with_github(&github).await;
    let (owner, org) = org_with_domain(&app).await;
    enable_comments(&app, &owner, org.id).await;
    connect_github(&app, &github, &owner, "ghu_owner", 28_800).await;
    let workspace = insert_workspace(&app, org.id, owner.id, "agent-creates", false).await;
    let token = agent_token(&app, org.id, workspace, owner.id).await;

    // `@me` is the owner, the owner's token sends, and the marker derives
    // from the agent's namespaced key.
    let marker = creation_marker(org.id, owner.id, "agent:a-1");
    assert_ne!(marker, creation_marker(org.id, owner.id, "a-1"));
    mount_created_by(&github, GITHUB_LOGIN, json!([]), None).await;
    Mock::given(method("POST"))
        .and(path(format!("/repositories/{ISSUES_REPO}/issues")))
        .and(header("authorization", "Bearer ghu_owner"))
        .and(body_partial_json(json!({
            "title": "Follow-up: tighten retries",
            "body": format!("Found while fixing #12\n\n{marker}"),
            "assignees": [GITHUB_LOGIN],
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(created_issue_json(
            &github,
            40,
            "Follow-up: tighten retries",
            &format!("Found while fixing #12\n\n{marker}"),
        )))
        .expect(1)
        .mount(&github)
        .await;
    let issues_path = format!("/api/internal/workspaces/{workspace}/issues");
    let (status, body) = agent_post(
        &app,
        &token,
        &issues_path,
        &json!({
            "title": "Follow-up: tighten retries",
            "body": "Found while fixing #12",
            "assignees": ["@me"],
            "idempotencyKey": "a-1",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(
        body["data"]["issue"]["url"].as_str().unwrap(),
        format!("{}/acme/issues/issues/40", github.uri())
    );
    assert_eq!(body["data"]["idempotencyKey"], "a-1");

    // The agent creates a sub-issue of an issue in the creation target.
    let marker = creation_marker(org.id, owner.id, "agent:a-2");
    Mock::given(method("POST"))
        .and(path(format!("/repositories/{ISSUES_REPO}/issues")))
        .and(body_partial_json(json!({"body": marker})))
        .respond_with(
            ResponseTemplate::new(201)
                .set_body_json(created_issue_json(&github, 41, "Sub-task", &marker)),
        )
        .expect(1)
        .mount(&github)
        .await;
    mount_attach(&github, 41, 201, Some(1), 1).await;
    let (status, body) = agent_post(
        &app,
        &token,
        &issues_path,
        &json!({"title": "Sub-task", "idempotencyKey": "a-2", "parent": "12"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["data"]["parent"]["state"], "attached", "{body}");
    assert_eq!(body["data"]["parent"]["displayKey"], "acme/issues#12");

    // Creating does not link; the agent links explicitly, as related by
    // default and by a bare number in the creation target.
    let links_path = format!("/api/internal/workspaces/{workspace}/issue-links");
    let (status, body) = json_of(
        reqwest::Client::new()
            .get(format!("{}{links_path}", app.base_url))
            .bearer_auth(&token)
            .send()
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["links"], json!([]));
    let (status, body) = agent_post(
        &app,
        &token,
        &links_path,
        &json!({"ref": "#12", "role": "related"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["data"]["role"], "related", "{body}");
    assert_eq!(body["data"]["displayKey"], "acme/issues#12");
    let (status, body) = agent_post(
        &app,
        &token,
        &links_path,
        &json!({"ref": "acme/issues#15", "role": "primary"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["data"]["role"], "primary", "{body}");
    // The primary bounds hold for the agent as for the owner.
    let (status, body) = agent_post(
        &app,
        &token,
        &links_path,
        &json!({"ref": "acme/issues#16", "role": "primary"}),
    )
    .await;
    assert_conflict_code(status, &body, "workspace_has_primary");
    let (status, body) = agent_post(
        &app,
        &token,
        &links_path,
        &json!({"ref": "16", "role": "owner"}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    // Another workspace's token cannot act here.
    let other = insert_workspace(&app, org.id, owner.id, "agent-other", false).await;
    let other_token = agent_token(&app, org.id, other, owner.id).await;
    let (status, _) = agent_post(
        &app,
        &other_token,
        &links_path,
        &json!({"ref": "#17", "role": "related"}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    github.verify().await;
    app.cleanup().await;
}

#[tokio::test]
async fn the_workspace_agent_comments_on_an_unlinked_github_issue_as_its_owner() {
    let github = MockServer::start().await;
    mount_github(&github).await;
    mount_repository_visibility(&github, "private").await;
    let app = spawn_with_github(&github).await;
    let (owner, org) = org_with_domain(&app).await;
    enable_comments(&app, &owner, org.id).await;
    connect_github(&app, &github, &owner, "ghu_owner", 28_800).await;
    let workspace = insert_workspace(&app, org.id, owner.id, "agent-any-issue", false).await;
    let token = agent_token(&app, org.id, workspace, owner.id).await;
    let comments_url = format!("/api/internal/workspaces/{workspace}/issue-comments");

    // Without a reference and without a primary issue, nothing is posted.
    let (status, body) = agent_post(
        &app,
        &token,
        &comments_url,
        &json!({"body": "Hello", "idempotencyKey": "any-0"}),
    )
    .await;
    assert_conflict_code(status, &body, "no_primary_issue");

    mount_no_issue_comments(&github, 15).await;
    Mock::given(method("POST"))
        .and(path(format!(
            "/repositories/{ISSUES_REPO}/issues/15/comments"
        )))
        .and(header("authorization", "Bearer ghu_owner"))
        .and(body_string_contains("<!-- g5-comment "))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "id": 9401,
            "user": {"id": GITHUB_USER_ID, "login": GITHUB_LOGIN},
            "body": "Seen in another workspace.",
            "created_at": "2026-09-26T20:00:00Z",
            "updated_at": "2026-09-26T20:00:00Z",
            "html_url": format!("{}/acme/issues/issues/15#issuecomment-9401", github.uri()),
        })))
        .expect(2)
        .mount(&github)
        .await;
    // `g5d issue comment acme/issues#15`, and `g5d issue comment 15` (a bare
    // number names the creation repository): the same guarded path as the
    // primary issue, recorded before it is sent.
    for (reference, key) in [("acme/issues#15", "any-1"), ("15", "any-2")] {
        let (status, posted) = agent_post(
            &app,
            &token,
            &comments_url,
            &json!({"body": "Seen in another workspace.", "idempotencyKey": key, "ref": reference}),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{reference}: {posted}");
        assert_eq!(posted["data"]["state"], "published", "{posted}");
        assert_eq!(posted["data"]["displayKey"], "acme/issues#15");
        assert_eq!(posted["data"]["origin"], "agent");
    }
    // An issue outside the configured domains is refused.
    let (status, body) = agent_post(
        &app,
        &token,
        &comments_url,
        &json!({"body": "x", "idempotencyKey": "any-3", "ref": "elsewhere/repo#1"}),
    )
    .await;
    assert_issue_error(
        status,
        &body,
        StatusCode::UNPROCESSABLE_ENTITY,
        "outside_domain",
    );

    app.cleanup().await;
}

/// `g5.external-issues.edit`: the agent edits an unlinked GitHub issue as
/// its owner through the comment guards, with one write whose answer
/// decides the outcome; `close` reaches the same issue.
#[tokio::test]
async fn the_workspace_agent_edits_and_closes_an_unlinked_github_issue_as_its_owner() {
    let github = MockServer::start().await;
    mount_github(&github).await;
    let app = spawn_with_github(&github).await;
    let (owner, org) = org_with_domain(&app).await;
    let workspace = insert_workspace(&app, org.id, owner.id, "agent-edit", false).await;
    let token = agent_token(&app, org.id, workspace, owner.id).await;
    let edit_url = format!("/api/internal/workspace-agent/{workspace}/issue-edit");
    let marker = "<!-- g5-issue 0123456789abcdef0123456789abcdef -->";
    let mount_current = |github: &MockServer| {
        let mut current = issue_json(github, 15, "Before");
        current["body"] = json!(format!("Steps\n\n{marker}"));
        current["assignees"] = json!([{"login": "fixer"}]);
        Mock::given(method("GET"))
            .and(path(format!("/repositories/{ISSUES_REPO}/issues/15")))
            .respond_with(ResponseTemplate::new(200).set_body_json(current))
            .with_priority(1)
    };
    let edit = json!({"ref": "acme/issues#15", "title": "Renamed", "body": "New",
        "addAssignees": ["octo"], "removeAssignees": ["fixer"]});

    // The comment guards refuse before anything is sent.
    let (status, body) = agent_post(&app, &token, &edit_url, &edit).await;
    assert_conflict_code(status, &body, "comments_disabled");
    enable_comments(&app, &owner, org.id).await;
    mount_repository_visibility(&github, "public").await;
    let (status, body) = agent_post(&app, &token, &edit_url, &edit).await;
    assert_conflict_code(status, &body, "public_destination");
    assert_no_patch(&github).await;
    github.reset().await;
    mount_github(&github).await;
    mount_repository_visibility(&github, "private").await;
    connect_github(&app, &github, &owner, "ghu_owner", 28_800).await;
    mount_current(&github).mount(&github).await;
    for (login, status) in [("octo", 204), ("nobody", 404)] {
        Mock::given(method("GET"))
            .and(path(format!(
                "/repositories/{ISSUES_REPO}/assignees/{login}"
            )))
            .respond_with(ResponseTemplate::new(status))
            .mount(&github)
            .await;
    }

    // A secret, a login GitHub cannot assign, and a login both added and
    // removed are refused with nothing sent.
    let secret = format!("ghp_{}", "a".repeat(36));
    for (request, field) in [
        (
            json!({"ref": "acme/issues#15", "body": format!("leak {secret}")}),
            "body",
        ),
        (
            json!({"ref": "acme/issues#15", "addAssignees": ["nobody"]}),
            "assignees",
        ),
        (
            json!({"ref": "acme/issues#15", "addAssignees": ["@me"], "removeAssignees": [GITHUB_LOGIN]}),
            "assignees",
        ),
    ] {
        let (status, body) = agent_post(&app, &token, &edit_url, &request).await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{request}: {body}"
        );
        assert_eq!(body["error"]["details"]["field"], field, "{body}");
        assert!(!body.to_string().contains(&secret), "{body}");
    }
    assert_no_patch(&github).await;

    // One write with the owner's token: the new title, the description with
    // its creation marker kept, and the assignee set computed from the issue.
    let mut answered = issue_json(&github, 15, "Renamed");
    answered["assignees"] = json!([{"login": "octo"}]);
    Mock::given(method("PATCH"))
        .and(path(format!("/repositories/{ISSUES_REPO}/issues/15")))
        .and(header("authorization", "Bearer ghu_owner"))
        .and(body_json(json!({
            "title": "Renamed",
            "body": format!("New\n\n{marker}"),
            "assignees": ["octo"],
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(answered))
        .expect(1)
        .named("one edit")
        .mount(&github)
        .await;
    let (status, body) = agent_post(&app, &token, &edit_url, &edit).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["outcome"], "applied", "{body}");
    assert_eq!(body["data"]["githubLogin"], GITHUB_LOGIN);
    assert_eq!(body["data"]["issue"]["title"], "Renamed");
    assert_eq!(body["data"]["issue"]["displayKey"], "acme/issues#15");
    github.verify().await;

    // GitHub dropping an assignee is not reported as applied.
    github.reset().await;
    mount_github(&github).await;
    mount_repository_visibility(&github, "private").await;
    mount_current(&github).mount(&github).await;
    Mock::given(method("GET"))
        .and(path(format!("/repositories/{ISSUES_REPO}/assignees/octo")))
        .respond_with(ResponseTemplate::new(204))
        .mount(&github)
        .await;
    let mut dropped = issue_json(&github, 15, "Before");
    dropped["assignees"] = json!([{"login": "fixer"}]);
    Mock::given(method("PATCH"))
        .and(path(format!("/repositories/{ISSUES_REPO}/issues/15")))
        .and(body_json(json!({"assignees": ["fixer", "octo"]})))
        .respond_with(ResponseTemplate::new(200).set_body_json(dropped))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&github)
        .await;
    let add_octo = json!({"ref": "acme/issues#15", "addAssignees": ["octo"]});
    let (status, body) = agent_post(&app, &token, &edit_url, &add_octo).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["outcome"], "partial", "{body}");
    assert_eq!(body["data"]["notAssigned"], json!(["octo"]), "{body}");

    // No answer is an unknown outcome, never success.
    Mock::given(method("PATCH"))
        .and(path(format!("/repositories/{ISSUES_REPO}/issues/15")))
        .respond_with(ResponseTemplate::new(502))
        .with_priority(2)
        .mount(&github)
        .await;
    let (status, body) = agent_post(&app, &token, &edit_url, &add_octo).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["outcome"], "unknown", "{body}");
    assert!(body["data"]["issue"].is_null(), "{body}");

    // `g5d issue close acme/issues#15`: an issue no workspace links.
    github.reset().await;
    mount_github(&github).await;
    mount_repository_visibility(&github, "private").await;
    let mut closed = issue_json(&github, 15, "Before");
    closed["state"] = json!("closed");
    closed["state_reason"] = json!("completed");
    Mock::given(method("PATCH"))
        .and(path(format!("/repositories/{ISSUES_REPO}/issues/15")))
        .and(header("authorization", "Bearer ghu_owner"))
        .and(body_json(
            json!({"state": "closed", "state_reason": "completed"}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(closed))
        .expect(1)
        .mount(&github)
        .await;
    let (status, body) = agent_post(
        &app,
        &token,
        &format!("/api/internal/workspace-agent/{workspace}/issue-state"),
        &json!({"ref": "acme/issues#15", "state": "closed"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["outcome"], "applied", "{body}");
    assert_eq!(body["data"]["issue"]["state"], "closed");
    github.verify().await;

    app.cleanup().await;
}

/// `g5.issue.native.page` for GitHub issues: a member edits, closes and
/// comments on a GitHub issue from its G5 page as their own GitHub account,
/// through the comment guards; an edit is refused when a changed field no
/// longer holds what the form showed.
#[tokio::test]
async fn a_person_writes_a_github_issue_from_its_page_as_themselves() {
    let github = MockServer::start().await;
    mount_github(&github).await;
    let app = spawn_with_github(&github).await;
    let (owner, org) = org_with_domain(&app).await;
    enable_comments(&app, &owner, org.id).await;
    mount_repository_visibility(&github, "private").await;
    let page = format!("/api/v1/orgs/{}/issues/github/{ISSUES_REPO}/15", org.id);
    let client = app.authenticated_client(&owner);
    let mut current = issue_json(&github, 15, "Before");
    current["body"] = json!("Steps\n\n<!-- g5-issue 0123456789abcdef0123456789abcdef -->");
    current["assignees"] = json!([{"login": "fixer"}]);
    Mock::given(method("GET"))
        .and(path(format!("/repositories/{ISSUES_REPO}/issues/15")))
        .respond_with(ResponseTemplate::new(200).set_body_json(current.clone()))
        .with_priority(1)
        .mount(&github)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/repositories/{ISSUES_REPO}/assignees")))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!([{"login": "fixer"}, {"login": "octo"}])),
        )
        .mount(&github)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/repositories/{ISSUES_REPO}/assignees/octo")))
        .respond_with(ResponseTemplate::new(204))
        .mount(&github)
        .await;

    // What the page offers: connect first, then ready with the assignees.
    let (status, target) = json_of(client.get(&format!("{page}/write-target")).await).await;
    assert_eq!(status, StatusCode::OK, "{target}");
    assert_eq!(target["data"]["state"], "connection_required", "{target}");
    assert_eq!(target["data"]["displayKey"], "acme/issues#15");
    assert_eq!(
        target["data"]["destination"]["externalIssueId"],
        issue_id(15).to_string()
    );
    connect_github(&app, &github, &owner, "ghu_owner", 28_800).await;
    let (_, target) = json_of(client.get(&format!("{page}/write-target")).await).await;
    assert_eq!(target["data"]["state"], "ready", "{target}");
    assert_eq!(target["data"]["githubLogin"], GITHUB_LOGIN);
    assert_eq!(target["data"]["assignableUsers"][1]["login"], "octo");

    // A changed field must still hold what the form showed; a missing
    // `seen` value is refused. Nothing is sent.
    let (status, body) = json_of(
        client
            .patch_json(
                &page,
                &json!({"title": "Renamed", "seen": {"title": "Older"}}),
            )
            .await,
    )
    .await;
    assert_conflict_code(status, &body, "issue_changed");
    let (status, body) = json_of(
        client
            .patch_json(&page, &json!({"title": "Renamed", "seen": {}}))
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["error"]["details"]["field"], "seen.title", "{body}");
    assert_no_patch(&github).await;

    // One write: the title, the description with its creation marker kept,
    // and the new assignee set; the form's description is compared without
    // the marker.
    let mut answered = issue_json(&github, 15, "Renamed");
    answered["assignees"] = json!([{"login": "octo"}]);
    Mock::given(method("PATCH"))
        .and(path(format!("/repositories/{ISSUES_REPO}/issues/15")))
        .and(header("authorization", "Bearer ghu_owner"))
        .and(body_json(json!({
            "title": "Renamed",
            "body": "New steps\n\n<!-- g5-issue 0123456789abcdef0123456789abcdef -->",
            "assignees": ["octo"],
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(answered))
        .expect(1)
        .named("page edit")
        .mount(&github)
        .await;
    let (status, edited) = json_of(
        client
            .patch_json(
                &page,
                &json!({
                    "title": "Renamed",
                    "body": "New steps",
                    "assignees": ["octo"],
                    "seen": {"title": "Before", "body": "Steps", "assignees": ["FIXER"]},
                }),
            )
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{edited}");
    assert_eq!(edited["data"]["outcome"], "applied", "{edited}");
    assert_eq!(edited["data"]["githubLogin"], GITHUB_LOGIN);
    assert_eq!(edited["data"]["issue"]["title"], "Renamed");

    // Close as completed, though no workspace links the issue.
    let mut closed = issue_json(&github, 15, "Renamed");
    closed["state"] = json!("closed");
    closed["state_reason"] = json!("completed");
    Mock::given(method("PATCH"))
        .and(path(format!("/repositories/{ISSUES_REPO}/issues/15")))
        .and(body_json(
            json!({"state": "closed", "state_reason": "completed"}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(closed))
        .expect(1)
        .named("page close")
        .mount(&github)
        .await;
    let (status, changed) = json_of(
        client
            .post_json(&format!("{page}/state"), &json!({"state": "closed"}))
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{changed}");
    assert_eq!(changed["data"]["issue"]["state"], "closed", "{changed}");

    // Comment, then post again with the same key: found, not resent.
    mount_no_issue_comments(&github, 15).await;
    Mock::given(method("POST"))
        .and(path(format!(
            "/repositories/{ISSUES_REPO}/issues/15/comments"
        )))
        .and(header("authorization", "Bearer ghu_owner"))
        .and(body_string_contains("From the page."))
        .respond_with(ResponseTemplate::new(201).set_body_json(github_comment(
            &github,
            9501,
            ME,
            "From the page.",
            15,
            "2026-10-29T10:00:00Z",
        )))
        .expect(1)
        .mount(&github)
        .await;
    let comment = json!({"body": "From the page.", "idempotencyKey": "page-1"});
    let (status, posted) = json_of(
        client
            .post_json(&format!("{page}/comments"), &comment)
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{posted}");
    assert_eq!(posted["data"]["state"], "published", "{posted}");
    assert_eq!(posted["data"]["id"], "9501");
    let payload = posted["data"]["payload"].as_str().unwrap().to_string();
    mount_issue_comments(
        &github,
        15,
        json!([github_comment(
            &github,
            9501,
            ME,
            &payload,
            15,
            "2026-10-29T10:00:00Z"
        )]),
    )
    .await;
    let (status, replay) = json_of(
        client
            .post_json(&format!("{page}/comments"), &comment)
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{replay}");
    assert_eq!(replay["data"]["id"], "9501");

    // Edit that comment: the person's own, on this issue, as seen.
    mount_github_comment(
        &github,
        github_comment(&github, 9501, ME, &payload, 15, "2026-10-29T10:00:00Z"),
    )
    .await;
    let reference = payload.rsplit("\n\n").next().unwrap().to_string();
    Mock::given(method("PATCH"))
        .and(path(format!(
            "/repositories/{ISSUES_REPO}/issues/comments/9501"
        )))
        .and(body_json(
            json!({"body": format!("Edited.\n\n{reference}")}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(github_comment(
            &github,
            9501,
            ME,
            &format!("Edited.\n\n{reference}"),
            15,
            "2026-10-29T10:05:00Z",
        )))
        .expect(1)
        .mount(&github)
        .await;
    let (status, edited) = json_of(
        client
            .patch_json(
                &format!("{page}/comments/9501"),
                &json!({"body": "Edited.", "expectedUpdatedAt": "2026-10-29T10:00:00Z"}),
            )
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{edited}");
    assert_eq!(edited["data"]["kind"], "edit");
    assert_eq!(edited["data"]["state"], "published", "{edited}");

    // Viewers read only; a repository outside the domains is refused.
    let viewer = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    sqlx::query("UPDATE org_memberships SET role='viewer' WHERE org_id=$1 AND user_id=$2")
        .bind(org.id)
        .bind(viewer.id)
        .execute(&app.db)
        .await
        .unwrap();
    let (status, _) = json_of(
        app.authenticated_client(&viewer)
            .post_json(&format!("{page}/state"), &json!({"state": "open"}))
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, body) = json_of(
        client
            .post_json(
                &format!(
                    "/api/v1/orgs/{}/issues/github/{UNCOVERED_REPO}/1/state",
                    org.id
                ),
                &json!({"state": "closed"}),
            )
            .await,
    )
    .await;
    assert!(status.is_client_error(), "{status}: {body}");

    github.verify().await;
    app.cleanup().await;
}

#[tokio::test]
async fn g5_issues_and_github_domains_search_together_and_report_failures_separately() {
    let github = MockServer::start().await;
    mount_github(&github).await;
    let app = spawn_with_github(&github).await;
    let (owner, org) = org_with_domain(&app).await;
    // The GitHub domain cannot be read: its binding is gone.
    seed_binding(&app, org.id, owner.id, "disconnected", json!({}), &[]).await;
    let (status, native) = json_of(
        app.authenticated_client(&owner)
            .post_json(
                &domains_path(org.id),
                &json!({"provider": "g5", "key": "ACME"}),
            )
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{native}");
    // The GitHub repository stays the creation default.
    assert_eq!(native["data"]["isDefaultCreateTarget"], false, "{native}");
    for statement in [
        "UPDATE issue_domains SET is_default_create_target = false WHERE org_id = $1",
        "UPDATE issue_domains SET is_default_create_target = true WHERE org_id = $1 AND provider = 'g5'",
    ] {
        sqlx::query(statement)
            .bind(org.id)
            .execute(&app.db)
            .await
            .unwrap();
    }
    let (status, created) = json_of(
        app.authenticated_client(&owner)
            .post_json(
                &format!("/api/v1/orgs/{}/issues", org.id),
                &json!({"title": "Crash in the native tracker", "assignees": ["@me"],
                        "idempotencyKey": "mixed-1", "domainId": native["data"]["id"]}),
            )
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");

    // Native results come back beside the GitHub domain's error, never as
    // a complete result.
    for query in ["q=crash", "assignee=me"] {
        let (status, search) = json_of(
            app.authenticated_client(&owner)
                .get(&format!("/api/v1/orgs/{}/issues/search?{query}", org.id))
                .await,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{query}: {search}");
        assert_eq!(search["data"]["domainCount"], 2, "{search}");
        assert_eq!(
            search["data"]["results"][0]["displayKey"], "ACME-1",
            "{search}"
        );
        assert_eq!(
            search["data"]["results"].as_array().unwrap().len(),
            1,
            "{search}"
        );
        assert_eq!(
            search["data"]["domainErrors"].as_array().unwrap().len(),
            1,
            "{search}"
        );
        assert_eq!(
            search["data"]["domainErrors"][0]["displayKey"],
            "acme/issues"
        );
    }

    app.cleanup().await;
}

/// GET as the workspace agent, returning the raw response.
async fn agent_get_raw(app: &TestApp, token: &str, path: &str) -> reqwest::Response {
    reqwest::Client::new()
        .get(format!("{}{path}", app.base_url))
        .bearer_auth(token)
        .send()
        .await
        .unwrap()
}

/// `g5.attachments`, `g5.attachments.github`: the agent lists the
/// attachments of any issue `view` reads and receives each one, streamed
/// by G5 from GitHub's own hosts without a credential: an image and a video
/// from the rendering's signed links, a public file, and a private file
/// GitHub serves only on its site. A changed text, a page standing in for
/// an image, another workspace and an issue outside the domains are
/// refused.
#[tokio::test]
async fn the_workspace_agent_receives_a_github_issue_s_private_attachments_through_g5() {
    let github = MockServer::start().await;
    mount_github(&github).await;
    let uri = github.uri();
    let full = "application/vnd.github.full+json";
    let description = "![Help popup](https://github.com/user-attachments/assets/a)\n\n<img alt=\"Mode\" src=\"https://github.com/user-attachments/assets/b\">\n\nhttps://github.com/user-attachments/assets/v\n\n[spec.pdf](https://github.com/user-attachments/files/5/spec.pdf) [log.txt](https://github.com/user-attachments/files/6/log.txt)";
    let mut issue = issue_json(&github, 21, "Mocks");
    issue["body"] = json!(description);
    issue["body_html"] = json!(format!(
        r#"<p><a href="{uri}/user-attachments/assets/a"><img src="{uri}/user-attachments/assets/a" alt="Help popup"></a></p>
<p><img alt="Mode" src="{uri}/signed/b.jpg?jwt=t&amp;x=1"></p>
<details><summary>v.mp4</summary><video src="{uri}/signed/v.mp4?jwt=t" controls></video></details>
<p><a href="{uri}/user-attachments/files/5/spec.pdf">spec.pdf</a> <a href="{uri}/user-attachments/files/6/log.txt">log.txt</a></p>"#
    ));
    issue["comments"] = json!(1);
    Mock::given(method("GET"))
        .and(path(format!("/repositories/{ISSUES_REPO}/issues/21")))
        .and(header("accept", full))
        .respond_with(ResponseTemplate::new(200).set_body_json(issue))
        .with_priority(1)
        .mount(&github)
        .await;
    let comment = json!({
        "id": 9501, "body": "Dropdown: ![mode](https://github.com/user-attachments/assets/c)",
        "body_html": format!(r#"<p>Dropdown: <img src="{uri}/signed/c.webp" alt="mode"></p>"#),
        "user": {"login": "reporter"},
        "created_at": "2026-01-02T00:00:00Z", "updated_at": "2026-01-02T00:00:00Z",
        "html_url": format!("{uri}/acme/issues/issues/21#issuecomment-9501"),
        "issue_url": format!("{uri}/repositories/{ISSUES_REPO}/issues/21"),
    });
    Mock::given(method("GET"))
        .and(path(format!(
            "/repositories/{ISSUES_REPO}/issues/21/comments"
        )))
        .and(header("accept", full))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([comment.clone()])))
        .with_priority(1)
        .mount(&github)
        .await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/repositories/{ISSUES_REPO}/issues/comments/9501"
        )))
        .and(header("accept", full))
        .respond_with(ResponseTemplate::new(200).set_body_json(comment))
        .mount(&github)
        .await;
    let png = b"\x89PNG\r\n\x1a\nprivate mock".to_vec();
    let webp = b"RIFF\x10\0\0\0WEBPVP8 mock".to_vec();
    let mp4 = b"\0\0\0\x18ftypmp42\0\0\0\0recording".to_vec();
    Mock::given(method("GET"))
        .and(path("/user-attachments/assets/a"))
        .respond_with(ResponseTemplate::new(302).insert_header("location", "/signed/a.png?jwt=t"))
        .mount(&github)
        .await;
    for (at, response) in [
        (
            "/signed/a.png",
            ResponseTemplate::new(200).set_body_bytes(png.clone()),
        ),
        (
            "/signed/b.jpg",
            ResponseTemplate::new(200).set_body_bytes(b"<!doctype html><p>sign in</p>".to_vec()),
        ),
        (
            "/signed/c.webp",
            ResponseTemplate::new(200).set_body_bytes(webp.clone()),
        ),
        (
            "/signed/v.mp4",
            ResponseTemplate::new(200).set_body_bytes(mp4.clone()),
        ),
        (
            "/user-attachments/files/5/spec.pdf",
            ResponseTemplate::new(404),
        ),
        (
            "/user-attachments/files/6/log.txt",
            ResponseTemplate::new(200).set_body_raw(b"step 1 ok\n".to_vec(), "text/plain"),
        ),
    ] {
        Mock::given(method("GET"))
            .and(path(at))
            .respond_with(response)
            .mount(&github)
            .await;
    }

    let app = spawn_with_github(&github).await;
    let (owner, org) = org_with_domain(&app).await;
    let workspace = insert_workspace(&app, org.id, owner.id, "agent-attachments", false).await;
    let (status, body) = link(&app, &owner, workspace, "acme/issues#21", "primary").await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let token = agent_token(&app, org.id, workspace, owner.id).await;
    let list_path = format!("/api/internal/workspaces/{workspace}/issues/attachments");

    // The primary issue's description: five attachments by place and kind,
    // without content or signed links.
    let (status, listing) = agent_get(&app, &token, &list_path).await;
    assert_eq!(status, StatusCode::OK, "{listing}");
    let data = &listing["data"];
    assert_eq!(data["issue"]["displayKey"], "acme/issues#21");
    let found = data["attachments"].as_array().unwrap();
    let kinds: Vec<&str> = found.iter().map(|a| a["kind"].as_str().unwrap()).collect();
    assert_eq!(
        kinds,
        ["image", "image", "video", "file", "file"],
        "{listing}"
    );
    assert_eq!(found[0]["place"], "description");
    assert_eq!(found[0]["name"], "Help popup");
    assert_eq!(found[3]["name"], "spec.pdf");
    assert!(found.iter().all(|a| a["retrievable"] == true), "{listing}");
    assert!(!listing.to_string().contains("jwt"), "{listing}");
    let handle = |index: usize| found[index]["handle"].as_str().unwrap().to_string();

    // With comments, by reference: the comment's image follows.
    let (status, with_comments) = agent_get(
        &app,
        &token,
        &format!(
            "{list_path}?comments=true&ref={}",
            urlencode("acme/issues#21")
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{with_comments}");
    let all = with_comments["data"]["attachments"].as_array().unwrap();
    assert_eq!(all.len(), 6, "{with_comments}");
    assert_eq!(all[5]["place"], "comment");
    assert_eq!(all[5]["commentId"], "9501");
    assert_eq!(all[5]["author"]["login"], "reporter");
    let comment_handle = all[5]["handle"].as_str().unwrap().to_string();

    let content = |handle: String| {
        let token = token.clone();
        let app = &app;
        async move {
            agent_get_raw(
                app,
                &token,
                &format!(
                    "/api/internal/workspaces/{workspace}/issues/attachments/content?handle={}",
                    urlencode(&handle)
                ),
            )
            .await
        }
    };
    // The private image, the video and the public file arrive as streamed,
    // recognized bytes.
    for (handle, expected_type, expected) in [
        (handle(0), "image/png", png.clone()),
        (handle(2), "video/mp4", mp4.clone()),
        (handle(4), "text/plain", b"step 1 ok\n".to_vec()),
        (comment_handle, "image/webp", webp.clone()),
    ] {
        let response = content(handle.clone()).await;
        assert_eq!(response.status(), StatusCode::OK, "{handle}");
        assert_eq!(response.headers()["content-type"], expected_type);
        assert_eq!(response.headers()["x-content-type-options"], "nosniff");
        assert_eq!(
            response.bytes().await.unwrap().to_vec(),
            expected,
            "{handle}"
        );
    }
    // A page standing in for an image is refused, never passed on; a
    // private file is GitHub's only on its site.
    let response = content(handle(1)).await;
    assert_ne!(response.status(), StatusCode::OK);
    let response = content(handle(3)).await;
    let (status, body) = json_of(response).await;
    assert_issue_error(status, &body, StatusCode::FORBIDDEN, "access_denied");
    assert!(body.to_string().contains("only on its site"), "{body}");
    // A text that changed since it was listed, a missing place or a forged
    // handle is refused.
    let stale = handle(0).rsplit_once(':').unwrap().0.to_string() + ":0000000000000000";
    let (status, body) = json_of(content(stale).await).await;
    assert_conflict_code(status, &body, "issue_text_changed");
    let missing = handle(0).replacen(":1:", ":9:", 1);
    let (status, body) = json_of(content(missing).await).await;
    assert_issue_error(status, &body, StatusCode::NOT_FOUND, "not_found");
    let (status, body) = json_of(content("https://evil.example/x.png".into()).await).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");

    // An issue outside the domains, and another workspace's credential, are refused.
    let (status, body) = agent_get(
        &app,
        &token,
        &format!("{list_path}?ref={}", urlencode("elsewhere/repo#1")),
    )
    .await;
    assert_issue_error(
        status,
        &body,
        StatusCode::UNPROCESSABLE_ENTITY,
        "outside_domain",
    );
    let other = insert_workspace(&app, org.id, owner.id, "agent-attachments-other", false).await;
    let other_token = agent_token(&app, org.id, other, owner.id).await;
    let (status, _) = agent_get(&app, &other_token, &list_path).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // No content request carried a credential.
    for request in github.received_requests().await.unwrap() {
        let at = request.url.path();
        if at.starts_with("/user-attachments/") || at.starts_with("/signed/") {
            assert!(!request.headers.contains_key("authorization"), "{at}");
        }
    }
    app.cleanup().await;
}

/// `g5.attachments.cli.attach`: GitHub takes no uploads from G5, so the
/// agent's upload to a GitHub issue, and a creation that would attach
/// images where new issues go to GitHub, are refused before anything is
/// sent to GitHub.
#[tokio::test]
async fn the_agent_cannot_attach_images_where_issues_are_on_github() {
    use base64::Engine as _;
    let github = MockServer::start().await;
    mount_github(&github).await;
    Mock::given(method("GET"))
        .and(path(format!("/repositories/{ISSUES_REPO}/issues/21")))
        .respond_with(ResponseTemplate::new(200).set_body_json(issue_json(&github, 21, "Mocks")))
        .mount(&github)
        .await;
    let app = spawn_with_github(&github).await;
    let (owner, org) = org_with_domain(&app).await;
    let workspace = insert_workspace(&app, org.id, owner.id, "agent-github-uploads", false).await;
    let token = agent_token(&app, org.id, workspace, owner.id).await;
    let writes = || async {
        github
            .received_requests()
            .await
            .unwrap()
            .into_iter()
            .filter(|r| {
                r.method != wiremock::http::Method::GET && !r.url.path().ends_with("/access_tokens")
            })
            .count()
    };
    let before = writes().await;
    let (status, body) = agent_post(
        &app,
        &token,
        &format!("/api/internal/workspaces/{workspace}/issues/attachments"),
        &json!({
            "ref": "acme/issues#21", "fileName": "shot.png",
            "contentBase64": base64::engine::general_purpose::STANDARD.encode(b"\x89PNG\r\n\x1a\nx"),
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(body.to_string().contains("uploads_unsupported"), "{body}");
    let (status, body) = agent_post(
        &app,
        &token,
        &format!("/api/internal/workspaces/{workspace}/issues"),
        &json!({"title": "T", "body": "B", "idempotencyKey": "k-attach", "attachments": true}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(body.to_string().contains("uploads_unsupported"), "{body}");
    assert_eq!(writes().await, before, "nothing was sent to GitHub");
}

/// `g5.issue.labels.github`: the workspace agent keeps a GitHub domain's
/// label catalog and changes an issue's labels as its owner, through the
/// comment guards; every named label must already exist; additions are one
/// request and removals one each, and a refusal or unclear answer stops the
/// sequence with per-label outcomes rather than a claimed success.
#[tokio::test]
async fn the_workspace_agent_changes_github_labels_as_its_owner() {
    let github = MockServer::start().await;
    mount_github(&github).await;
    let app = spawn_with_github(&github).await;
    let (owner, org) = org_with_domain(&app).await;
    let workspace = insert_workspace(&app, org.id, owner.id, "agent-labels", false).await;
    let token = agent_token(&app, org.id, workspace, owner.id).await;
    let route = format!("/api/internal/workspace-agent/{workspace}");
    let labels_path = format!("/repositories/{ISSUES_REPO}/labels");
    let label =
        |id: u64, name: &str| json!({"id": id, "name": name, "color": "D73A4A", "description": ""});
    let mount_label = |name: &str, found: Option<Value>| {
        let mock = Mock::given(method("GET")).and(path(format!("{labels_path}/{name}")));
        match found {
            Some(body) => mock.respond_with(ResponseTemplate::new(200).set_body_json(body)),
            None => mock.respond_with(ResponseTemplate::new(404)),
        }
    };
    async fn writes(github: &MockServer) -> usize {
        github
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|request| {
                request.method.as_str() != "GET" && request.url.path().starts_with("/repositories/")
            })
            .count()
    }

    // The catalog is read with the installation token, a page at a time.
    Mock::given(method("GET"))
        .and(path(labels_path.clone()))
        .and(query_param("page", "1"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("link", "<https://x/labels?page=2>; rel=\"next\"")
                .set_body_json(json!([label(1, "bug"), label(2, "wip")])),
        )
        .mount(&github)
        .await;
    let (status, page) = agent_get(
        &app,
        &token,
        &format!("{route}/issue-labels?domain=acme/issues"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert_eq!(
        page["data"]["labels"][0],
        json!({"id": "1", "name": "bug", "color": "d73a4a"})
    );
    assert_eq!(page["data"]["nextPage"], 2);

    // Writes need the comment guards: nothing is sent while they refuse.
    let (status, body) = agent_post(
        &app,
        &token,
        &format!("{route}/issue-labels"),
        &json!({"name": "p1"}),
    )
    .await;
    assert_conflict_code(status, &body, "comments_disabled");
    let edit_url = format!("{route}/issue-edit");
    let change = json!({"ref": "acme/issues#15", "addLabels": ["Bug"], "removeLabels": ["wip"]});
    let (status, body) = agent_post(&app, &token, &edit_url, &change).await;
    assert_conflict_code(status, &body, "comments_disabled");
    enable_comments(&app, &owner, org.id).await;
    mount_repository_visibility(&github, "private").await;
    connect_github(&app, &github, &owner, "ghu_owner", 28_800).await;
    // A secret in a label is refused without echoing it.
    let secret = format!("ghp_{}", "a".repeat(36));
    let (status, body) = agent_post(
        &app,
        &token,
        &format!("{route}/issue-labels"),
        &json!({"name": "p1", "description": format!("x {secret}")}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["error"]["details"]["code"], "secret_detected");
    assert!(!body.to_string().contains(&secret));
    assert_eq!(writes(&github).await, 0);

    // Create as the owner; a name GitHub refuses as taken is reconciled by
    // reading it: the same label is the retry's success, another a conflict.
    Mock::given(method("POST"))
        .and(path(labels_path.clone()))
        .and(header("authorization", "Bearer ghu_owner"))
        .and(body_json(
            json!({"name": "priority:p1", "color": "fbca04", "description": ""}),
        ))
        .respond_with(ResponseTemplate::new(201).set_body_json(
            json!({"id": 3, "name": "priority:p1", "color": "fbca04", "description": null}),
        ))
        .up_to_n_times(1)
        .mount(&github)
        .await;
    let create = json!({"name": "priority:p1", "color": "#FBCA04"});
    let (status, body) = agent_post(&app, &token, &format!("{route}/issue-labels"), &create).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["outcome"], "applied");
    assert_eq!(body["data"]["githubLogin"], GITHUB_LOGIN);
    assert_eq!(body["data"]["label"]["id"], "3");
    Mock::given(method("POST"))
        .and(path(labels_path.clone()))
        .respond_with(ResponseTemplate::new(422).set_body_json(
            json!({"message": "Validation Failed", "errors": [{"code": "already_exists"}]}),
        ))
        .mount(&github)
        .await;
    mount_label(
        "priority:p1",
        Some(json!({"id": 3, "name": "priority:p1", "color": "fbca04"})),
    )
    .mount(&github)
    .await;
    let (status, body) = agent_post(&app, &token, &format!("{route}/issue-labels"), &create).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["label"]["id"], "3");
    let (status, body) = agent_post(
        &app,
        &token,
        &format!("{route}/issue-labels"),
        &json!({"name": "priority:p1", "color": "000000"}),
    )
    .await;
    assert_conflict_code(status, &body, "label_exists");
    assert_eq!(body["error"]["details"]["label"]["id"], "3");

    // Tagging: an unknown label refuses the change before any write.
    mount_label("Bug", Some(label(1, "bug")))
        .mount(&github)
        .await;
    mount_label("wip", Some(label(2, "wip")))
        .mount(&github)
        .await;
    mount_label("nope", None).mount(&github).await;
    mount_issue(&github, 15).await;
    let before = writes(&github).await;
    let (status, body) = agent_post(
        &app,
        &token,
        &edit_url,
        &json!({"ref": "acme/issues#15", "addLabels": ["nope"]}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["error"]["details"]["code"], "unknown_label");
    assert_eq!(writes(&github).await, before);
    // A field edit GitHub cannot make (an unassignable login) refuses the
    // whole request before any label is written.
    let (status, body) = agent_post(
        &app,
        &token,
        &edit_url,
        &json!({"ref": "acme/issues#15", "addLabels": ["Bug"], "addAssignees": ["nobody"]}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["error"]["details"]["field"], "assignees", "{body}");
    assert_eq!(writes(&github).await, before);

    // One add with the catalog's names, then one removal each.
    let issue_labels = format!("/repositories/{ISSUES_REPO}/issues/15/labels");
    Mock::given(method("POST"))
        .and(path(issue_labels.clone()))
        .and(header("authorization", "Bearer ghu_owner"))
        .and(body_json(json!({"labels": ["bug"]})))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!([label(1, "bug"), label(2, "wip")])),
        )
        .up_to_n_times(1)
        .mount(&github)
        .await;
    Mock::given(method("DELETE"))
        .and(path(format!("{issue_labels}/wip")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([label(1, "bug")])))
        .up_to_n_times(1)
        .mount(&github)
        .await;
    let (status, body) = agent_post(&app, &token, &edit_url, &change).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["outcome"], "applied", "{body}");
    assert_eq!(
        body["data"]["labels"]["changes"],
        json!([
            {"label": "bug", "change": "add", "outcome": "applied"},
            {"label": "wip", "change": "remove", "outcome": "applied"}
        ])
    );
    assert_eq!(body["data"]["githubLogin"], GITHUB_LOGIN);
    assert_eq!(body["data"]["issue"]["displayKey"], "acme/issues#15");

    // A refused removal after an applied add is partial, the rest not
    // attempted; nothing is rolled back.
    Mock::given(method("POST"))
        .and(path(issue_labels.clone()))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([label(1, "bug")])))
        .up_to_n_times(1)
        .mount(&github)
        .await;
    Mock::given(method("DELETE"))
        .and(path(format!("{issue_labels}/wip")))
        .respond_with(
            ResponseTemplate::new(403).set_body_json(json!({"message": "Must have triage access"})),
        )
        .up_to_n_times(1)
        .mount(&github)
        .await;
    let (status, body) = agent_post(
        &app,
        &token,
        &edit_url,
        &json!({"ref": "acme/issues#15", "addLabels": ["Bug"], "removeLabels": ["wip", "priority:p1"]}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["outcome"], "partial", "{body}");
    let outcomes: Vec<&str> = body["data"]["labels"]["changes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|change| change["outcome"].as_str().unwrap())
        .collect();
    assert_eq!(outcomes, vec!["applied", "refused", "not_attempted"]);

    // A refusal before anything applied is the error; no answer is unknown.
    Mock::given(method("POST"))
        .and(path(issue_labels.clone()))
        .respond_with(ResponseTemplate::new(403).set_body_json(json!({"message": "no"})))
        .up_to_n_times(1)
        .mount(&github)
        .await;
    let (status, body) = agent_post(
        &app,
        &token,
        &edit_url,
        &json!({"ref": "acme/issues#15", "addLabels": ["Bug"]}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    Mock::given(method("POST"))
        .and(path(issue_labels.clone()))
        .respond_with(ResponseTemplate::new(502))
        .up_to_n_times(1)
        .mount(&github)
        .await;
    let (status, body) = agent_post(&app, &token, &edit_url, &change).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["outcome"], "unknown", "{body}");
    let outcomes: Vec<&str> = body["data"]["labels"]["changes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|change| change["outcome"].as_str().unwrap())
        .collect();
    assert_eq!(outcomes, vec!["unknown", "not_attempted"]);

    // Catalog edit and delete name the label; GitHub addresses it by name.
    mount_label("doing", None).mount(&github).await;
    Mock::given(method("PATCH"))
        .and(path(format!("{labels_path}/wip")))
        .and(body_json(json!({"new_name": "doing"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(label(2, "doing")))
        .expect(1)
        .mount(&github)
        .await;
    let (status, body) = agent_post(
        &app,
        &token,
        &format!("{route}/issue-labels/edit"),
        &json!({"label": "wip", "name": "doing"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["label"]["name"], "doing");
    Mock::given(method("DELETE"))
        .and(path(format!("{labels_path}/wip")))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&github)
        .await;
    let (status, body) = agent_post(
        &app,
        &token,
        &format!("{route}/issue-labels/delete"),
        &json!({"label": "wip"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["outcome"], "applied");
    github.verify().await;

    app.cleanup().await;
}

/// Labels selected when creating a GitHub issue must exist; GitHub silently
/// drops labels the author may not set, so a created issue missing them is
/// reported, and a person tags a GitHub issue from its page.
#[tokio::test]
async fn github_issues_take_labels_at_creation_and_from_their_page() {
    use wiremock::matchers::body_partial_json;
    let github = MockServer::start().await;
    mount_github(&github).await;
    mount_repository_visibility(&github, "private").await;
    let app = spawn_with_github(&github).await;
    let (owner, org) = org_with_domain(&app).await;
    enable_comments(&app, &owner, org.id).await;
    connect_github(&app, &github, &owner, "ghu_owner", 28_800).await;
    let domain_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM issue_domains WHERE org_id = $1 AND is_default_create_target",
    )
    .bind(org.id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    let labels_path = format!("/repositories/{ISSUES_REPO}/labels");
    Mock::given(method("GET"))
        .and(path(format!("{labels_path}/BUG")))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"id": 1, "name": "bug", "color": "d73a4a"})),
        )
        .mount(&github)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("{labels_path}/nope")))
        .respond_with(ResponseTemplate::new(404))
        .mount(&github)
        .await;
    mount_created_by(&github, GITHUB_LOGIN, json!([]), None).await;
    let request = |labels: Value, key: &str| json!({"title": "Crash", "body": "Steps", "labels": labels, "idempotencyKey": key, "domainId": domain_id});
    let (status, body) = create_issue(&app, &owner, org.id, &request(json!(["nope"]), "l-1")).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["error"]["details"]["code"], "unknown_label");
    let marker = g5_api::services::issue_creations::creation_marker(org.id, owner.id, "l-2");
    Mock::given(method("POST"))
        .and(path(format!("/repositories/{ISSUES_REPO}/issues")))
        .and(body_partial_json(json!({"labels": ["bug"]})))
        .respond_with(ResponseTemplate::new(201).set_body_json({
            // GitHub silently drops a label the author may not set.
            let mut created =
                created_issue_json(&github, 31, "Crash", &format!("Steps\n\n{marker}"));
            created["labels"] = json!([]);
            created
        }))
        .expect(1)
        .mount(&github)
        .await;
    let (status, created) =
        create_issue(&app, &owner, org.id, &request(json!(["BUG"]), "l-2")).await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    assert_eq!(created["data"]["state"], "created");
    assert_eq!(created["data"]["labels"], json!(["bug"]));
    assert_eq!(
        created["data"]["labelsNotApplied"],
        json!(["bug"]),
        "{created}"
    );

    // The person tags the issue from its page as their own account.
    mount_issue(&github, 31).await;
    Mock::given(method("POST"))
        .and(path(format!(
            "/repositories/{ISSUES_REPO}/issues/31/labels"
        )))
        .and(header("authorization", "Bearer ghu_owner"))
        .and(body_json(json!({"labels": ["bug"]})))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!([{"id": 1, "name": "bug", "color": "d73a4a"}])),
        )
        .expect(1)
        .mount(&github)
        .await;
    let (status, body) = json_of(
        app.authenticated_client(&owner)
            .post_json(
                &format!(
                    "/api/v1/orgs/{}/issues/github/{ISSUES_REPO}/31/labels",
                    org.id
                ),
                &json!({"addLabels": ["BUG"], "removeLabels": []}),
            )
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["outcome"], "applied", "{body}");
    github.verify().await;

    app.cleanup().await;
}
