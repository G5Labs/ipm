//! CRUD lifecycle integration tests — verifying ScopedDb threading through
//! org/workspace/repo reads, metadata updates, and lifecycle deletion flows.
//!
//! These tests exercise the full request→route→middleware→ScopedDb→service→DB
//! cycle against a real Postgres database. They verify:
//!
//! 1. **Create cascade**: Org creation provisions docs repo, base workspace,
//!    workspace branch, workspace settings, and owner membership atomically.
//! 2. **ScopedDb threading**: All CRUD flows properly set and clear RLS
//!    session context via `ScopedDb`.
//! 3. **Workspace lifecycle**: Read, update, and lifecycle delete with ScopedDb + RLS context setup/teardown in each operation. Canonical fork creation has its own production-boundary suite.
//! 4. **Invitation lifecycle**: Invite → accept → membership created, all
//!    through ScopedDb path.
//! 5. **Cross-org isolation**: Org A's data is invisible to Org B.

use crate::helpers;

use helpers::{TestApp, TestOrg, TestUser, TestWorkspace};
use reqwest::StatusCode;
use serde_json::{json, Value};
use uuid::Uuid;

// ===========================================================================
// 1. Org Create Cascade — Full ScopedDb Path
// ===========================================================================

/// Complete org creation cascade: creates org + owner membership + docs repo
/// + base workspace + workspace branch + workspace settings. Verifies each
/// DB artifact exists and is correctly linked. Uses the HTTP API (not direct
/// service calls) to exercise the full middleware + ScopedDb path.
#[tokio::test]
async fn test_org_create_cascade_full_artifacts() {
    let app = TestApp::spawn().await;
    let user = TestUser::create(&app).await;

    let response =
        helpers::platform::create_org_via_operator(&app, &user, json!({"name": "Cascade Corp"}))
            .await;
    assert_eq!(response.status(), StatusCode::CREATED);

    let body: Value = response.json().await.unwrap();
    let org_id: Uuid = body["data"]["org"]["id"].as_str().unwrap().parse().unwrap();
    let workspace_id: Uuid = body["data"]["baseWorkspace"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let docs_repo_id: Uuid = body["data"]["docsRepo"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();

    // 1. Org exists
    let org_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM orgs WHERE id = $1")
        .bind(org_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(org_count.0, 1, "Org should exist");

    // 2. Initial Admin membership exists
    let membership: (String,) =
        sqlx::query_as("SELECT role FROM org_memberships WHERE org_id = $1 AND user_id = $2")
            .bind(org_id)
            .bind(user.id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(membership.0, "admin");

    // 3. Docs repo exists and belongs to the org
    let repo_row: (Uuid, String) = sqlx::query_as("SELECT org_id, name FROM repos WHERE id = $1")
        .bind(docs_repo_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(repo_row.0, org_id, "Docs repo should belong to the org");
    assert_eq!(repo_row.1, "docs");

    // 4. Base workspace exists
    let ws_row: (Uuid, bool, Uuid) =
        sqlx::query_as("SELECT org_id, is_base, owner_user_id FROM workspaces WHERE id = $1")
            .bind(workspace_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(ws_row.0, org_id, "Workspace should belong to the org");
    assert!(ws_row.1, "Should be a base workspace");
    assert_eq!(ws_row.2, user.id, "Base workspace should retain its owner");

    // 5. Workspace branch exists (links workspace → docs repo)
    let branch_count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM workspace_repos WHERE workspace_id = $1 AND repo_id = $2",
    )
    .bind(workspace_id)
    .bind(docs_repo_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(
        branch_count.0, 1,
        "Workspace branch should link ws to docs repo"
    );

    // 6. Workspace settings exist
    let settings_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM workspace_settings WHERE workspace_id = $1")
            .bind(workspace_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(settings_count.0, 1, "Workspace settings should exist");

    // 7. RLS context should be cleared after the transaction (no leaked session)
    let ctx_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM authz.session_ctx WHERE pid = pg_backend_pid()")
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(
        ctx_count.0, 0,
        "RLS session context should be cleared after request"
    );

    app.cleanup().await;
}

// ===========================================================================
// 2. Workspace CRUD Lifecycle
// ===========================================================================

/// Read → Update → Delete lifecycle for a workspace, verifying each request
/// flows through ScopedDb and the RLS context is properly managed.
#[tokio::test]
async fn test_workspace_crud_lifecycle() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    let workspace = TestWorkspace::create(&app, &org).await;
    let workspace_id = workspace.id;

    // Verify workspace in DB
    let ws_row: (String, Uuid) =
        sqlx::query_as("SELECT name, org_id FROM workspaces WHERE id = $1")
            .bind(workspace_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(ws_row.0, workspace.name);
    assert_eq!(ws_row.1, org.id, "Workspace should belong to the org");

    // READ workspace via HTTP
    let response = client
        .get(&format!("/api/v1/workspaces/{}", workspace_id))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["data"]["name"], workspace.name);

    // UPDATE workspace via HTTP
    let response = client
        .patch_json(
            &format!("/api/v1/workspaces/{}", workspace_id),
            &json!({
                "description": "Updated description"
            }),
        )
        .await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Workspace update should return 200"
    );

    // Verify update in DB
    let ws_description: (Option<String>,) =
        sqlx::query_as("SELECT description FROM workspaces WHERE id = $1")
            .bind(workspace_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(ws_description.0.as_deref(), Some("Updated description"));

    // DELETE workspace via HTTP
    let response = client
        .post_json(
            &format!("/api/v1/workspaces/{}/delete", workspace_id),
            &json!({}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    // Verify workspace row is retained and tombstoned. Removal uses deleted_at;
    let ws_tombstoned: bool =
        sqlx::query_scalar("SELECT deleted_at IS NOT NULL FROM workspaces WHERE id = $1")
            .bind(workspace_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(ws_tombstoned, "Workspace should set deleted_at tombstone");

    app.cleanup().await;
}

// ===========================================================================
// 3. Cross-Org Isolation
// ===========================================================================

/// Verifies that an authenticated user in Org A cannot see Org B's workspaces.
/// This validates that ScopedDb + RLS policies enforce tenant isolation.
#[tokio::test]
async fn test_cross_org_isolation() {
    let app = TestApp::spawn().await;

    // Create two separate users with separate orgs
    let mut user_a = TestUser::create(&app).await;
    let org_a = TestOrg::create_with_admin(&app, &mut user_a).await;
    let _ws_a = TestWorkspace::create(&app, &org_a).await;

    let mut user_b = TestUser::create(&app).await;
    let org_b = TestOrg::create_with_admin(&app, &mut user_b).await;
    let _ws_b = TestWorkspace::create(&app, &org_b).await;

    let client_a = app.authenticated_client(&user_a);

    // User A listing Org B's workspaces should fail
    let response = client_a
        .get(&format!("/api/v1/orgs/{}/workspaces", org_b.id))
        .await;

    // Should return 403 (forbidden) or 404 (not found) — either is correct
    // for cross-org isolation
    assert!(
        response.status() == StatusCode::FORBIDDEN || response.status() == StatusCode::NOT_FOUND,
        "Cross-org access should be denied, got {}",
        response.status()
    );

    // User B listing Org A's workspaces should also fail
    let client_b = app.authenticated_client(&user_b);
    let response = client_b
        .get(&format!("/api/v1/orgs/{}/workspaces", org_a.id))
        .await;

    assert!(
        response.status() == StatusCode::FORBIDDEN || response.status() == StatusCode::NOT_FOUND,
        "Cross-org access should be denied, got {}",
        response.status()
    );

    // Each user can see their own org's workspaces
    let response = client_a
        .get(&format!("/api/v1/orgs/{}/workspaces", org_a.id))
        .await;
    assert_eq!(response.status(), StatusCode::OK);

    let response = client_b
        .get(&format!("/api/v1/orgs/{}/workspaces", org_b.id))
        .await;
    assert_eq!(response.status(), StatusCode::OK);

    app.cleanup().await;
}

// ===========================================================================
// 4. Invitation Accept → Membership Created
// ===========================================================================

/// Full invitation lifecycle: Owner invites user → User accepts → Membership
/// is created. Exercises the ScopedDb path for both the invite and accept flows.
#[tokio::test]
async fn test_invitation_lifecycle_via_http() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let owner_client = app.authenticated_client(&owner);

    // Create a new user who will be invited
    let invitee = TestUser::create(&app).await;
    let invitee_client = app.authenticated_client(&invitee);

    // Owner invites the user
    let response = owner_client
        .post_json(
            &format!("/api/v1/orgs/{}/members", org.id),
            &json!({
                "email": invitee.email,
                "role": "member"
            }),
        )
        .await;

    // Should be 201 Created or 200 OK depending on implementation
    assert!(
        response.status() == StatusCode::CREATED || response.status() == StatusCode::OK,
        "Invitation should succeed, got {}",
        response.status()
    );

    let body: Value = response.json().await.unwrap();

    // Find invitation ID from either nested invitation payload or flat field.
    let invitation_id = body["data"]["invitation"]["id"]
        .as_str()
        .or_else(|| body["data"]["id"].as_str())
        .or_else(|| body["data"]["invitationId"].as_str());

    // Verify either pending invitation OR immediate membership (existing-user fast-path).
    let invite_count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM org_invitations WHERE org_id = $1 AND email = $2 AND status = 'pending'",
    )
    .bind(org.id)
    .bind(&invitee.email)
    .fetch_one(&app.db)
    .await
    .unwrap();
    let member_count_before_accept: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM org_memberships WHERE org_id = $1 AND user_id = $2")
            .bind(org.id)
            .bind(invitee.id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(
        invite_count.0 == 1 || member_count_before_accept.0 == 1,
        "Expected pending invitation or immediate membership; invites={}, memberships={}",
        invite_count.0,
        member_count_before_accept.0
    );

    // If we got an invitation ID from the response, accept it via HTTP
    if invite_count.0 == 1 {
        let inv_id_str =
            invitation_id.expect("invitation id should be present when invitation exists");
        let inv_id: Uuid = inv_id_str.parse().unwrap();

        let response = invitee_client
            .post_json(
                &format!("/api/v1/invitations/{}/accept", inv_id),
                &json!({}),
            )
            .await;

        assert!(
            response.status() == StatusCode::OK || response.status() == StatusCode::CREATED,
            "Accept invitation should succeed, got {}",
            response.status()
        );

        // Verify membership was created
        let member_count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM org_memberships WHERE org_id = $1 AND user_id = $2",
        )
        .bind(org.id)
        .bind(invitee.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
        assert_eq!(
            member_count.0, 1,
            "Membership should exist after accepting invitation"
        );

        // Verify invitation is now accepted
        let invite_status: (String,) =
            sqlx::query_as("SELECT status FROM org_invitations WHERE id = $1")
                .bind(inv_id)
                .fetch_one(&app.db)
                .await
                .unwrap();
        assert_eq!(invite_status.0, "accepted");
    } else {
        assert_eq!(
            member_count_before_accept.0, 1,
            "Immediate membership path should create org membership"
        );
    }

    app.cleanup().await;
}

// ===========================================================================
// 5. Org Update Flow
// ===========================================================================

/// Verifies org update flows through ScopedDb correctly.
#[tokio::test]
async fn test_org_update_via_scoped_db_path() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    // Update org name
    let response = client
        .patch_json(
            &format!("/api/v1/orgs/{}", org.id),
            &json!({"name": "Updated Org Name"}),
        )
        .await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Org update should return 200"
    );

    // Verify in DB
    let org_name: (String,) = sqlx::query_as("SELECT name FROM orgs WHERE id = $1")
        .bind(org.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(org_name.0, "Updated Org Name");

    app.cleanup().await;
}

// ===========================================================================
// 6. Member Role Update
// ===========================================================================

/// Verifies member role update flows through ScopedDb. Owner promotes a
/// member to admin.
#[tokio::test]
async fn test_member_role_update_lifecycle() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let owner_client = app.authenticated_client(&owner);

    // Add a member directly in DB to avoid invitation complexity
    let member = TestUser::create(&app).await;
    sqlx::query("INSERT INTO org_memberships (org_id, user_id, role) VALUES ($1, $2, 'member')")
        .bind(org.id)
        .bind(member.id)
        .execute(&app.db)
        .await
        .unwrap();

    // Owner updates member's role to admin
    let response = owner_client
        .patch_json(
            &format!("/api/v1/orgs/{}/members/{}", org.id, member.id),
            &json!({"role": "admin"}),
        )
        .await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Member role update should return 200"
    );

    // Verify role changed in DB
    let role: (String,) =
        sqlx::query_as("SELECT role FROM org_memberships WHERE org_id = $1 AND user_id = $2")
            .bind(org.id)
            .bind(member.id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(role.0, "admin", "Member role should be updated to admin");

    app.cleanup().await;
}

// ===========================================================================
// 7. Member Removal
// ===========================================================================

/// Verifies removing a member from an org through the full ScopedDb path.
#[tokio::test]
async fn test_member_removal_lifecycle() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let owner_client = app.authenticated_client(&owner);

    // Add a member
    let member = TestUser::create(&app).await;
    sqlx::query("INSERT INTO org_memberships (org_id, user_id, role) VALUES ($1, $2, 'member')")
        .bind(org.id)
        .bind(member.id)
        .execute(&app.db)
        .await
        .unwrap();

    // Verify member exists
    let before_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM org_memberships WHERE org_id = $1 AND user_id = $2")
            .bind(org.id)
            .bind(member.id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(before_count.0, 1);

    // Remove member
    let response = owner_client
        .delete(&format!("/api/v1/orgs/{}/members/{}", org.id, member.id))
        .await;
    assert!(
        response.status() == StatusCode::OK || response.status() == StatusCode::NO_CONTENT,
        "Member removal should succeed, got {}",
        response.status()
    );

    // Verify member is gone
    let after_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM org_memberships WHERE org_id = $1 AND user_id = $2")
            .bind(org.id)
            .bind(member.id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(after_count.0, 0, "Membership should be removed");

    app.cleanup().await;
}

// ===========================================================================
// 8. Multiple Workspace Operations Within Same Org
// ===========================================================================

/// Seeds multiple valid workspace fixtures within the same org, verifies all are visible
/// in the list, then deletes one and confirms it no longer appears.
#[tokio::test]
async fn test_multiple_workspace_operations_same_org() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    // Seed three complete workspace fixtures. Canonical creation behavior is
    // exercised by the fork-service suite; this test owns list/delete behavior.
    let mut workspace_ids = Vec::new();
    for _ in 0..3 {
        workspace_ids.push(TestWorkspace::create(&app, &org).await.id);
    }

    // List workspaces
    let response = client
        .get(&format!("/api/v1/orgs/{}/workspaces", org.id))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();

    // Should have at least 3 workspaces (plus the base workspace from org creation
    // if the org was created via HTTP; since we used TestOrg::create, no base ws)
    let data = body["data"].as_array().expect("data should be array");
    assert!(
        data.len() >= 3,
        "Should have at least 3 workspaces, got {}",
        data.len()
    );

    // Delete the middle workspace
    let response = client
        .post_json(
            &format!("/api/v1/workspaces/{}/delete", workspace_ids[1]),
            &json!({}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    // List again — deleted workspace should be gone
    let response = client
        .get(&format!("/api/v1/orgs/{}/workspaces", org.id))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    let data = body["data"].as_array().expect("data should be array");

    let remaining_ids: Vec<String> = data
        .iter()
        .map(|ws| ws["id"].as_str().unwrap_or("").to_string())
        .collect();
    assert!(
        !remaining_ids.contains(&workspace_ids[1].to_string()),
        "Deleted workspace should not appear in list"
    );

    app.cleanup().await;
}

// ===========================================================================
// 9. Repo Listing Through ScopedDb
// ===========================================================================

/// Verifies that listing repos for an org flows through ScopedDb correctly.
#[tokio::test]
async fn test_repo_listing_scoped_db_path() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    // Insert a repo directly
    let repo_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO repos (id, org_id, name, slug, git_url, default_branch) VALUES ($1, $2, 'test-repo', 'test-repo', 'https://github.com/test/repo', 'main')",
    )
    .bind(repo_id)
    .bind(org.id)
    .execute(&app.db)
    .await
    .unwrap();

    // List repos via HTTP
    let response = client.get(&format!("/api/v1/orgs/{}/repos", org.id)).await;
    assert_eq!(response.status(), StatusCode::OK);

    let body: Value = response.json().await.unwrap();
    let repos = body["data"].as_array().expect("data should be array");
    assert!(
        repos
            .iter()
            .any(|r| r["id"].as_str() == Some(&repo_id.to_string())),
        "Repo should appear in listing"
    );

    app.cleanup().await;
}
