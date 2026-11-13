//! Credential and transactional authority (G5-Lean#660) over the real HTTP
//! boundary, authentication and PostgreSQL.
//!
//! - API-key management is a person's administration
//!   (`g5.user.api-key-authority`, `g5.security.credential-classes`): a
//!   delegated key cannot create or revoke keys, so no credential it could
//!   issue carries scopes it lacks or outlives its own revocation.
//! - OAuth acceptance resolves against the current invitation
//!   (`g5.organization.invitation-authority`): a revoke committed before the
//!   claim yields no membership and is not overwritten as accepted; a normal
//!   pending acceptance succeeds once.
//! - Additive grants cannot raise a viewer above read-only
//!   (`g5.organization.viewer-ceiling`), in current authority and in issued
//!   tokens, without denying owner or administrator actions.
//! - Refresh credentials are stored as digests
//!   (`g5.user.session-authority`): the plaintext is returned once and the
//!   stored value cannot be exchanged for a session.

use crate::helpers;

use g5_auth::Scope;
use helpers::{test_config, AuthenticatedClient, TestApp, TestOrg, TestUser, TestWorkspace};
use reqwest::StatusCode;
use serde_json::{json, Value};
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn set_role(app: &TestApp, org: Uuid, user: Uuid, role: &str) {
    sqlx::query(
        "INSERT INTO org_memberships (org_id, user_id, role) VALUES ($1, $2, $3)
         ON CONFLICT (org_id, user_id) DO UPDATE SET role = EXCLUDED.role",
    )
    .bind(org)
    .bind(user)
    .bind(role)
    .execute(&app.db)
    .await
    .expect("set the membership role");
}

fn key_client(app: &TestApp, key: &str) -> AuthenticatedClient {
    AuthenticatedClient::new(
        reqwest::Client::new(),
        app.base_url.clone(),
        key.to_string(),
    )
}

async fn create_key(
    client: &AuthenticatedClient,
    org_id: Uuid,
    scopes: &[&str],
) -> (StatusCode, Value) {
    let response = client
        .post_json(
            &format!("/api/v1/orgs/{org_id}/api-keys"),
            &json!({ "name": "delegated", "scopes": scopes }),
        )
        .await;
    let status = response.status();
    (status, response.json().await.unwrap_or(Value::Null))
}

async fn key_rows(app: &TestApp, org_id: Uuid) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM api_keys WHERE org_id = $1")
        .bind(org_id)
        .fetch_one(&app.db)
        .await
        .expect("count keys")
}

// ===========================================================================
// Delegated keys cannot mint or revoke credentials
// ===========================================================================

#[tokio::test]
async fn a_delegated_key_cannot_mint_or_revoke_keys_while_a_person_still_can() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    // A non-owner administrator, so their membership can later be deactivated.
    let admin = TestUser::create_with_scopes(&app, org.id, &[Scope::OrgAdmin]).await;
    set_role(&app, org.id, admin.id, "admin").await;
    let session = app.authenticated_client(&admin);
    let keys_path = format!("/api/v1/orgs/{}/api-keys", org.id);

    // The person issues a narrow administrator key.
    let (status, body) = create_key(&session, org.id, &["org:admin"]).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let parent_id = body["data"]["apiKey"]["id"].as_str().unwrap().to_string();
    let parent = key_client(&app, body["data"]["key"].as_str().unwrap());

    // The key reads the key list within its scope...
    assert_eq!(parent.get(&keys_path).await.status(), StatusCode::OK);

    // ...but cannot mint any credential: not its own scopes, not broader
    // ones, not an empty key. Nothing is written.
    let before = key_rows(&app, org.id).await;
    for scopes in [
        vec!["org:admin"],
        vec!["org:admin", "workspaces:write", "workers:admin"],
        vec![],
    ] {
        let (status, body) = create_key(&parent, org.id, &scopes).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{scopes:?}: {body}");
        assert_eq!(
            body["error"]["details"]["missing_scopes"],
            json!(["user_session_required"])
        );
    }
    assert_eq!(key_rows(&app, org.id).await, before);

    // Nor revoke one: the person's second key stays valid.
    let (status, body) = create_key(&session, org.id, &["workspaces:read"]).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let sibling_id = body["data"]["apiKey"]["id"].as_str().unwrap().to_string();
    let sibling = key_client(&app, body["data"]["key"].as_str().unwrap());
    let response = parent.delete(&format!("{keys_path}/{sibling_id}")).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let workspace = TestWorkspace::create(&app, &org).await;
    let workspace_path = format!("/api/v1/workspaces/{}", workspace.id);
    assert_eq!(sibling.get(&workspace_path).await.status(), StatusCode::OK);

    // The key stays bound to its organization and to a current membership.
    let other = TestOrg::create(&app).await;
    set_role(&app, other.id, admin.id, "admin").await;
    assert_eq!(
        parent
            .get(&format!("/api/v1/orgs/{}/api-keys", other.id))
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    sqlx::query(
        "UPDATE org_memberships SET deactivated_at = now() WHERE org_id = $1 AND user_id = $2",
    )
    .bind(org.id)
    .bind(admin.id)
    .execute(&app.db)
    .await
    .unwrap();
    assert_eq!(
        parent.get(&keys_path).await.status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        sibling.get(&workspace_path).await.status(),
        StatusCode::UNAUTHORIZED
    );
    sqlx::query(
        "UPDATE org_memberships SET deactivated_at = NULL WHERE org_id = $1 AND user_id = $2",
    )
    .bind(org.id)
    .bind(admin.id)
    .execute(&app.db)
    .await
    .unwrap();
    assert_eq!(parent.get(&keys_path).await.status(), StatusCode::OK);

    // The person's session administers: revoking the parent ends its use at
    // once, and the sibling it could not touch keeps working.
    let response = session.delete(&format!("{keys_path}/{parent_id}")).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        parent.get(&keys_path).await.status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(sibling.get(&workspace_path).await.status(), StatusCode::OK);
    let response = session.delete(&format!("{keys_path}/{sibling_id}")).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        sibling.get(&workspace_path).await.status(),
        StatusCode::UNAUTHORIZED
    );

    app.cleanup().await;
}

// ===========================================================================
// OAuth acceptance resolves against the current invitation
// ===========================================================================

/// Makes GitHub prove `address` for the account `oauth_id`, replacing any
/// identity the mock presented before.
async fn present_github_identity(github: &MockServer, address: &str, oauth_id: u64) {
    github.reset().await;
    Mock::given(method("POST"))
        .and(path("/login/oauth/access_token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "invitee-token", "token_type": "bearer", "scope": "user:email"
        })))
        .mount(github)
        .await;
    Mock::given(method("GET"))
        .and(path("/user"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": oauth_id, "login": format!("invitee-{oauth_id}"), "name": "Invitee",
            "email": address, "avatar_url": "https://example.com/invitee.png"
        })))
        .mount(github)
        .await;
    Mock::given(method("GET"))
        .and(path("/user/emails"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            { "email": address, "primary": true, "verified": true }
        ])))
        .mount(github)
        .await;
}

/// An app whose GitHub login proves `address` for one account.
async fn app_with_github_login(address: &str) -> (TestApp, MockServer) {
    let github = MockServer::start().await;
    present_github_identity(&github, address, 660_001).await;
    let mut config = test_config();
    config.auth.github = Some(g5_api::config::OAuthProviderConfig {
        client_id: "test-github-client-id".to_string(),
        client_secret: "test-github-client-secret".to_string(),
        authorize_url: Some(format!("{}/login/oauth/authorize", github.uri())),
        token_url: Some(format!("{}/login/oauth/access_token", github.uri())),
        userinfo_url: Some(format!("{}/user", github.uri())),
    });
    (TestApp::spawn_with_config(config).await, github)
}

fn oauth_attempt_cookie(response: &reqwest::Response) -> String {
    response
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find(|value| value.contains("g5_oauth_"))
        .and_then(|value| value.split(';').next())
        .expect("OAuth initiation sets an attempt cookie")
        .to_string()
}

/// Starts a GitHub login and returns the callback request that completes it,
/// so the caller decides when it runs.
async fn github_callback(app: &TestApp) -> reqwest::RequestBuilder {
    let browser = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let login = browser
        .get(format!("{}/api/v1/auth/login/github", app.base_url))
        .send()
        .await
        .unwrap();
    assert!(login.status().is_redirection(), "{}", login.status());
    let state = reqwest::Url::parse(login.headers()["location"].to_str().unwrap())
        .unwrap()
        .query_pairs()
        .find(|(name, _)| name == "state")
        .map(|(_, value)| value.to_string())
        .unwrap();
    let attempt = oauth_attempt_cookie(&login);
    browser
        .get(format!(
            "{}/api/v1/auth/callback/github?code=invitee-code&state={state}",
            app.base_url
        ))
        .header("cookie", attempt)
}

async fn invite(app: &TestApp, owner: &TestUser, org_id: Uuid, address: &str, role: &str) -> Uuid {
    let response = app
        .authenticated_client(owner)
        .post_json(
            &format!("/api/v1/orgs/{org_id}/members"),
            &json!({ "email": address, "role": role }),
        )
        .await;
    assert!(
        response.status().is_success(),
        "invite: {} {}",
        response.status(),
        response.text().await.unwrap_or_default()
    );
    sqlx::query_scalar(
        "SELECT id FROM org_invitations
         WHERE org_id = $1 AND lower(email) = lower($2) AND status = 'pending'",
    )
    .bind(org_id)
    .bind(address)
    .fetch_one(&app.db)
    .await
    .expect("the pending invitation")
}

async fn membership(app: &TestApp, org_id: Uuid, address: &str) -> Option<String> {
    sqlx::query_scalar(
        "SELECT m.role FROM org_memberships m JOIN users u ON u.id = m.user_id
         WHERE m.org_id = $1 AND lower(u.email) = lower($2)",
    )
    .bind(org_id)
    .bind(address)
    .fetch_optional(&app.db)
    .await
    .expect("read membership")
}

async fn invitation_status(app: &TestApp, invitation_id: Uuid) -> Option<String> {
    sqlx::query_scalar("SELECT status FROM org_invitations WHERE id = $1")
        .bind(invitation_id)
        .fetch_optional(&app.db)
        .await
        .expect("read invitation")
}

#[tokio::test]
async fn oauth_acceptance_waits_for_an_in_flight_revoke_and_grants_nothing_after_it() {
    let address = format!("invitee-{}@example.com", Uuid::new_v4().simple());
    let (app, github) = app_with_github_login(&address).await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let invitation_id = invite(&app, &owner, org.id, &address, "admin").await;

    // An administrator's revoke is in flight when the login arrives.
    let mut revoke = app.db.begin().await.unwrap();
    let deleted = sqlx::query("DELETE FROM org_invitations WHERE id = $1 AND status = 'pending'")
        .bind(invitation_id)
        .execute(&mut *revoke)
        .await
        .unwrap()
        .rows_affected();
    assert_eq!(deleted, 1);
    let callback = github_callback(&app).await;
    let login = tokio::spawn(async move { callback.send().await.unwrap() });
    helpers::wait_for_lock_wait(&app).await;
    assert!(!login.is_finished(), "the login waits for the revoke");
    revoke.commit().await.unwrap();

    // The login itself succeeds, but the revoked invitation grants nothing
    // and is not resurrected as accepted.
    let response = login.await.unwrap();
    assert!(response.status().is_redirection(), "{}", response.status());
    assert_eq!(membership(&app, org.id, &address).await, None);
    assert_eq!(invitation_status(&app, invitation_id).await, None);

    // A pending invitation is still accepted exactly once by a login, and a
    // revoke that arrives afterwards finds it accepted. (The first account
    // now exists, and an existing user is added immediately when invited,
    // so this is a second, not yet registered, person.)
    let address = format!("second-{}@example.com", Uuid::new_v4().simple());
    present_github_identity(&github, &address, 660_002).await;
    let invitation_id = invite(&app, &owner, org.id, &address, "member").await;
    let response = github_callback(&app).await.send().await.unwrap();
    assert!(response.status().is_redirection(), "{}", response.status());
    assert_eq!(
        membership(&app, org.id, &address).await.as_deref(),
        Some("member")
    );
    assert_eq!(
        invitation_status(&app, invitation_id).await.as_deref(),
        Some("accepted")
    );
    let response = app
        .authenticated_client(&owner)
        .delete(&format!(
            "/api/v1/orgs/{}/invitations/{invitation_id}",
            org.id
        ))
        .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(
        membership(&app, org.id, &address).await.as_deref(),
        Some("member")
    );
    let memberships: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM org_memberships m JOIN users u ON u.id = m.user_id
         WHERE lower(u.email) = lower($1)",
    )
    .bind(&address)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(memberships, 1);

    app.cleanup().await;
}

// ===========================================================================
// Viewer ceiling
// ===========================================================================

async fn configure_org_default_scope(app: &TestApp, org_id: Uuid, scope: Scope) {
    sqlx::query(
        "INSERT INTO oauth_scopes (scope, description, category) VALUES ($1, $2, 'test')
         ON CONFLICT (scope) DO NOTHING",
    )
    .bind(scope.as_str())
    .bind(format!("Test scope for {scope}"))
    .execute(&app.db)
    .await
    .unwrap();
    sqlx::query("INSERT INTO org_default_scopes (org_id, scope) VALUES ($1, $2)")
        .bind(org_id)
        .bind(scope.as_str())
        .execute(&app.db)
        .await
        .unwrap();
}

async fn check_scopes(client: &AuthenticatedClient, scopes: &[&str]) -> Value {
    let response = client
        .post_json("/api/v1/auth/me/scopes/check", &json!({ "scopes": scopes }))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    body["data"].clone()
}

#[tokio::test]
async fn additive_grants_cannot_raise_a_viewer_above_read_only() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let members_path = format!("/api/v1/orgs/{}/members", org.id);

    // The populated fixture: a viewer whose token already carries org:admin
    // and a write scope, backed by an individual org:admin grant, an
    // individual write grant and an organization default.
    let viewer = TestUser::create_with_scopes(
        &app,
        org.id,
        &[
            Scope::WorkspacesRead,
            Scope::WorkspacesWrite,
            Scope::OrgAdmin,
        ],
    )
    .await;
    set_role(&app, org.id, viewer.id, "viewer").await;
    viewer.grant_org_scope(&app, org.id, Scope::OrgAdmin).await;
    viewer
        .grant_org_scope(&app, org.id, Scope::WorkspacesWrite)
        .await;
    configure_org_default_scope(&app, org.id, Scope::TasksExecute).await;
    let target = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let as_viewer = app.authenticated_client(&viewer);

    // Current authority: administration and writes are refused and nothing
    // is written, although the token and the grants say org:admin.
    let response = as_viewer
        .post_json(
            &members_path,
            &json!({ "email": "ceiling@example.com", "role": "member" }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let invitations: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM org_invitations WHERE org_id = $1")
            .bind(org.id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(invitations, 0);
    let response = as_viewer
        .patch_json(
            &format!("{members_path}/{}", target.id),
            &json!({ "role": "admin" }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let role: String =
        sqlx::query_scalar("SELECT role FROM org_memberships WHERE org_id = $1 AND user_id = $2")
            .bind(org.id)
            .bind(target.id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(role, "member");

    // The service boundary and the runtime predicates apply the same
    // ceiling: the locked authority lacks the administrative and write
    // scopes, the execute default confers nothing, and reads remain.
    let mut db = app.scoped_db_for_user_org(viewer.id, org.id).await;
    for scope in ["org:admin", "workspaces:write", "tasks:execute"] {
        let refused = g5_api::services::PermissionService::require_user_scopes_on_conn(
            db.conn(),
            viewer.id,
            org.id,
            None,
            &[scope],
        )
        .await
        .expect_err(scope);
        assert!(
            matches!(refused, g5_contracts::G5Error::Forbidden { .. }),
            "{scope}: {refused:?}"
        );
    }
    g5_api::services::PermissionService::require_user_scopes_on_conn(
        db.conn(),
        viewer.id,
        org.id,
        None,
        &["workspaces:read"],
    )
    .await
    .expect("a viewer keeps read authority");
    db.rollback().await.unwrap();
    let workspace = TestWorkspace::create(&app, &org).await;
    for scope in ["org:admin", "workspaces:write", "tasks:execute"] {
        assert!(
            !g5_api::services::PermissionService::user_holds_workspace_scope(
                &app.state.system_pool,
                viewer.id,
                org.id,
                workspace.id,
                scope,
            )
            .await
            .unwrap(),
            "{scope}"
        );
    }
    assert!(!g5_api::services::PermissionService::user_has_org_scope(
        &app.state.system_pool,
        viewer.id,
        org.id,
        "org:admin",
    )
    .await
    .unwrap());

    // Issued tokens resolve the same way: no administrative or write scope.
    let issued = app
        .state
        .auth_service
        .issue_user_tokens(viewer.id, Some(org.id), g5_auth::SESSION_CLIENT_BROWSER)
        .await
        .expect("issue a session for the viewer");
    let refreshed = app
        .state
        .auth_service
        .refresh_token(&issued.refresh_token, Some(org.id))
        .await
        .expect("refresh the viewer's session");
    for jwt in [&issued.jwt, &refreshed.jwt] {
        let claims = helpers::decode_test_jwt(jwt);
        let scopes: Vec<&str> = claims["scopes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s.as_str().unwrap())
            .collect();
        assert!(scopes.contains(&"workspaces:read"), "{scopes:?}");
        for refused in ["org:admin", "workspaces:write", "tasks:execute"] {
            assert!(!scopes.contains(&refused), "{refused} in {scopes:?}");
        }
    }
    let shown = check_scopes(&as_viewer, &["org:admin", "workspaces:write"]).await;
    assert_eq!(shown["allowed"], json!(false));
    let response = as_viewer.get("/api/v1/auth/me/scopes").await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    let effective = body["data"]["scopes"].as_array().unwrap();
    assert!(
        !effective
            .iter()
            .any(|s| s == "org:admin" || s == "workspaces:write"),
        "{effective:?}"
    );

    // Owners and administrators keep their authority, with or without grants.
    let as_owner = app.authenticated_client(&owner);
    let response = as_owner
        .patch_json(
            &format!("{members_path}/{}", target.id),
            &json!({ "role": "admin" }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let mut admin = TestUser::create_with_scopes(&app, org.id, &[Scope::OrgAdmin]).await;
    set_role(&app, org.id, admin.id, "admin").await;
    admin
        .grant_org_scope(&app, org.id, Scope::WorkersAdmin)
        .await;
    admin.jwt = helpers::test_jwt_minter()
        .mint(admin.id, org.id, vec![Scope::OrgAdmin], 3600)
        .unwrap();
    let response = app
        .authenticated_client(&admin)
        .post_json(
            &members_path,
            &json!({ "email": "welcome@example.com", "role": "viewer" }),
        )
        .await;
    assert!(response.status().is_success(), "{}", response.status());
    let issued = app
        .state
        .auth_service
        .issue_user_tokens(admin.id, Some(org.id), g5_auth::SESSION_CLIENT_BROWSER)
        .await
        .unwrap();
    let claims = helpers::decode_test_jwt(&issued.jwt);
    assert!(claims["scopes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s == "org:admin"));

    app.cleanup().await;
}

// ===========================================================================
// Refresh credentials are stored as digests
// ===========================================================================

#[tokio::test]
async fn refresh_credentials_are_stored_as_digests_on_every_path() {
    let address = format!("digest-{}@example.com", Uuid::new_v4().simple());
    let (app, _github) = app_with_github_login(&address).await;
    let user = TestUser::create(&app).await;
    // The OAuth login below is admitted by an invitation (g5.user.admission).
    crate::helpers::platform::invite_for_sign_in(&app, &address).await;

    // Issuance (the shared path of refresh and CLI exchange) stores only the
    // digest; the stored value is not a usable bearer.
    let issued = app
        .state
        .auth_service
        .issue_user_tokens(user.id, None, g5_auth::SESSION_CLIENT_BROWSER)
        .await
        .unwrap();
    let stored: Vec<String> =
        sqlx::query_scalar("SELECT token_hash FROM refresh_tokens WHERE user_id = $1")
            .bind(user.id)
            .fetch_all(&app.db)
            .await
            .unwrap();
    assert_eq!(
        stored,
        vec![helpers::refresh_token_hash(&issued.refresh_token)]
    );
    assert_ne!(stored[0], issued.refresh_token);
    let refresh = |token: String| {
        let base_url = app.base_url.clone();
        async move {
            reqwest::Client::new()
                .post(format!("{base_url}/api/v1/auth/refresh"))
                .json(&json!({ "refreshToken": token }))
                .send()
                .await
                .unwrap()
                .status()
        }
    };
    assert_eq!(refresh(stored[0].clone()).await, StatusCode::UNAUTHORIZED);
    assert_eq!(refresh(issued.refresh_token.clone()).await, StatusCode::OK);
    assert!(app
        .state
        .auth_service
        .resolve_refresh_identity(&stored[0], None)
        .await
        .is_err());
    assert_eq!(
        app.state
            .auth_service
            .resolve_refresh_identity(&issued.refresh_token, None)
            .await
            .unwrap()
            .user_id,
        user.id
    );

    // Logout revokes by digest.
    let response = app
        .authenticated_client(&user)
        .post_json(
            "/api/v1/auth/logout",
            &json!({ "refreshToken": issued.refresh_token }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let remaining: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM refresh_tokens WHERE user_id = $1")
            .bind(user.id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(remaining, 0);

    // OAuth login stores the digest of the credential it sets in the cookie.
    let response = github_callback(&app).await.send().await.unwrap();
    assert!(response.status().is_redirection(), "{}", response.status());
    let cookie = response
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find(|value| value.starts_with("__Host-g5_refresh="))
        .expect("the login sets the refresh cookie")
        .to_string();
    let plaintext = cookie
        .trim_start_matches("__Host-g5_refresh=")
        .split(';')
        .next()
        .unwrap()
        .to_string();
    let stored: Vec<String> = sqlx::query_scalar(
        "SELECT token_hash FROM refresh_tokens r JOIN users u ON u.id = r.user_id
         WHERE lower(u.email) = lower($1)",
    )
    .bind(&address)
    .fetch_all(&app.db)
    .await
    .unwrap();
    assert_eq!(stored, vec![helpers::refresh_token_hash(&plaintext)]);
    assert_eq!(refresh(plaintext).await, StatusCode::OK);

    app.cleanup().await;
}
