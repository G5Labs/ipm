// ---------------------------------------------------------------------------
// g5-api/src/routes/mod.rs — Routes Module Root
//
// Responsibilities:
// 1. Submodule declarations — declares all 20 route submodules
// 2. Shared types — ResponseEnvelope, PaginationMeta, Pagination extractor
// 3. Router assembly — build_router() composes the complete axum Router
// ---------------------------------------------------------------------------

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use axum::{
    extract::{ws::Message, Path, Query, State, WebSocketUpgrade},
    response::IntoResponse,
    Extension, Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db::scoped::scoped_db_for_workspace;
use crate::errors::AppError;
use crate::middleware::{request_id::RequestId, AuthContext};
use crate::services::workspace_runtime::WorkspaceActivityReason;
use crate::services::AppState;
use g5_contracts::error::G5Error;
use g5_contracts::terminal::{
    TerminalClientMessage, TerminalRejection, TerminalServerMessage, TERMINAL_OPEN_FRAME_MAX_BYTES,
};

// ─── Submodule declarations ──────────────────────────────────────────────────

pub mod api_keys;
pub mod audited;
pub mod auth;
mod browser_origin;
mod change_graph_live;
pub mod chat;
pub mod comments;
pub mod compute;
pub mod config;
mod cookies;
pub mod docs;
pub mod explainer_templates;
pub mod external_issues;
pub mod files;
pub mod git;
pub mod github_app;
pub mod grounding;
mod guards;
pub mod home;
pub mod instance;
pub mod intent_log;
pub mod internal;
pub mod issue_labels;
pub mod notifications;
mod ontology_changes;
mod ontology_files;
pub mod org_approvals;
pub mod orgs;
pub mod permissions;
pub mod personal;
pub mod platform;
pub mod platform_issues;
pub mod preview;
pub mod preview_profiles;
pub mod provider_accounts;
pub mod pull_request_conversation;
pub mod questions;
pub mod resolve;
pub mod self_hosted;
pub mod settings;
pub mod source_control_browser;
pub mod source_control_change_graph;
pub mod start_and_send;
pub mod tasks;
pub mod telemetry;
pub mod users;
pub mod workbench;
pub mod worker_hosts;
pub mod workspace_agent;
pub mod workspace_approvals;
pub mod workspace_flush;
pub mod workspace_merge_override;
pub mod workspace_preview;
pub mod workspace_review_flow;
pub mod workspace_reviews;
pub mod workspaces;

// ─── Shared Response Types ───────────────────────────────────────────────────

/// Standard response envelope for all successful API responses.
///
/// Wraps the response data in a `{ "data": {...}, "meta"?: {...} }` structure.
/// The `meta` field is only present for paginated list endpoints.
///
/// # JSON Output
///
/// Non-paginated:
/// ```json
/// { "data": { ... } }
/// ```
///
/// Paginated:
/// ```json
/// { "data": [ ... ], "meta": { "page": 1, "perPage": 50, "total": 123 } }
/// ```
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponseEnvelope<T: Serialize> {
    pub data: T,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<PaginationMeta>,
}

impl<T: Serialize> ResponseEnvelope<T> {
    /// Create a response envelope with data only (no pagination metadata).
    pub fn ok(data: T) -> Self {
        Self { data, meta: None }
    }

    /// Create a response envelope with data and pagination metadata.
    pub fn paginated(data: T, meta: PaginationMeta) -> Self {
        Self {
            data,
            meta: Some(meta),
        }
    }
}

/// Pagination metadata included in paginated list responses.
///
/// Serializes to: `{ "page": 1, "perPage": 50, "total": 123 }`
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaginationMeta {
    pub page: i64,
    pub per_page: i64,
    pub total: i64,
}

/// Convenience type alias for paginated list endpoint responses.
pub type PaginatedResponse<T> = Json<ResponseEnvelope<Vec<T>>>;

// ─── Pagination Extractor ────────────────────────────────────────────────────

/// Default page number (1-based).
fn default_page() -> i64 {
    1
}

/// Default page size.
fn default_per_page() -> i64 {
    50
}

/// Custom pagination extractor for list endpoints.
///
/// Parses `page` and `perPage` query parameters with defaults and bounds.
/// Used as `Query<Pagination>` in handler signatures.
///
/// # Defaults and Bounds
///
/// - `page` defaults to `1`, minimum `1`
/// - `perPage` defaults to `50`, clamped to `[1, 100]`
/// - Query parameters use camelCase names (`perPage`, not `per_page`)
///
/// # Example
///
/// ```rust,ignore
/// async fn list_tasks(
///     State(state): State<Arc<AppState>>,
///     auth: AuthContext,
///     Path(org_id): Path<Uuid>,
///     Query(pagination): Query<Pagination>,
/// ) -> Result<Json<ResponseEnvelope<Vec<Task>>>, AppError> {
///     let (items, total) = state.task_service
///         .list_tasks(org_id, pagination.offset(), pagination.limit())
///         .await?;
///     Ok(Json(ResponseEnvelope::paginated(items, pagination.to_meta(total))))
/// }
/// ```
///
/// Reach for this extractor only where paging is actually correct — an
/// append-only history ordered by an immutable key. It is deliberately absent
/// from `list_workspaces`, whose mutable `updated_at` order made offset paging
/// silently skip and duplicate rows; that endpoint returns its whole set.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pagination {
    #[serde(default = "default_page")]
    pub page: i64,
    #[serde(default = "default_per_page")]
    pub per_page: i64,
}

impl Pagination {
    /// Returns the validated page number (minimum 1).
    #[must_use]
    pub fn page(&self) -> i64 {
        self.page.max(1)
    }

    /// Returns the validated per-page count (clamped to 1..=100).
    #[must_use]
    pub fn per_page(&self) -> i64 {
        self.per_page.clamp(1, 100)
    }

    /// Computes the SQL OFFSET for this page.
    #[must_use]
    pub fn offset(&self) -> i64 {
        (self.page() - 1) * self.per_page()
    }

    /// Computes the SQL LIMIT for this page (same as per_page()).
    #[must_use]
    pub fn limit(&self) -> i64 {
        self.per_page()
    }

    /// Builds a [`PaginationMeta`] from this pagination and a total count.
    #[must_use]
    pub fn to_meta(&self, total: i64) -> PaginationMeta {
        PaginationMeta {
            page: self.page(),
            per_page: self.per_page(),
            total,
        }
    }
}

// ─── Router Assembly ─────────────────────────────────────────────────────────

/// Assembles the complete axum Router from all endpoint group sub-routers.
///
/// Each route module exports one or more `router()` functions that return
/// `Router<Arc<AppState>>`. This function nests them under the correct
/// path prefixes and applies shared state.
///
/// The returned router has NO middleware applied. Middleware is added
/// externally by `middleware::build_middleware_stack()` in `main.rs`.
///
/// # Arguments
///
/// * `state` — Shared application state containing all service instances.
///
/// # Path Prefix Mapping
///
/// Public API: `/api/v1/*` (JWT auth)
/// Internal API: `/api/internal/*` (service token auth)
pub fn build_router(state: Arc<AppState>) -> Router {
    let (endpoint_groups, router_description) = if state.is_self_hosted {
        (
            25,
            "Assembling API router: 25 public endpoint groups + internal workspace-incarnation endpoints (self-hosted delivery/admin and worker-host cloud management disabled)",
        )
    } else {
        (
            28,
            "Assembling API router: 28 public endpoint groups + internal workspace-incarnation endpoints + self-hosted delivery/admin endpoints + worker-host cloud management",
        )
    };
    tracing::info!(
        endpoint_groups,
        is_self_hosted = state.is_self_hosted,
        "{router_description}"
    );

    let registry_router = if state.is_self_hosted {
        Router::new()
    } else {
        self_hosted::registry_router()
    };
    let admin_router = if state.is_self_hosted {
        Router::new()
    } else {
        self_hosted::admin_router()
    };
    let worker_hosts_router = if state.is_self_hosted {
        Router::new()
    } else {
        worker_hosts::router()
    };

    Router::new()
        // ── Top-level healthcheck (used by Docker HEALTHCHECK & load balancers) ──
        .route("/healthz", axum::routing::get(|| async { "OK" }))
        // ── Workspaces ──────────────────────────────────────────────
        .nest(
            "/api/v1/orgs/:orgId/workspaces",
            workspaces::org_workspaces_router(),
        )
        .nest("/api/v1/orgs/:orgId/home", home::home_router())
        .nest("/api/v1/orgs/:orgId/me", personal::me_router())
        .nest("/api/v1/orgs/:orgId/users", personal::users_router())
        .nest("/api/v1/orgs/:orgId/compute", compute::router())
        .nest("/api/v1/workspaces", workspaces::workspace_detail_router())
        .nest(
            "/api/v1/workspaces/:workspaceId/preview",
            workspace_preview::router(),
        )
        .nest(
            "/api/v1/workspaces/:workspaceId/workbench",
            workbench::router(),
        )
        // ── Docs ────────────────────────────────────────────────────
        .nest("/api/v1/workspaces/:workspaceId/docs", docs::router())
        // ── Referenced questions ────────────────────────────────────
        .nest(
            "/api/v1/workspaces/:workspaceId/questions",
            questions::router(),
        )
        // ── Preview reverse proxy (PR4 4b) ───────────────────────────
        // Browser-session-authenticated passthrough to a preview microVM over
        // the host's outbound reverse tunnel. Mounted as a sibling of the task
        // routes (it keys on workspaceId but proxies arbitrary app paths/methods).
        .nest("/api/v1/preview", preview::router())
        // ── Tasks ───────────────────────────────────────────────────
        .nest(
            "/api/v1/workspaces/:workspaceId/tasks",
            tasks::workspace_tasks_router(),
        )
        .nest("/api/v1/tasks", tasks::task_detail_router())
        // ── Comments ────────────────────────────────────────────────
        .nest(
            "/api/v1/workspaces/:workspaceId/comments",
            comments::workspace_comments_router(),
        )
        .nest("/api/v1/comments", comments::comment_detail_router())
        // ── Notifications ───────────────────────────────────────────
        .nest(
            "/api/v1/workspaces/:workspaceId/notifications",
            notifications::workspace_notifications_router(),
        )
        .nest(
            "/api/v1/notifications",
            notifications::notification_detail_router(),
        )
        // ── Chat Sessions ───────────────────────────────────────────
        .nest(
            "/api/v1/workspaces/:workspaceId/sessions",
            chat::workspace_sessions_router(),
        )
        .nest("/api/v1/sessions", chat::session_detail_router())
        // ── Settings ────────────────────────────────────────────────
        .nest(
            "/api/v1/workspaces/:workspaceId/settings",
            settings::router(),
        )
        .nest("/api/v1/orgs/:orgId/config", config::org_router())
        .nest(
            "/api/v1/orgs/:orgId/provider-accounts",
            provider_accounts::router(),
        )
        .nest(
            "/api/v1/workspaces/:workspaceId/config",
            config::workspace_router(),
        )
        // ── Auth & SCM ──────────────────────────────────────────────
        .nest("/api/v1/auth", auth::router())
        .nest("/api/v1/scm", auth::scm_router())
        // ── Self-hosted installer registry exchange ────────────────
        .nest("/api/v1/registry", registry_router)
        // ── Self-hosted admin provisioning ─────────────────────────
        .nest("/api/v1/admin", admin_router)
        // ── Orgs & Invitations ──────────────────────────────────────
        .nest("/api/v1/orgs", orgs::org_router())
        .nest("/api/v1/platform", platform::router())
        .nest(
            "/api/v1/orgs/:orgId/approval-definitions",
            org_approvals::org_approvals_router(),
        )
        .nest(
            "/api/v1/workspaces/:workspaceId/review-flow",
            workspace_review_flow::router(),
        )
        .nest(
            "/api/v1/workspaces/:workspaceId/approvals",
            workspace_approvals::workspace_approvals_router(),
        )
        .nest(
            "/api/v1/workspaces/:workspaceId/review",
            workspace_reviews::workspace_review_router(),
        )
        .nest("/api/v1/orgs/:orgId/github-app", github_app::router())
        .nest(
            "/api/v1/orgs/:orgId/issue-domains",
            external_issues::domain_router(),
        )
        .nest(
            "/api/v1/orgs/:orgId/issues",
            external_issues::org_issue_router(),
        )
        .nest(
            "/api/v1/workspaces/:workspaceId/issue-links",
            external_issues::workspace_router(),
        )
        .nest(
            "/api/v1/workspaces/:workspaceId/activity",
            external_issues::activity_router(),
        )
        .nest(
            "/api/v1/workspaces/:workspaceId/pull-requests/github",
            pull_request_conversation::router(),
        )
        .nest("/api/v1/github-app", github_app::callback_router())
        .nest("/api/v1/invitations", orgs::invitation_router())
        // ── Worker Hosts (rev-7 provisioning, PR-rev-7-9b) ───────────
        .nest("/api/v1/orgs/:orgId/worker-hosts", worker_hosts_router)
        // ── API Keys ────────────────────────────────────────────────
        .nest("/api/v1/orgs/:orgId/api-keys", api_keys::router())
        // ── Users ───────────────────────────────────────────────────
        .nest("/api/v1/users", users::router())
        .nest("/api/v1/users", config::user_router())
        // ── Permissions ─────────────────────────────────────────────
        .nest("/api/v1/auth/me/scopes", permissions::my_scopes_router())
        .nest(
            "/api/v1/orgs/:orgId/scopes",
            permissions::org_scopes_router(),
        )
        .nest(
            "/api/v1/workspaces/:workspaceId/permissions",
            permissions::workspace_permissions_router(),
        )
        // ── Files ───────────────────────────────────────────────────
        .nest(
            "/api/v1/workspaces/:workspaceId/repos/:repoId/files",
            files::router(),
        )
        .nest(
            "/api/v1/workspaces/:workspaceId/repos/:repoId/commits",
            files::commits_router(),
        )
        .nest(
            "/api/v1/workspaces/:workspaceId/repos/:repoId/preview-profiles",
            preview_profiles::router(),
        )
        .nest(
            "/api/v1/workspaces/:workspaceId/repos/:repoId/preview-applications",
            preview_profiles::applications_router(),
        )
        .nest(
            "/api/v1/workspaces/:workspaceId/repos/:repoId/explainer-templates",
            explainer_templates::router(),
        )
        // ── Instance ────────────────────────────────────────────────
        .nest("/api/v1/instance", instance::router())
        // ── Platform bug reporting (authenticated user; not org-scoped) ──
        .nest("/api/v1/platform/issues", platform_issues::router())
        // ── Resolve ─────────────────────────────────────────────────
        .nest("/api/v1/resolve", resolve::router())
        // ── UI Telemetry Ingest (issue #1552) ───────────────────────
        .nest("/api/v1/telemetry", telemetry::router())
        // ── Grounding (public) ──────────────────────────────────────
        .nest("/api/v1/grounding", grounding::router())
        // ── Intent Log (read-only projection) ───────────────────────
        .nest("/api/v1/intent-log", intent_log::router())
        // ── Source Control current-change graph ────────────────────
        .nest(
            "/api/v1/workspaces/:workspaceId/source-control",
            source_control_change_graph::router().merge(source_control_browser::router()),
        )
        // ── Git Smart HTTP Protocol ─────────────────────────────────
        .nest("/git/:repoId", git::router())
        // ── Internal Worker Endpoints ───────────────────────────────
        .nest("/api/internal", internal::router())
        .nest(
            "/api/internal/workspace-incarnations",
            workspace_flush::router(),
        )
        .nest("/api/internal/workspace-agent", workspace_agent::router())
        // ── WebSocket ───────────────────────────────────────────────
        .route("/api/v1/ws", axum::routing::get(ws_adapter))
        .route(
            "/api/v1/workspaces/:id/terminal",
            axum::routing::get(terminal_ws_adapter),
        )
        // ── Apply shared state ──────────────────────────────────────
        .with_state(state)
}

#[derive(Debug, Deserialize)]
struct TerminalWsParams {
    token: String,
    #[serde(default)]
    terminal_id: Option<String>,
    #[serde(default)]
    offset: Option<u64>,
    #[serde(default = "default_terminal_cols")]
    cols: u16,
    #[serde(default = "default_terminal_rows")]
    rows: u16,
}

fn default_terminal_cols() -> u16 {
    80
}

fn default_terminal_rows() -> u16 {
    24
}

/// Transport liveness only: pings and pongs never renew owner activity.
/// The override shortens the period for integration tests.
const TERMINAL_KEEPALIVE_DEFAULT_SECS: u64 = 25;
/// Bounds ancillary writes so a
/// stalled database or a dead peer's full send buffer cannot wedge the loop.
const TERMINAL_WRITE_TIMEOUT_SECS: u64 = 5;

/// Ping the browser with a bounded send: a dead peer whose buffer stays
/// writable-pending must not wedge the heartbeat arm past the tick that
/// would close the terminal. Timeout and send failure both end the loop.
async fn send_ping_bounded(socket: &mut axum::extract::ws::WebSocket) -> Result<(), ()> {
    match tokio::time::timeout(
        Duration::from_secs(TERMINAL_WRITE_TIMEOUT_SECS),
        socket.send(Message::Ping(Vec::new())),
    )
    .await
    {
        Ok(Ok(())) => Ok(()),
        _ => Err(()),
    }
}

fn terminal_keepalive_period() -> Duration {
    Duration::from_secs(
        std::env::var("G5_TERMINAL_KEEPALIVE_SECS")
            .ok()
            .and_then(|value| value.parse().ok())
            .filter(|seconds| (1..=TERMINAL_KEEPALIVE_DEFAULT_SECS).contains(seconds))
            .unwrap_or(TERMINAL_KEEPALIVE_DEFAULT_SECS),
    )
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum TerminalBrowserMessage {
    Input { data: String },
    // Emulator replies to device/status queries are necessary terminal I/O,
    // but originate in output, not keyboard/paste activity.
    Response { data: String },
    Resize { cols: u16, rows: u16 },
    Close,
}

fn terminal_browser_payload(bytes: &[u8], binary: bool) -> Option<(Vec<u8>, bool)> {
    if bytes.len() > 1024 * 1024 {
        return None;
    }
    let message = if binary {
        TerminalBrowserMessage::Input {
            data: std::str::from_utf8(bytes).ok()?.to_owned(),
        }
    } else {
        serde_json::from_slice(bytes).ok()?
    };
    let (message, user_input) = match message {
        TerminalBrowserMessage::Input { data } if !data.is_empty() => {
            (TerminalClientMessage::Input { data }, true)
        }
        TerminalBrowserMessage::Response { data } if !data.is_empty() => {
            (TerminalClientMessage::Input { data }, false)
        }
        TerminalBrowserMessage::Resize { cols, rows }
            if (1..=1000).contains(&cols) && (1..=1000).contains(&rows) =>
        {
            (TerminalClientMessage::Resize { cols, rows }, false)
        }
        TerminalBrowserMessage::Close => (TerminalClientMessage::Close, false),
        _ => return None,
    };
    Some((message.to_line().ok()?, user_input))
}

fn terminal_open_frame(
    cols: u16,
    rows: u16,
    terminal_id: Option<&str>,
    offset: Option<u64>,
    secrets: &HashMap<String, String>,
) -> Result<Vec<u8>, serde_json::Error> {
    // Older dispatchers reject unknown open fields, so `offset` is omitted
    // until a browser has observed a sequence-aware opened response.
    TerminalClientMessage::open(
        cols,
        rows,
        terminal_id.map(str::to_string),
        offset,
        secrets.clone(),
    )
    .to_line()
}

/// Answer the browser with the guest-shaped refusal for a secret set the
/// owner must correct, then close.
async fn reject_terminal(mut socket: axum::extract::ws::WebSocket, detail: String) {
    if let Ok(line) = (TerminalServerMessage::Rejected {
        reason: TerminalRejection::SecretsInvalid,
        detail: Some(detail),
    })
    .to_line()
    {
        let _ = socket.send(Message::Binary(line)).await;
    }
    let _ = socket.close().await;
}

async fn terminal_ws_adapter(
    ws: WebSocketUpgrade,
    Path(workspace_id): Path<Uuid>,
    Query(params): Query<TerminalWsParams>,
    State(state): State<Arc<AppState>>,
    request_id: Option<Extension<RequestId>>,
) -> Result<impl IntoResponse, AppError> {
    let correlation_id = request_id
        .map(|Extension(id)| id.0)
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    if params.cols == 0 || params.rows == 0 || params.cols > 1000 || params.rows > 1000 {
        return Err(AppError::from_error(G5Error::ValidationError {
            details: serde_json::json!({"field":"terminal size","error":"dimensions must be between 1 and 1000"}),
        }));
    }
    if params
        .terminal_id
        .as_ref()
        .is_some_and(|id| id.is_empty() || id.len() > g5_contracts::terminal::MAX_TERMINAL_ID_BYTES)
    {
        return Err(AppError::from_error(G5Error::ValidationError {
            details: serde_json::json!({"field":"terminal_id","error":"must be 1 to 128 bytes"}),
        }));
    }
    let validator = state.jwt_validator.as_ref().ok_or_else(|| {
        AppError::from_error(G5Error::Internal {
            message: "JwtValidator not configured for WebSocket".into(),
        })
    })?;
    let claims = validator
        .validate_websocket_ticket(&params.token, Some(workspace_id))
        .map_err(|_| AppError::from_error(G5Error::Unauthorized))?;
    let user_id =
        Uuid::parse_str(&claims.sub).map_err(|_| AppError::from_error(G5Error::Unauthorized))?;
    let auth = AuthContext::User(claims);
    let (handle, stream, activity) = state
        .workspace_runtime
        .open_owner_stream(
            workspace_id,
            user_id,
            g5_internal_api::stream_mux::Channel::Pty,
        )
        .await
        .map_err(AppError::from_error)?;
    let mut db =
        scoped_db_for_workspace(&state, &auth, workspace_id, &["workspaces:write"]).await?;
    let secrets = state
        .task_service
        .workspace_owner_terminal_secrets(&mut db, handle.org_id, workspace_id, user_id)
        .await?;
    db.commit().await.map_err(AppError::from)?;
    // The effective secrets are admitted here under the shared policy
    // (`g5.terminal.secret-admission`), before any open frame reaches the
    // guest: the stream closes unopened. A refusal reaches the browser as the
    // control line the guest would send, so the terminal shows the reason
    // instead of reconnecting.
    if let Err(error) = g5_contracts::types::task_env::validate_task_env_secrets(
        secrets
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str())),
    ) {
        return Ok(ws.on_upgrade(move |socket| async move {
            drop(stream);
            reject_terminal(socket, error.to_string()).await;
        }));
    }
    let cols = params.cols;
    let rows = params.rows;
    let terminal_id = params.terminal_id;
    let offset = params.offset;
    // A policy-valid set always fits the frame; this only guards encoding.
    let open = match terminal_open_frame(cols, rows, terminal_id.as_deref(), offset, &secrets) {
        Ok(frame) if frame.len() <= TERMINAL_OPEN_FRAME_MAX_BYTES => frame,
        Ok(frame) => {
            let detail = format!(
                "the terminal open frame is {} bytes; the limit is {TERMINAL_OPEN_FRAME_MAX_BYTES} bytes",
                frame.len()
            );
            return Ok(ws.on_upgrade(move |socket| async move {
                drop(stream);
                reject_terminal(socket, detail).await;
            }));
        }
        Err(error) => {
            let detail = format!("the terminal open frame cannot be encoded: {error}");
            return Ok(ws.on_upgrade(move |socket| async move {
                drop(stream);
                reject_terminal(socket, detail).await;
            }));
        }
    };
    Ok(ws.on_upgrade(move |socket| async move {
        let (mux_writer, mut mux_reader) = stream.into_split();
        if !activity.authorize().await || mux_writer.send(&open).await.is_err() {
            return;
        }
        let mut socket = socket;
        let keepalive = terminal_keepalive_period();
        let mut heartbeat = tokio::time::interval(keepalive);
        heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        heartbeat.tick().await;
        let mut browser_seen_at = tokio::time::Instant::now();
        let mut silence_close_deferred = false;
        loop {
            tokio::select! {
                message = socket.recv() => {
                    let Some(Ok(message)) = message else { break };
                    if !activity.authorize().await { break; }
                    browser_seen_at = tokio::time::Instant::now();
                    let payload = match message {
                        Message::Text(text) => terminal_browser_payload(text.as_bytes(), false),
                        Message::Binary(bytes) => terminal_browser_payload(&bytes, true),
                        // A browser reload or transport interruption sends a
                        // WebSocket close frame. Let the guest detach its PTY
                        // so the next socket can reattach during the grace
                        // period. Only an explicit terminal protocol `close`
                        // message destroys the shell.
                        Message::Close(_) => break,
                        Message::Ping(bytes) => {
                            if socket.send(Message::Pong(bytes)).await.is_err() { break; }
                            continue;
                        }
                        Message::Pong(_) => continue,
                    };
                    let Some((payload, user_input)) = payload else { continue; };
                    if mux_writer.send(&payload).await.is_err() { break; }
                    if user_input {
                        let _ = tokio::time::timeout(
                            Duration::from_secs(TERMINAL_WRITE_TIMEOUT_SECS),
                            activity.record(WorkspaceActivityReason::TerminalInput, &correlation_id),
                        ).await;
                    }
                }
                bytes = mux_reader.recv() => {
                    let Some(bytes) = bytes else { break };
                    if !activity.authorize().await { break; }
                    if socket.send(Message::Binary(bytes)).await.is_err() { break; }
                }
                _ = heartbeat.tick() => {
                    if browser_seen_at.elapsed() >= keepalive * 2 {
                        // An unbiased select can run an overdue tick ahead of
                        // a pong already queued on the socket. Ping once more
                        // and close only if the next tick is still overdue,
                        // so a live browser is not reaped over branch order
                        // while a silent one still is, one period later.
                        if silence_close_deferred { break; }
                        silence_close_deferred = true;
                        if send_ping_bounded(&mut socket).await.is_err() { break; }
                        continue;
                    }
                    silence_close_deferred = false;
                    if !activity.authorize().await { break; }
                    if send_ping_bounded(&mut socket).await.is_err() { break; }
                }
            }
        }
        mux_writer.close().await;
        let _ = socket.close().await;
    }))
}

// ---------------------------------------------------------------------------
// WebSocket adapter — bridges AppState to the ws module's handler
// ---------------------------------------------------------------------------

/// Thin adapter handler that validates the JWT from the query parameter and
/// upgrades the connection to a WebSocket, delegating to
/// [`crate::ws::handle_ws_connection`].
///
/// The ws module's `ws_handler` expects multiple separate `State<T>` extractors
/// (`Arc<WsHub>`, `Arc<JwtValidator>`, `PgPool`). Since the main router uses
/// `State<Arc<AppState>>`, this adapter extracts the individual pieces and
/// forwards them.
async fn ws_adapter(
    ws: WebSocketUpgrade,
    Query(params): Query<crate::ws::WsUpgradeParams>,
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    let jwt_validator = state.jwt_validator.as_ref().ok_or_else(|| {
        AppError::from_error(G5Error::Internal {
            message: "JwtValidator not configured for WebSocket".into(),
        })
    })?;

    let claims = jwt_validator
        .validate_websocket_ticket(&params.token, None)
        .map_err(|_| AppError::from_error(G5Error::Unauthorized))?;

    let user_id =
        Uuid::parse_str(&claims.sub).map_err(|_| AppError::from_error(G5Error::Unauthorized))?;
    // Keep the ticket's organization for live revocation, including sockets
    // that only subscribe to personal notifications. Resource subscriptions
    // still resolve their own organization independently.
    let org_id =
        Uuid::parse_str(&claims.org).map_err(|_| AppError::from_error(G5Error::Unauthorized))?;

    let hub = state.event_bus.ws_hub().clone();
    let ws_state = state.clone();

    Ok(ws.on_upgrade(move |socket| {
        crate::ws::handle_ws_connection(socket, hub, ws_state, user_id, org_id, claims)
    }))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_activity_requires_valid_nonempty_input() {
        for frame in [
            br#"{"type":"input","data":""}"#.as_slice(),
            br#"{"type":"input","data":null}"#,
            br#"{"type":"input","data":"x","extra":1}"#,
            br#"{"type":"open","cols":80,"rows":24}"#,
            br#"{"type":"resize","cols":0,"rows":24}"#,
            b"garbage",
            b"",
            b"{}",
            b"{\"type\":\"input\",\"data\":\"x\"}\n{\"type\":\"close\"}",
        ] {
            assert!(terminal_browser_payload(frame, false).is_none());
        }
        assert!(terminal_browser_payload(&[], true).is_none());
        assert!(terminal_browser_payload(&[0xff], true).is_none());
        assert!(terminal_browser_payload(&vec![b'x'; 1024 * 1024 + 1], true).is_none());
        for frame in [
            br#"{"type":"resize","cols":80,"rows":24}"#.as_slice(),
            br#"{"type":"close"}"#,
            br#"{"type":"response","data":"\u001b[0n"}"#,
        ] {
            assert!(!terminal_browser_payload(frame, false).unwrap().1);
        }
        for frame in [
            br#"{"type":"input","data":"\u0003"}"#.as_slice(),
            br#"{"type":"input","data":" "}"#,
            br#"{"type":"input","data":"pasted\ntext"}"#,
        ] {
            assert!(terminal_browser_payload(frame, false).unwrap().1);
        }
        assert!(terminal_browser_payload(b"pasted text", true).unwrap().1);
    }

    #[test]
    fn terminal_keepalive_override_is_bounded() {
        for (value, expected) in [
            ("1", 1),
            ("25", 25),
            ("30", TERMINAL_KEEPALIVE_DEFAULT_SECS),
            ("300", TERMINAL_KEEPALIVE_DEFAULT_SECS),
            ("0", TERMINAL_KEEPALIVE_DEFAULT_SECS),
            ("junk", TERMINAL_KEEPALIVE_DEFAULT_SECS),
        ] {
            std::env::set_var("G5_TERMINAL_KEEPALIVE_SECS", value);
            assert_eq!(
                terminal_keepalive_period(),
                Duration::from_secs(expected),
                "override {value}"
            );
        }
        std::env::remove_var("G5_TERMINAL_KEEPALIVE_SECS");
        assert_eq!(
            terminal_keepalive_period(),
            Duration::from_secs(TERMINAL_KEEPALIVE_DEFAULT_SECS)
        );
    }

    #[test]
    fn terminal_open_frame_adds_offset_only_after_sequence_capability_is_known() {
        let secrets = HashMap::new();
        let legacy: serde_json::Value = serde_json::from_slice(
            terminal_open_frame(80, 24, Some("terminal-1"), None, &secrets)
                .unwrap()
                .strip_suffix(b"\n")
                .unwrap(),
        )
        .unwrap();
        assert!(legacy.get("offset").is_none());

        let resumed: serde_json::Value = serde_json::from_slice(
            terminal_open_frame(80, 24, Some("terminal-1"), Some(42), &secrets)
                .unwrap()
                .strip_suffix(b"\n")
                .unwrap(),
        )
        .unwrap();
        assert_eq!(resumed["offset"], 42);
    }

    #[test]
    fn test_response_envelope_ok_serialization() {
        let envelope = ResponseEnvelope::ok("hello");
        let json = serde_json::to_value(&envelope).unwrap();

        assert_eq!(json["data"], "hello");
        // meta should be absent (not null) due to skip_serializing_if
        assert!(json.get("meta").is_none());
    }

    #[test]
    fn test_response_envelope_paginated_serialization() {
        let meta = PaginationMeta {
            page: 2,
            per_page: 25,
            total: 100,
        };
        let envelope = ResponseEnvelope::paginated(vec![1, 2, 3], meta);
        let json = serde_json::to_value(&envelope).unwrap();

        assert_eq!(json["data"], serde_json::json!([1, 2, 3]));
        assert_eq!(json["meta"]["page"], 2);
        // Verify camelCase serialization
        assert_eq!(json["meta"]["perPage"], 25);
        assert_eq!(json["meta"]["total"], 100);
        // Should NOT have snake_case key
        assert!(json["meta"].get("per_page").is_none());
    }

    #[test]
    fn test_pagination_meta_camel_case() {
        let meta = PaginationMeta {
            page: 1,
            per_page: 50,
            total: 123,
        };
        let json = serde_json::to_value(&meta).unwrap();

        assert_eq!(json["page"], 1);
        assert_eq!(json["perPage"], 50);
        assert_eq!(json["total"], 123);
        // Ensure snake_case key does not exist
        assert!(json.get("per_page").is_none());
    }

    #[test]
    fn test_pagination_defaults() {
        let json = "{}";
        let pagination: Pagination = serde_json::from_str(json).unwrap();
        assert_eq!(pagination.page, 1);
        assert_eq!(pagination.per_page, 50);
    }

    #[test]
    fn test_pagination_custom_values() {
        let json = r#"{"page": 3, "perPage": 25}"#;
        let pagination: Pagination = serde_json::from_str(json).unwrap();
        assert_eq!(pagination.page, 3);
        assert_eq!(pagination.per_page, 25);
    }

    #[test]
    fn test_pagination_page_min_clamp() {
        let pagination = Pagination {
            page: 0,
            per_page: 50,
        };
        assert_eq!(pagination.page(), 1);
        assert_eq!(pagination.offset(), 0);
    }

    #[test]
    fn test_pagination_page_negative_clamp() {
        let pagination = Pagination {
            page: -5,
            per_page: 50,
        };
        assert_eq!(pagination.page(), 1);
        assert_eq!(pagination.offset(), 0);
    }

    #[test]
    fn test_pagination_per_page_max_clamp() {
        let pagination = Pagination {
            page: 1,
            per_page: 500,
        };
        assert_eq!(pagination.per_page(), 100);
        assert_eq!(pagination.limit(), 100);
    }

    #[test]
    fn test_pagination_per_page_min_clamp() {
        let pagination = Pagination {
            page: 1,
            per_page: 0,
        };
        assert_eq!(pagination.per_page(), 1);
        assert_eq!(pagination.limit(), 1);
    }

    #[test]
    fn test_pagination_per_page_negative_clamp() {
        let pagination = Pagination {
            page: 1,
            per_page: -10,
        };
        assert_eq!(pagination.per_page(), 1);
        assert_eq!(pagination.limit(), 1);
    }

    #[test]
    fn test_pagination_offset_calculation() {
        let pagination = Pagination {
            page: 3,
            per_page: 25,
        };
        // offset = (3 - 1) * 25 = 50
        assert_eq!(pagination.offset(), 50);
        assert_eq!(pagination.limit(), 25);
    }

    #[test]
    fn test_pagination_offset_page_one() {
        let pagination = Pagination {
            page: 1,
            per_page: 50,
        };
        assert_eq!(pagination.offset(), 0);
        assert_eq!(pagination.limit(), 50);
    }

    #[test]
    fn test_pagination_offset_with_clamped_per_page() {
        let pagination = Pagination {
            page: 3,
            per_page: 200,
        };
        // offset = (3 - 1) * clamp(200, 1, 100) = 2 * 100 = 200
        assert_eq!(pagination.offset(), 200);
        assert_eq!(pagination.limit(), 100);
    }

    #[test]
    fn test_pagination_to_meta() {
        let pagination = Pagination {
            page: 2,
            per_page: 25,
        };
        let meta = pagination.to_meta(100);
        assert_eq!(meta.page, 2);
        assert_eq!(meta.per_page, 25);
        assert_eq!(meta.total, 100);
    }

    #[test]
    fn test_pagination_to_meta_with_clamping() {
        let pagination = Pagination {
            page: 0,
            per_page: 200,
        };
        let meta = pagination.to_meta(50);
        assert_eq!(meta.page, 1); // clamped from 0
        assert_eq!(meta.per_page, 100); // clamped from 200
        assert_eq!(meta.total, 50);
    }

    #[test]
    fn test_pagination_deserializes_camel_case() {
        let json = r#"{"page": 2, "perPage": 30}"#;
        let pagination: Pagination = serde_json::from_str(json).unwrap();
        assert_eq!(pagination.page, 2);
        assert_eq!(pagination.per_page, 30);
    }
}
