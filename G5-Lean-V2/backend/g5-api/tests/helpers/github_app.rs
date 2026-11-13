//! Shared GitHub App configuration for mocked provider tests.

use std::sync::{Arc, Mutex};

use wiremock::MockServer;

use super::github_key::TEST_GITHUB_APP_PRIVATE_KEY;

pub fn github_app_config(github: &MockServer) -> g5_api::config::GithubAppConfig {
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

/// A mocked organization GitHub App covering one repository, for tests that
/// must run the production App credential path (`Require organization
/// credentials for automatic publication`) without GitHub. The App API is a
/// mock that mints installation tokens and records what was requested; Git
/// transport for the repository's canonical GitHub URL is redirected to a
/// local bare repository, while credential selection still sees that URL.
pub struct MockedGithubApp {
    pub server: MockServer,
    /// What GitHub reports the installation as granted; read by binding
    /// refresh. [`Self::bind`] records the same initial snapshot.
    installation_permissions: Arc<Mutex<serde_json::Value>>,
    /// The remote [`Self::enforce_workflow_permission`] models, if any.
    enforced_remote: Arc<Mutex<Option<std::path::PathBuf>>>,
    /// Unique per fixture: the transport redirect is process-global.
    pub full_name: String,
    pub git_url: String,
    pub github_repo_id: i64,
    pub installation_id: i64,
}

impl MockedGithubApp {
    /// Start the mock App API. Call before spawning the app, then pass the
    /// config through [`Self::configure`].
    pub async fn start() -> Self {
        use wiremock::matchers::{method, path_regex};
        use wiremock::{Mock, ResponseTemplate};
        let server = MockServer::start().await;
        let installation_id = 555;
        let enforced_remote = Arc::new(Mutex::new(None::<std::path::PathBuf>));
        let remote = enforced_remote.clone();
        Mock::given(method("POST"))
            .and(path_regex(format!(
                "^/app/installations/{installation_id}/access_tokens$"
            )))
            .respond_with(move |request: &wiremock::Request| {
                // Only contents-write tokens push; record the newest one's
                // permissions as the credential the modeled remote sees.
                let permissions = serde_json::from_slice::<serde_json::Value>(&request.body)
                    .map(|body| body["permissions"].clone())
                    .unwrap_or_default();
                if let Some(remote) = remote.lock().unwrap().as_ref() {
                    if permissions["contents"] == "write" {
                        std::fs::write(
                            remote.join(g5_scm::native::TEST_TOKEN_PERMISSIONS_FILE),
                            permissions.to_string(),
                        )
                        .unwrap();
                    }
                }
                ResponseTemplate::new(201).set_body_json(serde_json::json!({
                    "token": "ghs_mocked_installation_token",
                    "expires_at": (chrono::Utc::now() + chrono::Duration::hours(1)).to_rfc3339(),
                }))
            })
            .mount(&server)
            .await;
        let installation_permissions = Arc::new(Mutex::new(
            serde_json::json!({"metadata":"read","contents":"write","pull_requests":"write"}),
        ));
        let granted = installation_permissions.clone();
        Mock::given(method("GET"))
            .and(path_regex(format!(
                "^/app/installations/{installation_id}$"
            )))
            .respond_with(move |_: &wiremock::Request| {
                ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "id": installation_id,
                    "account": {"id": 44, "login": "acme"},
                    "permissions": granted.lock().unwrap().clone(),
                }))
            })
            .mount(&server)
            .await;
        let full_name = format!("acme/rt-{}", uuid::Uuid::new_v4().simple());
        Self {
            installation_permissions,
            enforced_remote,
            git_url: format!("https://github.com/{full_name}.git"),
            full_name,
            server,
            github_repo_id: 1002,
            installation_id,
        }
    }

    /// Point the app's GitHub App and API configuration at the mock.
    pub fn configure(&self, config: &mut g5_api::config::AppConfig) {
        let mut app_config = github_app_config(&self.server);
        app_config.web_base_url = "https://github.com".into();
        config.github_app = Some(app_config);
        config.auth.scm_github_api_base_url = self.server.uri();
    }

    /// Install the App for `org_id` with contents write on `repo_id`, make
    /// the repository a GitHub repository with this fixture's URL, and route
    /// its transport to `local_bare`.
    pub async fn bind(
        &self,
        db: &sqlx::PgPool,
        org_id: uuid::Uuid,
        created_by: uuid::Uuid,
        repo_id: uuid::Uuid,
        local_bare: &std::path::Path,
    ) {
        let identity = uuid::Uuid::new_v4();
        let binding = uuid::Uuid::new_v4();
        let permissions = self.installation_permissions.lock().unwrap().clone();
        sqlx::query(
            "INSERT INTO github_app_identities(id,org_id,kind) VALUES($1,$2,'platform_managed')",
        )
        .bind(identity)
        .bind(org_id)
        .execute(db)
        .await
        .expect("App identity");
        sqlx::query("INSERT INTO github_app_installation_bindings(id,org_id,app_identity_id,version,disposition,installation_id,account_id,account_login,permissions,provenance,created_by) VALUES($1,$2,$3,1,'active',$4,44,'acme',$5,'coverage_change',$6)")
            .bind(binding).bind(org_id).bind(identity).bind(self.installation_id)
            .bind(permissions)
            .bind(created_by).execute(db).await.expect("App installation binding");
        sqlx::query("INSERT INTO github_app_installation_binding_repositories(org_id,binding_id,github_repo_id,full_name) VALUES($1,$2,$3,$4)")
            .bind(org_id).bind(binding).bind(self.github_repo_id).bind(&self.full_name)
            .execute(db).await.expect("App repository coverage");
        sqlx::query(
            "UPDATE repos SET git_url=$2, provider='github', provider_full_name=$3 WHERE id=$1",
        )
        .bind(repo_id)
        .bind(&self.git_url)
        .bind(&self.full_name)
        .execute(db)
        .await
        .expect("GitHub repository identity");
        g5_scm::native::redirect_transport_for_tests(&self.git_url, local_bare);
    }

    /// Change what GitHub reports the installation as granted, as when an
    /// installation owner approves new App permissions. G5 records it only
    /// when the binding is refreshed.
    pub fn grant_on_github(&self, permissions: serde_json::Value) {
        *self.installation_permissions.lock().unwrap() = permissions;
    }

    /// Make `local_bare` refuse workflow-file changes the way GitHub does,
    /// judging each push by the permissions of the newest contents-write
    /// installation token minted (none yet: contents only).
    pub fn enforce_workflow_permission(&self, local_bare: &std::path::Path) {
        g5_scm::native::emulate_github_workflow_permission_for_tests(local_bare);
        *self.enforced_remote.lock().unwrap() = Some(local_bare.to_path_buf());
    }

    /// The `permissions` of every installation token minted so far, in order.
    pub async fn minted_permissions(&self) -> Vec<serde_json::Value> {
        self.server
            .received_requests()
            .await
            .unwrap_or_default()
            .iter()
            .filter(|request| request.url.path().ends_with("/access_tokens"))
            .filter_map(|request| {
                serde_json::from_slice::<serde_json::Value>(&request.body)
                    .ok()
                    .map(|body| body["permissions"].clone())
            })
            .collect()
    }
}
