//! Integration coverage for repo metadata security boundaries and retired secret surface.

use crate::helpers;

use helpers::{TestApp, TestOrg, TestUser};
use reqwest::StatusCode;
use serde_json::json;
use uuid::Uuid;

async fn insert_repo(app: &TestApp, org: &TestOrg, slug: &str) -> Uuid {
    let repo_id = Uuid::new_v4();
    let name = format!("{slug}-name");
    let git_url = format!("https://github.com/example/{slug}.git");

    sqlx::query(
        r#"
        INSERT INTO repos (id, org_id, name, slug, git_url, default_branch, provider)
        VALUES ($1, $2, $3, $4, $5, 'main', 'github')
        "#,
    )
    .bind(repo_id)
    .bind(org.id)
    .bind(&name)
    .bind(slug)
    .bind(&git_url)
    .execute(&app.db)
    .await
    .expect("Failed to insert test repo");

    repo_id
}

#[tokio::test]
async fn test_repos_require_scoped_org_context_for_visibility() {
    let app = TestApp::spawn().await;
    let org_a = TestOrg::create(&app).await;
    let org_b = TestOrg::create(&app).await;
    let repo_a = insert_repo(&app, &org_a, "alpha-repo").await;
    let _repo_b = insert_repo(&app, &org_b, "beta-repo").await;

    let system_visible_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM repos")
        .fetch_one(&app.system_pool)
        .await
        .expect("System pool query should fail closed under repo RLS");
    assert_eq!(
        system_visible_count, 0,
        "unscoped system_pool should not see tenant-owned repos"
    );

    let mut scoped = app.scoped_db_for_org(org_a.id).await;
    let visible_repo_ids: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM repos ORDER BY slug ASC")
        .fetch_all(scoped.conn())
        .await
        .expect("Scoped tenant DB should see repos for the current org");
    assert_eq!(
        visible_repo_ids,
        vec![repo_a],
        "scoped tenant DB should only see the current org's repos"
    );
    scoped.rollback().await.expect("rollback scoped repo query");

    app.cleanup().await;
}

#[tokio::test]
async fn retired_repo_secret_surface_is_absent() {
    let app = TestApp::spawn().await;
    let retired: (Option<String>, Option<String>) = sqlx::query_as(
        "SELECT to_regclass('public.repo_secrets')::text,
                to_regprocedure('public.repo_webhook_secret_for_current_org(uuid)')::text",
    )
    .fetch_one(&app.system_pool)
    .await
    .unwrap();
    assert_eq!(retired, (None, None), "no callable orphan secret reader");
    app.cleanup().await;
}

#[tokio::test]
async fn blob_capability_routes_are_not_served() {
    // The repository-wide blob capability surface (G5Labs/G5-Lean#457, #458)
    // is gone: committed files and review artifacts are read only through
    // workspace-authorized routes. The router must not know these paths even
    // for a fully authenticated organization owner.
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let repo = insert_repo(&app, &org, "blob-gone").await;
    let client = app.authenticated_client(&user);

    let response = client
        .post_json(
            "/api/v1/blobs/capabilities",
            &json!({
                "orgId": org.id,
                "repoId": repo,
                "gitOid": "0123456789abcdef0123456789abcdef01234567",
                "maxBytes": 1024
            }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    for path in ["/api/v1/blobs/read?token=x", "/api/v1/blobs/tree?token=x"] {
        let response = client.get(path).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}");
    }

    app.cleanup().await;
}
