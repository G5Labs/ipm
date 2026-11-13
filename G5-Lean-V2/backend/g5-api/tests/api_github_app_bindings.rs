//! GitHub App organization binding: connection attempt → setup → OAuth
//! callback → binding v1; coverage change → v2; disconnect → v3; the
//! narrowed-token resolver; installation refresh when GitHub-side
//! permissions change; and rejection of replayed, expired, tampered and
//! forged state, non-admins, cross-org access and tenant UPDATE/DELETE.
//!
//! The real router, auth middleware and Postgres (RLS under `g5_tenant`)
//! are exercised; only GitHub is replaced by a wiremock server.

use crate::helpers;

use g5_api::services::github_app::{new_connection_state, oauth_state_for_installation};
use g5_api::services::github_app_binding::{
    load_pending_attempt, lookup_repository_selection, mint_repository_token, GithubAppCapability,
    GithubAppIneligibleReason, GithubAppSelectionPreflight,
};
use g5_auth::Scope;
use helpers::{test_config, TestApp, TestOrg, TestUser};
use reqwest::StatusCode;
use serde_json::{json, Value};
use uuid::Uuid;
use wiremock::matchers::{body_json, body_string_contains, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use helpers::github_app::github_app_config;

const INSTALLATION_ID: u64 = 555;
const ISSUES_REPO: i64 = 1001;
const API_REPO: i64 = 1002;

async fn spawn_with_github(github: &MockServer) -> TestApp {
    let mut config = test_config();
    config.github_app = Some(github_app_config(github));
    TestApp::spawn_with_config(config).await
}

// OAuth connection tests use an HTTP stub. Repository flows use the real
// HTTPS identity, with only OAuth's network endpoint redirected to that stub.
fn repository_client(
    github: &MockServer,
) -> std::sync::Arc<g5_api::services::github_app::GithubAppClient> {
    let mut config = github_app_config(github);
    config.web_base_url = "https://github.com".into();
    std::sync::Arc::new(
        g5_api::services::github_app::GithubAppClient::new(
            g5_api::services::github_app::GithubAppClient::provider_clients().unwrap(),
            config,
        )
        .with_oauth_web_url_for_test(github.uri()),
    )
}

fn repository_credentials(
    app: &TestApp,
    github: &MockServer,
) -> std::sync::Arc<g5_api::auth::RepositoryCredentials> {
    let key = hex::decode(test_config().auth.scm_encryption_key.unwrap())
        .unwrap()
        .try_into()
        .unwrap();
    let personal = std::sync::Arc::new(g5_api::auth::ScmTokenManager::new(
        key,
        g5_scm::providers::ProviderClients::new(),
    ));
    std::sync::Arc::new(
        g5_api::auth::RepositoryCredentials::new(
            app.state.system_pool.clone(),
            app.state.tenant_pool.clone(),
            app.state.authz.clone(),
            Some(personal),
            Some(repository_client(github)),
        )
        .with_origin_policy(g5_scm::ScmOriginPolicy::default()),
    )
}

/// GitHub behaviour for one reachable installation with Issues write.
async fn mount_github(github: &MockServer) {
    mount_github_with_issues(github, "write").await;
}

/// GitHub behaviour for one reachable installation whose owner has accepted
/// `issues` at `issues_level`.
async fn mount_github_with_issues(github: &MockServer, issues_level: &str) {
    mount_github_installation(github, INSTALLATION_ID, issues_level).await;
    Mock::given(method("POST"))
        .and(path("/login/oauth/access_token"))
        .and(body_string_contains("code=good-code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"access_token": "ghu_user"})))
        .mount(github)
        .await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/user/installations/{INSTALLATION_ID}/repositories"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"repositories": []})))
        .mount(github)
        .await;
    Mock::given(method("POST"))
        .and(path(format!(
            "/app/installations/{INSTALLATION_ID}/access_tokens"
        )))
        .and(body_json(json!({"permissions": {"metadata": "read"}})))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({"token": "ghs_meta"})))
        .mount(github)
        .await;
    Mock::given(method("GET"))
        .and(path("/installation/repositories"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "repositories": [
                {"id": ISSUES_REPO, "full_name": "acme/issues", "private": true},
                {"id": API_REPO, "full_name": "acme/api", "private": true}
            ]
        })))
        .mount(github)
        .await;
}

/// `GET /app/installations/{INSTALLATION_ID}` reporting `reported_id` with
/// `issues` at `issues_level`.
async fn mount_github_installation(github: &MockServer, reported_id: u64, issues_level: &str) {
    Mock::given(method("GET"))
        .and(path(format!("/app/installations/{INSTALLATION_ID}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": reported_id,
            "account": {"id": 44, "login": "acme"},
            "permissions": {"issues": issues_level, "metadata": "read"},
            "html_url": format!("{}/organizations/acme/settings/installations/{INSTALLATION_ID}", github.uri()),
        })))
        .mount(github)
        .await;
}

fn no_redirect_client() -> reqwest::Client {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("client")
}

fn query_param(url: &str, name: &str) -> String {
    let url = url::Url::parse(url).expect("absolute URL");
    url.query_pairs()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.into_owned())
        .unwrap_or_else(|| panic!("{name} missing from {url}"))
}

fn location(response: &reqwest::Response) -> String {
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

fn binding_path(org_id: Uuid, suffix: &str) -> String {
    format!("/api/v1/orgs/{org_id}/github-app{suffix}")
}

/// Create an attempt as `user`; returns its signed state and the
/// attempt-bound PKCE cookie set on the administrator's browser.
async fn start_attempt(app: &TestApp, user: &TestUser, org_id: Uuid) -> (String, String) {
    let response = app
        .authenticated_client(user)
        .post_json(&binding_path(org_id, "/connection-attempt"), &json!({}))
        .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let cookie = pkce_cookie(&response);
    let body: Value = response.json().await.unwrap();
    let redirect = body["data"]["redirectUrl"].as_str().unwrap();
    assert!(redirect.contains("/apps/g5-test-app/installations/new"));
    (query_param(redirect, "state"), cookie)
}

/// Drive GitHub's setup redirect from a browser holding `cookie` (empty for
/// none).
async fn setup(app: &TestApp, attempt_state: &str, cookie: &str) -> reqwest::Response {
    let mut request = no_redirect_client()
        .get(format!("{}/api/v1/github-app/setup", app.base_url))
        .query(&[
            ("installation_id", INSTALLATION_ID.to_string().as_str()),
            ("setup_action", "install"),
            ("state", attempt_state),
        ]);
    if !cookie.is_empty() {
        request = request.header(reqwest::header::COOKIE, cookie);
    }
    request.send().await.unwrap()
}

async fn callback(app: &TestApp, oauth_state: &str, cookie: &str) -> reqwest::Response {
    no_redirect_client()
        .get(format!("{}/api/v1/github-app/oauth/callback", app.base_url))
        .query(&[("code", "good-code"), ("state", oauth_state)])
        .header(reqwest::header::COOKIE, cookie)
        .send()
        .await
        .unwrap()
}

/// Complete the installation flow and return the callback redirect location.
async fn connect(app: &TestApp, user: &TestUser, org_id: Uuid) -> String {
    let (state, cookie) = start_attempt(app, user, org_id).await;
    let response = setup(app, &state, &cookie).await;
    assert!(response.status().is_redirection(), "{}", response.status());
    let authorize = location(&response);
    assert!(authorize.contains("/login/oauth/authorize"));
    assert!(authorize.contains("code_challenge_method=S256"));
    let oauth_state = query_param(&authorize, "state");
    location(&callback(app, &oauth_state, &cookie).await)
}

async fn binding_status(app: &TestApp, user: &TestUser, org_id: Uuid) -> Value {
    let response = app
        .authenticated_client(user)
        .get(&binding_path(org_id, "/binding"))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    body["data"]["binding"].clone()
}

async fn binding_count(app: &TestApp, org_id: Uuid) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM github_app_installation_bindings WHERE org_id = $1")
        .bind(org_id)
        .fetch_one(&app.db)
        .await
        .unwrap()
}

async fn selection(
    app: &TestApp,
    org_id: Uuid,
    repo: i64,
    capability: GithubAppCapability,
) -> GithubAppSelectionPreflight {
    let mut db = app.scoped_db_for_org(org_id).await;
    let result =
        lookup_repository_selection(&mut db, org_id, u64::try_from(repo).unwrap(), capability)
            .await
            .unwrap();
    db.commit().await.unwrap();
    result
}

#[tokio::test]
async fn connect_cover_resolve_and_disconnect() {
    let github = MockServer::start().await;
    mount_github(&github).await;
    let app = spawn_with_github(&github).await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;

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
        .contains(&json!("github_app")));

    let empty = binding_status(&app, &owner, org.id).await;
    assert!(empty.is_null());

    let redirect = connect(&app, &owner, org.id).await;
    assert_eq!(
        redirect,
        format!(
            "/{}/settings/github-app?githubAppConnection=connected",
            org.slug
        )
    );
    let v1 = binding_status(&app, &owner, org.id).await;
    assert_eq!(v1["version"], 1);
    assert_eq!(v1["disposition"], "active");
    assert_eq!(v1["provenance"], "installation_callback");
    assert_eq!(v1["installationId"], INSTALLATION_ID);
    assert_eq!(v1["accountLogin"], "acme");
    assert_eq!(v1["permissions"]["issues"], "write");
    assert_eq!(v1["coveredRepositories"], json!([]));
    let identities: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM github_app_identities WHERE org_id = $1")
            .bind(org.id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(identities, 1);

    // Empty coverage fails closed.
    assert_eq!(
        selection(&app, org.id, ISSUES_REPO, GithubAppCapability::IssuesWrite).await,
        GithubAppSelectionPreflight::Ineligible(GithubAppIneligibleReason::RepositoryUncovered)
    );

    let client = app.authenticated_client(&owner);
    let inventory: Value = client
        .get(&binding_path(org.id, "/binding/repositories"))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(inventory["data"]["bindingVersion"], 1);
    let names: Vec<&str> = inventory["data"]["repositories"]
        .as_array()
        .unwrap()
        .iter()
        .map(|repo| repo["fullName"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["acme/api", "acme/issues"]);

    // A repository the installation cannot access is rejected.
    let response = client
        .put_json(
            &binding_path(org.id, "/binding/coverage"),
            &json!({"bindingVersion": 1, "githubRepoIds": [ISSUES_REPO, 9999]}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let response = client
        .put_json(
            &binding_path(org.id, "/binding/coverage"),
            &json!({"bindingVersion": 1, "githubRepoIds": [ISSUES_REPO, ISSUES_REPO]}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(binding_count(&app, org.id).await, 1);

    let response = client
        .put_json(
            &binding_path(org.id, "/binding/coverage"),
            &json!({"bindingVersion": 1, "githubRepoIds": [ISSUES_REPO]}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let v2 = binding_status(&app, &owner, org.id).await;
    assert_eq!(v2["version"], 2);
    assert_eq!(v2["provenance"], "coverage_change");
    assert_eq!(
        v2["coveredRepositories"],
        json!([{"githubRepoId": ISSUES_REPO, "fullName": "acme/issues"}])
    );

    // A stale binding version is a conflict, not an overwrite.
    let response = client
        .put_json(
            &binding_path(org.id, "/binding/coverage"),
            &json!({"bindingVersion": 1, "githubRepoIds": [API_REPO]}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);

    // Resolver: covered repo with granted permission is eligible and mints a
    // token narrowed to exactly that repository and permission.
    assert!(matches!(
        selection(&app, org.id, ISSUES_REPO, GithubAppCapability::IssuesWrite).await,
        GithubAppSelectionPreflight::Eligible(ref selected)
            if selected.installation_id == INSTALLATION_ID && selected.binding_version == 2
    ));
    assert_eq!(
        selection(&app, org.id, API_REPO, GithubAppCapability::IssuesRead).await,
        GithubAppSelectionPreflight::Ineligible(GithubAppIneligibleReason::RepositoryUncovered)
    );
    assert_eq!(
        selection(
            &app,
            org.id,
            ISSUES_REPO,
            GithubAppCapability::ContentsWrite
        )
        .await,
        GithubAppSelectionPreflight::Ineligible(GithubAppIneligibleReason::CapabilityMissing {
            required: GithubAppCapability::ContentsWrite
        })
    );
    Mock::given(method("POST"))
        .and(path(format!(
            "/app/installations/{INSTALLATION_ID}/access_tokens"
        )))
        .and(body_json(json!({
            "repository_ids": [ISSUES_REPO],
            "permissions": {"issues": "read"}
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({"token": "ghs_issues_read"})))
        .expect(1)
        .mount(&github)
        .await;
    // The credential owner's sequence: selection preflight, then issuance.
    let github_app = app.state.github_app.as_deref().expect("configured client");
    let GithubAppSelectionPreflight::Eligible(selected) =
        selection(&app, org.id, ISSUES_REPO, GithubAppCapability::IssuesRead).await
    else {
        panic!("covered repository with granted permission is eligible");
    };
    let token = mint_repository_token(github_app, &selected)
        .await
        .expect("narrowed token");
    assert_eq!(token.token, "ghs_issues_read");

    let response = client
        .get(&binding_path(org.id, "/binding/management-url"))
        .await;
    assert_eq!(response.status(), StatusCode::OK);

    // Disconnect appends v3; the resolver then refuses everything.
    let response = client
        .post_json(
            &binding_path(org.id, "/binding/disconnect"),
            &json!({"bindingVersion": 2}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let v3 = binding_status(&app, &owner, org.id).await;
    assert_eq!(v3["version"], 3);
    assert_eq!(v3["disposition"], "disconnected");
    assert_eq!(v3["provenance"], "disconnect");
    assert_eq!(v3["coveredRepositories"], json!([]));
    assert_eq!(
        selection(&app, org.id, ISSUES_REPO, GithubAppCapability::IssuesRead).await,
        GithubAppSelectionPreflight::Ineligible(GithubAppIneligibleReason::BindingInactive)
    );
    let response = client
        .put_json(
            &binding_path(org.id, "/binding/coverage"),
            &json!({"bindingVersion": 3, "githubRepoIds": [ISSUES_REPO]}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);

    // Reconnecting after a disconnect is recorded as a reinstall (v4).
    let redirect = connect(&app, &owner, org.id).await;
    assert!(
        redirect.ends_with("githubAppConnection=connected"),
        "{redirect}"
    );
    let v4 = binding_status(&app, &owner, org.id).await;
    assert_eq!(v4["version"], 4);
    assert_eq!(v4["provenance"], "reinstall");
    assert_eq!(v4["disposition"], "active");

    app.cleanup().await;
}

/// GitHub owns an installation's permissions: an owner accepting a new
/// permission (or GitHub reducing one) is not visible to G5 until it re-reads
/// the installation. Refresh and coverage changes record the current
/// permissions so the resolver's capability check follows GitHub.
#[tokio::test]
async fn refresh_and_coverage_change_record_github_permission_changes() {
    let github = MockServer::start().await;
    mount_github_with_issues(&github, "read").await;
    let app = spawn_with_github(&github).await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let client = app.authenticated_client(&owner);

    connect(&app, &owner, org.id).await;
    let response = client
        .put_json(
            &binding_path(org.id, "/binding/coverage"),
            &json!({"bindingVersion": 1, "githubRepoIds": [ISSUES_REPO]}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let v2 = binding_status(&app, &owner, org.id).await;
    assert_eq!(v2["version"], 2);
    assert_eq!(v2["permissions"]["issues"], "read");

    // The recorded read permission cannot serve a write, and the error says
    // how to pick up a permission granted since.
    assert_eq!(
        selection(&app, org.id, ISSUES_REPO, GithubAppCapability::IssuesWrite).await,
        GithubAppSelectionPreflight::Ineligible(GithubAppIneligibleReason::CapabilityMissing {
            required: GithubAppCapability::IssuesWrite
        })
    );
    let GithubAppSelectionPreflight::Ineligible(error) =
        selection(&app, org.id, ISSUES_REPO, GithubAppCapability::IssuesWrite).await
    else {
        panic!("write is not recorded");
    };
    assert!(
        error
            .to_string()
            .contains("refresh the binding from GitHub"),
        "{error}"
    );

    // Refreshing an unchanged installation adds no snapshot.
    let response = client
        .post_json(
            &binding_path(org.id, "/binding/refresh"),
            &json!({"bindingVersion": 2}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(binding_count(&app, org.id).await, 2);

    // The owner accepts Issues write on GitHub.
    github.reset().await;
    mount_github_with_issues(&github, "write").await;

    // A stale version is a conflict, not an overwrite.
    let response = client
        .post_json(
            &binding_path(org.id, "/binding/refresh"),
            &json!({"bindingVersion": 1}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(binding_count(&app, org.id).await, 2);

    let response = client
        .post_json(
            &binding_path(org.id, "/binding/refresh"),
            &json!({"bindingVersion": 2}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let v3 = binding_status(&app, &owner, org.id).await;
    assert_eq!(v3["version"], 3);
    assert_eq!(v3["provenance"], "installation_refresh");
    assert_eq!(v3["disposition"], "active");
    assert_eq!(v3["permissions"]["issues"], "write");
    assert_eq!(
        v3["coveredRepositories"],
        json!([{"githubRepoId": ISSUES_REPO, "fullName": "acme/issues"}])
    );
    assert!(matches!(
        selection(&app, org.id, ISSUES_REPO, GithubAppCapability::IssuesWrite).await,
        GithubAppSelectionPreflight::Eligible(ref selected) if selected.binding_version == 3
    ));

    // GitHub reduces the permission; saving unchanged coverage records it as
    // a refresh instead of carrying the stale write forward.
    github.reset().await;
    mount_github_with_issues(&github, "read").await;
    let response = client
        .put_json(
            &binding_path(org.id, "/binding/coverage"),
            &json!({"bindingVersion": 3, "githubRepoIds": [ISSUES_REPO]}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let v4 = binding_status(&app, &owner, org.id).await;
    assert_eq!(v4["version"], 4);
    assert_eq!(v4["provenance"], "installation_refresh");
    assert_eq!(v4["permissions"]["issues"], "read");
    assert_eq!(
        selection(&app, org.id, ISSUES_REPO, GithubAppCapability::IssuesWrite).await,
        GithubAppSelectionPreflight::Ineligible(GithubAppIneligibleReason::CapabilityMissing {
            required: GithubAppCapability::IssuesWrite
        })
    );

    // A coverage change records the current permissions with the new set.
    let response = client
        .put_json(
            &binding_path(org.id, "/binding/coverage"),
            &json!({"bindingVersion": 4, "githubRepoIds": [ISSUES_REPO, API_REPO]}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let v5 = binding_status(&app, &owner, org.id).await;
    assert_eq!(v5["version"], 5);
    assert_eq!(v5["provenance"], "coverage_change");
    assert_eq!(v5["permissions"]["issues"], "read");

    // GitHub reporting a different installation is an upstream error, not a
    // re-bind.
    github.reset().await;
    mount_github_installation(&github, INSTALLATION_ID + 1, "write").await;
    let response = client
        .post_json(
            &binding_path(org.id, "/binding/refresh"),
            &json!({"bindingVersion": 5}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(binding_count(&app, org.id).await, 5);

    // A disconnected binding cannot be refreshed back to active.
    let response = client
        .post_json(
            &binding_path(org.id, "/binding/disconnect"),
            &json!({"bindingVersion": 5}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let response = client
        .post_json(
            &binding_path(org.id, "/binding/refresh"),
            &json!({"bindingVersion": 6}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(binding_count(&app, org.id).await, 6);

    app.cleanup().await;
}

#[tokio::test]
async fn github_app_attempt_cookie_supports_plain_http_development() {
    let github = MockServer::start().await;
    mount_github(&github).await;
    let mut config = test_config();
    config.github_app = Some(github_app_config(&github));
    config.auth.cookie_secure = false;
    let app = TestApp::spawn_with_config(config).await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let response = app
        .authenticated_client(&owner)
        .post_json(&binding_path(org.id, "/connection-attempt"), &json!({}))
        .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let cookie = response.headers()["set-cookie"].to_str().unwrap();
    assert!(cookie.starts_with("g5_github_app_pkce="));
    assert!(cookie.contains("Path=/api/v1/github-app;"));
    assert!(cookie.contains("HttpOnly") && cookie.contains("SameSite=Lax"));
    assert!(!cookie.contains("Secure") && !cookie.contains("Domain="));
    let cookie = cookie.split(';').next().unwrap().to_string();
    let body: Value = response.json().await.unwrap();
    let state = query_param(body["data"]["redirectUrl"].as_str().unwrap(), "state");
    let setup_response = setup(&app, &state, &cookie).await;
    let oauth_state = query_param(&location(&setup_response), "state");
    let completed = callback(&app, &oauth_state, &cookie).await;
    assert!(location(&completed).ends_with("githubAppConnection=connected"));
    assert_eq!(binding_count(&app, org.id).await, 1);
    let clear = completed.headers()["set-cookie"].to_str().unwrap();
    assert!(clear.starts_with("g5_github_app_pkce=;"));
    assert!(clear.contains("Path=/api/v1/github-app;") && clear.contains("Max-Age=0"));
    assert!(!clear.contains("Secure"));
    app.cleanup().await;
}

#[tokio::test]
async fn replayed_expired_tampered_and_forged_state_is_rejected() {
    let github = MockServer::start().await;
    mount_github(&github).await;
    let app = spawn_with_github(&github).await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let failed = format!(
        "/{}/settings/github-app?githubAppConnection=failed",
        org.slug
    );

    // Replay: the same OAuth state and cookie cannot bind twice.
    let (state, cookie) = start_attempt(&app, &owner, org.id).await;
    let response = setup(&app, &state, &cookie).await;
    let oauth_state = query_param(&location(&response), "state");
    let first = callback(&app, &oauth_state, &cookie).await;
    assert!(location(&first).ends_with("githubAppConnection=connected"));
    let replay = callback(&app, &oauth_state, &cookie).await;
    assert_eq!(location(&replay), failed);
    // The consumed attempt cannot restart setup either.
    assert_eq!(location(&setup(&app, &state, &cookie).await), failed);
    assert_eq!(binding_count(&app, org.id).await, 1);

    // The setup link only works in the browser that started the attempt: a
    // browser without that attempt's cookie (for example a forwarded link),
    // or holding another attempt's cookie, cannot begin OAuth.
    let (state, cookie) = start_attempt(&app, &owner, org.id).await;
    assert_eq!(location(&setup(&app, &state, "").await), failed);
    let (_, other_cookie) = start_attempt(&app, &owner, org.id).await;
    assert_eq!(location(&setup(&app, &state, &other_cookie).await), failed);
    assert_eq!(binding_count(&app, org.id).await, 1);

    // Missing, tampered or other-attempt PKCE cookie at the OAuth callback.
    let response = setup(&app, &state, &cookie).await;
    let oauth_state = query_param(&location(&response), "state");
    assert!(
        response
            .headers()
            .get(reqwest::header::SET_COOKIE)
            .is_none(),
        "setup must not mint a fresh unbound cookie"
    );
    let before: Value = sqlx::query_scalar(
        "SELECT jsonb_agg(to_jsonb(a) ORDER BY id) FROM github_app_connection_attempts a",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    let provider_calls = github.received_requests().await.unwrap().len();
    for bad_cookie in [
        cookie.trim_start_matches("__Host-").to_string(),
        format!("{cookie}; {other_cookie}"),
        format!("{other_cookie}; {cookie}"),
        format!("{cookie}; {cookie}"),
        format!("__Host-g5_github_app_pkce=; {cookie}"),
    ] {
        assert_eq!(location(&setup(&app, &state, &bad_cookie).await), failed);
        assert_eq!(
            location(&callback(&app, &oauth_state, &bad_cookie).await),
            failed
        );
        assert_eq!(binding_count(&app, org.id).await, 1);
        let after: Value = sqlx::query_scalar(
            "SELECT jsonb_agg(to_jsonb(a) ORDER BY id) FROM github_app_connection_attempts a",
        )
        .fetch_one(&app.db)
        .await
        .unwrap();
        assert_eq!(after, before, "rejected cookies must not consume attempts");
        assert_eq!(
            github.received_requests().await.unwrap().len(),
            provider_calls
        );
    }
    assert_eq!(
        location(&callback(&app, &oauth_state, &other_cookie).await),
        failed
    );
    let tampered_cookie = format!("{cookie}x");
    assert_eq!(
        location(&callback(&app, &oauth_state, &tampered_cookie).await),
        failed
    );
    assert_eq!(
        location(&callback(&app, &oauth_state, "other=1").await),
        failed
    );
    assert_eq!(binding_count(&app, org.id).await, 1);

    // Tampered attempt state: moved to another org, or signature altered.
    let mut parts: Vec<String> = state.split('.').map(ToOwned::to_owned).collect();
    parts[1] = Uuid::new_v4().to_string();
    let moved = parts.join(".");
    assert_eq!(
        location(&setup(&app, &moved, &cookie).await),
        "/?githubAppConnection=failed"
    );
    // Always change the final signature character (it may already be `A`).
    let replacement = if state.ends_with('A') { 'B' } else { 'A' };
    let resigned = format!("{}{replacement}", &state[..state.len() - 1]);
    assert_eq!(
        location(&setup(&app, &resigned, &cookie).await),
        "/?githubAppConnection=failed"
    );
    // A tampered installation id inside the OAuth state.
    let swapped = oauth_state.replace(&format!(".i.{INSTALLATION_ID}."), ".i.556.");
    assert_eq!(
        location(&callback(&app, &swapped, &cookie).await),
        "/?githubAppConnection=failed"
    );

    // Forged state: correctly signed for a real attempt, but not the state
    // whose digest was stored. The digest comparison rejects it.
    let attempt_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM github_app_connection_attempts WHERE org_id = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(org.id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    let cfg = app.state.github_app.as_deref().unwrap().config();
    let forged = new_connection_state(cfg, org.id, attempt_id, i64::MAX / 2);
    assert_eq!(location(&setup(&app, &forged, &cookie).await), failed);
    let forged_oauth = oauth_state_for_installation(cfg, &forged, INSTALLATION_ID).unwrap();
    assert_eq!(
        location(&callback(&app, &forged_oauth, &cookie).await),
        failed
    );

    // Expired in the database while the signed expiry is still in the future.
    let (state, cookie) = start_attempt(&app, &owner, org.id).await;
    let response = setup(&app, &state, &cookie).await;
    let oauth_state = query_param(&location(&response), "state");
    sqlx::query(
        "UPDATE github_app_connection_attempts
         SET created_at = now() - interval '2 hours', expires_at = now() - interval '1 hour'
         WHERE org_id = $1",
    )
    .bind(org.id)
    .execute(&app.db)
    .await
    .unwrap();
    assert_eq!(
        location(&callback(&app, &oauth_state, &cookie).await),
        failed
    );
    assert_eq!(location(&setup(&app, &state, &cookie).await), failed);
    assert_eq!(binding_count(&app, org.id).await, 1);

    app.cleanup().await;
}

/// Reconnect with several accessible installations shows a chooser. The
/// attempt's PKCE cookie survives the chooser and is required (then rotated)
/// by the selection, so a forwarded chooser link cannot complete the attempt.
#[tokio::test]
async fn reconnect_selection_requires_the_attempt_cookie() {
    let github = MockServer::start().await;
    mount_github(&github).await;
    Mock::given(method("GET"))
        .and(path("/user/installations"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "installations": [
                {"id": INSTALLATION_ID, "app_id": 123456, "account": {"id": 44, "login": "acme"}},
                {"id": 777, "app_id": 123456, "account": {"id": 45, "login": "other"}}
            ]
        })))
        .mount(&github)
        .await;
    let app = spawn_with_github(&github).await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let failed = format!(
        "/{}/settings/github-app?githubAppConnection=failed",
        org.slug
    );

    let response = app
        .authenticated_client(&owner)
        .post_json(&binding_path(org.id, "/reconnect-attempt"), &json!({}))
        .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let cookie = pkce_cookie(&response);
    let body: Value = response.json().await.unwrap();
    let discovery_state = query_param(body["data"]["redirectUrl"].as_str().unwrap(), "state");

    let chooser = callback(&app, &discovery_state, &cookie).await;
    assert_eq!(chooser.status(), StatusCode::OK);
    assert!(
        chooser
            .headers()
            .get_all(reqwest::header::SET_COOKIE)
            .iter()
            .all(|value| !value.to_str().unwrap().contains("Max-Age=0")),
        "the chooser must keep the attempt cookie"
    );
    let page = chooser.text().await.unwrap();
    let start = page
        .find("/api/v1/github-app/reconnect/select?state=")
        .unwrap();
    let href = &page[start..start + page[start..].find('"').unwrap()];
    let selection_url = format!("{}{href}", app.base_url);

    let select = |cookie: &str| {
        let mut request = no_redirect_client().get(&selection_url);
        if !cookie.is_empty() {
            request = request.header(reqwest::header::COOKIE, cookie);
        }
        request.send()
    };
    // A browser without the attempt's cookie, or with another attempt's,
    // cannot continue from the chooser.
    assert_eq!(location(&select("").await.unwrap()), failed);
    let (_, other_cookie) = start_attempt(&app, &owner, org.id).await;
    assert_eq!(location(&select(&other_cookie).await.unwrap()), failed);

    for bad_cookie in [
        cookie.trim_start_matches("__Host-").to_string(),
        format!("{cookie}; {other_cookie}"),
        format!("{cookie}; {cookie}"),
        format!("{cookie}; __Host-g5_github_app_pkce="),
    ] {
        let provider_calls = github.received_requests().await.unwrap().len();
        assert_eq!(location(&select(&bad_cookie).await.unwrap()), failed);
        assert_eq!(binding_count(&app, org.id).await, 0);
        assert_eq!(
            github.received_requests().await.unwrap().len(),
            provider_calls
        );
    }

    let selected = select(&cookie).await.unwrap();
    let authorize = location(&selected);
    assert!(authorize.contains("/login/oauth/authorize"), "{authorize}");
    let rotated = pkce_cookie(&selected);
    assert_ne!(rotated, cookie, "the discovery verifier must be rotated");
    let selection_state = query_param(&authorize, "state");
    let done = callback(&app, &selection_state, &rotated).await;
    assert!(
        location(&done).ends_with("githubAppConnection=connected"),
        "{}",
        location(&done)
    );
    let binding = binding_status(&app, &owner, org.id).await;
    assert_eq!(binding["installationId"], INSTALLATION_ID);
    assert_eq!(binding_count(&app, org.id).await, 1);

    app.cleanup().await;
}

#[tokio::test]
async fn non_admins_cannot_manage_or_complete_bindings() {
    let github = MockServer::start().await;
    mount_github(&github).await;
    let app = spawn_with_github(&github).await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;

    // A plain member without the scope is stopped by the route scope.
    let member = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let response = app
        .authenticated_client(&member)
        .post_json(&binding_path(org.id, "/connection-attempt"), &json!({}))
        .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    // A member whose JWT carries OrgAdmin (e.g. from another selected org)
    // is still refused by the path-org role check.
    let scoped_member = TestUser::create_with_scopes(&app, org.id, &[Scope::OrgAdmin]).await;
    for response in [
        app.authenticated_client(&scoped_member)
            .post_json(&binding_path(org.id, "/connection-attempt"), &json!({}))
            .await,
        app.authenticated_client(&scoped_member)
            .get(&binding_path(org.id, "/binding"))
            .await,
        app.authenticated_client(&scoped_member)
            .post_json(
                &binding_path(org.id, "/binding/disconnect"),
                &json!({"bindingVersion": 1}),
            )
            .await,
        app.authenticated_client(&scoped_member)
            .post_json(
                &binding_path(org.id, "/binding/refresh"),
                &json!({"bindingVersion": 1}),
            )
            .await,
    ] {
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    // An admin demoted after starting the attempt cannot complete it.
    TestUser::create_admin(&app, org.id).await;
    let (state, cookie) = start_attempt(&app, &owner, org.id).await;
    let response = setup(&app, &state, &cookie).await;
    let oauth_state = query_param(&location(&response), "state");
    sqlx::query("UPDATE org_memberships SET role = 'member' WHERE org_id = $1 AND user_id = $2")
        .bind(org.id)
        .bind(owner.id)
        .execute(&app.db)
        .await
        .unwrap();
    let response = callback(&app, &oauth_state, &cookie).await;
    assert!(location(&response).ends_with("githubAppConnection=failed"));
    assert_eq!(binding_count(&app, org.id).await, 0);

    app.cleanup().await;
}

#[tokio::test]
async fn bindings_are_isolated_between_orgs_and_append_only() {
    let github = MockServer::start().await;
    mount_github(&github).await;
    let app = spawn_with_github(&github).await;
    let mut owner_a = TestUser::create(&app).await;
    let org_a = TestOrg::create_with_admin(&app, &mut owner_a).await;
    let mut owner_b = TestUser::create(&app).await;
    let org_b = TestOrg::create_with_admin(&app, &mut owner_b).await;

    connect(&app, &owner_a, org_a.id).await;
    let client_a = app.authenticated_client(&owner_a);
    let response = client_a
        .put_json(
            &binding_path(org_a.id, "/binding/coverage"),
            &json!({"bindingVersion": 1, "githubRepoIds": [ISSUES_REPO]}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);

    // Org B's admin cannot see or change org A's binding through the API.
    let client_b = app.authenticated_client(&owner_b);
    assert_eq!(
        client_b
            .get(&binding_path(org_a.id, "/binding"))
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        client_b
            .post_json(
                &binding_path(org_a.id, "/binding/disconnect"),
                &json!({"bindingVersion": 2}),
            )
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    assert!(binding_status(&app, &owner_b, org_b.id).await.is_null());

    // Under RLS, org B's tenant scope sees none of org A's rows.
    let mut db_b = app.scoped_db_for_org(org_b.id).await;
    for table in [
        "github_app_identities",
        "github_app_connection_attempts",
        "github_app_installation_bindings",
        "github_app_installation_binding_repositories",
    ] {
        let visible: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
            .fetch_one(db_b.conn())
            .await
            .unwrap();
        assert_eq!(visible, 0, "{table} leaked across orgs");
    }
    // Org A's attempt cannot be loaded from org B's scope even by exact key.
    let attempt: (Uuid, Vec<u8>) = sqlx::query_as(
        "SELECT id, state_digest FROM github_app_connection_attempts WHERE org_id = $1 LIMIT 1",
    )
    .bind(org_a.id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    let attempt_ref = g5_api::services::github_app::ConnectionAttemptRef {
        org_id: org_a.id,
        attempt_id: attempt.0,
        expires_at: i64::MAX,
        state_digest: attempt.1.try_into().unwrap(),
    };
    assert!(load_pending_attempt(&mut db_b, &attempt_ref)
        .await
        .unwrap()
        .is_none());
    // Org B cannot insert a snapshot for org A.
    let foreign_insert = sqlx::query(
        "INSERT INTO github_app_identities (org_id, kind) VALUES ($1, 'platform_managed')",
    )
    .bind(org_a.id)
    .execute(db_b.conn())
    .await;
    assert!(foreign_insert.is_err());
    drop(db_b);
    assert_eq!(
        selection(&app, org_b.id, ISSUES_REPO, GithubAppCapability::IssuesRead).await,
        GithubAppSelectionPreflight::Ineligible(GithubAppIneligibleReason::NoBinding)
    );

    // The tenant role can neither update nor delete snapshots or coverage.
    for statement in [
        "UPDATE github_app_installation_bindings SET disposition = 'disconnected'",
        "DELETE FROM github_app_installation_bindings",
        "UPDATE github_app_installation_binding_repositories SET full_name = 'x/y'",
        "DELETE FROM github_app_installation_binding_repositories",
        "UPDATE github_app_connection_attempts SET expires_at = now() + interval '1 day'",
        "DELETE FROM github_app_connection_attempts",
        "DELETE FROM github_app_identities",
    ] {
        let mut db_a = app.scoped_db_for_org(org_a.id).await;
        let error = sqlx::query(statement)
            .execute(db_a.conn())
            .await
            .expect_err(statement);
        let code = error
            .as_database_error()
            .and_then(|error| error.code())
            .map(|code| code.to_string());
        assert_eq!(code.as_deref(), Some("42501"), "{statement}: {error}");
    }
    assert_eq!(binding_count(&app, org_a.id).await, 2);

    app.cleanup().await;
}

#[tokio::test]
async fn unconfigured_deployment_does_not_advertise_or_serve_bindings() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let info: Value = app
        .client
        .get(format!("{}/api/v1/instance/info", app.base_url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(!info["data"]["features"]
        .as_array()
        .unwrap()
        .contains(&json!("github_app")));
    let response = app
        .authenticated_client(&owner)
        .post_json(&binding_path(org.id, "/connection-attempt"), &json!({}))
        .await;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let response = no_redirect_client()
        .get(format!(
            "{}/api/v1/github-app/setup?state=x&installation_id=1",
            app.base_url
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(location(&response), "/?githubAppConnection=failed");
    app.cleanup().await;
}

async fn credential_repo(app: &TestApp, org: &TestOrg, url: &str) -> Uuid {
    let workspace = helpers::TestWorkspace::create(app, org).await;
    // Legacy imports used owner/name here; token selection must use verified
    // installation coverage rather than parsing this column as a number.
    sqlx::query("UPDATE repos SET git_url=$2, provider=NULL, provider_repo_id='acme/api', provider_full_name='acme/api' WHERE id=$1")
        .bind(workspace.repo_id).bind(url).execute(&app.db).await.unwrap();
    workspace.repo_id
}

async fn credential_personal_token(app: &TestApp, user: &TestUser) {
    let encrypted = app
        .state
        .scm_token_manager
        .as_ref()
        .unwrap()
        .encrypt_token("personal-test-token")
        .unwrap();
    sqlx::query("INSERT INTO user_scm_connections(id,user_id,provider,provider_user_id,provider_username,access_token_enc,scopes) VALUES($1,$2,'github','test','test',$3,ARRAY['repo'])")
        .bind(Uuid::new_v4()).bind(user.id).bind(encrypted).execute(&app.db).await.unwrap();
}

async fn credential_coverage(app: &TestApp, owner: &TestUser, org: &TestOrg) {
    let response = app
        .authenticated_client(owner)
        .put_json(
            &binding_path(org.id, "/binding/coverage"),
            &json!({"bindingVersion":1,"githubRepoIds":[API_REPO]}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
}

async fn credential_user_grant(app: &TestApp, user: &TestUser, token: &str) {
    let encrypted = app
        .state
        .scm_token_manager
        .as_ref()
        .unwrap()
        .encrypt_token(token)
        .unwrap();
    sqlx::query("INSERT INTO github_app_user_grants(user_id,github_user_id,github_login,access_token_enc) VALUES($1,123,'author',$2)")
        .bind(user.id).bind(encrypted).execute(&app.db).await.unwrap();
}

async fn pr_credential_fixture(github: &MockServer) -> (TestApp, TestUser, TestOrg, Uuid) {
    mount_github(github).await;
    Mock::given(method("GET"))
        .and(path(format!("/app/installations/{INSTALLATION_ID}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id":INSTALLATION_ID,"account":{"id":44,"login":"acme"},
            "permissions":{"metadata":"read","contents":"write","pull_requests":"write"}
        })))
        .with_priority(1)
        .mount(github)
        .await;
    let app = spawn_with_github(github).await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let repo = credential_repo(&app, &org, "https://github.com/acme/api.git").await;
    connect(&app, &owner, org.id).await;
    credential_coverage(&app, &owner, &org).await;
    (app, owner, org, repo)
}

#[tokio::test]
async fn pr_creation_requires_the_initiating_users_app_grant() {
    use g5_api::auth::RepositoryOperation::CreatePullRequest;
    use wiremock::matchers::header;
    let github = MockServer::start().await;
    let (app, owner, org, repo) = pr_credential_fixture(&github).await;
    let credentials = repository_credentials(&app, &github);
    let actor = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesWrite]).await;
    credential_user_grant(&app, &owner, "ghu_other_person").await;
    credential_personal_token(&app, &actor).await;
    let resolve = || {
        credentials.resolve(
            org.id,
            repo,
            actor.id,
            "https://github.com/acme/api.git",
            CreatePullRequest,
        )
    };
    let error = resolve().await.unwrap_err().to_string();
    assert!(error.contains("github_user_connection_required"), "{error}");
    credential_user_grant(&app, &actor, "ghu_initiating_actor").await;
    Mock::given(method("GET"))
        .and(path("/repos/acme/api"))
        .and(header("authorization", "Bearer ghu_initiating_actor"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id":API_REPO})))
        .expect(1)
        .mount(&github)
        .await;
    let auth = resolve().await.unwrap();
    assert!(
        matches!(auth, g5_scm::GitAuth::ScmToken { ref token, .. } if token == "ghu_initiating_actor")
    );
    assert!(!format!("{auth:?}").contains("ghu_initiating_actor"));
    // The creation credential path never mints a repository installation token.
    assert!(!github
        .received_requests()
        .await
        .unwrap()
        .iter()
        .any(|request| {
            request.url.path().ends_with("/access_tokens")
                && request
                    .body_json::<Value>()
                    .unwrap()
                    .get("repository_ids")
                    .is_some()
        }));
    sqlx::query("DELETE FROM org_memberships WHERE org_id=$1 AND user_id=$2")
        .bind(org.id)
        .bind(actor.id)
        .execute(&app.db)
        .await
        .unwrap();
    assert!(resolve()
        .await
        .unwrap_err()
        .to_string()
        .contains("owner_not_org_member"));
    github.verify().await;
    app.cleanup().await;
}

#[tokio::test]
async fn pr_creation_refreshes_the_user_grant_and_refuses_revoked_or_inaccessible_access() {
    use g5_api::auth::RepositoryOperation::CreatePullRequest;
    use wiremock::matchers::header;
    let github = MockServer::start().await;
    let (app, owner, org, repo) = pr_credential_fixture(&github).await;
    let credentials = repository_credentials(&app, &github);
    credential_user_grant(&app, &owner, "ghu_expired").await;
    let refresh = app
        .state
        .scm_token_manager
        .as_ref()
        .unwrap()
        .encrypt_token("ghr_refresh")
        .unwrap();
    sqlx::query("UPDATE github_app_user_grants SET access_token_expires_at=now()-interval '1 minute',refresh_token_enc=$2 WHERE user_id=$1")
        .bind(owner.id).bind(refresh).execute(&app.db).await.unwrap();
    Mock::given(method("POST")).and(path("/login/oauth/access_token"))
        .and(body_string_contains("refresh_token=ghr_refresh"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token":"ghu_renewed", "expires_in":3600,"refresh_token":"ghr_rotated","refresh_token_expires_in":86400
        }))).expect(1).mount(&github).await;
    Mock::given(method("GET"))
        .and(path("/repos/acme/api"))
        .and(header("authorization", "Bearer ghu_renewed"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id":API_REPO})))
        .expect(1)
        .mount(&github)
        .await;
    let resolve = || {
        credentials.resolve(
            org.id,
            repo,
            owner.id,
            "https://github.com/acme/api.git",
            CreatePullRequest,
        )
    };
    assert!(
        matches!(resolve().await.unwrap(), g5_scm::GitAuth::ScmToken { token, .. } if token == "ghu_renewed")
    );
    github.verify().await;
    for (status, expected) in [
        (401, "github_user_connection_required"),
        (403, "cannot access"),
        (404, "cannot access"),
        (503, "HTTP 503"),
    ] {
        github.reset().await;
        Mock::given(method("GET"))
            .and(path("/repos/acme/api"))
            .and(header("authorization", "Bearer ghu_renewed"))
            .respond_with(
                ResponseTemplate::new(status).set_body_string("ghu_renewed must not be echoed"),
            )
            .expect(1)
            .mount(&github)
            .await;
        let error = resolve().await.unwrap_err();
        assert_eq!(
            error.status_code().as_u16(),
            if status == 503 { 503 } else { 409 }
        );
        let error = error.to_string();
        assert!(error.contains(expected), "{status}: {error}");
        assert!(!error.contains("ghu_renewed"));
        github.verify().await;
    }
    // Repository names can be reassigned. A visible repository with a different
    // stable identity must not receive this organization's PR.
    github.reset().await;
    Mock::given(method("GET"))
        .and(path("/repos/acme/api"))
        .and(header("authorization", "Bearer ghu_renewed"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id":API_REPO + 1})))
        .expect(1)
        .mount(&github)
        .await;
    assert!(resolve()
        .await
        .unwrap_err()
        .to_string()
        .contains("identity no longer matches"));
    github.verify().await;
    app.cleanup().await;
}

#[tokio::test]
async fn pr_creation_without_app_binding_keeps_personal_scm_authentication() {
    use g5_api::auth::RepositoryOperation::CreatePullRequest;
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let repo = credential_repo(&app, &org, "https://github.com/acme/api.git").await;
    credential_personal_token(&app, &owner).await;
    let auth = app
        .state
        .repository_credentials
        .resolve(
            org.id,
            repo,
            owner.id,
            "https://github.com/acme/api.git",
            CreatePullRequest,
        )
        .await
        .unwrap();
    assert!(
        matches!(auth, g5_scm::GitAuth::ScmToken { token, .. } if token == "personal-test-token")
    );
    app.cleanup().await;
}

#[tokio::test]
async fn pull_request_creation_attempts_app_credentials_before_personal_tokens() {
    let github = MockServer::start().await;
    mount_github(&github).await;
    Mock::given(method("GET"))
        .and(path(format!("/app/installations/{INSTALLATION_ID}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id":INSTALLATION_ID,"account":{"id":44,"login":"acme"},
            "permissions":{"metadata":"read","contents":"write","pull_requests":"write"}
        })))
        .with_priority(1)
        .mount(&github)
        .await;
    let app = spawn_with_github(&github).await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let source = helpers::TestWorkspace::create(&app, &org).await;
    let target = helpers::TestWorkspace::create(&app, &org).await;
    sqlx::query("UPDATE repos SET git_url='https://github.com/acme/api.git',provider='github',provider_full_name='acme/api' WHERE id=$1")
        .bind(source.repo_id).execute(&app.db).await.unwrap();
    sqlx::query("INSERT INTO workspace_repos(org_id,workspace_id,repo_id,root_path,role) VALUES($1,$2,$3,'shared','code')")
        .bind(org.id).bind(target.id).bind(source.repo_id).execute(&app.db).await.unwrap();
    for (workspace, branch) in [(source.id, "feature"), (target.id, "main")] {
        sqlx::query("INSERT INTO remote_integrations(org_id,workspace_id,repo_id,remote_branch) VALUES($1,$2,$3,$4)")
            .bind(org.id).bind(workspace).bind(source.repo_id).bind(branch)
            .execute(&app.db).await.unwrap();
    }
    connect(&app, &owner, org.id).await;
    credential_coverage(&app, &owner, &org).await;
    credential_personal_token(&app, &owner).await;
    // Exercise the actual PR lifecycle, stopping at token minting so the test
    // never contacts a real Git remote. An App outage must not silently switch
    // to the personal token and bypass the org's selected repository access.
    Mock::given(method("POST"))
        .and(path(format!(
            "/app/installations/{INSTALLATION_ID}/access_tokens"
        )))
        .and(body_json(
            json!({"repository_ids":[API_REPO],"permissions":{"pull_requests":"write","contents":"read"}}),
        ))
        .respond_with(
            ResponseTemplate::new(503).set_body_json(json!({"message":"temporarily unavailable"})),
        )
        .expect(1)
        .mount(&github)
        .await;
    let pr_id = Uuid::new_v4();
    let preflight = json!({
        "sourceKind":"workspace","sourceWorkspaceId":source.id,
        "targetWorkspaceId":target.id,"decision":"fastForwardOrNoOp","canDirectMerge":false,
        "message":"test","preflightFingerprint":"test",
        "repoResults":[{"repoId":source.repo_id,"sourceWorkspaceId":source.id,"targetWorkspaceId":target.id,
            "sourceBranchName":source.name,"targetBranchName":target.name,
            "sourceHead":"a".repeat(40),"targetHead":"b".repeat(40),"outcome":"fastForward","conflictPaths":[],"message":null}]
    });
    sqlx::query("INSERT INTO workspace_pull_requests(id,org_id,source_workspace_id,target_workspace_id,local_merge_status,title,preflight,last_preflight_fingerprint,created_by) VALUES($1,$2,$3,$4,'ready','Test PR',$5,'test',$6)")
        .bind(pr_id).bind(org.id).bind(source.id).bind(target.id).bind(preflight).bind(owner.id)
        .execute(&app.db).await.unwrap();
    let mut db = app.scoped_db_for_org(org.id).await;
    g5_api::services::workspace_pull_request_provider_lifecycle::WorkspacePullRequestProviderLifecycleService::new(
        app.system_pool.clone(), app.repo_storage.clone(), github.uri(),
        "https://github.com".into(), app.base_url.clone(), repository_credentials(&app, &github),
        std::sync::Arc::new(g5_api::services::approval_config_service::ApprovalMergeAdmission::default()),
    )
        .create_or_update_provider_pull_requests(&mut db, pr_id, owner.id, None)
        .await
        .unwrap();
    db.commit().await.unwrap();
    let (status, url, error): (String, Option<String>, Option<String>) = sqlx::query_as(
        "SELECT provider_status,provider_pr_url,last_error FROM workspace_pull_request_provider_repos WHERE pull_request_id=$1",
    ).bind(pr_id).fetch_one(&app.db).await.unwrap();
    assert_eq!(status, "action_required");
    assert!(url.is_none());
    assert!(!error.unwrap().contains("personal-test-token"));
    github.verify().await;
    app.cleanup().await;
}

#[tokio::test]
async fn pull_request_credentials_reject_binding_without_contents_read() {
    use g5_api::auth::RepositoryOperation;
    let github = MockServer::start().await;
    mount_github(&github).await;
    Mock::given(method("GET"))
        .and(path(format!("/app/installations/{INSTALLATION_ID}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id":INSTALLATION_ID,"account":{"id":44,"login":"acme"},
            "permissions":{"metadata":"read","pull_requests":"write"}
        })))
        .with_priority(1)
        .mount(&github)
        .await;
    let app = spawn_with_github(&github).await;
    let credentials = repository_credentials(&app, &github);
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let url = "https://github.com/acme/api.git";
    let repo_id = credential_repo(&app, &org, url).await;
    connect(&app, &owner, org.id).await;
    credential_coverage(&app, &owner, &org).await;
    credential_personal_token(&app, &owner).await;
    // Reads need contents too: review commits are Git objects (issue #486).
    for operation in [
        RepositoryOperation::WritePullRequests,
        RepositoryOperation::ReadPullRequests,
    ] {
        let error = credentials
            .resolve(org.id, repo_id, owner.id, url, operation)
            .await
            .unwrap_err();
        assert!(
            error.to_string().contains("github_app_permission_denied"),
            "{operation:?}: {error}"
        );
    }
    // Fail before minting and never fall back to the personal token.
    let requests = github.received_requests().await.unwrap();
    assert!(!requests.iter().any(|request| {
        request.method == "POST"
            && request.url.path().ends_with("/access_tokens")
            && request
                .body_json::<Value>()
                .unwrap()
                .get("repository_ids")
                .is_some()
    }));
    app.cleanup().await;
}

#[tokio::test]
async fn repository_credentials_use_narrow_app_tokens_without_personal_connection() {
    use g5_api::auth::RepositoryOperation;
    let github = MockServer::start().await;
    mount_github(&github).await;
    Mock::given(method("GET"))
        .and(path(format!("/app/installations/{INSTALLATION_ID}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id":INSTALLATION_ID,"account":{"id":44,"login":"acme"},
            "permissions":{"metadata":"read","contents":"write","pull_requests":"write"}
        })))
        .with_priority(1)
        .mount(&github)
        .await;
    let app = spawn_with_github(&github).await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let url = "https://github.com/acme/api.git";
    let repo_id = credential_repo(&app, &org, url).await;
    connect(&app, &owner, org.id).await;
    credential_coverage(&app, &owner, &org).await;
    let repository_client = repository_client(&github);
    let credentials = repository_credentials(&app, &github);
    // A policy that does not admit this origin never mints or releases App credentials.
    let untrusted = g5_api::auth::RepositoryCredentials::new(
        app.state.system_pool.clone(),
        app.state.tenant_pool.clone(),
        app.state.authz.clone(),
        app.state.scm_token_manager.clone(),
        Some(repository_client),
    )
    .with_origin_policy(
        g5_scm::ScmOriginPolicy::default().with_github_web_url("https://other-enterprise.test/git"),
    );
    let before = github.received_requests().await.unwrap().len();
    for operation in [
        RepositoryOperation::ReadGit,
        RepositoryOperation::WriteGit,
        RepositoryOperation::CreatePullRequest,
    ] {
        assert!(matches!(
            untrusted
                .resolve(org.id, repo_id, owner.id, url, operation)
                .await
                .unwrap(),
            g5_scm::GitAuth::None
        ));
    }
    assert_eq!(github.received_requests().await.unwrap().len(), before);
    for (operation, token, permissions) in [
        (
            RepositoryOperation::ReadGit,
            "installation-contents-read",
            json!({"contents":"read"}),
        ),
        (
            RepositoryOperation::WriteGit,
            "installation-contents-write",
            json!({"contents":"write"}),
        ),
        (
            RepositoryOperation::ReadPullRequests,
            "installation-pull_requests-read",
            json!({"pull_requests":"read","contents":"read"}),
        ),
        (
            RepositoryOperation::WritePullRequests,
            "installation-pull_requests-write",
            json!({"pull_requests":"write","contents":"read"}),
        ),
    ] {
        Mock::given(method("POST")).and(path(format!("/app/installations/{INSTALLATION_ID}/access_tokens")))
            .and(body_json(json!({"repository_ids":[API_REPO],"permissions":permissions})))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({"token":token,"expires_at":(chrono::Utc::now()+chrono::Duration::hours(1)).to_rfc3339()})))
            .expect(1).mount(&github).await;
        for _ in 0..2 {
            let auth = credentials
                .resolve(org.id, repo_id, owner.id, url, operation)
                .await
                .unwrap();
            assert!(
                matches!(auth, g5_scm::GitAuth::ScmToken { token: ref value, provider: g5_scm::ScmProvider::GitHub } if value == token)
            );
            assert!(!format!("{auth:?}").contains(token));
        }
    }
    let automatic = credentials
        .resolve(
            org.id,
            repo_id,
            owner.id,
            url,
            RepositoryOperation::AutoPush,
        )
        .await
        .unwrap();
    assert!(
        matches!(automatic, g5_scm::GitAuth::ScmToken { ref token, .. } if token == "installation-contents-write")
    );
    let merge = credentials
        .resolve(
            org.id,
            repo_id,
            owner.id,
            url,
            RepositoryOperation::MergePullRequest,
        )
        .await
        .unwrap();
    assert!(
        matches!(merge, g5_scm::GitAuth::ScmToken { token: ref value, provider: g5_scm::ScmProvider::GitHub } if value == "installation-contents-write")
    );
    let local = helpers::TestWorkspace::create(&app, &org).await;
    sqlx::query("UPDATE repos SET git_url='g5-local://pending',provider=NULL WHERE id=$1")
        .bind(local.repo_id)
        .execute(&app.db)
        .await
        .unwrap();
    let proposed = credentials
        .resolve_adoption(
            org.id,
            local.repo_id,
            owner.id,
            "g5-local://pending",
            url,
            RepositoryOperation::WriteGit,
        )
        .await
        .unwrap();
    assert!(
        matches!(proposed, g5_scm::GitAuth::ScmToken { token: ref value, provider: g5_scm::ScmProvider::GitHub } if value == "installation-contents-write")
    );
    let member = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesWrite]).await;
    assert!(credentials
        .resolve_adoption(
            org.id,
            local.repo_id,
            member.id,
            "g5-local://pending",
            url,
            RepositoryOperation::WriteGit
        )
        .await
        .is_err());
    assert!(credentials
        .resolve_adoption(
            org.id,
            repo_id,
            owner.id,
            url,
            "https://github.com/acme/other.git",
            RepositoryOperation::WriteGit
        )
        .await
        .is_err());
    let without_app = g5_api::auth::RepositoryCredentials::new(
        app.state.system_pool.clone(),
        app.state.tenant_pool.clone(),
        app.state.authz.clone(),
        app.state.scm_token_manager.clone(),
        None,
    )
    .with_origin_policy(g5_scm::ScmOriginPolicy::default());
    assert!(without_app
        .resolve(org.id, repo_id, owner.id, url, RepositoryOperation::ReadGit)
        .await
        .unwrap_err()
        .to_string()
        .contains("github_app_unavailable"));
    let personal_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM user_scm_connections WHERE user_id=$1")
            .bind(owner.id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(personal_count, 0);
    // Cached tokens never bypass current G5 authorization, captured URL, or RLS.
    assert!(credentials
        .resolve(
            org.id,
            repo_id,
            owner.id,
            "https://github.com/acme/other.git",
            RepositoryOperation::ReadGit
        )
        .await
        .is_err());
    let outsider = TestUser::create(&app).await;
    assert!(credentials
        .resolve(
            org.id,
            repo_id,
            outsider.id,
            url,
            RepositoryOperation::ReadGit
        )
        .await
        .is_err());
    let foreign_org = TestOrg::create_with_admin(&app, &mut owner).await;
    assert!(credentials
        .resolve(
            foreign_org.id,
            repo_id,
            owner.id,
            url,
            RepositoryOperation::ReadGit
        )
        .await
        .is_err());
    let successor = TestUser::create_admin(&app, org.id).await;
    sqlx::query("UPDATE workspaces SET owner_user_id=$2 WHERE org_id=$1 AND owner_user_id=$3")
        .bind(org.id)
        .bind(successor.id)
        .bind(owner.id)
        .execute(&app.db)
        .await
        .unwrap();
    sqlx::query("DELETE FROM org_memberships WHERE org_id=$1 AND user_id=$2")
        .bind(org.id)
        .bind(owner.id)
        .execute(&app.db)
        .await
        .unwrap();
    assert!(credentials
        .resolve(org.id, repo_id, owner.id, url, RepositoryOperation::ReadGit)
        .await
        .is_err());
    github.verify().await;
    app.cleanup().await;
}

#[tokio::test]
async fn repository_credentials_never_fall_back_after_app_denial() {
    use g5_api::auth::RepositoryOperation;
    let github = MockServer::start().await;
    mount_github(&github).await; // Issues-only installation: no Git/PR permission.
    let app = spawn_with_github(&github).await;
    let credentials = repository_credentials(&app, &github);
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let url = "https://github.com/acme/api.git";
    let repo_id = credential_repo(&app, &org, url).await;
    credential_personal_token(&app, &owner).await;
    // The absence of an App binding explicitly selects personal credentials.
    let auth = credentials
        .resolve(org.id, repo_id, owner.id, url, RepositoryOperation::ReadGit)
        .await
        .unwrap();
    assert!(
        matches!(auth, g5_scm::GitAuth::ScmToken { ref token, provider: g5_scm::ScmProvider::GitHub } if token == "personal-test-token")
    );
    let personal_push = credentials
        .resolve(
            org.id,
            repo_id,
            owner.id,
            url,
            RepositoryOperation::WriteGit,
        )
        .await
        .unwrap();
    assert!(
        matches!(personal_push, g5_scm::GitAuth::ScmToken { ref token, .. } if token == "personal-test-token")
    );
    let auto_error = credentials
        .resolve(
            org.id,
            repo_id,
            owner.id,
            url,
            RepositoryOperation::AutoPush,
        )
        .await
        .unwrap_err();
    assert!(auto_error
        .to_string()
        .contains("Automatic push requires a GitHub App"));
    connect(&app, &owner, org.id).await;
    let uncovered = credentials
        .resolve(org.id, repo_id, owner.id, url, RepositoryOperation::ReadGit)
        .await
        .unwrap_err();
    assert!(uncovered.to_string().contains("repository_uncovered"));
    credential_coverage(&app, &owner, &org).await;
    for op in [
        RepositoryOperation::ReadGit,
        RepositoryOperation::WriteGit,
        RepositoryOperation::AutoPush,
        RepositoryOperation::ReadPullRequests,
        RepositoryOperation::WritePullRequests,
        RepositoryOperation::CreatePullRequest,
        RepositoryOperation::MergePullRequest,
    ] {
        let error = credentials
            .resolve(org.id, repo_id, owner.id, url, op)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("permission_denied"), "{error}");
    }
    let spoof = "https://github.com.evil.test/acme/api.git";
    sqlx::query("UPDATE repos SET git_url=$2,provider='github' WHERE id=$1")
        .bind(repo_id)
        .bind(spoof)
        .execute(&app.db)
        .await
        .unwrap();
    let auth = credentials
        .resolve(
            org.id,
            repo_id,
            owner.id,
            spoof,
            RepositoryOperation::ReadGit,
        )
        .await
        .unwrap();
    assert!(matches!(auth, g5_scm::GitAuth::None));
    sqlx::query("UPDATE repos SET git_url=$2 WHERE id=$1")
        .bind(repo_id)
        .bind(url)
        .execute(&app.db)
        .await
        .unwrap();
    let response = app
        .authenticated_client(&owner)
        .post_json(
            &binding_path(org.id, "/binding/disconnect"),
            &json!({"bindingVersion":2}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let error = credentials
        .resolve(org.id, repo_id, owner.id, url, RepositoryOperation::ReadGit)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("binding_inactive"));
    app.cleanup().await;
}

#[tokio::test]
async fn background_sync_retries_app_outages_without_an_authentication_pause() {
    use g5_api::services::remote_integrations::{
        run_branch_integration_worker, BranchIntegrationWorkerDeps,
    };
    use tokio_util::sync::CancellationToken;
    let github = MockServer::start().await;
    mount_github(&github).await;
    Mock::given(method("GET"))
        .and(path(format!("/app/installations/{INSTALLATION_ID}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id":INSTALLATION_ID,"account":{"id":44,"login":"acme"},
            "permissions":{"metadata":"read","contents":"write","pull_requests":"write"}
        })))
        .with_priority(1)
        .mount(&github)
        .await;
    let app = spawn_with_github(&github).await;
    let credentials = repository_credentials(&app, &github);
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = helpers::TestWorkspace::create(&app, &org).await;
    sqlx::query("UPDATE repos SET git_url='https://github.com/acme/api.git',provider='github',provider_full_name='acme/api' WHERE id=$1")
        .bind(workspace.repo_id).execute(&app.db).await.unwrap();
    sqlx::query("UPDATE workspaces SET owner_user_id=$2,push_mode='manual' WHERE id=$1")
        .bind(workspace.id)
        .bind(owner.id)
        .execute(&app.db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO remote_integrations(org_id,workspace_id,repo_id,remote_branch,auto_pull_enabled) VALUES($1,$2,$3,'main',true)")
        .bind(org.id).bind(workspace.id).bind(workspace.repo_id).execute(&app.db).await.unwrap();
    connect(&app, &owner, org.id).await;
    credential_coverage(&app, &owner, &org).await;
    credential_personal_token(&app, &owner).await;
    Mock::given(method("POST"))
        .and(path(format!(
            "/app/installations/{INSTALLATION_ID}/access_tokens"
        )))
        .and(body_json(
            json!({"repository_ids":[API_REPO],"permissions":{"contents":"read"}}),
        ))
        .respond_with(
            ResponseTemplate::new(503).set_body_json(json!({"message":"temporary outage"})),
        )
        .expect(2)
        .mount(&github)
        .await;
    for expected in ["fetch_failed", "fetch_failed", "auth_required"] {
        if expected == "auth_required" {
            sqlx::query("UPDATE github_app_installation_bindings SET disposition='disconnected' WHERE org_id=$1")
                .bind(org.id).execute(&app.db).await.unwrap();
        }
        sqlx::query("INSERT INTO remote_integration_queue(org_id,workspace_id,repo_id,trigger_kind) VALUES($1,$2,$3,'background_inbound')")
            .bind(org.id).bind(workspace.id).bind(workspace.repo_id).execute(&app.db).await.unwrap();
        let deps = BranchIntegrationWorkerDeps {
            landed_commit_notifier: std::sync::Arc::new(
                g5_api::services::review_source_notifications::ReviewSourceNotifier::from_state(
                    &app.state,
                ),
            ),
            system_pool: app.system_pool.clone(),
            tenant_pool: app.state.tenant_pool.clone(),
            authz: app.state.authz.clone(),
            repo_storage: app.state.repo_storage.clone(),
            repository_credentials: credentials.clone(),
            event_bus: app.state.event_bus.clone(),
        };
        let cancel = CancellationToken::new();
        let worker = tokio::spawn(run_branch_integration_worker(deps, cancel.clone()));
        let drained = tokio::time::timeout(std::time::Duration::from_secs(20), async {
            loop {
                let queued: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM remote_integration_queue WHERE workspace_id=$1 AND repo_id=$2)")
                    .bind(workspace.id).bind(workspace.repo_id).fetch_one(&app.db).await.unwrap();
                if !queued { break; }
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        }).await;
        cancel.cancel();
        worker.await.unwrap();
        drained.unwrap();
        let (status, detail): (String, Option<String>) = sqlx::query_as("SELECT status,error_detail FROM remote_sync_runs WHERE workspace_id=$1 AND repo_id=$2 ORDER BY started_at DESC LIMIT 1")
            .bind(workspace.id).bind(workspace.repo_id).fetch_one(&app.db).await.unwrap();
        assert_eq!(status, expected, "{detail:?}");
        let pause: Option<String> = sqlx::query_scalar("SELECT sync_paused_reason FROM remote_integrations WHERE workspace_id=$1 AND repo_id=$2")
            .bind(workspace.id).bind(workspace.repo_id).fetch_one(&app.db).await.unwrap();
        assert_eq!(
            pause.as_deref(),
            (expected == "auth_required").then_some("auth_required")
        );
        assert!(!detail.unwrap_or_default().contains("personal-test-token"));
    }
    github.verify().await;
    app.cleanup().await;
}
