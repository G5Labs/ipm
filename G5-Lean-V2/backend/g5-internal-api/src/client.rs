//! HTTP client for calling g5-api's `/api/internal/*` endpoints.
//!
//! Provides [`InternalApiClient`], a typed reqwest wrapper with automatic
//! worker session JWT authentication and retry logic for transient failures.
//!
//! This module is gated behind the `http-client` feature flag so that
//! crates that only need the DTOs (e.g., g5-api) can skip the reqwest
//! dependency.

use std::sync::{Arc, RwLock};
use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use tracing::warn;

use super::dtos::*;
use g5_contracts::error::G5Error;

/// Maximum number of retry attempts for transient failures.
const MAX_RETRIES: u32 = 3;

/// Base backoff duration (doubled on each retry: 1s, 2s, 4s).
const BASE_BACKOFF: Duration = Duration::from_secs(1);
/// Attempts of a PR create/update through gateway errors.
const PR_WRITE_ATTEMPTS: u32 = 4;

/// A gateway between the client and the API failed; the API may still run.
fn is_gateway_status(status: reqwest::StatusCode) -> bool {
    matches!(status.as_u16(), 502..=504)
}

/// Per-attempt classification for [`InternalApiClient::send_with_retry`].
///
/// `Transient` attempts (5xx responses, network errors, timeouts) are retried
/// on the shared [`retry_schedule`]; `Permanent` attempts (any other non-2xx
/// response) fail immediately — retrying a logic error cannot succeed.
enum RequestError {
    Transient(G5Error),
    Permanent(G5Error),
}

impl RequestError {
    fn into_g5(self) -> G5Error {
        match self {
            RequestError::Transient(e) | RequestError::Permanent(e) => e,
        }
    }

    fn as_g5(&self) -> &G5Error {
        match self {
            RequestError::Transient(e) | RequestError::Permanent(e) => e,
        }
    }
}

/// The shared bounded-retry schedule for every verb helper: `MAX_RETRIES`
/// retries after the initial attempt, exponential 1s/2s/4s, no jitter.
fn retry_schedule() -> backon::ExponentialBuilder {
    backon::ExponentialBuilder::default()
        .with_min_delay(BASE_BACKOFF)
        .with_factor(2.0)
        .with_max_times(MAX_RETRIES as usize)
}

/// HTTP client for g5-api's internal endpoints.
///
/// A thin typed wrapper around `reqwest::Client` that provides one method
/// per internal API endpoint. Every request includes the current worker
/// session JWT for authentication and a unique `X-Request-ID` header for
/// tracing correlation.
///
/// # Clone
///
/// `InternalApiClient` implements `Clone`. All clones share the same canonical
/// worker-session state, so token rotation and its inspected capability receipt
/// become visible to all API calls as one transition.
#[derive(Debug, Clone)]
pub struct InternalApiClient {
    client: reqwest::Client,
    base_url: String,
    /// Shared bearer token, normally a workspace-incarnation credential.
    token: Arc<RwLock<String>>,
    token_file: Option<std::path::PathBuf>,
}

/// Read a complete credential; atomic publisher replacement prevents partial tokens.
pub fn read_workspace_token(path: &std::path::Path) -> Result<String, G5Error> {
    use std::io::Read;
    let mut token = String::new();
    std::fs::File::open(path)
        .and_then(|file| file.take(16_385).read_to_string(&mut token))
        .map_err(|_| G5Error::Unauthorized)?;
    if token.is_empty() || token.len() > 16_384 || token.chars().any(char::is_whitespace) {
        return Err(G5Error::Unauthorized);
    }
    Ok(token)
}

/// The suffix and expected full-file identity for a fenced append.
pub struct ChatSessionFileAppend<'a> {
    pub suffix: &'a [u8],
    pub full_sha256: &'a str,
    pub base_size: i64,
    pub base_sha256: &'a str,
    pub generation: uuid::Uuid,
}

impl InternalApiClient {
    pub async fn update_codex_auth(
        &self,
        session_id: &str,
        update: &crate::codex_auth::CodexAuthUpdate,
    ) -> Result<crate::codex_auth::CodexAuthDisposition, G5Error> {
        self.post_json(
            &format!(
                "{}/api/internal/chat-sessions/{session_id}/codex-auth",
                self.base_url
            ),
            update,
        )
        .await
    }

    /// Creates a new `InternalApiClient`.
    ///
    /// Construction is infallible — URL validation and token format checking
    /// happen on first use (fail-fast on first call, not on construction).
    ///
    /// The client starts with no auth token. After registration, call
    /// [`update_token`] with the session JWT received from g5-api.
    ///
    /// The client is configured with:
    /// - 30s connection timeout
    /// - 120s total request timeout
    /// - gzip decompression
    /// - shared connection pool
    #[must_use]
    pub fn new(base_url: &str) -> Self {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(30))
            .timeout(Duration::from_secs(120))
            .build()
            .expect("Failed to build reqwest client — this should never happen with default TLS");

        Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            token: Arc::new(RwLock::new(String::new())),
            token_file: None,
        }
    }

    /// Read an atomically replaced, dispatcher-owned credential for each HTTP attempt.
    /// A missing/unreadable file fails closed; never fall back to a boot token.
    #[must_use]
    pub fn with_token_file(mut self, path: impl Into<std::path::PathBuf>) -> Self {
        self.token_file = Some(path.into());
        self
    }

    /// Shared workspace authentication for agent supervisors, MCP and CLI calls.
    /// A file takes precedence over a static token, including when both are present.
    pub fn from_workspace_env(api_url: &str, env: impl Fn(&str) -> Option<String>) -> Option<Self> {
        let present = |name| env(name).filter(|value| !value.trim().is_empty());
        let client = Self::new(api_url);
        if let Some(path) = present("G5D_WORKSPACE_TOKEN_FILE") {
            return Some(client.with_token_file(path));
        }
        let token = present("G5D_WORKSPACE_TOKEN")?;
        client.update_token(&token);
        Some(client)
    }

    /// Replace the current auth token without a response receipt.
    ///
    /// This is used to install a workspace-incarnation credential before the
    /// first in-guest request.
    pub fn update_token(&self, new_token: &str) {
        let mut token = self.token.write().unwrap_or_else(|e| e.into_inner());
        new_token.clone_into(&mut token);
    }

    /// Returns a snapshot of the current auth token.
    ///
    /// After registration, this returns the session JWT. Used by code paths
    /// that need the live token (e.g., WebSocket connections, live workspace file peers).
    pub fn current_token(&self) -> Result<String, G5Error> {
        if let Some(path) = &self.token_file {
            return read_workspace_token(path);
        }
        Ok(self.token.read().unwrap_or_else(|e| e.into_inner()).clone())
    }

    /// Returns the configured g5-api base URL.
    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// One live-only report. No retries: the next native observation supersedes it.
    pub async fn report_subscription_usage(
        &self,
        session_id: &str,
        report: &crate::subscription_usage::SubscriptionUsageReport,
    ) -> Result<(), G5Error> {
        let _: serde_json::Value = self
            .post_json_once(
                &format!(
                    "{}/api/internal/chat-sessions/{session_id}/subscription-usage",
                    self.base_url
                ),
                report,
            )
            .await?;
        Ok(())
    }

    /// Reports LLM usage recorded by the in-guest proxy.
    pub async fn report_usage(
        &self,
        records: &[UsageRecordRequest],
    ) -> Result<UsageAcknowledgement, G5Error> {
        let url = format!("{}/api/internal/usage", self.base_url);
        self.post_json(&url, records).await
    }

    /// Reports a provider quota snapshot recorded by the in-guest proxy.
    pub async fn report_provider_quota(
        &self,
        req: &ProviderQuotaReportRequest,
    ) -> Result<ProviderQuotaAcknowledgement, G5Error> {
        let url = format!("{}/api/internal/provider-quota", self.base_url);
        self.post_json(&url, req).await
    }

    // -----------------------------------------------------------------------
    // Provider session files (design v6 §8.4)
    //
    // Worker-only endpoints under
    // /api/internal/chat-sessions/:id/session-files. File bytes travel
    // zstd-compressed in both directions; URLs are built through
    // `reqwest::Url::query_pairs_mut()` so ids and paths never break the
    // query string.
    // -----------------------------------------------------------------------

    /// GET /api/internal/chat-sessions/:sessionId/session-files/manifest
    pub async fn chat_session_files_manifest(
        &self,
        chat_session_id: &str,
        provider_session_id: &str,
    ) -> Result<ChatSessionFilesManifestResponse, G5Error> {
        let url = self.build_session_files_url(
            chat_session_id,
            "manifest",
            &[("provider_session_id", provider_session_id)],
        )?;
        self.get_json_url(&url).await
    }

    pub async fn published_session_file_bundle(
        &self,
        chat_session_id: &str,
        provider: crate::session_files::SessionFileProvider,
        provider_session_id: &str,
    ) -> Result<Option<crate::dtos::SessionFileBundlePublication>, G5Error> {
        let url = self.build_session_files_url(
            chat_session_id,
            "published",
            &[
                ("provider", provider.as_str()),
                ("provider_session_id", provider_session_id),
            ],
        )?;
        self.get_json_url(&url).await
    }

    pub async fn begin_session_file_bundle(
        &self,
        chat_session_id: &str,
        body: &crate::dtos::SessionFileBundleBegin,
    ) -> Result<(), G5Error> {
        let url = self.build_session_files_url(chat_session_id, "begin", &[])?;
        let _: serde_json::Value = self.post_json(&url, body).await?;
        Ok(())
    }

    pub async fn pending_session_file_bundles(
        &self,
        workspace_id: uuid::Uuid,
        session_id: Option<uuid::Uuid>,
    ) -> Result<Vec<crate::dtos::PendingSessionFileBundle>, G5Error> {
        let path = format!(
            "/api/internal/workspaces/{workspace_id}/session-files/pending{}",
            session_id.map_or_else(String::new, |id| format!("?session_id={id}")),
        );
        self.get_json(&path).await
    }

    pub async fn publish_session_file_bundle(
        &self,
        chat_session_id: &str,
        publication: &crate::dtos::SessionFileBundlePublication,
    ) -> Result<(), G5Error> {
        let url = self.build_session_files_url(chat_session_id, "publish", &[])?;
        let _: serde_json::Value = self.post_json(&url, publication).await?;
        Ok(())
    }

    /// GET /api/internal/chat-sessions/:sessionId/session-files/content
    ///
    /// Returns the decompressed bytes after checking them against the
    /// digest and size the API reports; a mismatch is an error, never
    /// partial content.
    pub async fn chat_session_file_content(
        &self,
        chat_session_id: &str,
        provider_session_id: &str,
        relative_path: &str,
    ) -> Result<crate::session_files::ChatSessionFileContent, G5Error> {
        use crate::session_files::{
            decode_session_file_body, sha256_hex, SessionFileProvider,
            SESSION_FILE_PROVIDER_HEADER, SESSION_FILE_SHA256_HEADER, SESSION_FILE_SIZE_HEADER,
        };
        let url = self.build_session_files_url(
            chat_session_id,
            "content",
            &[
                ("provider_session_id", provider_session_id),
                ("path", relative_path),
            ],
        )?;
        let response = self
            .send_with_retry::<()>(reqwest::Method::GET, &url, None)
            .await?;
        let header = |name: &str| {
            response
                .headers()
                .get(name)
                .and_then(|value| value.to_str().ok())
                .map(str::to_string)
        };
        let integrity_error = |detail: &str| G5Error::SessionFileIntegrity {
            message: format!("session file content for {relative_path:?}: {detail}"),
        };
        let provider = header(SESSION_FILE_PROVIDER_HEADER)
            .as_deref()
            .and_then(SessionFileProvider::parse)
            .ok_or_else(|| integrity_error("missing or unknown provider header"))?;
        let sha256 = header(SESSION_FILE_SHA256_HEADER)
            .ok_or_else(|| integrity_error("missing sha256 header"))?;
        let size = header(SESSION_FILE_SIZE_HEADER)
            .and_then(|value| value.parse::<u64>().ok())
            .ok_or_else(|| integrity_error("missing or invalid size header"))?;
        let body = response.bytes().await.map_err(|e| G5Error::Internal {
            message: format!("session file content for {relative_path:?}: body read failed: {e}"),
        })?;
        let bytes =
            decode_session_file_body(&body).map_err(|error| integrity_error(&error.to_string()))?;
        if bytes.len() as u64 != size || sha256_hex(&bytes) != sha256 {
            return Err(integrity_error(
                "bytes do not match the reported sha256 and size",
            ));
        }
        Ok(crate::session_files::ChatSessionFileContent {
            provider,
            sha256,
            bytes,
        })
    }

    /// PUT /api/internal/chat-sessions/:sessionId/session-files
    ///
    /// Upserts one file. Re-sending the same bytes is a no-op the response
    /// reports as `changed: false`, so a retried upload is safe.
    pub async fn put_chat_session_file(
        &self,
        chat_session_id: &str,
        provider: crate::session_files::SessionFileProvider,
        provider_session_id: &str,
        relative_path: &str,
        bytes: &[u8],
        generation: Option<uuid::Uuid>,
    ) -> Result<PutChatSessionFileResponse, G5Error> {
        self.put_chat_session_file_with_base(
            chat_session_id,
            provider,
            provider_session_id,
            relative_path,
            bytes,
            (None, generation),
        )
        .await
    }

    /// Send only the suffix of a file whose stored prefix matches `base`.
    /// `full_sha256` names the bytes after the append, not the request body.
    pub async fn append_chat_session_file(
        &self,
        chat_session_id: &str,
        provider: crate::session_files::SessionFileProvider,
        provider_session_id: &str,
        relative_path: &str,
        append: ChatSessionFileAppend<'_>,
    ) -> Result<PutChatSessionFileResponse, G5Error> {
        self.put_chat_session_file_with_base(
            chat_session_id,
            provider,
            provider_session_id,
            relative_path,
            append.suffix,
            (
                Some((append.full_sha256, append.base_size, append.base_sha256)),
                Some(append.generation),
            ),
        )
        .await
    }

    async fn put_chat_session_file_with_base(
        &self,
        chat_session_id: &str,
        provider: crate::session_files::SessionFileProvider,
        provider_session_id: &str,
        relative_path: &str,
        bytes: &[u8],
        (base, generation): (Option<(&str, i64, &str)>, Option<uuid::Uuid>),
    ) -> Result<PutChatSessionFileResponse, G5Error> {
        use crate::session_files::{
            encode_session_file_body, sha256_hex, SESSION_FILE_CONTENT_TYPE,
        };
        let sha256 = base
            .map(|(full, _, _)| full.to_string())
            .unwrap_or_else(|| sha256_hex(bytes));
        let generation_text = generation.map(|generation| generation.to_string());
        let base_size_text = base.map(|(_, size, _)| size.to_string());
        let mut query = vec![
            ("provider", provider.as_str()),
            ("provider_session_id", provider_session_id),
            ("path", relative_path),
            ("sha256", sha256.as_str()),
        ];
        if let Some(generation) = generation_text.as_deref() {
            query.push(("generation", generation));
        }
        if let Some((_, _, base_sha256)) = base {
            query.push(("base_sha256", base_sha256));
            query.push((
                "base_size",
                base_size_text.as_deref().expect("append base size"),
            ));
        }
        let url = self.build_session_files_url(chat_session_id, "", &query)?;
        let body = encode_session_file_body(bytes).map_err(|error| G5Error::Internal {
            message: format!("session file {relative_path:?}: {error}"),
        })?;
        let response = self
            .send_request_with_retry(reqwest::Method::PUT, &url, |request| {
                request
                    .header(CONTENT_TYPE, SESSION_FILE_CONTENT_TYPE)
                    .body(body.clone())
            })
            .await?;
        response
            .json::<PutChatSessionFileResponse>()
            .await
            .map_err(|e| G5Error::Internal {
                message: format!("PUT session file {relative_path:?}: failed to decode JSON: {e}"),
            })
    }

    fn build_session_files_url(
        &self,
        chat_session_id: &str,
        leaf: &str,
        pairs: &[(&str, &str)],
    ) -> Result<String, G5Error> {
        let suffix = if leaf.is_empty() {
            String::new()
        } else {
            format!("/{leaf}")
        };
        let raw = format!(
            "{}/api/internal/chat-sessions/{}/session-files{}",
            self.base_url, chat_session_id, suffix
        );
        let mut url = reqwest::Url::parse(&raw).map_err(|e| G5Error::Internal {
            message: format!("Failed to parse session-files URL {raw}: {e}"),
        })?;
        {
            let mut qp = url.query_pairs_mut();
            for (k, v) in pairs {
                qp.append_pair(k, v);
            }
        }
        Ok(url.into())
    }

    /// Fetches stored chat attachment bytes for the worker-owned chat task.
    ///
    /// GET /api/internal/chat-attachments/:attachmentId/content
    pub async fn get_chat_attachment_content(
        &self,
        attachment_id: &str,
    ) -> Result<ChatAttachmentContentResponse, G5Error> {
        let path = format!("/api/internal/chat-attachments/{}/content", attachment_id);
        self.get_json(&path).await
    }

    /// Uploads one agent-produced screenshot as a session attachment
    /// (routes/internal D35; persistence contract chat_service D63).
    ///
    /// POST /api/internal/chat-sessions/:sessionId/attachments
    pub async fn upload_agent_chat_attachment(
        &self,
        session_id: &str,
        req: &UploadAgentChatAttachmentRequest,
    ) -> Result<UploadAgentChatAttachmentResponse, G5Error> {
        // Full URL, not a bare path: `post_json` hands the string straight to
        // reqwest, and a relative URL fails request construction ("builder
        // error") — observed live on the first in-guest screenshot upload.
        let url = format!(
            "{}/api/internal/chat-sessions/{}/attachments",
            self.base_url, session_id
        );
        self.post_json(&url, req).await
    }

    // -----------------------------------------------------------------------
    // Workspace manifest (§38.1155)
    // -----------------------------------------------------------------------

    /// Fetches a workspace manifest via the internal API.
    ///
    /// GET /api/internal/workspaces/:workspaceId/manifest
    ///
    /// Returns the raw JSON value; callers in g5d deserialize into their
    /// own `WorkspaceConfig` type. Uses the shared client's retry logic,
    /// token rotation, and tracing headers.
    pub async fn fetch_manifest_json(
        &self,
        workspace_id: &str,
    ) -> Result<serde_json::Value, G5Error> {
        let path = format!("/api/internal/workspaces/{}/manifest", workspace_id);
        self.get_json(&path).await
    }

    /// Run the UI's create/update PR flow for this VM workspace. A gateway
    /// error (HTTP 502/503/504) or a lost connection is retried a bounded
    /// number of times: the operation reconciles current state and the server
    /// serializes provider synchronization per G5 PR, so a rerun reuses what
    /// an earlier attempt published. A rerun also saves edits made since.
    pub async fn create_workspace_pull_request(
        &self,
        workspace_id: &str,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, G5Error> {
        let url = format!(
            "{}/api/internal/workspaces/{workspace_id}/pull-requests",
            self.base_url
        );
        let mut delay = std::time::Duration::from_secs(2);
        for attempt in 1..=PR_WRITE_ATTEMPTS {
            let sent = self
                .client
                .request(reqwest::Method::POST, &url)
                .headers(self.default_headers()?)
                .json(body)
                .send()
                .await;
            let gateway = match sent {
                Ok(response) if !is_gateway_status(response.status()) => {
                    if !response.status().is_success() {
                        return Err(self.parse_error_response(response).await);
                    }
                    return response.json().await.map_err(|e| G5Error::Internal {
                        message: format!("Failed to deserialize response: {e}"),
                    });
                }
                Ok(response) => {
                    let status = response.status();
                    let text = response.text().await.unwrap_or_default();
                    // The API's own error envelope is an answer, not a gateway.
                    if serde_json::from_str::<serde_json::Value>(&text)
                        .is_ok_and(|value| value.get("error").is_some())
                    {
                        return Err(self.parse_error_response_from_status_and_text(status, text));
                    }
                    format!("HTTP {}", status.as_u16())
                }
                Err(error) if error.is_connect() || error.is_request() => {
                    format!("connection lost: {error}")
                }
                Err(error) => {
                    return Err(G5Error::Internal {
                        message: format!("Network error: {error}"),
                    })
                }
            };
            if attempt == PR_WRITE_ATTEMPTS {
                return Err(G5Error::ServiceUnavailable {
                    message: format!(
                        "The G5 API gateway failed after {attempt} attempts ({gateway}); the PR may be saved. Run `g5d pr status` before retrying."
                    ),
                });
            }
            tracing::warn!(attempt, %gateway, "retrying PR create/update after a gateway error");
            tokio::time::sleep(delay).await;
            delay *= 2;
        }
        unreachable!("the last attempt returns")
    }

    /// Comment on, or reply in a review thread of, a GitHub PR as the
    /// workspace owner (`g5d pr comment`). Retried: the server records the
    /// idempotency key first, so a replay reconciles and never posts twice.
    pub async fn post_pull_request_comment(
        &self,
        workspace_id: &str,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, G5Error> {
        let url = format!(
            "{}/api/internal/workspaces/{workspace_id}/pull-requests/github/comments",
            self.base_url
        );
        self.post_json(&url, body).await
    }

    /// Preview what `g5d pr create` would publish into a target (`g5d pr
    /// create --dry-run`). Writes nothing, so it is retried.
    pub async fn preview_workspace_pull_request(
        &self,
        workspace_id: &str,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, G5Error> {
        let url = format!(
            "{}/api/internal/workspaces/{workspace_id}/pull-requests/preview",
            self.base_url
        );
        self.post_json(&url, body).await
    }

    /// Resolve or unresolve a review thread as the workspace owner (`g5d pr
    /// resolve`). Retried: resolution is idempotent.
    pub async fn resolve_review_thread(
        &self,
        workspace_id: &str,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, G5Error> {
        let url = format!(
            "{}/api/internal/workspaces/{workspace_id}/pull-requests/github/threads/resolve",
            self.base_url
        );
        self.post_json(&url, body).await
    }

    // -----------------------------------------------------------------------
    // External issues for the workspace agent (`g5d issue create|link`,
    // ontology `g5.external-issues.cli`)
    // -----------------------------------------------------------------------

    /// Create an issue in the organization's creation target as the
    /// workspace owner. Not retried here: the server looks for the issue an
    /// earlier attempt with the same key created, but only once GitHub lists
    /// it, so a replay is the agent's explicit rerun, never automatic.
    ///
    /// POST /api/internal/workspaces/:workspaceId/issues
    pub async fn create_issue(
        &self,
        workspace_id: &str,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, G5Error> {
        let url = format!(
            "{}/api/internal/workspaces/{workspace_id}/issues",
            self.base_url
        );
        self.post_json_once(&url, body).await
    }

    /// Run the shared explicit Save changes action as the workspace's agent.
    /// Never replay an uncertain response: another Save could capture later edits.
    pub async fn save_workspace_changes(
        &self,
        workspace_id: &str,
    ) -> Result<serde_json::Value, G5Error> {
        let url = format!(
            "{}/api/internal/workspaces/{workspace_id}/source-control/save",
            self.base_url
        );
        self.post_json_once(&url, &serde_json::json!({})).await
    }

    /// Email the workspace's current owner as its agent (`g5d workspace email-owner`,
    /// ontology `g5.workspace.owner-email`). Retried like any POST: the
    /// server returns the earlier request for a repeated idempotency key, so
    /// a replay never sends twice.
    ///
    /// POST /api/internal/workspaces/:workspaceId/owner-email
    pub async fn email_owner(
        &self,
        workspace_id: &str,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, G5Error> {
        let url = format!(
            "{}/api/internal/workspaces/{workspace_id}/owner-email",
            self.base_url
        );
        self.post_json(&url, body).await
    }

    /// Read a workspace-agent resource:
    /// `GET /api/internal/workspace-agent/{workspace_id}/{path}` (G5-Lean#710).
    /// `path` may carry a query string.
    pub async fn workspace_agent_get(
        &self,
        workspace_id: &str,
        path: &str,
    ) -> Result<serde_json::Value, G5Error> {
        let url = format!(
            "{}/api/internal/workspace-agent/{workspace_id}/{path}",
            self.base_url
        );
        self.get_json_url(&url).await
    }

    /// Run a workspace-agent operation. A keyed (`idempotent`) request is
    /// retried on transient failures; any other is sent once, so an unclear
    /// outcome is reported rather than repeated.
    pub async fn workspace_agent_post(
        &self,
        workspace_id: &str,
        path: &str,
        body: &serde_json::Value,
        idempotent: bool,
    ) -> Result<serde_json::Value, G5Error> {
        let url = format!(
            "{}/api/internal/workspace-agent/{workspace_id}/{path}",
            self.base_url
        );
        if idempotent {
            self.post_json(&url, body).await
        } else {
            self.post_json_once(&url, body).await
        }
    }

    /// Link an existing issue to the workspace as the owner. Not retried: a
    /// repeated link reports `already_linked`.
    ///
    /// POST /api/internal/workspaces/:workspaceId/issue-links
    pub async fn link_issue(
        &self,
        workspace_id: &str,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, G5Error> {
        let url = format!(
            "{}/api/internal/workspaces/{workspace_id}/issue-links",
            self.base_url
        );
        self.post_json_once(&url, body).await
    }

    /// Read an issue as the workspace owner: a link of the workspace or any
    /// issue in the organization's issue domains; `None` is the primary
    /// issue. A read, so it retries like every GET.
    ///
    /// GET /api/internal/workspaces/:workspaceId/issues/view?ref=&comments=
    pub async fn view_issue(
        &self,
        workspace_id: &str,
        reference: Option<&str>,
        comments: bool,
    ) -> Result<serde_json::Value, G5Error> {
        let mut query = vec![("comments", comments.to_string())];
        if let Some(reference) = reference {
            query.push(("ref", reference.to_string()));
        }
        self.get_issue_route(workspace_id, "view", &query).await
    }

    /// The attachments of an issue `view_issue` can read, by place, without
    /// their content.
    ///
    /// GET /api/internal/workspaces/:workspaceId/issues/attachments?ref=&comments=
    pub async fn list_issue_attachments(
        &self,
        workspace_id: &str,
        reference: Option<&str>,
        comments: bool,
    ) -> Result<serde_json::Value, G5Error> {
        let mut query = vec![("comments", comments.to_string())];
        if let Some(reference) = reference {
            query.push(("ref", reference.to_string()));
        }
        self.get_issue_route(workspace_id, "attachments", &query)
            .await
    }

    /// One listed issue attachment, by the handle its listing gave: the
    /// successful response, whose body the caller streams and whose headers
    /// carry its type, kind and declared length. A read, so it retries
    /// until the response starts.
    ///
    /// GET /api/internal/workspaces/:workspaceId/issues/attachments/content?ref=&handle=
    pub async fn issue_attachment_content(
        &self,
        workspace_id: &str,
        reference: Option<&str>,
        handle: &str,
    ) -> Result<reqwest::Response, G5Error> {
        let raw = format!(
            "{}/api/internal/workspaces/{workspace_id}/issues/attachments/content",
            self.base_url
        );
        let mut url = reqwest::Url::parse(&raw).map_err(|e| G5Error::Internal {
            message: format!("Failed to parse attachment URL {raw}: {e}"),
        })?;
        url.query_pairs_mut().append_pair("handle", handle);
        if let Some(reference) = reference {
            url.query_pairs_mut().append_pair("ref", reference);
        }
        // The client's 120 s default covers the body too; outlast the
        // server's largest attachment budget (videos: 180 s).
        self.send_request_with_retry(reqwest::Method::GET, url.as_str(), |request| {
            request.timeout(Duration::from_secs(240))
        })
        .await
    }

    /// Add an image to a G5 issue as the workspace owner (`g5d issue …
    /// --attach`): the primary issue, or the one `reference` names. Retried:
    /// the server keys the upload by the issue, file name and content, so a
    /// replay adds it once.
    ///
    /// POST /api/internal/workspaces/:workspaceId/issues/attachments
    pub async fn upload_issue_attachment(
        &self,
        workspace_id: &str,
        reference: Option<&str>,
        file_name: &str,
        content: &[u8],
    ) -> Result<serde_json::Value, G5Error> {
        use base64::Engine as _;
        let url = format!(
            "{}/api/internal/workspaces/{workspace_id}/issues/attachments",
            self.base_url
        );
        let mut body = serde_json::json!({
            "fileName": file_name,
            "contentBase64": base64::engine::general_purpose::STANDARD.encode(content),
        });
        if let Some(reference) = reference {
            body["ref"] = serde_json::json!(reference);
        }
        self.post_json(&url, &body).await
    }

    /// A GitHub pull request's attachments, listed as for issues
    /// (`g5d pr attachments`).
    ///
    /// GET /api/internal/workspaces/:workspaceId/pull-requests/github/attachments?selector=&repo=&comments=
    pub async fn list_pull_request_attachments(
        &self,
        workspace_id: &str,
        selector: &str,
        repo: Option<&str>,
        comments: bool,
    ) -> Result<serde_json::Value, G5Error> {
        let mut url = self.pull_request_attachments_url(workspace_id, "", selector, repo)?;
        url.query_pairs_mut()
            .append_pair("comments", &comments.to_string());
        self.get_json_url(url.as_str()).await
    }

    /// One listed pull-request attachment, by the handle its listing gave;
    /// streamed like [`Self::issue_attachment_content`].
    ///
    /// GET /api/internal/workspaces/:workspaceId/pull-requests/github/attachments/content?selector=&repo=&handle=
    pub async fn pull_request_attachment_content(
        &self,
        workspace_id: &str,
        selector: &str,
        repo: Option<&str>,
        handle: &str,
    ) -> Result<reqwest::Response, G5Error> {
        let mut url =
            self.pull_request_attachments_url(workspace_id, "/content", selector, repo)?;
        url.query_pairs_mut().append_pair("handle", handle);
        self.send_request_with_retry(reqwest::Method::GET, url.as_str(), |request| {
            request.timeout(Duration::from_secs(240))
        })
        .await
    }

    fn pull_request_attachments_url(
        &self,
        workspace_id: &str,
        leaf: &str,
        selector: &str,
        repo: Option<&str>,
    ) -> Result<reqwest::Url, G5Error> {
        let raw = format!(
            "{}/api/internal/workspaces/{workspace_id}/pull-requests/github/attachments{leaf}",
            self.base_url
        );
        let mut url = reqwest::Url::parse(&raw).map_err(|e| G5Error::Internal {
            message: format!("Failed to parse attachment URL {raw}: {e}"),
        })?;
        url.query_pairs_mut().append_pair("selector", selector);
        if let Some(repo) = repo {
            url.query_pairs_mut().append_pair("repo", repo);
        }
        Ok(url)
    }

    /// Search the organization's issue domains as the workspace owner.
    /// `query` carries `q`, `state`, `assignee`, `repo` and `limit`.
    ///
    /// GET /api/internal/workspaces/:workspaceId/issues/search
    pub async fn search_issues(
        &self,
        workspace_id: &str,
        query: &[(&str, String)],
    ) -> Result<serde_json::Value, G5Error> {
        self.get_issue_route(workspace_id, "search", query).await
    }

    async fn get_issue_route(
        &self,
        workspace_id: &str,
        leaf: &str,
        query: &[(&str, String)],
    ) -> Result<serde_json::Value, G5Error> {
        let raw = format!(
            "{}/api/internal/workspaces/{workspace_id}/issues/{leaf}",
            self.base_url
        );
        let mut url = reqwest::Url::parse(&raw).map_err(|e| G5Error::Internal {
            message: format!("Failed to parse issue URL {raw}: {e}"),
        })?;
        for (key, value) in query {
            url.query_pairs_mut().append_pair(key, value);
        }
        self.get_json_url(url.as_str()).await
    }

    /// End one of the workspace's issue links as the owner. Not retried: a
    /// repeated unlink reports the issue as not linked.
    ///
    /// POST /api/internal/workspaces/:workspaceId/issue-links/unlink
    pub async fn unlink_issue(
        &self,
        workspace_id: &str,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, G5Error> {
        let url = format!(
            "{}/api/internal/workspaces/{workspace_id}/issue-links/unlink",
            self.base_url
        );
        self.post_json_once(&url, body).await
    }

    // -----------------------------------------------------------------------
    // Referenced questions for the workspace agent (`g5d question`,
    // ontology `g5.documents.question.agent-access`)
    // -----------------------------------------------------------------------

    /// List the question sets of the workspace this VM runs, or one set by
    /// its repository-relative `path`. A read, so it retries like every GET.
    ///
    /// GET /api/internal/workspaces/:workspaceId/questions[?path=...]
    pub async fn list_question_sets(
        &self,
        workspace_id: &str,
        path: Option<&str>,
    ) -> Result<serde_json::Value, G5Error> {
        let raw = self.question_sets_url(workspace_id);
        let mut url = reqwest::Url::parse(&raw).map_err(|e| G5Error::Internal {
            message: format!("Failed to parse question-set URL {raw}: {e}"),
        })?;
        if let Some(path) = path {
            url.query_pairs_mut().append_pair("path", path);
        }
        self.get_json_url(url.as_str()).await
    }

    /// Create a question set in the workspace's live docs tree, or add
    /// questions to an existing one and edit the ones the body identifies,
    /// with `agent` provenance. Not retried: the write lands in the working
    /// tree and a replay appends unidentified questions again, so a lost
    /// response comes back as an error and the agent lists the set before
    /// re-running.
    ///
    /// POST /api/internal/workspaces/:workspaceId/questions
    pub async fn create_question_set(
        &self,
        workspace_id: &str,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, G5Error> {
        self.post_json_once(&self.question_sets_url(workspace_id), body)
            .await
    }

    /// Propose an answer to one referenced question as the workspace agent.
    /// Not retried here: the body's idempotency key makes the agent's own
    /// re-run a replay, and a moved revision comes back as
    /// [`G5Error::QuestionConflict`] carrying the current entry.
    ///
    /// POST /api/internal/workspaces/:workspaceId/questions/answer
    pub async fn answer_question(
        &self,
        workspace_id: &str,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, G5Error> {
        let url = format!("{}/answer", self.question_sets_url(workspace_id));
        self.post_json_once(&url, body).await
    }

    fn question_sets_url(&self, workspace_id: &str) -> String {
        format!(
            "{}/api/internal/workspaces/{workspace_id}/questions",
            self.base_url
        )
    }

    // -----------------------------------------------------------------------
    // Generic GET helper (§38.1155)
    // -----------------------------------------------------------------------

    /// GET a JSON resource, returning a deserialized response.
    /// Retries on transient failures (5xx, network, timeout).
    pub async fn get_json<Resp>(&self, path: &str) -> Result<Resp, G5Error>
    where
        Resp: serde::de::DeserializeOwned,
    {
        let url = format!("{}{}", self.base_url, path);
        self.get_json_from_absolute_url(path, &url).await
    }

    /// GET a JSON resource from an already-absolute URL (path + query),
    /// mirroring `get_json` but skipping the base-URL prefix step. Used by
    /// the client methods that build query strings through
    /// `reqwest::Url::query_pairs_mut`. The `label` is used in error messages
    /// and is typically a logical path like `/api/internal/.../manifest`.
    async fn get_json_url<Resp>(&self, url: &str) -> Result<Resp, G5Error>
    where
        Resp: serde::de::DeserializeOwned,
    {
        self.get_json_from_absolute_url(url, url).await
    }

    /// Shared GET-with-retry primitive. Both `get_json` (relative path +
    /// base_url) and `get_json_url` (already-absolute URL) dispatch here so
    /// retry/error-mapping logic stays in a single place — fixes here
    /// automatically apply to both call paths.
    async fn get_json_from_absolute_url<Resp>(
        &self,
        label: &str,
        url: &str,
    ) -> Result<Resp, G5Error>
    where
        Resp: serde::de::DeserializeOwned,
    {
        let response = self
            .send_with_retry::<()>(reqwest::Method::GET, url, None)
            .await?;
        response
            .json::<Resp>()
            .await
            .map_err(|e| G5Error::Internal {
                message: format!("GET {label}: failed to decode JSON: {e}"),
            })
    }

    /// Sends one HTTP request under the shared bounded-retry policy
    /// ([`retry_schedule`]: 1s/2s/4s, `MAX_RETRIES` retries).
    ///
    /// Every verb helper dispatches here so the schedule and the
    /// transient/permanent classification cannot drift between verbs.
    /// Transient failures (5xx responses, network errors, timeouts) are
    /// retried; any other non-2xx response is mapped through
    /// `parse_error_response` and returned immediately. Success returns the
    /// raw [`reqwest::Response`] for the caller to decode.
    async fn send_with_retry<Req>(
        &self,
        method: reqwest::Method,
        url: &str,
        body: Option<&Req>,
    ) -> Result<reqwest::Response, G5Error>
    where
        Req: serde::Serialize + ?Sized,
    {
        self.send_request_with_retry(method, url, |request| match body {
            Some(body) => request.json(body),
            None => request,
        })
        .await
    }

    /// [`Self::send_with_retry`] for a request whose body is not JSON:
    /// `with_body` adds the body (and its headers) to every attempt.
    async fn send_request_with_retry<F>(
        &self,
        method: reqwest::Method,
        url: &str,
        with_body: F,
    ) -> Result<reqwest::Response, G5Error>
    where
        F: Fn(reqwest::RequestBuilder) -> reqwest::RequestBuilder,
    {
        use backon::Retryable;

        let op = || async {
            let request = with_body(
                self.client
                    .request(method.clone(), url)
                    .headers(self.default_headers().map_err(RequestError::Permanent)?),
            );
            match request.send().await {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        return Ok(response);
                    }
                    let err = self.parse_error_response(response).await;
                    if status.is_server_error() {
                        Err(RequestError::Transient(err))
                    } else {
                        Err(RequestError::Permanent(err))
                    }
                }
                Err(e) => {
                    let message = if e.is_timeout() {
                        format!("Request timeout: {}", e)
                    } else {
                        format!("Network error: {}", e)
                    };
                    Err(RequestError::Transient(G5Error::Internal { message }))
                }
            }
        };

        op.retry(retry_schedule())
            .when(|err: &RequestError| matches!(err, RequestError::Transient(_)))
            .notify(|err: &RequestError, backoff: Duration| {
                warn!(
                    url = url,
                    backoff_ms = backoff.as_millis() as u64,
                    error = %err.as_g5(),
                    "Transient internal API error, will retry"
                );
            })
            .await
            .map_err(RequestError::into_g5)
    }

    // -----------------------------------------------------------------------
    // Internal HTTP helpers
    // -----------------------------------------------------------------------

    /// Builds the default headers for every request.
    fn default_headers(&self) -> Result<HeaderMap, G5Error> {
        let mut headers = HeaderMap::new();
        let token = self.current_token()?;
        let auth_value = format!("Bearer {}", token);
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&auth_value).unwrap_or_else(|_| HeaderValue::from_static("")),
        );
        // X-Request-ID for tracing correlation
        let request_id = uuid::Uuid::now_v7().to_string();
        if let Ok(val) = HeaderValue::from_str(&request_id) {
            headers.insert("X-Request-ID", val);
        }
        Ok(headers)
    }

    /// One POST with no retry. A review decision notifies people and can
    /// write the working tree on every call, so a lost response must come
    /// back as an error the tool reports rather than be replayed.
    async fn post_json_once<Req, Resp>(&self, url: &str, body: &Req) -> Result<Resp, G5Error>
    where
        Req: serde::Serialize + ?Sized,
        Resp: serde::de::DeserializeOwned,
    {
        let response = self
            .client
            .request(reqwest::Method::POST, url)
            .headers(self.default_headers()?)
            .json(body)
            .send()
            .await
            .map_err(|e| G5Error::Internal {
                message: if e.is_timeout() {
                    format!("Request timeout: {}", e)
                } else {
                    format!("Network error: {}", e)
                },
            })?;
        if !response.status().is_success() {
            return Err(self.parse_error_response(response).await);
        }
        response
            .json::<Resp>()
            .await
            .map_err(|e| G5Error::Internal {
                message: format!("Failed to deserialize response: {}", e),
            })
    }

    /// POST with JSON body, expecting a JSON response of type `T`.
    /// Retries on transient failures (5xx, network, timeout).
    async fn post_json<Req, Resp>(&self, url: &str, body: &Req) -> Result<Resp, G5Error>
    where
        Req: serde::Serialize + ?Sized,
        Resp: serde::de::DeserializeOwned,
    {
        let response = self
            .send_with_retry(reqwest::Method::POST, url, Some(body))
            .await?;
        response
            .json::<Resp>()
            .await
            .map_err(|e| G5Error::Internal {
                message: format!("Failed to deserialize response: {}", e),
            })
    }

    /// Parses a non-2xx response into a `G5Error`.
    ///
    /// Attempts to parse the standard error envelope. Falls back to a
    /// generic error if the response body cannot be parsed.
    async fn parse_error_response(&self, response: reqwest::Response) -> G5Error {
        let status = response.status();
        let body = response
            .text()
            .await
            .unwrap_or_else(|e| format!("<body read failed: {e}>"));
        self.parse_error_response_from_status_and_text(status, body)
    }

    /// Parses a non-2xx response from status + body text into a `G5Error`.
    fn parse_error_response_from_status_and_text(
        &self,
        status: reqwest::StatusCode,
        body: String,
    ) -> G5Error {
        // Try to parse the standard error envelope. The G5 API wraps error
        // bodies as `{"error": {code, message, details, requestId}}`
        // (`g5-api` `ErrorEnvelope`), so accept both the wrapped envelope and a
        // bare `ErrorResponse`. Without the wrapped path a real FORBIDDEN body
        // carrying `details.missing_scopes` fails to parse and falls back to an
        // empty `missing_scopes`, erasing the scope diagnosis D6 promises to
        // preserve.
        let parsed_envelope = serde_json::from_str::<g5_contracts::error::ErrorResponse>(&body)
            .ok()
            .or_else(|| {
                serde_json::from_str::<serde_json::Value>(&body)
                    .ok()
                    .and_then(|value| value.get("error").cloned())
                    .and_then(|inner| {
                        serde_json::from_value::<g5_contracts::error::ErrorResponse>(inner).ok()
                    })
            });
        if let Some(envelope) = parsed_envelope {
            let envelope_message = format!("{} — {}", envelope.code, envelope.message);
            return match status {
                reqwest::StatusCode::UNAUTHORIZED => G5Error::Unauthorized,
                reqwest::StatusCode::FORBIDDEN
                    if envelope.code == "AGENT_OTHER_WORKSPACES_DISABLED" =>
                {
                    G5Error::AgentOtherWorkspacesDisabled
                }
                reqwest::StatusCode::FORBIDDEN => {
                    let missing_scopes = envelope
                        .details
                        .get("missing_scopes")
                        .and_then(serde_json::Value::as_array)
                        .map(|values| {
                            values
                                .iter()
                                .filter_map(serde_json::Value::as_str)
                                .map(ToOwned::to_owned)
                                .collect()
                        })
                        .unwrap_or_default();
                    G5Error::Forbidden { missing_scopes }
                }
                reqwest::StatusCode::NOT_FOUND => G5Error::NotFound {
                    resource: envelope_message,
                },
                reqwest::StatusCode::CONFLICT if envelope.code == "STALE_HEAD" => {
                    let details = &envelope.details;
                    let string_field = |snake: &str, camel: &str| {
                        details
                            .get(snake)
                            .or_else(|| details.get(camel))
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or_default()
                            .to_string()
                    };
                    let workspace_id = details
                        .get("workspace_id")
                        .or_else(|| details.get("workspaceId"))
                        .and_then(serde_json::Value::as_str)
                        .map(ToOwned::to_owned);
                    G5Error::StaleHead {
                        worker_git_sha: string_field("worker_git_sha", "workerGitSha"),
                        current_git_sha: string_field("current_git_sha", "currentGitSha"),
                        workspace_id,
                        path: string_field("path", "path"),
                    }
                }
                reqwest::StatusCode::CONFLICT if envelope.code == "AUTHORITY_CUT_STALE" => {
                    match serde_json::from_value(envelope.details) {
                        Ok(details) => G5Error::AuthorityCutStale {
                            details: Box::new(details),
                        },
                        Err(error) => G5Error::Internal {
                            message: format!(
                                "API returned malformed AUTHORITY_CUT_STALE details: {error}"
                            ),
                        },
                    }
                }
                reqwest::StatusCode::CONFLICT if envelope.code == "TASK_NOT_ELIGIBLE" => {
                    let details = &envelope.details;
                    let string_field = |snake: &str, camel: &str| {
                        details
                            .get(snake)
                            .or_else(|| details.get(camel))
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or_default()
                            .to_string()
                    };
                    G5Error::TaskNotEligible {
                        current_status: string_field("current_status", "currentStatus"),
                        requested_status: string_field("requested_status", "requestedStatus"),
                    }
                }
                reqwest::StatusCode::CONFLICT if envelope.code == "QUESTION_CONFLICT" => {
                    G5Error::QuestionConflict {
                        details: envelope.details,
                    }
                }
                // A typed conflict (`details.kind`) keeps its details, which
                // callers such as `g5d pr create` act on.
                reqwest::StatusCode::CONFLICT if envelope.details["kind"].is_string() => {
                    G5Error::ConflictDetails {
                        reason: envelope_message,
                        details: envelope.details,
                    }
                }
                reqwest::StatusCode::CONFLICT => G5Error::Conflict {
                    reason: envelope_message,
                },
                reqwest::StatusCode::BAD_REQUEST => G5Error::ValidationError {
                    details: serde_json::json!({
                        "code": envelope.code,
                        "message": envelope.message,
                        "details": envelope.details,
                        "requestId": envelope.request_id,
                    }),
                },
                reqwest::StatusCode::UNPROCESSABLE_ENTITY => G5Error::ValidationError {
                    details: envelope.details,
                },
                reqwest::StatusCode::TOO_MANY_REQUESTS => {
                    let retry_after_secs = envelope
                        .details
                        .get("retry_after_secs")
                        .or_else(|| envelope.details.get("retryAfterSecs"))
                        .and_then(serde_json::Value::as_u64);
                    G5Error::RateLimited { retry_after_secs }
                }
                _ => G5Error::Internal {
                    message: format!("API error {}: {}", status.as_u16(), envelope_message),
                },
            };
        }

        // Fallback: generic error with status and raw body.
        let body = if body.len() > 200 {
            // Use floor_char_boundary to avoid panic on multi-byte UTF-8.
            let end = body.floor_char_boundary(200);
            format!("{}...", &body[..end])
        } else {
            body
        };
        match status {
            reqwest::StatusCode::UNAUTHORIZED => G5Error::Unauthorized,
            reqwest::StatusCode::FORBIDDEN => G5Error::Forbidden {
                missing_scopes: Vec::new(),
            },
            reqwest::StatusCode::NOT_FOUND => G5Error::NotFound { resource: body },
            reqwest::StatusCode::CONFLICT => G5Error::Conflict { reason: body },
            reqwest::StatusCode::BAD_REQUEST => G5Error::ValidationError {
                details: serde_json::json!({
                    "code": "BAD_REQUEST",
                    "message": body,
                    "details": {},
                    "requestId": "",
                }),
            },
            reqwest::StatusCode::UNPROCESSABLE_ENTITY => G5Error::ValidationError {
                details: serde_json::json!({ "message": body }),
            },
            reqwest::StatusCode::TOO_MANY_REQUESTS => G5Error::RateLimited {
                retry_after_secs: None,
            },
            _ => G5Error::Internal {
                message: format!("API error {}: {}", status.as_u16(), body),
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn explicit_save_never_replays_a_transient_response() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let server = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", server.local_addr().unwrap());
        let request = tokio::spawn(async move {
            let (mut socket, _) = server.accept().await.unwrap();
            let mut bytes = [0; 4096];
            let count = socket.read(&mut bytes).await.unwrap();
            assert!(String::from_utf8_lossy(&bytes[..count])
                .starts_with("POST /api/internal/workspaces/workspace/source-control/save "));
            socket.write_all(b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await.unwrap();
        });
        let client = InternalApiClient::new(&url);
        assert!(tokio::time::timeout(
            Duration::from_secs(2),
            client.save_workspace_changes("workspace")
        )
        .await
        .expect("Save must return the uncertain refusal without waiting to retry")
        .is_err());
        request.await.unwrap();
    }

    #[test]
    fn test_client_construction() {
        let client = InternalApiClient::new("http://localhost:8080");
        assert_eq!(client.base_url, "http://localhost:8080");
        assert_eq!(*client.token.read().unwrap(), "");
    }

    #[test]
    fn test_client_strips_trailing_slash() {
        let client = InternalApiClient::new("http://localhost:8080/");
        assert_eq!(client.base_url, "http://localhost:8080");
    }

    #[test]
    fn test_client_is_clone() {
        let client = InternalApiClient::new("http://localhost:8080");
        let cloned = client.clone();
        assert_eq!(cloned.base_url, client.base_url);
        // Clones share the same token via Arc
        assert_eq!(*cloned.token.read().unwrap(), *client.token.read().unwrap());
    }

    #[test]
    fn test_default_headers_contain_auth() {
        let client = InternalApiClient::new("http://localhost:8080");
        client.update_token("my-secret-token");
        let headers = client.default_headers().unwrap();
        let auth = headers.get(AUTHORIZATION).unwrap().to_str().unwrap();
        assert_eq!(auth, "Bearer my-secret-token");
    }

    #[test]
    fn test_default_headers_contain_request_id() {
        let client = InternalApiClient::new("http://localhost:8080");
        let headers = client.default_headers().unwrap();
        let request_id = headers.get("X-Request-ID").unwrap().to_str().unwrap();
        // UUID v7 format: 8-4-4-4-12 hex chars
        assert_eq!(request_id.len(), 36);
        assert!(request_id.contains('-'));
    }

    #[test]
    fn test_update_token_propagates_to_clones() {
        let client = InternalApiClient::new("http://localhost:8080");
        let cloned = client.clone();

        // Update token on original
        client.update_token("new-session-token");

        // Clone sees the new token (shared via Arc<RwLock>)
        assert_eq!(*cloned.token.read().unwrap(), "new-session-token");

        // Headers use the updated token
        let headers = cloned.default_headers().unwrap();
        let auth = headers.get(AUTHORIZATION).unwrap().to_str().unwrap();
        assert_eq!(auth, "Bearer new-session-token");
    }

    #[test]
    fn test_session_files_url_uses_query_pair_encoding() {
        let client = InternalApiClient::new("http://localhost:8080");
        let url = client
            .build_session_files_url(
                "chat-1",
                "content",
                &[
                    ("provider_session_id", "provider/with?chars"),
                    ("path", "projects/-work/a&b.jsonl"),
                ],
            )
            .expect("session-files URL");
        let parsed = reqwest::Url::parse(&url).expect("parse built url");
        assert_eq!(
            parsed.path(),
            "/api/internal/chat-sessions/chat-1/session-files/content"
        );
        let pairs: std::collections::HashMap<_, _> = parsed.query_pairs().into_owned().collect();
        assert_eq!(
            pairs.get("provider_session_id").map(String::as_str),
            Some("provider/with?chars")
        );
        assert_eq!(
            pairs.get("path").map(String::as_str),
            Some("projects/-work/a&b.jsonl")
        );
        let upsert = client
            .build_session_files_url("chat-1", "", &[("provider", "codex")])
            .expect("upsert URL");
        assert_eq!(
            reqwest::Url::parse(&upsert).unwrap().path(),
            "/api/internal/chat-sessions/chat-1/session-files"
        );
    }

    #[test]
    fn test_parse_error_response_from_envelope_maps_status() {
        let client = InternalApiClient::new("http://localhost:8080");
        let body = serde_json::json!({
            "code": "NOT_FOUND",
            "message": "Resource not found: workspace ws-123",
            "details": {},
            "requestId": "req_001"
        })
        .to_string();

        let err =
            client.parse_error_response_from_status_and_text(reqwest::StatusCode::NOT_FOUND, body);

        match err {
            G5Error::NotFound { resource } => {
                assert!(resource.contains("NOT_FOUND"));
                assert!(resource.contains("workspace ws-123"));
            }
            other => panic!("Expected NotFound variant, got {other:?}"),
        }
    }

    #[test]
    fn bad_request_envelope_preserves_validation_and_request_context() {
        let client = InternalApiClient::new("http://localhost:8080");
        let err = client.parse_error_response_from_status_and_text(
            reqwest::StatusCode::BAD_REQUEST,
            serde_json::json!({
                "error": {
                    "code": "BAD_REQUEST",
                    "message": "Invalid poll request body",
                    "details": { "field": "body", "error": "malformed JSON" },
                    "requestId": "req_bad_poll"
                }
            })
            .to_string(),
        );

        let G5Error::ValidationError { details } = err else {
            panic!("expected validation error, got {err:?}");
        };
        assert_eq!(details["code"], "BAD_REQUEST");
        assert_eq!(details["message"], "Invalid poll request body");
        assert_eq!(details["requestId"], "req_bad_poll");
        assert_eq!(details["details"]["field"], "body");
    }

    #[test]
    fn test_parse_error_response_maps_auth_and_rate_limit_statuses() {
        let client = InternalApiClient::new("http://localhost:8080");

        let unauthorized = client.parse_error_response_from_status_and_text(
            reqwest::StatusCode::UNAUTHORIZED,
            serde_json::json!({
                "code": "UNAUTHORIZED",
                "message": "Missing or invalid authentication token",
                "details": {},
                "requestId": "req_401"
            })
            .to_string(),
        );
        assert!(matches!(unauthorized, G5Error::Unauthorized));

        let forbidden = client.parse_error_response_from_status_and_text(
            reqwest::StatusCode::FORBIDDEN,
            serde_json::json!({
                "code": "FORBIDDEN",
                "message": "Insufficient permissions",
                "details": { "missing_scopes": ["task:owner"] },
                "requestId": "req_403"
            })
            .to_string(),
        );
        match forbidden {
            G5Error::Forbidden { missing_scopes } => {
                assert_eq!(missing_scopes, vec!["task:owner".to_string()]);
            }
            other => panic!("Expected Forbidden variant, got {other:?}"),
        }

        let rate_limited = client.parse_error_response_from_status_and_text(
            reqwest::StatusCode::TOO_MANY_REQUESTS,
            serde_json::json!({
                "code": "RATE_LIMITED",
                "message": "Rate limit exceeded",
                "details": { "retry_after_secs": 9 },
                "requestId": "req_429"
            })
            .to_string(),
        );
        match rate_limited {
            G5Error::RateLimited { retry_after_secs } => {
                assert_eq!(retry_after_secs, Some(9));
            }
            other => panic!("Expected RateLimited variant, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_error_response_unwraps_forbidden_envelope_preserving_scopes() {
        // The G5 API serializes errors wrapped in a top-level "error" key
        // (`g5-api` `ErrorEnvelope`). A real `require_task_owner` rejection
        // therefore arrives wrapped; the client must unwrap it and preserve
        // `details.missing_scopes` rather than falling back to an empty list.
        let client = InternalApiClient::new("http://localhost:8080");

        let forbidden = client.parse_error_response_from_status_and_text(
            reqwest::StatusCode::FORBIDDEN,
            serde_json::json!({
                "error": {
                    "code": "FORBIDDEN",
                    "message": "Insufficient permissions",
                    "details": { "missing_scopes": ["task:owner"] },
                    "requestId": "req_403"
                }
            })
            .to_string(),
        );
        match forbidden {
            G5Error::Forbidden { missing_scopes } => {
                assert_eq!(missing_scopes, vec!["task:owner".to_string()]);
            }
            other => panic!("Expected Forbidden variant, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_error_response_keeps_the_agent_opt_in_refusal() {
        // The agent must learn that its owner's setting refused it, not see a
        // bare permission error (`g5.security.agent-other-workspaces`).
        let client = InternalApiClient::new("http://localhost:8080");
        let refused = client.parse_error_response_from_status_and_text(
            reqwest::StatusCode::FORBIDDEN,
            serde_json::json!({
                "error": {
                    "code": "AGENT_OTHER_WORKSPACES_DISABLED",
                    "message": "not allowed",
                    "details": {},
                    "requestId": "req_403"
                }
            })
            .to_string(),
        );
        assert!(
            matches!(refused, G5Error::AgentOtherWorkspacesDisabled),
            "{refused:?}"
        );
    }

    #[test]
    fn test_parse_error_response_unwraps_stale_head_envelope() {
        // A wrapped CONFLICT/STALE_HEAD body must also unwrap so the worker
        // reconciles against the current head rather than treating it as a
        // generic conflict.
        let client = InternalApiClient::new("http://localhost:8080");

        let err = client.parse_error_response_from_status_and_text(
            reqwest::StatusCode::CONFLICT,
            serde_json::json!({
                "error": {
                    "code": "STALE_HEAD",
                    "message": "stale branch head",
                    "details": {
                        "worker_git_sha": "old",
                        "current_git_sha": "new",
                        "workspace_id": "branch-1",
                        "path": "src/lib.rs"
                    },
                    "requestId": "req_409"
                }
            })
            .to_string(),
        );

        match err {
            G5Error::StaleHead {
                worker_git_sha,
                current_git_sha,
                workspace_id,
                path,
            } => {
                assert_eq!(worker_git_sha, "old");
                assert_eq!(current_git_sha, "new");
                assert_eq!(workspace_id.as_deref(), Some("branch-1"));
                assert_eq!(path, "src/lib.rs");
            }
            other => panic!("Expected StaleHead variant, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_error_response_maps_stale_head_conflict() {
        let client = InternalApiClient::new("http://localhost:8080");

        let err = client.parse_error_response_from_status_and_text(
            reqwest::StatusCode::CONFLICT,
            serde_json::json!({
                "code": "STALE_HEAD",
                "message": "stale branch head",
                "details": {
                    "worker_git_sha": "old",
                    "current_git_sha": "new",
                    "workspaceId": "workspace-1",
                    "path": "src/lib.rs"
                },
                "requestId": "req_409"
            })
            .to_string(),
        );

        match err {
            G5Error::StaleHead {
                worker_git_sha,
                current_git_sha,
                workspace_id,
                path,
            } => {
                assert_eq!(worker_git_sha, "old");
                assert_eq!(current_git_sha, "new");
                assert_eq!(workspace_id.as_deref(), Some("workspace-1"));
                assert_eq!(path, "src/lib.rs");
            }
            other => panic!("Expected StaleHead variant, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_error_response_keeps_the_current_entry_of_a_question_conflict() {
        let client = InternalApiClient::new("http://localhost:8080");

        let err = client.parse_error_response_from_status_and_text(
            reqwest::StatusCode::CONFLICT,
            serde_json::json!({
                "error": {
                    "code": "QUESTION_CONFLICT",
                    "message": "question moved",
                    "details": {
                        "code": "revision-moved",
                        "currentRevision": 3,
                        "current": {"id": "pick", "revision": 3}
                    },
                    "requestId": "req_question_409"
                }
            })
            .to_string(),
        );

        match err {
            G5Error::QuestionConflict { details } => {
                assert_eq!(details["code"], "revision-moved");
                assert_eq!(details["current"]["revision"], 3);
            }
            other => panic!("Expected QuestionConflict variant, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_error_response_preserves_task_not_eligible_statuses() {
        let client = InternalApiClient::new("http://localhost:8080");

        let err = client.parse_error_response_from_status_and_text(
            reqwest::StatusCode::CONFLICT,
            serde_json::json!({
                "error": {
                    "code": "TASK_NOT_ELIGIBLE",
                    "message": "Task cannot transition from 'running' to 'waiting'",
                    "details": {
                        "current_status": "running",
                        "requested_status": "waiting"
                    },
                    "requestId": "req_task_wait_regression"
                }
            })
            .to_string(),
        );

        match err {
            G5Error::TaskNotEligible {
                current_status,
                requested_status,
            } => {
                assert_eq!(current_status, "running");
                assert_eq!(requested_status, "waiting");
            }
            other => panic!("Expected TaskNotEligible variant, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_error_response_maps_unauthorized_to_unauthorized() {
        let client = InternalApiClient::new("http://localhost:8080");

        let err = client.parse_error_response_from_status_and_text(
            reqwest::StatusCode::UNAUTHORIZED,
            serde_json::json!({
                "code": "UNAUTHORIZED",
                "message": "expired session",
                "details": {},
                "requestId": "req_unauth"
            })
            .to_string(),
        );

        assert!(matches!(err, G5Error::Unauthorized));
    }

    #[test]
    fn test_parse_error_response_fallback() {
        let client = InternalApiClient::new("http://localhost:8080");
        let err = client.parse_error_response_from_status_and_text(
            reqwest::StatusCode::INTERNAL_SERVER_ERROR,
            "not json".to_string(),
        );

        match err {
            G5Error::Internal { message } => {
                assert!(message.contains("500"));
                assert!(message.contains("not json"));
            }
            _ => panic!("Expected Internal variant"),
        }
    }

    #[test]
    fn test_parse_error_response_truncates_long_body() {
        let client = InternalApiClient::new("http://localhost:8080");
        let long_body = "x".repeat(500);
        let err = client.parse_error_response_from_status_and_text(
            reqwest::StatusCode::INTERNAL_SERVER_ERROR,
            long_body,
        );

        match err {
            G5Error::Internal { message } => {
                assert!(message.contains("..."));
                // Should be truncated to ~200 chars of body
                assert!(message.len() < 300);
            }
            _ => panic!("Expected Internal variant"),
        }
    }
}
