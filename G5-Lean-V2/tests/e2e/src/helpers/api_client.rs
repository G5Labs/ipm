//! TestApiClient — typed HTTP client for the G5 API in E2E tests.
//!
//! Wraps reqwest with convenience methods for common API operations,
//! including a generic `poll_until` helper for waiting on async state changes.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

/// Default rust-e2e HTTP request timeout.
///
/// Issue #2475: under shared Cloud Build load, `g5-api-re` can finish a
/// healthy request in 45–50s after this default has already fired. CI
/// rust-e2e sets `G5_E2E_HTTP_TIMEOUT_SECS=90`. Values outside 1..=180
/// are ignored so the timeout cannot become unbounded.
pub const DEFAULT_HTTP_TIMEOUT_SECS: u64 = 45;
pub const MAX_HTTP_TIMEOUT_SECS: u64 = 180;

pub fn http_timeout_secs(raw: Option<&str>) -> u64 {
    raw.and_then(|value| value.parse::<u64>().ok())
        .filter(|&secs| (1..=MAX_HTTP_TIMEOUT_SECS).contains(&secs))
        .unwrap_or(DEFAULT_HTTP_TIMEOUT_SECS)
}

pub fn http_timeout() -> Duration {
    Duration::from_secs(http_timeout_secs(
        std::env::var("G5_E2E_HTTP_TIMEOUT_SECS").ok().as_deref(),
    ))
}

/// A run-level suffix appended to generated test names so that tests are
/// idempotent even when the DB is not wiped between runs. Helpers add a
/// per-entity UUID fragment for same-process libtest parallelism.
static RUN_SUFFIX: LazyLock<String> = LazyLock::new(|| {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    format!("{:x}", ms)
});

fn unique_test_name(prefix: &str) -> String {
    let id = Uuid::new_v4().simple().to_string();
    format!("{}-{}-{}", prefix, *RUN_SUFFIX, &id[..8])
}

#[test]
fn http_timeout_defaults_and_rejects_unbounded() {
    assert_eq!(http_timeout_secs(None), DEFAULT_HTTP_TIMEOUT_SECS);
    assert_eq!(http_timeout_secs(Some("90")), 90);
    assert_eq!(http_timeout_secs(Some("0")), DEFAULT_HTTP_TIMEOUT_SECS);
    assert_eq!(http_timeout_secs(Some("181")), DEFAULT_HTTP_TIMEOUT_SECS);
    assert_eq!(
        http_timeout_secs(Some("not-a-number")),
        DEFAULT_HTTP_TIMEOUT_SECS
    );
}

/// Read the credential issued by our OAuth callback, ignoring legacy expiry.
/// The API's internal URL can be HTTP even when its browser cookies are secure.
fn issued_refresh_token(
    headers: &reqwest::header::HeaderMap,
    base_name: &str,
) -> anyhow::Result<String> {
    let protected_name = if base_name.starts_with("__Host-") {
        base_name.to_owned()
    } else {
        format!("__Host-{base_name}")
    };
    let mut tokens = headers
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .filter_map(|header| header.to_str().ok())
        .filter_map(|cookie| cookie.split(';').next()?.split_once('='))
        .filter(|(name, token)| {
            (*name == base_name || *name == protected_name) && !token.is_empty()
        })
        .map(|(_, token)| token);
    let token = tokens.next().ok_or_else(|| {
        anyhow::anyhow!("No issued refresh credential on OAuth callback response")
    })?;
    anyhow::ensure!(
        tokens.next().is_none(),
        "Multiple issued refresh credentials"
    );
    Ok(token.to_owned())
}

#[test]
fn oauth_callback_refresh_credential_ignores_expiry_in_either_transport_mode() {
    use reqwest::header::{HeaderMap, SET_COOKIE};

    for base_name in ["g5_refresh", "preview_refresh", "__Host-preview_refresh"] {
        let protected_name = if base_name.starts_with("__Host-") {
            base_name.to_owned()
        } else {
            format!("__Host-{base_name}")
        };
        let mut headers = HeaderMap::new();
        headers.append(
            SET_COOKIE,
            format!("{base_name}=; Max-Age=0").parse().unwrap(),
        );
        headers.append(
            SET_COOKIE,
            format!("{protected_name}=fresh; Secure; HttpOnly; Path=/")
                .parse()
                .unwrap(),
        );
        assert_eq!(issued_refresh_token(&headers, base_name).unwrap(), "fresh");
    }
    let mut headers = HeaderMap::new();
    headers.append(
        SET_COOKIE,
        "g5_refresh=dev; Path=/api/v1/auth".parse().unwrap(),
    );
    assert_eq!(issued_refresh_token(&headers, "g5_refresh").unwrap(), "dev");
    headers.insert(SET_COOKIE, "g5_refresh=; Max-Age=0".parse().unwrap());
    assert!(issued_refresh_token(&headers, "g5_refresh").is_err());
}

/// Base URL for the G5 API, configurable via environment variable.
pub fn api_base_url() -> String {
    resolve_runtime_url(
        &["G5_E2E_API_URL", "E2E_API_URL", "G5_API_URL"],
        "http://g5-api:7500",
        "http://localhost:7501",
    )
}

/// Restart only the API assigned to the serialized value-loop lane. This
/// deliberately refuses arbitrary container names so a recovery scenario
/// cannot interrupt another E2E lane's API.
pub async fn restart_api_container(api: &TestApiClient) -> anyhow::Result<()> {
    let container = std::env::var("G5_E2E_API_RESTART_CONTAINER").map_err(|_| {
        anyhow::anyhow!("recovery tests require G5_E2E_API_RESTART_CONTAINER for the isolated API")
    })?;
    anyhow::ensure!(
        matches!(container.as_str(), "g5-api" | "g5-api-re" | "g5-dev-api"),
        "refusing to restart an unexpected API container: {container}"
    );
    let target = reqwest::Url::parse(api.base_url())?;
    anyhow::ensure!(
        target.host_str() == Some(container.as_str()),
        "restart container {container} does not serve API {}",
        api.base_url()
    );
    let mut command = tokio::process::Command::new("docker");
    command
        .args(["restart", "--time", "10", &container])
        .kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(90), command.output())
        .await
        .map_err(|_| anyhow::anyhow!("API container restart timed out: {container}"))??;
    anyhow::ensure!(
        output.status.success(),
        "API container restart failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    tokio::time::timeout(Duration::from_secs(90), async {
        loop {
            if api.healthz().await.unwrap_or(false) {
                return Ok::<(), anyhow::Error>(());
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    })
    .await
    .map_err(|_| anyhow::anyhow!("restarted API {container} did not become healthy"))??;
    Ok(())
}

fn oauth_stub_base_url() -> String {
    resolve_runtime_url(
        &["G5_E2E_OAUTH_STUB_URL", "E2E_OAUTH_STUB_URL"],
        "http://oauth-stub:9876",
        "http://localhost:9876",
    )
}

fn oauth_stub_identity_cookie_value_for(
    id: &str,
    login: &str,
    email: &str,
    name: &str,
    avatar_url: &str,
) -> String {
    let identity = serde_json::json!({
        "id": id,
        "login": login,
        "email": email,
        "name": name,
        "avatar_url": avatar_url,
    });
    URL_SAFE_NO_PAD.encode(identity.to_string())
}

fn oauth_stub_identity_cookie_value() -> String {
    let id = std::env::var("G5_E2E_OAUTH_USER_ID").unwrap_or_else(|_| "100001".to_string());
    let login =
        std::env::var("G5_E2E_OAUTH_USER_LOGIN").unwrap_or_else(|_| "e2e-test-user".to_string());
    let email =
        std::env::var("G5_E2E_TEST_EMAIL").unwrap_or_else(|_| "e2e-test@g5.dev".to_string());
    let name =
        std::env::var("G5_E2E_OAUTH_USER_NAME").unwrap_or_else(|_| "E2E Test User".to_string());
    let avatar_url = std::env::var("G5_E2E_OAUTH_USER_AVATAR")
        .unwrap_or_else(|_| "https://i.pravatar.cc/150?u=e2e".to_string());
    oauth_stub_identity_cookie_value_for(&id, &login, &email, &name, &avatar_url)
}

fn resolve_runtime_url(keys: &[&str], container_default: &str, host_default: &str) -> String {
    keys.iter()
        .find_map(|key| match std::env::var(key) {
            Ok(value) if !value.trim().is_empty() => Some(value),
            _ => None,
        })
        .unwrap_or_else(|| {
            if running_in_container() {
                container_default.to_string()
            } else {
                host_default.to_string()
            }
        })
}

fn running_in_container() -> bool {
    std::path::Path::new("/.dockerenv").exists()
        || std::fs::read_to_string("/proc/1/cgroup")
            .map(|cgroup| cgroup.contains("docker") || cgroup.contains("containerd"))
            .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// API Response Envelope — all /api/v1/ responses are wrapped in this
// ---------------------------------------------------------------------------

/// Generic response envelope matching the g5-api convention:
/// `{ "data": <T>, "meta"?: { ... } }`
#[derive(Debug, Deserialize)]
pub struct ResponseEnvelope<T> {
    pub data: T,
}

/// HTTP client for the G5 API with typed request/response methods.
#[derive(Clone)]
pub struct TestApiClient {
    client: Client,
    base_url: String,
    auth_token: Arc<Mutex<Option<String>>>,
    refresh_token: Arc<Mutex<Option<String>>>,
    active_org_id: Arc<Mutex<Option<Uuid>>>,
}

// --- Request/Response DTOs ---

/// Request body for `POST /api/v1/platform/orgs`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatePlatformOrgRequest {
    pub name: String,
    pub slug: String,
    pub admin_email: String,
}

/// Envelope contents from `POST /api/v1/platform/orgs`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreatePlatformOrgResponseData {
    org: OrgResponse,
}

/// A created organization with its cascade-created Base workspace and docs
/// repository.
#[derive(Debug)]
pub struct CreateOrgResponseData {
    pub org: OrgResponse,
    pub base_workspace: WorkspaceResponse,
    pub docs_repo: Option<DocsRepoResponse>,
}

/// The dev/E2E fixture operator persona. Its test token provisions the
/// operator organization, so it may create organizations through the
/// platform API (`g5.organization.platform-operator.create-org`).
pub const E2E_OPERATOR_EMAIL: &str = "e2e-operator@g5labs.ai";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocsRepoResponse {
    pub id: Uuid,
    pub name: String,
    #[serde(default)]
    pub slug: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OrgResponse {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
}

/// Response from `GET /api/v1/workspaces/:id/branches`.
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceBranchResponse {
    pub workspace_id: String,
    pub branch_name: String,
    pub head_commit: String,
    pub repo_id: String,
    #[serde(default)]
    pub materialization_status: String,
    #[serde(default)]
    pub repo_name: String,
}

fn format_branch_statuses(branches: &[WorkspaceBranchResponse]) -> String {
    branches
        .iter()
        .map(|branch| {
            format!(
                "{}:{}:{}:{}",
                branch.workspace_id,
                branch.repo_id,
                branch.branch_name,
                branch.materialization_status
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceResponse {
    pub id: Uuid,
    #[serde(default)]
    pub org_id: Option<Uuid>,
    pub name: String,
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpecContextResponse {
    pub component_id: String,
    #[serde(default)]
    pub src_files: Vec<String>,
    #[serde(default)]
    pub parent_spec: Option<String>,
    #[serde(default)]
    pub child_specs: Vec<String>,
    #[serde(default)]
    pub last_uplifted_commit_sha: Option<String>,
    #[serde(default)]
    pub last_downlifted_commit_sha: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskResponse {
    pub id: Uuid,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub task_type: String,
    pub status: String,
    #[serde(default)]
    pub assigned_at: Option<String>,
    #[serde(default)]
    pub completed_at: Option<String>,
    #[serde(default)]
    pub component_id: Option<String>,
    #[serde(default)]
    pub node_level: Option<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub spec_context: Option<SpecContextResponse>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceListItemResponse {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub is_base: bool,
    #[serde(default)]
    pub branches: Vec<WorkspaceBranchResponse>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveResponse {
    pub org_id: String,
    #[serde(default)]
    pub repo_id: Option<String>,
    #[serde(default)]
    pub workspace_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OntologyNodeResponse {
    pub id: Uuid,
    pub repo_id: Uuid,
    pub workspace_id: Uuid,
    #[serde(default)]
    pub component_id: Option<String>,
    #[serde(default)]
    pub node_level: Option<String>,
    #[serde(default)]
    pub parent_id: Option<Uuid>,
    #[serde(default)]
    pub src_files: Vec<String>,
    #[serde(default)]
    pub children: Vec<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineEntry {
    /// Durable identity of this persisted timeline record. Value-loop tests use
    /// it to distinguish separate host launch observations for one logical task.
    #[serde(default)]
    pub id: String,
    /// Server sends `entryType` (from `TimelineEntryDTO.entry_type`).
    /// Accept both `eventType` (legacy) and `entryType` (actual server key).
    #[serde(default, alias = "entryType")]
    pub event_type: String,
    #[serde(default)]
    pub content: String,
    /// Server sends `metadata`; keep Rust field name `data` for test compat.
    #[serde(default, rename = "metadata")]
    pub data: serde_json::Value,
}

impl TestApiClient {
    /// Create a new client pointing at the given base URL.
    ///
    /// Public `/api/v1/*` calls authenticate with the current user JWT.
    pub fn new(base_url: &str) -> Self {
        // Fail fast in CI when API calls stall; avoids indefinite hangs.
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(http_timeout())
            .pool_idle_timeout(Duration::from_secs(30))
            .build()
            .expect("failed to build reqwest client");
        Self {
            client,
            base_url: base_url.to_string(),
            auth_token: Arc::new(Mutex::new(None)),
            refresh_token: Arc::new(Mutex::new(None)),
            active_org_id: Arc::new(Mutex::new(None)),
        }
    }

    /// Create a client using the default/env-configured base URL.
    pub fn from_env() -> Self {
        Self::new(&api_base_url())
    }

    /// Create an auth client using a test-token JWT.
    ///
    /// Email defaults to `e2e-test@g5.dev` and can be overridden with
    /// `G5_E2E_TEST_EMAIL`.
    ///
    /// When `G5_E2E_AUTH_MODE=oauth`, authenticates via the full OAuth
    /// redirect chain against the local OAuth stub instead.
    pub async fn from_env_authed() -> anyhow::Result<Self> {
        Self::new_authed(&api_base_url()).await
    }

    /// Create an authenticated client with a fresh user identity that cannot
    /// share user-scoped preferences with parallel E2E scenarios.
    ///
    /// The OAuth path supplies the identity per authorization request through
    /// the stub cookie; the test-token path uses the same unique email. This
    /// intentionally avoids process-global environment mutation under
    /// parallel libtest execution.
    pub async fn from_env_authed_isolated(identity_prefix: &str) -> anyhow::Result<Self> {
        Ok(Self::from_env_authed_isolated_with_email(identity_prefix)
            .await?
            .0)
    }

    /// Create an isolated authenticated client and return the generated email
    /// when a fixture must add that fresh user to an existing organization.
    pub async fn from_env_authed_isolated_with_email(
        identity_prefix: &str,
    ) -> anyhow::Result<(Self, String)> {
        let base_url = api_base_url();
        let client = Self::new(&base_url);
        let identity_uuid = Uuid::new_v4().simple().to_string();
        let suffix = &identity_uuid[..12];
        let normalized_prefix = identity_prefix
            .chars()
            .map(|character| {
                if character.is_ascii_alphanumeric() {
                    character.to_ascii_lowercase()
                } else {
                    '-'
                }
            })
            .collect::<String>();
        let normalized_prefix = normalized_prefix.trim_matches('-');
        let normalized_prefix = if normalized_prefix.is_empty() {
            "isolated"
        } else {
            normalized_prefix
        };
        let login = format!("e2e-{normalized_prefix}-{suffix}");
        let email = format!("{login}@g5.dev");

        let auth_mode = std::env::var("G5_E2E_AUTH_MODE").unwrap_or_default();
        let (access_token, refresh_token) = if auth_mode.eq_ignore_ascii_case("oauth") {
            // Twelve hex digits stay below JavaScript's safe-integer ceiling;
            // the OAuth stub normalizes this string into its GitHub-like
            // numeric `id` response.
            let provider_id = u64::from_str_radix(suffix, 16)?.to_string();
            let avatar_url = format!("https://i.pravatar.cc/150?u={login}");
            let identity_cookie = oauth_stub_identity_cookie_value_for(
                &provider_id,
                &login,
                &email,
                &format!("E2E {normalized_prefix}"),
                &avatar_url,
            );
            client
                .login_via_oauth_with_identity_cookie(&identity_cookie)
                .await?
        } else {
            client.obtain_test_tokens(&email).await?
        };
        client.set_auth_tokens(access_token, refresh_token);
        Ok((client, email))
    }

    /// Create an auth client for a specific base URL.
    ///
    /// Auth mode is selected by `G5_E2E_AUTH_MODE`:
    /// - `"oauth"` — full OAuth redirect flow against the local OAuth stub
    /// - anything else — `POST /api/v1/auth/test-token` (default)
    pub async fn new_authed(base_url: &str) -> anyhow::Result<Self> {
        let auth_mode = std::env::var("G5_E2E_AUTH_MODE").unwrap_or_default();
        if auth_mode.eq_ignore_ascii_case("oauth") {
            Self::new_authed_oauth(base_url).await
        } else {
            Self::new_authed_test_token(base_url).await
        }
    }

    /// Authenticate via `POST /api/v1/auth/test-token`.
    async fn new_authed_test_token(base_url: &str) -> anyhow::Result<Self> {
        let client = Self::new(base_url);
        let email =
            std::env::var("G5_E2E_TEST_EMAIL").unwrap_or_else(|_| "e2e-test@g5.dev".to_string());
        let (access_token, refresh_token) = client.obtain_test_tokens(&email).await?;
        client.set_auth_tokens(access_token, refresh_token);
        Ok(client)
    }

    /// Authenticate via the full OAuth redirect chain.
    ///
    /// Exercises the same flow a browser would:
    /// 1. `GET /api/v1/auth/login/github` → 303 to OAuth stub
    /// 2. `GET <oauth-stub>/login/oauth/authorize` → 302 back with code+state
    /// 3. `GET /api/v1/auth/callback/github?code=...&state=...` → Set-Cookie + 303
    /// 4. `POST /api/v1/auth/refresh` with refresh token → JWT
    ///
    /// Requires `G5_E2E_OAUTH_STUB_URL` (e.g. `http://oauth-stub:9876`).
    async fn new_authed_oauth(base_url: &str) -> anyhow::Result<Self> {
        let client = Self::new(base_url);
        let (access_token, refresh_token) = client.login_via_oauth().await?;
        client.set_auth_tokens(access_token, refresh_token);
        Ok(client)
    }

    /// Create an auth client scoped to a specific org.
    ///
    /// Like [`new_authed`] but immediately calls `switch_active_org` so the
    /// JWT is scoped to `org_id`. Use this inside `poll_until` closures where
    /// the polled resources belong to a non-default org.
    pub async fn new_authed_for_org(base_url: &str, org_id: Uuid) -> anyhow::Result<Self> {
        let client = Self::new_authed(base_url).await?;
        client.switch_active_org(org_id).await?;
        Ok(client)
    }

    fn set_auth_tokens(&self, access_token: String, refresh_token: Option<String>) {
        *self.auth_token.lock().expect("auth_token mutex poisoned") = Some(access_token);
        *self
            .refresh_token
            .lock()
            .expect("refresh_token mutex poisoned") = refresh_token;
    }

    fn auth_header_token(&self) -> Option<String> {
        self.auth_token
            .lock()
            .expect("auth_token mutex poisoned")
            .clone()
    }

    /// Set the auth token for subsequent requests.
    pub fn with_auth(self, token: &str) -> Self {
        // Read refresh_token first, then release the lock before calling
        // set_auth_tokens (which locks the same mutex internally).
        let refresh = self
            .refresh_token
            .lock()
            .expect("refresh_token mutex poisoned")
            .clone();
        self.set_auth_tokens(token.to_string(), refresh);
        self
    }

    /// Replace both access and refresh tokens at once.
    ///
    /// Use when switching to a test-token identity so the refresh token
    /// matches the access token (avoids 403 on `/auth/refresh`).
    pub fn with_auth_tokens(self, access: String, refresh: Option<String>) -> Self {
        self.set_auth_tokens(access, refresh);
        self
    }

    /// Access the underlying reqwest client for custom requests.
    pub fn client(&self) -> &Client {
        &self.client
    }

    /// Get the active org ID, if any.
    pub fn active_org(&self) -> Option<Uuid> {
        *self
            .active_org_id
            .lock()
            .expect("active_org_id mutex poisoned")
    }

    /// Get the base URL.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Get the current Bearer JWT used for public API calls, if configured.
    pub fn auth_token(&self) -> Option<String> {
        self.auth_header_token()
    }

    /// Get the current refresh token for CLI auth-state interoperability tests.
    pub fn refresh_token(&self) -> Option<String> {
        self.refresh_token
            .lock()
            .expect("refresh_token mutex poisoned")
            .clone()
    }

    /// Build an authenticated public GET request.
    /// Only Bearer user JWT is attached.
    pub fn authed_get(&self, url: &str) -> reqwest::RequestBuilder {
        let mut req = self.client.get(url);
        if let Some(token) = self.auth_header_token() {
            req = req.bearer_auth(token);
        }
        req
    }

    /// Build an authenticated public POST request.
    /// Only Bearer user JWT is attached.
    pub fn authed_post(&self, url: &str) -> reqwest::RequestBuilder {
        let mut req = self.client.post(url);
        if let Some(token) = self.auth_header_token() {
            req = req.bearer_auth(token);
        }
        req
    }

    /// Build an authenticated public PATCH request.
    /// Only Bearer user JWT is attached.
    pub fn authed_patch(&self, url: &str) -> reqwest::RequestBuilder {
        let mut req = self.client.patch(url);
        if let Some(token) = self.auth_header_token() {
            req = req.bearer_auth(token);
        }
        req
    }

    /// Build an authenticated public PUT request.
    /// Only Bearer user JWT is attached.
    pub fn authed_put(&self, url: &str) -> reqwest::RequestBuilder {
        let mut req = self.client.put(url);
        if let Some(token) = self.auth_header_token() {
            req = req.bearer_auth(token);
        }
        req
    }

    /// Build an authenticated public DELETE request.
    /// Only Bearer user JWT is attached.
    pub fn authed_delete(&self, url: &str) -> reqwest::RequestBuilder {
        let mut req = self.client.delete(url);
        if let Some(token) = self.auth_header_token() {
            req = req.bearer_auth(token);
        }
        req
    }

    pub async fn switch_active_org(&self, org_id: Uuid) -> anyhow::Result<()> {
        let Some(refresh_token) = self
            .refresh_token
            .lock()
            .expect("refresh_token mutex poisoned")
            .clone()
        else {
            return Ok(());
        };

        let mut req = self
            .client
            .post(format!("{}/api/v1/auth/refresh", self.base_url));
        if let Some(token) = self.auth_header_token() {
            req = req.bearer_auth(token);
        }

        let resp = req
            .json(&serde_json::json!({
                "refreshToken": refresh_token,
                "orgId": org_id.to_string(),
            }))
            .send()
            .await?
            .error_for_status()?;

        let body: serde_json::Value = resp.json().await?;
        let access = body["data"]["accessToken"]
            .as_str()
            .or(body["data"]["jwt"].as_str())
            .or(body["accessToken"].as_str())
            .or(body["jwt"].as_str())
            .ok_or_else(|| anyhow::anyhow!("No access token in refresh response"))?
            .to_string();
        let refreshed = body["data"]["refreshToken"]
            .as_str()
            .or(body["refreshToken"].as_str())
            .map(std::string::ToString::to_string)
            .or(Some(refresh_token));

        self.set_auth_tokens(access, refreshed);
        *self
            .active_org_id
            .lock()
            .expect("active_org_id mutex poisoned") = Some(org_id);
        Ok(())
    }

    // --- Health ---

    /// Check if the API server is healthy via GET /healthz.
    pub async fn healthz(&self) -> anyhow::Result<bool> {
        let resp = self
            .client
            .get(format!("{}/healthz", self.base_url))
            .send()
            .await?;
        Ok(resp.status().is_success())
    }

    // --- Orgs ---

    /// Create an organization administered by this client's user. Self-serve
    /// creation is gone, so the fixture operator persona creates it through
    /// `POST /api/v1/platform/orgs`. Returns the org with its cascade-created
    /// Base workspace and docs repository.
    pub async fn create_org_full(&self, name: &str) -> anyhow::Result<CreateOrgResponseData> {
        let unique_name = unique_test_name(name);
        let me: serde_json::Value = self
            .authed_get(&format!("{}/api/v1/users/me", self.base_url))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let admin_email = me["data"]["email"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("GET /api/v1/users/me returned no email"))?
            .to_string();

        let operator = Self::new(&self.base_url);
        let operator_token = operator.obtain_test_token(E2E_OPERATOR_EMAIL).await?;
        let resp = operator
            .with_auth(&operator_token)
            .authed_post(&format!("{}/api/v1/platform/orgs", self.base_url))
            .json(&CreatePlatformOrgRequest {
                name: unique_name.clone(),
                slug: unique_name,
                admin_email,
            })
            .send()
            .await?
            .error_for_status()?;
        let envelope: ResponseEnvelope<CreatePlatformOrgResponseData> = resp.json().await?;
        let org = envelope.data.org;

        // Align public JWT auth context to the newly created organization.
        self.switch_active_org(org.id).await?;

        // Seed a non-live direct-Anthropic API key so agentic fixture tasks pass
        // the canonical pre-dispatch resolver. ANTHROPIC_AUTH_TOKEN is the
        // OpenRouter bearer-token projection; direct Claude OAuth uses
        // CLAUDE_CODE_OAUTH_TOKEN and must not be simulated by this helper.
        // Mirrors the in-process TestOrg fixture and seed-ci-org.sh.
        self.authed_put(&format!("{}/api/v1/orgs/{}/config", self.base_url, org.id))
            .json(&serde_json::json!({
                "setSecrets": {
                    "ANTHROPIC_API_KEY": "sk-ant-e2e-test-key",
                }
            }))
            .send()
            .await?
            .error_for_status()?;

        let base = self
            .list_workspaces(org.id)
            .await?
            .into_iter()
            .find(|workspace| workspace.is_base || workspace.name == "Base")
            .ok_or_else(|| anyhow::anyhow!("Org {} has no Base workspace", org.id))?;
        let base_workspace = WorkspaceResponse {
            id: base.id.parse()?,
            org_id: Some(org.id),
            name: base.name,
            status: None,
        };
        let docs_repo_id = self.get_docs_repo_id(org.id).await?;
        Ok(CreateOrgResponseData {
            docs_repo: Some(DocsRepoResponse {
                id: docs_repo_id,
                name: "docs".to_string(),
                slug: Some("docs".to_string()),
            }),
            org,
            base_workspace,
        })
    }

    /// Create an organization (convenience — returns just the OrgResponse).
    pub async fn create_org(&self, name: &str) -> anyhow::Result<OrgResponse> {
        Ok(self.create_org_full(name).await?.org)
    }

    /// Use the pre-seeded CI org (slug `g5labs`, ID from `G5D_ORG_ID` env).
    ///
    /// The seed script adds the E2E test user to this organization. This
    /// switches the client's public auth context so scoped routes use the
    /// seeded organization.
    pub async fn use_seeded_org(&self) -> anyhow::Result<OrgResponse> {
        let org_id_str = std::env::var("G5D_ORG_ID").map_err(|_| {
            anyhow::anyhow!("G5D_ORG_ID not set — run seed-ci-org.sh before E2E tests")
        })?;
        let org_id: Uuid = org_id_str
            .parse()
            .map_err(|e| anyhow::anyhow!("G5D_ORG_ID is not a valid UUID: {e}"))?;
        self.switch_active_org(org_id).await?;
        Ok(OrgResponse {
            id: org_id,
            name: "G5 Labs".to_string(),
            slug: "g5labs".to_string(),
        })
    }

    // --- Repos ---

    /// Fetch the org's docs repo ID via `GET /api/v1/orgs/:orgId/repos`.
    ///
    /// Every org is provisioned with a canonical `slug = docs` repo on
    /// creation, so this should always return a valid UUID.
    pub async fn get_docs_repo_id(&self, org_id: Uuid) -> anyhow::Result<Uuid> {
        let resp = self
            .authed_get(&format!("{}/api/v1/orgs/{}/repos", self.base_url, org_id))
            .send()
            .await?
            .error_for_status()?;
        let envelope: ResponseEnvelope<Vec<DocsRepoResponse>> = resp.json().await?;
        let repos = envelope.data;
        let repo = repos
            .iter()
            .find(|r| r.slug.as_deref() == Some("docs"))
            .or_else(|| {
                repos
                    .iter()
                    .all(|r| r.slug.is_none())
                    .then(|| repos.iter().find(|r| r.name == "docs"))
                    .flatten()
            })
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Org {} has no canonical docs repo; repos={:?}",
                    org_id,
                    repos
                )
            })?;
        Ok(repo.id)
    }

    /// Discover the repo id of the selected pre-seeded E2E workspace.
    ///
    /// Phase 4c seeds a dedicated repository for E2E coverage. Late-suite
    /// tests that need stable git transport should prefer this repo over the
    /// shared org docs repo because it is purpose-built for CI and is easier
    /// to keep deterministic across the full matrix.
    pub async fn seeded_test_repo_id(&self, org_id: Uuid) -> anyhow::Result<Uuid> {
        if let Ok(repo_id) = std::env::var("G5_E2E_TEST_REPO_ID") {
            return repo_id
                .parse()
                .map_err(|e| anyhow::anyhow!("G5_E2E_TEST_REPO_ID is not a valid UUID: {e}"));
        }

        let workspace_name = super::env::e2e_workspace_name();
        let workspaces = self.list_workspaces(org_id).await?;
        let workspace = workspaces
            .into_iter()
            .find(|ws| ws.name == workspace_name)
            .ok_or_else(|| {
                anyhow::anyhow!("No workspace named '{workspace_name}' found in org {org_id}")
            })?;
        let workspace_id: Uuid = workspace
            .id
            .parse()
            .map_err(|e| anyhow::anyhow!("Invalid seeded workspace id '{}': {e}", workspace.id))?;
        let branches = self.get_workspace_branches(workspace_id).await?;
        let docs_repo_id = self.get_docs_repo_id(org_id).await.ok();
        let main_branches: Vec<_> = branches
            .iter()
            .filter(|branch| branch.branch_name == "main")
            .collect();
        let candidates: Vec<_> = if main_branches.is_empty() {
            branches.iter().collect()
        } else {
            main_branches
        };
        let branch = candidates
            .iter()
            .copied()
            .find(|branch| {
                let name = branch.repo_name.to_ascii_lowercase();
                (name.contains("code") || name.contains("test-repo")) && !name.contains("doc")
            })
            .or_else(|| {
                candidates.iter().copied().find(|branch| {
                    let name = branch.repo_name.to_ascii_lowercase();
                    let repo_id = Uuid::parse_str(&branch.repo_id).ok();
                    !name.contains("doc")
                        && docs_repo_id.is_none_or(|docs_id| repo_id != Some(docs_id))
                })
            })
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Seeded workspace {workspace_id} has no code repo branch: {branches:?}"
                )
            })?;
        branch
            .repo_id
            .parse()
            .map_err(|e| anyhow::anyhow!("Invalid seeded repo id '{}': {e}", branch.repo_id))
    }

    /// Build an unauthenticated GET request and execute it.
    pub async fn raw_get(&self, url: &str) -> anyhow::Result<reqwest::Response> {
        Ok(self.client.get(url).send().await?)
    }

    // --- Workspaces ---

    /// Create a workspace within an organization.
    ///
    /// Automatically links the org's docs repo and creates a default branch
    /// so that task execution have a valid durable branch mapping.
    pub async fn create_workspace(
        &self,
        org_id: Uuid,
        name: &str,
    ) -> anyhow::Result<WorkspaceResponse> {
        // The public creator is the checked all-member fork service. Select
        // the base that owns the org docs repository and fork every one of its
        // frozen members under the exact same destination name.
        let repo_id = self.get_docs_repo_id(org_id).await?;
        self.create_workspace_with_repo(org_id, name, Some(repo_id))
            .await
    }

    /// Fork an org base workspace whose checked member set contains `repo_id`.
    /// `None` selects the first base workspace. Tests that need a new member
    /// must arrange it on the base before forking; workspace creation cannot
    /// attach repositories through a second creator.
    pub async fn create_workspace_with_repo(
        &self,
        org_id: Uuid,
        name: &str,
        repo_id: Option<Uuid>,
    ) -> anyhow::Result<WorkspaceResponse> {
        let source = self
            .list_workspaces(org_id)
            .await?
            .into_iter()
            .find(|workspace| {
                workspace.is_base
                    && repo_id.is_none_or(|expected| {
                        workspace
                            .branches
                            .iter()
                            .any(|branch| Uuid::parse_str(&branch.repo_id).ok() == Some(expected))
                    })
            })
            .ok_or_else(|| anyhow::anyhow!("No base workspace contains requested repository"))?;
        let source_id = Uuid::parse_str(&source.id)?;
        let unique_ws_name = unique_test_name(name);
        let idempotency_key = Uuid::new_v4().to_string();
        for attempt in 1..=4 {
            let resp = self
                .authed_post(&format!(
                    "{}/api/v1/workspaces/{}/clone",
                    self.base_url, source_id
                ))
                .json(&serde_json::json!({
                    "name": unique_ws_name,
                    "idempotencyKey": idempotency_key
                }))
                .send()
                .await?;
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            if status.is_success() {
                let envelope: ResponseEnvelope<serde_json::Value> = serde_json::from_str(&body)?;
                return Ok(serde_json::from_value(envelope.data["workspace"].clone())?);
            }
            if status == reqwest::StatusCode::SERVICE_UNAVAILABLE && attempt < 4 {
                tokio::time::sleep(Duration::from_millis(250 * attempt)).await;
                continue;
            }
            anyhow::bail!(
                "workspace fork failed after {attempt} attempt(s): status={status}, body={body}"
            );
        }
        unreachable!("bounded workspace fork attempts must return or fail")
    }

    /// Resume the owner workspace VM and wait for the attached incarnation.
    pub async fn resume_workspace_vm(&self, workspace_id: Uuid) -> anyhow::Result<()> {
        let response = self
            .authed_post(&format!(
                "{}/api/v1/workspaces/{workspace_id}/resume",
                self.base_url
            ))
            .json(&serde_json::json!({"operationId": Uuid::new_v4()}))
            .send()
            .await?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("workspace {workspace_id} resume failed: {status}: {body}");
        }

        let deadline = tokio::time::Instant::now() + Duration::from_secs(120);
        loop {
            let body: serde_json::Value = self
                .authed_get(&format!(
                    "{}/api/v1/workspaces/{workspace_id}/vm",
                    self.base_url
                ))
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;
            if body["data"]["status"] == "running" {
                return Ok(());
            }
            if tokio::time::Instant::now() >= deadline {
                anyhow::bail!(
                    "workspace {workspace_id} VM did not reach running state: {}",
                    body["data"]
                );
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    /// Stop the owner workspace VM, including the durable flush boundary.
    pub async fn stop_workspace_vm(&self, workspace_id: Uuid) -> anyhow::Result<()> {
        self.authed_post(&format!(
            "{}/api/v1/workspaces/{workspace_id}/stop",
            self.base_url
        ))
        .json(&serde_json::json!({"operationId": Uuid::new_v4()}))
        .send()
        .await?
        .error_for_status()?;
        Ok(())
    }

    /// Commit and push live changes for one workspace repository.
    pub async fn push_workspace_changes(
        &self,
        workspace_id: Uuid,
        repo_id: Uuid,
    ) -> anyhow::Result<serde_json::Value> {
        let response = self
            .authed_post(&format!(
                "{}/api/v1/workspaces/{workspace_id}/changes/push",
                self.base_url
            ))
            .json(&serde_json::json!({"repoId": repo_id}))
            .send()
            .await?
            .error_for_status()?;
        Ok(response.json().await?)
    }

    /// List workspace branches via `GET /api/v1/workspaces/:id/branches`.
    pub async fn get_workspace_branches(
        &self,
        workspace_id: Uuid,
    ) -> anyhow::Result<Vec<WorkspaceBranchResponse>> {
        let resp = self
            .authed_get(&format!(
                "{}/api/v1/workspaces/{}/branches",
                self.base_url, workspace_id
            ))
            .send()
            .await?
            .error_for_status()?;
        let envelope: ResponseEnvelope<Vec<WorkspaceBranchResponse>> = resp.json().await?;
        Ok(envelope.data)
    }

    /// Clone a workspace via `POST /api/v1/workspaces/:id/clone`.
    ///
    /// Returns the raw JSON response so callers can inspect clone stats and the
    /// nested workspace detail shape without needing a dedicated DTO here.
    pub async fn clone_workspace(
        &self,
        workspace_id: Uuid,
        name: &str,
    ) -> anyhow::Result<serde_json::Value> {
        let resp = self
            .authed_post(&format!(
                "{}/api/v1/workspaces/{}/clone",
                self.base_url, workspace_id
            ))
            .json(&serde_json::json!({
                "name": name,
                "idempotencyKey": Uuid::new_v4().to_string()
            }))
            .send()
            .await?
            .error_for_status()?;
        Ok(resp.json().await?)
    }

    /// Fetch the workspace materialization manifest via
    /// `GET /api/v1/workspaces/:id/manifest`.
    ///
    /// Returns the raw JSON envelope so tests can assert only the fields they
    /// care about without locking the helper to a full DTO.
    pub async fn get_workspace_manifest(
        &self,
        workspace_id: Uuid,
    ) -> anyhow::Result<serde_json::Value> {
        let resp = self
            .authed_get(&format!(
                "{}/api/v1/workspaces/{}/manifest",
                self.base_url, workspace_id
            ))
            .send()
            .await?
            .error_for_status()?;
        Ok(resp.json().await?)
    }

    /// Fetch workspace detail via `GET /api/v1/workspaces/:id`.
    pub async fn get_workspace(&self, workspace_id: Uuid) -> anyhow::Result<serde_json::Value> {
        let resp = self
            .authed_get(&format!(
                "{}/api/v1/workspaces/{}",
                self.base_url, workspace_id
            ))
            .send()
            .await?
            .error_for_status()?;
        Ok(resp.json().await?)
    }

    // --- Docs ---

    pub async fn create_doc(
        &self,
        workspace_id: Uuid,
        name: &str,
        doc_type: &str,
        file_path: Option<&str>,
        content: Option<&str>,
    ) -> anyhow::Result<serde_json::Value> {
        let resp = self
            .authed_post(&format!(
                "{}/api/v1/workspaces/{}/docs",
                self.base_url, workspace_id
            ))
            .json(&serde_json::json!({
                "name": name,
                "docType": doc_type,
                "filePath": file_path,
                "content": content,
            }))
            .send()
            .await?
            .error_for_status()?;
        Ok(resp.json().await?)
    }

    pub async fn get_doc(
        &self,
        workspace_id: Uuid,
        doc_id: Uuid,
    ) -> anyhow::Result<serde_json::Value> {
        let resp = self
            .authed_get(&format!(
                "{}/api/v1/workspaces/{}/docs/{}",
                self.base_url, workspace_id, doc_id
            ))
            .send()
            .await?
            .error_for_status()?;
        Ok(resp.json().await?)
    }

    /// Fetch a current Project Doc through its repository-authoritative path.
    pub async fn get_doc_by_path(
        &self,
        workspace_id: Uuid,
        file_path: &str,
    ) -> anyhow::Result<serde_json::Value> {
        let resp = self
            .authed_get(&format!(
                "{}/api/v1/workspaces/{}/docs/by-path",
                self.base_url, workspace_id
            ))
            .query(&[("path", file_path)])
            .send()
            .await?
            .error_for_status()?;
        Ok(resp.json().await?)
    }

    pub async fn update_doc(
        &self,
        workspace_id: Uuid,
        doc_id: Uuid,
        body: serde_json::Value,
    ) -> anyhow::Result<serde_json::Value> {
        let resp = self
            .authed_put(&format!(
                "{}/api/v1/workspaces/{}/docs/{}",
                self.base_url, workspace_id, doc_id
            ))
            .json(&body)
            .send()
            .await?
            .error_for_status()?;
        Ok(resp.json().await?)
    }

    /// Convenience: get the first branch name for a workspace.
    /// Panics if the workspace has no branches.
    pub async fn first_branch_name(&self, workspace_id: Uuid) -> anyhow::Result<String> {
        let branches = self.get_workspace_branches(workspace_id).await?;
        Ok(branches
            .first()
            .expect("workspace should have at least one branch")
            .branch_name
            .clone())
    }

    /// Return the only materialized branch for a workspace.
    ///
    /// Internal durable ontology/content endpoints require an explicit
    /// workspace branch scope. Tests that only care about a single-repo
    /// workspace use this helper to make that scope concrete; multi-branch
    /// tests must pass a branch ID explicitly.
    pub async fn only_materialized_workspace_branch(
        &self,
        workspace_id: Uuid,
    ) -> anyhow::Result<WorkspaceBranchResponse> {
        let branches = self.get_workspace_branches(workspace_id).await?;
        let materialized = branches
            .iter()
            .filter(|branch| branch.materialization_status == "materialized")
            .collect::<Vec<_>>();

        match materialized.as_slice() {
            [branch] => Ok((*branch).clone()),
            [] => {
                let statuses = format_branch_statuses(&branches);
                Err(anyhow::anyhow!(
                    "Workspace {workspace_id} has no materialized branches; branches=[{statuses}]"
                ))
            }
            _ => {
                let statuses = format_branch_statuses(&branches);
                Err(anyhow::anyhow!(
                    "Workspace {workspace_id} has multiple materialized branches; pass an explicit repo_id instead of relying on branch order. branches=[{statuses}]"
                ))
            }
        }
    }

    pub async fn materialized_workspace_branch_for_repo(
        &self,
        workspace_id: Uuid,
        repo_id: Uuid,
    ) -> anyhow::Result<WorkspaceBranchResponse> {
        let manifest = self.get_workspace_manifest(workspace_id).await?;
        let repo_suffix = format!("/git/{repo_id}");
        let member = manifest["data"]["repos"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|repo| {
                repo["url"]
                    .as_str()
                    .is_some_and(|url| url.trim_end_matches('/').ends_with(&repo_suffix))
            })
            .or_else(|| {
                let docs = &manifest["data"]["docs"];
                docs["repo_url"]
                    .as_str()
                    .filter(|url| url.trim_end_matches('/').ends_with(&repo_suffix))
                    .map(|_| docs)
            })
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Workspace {workspace_id} manifest has no authoritative member for repo {repo_id}; manifest={manifest}"
                )
            })?;
        let field = |name: &str| -> anyhow::Result<&str> {
            member[name]
                .as_str()
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    anyhow::anyhow!(
                    "Workspace {workspace_id} committed member for repo {repo_id} omitted {name}"
                )
                })
        };
        let manifest_repo_id = field("repo_id")?;
        let branches = self.get_workspace_branches(workspace_id).await?;
        let branch = branches
            .iter()
            .find(|branch| branch.repo_id == manifest_repo_id)
            .ok_or_else(|| {
                let statuses = format_branch_statuses(&branches);
                anyhow::anyhow!(
                    "Workspace {workspace_id} manifest member {manifest_repo_id} for repo {repo_id} is absent from branch detail; branches=[{statuses}]"
                )
            })?;
        if branch.repo_id != repo_id.to_string() || branch.materialization_status != "materialized"
        {
            let statuses = format_branch_statuses(&branches);
            return Err(anyhow::anyhow!(
                "Workspace {workspace_id} manifest member {manifest_repo_id} is not a materialized branch for repo {repo_id}; branches=[{statuses}]"
            ));
        }
        // The manifest resolves
        // the committed Git snapshot, including native pushes not yet projected.
        // Keep detail metadata but never use its stale ref/head as publication
        // authority in chat, Sync, or any sibling fixture.
        let mut committed = branch.clone();
        committed.branch_name = field("branch")?.to_owned();
        committed.head_commit = field("commit")?.to_owned();
        Ok(committed)
    }

    /// List workspaces in an org via `GET /api/v1/orgs/:orgId/workspaces`.
    ///
    /// The endpoint is unpaginated, so this returns the org's complete
    /// workspace set — callers may rely on that. (It previously sent
    /// `?perPage=100`, which capped the helper at the first 100 workspaces and
    /// made `find_workspace_by_name` silently return `None` past that.)
    pub async fn list_workspaces(
        &self,
        org_id: Uuid,
    ) -> anyhow::Result<Vec<WorkspaceListItemResponse>> {
        let resp = self
            .authed_get(&format!(
                "{}/api/v1/orgs/{}/workspaces",
                self.base_url, org_id
            ))
            .send()
            .await?
            .error_for_status()?;
        let envelope: ResponseEnvelope<Vec<WorkspaceListItemResponse>> = resp.json().await?;
        Ok(envelope.data)
    }

    /// Find a workspace by exact name within an org.
    pub async fn find_workspace_by_name(
        &self,
        org_id: Uuid,
        name: &str,
    ) -> anyhow::Result<Option<WorkspaceListItemResponse>> {
        let workspaces = self.list_workspaces(org_id).await?;
        Ok(workspaces
            .into_iter()
            .find(|workspace| workspace.name == name))
    }

    /// Resolve a workspace URL's org/workspace slugs to IDs through the live API.
    pub async fn resolve_workspace(
        &self,
        org_slug: &str,
        workspace_name: &str,
    ) -> anyhow::Result<ResolveResponse> {
        let mut url = reqwest::Url::parse(&format!("{}/api/v1/resolve", self.base_url))?;
        url.query_pairs_mut()
            .append_pair("org", org_slug)
            .append_pair("workspace", workspace_name);
        let resp = self
            .authed_get(url.as_str())
            .send()
            .await?
            .error_for_status()?;
        let envelope: ResponseEnvelope<ResolveResponse> = resp.json().await?;
        Ok(envelope.data)
    }

    /// Stop every active task through its ordinary cancellation endpoint.
    pub async fn cancel_all_workspace_tasks(&self, workspace_id: Uuid) -> anyhow::Result<()> {
        tokio::time::timeout(Duration::from_secs(90), async {
            loop {
                let body: serde_json::Value = self
                    .authed_get(&format!(
                        "{}/api/v1/workspaces/{workspace_id}/tasks?activeOnly=true&limit=100",
                        self.base_url
                    ))
                    .send()
                    .await?
                    .error_for_status()?
                    .json()
                    .await?;
                let tasks = body["data"]["tasks"]
                    .as_array()
                    .ok_or_else(|| anyhow::anyhow!("task page missing"))?;
                if tasks.is_empty() {
                    return Ok(());
                }
                for task in tasks {
                    let id = task["id"]
                        .as_str()
                        .ok_or_else(|| anyhow::anyhow!("task id missing"))?;
                    self.authed_post(&format!("{}/api/v1/tasks/{id}/cancel", self.base_url))
                        .send()
                        .await?
                        .error_for_status()?;
                }
                // Running attempts acknowledge cancellation asynchronously.
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        })
        .await
        .map_err(|_| anyhow::anyhow!("task cleanup timed out for workspace {workspace_id}"))?
    }

    // --- Ontology ---

    /// Get the full ontology tree for a workspace.
    ///
    /// The server returns `ResponseEnvelope<OntologyTreeResponse>`, so the
    /// shape is: `{ "data": { "nodes": [...] } }` where each node may have
    /// a nested `children` array (tree assembly). This method flattens the
    /// tree so callers get every node regardless of nesting depth.
    pub async fn get_ontology(
        &self,
        workspace_id: Uuid,
    ) -> anyhow::Result<Vec<OntologyNodeResponse>> {
        let resp = self
            .authed_get(&format!(
                "{}/api/v1/workspaces/{}/ontology",
                self.base_url, workspace_id
            ))
            .send()
            .await?;
        if resp.status() == 404 {
            return Ok(vec![]);
        }
        let resp = resp.error_for_status()?;
        let body: serde_json::Value = resp.json().await?;

        // Extract the nodes array from the response envelope.
        let nodes_val = if let Some(arr) = body
            .get("data")
            .and_then(|d| d.get("nodes"))
            .and_then(|n| n.as_array())
        {
            arr.clone()
        } else if let Some(arr) = body.get("data").and_then(|d| d.as_array()) {
            // Fallback: data is a plain array
            arr.clone()
        } else if body.is_array() {
            body.as_array().cloned().unwrap_or_default()
        } else {
            return Ok(vec![]);
        };

        // Recursively flatten the tree so nested children are included.
        fn flatten_nodes(vals: &[serde_json::Value], out: &mut Vec<serde_json::Value>) {
            for v in vals {
                out.push(v.clone());
                if let Some(children) = v.get("children").and_then(|c| c.as_array()) {
                    flatten_nodes(children, out);
                }
            }
        }

        let mut flat = Vec::new();
        flatten_nodes(&nodes_val, &mut flat);

        Ok(serde_json::from_value(serde_json::Value::Array(flat))?)
    }

    // --- Timeline ---

    /// Get timeline entries for a task.
    ///
    /// The server returns `TimelineResponse` directly (NOT wrapped in
    /// `ResponseEnvelope`), so the shape is:
    /// `{ "entries": [...], "cursor": ..., "hasMore": ... }`
    pub async fn get_timeline(
        &self,
        _workspace_id: Uuid,
        task_id: Uuid,
    ) -> anyhow::Result<Vec<TimelineEntry>> {
        let resp = self
            .authed_get(&format!(
                "{}/api/v1/tasks/{}/timeline?limit=500",
                self.base_url, task_id
            ))
            .send()
            .await?;
        if resp.status() == 404 {
            return Ok(vec![]);
        }
        let resp = resp.error_for_status()?;
        let body: serde_json::Value = resp.json().await?;
        // The API wraps in ResponseEnvelope: {"data": {"entries": [...]}}
        // Also handle the flat case where entries is top-level.
        if let Some(arr) = body
            .get("data")
            .and_then(|d| d.get("entries"))
            .and_then(|e| e.as_array())
        {
            Ok(serde_json::from_value(serde_json::Value::Array(
                arr.clone(),
            ))?)
        } else if let Some(arr) = body.get("entries").and_then(|d| d.as_array()) {
            Ok(serde_json::from_value(serde_json::Value::Array(
                arr.clone(),
            ))?)
        } else if let Some(arr) = body.get("data").and_then(|d| d.as_array()) {
            // Fallback in case "data" is a flat array
            Ok(serde_json::from_value(serde_json::Value::Array(
                arr.clone(),
            ))?)
        } else if body.is_array() {
            Ok(serde_json::from_value(body)?)
        } else {
            Ok(vec![])
        }
    }

    /// Get timeline entries for a chat session across all associated tasks.
    pub async fn get_session_timeline(
        &self,
        session_id: &str,
    ) -> anyhow::Result<Vec<TimelineEntry>> {
        let resp = self
            .authed_get(&format!(
                "{}/api/v1/sessions/{}/timeline",
                self.base_url, session_id
            ))
            .send()
            .await?;
        if resp.status() == 404 {
            return Ok(vec![]);
        }
        let resp = resp.error_for_status()?;
        let body: serde_json::Value = resp.json().await?;
        if let Some(arr) = body
            .get("data")
            .and_then(|d| d.get("entries"))
            .and_then(|e| e.as_array())
        {
            Ok(serde_json::from_value(serde_json::Value::Array(
                arr.clone(),
            ))?)
        } else {
            Ok(vec![])
        }
    }

    /// Save a file through the public owner-only workspace runtime.
    pub async fn upload_file_to_branch(
        &self,
        workspace_id: &str,
        repo_id: &str,
        path: &str,
        content: &str,
    ) -> anyhow::Result<()> {
        let mut url = reqwest::Url::parse(&format!(
            "{}/api/v1/workspaces/{workspace_id}/repos/{repo_id}/files",
            self.base_url.trim_end_matches('/')
        ))?;
        {
            let url_for_error = url.to_string();
            let mut segments = url.path_segments_mut().map_err(|()| {
                anyhow::anyhow!("file fixture URL cannot carry path segments: {url_for_error}")
            })?;
            for segment in path.split('/') {
                segments.push(segment);
            }
        }
        let resp = self
            .authed_put(url.as_str())
            .json(&serde_json::json!({ "content": content }))
            .send()
            .await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!(
                "public file fixture write failed for repository {workspace_id}/{repo_id} path '{path}': status={status}, body={body}"
            );
        }
        Ok(())
    }

    // --- Chat Sessions ---

    /// Create a chat session in a workspace.
    /// Returns the raw JSON response (envelope with session data).
    pub async fn create_chat_session(
        &self,
        workspace_id: Uuid,
        title: &str,
    ) -> anyhow::Result<serde_json::Value> {
        let resp = self
            .authed_post(&format!(
                "{}/api/v1/workspaces/{}/sessions",
                self.base_url, workspace_id
            ))
            .json(&serde_json::json!({
                "sessionType": "chat",
                "title": title
            }))
            .send()
            .await?
            .error_for_status()?;
        Ok(resp.json().await?)
    }

    /// Create a chat session with an explicit agent provider (e.g.
    /// `"claude_code"`). Fork-chat only carries Claude Code sessions
    /// (clone_service D31 / chat_service D56), so fork-carry coverage seeds
    /// its source sessions through this helper.
    pub async fn create_chat_session_with_provider(
        &self,
        workspace_id: Uuid,
        title: &str,
        agent_provider: &str,
    ) -> anyhow::Result<serde_json::Value> {
        let resp = self
            .authed_post(&format!(
                "{}/api/v1/workspaces/{}/sessions",
                self.base_url, workspace_id
            ))
            .json(&serde_json::json!({
                "sessionType": "chat",
                "title": title,
                "agentProvider": agent_provider,
            }))
            .send()
            .await?
            .error_for_status()?;
        Ok(resp.json().await?)
    }

    /// List chat sessions for a workspace via
    /// `GET /api/v1/workspaces/:id/sessions`. Returns the `data` array.
    pub async fn list_chat_sessions(
        &self,
        workspace_id: Uuid,
    ) -> anyhow::Result<Vec<serde_json::Value>> {
        let resp = self
            .authed_get(&format!(
                "{}/api/v1/workspaces/{}/sessions",
                self.base_url, workspace_id
            ))
            .send()
            .await?
            .error_for_status()?;
        let body: serde_json::Value = resp.json().await?;
        Ok(body
            .get("data")
            .and_then(|d| d.as_array())
            .cloned()
            .unwrap_or_default())
    }

    /// Upload an attachment onto an existing user message via
    /// `POST /api/v1/sessions/:id/attachments`. Returns the response envelope
    /// (attachment id/sha256 under `data`). Fork-carry coverage uses this to
    /// seed an upload whose row must be re-bound to the carried session
    /// (chat_service D56).
    pub async fn upload_chat_attachment(
        &self,
        session_id: &str,
        message_id: &str,
        file_name: &str,
        content_type: &str,
        content_base64: &str,
    ) -> anyhow::Result<serde_json::Value> {
        let resp = self
            .authed_post(&format!(
                "{}/api/v1/sessions/{}/attachments",
                self.base_url, session_id
            ))
            .json(&serde_json::json!({
                "messageId": message_id,
                "fileName": file_name,
                "contentType": content_type,
                "contentBase64": content_base64,
            }))
            .send()
            .await?
            .error_for_status()?;
        Ok(resp.json().await?)
    }

    /// Send a user message to a chat session.
    /// Returns the raw JSON response (envelope with message data).
    pub async fn send_chat_message(
        &self,
        session_id: &str,
        content: &str,
        permission_mode: &str,
    ) -> anyhow::Result<serde_json::Value> {
        let resp = self
            .authed_post(&format!(
                "{}/api/v1/sessions/{}/messages",
                self.base_url, session_id
            ))
            .json(&serde_json::json!({
                "content": content,
                "permissionMode": permission_mode,
            }))
            .send()
            .await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await?;
            anyhow::bail!("chat send failed ({status}): {body}");
        }
        Ok(resp.json().await?)
    }

    /// Get messages for a chat session.
    /// Returns the raw JSON response (MessageListResponseDTO).
    pub async fn get_chat_messages(&self, session_id: &str) -> anyhow::Result<serde_json::Value> {
        let resp = self
            .authed_get(&format!(
                "{}/api/v1/sessions/{}/messages",
                self.base_url, session_id
            ))
            .send()
            .await?
            .error_for_status()?;
        Ok(resp.json().await?)
    }

    /// Get a chat session's detail.
    pub async fn get_chat_session(&self, session_id: &str) -> anyhow::Result<serde_json::Value> {
        let resp = self
            .authed_get(&format!("{}/api/v1/sessions/{}", self.base_url, session_id))
            .send()
            .await?
            .error_for_status()?;
        Ok(resp.json().await?)
    }

    // --- Polling ---

    /// Poll a closure until it returns `Some(T)` or timeout is reached.
    ///
    /// The closure is called repeatedly with 500ms intervals. If it returns
    /// `Ok(Some(value))`, that value is returned. If it returns `Ok(None)`,
    /// polling continues. On error, the error is propagated immediately.
    pub async fn poll_until<F, T>(&self, mut f: F, timeout_secs: u64) -> anyhow::Result<T>
    where
        F: FnMut() -> std::pin::Pin<
            Box<dyn std::future::Future<Output = anyhow::Result<Option<T>>> + Send>,
        >,
    {
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);
        loop {
            if let Some(result) = f().await? {
                return Ok(result);
            }
            if tokio::time::Instant::now() >= deadline {
                anyhow::bail!("poll_until timed out after {}s", timeout_secs);
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
    }

    // --- Auth (E2E test token) ---

    /// Obtain E2E auth tokens for the given email.
    /// Returns `(access_token, refresh_token)`.
    pub async fn obtain_test_tokens(
        &self,
        email: &str,
    ) -> anyhow::Result<(String, Option<String>)> {
        let resp = self
            .client
            .post(format!("{}/api/v1/auth/test-token", self.base_url))
            .json(&serde_json::json!({ "email": email }))
            .send()
            .await?
            .error_for_status()?;
        let body: serde_json::Value = resp.json().await?;
        let access_token = body["data"]["accessToken"]
            .as_str()
            .or(body["data"]["jwt"].as_str())
            .or(body["accessToken"].as_str())
            .or(body["jwt"].as_str())
            .ok_or_else(|| anyhow::anyhow!("No access token in test-token response"))?
            .to_string();
        let refresh_token = body["data"]["refreshToken"]
            .as_str()
            .or(body["refreshToken"].as_str())
            .map(std::string::ToString::to_string);
        Ok((access_token, refresh_token))
    }

    /// Backward-compatible helper that returns only the access token.
    pub async fn obtain_test_token(&self, email: &str) -> anyhow::Result<String> {
        let (access_token, _) = self.obtain_test_tokens(email).await?;
        Ok(access_token)
    }

    // --- OAuth login (full redirect chain) ---

    /// Programmatically walk the OAuth redirect chain against the local stub.
    ///
    /// Returns `(access_token, Option<refresh_token>)` — identical shape to
    /// [`obtain_test_tokens`] so the caller doesn't care which auth method
    /// was used.
    ///
    /// Requires env vars:
    /// - `G5_E2E_OAUTH_STUB_URL` — base URL of the OAuth stub
    ///   (defaults to `http://oauth-stub:9876` in containers,
    ///   `http://localhost:9876` on the host)
    async fn login_via_oauth(&self) -> anyhow::Result<(String, Option<String>)> {
        let identity_cookie = oauth_stub_identity_cookie_value();
        self.login_via_oauth_with_identity_cookie(&identity_cookie)
            .await
    }

    async fn login_via_oauth_with_identity_cookie(
        &self,
        identity_cookie: &str,
    ) -> anyhow::Result<(String, Option<String>)> {
        let oauth_stub_base = oauth_stub_base_url();

        // Build a reqwest client that does NOT follow redirects so we can
        // inspect Location headers at each step.
        let no_redirect = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(30))
            .build()?;

        // Step 1: GET /api/v1/auth/login/github → 303 with Location to OAuth
        // stub's /login/oauth/authorize?...
        let login_url = format!("{}/api/v1/auth/login/github", self.base_url);
        let resp1 = no_redirect.get(&login_url).send().await?;
        anyhow::ensure!(
            resp1.status().is_redirection(),
            "Expected redirect from /auth/login/github, got {}",
            resp1.status()
        );
        let authorize_url = resp1
            .headers()
            .get("location")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| anyhow::anyhow!("No Location header on login redirect"))?
            .to_string();
        // The login redirect binds the attempt to this client with an
        // attempt cookie (SEC-M35); the callback must return it, as the
        // initiating browser would.
        let attempt_cookie = resp1
            .headers()
            .get_all("set-cookie")
            .iter()
            .flat_map(|v| v.to_str().ok())
            .find(|s| s.contains("g5_oauth_login="))
            .and_then(|s| s.split(';').next())
            .ok_or_else(|| anyhow::anyhow!("No OAuth attempt Set-Cookie on login redirect"))?
            .to_string();

        // The authorize_url may point at g5-test-client (G5_EXTERNAL_URL) but
        // the OAuth stub is reachable at G5_E2E_OAUTH_STUB_URL.  Parse the
        // path+query from the authorized URL and re-target at the stub.
        let parsed_auth = url::Url::parse(&authorize_url)?;
        let stub_authorize = format!(
            "{}{}?{}",
            oauth_stub_base,
            parsed_auth.path(),
            parsed_auth.query().unwrap_or("")
        );

        // Step 2: GET <oauth-stub>/login/oauth/authorize → 302 with code+state
        // back to the callback URL (http://g5-test-client/api/v1/auth/callback/github?code=...&state=...)
        let resp2 = no_redirect
            .get(&stub_authorize)
            .header(
                reqwest::header::COOKIE,
                format!("g5_e2e_oauth_identity={identity_cookie}"),
            )
            .send()
            .await?;
        anyhow::ensure!(
            resp2.status().is_redirection(),
            "Expected redirect from OAuth stub authorize, got {}",
            resp2.status()
        );
        let callback_redirect = resp2
            .headers()
            .get("location")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| anyhow::anyhow!("No Location header on OAuth stub redirect"))?
            .to_string();

        // The callback redirect points at G5_EXTERNAL_URL (e.g. http://g5-test-client/api/v1/auth/callback/github?code=...&state=...).
        // Extract the path+query and call g5-api directly so we don't need
        // Caddy in the loop.
        let parsed_cb = url::Url::parse(&callback_redirect)?;
        let callback_direct = format!(
            "{}{}?{}",
            self.base_url,
            parsed_cb.path(),
            parsed_cb.query().unwrap_or("")
        );

        // Step 3: GET /api/v1/auth/callback/github?code=...&state=...
        // → 303 to "/" with the effective refresh cookie and legacy expiry.
        let resp3 = no_redirect
            .get(&callback_direct)
            .header(reqwest::header::COOKIE, &attempt_cookie)
            .send()
            .await?;
        anyhow::ensure!(
            resp3.status().is_redirection(),
            "Expected redirect from /auth/callback, got {}",
            resp3.status()
        );

        let cookie_name =
            std::env::var("G5_REFRESH_COOKIE_NAME").unwrap_or_else(|_| "g5_refresh".to_owned());
        let refresh_token = issued_refresh_token(resp3.headers(), cookie_name.trim())?;

        // Step 4: POST /api/v1/auth/refresh with the refresh token → JWT
        let resp4 = self
            .client
            .post(format!("{}/api/v1/auth/refresh", self.base_url))
            .json(&serde_json::json!({ "refreshToken": &refresh_token }))
            .send()
            .await?
            .error_for_status()?;
        let body: serde_json::Value = resp4.json().await?;
        let access_token = body["data"]["accessToken"]
            .as_str()
            .or(body["data"]["jwt"].as_str())
            .or(body["accessToken"].as_str())
            .or(body["jwt"].as_str())
            .ok_or_else(|| anyhow::anyhow!("No access token in OAuth refresh response"))?
            .to_string();

        // The refresh response may rotate the token (the new cookie value).
        let final_refresh = body["data"]["refreshToken"]
            .as_str()
            .or(body["refreshToken"].as_str())
            .map(std::string::ToString::to_string)
            .unwrap_or(refresh_token);

        Ok((access_token, Some(final_refresh)))
    }

    // --- Comments ---

    /// Create a node-scoped comment.
    pub async fn create_comment(
        &self,
        workspace_id: Uuid,
        node_id: Option<Uuid>,
        content: &str,
        mention_user_ids: Option<Vec<Uuid>>,
    ) -> anyhow::Result<serde_json::Value> {
        let mut body = serde_json::json!({
            "scope": if node_id.is_some() { "node" } else { "line" },
            "content": content,
        });
        if let Some(nid) = node_id {
            body["nodeId"] = serde_json::Value::String(nid.to_string());
        }
        if let Some(ids) = mention_user_ids {
            body["mentionUserIds"] = serde_json::json!(ids);
        }
        let resp = self
            .authed_post(&format!(
                "{}/api/v1/workspaces/{}/comments",
                self.base_url, workspace_id
            ))
            .json(&body)
            .send()
            .await?
            .error_for_status()?;
        Ok(resp.json().await?)
    }

    /// List comments in a workspace.
    pub async fn list_comments(
        &self,
        workspace_id: Uuid,
        node_id: Option<Uuid>,
    ) -> anyhow::Result<serde_json::Value> {
        let mut url = format!(
            "{}/api/v1/workspaces/{}/comments",
            self.base_url, workspace_id
        );
        if let Some(nid) = node_id {
            url.push_str(&format!("?nodeId={}", nid));
        }
        let resp = self.authed_get(&url).send().await?.error_for_status()?;
        Ok(resp.json().await?)
    }

    /// Add a reply to a comment.
    pub async fn add_reply(
        &self,
        comment_id: Uuid,
        content: &str,
        mention_user_ids: Option<Vec<Uuid>>,
    ) -> anyhow::Result<serde_json::Value> {
        let mut body = serde_json::json!({ "content": content });
        if let Some(ids) = mention_user_ids {
            body["mentionUserIds"] = serde_json::json!(ids);
        }
        let resp = self
            .authed_post(&format!(
                "{}/api/v1/comments/{}/replies",
                self.base_url, comment_id
            ))
            .json(&body)
            .send()
            .await?
            .error_for_status()?;
        Ok(resp.json().await?)
    }

    /// Update (resolve) a comment.
    pub async fn update_comment(
        &self,
        comment_id: Uuid,
        body: serde_json::Value,
    ) -> anyhow::Result<serde_json::Value> {
        let resp = self
            .authed_patch(&format!("{}/api/v1/comments/{}", self.base_url, comment_id))
            .json(&body)
            .send()
            .await?
            .error_for_status()?;
        Ok(resp.json().await?)
    }

    /// Delete a comment.
    pub async fn delete_comment(&self, comment_id: Uuid) -> anyhow::Result<serde_json::Value> {
        let resp = self
            .authed_delete(&format!("{}/api/v1/comments/{}", self.base_url, comment_id))
            .send()
            .await?
            .error_for_status()?;
        Ok(resp.json().await?)
    }

    /// Get replies for a comment.
    pub async fn get_replies(&self, comment_id: Uuid) -> anyhow::Result<serde_json::Value> {
        let resp = self
            .authed_get(&format!(
                "{}/api/v1/comments/{}/replies",
                self.base_url, comment_id
            ))
            .send()
            .await?
            .error_for_status()?;
        Ok(resp.json().await?)
    }

    // --- Notifications ---

    /// List notifications for the current user in a workspace.
    pub async fn list_notifications(
        &self,
        workspace_id: Uuid,
        unread_only: bool,
    ) -> anyhow::Result<serde_json::Value> {
        let url = format!(
            "{}/api/v1/workspaces/{}/notifications?unreadOnly={}",
            self.base_url, workspace_id, unread_only
        );
        let resp = self.authed_get(&url).send().await?.error_for_status()?;
        Ok(resp.json().await?)
    }

    /// Mark a single notification as read.
    pub async fn mark_notification_read(
        &self,
        notification_id: Uuid,
    ) -> anyhow::Result<serde_json::Value> {
        let resp = self
            .authed_patch(&format!(
                "{}/api/v1/notifications/{}",
                self.base_url, notification_id
            ))
            .send()
            .await?
            .error_for_status()?;
        Ok(resp.json().await?)
    }

    /// Mark all notifications in a workspace as read (`{}` selects all).
    pub async fn mark_all_notifications_read(
        &self,
        workspace_id: Uuid,
    ) -> anyhow::Result<serde_json::Value> {
        let resp = self
            .authed_post(&format!(
                "{}/api/v1/workspaces/{}/notifications/read-all",
                self.base_url, workspace_id
            ))
            .json(&serde_json::json!({}))
            .send()
            .await?
            .error_for_status()?;
        Ok(resp.json().await?)
    }
}
