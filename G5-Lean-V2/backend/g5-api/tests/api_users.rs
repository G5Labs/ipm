//! Integration tests for G5 API user profile endpoints.
//!
//! Covers the complete request→route→middleware→service→database→response cycle
//! for the 3 endpoints under the `/api/v1/users` path prefix:
//!
//! 1. **`GET /api/v1/users/me`** — Get the current authenticated user's full profile
//!    (including org memberships and SCM connections)
//! 2. **`PATCH /api/v1/users/me`** — Update the current user's display name and/or avatar URL
//! 3. **`GET /api/v1/users/:id`** — Get a minimal public user profile (id, display name, avatar URL)
//!
//! Each test spawns a full `TestApp` instance with an isolated Postgres database,
//! makes real HTTP requests via `AuthenticatedClient` (or raw `reqwest::Client`
//! for unauthenticated tests), and asserts at the HTTP response level (status codes,
//! JSON body shapes) and the database level (verifying rows were created/updated).

use crate::helpers;
use g5_auth::Scope;

use helpers::{assert_json_error, TestApp, TestOrg, TestUser};
use reqwest::StatusCode;
use serde_json::{json, Value};
use uuid::Uuid;

// ===========================================================================
// Group 1: Get Current User — GET /api/v1/users/me
// ===========================================================================

/// Verifies that `GET /api/v1/users/me` returns the full profile of the
/// authenticated user, including org memberships and SCM connections.
#[tokio::test]
async fn test_get_me_returns_current_user_profile() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(&app, org.id, &[Scope::UsersRead]).await;
    let client = app.authenticated_client(&user);

    let response = client.get("/api/v1/users/me").await;

    assert_eq!(response.status(), StatusCode::OK);

    let body: Value = response.json().await.expect("Failed to parse JSON");
    let data = &body["data"];

    // Verify core user fields
    assert_eq!(data["id"].as_str().unwrap(), user.id.to_string());
    assert_eq!(data["email"].as_str().unwrap(), user.email);
    assert_eq!(data["displayName"].as_str().unwrap(), user.display_name);

    // Verify org memberships are present (user was created with org membership)
    assert!(data["orgs"].is_array(), "orgs should be an array");
    let orgs = data["orgs"].as_array().unwrap();
    assert!(
        !orgs.is_empty(),
        "User should have at least one org membership"
    );
    let first_org = &orgs[0];
    assert_eq!(first_org["orgId"].as_str().unwrap(), org.id.to_string());
    assert_eq!(first_org["orgName"].as_str().unwrap(), org.name);
    assert_eq!(first_org["orgSlug"].as_str().unwrap(), org.slug);
    assert!(first_org["role"].is_string(), "role should be a string");

    // Verify SCM connections are present (initially empty)
    assert!(
        data["scmConnections"].is_array(),
        "scmConnections should be an array"
    );

    // Verify createdAt is present
    assert!(
        data["createdAt"].is_string(),
        "createdAt should be a string"
    );

    app.cleanup().await;
}

/// Verifies that `GET /api/v1/users/me` without a JWT returns 401 UNAUTHORIZED.
#[tokio::test]
async fn test_get_me_without_auth_returns_401() {
    let app = TestApp::spawn().await;

    let response = app
        .client
        .get(format!("{}/api/v1/users/me", app.base_url))
        .send()
        .await
        .expect("Failed to send request");

    assert_json_error(response, StatusCode::UNAUTHORIZED, "UNAUTHORIZED").await;

    app.cleanup().await;
}

// ===========================================================================
// Group 2: Update Current User — PATCH /api/v1/users/me
// ===========================================================================

/// Verifies that `PATCH /api/v1/users/me` updates the user's display name
/// and returns the full updated profile.
#[tokio::test]
async fn test_update_me_display_name() {
    let app = TestApp::spawn().await;
    let user = TestUser::create(&app).await;
    let client = app.authenticated_client(&user);

    let response = client
        .patch_json(
            "/api/v1/users/me",
            &json!({
                "displayName": "Updated Name"
            }),
        )
        .await;

    assert_eq!(response.status(), StatusCode::OK);

    let body: Value = response.json().await.expect("Failed to parse JSON");
    let data = &body["data"];
    assert_eq!(data["displayName"].as_str().unwrap(), "Updated Name");
    assert_eq!(data["id"].as_str().unwrap(), user.id.to_string());

    // Verify database was updated
    let row = sqlx::query!("SELECT display_name FROM users WHERE id = $1", user.id)
        .fetch_one(&app.db)
        .await
        .expect("Failed to query database");
    assert_eq!(row.display_name, "Updated Name");

    app.cleanup().await;
}

/// Verifies that `PATCH /api/v1/users/me` can update the avatar URL and also
/// clear it by setting to null.
#[tokio::test]
async fn test_update_me_avatar_url() {
    let app = TestApp::spawn().await;
    let user = TestUser::create(&app).await;
    let client = app.authenticated_client(&user);

    // Set avatar URL
    let response = client
        .patch_json(
            "/api/v1/users/me",
            &json!({
                "avatarUrl": "https://avatars.example.com/new-avatar.png"
            }),
        )
        .await;

    assert_eq!(response.status(), StatusCode::OK);

    let body: Value = response.json().await.expect("Failed to parse JSON");
    let data = &body["data"];
    assert_eq!(
        data["avatarUrl"].as_str().unwrap(),
        "https://avatars.example.com/new-avatar.png"
    );

    // Clear avatar URL by setting to null
    let response = client
        .patch_json(
            "/api/v1/users/me",
            &json!({
                "avatarUrl": null
            }),
        )
        .await;

    assert_eq!(response.status(), StatusCode::OK);

    let body: Value = response.json().await.expect("Failed to parse JSON");
    let data = &body["data"];
    assert!(
        data["avatarUrl"].is_null(),
        "avatarUrl should be null after clearing"
    );

    app.cleanup().await;
}

/// Verifies that `PATCH /api/v1/users/me` without authentication returns 401.
#[tokio::test]
async fn test_update_me_without_auth_returns_401() {
    let app = TestApp::spawn().await;

    let response = app
        .client
        .patch(format!("{}/api/v1/users/me", app.base_url))
        .header("Content-Type", "application/json")
        .json(&json!({
            "displayName": "Hacker"
        }))
        .send()
        .await
        .expect("Failed to send request");

    assert_json_error(response, StatusCode::UNAUTHORIZED, "UNAUTHORIZED").await;

    app.cleanup().await;
}

/// Verifies that `PATCH /api/v1/users/me` with an empty display name returns
/// 422 VALIDATION_ERROR.
#[tokio::test]
async fn test_update_me_empty_display_name_returns_422() {
    let app = TestApp::spawn().await;
    let user = TestUser::create(&app).await;
    let client = app.authenticated_client(&user);

    let response = client
        .patch_json(
            "/api/v1/users/me",
            &json!({
                "displayName": ""
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

/// Verifies that `PATCH /api/v1/users/me` with an empty JSON body (no fields
/// to update) returns 422 VALIDATION_ERROR.
#[tokio::test]
async fn test_update_me_empty_body_returns_422() {
    let app = TestApp::spawn().await;
    let user = TestUser::create(&app).await;
    let client = app.authenticated_client(&user);

    let response = client.patch_json("/api/v1/users/me", &json!({})).await;

    assert_json_error(
        response,
        StatusCode::UNPROCESSABLE_ENTITY,
        "VALIDATION_ERROR",
    )
    .await;

    app.cleanup().await;
}

/// The approval-request email preference defaults on and round-trips through
/// `PATCH /api/v1/users/me` (users g5.user.notifications).
#[tokio::test]
async fn test_email_on_approval_request_preference_defaults_on_and_round_trips() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(&app, org.id, &[Scope::UsersRead]).await;
    let client = app.authenticated_client(&user);

    let initial: Value = client
        .get("/api/v1/users/me")
        .await
        .json()
        .await
        .expect("JSON");
    assert_eq!(
        initial["data"]["emailOnApprovalRequest"], true,
        "defaults on for existing and new users"
    );

    let off: Value = client
        .patch_json(
            "/api/v1/users/me",
            &json!({ "emailOnApprovalRequest": false }),
        )
        .await
        .json()
        .await
        .expect("JSON");
    assert_eq!(off["data"]["emailOnApprovalRequest"], false);

    let reread: Value = client
        .get("/api/v1/users/me")
        .await
        .json()
        .await
        .expect("JSON");
    assert_eq!(reread["data"]["emailOnApprovalRequest"], false);

    app.cleanup().await;
}

/// Personal merge-stop preferences persist on the account and cannot leak
/// through a public profile or change another person's default.
#[tokio::test]
async fn personal_merge_stop_default_round_trips_and_is_private() {
    use crate::helpers::review_flow::body;
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(&app, org.id, &[Scope::UsersRead]).await;
    let other = TestUser::create_with_scopes(&app, org.id, &[Scope::UsersRead]).await;
    let client = app.authenticated_client(&user);
    let initial = body(client.get("/api/v1/users/me").await, StatusCode::OK).await;
    assert_eq!(initial["stopVmAfterMergeDefault"], false);
    for selected in [true, false] {
        let saved = body(
            client
                .patch_json(
                    "/api/v1/users/me",
                    &json!({"stopVmAfterMergeDefault": selected}),
                )
                .await,
            StatusCode::OK,
        )
        .await;
        assert_eq!(saved["stopVmAfterMergeDefault"], selected);
        assert_eq!(saved["emailOnApprovalRequest"], true);
        let unchanged = body(
            client
                .patch_json("/api/v1/users/me", &json!({"displayName":"New name"}))
                .await,
            StatusCode::OK,
        )
        .await;
        assert_eq!(unchanged["stopVmAfterMergeDefault"], selected);
        let reread = body(client.get("/api/v1/users/me").await, StatusCode::OK).await;
        assert_eq!(reread["stopVmAfterMergeDefault"], selected);
        let isolated = body(
            app.authenticated_client(&other)
                .get("/api/v1/users/me")
                .await,
            StatusCode::OK,
        )
        .await;
        assert_eq!(isolated["stopVmAfterMergeDefault"], false);
        let public = body(
            app.authenticated_client(&other)
                .get(&format!("/api/v1/users/{}", user.id))
                .await,
            StatusCode::OK,
        )
        .await;
        assert!(public.get("stopVmAfterMergeDefault").is_none());
    }
    let invalid = client
        .patch_json(
            "/api/v1/users/me",
            &json!({"stopVmAfterMergeDefault":"yes"}),
        )
        .await;
    assert_eq!(invalid.status(), StatusCode::UNPROCESSABLE_ENTITY);
    app.cleanup().await;
}

// ===========================================================================
// Group 3: Get Public User Profile — GET /api/v1/users/:id
// ===========================================================================

/// Verifies that `GET /api/v1/users/:id` returns a minimal public profile
/// (id, display name, avatar URL only — no email, no org memberships, no SCM
/// connections).
#[tokio::test]
async fn test_get_user_public_returns_minimal_profile() {
    let app = TestApp::spawn().await;
    let target_user = TestUser::create(&app).await;
    let viewer = TestUser::create(&app).await;
    let client = app.authenticated_client(&viewer);

    let response = client
        .get(&format!("/api/v1/users/{}", target_user.id))
        .await;

    assert_eq!(response.status(), StatusCode::OK);

    let body: Value = response.json().await.expect("Failed to parse JSON");
    let data = &body["data"];

    // Verify public fields are present
    assert_eq!(data["id"].as_str().unwrap(), target_user.id.to_string());
    assert_eq!(
        data["displayName"].as_str().unwrap(),
        target_user.display_name
    );

    // Verify private fields are NOT present
    assert!(
        data["email"].is_null() || data.get("email").is_none(),
        "Public profile should NOT include email"
    );
    assert!(
        data["oauthProvider"].is_null() || data.get("oauthProvider").is_none(),
        "Public profile should NOT include oauthProvider"
    );
    assert!(
        data["orgs"].is_null() || data.get("orgs").is_none(),
        "Public profile should NOT include orgs"
    );
    assert!(
        data["scmConnections"].is_null() || data.get("scmConnections").is_none(),
        "Public profile should NOT include scmConnections"
    );

    app.cleanup().await;
}

/// Verifies that `GET /api/v1/users/:id` with a non-existent UUID returns
/// 404 NOT_FOUND.
#[tokio::test]
async fn test_get_user_public_nonexistent_returns_404() {
    let app = TestApp::spawn().await;
    let user = TestUser::create(&app).await;
    let client = app.authenticated_client(&user);

    let fake_id = Uuid::new_v4();
    let response = client.get(&format!("/api/v1/users/{}", fake_id)).await;

    assert_json_error(response, StatusCode::NOT_FOUND, "NOT_FOUND").await;

    app.cleanup().await;
}

/// Verifies that `GET /api/v1/users/:id` without authentication returns 401.
#[tokio::test]
async fn test_get_user_public_without_auth_returns_401() {
    let app = TestApp::spawn().await;

    let fake_id = Uuid::new_v4();
    let response = app
        .client
        .get(format!("{}/api/v1/users/{}", app.base_url, fake_id))
        .send()
        .await
        .expect("Failed to send request");

    assert_json_error(response, StatusCode::UNAUTHORIZED, "UNAUTHORIZED").await;

    app.cleanup().await;
}

// ===========================================================================
// Group 4: Onboarding tour progress — users g5.user.onboarding
// ===========================================================================

/// A fresh user has never seen the tour: null step, not skipped.
#[tokio::test]
async fn test_me_reports_onboarding_never_shown() {
    let app = TestApp::spawn().await;
    let user = TestUser::create(&app).await;
    let client = app.authenticated_client(&user);

    let response = client.get("/api/v1/users/me").await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("Failed to parse JSON");
    assert!(body["data"]["onboardingStep"].is_null());
    assert_eq!(body["data"]["onboardingSkipped"], json!(false));

    app.cleanup().await;
}

/// The recorded step only rises: a replay reporting an earlier step leaves
/// the record where it was, and the skipped flag follows what was sent.
#[tokio::test]
async fn test_update_me_onboarding_step_never_goes_down() {
    let app = TestApp::spawn().await;
    let user = TestUser::create(&app).await;
    let client = app.authenticated_client(&user);

    let response = client
        .patch_json("/api/v1/users/me", &json!({ "onboardingStep": 3 }))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("Failed to parse JSON");
    assert_eq!(body["data"]["onboardingStep"], json!(3));
    assert_eq!(body["data"]["onboardingSkipped"], json!(false));

    let response = client
        .patch_json(
            "/api/v1/users/me",
            &json!({ "onboardingStep": 1, "onboardingSkipped": true }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("Failed to parse JSON");
    assert_eq!(body["data"]["onboardingStep"], json!(3));
    assert_eq!(body["data"]["onboardingSkipped"], json!(true));

    let response = client
        .patch_json(
            "/api/v1/users/me",
            &json!({ "onboardingStep": 7, "onboardingSkipped": false }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("Failed to parse JSON");
    assert_eq!(body["data"]["onboardingStep"], json!(7));
    assert_eq!(body["data"]["onboardingSkipped"], json!(false));

    let row = sqlx::query!(
        "SELECT onboarding_step, onboarding_skipped FROM users WHERE id = $1",
        user.id
    )
    .fetch_one(&app.db)
    .await
    .expect("Failed to query database");
    assert_eq!(row.onboarding_step, Some(7));
    assert!(!row.onboarding_skipped);

    // Display name updates leave the tour record alone.
    let response = client
        .patch_json("/api/v1/users/me", &json!({ "displayName": "Still Me" }))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("Failed to parse JSON");
    assert_eq!(body["data"]["onboardingStep"], json!(7));

    app.cleanup().await;
}

/// Steps outside the tour (negative or past the finished value) are refused.
#[tokio::test]
async fn test_update_me_onboarding_step_out_of_range() {
    let app = TestApp::spawn().await;
    let user = TestUser::create(&app).await;
    let client = app.authenticated_client(&user);

    for step in [-1, 8] {
        let response = client
            .patch_json("/api/v1/users/me", &json!({ "onboardingStep": step }))
            .await;
        assert_eq!(
            response.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "step {step}"
        );
    }

    let response = client.get("/api/v1/users/me").await;
    let body: Value = response.json().await.expect("Failed to parse JSON");
    assert!(body["data"]["onboardingStep"].is_null());

    app.cleanup().await;
}
