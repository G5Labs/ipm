//! Workspace-agent operations: the agent in a workspace VM coordinating its
//! owner's workspaces and reading the workspaces its owner can read, with the
//! VM's own credential (G5-Lean#710).
//!
//! Every route is `/api/internal/workspace-agent/:id/…`, where `:id` must be
//! the credential's own workspace. The middleware admits the workspace token;
//! each handler then checks the incarnation is current and the target's
//! relation through [`crate::db::workspace_agent_scope`]:
//!
//! - operations reach the caller's own workspace and its owner's other
//!   workspaces, never Base or a review workspace
//!   (`g5.security.agent-workspace-scope`); chat mutations and deletion never
//!   reach the caller's own (`g5.security.agent-chat-control`,
//!   `g5.security.agent-workspace-delete`);
//! - reads reach any workspace the owner can read (`g5.security.agent-read-scope`).
//!
//! Each handler runs the same domain operation the UI runs, as the owner.
//! Records keep the owner as actor; each mutating request also emits one
//! `g5_audit` `workspace_agent.action` event naming the calling incarnation,
//! whether it succeeded, failed or was refused (see [`audit_agent_request`]).

use std::sync::Arc;

use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db::scoped::resolve_chat_session_context;
use crate::db::workspace_agent_scope::{
    agent_fork_source, agent_owner_db, agent_owner_system_db, agent_read_target, agent_relation,
    agent_target, list_agent_children, require_other_workspaces_allowed, workspace_summary,
    AgentChild, AgentTarget, AgentTargetNeed, AgentWorkspaceSummary,
};
use crate::errors::AppError;
use crate::middleware::WorkspaceIncarnationContext;
use crate::routes::{audited::AuditedRouter, ResponseEnvelope};
use crate::services::approval_review_service::inspection::{
    ReviewCommitQuery, ReviewTranscriptPage, ReviewTranscriptQuery,
};
use crate::services::workspace_messages::{MessageTarget, WorkspaceMessage};
use crate::services::workspace_vm_service::{
    ResumeOutcome, StartKind, StopOutcome, StopReason, WorkspaceVmView,
};
use crate::services::AppState;
use g5_auth::WorkspaceIncarnationClaims;
use g5_contracts::error::G5Error;
use g5_contracts::types::ChatAgentProvider;

/// Workspace-agent routes nested under `/api/internal/workspace-agent`.
pub fn router() -> Router<Arc<AppState>> {
    const REASON: &str =
        "Workspace-incarnation agent operation; the handler checks the live incarnation and the target's agent scope";
    AuditedRouter::new()
        .exempt_route("/:id/workspaces", get(list_readable_workspaces), REASON)
        .exempt_route("/:id/children", get(list_children), REASON)
        .exempt_route("/:id/workspaces/:targetId", get(probe_workspace), REASON)
        .exempt_route(
            "/:id/workspaces/:targetId/sessions",
            get(list_workspace_sessions).post(start_child_chat),
            REASON,
        )
        .exempt_route(
            "/:id/workspaces/:targetId/sessions/:sessionId/messages",
            post(send_child_follow_up),
            REASON,
        )
        .exempt_route("/:id/workspaces/:targetId/start", post(start_child), REASON)
        .exempt_route(
            "/:id/workspaces/:targetId/stop",
            post(stop_workspace),
            REASON,
        )
        .exempt_route(
            "/:id/workspaces/:targetId/delete",
            post(delete_child),
            REASON,
        )
        .exempt_route(
            "/:id/workspaces/:targetId/pull-request",
            post(publish_child_pull_request),
            REASON,
        )
        .exempt_route(
            "/:id/pull-requests/:pullRequestId/merge",
            post(merge_pull_request),
            REASON,
        )
        .exempt_route(
            "/:id/sessions/:sessionId/messages",
            get(read_session_messages),
            REASON,
        )
        .exempt_route(
            "/:id/sessions/:sessionId/timeline",
            get(read_session_timeline),
            REASON,
        )
        .exempt_route("/:id/fork", post(fork_with_prompt), REASON)
        // Increment 2: chat and task controls in children, approvals, PRs
        // and linked-issue state.
        .exempt_route(
            "/:id/workspaces/:targetId/sessions/:sessionId/stop",
            post(stop_child_turn),
            REASON,
        )
        .exempt_route(
            "/:id/workspaces/:targetId/sessions/:sessionId/background-end",
            post(end_child_background_wait),
            REASON,
        )
        .exempt_route(
            "/:id/workspaces/:targetId/sessions/:sessionId/rename",
            post(rename_child_chat),
            REASON,
        )
        .exempt_route(
            "/:id/sessions/:sessionId/inputs",
            get(read_session_inputs),
            REASON,
        )
        .exempt_route(
            "/:id/workspaces/:targetId/sessions/:sessionId/inputs/:inputId/:action",
            post(change_child_input),
            REASON,
        )
        .exempt_route(
            "/:id/workspaces/:targetId/tasks/:taskId/:action",
            post(control_task),
            REASON,
        )
        // The org-wide approval activity feed over every workspace the owner
        // can read (G5-Lean#1099); no target workspace, the scope is applied in
        // the query.
        .exempt_route(
            "/:id/approval-activity",
            get(read_approval_activity),
            REASON,
        )
        .exempt_route(
            "/:id/workspaces/:targetId/approvals",
            get(read_approvals),
            REASON,
        )
        .exempt_route(
            "/:id/workspaces/:targetId/approvals/requests",
            post(request_human_approvals),
            REASON,
        )
        .exempt_route(
            "/:id/workspaces/:targetId/approvals/reviews",
            post(request_agent_review),
            REASON,
        )
        .exempt_route(
            "/:id/workspaces/:targetId/approvals/requests/:requestId/cancel",
            post(cancel_approval_request),
            REASON,
        )
        // A formal agent review's retained evidence, read as the approvals
        // are (G5-Lean#1019).
        .exempt_route(
            "/:id/workspaces/:targetId/approvals/reviews/:reviewId/report",
            get(read_review_report),
            REASON,
        )
        .exempt_route(
            "/:id/workspaces/:targetId/approvals/reviews/:reviewId/transcript",
            get(read_review_transcript),
            REASON,
        )
        .exempt_route(
            "/:id/workspaces/:targetId/approvals/reviews/:reviewId/commits",
            get(read_review_commits),
            REASON,
        )
        .exempt_route(
            "/:id/workspaces/:targetId/pull-requests",
            get(list_pull_requests),
            REASON,
        )
        .exempt_route(
            "/:id/pull-requests/:pullRequestId/close",
            post(close_pull_request),
            REASON,
        )
        .exempt_route(
            "/:id/pull-requests/:pullRequestId/refresh",
            post(refresh_pull_request),
            REASON,
        )
        .exempt_route(
            "/:id/pull-requests/:pullRequestId/auto-merge/disable",
            post(disable_auto_merge),
            REASON,
        )
        .exempt_route("/:id/issue-state", post(change_issue_state), REASON)
        .exempt_route("/:id/issue-edit", post(edit_issue), REASON)
        // A domain's label catalog (`g5d issue label`, `g5.issue.labels`).
        .exempt_route(
            "/:id/issue-labels",
            get(list_issue_labels).post(create_issue_label),
            REASON,
        )
        .exempt_route("/:id/issue-labels/edit", post(edit_issue_label), REASON)
        .exempt_route("/:id/issue-labels/delete", post(delete_issue_label), REASON)
        // The organization's members, for naming assignees.
        .exempt_route("/:id/users", get(lookup_users), REASON)
        // Managed preview of the own workspace or a child.
        .exempt_route(
            "/:id/workspaces/:targetId/preview",
            get(read_preview).post(start_preview),
            REASON,
        )
        .exempt_route(
            "/:id/workspaces/:targetId/preview/credential",
            post(mint_preview_credential),
            REASON,
        )
        .exempt_route(
            "/:id/workspaces/:targetId/preview/cancel",
            post(close_preview),
            REASON,
        )
        .exempt_route(
            "/:id/workspaces/:targetId/repos/:repoId/preview-applications",
            get(list_preview_applications),
            REASON,
        )
        .exempt_route(
            "/:id/workspaces/:targetId/repos/:repoId/preview-profiles",
            get(list_preview_profiles).post(save_preview_profile),
            REASON,
        )
        // Files of a readable workspace; changes in the own workspace or a child.
        .exempt_route("/:id/workspaces/:targetId/files", get(list_files), REASON)
        .exempt_route(
            "/:id/workspaces/:targetId/files/content",
            get(read_file),
            REASON,
        )
        .exempt_route(
            "/:id/workspaces/:targetId/files/write",
            post(write_file),
            REASON,
        )
        .exempt_route(
            "/:id/workspaces/:targetId/files/delete",
            post(delete_file),
            REASON,
        )
        .exempt_route(
            "/:id/workspaces/:targetId/files/rename",
            post(rename_file),
            REASON,
        )
        // A running chat turn's ontology record (g5.chat.ontology-card).
        .exempt_route(
            "/:id/chat-tasks/:taskId/ontology",
            get(get_turn_ontology),
            REASON,
        )
        .exempt_route(
            "/:id/chat-tasks/:taskId/ontology/baseline",
            post(record_turn_baseline).layer(axum::extract::DefaultBodyLimit::max(
                crate::services::chat_ontology::MAX_BASELINE_BYTES + 64 * 1024,
            )),
            REASON,
        )
        .exempt_route(
            "/:id/chat-tasks/:taskId/ontology/governing",
            post(record_turn_governing),
            REASON,
        )
        .exempt_route(
            "/:id/stop-owner-workspaces",
            get(owner_workspaces_progress).post(stop_owner_workspaces),
            REASON,
        )
        .into_router()
        .route_layer(axum::middleware::from_fn(audit_agent_request))
}

// ─── Shared helpers ──────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CallerPath {
    id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct TargetPath {
    id: Uuid,
    #[serde(rename = "targetId")]
    target_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct TargetSessionPath {
    id: Uuid,
    #[serde(rename = "targetId")]
    target_id: Uuid,
    #[serde(rename = "sessionId")]
    session_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct SessionPath {
    id: Uuid,
    #[serde(rename = "sessionId")]
    session_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct PullRequestPath {
    id: Uuid,
    #[serde(rename = "pullRequestId")]
    pull_request_id: Uuid,
}

fn forbidden() -> AppError {
    AppError::from(G5Error::Forbidden {
        missing_scopes: Vec::new(),
    })
}

/// The path workspace must be the credential's own.
fn require_own_workspace(
    path_workspace: Uuid,
    incarnation: &WorkspaceIncarnationClaims,
) -> Result<(), AppError> {
    if path_workspace == incarnation.workspace_id {
        Ok(())
    } else {
        Err(forbidden())
    }
}

/// An agent-chosen key naming one request, so a retry after a lost reply
/// replays the same operation.
fn validate_key(field: &'static str, key: &str) -> Result<(), AppError> {
    if key.is_empty() || key.trim() != key || key.len() > 190 {
        return Err(AppError::from(G5Error::ValidationError {
            details: serde_json::json!({
                "field": field,
                "error": "must be 1-190 characters without surrounding whitespace",
            }),
        }));
    }
    Ok(())
}

/// A lifecycle operation identity derived from the agent's key, scoped to the
/// owner, caller workspace and purpose, so a replay names the same operation.
fn derived_operation_id(
    incarnation: &WorkspaceIncarnationClaims,
    purpose: &str,
    target: Uuid,
    key: Option<&str>,
) -> Result<Uuid, AppError> {
    const NAMESPACE: Uuid = Uuid::from_u128(0x2b7c_41de_8f3a_4a65_b0d9_7e61_c5a2_9f08);
    match key {
        None => Ok(Uuid::new_v4()),
        Some(key) => {
            validate_key("operationKey", key)?;
            let name = format!(
                "{}:{}:{purpose}:{target}:{key}",
                incarnation.owner_user_id, incarnation.workspace_id
            );
            Ok(Uuid::new_v5(&NAMESPACE, name.as_bytes()))
        }
    }
}

// ─── Audit ───────────────────────────────────────────────────────────────────
//
// Every mutating request emits exactly one `g5_audit` `workspace_agent.action`
// event when it completes, whatever its status: refused, failed and
// successful attempts alike, as the person's `user_action.http` stream does
// for the UI. The handler adds what only it knows (operation, target, its
// domain result, a file's path) through [`audit`] and [`note_file`]; a request
// refused before that still carries the route, path and status.

tokio::task_local! {
    /// What the handler serving the current request reported for its event.
    static AUDIT_NOTE: std::cell::RefCell<AuditNote>;
}

#[derive(Debug, Default)]
struct AuditNote {
    operation: Option<&'static str>,
    target: Option<Uuid>,
    result: Option<String>,
    repo_id: Option<Uuid>,
    file_path: Option<String>,
    new_file_path: Option<String>,
}

/// The longest path recorded in an event, in characters.
const AUDIT_PATH_CHARS: usize = 512;

fn audit_text(value: &str) -> String {
    value.chars().take(AUDIT_PATH_CHARS).collect()
}

/// Record the handler's operation, target and domain result for the
/// request's event.
fn audit(
    incarnation: &WorkspaceIncarnationClaims,
    operation: &'static str,
    target: Uuid,
    outcome: &str,
) {
    let noted = AUDIT_NOTE.try_with(|note| {
        let mut note = note.borrow_mut();
        note.operation = Some(operation);
        note.target = Some(target);
        note.result = Some(outcome.to_string());
    });
    if noted.is_err() {
        // Outside a routed request: emit on the spot rather than lose it.
        emit_audit(
            incarnation,
            "",
            "",
            &AuditNote {
                operation: Some(operation),
                target: Some(target),
                result: Some(outcome.to_string()),
                ..AuditNote::default()
            },
            None,
        );
    }
}

/// Record a file operation before it runs, so a refused or failed attempt
/// names the file too.
fn note_file(
    operation: &'static str,
    target: Uuid,
    repo_id: Uuid,
    file_path: &str,
    new_file_path: Option<&str>,
) {
    let _ = AUDIT_NOTE.try_with(|note| {
        let mut note = note.borrow_mut();
        note.operation = Some(operation);
        note.target = Some(target);
        note.repo_id = Some(repo_id);
        note.file_path = Some(audit_text(file_path));
        note.new_file_path = new_file_path.map(audit_text);
    });
}

fn emit_audit(
    incarnation: &WorkspaceIncarnationClaims,
    action: &str,
    path: &str,
    note: &AuditNote,
    status: Option<StatusCode>,
) {
    let outcome = match status {
        Some(status) if status.is_server_error() => "server_error",
        Some(status) if status.is_client_error() => "client_error",
        Some(_) => "success",
        None => "",
    };
    let text = |value: Option<&str>| value.unwrap_or("").to_string();
    let id = |value: Option<Uuid>| value.map(|id| id.to_string()).unwrap_or_default();
    tracing::info!(
        target: "g5_audit",
        event = "workspace_agent.action",
        action,
        path = %audit_text(path),
        operation = note.operation.unwrap_or(""),
        org_id = %incarnation.org_id,
        caller_workspace_id = %incarnation.workspace_id,
        caller_vm_id = %incarnation.vm_id,
        caller_epoch = incarnation.epoch,
        owner_user_id = %incarnation.owner_user_id,
        target_workspace_id = %id(note.target),
        repo_id = %id(note.repo_id),
        file_path = %text(note.file_path.as_deref()),
        new_file_path = %text(note.new_file_path.as_deref()),
        status = status.map_or(0, |status| status.as_u16()),
        outcome,
        result = %text(note.result.as_deref()),
        "workspace agent action"
    );
}

/// Route layer emitting the one audit event of each mutating request.
async fn audit_agent_request(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let read_only = matches!(
        *request.method(),
        axum::http::Method::GET | axum::http::Method::HEAD | axum::http::Method::OPTIONS
    );
    let incarnation = request
        .extensions()
        .get::<WorkspaceIncarnationContext>()
        .map(|context| context.0.clone());
    let Some(incarnation) = incarnation.filter(|_| !read_only) else {
        return next.run(request).await;
    };
    let template = request
        .extensions()
        .get::<axum::extract::MatchedPath>()
        .map_or("<unmatched>", |matched| matched.as_str());
    let action = format!("{} {template}", request.method());
    let path = request.uri().path().to_string();
    // The target named in the path, until the handler names it.
    let path_target = path
        .split('/')
        .skip_while(|segment| *segment != "workspaces")
        .nth(1)
        .and_then(|segment| Uuid::parse_str(segment).ok());
    let (response, mut note) = AUDIT_NOTE
        .scope(std::cell::RefCell::new(AuditNote::default()), async {
            let response = next.run(request).await;
            (response, AUDIT_NOTE.with(std::cell::RefCell::take))
        })
        .await;
    note.target = note.target.or(path_target);
    emit_audit(&incarnation, &action, &path, &note, Some(response.status()));
    response
}

/// The lifecycle service running as the owner, admitted for `target`.
async fn lifecycle_for(
    state: &Arc<AppState>,
    incarnation: &WorkspaceIncarnationClaims,
    target: Uuid,
    need: AgentTargetNeed,
) -> Result<crate::services::WorkspaceVmService, AppError> {
    let mut db = agent_owner_db(state, incarnation, &["workspaces:write"]).await?;
    agent_target(&mut db, incarnation, target, need).await?;
    let scope = db.reopener();
    db.rollback().await.map_err(AppError::from)?;
    super::workspaces::workspace_vm_service(state, scope)
}

/// Launch choices an agent may name for a child chat; omitted fields inherit
/// the configured defaults.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchChoices {
    #[serde(default)]
    agent_provider: Option<ChatAgentProvider>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    effort: Option<String>,
    #[serde(default)]
    permission_mode: Option<String>,
}

impl LaunchChoices {
    fn is_empty(&self) -> bool {
        self.agent_provider.is_none()
            && self.model.is_none()
            && self.effort.is_none()
            && self.permission_mode.is_none()
    }

    fn preferences(&self) -> g5_contracts::types::AgentLaunchPreferencesV1 {
        crate::services::config_service::legacy_agent_launch_preferences(
            self.agent_provider,
            self.model.as_deref(),
            self.permission_mode.as_deref(),
            self.effort.as_deref(),
        )
    }
}

/// Refuse a permission mode no agent turn can run (`g5.story.agent-chat-child`).
/// Dispatch admission would fail such a turn after it was queued, and the
/// agent would see only a failed task. `fixed_by_chat` names a follow-up's
/// frozen mode, which only a new chat can change.
fn ensure_runnable_permission_mode(
    provider: ChatAgentProvider,
    mode: &str,
    fixed_by_chat: bool,
) -> Result<(), AppError> {
    let runnable = crate::services::agent_dispatch::agent_turn_permission_modes(provider);
    if runnable.contains(&mode) {
        return Ok(());
    }
    let (subject, remedy) = if fixed_by_chat {
        (
            "this chat's permission mode",
            "start a new chat with one of",
        )
    } else {
        ("permission mode", "use one of")
    };
    Err(AppError::from_error(G5Error::ValidationError {
        details: serde_json::json!({
            "field": "permissionMode",
            "code": "unsupported_agent_turn_permission_mode",
            "message": format!(
                "{subject} {mode} cannot run an agent turn for {}; {remedy}: {}",
                provider.as_str(),
                runnable.join(", ")
            ),
        }),
    }))
}

/// Resolve the launch a new chat in `workspace_id` would get and refuse it
/// before anything is created or queued when a choice is invalid or its
/// permission mode cannot run. Any other resolution failure is left to the
/// shared path, which reports it as it does for the UI.
async fn ensure_runnable_launch(
    state: &AppState,
    db: &mut g5_authz::ScopedDb,
    incarnation: &WorkspaceIncarnationClaims,
    workspace_id: Uuid,
    preferences: g5_contracts::types::AgentLaunchPreferencesV1,
) -> Result<(), AppError> {
    let launch = match state
        .config_service
        .resolve_launch(
            db,
            incarnation.org_id,
            workspace_id,
            Some(incarnation.owner_user_id),
            None,
            preferences,
            &state.provider_account_service,
        )
        .await
    {
        Ok(launch) => launch,
        // An unsupported model or mode would otherwise fail only after the
        // fork or chat exists.
        Err(error) if matches!(error.inner, G5Error::ValidationError { .. }) => return Err(error),
        Err(_) => return Ok(()),
    };
    ensure_runnable_permission_mode(
        launch.resolved.provider.value,
        &launch.resolved.permission_mode.value,
        false,
    )
}

/// Start outcome of a compound chat-and-start request.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartReport {
    #[serde(skip_serializing_if = "Option::is_none")]
    outcome: Option<ResumeOutcome>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    /// A replayed request never adds Start consent (`g5.lifecycle.start`).
    skipped_on_replay: bool,
}

async fn start_after_input(
    state: &Arc<AppState>,
    incarnation: &WorkspaceIncarnationClaims,
    child: Uuid,
    replayed: bool,
) -> StartReport {
    if replayed {
        return StartReport {
            outcome: None,
            error: None,
            skipped_on_replay: true,
        };
    }
    let result = match lifecycle_for(state, incarnation, child, AgentTargetNeed::OtherOnly).await {
        Ok(lifecycle) => lifecycle
            .resume(
                child,
                incarnation.owner_user_id,
                Uuid::new_v4(),
                StartKind::Resume,
            )
            .await
            .map_err(AppError::from),
        Err(error) => Err(error),
    };
    match result {
        Ok(outcome) => StartReport {
            outcome: Some(outcome),
            error: None,
            skipped_on_replay: false,
        },
        Err(error) => StartReport {
            outcome: None,
            error: Some(error.inner.to_string()),
            skipped_on_replay: false,
        },
    }
}

// ─── Reads (any workspace the owner can read) ────────────────────────────────

async fn list_readable_workspaces(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(path): Path<CallerPath>,
) -> Result<
    Json<ResponseEnvelope<Vec<crate::services::workspace_service::WorkspaceListItem>>>,
    AppError,
> {
    require_own_workspace(path.id, &incarnation)?;
    let mut db = agent_owner_db(&state, &incarnation, &["workspaces:read"]).await?;
    let workspaces = state
        .workspace_service
        .list_workspaces(&mut db, incarnation.org_id, incarnation.owner_user_id)
        .await?;
    db.rollback().await.map_err(AppError::from)?;
    Ok(Json(ResponseEnvelope::ok(workspaces)))
}

async fn list_children(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(path): Path<CallerPath>,
) -> Result<Json<ResponseEnvelope<Vec<AgentChild>>>, AppError> {
    require_own_workspace(path.id, &incarnation)?;
    let mut db = agent_owner_db(&state, &incarnation, &["workspaces:read"]).await?;
    let children = list_agent_children(&mut db, &incarnation).await?;
    db.rollback().await.map_err(AppError::from)?;
    Ok(Json(ResponseEnvelope::ok(children)))
}

// ─── Member lookup (`g5.organization.agent-member-directory`) ────────────────

/// At most this many members per lookup unless `limit` says otherwise.
const DEFAULT_USER_LIMIT: usize = 30;
const MAX_USER_LIMIT: usize = 100;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UserLookupQuery {
    /// Words that must each match a name, an email or a login.
    #[serde(default)]
    q: Option<String>,
    /// One member, exactly: `@me` (the owner), a user id, an email, a login
    /// or `provider:login`.
    #[serde(default)]
    user: Option<String>,
    /// Include deactivated members in a word search.
    #[serde(default)]
    include_deactivated: bool,
    #[serde(default)]
    limit: Option<usize>,
}

/// An account a member connected to G5 at an external provider.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ConnectedAccountDTO {
    /// `github` today.
    provider: &'static str,
    login: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryUserDTO {
    id: Uuid,
    name: String,
    email: String,
    /// `admin`, `member` or `viewer`.
    role: String,
    /// `active` or `deactivated`.
    state: &'static str,
    accounts: Vec<ConnectedAccountDTO>,
    /// The workspace owner, whom the agent acts for (`@me`).
    me: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserLookupDTO {
    users: Vec<DirectoryUserDTO>,
    has_more: bool,
}

impl DirectoryUserDTO {
    fn active(&self) -> bool {
        self.state == "active"
    }

    /// Every word is in the name, the email or a login, ignoring case.
    fn matches_words(&self, words: &[String]) -> bool {
        let fields: Vec<String> = [self.name.to_lowercase(), self.email.to_lowercase()]
            .into_iter()
            .chain(
                self.accounts
                    .iter()
                    .map(|account| account.login.to_lowercase()),
            )
            .collect();
        words
            .iter()
            .all(|word| fields.iter().any(|field| field.contains(word.as_str())))
    }

    /// `@me`, the user id, the email or a login (bare or `provider:login`),
    /// ignoring case.
    fn is_named(&self, wanted: &str) -> bool {
        if wanted.eq_ignore_ascii_case("@me") {
            return self.me;
        }
        if Uuid::parse_str(wanted).ok() == Some(self.id) || self.email.eq_ignore_ascii_case(wanted)
        {
            return true;
        }
        self.accounts.iter().any(|account| {
            account.login.eq_ignore_ascii_case(wanted)
                || wanted.split_once(':').is_some_and(|(provider, login)| {
                    provider.eq_ignore_ascii_case(account.provider)
                        && login.eq_ignore_ascii_case(&account.login)
                })
        })
    }
}

/// The members a lookup returns, in roster order, and whether more matched.
fn select_users(
    roster: Vec<DirectoryUserDTO>,
    query: &UserLookupQuery,
) -> Result<UserLookupDTO, AppError> {
    let limit = query.limit.unwrap_or(DEFAULT_USER_LIMIT);
    if !(1..=MAX_USER_LIMIT).contains(&limit) {
        return Err(validation("limit", format!("must be 1-{MAX_USER_LIMIT}")));
    }
    let user = query
        .user
        .as_deref()
        .map(str::trim)
        .filter(|user| !user.is_empty());
    let words: Vec<String> = query
        .q
        .as_deref()
        .unwrap_or_default()
        .split_whitespace()
        .map(str::to_lowercase)
        .collect();
    if let Some(user) = user {
        if !words.is_empty() {
            return Err(validation(
                "user",
                "name one member or search with words, not both",
            ));
        }
        // An exact name finds a deactivated member too: their issues remain.
        let named: Vec<DirectoryUserDTO> = roster
            .into_iter()
            .filter(|member| member.is_named(user))
            .collect();
        if named.is_empty() {
            return Err(AppError::from(G5Error::NotFound {
                resource: format!("organization member {user}"),
            }));
        }
        return Ok(UserLookupDTO {
            users: named,
            has_more: false,
        });
    }
    let mut users: Vec<DirectoryUserDTO> = roster
        .into_iter()
        .filter(|member| query.include_deactivated || member.active())
        .filter(|member| member.matches_words(&words))
        .collect();
    let has_more = users.len() > limit;
    users.truncate(limit);
    Ok(UserLookupDTO { users, has_more })
}

fn validation(field: &str, error: impl Into<String>) -> AppError {
    AppError::from(G5Error::ValidationError {
        details: serde_json::json!({"field": field, "error": error.into()}),
    })
}

/// `GET /api/internal/workspace-agent/:id/users`: the owner's organization
/// roster as the Members page reads it, with each member's connected
/// provider accounts, so the agent can name assignees
/// (`g5.organization.agent-member-directory`). Unaudited, like other reads.
async fn lookup_users(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(path): Path<CallerPath>,
    Query(query): Query<UserLookupQuery>,
) -> Result<Json<ResponseEnvelope<UserLookupDTO>>, AppError> {
    require_own_workspace(path.id, &incarnation)?;
    // The owner's actor scope rechecks the incarnation and their active
    // membership, as for the Members page.
    let db = agent_owner_db(&state, &incarnation, &["workspaces:read"]).await?;
    let members = state
        .org_service
        .list_members(&state.system_pool, incarnation.org_id)
        .await?;
    let ids: Vec<Uuid> = members.iter().map(|member| member.user_id).collect();
    let mut logins =
        crate::services::github_user_grants::connected_logins(&state.system_pool, &ids).await?;
    db.rollback().await.map_err(AppError::from)?;
    let roster = members
        .into_iter()
        .map(|member| DirectoryUserDTO {
            accounts: logins
                .remove(&member.user_id)
                .map(|login| ConnectedAccountDTO {
                    provider: "github",
                    login,
                })
                .into_iter()
                .collect(),
            me: member.user_id == incarnation.owner_user_id,
            state: if member.deactivated_at.is_some() {
                "deactivated"
            } else {
                "active"
            },
            id: member.user_id,
            name: member.display_name,
            email: member.email,
            role: member.role,
        })
        .collect();
    Ok(Json(ResponseEnvelope::ok(select_users(roster, &query)?)))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceProbeDTO {
    workspace: AgentWorkspaceSummary,
    /// `self`, `child` or `other` (readable only).
    relation: &'static str,
    vm: WorkspaceVmView,
    tasks: Vec<g5_api_foundation::contracts::tasks::TaskListEntry>,
    sessions: Vec<crate::services::chat_service::SessionListItem>,
}

/// Durable state of a readable workspace: lifecycle, recent tasks with
/// outcomes and its chats. Reads only the database, so it never starts or
/// prolongs compute (`g5.story.agent-probe-child`).
async fn probe_workspace(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(path): Path<TargetPath>,
) -> Result<Json<ResponseEnvelope<WorkspaceProbeDTO>>, AppError> {
    require_own_workspace(path.id, &incarnation)?;
    let target_id = path.target_id;
    let mut db = agent_owner_db(&state, &incarnation, &["workspaces:read"]).await?;
    agent_read_target(&state, &mut db, &incarnation, target_id).await?;
    let workspace = workspace_summary(&mut db, target_id).await?;
    // A probe only reads, so it describes a workspace in scope whether or not
    // the owner lets this agent change it.
    let relation = match agent_relation(
        &mut db,
        &incarnation,
        target_id,
        AgentTargetNeed::SelfOrOther,
    )
    .await
    {
        Ok(AgentTarget::SelfWorkspace) => "self",
        Ok(AgentTarget::Other) if workspace.cloned_from_id == Some(incarnation.workspace_id) => {
            "child"
        }
        Ok(AgentTarget::Other) => "owned",
        Err(_) => "other",
    };
    let (tasks, _, _) = state
        .task_service
        .list_workspace_tasks(&mut db, target_id, None, false, 20)
        .await?;
    let sessions = state
        .chat_service
        .list_sessions(
            &mut db,
            target_id,
            incarnation.owner_user_id,
            crate::services::chat_service::SessionListFilters { scope_type: None },
            &crate::services::PaginationParams {
                page: 1,
                per_page: 50,
            },
        )
        .await?
        .data;
    let scope = db.reopener();
    db.rollback().await.map_err(AppError::from)?;
    let vm = super::workspaces::workspace_vm_service(&state, scope)?
        .view(target_id)
        .await?;
    Ok(Json(ResponseEnvelope::ok(WorkspaceProbeDTO {
        workspace,
        relation,
        vm,
        tasks,
        sessions,
    })))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionListQuery {
    #[serde(default = "default_page")]
    page: i64,
    #[serde(default = "default_per_page")]
    per_page: i64,
}

fn default_page() -> i64 {
    1
}

fn default_per_page() -> i64 {
    50
}

async fn list_workspace_sessions(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(path): Path<TargetPath>,
    Query(query): Query<SessionListQuery>,
) -> Result<
    Json<
        ResponseEnvelope<
            crate::services::PaginatedResult<crate::services::chat_service::SessionListItem>,
        >,
    >,
    AppError,
> {
    require_own_workspace(path.id, &incarnation)?;
    let target_id = path.target_id;
    let mut db = agent_owner_db(&state, &incarnation, &["workspaces:read"]).await?;
    agent_read_target(&state, &mut db, &incarnation, target_id).await?;
    let sessions = state
        .chat_service
        .list_sessions(
            &mut db,
            target_id,
            incarnation.owner_user_id,
            crate::services::chat_service::SessionListFilters { scope_type: None },
            &crate::services::PaginationParams {
                page: query.page,
                per_page: query.per_page,
            },
        )
        .await?;
    db.rollback().await.map_err(AppError::from)?;
    Ok(Json(ResponseEnvelope::ok(sessions)))
}

/// A readable session's workspace, admitted for reads.
async fn admit_session_read(
    state: &Arc<AppState>,
    incarnation: &WorkspaceIncarnationClaims,
    session_id: Uuid,
) -> Result<g5_authz::ScopedDb, AppError> {
    let mut db = agent_owner_db(state, incarnation, &["workspaces:read"]).await?;
    let context = resolve_chat_session_context(&mut db, session_id)
        .await
        .map_err(|_| forbidden())?;
    agent_read_target(state, &mut db, incarnation, context.workspace_id).await?;
    Ok(db)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessagesQuery {
    #[serde(default)]
    cursor: Option<i32>,
    #[serde(default = "default_message_limit")]
    limit: i32,
    /// Return the latest `limit` messages instead of the first.
    #[serde(default)]
    tail: bool,
}

fn default_message_limit() -> i32 {
    50
}

/// A readable chat's messages: the inputs and the agent's replies, in order
/// (`g5.security.agent-read-scope`).
async fn read_session_messages(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(path): Path<SessionPath>,
    Query(query): Query<MessagesQuery>,
) -> Result<Json<ResponseEnvelope<crate::services::chat_service::MessageListResponse>>, AppError> {
    require_own_workspace(path.id, &incarnation)?;
    let session_id = path.session_id;
    let mut db = admit_session_read(&state, &incarnation, session_id).await?;
    let messages = state
        .chat_service
        .get_messages(
            &mut db,
            session_id,
            query.cursor,
            query.limit.clamp(1, 200),
            query.tail,
        )
        .await?;
    db.rollback().await.map_err(AppError::from)?;
    Ok(Json(ResponseEnvelope::ok(messages)))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineQuery {
    #[serde(default)]
    cursor: Option<String>,
    #[serde(default = "default_timeline_limit")]
    limit: i64,
}

fn default_timeline_limit() -> i64 {
    100
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineDTO {
    entries: Vec<crate::services::chat_service::SessionTimelineEntry>,
    cursor: Option<String>,
    has_more: bool,
}

/// A readable chat's timeline, tool calls and their output included, as the
/// UI shows it to the owner.
async fn read_session_timeline(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(path): Path<SessionPath>,
    Query(query): Query<TimelineQuery>,
) -> Result<Json<ResponseEnvelope<TimelineDTO>>, AppError> {
    require_own_workspace(path.id, &incarnation)?;
    let session_id = path.session_id;
    let mut db = admit_session_read(&state, &incarnation, session_id).await?;
    let (entries, cursor, has_more) = state
        .chat_service
        .get_session_timeline(&mut db, session_id, query.cursor, query.limit.clamp(1, 200))
        .await?;
    db.rollback().await.map_err(AppError::from)?;
    Ok(Json(ResponseEnvelope::ok(TimelineDTO {
        entries,
        cursor,
        has_more,
    })))
}

// ─── Fork with a prompt ──────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ForkRequestDTO {
    name: String,
    /// The workspace to fork: this one (default), another of the owner's
    /// workspaces or Base (`g5.security.agent-workspace-scope`).
    #[serde(default)]
    source_workspace_id: Option<Uuid>,
    #[serde(default)]
    description: Option<String>,
    /// The child agent's first message; without it the child is only forked.
    #[serde(default)]
    prompt: Option<String>,
    idempotency_key: String,
    /// Start the child for its prompt (default). Without a prompt nothing
    /// starts: an ordinary fork stays stopped.
    #[serde(default = "default_true")]
    start: bool,
    /// Take `name-2`, `name-3`, … when the name is taken.
    #[serde(default)]
    uniquify_name: bool,
    /// An existing issue to link to the child, as the UI's fork dialog does.
    #[serde(default)]
    issue: Option<ForkIssueDTO>,
    #[serde(flatten)]
    launch: LaunchChoices,
}

fn default_true() -> bool {
    true
}

/// The issue a fork links to its child (`g5d workspace fork --issue`).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ForkIssueDTO {
    /// An issue URL, `owner/name#N`, `KEY-N` or a number in the creation
    /// domain.
    #[serde(rename = "ref")]
    reference: String,
    /// `primary` (default, as in the dialog) or `related`.
    #[serde(default)]
    role: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForkResponseDTO {
    workspace: crate::services::workspace_fork_service::CloneWorkspaceDetail,
    started_chat: Option<crate::services::workspace_fork_service::ForkStartedChatOutcome>,
    /// The link the fork made, when it was asked for one.
    #[serde(skip_serializing_if = "Option::is_none")]
    issue_link: Option<crate::services::workspace_fork_service::ForkIssueLinkOutcome>,
    #[serde(skip_serializing_if = "Option::is_none")]
    start: Option<StartReport>,
}

/// Fork the caller's workspace, another of its owner's or Base into a new
/// workspace of the owner, with a prompt for its agent, then launch it
/// (`g5.story.agent-fork-child`). The fork is durable: it copies the source's
/// last complete save or its committed branch heads, never interrupting a
/// running turn.
async fn fork_with_prompt(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(path): Path<CallerPath>,
    Json(body): Json<ForkRequestDTO>,
) -> Result<(StatusCode, Json<ResponseEnvelope<ForkResponseDTO>>), AppError> {
    require_own_workspace(path.id, &incarnation)?;
    validate_key("idempotencyKey", &body.idempotency_key)?;
    let source_workspace_id = body.source_workspace_id.unwrap_or(incarnation.workspace_id);
    let mut db = agent_owner_db(&state, &incarnation, &["workspaces:read"]).await?;
    // A fork makes a new workspace: the owner must let this agent change
    // other workspaces (`g5.security.agent-other-workspaces`).
    require_other_workspaces_allowed(&mut db, &incarnation).await?;
    agent_fork_source(&state, &mut db, &incarnation, source_workspace_id).await?;
    let source = source_workspace_id;
    crate::services::workspace_review_service::ensure_not_review_workspace(
        &mut db,
        incarnation.org_id,
        source,
        "Fork",
    )
    .await?;
    // The child copies this workspace's configuration, so its first chat
    // resolves as one here would.
    if body.prompt.is_some() {
        ensure_runnable_launch(
            &state,
            &mut db,
            &incarnation,
            source,
            body.launch.preferences(),
        )
        .await?;
    }
    // The issue is resolved, checked and linked inside the fork, as for the
    // UI's dialog; only a bare number is qualified here, as `issue link`
    // qualifies it.
    let issue = match body.issue {
        Some(issue) => {
            let role = match issue.role.as_deref().map(str::trim) {
                None => crate::services::external_issues::LinkRole::Primary,
                Some(role) => {
                    crate::services::external_issues::LinkRole::parse(role).ok_or_else(|| {
                        AppError::from(G5Error::ValidationError {
                            details: serde_json::json!({
                                "field": "issue.role",
                                "error": "must be primary or related",
                            }),
                        })
                    })?
                }
            };
            let domains =
                crate::services::external_issues::list_domain_access(&mut db, incarnation.org_id)
                    .await?;
            Some(crate::services::workspace_fork_service::ForkIssueRequest {
                reference: crate::services::issue_creations::creation_reference(
                    &domains,
                    &issue.reference,
                ),
                role,
            })
        }
        None => None,
    };
    let fork_scope = db.reopener();
    db.commit().await.map_err(AppError::from)?;

    let launch_preferences = (!body.launch.is_empty()).then(|| body.launch.preferences());
    let request = crate::services::workspace_fork_service::ForkWorkspaceRequest {
        name: body.name,
        description: body.description,
        // The owner's fork keys are one namespace; the agent's stay apart
        // from the browser's and from the reserved review prefix.
        idempotency_key: format!("agent:{}", body.idempotency_key),
        read_only: false,
        merge_target_workspace_id: None,
        chat_session_ids: Vec::new(),
        source: crate::services::workspace_fork_service::ForkSource::Durable,
        interrupt: false,
        issue,
        start_chat: body.prompt.map(|content| {
            crate::services::workspace_fork_service::ForkStartChatRequest {
                content,
                launch_preferences,
            }
        }),
        uniquify_name: body.uniquify_name,
        reverification: None,
    };
    let result = state
        .workspace_fork_service
        .scoped(fork_scope)
        .fork_workspace(
            incarnation.org_id,
            source,
            incarnation.owner_user_id,
            request,
        )
        .await?;
    let child = result.workspace.id;
    let start =
        match result
            .started_chat
            .as_ref()
            .and_then(|chat| chat.session_id)
            .filter(|_| body.start)
        {
            Some(session_id) => {
                let launched =
                    match lifecycle_for(&state, &incarnation, child, AgentTargetNeed::OtherOnly)
                        .await
                    {
                        Ok(lifecycle) => lifecycle
                            .launch_forked_workspace(child, incarnation.owner_user_id, session_id)
                            .await
                            .map_err(AppError::from),
                        Err(error) => Err(error),
                    };
                Some(match launched {
                    Ok(outcome) => StartReport {
                        outcome: Some(outcome),
                        error: None,
                        skipped_on_replay: false,
                    },
                    Err(error) => StartReport {
                        outcome: None,
                        error: Some(error.inner.to_string()),
                        skipped_on_replay: false,
                    },
                })
            }
            None => None,
        };
    audit(&incarnation, "fork", child, "completed");
    Ok((
        StatusCode::CREATED,
        Json(ResponseEnvelope::ok(ForkResponseDTO {
            workspace: result.workspace,
            started_chat: result.started_chat,
            issue_link: result.issue_link,
            start,
        })),
    ))
}

// ─── Child chats ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChildChatRequestDTO {
    content: String,
    request_key: String,
    #[serde(default)]
    title: Option<String>,
    /// Start the child after queuing, as one labelled compound request.
    #[serde(default)]
    start: bool,
    #[serde(flatten)]
    launch: LaunchChoices,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChildChatResponseDTO {
    session_id: Uuid,
    input_id: Uuid,
    task_id: Uuid,
    task_status: String,
    delivery: String,
    replayed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    start: Option<StartReport>,
}

/// Start a new chat in a child (`g5.story.agent-chat-child`). The input
/// queues; it starts nothing unless the request also asks to start.
async fn start_child_chat(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(path): Path<TargetPath>,
    Json(body): Json<ChildChatRequestDTO>,
) -> Result<(StatusCode, Json<ResponseEnvelope<ChildChatResponseDTO>>), AppError> {
    require_own_workspace(path.id, &incarnation)?;
    validate_key("requestKey", &body.request_key)?;
    let target_id = path.target_id;
    let mut db = agent_owner_db(&state, &incarnation, &["workspaces:write"]).await?;
    agent_target(&mut db, &incarnation, target_id, AgentTargetNeed::OtherOnly).await?;
    crate::services::workspace_mutation_policy::ensure_workspace_mutable(
        &mut db,
        target_id,
        "agent.start_child_chat",
    )
    .await?;
    ensure_runnable_launch(
        &state,
        &mut db,
        &incarnation,
        target_id,
        body.launch.preferences(),
    )
    .await?;
    let message = WorkspaceMessage {
        org_id: incarnation.org_id,
        workspace_id: target_id,
        actor_user_id: incarnation.owner_user_id,
        target: MessageTarget::NewSession {
            title: body.title,
            scope_type: None,
            scope_ref: None,
        },
        content: body.content,
        agent_launch: None,
        legacy_preferences: body.launch.preferences(),
        attachment_digest: None,
        request_key: Some(format!("agent:{}", body.request_key)),
        workbench: None,
    };
    let sent = state.workspace_messenger.send(&mut db, message).await?;
    db.commit().await.map_err(AppError::from)?;
    let start = if body.start {
        Some(start_after_input(&state, &incarnation, target_id, sent.replayed).await)
    } else {
        None
    };
    audit(
        &incarnation,
        "child_chat",
        target_id,
        if sent.replayed { "replayed" } else { "queued" },
    );
    let status = if sent.replayed {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };
    Ok((
        status,
        Json(ResponseEnvelope::ok(ChildChatResponseDTO {
            session_id: sent.session.id,
            input_id: sent.queued.input_id,
            task_id: sent.queued.task_id,
            task_status: sent.queued.task_status,
            delivery: sent.queued.delivery,
            replayed: sent.replayed,
            start,
        })),
    ))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChildFollowUpRequestDTO {
    content: String,
    request_key: String,
    #[serde(default)]
    start: bool,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    effort: Option<String>,
    #[serde(default)]
    permission_mode: Option<String>,
}

/// Queue a follow-up in a child's chat with the same launch inheritance as
/// the composer (`g5.story.agent-chat-child`). It is queued, not steered
/// into a running turn.
async fn send_child_follow_up(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(path): Path<TargetSessionPath>,
    Json(body): Json<ChildFollowUpRequestDTO>,
) -> Result<(StatusCode, Json<ResponseEnvelope<ChildChatResponseDTO>>), AppError> {
    require_own_workspace(path.id, &incarnation)?;
    validate_key("requestKey", &body.request_key)?;
    let target_id = path.target_id;
    let session_id = path.session_id;
    let mut db = agent_owner_db(&state, &incarnation, &["workspaces:write"]).await?;
    agent_target(&mut db, &incarnation, target_id, AgentTargetNeed::OtherOnly).await?;
    let context = resolve_chat_session_context(&mut db, session_id)
        .await
        .map_err(|_| forbidden())?;
    if context.workspace_id != target_id {
        return Err(forbidden());
    }
    crate::services::workspace_mutation_policy::ensure_workspace_mutable(
        &mut db,
        target_id,
        "agent.child_follow_up",
    )
    .await?;
    // A chat keeps its first turn's permission mode, so a follow-up runs with
    // that mode, or with the one it names when the chat has none yet.
    let session = state.chat_service.get_session(&mut db, session_id).await?;
    let frozen = super::chat::frozen_chat_permission_mode(
        &state,
        &mut db,
        session_id,
        session.agent_provider,
        &session.provider_state,
    )
    .await?;
    match (frozen.as_deref(), body.permission_mode.as_deref()) {
        (Some(mode), _) => ensure_runnable_permission_mode(session.agent_provider, mode, true)?,
        (None, Some(mode)) => ensure_runnable_permission_mode(session.agent_provider, mode, false)?,
        (None, None) => {}
    }
    let message = WorkspaceMessage {
        org_id: incarnation.org_id,
        workspace_id: target_id,
        actor_user_id: incarnation.owner_user_id,
        target: MessageTarget::ExistingSession { session_id },
        content: body.content.clone(),
        agent_launch: None,
        legacy_preferences: crate::services::config_service::legacy_agent_launch_preferences(
            None,
            body.model.as_deref(),
            body.permission_mode.as_deref(),
            body.effort.as_deref(),
        ),
        attachment_digest: None,
        request_key: Some(format!("agent:{}", body.request_key)),
        workbench: None,
    };
    let (queued, replayed) = match state.workspace_messenger.claim(&mut db, &message).await? {
        Some(replayed) => (replayed.queued, true),
        None => {
            let queued = super::chat::queue_follow_up(
                &state,
                &mut db,
                incarnation.org_id,
                target_id,
                session_id,
                incarnation.owner_user_id,
                super::chat::FollowUp {
                    content: body.content,
                    model: body.model,
                    permission_mode: body.permission_mode,
                    effort: body.effort,
                    agent_provider: None,
                    account: None,
                    attachments: None,
                },
            )
            .await?;
            state
                .workspace_messenger
                .record_claim(&mut db, &message, session_id, queued.input_id)
                .await?;
            (queued, false)
        }
    };
    db.commit().await.map_err(AppError::from)?;
    let start = if body.start {
        Some(start_after_input(&state, &incarnation, target_id, replayed).await)
    } else {
        None
    };
    audit(
        &incarnation,
        "child_follow_up",
        target_id,
        if replayed { "replayed" } else { "queued" },
    );
    let status = if replayed {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };
    Ok((
        status,
        Json(ResponseEnvelope::ok(ChildChatResponseDTO {
            session_id,
            input_id: queued.input_id,
            task_id: queued.task_id,
            task_status: queued.task_status,
            delivery: queued.delivery,
            replayed,
            start,
        })),
    ))
}

// ─── Lifecycle ───────────────────────────────────────────────────────────────

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LifecycleRequestDTO {
    /// Names the operation so a retry joins it.
    #[serde(default)]
    operation_key: Option<String>,
    /// Delete only: stop without waiting for a final save.
    #[serde(default)]
    force: bool,
}

/// Start a child (explicit Start consent for its owner).
async fn start_child(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(path): Path<TargetPath>,
    body: Option<Json<LifecycleRequestDTO>>,
) -> Result<Json<ResponseEnvelope<ResumeOutcome>>, AppError> {
    require_own_workspace(path.id, &incarnation)?;
    let target_id = path.target_id;
    let body = body.map(|Json(body)| body).unwrap_or_default();
    let intent_id = derived_operation_id(
        &incarnation,
        "start",
        target_id,
        body.operation_key.as_deref(),
    )?;
    let outcome = lifecycle_for(&state, &incarnation, target_id, AgentTargetNeed::OtherOnly)
        .await?
        .resume(
            target_id,
            incarnation.owner_user_id,
            intent_id,
            StartKind::Resume,
        )
        .await?;
    audit(&incarnation, "start", target_id, "admitted");
    Ok(Json(ResponseEnvelope::ok(outcome)))
}

/// Stop the caller's own workspace or a child. Stopping itself ends the
/// caller's VM after its save window; the admitted stop outlives the request.
async fn stop_workspace(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(path): Path<TargetPath>,
    body: Option<Json<LifecycleRequestDTO>>,
) -> Result<Json<ResponseEnvelope<StopOutcome>>, AppError> {
    require_own_workspace(path.id, &incarnation)?;
    let target_id = path.target_id;
    let body = body.map(|Json(body)| body).unwrap_or_default();
    let operation_id = derived_operation_id(
        &incarnation,
        "stop",
        target_id,
        body.operation_key.as_deref(),
    )?;
    let lifecycle = lifecycle_for(
        &state,
        &incarnation,
        target_id,
        AgentTargetNeed::SelfOrOther,
    )
    .await?;
    audit(&incarnation, "stop", target_id, "admitting");
    let outcome = lifecycle
        .stop(
            target_id,
            incarnation.owner_user_id,
            operation_id,
            StopReason::User,
            None,
        )
        .await?;
    Ok(Json(ResponseEnvelope::ok(outcome)))
}

/// Delete a child through the UI's Stop-then-delete operations. The caller's
/// own workspace is refused before any effect, and a child with live
/// children is refused by the deletion itself.
async fn delete_child(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(path): Path<TargetPath>,
    body: Option<Json<LifecycleRequestDTO>>,
) -> Result<Json<ResponseEnvelope<DeletedDTO>>, AppError> {
    require_own_workspace(path.id, &incarnation)?;
    let target_id = path.target_id;
    let body = body.map(|Json(body)| body).unwrap_or_default();
    let operation_id = derived_operation_id(
        &incarnation,
        "delete",
        target_id,
        body.operation_key.as_deref(),
    )?;
    let stopped = lifecycle_for(&state, &incarnation, target_id, AgentTargetNeed::OtherOnly)
        .await?
        .stop(
            target_id,
            incarnation.owner_user_id,
            operation_id,
            if body.force {
                StopReason::Delete
            } else {
                StopReason::User
            },
            body.force.then(chrono::Utc::now),
        )
        .await?;
    if stopped.status != "stopped" {
        return Err(G5Error::WorkspaceStopping.into());
    }
    let mut db = agent_owner_db(&state, &incarnation, &["workspaces:write"]).await?;
    agent_target(&mut db, &incarnation, target_id, AgentTargetNeed::OtherOnly).await?;
    crate::services::workspace_mutation_policy::ensure_workspace_owner(&mut db, target_id).await?;
    // Its pull requests close first, in their own transaction, so a deletion
    // that fails afterwards is simply retried (`g5.workspace.delete.pull-requests`).
    let scope = db.reopener();
    db.rollback().await.map_err(AppError::from)?;
    state
        .workspace_service
        .close_pull_requests_for_deletion(&scope, target_id, incarnation.owner_user_id)
        .await?;
    let mut db = scope.open().await.map_err(AppError::from)?;
    agent_target(&mut db, &incarnation, target_id, AgentTargetNeed::OtherOnly).await?;
    crate::services::workspace_mutation_policy::ensure_workspace_owner(&mut db, target_id).await?;
    state
        .workspace_service
        .delete_workspace(&mut db, target_id, incarnation.owner_user_id, operation_id)
        .await?;
    db.commit().await.map_err(AppError::from)?;
    audit(&incarnation, "delete", target_id, "deleted");
    Ok(Json(ResponseEnvelope::ok(DeletedDTO {
        workspace_id: target_id,
        deleted: true,
    })))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletedDTO {
    workspace_id: Uuid,
    deleted: bool,
}

// ─── Merge a child ───────────────────────────────────────────────────────────

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChildPullRequestDTO {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    mode: crate::services::workspace_pull_request_create::PullRequestWriteMode,
    #[serde(default)]
    replace: bool,
}

/// Open or update the G5 pull request from a child into the caller's
/// workspace, as the owner would from the child's page.
async fn publish_child_pull_request(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(path): Path<TargetPath>,
    body: Option<Json<ChildPullRequestDTO>>,
) -> Result<
    Json<
        ResponseEnvelope<crate::services::workspace_pull_request_create::CreatePullRequestOutcome>,
    >,
    AppError,
> {
    require_own_workspace(path.id, &incarnation)?;
    let target_id = path.target_id;
    let body = body.map(|Json(body)| body).unwrap_or_default();
    let outcome = crate::services::workspace_pull_request_create::create_or_update(
        &state,
        crate::services::workspace_pull_request_create::PullRequestCaller::WorkspaceChild(
            &incarnation,
        ),
        target_id,
        crate::services::workspace_pull_request_create::CreatePullRequestRequest {
            target_workspace_id: Some(incarnation.workspace_id),
            title: body.title,
            description: body.description,
            mode: body.mode,
            replace: body.replace,
            // Publishing a child saves the child, as the owner's UI would.
            save: true,
        },
    )
    .await?;
    audit(&incarnation, "child_pull_request", target_id, "published");
    Ok(Json(ResponseEnvelope::ok(outcome)))
}

/// Merge a pull request into the caller's own workspace with the UI's merge:
/// approvals, external-check policy and provider-first merge included
/// (`g5.story.agent-merge-child`). The caller's branch then moves centrally;
/// its agent integrates it before its next push.
async fn merge_pull_request(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(path): Path<PullRequestPath>,
) -> Result<Json<ResponseEnvelope<super::workspaces::MergeWorkspacePullRequestResponseDTO>>, AppError>
{
    require_own_workspace(path.id, &incarnation)?;
    let pull_request_id = path.pull_request_id;
    let merged = super::workspaces::merge_pull_request_as(
        &state,
        &super::workspaces::PullRequestMerger::WorkspaceAgent(&incarnation),
        &super::workspaces::WorkspacePullRequestPathParams {
            id: incarnation.workspace_id,
            pull_request_id,
        },
        // A workspace agent never merges with an override
        // (g5.workspace.merge.admin-override).
        None,
    )
    .await?;
    audit(
        &incarnation,
        "merge_pull_request",
        incarnation.workspace_id,
        "merged",
    );
    Ok(Json(ResponseEnvelope::ok(merged)))
}

// ─── Stop workspaces by user ─────────────────────────────────────────────────

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OwnerWorkspacesDTO {
    #[serde(skip_serializing_if = "Option::is_none")]
    hold_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    joined: Option<bool>,
    workspaces: Vec<crate::services::org_service::MemberShutdownWorkspace>,
}

/// Stop every workspace the owner has in the organization, this one included
/// (`g5.lifecycle.stop.by-user`).
async fn stop_owner_workspaces(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(path): Path<CallerPath>,
) -> Result<(StatusCode, Json<ResponseEnvelope<OwnerWorkspacesDTO>>), AppError> {
    require_own_workspace(path.id, &incarnation)?;
    let mut db = agent_owner_system_db(&state, &incarnation, &["workspaces:write"]).await?;
    let owner = incarnation.owner_user_id;
    let hold = state
        .org_service
        .stop_member_workspaces(&mut db, incarnation.org_id, owner, owner)
        .await?;
    let workspaces = state
        .org_service
        .member_shutdown_progress(&mut db, incarnation.org_id, owner, owner)
        .await?;
    db.commit().await.map_err(AppError::from)?;
    audit(
        &incarnation,
        "stop_owner_workspaces",
        incarnation.workspace_id,
        if hold.joined { "joined" } else { "held" },
    );
    let status = if hold.joined {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };
    Ok((
        status,
        Json(ResponseEnvelope::ok(OwnerWorkspacesDTO {
            hold_id: Some(hold.hold_id),
            joined: Some(hold.joined),
            workspaces,
        })),
    ))
}

/// Shutdown state of the owner's workspaces while a hold is pending.
async fn owner_workspaces_progress(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(path): Path<CallerPath>,
) -> Result<Json<ResponseEnvelope<OwnerWorkspacesDTO>>, AppError> {
    require_own_workspace(path.id, &incarnation)?;
    let mut db = agent_owner_system_db(&state, &incarnation, &["workspaces:read"]).await?;
    let owner = incarnation.owner_user_id;
    let workspaces = state
        .org_service
        .member_shutdown_progress(&mut db, incarnation.org_id, owner, owner)
        .await?;
    db.rollback().await.map_err(AppError::from)?;
    Ok(Json(ResponseEnvelope::ok(OwnerWorkspacesDTO {
        hold_id: None,
        joined: None,
        workspaces,
    })))
}

// ─── Increment 2: child chat and task controls ───────────────────────────────

#[derive(Debug, Deserialize)]
pub struct TargetSessionInputPath {
    id: Uuid,
    #[serde(rename = "targetId")]
    target_id: Uuid,
    #[serde(rename = "sessionId")]
    session_id: Uuid,
    #[serde(rename = "inputId")]
    input_id: Uuid,
    action: String,
}

#[derive(Debug, Deserialize)]
pub struct TargetTaskPath {
    id: Uuid,
    #[serde(rename = "targetId")]
    target_id: Uuid,
    #[serde(rename = "taskId")]
    task_id: Uuid,
    action: String,
}

#[derive(Debug, Deserialize)]
pub struct TargetRequestPath {
    id: Uuid,
    #[serde(rename = "targetId")]
    target_id: Uuid,
    #[serde(rename = "requestId")]
    request_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct TargetReviewPath {
    id: Uuid,
    #[serde(rename = "targetId")]
    target_id: Uuid,
    #[serde(rename = "reviewId")]
    review_id: Uuid,
}

/// A write scope as the owner with `target_id` admitted as a child and
/// `session_id` one of its chats; read-only workspaces refuse chat writes.
async fn admit_child_chat(
    state: &Arc<AppState>,
    incarnation: &WorkspaceIncarnationClaims,
    target_id: Uuid,
    session_id: Uuid,
    operation: &'static str,
) -> Result<g5_authz::ScopedDb, AppError> {
    let mut db = agent_owner_db(state, incarnation, &["workspaces:write"]).await?;
    agent_target(&mut db, incarnation, target_id, AgentTargetNeed::OtherOnly).await?;
    let context = resolve_chat_session_context(&mut db, session_id)
        .await
        .map_err(|_| forbidden())?;
    if context.workspace_id != target_id {
        return Err(forbidden());
    }
    crate::services::workspace_mutation_policy::ensure_workspace_mutable(
        &mut db, target_id, operation,
    )
    .await?;
    Ok(db)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StopTurnDTO {
    /// The input or message id the chat returned for the turn.
    turn_id: Uuid,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoppedDTO {
    stopped: bool,
    /// What the Stop established (`g5.chat.controls`), as the composer's
    /// Stop reports it.
    #[serde(skip_serializing_if = "Option::is_none")]
    disposition: Option<&'static str>,
}

/// Stop a child chat's turn, as the composer's Stop does.
async fn stop_child_turn(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(path): Path<TargetSessionPath>,
    Json(body): Json<StopTurnDTO>,
) -> Result<Json<ResponseEnvelope<StoppedDTO>>, AppError> {
    require_own_workspace(path.id, &incarnation)?;
    let target_id = path.target_id;
    let session_id = path.session_id;
    let mut db = admit_child_chat(
        &state,
        &incarnation,
        target_id,
        session_id,
        "agent.stop_child_turn",
    )
    .await?;
    crate::db::scoped::ensure_session_turn_id(&mut db, session_id, body.turn_id, "turnId").await?;
    let stop = state
        .task_service
        .end_chat_turn(&mut db, incarnation.owner_user_id, session_id, body.turn_id)
        .await?;
    db.commit().await.map_err(AppError::from)?;
    let disposition = match &stop {
        crate::services::task_lifecycle::ChatTurnStop::Stopped(stop) => state
            .task_service
            .deliver_workspace_task_cancel(stop)
            .await?
            .as_str(),
        crate::services::task_lifecycle::ChatTurnStop::Withdrawn => "withdrawn",
        crate::services::task_lifecycle::ChatTurnStop::AlreadyEnded => "already_ended",
    };
    let stopped = !matches!(
        stop,
        crate::services::task_lifecycle::ChatTurnStop::AlreadyEnded
    );
    audit(
        &incarnation,
        "stop_child_turn",
        target_id,
        if stopped { "stopped" } else { "already_ended" },
    );
    Ok(Json(ResponseEnvelope::ok(StoppedDTO {
        stopped,
        disposition: Some(disposition),
    })))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BackgroundEndDTO {
    task_id: Uuid,
    attempt: i32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeliveredDTO {
    delivered: bool,
}

/// End a child chat's background wait (needs the child's running VM).
async fn end_child_background_wait(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(path): Path<TargetSessionPath>,
    Json(body): Json<BackgroundEndDTO>,
) -> Result<Json<ResponseEnvelope<DeliveredDTO>>, AppError> {
    require_own_workspace(path.id, &incarnation)?;
    let target_id = path.target_id;
    let session_id = path.session_id;
    let db = admit_child_chat(
        &state,
        &incarnation,
        target_id,
        session_id,
        "agent.end_child_background_wait",
    )
    .await?;
    state
        .task_service
        .end_session_background_wait(
            db,
            incarnation.owner_user_id,
            session_id,
            body.task_id,
            body.attempt,
        )
        .await?;
    audit(
        &incarnation,
        "end_child_background_wait",
        target_id,
        "delivered",
    );
    Ok(Json(ResponseEnvelope::ok(DeliveredDTO { delivered: true })))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RenameDTO {
    /// The new title; empty or omitted clears it.
    #[serde(default)]
    title: Option<String>,
}

/// Rename a child's chat.
async fn rename_child_chat(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(path): Path<TargetSessionPath>,
    Json(body): Json<RenameDTO>,
) -> Result<Json<ResponseEnvelope<super::chat::SessionDetailDTO>>, AppError> {
    require_own_workspace(path.id, &incarnation)?;
    let target_id = path.target_id;
    let session_id = path.session_id;
    let mut db = admit_child_chat(
        &state,
        &incarnation,
        target_id,
        session_id,
        "agent.rename_child_chat",
    )
    .await?;
    let mut session = state
        .chat_service
        .update_session_title(&mut db, session_id, body.title)
        .await?;
    state
        .chat_service
        .enrich_session_detail(&mut db, &mut session)
        .await?;
    db.commit().await.map_err(AppError::from)?;
    audit(&incarnation, "rename_child_chat", target_id, "renamed");
    Ok(Json(ResponseEnvelope::ok(session.into())))
}

/// A readable chat's queued inputs, with their revisions and delivery.
async fn read_session_inputs(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(path): Path<SessionPath>,
) -> Result<Json<ResponseEnvelope<Vec<crate::services::agent_dispatch::SessionInputView>>>, AppError>
{
    require_own_workspace(path.id, &incarnation)?;
    let session_id = path.session_id;
    let mut db = admit_session_read(&state, &incarnation, session_id).await?;
    let inputs = state
        .task_service
        .list_session_inputs(&mut db, session_id)
        .await?;
    db.rollback().await.map_err(AppError::from)?;
    Ok(Json(ResponseEnvelope::ok(inputs)))
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InputChangeDTO {
    /// The revision the agent last saw (edit and withdraw).
    #[serde(default)]
    expected_revision: Option<i32>,
    /// The new text (edit).
    #[serde(default)]
    content: Option<String>,
}

/// Edit, withdraw or redeliver a queued input in a child's chat, with the
/// composer's revision guards (`g5.chat.queue`).
async fn change_child_input(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(path): Path<TargetSessionInputPath>,
    body: Option<Json<InputChangeDTO>>,
) -> Result<Json<ResponseEnvelope<crate::services::agent_dispatch::SessionInputView>>, AppError> {
    require_own_workspace(path.id, &incarnation)?;
    let target_id = path.target_id;
    let session_id = path.session_id;
    let input_id = path.input_id;
    let body = body.map(|Json(body)| body).unwrap_or_default();
    let revision = || {
        body.expected_revision.ok_or_else(|| {
            AppError::from(G5Error::ValidationError {
                details: serde_json::json!({"field": "expectedRevision", "error": "is required"}),
            })
        })
    };
    let mut db = admit_child_chat(
        &state,
        &incarnation,
        target_id,
        session_id,
        "agent.change_child_input",
    )
    .await?;
    let owner = incarnation.owner_user_id;
    let input = match path.action.as_str() {
        "edit" => {
            let content = body.content.clone().ok_or_else(|| {
                AppError::from(G5Error::ValidationError {
                    details: serde_json::json!({"field": "content", "error": "is required"}),
                })
            })?;
            state
                .task_service
                .edit_queued_session_input(
                    &mut db,
                    owner,
                    session_id,
                    input_id,
                    revision()?,
                    content,
                )
                .await?
        }
        "withdraw" => {
            state
                .task_service
                .withdraw_queued_session_input(&mut db, owner, session_id, input_id, revision()?)
                .await?
        }
        "redeliver" => {
            state
                .task_service
                .authorize_unknown_input_redelivery(&mut db, owner, session_id, input_id)
                .await?
        }
        _ => {
            return Err(AppError::from(G5Error::NotFound {
                resource: format!("input action {}", path.action),
            }))
        }
    };
    db.commit().await.map_err(AppError::from)?;
    audit(&incarnation, "change_child_input", target_id, &path.action);
    Ok(Json(ResponseEnvelope::ok(input)))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskActionDTO {
    task_id: Uuid,
    status: String,
}

/// Continue or cancel a task. A chat task is controlled only in a child;
/// another task, such as a preview, also in the caller's own workspace.
async fn control_task(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(path): Path<TargetTaskPath>,
) -> Result<Json<ResponseEnvelope<TaskActionDTO>>, AppError> {
    require_own_workspace(path.id, &incarnation)?;
    let target_id = path.target_id;
    let task_id = path.task_id;
    let mut db = agent_owner_db(&state, &incarnation, &["workspaces:write"]).await?;
    let context = crate::db::scoped::resolve_task_context(&mut db, task_id)
        .await
        .map_err(|_| forbidden())?;
    if context.workspace_id != target_id {
        return Err(forbidden());
    }
    let need = if crate::db::workspace_agent_scope::task_is_chat(&mut db, task_id).await? {
        AgentTargetNeed::OtherOnly
    } else {
        AgentTargetNeed::SelfOrOther
    };
    agent_target(&mut db, &incarnation, target_id, need).await?;
    let owner = incarnation.owner_user_id;
    let status = match path.action.as_str() {
        "continue" => {
            let status = state
                .task_service
                .continue_workspace_task(&mut db, owner, task_id)
                .await?;
            db.commit().await.map_err(AppError::from)?;
            status
        }
        "cancel" => {
            let result = state
                .task_service
                .request_workspace_task_cancel(&mut db, owner, task_id)
                .await?;
            db.commit().await.map_err(AppError::from)?;
            if let Err(error) = state
                .task_service
                .deliver_workspace_task_cancel(&result)
                .await
            {
                tracing::warn!(%task_id, %error, "durable task cancellation delivery remains pending");
            }
            result.status
        }
        _ => {
            return Err(AppError::from(G5Error::NotFound {
                resource: format!("task action {}", path.action),
            }))
        }
    };
    audit(&incarnation, "control_task", target_id, &path.action);
    Ok(Json(ResponseEnvelope::ok(TaskActionDTO {
        task_id,
        status,
    })))
}

// ─── Ontology changes cards ──────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ChatTaskPath {
    id: Uuid,
    #[serde(rename = "taskId")]
    task_id: Uuid,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TurnBaselineRequest {
    attempt: u32,
    baseline: serde_json::Value,
}

/// `GET …/chat-tasks/:taskId/ontology`: the running turn's start snapshot
/// (`null` until recorded) and the governing statements its agent named
/// (`g5.chat.ontology-card`). Only the turn's own VM reads it while it runs.
async fn get_turn_ontology(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(path): Path<ChatTaskPath>,
) -> Result<Json<ResponseEnvelope<serde_json::Value>>, AppError> {
    require_own_workspace(path.id, &incarnation)?;
    let task_id = path.task_id;
    let mut db = agent_owner_db(&state, &incarnation, &["workspaces:read"]).await?;
    let turn = crate::services::chat_ontology::running_turn(&mut db, &incarnation, task_id).await?;
    let record = crate::services::chat_ontology::turn_record(&mut db, turn.task_id).await?;
    Ok(Json(ResponseEnvelope::ok(record)))
}

/// `POST …/chat-tasks/:taskId/ontology/baseline`: the runner records the
/// turn's start before its provider starts; the first record stands.
async fn record_turn_baseline(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(path): Path<ChatTaskPath>,
    Json(body): Json<TurnBaselineRequest>,
) -> Result<Json<ResponseEnvelope<serde_json::Value>>, AppError> {
    require_own_workspace(path.id, &incarnation)?;
    let task_id = path.task_id;
    let mut db = agent_owner_db(&state, &incarnation, &["workspaces:write"]).await?;
    let turn = crate::services::chat_ontology::running_turn(&mut db, &incarnation, task_id).await?;
    if i64::from(body.attempt) != i64::from(turn.attempt) {
        return Err(AppError::from(G5Error::Conflict {
            reason: "this attempt is no longer the turn's current attempt".into(),
        }));
    }
    let record = crate::services::chat_ontology::record_baseline(
        &mut db,
        &incarnation,
        turn,
        &body.baseline,
    )
    .await?;
    db.commit().await.map_err(AppError::from)?;
    audit(
        &incarnation,
        "record_turn_baseline",
        incarnation.workspace_id,
        "recorded",
    );
    Ok(Json(ResponseEnvelope::ok(record)))
}

/// `POST …/chat-tasks/:taskId/ontology/governing`: `g5d chat governing`
/// names one unchanged statement governing the turn's changes
/// (`g5.chat.ontology-card.governing`).
async fn record_turn_governing(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(path): Path<ChatTaskPath>,
    Json(entry): Json<crate::services::chat_ontology::GoverningEntry>,
) -> Result<Json<ResponseEnvelope<serde_json::Value>>, AppError> {
    require_own_workspace(path.id, &incarnation)?;
    let task_id = path.task_id;
    let mut db = agent_owner_db(&state, &incarnation, &["workspaces:write"]).await?;
    let turn = crate::services::chat_ontology::running_turn(&mut db, &incarnation, task_id).await?;
    let record =
        crate::services::chat_ontology::record_governing(&mut db, &incarnation, turn, entry)
            .await?;
    db.commit().await.map_err(AppError::from)?;
    audit(
        &incarnation,
        "record_turn_governing",
        incarnation.workspace_id,
        "recorded",
    );
    Ok(Json(ResponseEnvelope::ok(record)))
}

// ─── Increment 2: approvals ──────────────────────────────────────────────────

/// A write scope as the owner, with `target_id` admitted as the caller's own
/// workspace or a child, and its ownership held for the approval operation.
async fn admit_approval_target(
    state: &Arc<AppState>,
    incarnation: &WorkspaceIncarnationClaims,
    target_id: Uuid,
) -> Result<g5_authz::ScopedDb, AppError> {
    let mut db = agent_owner_db(state, incarnation, &["workspaces:write"]).await?;
    agent_target(
        &mut db,
        incarnation,
        target_id,
        AgentTargetNeed::SelfOrOther,
    )
    .await?;
    crate::services::workspace_mutation_policy::ensure_workspace_owner(&mut db, target_id).await?;
    Ok(db)
}

/// A read scope as the owner, with `target_id` admitted as any workspace the
/// owner may currently read (`g5.security.agent-read-scope`): the approvals and
/// their reviews' evidence read wherever the owner could open the Approvals page,
/// so the agent can drill into any event its [activity feed] surfaces
/// (`g5.agent-approval.human-read-only.agent-inspection`, widened by #1099). The
/// transcript still gates target-side content on target read access; requesting
/// and cancelling keep the narrower `admit_approval_target` scope.
async fn admit_approval_read(
    state: &Arc<AppState>,
    incarnation: &WorkspaceIncarnationClaims,
    target_id: Uuid,
) -> Result<g5_authz::ScopedDb, AppError> {
    let mut db = agent_owner_db(state, incarnation, &["workspaces:read"]).await?;
    agent_read_target(state, &mut db, incarnation, target_id).await?;
    Ok(db)
}

/// Query of the approval activity feed. List parameters arrive comma-joined.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalActivityParams {
    since: Option<String>,
    until: Option<String>,
    workspaces: Option<String>,
    definitions: Option<String>,
    reviewer: Option<String>,
    requester: Option<String>,
    kinds: Option<String>,
    actor: Option<String>,
    outcomes: Option<String>,
    limit: Option<i64>,
    after: Option<String>,
    #[serde(default)]
    pending: bool,
    #[serde(default)]
    summary: bool,
}

fn split_list(value: Option<String>) -> Vec<String> {
    value
        .map(|raw| {
            raw.split(',')
                .map(|item| item.trim().to_string())
                .filter(|item| !item.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// The org-wide approval activity feed (`g5.governance.approval.activity-feed`):
/// request, decision and closure events over every workspace the owner can
/// read. No target workspace; the readable-workspace scope is applied inside the
/// query.
async fn read_approval_activity(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(path): Path<CallerPath>,
    Query(params): Query<ApprovalActivityParams>,
) -> Result<
    Json<ResponseEnvelope<crate::services::approval_activity::ApprovalActivityResponse>>,
    AppError,
> {
    require_own_workspace(path.id, &incarnation)?;
    let reader = crate::services::git::workspace_audience::AudienceReader::resolve(
        &state.system_pool,
        incarnation.org_id,
        incarnation.owner_user_id,
    )
    .await?;
    let mut db = agent_owner_db(&state, &incarnation, &["workspaces:read"]).await?;
    let query = crate::services::approval_activity::ActivityQuery {
        since: params.since,
        until: params.until,
        workspaces: split_list(params.workspaces),
        definitions: split_list(params.definitions),
        reviewer: params.reviewer,
        requester: params.requester,
        kinds: split_list(params.kinds),
        actor: params.actor,
        outcomes: split_list(params.outcomes),
        limit: params.limit,
        after: params.after,
        pending: params.pending,
        summary: params.summary,
    };
    let result = state
        .approval_activity_service
        .activity(&mut db, &state.system_pool, reader, &query)
        .await?;
    db.rollback().await?;
    Ok(Json(ResponseEnvelope::ok(result)))
}

/// Approval requirements, requests and history of the caller's workspace or
/// a child.
async fn read_approvals(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(path): Path<TargetPath>,
) -> Result<Json<ResponseEnvelope<super::workspace_approvals::WorkspaceApprovalsStateDTO>>, AppError>
{
    require_own_workspace(path.id, &incarnation)?;
    let target_id = path.target_id;
    let db = admit_approval_read(&state, &incarnation, target_id).await?;
    let approvals = super::workspace_approvals::approvals_state_with(
        &state,
        db,
        target_id,
        incarnation.owner_user_id,
    )
    .await?;
    Ok(Json(ResponseEnvelope::ok(approvals)))
}

/// A formal agent review's retained report: the reviewer's document with its
/// verdict, assignment, execution provenance and any failure, read as the
/// approvals page reads it. A review of another workspace is not found.
async fn read_review_report(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(path): Path<TargetReviewPath>,
) -> Result<Json<ResponseEnvelope<serde_json::Value>>, AppError> {
    require_own_workspace(path.id, &incarnation)?;
    let mut db = admit_approval_read(&state, &incarnation, path.target_id).await?;
    let report = state
        .approval_review_service
        .report(&mut db, path.target_id, path.review_id)
        .await?;
    db.commit().await?;
    Ok(Json(ResponseEnvelope::ok(report)))
}

/// A page of a formal agent review's retained transcript, target-side tool
/// output shown only when the owner can read the merge target.
async fn read_review_transcript(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(path): Path<TargetReviewPath>,
    Query(query): Query<ReviewTranscriptQuery>,
) -> Result<Json<ResponseEnvelope<ReviewTranscriptPage>>, AppError> {
    require_own_workspace(path.id, &incarnation)?;
    let mut db = admit_approval_read(&state, &incarnation, path.target_id).await?;
    let page = state
        .approval_review_service
        .transcript(
            &state,
            &mut db,
            path.target_id,
            path.review_id,
            incarnation.owner_user_id,
            query,
        )
        .await?;
    db.rollback().await?;
    Ok(Json(ResponseEnvelope::ok(page)))
}

/// A formal agent review's retained commits of one of its repositories:
/// paths, a file or the reviewed diff, target-side content only when the
/// owner can read the merge target.
async fn read_review_commits(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(path): Path<TargetReviewPath>,
    Query(query): Query<ReviewCommitQuery>,
) -> Result<Json<ResponseEnvelope<serde_json::Value>>, AppError> {
    require_own_workspace(path.id, &incarnation)?;
    let mut db = admit_approval_read(&state, &incarnation, path.target_id).await?;
    let content = state
        .approval_review_service
        .read_commits(
            &state,
            &mut db,
            path.target_id,
            path.review_id,
            incarnation.owner_user_id,
            query,
        )
        .await?;
    db.rollback().await?;
    Ok(Json(ResponseEnvelope::ok(content)))
}

/// Request human approvals for the caller's workspace or a child. The
/// request is not the decision (`g5.security.agent-human-exceptions`).
async fn request_human_approvals(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(path): Path<TargetPath>,
    Json(body): Json<super::workspace_approvals::CreateHumanApprovalRequestsDTO>,
) -> Result<
    (
        StatusCode,
        Json<ResponseEnvelope<super::workspace_approvals::HumanApprovalRequestsAdmissionDTO>>,
    ),
    AppError,
> {
    require_own_workspace(path.id, &incarnation)?;
    let target_id = path.target_id;
    let db = admit_approval_target(&state, &incarnation, target_id).await?;
    let response = super::workspace_approvals::create_human_requests_with(
        &state,
        db,
        target_id,
        incarnation.owner_user_id,
        body,
    )
    .await?;
    audit(
        &incarnation,
        "request_human_approvals",
        target_id,
        "requested",
    );
    Ok(response)
}

/// Request a formal agent review of the caller's workspace or a child (owner
/// decision 4, G5-Lean#710): the confined approver produces the verdict.
async fn request_agent_review(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(path): Path<TargetPath>,
    Json(body): Json<super::workspace_approvals::CreateApprovalReviewRequestDTO>,
) -> Result<
    (
        StatusCode,
        Json<ResponseEnvelope<super::workspace_approvals::ApprovalReviewAdmissionDTO>>,
    ),
    AppError,
> {
    require_own_workspace(path.id, &incarnation)?;
    let target_id = path.target_id;
    let db = admit_approval_target(&state, &incarnation, target_id).await?;
    let response = super::workspace_approvals::create_review_with(
        &state,
        db,
        target_id,
        incarnation.owner_user_id,
        body,
    )
    .await?;
    audit(&incarnation, "request_agent_review", target_id, "requested");
    Ok(response)
}

/// Cancel an approval request the owner made for the caller's workspace or a
/// child.
async fn cancel_approval_request(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(path): Path<TargetRequestPath>,
) -> Result<Json<ResponseEnvelope<super::workspace_approvals::WorkspaceApprovalsStateDTO>>, AppError>
{
    require_own_workspace(path.id, &incarnation)?;
    let target_id = path.target_id;
    let request_id = path.request_id;
    let db = admit_approval_target(&state, &incarnation, target_id).await?;
    let approvals = super::workspace_approvals::cancel_human_request_with(
        &state,
        db,
        target_id,
        incarnation.owner_user_id,
        request_id,
    )
    .await?;
    audit(
        &incarnation,
        "cancel_approval_request",
        target_id,
        "cancelled",
    );
    Ok(Json(ResponseEnvelope::ok(approvals)))
}

// ─── Increment 2: pull requests and issue state ─────────────────────────────

/// G5 pull requests of a readable workspace (`role` source, target or all;
/// `status` open, merged, closed or all).
async fn list_pull_requests(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(path): Path<TargetPath>,
    Query(query): Query<super::workspaces::WorkspacePullRequestListQueryDTO>,
) -> Result<
    Json<
        ResponseEnvelope<
            Vec<crate::services::coordinated_repo_publication::WorkspacePullRequestSummary>,
        >,
    >,
    AppError,
> {
    require_own_workspace(path.id, &incarnation)?;
    let target_id = path.target_id;
    let role = super::workspaces::parse_workspace_pull_request_role(query.role.as_deref())?;
    let status = super::workspaces::parse_workspace_pull_request_status(query.status.as_deref())?;
    let mut db = agent_owner_db(&state, &incarnation, &["workspaces:read"]).await?;
    agent_read_target(&state, &mut db, &incarnation, target_id).await?;
    let pull_requests = state
        .coordinated_repo_publication_service
        .list_workspace_pull_requests(&mut db, target_id, role, status)
        .await?;
    db.rollback().await.map_err(AppError::from)?;
    Ok(Json(ResponseEnvelope::ok(pull_requests)))
}

/// Close a G5 pull request into or out of the caller's workspace.
async fn close_pull_request(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(path): Path<PullRequestPath>,
) -> Result<
    Json<
        ResponseEnvelope<
            crate::services::coordinated_repo_publication::WorkspacePullRequestSummary,
        >,
    >,
    AppError,
> {
    require_own_workspace(path.id, &incarnation)?;
    let pull_request_id = path.pull_request_id;
    let closed = super::workspaces::close_pull_request_as(
        &state,
        &super::workspaces::PullRequestMerger::WorkspaceAgent(&incarnation),
        &super::workspaces::WorkspacePullRequestPathParams {
            id: incarnation.workspace_id,
            pull_request_id,
        },
    )
    .await?;
    audit(
        &incarnation,
        "close_pull_request",
        incarnation.workspace_id,
        "closed",
    );
    Ok(Json(ResponseEnvelope::ok(closed)))
}

/// Turn off merge when ready (`g5.git.pr-auto-merge`) on a pull request out
/// of the caller's workspace. No agent route turns it on: that is a person's
/// decision (`g5.security.agent-human-exceptions`).
async fn disable_auto_merge(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(path): Path<PullRequestPath>,
) -> Result<Json<ResponseEnvelope<super::workspaces::WorkspacePullRequestResponseDTO>>, AppError> {
    use crate::services::workspace_pull_request_auto_merge::Side;
    require_own_workspace(path.id, &incarnation)?;
    let mut db = crate::db::workspace_agent_scope::agent_owner_db(
        &state,
        &incarnation,
        &["workspaces:write"],
    )
    .await?;
    crate::services::workspace_mutation_policy::ensure_workspace_owner(
        &mut db,
        incarnation.workspace_id,
    )
    .await?;
    let changed = crate::services::workspace_pull_request_auto_merge::disable(
        &mut db,
        Side::Source(incarnation.workspace_id),
        path.pull_request_id,
        incarnation.owner_user_id,
        Some("Turned off by the source workspace's agent.".to_string()),
    )
    .await?
    .is_some();
    let response =
        super::workspaces::auto_merge_response(&state, db, path.pull_request_id, changed).await?;
    audit(
        &incarnation,
        "disable_auto_merge",
        incarnation.workspace_id,
        if changed { "disabled" } else { "already_off" },
    );
    Ok(response)
}

/// Recompute the stored merge evidence of a pull request into or out of the
/// caller's workspace.
async fn refresh_pull_request(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(path): Path<PullRequestPath>,
) -> Result<
    Json<
        ResponseEnvelope<
            crate::services::coordinated_repo_publication::WorkspacePullRequestSummary,
        >,
    >,
    AppError,
> {
    require_own_workspace(path.id, &incarnation)?;
    let pull_request_id = path.pull_request_id;
    let refreshed = super::workspaces::refresh_pull_request_as(
        &state,
        &super::workspaces::PullRequestMerger::WorkspaceAgent(&incarnation),
        &super::workspaces::WorkspacePullRequestPathParams {
            id: incarnation.workspace_id,
            pull_request_id,
        },
    )
    .await?;
    audit(
        &incarnation,
        "refresh_pull_request",
        incarnation.workspace_id,
        "refreshed",
    );
    Ok(Json(ResponseEnvelope::ok(refreshed)))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IssueStateDTO {
    /// The issue, named as `g5d issue view` names it (a link id, URL,
    /// `owner/name#N`, `KEY-N` or number); the primary issue when omitted.
    #[serde(default, rename = "ref")]
    reference: Option<String>,
    /// `closed` or `open`.
    state: String,
}

/// The issue `reference` names for a write from the caller's own workspace,
/// the organization's domains, and the owner's tenant scope admitting that
/// write. The issue is chosen between two scopes, so no transaction is held
/// while a tracker is read.
async fn agent_issue_write(
    state: &Arc<AppState>,
    incarnation: &WorkspaceIncarnationClaims,
    reference: Option<&str>,
) -> Result<
    (
        g5_authz::ScopedDb,
        Vec<crate::services::external_issues::IssueDomainAccess>,
        super::external_issues::CommentTarget,
    ),
    AppError,
> {
    use crate::services::external_issues::{
        ensure_external_issues_enabled, list_domain_access, list_workspace_links,
    };
    let workspace_id = incarnation.workspace_id;
    let mut db = agent_owner_db(state, incarnation, &["workspaces:write"]).await?;
    agent_target(
        &mut db,
        incarnation,
        workspace_id,
        AgentTargetNeed::SelfOrOther,
    )
    .await?;
    ensure_external_issues_enabled(&mut db, incarnation.org_id).await?;
    let links = list_workspace_links(&mut db, incarnation.org_id, workspace_id).await?;
    let domains = list_domain_access(&mut db, incarnation.org_id).await?;
    db.commit().await.map_err(AppError::from)?;
    let target =
        super::internal::agent_write_target(state, incarnation, &links, &domains, reference)
            .await?;
    let mut db = agent_owner_db(state, incarnation, &["workspaces:write"]).await?;
    agent_target(
        &mut db,
        incarnation,
        workspace_id,
        AgentTargetNeed::SelfOrOther,
    )
    .await?;
    Ok((db, domains, target))
}

/// Close or reopen an issue under the owner's connected GitHub account, or
/// as the owner on G5 issues (`g5.external-issues.transition`, owner
/// decision 5 of G5-Lean#710): the primary issue by default, or any issue
/// `g5d issue view` can read (G5-Lean#808).
async fn change_issue_state(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(path): Path<CallerPath>,
    Json(body): Json<IssueStateDTO>,
) -> Result<Json<ResponseEnvelope<super::external_issues::IssueStateChangeDTO>>, AppError> {
    use super::external_issues::{change_issue_state_as, IssueWriter};
    require_own_workspace(path.id, &incarnation)?;
    let workspace_id = incarnation.workspace_id;
    let (db, domains, target) =
        agent_issue_write(&state, &incarnation, body.reference.as_deref()).await?;
    let changed = change_issue_state_as(
        &state,
        db,
        &IssueWriter {
            org_id: incarnation.org_id,
            workspace_id: Some(workspace_id),
            actor: incarnation.owner_user_id,
            domains: &domains,
        },
        &target,
        &body.state,
    )
    .await?;
    audit(
        &incarnation,
        "change_issue_state",
        workspace_id,
        changed.outcome,
    );
    Ok(Json(ResponseEnvelope::ok(changed)))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IssueEditRequestDTO {
    /// The issue, as for [`IssueStateDTO`]; the primary issue when omitted.
    #[serde(default, rename = "ref")]
    reference: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    body: Option<String>,
    /// People to assign: GitHub logins, member emails on G5 issues, `@me`.
    #[serde(default)]
    add_assignees: Vec<String>,
    /// People to unassign, named the same way.
    #[serde(default)]
    remove_assignees: Vec<String>,
    /// The description `body` replaces: the edit is refused (`issue_changed`)
    /// when the issue no longer has it. G5 issues only (`g5d issue create
    /// --attach`).
    #[serde(default)]
    expected_body: Option<String>,
    /// Catalog labels to add (`g5.issue.labels.contract`).
    #[serde(default)]
    add_labels: Vec<String>,
    /// Labels to remove.
    #[serde(default)]
    remove_labels: Vec<String>,
}

/// Edit an issue's title, description, assignees or labels as the owner
/// (`g5.external-issues.edit`, G5-Lean#808; labels G5-Lean#1055): the
/// primary issue by default, or any issue `g5d issue view` can read. Every
/// check runs before anything is written: a G5 issue changes fields and
/// labels in one transaction; on GitHub every named label must exist, the
/// field edit (which checks before its one write) goes first, and the label
/// changes then report their own outcomes, so a label refusal after an
/// applied field edit makes the request `partial`.
async fn edit_issue(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(path): Path<CallerPath>,
    Json(body): Json<IssueEditRequestDTO>,
) -> Result<Json<ResponseEnvelope<super::external_issues::IssueEditDTO>>, AppError> {
    use super::external_issues::{edit_issue_as, IssueChange, IssueEditDTO, IssueWriter};
    use super::issue_labels::{
        change_issue_labels_as, edit_native_issue_with_labels, ensure_labels_exist,
    };
    require_own_workspace(path.id, &incarnation)?;
    let workspace_id = incarnation.workspace_id;
    let labels = crate::services::issue_labels::validate_change(
        &body.add_labels,
        &body.remove_labels,
        true,
    )?;
    let edits_fields = body.title.is_some()
        || body.body.is_some()
        || !body.add_assignees.is_empty()
        || !body.remove_assignees.is_empty();
    let (db, domains, target) =
        agent_issue_write(&state, &incarnation, body.reference.as_deref()).await?;
    let writer = IssueWriter {
        org_id: incarnation.org_id,
        workspace_id: Some(workspace_id),
        actor: incarnation.owner_user_id,
        domains: &domains,
    };
    let change = IssueChange {
        title: body.title.as_deref(),
        body: body.body.as_deref(),
        add: &body.add_assignees,
        remove: &body.remove_assignees,
        seen: None,
        expected_body: body.expected_body.as_deref(),
    };
    let native = target.repository_id == crate::services::native_issues::NATIVE_LOCATOR_REPOSITORY;
    let edited = if labels.is_empty() {
        edit_issue_as(&state, db, &writer, &target, &change).await?
    } else if !edits_fields {
        let labeled = change_issue_labels_as(&state, db, &writer, &target, &labels).await?;
        IssueEditDTO {
            outcome: labeled.outcome,
            issue: labeled.issue.clone(),
            github_login: labeled.github_login.clone(),
            not_assigned: Vec::new(),
            not_unassigned: Vec::new(),
            message: labeled.message.clone(),
            labels: Some(labeled),
        }
    } else if native {
        edit_native_issue_with_labels(&state, db, &writer, &target, &change, &labels).await?
    } else {
        ensure_labels_exist(&state, &writer, &target, &labels).await?;
        let mut edited = edit_issue_as(&state, db, &writer, &target, &change).await?;
        let mut db = agent_owner_db(&state, &incarnation, &["workspaces:write"]).await?;
        agent_target(
            &mut db,
            &incarnation,
            workspace_id,
            AgentTargetNeed::SelfOrOther,
        )
        .await?;
        match change_issue_labels_as(&state, db, &writer, &target, &labels).await {
            Ok(labeled) => {
                if edited.outcome == "applied" && labeled.outcome != "applied" {
                    edited.outcome = labeled.outcome;
                    edited.message = labeled.message.clone();
                }
                if labeled.issue.is_some() {
                    edited.issue = labeled.issue.clone();
                }
                edited.labels = Some(labeled);
            }
            Err(error) => {
                if edited.outcome == "applied" {
                    edited.outcome = "partial";
                }
                edited.message = Some(format!(
                    "the edit was made, but the label change was refused: {}",
                    error.inner
                ));
            }
        }
        edited
    };
    audit(&incarnation, "edit_issue", workspace_id, edited.outcome);
    Ok(Json(ResponseEnvelope::ok(edited)))
}

/// The issue domain a label catalog request names: its key (`ACME`) or
/// repository (`owner/name`), or the creation domain when omitted
/// (`g5.issue.labels.surfaces`).
fn label_domain<'a>(
    domains: &'a [crate::services::external_issues::IssueDomainAccess],
    domain: Option<&str>,
) -> Result<&'a crate::services::external_issues::IssueDomainAccess, AppError> {
    match domain.map(str::trim).filter(|domain| !domain.is_empty()) {
        Some(named) => domains
            .iter()
            .find(|access| access.domain.display_key.eq_ignore_ascii_case(named))
            .ok_or_else(|| {
                let known: Vec<&str> = domains
                    .iter()
                    .map(|access| access.domain.display_key.as_str())
                    .collect();
                AppError::from(G5Error::ValidationError {
                    details: serde_json::json!({
                        "field": "domain",
                        "code": "unknown_domain",
                        "error": format!(
                            "{named} is not one of the organization's issue domains ({})",
                            known.join(", ")
                        ),
                    }),
                })
            }),
        None => super::external_issues::creation_target(domains).ok_or_else(|| {
            AppError::from(G5Error::ValidationError {
                details: serde_json::json!({
                    "field": "domain",
                    "code": "domain_required",
                    "error": "the organization has no creation domain; name one with --domain",
                }),
            })
        }),
    }
}

/// The owner's scope for a label catalog request and the organization's
/// domains. Writes admit the caller's own workspace as issue writes do.
async fn agent_label_db(
    state: &Arc<AppState>,
    incarnation: &WorkspaceIncarnationClaims,
    write: bool,
) -> Result<
    (
        g5_authz::ScopedDb,
        Vec<crate::services::external_issues::IssueDomainAccess>,
    ),
    AppError,
> {
    use crate::services::external_issues::{ensure_external_issues_enabled, list_domain_access};
    let scopes: &[&str] = if write {
        &["workspaces:write"]
    } else {
        &["workspaces:read"]
    };
    let mut db = agent_owner_db(state, incarnation, scopes).await?;
    if write {
        agent_target(
            &mut db,
            incarnation,
            incarnation.workspace_id,
            AgentTargetNeed::SelfOrOther,
        )
        .await?;
    }
    ensure_external_issues_enabled(&mut db, incarnation.org_id).await?;
    let domains = list_domain_access(&mut db, incarnation.org_id).await?;
    Ok((db, domains))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueLabelListQuery {
    #[serde(default)]
    domain: Option<String>,
    #[serde(default)]
    page: Option<u32>,
}

/// One page of a domain's label catalog, read as the owner.
async fn list_issue_labels(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(path): Path<CallerPath>,
    Query(query): Query<IssueLabelListQuery>,
) -> Result<Json<ResponseEnvelope<crate::services::issue_labels::LabelPageDTO>>, AppError> {
    require_own_workspace(path.id, &incarnation)?;
    let (db, domains) = agent_label_db(&state, &incarnation, false).await?;
    let access = label_domain(&domains, query.domain.as_deref())?;
    let page =
        super::issue_labels::list_labels_as(&state, db, access, query.page.unwrap_or(1)).await?;
    Ok(Json(ResponseEnvelope::ok(page)))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IssueLabelCreateDTO {
    #[serde(default)]
    domain: Option<String>,
    name: String,
    #[serde(default)]
    color: Option<String>,
    #[serde(default)]
    description: Option<String>,
}

/// Add a label to a domain's catalog as the owner. A retry finds the label
/// the first attempt created.
async fn create_issue_label(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(path): Path<CallerPath>,
    Json(body): Json<IssueLabelCreateDTO>,
) -> Result<Json<ResponseEnvelope<crate::services::issue_labels::LabelWriteDTO>>, AppError> {
    require_own_workspace(path.id, &incarnation)?;
    let new = crate::services::issue_labels::validate_new_label(
        &body.name,
        body.color.as_deref(),
        body.description.as_deref(),
    )?;
    let (db, domains) = agent_label_db(&state, &incarnation, true).await?;
    let access = label_domain(&domains, body.domain.as_deref())?;
    let created = super::issue_labels::create_label_as(
        &state,
        db,
        &super::issue_labels::LabelWriter {
            org_id: incarnation.org_id,
            workspace_id: Some(incarnation.workspace_id),
            actor: incarnation.owner_user_id,
            access,
        },
        &new,
    )
    .await?;
    audit(
        &incarnation,
        "create_issue_label",
        incarnation.workspace_id,
        created.outcome,
    );
    Ok(Json(ResponseEnvelope::ok(created)))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IssueLabelEditDTO {
    #[serde(default)]
    domain: Option<String>,
    /// The label's current name.
    #[serde(default)]
    label: Option<String>,
    /// Or its id.
    #[serde(default)]
    label_id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    color: Option<String>,
    #[serde(default)]
    description: Option<String>,
}

/// Rename, recolour or redescribe a catalog label as the owner.
async fn edit_issue_label(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(path): Path<CallerPath>,
    Json(body): Json<IssueLabelEditDTO>,
) -> Result<Json<ResponseEnvelope<crate::services::issue_labels::LabelWriteDTO>>, AppError> {
    use crate::services::issue_labels::{validate_patch, LabelSelector};
    require_own_workspace(path.id, &incarnation)?;
    let patch = validate_patch(
        body.name.as_deref(),
        body.color.as_deref(),
        body.description.as_deref(),
    )?;
    let (db, domains) = agent_label_db(&state, &incarnation, true).await?;
    let access = label_domain(&domains, body.domain.as_deref())?;
    let edited = super::issue_labels::edit_label_as(
        &state,
        db,
        &super::issue_labels::LabelWriter {
            org_id: incarnation.org_id,
            workspace_id: Some(incarnation.workspace_id),
            actor: incarnation.owner_user_id,
            access,
        },
        &LabelSelector::from_request(body.label_id.as_deref(), body.label.as_deref())?,
        &patch,
    )
    .await?;
    audit(
        &incarnation,
        "edit_issue_label",
        incarnation.workspace_id,
        edited.outcome,
    );
    Ok(Json(ResponseEnvelope::ok(edited)))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IssueLabelDeleteDTO {
    #[serde(default)]
    domain: Option<String>,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    label_id: Option<String>,
}

/// Delete a catalog label as the owner; it leaves every issue too.
async fn delete_issue_label(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(path): Path<CallerPath>,
    Json(body): Json<IssueLabelDeleteDTO>,
) -> Result<Json<ResponseEnvelope<crate::services::issue_labels::LabelWriteDTO>>, AppError> {
    use crate::services::issue_labels::LabelSelector;
    require_own_workspace(path.id, &incarnation)?;
    let (db, domains) = agent_label_db(&state, &incarnation, true).await?;
    let access = label_domain(&domains, body.domain.as_deref())?;
    let deleted = super::issue_labels::delete_label_as(
        &state,
        db,
        &super::issue_labels::LabelWriter {
            org_id: incarnation.org_id,
            workspace_id: Some(incarnation.workspace_id),
            actor: incarnation.owner_user_id,
            access,
        },
        &LabelSelector::from_request(body.label_id.as_deref(), body.label.as_deref())?,
    )
    .await?;
    audit(
        &incarnation,
        "delete_issue_label",
        incarnation.workspace_id,
        deleted.outcome,
    );
    Ok(Json(ResponseEnvelope::ok(deleted)))
}

// ─── Managed preview ─────────────────────────────────────────────────────────
//
// The UI's managed preview for the caller's own workspace or a child: it is
// an application service, so the child-only chat rule does not apply. Each
// service checks the owner and the live runtime itself. Opening the preview
// in a browser needs a person's browser ticket and stays outside the agent's
// reach; the agent reaches its own preview on loopback.

#[derive(Debug, Deserialize)]
pub struct TargetRepoPath {
    id: Uuid,
    #[serde(rename = "targetId")]
    target_id: Uuid,
    #[serde(rename = "repoId")]
    repo_id: Uuid,
}

/// A scope as the owner with `target_id` admitted as the caller's own
/// workspace or a child: previews and file changes.
async fn admit_self_or_child(
    state: &Arc<AppState>,
    incarnation: &WorkspaceIncarnationClaims,
    target_id: Uuid,
    scopes: &[&str],
) -> Result<g5_authz::ScopedDb, AppError> {
    let mut db = agent_owner_db(state, incarnation, scopes).await?;
    // Reads of a child need no opt-in to change it
    // (`g5.security.agent-other-workspaces`).
    if scopes.contains(&"workspaces:write") {
        agent_target(
            &mut db,
            incarnation,
            target_id,
            AgentTargetNeed::SelfOrOther,
        )
        .await?;
    } else {
        agent_relation(
            &mut db,
            incarnation,
            target_id,
            AgentTargetNeed::SelfOrOther,
        )
        .await?;
    }
    Ok(db)
}

/// The current managed preview: status, port, readiness and close state.
async fn read_preview(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(path): Path<TargetPath>,
) -> Result<
    Json<ResponseEnvelope<Option<crate::services::task_lifecycle::WorkspacePreviewView>>>,
    AppError,
> {
    require_own_workspace(path.id, &incarnation)?;
    let target_id = path.target_id;
    let mut db = admit_self_or_child(&state, &incarnation, target_id, &["workspaces:read"]).await?;
    let preview = state
        .task_service
        .current_workspace_preview(&mut db, target_id, incarnation.owner_user_id)
        .await?;
    db.rollback().await.map_err(AppError::from)?;
    Ok(Json(ResponseEnvelope::ok(preview)))
}

/// Start the selected application as the managed preview. One preview runs
/// at a time (`g5.vm.preview-exclusive`); a replay of the same key returns it.
async fn start_preview(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(path): Path<TargetPath>,
    Json(body): Json<super::workspace_preview::StartPreviewRequest>,
) -> Result<Json<ResponseEnvelope<crate::services::task_lifecycle::WorkspacePreviewView>>, AppError>
{
    require_own_workspace(path.id, &incarnation)?;
    let target_id = path.target_id;
    validate_key("idempotencyKey", &body.idempotency_key)?;
    let mut db =
        admit_self_or_child(&state, &incarnation, target_id, &["workspaces:write"]).await?;
    let started = state
        .task_service
        .start_workspace_preview(
            &mut db,
            crate::services::task_lifecycle::StartWorkspacePreview {
                workspace_id: target_id,
                actor_user_id: incarnation.owner_user_id,
                repo_id: body.repo_id,
                profile: body.profile,
                idempotency_key: format!("agent:{}", body.idempotency_key),
                preview_host_suffix: state.preview_host_suffix.clone(),
                // Agents keep the running-VM requirement; a Start is the
                // owner's consent (`g5.lifecycle.start`).
                committed_application: None,
                start_intent_id: None,
            },
        )
        .await?;
    db.commit().await.map_err(AppError::from)?;
    audit(&incarnation, "start_preview", target_id, "started");
    Ok(Json(ResponseEnvelope::ok(started)))
}

/// A credential for HTTP requests to the target's ready managed preview
/// through the preview relay (`g5.preview.agent-access`).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PreviewCredentialDto {
    task_id: Uuid,
    /// The preview's isolated origin, ending in `/`.
    preview_url: String,
    /// The request header that carries `credential`.
    header: &'static str,
    credential: String,
    expires_at: chrono::DateTime<chrono::Utc>,
}

/// Mint a preview credential for the target's current ready preview. The
/// relay rechecks this caller, the target's scope and the task on every
/// request; the credential only names them.
async fn mint_preview_credential(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(path): Path<TargetPath>,
) -> Result<Json<ResponseEnvelope<PreviewCredentialDto>>, AppError> {
    require_own_workspace(path.id, &incarnation)?;
    let target_id = path.target_id;
    let mut db =
        admit_self_or_child(&state, &incarnation, target_id, &["workspaces:write"]).await?;
    let not_ready = || {
        AppError::from_error(G5Error::ServiceUnavailable {
            message: "PREVIEW_NOT_READY".to_string(),
        })
    };
    let task_id = state
        .task_service
        .current_workspace_preview(&mut db, target_id, incarnation.owner_user_id)
        .await?
        .ok_or_else(not_ready)?
        .task_id;
    state
        .task_service
        .workspace_preview_target(&mut db, task_id, true)
        .await?;
    db.rollback().await.map_err(AppError::from)?;
    let suffix = state
        .preview_host_suffix
        .as_deref()
        .ok_or_else(|| AppError::from_error(G5Error::PreviewOriginUnconfigured))?;
    let minter = state.jwt_minter.as_ref().ok_or_else(|| {
        AppError::from_error(G5Error::Internal {
            message: "JWT signing key not configured".to_string(),
        })
    })?;
    let credential = minter.mint_preview_agent(&incarnation, target_id, task_id)?;
    audit(&incarnation, "mint_preview_credential", target_id, "minted");
    Ok(Json(ResponseEnvelope::ok(PreviewCredentialDto {
        task_id,
        preview_url: format!("https://{task_id}.{suffix}/"),
        header: g5_auth::PREVIEW_AGENT_HEADER,
        credential,
        expires_at: chrono::Utc::now() + chrono::Duration::seconds(g5_auth::PREVIEW_AGENT_TTL_SECS),
    })))
}

/// Close the managed preview (a bounded graceful stop, `g5.vm.preview-stop`).
async fn close_preview(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(path): Path<TargetPath>,
) -> Result<
    Json<ResponseEnvelope<crate::services::task_lifecycle::WorkspaceTaskCancelResult>>,
    AppError,
> {
    require_own_workspace(path.id, &incarnation)?;
    let target_id = path.target_id;
    let db = admit_self_or_child(&state, &incarnation, target_id, &["workspaces:write"]).await?;
    let cancellation = super::workspace_preview::cancel_preview_with(
        &state,
        db,
        target_id,
        incarnation.owner_user_id,
    )
    .await?;
    audit(&incarnation, "close_preview", target_id, "closing");
    Ok(Json(ResponseEnvelope::ok(cancellation)))
}

/// The preview applications the live checkout declares for one repository.
async fn list_preview_applications(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(path): Path<TargetRepoPath>,
) -> Result<Json<ResponseEnvelope<Vec<super::preview_profiles::PreviewApplicationDTO>>>, AppError> {
    require_own_workspace(path.id, &incarnation)?;
    let target_id = path.target_id;
    let repo_id = path.repo_id;
    let mut db = admit_self_or_child(&state, &incarnation, target_id, &["workspaces:read"]).await?;
    let branch = state
        .workspace_service
        .get_workspace_repository(&mut db, target_id, repo_id)
        .await?;
    db.rollback().await.map_err(AppError::from)?;
    let applications = state
        .task_service
        .list_live_preview_applications(
            branch.workspace_id,
            incarnation.owner_user_id,
            branch.repo_id,
        )
        .await?;
    Ok(Json(ResponseEnvelope::ok(applications)))
}

/// The preview profiles committed on the repository's branch (`mode=merged`
/// lists the merge target's).
async fn list_preview_profiles(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(path): Path<TargetRepoPath>,
    Query(query): Query<super::preview_profiles::ListProfilesQuery>,
) -> Result<Json<ResponseEnvelope<Vec<super::preview_profiles::PreviewProfileDTO>>>, AppError> {
    require_own_workspace(path.id, &incarnation)?;
    let target_id = path.target_id;
    let repo_id = path.repo_id;
    let mut db = admit_self_or_child(&state, &incarnation, target_id, &["workspaces:read"]).await?;
    let branch = state
        .workspace_service
        .get_workspace_repository(&mut db, target_id, repo_id)
        .await?;
    let profiles = state
        .workspace_service
        .list_preview_profiles(&mut db, &branch, query.mode.as_deref() == Some("merged"))
        .await?;
    db.rollback().await.map_err(AppError::from)?;
    Ok(Json(ResponseEnvelope::ok(profiles)))
}

/// Save a preview profile into the live checkout, as the UI's commit does; a
/// later save of the workspace advances Git.
async fn save_preview_profile(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(path): Path<TargetRepoPath>,
    Json(body): Json<super::preview_profiles::CreatePreviewProfileBody>,
) -> Result<Json<ResponseEnvelope<super::preview_profiles::CommitPreviewProfileResult>>, AppError> {
    require_own_workspace(path.id, &incarnation)?;
    let target_id = path.target_id;
    let repo_id = path.repo_id;
    let db = admit_self_or_child(&state, &incarnation, target_id, &["workspaces:write"]).await?;
    let saved = super::preview_profiles::create_preview_profile_with(
        &state,
        db,
        target_id,
        repo_id,
        incarnation.owner_user_id,
        body,
    )
    .await?;
    audit(&incarnation, "save_preview_profile", target_id, "saved");
    Ok(Json(ResponseEnvelope::ok(saved)))
}

// ─── Files ───────────────────────────────────────────────────────────────────
//
// The UI's Files panel and editor. Listing and reading reach any workspace the
// owner can read (`g5.security.agent-read-scope`): the owner's running
// checkout live, anything else from the committed cut. Writes, deletions and
// renames reach the caller's own workspace and its children
// (`g5.security.agent-workspace-scope`) and need the target running, as in
// the editor; each is audited with its repository and path.

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileTreeQueryDTO {
    repo_id: Uuid,
    /// Subdirectory to list; the repository root when omitted.
    #[serde(default)]
    path: Option<String>,
    /// 1 lists immediate children (the default), -1 the whole subtree.
    #[serde(default)]
    depth: Option<i32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileQueryDTO {
    repo_id: Uuid,
    path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileWriteDTO {
    repo_id: Uuid,
    path: String,
    content: String,
    /// `utf-8` (the default) or `base64`.
    #[serde(default = "utf8")]
    encoding: String,
    /// The hash the file had when read; a changed file is refused.
    #[serde(default)]
    expected_hash: Option<String>,
    /// The VM the file was read from; a restarted workspace is refused.
    #[serde(default)]
    expected_vm_id: Option<Uuid>,
    /// Commit the file alone and push it, as the editor's Save does; needs
    /// `expected_vm_id`.
    #[serde(default)]
    commit_and_push: bool,
}

fn utf8() -> String {
    "utf-8".to_string()
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileDeleteDTO {
    repo_id: Uuid,
    path: String,
    #[serde(default)]
    expected_hash: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileRenameDTO {
    repo_id: Uuid,
    from: String,
    to: String,
    #[serde(default)]
    expected_hash: Option<String>,
}

/// A read scope as the owner on a workspace the owner can read.
async fn admit_file_read(
    state: &Arc<AppState>,
    incarnation: &WorkspaceIncarnationClaims,
    target_id: Uuid,
) -> Result<g5_authz::ScopedDb, AppError> {
    let mut db = agent_owner_db(state, incarnation, &["workspaces:read"]).await?;
    agent_read_target(state, &mut db, incarnation, target_id).await?;
    Ok(db)
}

/// List a repository's files in a readable workspace, as the Files panel does.
async fn list_files(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(path): Path<TargetPath>,
    Query(query): Query<FileTreeQueryDTO>,
) -> Result<Json<ResponseEnvelope<super::files::FileTreeResponse>>, AppError> {
    require_own_workspace(path.id, &incarnation)?;
    let target_id = path.target_id;
    let db = admit_file_read(&state, &incarnation, target_id).await?;
    let tree_path = query.path.unwrap_or_else(|| "/".to_string());
    let missing_root_is_empty = tree_path.trim_matches('/') == ".g5/ontology";
    super::files::read_file_tree_as(
        &state,
        db,
        incarnation.owner_user_id,
        super::files::BranchPathParams {
            workspace_id: target_id,
            repo_id: query.repo_id,
        },
        super::files::FileTreeQuery {
            path: tree_path,
            depth: query.depth.unwrap_or(1),
            task_id: None,
        },
        missing_root_is_empty,
    )
    .await
}

/// Read one file of a readable workspace, as the editor opens it.
async fn read_file(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(path): Path<TargetPath>,
    Query(query): Query<FileQueryDTO>,
) -> Result<Json<ResponseEnvelope<super::workspaces::WorkspaceFileContentDTO>>, AppError> {
    require_own_workspace(path.id, &incarnation)?;
    let target_id = path.target_id;
    let db = admit_file_read(&state, &incarnation, target_id).await?;
    let file = super::workspaces::read_workspace_file_as(
        &state,
        db,
        incarnation.owner_user_id,
        target_id,
        query.repo_id,
        &query.path,
        super::workspaces::MAX_WORKSPACE_FILE_BYTES,
    )
    .await?;
    Ok(Json(ResponseEnvelope::ok(file)))
}

/// Save a file into the running checkout of this workspace or a child, as the
/// editor's Save does (with `commitAndPush`, also committed and pushed).
async fn write_file(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(path): Path<TargetPath>,
    Json(body): Json<FileWriteDTO>,
) -> Result<Json<ResponseEnvelope<serde_json::Value>>, AppError> {
    require_own_workspace(path.id, &incarnation)?;
    let target_id = path.target_id;
    note_file("write_file", target_id, body.repo_id, &body.path, None);
    let db = admit_self_or_child(&state, &incarnation, target_id, &["workspaces:write"]).await?;
    let saved = super::workspaces::write_workspace_file_and_maybe_push_as(
        &state,
        db,
        incarnation.owner_user_id,
        target_id,
        body.repo_id,
        &body.path,
        &body.content,
        &body.encoding,
        body.expected_hash.as_deref(),
        body.expected_vm_id,
        body.commit_and_push,
    )
    .await?;
    audit(
        &incarnation,
        "write_file",
        target_id,
        if body.commit_and_push {
            "committed"
        } else {
            "written"
        },
    );
    Ok(Json(ResponseEnvelope::ok(saved)))
}

/// Delete a file or folder from the running checkout of this workspace or a
/// child.
async fn delete_file(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(path): Path<TargetPath>,
    Json(body): Json<FileDeleteDTO>,
) -> Result<Json<ResponseEnvelope<serde_json::Value>>, AppError> {
    require_own_workspace(path.id, &incarnation)?;
    let target_id = path.target_id;
    note_file("delete_file", target_id, body.repo_id, &body.path, None);
    let db = admit_self_or_child(&state, &incarnation, target_id, &["workspaces:write"]).await?;
    let deleted = super::workspaces::delete_workspace_file_as(
        &state,
        db,
        incarnation.owner_user_id,
        target_id,
        body.repo_id,
        &body.path,
        body.expected_hash.as_deref(),
    )
    .await?;
    audit(&incarnation, "delete_file", target_id, "deleted");
    Ok(Json(ResponseEnvelope::ok(deleted)))
}

/// Rename or move a file in the running checkout of this workspace or a child.
async fn rename_file(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(path): Path<TargetPath>,
    Json(body): Json<FileRenameDTO>,
) -> Result<Json<ResponseEnvelope<serde_json::Value>>, AppError> {
    require_own_workspace(path.id, &incarnation)?;
    let target_id = path.target_id;
    note_file(
        "rename_file",
        target_id,
        body.repo_id,
        &body.from,
        Some(&body.to),
    );
    let db = admit_self_or_child(&state, &incarnation, target_id, &["workspaces:write"]).await?;
    let renamed = super::workspaces::rename_workspace_file_as(
        &state,
        db,
        incarnation.owner_user_id,
        target_id,
        body.repo_id,
        &body.from,
        &body.to,
        body.expected_hash.as_deref(),
    )
    .await?;
    audit(&incarnation, "rename_file", target_id, "renamed");
    Ok(Json(ResponseEnvelope::ok(renamed)))
}
