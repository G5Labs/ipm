//! Integration tests for G5 API authentication and SCM connection endpoints.
//!
//! Covers the complete request→route→middleware→service→database→response cycle
//! for all endpoints across two auth-related routers:
//!
//! 1. **Auth router** (`/api/v1/auth/*`) — OAuth login initiation, OAuth callback,
//!    JWT refresh, logout, and scope check.
//! 2. **SCM router** (`/api/v1/scm/*`) — SCM provider connection, callback,
//!    disconnection, repo listing, repo import, and org listing.
//!
//! Each test spawns a full `TestApp` instance with an isolated Postgres database,
//! makes real HTTP requests via `AuthenticatedClient` (or raw `reqwest::Client`
//! for unauthenticated endpoints), and asserts at the HTTP response level
//! (status codes, JSON body shapes, header values) and the database level.

use crate::helpers;
use g5_auth::{JwtValidator, Scope};

use helpers::{
    assert_json_error, test_config, TestApp, TestOrg, TestUser, TestWorkspace, TEST_PUBLIC_KEY_PEM,
};
use reqwest::StatusCode;
use serde_json::{json, Value};
use tokio::time::{sleep, Duration};
use uuid::Uuid;

async fn seed_github_scm_connection(app: &TestApp, user_id: Uuid, token: &str) {
    let encrypted = app
        .state
        .scm_token_manager
        .as_ref()
        .expect("test app should have scm token manager")
        .encrypt_token(token)
        .expect("failed to encrypt test scm token");

    sqlx::query(
        r#"
        INSERT INTO user_scm_connections (
            id, user_id, provider, provider_user_id, provider_username,
            access_token_enc, scopes
        )
        VALUES ($1, $2, 'github', $3, $4, $5, ARRAY['repo']::text[])
        ON CONFLICT (user_id, provider)
        DO UPDATE SET
            provider_user_id = EXCLUDED.provider_user_id,
            provider_username = EXCLUDED.provider_username,
            access_token_enc = EXCLUDED.access_token_enc,
            scopes = EXCLUDED.scopes,
            updated_at = now()
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(user_id)
    .bind("api-auth-test-user")
    .bind("api-auth-test-user")
    .bind(encrypted)
    .execute(&app.db)
    .await
    .expect("failed to seed SCM connection");
}

async fn seed_github_repo(
    app: &TestApp,
    org_id: Uuid,
    provider_repo_id: &str,
    full_name: &str,
) -> Uuid {
    let repo_id = Uuid::new_v4();
    let name = full_name
        .rsplit('/')
        .next()
        .unwrap_or(full_name)
        .to_string();
    let slug = full_name.replace('/', "-").to_lowercase();
    let git_url = format!("https://github.com/{full_name}.git");

    sqlx::query(
        r#"
        INSERT INTO repos (
            id, org_id, name, slug, git_url, default_branch,
            provider, provider_repo_id, provider_full_name
        )
        VALUES ($1, $2, $3, $4, $5, 'main', 'github', $6, $7)
        "#,
    )
    .bind(repo_id)
    .bind(org_id)
    .bind(&name)
    .bind(&slug)
    .bind(&git_url)
    .bind(provider_repo_id)
    .bind(full_name)
    .execute(&app.db)
    .await
    .expect("failed to seed imported repo");

    repo_id
}

/// The `Cookie` header value that returns a response's OAuth attempt cookie,
/// as the initiating browser would (SEC-M35).
fn oauth_attempt_cookie(response: &reqwest::Response) -> String {
    response
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find(|value| value.contains("g5_oauth_"))
        .and_then(|value| value.split(';').next())
        .expect("OAuth initiation should set an attempt cookie")
        .to_string()
}

fn query_param(url: &str, name: &str) -> String {
    reqwest::Url::parse(url)
        .expect("parse URL")
        .query_pairs()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.to_string())
        .unwrap_or_else(|| panic!("expected {name} in {url}"))
}

async fn github_oauth_callback_response(
    app: &TestApp,
    no_redirect_client: &reqwest::Client,
    code: &str,
) -> reqwest::Response {
    let login_response = no_redirect_client
        .get(format!("{}/api/v1/auth/login/github", app.base_url))
        .send()
        .await
        .expect("Login request failed");

    assert!(
        login_response.status().is_redirection(),
        "Expected login redirect, got {}",
        login_response.status()
    );

    let location = login_response
        .headers()
        .get("location")
        .expect("Expected Location header on redirect")
        .to_str()
        .expect("Location header should be valid UTF-8");
    let url = reqwest::Url::parse(location).expect("Parse redirect URL");
    let state = url
        .query_pairs()
        .find(|(k, _)| k == "state")
        .map(|(_, v)| v.to_string())
        .expect("Expected state parameter in redirect URL");
    let attempt = oauth_attempt_cookie(&login_response);

    no_redirect_client
        .get(format!(
            "{}/api/v1/auth/callback/github?code={}&state={}",
            app.base_url, code, state
        ))
        .header("cookie", attempt)
        .send()
        .await
        .expect("Callback request failed")
}

async fn complete_github_oauth_login(
    app: &TestApp,
    no_redirect_client: &reqwest::Client,
    code: &str,
) -> Value {
    let callback_response = github_oauth_callback_response(app, no_redirect_client, code).await;

    assert!(
        callback_response.status().is_redirection(),
        "Callback should redirect, got {}",
        callback_response.status()
    );

    let set_cookie = callback_response
        .headers()
        .get("set-cookie")
        .expect("Expected Set-Cookie header")
        .to_str()
        .expect("Set-Cookie should be valid UTF-8");
    let refresh_token = set_cookie
        .split(';')
        .next()
        .expect("Set-Cookie should contain a cookie pair")
        .strip_prefix("__Host-g5_refresh=")
        .expect("Cookie should be __Host-g5_refresh");

    let refresh_response = app
        .client
        .post(format!("{}/api/v1/auth/refresh", app.base_url))
        .header("Content-Type", "application/json")
        .body(json!({ "refreshToken": refresh_token }).to_string())
        .send()
        .await
        .expect("Refresh request failed");

    assert_eq!(
        refresh_response.status(),
        StatusCode::OK,
        "Refresh should return 200"
    );

    let refresh_body: Value = refresh_response
        .json()
        .await
        .expect("Parse refresh response");
    refresh_body["data"].clone()
}

// ===========================================================================
// Group 1: OAuth Login Initiation
// ===========================================================================

/// Verifies that `GET /api/v1/auth/login/github` returns a 303 redirect
/// to a GitHub OAuth authorize URL with correct query parameters.
///
/// This endpoint is **unauthenticated** — no `Authorization` header is sent.
/// The login handler returns an HTTP redirect so the browser follows
/// the OAuth flow automatically. We use a no-redirect client to inspect
/// the Location header.
#[tokio::test]
async fn test_login_github_returns_redirect_url() {
    let app = TestApp::spawn().await;

    // Use a no-redirect client so we can inspect the 303 response
    let no_redirect_client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();

    let response = no_redirect_client
        .get(format!("{}/api/v1/auth/login/github", app.base_url))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(
        response.status(),
        StatusCode::SEE_OTHER,
        "Login should return 303 redirect, got {}",
        response.status()
    );

    let location = response
        .headers()
        .get("location")
        .expect("Expected Location header on redirect")
        .to_str()
        .unwrap();

    assert!(
        location.contains("github.com"),
        "Redirect URL should point to GitHub, got: {}",
        location
    );
    assert!(
        location.contains("client_id="),
        "Redirect URL should contain client_id, got: {}",
        location
    );
    assert!(
        location.contains("redirect_uri="),
        "Redirect URL should contain redirect_uri, got: {}",
        location
    );
    assert!(
        location.contains("state="),
        "Redirect URL should contain state parameter, got: {}",
        location
    );

    app.cleanup().await;
}

/// Verifies that `GET /api/v1/auth/login/unknown_provider` returns 422
/// VALIDATION_ERROR for an unknown OAuth provider.
#[tokio::test]
async fn test_login_unknown_provider_returns_422() {
    let app = TestApp::spawn().await;

    let response = app
        .client
        .get(format!(
            "{}/api/v1/auth/login/unknown_provider",
            app.base_url
        ))
        .send()
        .await
        .expect("Failed to send request");

    assert_json_error(
        response,
        StatusCode::UNPROCESSABLE_ENTITY,
        "VALIDATION_ERROR",
    )
    .await;

    app.cleanup().await;
}

/// Providers without platform-login credentials (retired ones and an
/// unconfigured Microsoft Entra) cannot initiate login or finish even a
/// still-valid state. The fixture includes GitLab SCM credentials to catch
/// cross-domain exposure.
#[tokio::test]
async fn test_unconfigured_platform_login_providers_are_rejected() {
    let app = TestApp::spawn().await;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let response = client
        .get(format!("{}/api/v1/instance/info", app.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["data"]["authProviders"], json!(["github"]));

    for provider in ["google", "gitlab", "microsoft"] {
        let response = client
            .get(format!("{}/api/v1/auth/login/{provider}", app.base_url))
            .send()
            .await
            .unwrap();
        assert_json_error(
            response,
            StatusCode::UNPROCESSABLE_ENTITY,
            "VALIDATION_ERROR",
        )
        .await;

        let state = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO oauth_states (state, provider, expires_at) VALUES ($1, $2, now() + interval '5 minutes')")
            .bind(&state).bind(provider).execute(&app.db).await.unwrap();
        let response = client
            .get(format!(
                "{}/api/v1/auth/callback/{provider}?code=retired-code&state={state}",
                app.base_url
            ))
            .send()
            .await
            .unwrap();
        assert!(!response.headers().contains_key("set-cookie"));
        assert_json_error(response, StatusCode::UNAUTHORIZED, "UNAUTHORIZED").await;
    }
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM refresh_tokens")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(count, 0);
    app.cleanup().await;
}

/// The retained GitLab repository OAuth callback still stores encrypted tokens.
#[tokio::test]
async fn test_gitlab_scm_oauth_with_github_only_platform_login() {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/oauth/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "gitlab-repo-token",
            "refresh_token": "gitlab-refresh-token",
            "expires_in": 7200,
            "token_type": "bearer"
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v4/user"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 42, "username": "repo-user", "email": "repo-user@example.com", "name": "Repo User"
        })))
        .expect(1)
        .mount(&server)
        .await;
    let mut config = test_config();
    config.auth.scm_gitlab = Some(g5_api::config::OAuthProviderConfig {
        client_id: "scm-id".into(),
        client_secret: "scm-secret".into(),
        authorize_url: Some(format!("{}/oauth/authorize", server.uri())),
        token_url: Some(format!("{}/oauth/token", server.uri())),
        userinfo_url: Some(format!("{}/api/v4/user", server.uri())),
    });
    let app = TestApp::spawn_with_config(config).await;
    let user = TestUser::create(&app).await;
    let response = app
        .authenticated_client(&user)
        .get("/api/v1/scm/gitlab/connect")
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let attempt = oauth_attempt_cookie(&response);
    let body: Value = response.json().await.unwrap();
    let url = reqwest::Url::parse(body["data"]["url"].as_str().unwrap()).unwrap();
    let params: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
    assert_eq!(params["scope"], "read_repository write_repository read_api");
    assert!(params["redirect_uri"].ends_with("/api/v1/scm/gitlab/callback"));
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let response = client
        .get(format!(
            "{}/api/v1/scm/gitlab/callback?code=scm-code&state={}",
            app.base_url, params["state"]
        ))
        .header("cookie", &attempt)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert!(response.headers()["location"]
        .to_str()
        .unwrap()
        .contains("scm_connected=true"));
    let (provider_id, access_token, refresh_token): (String, Vec<u8>, Vec<u8>) = sqlx::query_as(
        "SELECT provider_user_id, access_token_enc, refresh_token_enc FROM user_scm_connections WHERE user_id = $1 AND provider = 'gitlab'"
    ).bind(user.id).fetch_one(&app.db).await.unwrap();
    assert_eq!(provider_id, "42");
    let manager = app.state.scm_token_manager.as_ref().unwrap();
    assert_eq!(
        manager.decrypt_token(&access_token).unwrap(),
        "gitlab-repo-token"
    );
    assert_eq!(
        manager.decrypt_token(&refresh_token).unwrap(),
        "gitlab-refresh-token"
    );
    // Replaying a consumed SCM state must fail without another provider request.
    let replay = client
        .get(format!(
            "{}/api/v1/scm/gitlab/callback?code=scm-code&state={}",
            app.base_url, params["state"]
        ))
        .header("cookie", &attempt)
        .send()
        .await
        .unwrap();
    assert!(replay.headers()["location"]
        .to_str()
        .unwrap()
        .contains("scm_error="));
    app.cleanup().await;
}

/// The S256 challenge of a PKCE verifier.
fn pkce_challenge(verifier: &str) -> String {
    use base64::Engine as _;
    use sha2::Digest as _;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(sha2::Sha256::digest(verifier))
}

/// The `code_verifier` a provider received in its one token request.
async fn token_request_verifier(server: &wiremock::MockServer, token_path: &str) -> String {
    let requests = server.received_requests().await.expect("request recording");
    let token_requests: Vec<_> = requests
        .iter()
        .filter(|request| request.url.path() == token_path)
        .collect();
    assert_eq!(token_requests.len(), 1, "exactly one code exchange");
    url::form_urlencoded::parse(&token_requests[0].body)
        .find(|(k, _)| k == "code_verifier")
        .map(|(_, v)| v.to_string())
        .expect("the code exchange carries a PKCE verifier")
}

fn set_cookies(response: &reqwest::Response) -> Vec<String> {
    response
        .headers()
        .get_all("set-cookie")
        .iter()
        .map(|value| value.to_str().unwrap().to_string())
        .collect()
}

fn assert_attempt_cookie_attributes(response: &reqwest::Response, name: &str) {
    let cookie = set_cookies(response)
        .into_iter()
        .find(|value| value.starts_with(&format!("{name}=")))
        .unwrap_or_else(|| panic!("expected a {name} cookie"));
    for attribute in [
        "HttpOnly",
        "Secure",
        "SameSite=Lax",
        "Path=/;",
        "Max-Age=600",
    ] {
        assert!(cookie.contains(attribute), "{cookie} lacks {attribute}");
    }
    assert!(!cookie.contains("Domain"), "{cookie} must be host-only");
}

/// SEC-M35 / issue #456: a login attempt is finished only by the browser
/// that started it. A callback URL replayed in another browser, with no
/// attempt cookie, another attempt's cookie or a tampered one, is refused
/// before the state is consumed or the code exchanged, and sets no cookie.
/// The initiating browser then completes, its exchange carries the PKCE
/// verifier for the authorize URL's challenge, and a replay fails.
#[tokio::test]
async fn test_oauth_login_callback_requires_initiating_browser() {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/login/oauth/access_token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "bound-login-token", "token_type": "bearer"
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/user"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 4560, "login": "bound-user", "name": "Bound User", "email": "bound@g5.dev"
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/user/emails"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"email": "bound@g5.dev", "primary": true, "verified": true}
        ])))
        .expect(1)
        .mount(&server)
        .await;
    let mut config = test_config();
    config.auth.github = Some(g5_api::config::OAuthProviderConfig {
        client_id: "test-github-client-id".to_string(),
        client_secret: "test-github-client-secret".to_string(),
        authorize_url: Some(format!("{}/login/oauth/authorize", server.uri())),
        token_url: Some(format!("{}/login/oauth/access_token", server.uri())),
        userinfo_url: Some(format!("{}/user", server.uri())),
    });
    let app = TestApp::spawn_with_config(config).await;
    // Sign-in admits only invitations and memberships (g5.user.admission).
    crate::helpers::platform::invite_for_sign_in(&app, "bound@g5.dev").await;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let login = || async {
        client
            .get(format!("{}/api/v1/auth/login/github", app.base_url))
            .send()
            .await
            .unwrap()
    };
    let callback = |state: String, cookie: Option<String>| {
        let mut request = client.get(format!(
            "{}/api/v1/auth/callback/github?code=bound-code&state={state}",
            app.base_url
        ));
        if let Some(cookie) = cookie {
            request = request.header("cookie", cookie);
        }
        async move { request.send().await.unwrap() }
    };

    let initiated = login().await;
    assert_attempt_cookie_attributes(&initiated, "__Host-g5_oauth_login");
    let location = initiated.headers()["location"]
        .to_str()
        .unwrap()
        .to_string();
    let state = query_param(&location, "state");
    let challenge = query_param(&location, "code_challenge");
    assert_eq!(query_param(&location, "code_challenge_method"), "S256");
    let cookie = oauth_attempt_cookie(&initiated);
    let other_attempt = oauth_attempt_cookie(&login().await);

    for (case, presented) in [
        ("another browser", None),
        ("another attempt's cookie", Some(other_attempt)),
        ("a tampered cookie", Some(format!("{cookie}A"))),
        ("duplicate cookies", Some(format!("{cookie}; {cookie}"))),
        (
            "empty duplicate",
            Some(format!("__Host-g5_oauth_login=; {cookie}")),
        ),
    ] {
        let response = callback(state.clone(), presented).await;
        assert!(set_cookies(&response).is_empty(), "{case}: no cookie");
        assert_json_error(response, StatusCode::UNAUTHORIZED, "UNAUTHORIZED").await;
    }
    let pending: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM oauth_states WHERE state = $1")
        .bind(&state)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(
        pending, 1,
        "a refused callback does not consume the attempt"
    );
    let sessions = || async {
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM refresh_tokens")
            .fetch_one(&app.db)
            .await
            .unwrap()
    };
    assert_eq!(sessions().await, 0);

    let response = callback(state.clone(), Some(cookie.clone())).await;
    assert!(response.status().is_redirection());
    let cookies = set_cookies(&response);
    assert!(cookies.iter().any(|c| c.starts_with("__Host-g5_refresh=")));
    assert!(cookies
        .iter()
        .any(|c| c.starts_with("__Host-g5_oauth_login=;") && c.contains("Max-Age=0")));
    assert_eq!(sessions().await, 1);
    assert_eq!(
        pkce_challenge(&token_request_verifier(&server, "/login/oauth/access_token").await),
        challenge
    );

    let replay = callback(state, Some(cookie)).await;
    assert!(set_cookies(&replay).is_empty());
    assert_json_error(replay, StatusCode::UNAUTHORIZED, "UNAUTHORIZED").await;
    assert_eq!(sessions().await, 1);
    app.cleanup().await;
}

/// SEC-M35 / issue #449: a GitLab authorize URL forwarded to another browser
/// cannot store that browser's GitLab account under the initiating user. With
/// no attempt cookie, another user's attempt cookie or a tampered one the
/// callback is refused before the exchange, stores nothing and returns to
/// the fixed settings page rather than the state's path. The initiating
/// browser then connects with a PKCE-verified exchange.
#[tokio::test]
async fn test_gitlab_scm_callback_requires_initiating_browser() {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/oauth/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "bound-repo-token", "expires_in": 7200, "token_type": "bearer"
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v4/user"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 449, "username": "bound-repo-user", "email": "r@example.com", "name": "R"
        })))
        .expect(1)
        .mount(&server)
        .await;
    let mut config = test_config();
    config.auth.scm_gitlab = Some(g5_api::config::OAuthProviderConfig {
        client_id: "scm-id".into(),
        client_secret: "scm-secret".into(),
        authorize_url: Some(format!("{}/oauth/authorize", server.uri())),
        token_url: Some(format!("{}/oauth/token", server.uri())),
        userinfo_url: Some(format!("{}/api/v4/user", server.uri())),
    });
    let app = TestApp::spawn_with_config(config).await;
    let initiator = TestUser::create(&app).await;
    let other = TestUser::create(&app).await;
    let connect = |user: &TestUser| {
        let client = app.authenticated_client(user);
        async move {
            client
                .get("/api/v1/scm/gitlab/connect?return_to=%2Facme%2Fbase%3Fsettings%3Dscm")
                .await
        }
    };
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let callback = |state: String, cookie: Option<String>| {
        let mut request = client.get(format!(
            "{}/api/v1/scm/gitlab/callback?code=scm-code&state={}",
            app.base_url,
            url::form_urlencoded::byte_serialize(state.as_bytes()).collect::<String>()
        ));
        if let Some(cookie) = cookie {
            request = request.header("cookie", cookie);
        }
        async move { request.send().await.unwrap() }
    };

    let initiated = connect(&initiator).await;
    assert_eq!(initiated.status(), StatusCode::OK);
    assert_attempt_cookie_attributes(&initiated, "__Host-g5_oauth_scm");
    let cookie = oauth_attempt_cookie(&initiated);
    let body: Value = initiated.json().await.unwrap();
    let authorize = body["data"]["url"].as_str().unwrap();
    assert!(body["data"].get("attemptCookie").is_none());
    let state = query_param(authorize, "state");
    let challenge = query_param(authorize, "code_challenge");
    assert_eq!(query_param(authorize, "code_challenge_method"), "S256");
    let other_attempt = oauth_attempt_cookie(&connect(&other).await);

    for (case, presented) in [
        ("another browser", None),
        ("another user's attempt cookie", Some(other_attempt)),
        ("a tampered cookie", Some(format!("{cookie}A"))),
        ("duplicate cookies", Some(format!("{cookie}; {cookie}"))),
        (
            "empty duplicate",
            Some(format!("__Host-g5_oauth_scm=; {cookie}")),
        ),
    ] {
        let response = callback(state.clone(), presented).await;
        assert_eq!(response.status(), StatusCode::SEE_OTHER, "{case}");
        assert_eq!(
            response.headers()["location"],
            "/settings?scm_error=oauth_state_invalid",
            "{case}"
        );
    }
    let stored = || async {
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM user_scm_connections WHERE provider = 'gitlab'",
        )
        .fetch_one(&app.db)
        .await
        .unwrap()
    };
    assert_eq!(stored().await, 0, "a refused callback stores no credential");
    let pending: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM oauth_states WHERE state = $1")
        .bind(&state)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(
        pending, 1,
        "a refused callback does not consume the attempt"
    );

    let response = callback(state.clone(), Some(cookie.clone())).await;
    assert_eq!(
        response.headers()["location"],
        "/acme/base?settings=scm&scm_connected=true"
    );
    assert!(set_cookies(&response)
        .iter()
        .any(|c| c.starts_with("__Host-g5_oauth_scm=;") && c.contains("Max-Age=0")));
    let owner: Uuid =
        sqlx::query_scalar("SELECT user_id FROM user_scm_connections WHERE provider = 'gitlab'")
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(owner, initiator.id);
    assert_eq!(
        pkce_challenge(&token_request_verifier(&server, "/oauth/token").await),
        challenge
    );

    // A replay from the initiating browser fails and returns to its own page.
    let replay = callback(state, Some(cookie)).await;
    assert_eq!(
        replay.headers()["location"],
        "/acme/base?settings=scm&scm_error=oauth_state_invalid"
    );
    assert_eq!(stored().await, 1);
    app.cleanup().await;
}

/// SEC-M36: the unauthenticated SCM callback's error redirect never leaves
/// the origin. A crafted state carrying `/\attacker.example/path` (which
/// browsers read as `//attacker.example/path`) goes to the fixed settings
/// page, for a configured or unknown provider alike.
#[tokio::test]
async fn test_scm_callback_error_redirect_stays_on_origin() {
    use base64::Engine as _;

    let app = TestApp::spawn().await;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let state = format!(
        "{}.{}",
        "0".repeat(64),
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode("/\\attacker.example/path")
    );
    for provider in ["x", "gitlab"] {
        let response = client
            .get(format!(
                "{}/api/v1/scm/{provider}/callback?code=1&state={state}",
                app.base_url
            ))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        assert_eq!(
            response.headers()["location"],
            "/settings?scm_error=oauth_state_invalid",
            "{provider}"
        );
    }
    app.cleanup().await;
}

/// Issue #459: refreshing slides a refresh token's idle expiry but never past
/// its absolute lifetime from issue (90 days in the test config). A token
/// issued 91 days ago is refused although its idle expiry is still ahead.
#[tokio::test]
async fn test_refresh_token_absolute_lifetime_cannot_slide() {
    let app = TestApp::spawn().await;
    let user = TestUser::create(&app).await;
    let a = &app;
    let issue = || async {
        a.state
            .auth_service
            .issue_user_tokens(user.id, None, g5_auth::SESSION_CLIENT_BROWSER)
            .await
            .expect("issue refresh token")
            .refresh_token
    };
    let refresh = |token: String| async move {
        a.client
            .post(format!("{}/api/v1/auth/refresh", a.base_url))
            .json(&json!({ "refreshToken": token }))
            .send()
            .await
            .unwrap()
            .status()
    };
    let expires_in_days = |token: String| async move {
        sqlx::query_scalar::<_, f64>(
            "SELECT (EXTRACT(EPOCH FROM expires_at - now()) / 86400)::float8 \
             FROM refresh_tokens WHERE token_hash = $1",
        )
        .bind(helpers::refresh_token_hash(&token))
        .fetch_one(&a.db)
        .await
        .unwrap()
    };
    let backdate = |token: String, age: &'static str| async move {
        sqlx::query(
            "UPDATE refresh_tokens SET created_at = now() - $2::interval, \
             expires_at = now() + interval '1 day' WHERE token_hash = $1",
        )
        .bind(helpers::refresh_token_hash(&token))
        .bind(age)
        .execute(&a.db)
        .await
        .unwrap();
    };

    let fresh = issue().await;
    assert_eq!(refresh(fresh.clone()).await, StatusCode::OK);
    let days = expires_in_days(fresh).await;
    assert!((29.9..=30.1).contains(&days), "idle window slides: {days}");

    let near_cap = issue().await;
    backdate(near_cap.clone(), "89 days 12 hours").await;
    for _ in 0..2 {
        assert_eq!(refresh(near_cap.clone()).await, StatusCode::OK);
        let days = expires_in_days(near_cap.clone()).await;
        assert!(days < 0.6, "renewal stops at the absolute lifetime: {days}");
    }

    let too_old = issue().await;
    backdate(too_old.clone(), "91 days").await;
    assert_eq!(refresh(too_old).await, StatusCode::UNAUTHORIZED);
    app.cleanup().await;
}

// ===========================================================================
// Group 2: OAuth Callback
// ===========================================================================

/// Verifies that `GET /api/v1/auth/callback/github?code=abc&state=invalid`
/// returns 401 when the state parameter doesn't match a stored value.
#[tokio::test]
async fn test_callback_with_invalid_state_returns_401() {
    let app = TestApp::spawn().await;

    let response = app
        .client
        .get(format!(
            "{}/api/v1/auth/callback/github?code=fake_code&state=invalid_state",
            app.base_url
        ))
        .send()
        .await
        .expect("Failed to send request");

    assert_json_error(response, StatusCode::UNAUTHORIZED, "UNAUTHORIZED").await;

    app.cleanup().await;
}

/// Verifies that `GET /api/v1/auth/callback/github?state=abc` (missing `code`
/// parameter) returns 422 or 400.
#[tokio::test]
async fn test_callback_missing_code_returns_422() {
    let app = TestApp::spawn().await;

    let response = app
        .client
        .get(format!(
            "{}/api/v1/auth/callback/github?state=some_state",
            app.base_url
        ))
        .send()
        .await
        .expect("Failed to send request");

    // Missing required query parameter should result in 422 or 400
    assert!(
        response.status() == StatusCode::UNPROCESSABLE_ENTITY
            || response.status() == StatusCode::BAD_REQUEST,
        "Expected 422 or 400 for missing code parameter, got {}",
        response.status()
    );

    app.cleanup().await;
}

// ===========================================================================
// Group 3: Token Refresh
// ===========================================================================

/// Verifies that `POST /api/v1/auth/refresh` without a JWT returns 401.
#[tokio::test]
async fn test_refresh_without_auth_returns_401() {
    let app = TestApp::spawn().await;

    let response = app
        .client
        .post(format!("{}/api/v1/auth/refresh", app.base_url))
        .header("Content-Type", "application/json")
        .json(&json!({
            "refreshToken": "some-refresh-token"
        }))
        .send()
        .await
        .expect("Failed to send request");

    assert_json_error(response, StatusCode::UNAUTHORIZED, "UNAUTHORIZED").await;

    app.cleanup().await;
}

/// Verifies that `POST /api/v1/auth/refresh` with a valid JWT but an invalid
/// refresh token in the body returns 401.
#[tokio::test]
async fn test_refresh_with_invalid_token_returns_401() {
    let app = TestApp::spawn().await;
    let user = TestUser::create(&app).await;
    let client = app.authenticated_client(&user);

    let response = client
        .post_json(
            "/api/v1/auth/refresh",
            &json!({
                "refreshToken": "nonexistent-refresh-token"
            }),
        )
        .await;

    assert_json_error(response, StatusCode::UNAUTHORIZED, "UNAUTHORIZED").await;

    app.cleanup().await;
}

/// G5Labs/G5-Lean#454 (`g5.security.login.refresh-exchange`): the ambient
/// `__Host-g5_refresh` cookie is exchanged only for trusted browser origins, and a
/// cookie-sourced exchange returns no refresh credential to script. Body-token
/// clients (the CLI) are unaffected and still receive their token back.
#[tokio::test]
async fn test_refresh_cookie_exchange_requires_trusted_browser_origin() {
    let app = TestApp::spawn().await;
    let user = TestUser::create(&app).await;
    let token = helpers::insert_refresh_token(&app, user.id).await;
    let cookie = format!("__Host-g5_refresh={token}");

    async fn exchange(app: &TestApp, cookie: &str, headers: &[(&str, &str)]) -> reqwest::Response {
        let mut request = app
            .client
            .post(format!("{}/api/v1/auth/refresh", app.base_url))
            .header("Cookie", cookie);
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        request.send().await.unwrap()
    }

    // Untrusted browser callers are refused before the cookie is consumed,
    // and no session cookie is renewed for them.
    let sibling_preview = "https://job.preview.example";
    for headers in [
        vec![
            ("Origin", "https://evil.example"),
            ("Sec-Fetch-Site", "cross-site"),
        ],
        vec![("Origin", "https://evil.example")],
        vec![("Origin", "null")],
        vec![("Origin", sibling_preview), ("Sec-Fetch-Site", "same-site")],
        vec![
            ("Origin", app.base_url.as_str()),
            ("Sec-Fetch-Site", "cross-site"),
        ],
        vec![("Sec-Fetch-Site", "cross-site")],
    ] {
        let response = exchange(&app, &cookie, &headers).await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN, "{headers:?}");
        assert!(
            response.headers().get("set-cookie").is_none(),
            "{headers:?}"
        );
        let text = response.text().await.unwrap();
        assert!(
            text.contains("untrusted browser origin"),
            "{headers:?}: {text}"
        );
    }

    // The platform origin, a CORS-allowed origin (the test config allows
    // http://localhost) and a client without browser headers all exchange
    // the cookie, and none of them receives the refresh credential back.
    for headers in [
        vec![
            ("Origin", app.base_url.as_str()),
            ("Sec-Fetch-Site", "same-origin"),
        ],
        vec![("Origin", app.base_url.as_str())],
        vec![
            ("Origin", "http://localhost"),
            ("Sec-Fetch-Site", "same-site"),
        ],
        vec![],
    ] {
        let response = exchange(&app, &cookie, &headers).await;
        assert_eq!(response.status(), StatusCode::OK, "{headers:?}");
        let renewed = response
            .headers()
            .get("set-cookie")
            .expect("cookie exchange renews __Host-g5_refresh")
            .to_str()
            .unwrap();
        assert!(renewed.starts_with("__Host-g5_refresh="), "{renewed}");
        helpers::assert_host_cookie(renewed);
        let body: Value = response.json().await.unwrap();
        assert!(body["data"]["jwt"].is_string(), "{headers:?}: {body}");
        assert!(
            body["data"].get("refreshToken").is_none(),
            "{headers:?}: cookie exchange must not return the refresh credential: {body}"
        );
    }

    // A body token is a credential the caller already holds: the exchange is
    // not origin-gated and the CLI still receives its token back.
    let response = app
        .client
        .post(format!("{}/api/v1/auth/refresh", app.base_url))
        .header("Origin", "https://evil.example")
        .json(&json!({ "refreshToken": token }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    assert!(body["data"]["jwt"].is_string(), "{body}");
    assert_eq!(body["data"]["refreshToken"], json!(token), "{body}");

    app.cleanup().await;
}

/// Valid tokens belonging to two people must never be selected by cookie order.
#[tokio::test]
async fn credential_cookie_ambiguity_cannot_renew_or_revoke_a_session() {
    for secure in [true, false] {
        let mut config = test_config();
        config.auth.cookie_secure = secure;
        let app = TestApp::spawn_with_config(config).await;
        let attacker = TestUser::create(&app).await;
        let victim = TestUser::create(&app).await;
        let attacker_token = helpers::insert_refresh_token(&app, attacker.id).await;
        let victim_token = helpers::insert_refresh_token(&app, victim.id).await;
        let name = if secure {
            "__Host-g5_refresh"
        } else {
            "g5_refresh"
        };
        let snapshot = || async {
            sqlx::query_scalar::<_, Value>(
                "SELECT jsonb_agg(to_jsonb(t) ORDER BY token_hash) FROM refresh_tokens t",
            )
            .fetch_one(&app.db)
            .await
            .unwrap()
        };
        let before = snapshot().await;
        let mut rejected = vec![
            vec![format!("{name}={attacker_token}; {name}={victim_token}")],
            vec![format!("{name}={victim_token}; {name}={attacker_token}")],
            vec![format!("{name}={victim_token}; {name}={victim_token}")],
            vec![format!("{name}=; {name}={victim_token}")],
            vec![format!("{name}={victim_token}; {name}=")],
            vec![
                format!("{name}={attacker_token}"),
                format!("{name}={victim_token}"),
            ],
        ];
        if secure {
            rejected.push(vec![format!("g5_refresh={attacker_token}")]);
        }
        for cookies in rejected {
            let mut request = app
                .client
                .post(format!("{}/api/v1/auth/refresh", app.base_url));
            for cookie in &cookies {
                request = request.header("Cookie", cookie);
            }
            let response = request.send().await.unwrap();
            assert_eq!(
                response.status(),
                StatusCode::UNAUTHORIZED,
                "{secure}: {cookies:?}"
            );
            assert!(!response.headers().contains_key("set-cookie"));
            assert_eq!(
                snapshot().await,
                before,
                "refused refresh must not renew either token"
            );

            let mut request = app
                .client
                .post(format!("{}/api/v1/auth/logout", app.base_url))
                .bearer_auth(&victim.jwt);
            for cookie in &cookies {
                request = request.header("Cookie", cookie);
            }
            let response = request.send().await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            assert!(response.headers()["set-cookie"]
                .to_str()
                .unwrap()
                .contains("Max-Age=0"));
            assert_eq!(
                snapshot().await,
                before,
                "logout must not select one ambiguous or legacy token"
            );
        }

        // A planted legacy cookie cannot block or replace the protected identity.
        let cookie = if secure {
            format!("g5_refresh={attacker_token}; {name}={victim_token}")
        } else {
            format!("other={attacker_token}; {name}={victim_token}")
        };
        let response = app
            .client
            .post(format!("{}/api/v1/auth/refresh", app.base_url))
            .header("Cookie", cookie)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: Value = response.json().await.unwrap();
        let claims = helpers::decode_test_jwt(body["data"]["jwt"].as_str().unwrap());
        assert_eq!(claims["sub"], victim.id.to_string());
        assert!(body["data"].get("refreshToken").is_none());

        // Explicit credentials remain independent of an ambiguous ambient jar.
        let response = app
            .client
            .post(format!("{}/api/v1/auth/refresh", app.base_url))
            .header(
                "Cookie",
                format!("{name}={attacker_token}; {name}={victim_token}"),
            )
            .json(&json!({"refreshToken": victim_token}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: Value = response.json().await.unwrap();
        assert_eq!(body["data"]["refreshToken"], victim_token);
        assert_eq!(
            helpers::decode_test_jwt(body["data"]["jwt"].as_str().unwrap())["sub"],
            victim.id.to_string()
        );
        app.cleanup().await;
    }
}

/// Unrelated app-cookie bytes cannot prevent the browser's session operations.
#[tokio::test]
async fn unrelated_cookie_bytes_do_not_block_session_refresh_or_logout() {
    let app = TestApp::spawn().await;
    let user = TestUser::create(&app).await;
    let token = helpers::insert_refresh_token(&app, user.id).await;
    let credential = format!("__Host-g5_refresh={token}");

    for other in [b"other=caf\xc3\xa9".as_slice(), b"other=\xff"] {
        for unrelated_first in [false, true] {
            for separate_headers in [false, true] {
                let values = if unrelated_first {
                    [other, credential.as_bytes()]
                } else {
                    [credential.as_bytes(), other]
                };
                let mut headers = reqwest::header::HeaderMap::new();
                if separate_headers {
                    for value in values {
                        headers.append(
                            "Cookie",
                            reqwest::header::HeaderValue::from_bytes(value).unwrap(),
                        );
                    }
                } else {
                    headers.append(
                        "Cookie",
                        reqwest::header::HeaderValue::from_bytes(&values.join(b"; ".as_slice()))
                            .unwrap(),
                    );
                }
                let response = app
                    .client
                    .post(format!("{}/api/v1/auth/refresh", app.base_url))
                    .header("Origin", &app.base_url)
                    .header("Sec-Fetch-Site", "same-origin")
                    .headers(headers)
                    .send()
                    .await
                    .unwrap();
                assert_eq!(response.status(), StatusCode::OK);
                let body: Value = response.json().await.unwrap();
                assert_eq!(
                    helpers::decode_test_jwt(body["data"]["jwt"].as_str().unwrap())["sub"],
                    user.id.to_string()
                );
                assert!(body["data"].get("refreshToken").is_none());
            }
        }
    }

    let response = app
        .client
        .post(format!("{}/api/v1/auth/logout", app.base_url))
        .bearer_auth(&user.jwt)
        .header("Cookie", format!("other=café; {credential}"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let replay = app
        .client
        .post(format!("{}/api/v1/auth/refresh", app.base_url))
        .json(&json!({"refreshToken": token}))
        .send()
        .await
        .unwrap();
    assert_eq!(replay.status(), StatusCode::UNAUTHORIZED);
    app.cleanup().await;
}

#[tokio::test]
async fn startup_rejects_refresh_cookie_names_that_overwrite_attempts() {
    use futures::FutureExt;
    use g5_api::auth::oauth_attempt::OAuthFlow;
    use g5_api::services::{github_app::pkce_cookie_name, AppState};

    let storage = tempfile::tempdir().unwrap();
    let db = sqlx::PgPool::connect_lazy("postgres://unused@127.0.0.1:1/unused").unwrap();
    for protected in [
        pkce_cookie_name(true),
        OAuthFlow::Login.cookie_name(true),
        OAuthFlow::ScmConnect.cookie_name(true),
    ] {
        let base = protected.strip_prefix("__Host-").unwrap();
        for (secure, configured) in [(true, base), (true, protected), (false, base)] {
            let mut config = test_config();
            config.auth.cookie_secure = secure;
            config.auth.refresh_cookie_name = configured.to_string();
            config.storage = g5_storage::StorageConfig::Local {
                base_dir: storage.path().join("repos"),
                worktree_dir: storage.path().join("worktrees"),
            };
            // Startup is synchronous: a lazy, unreachable pool proves this
            // check needs neither database access nor a serving HTTP listener.
            let result = std::panic::AssertUnwindSafe(AppState::new_for_test(db.clone(), &config))
                .catch_unwind()
                .await;
            if secure {
                let error = result
                    .err()
                    .expect("colliding secure names must refuse startup");
                let message = error
                    .downcast_ref::<String>()
                    .map(String::as_str)
                    .or_else(|| error.downcast_ref::<&str>().copied())
                    .unwrap_or("");
                assert!(
                    message.contains("G5_REFRESH_COOKIE_NAME collides"),
                    "{message}"
                );
            } else {
                assert!(result.is_ok(), "HTTP retains separate cookie paths");
            }
        }
    }
    db.close().await;
}

/// A G5 stack served through a G5 preview renames its refresh cookie, since
/// the preview relay never relays a `g5_` cookie
/// (`g5.workspace.preview-origin-security`). Every cookie path follows the
/// configured name: the exchange sets and reads it, the platform name is not
/// a credential, and logout expires the configured one.
#[tokio::test]
async fn test_refresh_cookie_follows_the_configured_name() {
    for (secure, configured, name) in [
        (true, "preview_refresh", "__Host-preview_refresh"),
        (true, "__Host-preview_refresh", "__Host-preview_refresh"),
        (false, "preview_refresh", "preview_refresh"),
    ] {
        let mut config = test_config();
        config.auth.cookie_secure = secure;
        config.auth.refresh_cookie_name = configured.to_string();
        let app = TestApp::spawn_with_config(config).await;
        let user = TestUser::create(&app).await;
        let token = helpers::insert_refresh_token(&app, user.id).await;
        let refresh = |cookie: String| {
            app.client
                .post(format!("{}/api/v1/auth/refresh", app.base_url))
                .header("Origin", app.base_url.as_str())
                .header("Sec-Fetch-Site", "same-origin")
                .header("Cookie", cookie)
                .send()
        };

        let response = refresh(format!("__Host-g5_refresh={token}")).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let response = refresh(format!("{name}={token}")).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let renewed = response
            .headers()
            .get("set-cookie")
            .expect("the exchange renews the configured cookie")
            .to_str()
            .unwrap()
            .to_string();
        assert!(
            renewed.starts_with(&format!("{name}={token};")),
            "{renewed}"
        );
        if secure {
            helpers::assert_host_cookie(&renewed);
        } else {
            assert!(!renewed.contains("Secure"));
            assert!(renewed.contains("Path=/api/v1/auth;"));
        }
        let cookies: Vec<_> = response
            .headers()
            .get_all("set-cookie")
            .iter()
            .map(|v| v.to_str().unwrap())
            .collect();
        assert_eq!(
            cookies.len(),
            if secure && configured != name { 2 } else { 1 }
        );
        if secure && configured != name {
            assert!(cookies[1].starts_with(&format!("{configured}=;")));
            assert!(cookies[1].contains("Path=/api/v1/auth;"));
            assert!(cookies[1].contains("Max-Age=0"));
            assert_eq!(
                refresh(format!("{configured}={token}"))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::UNAUTHORIZED
            );
        }
        let body: Value = response.json().await.unwrap();
        let jwt = body["data"]["jwt"]
            .as_str()
            .expect("access token")
            .to_string();

        let response = app
            .client
            .post(format!("{}/api/v1/auth/logout", app.base_url))
            .bearer_auth(&jwt)
            .header("Cookie", format!("{name}={token}"))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let cleared = response
            .headers()
            .get("set-cookie")
            .expect("logout expires the configured cookie")
            .to_str()
            .unwrap()
            .to_string();
        assert!(cleared.starts_with(&format!("{name}=;")), "{cleared}");
        assert!(cleared.contains("Max-Age=0"), "{cleared}");
        let response = refresh(format!("{name}={token}")).await.unwrap();
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "logout revoked it"
        );

        app.cleanup().await;
    }
}

// ===========================================================================
// Group 4: Logout
// ===========================================================================

/// Verifies that `POST /api/v1/auth/logout` without a JWT returns 401.
#[tokio::test]
async fn test_logout_without_auth_returns_401() {
    let app = TestApp::spawn().await;

    let response = app
        .client
        .post(format!("{}/api/v1/auth/logout", app.base_url))
        .header("Content-Type", "application/json")
        .json(&json!({
            "refreshToken": "some-token"
        }))
        .send()
        .await
        .expect("Failed to send request");

    assert_json_error(response, StatusCode::UNAUTHORIZED, "UNAUTHORIZED").await;

    app.cleanup().await;
}

/// Verifies that `POST /api/v1/auth/logout` returns 200 even when the refresh
/// token doesn't exist (idempotent behavior).
#[tokio::test]
async fn test_logout_is_idempotent() {
    let app = TestApp::spawn().await;
    let user = TestUser::create(&app).await;
    let client = app.authenticated_client(&user);

    let response = client
        .post_json(
            "/api/v1/auth/logout",
            &json!({
                "refreshToken": "nonexistent-token"
            }),
        )
        .await;

    assert_eq!(response.status(), StatusCode::OK);

    let body: Value = response.json().await.expect("Failed to parse JSON");
    assert!(
        body["data"].is_null(),
        "Logout response data should be null"
    );

    app.cleanup().await;
}

/// The browser sends no JSON body; CLI clients send their refresh token.
/// Both forms must revoke the credential and expire the browser cookie.
#[tokio::test]
async fn test_logout_revokes_refresh_credential_and_expires_cookie() {
    let app = TestApp::spawn().await;
    let user = TestUser::create(&app).await;
    for browser in [true, false] {
        let token = helpers::insert_refresh_token(&app, user.id).await;
        let cookie = format!("__Host-g5_refresh={token}");
        let before = app
            .client
            .post(format!("{}/api/v1/auth/refresh", app.base_url))
            .header("Cookie", &cookie)
            .send()
            .await
            .unwrap();
        assert_eq!(before.status(), StatusCode::OK);
        for _ in 0..2 {
            let mut request = app
                .client
                .post(format!("{}/api/v1/auth/logout", app.base_url))
                .bearer_auth(&user.jwt);
            request = if browser {
                request.header("Cookie", &cookie)
            } else {
                request.json(&json!({"refreshToken": token}))
            };
            let response = request.send().await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let cleared = response
                .headers()
                .get("set-cookie")
                .unwrap()
                .to_str()
                .unwrap();
            assert!(cleared.starts_with("__Host-g5_refresh=;"), "{cleared}");
            assert!(cleared.contains("Max-Age=0"), "{cleared}");
            assert!(cleared.contains("Path=/;"), "{cleared}");
            assert!(cleared.contains("HttpOnly"), "{cleared}");
            helpers::assert_host_cookie(cleared);
            assert!(
                response.headers().get_all("set-cookie").iter().any(|v| {
                    let v = v.to_str().unwrap();
                    v.starts_with("g5_refresh=;")
                        && v.contains("Path=/api/v1/auth;")
                        && v.contains("Max-Age=0")
                }),
                "logout must also expire the former host-only cookie"
            );
            let replay = app
                .client
                .post(format!("{}/api/v1/auth/refresh", app.base_url))
                .header("Cookie", &cookie)
                .send()
                .await
                .unwrap();
            assert_eq!(replay.status(), StatusCode::UNAUTHORIZED);
        }
    }
    app.cleanup().await;
}

#[tokio::test]
async fn test_logout_rejects_invalid_bodies_without_revoking_cookie() {
    let app = TestApp::spawn().await;
    let user = TestUser::create(&app).await;
    let token = helpers::insert_refresh_token(&app, user.id).await;
    let valid_body = json!({"refreshToken": token}).to_string();
    for (content_type, body, expected) in [
        (
            Some("application/json"),
            "{".to_string(),
            StatusCode::BAD_REQUEST,
        ),
        (
            Some("application/json"),
            r#"{"refreshToken": 123}"#.to_string(),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        (None, valid_body.clone(), StatusCode::UNSUPPORTED_MEDIA_TYPE),
        (
            Some("text/plain"),
            valid_body.clone(),
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
        ),
        (
            Some("application/x-www-form-urlencoded"),
            valid_body,
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
        ),
        (
            Some("application/json"),
            "x".repeat(2 * 1024 * 1024 + 1),
            StatusCode::PAYLOAD_TOO_LARGE,
        ),
    ] {
        let mut request = app
            .client
            .post(format!("{}/api/v1/auth/logout", app.base_url))
            .bearer_auth(&user.jwt)
            .header("Cookie", format!("__Host-g5_refresh={token}"))
            .body(body);
        if let Some(content_type) = content_type {
            request = request.header("Content-Type", content_type);
        }
        let response = request.send().await.unwrap();
        assert_eq!(response.status(), expected);
        assert!(!response.headers().contains_key("set-cookie"));
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM refresh_tokens WHERE token_hash = $1)",
        )
        .bind(helpers::refresh_token_hash(&token))
        .fetch_one(&app.db)
        .await
        .unwrap();
        assert!(exists);
    }
    app.cleanup().await;
}

// ===========================================================================
// Group 5: CLI Device Authorization
// ===========================================================================

#[tokio::test]
#[serial_test::serial]
async fn test_cli_device_start_with_workspace_scope_derives_authoritative_org() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let workspace = TestWorkspace::create(&app, &org).await;

    let response = app
        .client
        .post(format!("{}/api/v1/auth/cli/start", app.base_url))
        .json(&json!({
            "workspaceId": workspace.id.to_string(),
            "deviceName": "worker-workspace-scope"
        }))
        .send()
        .await
        .expect("CLI start request failed");

    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("parse start body");
    assert!(
        body["data"]["deviceCode"].as_str().is_some(),
        "start response should include a device code: {body}"
    );
    assert!(
        body["data"]["userCode"].as_str().is_some(),
        "start response should include a user code: {body}"
    );

    let row: (Uuid, Uuid, Option<String>) = sqlx::query_as(
        r#"
        SELECT org_id, workspace_id, device_name
        FROM cli_device_authorizations
        WHERE workspace_id = $1
        ORDER BY created_at DESC
        LIMIT 1
        "#,
    )
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .expect("fetch created CLI device authorization");

    assert_eq!(row.0, org.id);
    assert_eq!(row.1, workspace.id);
    assert_eq!(row.2.as_deref(), Some("worker-workspace-scope"));

    app.cleanup().await;
}

#[tokio::test]
#[serial_test::serial]
async fn test_cli_device_start_rejects_tombstoned_workspace() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let workspace = TestWorkspace::create(&app, &org).await;

    sqlx::query("UPDATE workspaces SET deleted_at = now() WHERE id = $1")
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .expect("tombstone workspace");

    let response = app
        .client
        .post(format!("{}/api/v1/auth/cli/start", app.base_url))
        .json(&json!({
            "workspaceId": workspace.id.to_string(),
            "deviceName": "removed-workspace"
        }))
        .send()
        .await
        .expect("CLI start request failed");

    assert_json_error(response, StatusCode::NOT_FOUND, "NOT_FOUND").await;

    app.cleanup().await;
}

/// Verifies the production CLI device authorization flow end to end:
/// an unauthenticated CLI starts/polls, an authenticated browser approves, and
/// the server returns refresh-capable tokens exactly once after approval.
#[tokio::test]
#[serial_test::serial]
async fn test_cli_device_authorization_happy_path_issues_tokens_once() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let browser_client = app.authenticated_client(&user);

    let start_response = app
        .client
        .post(format!("{}/api/v1/auth/cli/start", app.base_url))
        .json(&json!({
            "workspaceId": workspace.id.to_string(),
            "orgId": org.id.to_string(),
            "deviceName": "integration-test-cli"
        }))
        .send()
        .await
        .expect("CLI start request failed");
    assert_eq!(start_response.status(), StatusCode::OK);
    let start_body: Value = start_response.json().await.expect("parse start body");
    let start = &start_body["data"];
    let device_code = start["deviceCode"].as_str().expect("device code");
    let user_code = start["userCode"].as_str().expect("user code");
    assert!(!device_code.is_empty());
    assert!(user_code.contains('-'));
    assert!(start["verificationUriComplete"]
        .as_str()
        .expect("verification URI")
        .contains(user_code));

    let pending_response = app
        .client
        .post(format!("{}/api/v1/auth/cli/token", app.base_url))
        .json(&json!({ "deviceCode": device_code }))
        .send()
        .await
        .expect("CLI pending poll failed");
    assert_eq!(pending_response.status(), StatusCode::OK);
    let pending_body: Value = pending_response.json().await.expect("parse pending body");
    assert_eq!(pending_body["data"]["status"], "pending");
    assert!(pending_body["data"]["refreshToken"].is_null());

    let approve_response = browser_client
        .post_json(
            "/api/v1/auth/cli/approve",
            &json!({
                "userCode": user_code,
                "approve": true
            }),
        )
        .await;
    assert_eq!(approve_response.status(), StatusCode::OK);
    let approve_body: Value = approve_response.json().await.expect("parse approve body");
    assert_eq!(approve_body["data"]["status"], "approved");

    let token_response = app
        .client
        .post(format!("{}/api/v1/auth/cli/token", app.base_url))
        .json(&json!({ "deviceCode": device_code }))
        .send()
        .await
        .expect("CLI approved poll failed");
    assert_eq!(token_response.status(), StatusCode::OK);
    let token_body: Value = token_response.json().await.expect("parse token body");
    let token_data = &token_body["data"];
    assert_eq!(token_data["status"], "approved");
    assert!(token_data["jwt"]
        .as_str()
        .is_some_and(|jwt| !jwt.is_empty()));
    let refresh_token = token_data["refreshToken"]
        .as_str()
        .expect("approved poll should return refresh token");
    assert_eq!(token_data["userId"], user.id.to_string());

    let refresh_response = app
        .client
        .post(format!("{}/api/v1/auth/refresh", app.base_url))
        .json(&json!({ "refreshToken": refresh_token }))
        .send()
        .await
        .expect("refresh with CLI token failed");
    assert_eq!(refresh_response.status(), StatusCode::OK);
    let refresh_body: Value = refresh_response.json().await.expect("parse refresh body");
    assert!(refresh_body["data"]["jwt"].as_str().is_some());

    let second_poll_response = app
        .client
        .post(format!("{}/api/v1/auth/cli/token", app.base_url))
        .json(&json!({ "deviceCode": device_code }))
        .send()
        .await
        .expect("CLI second poll failed");
    assert_eq!(second_poll_response.status(), StatusCode::OK);
    let second_poll_body: Value = second_poll_response
        .json()
        .await
        .expect("parse second poll body");
    assert_eq!(second_poll_body["data"]["status"], "consumed");
    assert!(second_poll_body["data"]["jwt"].is_null());
    assert!(second_poll_body["data"]["refreshToken"].is_null());

    app.cleanup().await;
}

/// Verifies approval does not extend the device authorization expiry window:
/// a request approved before expiry cannot mint tokens if the CLI polls after
/// expiry.
#[tokio::test]
#[serial_test::serial]
async fn test_cli_device_authorization_approved_request_cannot_exchange_after_expiry() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let browser_client = app.authenticated_client(&user);

    let start_response = app
        .client
        .post(format!("{}/api/v1/auth/cli/start", app.base_url))
        .json(&json!({
            "workspaceId": workspace.id.to_string(),
            "orgId": org.id.to_string(),
            "deviceName": "integration-test-cli-expiry"
        }))
        .send()
        .await
        .expect("CLI start request failed");
    assert_eq!(start_response.status(), StatusCode::OK);
    let start_body: Value = start_response.json().await.expect("parse start body");
    let device_code = start_body["data"]["deviceCode"].as_str().unwrap();
    let user_code = start_body["data"]["userCode"].as_str().unwrap();

    let approve_response = browser_client
        .post_json(
            "/api/v1/auth/cli/approve",
            &json!({
                "userCode": user_code,
                "approve": true
            }),
        )
        .await;
    assert_eq!(approve_response.status(), StatusCode::OK);
    let approve_body: Value = approve_response.json().await.expect("parse approve body");
    assert_eq!(approve_body["data"]["status"], "approved");

    let update_result = sqlx::query(
        r#"
        UPDATE cli_device_authorizations
        SET expires_at = now() - interval '1 second'
        WHERE workspace_id = $1
        "#,
    )
    .bind(workspace.id)
    .execute(&app.db)
    .await
    .expect("force CLI device authorization expiry");
    assert_eq!(update_result.rows_affected(), 1);

    let token_response = app
        .client
        .post(format!("{}/api/v1/auth/cli/token", app.base_url))
        .json(&json!({ "deviceCode": device_code }))
        .send()
        .await
        .expect("CLI expired approved poll failed");
    assert_eq!(token_response.status(), StatusCode::OK);
    let token_body: Value = token_response.json().await.expect("parse token body");
    assert_eq!(token_body["data"]["status"], "expired");
    assert!(token_body["data"]["jwt"].is_null());
    assert!(token_body["data"]["refreshToken"].is_null());

    let refresh_token_count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM refresh_tokens
        WHERE user_id = $1
        "#,
    )
    .bind(user.id)
    .fetch_one(&app.db)
    .await
    .expect("count refresh tokens after expired CLI poll");
    assert_eq!(refresh_token_count, 0);

    app.cleanup().await;
}

/// Verifies browser approval cannot be performed without an authenticated
/// user JWT.
#[tokio::test]
#[serial_test::serial]
async fn test_cli_device_approve_requires_browser_authentication() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;

    let start_response = app
        .client
        .post(format!("{}/api/v1/auth/cli/start", app.base_url))
        .json(&json!({
            "workspaceId": workspace.id.to_string(),
            "orgId": org.id.to_string()
        }))
        .send()
        .await
        .expect("CLI start request failed");
    assert_eq!(start_response.status(), StatusCode::OK);
    let start_body: Value = start_response.json().await.expect("parse start body");
    let user_code = start_body["data"]["userCode"].as_str().unwrap();

    let approve_response = app
        .client
        .post(format!("{}/api/v1/auth/cli/approve", app.base_url))
        .json(&json!({ "userCode": user_code }))
        .send()
        .await
        .expect("unauthenticated approve request failed");
    assert_json_error(approve_response, StatusCode::UNAUTHORIZED, "UNAUTHORIZED").await;

    app.cleanup().await;
}

/// Creates a `g5k_` API key in `org_id` for `user` through the public route
/// and returns its plaintext.
async fn create_api_key(app: &TestApp, user: &TestUser, org_id: Uuid, scopes: &[&str]) -> String {
    let response = app
        .authenticated_client(user)
        .post_json(
            &format!("/api/v1/orgs/{org_id}/api-keys"),
            &json!({ "name": "delegated key", "scopes": scopes }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("parse api key body");
    let key = body["data"]["key"].as_str().expect("plaintext key");
    assert!(key.starts_with("g5k_"));
    key.to_string()
}

/// Account-wide authority cannot be delegated by an organization API key,
/// including a key with every user scope (#539). Exercise valid mutations so
/// authorization, rather than validation or a provider failure, rejects them.
#[tokio::test]
async fn test_account_routes_refuse_api_keys_without_side_effects() {
    use reqwest::Method;
    use wiremock::MockServer;

    let provider = MockServer::start().await;
    let mut config = test_config();
    config.auth.scm_github_api_base_url = provider.uri();
    let app = TestApp::spawn_with_config(config).await;
    let mut user = TestUser::create(&app).await;
    let org_a = TestOrg::create_with_admin(&app, &mut user).await;
    let org_b = TestOrg::create_with_admin(&app, &mut user).await;
    seed_github_scm_connection(&app, user.id, "original-account-token").await;
    let all_scopes: Vec<&str> = Scope::all()
        .iter()
        .filter(|scope| **scope != Scope::WorkerHost)
        .map(|scope| scope.as_str())
        .collect();
    let keys = [
        (
            "zero-scope",
            create_api_key(&app, &user, org_a.id, &[]).await,
        ),
        (
            "full-scope",
            create_api_key(&app, &user, org_a.id, &all_scopes).await,
        ),
    ];
    let invitation_ids = [Uuid::new_v4(), Uuid::new_v4()];
    for invitation in invitation_ids {
        let invitation_org = TestOrg::create(&app).await;
        sqlx::query("INSERT INTO org_invitations (id, org_id, email, role, invited_by, status, expires_at) VALUES ($1, $2, $3, 'member', $4, 'pending', now() + interval '7 days')")
            .bind(invitation).bind(invitation_org.id).bind(&user.email).bind(user.id)
            .execute(&app.db).await.unwrap();
    }
    let snapshot_query = "SELECT jsonb_build_object(
        'profile', (SELECT to_jsonb(u) FROM users u WHERE id = $1),
        'scm', (SELECT jsonb_agg(to_jsonb(s) ORDER BY id) FROM user_scm_connections s WHERE user_id = $1),
        'orgs', (SELECT count(*) FROM orgs),
        'memberships', (SELECT jsonb_agg(to_jsonb(m) ORDER BY org_id) FROM org_memberships m WHERE user_id = $1),
        'invitations', (SELECT jsonb_agg(to_jsonb(i) ORDER BY id) FROM org_invitations i),
        'oauthStates', (SELECT count(*) FROM oauth_states),
        'repos', (SELECT count(*) FROM repos))";
    let before: Value = sqlx::query_scalar(snapshot_query)
        .bind(user.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    let mut routes = vec![
        (Method::GET, "/api/v1/users/me".to_string(), json!({}), None),
        (
            Method::PATCH,
            "/api/v1/users/me".into(),
            json!({"displayName":"key changed profile", "avatarUrl":"https://example.com/key.png"}),
            None,
        ),
        (Method::GET, "/api/v1/orgs".into(), json!({}), None),
        (Method::GET, "/api/v1/platform/orgs".into(), json!({}), None),
        (
            Method::POST,
            "/api/v1/platform/orgs".into(),
            json!({"name":"Key organization", "slug":"key-created-org", "adminEmail":"key-owner@g5.test"}),
            None,
        ),
        (
            Method::GET,
            "/api/v1/scm/gitlab/connect".into(),
            json!({}),
            None,
        ),
        (
            Method::POST,
            "/api/v1/scm/github/connect".into(),
            json!({"token":"github_pat_11ABCDEFG_replacement"}),
            None,
        ),
        (
            Method::DELETE,
            "/api/v1/scm/github/disconnect".into(),
            json!({}),
            None,
        ),
        (
            Method::GET,
            "/api/v1/scm/github/orgs".into(),
            json!({}),
            None,
        ),
        (
            Method::POST,
            format!("/api/v1/invitations/{}/accept", invitation_ids[0]),
            json!({}),
            None,
        ),
        (
            Method::POST,
            format!("/api/v1/invitations/{}/decline", invitation_ids[1]),
            json!({}),
            None,
        ),
    ];
    // Both the key's organization and another organization of its creator.
    for org_id in [org_a.id, org_b.id] {
        routes.push((
            Method::GET,
            format!("/api/v1/scm/github/repos?orgId={org_id}"),
            json!({}),
            Some("workspaces:read"),
        ));
        routes.push((
            Method::POST,
            "/api/v1/scm/github/repos/import".into(),
            scm_import_body(org_id, "key-repo"),
            Some("workspaces:write"),
        ));
    }
    for (label, key) in &keys {
        for (method, path, body, required_scope) in &routes {
            let response = app
                .client
                .request(method.clone(), format!("{}{path}", app.base_url))
                .bearer_auth(key)
                .json(body)
                .send()
                .await
                .unwrap();
            let status = response.status();
            let error: Value = response.json().await.unwrap();
            assert_eq!(
                status,
                StatusCode::FORBIDDEN,
                "{label} {method} {path}: {error}"
            );
            assert_eq!(error["error"]["code"], "FORBIDDEN");
            // Audited routes reject an empty scope set before the handler;
            // full-scope keys still fail the person-session boundary.
            let missing = if *label == "zero-scope" {
                required_scope.unwrap_or("user_session_required")
            } else {
                "user_session_required"
            };
            assert_eq!(
                error["error"]["details"]["missing_scopes"],
                json!([missing]),
                "{label} {method} {path}"
            );
            let after: Value = sqlx::query_scalar(snapshot_query)
                .bind(user.id)
                .fetch_one(&app.db)
                .await
                .unwrap();
            assert_eq!(
                after, before,
                "{label} {method} {path} must leave account state unchanged"
            );
        }
    }
    assert!(
        provider.received_requests().await.unwrap().is_empty(),
        "keys must not use the person's provider credential"
    );

    // The person's session retains cross-org profile access and account control.
    let client = app.authenticated_client(&user);
    let response = client.get("/api/v1/users/me").await;
    assert_eq!(response.status(), StatusCode::OK);
    let profile: Value = response.json().await.unwrap();
    assert_eq!(profile["data"]["orgs"].as_array().unwrap().len(), 2);
    assert_eq!(
        profile["data"]["scmConnections"].as_array().unwrap().len(),
        1
    );
    assert_eq!(client.get("/api/v1/orgs").await.status(), StatusCode::OK);
    assert_eq!(
        client
            .patch_json(
                "/api/v1/users/me",
                &json!({"displayName":"Person updated profile"})
            )
            .await
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        client.get("/api/v1/scm/gitlab/connect").await.status(),
        StatusCode::OK
    );
    assert_eq!(
        client
            .delete("/api/v1/scm/github/disconnect")
            .await
            .status(),
        StatusCode::OK
    );
    // The person can decline one invitation and accept another.
    for (invitation, action, expected) in [
        (invitation_ids[1], "decline", StatusCode::NO_CONTENT),
        (invitation_ids[0], "accept", StatusCode::OK),
    ] {
        assert_eq!(
            client
                .post_json(
                    &format!("/api/v1/invitations/{invitation}/{action}"),
                    &json!({})
                )
                .await
                .status(),
            expected
        );
    }
    // An organization key still works on its intended organization-scoped API.
    let response = app
        .client
        .get(format!("{}/api/v1/orgs/{}/repos", app.base_url, org_a.id))
        .bearer_auth(&keys[1].1)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    app.cleanup().await;
}

fn scm_import_body(org_id: Uuid, name: &str) -> Value {
    json!({"orgId": org_id, "repos": [{
        "providerRepoId": name,
        "fullName": format!("acme/{name}"),
        "cloneUrl": format!("https://github.com/acme/{name}.git"),
        "defaultBranch": "main"
    }]})
}

/// Import must satisfy both the presented scope ceiling and current authority
/// in the body-selected organization, with that authority held through commit.
#[tokio::test]
async fn test_scm_import_requires_current_target_org_write_authority() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org_a = TestOrg::create_with_admin(&app, &mut owner).await;
    let org_b = TestOrg::create_with_admin(&app, &mut owner).await;
    let member = TestUser::create_with_scopes(&app, org_b.id, &[Scope::WorkspacesWrite]).await;
    let viewer = TestUser::create_with_scopes(&app, org_b.id, &[Scope::WorkspacesWrite]).await;
    sqlx::query("UPDATE org_memberships SET role = 'viewer' WHERE org_id = $1 AND user_id = $2")
        .bind(org_b.id)
        .bind(viewer.id)
        .execute(&app.db)
        .await
        .unwrap();
    // Deliberately retain the pre-demotion write token to exercise the DB check.
    let read_only_token = helpers::test_jwt_minter()
        .mint(owner.id, org_b.id, vec![Scope::WorkspacesRead], 3600)
        .unwrap();
    let no_membership = TestOrg::create(&app).await;
    let mut foreign_owner = TestUser::create(&app).await;
    TestOrg::create_with_admin(&app, &mut foreign_owner).await;
    sqlx::query("INSERT INTO org_memberships (org_id, user_id, role) VALUES ($1, $2, 'viewer')")
        .bind(org_b.id)
        .bind(foreign_owner.id)
        .execute(&app.db)
        .await
        .unwrap();
    let cases = [
        (
            "owner elsewhere, viewer in target",
            foreign_owner.jwt.clone(),
            org_b.id,
        ),
        (
            "viewer with stale write token",
            viewer.jwt.clone(),
            org_b.id,
        ),
        ("owner with read-only token", read_only_token, org_b.id),
        (
            "member without target membership",
            member.jwt.clone(),
            no_membership.id,
        ),
        (
            "zero-scope key",
            create_api_key(&app, &owner, org_b.id, &[]).await,
            org_b.id,
        ),
        (
            "same-org write key",
            create_api_key(&app, &owner, org_b.id, &["workspaces:write"]).await,
            org_b.id,
        ),
        (
            "cross-org write key",
            create_api_key(&app, &owner, org_a.id, &["workspaces:write"]).await,
            org_b.id,
        ),
    ];
    for (label, token, target) in cases {
        let response = app
            .client
            .post(format!("{}/api/v1/scm/github/repos/import", app.base_url))
            .bearer_auth(token)
            .json(&scm_import_body(target, "refused-repo"))
            .send()
            .await
            .unwrap();
        let status = response.status();
        let error: Value = response.json().await.unwrap();
        assert_eq!(status, StatusCode::FORBIDDEN, "{label}: {error}");
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM repos WHERE provider_repo_id = 'refused-repo'",
        )
        .fetch_one(&app.db)
        .await
        .unwrap();
        assert_eq!(count, 0, "{label}");
    }
    // Existing member write permission remains sufficient; a new admin-only
    // policy is not introduced. Owner sessions may select another owned org.
    for (user, target, name) in [
        (&member, org_b.id, "member-repo"),
        (&owner, org_a.id, "owner-repo"),
    ] {
        let response = app
            .authenticated_client(user)
            .post_json(
                "/api/v1/scm/github/repos/import",
                &scm_import_body(target, name),
            )
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        let body: Value = response.json().await.unwrap();
        assert_eq!(body["data"]["imported"][0]["orgId"], target.to_string());
        let stored_org: Uuid =
            sqlx::query_scalar("SELECT org_id FROM repos WHERE provider_repo_id = $1")
                .bind(name)
                .fetch_one(&app.db)
                .await
                .unwrap();
        assert_eq!(stored_org, target);
    }
    // Service callers cannot turn a capability label into write permission.
    let capability = app
        .state
        .authz
        .mint_db_capability(
            g5_contracts::types::ids::UserId::from_uuid(viewer.id),
            &[g5_contracts::types::ids::OrgId::from_uuid(org_b.id)],
            &["workspaces:write".to_string()],
        )
        .await
        .unwrap();
    let mut db = g5_authz::ScopedDb::from_capability(&app.state.tenant_pool, capability)
        .await
        .unwrap();
    let error = app
        .state
        .auth_service
        .import_repos(&mut db, "github", org_b.id, vec![])
        .await
        .unwrap_err();
    assert!(matches!(
        error.inner,
        g5_contracts::error::G5Error::Forbidden { .. }
    ));
    db.rollback().await.unwrap();
    app.cleanup().await;
}

#[tokio::test]
async fn test_scm_repo_list_rechecks_current_target_read_authority() {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};
    let provider = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/user/repos"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .expect(1)
        .mount(&provider)
        .await;
    let mut config = test_config();
    config.auth.scm_github_api_base_url = provider.uri();
    let app = TestApp::spawn_with_config(config).await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    TestUser::create_admin(&app, org.id).await;
    seed_github_scm_connection(&app, user.id, "read-token").await;
    // Even a valid session with read in its token loses access after removal.
    sqlx::query("DELETE FROM org_memberships WHERE org_id = $1 AND user_id = $2")
        .bind(org.id)
        .bind(user.id)
        .execute(&app.db)
        .await
        .unwrap();
    let route = format!("/api/v1/scm/github/repos?orgId={}", org.id);
    assert_json_error(
        app.authenticated_client(&user).get(&route).await,
        StatusCode::FORBIDDEN,
        "FORBIDDEN",
    )
    .await;
    assert!(provider.received_requests().await.unwrap().is_empty());
    // A read-only viewer session is an authorized account-repo reader.
    sqlx::query("INSERT INTO org_memberships (org_id, user_id, role) VALUES ($1, $2, 'viewer')")
        .bind(org.id)
        .bind(user.id)
        .execute(&app.db)
        .await
        .unwrap();
    let token = helpers::test_jwt_minter()
        .mint(user.id, org.id, vec![Scope::WorkspacesRead], 3600)
        .unwrap();
    let response = app
        .client
        .get(format!("{}{route}", app.base_url))
        .bearer_auth(token)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    app.cleanup().await;
}

#[tokio::test]
async fn test_scm_import_waits_for_in_flight_role_revocation() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesWrite]).await;
    let mut demotion = app.db.begin().await.unwrap();
    let demotion_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *demotion)
        .await
        .unwrap();
    sqlx::query("UPDATE org_memberships SET role = 'viewer' WHERE org_id = $1 AND user_id = $2")
        .bind(org.id)
        .bind(user.id)
        .execute(&mut *demotion)
        .await
        .unwrap();
    let client = app.authenticated_client(&user);
    let body = scm_import_body(org.id, "revoked-repo");
    let request = client.post_json("/api/v1/scm/github/repos/import", &body);
    tokio::pin!(request);
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            tokio::select! {
                response = &mut request => panic!("import escaped the authority lock: {}", response.status()),
                _ = sleep(Duration::from_millis(20)) => {}
            }
            let waiting: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM pg_stat_activity WHERE $1 = ANY(pg_blocking_pids(pid)))",
            ).bind(demotion_pid).fetch_one(&app.db).await.unwrap();
            if waiting { break; }
        }
    }).await.expect("import must wait for the role revocation");
    demotion.commit().await.unwrap();
    assert_json_error(request.await, StatusCode::FORBIDDEN, "FORBIDDEN").await;
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM repos WHERE org_id = $1")
        .bind(org.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(count, 0);
    app.cleanup().await;
}

/// Provider I/O must neither hold organization authority locked nor allow a
/// membership removed during that I/O to read the organization's import flags.
#[tokio::test]
async fn test_scm_repo_list_rechecks_membership_after_provider_io() {
    use axum::{routing::get, Json, Router};
    use std::sync::Arc;
    use tokio::sync::Notify;

    let requested = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let provider_app = Router::new().route(
        "/user/repos",
        get({
            let requested = requested.clone();
            let release = release.clone();
            move || {
                let requested = requested.clone();
                let release = release.clone();
                async move {
                    requested.notify_one();
                    release.notified().await;
                    Json(json!([{
                        "id": 12345, "full_name": "acme/private-repo", "name": "private-repo",
                        "owner": {"login":"acme"}, "default_branch":"main", "private":true,
                        "clone_url":"https://github.com/acme/private-repo.git",
                        "updated_at":"2026-10-15T00:00:00Z"
                    }]))
                }
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut config = test_config();
    config.auth.scm_github_api_base_url = format!("http://{}", listener.local_addr().unwrap());
    let provider = tokio::spawn(async move { axum::serve(listener, provider_app).await.unwrap() });
    let app = TestApp::spawn_with_config(config).await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    TestUser::create_admin(&app, org.id).await;
    seed_github_scm_connection(&app, user.id, "read-token").await;
    seed_github_repo(&app, org.id, "12345", "acme/private-repo").await;
    let client = app.authenticated_client(&user);
    let route = format!("/api/v1/scm/github/repos?orgId={}", org.id);
    let request = tokio::spawn(async move { client.get(&route).await });
    tokio::time::timeout(Duration::from_secs(10), requested.notified())
        .await
        .unwrap();
    tokio::time::timeout(
        Duration::from_secs(5),
        sqlx::query("DELETE FROM org_memberships WHERE org_id = $1 AND user_id = $2")
            .bind(org.id)
            .bind(user.id)
            .execute(&app.db),
    )
    .await
    .expect("provider I/O must release the authority transaction")
    .unwrap();
    release.notify_one();
    let response = tokio::time::timeout(Duration::from_secs(10), request)
        .await
        .unwrap()
        .unwrap();
    assert_json_error(response, StatusCode::FORBIDDEN, "FORBIDDEN").await;
    provider.abort();
    app.cleanup().await;
}

async fn start_cli_device(app: &TestApp, org_id: Uuid, workspace_id: Uuid) -> (String, String) {
    let response = app
        .client
        .post(format!("{}/api/v1/auth/cli/start", app.base_url))
        .json(&json!({
            "workspaceId": workspace_id.to_string(),
            "orgId": org_id.to_string(),
        }))
        .send()
        .await
        .expect("CLI start request failed");
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("parse start body");
    (
        body["data"]["deviceCode"].as_str().unwrap().to_string(),
        body["data"]["userCode"].as_str().unwrap().to_string(),
    )
}

async fn poll_cli_device(app: &TestApp, device_code: &str) -> Value {
    let response = app
        .client
        .post(format!("{}/api/v1/auth/cli/token", app.base_url))
        .json(&json!({ "deviceCode": device_code }))
        .send()
        .await
        .expect("CLI poll request failed");
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("parse poll body");
    body["data"].clone()
}

/// SEC-H05: an API key is a delegated credential with its own organization
/// and scope ceiling, so it must never approve a device grant that mints the
/// creator's full user session. A read-only key in the target org, a key from
/// another org of the same creator, and even a key holding every scope are all
/// refused; the request stays pending and no refresh token is issued, while
/// the creator's own browser session can still approve it.
#[tokio::test]
#[serial_test::serial]
async fn test_cli_device_approve_refuses_api_keys() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org_a = TestOrg::create_with_admin(&app, &mut user).await;
    let all_scopes: Vec<&str> = Scope::all()
        .iter()
        .filter(|scope| **scope != Scope::WorkerHost)
        .map(|scope| scope.as_str())
        .collect();
    let org_a_full_key = create_api_key(&app, &user, org_a.id, &all_scopes).await;
    let org_b = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace_b = TestWorkspace::create(&app, &org_b).await;
    let org_b_read_key = create_api_key(&app, &user, org_b.id, &["workspaces:read"]).await;
    let org_b_full_key = create_api_key(&app, &user, org_b.id, &all_scopes).await;

    let (device_code, user_code) = start_cli_device(&app, org_b.id, workspace_b.id).await;
    for (label, key) in [
        ("read-only key of the target org", &org_b_read_key),
        ("key of another org of the creator", &org_a_full_key),
        ("full-scope key of the target org", &org_b_full_key),
    ] {
        let response = helpers::AuthenticatedClient::new(
            app.client.clone(),
            app.base_url.clone(),
            key.clone(),
        )
        .post_json(
            "/api/v1/auth/cli/approve",
            &json!({ "userCode": user_code, "approve": true }),
        )
        .await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN, "{label}");
        let body: Value = response.json().await.expect("parse approve error");
        assert_eq!(body["error"]["code"], "FORBIDDEN", "{label}");

        let poll = poll_cli_device(&app, &device_code).await;
        assert_eq!(poll["status"], "pending", "{label}");
        assert!(poll["jwt"].is_null(), "{label}");
        assert!(poll["refreshToken"].is_null(), "{label}");
    }

    // A key cannot deny the request either: approval decisions belong to the
    // person.
    let deny = helpers::AuthenticatedClient::new(
        app.client.clone(),
        app.base_url.clone(),
        org_b_full_key.clone(),
    )
    .post_json(
        "/api/v1/auth/cli/approve",
        &json!({ "userCode": user_code, "approve": false }),
    )
    .await;
    assert_eq!(deny.status(), StatusCode::FORBIDDEN);

    let refresh_tokens: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM refresh_tokens WHERE user_id = $1")
            .bind(user.id)
            .fetch_one(&app.db)
            .await
            .expect("count refresh tokens");
    assert_eq!(refresh_tokens, 0);
    let (status, approver): (String, Option<Uuid>) = sqlx::query_as(
        "SELECT status, user_id FROM cli_device_authorizations WHERE workspace_id = $1",
    )
    .bind(workspace_b.id)
    .fetch_one(&app.db)
    .await
    .expect("read device authorization");
    assert_eq!((status.as_str(), approver), ("pending", None));

    let approve = app
        .authenticated_client(&user)
        .post_json(
            "/api/v1/auth/cli/approve",
            &json!({ "userCode": user_code, "approve": true }),
        )
        .await;
    assert_eq!(approve.status(), StatusCode::OK);
    let poll = poll_cli_device(&app, &device_code).await;
    assert_eq!(poll["status"], "approved");
    assert!(poll["refreshToken"].as_str().is_some());

    app.cleanup().await;
}

/// Verifies a logged-in user cannot approve a device request for a workspace
/// in an org they do not belong to.
#[tokio::test]
#[serial_test::serial]
async fn test_cli_device_approve_requires_target_org_membership() {
    let app = TestApp::spawn().await;
    let target_org = TestOrg::create(&app).await;
    let workspace = TestWorkspace::create(&app, &target_org).await;
    let other_org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(&app, other_org.id, &[Scope::WorkspacesRead]).await;
    let browser_client = app.authenticated_client(&user);

    let start_response = app
        .client
        .post(format!("{}/api/v1/auth/cli/start", app.base_url))
        .json(&json!({
            "workspaceId": workspace.id.to_string(),
            "orgId": target_org.id.to_string()
        }))
        .send()
        .await
        .expect("CLI start request failed");
    assert_eq!(start_response.status(), StatusCode::OK);
    let start_body: Value = start_response.json().await.expect("parse start body");
    let user_code = start_body["data"]["userCode"].as_str().unwrap();
    let device_code = start_body["data"]["deviceCode"].as_str().unwrap();

    let approve_response = browser_client
        .post_json(
            "/api/v1/auth/cli/approve",
            &json!({
                "userCode": user_code,
                "approve": true
            }),
        )
        .await;
    assert_json_error(approve_response, StatusCode::FORBIDDEN, "FORBIDDEN").await;

    let poll_response = app
        .client
        .post(format!("{}/api/v1/auth/cli/token", app.base_url))
        .json(&json!({ "deviceCode": device_code }))
        .send()
        .await
        .expect("CLI poll request failed");
    assert_eq!(poll_response.status(), StatusCode::OK);
    let poll_body: Value = poll_response.json().await.expect("parse poll body");
    assert_eq!(poll_body["data"]["status"], "pending");
    assert!(poll_body["data"]["refreshToken"].is_null());

    app.cleanup().await;
}

/// Verifies an org member cannot approve a workspace-scoped device request for
/// an invite-only workspace unless they also have workspace access.
#[tokio::test]
#[serial_test::serial]
async fn test_cli_device_approve_rejects_personal_workspace_of_another_member() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create_personal(&app, &org, owner.id).await;
    let member = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;

    let start_response = app
        .client
        .post(format!("{}/api/v1/auth/cli/start", app.base_url))
        .json(&json!({
            "workspaceId": workspace.id.to_string(),
            "orgId": org.id.to_string()
        }))
        .send()
        .await
        .expect("CLI start request failed");
    assert_eq!(start_response.status(), StatusCode::OK);
    let start_body: Value = start_response.json().await.expect("parse start body");
    let user_code = start_body["data"]["userCode"].as_str().unwrap();
    let device_code = start_body["data"]["deviceCode"].as_str().unwrap();

    let approve_response = app
        .authenticated_client(&member)
        .post_json(
            "/api/v1/auth/cli/approve",
            &json!({
                "userCode": user_code,
                "approve": true
            }),
        )
        .await;
    assert_json_error(approve_response, StatusCode::FORBIDDEN, "FORBIDDEN").await;

    let poll_response = app
        .client
        .post(format!("{}/api/v1/auth/cli/token", app.base_url))
        .json(&json!({ "deviceCode": device_code }))
        .send()
        .await
        .expect("CLI poll request failed");
    assert_eq!(poll_response.status(), StatusCode::OK);
    let poll_body: Value = poll_response.json().await.expect("parse poll body");
    assert_eq!(poll_body["data"]["status"], "pending");
    assert!(poll_body["data"]["refreshToken"].is_null());

    app.cleanup().await;
}

/// Verifies the actual `g5d auth login` command implementation against a real
/// TestApp and Postgres database. The test starts the CLI login future, reads
/// the pending user code from the real database, approves through the HTTP
/// browser endpoint, and asserts that g5d persisted refresh-capable auth only
/// after approval.
#[tokio::test]
#[serial_test::serial]
async fn test_g5d_auth_login_completes_against_real_api_device_flow() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let browser_client = app.authenticated_client(&user);

    let workspace_dir = tempfile::tempdir().expect("workspace tempdir");
    let g5_dir = workspace_dir.path().join(".g5");
    std::fs::create_dir_all(&g5_dir).expect("create .g5 dir");
    std::fs::write(
        g5_dir.join("workspace.yaml"),
        format!(
            "workspace_id: {}\nremote:\n  enabled: true\n  api_url: {}\n",
            workspace.id, app.base_url
        ),
    )
    .expect("write workspace yaml");
    std::fs::write(
        g5_dir.join("auth_state.json"),
        serde_json::to_string_pretty(&json!({
            "apiUrl": app.base_url,
            "webUrl": app.base_url,
            "orgId": org.id.to_string(),
            "workspaceId": workspace.id.to_string(),
            "authMode": "device"
        }))
        .unwrap(),
    )
    .expect("write secretless auth state");

    let auth_args = g5d::cli::AuthArgs {
        command: g5d::cli::AuthCommand::Login(g5d::cli::AuthLoginArgs {
            device_name: Some("integration-test-g5d".to_string()),
            org_id: None,
            workspace_id: None,
            timeout_secs: 10,
            poll_interval_secs: Some(1),
        }),
    };
    let global = g5d::cli::GlobalFlags {
        workspace: Some(workspace_dir.path().to_path_buf()),
        verbose: 0,
        quiet: true,
        json: false,
        api_url: Some(app.base_url.clone()),
    };

    let login_handle =
        tokio::spawn(async move { g5d::cli::auth::execute(&auth_args, &global).await });
    let user_code = wait_for_cli_device_user_code(&app, workspace.id).await;

    let approve_response = browser_client
        .post_json(
            "/api/v1/auth/cli/approve",
            &json!({
                "userCode": user_code,
                "approve": true
            }),
        )
        .await;
    assert_eq!(approve_response.status(), StatusCode::OK);

    let login_result = tokio::time::timeout(Duration::from_secs(15), login_handle)
        .await
        .expect("g5d auth login should finish after approval")
        .expect("g5d auth login task should not panic");
    login_result.expect("g5d auth login should succeed");

    let auth_state_body = std::fs::read_to_string(g5_dir.join("auth_state.json"))
        .expect("auth state should be saved");
    let auth_state: Value =
        serde_json::from_str(&auth_state_body).expect("saved auth state should be JSON");
    assert_eq!(auth_state["apiUrl"], app.base_url);
    assert_eq!(auth_state["workspaceId"], workspace.id.to_string());
    assert_eq!(auth_state["orgId"], org.id.to_string());
    assert_eq!(auth_state["authMode"], "device");
    assert!(auth_state["jwt"]
        .as_str()
        .is_some_and(|jwt| !jwt.is_empty()));
    let refresh_token = auth_state["refreshToken"]
        .as_str()
        .expect("g5d login should persist refresh token only after approval");

    let refresh_response = app
        .client
        .post(format!("{}/api/v1/auth/refresh", app.base_url))
        .json(&json!({ "refreshToken": refresh_token }))
        .send()
        .await
        .expect("refresh with g5d auth token failed");
    assert_eq!(refresh_response.status(), StatusCode::OK);

    app.cleanup().await;
}

async fn wait_for_cli_device_user_code(app: &TestApp, workspace_id: Uuid) -> String {
    for _ in 0..100 {
        let user_code: Option<String> = sqlx::query_scalar(
            r#"
            SELECT user_code
            FROM cli_device_authorizations
            WHERE workspace_id = $1
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .bind(workspace_id)
        .fetch_optional(&app.db)
        .await
        .expect("query pending CLI device code");

        if let Some(user_code) = user_code {
            return user_code;
        }

        sleep(Duration::from_millis(50)).await;
    }

    panic!("timed out waiting for CLI device authorization row");
}

// ===========================================================================
// Group 6: Scope Check
// ===========================================================================

// ===========================================================================
// Group 6: WebSocket ticket minting
// ===========================================================================

/// Verifies that `POST /api/v1/auth/ws-ticket` mints a short-lived ticket for
/// a workspace the caller can access under the restricted runtime DB roles.
#[tokio::test]
async fn test_ws_ticket_returns_ticket_for_accessible_workspace() {
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

    let response = client
        .post_json(
            "/api/v1/auth/ws-ticket",
            &json!({
                "workspaceId": workspace.id,
            }),
        )
        .await;

    assert_eq!(response.status(), StatusCode::OK);

    let body: Value = response.json().await.expect("Failed to parse JSON");
    let ticket = body["data"]["ticket"]
        .as_str()
        .expect("Expected ticket string in response");
    assert!(
        !ticket.is_empty(),
        "Expected non-empty websocket ticket for accessible workspace",
    );
    let validator = JwtValidator::new(TEST_PUBLIC_KEY_PEM, None, None).expect("test validator");
    validator
        .validate_websocket_ticket(ticket, Some(workspace.id))
        .expect("workspace ticket must bind to its requested workspace");
    assert!(
        validator.validate_websocket_ticket(ticket, None).is_err(),
        "workspace ticket must not open a platform-wide event socket"
    );
    assert!(
        validator
            .validate_websocket_ticket(ticket, Some(Uuid::new_v4()))
            .is_err(),
        "workspace ticket must not open another workspace socket"
    );

    app.cleanup().await;
}

#[tokio::test]
async fn test_platform_ws_ticket_is_disjoint_from_workspace_ticket() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let response = app
        .authenticated_client(&user)
        .post_json("/api/v1/auth/ws-ticket", &json!({}))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("Failed to parse JSON");
    let ticket = body["data"]["ticket"]
        .as_str()
        .expect("Expected ticket string in response");
    let validator = JwtValidator::new(TEST_PUBLIC_KEY_PEM, None, None).expect("test validator");
    validator
        .validate_websocket_ticket(ticket, None)
        .expect("platform event ticket must validate for the platform socket");
    assert!(
        validator
            .validate_websocket_ticket(ticket, Some(workspace.id))
            .is_err(),
        "platform event ticket must not open a workspace socket"
    );

    app.cleanup().await;
}

#[tokio::test]
async fn test_ws_ticket_rejects_personal_workspace_of_another_member() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create_personal(&app, &org, owner.id).await;
    let member = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;

    let response = app
        .authenticated_client(&member)
        .post_json(
            "/api/v1/auth/ws-ticket",
            &json!({
                "workspaceId": workspace.id,
            }),
        )
        .await;

    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    app.cleanup().await;
}

/// SEC-M75: a WebSocket ticket is minted from the user's current role, so an
/// API key, however scoped, receives neither a workspace ticket for a
/// workspace its creator owns nor a platform ticket. The creator's own session
/// still does.
#[tokio::test]
async fn test_ws_ticket_refuses_api_keys() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let key = create_api_key(&app, &user, org.id, &["workspaces:read"]).await;
    let key_client =
        helpers::AuthenticatedClient::new(app.client.clone(), app.base_url.clone(), key);

    for body in [json!({ "workspaceId": workspace.id }), json!({})] {
        let response = key_client.post_json("/api/v1/auth/ws-ticket", &body).await;
        assert_json_error(response, StatusCode::FORBIDDEN, "FORBIDDEN").await;

        let response = app
            .authenticated_client(&user)
            .post_json("/api/v1/auth/ws-ticket", &body)
            .await;
        assert_eq!(response.status(), StatusCode::OK, "{body}");
    }

    app.cleanup().await;
}

// ===========================================================================
// Group 7: SCM Endpoints — Authentication
// ===========================================================================

/// Verifies that `DELETE /api/v1/scm/github/disconnect` returns 404 when the
/// user has no GitHub connection.
#[tokio::test]
async fn test_scm_disconnect_nonexistent_returns_404() {
    let app = TestApp::spawn().await;
    let user = TestUser::create(&app).await;
    let client = app.authenticated_client(&user);

    let response = client.delete("/api/v1/scm/github/disconnect").await;

    assert_json_error(response, StatusCode::NOT_FOUND, "NOT_FOUND").await;

    app.cleanup().await;
}

// ===========================================================================
// Group 7: SCM Connect Flow
// ===========================================================================

/// Verifies that `POST /api/v1/scm/github/connect` connects GitHub by
/// validating and storing a user-supplied fine-grained PAT.
///
/// GitHub SCM connect migrated off the OAuth-redirect flow (now GitLab-only)
/// to a PAT: the handler validates the token against
/// `{scm_github_api_base_url}/user` and upserts an `auth_method='pat'`
/// connection. The legacy `GET .../connect` path is intentionally unsupported
/// for GitHub (it is absent from `scm_oauth_configs`). (g5/g5-api/auth/scm D8.)
#[tokio::test]
async fn test_scm_connect_github_via_pat_stores_connection() {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let mock_server = MockServer::start().await;
    // `validate_pat` calls `GET {scm_github_api_base_url}/user` with the PAT.
    Mock::given(method("GET"))
        .and(path("/user"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 99001,
            "login": "octocat"
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    let mut config = test_config();
    config.auth.scm_github_api_base_url = mock_server.uri();

    let app = TestApp::spawn_with_config(config).await;
    let user = TestUser::create(&app).await;
    let client = app.authenticated_client(&user);

    let response = client
        .post_json(
            "/api/v1/scm/github/connect",
            &json!({ "token": "github_pat_11ABCDEFG_validfinegrainedtoken" }),
        )
        .await;

    assert_eq!(response.status(), StatusCode::OK);

    let body: Value = response.json().await.expect("Failed to parse JSON");
    let data = &body["data"];
    assert_eq!(data["provider"], "github");
    assert_eq!(
        data["providerUsername"], "octocat",
        "connect result echoes the identity GitHub returned for the PAT"
    );

    // The connection is persisted as a PAT (no OAuth refresh token loop).
    let auth_method: String = sqlx::query_scalar(
        "SELECT auth_method FROM user_scm_connections WHERE user_id = $1 AND provider = 'github'",
    )
    .bind(user.id)
    .fetch_one(&app.db)
    .await
    .expect("github SCM connection should be persisted");
    assert_eq!(auth_method, "pat");

    app.cleanup().await;
}

#[tokio::test]
async fn test_scm_repo_list_scopes_import_status_to_requested_org() {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/user/repos"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {
                "id": 12345,
                "full_name": "acme/private-repo",
                "name": "private-repo",
                "owner": { "login": "acme" },
                "default_branch": "main",
                "private": true,
                "clone_url": "https://github.com/acme/private-repo.git",
                "description": "private repo",
                "language": "Rust",
                "updated_at": "2026-03-19T12:00:00Z"
            }
        ])))
        .expect(1)
        .mount(&mock_server)
        .await;

    let mut config = test_config();
    config.auth.github = Some(g5_api::config::OAuthProviderConfig {
        client_id: "test-github-client-id".to_string(),
        client_secret: "test-github-client-secret".to_string(),
        authorize_url: Some(format!("{}/login/oauth/authorize", mock_server.uri())),
        token_url: Some(format!("{}/login/oauth/access_token", mock_server.uri())),
        userinfo_url: Some(format!("{}/user", mock_server.uri())),
    });
    // GitHub repo listing hits `{scm_github_api_base_url}/user/repos` (the
    // PAT-era config knob, not the OAuth provider config), so point it at the
    // mock that stubs `/user/repos`.
    config.auth.scm_github_api_base_url = mock_server.uri();

    let app = TestApp::spawn_with_config(config).await;
    let mut user = TestUser::create(&app).await;
    let target_org = TestOrg::create_with_admin(&app, &mut user).await;
    let foreign_org = TestOrg::create(&app).await;

    seed_github_scm_connection(&app, user.id, "ghp_repo_list_token").await;
    let foreign_repo_id =
        seed_github_repo(&app, foreign_org.id, "12345", "acme/private-repo").await;

    let response = app
        .client
        .get(format!(
            "{}/api/v1/scm/github/repos?orgId={}",
            app.base_url, target_org.id
        ))
        .header("Authorization", format!("Bearer {}", user.jwt))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("Failed to parse JSON");
    let repos = body["data"]["repos"]
        .as_array()
        .expect("repos should be an array");
    assert_eq!(repos.len(), 1, "expected one repo from the mock provider");
    assert_eq!(repos[0]["providerRepoId"], "12345");
    assert_eq!(
        repos[0]["alreadyImported"],
        Value::Bool(false),
        "foreign-org imports must not be surfaced as already imported"
    );
    assert_eq!(
        repos[0]["importedRepoId"],
        Value::Null,
        "foreign-org repo IDs must not leak through import status"
    );

    let db_row = sqlx::query_scalar::<_, Uuid>("SELECT id FROM repos WHERE provider_repo_id = $1")
        .bind("12345")
        .fetch_one(&app.db)
        .await
        .expect("seeded repo should still exist");
    assert_eq!(db_row, foreign_repo_id);

    app.cleanup().await;
}

#[tokio::test]
async fn test_scm_repo_list_requires_membership_for_requested_org() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let member_org = TestOrg::create_with_admin(&app, &mut user).await;
    let foreign_org = TestOrg::create(&app).await;

    let response = app
        .client
        .get(format!(
            "{}/api/v1/scm/github/repos?orgId={}",
            app.base_url, foreign_org.id
        ))
        .header("Authorization", format!("Bearer {}", user.jwt))
        .send()
        .await
        .expect("Failed to send request");

    assert_json_error(response, StatusCode::FORBIDDEN, "FORBIDDEN").await;

    let membership_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM org_memberships WHERE user_id = $1 AND org_id = $2",
    )
    .bind(user.id)
    .bind(member_org.id)
    .fetch_one(&app.db)
    .await
    .expect("failed to verify membership");
    assert_eq!(membership_count, 1);

    app.cleanup().await;
}

#[tokio::test]
async fn test_scm_import_allows_same_provider_repo_in_multiple_orgs() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let target_org = TestOrg::create_with_admin(&app, &mut user).await;
    let foreign_org = TestOrg::create(&app).await;

    let foreign_repo_id =
        seed_github_repo(&app, foreign_org.id, "12345", "acme/private-repo").await;

    let response = app
        .authenticated_client(&user)
        .post_json(
            "/api/v1/scm/github/repos/import",
            &json!({
                "orgId": target_org.id,
                "repos": [
                    {
                        "providerRepoId": "12345",
                        "fullName": "acme/private-repo",
                        "cloneUrl": "https://github.com/acme/private-repo.git",
                        "defaultBranch": "main"
                    }
                ]
            }),
        )
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("Failed to parse JSON");
    let imported = body["data"]["imported"]
        .as_array()
        .expect("imported should be an array");
    assert_eq!(imported.len(), 1);
    assert_ne!(
        imported[0]["id"]
            .as_str()
            .expect("repo id should be present"),
        foreign_repo_id.to_string(),
        "target org import should create its own repo row",
    );

    let rows = sqlx::query_as::<_, (Uuid, Uuid)>(
        "SELECT id, org_id FROM repos WHERE provider = 'github' AND provider_repo_id = $1",
    )
    .bind("12345")
    .fetch_all(&app.db)
    .await
    .expect("failed to query repos");
    assert_eq!(
        rows.len(),
        2,
        "each org should own its own imported repo row"
    );
    assert!(
        rows.iter()
            .any(|(repo_id, org_id)| *repo_id == foreign_repo_id && *org_id == foreign_org.id),
        "foreign org repo row should remain intact",
    );
    assert!(
        rows.iter()
            .any(|(repo_id, org_id)| *repo_id != foreign_repo_id && *org_id == target_org.id),
        "target org should get a distinct repo row",
    );

    app.cleanup().await;
}

#[tokio::test]
async fn test_scm_import_same_org_remains_idempotent() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let existing_repo_id = seed_github_repo(&app, org.id, "12345", "acme/private-repo").await;

    let response = app
        .authenticated_client(&user)
        .post_json(
            "/api/v1/scm/github/repos/import",
            &json!({
                "orgId": org.id,
                "repos": [
                    {
                        "providerRepoId": "12345",
                        "fullName": "acme/private-repo",
                        "cloneUrl": "https://github.com/acme/private-repo.git",
                        "defaultBranch": "main"
                    }
                ]
            }),
        )
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("Failed to parse JSON");
    let imported = body["data"]["imported"]
        .as_array()
        .expect("imported should be an array");
    assert_eq!(imported.len(), 1);
    assert_eq!(
        imported[0]["id"],
        Value::String(existing_repo_id.to_string())
    );
    assert_eq!(imported[0]["orgId"], Value::String(org.id.to_string()));

    app.cleanup().await;
}

// ===========================================================================
// Group 5: Full OAuth Callback (Success Path via wiremock)
// ===========================================================================

/// Verifies the full OAuth login success path end-to-end:
///
/// 1. Initiate OAuth login → get state + redirect URL
/// 2. Simulate provider callback with authorization code
/// 3. g5-api exchanges code for token (wiremock token endpoint)
/// 4. g5-api fetches user info (wiremock userinfo endpoint)
/// 5. User is upserted, JWT + refresh token are minted and returned
///
/// This test uses wiremock to stand up a local server that mimics GitHub's
/// `/login/oauth/access_token` and `/user` endpoints.
///
/// The login endpoint now returns a 302 redirect (not JSON), and the
/// callback endpoint sets an httpOnly cookie and redirects to `/`.
/// We use a no-redirect client to inspect the intermediate responses.
#[tokio::test]
async fn test_oauth_callback_success_mints_jwt() {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    // Start wiremock server to mock GitHub token + userinfo endpoints
    let mock_server = MockServer::start().await;

    // Mock token exchange endpoint
    Mock::given(method("POST"))
        .and(path("/login/oauth/access_token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "mock-access-token-12345",
            "token_type": "bearer",
            "scope": "user:email read:user"
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    // Mock user info endpoint (GitHub format)
    Mock::given(method("GET"))
        .and(path("/user"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 98765,
            "login": "oauth-test-user",
            "name": "OAuth Test User",
            "email": "oauth-test@g5.dev",
            "avatar_url": "https://example.com/avatar.png"
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/user/emails"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"email": "oauth-test@g5.dev", "primary": true, "verified": true}
        ])))
        .expect(1)
        .mount(&mock_server)
        .await;

    // Build config with GitHub URLs pointing at wiremock
    let mut config = test_config();
    config.auth.github = Some(g5_api::config::OAuthProviderConfig {
        client_id: "test-github-client-id".to_string(),
        client_secret: "test-github-client-secret".to_string(),
        authorize_url: Some(format!("{}/login/oauth/authorize", mock_server.uri())),
        token_url: Some(format!("{}/login/oauth/access_token", mock_server.uri())),
        userinfo_url: Some(format!("{}/user", mock_server.uri())),
    });

    let app = TestApp::spawn_with_config(config).await;
    // Sign-in admits only invitations and memberships (g5.user.admission).
    crate::helpers::platform::invite_for_sign_in(&app, "oauth-test@g5.dev").await;

    // Use a no-redirect client to inspect 302 responses
    let no_redirect_client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .cookie_store(true)
        .build()
        .unwrap();

    // Step 1: Initiate OAuth login — expect 303 redirect to authorize URL
    let login_response = no_redirect_client
        .get(format!("{}/api/v1/auth/login/github", app.base_url))
        .send()
        .await
        .expect("Login request failed");

    assert!(
        login_response.status().is_redirection(),
        "Expected a redirect, got {}",
        login_response.status()
    );
    let location = login_response
        .headers()
        .get("location")
        .expect("Expected Location header on redirect")
        .to_str()
        .unwrap();
    assert!(
        location.contains("/login/oauth/authorize"),
        "Redirect should point to the authorize URL, got: {}",
        location
    );

    // Extract state from the redirect URL
    let url = reqwest::Url::parse(location).expect("Parse redirect URL");
    let state = url
        .query_pairs()
        .find(|(k, _)| k == "state")
        .map(|(_, v)| v.to_string())
        .expect("Expected state parameter in redirect URL");

    // Step 2: Simulate provider callback from the same browser — expect 303
    // redirect with cookie
    let callback_response = no_redirect_client
        .get(format!(
            "{}/api/v1/auth/callback/github?code=mock-auth-code&state={}",
            app.base_url, state
        ))
        .header("cookie", oauth_attempt_cookie(&login_response))
        .send()
        .await
        .expect("Callback request failed");

    assert!(
        callback_response.status().is_redirection(),
        "Callback should redirect, got {}",
        callback_response.status()
    );

    // Verify the Set-Cookie header contains the refresh token
    let set_cookie = callback_response
        .headers()
        .get("set-cookie")
        .expect("Expected Set-Cookie header")
        .to_str()
        .unwrap();
    assert!(
        set_cookie.starts_with("__Host-g5_refresh="),
        "Cookie should be __Host-g5_refresh, got: {}",
        set_cookie
    );
    assert!(set_cookie.contains("HttpOnly"), "Cookie should be HttpOnly");
    helpers::assert_host_cookie(set_cookie);
    assert!(
        callback_response
            .headers()
            .get_all("set-cookie")
            .iter()
            .any(|value| {
                let value = value.to_str().unwrap();
                value.starts_with("g5_login_denied=")
                    && value.contains("Path=/login")
                    && value.contains("Max-Age=0")
            }),
        "successful OAuth callback must clear any prior denied-login context cookie"
    );

    // Extract the refresh token from the cookie
    let refresh_token = set_cookie
        .split(';')
        .next()
        .unwrap()
        .strip_prefix("__Host-g5_refresh=")
        .unwrap();

    // Step 3: Use the refresh token to get a JWT (simulates SPA boot)
    let refresh_response = app
        .client
        .post(format!("{}/api/v1/auth/refresh", app.base_url))
        .header("Content-Type", "application/json")
        .body(json!({ "refreshToken": refresh_token }).to_string())
        .send()
        .await
        .expect("Refresh request failed");

    assert_eq!(
        refresh_response.status(),
        StatusCode::OK,
        "Refresh should return 200"
    );

    let refresh_body: Value = refresh_response
        .json()
        .await
        .expect("Parse refresh response");
    let data = &refresh_body["data"];

    // Verify JWT and refresh token are returned
    assert!(
        data["jwt"].is_string(),
        "Expected JWT in response: {:?}",
        data
    );
    assert!(
        data["refreshToken"].is_string(),
        "Expected refreshToken in response: {:?}",
        data
    );
    assert!(
        data["userId"].is_string(),
        "Expected userId in response: {:?}",
        data
    );

    // Verify the returned JWT works for authenticated requests
    let jwt = data["jwt"].as_str().unwrap();
    let user_id = data["userId"].as_str().unwrap();

    let me_response = app
        .client
        .get(format!("{}/api/v1/users/me", app.base_url))
        .header("Authorization", format!("Bearer {}", jwt))
        .send()
        .await
        .expect("Me request failed");

    assert_eq!(me_response.status(), StatusCode::OK);
    let me_body: Value = me_response.json().await.expect("Parse me response");
    let me_data = &me_body["data"];
    assert_eq!(
        me_data["email"].as_str().unwrap(),
        "oauth-test@g5.dev",
        "User email should match the OAuth provider response"
    );

    // Verify user was persisted in the database
    let db_user = sqlx::query_as::<_, (String, Option<String>, Option<String>, Option<String>)>(
        "SELECT email, display_name, oauth_provider, oauth_id FROM users WHERE id = $1",
    )
    .bind(uuid::Uuid::parse_str(user_id).unwrap())
    .fetch_one(&app.db)
    .await
    .expect("User should exist in database");

    assert_eq!(db_user.0, "oauth-test@g5.dev");
    assert_eq!(db_user.1.as_deref(), Some("OAuth Test User"));
    assert_eq!(db_user.2.as_deref(), Some("github"));
    assert_eq!(db_user.3.as_deref(), Some("98765"));

    app.cleanup().await;
}

mod entra_fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/entra/mod.rs"
    ));
}

async fn microsoft_test_app() -> (TestApp, wiremock::MockServer) {
    let server = wiremock::MockServer::start().await;
    entra_fixture::keys(&server, &[1]).await;
    let mut config = test_config();
    config.auth.microsoft = Some(g5_api::config::OAuthProviderConfig {
        client_id: entra_fixture::CLIENT.to_string(),
        client_secret: "test-microsoft-client-secret".to_string(),
        authorize_url: Some(format!("{}/tenant/authorize", server.uri())),
        token_url: Some(format!("{}/tenant/token", server.uri())),
        userinfo_url: None,
    });
    config.auth.microsoft_tenant_id = Some(entra_fixture::TENANT.to_string());
    config.auth.microsoft_jwks_url = Some(format!("{}/keys", server.uri()));
    (TestApp::spawn_with_config(config).await, server)
}

fn microsoft_browser() -> reqwest::Client {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap()
}

async fn microsoft_attempt(app: &TestApp, browser: &reqwest::Client) -> (String, String) {
    let response = browser
        .get(format!("{}/api/v1/auth/login/microsoft", app.base_url))
        .send()
        .await
        .unwrap();
    assert!(response.status().is_redirection());
    let location = response.headers()["location"].to_str().unwrap().to_string();
    assert!(location.contains("/tenant/authorize?"));
    assert_eq!(query_param(&location, "scope"), "openid email profile");
    assert_eq!(
        query_param(&location, "nonce"),
        query_param(&location, "state")
    );
    assert_eq!(query_param(&location, "state").len(), 64);
    (location, oauth_attempt_cookie(&response))
}

async fn microsoft_token(server: &wiremock::MockServer, token: Option<String>, code: &str) {
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/tenant/token"))
        .and(wiremock::matchers::body_string_contains(format!(
            "code={code}&"
        )))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "mock-microsoft-access-token", "id_token": token
        })))
        .expect(1)
        .mount(server)
        .await;
}

async fn microsoft_callback(
    app: &TestApp,
    browser: &reqwest::Client,
    attempt: &(String, String),
    code: &str,
) -> reqwest::Response {
    browser
        .get(format!(
            "{}/api/v1/auth/callback/microsoft?code={code}&state={}",
            app.base_url,
            query_param(&attempt.0, "state")
        ))
        .header("cookie", &attempt.1)
        .send()
        .await
        .unwrap()
}

async fn login_state_snapshot(app: &TestApp) -> Value {
    sqlx::query_scalar(
        "SELECT jsonb_build_object(
            'users', (SELECT jsonb_agg(to_jsonb(u) ORDER BY id) FROM users u),
            'memberships', (SELECT jsonb_agg(to_jsonb(m) ORDER BY org_id, user_id) FROM org_memberships m),
            'invitations', (SELECT jsonb_agg(to_jsonb(i) ORDER BY id) FROM org_invitations i),
            'refreshTokens', (SELECT count(*) FROM refresh_tokens))",
    )
    .fetch_one(&app.db)
    .await
    .unwrap()
}

#[tokio::test]
async fn test_microsoft_email_never_links_accounts_or_accepts_invitations() {
    let (app, server) = microsoft_test_app().await;
    let mut github = TestUser::create(&app).await;
    let _github_org = TestOrg::create_with_admin(&app, &mut github).await;
    let mut service = TestUser::create(&app).await;
    let _service_org = TestOrg::create_with_admin(&app, &mut service).await;
    sqlx::query(
        "UPDATE users SET oauth_provider = 'service', email = 'worker@g5.internal' WHERE id = $1",
    )
    .bind(service.id)
    .execute(&app.db)
    .await
    .unwrap();
    crate::helpers::platform::invite_for_sign_in(&app, "pending@example.com").await;
    let browser = microsoft_browser();
    for (index, email) in [
        github.email.as_str(),
        "worker@g5.internal",
        "pending@example.com",
        "uninvited@example.com",
    ]
    .into_iter()
    .enumerate()
    {
        let before = login_state_snapshot(&app).await;
        let attempt = microsoft_attempt(&app, &browser).await;
        let mut claims = entra_fixture::claims(&query_param(&attempt.0, "nonce"));
        claims["email"] = json!(email);
        let code = format!("collision-{index}");
        microsoft_token(&server, Some(entra_fixture::signed(&claims, 1)), &code).await;
        let callback = microsoft_callback(&app, &browser, &attempt, &code).await;
        assert!(callback.status().is_redirection());
        assert_eq!(callback.headers()["location"], "/login/denied");
        assert!(!set_cookies(&callback)
            .iter()
            .any(|c| c.starts_with("__Host-g5_refresh=")));
        assert_eq!(
            login_state_snapshot(&app).await,
            before,
            "email hint {email} must grant nothing"
        );
    }
    assert!(server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .all(|r| r.url.path() != "/oidc/userinfo"));
    app.cleanup().await;
}

#[tokio::test]
async fn test_microsoft_existing_pair_signs_in_without_using_email_and_replay_is_refused() {
    let (app, server) = microsoft_test_app().await;
    let mut member = TestUser::create(&app).await;
    let _org = TestOrg::create_with_admin(&app, &mut member).await;
    let identity = format!("{}:{}", entra_fixture::TENANT, entra_fixture::OBJECT);
    sqlx::query("UPDATE users SET oauth_provider = 'microsoft', oauth_id = $2 WHERE id = $1")
        .bind(member.id)
        .bind(&identity)
        .execute(&app.db)
        .await
        .unwrap();
    crate::helpers::platform::invite_for_sign_in(&app, "hint@example.com").await;
    let browser = microsoft_browser();
    let info: Value = app
        .client
        .get(format!("{}/api/v1/instance/info", app.base_url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        info["data"]["authProviders"],
        json!(["github", "microsoft"])
    );
    for (index, hint) in [Some("hint@example.com"), None].into_iter().enumerate() {
        let attempt = microsoft_attempt(&app, &browser).await;
        let mut claims = entra_fixture::claims(&query_param(&attempt.0, "nonce"));
        claims["email"] = json!(hint);
        let code = format!("member-{index}");
        microsoft_token(&server, Some(entra_fixture::signed(&claims, 1)), &code).await;
        let callback = microsoft_callback(&app, &browser, &attempt, &code).await;
        assert!(callback.status().is_redirection());
        assert_ne!(callback.headers()["location"], "/login/denied");
        let cookies = set_cookies(&callback);
        let refresh = cookies
            .iter()
            .find_map(|c| {
                c.strip_prefix("__Host-g5_refresh=")
                    .and_then(|v| v.split(';').next())
            })
            .expect("successful login returns refresh credential");
        let refreshed: Value = app
            .client
            .post(format!("{}/api/v1/auth/refresh", app.base_url))
            .json(&json!({"refreshToken": refresh}))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(refreshed["data"]["userId"], member.id.to_string());
        let jwt = refreshed["data"]["jwt"].as_str().unwrap();
        let me: Value = app
            .client
            .get(format!("{}/api/v1/users/me", app.base_url))
            .bearer_auth(jwt)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(me["data"]["email"], member.email);
        let stored: (String, String, String) =
            sqlx::query_as("SELECT email, oauth_provider, oauth_id FROM users WHERE id = $1")
                .bind(member.id)
                .fetch_one(&app.db)
                .await
                .unwrap();
        assert_eq!(
            stored,
            (member.email.clone(), "microsoft".into(), identity.clone())
        );
        let before = login_state_snapshot(&app).await;
        let replay = microsoft_callback(&app, &browser, &attempt, &code).await;
        assert_json_error(replay, StatusCode::UNAUTHORIZED, "UNAUTHORIZED").await;
        assert_eq!(login_state_snapshot(&app).await, before);
        let pending: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM org_invitations WHERE email = 'hint@example.com' AND status = 'pending')")
            .fetch_one(&app.db).await.unwrap();
        assert!(
            pending,
            "even a linked identity cannot accept an invitation using its hint"
        );
    }
    let requests = server.received_requests().await.unwrap();
    let first_token = requests
        .iter()
        .find(|r| r.url.path() == "/tenant/token")
        .unwrap();
    let verifier = url::form_urlencoded::parse(&first_token.body)
        .find(|(key, _)| key == "code_verifier")
        .unwrap()
        .1
        .into_owned();
    assert!(!verifier.is_empty());
    assert_eq!(
        requests
            .iter()
            .filter(|r| r.url.path() == "/tenant/token")
            .count(),
        2,
        "replays do not exchange codes"
    );
    let before = login_state_snapshot(&app).await;
    let attempt = microsoft_attempt(&app, &browser).await;
    let mut other_object = entra_fixture::claims(&query_param(&attempt.0, "nonce"));
    other_object["oid"] = json!(Uuid::new_v4());
    other_object["email"] = json!(member.email);
    microsoft_token(
        &server,
        Some(entra_fixture::signed(&other_object, 1)),
        "other-object",
    )
    .await;
    let denied = microsoft_callback(&app, &browser, &attempt, "other-object").await;
    assert!(denied.status().is_redirection());
    assert_eq!(denied.headers()["location"], "/login/denied");
    assert_eq!(login_state_snapshot(&app).await, before);
    app.cleanup().await;
}

#[tokio::test]
async fn test_microsoft_invalid_tokens_consume_attempt_without_mutation() {
    let (app, server) = microsoft_test_app().await;
    let browser = microsoft_browser();
    for field in [
        "signature",
        "iss",
        "tid",
        "aud",
        "exp",
        "nbf",
        "nonce",
        "missing-nonce",
        "missing-token",
        "oid",
    ] {
        let attempt = microsoft_attempt(&app, &browser).await;
        let mut claims = entra_fixture::claims(&query_param(&attempt.0, "nonce"));
        match field {
            "iss" => claims["iss"] = json!("https://attacker.example/v2.0"),
            "tid" => claims["tid"] = json!(Uuid::new_v4()),
            "aud" => claims["aud"] = json!("wrong-client"),
            "exp" => claims["exp"] = json!(jsonwebtoken::get_current_timestamp() - 120),
            "nbf" => claims["nbf"] = json!(jsonwebtoken::get_current_timestamp() + 120),
            "nonce" => claims["nonce"] = json!("another-attempt"),
            "missing-nonce" => {
                claims.as_object_mut().unwrap().remove("nonce");
            }
            "oid" => {
                claims.as_object_mut().unwrap().remove("oid");
            }
            _ => {}
        }
        let mut token = entra_fixture::signed(&claims, 1);
        if field == "signature" {
            let other = entra_fixture::signed(&claims, 2);
            token = format!(
                "{}.{}",
                token.rsplit_once('.').unwrap().0,
                other.rsplit_once('.').unwrap().1
            );
        }
        microsoft_token(&server, (field != "missing-token").then_some(token), field).await;
        let before = login_state_snapshot(&app).await;
        let callback = microsoft_callback(&app, &browser, &attempt, field).await;
        assert!(!set_cookies(&callback)
            .iter()
            .any(|c| c.starts_with("__Host-g5_refresh=")));
        assert_json_error(callback, StatusCode::UNAUTHORIZED, "UNAUTHORIZED").await;
        assert_json_error(
            microsoft_callback(&app, &browser, &attempt, field).await,
            StatusCode::UNAUTHORIZED,
            "UNAUTHORIZED",
        )
        .await;
        assert_eq!(login_state_snapshot(&app).await, before, "{field}");
    }
    app.cleanup().await;
}

#[tokio::test]
async fn test_microsoft_nonce_binds_concurrent_attempts_and_is_consumed_once() {
    let (app, server) = microsoft_test_app().await;
    let mut member = TestUser::create(&app).await;
    let _org = TestOrg::create_with_admin(&app, &mut member).await;
    sqlx::query("UPDATE users SET oauth_provider = 'microsoft', oauth_id = $2 WHERE id = $1")
        .bind(member.id)
        .bind(format!(
            "{}:{}",
            entra_fixture::TENANT,
            entra_fixture::OBJECT
        ))
        .execute(&app.db)
        .await
        .unwrap();
    let browser = microsoft_browser();
    let first = microsoft_attempt(&app, &browser).await;
    let second = microsoft_attempt(&app, &browser).await;
    assert_ne!(
        query_param(&first.0, "nonce"),
        query_param(&second.0, "nonce")
    );
    let token = entra_fixture::signed(&entra_fixture::claims(&query_param(&first.0, "nonce")), 1);
    microsoft_token(&server, Some(token.clone()), "wrong-attempt").await;
    let before = login_state_snapshot(&app).await;
    assert_json_error(
        microsoft_callback(&app, &browser, &second, "wrong-attempt").await,
        StatusCode::UNAUTHORIZED,
        "UNAUTHORIZED",
    )
    .await;
    assert_eq!(login_state_snapshot(&app).await, before);
    assert_eq!(
        pkce_challenge(&token_request_verifier(&server, "/tenant/token").await),
        query_param(&second.0, "code_challenge")
    );
    microsoft_token(&server, Some(token), "concurrent").await;
    let (a, b) = tokio::join!(
        microsoft_callback(&app, &browser, &first, "concurrent"),
        microsoft_callback(&app, &browser, &first, "concurrent")
    );
    let statuses = [a.status(), b.status()];
    assert_eq!(statuses.iter().filter(|s| s.is_redirection()).count(), 1);
    assert_eq!(
        statuses
            .iter()
            .filter(|s| **s == StatusCode::UNAUTHORIZED)
            .count(),
        1
    );
    let refresh_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM refresh_tokens WHERE user_id = $1")
            .bind(member.id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(refresh_count, 1);

    app.cleanup().await;
}

#[tokio::test]
async fn test_microsoft_membership_removed_during_callback_cannot_admit() {
    let (app, server) = microsoft_test_app().await;
    let org = TestOrg::create(&app).await;
    let member = TestUser::create_with_scopes(&app, org.id, &[]).await;
    sqlx::query("UPDATE users SET oauth_provider = 'microsoft', oauth_id = $2 WHERE id = $1")
        .bind(member.id)
        .bind(format!(
            "{}:{}",
            entra_fixture::TENANT,
            entra_fixture::OBJECT
        ))
        .execute(&app.db)
        .await
        .unwrap();
    let browser = microsoft_browser();
    let attempt = microsoft_attempt(&app, &browser).await;
    microsoft_token(
        &server,
        Some(entra_fixture::signed(
            &entra_fixture::claims(&query_param(&attempt.0, "nonce")),
            1,
        )),
        "removed-member",
    )
    .await;
    let mut removal = app.db.begin().await.unwrap();
    sqlx::query("DELETE FROM org_memberships WHERE user_id = $1")
        .bind(member.id)
        .execute(&mut *removal)
        .await
        .unwrap();
    let (response, ()) = tokio::join!(
        microsoft_callback(&app, &browser, &attempt, "removed-member"),
        async {
            helpers::wait_for_lock_wait(&app).await;
            removal.commit().await.unwrap();
        }
    );
    assert!(response.status().is_redirection());
    assert_eq!(response.headers()["location"], "/login/denied");
    let refresh_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM refresh_tokens WHERE user_id = $1")
            .bind(member.id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(refresh_count, 0);
    app.cleanup().await;
}

#[tokio::test]
async fn test_microsoft_oauth_callback_refused_code_is_unauthorized() {
    let (app, server) = microsoft_test_app().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/tenant/token"))
        .respond_with(
            wiremock::ResponseTemplate::new(400).set_body_json(json!({"error": "invalid_grant"})),
        )
        .expect(1)
        .mount(&server)
        .await;
    let browser = microsoft_browser();
    let attempt = microsoft_attempt(&app, &browser).await;
    let before = login_state_snapshot(&app).await;
    assert_json_error(
        microsoft_callback(&app, &browser, &attempt, "refused").await,
        StatusCode::UNAUTHORIZED,
        "UNAUTHORIZED",
    )
    .await;
    assert_eq!(login_state_snapshot(&app).await, before);
    assert_eq!(
        server.received_requests().await.unwrap().len(),
        1,
        "no identity or key lookup after refused exchange"
    );
    app.cleanup().await;
}

#[tokio::test]
async fn test_microsoft_legacy_link_migration_clears_only_microsoft() {
    let (app, _server) = microsoft_test_app().await;
    let microsoft = TestUser::create(&app).await;
    let github = TestUser::create(&app).await;
    sqlx::query(
        "UPDATE users SET oauth_provider = 'microsoft', oauth_id = 'legacy-sub' WHERE id = $1",
    )
    .bind(microsoft.id)
    .execute(&app.db)
    .await
    .unwrap();
    let before: Value = sqlx::query_scalar("SELECT to_jsonb(u) FROM users u WHERE id = $1")
        .bind(github.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    sqlx::raw_sql(include_str!(
        "../src/db/migrations/148_clear_legacy_microsoft_links.sql"
    ))
    .execute(&app.db)
    .await
    .unwrap();
    let cleared: (Option<String>, Option<String>, String) =
        sqlx::query_as("SELECT oauth_provider, oauth_id, email FROM users WHERE id = $1")
            .bind(microsoft.id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(cleared, (None, None, microsoft.email));
    let after: Value = sqlx::query_scalar("SELECT to_jsonb(u) FROM users u WHERE id = $1")
        .bind(github.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(before, after);
    app.cleanup().await;
}

#[tokio::test]
async fn test_github_verified_email_cannot_attach_to_service_account() {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/login/oauth/access_token"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"access_token":"token", "token_type":"bearer"})),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/user"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(
                json!({"id":12345, "login":"attacker", "email":"worker@g5.internal"}),
            ),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/user/emails"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"email":"worker@g5.internal", "verified":true, "primary":true},
            {"email":"pending@example.com", "verified":true, "primary":false}
        ])))
        .mount(&server)
        .await;
    let mut config = test_config();
    config.auth.github = Some(g5_api::config::OAuthProviderConfig {
        client_id: "client".into(),
        client_secret: "secret".into(),
        authorize_url: Some(format!("{}/login/oauth/authorize", server.uri())),
        token_url: Some(format!("{}/login/oauth/access_token", server.uri())),
        userinfo_url: Some(format!("{}/user", server.uri())),
    });
    let app = TestApp::spawn_with_config(config).await;
    let mut service = TestUser::create(&app).await;
    let _org = TestOrg::create_with_admin(&app, &mut service).await;
    sqlx::query(
        "UPDATE users SET oauth_provider = 'service', email = 'worker@g5.internal' WHERE id = $1",
    )
    .bind(service.id)
    .execute(&app.db)
    .await
    .unwrap();
    // The separate verified invitation passes pre-admission. The account
    // transaction must still refuse the service candidate and accept nothing.
    crate::helpers::platform::invite_for_sign_in(&app, "pending@example.com").await;
    let before = login_state_snapshot(&app).await;
    let response = github_oauth_callback_response(&app, &microsoft_browser(), "service-code").await;
    assert!(!set_cookies(&response)
        .iter()
        .any(|c| c.starts_with("__Host-g5_refresh=")));
    assert_json_error(response, StatusCode::UNAUTHORIZED, "UNAUTHORIZED").await;
    assert_eq!(login_state_snapshot(&app).await, before);
    // A concurrent writer changing an admitted candidate to service must be
    // observed under the row lock before OAuth credentials can be rewritten.
    sqlx::query("UPDATE users SET oauth_provider = 'github' WHERE id = $1")
        .bind(service.id)
        .execute(&app.db)
        .await
        .unwrap();
    let mut competing = app.db.begin().await.unwrap();
    sqlx::query("UPDATE users SET oauth_provider = 'service' WHERE id = $1")
        .bind(service.id)
        .execute(&mut *competing)
        .await
        .unwrap();
    let browser = microsoft_browser();
    let (response, ()) = tokio::join!(
        github_oauth_callback_response(&app, &browser, "concurrent-service"),
        async {
            helpers::wait_for_lock_wait(&app).await;
            competing.commit().await.unwrap();
        }
    );
    assert_json_error(response, StatusCode::UNAUTHORIZED, "UNAUTHORIZED").await;
    assert_eq!(login_state_snapshot(&app).await, before);
    app.cleanup().await;
}

/// An operator-created Admin account (`g5.organization.platform-operator.create-org`)
/// has no sign-in identity; the person's first verified GitHub sign-in with
/// that email, here a secondary one, links to it and keeps its Admin role.
#[tokio::test]
async fn test_oauth_callback_links_operator_created_admin_by_secondary_verified_email() {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/login/oauth/access_token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "mock-access-token-secondary-allowlist",
            "token_type": "bearer",
            "scope": "user:email read:user"
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/user"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 24680,
            "login": "secondary-allowlist-user",
            "name": "Secondary Allowlist User",
            "email": "primary@g5.dev",
            "avatar_url": "https://example.com/secondary-allowlist.png"
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/user/emails"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"email": "primary@g5.dev", "primary": true, "verified": true},
            {"email": "secondary@g5.dev", "primary": false, "verified": true},
            {"email": "unverified@g5.dev", "primary": false, "verified": false}
        ])))
        .expect(1)
        .mount(&mock_server)
        .await;

    let mut config = test_config();
    config.auth.github = Some(g5_api::config::OAuthProviderConfig {
        client_id: "test-github-client-id".to_string(),
        client_secret: "test-github-client-secret".to_string(),
        authorize_url: Some(format!("{}/login/oauth/authorize", mock_server.uri())),
        token_url: Some(format!("{}/login/oauth/access_token", mock_server.uri())),
        userinfo_url: Some(format!("{}/user", mock_server.uri())),
    });

    let app = TestApp::spawn_with_config(config).await;
    let operator = TestUser::create(&app).await;
    crate::helpers::platform::make_operator(&app, &operator, "member").await;
    let created = app
        .authenticated_client(&operator)
        .post_json(
            "/api/v1/platform/orgs",
            &json!({ "name": "Linked Owner Org", "adminEmail": "Secondary@G5.dev" }),
        )
        .await;
    assert_eq!(created.status(), StatusCode::CREATED);
    let created: Value = created.json().await.unwrap();
    assert_eq!(created["data"]["admin"]["accountCreated"], true);
    let owner_id = Uuid::parse_str(created["data"]["admin"]["userId"].as_str().unwrap()).unwrap();
    let org_id = Uuid::parse_str(created["data"]["org"]["id"].as_str().unwrap()).unwrap();

    let no_redirect_client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .cookie_store(true)
        .build()
        .unwrap();

    let data = complete_github_oauth_login(&app, &no_redirect_client, "secondary-code").await;
    let user_id = Uuid::parse_str(data["userId"].as_str().expect("user id should be present"))
        .expect("user id should parse");
    assert_eq!(
        user_id, owner_id,
        "sign-in links the operator-created account"
    );

    let db_user = sqlx::query_as::<_, (String, Option<String>, Option<String>)>(
        "SELECT email, oauth_provider, oauth_id FROM users WHERE id = $1",
    )
    .bind(user_id)
    .fetch_one(&app.db)
    .await
    .expect("user should exist");
    assert_eq!(
        db_user.0, "secondary@g5.dev",
        "the account keeps the email it was created for"
    );
    assert_eq!(db_user.1.as_deref(), Some("github"));
    assert_eq!(db_user.2.as_deref(), Some("24680"));
    let role: String =
        sqlx::query_scalar("SELECT role FROM org_memberships WHERE org_id = $1 AND user_id = $2")
            .bind(org_id)
            .bind(user_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(role, "admin");

    app.cleanup().await;
}

#[tokio::test]
async fn test_oauth_callback_auto_accepts_invitation_for_secondary_verified_github_email() {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/login/oauth/access_token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "mock-access-token-secondary-invite",
            "token_type": "bearer",
            "scope": "user:email read:user"
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/user"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 24681,
            "login": "secondary-invite-user",
            "name": "Secondary Invite User",
            "email": "primary-invite@g5.dev",
            "avatar_url": "https://example.com/secondary-invite.png"
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/user/emails"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"email": "primary-invite@g5.dev", "primary": true, "verified": true},
            {"email": "invited-secondary@g5.dev", "primary": false, "verified": true},
            {"email": "invited-unverified@g5.dev", "primary": false, "verified": false}
        ])))
        .expect(1)
        .mount(&mock_server)
        .await;

    let mut config = test_config();
    config.auth.github = Some(g5_api::config::OAuthProviderConfig {
        client_id: "test-github-client-id".to_string(),
        client_secret: "test-github-client-secret".to_string(),
        authorize_url: Some(format!("{}/login/oauth/authorize", mock_server.uri())),
        token_url: Some(format!("{}/login/oauth/access_token", mock_server.uri())),
        userinfo_url: Some(format!("{}/user", mock_server.uri())),
    });

    let app = TestApp::spawn_with_config(config).await;
    let mut inviter = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut inviter).await;
    let verified_invitation_id = Uuid::new_v4();
    let unverified_invitation_id = Uuid::new_v4();
    let expires_at = chrono::Utc::now() + chrono::Duration::days(7);

    sqlx::query(
        r#"
        INSERT INTO org_invitations (id, org_id, email, role, invited_by, status, expires_at)
        VALUES ($1, $2, $3, 'member', $4, 'pending', $5)
        "#,
    )
    .bind(verified_invitation_id)
    .bind(org.id)
    .bind("Invited-Secondary@G5.Dev")
    .bind(inviter.id)
    .bind(expires_at)
    .execute(&app.db)
    .await
    .expect("failed to insert verified invitation");

    sqlx::query(
        r#"
        INSERT INTO org_invitations (id, org_id, email, role, invited_by, status, expires_at)
        VALUES ($1, $2, $3, 'member', $4, 'pending', $5)
        "#,
    )
    .bind(unverified_invitation_id)
    .bind(org.id)
    .bind("invited-unverified@g5.dev")
    .bind(inviter.id)
    .bind(expires_at)
    .execute(&app.db)
    .await
    .expect("failed to insert unverified invitation");

    let no_redirect_client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .cookie_store(true)
        .build()
        .unwrap();

    let data =
        complete_github_oauth_login(&app, &no_redirect_client, "secondary-invite-code").await;
    let user_id = Uuid::parse_str(data["userId"].as_str().expect("user id should be present"))
        .expect("user id should parse");

    let verified_invitation = sqlx::query_as::<_, (String, Option<Uuid>)>(
        "SELECT status, accepted_by FROM org_invitations WHERE id = $1",
    )
    .bind(verified_invitation_id)
    .fetch_one(&app.db)
    .await
    .expect("verified invitation should exist");
    assert_eq!(verified_invitation.0, "accepted");
    assert_eq!(verified_invitation.1, Some(user_id));

    let unverified_status: String =
        sqlx::query_scalar("SELECT status FROM org_invitations WHERE id = $1")
            .bind(unverified_invitation_id)
            .fetch_one(&app.db)
            .await
            .expect("unverified invitation should exist");
    assert_eq!(
        unverified_status, "pending",
        "unverified GitHub email must not satisfy invitation matching"
    );

    let membership_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM org_memberships WHERE org_id = $1 AND user_id = $2",
    )
    .bind(org.id)
    .bind(user_id)
    .fetch_one(&app.db)
    .await
    .expect("membership query should succeed");
    assert_eq!(membership_count, 1);

    app.cleanup().await;
}

/// Issue #1517 (denied-page context): a login refused admission
/// redirects to `/login/denied` and sets the short-lived, non-HttpOnly
/// `g5_login_denied` context cookie carrying the attempted verified email, so
/// the SPA can name the rejected address. No session credential is issued and
/// no user row is created.
#[tokio::test]
async fn test_oauth_callback_denied_sets_login_denied_context_cookie() {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/login/oauth/access_token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "mock-access-token-denied-context",
            "token_type": "bearer",
            "scope": "user:email read:user"
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/user"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 24710,
            "login": "denied-context-user",
            "name": "Denied Context User",
            "email": "denied-user@example.com",
            "avatar_url": "https://example.com/denied-context.png"
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/user/emails"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"email": "denied-user@example.com", "primary": true, "verified": true}
        ])))
        .expect(1)
        .mount(&mock_server)
        .await;

    let mut config = test_config();
    config.auth.github = Some(g5_api::config::OAuthProviderConfig {
        client_id: "test-github-client-id".to_string(),
        client_secret: "test-github-client-secret".to_string(),
        authorize_url: Some(format!("{}/login/oauth/authorize", mock_server.uri())),
        token_url: Some(format!("{}/login/oauth/access_token", mock_server.uri())),
        userinfo_url: Some(format!("{}/user", mock_server.uri())),
    });
    // No invitation and no membership exist (g5.user.admission).

    let app = TestApp::spawn_with_config(config).await;

    let no_redirect_client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .cookie_store(true)
        .build()
        .unwrap();

    let callback = github_oauth_callback_response(&app, &no_redirect_client, "denied-code").await;

    assert!(
        callback.status().is_redirection(),
        "denied callback should redirect, got {}",
        callback.status()
    );
    let location = callback
        .headers()
        .get("location")
        .expect("Expected Location header")
        .to_str()
        .unwrap();
    assert_eq!(location, "/login/denied");

    // The context cookie carries the attempted verified email (percent-encoded),
    // is readable by the SPA (not HttpOnly), and is scoped to /login.
    let set_cookie = callback
        .headers()
        .get_all("set-cookie")
        .iter()
        .map(|v| v.to_str().unwrap().to_string())
        .find(|c| c.starts_with("g5_login_denied="))
        .expect("Expected g5_login_denied Set-Cookie on denial");
    assert!(
        set_cookie.contains("denied-user%40example.com"),
        "context cookie must carry the percent-encoded attempted email, got: {set_cookie}"
    );
    assert!(
        set_cookie.contains("Path=/login"),
        "context cookie must be scoped to /login, got: {set_cookie}"
    );
    assert!(
        !set_cookie.contains("HttpOnly"),
        "context cookie must be readable by the SPA (not HttpOnly), got: {set_cookie}"
    );
    // No session credential is minted on denial.
    assert!(
        !callback
            .headers()
            .get_all("set-cookie")
            .iter()
            .any(|v| v.to_str().unwrap().starts_with("__Host-g5_refresh=")),
        "denied login must not mint a __Host-g5_refresh session cookie"
    );

    // Denial happens before the user upsert, so no user row is created.
    let denied_user_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE email = 'denied-user@example.com'")
            .fetch_one(&app.db)
            .await
            .expect("denied user count query should succeed");
    assert_eq!(denied_user_count, 0);

    app.cleanup().await;
}

/// Regression test for issue #1517: an invited member must be able to log in
/// **more than once**.
///
/// The first login consumes the pending invitation (flips it to `accepted`
/// and creates the `org_memberships` row). Before the B7/D5 fix, the second
/// login found no pending invitation, so it was
/// rejected with 403 → `/login/denied` — the invited org was locked out after
/// exactly one login. The B7 gate now also admits a login whose identity maps
/// to an existing user holding at least one `org_memberships` row, so re-entry
/// succeeds. (`.semlift/specs/core_workflow/auth_session_lifecycle.semlift.md`
/// D5/B7.)
#[tokio::test]
async fn test_oauth_callback_existing_membership_admits_repeat_login_issue_1517() {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let mock_server = MockServer::start().await;

    // Three login rounds include account-only sign-in after deactivation;
    // each round exchanges a token and fetches user + verified emails.
    Mock::given(method("POST"))
        .and(path("/login/oauth/access_token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "mock-access-token-repeat-login",
            "token_type": "bearer",
            "scope": "user:email read:user"
        })))
        .expect(3)
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/user"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 24699,
            "login": "repeat-login-user",
            "name": "Repeat Login User",
            "email": "invited-member@g5.dev",
            "avatar_url": "https://example.com/repeat-login.png"
        })))
        .expect(3)
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/user/emails"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"email": "invited-member@g5.dev", "primary": true, "verified": true}
        ])))
        .expect(3)
        .mount(&mock_server)
        .await;

    let mut config = test_config();
    config.auth.github = Some(g5_api::config::OAuthProviderConfig {
        client_id: "test-github-client-id".to_string(),
        client_secret: "test-github-client-secret".to_string(),
        authorize_url: Some(format!("{}/login/oauth/authorize", mock_server.uri())),
        token_url: Some(format!("{}/login/oauth/access_token", mock_server.uri())),
        userinfo_url: Some(format!("{}/user", mock_server.uri())),
    });
    // The only thing that can admit the second login is the existing membership.

    let app = TestApp::spawn_with_config(config).await;
    let mut inviter = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut inviter).await;
    let invitation_id = Uuid::new_v4();
    let expires_at = chrono::Utc::now() + chrono::Duration::days(7);

    sqlx::query(
        r#"
        INSERT INTO org_invitations (id, org_id, email, role, invited_by, status, expires_at)
        VALUES ($1, $2, $3, 'member', $4, 'pending', $5)
        "#,
    )
    .bind(invitation_id)
    .bind(org.id)
    .bind("invited-member@g5.dev")
    .bind(inviter.id)
    .bind(expires_at)
    .execute(&app.db)
    .await
    .expect("failed to insert invitation");

    let no_redirect_client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .cookie_store(true)
        .build()
        .unwrap();

    // First login: admitted via the pending invitation; it is consumed here.
    let data = complete_github_oauth_login(&app, &no_redirect_client, "repeat-login-code-1").await;
    let user_id = Uuid::parse_str(data["userId"].as_str().expect("user id should be present"))
        .expect("user id should parse");

    let invitation_status: String =
        sqlx::query_scalar("SELECT status FROM org_invitations WHERE id = $1")
            .bind(invitation_id)
            .fetch_one(&app.db)
            .await
            .expect("invitation should exist");
    assert_eq!(
        invitation_status, "accepted",
        "first login should consume the invitation"
    );

    let membership_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM org_memberships WHERE org_id = $1 AND user_id = $2",
    )
    .bind(org.id)
    .bind(user_id)
    .fetch_one(&app.db)
    .await
    .expect("membership query should succeed");
    assert_eq!(membership_count, 1, "first login should create membership");

    // Second login: the invitation is now `accepted`, so the only credential
    // that can admit this login is the existing membership. This is the #1517
    // regression assertion — before the fix it redirected to /login/denied.
    let second_callback =
        github_oauth_callback_response(&app, &no_redirect_client, "repeat-login-code-2").await;

    assert!(
        second_callback.status().is_redirection(),
        "second callback should redirect, got {}",
        second_callback.status()
    );

    let second_location = second_callback
        .headers()
        .get("location")
        .expect("Expected Location header on second callback")
        .to_str()
        .expect("Location header should be valid UTF-8")
        .to_string();
    assert_ne!(
        second_location, "/login/denied",
        "an existing org member must not be locked out on repeat login (issue #1517)"
    );
    // Pin the admitted redirect target: an admitted login lands on `/login`
    // (the SPA then bootstraps the session from the refresh cookie), not the
    // denied page.
    assert_eq!(
        second_location, "/login",
        "an admitted repeat login must redirect to /login, got: {second_location}"
    );

    // The denial branch also emits a Set-Cookie (it clears `g5_login_denied`),
    // so `get("set-cookie").is_some()` cannot prove admission. Require the
    // `__Host-g5_refresh=` session cookie specifically — mirroring the negative check
    // the denial test makes — to prove the member was actually admitted.
    assert!(
        second_callback
            .headers()
            .get_all("set-cookie")
            .iter()
            .any(|v| v.to_str().unwrap().starts_with("__Host-g5_refresh=")),
        "an admitted repeat login must mint a __Host-g5_refresh session cookie"
    );

    // Organization inactivity cannot disable the person's global account.
    let response = app
        .authenticated_client(&inviter)
        .post_json(
            &format!("/api/v1/orgs/{}/members/{user_id}/deactivate", org.id),
            &json!({"activityRevision":0}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let account =
        complete_github_oauth_login(&app, &no_redirect_client, "repeat-login-code-3").await;
    assert_eq!(account["userId"], user_id.to_string());
    let claims = JwtValidator::new(TEST_PUBLIC_KEY_PEM, None, None)
        .unwrap()
        .validate(account["jwt"].as_str().expect("account session JWT"))
        .unwrap();
    assert_eq!(claims.org, Uuid::nil().to_string());
    assert!(sqlx::query_scalar::<_, bool>(
        "SELECT deactivated_at IS NOT NULL FROM org_memberships WHERE org_id=$1 AND user_id=$2"
    )
    .bind(org.id)
    .bind(user_id)
    .fetch_one(&app.db)
    .await
    .unwrap());

    app.cleanup().await;
}

/// Issue #1517 (fail-open on infrastructure fault): admission runs two
/// existence queries (pending invitation, existing membership). A transient
/// database failure on either must **not** be swallowed into "not admitted" —
/// that would lock a legitimate member out and mislead the operator with a
/// normal-looking admission denial. The membership query decides re-entry for
/// every existing member, so we inject a real fault there by renaming the
/// `org_memberships` table out from under the query and assert the callback
/// surfaces a 500 `INTERNAL_ERROR` instead of the `/login/denied` redirect.
#[tokio::test]
async fn test_oauth_callback_membership_query_db_error_returns_500_not_denial() {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/login/oauth/access_token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "mock-access-token-db-fault",
            "token_type": "bearer",
            "scope": "user:email read:user"
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/user"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 24711,
            "login": "db-fault-user",
            "name": "DB Fault User",
            "email": "db-fault-user@example.com",
            "avatar_url": "https://example.com/db-fault.png"
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/user/emails"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"email": "db-fault-user@example.com", "primary": true, "verified": true}
        ])))
        .expect(1)
        .mount(&mock_server)
        .await;

    let mut config = test_config();
    config.auth.github = Some(g5_api::config::OAuthProviderConfig {
        client_id: "test-github-client-id".to_string(),
        client_secret: "test-github-client-secret".to_string(),
        authorize_url: Some(format!("{}/login/oauth/authorize", mock_server.uri())),
        token_url: Some(format!("{}/login/oauth/access_token", mock_server.uri())),
        userinfo_url: Some(format!("{}/user", mock_server.uri())),
    });
    // The callback reaches the invitation/membership queries.

    let app = TestApp::spawn_with_config(config).await;

    // Inject a real database fault targeting the membership existence query:
    // rename the table out from under it so the query fails at execution time.
    // The pending-invitation query (org_invitations) still succeeds, so this
    // isolates the membership-query failure path. The isolated per-test
    // database is discarded on cleanup.
    sqlx::query("ALTER TABLE org_memberships RENAME TO org_memberships_injected_fault")
        .execute(&app.db)
        .await
        .expect("failed to inject DB fault by renaming org_memberships");

    let no_redirect_client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .cookie_store(true)
        .build()
        .unwrap();

    let callback = github_oauth_callback_response(&app, &no_redirect_client, "db-fault-code").await;

    // The login must FAIL OPEN into an error, not fail closed into a denial:
    // no denied-page redirect, no denied-context cookie, and no session cookie.
    let location = callback
        .headers()
        .get("location")
        .map(|v| v.to_str().unwrap().to_string());
    assert_ne!(
        location.as_deref(),
        Some("/login/denied"),
        "a database fault must not masquerade as an admission denial"
    );
    assert!(
        !callback
            .headers()
            .get_all("set-cookie")
            .iter()
            .any(|v| v.to_str().unwrap().starts_with("g5_login_denied=")),
        "a database fault must not set the denied-context cookie (no fail-closed denial)"
    );
    assert!(
        !callback
            .headers()
            .get_all("set-cookie")
            .iter()
            .any(|v| v.to_str().unwrap().starts_with("__Host-g5_refresh=")),
        "a database fault must not mint a session cookie"
    );

    // The attempted user is never upserted: the fault aborts before user upsert.
    let user_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE email = 'db-fault-user@example.com'")
            .fetch_one(&app.db)
            .await
            .expect("user count query should succeed");
    assert_eq!(user_count, 0, "no user row should be created on a DB fault");

    // Consumes the response; also asserts the 500 status.
    assert_json_error(
        callback,
        StatusCode::INTERNAL_SERVER_ERROR,
        "INTERNAL_ERROR",
    )
    .await;

    app.cleanup().await;
}

#[tokio::test]
async fn test_oauth_callback_links_existing_user_by_secondary_verified_github_email() {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/login/oauth/access_token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "mock-access-token-secondary-link",
            "token_type": "bearer",
            "scope": "user:email read:user"
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/user"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 24682,
            "login": "secondary-link-user",
            "name": "Secondary Link User",
            "email": "primary-link@g5.dev",
            "avatar_url": "https://example.com/secondary-link.png"
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/user/emails"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"email": "primary-link@g5.dev", "primary": true, "verified": true},
            {"email": "existing-secondary@g5.dev", "primary": false, "verified": true}
        ])))
        .expect(1)
        .mount(&mock_server)
        .await;

    let mut config = test_config();
    config.auth.github = Some(g5_api::config::OAuthProviderConfig {
        client_id: "test-github-client-id".to_string(),
        client_secret: "test-github-client-secret".to_string(),
        authorize_url: Some(format!("{}/login/oauth/authorize", mock_server.uri())),
        token_url: Some(format!("{}/login/oauth/access_token", mock_server.uri())),
        userinfo_url: Some(format!("{}/user", mock_server.uri())),
    });

    let app = TestApp::spawn_with_config(config).await;
    let existing_user_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO users (id, email, display_name, oauth_provider, oauth_id)
        VALUES ($1, 'existing-secondary@g5.dev', 'Existing Secondary', 'google', 'google-existing-secondary')
        "#,
    )
    .bind(existing_user_id)
    .execute(&app.db)
    .await
    .expect("failed to insert existing user");
    // Its membership admits the sign-in (g5.user.admission).
    let org = TestOrg::create(&app).await;
    sqlx::query("INSERT INTO org_memberships (org_id, user_id, role) VALUES ($1, $2, 'member')")
        .bind(org.id)
        .bind(existing_user_id)
        .execute(&app.db)
        .await
        .expect("failed to insert existing membership");

    let users_before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&app.db)
        .await
        .unwrap();

    let no_redirect_client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .cookie_store(true)
        .build()
        .unwrap();

    let data = complete_github_oauth_login(&app, &no_redirect_client, "secondary-link-code").await;
    let returned_user_id =
        Uuid::parse_str(data["userId"].as_str().expect("user id should be present"))
            .expect("user id should parse");
    assert_eq!(
        returned_user_id, existing_user_id,
        "login should link the existing account matched by a secondary verified email"
    );

    let db_user = sqlx::query_as::<_, (String, Option<String>, Option<String>, Option<String>)>(
        "SELECT email, display_name, oauth_provider, oauth_id FROM users WHERE id = $1",
    )
    .bind(existing_user_id)
    .fetch_one(&app.db)
    .await
    .expect("existing user should still exist");

    assert_eq!(
        db_user.0, "existing-secondary@g5.dev",
        "secondary-email account linking should not silently rewrite the stored account email"
    );
    assert_eq!(db_user.1.as_deref(), Some("Secondary Link User"));
    assert_eq!(db_user.2.as_deref(), Some("github"));
    assert_eq!(db_user.3.as_deref(), Some("24682"));

    let user_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&app.db)
        .await
        .expect("user count query should succeed");
    assert_eq!(
        user_count, users_before,
        "login should not create a duplicate user"
    );

    app.cleanup().await;
}

/// Verifies that a second OAuth login with the same provider user updates
/// (not duplicates) the existing user record.
#[tokio::test]
async fn test_oauth_callback_upserts_existing_user() {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let mock_server = MockServer::start().await;

    // Mock token exchange
    Mock::given(method("POST"))
        .and(path("/login/oauth/access_token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "mock-token",
            "token_type": "bearer",
            "scope": "user:email read:user"
        })))
        .expect(2) // Will be called twice (two logins)
        .mount(&mock_server)
        .await;

    // Mock user info — same oauth_id, updated name
    Mock::given(method("GET"))
        .and(path("/user"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 55555,
            "login": "double-login-user",
            "name": "Updated Name",
            "email": "double@g5.dev",
            "avatar_url": "https://example.com/avatar2.png"
        })))
        .expect(2)
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/user/emails"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"email": "double@g5.dev", "primary": true, "verified": true}
        ])))
        .expect(2)
        .mount(&mock_server)
        .await;

    let mut config = test_config();
    config.auth.github = Some(g5_api::config::OAuthProviderConfig {
        client_id: "test-github-client-id".to_string(),
        client_secret: "test-github-client-secret".to_string(),
        authorize_url: Some(format!("{}/login/oauth/authorize", mock_server.uri())),
        token_url: Some(format!("{}/login/oauth/access_token", mock_server.uri())),
        userinfo_url: Some(format!("{}/user", mock_server.uri())),
    });

    let app = TestApp::spawn_with_config(config).await;
    // Sign-in admits only invitations and memberships (g5.user.admission).
    crate::helpers::platform::invite_for_sign_in(&app, "double@g5.dev").await;

    // No-redirect client for inspecting 302 responses
    let no_redirect = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .cookie_store(true)
        .build()
        .unwrap();

    // Helper: perform full login flow, return userId from refresh response
    async fn do_login(
        no_redirect: &reqwest::Client,
        app_client: &reqwest::Client,
        base_url: &str,
        code: &str,
    ) -> String {
        // 1. Initiate → get redirect URL → extract state
        let login = no_redirect
            .get(format!("{}/api/v1/auth/login/github", base_url))
            .send()
            .await
            .unwrap();
        let loc = login.headers().get("location").unwrap().to_str().unwrap();
        let url = reqwest::Url::parse(loc).unwrap();
        let state = url
            .query_pairs()
            .find(|(k, _)| k == "state")
            .unwrap()
            .1
            .to_string();

        // 2. Callback → get refresh token cookie
        let cb = no_redirect
            .get(format!(
                "{}/api/v1/auth/callback/github?code={}&state={}",
                base_url, code, state
            ))
            .header("cookie", oauth_attempt_cookie(&login))
            .send()
            .await
            .unwrap();
        let cookie = cb.headers().get("set-cookie").unwrap().to_str().unwrap();
        let refresh = cookie
            .split(';')
            .next()
            .unwrap()
            .strip_prefix("__Host-g5_refresh=")
            .unwrap()
            .to_string();

        // 3. Refresh → get JWT + userId
        let resp = app_client
            .post(format!("{}/api/v1/auth/refresh", base_url))
            .header("Content-Type", "application/json")
            .body(serde_json::json!({ "refreshToken": refresh }).to_string())
            .send()
            .await
            .unwrap();
        let body: serde_json::Value = resp.json().await.unwrap();
        body["data"]["userId"].as_str().unwrap().to_string()
    }

    let user_id_1 = do_login(&no_redirect, &app.client, &app.base_url, "code1").await;
    let user_id_2 = do_login(&no_redirect, &app.client, &app.base_url, "code2").await;

    // Same user ID — not duplicated
    assert_eq!(
        user_id_1, user_id_2,
        "Repeated login should return same userId"
    );

    // Only one user row in DB for this oauth_id
    let count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM users WHERE oauth_provider = 'github' AND oauth_id = '55555'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();

    assert_eq!(
        count.0, 1,
        "Should have exactly one user row, not a duplicate"
    );

    app.cleanup().await;
}

#[tokio::test]
async fn scm_import_rejects_untrusted_clone_urls_atomically_including_retries() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    seed_github_repo(&app, org.id, "existing", "acme/existing").await;
    for url in [
        "https://github.com.attacker.example/o/r.git",
        "https://attacker.example/github.com/r.git",
        "https://github.com@attacker.example/o/r.git",
        "https://user@github.com/o/r.git",
        "http://github.com/o/r.git",
        "https://github.com:8443/o/r.git",
        "https://gitlab.com/o/r.git",
    ] {
        for provider_id in ["new", "existing"] {
            let response = app.authenticated_client(&user).post_json("/api/v1/scm/github/repos/import", &json!({
                "orgId": org.id,
                "repos": [
                    {"providerRepoId": "good-before-bad", "fullName": "acme/good", "cloneUrl": "https://github.com/acme/good.git", "defaultBranch": "main"},
                    {"providerRepoId": provider_id, "fullName": "acme/bad", "cloneUrl": url, "defaultBranch": "main"}
                ]
            })).await;
            assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY, "{url}");
            let body: Value = response.json().await.unwrap();
            assert_eq!(body["error"]["details"]["code"], "SCM_CLONE_URL_INVALID");
        }
    }
    let inserted: i64 = sqlx::query_scalar("SELECT count(*) FROM repos WHERE org_id = $1 AND provider_repo_id IN ('good-before-bad', 'new')").bind(org.id).fetch_one(&app.db).await.unwrap();
    assert_eq!(inserted, 0);
    app.cleanup().await;
}

#[tokio::test]
async fn scm_resolvers_bind_stored_member_token_to_provider_origin() {
    use g5_api::auth::RepositoryOperation;
    use g5_scm::GitAuth;
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    TestUser::create_admin(&app, org.id).await;
    seed_github_scm_connection(&app, user.id, "stored-member-token").await;
    let repo_id = seed_github_repo(&app, org.id, "credential-probe", "o/r").await;
    let manager = app.state.scm_token_manager.as_deref().unwrap();
    for url in [
        "https://github.com.attacker.example/o/r.git",
        "https://gitlab.com/o/r.git",
        "http://github.com/o/r.git",
        "https://user@github.com/o/r.git",
    ] {
        sqlx::query("UPDATE repos SET git_url=$2 WHERE id=$1")
            .bind(repo_id)
            .bind(url)
            .execute(&app.db)
            .await
            .unwrap();
        assert!(matches!(
            app.state
                .repository_credentials
                .resolve(org.id, repo_id, user.id, url, RepositoryOperation::ReadGit)
                .await
                .unwrap(),
            GitAuth::None
        ));
        assert!(matches!(
            manager
                .token_for_user(&app.db, user.id, org.id, "github", url)
                .await
                .unwrap(),
            GitAuth::None
        ));
    }
    sqlx::query("UPDATE repos SET git_url='https://github.com/o/r.git' WHERE id=$1")
        .bind(repo_id)
        .execute(&app.db)
        .await
        .unwrap();
    assert!(matches!(
        app.state
            .repository_credentials
            .resolve(
                org.id,
                repo_id,
                user.id,
                "https://github.com/o/r.git",
                RepositoryOperation::ReadGit
            )
            .await
            .unwrap(),
        GitAuth::ScmToken { .. }
    ));
    assert!(matches!(
        manager
            .token_for_user(
                &app.db,
                user.id,
                org.id,
                "github",
                "https://github.com/o/r.git"
            )
            .await
            .unwrap(),
        GitAuth::ScmToken { .. }
    ));
    sqlx::query("DELETE FROM org_memberships WHERE org_id = $1 AND user_id = $2")
        .bind(org.id)
        .bind(user.id)
        .execute(&app.db)
        .await
        .unwrap();
    assert!(manager
        .token_for_user(
            &app.db,
            user.id,
            org.id,
            "github",
            "https://github.com/o/r.git"
        )
        .await
        .is_err());
    app.cleanup().await;
}

#[tokio::test]
async fn scm_gitlab_groups_and_projects_use_configured_api_after_oauth_is_disabled() {
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    for oauth_enabled in [true, false] {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/gitlab/api/v4/groups"))
            .and(header("authorization", "Bearer configured-gitlab-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                {"id": 7, "name": "Configured group", "avatar_url": null}
            ])))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/gitlab/api/v4/projects"))
            .and(header("authorization", "Bearer configured-gitlab-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
            .expect(1)
            .mount(&server)
            .await;
        let mut config = test_config();
        // The external provider is a hermetic HTTP fixture. Production routing
        // still goes through AuthService and the encrypted connection store.
        let userinfo_url = format!("{}/gitlab/api/v4/user", server.uri());
        config.auth.scm_gitlab_userinfo_url = Some(userinfo_url.clone());
        config.auth.scm_gitlab = oauth_enabled.then(|| g5_api::config::OAuthProviderConfig {
            client_id: "configured-id".into(),
            client_secret: "configured-secret".into(),
            authorize_url: None,
            token_url: None,
            userinfo_url: Some(userinfo_url),
        });
        let app = TestApp::spawn_with_config(config).await;
        let mut user = TestUser::create(&app).await;
        let org = TestOrg::create_with_admin(&app, &mut user).await;
        seed_github_scm_connection(&app, user.id, "configured-gitlab-token").await;
        sqlx::query("UPDATE user_scm_connections SET provider = 'gitlab' WHERE user_id = $1")
            .bind(user.id)
            .execute(&app.db)
            .await
            .unwrap();

        let response = app
            .authenticated_client(&user)
            .get("/api/v1/scm/gitlab/orgs")
            .await;
        let status = response.status();
        let body: Value = response.json().await.unwrap();
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["data"][0]["name"], "Configured group");
        let response = app
            .authenticated_client(&user)
            .get(&format!("/api/v1/scm/gitlab/repos?orgId={}", org.id))
            .await;
        let status = response.status();
        let body: Value = response.json().await.unwrap();
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["data"]["repos"], json!([]));
        server.verify().await;
        app.cleanup().await;
    }
}

#[tokio::test]
async fn scm_configured_gitlab_origin_controls_import_and_both_token_resolvers() {
    use g5_api::auth::RepositoryOperation;
    use g5_scm::GitAuth;
    for oauth_enabled in [true, false] {
        let mut config = test_config();
        config.auth.scm_gitlab = Some(g5_api::config::OAuthProviderConfig {
            client_id: "gitlab-client".into(),
            client_secret: "gitlab-client-secret".into(),
            authorize_url: Some("https://gitlab.internal:8443/oauth/authorize".into()),
            token_url: Some("https://gitlab.internal:8443/oauth/token".into()),
            userinfo_url: Some("https://gitlab.internal:8443/api/v4/user".into()),
        });
        if !oauth_enabled {
            config.auth.scm_gitlab_userinfo_url =
                Some("https://gitlab.internal:8443/api/v4/user".into());
            config.auth.scm_gitlab = None;
        }
        let app = TestApp::spawn_with_config(config).await;
        let mut user = TestUser::create(&app).await;
        let org = TestOrg::create_with_admin(&app, &mut user).await;
        seed_github_scm_connection(&app, user.id, "self-managed-member-token").await;
        sqlx::query("UPDATE user_scm_connections SET provider = 'gitlab' WHERE user_id = $1")
            .bind(user.id)
            .execute(&app.db)
            .await
            .unwrap();
        let repo_id = seed_github_repo(&app, org.id, "credential-probe", "credential/probe").await;
        let manager = app.state.scm_token_manager.as_deref().unwrap();
        for (url, accepted) in [
            ("https://gitlab.internal:8443/group/repo.git", true),
            ("https://gitlab.com/group/repo.git", false),
            ("https://gitlab.internal/group/repo.git", false),
            (
                "https://gitlab.internal.attacker:8443/group/repo.git",
                false,
            ),
            ("https://github.com/group/repo.git", false),
        ] {
            sqlx::query("UPDATE repos SET git_url=$2, provider='gitlab' WHERE id=$1")
                .bind(repo_id)
                .bind(url)
                .execute(&app.db)
                .await
                .unwrap();
            let auth = app
                .state
                .repository_credentials
                .resolve(org.id, repo_id, user.id, url, RepositoryOperation::ReadGit)
                .await
                .unwrap();
            assert_eq!(matches!(auth, GitAuth::ScmToken { .. }), accepted, "{url}");
            let auth = manager
                .token_for_user(&app.db, user.id, org.id, "gitlab", url)
                .await
                .unwrap();
            assert_eq!(matches!(auth, GitAuth::ScmToken { .. }), accepted, "{url}");
            let response = app.authenticated_client(&user).post_json("/api/v1/scm/gitlab/repos/import", &json!({
            "orgId": org.id,
            "repos": [{"providerRepoId": "configured-origin", "fullName": "group/repo", "cloneUrl": url, "defaultBranch": "main"}]
        })).await;
            assert_eq!(
                response.status(),
                if accepted {
                    StatusCode::OK
                } else {
                    StatusCode::UNPROCESSABLE_ENTITY
                },
                "{url}"
            );
        }
        app.cleanup().await;
    }
}

#[tokio::test]
async fn scm_configured_github_prefix_controls_import_and_both_personal_resolvers() {
    use g5_api::auth::RepositoryOperation;
    use g5_scm::GitAuth;
    let server = wiremock::MockServer::start().await;
    let mut config = test_config();
    let mut integration = helpers::github_app::github_app_config(&server);
    integration.web_base_url = "https://github.internal:8443/git".into();
    config.github_app = Some(integration);
    let app = TestApp::spawn_with_config(config).await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    seed_github_scm_connection(&app, user.id, "instance-member-token").await;
    let repo_id = seed_github_repo(&app, org.id, "credential-probe", "credential/probe").await;
    let manager = app.state.scm_token_manager.as_deref().unwrap();
    for (url, accepted) in [
        ("https://github.internal:8443/git/group/repo.git", true),
        ("https://github.internal:8443/other/repo.git", false),
        ("https://github.internal:8443/git/../other/repo.git", false),
        (
            "https://github.internal:8443/git/%2e%2e/other/repo.git",
            false,
        ),
        ("https://github.internal:8443/git-other/repo.git", false),
        ("https://github.internal/git/group/repo.git", false),
        ("https://github.com/group/repo.git", false),
    ] {
        sqlx::query("UPDATE repos SET git_url=$2,provider='github' WHERE id=$1")
            .bind(repo_id)
            .bind(url)
            .execute(&app.db)
            .await
            .unwrap();
        let auth = app
            .state
            .repository_credentials
            .resolve(org.id, repo_id, user.id, url, RepositoryOperation::ReadGit)
            .await
            .unwrap();
        assert_eq!(matches!(auth, GitAuth::ScmToken { .. }), accepted, "{url}");
        let auth = manager
            .token_for_user(&app.db, user.id, org.id, "github", url)
            .await
            .unwrap();
        assert_eq!(matches!(auth, GitAuth::ScmToken { .. }), accepted, "{url}");
        let response = app.authenticated_client(&user).post_json("/api/v1/scm/github/repos/import",&json!({
            "orgId":org.id,"repos":[{"providerRepoId":"configured-instance","fullName":"group/repo","cloneUrl":url,"defaultBranch":"main"}]
        })).await;
        assert_eq!(
            response.status(),
            if accepted {
                StatusCode::OK
            } else {
                StatusCode::UNPROCESSABLE_ENTITY
            },
            "{url}"
        );
    }
    assert!(
        server.received_requests().await.unwrap().is_empty(),
        "admission needs no provider request"
    );
    app.cleanup().await;
}

/// Sessions record how they were issued (g5.security.browser-session-actions):
/// the web sign-in issues a browser session and CLI device login a CLI
/// session, each stamped on the stored refresh credential and on every access
/// token minted from it, refresh included. A session stored before sessions
/// recorded their kind mints tokens with no kind.
#[tokio::test]
#[serial_test::serial]
async fn sessions_record_whether_the_web_sign_in_or_cli_login_issued_them() {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/login/oauth/access_token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "mock-access-token-session-kind",
            "token_type": "bearer",
            "scope": "user:email read:user"
        })))
        .mount(&mock_server)
        .await;
    Mock::given(method("GET"))
        .and(path("/user"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 97531,
            "login": "session-kind-user",
            "name": "Session Kind",
            "email": "session-kind@g5.dev",
            "avatar_url": "https://example.com/session-kind.png"
        })))
        .mount(&mock_server)
        .await;
    Mock::given(method("GET"))
        .and(path("/user/emails"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"email": "session-kind@g5.dev", "primary": true, "verified": true}
        ])))
        .mount(&mock_server)
        .await;
    let mut config = test_config();
    config.auth.github = Some(g5_api::config::OAuthProviderConfig {
        client_id: "test-github-client-id".to_string(),
        client_secret: "test-github-client-secret".to_string(),
        authorize_url: Some(format!("{}/login/oauth/authorize", mock_server.uri())),
        token_url: Some(format!("{}/login/oauth/access_token", mock_server.uri())),
        userinfo_url: Some(format!("{}/user", mock_server.uri())),
    });
    let app = TestApp::spawn_with_config(config).await;
    // Sign-in admits only invitations and memberships (g5.user.admission).
    crate::helpers::platform::invite_for_sign_in(&app, "session-kind@g5.dev").await;
    let kinds = |user_id: Uuid| {
        sqlx::query_scalar::<_, Option<String>>(
            "SELECT client_kind FROM refresh_tokens WHERE user_id = $1 ORDER BY created_at",
        )
        .bind(user_id)
        .fetch_all(&app.db)
    };

    // The web sign-in: the callback's session, then its cookie refresh.
    let no_redirect_client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .cookie_store(true)
        .build()
        .unwrap();
    let browser = complete_github_oauth_login(&app, &no_redirect_client, "session-kind").await;
    let browser_user = Uuid::parse_str(browser["userId"].as_str().unwrap()).unwrap();
    assert_eq!(
        helpers::decode_test_jwt(browser["jwt"].as_str().unwrap())["client"],
        "browser"
    );
    assert_eq!(
        kinds(browser_user).await.unwrap(),
        vec![Some("browser".to_string())]
    );

    // CLI device login, and a refresh of that session.
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let (device_code, user_code) = start_cli_device(&app, org.id, workspace.id).await;
    let approve = app
        .authenticated_client(&user)
        .post_json(
            "/api/v1/auth/cli/approve",
            &json!({"userCode": user_code, "approve": true}),
        )
        .await;
    assert_eq!(approve.status(), StatusCode::OK);
    let cli = poll_cli_device(&app, &device_code).await;
    assert_eq!(
        helpers::decode_test_jwt(cli["jwt"].as_str().unwrap())["client"],
        "cli"
    );
    assert_eq!(kinds(user.id).await.unwrap(), vec![Some("cli".to_string())]);
    let refreshed: Value = app
        .client
        .post(format!("{}/api/v1/auth/refresh", app.base_url))
        .json(&json!({"refreshToken": cli["refreshToken"]}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        helpers::decode_test_jwt(refreshed["data"]["jwt"].as_str().unwrap())["client"],
        "cli"
    );

    // A session stored before sessions recorded their kind has none.
    let legacy_user = TestUser::create(&app).await;
    let legacy = helpers::insert_refresh_token(&app, legacy_user.id).await;
    let refreshed: Value = app
        .client
        .post(format!("{}/api/v1/auth/refresh", app.base_url))
        .json(&json!({"refreshToken": legacy}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(
        helpers::decode_test_jwt(refreshed["data"]["jwt"].as_str().unwrap())
            .get("client")
            .is_none()
    );

    app.cleanup().await;
}
