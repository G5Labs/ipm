//! Centralized API client for CLI public-API calls.
//!
//! Handles bearer-token authentication with automatic refresh on 401.
//! Auth state (JWT + refresh token) is persisted in `.g5/auth_state.json`
//! so tokens survive across CLI invocations.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use g5_contracts::error::G5Error;

// ─── Auth state ──────────────────────────────────────────────────────────────

/// Persisted auth state stored in `.g5/auth_state.json`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AuthState {
    /// Current JWT for bearer auth.
    #[serde(skip_serializing_if = "Option::is_none")]
    jwt: Option<String>,
    /// Refresh token for obtaining new JWTs.
    #[serde(skip_serializing_if = "Option::is_none")]
    refresh_token: Option<String>,
    /// API URL this auth state was issued for.
    #[serde(skip_serializing_if = "Option::is_none")]
    api_url: Option<String>,
    /// Browser app URL used for user-facing approval routes.
    #[serde(skip_serializing_if = "Option::is_none")]
    web_url: Option<String>,
    /// Org ID used for token requests.
    #[serde(skip_serializing_if = "Option::is_none")]
    org_id: Option<String>,
    /// Workspace ID this local checkout belongs to.
    #[serde(skip_serializing_if = "Option::is_none")]
    workspace_id: Option<String>,
    /// Auth mode for the local checkout (`device`, `legacy-refresh`, etc.).
    #[serde(skip_serializing_if = "Option::is_none")]
    auth_mode: Option<String>,
}

pub const AUTH_STATE_FILE: &str = ".g5/auth_state.json";

impl AuthState {
    pub fn load(workspace_path: &Path) -> Self {
        Self::load_from_path(&workspace_path.join(AUTH_STATE_FILE))
    }

    pub fn load_from_path(path: &Path) -> Self {
        if !path.exists() {
            return Self::default();
        }
        match std::fs::read_to_string(path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self, workspace_path: &Path) -> Result<(), G5Error> {
        self.save_to_path(&workspace_path.join(AUTH_STATE_FILE))
    }

    pub fn save_to_path(&self, path: &Path) -> Result<(), G5Error> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| G5Error::Internal {
                message: format!(
                    "Failed to create auth state directory {}: {e}",
                    parent.display()
                ),
            })?;
        }

        let content = serde_json::to_string_pretty(self).map_err(|e| G5Error::Internal {
            message: format!("Failed to serialize auth state: {e}"),
        })?;
        std::fs::write(path, content).map_err(|e| G5Error::Internal {
            message: format!("Failed to write auth state {}: {e}", path.display()),
        })?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).map_err(
                |e| G5Error::Internal {
                    message: format!(
                        "Failed to set auth state permissions {}: {e}",
                        path.display()
                    ),
                },
            )?;
        }
        Ok(())
    }

    pub fn jwt(&self) -> Option<&str> {
        self.jwt.as_deref()
    }

    pub fn refresh_token(&self) -> Option<&str> {
        self.refresh_token.as_deref()
    }

    pub fn api_url(&self) -> Option<&str> {
        self.api_url.as_deref()
    }

    pub fn web_url(&self) -> Option<&str> {
        self.web_url.as_deref()
    }

    pub fn org_id(&self) -> Option<&str> {
        self.org_id.as_deref()
    }

    pub fn workspace_id(&self) -> Option<&str> {
        self.workspace_id.as_deref()
    }

    pub fn auth_mode(&self) -> Option<&str> {
        self.auth_mode.as_deref()
    }

    pub fn set_authenticated(
        &mut self,
        jwt: String,
        refresh_token: String,
        api_url: String,
        org_id: Option<String>,
        workspace_id: Option<String>,
    ) {
        self.jwt = Some(jwt);
        self.refresh_token = Some(refresh_token);
        self.api_url = Some(api_url);
        self.org_id = org_id;
        self.workspace_id = workspace_id;
        self.auth_mode = Some("device".to_string());
    }

    pub fn clear_tokens_preserve_metadata(&mut self) {
        self.jwt = None;
        self.refresh_token = None;
        if self.auth_mode.is_none() {
            self.auth_mode = Some("device".to_string());
        }
    }
}

// ─── Response envelope ───────────────────────────────────────────────────────

/// Standard g5-api JSON response envelope.
#[derive(Debug, Deserialize)]
pub struct ResponseEnvelope<T> {
    pub data: T,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiErrorEnvelope {
    error: ApiErrorBody,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiErrorBody {
    code: String,
    message: String,
    #[serde(default)]
    details: serde_json::Value,
    #[serde(default)]
    request_id: String,
}

// ─── Refresh / test-token DTOs ───────────────────────────────────────────────

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RefreshRequest {
    refresh_token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    org_id: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TestTokenRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    org_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AuthTokensResponse {
    jwt: String,
    refresh_token: String,
    #[allow(dead_code)]
    expires_in: i64,
}

// ─── API Client ──────────────────────────────────────────────────────────────

/// Centralized HTTP client for public g5-api calls.
///
/// Provides `get` and `post` methods that automatically:
/// 1. Attach the current bearer token
/// 2. On 401, attempt token refresh and retry once
/// 3. Persist updated tokens to disk
pub struct ApiClient {
    http: reqwest::Client,
    api_url: String,
    auth: Mutex<AuthState>,
    auth_state_path: PathBuf,
}

impl ApiClient {
    /// Create a new ApiClient, loading persisted auth state from the workspace.
    ///
    /// Token resolution priority:
    /// 1. `G5_API_KEY` / `G5D_TOKEN` env var (used as initial JWT)
    /// 2. Persisted workspace-local `.g5/auth_state.json`
    /// 3. Global `g5d auth login --org-id/--workspace-id` cache when it is for
    ///    the same API URL and the workspace-local cache has no refresh token
    /// 4. Auto-bootstrap via test-token endpoint (dev/E2E mode)
    pub fn new(api_url: &str, workspace_path: &Path) -> Self {
        let normalized_api_url = Self::normalize_api_url(api_url);
        let workspace_auth_state_path = workspace_path.join(AUTH_STATE_FILE);
        let mut auth = AuthState::load_from_path(&workspace_auth_state_path);
        let mut auth_state_path = workspace_auth_state_path;

        // If API URL changed, discard stale tokens
        if !Self::auth_state_matches_api_url(&auth, &normalized_api_url) {
            auth = AuthState::default();
            auth.api_url = Some(normalized_api_url.clone());
        }

        if !Self::has_refresh_token(&auth) {
            if let Some((global_path, global_auth)) =
                Self::load_global_auth_fallback(&normalized_api_url)
            {
                auth = global_auth;
                auth_state_path = global_path;
            }
        }

        Self::from_auth_state(normalized_api_url, auth, auth_state_path)
    }

    /// Create a new ApiClient using only the global auth cache.
    ///
    /// This is for first-time commands such as `g5d clone <workspace-id> <path>`
    /// that run before any workspace-local `.g5/auth_state.json` exists.
    pub fn new_global(api_url: &str) -> Self {
        let normalized_api_url = Self::normalize_api_url(api_url);
        let auth_state_path = Self::global_auth_state_path();
        let mut auth = AuthState::load_from_path(&auth_state_path);

        if !Self::auth_state_matches_api_url(&auth, &normalized_api_url) {
            auth = AuthState::default();
            auth.api_url = Some(normalized_api_url.clone());
        }

        Self::from_auth_state(normalized_api_url, auth, auth_state_path)
    }

    fn from_auth_state(
        normalized_api_url: String,
        mut auth: AuthState,
        auth_state_path: PathBuf,
    ) -> Self {
        let env_token = Self::env_token();

        // Env var overrides persisted JWT (so user can force a new token)
        if let Some(env_token) = env_token.as_ref() {
            auth.jwt = Some(env_token.clone());
        }

        let has_refresh_token = Self::has_refresh_token(&auth);
        let has_org_id = auth
            .org_id
            .as_deref()
            .map(str::trim)
            .is_some_and(|org| !org.is_empty());

        // Bare org env should not replace org metadata from refresh-capable auth.
        if let Some(org) =
            Self::env_org_id().filter(|_| env_token.is_some() || !has_refresh_token || !has_org_id)
        {
            auth.org_id = Some(org);
        }

        Self {
            http: Self::build_http_client(),
            api_url: normalized_api_url,
            auth: Mutex::new(auth),
            auth_state_path,
        }
    }

    fn build_http_client() -> reqwest::Client {
        reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(30))
            .timeout(Duration::from_secs(120))
            .build()
            .expect("Failed to build HTTP client")
    }

    fn normalize_api_url(api_url: &str) -> String {
        api_url.trim_end_matches('/').to_string()
    }

    fn auth_state_matches_api_url(auth: &AuthState, api_url: &str) -> bool {
        auth.api_url
            .as_deref()
            .map(Self::normalize_api_url)
            .as_deref()
            == Some(api_url)
    }

    fn has_refresh_token(auth: &AuthState) -> bool {
        auth.refresh_token
            .as_deref()
            .map(str::trim)
            .is_some_and(|token| !token.is_empty())
    }

    fn has_auth_material(auth: &AuthState) -> bool {
        Self::has_refresh_token(auth)
            || auth
                .jwt
                .as_deref()
                .map(str::trim)
                .is_some_and(|token| !token.is_empty())
    }

    fn global_auth_state_path() -> PathBuf {
        std::env::var_os("G5D_AUTH_STATE")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("G5_HOME")
                    .filter(|value| !value.is_empty())
                    .map(|home| PathBuf::from(home).join("auth_state.json"))
            })
            .or_else(|| {
                std::env::var_os("HOME")
                    .filter(|value| !value.is_empty())
                    .map(|home| PathBuf::from(home).join(".g5").join("auth_state.json"))
            })
            .unwrap_or_else(|| PathBuf::from(".g5").join("auth_state.json"))
    }

    fn load_global_auth_fallback(api_url: &str) -> Option<(PathBuf, AuthState)> {
        let path = Self::global_auth_state_path();
        let auth = AuthState::load_from_path(&path);
        if Self::auth_state_matches_api_url(&auth, api_url) && Self::has_auth_material(&auth) {
            Some((path, auth))
        } else {
            None
        }
    }

    fn env_token() -> Option<String> {
        std::env::var("G5_API_KEY")
            .ok()
            .filter(|s| !s.is_empty())
            .or_else(|| std::env::var("G5D_TOKEN").ok().filter(|s| !s.is_empty()))
    }

    fn env_org_id() -> Option<String> {
        ["G5_ORG_ID", "G5D_ORG_ID", "ORG_ID"]
            .into_iter()
            .find_map(|name| std::env::var(name).ok().filter(|s| !s.is_empty()))
    }

    /// Get the current JWT (if any).
    fn current_jwt(&self) -> Option<String> {
        self.auth.lock().unwrap().jwt.clone()
    }

    /// Get the org_id.
    pub fn org_id(&self) -> Result<String, G5Error> {
        self.auth
            .lock()
            .unwrap()
            .org_id
            .clone()
            .ok_or_else(|| G5Error::Internal {
                message: "No org ID configured. Set G5_ORG_ID or G5D_ORG_ID environment variable."
                    .into(),
            })
    }

    /// GET request with auto-refresh on 401.
    pub async fn get<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T, G5Error> {
        let url = format!("{}{}", self.api_url, path);

        // First attempt
        match self.do_get::<T>(&url).await {
            Ok(val) => Ok(val),
            Err(e) if Self::is_auth_error(&e) => {
                debug!("Got 401, attempting token refresh");
                self.refresh_auth().await?;
                // Retry
                self.do_get::<T>(&url).await
            }
            Err(e) => Err(e),
        }
    }

    /// POST request with auto-refresh on 401.
    pub async fn post<B: Serialize, T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, G5Error> {
        let url = format!("{}{}", self.api_url, path);

        // First attempt
        match self.do_post::<B, T>(&url, body).await {
            Ok(val) => Ok(val),
            Err(e) if Self::is_auth_error(&e) => {
                debug!("Got 401, attempting token refresh");
                self.refresh_auth().await?;
                // Retry
                self.do_post::<B, T>(&url, body).await
            }
            Err(e) => Err(e),
        }
    }

    /// POST a request that reconciles current state (PR create/update),
    /// retrying gateway errors (HTTP 502/503/504) and lost connections a
    /// bounded number of times. Auth refresh follows [`Self::post`].
    pub async fn post_reconciling<B: Serialize, T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, G5Error> {
        const ATTEMPTS: u32 = 4;
        let mut delay = std::time::Duration::from_secs(2);
        for attempt in 1..=ATTEMPTS {
            let gateway = match self.post::<B, T>(path, body).await {
                // `do_post`'s transport failure, or a non-API (gateway) 502-504.
                Err(G5Error::Internal { message })
                    if message.starts_with("API request failed")
                        || ["502", "503", "504"]
                            .iter()
                            .any(|code| message.starts_with(&format!("API returned {code} "))) =>
                {
                    message.chars().take(120).collect::<String>()
                }
                result => return result,
            };
            if attempt == ATTEMPTS {
                return Err(G5Error::ServiceUnavailable {
                    message: format!(
                        "The G5 API gateway failed after {attempt} attempts ({gateway}); the PR may be saved. Run `g5d pr status` before retrying."
                    ),
                });
            }
            tokio::time::sleep(delay).await;
            delay *= 2;
        }
        unreachable!("the last attempt returns")
    }

    /// PATCH request with auto-refresh on 401.
    pub async fn patch<B: Serialize, T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, G5Error> {
        let url = format!("{}{}", self.api_url, path);

        match self.do_patch::<B, T>(&url, body).await {
            Ok(val) => Ok(val),
            Err(e) if Self::is_auth_error(&e) => {
                debug!("Got 401, attempting token refresh");
                self.refresh_auth().await?;
                self.do_patch::<B, T>(&url, body).await
            }
            Err(e) => Err(e),
        }
    }

    /// DELETE request with auto-refresh on 401.
    ///
    /// The g5-api DELETE handlers return the affected resource inside the
    /// standard `{ data: ... }` envelope, so this returns the deserialized
    /// `data` payload like the other verbs.
    pub async fn delete<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T, G5Error> {
        let url = format!("{}{}", self.api_url, path);

        match self.do_delete::<T>(&url).await {
            Ok(val) => Ok(val),
            Err(e) if Self::is_auth_error(&e) => {
                debug!("Got 401, attempting token refresh");
                self.refresh_auth().await?;
                self.do_delete::<T>(&url).await
            }
            Err(e) => Err(e),
        }
    }

    // ── Internal request methods ────────────────────────────────────────

    async fn do_get<T: serde::de::DeserializeOwned>(&self, url: &str) -> Result<T, G5Error> {
        let mut req = self.http.get(url);
        if let Some(token) = self.current_jwt() {
            req = req.bearer_auth(&token);
        }
        let resp = req.send().await.map_err(|e| G5Error::Internal {
            message: format!("API request failed: {}", e),
        })?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(self.status_to_error(status, &body));
        }
        let envelope: ResponseEnvelope<T> = resp.json().await.map_err(|e| G5Error::Internal {
            message: format!("Failed to parse API response: {}", e),
        })?;
        Ok(envelope.data)
    }

    async fn do_post<B: Serialize, T: serde::de::DeserializeOwned>(
        &self,
        url: &str,
        body: &B,
    ) -> Result<T, G5Error> {
        let mut req = self.http.post(url).json(body);
        if let Some(token) = self.current_jwt() {
            req = req.bearer_auth(&token);
        }
        let resp = req.send().await.map_err(|e| G5Error::Internal {
            message: format!("API request failed: {}", e),
        })?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(self.status_to_error(status, &body));
        }
        let envelope: ResponseEnvelope<T> = resp.json().await.map_err(|e| G5Error::Internal {
            message: format!("Failed to parse API response: {}", e),
        })?;
        Ok(envelope.data)
    }

    async fn do_patch<B: Serialize, T: serde::de::DeserializeOwned>(
        &self,
        url: &str,
        body: &B,
    ) -> Result<T, G5Error> {
        let mut req = self.http.patch(url).json(body);
        if let Some(token) = self.current_jwt() {
            req = req.bearer_auth(&token);
        }
        let resp = req.send().await.map_err(|e| G5Error::Internal {
            message: format!("API request failed: {}", e),
        })?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(self.status_to_error(status, &body));
        }
        let envelope: ResponseEnvelope<T> = resp.json().await.map_err(|e| G5Error::Internal {
            message: format!("Failed to parse API response: {}", e),
        })?;
        Ok(envelope.data)
    }

    async fn do_delete<T: serde::de::DeserializeOwned>(&self, url: &str) -> Result<T, G5Error> {
        let mut req = self.http.delete(url);
        if let Some(token) = self.current_jwt() {
            req = req.bearer_auth(&token);
        }
        let resp = req.send().await.map_err(|e| G5Error::Internal {
            message: format!("API request failed: {}", e),
        })?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(self.status_to_error(status, &body));
        }
        let envelope: ResponseEnvelope<T> = resp.json().await.map_err(|e| G5Error::Internal {
            message: format!("Failed to parse API response: {}", e),
        })?;
        Ok(envelope.data)
    }

    fn status_to_error(&self, status: reqwest::StatusCode, body: &str) -> G5Error {
        if let Ok(envelope) = serde_json::from_str::<ApiErrorEnvelope>(body) {
            let api_error = envelope.error;
            let message = if api_error.request_id.is_empty() {
                format!("{}: {}", api_error.code, api_error.message)
            } else {
                format!(
                    "{}: {} (requestId: {})",
                    api_error.code, api_error.message, api_error.request_id
                )
            };
            return match api_error.code.as_str() {
                // A typed conflict (`details.kind`) keeps its details, which
                // callers such as `g5d pr create` act on.
                _ if status == reqwest::StatusCode::CONFLICT
                    && api_error.details["kind"].is_string() =>
                {
                    G5Error::ConflictDetails {
                        reason: message,
                        details: api_error.details,
                    }
                }
                "UNAUTHORIZED" => G5Error::Unauthorized,
                // Surface the read-only envelope code as a distinct worker-side
                // error rather than collapsing it into an opaque `Forbidden`
                // (closes workspace-transition Gap B, spec
                // g5d/worker/implementation_map [D9]). A durable write blocked by
                // a read-only workspace can then fail the task cleanly with a
                // read-only reason instead of retrying opaquely. `operation` is a
                // fixed worker-side label (the shared variant carries a
                // `&'static str`); the specific server operation stays in
                // `message`.
                "WORKSPACE_READ_ONLY" => G5Error::WorkspaceReadOnly {
                    operation: "workspace.write",
                    workspace_id: api_error
                        .details
                        .get("workspaceId")
                        .or_else(|| api_error.details.get("workspace_id"))
                        .and_then(|value| value.as_str())
                        .unwrap_or_default()
                        .to_string(),
                },
                "FORBIDDEN" | "SCOPE_MISSING" => G5Error::Forbidden {
                    missing_scopes: api_error
                        .details
                        .get("missing_scopes")
                        .or_else(|| api_error.details.get("missingScopes"))
                        .and_then(|scopes| scopes.as_array())
                        .map(|scopes| {
                            scopes
                                .iter()
                                .filter_map(|scope| scope.as_str().map(str::to_string))
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default(),
                },
                "NOT_FOUND" | "INSTANCE_NOT_FOUND" | "WORKER_NOT_FOUND" | "FILE_NOT_FOUND" => {
                    G5Error::NotFound { resource: message }
                }
                "CONFLICT" | "STALE_HEAD" | "BRANCH_DIVERGED" | "DRAFT_CONFLICT"
                | "TASK_NOT_ELIGIBLE" | "WORKER_BUSY" | "DRAFT_DIRTY" | "MERGE_CONFLICT" => {
                    G5Error::Conflict { reason: message }
                }
                "VALIDATION_ERROR" => G5Error::ValidationError {
                    details: serde_json::json!({
                        "code": api_error.code,
                        "message": api_error.message,
                        "requestId": api_error.request_id,
                        "details": api_error.details,
                    }),
                },
                "RATE_LIMITED" => G5Error::RateLimited {
                    retry_after_secs: api_error
                        .details
                        .get("retryAfterSecs")
                        .or_else(|| api_error.details.get("retry_after_secs"))
                        .and_then(|value| value.as_u64()),
                },
                "SPEND_LIMIT_REACHED" => G5Error::SpendLimitReached {
                    period_spend_usd: api_error
                        .details
                        .get("periodSpendUsd")
                        .or_else(|| api_error.details.get("period_spend_usd"))
                        .and_then(|value| value.as_f64())
                        .unwrap_or_default(),
                    limit_usd: api_error
                        .details
                        .get("limitUsd")
                        .or_else(|| api_error.details.get("limit_usd"))
                        .and_then(|value| value.as_f64())
                        .unwrap_or_default(),
                },
                "SERVICE_UNAVAILABLE" => G5Error::ServiceUnavailable { message },
                _ if status == reqwest::StatusCode::UNAUTHORIZED => G5Error::Unauthorized,
                _ if status == reqwest::StatusCode::FORBIDDEN => G5Error::Forbidden {
                    missing_scopes: Vec::new(),
                },
                _ if status == reqwest::StatusCode::NOT_FOUND => {
                    G5Error::NotFound { resource: message }
                }
                _ if status == reqwest::StatusCode::CONFLICT => {
                    G5Error::Conflict { reason: message }
                }
                _ if status == reqwest::StatusCode::UNPROCESSABLE_ENTITY => {
                    G5Error::ValidationError {
                        details: serde_json::json!({
                            "code": api_error.code,
                            "message": api_error.message,
                            "requestId": api_error.request_id,
                            "details": api_error.details,
                        }),
                    }
                }
                _ => G5Error::Internal { message },
            };
        }
        if status == reqwest::StatusCode::UNAUTHORIZED {
            G5Error::Unauthorized
        } else {
            G5Error::Internal {
                message: format!("API returned {} — {}", status, body),
            }
        }
    }

    fn is_auth_error(err: &G5Error) -> bool {
        matches!(err, G5Error::Unauthorized)
    }

    // ── Token refresh ───────────────────────────────────────────────────

    /// Attempt to refresh the JWT. Tries in order:
    /// 1. POST /api/v1/auth/refresh (if we have a refresh_token)
    /// 2. POST /api/v1/auth/test-token (dev/E2E fallback)
    async fn refresh_auth(&self) -> Result<(), G5Error> {
        let (refresh_token, org_id) = {
            let auth = self.auth.lock().unwrap();
            (auth.refresh_token.clone(), auth.org_id.clone())
        };

        let mut refresh_failure = None;

        // Try refresh token first
        if let Some(ref rt) = refresh_token {
            let url = format!("{}/api/v1/auth/refresh", self.api_url);
            let body = RefreshRequest {
                refresh_token: rt.clone(),
                org_id: org_id.clone(),
            };
            let resp = self.http.post(&url).json(&body).send().await;
            match resp {
                Ok(resp) if resp.status().is_success() => {
                    match resp.json::<ResponseEnvelope<AuthTokensResponse>>().await {
                        Ok(envelope) => {
                            info!("JWT refreshed via refresh token");
                            let mut auth = self.auth.lock().unwrap();
                            auth.jwt = Some(envelope.data.jwt);
                            auth.refresh_token = Some(envelope.data.refresh_token);
                            auth.save_to_path(&self.auth_state_path)?;
                            return Ok(());
                        }
                        Err(e) => {
                            refresh_failure = Some(G5Error::Internal {
                                message: format!("Failed to parse auth response: {e}"),
                            });
                        }
                    }
                }
                Ok(resp) => {
                    let status = resp.status();
                    let body = resp.text().await.unwrap_or_default();
                    refresh_failure = Some(self.status_to_error(status, &body));
                }
                Err(e) => {
                    refresh_failure = Some(G5Error::Internal {
                        message: format!("Token refresh failed: {e}"),
                    });
                }
            }
            warn!("Refresh token failed");
        }

        // Production-safe default: missing/unset/anything-other-than-"true" → false.
        let allow_test_token = g5_contracts::env::is_env_flag_enabled("G5_E2E_AUTH")
            || g5_contracts::env::is_env_flag_enabled("G5D_ALLOW_TEST_TOKEN_FALLBACK");

        if !allow_test_token {
            if let Some(refresh_failure) = refresh_failure {
                return Err(refresh_failure);
            }
            return Err(G5Error::Internal {
                message: "No local refresh token is available. Run `g5d auth login` to authorize this workspace.".to_string(),
            });
        }

        // Fallback: test-token (dev/E2E mode)
        let url = format!("{}/api/v1/auth/test-token", self.api_url);
        let body = TestTokenRequest {
            email: None,
            org_id: org_id.clone(),
        };
        let resp =
            self.http
                .post(&url)
                .json(&body)
                .send()
                .await
                .map_err(|e| G5Error::Internal {
                    message: format!("Token refresh failed: {}", e),
                })?;

        if !resp.status().is_success() {
            let _body = resp.text().await.unwrap_or_default();
            return Err(G5Error::Unauthorized);
        }

        let envelope: ResponseEnvelope<AuthTokensResponse> =
            resp.json().await.map_err(|e| G5Error::Internal {
                message: format!("Failed to parse auth response: {}", e),
            })?;

        info!("JWT obtained via test-token endpoint");
        let mut auth = self.auth.lock().unwrap();
        auth.jwt = Some(envelope.data.jwt);
        auth.refresh_token = Some(envelope.data.refresh_token);
        auth.save_to_path(&self.auth_state_path)?;
        Ok(())
    }

    /// Bootstrap auth if no JWT is available yet.
    /// Called before the first API request to ensure we have a token.
    pub async fn ensure_auth(&self) -> Result<(), G5Error> {
        if self.current_jwt().is_some() {
            return Ok(());
        }
        info!("No JWT available, bootstrapping auth");
        self.refresh_auth().await
    }

    /// Ensure auth is available and return the current JWT.
    pub async fn ensure_auth_token(&self) -> Result<String, G5Error> {
        self.ensure_auth().await?;
        self.current_jwt().ok_or(G5Error::Unauthorized)
    }

    /// Refresh auth and return the new current JWT.
    ///
    /// This is useful before non-API transports (for example smart-HTTP git)
    /// where we cannot observe a 401 response and retry through this client.
    pub async fn refresh_auth_token(&self) -> Result<String, G5Error> {
        self.refresh_auth().await?;
        self.current_jwt().ok_or(G5Error::Unauthorized)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use serial_test::serial;

    struct EnvGuard {
        name: &'static str,
        previous: Option<std::ffi::OsString>,
    }

    impl EnvGuard {
        fn set(name: &'static str, value: Option<&std::path::Path>) -> Self {
            let previous = std::env::var_os(name);
            match value {
                Some(value) => std::env::set_var(name, value),
                None => std::env::remove_var(name),
            }
            Self { name, previous }
        }

        fn set_str(name: &'static str, value: Option<&str>) -> Self {
            let previous = std::env::var_os(name);
            match value {
                Some(value) => std::env::set_var(name, value),
                None => std::env::remove_var(name),
            }
            Self { name, previous }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match self.previous.as_ref() {
                Some(value) => std::env::set_var(self.name, value),
                None => std::env::remove_var(self.name),
            }
        }
    }

    #[test]
    fn auth_state_save_returns_error_when_auth_directory_cannot_be_created() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join(".g5"), "not a directory").expect("write blocking .g5 file");

        let mut state = AuthState::default();
        state.set_authenticated(
            "jwt".to_string(),
            "refresh".to_string(),
            "http://localhost:8080".to_string(),
            None,
            None,
        );

        let result = state.save(dir.path());
        assert!(
            matches!(
                result,
                Err(G5Error::Internal { message })
                    if message.contains("Failed to create auth state directory")
            ),
            "auth state save should return the filesystem error instead of ignoring it"
        );
    }

    #[test]
    fn auth_state_set_authenticated_replaces_stale_org_and_workspace_metadata() {
        let mut state = AuthState::default();
        state.set_authenticated(
            "jwt-a".to_string(),
            "refresh-a".to_string(),
            "http://localhost:8080".to_string(),
            Some("org-a".to_string()),
            Some("workspace-a".to_string()),
        );

        state.set_authenticated(
            "jwt-b".to_string(),
            "refresh-b".to_string(),
            "http://localhost:8080".to_string(),
            None,
            Some("workspace-b".to_string()),
        );

        assert_eq!(state.jwt(), Some("jwt-b"));
        assert_eq!(state.refresh_token(), Some("refresh-b"));
        assert_eq!(state.org_id(), None);
        assert_eq!(state.workspace_id(), Some("workspace-b"));
    }

    #[test]
    fn status_to_error_parses_api_validation_envelope() {
        let workspace = tempfile::tempdir().expect("workspace tempdir");
        let client = ApiClient::new("https://dev.g5labs.ai", workspace.path());
        let body = r#"{
            "error": {
                "code": "VALIDATION_ERROR",
                "message": "Spec node violates a unique database constraint",
                "details": {
                    "componentId": "g5/example",
                    "path": ".semlift/specs/g5/example.semlift.md",
                    "constraint": "ontology_nodes_repo_id_parent_id_name_key"
                },
                "requestId": "req_535"
            }
        }"#;

        let error = client.status_to_error(reqwest::StatusCode::UNPROCESSABLE_ENTITY, body);

        match error {
            G5Error::ValidationError { details } => {
                assert_eq!(details["code"], "VALIDATION_ERROR");
                assert_eq!(
                    details["details"]["componentId"],
                    serde_json::json!("g5/example")
                );
                assert_eq!(details["requestId"], "req_535");
            }
            other => panic!("expected validation error, got {other:?}"),
        }
    }

    #[test]
    fn status_to_error_preserves_non_validation_api_codes() {
        let workspace = tempfile::tempdir().expect("workspace tempdir");
        let client = ApiClient::new("https://dev.g5labs.ai", workspace.path());
        let not_found = r#"{
            "error": {
                "code": "WORKER_NOT_FOUND",
                "message": "Worker w-1 not found",
                "details": {},
                "requestId": "req_nf"
            }
        }"#;
        let conflict = r#"{
            "error": {
                "code": "TASK_NOT_ELIGIBLE",
                "message": "Task cannot transition",
                "details": {"currentStatus": "completed"},
                "requestId": "req_conflict"
            }
        }"#;

        assert!(matches!(
            client.status_to_error(reqwest::StatusCode::NOT_FOUND, not_found),
            G5Error::NotFound { .. }
        ));
        assert!(matches!(
            client.status_to_error(reqwest::StatusCode::CONFLICT, conflict),
            G5Error::Conflict { .. }
        ));
    }

    #[test]
    fn status_to_error_falls_back_to_http_status_for_unknown_api_code() {
        let workspace = tempfile::tempdir().expect("workspace tempdir");
        let client = ApiClient::new("https://dev.g5labs.ai", workspace.path());
        let body = r#"{
            "error": {
                "code": "NEW_VALIDATION_CODE",
                "message": "Future validation-style server code",
                "details": {"field": "name"},
                "requestId": "req_future"
            }
        }"#;

        let error = client.status_to_error(reqwest::StatusCode::UNPROCESSABLE_ENTITY, body);

        match error {
            G5Error::ValidationError { details } => {
                assert_eq!(details["code"], "NEW_VALIDATION_CODE");
                assert_eq!(details["details"]["field"], serde_json::json!("name"));
                assert_eq!(details["requestId"], "req_future");
            }
            other => panic!("expected validation fallback from HTTP 422, got {other:?}"),
        }
    }

    #[test]
    fn auth_state_save_writes_auth_state_with_restrictive_permissions() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut state = AuthState::default();
        state.set_authenticated(
            "jwt".to_string(),
            "refresh".to_string(),
            "http://localhost:8080".to_string(),
            Some("org".to_string()),
            Some("workspace".to_string()),
        );

        state.save(dir.path()).expect("save auth state");

        let path = dir.path().join(AUTH_STATE_FILE);
        let body = std::fs::read_to_string(&path).expect("read saved auth state");
        let json: serde_json::Value = serde_json::from_str(&body).expect("auth state json");
        assert_eq!(json["jwt"], "jwt");
        assert_eq!(json["refreshToken"], "refresh");
        assert_eq!(json["authMode"], "device");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            let mode = std::fs::metadata(&path)
                .expect("auth state metadata")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600);
        }
    }

    #[test]
    #[serial]
    fn api_client_uses_global_auth_fallback_when_workspace_has_no_refresh_token() {
        let workspace = tempfile::tempdir().expect("workspace tempdir");
        let global_dir = tempfile::tempdir().expect("global auth tempdir");
        let global_path = global_dir.path().join("auth_state.json");
        let _auth_path = EnvGuard::set("G5D_AUTH_STATE", Some(&global_path));
        let _g5d_token = EnvGuard::set("G5D_TOKEN", None);
        let _g5_api_key = EnvGuard::set("G5_API_KEY", None);

        let mut global_state = AuthState::default();
        global_state.set_authenticated(
            "global-jwt".to_string(),
            "global-refresh".to_string(),
            "https://dev.g5labs.ai".to_string(),
            Some("org-1".to_string()),
            Some("workspace-1".to_string()),
        );
        global_state
            .save_to_path(&global_path)
            .expect("save global auth state");

        let client = ApiClient::new("https://dev.g5labs.ai/", workspace.path());

        assert_eq!(client.current_jwt().as_deref(), Some("global-jwt"));
        assert_eq!(client.auth_state_path, global_path);
    }

    #[test]
    #[serial]
    fn api_client_keeps_workspace_auth_when_workspace_has_refresh_token() {
        let workspace = tempfile::tempdir().expect("workspace tempdir");
        let global_dir = tempfile::tempdir().expect("global auth tempdir");
        let global_path = global_dir.path().join("auth_state.json");
        let _auth_path = EnvGuard::set("G5D_AUTH_STATE", Some(&global_path));
        let _g5d_token = EnvGuard::set("G5D_TOKEN", None);
        let _g5_api_key = EnvGuard::set("G5_API_KEY", None);

        let mut workspace_state = AuthState::default();
        workspace_state.set_authenticated(
            "workspace-jwt".to_string(),
            "workspace-refresh".to_string(),
            "https://dev.g5labs.ai".to_string(),
            Some("org-1".to_string()),
            Some("workspace-local".to_string()),
        );
        workspace_state
            .save(workspace.path())
            .expect("save workspace auth state");

        let mut global_state = AuthState::default();
        global_state.set_authenticated(
            "global-jwt".to_string(),
            "global-refresh".to_string(),
            "https://dev.g5labs.ai".to_string(),
            Some("org-1".to_string()),
            Some("workspace-remote".to_string()),
        );
        global_state
            .save_to_path(&global_path)
            .expect("save global auth state");

        let client = ApiClient::new("https://dev.g5labs.ai", workspace.path());

        assert_eq!(client.current_jwt().as_deref(), Some("workspace-jwt"));
        assert_eq!(
            client.auth_state_path,
            workspace.path().join(AUTH_STATE_FILE)
        );
    }

    #[test]
    #[serial]
    fn api_client_keeps_auth_state_org_when_bare_org_env_is_set() {
        let workspace = tempfile::tempdir().expect("workspace tempdir");
        let global_dir = tempfile::tempdir().expect("global auth tempdir");
        let global_path = global_dir.path().join("auth_state.json");
        let _auth_path = EnvGuard::set("G5D_AUTH_STATE", Some(&global_path));
        let _g5d_token = EnvGuard::set_str("G5D_TOKEN", None);
        let _g5_api_key = EnvGuard::set_str("G5_API_KEY", None);
        let _g5_org_id = EnvGuard::set_str("G5_ORG_ID", None);
        let _g5d_org_id = EnvGuard::set_str("G5D_ORG_ID", Some("stale-org"));
        let _org_id = EnvGuard::set_str("ORG_ID", None);

        let mut workspace_state = AuthState::default();
        workspace_state.set_authenticated(
            "workspace-jwt".to_string(),
            "workspace-refresh".to_string(),
            "https://dev.g5labs.ai".to_string(),
            Some("auth-state-org".to_string()),
            Some("workspace-local".to_string()),
        );
        workspace_state
            .save(workspace.path())
            .expect("save workspace auth state");

        let client = ApiClient::new("https://dev.g5labs.ai", workspace.path());

        assert_eq!(
            client.auth.lock().unwrap().org_id.as_deref(),
            Some("auth-state-org")
        );
    }

    #[test]
    #[serial]
    fn api_client_keeps_bare_org_env_override_for_non_refresh_auth() {
        let workspace = tempfile::tempdir().expect("workspace tempdir");
        let global_dir = tempfile::tempdir().expect("global auth tempdir");
        let global_path = global_dir.path().join("auth_state.json");
        let _auth_path = EnvGuard::set("G5D_AUTH_STATE", Some(&global_path));
        let _g5d_token = EnvGuard::set_str("G5D_TOKEN", None);
        let _g5_api_key = EnvGuard::set_str("G5_API_KEY", None);
        let _g5_org_id = EnvGuard::set_str("G5_ORG_ID", None);
        let _g5d_org_id = EnvGuard::set_str("G5D_ORG_ID", Some("env-org"));
        let _org_id = EnvGuard::set_str("ORG_ID", None);

        let workspace_state = AuthState {
            jwt: Some("workspace-jwt".to_string()),
            refresh_token: None,
            api_url: Some("https://dev.g5labs.ai".to_string()),
            web_url: None,
            org_id: Some("auth-state-org".to_string()),
            workspace_id: Some("workspace-local".to_string()),
            auth_mode: Some("legacy-refresh".to_string()),
        };
        workspace_state
            .save(workspace.path())
            .expect("save workspace auth state");

        let client = ApiClient::new("https://dev.g5labs.ai", workspace.path());

        assert_eq!(
            client.auth.lock().unwrap().org_id.as_deref(),
            Some("env-org")
        );
    }

    #[test]
    #[serial]
    fn api_client_pairs_org_env_with_env_token() {
        let workspace = tempfile::tempdir().expect("workspace tempdir");
        let global_dir = tempfile::tempdir().expect("global auth tempdir");
        let global_path = global_dir.path().join("auth_state.json");
        let _auth_path = EnvGuard::set("G5D_AUTH_STATE", Some(&global_path));
        let _g5d_token = EnvGuard::set_str("G5D_TOKEN", Some("env-jwt"));
        let _g5_api_key = EnvGuard::set_str("G5_API_KEY", None);
        let _g5_org_id = EnvGuard::set_str("G5_ORG_ID", None);
        let _g5d_org_id = EnvGuard::set_str("G5D_ORG_ID", Some("env-org"));
        let _org_id = EnvGuard::set_str("ORG_ID", None);

        let mut workspace_state = AuthState::default();
        workspace_state.set_authenticated(
            "workspace-jwt".to_string(),
            "workspace-refresh".to_string(),
            "https://dev.g5labs.ai".to_string(),
            Some("auth-state-org".to_string()),
            Some("workspace-local".to_string()),
        );
        workspace_state
            .save(workspace.path())
            .expect("save workspace auth state");

        let client = ApiClient::new("https://dev.g5labs.ai", workspace.path());
        let auth = client.auth.lock().unwrap();

        assert_eq!(auth.jwt.as_deref(), Some("env-jwt"));
        assert_eq!(auth.org_id.as_deref(), Some("env-org"));
    }
}
