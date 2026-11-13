//! Integration tests for G5 API organization management endpoints.
//!
//! Covers the complete request→route→middleware→service→database→response cycle
//! for all endpoints across two org-related routers:
//!
//! 1. **Org router** (`/api/v1/orgs/*`) — Org CRUD, member management,
//!    invitation lifecycle, repo listing (12 endpoints).
//! 2. **Invitation router** (`/api/v1/invitations/*`) — Accept/decline
//!    invitations (2 endpoints).
//!
//! Each test spawns a full `TestApp` instance with an isolated Postgres database,
//! makes real HTTP requests via `AuthenticatedClient`, and asserts at the HTTP
//! response level (status codes, JSON body shapes) and the database level
//! (verifying rows were created/updated/deleted).

use crate::helpers;

use std::time::Duration;

use g5_scm::GitRepo;
use helpers::{
    assert_json_error, AuthenticatedClient, EnvVarGuard, TestApp, TestOrg, TestUser, TestWorkspace,
};
use reqwest::StatusCode;
use serde_json::{json, Value};
use uuid::Uuid;

// ===========================================================================
// Group 1: Org CRUD
// ===========================================================================

#[tokio::test]
#[serial_test::serial(worker_image_catalog)]
async fn new_org_tracks_the_configured_workspace_image() {
    let image_ref = format!(
        "us-docker.pkg.dev/g5-test/runtime/g5d-g5dev@sha256:{}",
        "a".repeat(64)
    );
    let catalog = format!("g5d-g5dev={image_ref}");
    let _env = EnvVarGuard::set_many(&[
        ("G5_NEW_ORG_DEFAULT_IMAGE_TRACK", "g5d-g5dev"),
        ("G5_PROMOTED_BASE_IMAGES", &catalog),
    ]);
    let app = TestApp::spawn().await;
    let user = TestUser::create(&app).await;
    let response = helpers::platform::create_org_via_operator(
        &app,
        &user,
        json!({"name": "Image Policy Org", "slug": "image-policy-org"}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let body: Value = response.json().await.expect("parse created org");
    let org_id = Uuid::parse_str(body["data"]["org"]["id"].as_str().unwrap()).unwrap();
    let policy: (Option<String>, Option<String>) = sqlx::query_as(
        "SELECT default_base_image_track, default_base_image_ref FROM orgs WHERE id = $1",
    )
    .bind(org_id)
    .fetch_one(&app.db)
    .await
    .expect("load new org policy");
    assert_eq!(policy, (Some("g5d-g5dev".into()), Some(image_ref)));
    app.cleanup().await;
}

/// Verifies that `POST /api/v1/orgs` with a valid name creates an org and
/// returns the full creation cascade response (org + base workspace + docs repo)
/// with HTTP 201.
#[tokio::test]
async fn test_create_org_returns_201_with_cascade() {
    let app = TestApp::spawn().await;
    let user = TestUser::create(&app).await;

    let response = helpers::platform::create_org_via_operator(
        &app,
        &user,
        json!({
            "name": "Acme Corp"
        }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::CREATED);

    let body: Value = response.json().await.expect("Failed to parse JSON");
    let data = &body["data"];

    // Verify org fields
    assert!(data["org"]["id"].is_string(), "Expected org.id");
    assert_eq!(data["org"]["name"], "Acme Corp");
    assert!(data["org"]["slug"].is_string(), "Expected org.slug");
    assert!(
        data["org"]["createdAt"].is_string(),
        "Expected org.createdAt"
    );

    // Verify base workspace was created
    assert!(
        data["baseWorkspace"]["id"].is_string(),
        "Expected baseWorkspace.id"
    );
    assert_eq!(data["baseWorkspace"]["name"], "Base");

    // Verify docs repo was created
    assert!(data["docsRepo"]["id"].is_string(), "Expected docsRepo.id");
    assert_eq!(data["docsRepo"]["name"], "docs");
    assert!(
        data["docsRepo"]["gitUrl"].is_string(),
        "Expected docsRepo.gitUrl"
    );

    // Verify the code repo was created — the Base is born docs+code
    assert!(data["codeRepo"]["id"].is_string(), "Expected codeRepo.id");
    assert_eq!(data["codeRepo"]["name"], "code");
    assert!(
        data["codeRepo"]["gitUrl"].is_string(),
        "Expected codeRepo.gitUrl"
    );

    // G5-hosted repositories record scoped URLs naming no disk path
    // (`g5.repo.hosted-url`).
    let slug = data["org"]["slug"].as_str().unwrap();
    assert_eq!(
        data["docsRepo"]["gitUrl"],
        format!("g5-local://{slug}/docs")
    );
    assert_eq!(
        data["codeRepo"]["gitUrl"],
        format!("g5-local://{slug}/code")
    );

    // Every tenant organization tracks issues with G5 issues from the start
    // (issues g5.issue.native.domain): its native domain, keyed from the
    // slug, trusted and the default creation domain, configured by the owner.
    let org_id: Uuid = data["org"]["id"].as_str().unwrap().parse().unwrap();
    let (provider, display_key, trusted, is_default, configured_by): (
        String,
        String,
        bool,
        bool,
        Uuid,
    ) = sqlx::query_as(
        "SELECT provider, display_key, content_trusted, is_default_create_target, configured_by \
             FROM issue_domains WHERE org_id = $1",
    )
    .bind(org_id)
    .fetch_one(&app.db)
    .await
    .expect("the new organization has exactly one issue domain");
    assert_eq!(provider, "g5");
    assert_eq!(display_key, "ACMECORP");
    assert!(trusted);
    assert!(is_default);
    assert_eq!(configured_by, user.id);

    // Verify the org exists in the database
    let org_id: Uuid = data["org"]["id"].as_str().unwrap().parse().unwrap();
    let org_row = sqlx::query!("SELECT name, slug FROM orgs WHERE id = $1", org_id)
        .fetch_one(&app.db)
        .await
        .expect("Org should exist in database");
    assert_eq!(org_row.name, "Acme Corp");

    // Verify the calling user is the initial Admin.
    let membership = sqlx::query!(
        "SELECT role FROM org_memberships WHERE org_id = $1 AND user_id = $2",
        org_id,
        user.id
    )
    .fetch_one(&app.db)
    .await
    .expect("Admin membership should exist");
    assert_eq!(membership.role, "admin");

    // Verify the Base workspace is born with BOTH a docs and a code repo/branch.
    let base_ws_id: Uuid = data["baseWorkspace"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let docs_repo_id: Uuid = data["docsRepo"]["id"].as_str().unwrap().parse().unwrap();
    let code_repo_id: Uuid = data["codeRepo"]["id"].as_str().unwrap().parse().unwrap();
    assert_ne!(
        docs_repo_id, code_repo_id,
        "docs and code repos must be distinct"
    );
    let branch_repo_ids: Vec<Uuid> = sqlx::query_scalar::<_, Uuid>(
        "SELECT repo_id FROM workspace_repos WHERE workspace_id = $1",
    )
    .bind(base_ws_id)
    .fetch_all(&app.db)
    .await
    .expect("Base workspace branches should exist");
    assert!(
        branch_repo_ids.contains(&docs_repo_id),
        "Base must have a docs branch"
    );
    assert!(
        branch_repo_ids.contains(&code_repo_id),
        "Base must have a code branch"
    );
    assert_eq!(
        branch_repo_ids.len(),
        2,
        "Base is born with exactly two repo branches (docs + code)"
    );

    app.cleanup().await;
}

/// A late durable-cascade failure must roll back the org and every earlier row.
#[tokio::test]
async fn test_create_org_rolls_back_all_cascade_rows_on_late_failure() {
    let app = TestApp::spawn().await;
    let user = TestUser::create(&app).await;
    // The operator exists before any cascade-failure trigger is installed.
    let operator = TestUser::create(&app).await;
    helpers::platform::make_operator(&app, &operator, "member").await;

    // Fail after each durable cascade stage. Each trigger runs only in this
    // isolated test database, and every response must leave no org row behind.
    for failing_table in [
        "org_memberships",
        "repos",
        "workspaces",
        "workspace_repos",
        "workspace_settings",
    ] {
        sqlx::query(
            r#"
            CREATE OR REPLACE FUNCTION fail_org_creation_cascade_stage()
            RETURNS trigger AS $$
            BEGIN
                RAISE EXCEPTION 'forced org creation cascade failure';
            END;
            $$ LANGUAGE plpgsql
            "#,
        )
        .execute(&app.db)
        .await
        .expect("create cascade-failure trigger function");
        sqlx::query(&format!(
            "CREATE TRIGGER fail_org_creation_cascade_stage \
             BEFORE INSERT ON {failing_table} \
             FOR EACH ROW EXECUTE FUNCTION fail_org_creation_cascade_stage()"
        ))
        .execute(&app.db)
        .await
        .expect("create cascade-failure trigger");

        let slug = format!("rollback-cascade-{failing_table}");
        let response = helpers::platform::create_org_as(
            &app,
            &operator,
            &user,
            json!({
                "name": "Rollback Cascade Org",
                "slug": slug,
            }),
        )
        .await;
        assert!(
            !response.status().is_success(),
            "failure at {failing_table} must abort organization creation, got {}",
            response.status()
        );

        let persisted_orgs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM orgs WHERE slug = $1")
            .bind(&slug)
            .fetch_one(&app.db)
            .await
            .expect("count orgs after failed cascade");
        assert_eq!(
            persisted_orgs, 0,
            "failure at {failing_table} must not leave an org, membership, repo, workspace, branch, or settings row"
        );

        sqlx::query(&format!(
            "DROP TRIGGER fail_org_creation_cascade_stage ON {failing_table}"
        ))
        .execute(&app.db)
        .await
        .expect("drop cascade-failure trigger");
        sqlx::query("DROP FUNCTION fail_org_creation_cascade_stage()")
            .execute(&app.db)
            .await
            .expect("drop cascade-failure trigger function");
    }

    app.cleanup().await;
}

#[tokio::test]
async fn test_create_org_bootstraps_docs_repo_mirror() {
    let app = TestApp::spawn().await;
    let user = TestUser::create(&app).await;

    let response = helpers::platform::create_org_via_operator(
        &app,
        &user,
        json!({
            "name": "Mirror Ready Org"
        }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::CREATED);

    let body: Value = response.json().await.expect("Failed to parse JSON");
    let repo_id: Uuid = body["data"]["docsRepo"]["id"]
        .as_str()
        .expect("docsRepo.id should be present")
        .parse()
        .expect("docsRepo.id should be a UUID");
    let workspace_id: Uuid = body["data"]["baseWorkspace"]["id"]
        .as_str()
        .expect("baseWorkspace.id should be present")
        .parse()
        .expect("baseWorkspace.id should be a UUID");

    let default_branch: String = sqlx::query_scalar("SELECT r.default_branch FROM workspace_repos m JOIN repos r ON r.id=m.repo_id WHERE m.workspace_id=$1 AND m.repo_id=$2")
        .bind(workspace_id).bind(repo_id).fetch_one(&app.db).await.unwrap();
    let repo_path = app.repo_path(repo_id).await;

    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    let database_head = loop {
        if let Ok(repo) = GitRepo::open(&repo_path) {
            if let Ok(head) = repo.resolve_branch_head(&default_branch) {
                break head;
            }
        }

        assert!(
            tokio::time::Instant::now() < deadline,
            "docs repo mirror at {} was not bootstrapped within 5s",
            repo_path.display()
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    // The bootstrap initialized the unborn branch: its landed head is a root
    // commit rather than an advance of an earlier head.
    let parents = std::process::Command::new("git")
        .args(["rev-list", "--parents", "-n", "1", &database_head])
        .current_dir(&repo_path)
        .output()
        .expect("git rev-list should run");
    assert!(
        parents.status.success(),
        "git rev-list failed: {}",
        String::from_utf8_lossy(&parents.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&parents.stdout).trim(),
        database_head,
        "the docs bootstrap commit must initialize the branch without a parent"
    );

    app.cleanup().await;
}

#[tokio::test]
async fn renaming_an_org_moves_its_hosted_repository_urls_and_keeps_remote_ones() {
    use g5_api_core::services::identity::org_service::UpdateOrgRequest;
    let app = TestApp::spawn().await;
    let user = TestUser::create(&app).await;
    let response =
        helpers::platform::create_org_via_operator(&app, &user, json!({"name": "Rename Corp"}))
            .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let body: Value = response.json().await.unwrap();
    let org_id: Uuid = body["data"]["org"]["id"].as_str().unwrap().parse().unwrap();
    let remote = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO repos (id, org_id, name, slug, git_url) VALUES ($1, $2, 'remote', 'remote', 'https://example.test/remote.git')",
    )
    .bind(remote)
    .bind(org_id)
    .execute(&app.db)
    .await
    .unwrap();

    let renamed = format!("renamed-{}", Uuid::new_v4().simple());
    let capability = app
        .state
        .authz
        .mint_db_capability(
            g5_contracts::types::ids::UserId::from_uuid(user.id),
            &[g5_contracts::types::ids::OrgId::from_uuid(org_id)],
            &["org:admin".into()],
        )
        .await
        .unwrap();
    let mut db = g5_authz::ScopedDb::from_capability(&app.state.system_pool, capability)
        .await
        .unwrap();
    app.state
        .org_service
        .update_org(
            &mut db,
            org_id,
            user.id,
            UpdateOrgRequest {
                name: None,
                slug: Some(renamed.clone()),
            },
        )
        .await
        .expect("rename the organization");
    db.commit().await.unwrap();

    let urls: Vec<(String, String)> =
        sqlx::query_as("SELECT slug, git_url FROM repos WHERE org_id = $1 ORDER BY slug")
            .bind(org_id)
            .fetch_all(&app.db)
            .await
            .unwrap();
    // Code, docs and the owner's personal repository follow the rename; a
    // repository with a real remote keeps its URL.
    assert!(urls.iter().any(|(slug, _)| slug == "code"));
    assert!(urls.iter().any(|(slug, _)| slug == "docs"));
    for (slug, url) in &urls {
        if slug == "remote" {
            assert_eq!(url, "https://example.test/remote.git");
        } else {
            assert_eq!(url, &format!("g5-local://{renamed}/{slug}"));
        }
    }
    app.cleanup().await;
}

/// A name-only update that overlaps a rename must not write the old slug
/// back: the hosted repository URLs already name the new one
/// (`g5.repo.hosted-url`).
#[tokio::test]
async fn a_name_update_overlapping_a_rename_keeps_the_new_slug() {
    use g5_api_core::services::identity::org_service::UpdateOrgRequest;
    let app = TestApp::spawn().await;
    let user = TestUser::create(&app).await;
    let response =
        helpers::platform::create_org_via_operator(&app, &user, json!({"name": "Overlap Corp"}))
            .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let body: Value = response.json().await.unwrap();
    let org_id: Uuid = body["data"]["org"]["id"].as_str().unwrap().parse().unwrap();
    let admin_db = || async {
        let capability = app
            .state
            .authz
            .mint_db_capability(
                g5_contracts::types::ids::UserId::from_uuid(user.id),
                &[g5_contracts::types::ids::OrgId::from_uuid(org_id)],
                &["org:admin".into()],
            )
            .await
            .unwrap();
        g5_authz::ScopedDb::from_capability(&app.state.system_pool, capability)
            .await
            .unwrap()
    };

    let renamed = format!("renamed-{}", Uuid::new_v4().simple());
    let mut renaming = admin_db().await;
    app.state
        .org_service
        .update_org(
            &mut renaming,
            org_id,
            user.id,
            UpdateOrgRequest {
                name: None,
                slug: Some(renamed.clone()),
            },
        )
        .await
        .expect("rename the organization");

    // The rename commits while the name update is under way.
    let mut naming = admin_db().await;
    let commit_rename = async {
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        renaming.commit().await.unwrap();
    };
    let (named, ()) = tokio::join!(
        app.state.org_service.update_org(
            &mut naming,
            org_id,
            user.id,
            UpdateOrgRequest {
                name: Some("Overlap Renamed Corp".into()),
                slug: None,
            },
        ),
        commit_rename,
    );
    let named = named.expect("update the name");
    naming.commit().await.unwrap();

    assert_eq!(named.slug, renamed);
    assert_eq!(named.name, "Overlap Renamed Corp");
    let urls: Vec<(String, String)> =
        sqlx::query_as("SELECT slug, git_url FROM repos WHERE org_id = $1 AND provider IS NULL")
            .bind(org_id)
            .fetch_all(&app.db)
            .await
            .unwrap();
    assert!(!urls.is_empty());
    for (slug, url) in &urls {
        assert_eq!(url, &format!("g5-local://{renamed}/{slug}"));
    }
    app.cleanup().await;
}

/// Verifies that creating an org with a slug that already exists returns 409 CONFLICT.
#[tokio::test]
async fn test_create_org_duplicate_slug_returns_409() {
    let app = TestApp::spawn().await;
    let user = TestUser::create(&app).await;

    // Create first org
    let response = helpers::platform::create_org_via_operator(
        &app,
        &user,
        json!({
            "name": "Acme Corp",
            "slug": "acme"
        }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);

    // Try to create another org with the same slug
    let response = helpers::platform::create_org_via_operator(
        &app,
        &user,
        json!({
            "name": "Another Corp",
            "slug": "acme"
        }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["error"]["code"], "CONFLICT");

    app.cleanup().await;
}

/// Verifies that `GET /api/v1/orgs` returns all orgs the authenticated user
/// belongs to, including the user's role in each.
#[tokio::test]
async fn test_list_orgs_returns_user_orgs() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    let response = client.get("/api/v1/orgs").await;

    assert_eq!(response.status(), StatusCode::OK);

    let body: Value = response.json().await.expect("Failed to parse JSON");
    let data = body["data"].as_array().expect("data should be an array");

    assert!(!data.is_empty(), "User should belong to at least one org");

    let org_item = data
        .iter()
        .find(|o| o["orgId"].as_str() == Some(&org.id.to_string()))
        .expect("Created org should be in the list");

    assert_eq!(org_item["orgName"], org.name);
    assert_eq!(org_item["orgSlug"], org.slug);
    assert_eq!(org_item["role"], "admin");
    assert!(org_item["createdAt"].is_string(), "Expected createdAt");

    app.cleanup().await;
}

/// Verifies that `GET /api/v1/orgs` returns an empty array (not an error) for
/// a user with no org memberships.
#[tokio::test]
async fn test_list_orgs_returns_empty_for_no_memberships() {
    let app = TestApp::spawn().await;
    let user = TestUser::create(&app).await;
    let client = app.authenticated_client(&user);

    let response = client.get("/api/v1/orgs").await;

    assert_eq!(response.status(), StatusCode::OK);

    let body: Value = response.json().await.expect("Failed to parse JSON");
    let data = body["data"].as_array().expect("data should be an array");
    assert!(
        data.is_empty(),
        "User with no memberships should get empty array"
    );

    app.cleanup().await;
}

/// Verifies that `GET /api/v1/orgs/:orgId` returns the full org detail.
#[tokio::test]
async fn test_get_org_returns_org_detail() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    let response = client.get(&format!("/api/v1/orgs/{}", org.id)).await;

    assert_eq!(response.status(), StatusCode::OK);

    let body: Value = response.json().await.expect("Failed to parse JSON");
    let data = &body["data"];

    assert_eq!(data["id"], org.id.to_string());
    assert_eq!(data["name"], org.name);
    assert_eq!(data["slug"], org.slug);
    assert!(data["createdAt"].is_string(), "Expected createdAt");

    app.cleanup().await;
}

/// Verifies that `GET /api/v1/orgs/:orgId` returns 404 for a non-existent org ID.
#[tokio::test]
async fn test_get_org_nonexistent_returns_404() {
    let app = TestApp::spawn().await;
    let user = TestUser::create(&app).await;
    let client = app.authenticated_client(&user);

    let fake_id = Uuid::new_v4();
    let response = client.get(&format!("/api/v1/orgs/{}", fake_id)).await;

    assert_json_error(response, StatusCode::NOT_FOUND, "NOT_FOUND").await;

    app.cleanup().await;
}

/// Verifies that all org endpoints return 401 when no JWT is provided.
#[tokio::test]
async fn test_org_endpoints_without_auth_return_401() {
    let app = TestApp::spawn().await;

    // Test list orgs
    let response = app
        .client
        .get(format!("{}/api/v1/orgs", app.base_url))
        .send()
        .await
        .expect("Failed to send request");
    assert_json_error(response, StatusCode::UNAUTHORIZED, "UNAUTHORIZED").await;

    // Test create org
    let response = app
        .client
        .post(format!("{}/api/v1/orgs", app.base_url))
        .header("Content-Type", "application/json")
        .json(&json!({"name": "Test"}))
        .send()
        .await
        .expect("Failed to send request");
    assert_json_error(response, StatusCode::UNAUTHORIZED, "UNAUTHORIZED").await;

    // Test get org
    let fake_id = Uuid::new_v4();
    let response = app
        .client
        .get(format!("{}/api/v1/orgs/{}", app.base_url, fake_id))
        .send()
        .await
        .expect("Failed to send request");
    assert_json_error(response, StatusCode::UNAUTHORIZED, "UNAUTHORIZED").await;

    app.cleanup().await;
}

// ===========================================================================
// Group 2: Member Management
// ===========================================================================

/// Verifies that `GET /api/v1/orgs/:orgId/members` returns all org members
/// with their roles and profile info, ordered by role priority.
#[tokio::test]
async fn test_list_members_returns_org_members() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let client = app.authenticated_client(&owner);

    let response = client
        .get(&format!("/api/v1/orgs/{}/members", org.id))
        .await;

    assert_eq!(response.status(), StatusCode::OK);

    let body: Value = response.json().await.expect("Failed to parse JSON");
    let data = body["data"].as_array().expect("data should be an array");

    assert!(
        !data.is_empty(),
        "Org should have at least one member (the owner)"
    );

    let owner_member = &data[0];
    assert_eq!(owner_member["userId"], owner.id.to_string());
    assert!(owner_member["email"].is_string(), "Expected email");
    assert!(
        owner_member["displayName"].is_string(),
        "Expected displayName"
    );
    assert_eq!(owner_member["role"], "admin");
    assert!(owner_member["joinedAt"].is_string(), "Expected joinedAt");

    app.cleanup().await;
}

/// Verifies that `POST /api/v1/orgs/:orgId/members` with an email belonging
/// to an existing G5 user creates an immediate membership (no pending invitation).
#[tokio::test]
async fn test_invite_member_existing_user_creates_membership() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let invitee = TestUser::create(&app).await;
    let client = app.authenticated_client(&owner);

    let response = client
        .post_json(
            &format!("/api/v1/orgs/{}/members", org.id),
            &json!({
                "email": invitee.email,
                "role": "member"
            }),
        )
        .await;

    assert_eq!(response.status(), StatusCode::OK);

    let body: Value = response.json().await.expect("Failed to parse JSON");
    let data = &body["data"];

    // Should return membership (not invitation) for existing user
    assert!(
        data["membership"].is_object(),
        "Expected membership for existing user"
    );
    assert_eq!(data["membership"]["userId"], invitee.id.to_string());
    assert_eq!(data["membership"]["role"], "member");
    assert!(
        data["invitation"].is_null(),
        "Invitation should be null for existing user"
    );

    // Verify membership exists in database
    let membership = sqlx::query!(
        "SELECT role FROM org_memberships WHERE org_id = $1 AND user_id = $2",
        org.id,
        invitee.id
    )
    .fetch_one(&app.db)
    .await
    .expect("Membership should exist in database");
    assert_eq!(membership.role, "member");

    app.cleanup().await;
}

/// Verifies that `POST /api/v1/orgs/:orgId/members` with an email not belonging
/// to any G5 user creates a pending invitation.
#[tokio::test]
async fn test_invite_member_nonexistent_user_creates_invitation() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let client = app.authenticated_client(&owner);

    let nonexistent_email = format!("unknown-{}@example.com", Uuid::new_v4());

    let response = client
        .post_json(
            &format!("/api/v1/orgs/{}/members", org.id),
            &json!({
                "email": nonexistent_email,
                "role": "member"
            }),
        )
        .await;

    assert_eq!(response.status(), StatusCode::OK);

    let body: Value = response.json().await.expect("Failed to parse JSON");
    let data = &body["data"];

    // Should return invitation (not membership) for non-existent user
    assert!(
        data["membership"].is_null(),
        "Membership should be null for non-existent user"
    );
    assert!(
        data["invitation"].is_object(),
        "Expected invitation for non-existent user"
    );
    assert_eq!(data["invitation"]["email"], nonexistent_email);
    assert_eq!(data["invitation"]["role"], "member");
    assert_eq!(data["invitation"]["status"], "pending");
    assert!(
        data["invitation"]["expiresAt"].is_string(),
        "Expected expiresAt"
    );

    // Verify invitation exists in database
    let invitation = sqlx::query!(
        "SELECT status, role FROM org_invitations WHERE org_id = $1 AND email = $2",
        org.id,
        nonexistent_email
    )
    .fetch_one(&app.db)
    .await
    .expect("Invitation should exist in database");
    assert_eq!(invitation.status, "pending");
    assert_eq!(invitation.role, "member");

    app.cleanup().await;
}

/// Verifies that attempting to invite a member with `role: "owner"` returns
/// 422 VALIDATION_ERROR.
#[tokio::test]
async fn test_invite_member_with_owner_role_returns_422() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let client = app.authenticated_client(&owner);

    let response = client
        .post_json(
            &format!("/api/v1/orgs/{}/members", org.id),
            &json!({
                "email": "someone@example.com",
                "role": "owner"
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

/// Verifies that `DELETE /api/v1/orgs/:orgId/members/:userId` removes a member
/// and returns 204.
#[tokio::test]
async fn test_remove_member_returns_204() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let member = TestUser::create(&app).await;
    let client = app.authenticated_client(&owner);

    // First invite the member (existing user → immediate membership)
    let response = client
        .post_json(
            &format!("/api/v1/orgs/{}/members", org.id),
            &json!({
                "email": member.email,
                "role": "member"
            }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);

    // Now remove the member
    let response = client
        .delete(&format!("/api/v1/orgs/{}/members/{}", org.id, member.id))
        .await;

    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    // Verify membership is gone from database
    let count = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM org_memberships WHERE org_id = $1 AND user_id = $2",
        org.id,
        member.id
    )
    .fetch_one(&app.db)
    .await
    .expect("Query should succeed");
    assert_eq!(count.unwrap_or(0), 0, "Membership should be deleted");

    app.cleanup().await;
}

/// Verifies that attempting to remove the last active Admin returns 409 CONFLICT.
#[tokio::test]
async fn test_remove_last_active_admin_returns_409() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let client = app.authenticated_client(&owner);

    let response = client
        .delete(&format!("/api/v1/orgs/{}/members/{}", org.id, owner.id))
        .await;

    assert_json_error(response, StatusCode::CONFLICT, "CONFLICT").await;

    // Verify Admin membership still exists
    let membership = sqlx::query!(
        "SELECT role FROM org_memberships WHERE org_id = $1 AND user_id = $2",
        org.id,
        owner.id
    )
    .fetch_one(&app.db)
    .await
    .expect("Admin membership should still exist");
    assert_eq!(membership.role, "admin");

    app.cleanup().await;
}

/// Active workspace ownership is a durable write-authority binding. The API
/// must report an actionable conflict instead of leaking the deferred FK as a
/// 500 when an org admin tries to remove or demote that owner.
#[tokio::test]
async fn test_active_workspace_owner_cannot_be_removed_or_demoted_to_viewer() {
    let app = TestApp::spawn().await;
    let mut org_admin = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut org_admin).await;
    let workspace_owner = TestUser::create(&app).await;
    sqlx::query("INSERT INTO org_memberships(org_id, user_id, role) VALUES ($1, $2, 'member')")
        .bind(org.id)
        .bind(workspace_owner.id)
        .execute(&app.db)
        .await
        .expect("insert eligible workspace owner membership");
    let workspace = TestWorkspace::create(&app, &org).await;
    sqlx::query("UPDATE workspaces SET owner_user_id = $2 WHERE id = $1")
        .bind(workspace.id)
        .bind(workspace_owner.id)
        .execute(&app.db)
        .await
        .expect("assign active workspace owner");

    let client = app.authenticated_client(&org_admin);
    let remove = client
        .delete(&format!(
            "/api/v1/orgs/{}/members/{}",
            org.id, workspace_owner.id
        ))
        .await;
    assert_eq!(remove.status(), StatusCode::CONFLICT);
    let remove_body: Value = remove.json().await.expect("parse remove conflict");
    assert_eq!(remove_body["error"]["code"], "CONFLICT");
    assert!(
        remove_body["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains(&workspace.id.to_string())),
        "conflict must identify the blocking workspace: {remove_body}"
    );

    let demote = client
        .patch_json(
            &format!("/api/v1/orgs/{}/members/{}", org.id, workspace_owner.id),
            &json!({"role": "viewer"}),
        )
        .await;
    assert_eq!(demote.status(), StatusCode::CONFLICT);
    let demote_body: Value = demote.json().await.expect("parse demotion conflict");
    assert_eq!(demote_body["error"]["code"], "CONFLICT");
    assert!(
        demote_body["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains(&workspace.id.to_string())),
        "conflict must identify the blocking workspace: {demote_body}"
    );

    let role: String =
        sqlx::query_scalar("SELECT role FROM org_memberships WHERE org_id = $1 AND user_id = $2")
            .bind(org.id)
            .bind(workspace_owner.id)
            .fetch_one(&app.db)
            .await
            .expect("read retained owner-eligible role");
    assert_eq!(role, "member");

    // Tombstoning removes the workspace from the active-owner FK, so cleanup
    // can then retire the owner membership without weakening the active-workspace
    // guard above. Live E2E fixtures rely on this order for isolated users.
    let workspace_owner_client = app.authenticated_client(&workspace_owner);
    let tombstone = workspace_owner_client
        .post_json(
            &format!("/api/v1/workspaces/{}/delete", workspace.id),
            &json!({}),
        )
        .await;
    let tombstone_status = tombstone.status();
    let tombstone_body = tombstone.text().await.expect("read tombstone response");
    assert_eq!(
        tombstone_status,
        StatusCode::NO_CONTENT,
        "workspace owner must be able to tombstone their workspace: {tombstone_body}"
    );

    let remove_after_tombstone = client
        .delete(&format!(
            "/api/v1/orgs/{}/members/{}",
            org.id, workspace_owner.id
        ))
        .await;
    assert_eq!(remove_after_tombstone.status(), StatusCode::NO_CONTENT);

    let membership_after_tombstone: Option<String> =
        sqlx::query_scalar("SELECT role FROM org_memberships WHERE org_id = $1 AND user_id = $2")
            .bind(org.id)
            .bind(workspace_owner.id)
            .fetch_optional(&app.db)
            .await
            .expect("read membership after tombstone");
    assert!(
        membership_after_tombstone.is_none(),
        "tombstoned workspaces must not retain the owner membership"
    );

    app.cleanup().await;
}

// ===========================================================================
// Group 3: Invitation Lifecycle
// ===========================================================================

/// Verifies that `POST /api/v1/invitations/:id/accept` accepts a pending
/// invitation and creates a membership.
#[tokio::test]
async fn test_accept_invitation_creates_membership() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let invitee = TestUser::create(&app).await;

    // Create invitation by inserting directly (since the invitee already exists,
    // the invite endpoint would create immediate membership; we need a pending invitation)
    let invitation_id = Uuid::new_v4();
    sqlx::query!(
        r#"INSERT INTO org_invitations (id, org_id, email, role, invited_by, status, expires_at)
           VALUES ($1, $2, $3, 'member', $4, 'pending', now() + interval '7 days')"#,
        invitation_id,
        org.id,
        invitee.email,
        owner.id
    )
    .execute(&app.db)
    .await
    .expect("Failed to insert test invitation");

    // Accept the invitation as the invitee
    let invitee_client = app.authenticated_client(&invitee);
    let response = invitee_client
        .post_json(
            &format!("/api/v1/invitations/{}/accept", invitation_id),
            &json!({}),
        )
        .await;

    assert_eq!(response.status(), StatusCode::OK);

    let body: Value = response.json().await.expect("Failed to parse JSON");
    let data = &body["data"];
    assert_eq!(data["userId"], invitee.id.to_string());
    assert_eq!(data["role"], "member");

    // Verify membership was created in database
    let membership = sqlx::query!(
        "SELECT role FROM org_memberships WHERE org_id = $1 AND user_id = $2",
        org.id,
        invitee.id
    )
    .fetch_one(&app.db)
    .await
    .expect("Membership should exist after acceptance");
    assert_eq!(membership.role, "member");

    // Verify invitation status changed to accepted
    let inv = sqlx::query!(
        "SELECT status FROM org_invitations WHERE id = $1",
        invitation_id
    )
    .fetch_one(&app.db)
    .await
    .expect("Invitation should still exist");
    assert_eq!(inv.status, "accepted");

    app.cleanup().await;
}

/// Verifies that accepting an invitation rolls back its status when membership
/// creation fails.
#[tokio::test]
async fn test_accept_invitation_rolls_back_status_when_membership_insert_fails() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let invitee = TestUser::create(&app).await;

    let invitation_id = Uuid::new_v4();
    sqlx::query!(
        r#"INSERT INTO org_invitations (id, org_id, email, role, invited_by, status, expires_at)
           VALUES ($1, $2, $3, 'member', $4, 'pending', now() + interval '7 days')"#,
        invitation_id,
        org.id,
        invitee.email,
        owner.id
    )
    .execute(&app.db)
    .await
    .expect("Failed to insert test invitation");

    sqlx::query(
        r#"
        CREATE FUNCTION fail_accept_membership_insert() RETURNS trigger
        LANGUAGE plpgsql AS $$
        BEGIN
            RAISE EXCEPTION 'force accept invitation membership failure';
        END;
        $$
        "#,
    )
    .execute(&app.db)
    .await
    .expect("Failed to create membership failure function");
    sqlx::query(
        r#"
        CREATE TRIGGER fail_accept_membership_insert
        BEFORE INSERT ON org_memberships
        FOR EACH ROW EXECUTE FUNCTION fail_accept_membership_insert()
        "#,
    )
    .execute(&app.db)
    .await
    .expect("Failed to create membership failure trigger");

    let invitee_client = app.authenticated_client(&invitee);
    let response = invitee_client
        .post_json(
            &format!("/api/v1/invitations/{}/accept", invitation_id),
            &json!({}),
        )
        .await;

    assert!(
        !response.status().is_success(),
        "acceptance must fail when membership insertion fails"
    );

    let invitation = sqlx::query!(
        "SELECT status FROM org_invitations WHERE id = $1",
        invitation_id
    )
    .fetch_one(&app.db)
    .await
    .expect("Invitation should still exist");
    assert_eq!(invitation.status, "pending");

    let membership_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM org_memberships WHERE org_id = $1 AND user_id = $2",
    )
    .bind(org.id)
    .bind(invitee.id)
    .fetch_one(&app.db)
    .await
    .expect("Membership count query should succeed");
    assert_eq!(membership_count, 0);

    app.cleanup().await;
}

/// Verifies that `POST /api/v1/invitations/:id/decline` declines a pending
/// invitation and returns 204.
#[tokio::test]
async fn test_decline_invitation_returns_204() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let invitee = TestUser::create(&app).await;

    // Create pending invitation directly
    let invitation_id = Uuid::new_v4();
    sqlx::query!(
        r#"INSERT INTO org_invitations (id, org_id, email, role, invited_by, status, expires_at)
           VALUES ($1, $2, $3, 'member', $4, 'pending', now() + interval '7 days')"#,
        invitation_id,
        org.id,
        invitee.email,
        owner.id
    )
    .execute(&app.db)
    .await
    .expect("Failed to insert test invitation");

    let invitee_client = app.authenticated_client(&invitee);
    let response = invitee_client
        .post_json(
            &format!("/api/v1/invitations/{}/decline", invitation_id),
            &json!({}),
        )
        .await;

    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    // Verify invitation status changed to declined
    let inv = sqlx::query!(
        "SELECT status FROM org_invitations WHERE id = $1",
        invitation_id
    )
    .fetch_one(&app.db)
    .await
    .expect("Invitation should still exist");
    assert_eq!(inv.status, "declined");

    // Verify no membership was created
    let count = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM org_memberships WHERE org_id = $1 AND user_id = $2",
        org.id,
        invitee.id
    )
    .fetch_one(&app.db)
    .await
    .expect("Query should succeed");
    assert_eq!(
        count.unwrap_or(0),
        0,
        "No membership should exist after decline"
    );

    app.cleanup().await;
}

/// Verifies that an unrelated authenticated user cannot decline another
/// user's pending invitation.
#[tokio::test]
async fn test_decline_invitation_requires_matching_user_email() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let invitee = TestUser::create(&app).await;
    let unrelated_user = TestUser::create(&app).await;

    let invitation_id = Uuid::new_v4();
    sqlx::query!(
        r#"INSERT INTO org_invitations (id, org_id, email, role, invited_by, status, expires_at)
           VALUES ($1, $2, $3, 'member', $4, 'pending', now() + interval '7 days')"#,
        invitation_id,
        org.id,
        invitee.email,
        owner.id
    )
    .execute(&app.db)
    .await
    .expect("Failed to insert test invitation");

    let unrelated_client = app.authenticated_client(&unrelated_user);
    let response = unrelated_client
        .post_json(
            &format!("/api/v1/invitations/{}/decline", invitation_id),
            &json!({}),
        )
        .await;

    assert_json_error(response, StatusCode::CONFLICT, "CONFLICT").await;

    let inv = sqlx::query!(
        "SELECT status FROM org_invitations WHERE id = $1",
        invitation_id
    )
    .fetch_one(&app.db)
    .await
    .expect("Invitation should still exist");
    assert_eq!(inv.status, "pending");

    app.cleanup().await;
}

/// Verifies that accepting an already-accepted or declined invitation returns
/// 409 CONFLICT.
#[tokio::test]
async fn test_accept_nonpending_invitation_returns_409() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let invitee = TestUser::create(&app).await;

    // Create a declined invitation directly
    let invitation_id = Uuid::new_v4();
    sqlx::query!(
        r#"INSERT INTO org_invitations (id, org_id, email, role, invited_by, status, expires_at)
           VALUES ($1, $2, $3, 'member', $4, 'declined', now() + interval '7 days')"#,
        invitation_id,
        org.id,
        invitee.email,
        owner.id
    )
    .execute(&app.db)
    .await
    .expect("Failed to insert test invitation");

    let invitee_client = app.authenticated_client(&invitee);
    let response = invitee_client
        .post_json(
            &format!("/api/v1/invitations/{}/accept", invitation_id),
            &json!({}),
        )
        .await;

    assert_json_error(response, StatusCode::CONFLICT, "CONFLICT").await;

    app.cleanup().await;
}

// ===========================================================================
// Group 4: Repo Listing
// ===========================================================================

/// Verifies that `GET /api/v1/orgs/:orgId/repos` returns repos belonging to
/// the org.
#[tokio::test]
async fn test_list_repos_returns_org_repos() {
    let app = TestApp::spawn().await;
    let user = TestUser::create(&app).await;
    let client = app.authenticated_client(&user);

    // Create org via API (which creates a docs repo as part of cascade)
    let response = helpers::platform::create_org_via_operator(
        &app,
        &user,
        json!({
            "name": "Repo Test Org"
        }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let create_body: Value = response.json().await.unwrap();
    let org_id = create_body["data"]["org"]["id"].as_str().unwrap();

    // List repos
    let response = client.get(&format!("/api/v1/orgs/{}/repos", org_id)).await;

    assert_eq!(response.status(), StatusCode::OK);

    let body: Value = response.json().await.expect("Failed to parse JSON");
    let data = body["data"].as_array().expect("data should be an array");

    // The org creation cascade creates a docs repo
    assert!(!data.is_empty(), "Org should have at least the docs repo");

    let repo = &data[0];
    assert!(repo["id"].is_string(), "Expected repo.id");
    assert!(repo["name"].is_string(), "Expected repo.name");
    assert!(repo["slug"].is_string(), "Expected repo.slug");
    assert!(repo["gitUrl"].is_string(), "Expected repo.gitUrl");
    assert!(
        repo["defaultBranch"].is_string(),
        "Expected repo.defaultBranch"
    );
    assert!(repo["createdAt"].is_string(), "Expected repo.createdAt");

    app.cleanup().await;
}

// ===========================================================================
// Group: Dev/E2E test-token Base provisioning (regression)
// ===========================================================================

async fn test_token_row_counts(app: &TestApp) -> (i64, i64, i64, i64, i64, i64) {
    sqlx::query_as(
        "SELECT (SELECT count(*) FROM users), (SELECT count(*) FROM orgs), \
         (SELECT count(*) FROM org_memberships), (SELECT count(*) FROM refresh_tokens), \
         (SELECT count(*) FROM repos), (SELECT count(*) FROM workspaces)",
    )
    .fetch_one(&app.db)
    .await
    .expect("count test-token provisioning rows")
}

#[tokio::test]
#[serial_test::serial(e2e_auth)]
async fn test_test_token_disabled_route_and_direct_service_leave_no_rows() {
    let _env = EnvVarGuard::set_many(&[("G5_E2E_AUTH", "false")]);
    // Keep restoration on drop while exercising the genuinely absent flag.
    std::env::remove_var("G5_E2E_AUTH");
    let app = TestApp::spawn().await;
    let before = test_token_row_counts(&app).await;
    let response = app
        .client
        .post(format!("{}/api/v1/auth/test-token", app.base_url))
        .json(&json!({"email": "demo@g5labs.ai"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let Err(error) = app
        .state
        .auth_service
        .issue_test_token("demo@g5labs.ai")
        .await
    else {
        panic!("a disabled service must refuse before minting tokens");
    };
    assert!(matches!(
        error.inner,
        g5_contracts::G5Error::Forbidden { .. }
    ));
    assert_eq!(test_token_row_counts(&app).await, before);
    let info: Value = app
        .client
        .get(format!("{}/api/v1/instance/info", app.base_url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(info["data"]["devLoginEnabled"], false);
    app.cleanup().await;
}

#[tokio::test]
#[serial_test::serial(e2e_auth)]
async fn test_test_token_enabled_and_registered_route_recheck() {
    let _env = EnvVarGuard::set_many(&[("G5_E2E_AUTH", "true")]);
    let app = TestApp::spawn().await;
    let endpoint = format!("{}/api/v1/auth/test-token", app.base_url);

    let allowed = app
        .client
        .post(&endpoint)
        .json(&json!({"email": "demo@g5labs.ai"}))
        .send()
        .await
        .unwrap();
    assert_eq!(allowed.status(), StatusCode::OK);
    let body: Value = allowed.json().await.unwrap();
    let jwt = body["data"]["jwt"].as_str().expect("minted JWT");
    let me = app
        .client
        .get(format!("{}/api/v1/users/me", app.base_url))
        .bearer_auth(jwt)
        .send()
        .await
        .unwrap();
    assert_eq!(
        me.status(),
        StatusCode::OK,
        "the minted credential must authenticate"
    );
    let info: Value = app
        .client
        .get(format!("{}/api/v1/instance/info", app.base_url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(info["data"]["devLoginEnabled"], true);

    // Registration happened while enabled. A later service invocation must
    // still refuse if the opt-in is absent, without minting another credential.
    let provisioned = test_token_row_counts(&app).await;
    std::env::remove_var("G5_E2E_AUTH");
    let disabled = app
        .client
        .post(&endpoint)
        .json(&json!({"email": "demo@g5labs.ai"}))
        .send()
        .await
        .unwrap();
    assert_eq!(disabled.status(), StatusCode::FORBIDDEN);
    assert_eq!(test_token_row_counts(&app).await, provisioned);
    app.cleanup().await;
}

/// Regression for the dev-seed bug where `just dev-up` (which drives
/// `seed-dev-org.sh` → the test-token endpoint) patched only the parent docs
/// branch while leaving its stable repository authority row unborn.
///
/// Exercises the exact orchestration the `POST /api/v1/auth/test-token`
/// endpoint runs (`routes::auth::provision_test_token`) and asserts every org
/// the token path provisions is born docs+code with both parent and stable repo
/// rows fully materialized to the same real commit object.
#[tokio::test]
#[serial_test::serial(e2e_auth)]
async fn test_test_token_materializes_base_code_repo() {
    let _env = EnvVarGuard::set_many(&[("G5_E2E_AUTH", "true")]);
    let app = TestApp::spawn().await;

    // Run the exact provisioning + materialization the test-token endpoint runs
    // for a fresh fixture org set (create_org cascade + post-commit code
    // bootstrap). `provision_test_token` upserts its own fixture user.
    g5_api::routes::auth::provision_test_token(&app.state, "demo@g5labs.ai")
        .await
        .expect("test-token provisioning should succeed");

    // Every org the token path provisions must be materialized docs+code.
    assert_base_code_materialized(&app, "g5labs").await;
    assert_base_code_materialized(&app, "g5labs-member").await;
    assert_test_token_workspaces_have_runtime_authority(&app).await;

    // Repeated execution is harmless: re-running fixture provisioning must
    // succeed and leave the Base code repo materialized (idempotent).
    g5_api::routes::auth::provision_test_token(&app.state, "demo@g5labs.ai")
        .await
        .expect("repeated test-token provisioning should be idempotent");
    assert_base_code_materialized(&app, "g5labs").await;
    assert_test_token_workspaces_have_runtime_authority(&app).await;

    app.cleanup().await;
}

/// Token reissuance must not re-promote an existing fixture membership: the
/// dev member persona seeded by `seed-dev-org.sh` (or a test's deliberate role
/// change) keeps its role, while a first-time fixture user still defaults to
/// Admin.
#[tokio::test]
#[serial_test::serial(e2e_auth)]
async fn test_test_token_preserves_existing_membership_role() {
    let _env = EnvVarGuard::set_many(&[("G5_E2E_AUTH", "true")]);
    let app = TestApp::spawn().await;

    let owner = g5_api::routes::auth::provision_test_token(&app.state, "demo@g5labs.ai")
        .await
        .expect("fixture org provisioning should succeed");
    let first = g5_api::routes::auth::provision_test_token(&app.state, "member@g5labs.ai")
        .await
        .expect("first-time fixture user provisioning should succeed");
    assert_eq!(
        fixture_membership_roles(&app, first.user_id).await,
        [("g5labs", "admin"), ("g5labs-member", "admin")]
            .map(|(s, r)| (s.to_string(), r.to_string())),
        "a missing fixture membership defaults to Admin"
    );

    sqlx::query("UPDATE org_memberships SET role = 'member' WHERE user_id = $1")
        .bind(first.user_id)
        .execute(&app.db)
        .await
        .expect("fixture user demotion should succeed");

    let reissued = g5_api::routes::auth::provision_test_token(&app.state, "member@g5labs.ai")
        .await
        .expect("test-token reissuance should succeed");
    assert_eq!(reissued.user_id, first.user_id);
    assert_eq!(
        fixture_membership_roles(&app, reissued.user_id).await,
        [("g5labs", "member"), ("g5labs-member", "member")]
            .map(|(s, r)| (s.to_string(), r.to_string())),
        "token reissuance must keep an existing membership's role"
    );

    // The reissued credential itself is member-scoped: it reports the
    // preserved role and is refused an Admin-only request that the
    // owner's credential is authorized for.
    let member_client =
        AuthenticatedClient::new(app.client.clone(), app.base_url.clone(), reissued.jwt);
    let me: Value = member_client
        .get("/api/v1/users/me")
        .await
        .json()
        .await
        .expect("current user response should be JSON");
    let roles: Vec<&str> = me["data"]["orgs"]
        .as_array()
        .expect("current user orgs")
        .iter()
        .map(|org| org["role"].as_str().expect("org role"))
        .collect();
    assert_eq!(
        roles,
        ["member", "member"],
        "reissued token must report the preserved role"
    );

    let org_id: Uuid = sqlx::query_scalar("SELECT id FROM orgs WHERE slug = 'g5labs'")
        .fetch_one(&app.db)
        .await
        .expect("fixture org lookup should succeed");
    let role_change = format!("/api/v1/orgs/{org_id}/members/{}", owner.user_id);
    let body = json!({ "role": "viewer" });
    assert_eq!(
        member_client.patch_json(&role_change, &body).await.status(),
        StatusCode::FORBIDDEN,
        "reissued member token must not carry owner authority"
    );
    let owner_client =
        AuthenticatedClient::new(app.client.clone(), app.base_url.clone(), owner.jwt);
    assert_eq!(
        owner_client.patch_json(&role_change, &body).await.status(),
        StatusCode::CONFLICT,
        "the Admin token passes authorization and reaches last-active-Admin protection"
    );

    app.cleanup().await;
}

async fn fixture_membership_roles(app: &TestApp, user_id: Uuid) -> Vec<(String, String)> {
    sqlx::query_as::<_, (String, String)>(
        "SELECT o.slug, m.role FROM org_memberships m JOIN orgs o ON o.id = m.org_id \
         WHERE m.user_id = $1 ORDER BY o.slug",
    )
    .bind(user_id)
    .fetch_all(&app.db)
    .await
    .expect("fixture membership lookup should succeed")
}

/// The dev/E2E fixtures must be completed checked forks: the user owns them,
/// every docs+code member uses the exact workspace name, durable authority and
/// Git refs agree, and the published VM starts stopped.
async fn assert_test_token_workspaces_have_runtime_authority(app: &TestApp) {
    let authoritative_workspaces: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM workspaces AS workspace \
         JOIN orgs AS org ON org.id = workspace.org_id \
         JOIN users AS owner ON owner.id = workspace.owner_user_id \
         WHERE org.slug IN ('g5labs', 'g5labs-member') \
           AND workspace.name IN ('demo', 'e2e-test') \
           AND workspace.created_by = workspace.owner_user_id \
           AND owner.email = 'demo@g5labs.ai'",
    )
    .fetch_one(&app.db)
    .await
    .expect("test-token workspace authority lookup should succeed");

    assert_eq!(
        authoritative_workspaces, 4,
        "each test-token workspace must have its fixture user as owner and its exact name as branch"
    );

    let members = sqlx::query_as::<_, (Uuid, String)>(
        "SELECT m.repo_id,w.name FROM workspace_repos m JOIN workspaces w ON w.id=m.workspace_id JOIN orgs o ON o.id=w.org_id WHERE o.slug IN ('g5labs','g5labs-member') AND w.name IN ('demo','e2e-test') ORDER BY o.slug,w.name,m.repo_id",
    )
    .fetch_all(&app.db)
    .await
    .expect("test-token fork member lookup should succeed");
    assert_eq!(
        members.len(),
        8,
        "four fixture forks must each preserve the Base docs+code member set"
    );
    for (repo_id, branch_name) in members {
        let repo_path = app.repo_path(repo_id).await;
        let repo = git2::Repository::open(repo_path).expect("fixture repository should open");
        let reference = repo
            .find_reference(&format!("refs/heads/{branch_name}"))
            .expect("checked fork must create its ordinary branch ref");
        reference
            .peel_to_commit()
            .expect("the selected Git ref must name an existing commit");
    }

    let completed_forks: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM workspace_forks AS fork \
         JOIN orgs AS org ON org.id = fork.org_id \
         WHERE org.slug IN ('g5labs', 'g5labs-member') \
           AND fork.proposed_name IN ('demo', 'e2e-test') \
           AND fork.status = 'completed' \
           AND fork.target_workspace_id IS NOT NULL",
    )
    .fetch_one(&app.db)
    .await
    .expect("completed test-token fork lookup should succeed");
    assert_eq!(completed_forks, 4);

    let stopped_vms: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM workspace_vms AS vm \
         JOIN workspaces AS workspace ON workspace.id = vm.workspace_id \
         JOIN orgs AS org ON org.id = workspace.org_id \
         WHERE org.slug IN ('g5labs', 'g5labs-member') \
           AND workspace.name IN ('demo', 'e2e-test') \
           AND vm.status = 'stopped'",
    )
    .fetch_one(&app.db)
    .await
    .expect("stopped test-token VM lookup should succeed");
    assert_eq!(stopped_vms, 4);
}

/// Legacy fixture orgs are intentionally not repaired by test-token seeding.
/// The route still provisions membership and returns successfully when there
/// are no newly-created Base repositories to materialize or fork.
#[tokio::test]
#[serial_test::serial(e2e_auth)]
async fn test_test_token_skips_legacy_orgs_without_base_code_repos() {
    let _env = EnvVarGuard::set_many(&[("G5_E2E_AUTH", "true")]);
    let app = TestApp::spawn().await;

    for (name, slug) in [
        ("Legacy G5 Labs", "g5labs"),
        ("Legacy G5 Labs Member", "g5labs-member"),
    ] {
        let org_id = Uuid::new_v4();
        sqlx::query("WITH o AS (INSERT INTO orgs (id,name,slug) VALUES ($1,$2,$3) RETURNING id),
             u AS (INSERT INTO users (email,display_name) VALUES ($1::text || '@fixture.test','Fixture Admin') RETURNING id)
             INSERT INTO org_memberships (org_id,user_id,role) SELECT o.id,u.id,'admin' FROM o,u")
            .bind(org_id)
            .bind(name)
            .bind(slug)
            .execute(&app.db)
            .await
            .expect("legacy org fixture should insert");

        sqlx::query(
            "INSERT INTO repos (id, org_id, name, slug, git_url, default_branch) \
             VALUES ($1, $2, 'docs', 'docs', $3, 'main')",
        )
        .bind(Uuid::new_v4())
        .bind(org_id)
        .bind(format!("g5-local://legacy-{slug}.git"))
        .execute(&app.db)
        .await
        .expect("legacy docs repo fixture should insert");
    }

    g5_api::routes::auth::provision_test_token(&app.state, "demo@g5labs.ai")
        .await
        .expect("test-token provisioning should skip legacy Base code repos");

    for slug in ["g5labs", "g5labs-member"] {
        let code_repo_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM repos r JOIN orgs o ON o.id = r.org_id \
             WHERE o.slug = $1 AND r.slug = 'code'",
        )
        .bind(slug)
        .fetch_one(&app.db)
        .await
        .expect("legacy code repo count query should succeed");
        assert_eq!(
            code_repo_count, 0,
            "test-token provisioning must not repair legacy {slug}"
        );
    }

    app.cleanup().await;
}

/// Test-token provisioning must only bootstrap Base code repos born in this
/// invocation. Existing fixture orgs can be incomplete without turning a
/// reissued token into a repair attempt or a 500 response.
#[tokio::test]
#[serial_test::serial(e2e_auth)]
async fn test_test_token_does_not_repair_existing_base_code_branch() {
    let _env = EnvVarGuard::set_many(&[("G5_E2E_AUTH", "true")]);
    let app = TestApp::spawn().await;
    let fixture_user = TestUser::create(&app).await;

    let mut fixture_code_repo_ids = Vec::new();
    for (name, slug) in [
        ("Existing G5 Labs", "g5labs"),
        ("Existing G5 Labs Member", "g5labs-member"),
    ] {
        let org_id = Uuid::new_v4();
        let docs_repo_id = Uuid::new_v4();
        let code_repo_id = Uuid::new_v4();
        let base_workspace_id = Uuid::new_v4();

        let mut setup = app.db.begin().await.unwrap();
        sqlx::query("INSERT INTO orgs (id, name, slug) VALUES ($1, $2, $3)")
            .bind(org_id)
            .bind(name)
            .bind(slug)
            .execute(&mut *setup)
            .await
            .expect("existing org fixture should insert");
        sqlx::query("INSERT INTO org_memberships (org_id, user_id, role) VALUES ($1, $2, 'admin')")
            .bind(org_id)
            .bind(fixture_user.id)
            .execute(&mut *setup)
            .await
            .expect("existing org owner membership should insert");
        setup.commit().await.unwrap();
        sqlx::query(
            "INSERT INTO repos (id, org_id, name, slug, git_url, default_branch) \
             VALUES ($1, $2, $3, $3, $4, 'main')",
        )
        .bind(docs_repo_id)
        .bind(org_id)
        .bind("docs")
        .bind(format!("g5-local://existing-{slug}-docs.git"))
        .execute(&app.db)
        .await
        .expect("existing docs repo fixture should insert");
        sqlx::query(
            "INSERT INTO repos (id, org_id, name, slug, git_url, default_branch) \
             VALUES ($1, $2, $3, $3, $4, 'main')",
        )
        .bind(code_repo_id)
        .bind(org_id)
        .bind("code")
        .bind(format!("g5-local://existing-{slug}-code.git"))
        .execute(&app.db)
        .await
        .expect("existing code repo fixture should insert");
        sqlx::query(
            "INSERT INTO workspaces \
             (id, org_id, name, is_base, docs_repo_id, \
              owner_user_id, created_by) \
             VALUES ($1, $2, 'Base', true, $3, $4, $4)",
        )
        .bind(base_workspace_id)
        .bind(org_id)
        .bind(docs_repo_id)
        .bind(fixture_user.id)
        .execute(&app.db)
        .await
        .expect("existing Base workspace fixture should insert");
        sqlx::query(
            "INSERT INTO workspace_repos (org_id,workspace_id,repo_id,root_path,role) VALUES ($1,$2,$3,'docs','docs')",
        )
        .bind(org_id)
        .bind(base_workspace_id)
        .bind(docs_repo_id)
        .execute(&app.db)
        .await
        .expect("existing Base docs branch fixture should insert");

        fixture_code_repo_ids.push((base_workspace_id, code_repo_id));
    }

    g5_api::routes::auth::provision_test_token(&app.state, "demo@g5labs.ai")
        .await
        .expect("test-token provisioning must not repair existing Base code branches");

    for (base_workspace_id, code_repo_id) in fixture_code_repo_ids {
        let code_main_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM workspace_repos \
             WHERE workspace_id = $1 AND repo_id = $2)",
        )
        .bind(base_workspace_id)
        .bind(code_repo_id)
        .fetch_one(&app.db)
        .await
        .expect("existing Base code branch lookup should succeed");
        assert!(
            !code_main_exists,
            "test-token provisioning must not repair an existing Base code branch"
        );
    }

    app.cleanup().await;
}

/// Asserts the named org's Base workspace has fully materialized, distinct
/// docs+code repository authority members.
async fn assert_base_code_materialized(app: &TestApp, org_slug: &str) {
    let docs_repo_id = assert_base_repo_materialized(app, org_slug, "docs").await;
    let code_repo_id = assert_base_repo_materialized(app, org_slug, "code").await;
    assert_ne!(
        docs_repo_id, code_repo_id,
        "{org_slug} docs and code repos must be distinct"
    );
}

/// Asserts one Base repository's parent projection and stable authority row
/// select the same materialized commit and that the object exists locally.
async fn assert_base_repo_materialized(app: &TestApp, org_slug: &str, repo_slug: &str) -> Uuid {
    let (base_ws_id, repo_id) = sqlx::query_as::<_, (Uuid, Uuid)>(
        "SELECT w.id, r.id FROM workspaces w \
         JOIN repos r ON r.org_id = w.org_id AND r.slug = $2 \
         WHERE w.org_id = (SELECT id FROM orgs WHERE slug = $1) AND w.is_base = true",
    )
    .bind(org_slug)
    .bind(repo_slug)
    .fetch_one(&app.db)
    .await
    .unwrap_or_else(|e| panic!("{org_slug} Base workspace + {repo_slug} repo should exist: {e}"));

    let (mapped_root, default_branch): (String,String) = sqlx::query_as("SELECT m.root_path,r.default_branch FROM workspace_repos m JOIN repos r ON r.id=m.repo_id WHERE m.workspace_id=$1 AND m.repo_id=$2")
        .bind(base_ws_id).bind(repo_id).fetch_one(&app.db).await.unwrap();
    assert_eq!(mapped_root, repo_slug);
    let path = app.repo_path(repo_id).await;
    let repo = git2::Repository::open(path).expect("Base repository exists");
    repo.find_reference(&format!("refs/heads/{default_branch}"))
        .expect("Base uses configured default ref")
        .peel_to_commit()
        .expect("Base ref names an existing commit");

    repo_id
}

#[tokio::test]
async fn local_repo_preparation_leaves_initial_candidate_unpublished() {
    use g5_api_core::services::git::repo_store::TenantRepoStorage;
    let app = TestApp::spawn().await;
    let org_id = Uuid::new_v4();
    let repo_id = Uuid::new_v4();
    let candidate = app
        .state
        .workspace_service
        .initialize_local_workspace_repo(
            &TenantRepoStorage::test_key(org_id, repo_id),
            "Prepared project",
            "trunk",
        )
        .await
        .unwrap();
    let path = app.repo_storage.test_path(org_id, repo_id);
    let repo = git2::Repository::open_bare(&path).unwrap();
    assert_eq!(
        repo.find_reference("HEAD")
            .unwrap()
            .symbolic_target()
            .unwrap(),
        Some("refs/heads/trunk")
    );
    assert!(repo.find_reference("refs/heads/trunk").is_err());
    let commit = repo
        .find_commit(git2::Oid::from_str(&candidate).unwrap())
        .unwrap();
    assert_eq!(commit.parent_count(), 0);
    let entry = commit.tree().unwrap().get_name("README.md").unwrap().id();
    assert_eq!(
        repo.find_blob(entry).unwrap().content(),
        b"# Prepared project\n\nThis is a G5-managed local project repository.\n"
    );
    let invalid_id = Uuid::new_v4();
    assert!(app
        .state
        .workspace_service
        .initialize_local_workspace_repo(
            &TenantRepoStorage::test_key(org_id, invalid_id),
            "Invalid",
            "bad..branch",
        )
        .await
        .is_err());
    assert!(!app.repo_storage.test_path(org_id, invalid_id).exists());
    app.cleanup().await;
}
