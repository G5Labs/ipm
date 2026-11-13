//! Internal endpoints used by a live workspace VM and by a registered host tunnel.
//!
//! Workspace tools authenticate with a signed workspace-incarnation token and
//! revalidate the workspace owner, VM id, epoch, status, and lease in Postgres.
//! The host tunnel authenticates with a host JWT.

use std::sync::Arc;

use axum::{
    extract::ws::WebSocketUpgrade,
    extract::{DefaultBodyLimit, Extension, Path, Query, State},
    http::{header, HeaderName, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Json, Router,
};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db::scoped::scoped_db_for_system_org;
use crate::errors::{AppError, ErrorEnvelope};
use crate::middleware::{AuthenticatedHost, RequestId, WorkspaceIncarnationContext};
use crate::routes::{audited::AuditedRouter, ResponseEnvelope};
use crate::services::chat_service::MAX_CHAT_ATTACHMENT_BASE64_CHARS;
use crate::services::workspace_incarnation::{self, IncarnationLock};
use crate::services::workspace_mutation_policy::{
    ensure_workspace_allows_mutation, WorkspaceMutationKind,
};
use crate::services::workspace_reader::{resolve_committed_snapshot, RepoReadView};
use crate::services::AppState;
use g5_auth::WorkspaceIncarnationClaims;
use g5_authz::ScopedDb;
use g5_contracts::error::{ErrorResponse, G5Error};
use g5_contracts::types::AuthorityTarget;
use g5_internal_api::dtos::ChatAttachmentContentResponse;

// ─── Router ─────────────────────────────────────────────────────────────────

/// Returns an axum `Router` with all internal endpoints registered.
///
/// This router is nested under `/api/internal` by `build_router()` in `routes/mod.rs`.
/// Agent-facing endpoints require the current workspace-incarnation JWT.
/// The tunnel endpoint requires the matching live host JWT.
pub fn router() -> Router<Arc<AppState>> {
    AuditedRouter::new()
        .exempt_route(
            "/chat-sessions/:id/subscription-usage",
            post(report_subscription_usage).layer(DefaultBodyLimit::max(4096)),
            "Live-only display observation: current incarnation, session and subscription attempt verified; no durable write",
        )
        .exempt_route(
            "/chat-sessions/:id/codex-auth",
            post(update_codex_auth).layer(DefaultBodyLimit::max(4096)),
            "Owner-incarnation Codex device authorization gate",
        )
        .exempt_route(
            "/chat-sessions/:id/session-files",
            put(put_chat_session_file)
                .layer(DefaultBodyLimit::max(CHAT_SESSION_FILE_BODY_LIMIT_BYTES)),
            "Workspace-incarnation session artifact upload",
        )
        .exempt_route(
            "/chat-sessions/:id/session-files/manifest",
            get(chat_session_files_manifest),
            "Workspace-incarnation session artifact manifest",
        )
        .exempt_route(
            "/chat-sessions/:id/session-files/published",
            get(published_chat_session_file_bundle),
            "Workspace-incarnation verified session bundle",
        )
        .exempt_route(
            "/chat-sessions/:id/session-files/begin",
            post(begin_chat_session_file_bundle),
            "Workspace-incarnation session bundle fence",
        )
        .exempt_route(
            "/chat-sessions/:id/session-files/publish",
            post(publish_chat_session_file_bundle),
            "Workspace-incarnation complete session bundle publication",
        )
        .exempt_route(
            "/workspaces/:id/session-files/pending",
            get(pending_session_file_bundles),
            "Workspace-incarnation ended-attempt session bundle recovery",
        )
        .exempt_route(
            "/chat-sessions/:id/session-files/content",
            get(chat_session_file_content),
            "Workspace-incarnation session artifact read",
        )
        .exempt_route(
            "/chat-sessions/:id/attachments",
            post(upload_agent_chat_attachment)
                .layer(DefaultBodyLimit::max(CHAT_ATTACHMENT_UPLOAD_BODY_LIMIT_BYTES)),
            "Workspace-incarnation chat attachment upload",
        )
        .exempt_route(
            "/chat-attachments/:id/content",
            get(download_chat_attachment_content),
            "Workspace-incarnation chat attachment read",
        )
        .exempt_route(
            "/workspaces/:id/manifest",
            get(get_workspace_manifest_internal),
            "Workspace-incarnation manifest read",
        )
        .exempt_route(
            "/workspaces/:id/pull-requests",
            post(create_pull_request_internal),
            "Workspace-incarnation PR creation: shared operation revalidates current owner/VM/epoch/lease and source/target authority",
        )
        .exempt_route(
            "/workspaces/:id/pull-requests/info",
            get(get_pull_request_info_internal),
            "Workspace-incarnation read-only pull request information",
        )
        .exempt_route(
            "/workspaces/:id/pull-requests/:pullRequestId/description",
            get(get_pull_request_description_internal),
            "Workspace-incarnation read of one PR's canonical description, the same read as the landing's (the owner's agent, as the owner)",
        )
        .exempt_route(
            "/workspaces/:id/pull-requests/descriptions",
            get(get_pull_request_descriptions_internal),
            "Workspace-incarnation batch read of PRs' canonical descriptions for a listing's body, with no provider call (the owner's agent, as the owner)",
        )
        .exempt_route(
            "/workspaces/:id/pull-requests/targets",
            get(list_pull_request_targets_internal),
            "Workspace-incarnation PR target list for g5d pr create -B (the owner's agent, as the owner)",
        )
        .exempt_route(
            "/workspaces/:id/pull-requests/preview",
            post(preview_pull_request_internal),
            "Workspace-incarnation PR text preview for g5d pr create --dry-run; writes nothing",
        )
        .exempt_route(
            "/workspaces/:id/pull-requests/github",
            get(list_repository_pull_requests_internal),
            "Workspace-incarnation repository PR listing (the owner's agent, as the owner); searches spend the agents' search budget",
        )
        .exempt_route(
            "/workspaces/:id/pull-requests/github/attachments",
            get(list_pull_request_attachments_internal),
            "Workspace-incarnation listing of a readable GitHub PR's attachments (g5d pr attachments, the owner's agent, as the owner)",
        )
        .exempt_route(
            "/workspaces/:id/pull-requests/github/attachments/content",
            get(pull_request_attachment_content_internal),
            "Workspace-incarnation read of one listed GitHub PR attachment, streamed (g5d pr attachments)",
        )
        .exempt_route(
            "/workspaces/:id/pull-requests/github/comments",
            post(post_pull_request_comment_internal).layer(DefaultBodyLimit::max(256 * 1024)),
            "Workspace-incarnation GitHub PR comment or review-thread reply (the owner's agent, as the owner)",
        )
        .exempt_route(
            "/workspaces/:id/pull-requests/github/threads/resolve",
            post(resolve_review_thread_internal),
            "Workspace-incarnation GitHub review-thread resolution (the owner's agent, as the owner)",
        )
        .exempt_route(
            "/workspaces/:id/issue-links",
            get(list_issue_links_internal).post(link_issue_internal),
            "Workspace-incarnation linked issue list and link (the owner's agent, as the owner)",
        )
        .exempt_route(
            "/workspaces/:id/issues",
            post(create_issue_internal).layer(DefaultBodyLimit::max(256 * 1024)),
            "Workspace-incarnation issue creation in the organization's creation target (the owner's agent, as the owner)",
        )
        .exempt_route(
            "/workspaces/:id/source-control/save",
            post(save_source_control_internal),
            "Workspace-incarnation explicit Save changes; same operation as the UI checkpoint",
        )
        .exempt_route(
            "/workspaces/:id/owner-email",
            post(email_owner_internal).layer(DefaultBodyLimit::max(256 * 1024)),
            "Workspace-incarnation email to the workspace's current owner (the owner's agent emails its own owner)",
        )
        .exempt_route(
            "/workspaces/:id/issue-links/:link_id",
            get(get_linked_issue_internal),
            "Workspace-incarnation linked issue read (the owner's agent; g5d before issue #558)",
        )
        .exempt_route(
            "/workspaces/:id/issue-links/unlink",
            post(unlink_issue_internal),
            "Workspace-incarnation unlink of one of the workspace's issues (the owner's agent, as the owner)",
        )
        .exempt_route(
            "/workspaces/:id/issues/view",
            get(view_issue_internal),
            "Workspace-incarnation read of any issue in the organization's issue domains (the owner's agent, as the owner)",
        )
        .exempt_route(
            "/workspaces/:id/issues/attachments",
            get(list_issue_attachments_internal)
                .post(upload_issue_attachment_internal)
                .layer(DefaultBodyLimit::max(AGENT_UPLOAD_BODY_LIMIT_BYTES)),
            "Workspace-incarnation listing of the attachments of an issue the owner's agent can view (g5d issue attachments), and the agent's image upload to a G5 issue as the owner (--attach)",
        )
        .exempt_route(
            "/workspaces/:id/issues/attachments/content",
            get(issue_attachment_content_internal),
            "Workspace-incarnation read of one listed issue attachment through its adapter, streamed (g5d issue attachments)",
        )
        .exempt_route(
            "/workspaces/:id/issues/search",
            get(search_issues_internal),
            "Workspace-incarnation federated issue search within the agents' budget (the owner's agent, as the owner)",
        )
        .exempt_route(
            "/workspaces/:id/issue-comments",
            post(post_issue_comment_internal).layer(DefaultBodyLimit::max(256 * 1024)),
            "Workspace-incarnation comment on the primary issue (the owner's agent, as the owner)",
        )
        .exempt_route(
            "/workspaces/:id/questions",
            get(list_question_sets_internal).post(create_question_set_internal),
            "Workspace-incarnation question sets: the owner's agent reads and authors (live writes, agent provenance)",
        )
        .exempt_route(
            "/workspaces/:id/questions/answer",
            post(answer_question_internal),
            "Workspace-incarnation question answer: the owner's agent proposes (agent provenance, live write)",
        )
        .exempt_route(
            "/workspaces/:id/platform/issues",
            post(crate::routes::platform_issues::create_platform_issue_internal),
            "Workspace-incarnation platform issue intake",
        )
        .exempt_route(
            "/docs/:id/content",
            put(update_doc_content),
            "Workspace-incarnation document update",
        )
        .exempt_route(
            "/hosts/:hostId/tunnel",
            get(host_tunnel_attach),
            "Host control and stream tunnel",
        )
        .into_router()
        .layer(DefaultBodyLimit::max(INTERNAL_BODY_LIMIT_BYTES))
}

/// Body limit for the internal workspace→server router. Sized to accommodate
/// large bootstrap task-graph submissions (thousands of nodes) plus other
/// worker payloads; far above the 2 MiB axum default.
const INTERNAL_BODY_LIMIT_BYTES: usize = 128 * 1024 * 1024;

/// Bound buffering before JSON deserialization allocates the base64 String.
/// Allow the service's maximum attachment plus 64 KiB for JSON and metadata.
const CHAT_ATTACHMENT_UPLOAD_BODY_LIMIT_BYTES: usize = MAX_CHAT_ATTACHMENT_BASE64_CHARS + 64 * 1024;

/// `GET /api/internal/hosts/:hostId/tunnel` (WebSocket upgrade).
///
/// A workspace host dials this outbound to establish its persistent control and
/// data tunnel. g5-api accepts the upgrade, builds a mux over it, and registers
/// it in the per-host tunnel registry keyed by `host_id`. Workspace RPC, preview
/// HTTP/WebSocket and terminal PTY streams relay through this carrier.
///
/// Authenticated by the `AuthenticatedHost` extractor (host_jwt with
/// `Scope::WorkerHost`). The path `:hostId` must equal the authenticated host's
/// own id — a host may only attach its own tunnel, never another host's.
async fn host_tunnel_attach(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
    host: AuthenticatedHost,
    Path(host_id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    if host.host_id() != host_id {
        tracing::warn!(
            authed_host = %host.host_id(),
            path_host = %host_id,
            "Host tunnel attach rejected: path host_id does not match authenticated host"
        );
        return Err(AppError::from_error(
            g5_contracts::error::G5Error::Forbidden {
                missing_scopes: vec![format!("host:{host_id}")],
            },
        ));
    }

    let registry = state.preview_tunnels.clone();
    tracing::info!(%host_id, "Host preview reverse-tunnel attach requested");

    Ok(ws.on_upgrade(move |socket| async move {
        registry.attach(host_id, socket).await;
    }))
}

// ─── Provider session files (design v6 §8.4) ───────────────────────────────
//
// The provider runner uploads the files a CLI needs to resume its own session
// (Claude transcripts, Codex rollouts) after every turn and restores them
// before a resumed turn on a host that lacks them. Every route requires the
// worker that owns the session's running task; the upload also takes the
// worker-session generation fence, so a superseded worker process cannot
// overwrite what its successor harvested. File bytes travel zstd-compressed in
// both directions.

/// Request body limit of the session-file upload: the zstd frame of an
/// incompressible file at the cap is slightly larger than the file itself.
const CHAT_SESSION_FILE_BODY_LIMIT_BYTES: usize =
    g5_internal_api::session_files::CHAT_SESSION_FILE_MAX_BYTES as usize + 4 * 1024 * 1024;

#[derive(Debug, Deserialize)]
struct ChatSessionFilesManifestQuery {
    provider_session_id: String,
}

#[derive(Debug, Deserialize)]
struct ChatSessionFileContentQuery {
    provider_session_id: String,
    path: String,
}

#[derive(Debug, Deserialize)]
struct PutChatSessionFileQuery {
    provider: String,
    provider_session_id: String,
    path: String,
    sha256: String,
    generation: Option<Uuid>,
    base_size: Option<i64>,
    base_sha256: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PublishedChatSessionFileQuery {
    provider: String,
    provider_session_id: String,
}

async fn published_chat_session_file_bundle(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(chat_session_id): Path<Uuid>,
    Query(query): Query<PublishedChatSessionFileQuery>,
) -> Result<Json<Option<g5_internal_api::dtos::SessionFileBundlePublication>>, AppError> {
    let provider = g5_internal_api::session_files::SessionFileProvider::parse(&query.provider)
        .ok_or_else(|| {
            AppError::from(g5_contracts::error::G5Error::ValidationError {
                details: serde_json::json!({"field":"provider","message":"unknown provider"}),
            })
        })?;
    let mut db = scoped_db_for_session_file_incarnation(
        &state,
        chat_session_id,
        &incarnation,
        &["sessions:read"],
    )
    .await?;
    let bundle = state
        .chat_session_file_service
        .published_bundle(
            db.conn(),
            chat_session_id,
            provider,
            &query.provider_session_id,
        )
        .await?;
    db.commit().await?;
    Ok(Json(bundle))
}

async fn begin_chat_session_file_bundle(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(chat_session_id): Path<Uuid>,
    Json(body): Json<g5_internal_api::dtos::SessionFileBundleBegin>,
) -> Result<Json<serde_json::Value>, AppError> {
    let mut db = scoped_db_for_session_file_incarnation(
        &state,
        chat_session_id,
        &incarnation,
        &["sessions:write"],
    )
    .await?;
    state
        .chat_session_file_service
        .begin_bundle(
            db.conn(),
            chat_session_id,
            &body,
            g5_api_core::services::chat_session_file_service::SessionFileIncarnation {
                vm_id: incarnation.vm_id,
                epoch: incarnation.epoch,
            },
        )
        .await?;
    db.commit().await?;
    Ok(Json(serde_json::json!({})))
}

#[derive(Debug, Deserialize)]
struct PendingSessionFileBundlesQuery {
    session_id: Option<Uuid>,
}

async fn pending_session_file_bundles(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(workspace_id): Path<Uuid>,
    Query(query): Query<PendingSessionFileBundlesQuery>,
) -> Result<Json<Vec<g5_internal_api::dtos::PendingSessionFileBundle>>, AppError> {
    let mut db =
        scoped_db_for_workspace_incarnation(&state, workspace_id, &incarnation, &["sessions:read"])
            .await?;
    let bundles = state
        .chat_session_file_service
        .pending_bundles(
            db.conn(),
            workspace_id,
            g5_api_core::services::chat_session_file_service::SessionFileIncarnation {
                vm_id: incarnation.vm_id,
                epoch: incarnation.epoch,
            },
            query.session_id,
        )
        .await?;
    db.commit().await?;
    Ok(Json(bundles))
}

async fn publish_chat_session_file_bundle(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(chat_session_id): Path<Uuid>,
    Json(body): Json<g5_internal_api::dtos::SessionFileBundlePublication>,
) -> Result<Json<serde_json::Value>, AppError> {
    let mut db = scoped_db_for_session_file_incarnation(
        &state,
        chat_session_id,
        &incarnation,
        &["sessions:write"],
    )
    .await?;
    state
        .chat_session_file_service
        .publish_bundle(
            db.conn(),
            chat_session_id,
            &body,
            g5_api_core::services::chat_session_file_service::SessionFileIncarnation {
                vm_id: incarnation.vm_id,
                epoch: incarnation.epoch,
            },
        )
        .await?;
    db.commit().await?;
    Ok(Json(serde_json::json!({})))
}

/// `GET /api/internal/chat-sessions/:id/session-files/manifest?provider_session_id=`
async fn chat_session_files_manifest(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(chat_session_id): Path<Uuid>,
    Query(query): Query<ChatSessionFilesManifestQuery>,
) -> Result<Json<g5_internal_api::dtos::ChatSessionFilesManifestResponse>, AppError> {
    let mut db = scoped_db_for_session_file_incarnation(
        &state,
        chat_session_id,
        &incarnation,
        &["sessions:read"],
    )
    .await?;
    let files = state
        .chat_session_file_service
        .manifest(db.conn(), chat_session_id, &query.provider_session_id)
        .await?;
    db.commit().await?;
    Ok(Json(
        g5_internal_api::dtos::ChatSessionFilesManifestResponse { files },
    ))
}

/// `GET /api/internal/chat-sessions/:id/session-files/content?provider_session_id=&path=`
///
/// The body is the zstd-compressed file; the headers carry its provider,
/// sha256 and decompressed size, which the worker checks before restoring it.
async fn chat_session_file_content(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(chat_session_id): Path<Uuid>,
    Query(query): Query<ChatSessionFileContentQuery>,
) -> Result<Response, AppError> {
    use g5_internal_api::session_files::{
        encode_session_file_body, SESSION_FILE_CONTENT_TYPE, SESSION_FILE_PROVIDER_HEADER,
        SESSION_FILE_SHA256_HEADER, SESSION_FILE_SIZE_HEADER,
    };

    let mut db = scoped_db_for_session_file_incarnation(
        &state,
        chat_session_id,
        &incarnation,
        &["sessions:read"],
    )
    .await?;
    let file = state
        .chat_session_file_service
        .load(
            db.conn(),
            chat_session_id,
            &query.provider_session_id,
            &query.path,
        )
        .await?;
    db.commit().await?;
    let size = file.bytes.len();
    let body = encode_session_file_body(&file.bytes).map_err(|error| {
        AppError::from(G5Error::Internal {
            message: format!("failed to compress a chat session file: {error}"),
        })
    })?;
    Ok((
        [
            (header::CONTENT_TYPE, SESSION_FILE_CONTENT_TYPE.to_string()),
            (
                HeaderName::from_static(SESSION_FILE_PROVIDER_HEADER),
                file.provider.as_str().to_string(),
            ),
            (
                HeaderName::from_static(SESSION_FILE_SHA256_HEADER),
                file.sha256,
            ),
            (
                HeaderName::from_static(SESSION_FILE_SIZE_HEADER),
                size.to_string(),
            ),
        ],
        body,
    )
        .into_response())
}

/// `PUT /api/internal/chat-sessions/:id/session-files?provider=&provider_session_id=&path=&sha256=`
///
/// Upserts one file from a zstd-compressed body. A body that decompresses
/// past the 128 MiB cap is refused with 413; re-sending the stored bytes is a
/// no-op reported as `changed: false`.
async fn put_chat_session_file(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    request_id: Option<Extension<RequestId>>,
    Path(chat_session_id): Path<Uuid>,
    Query(query): Query<PutChatSessionFileQuery>,
    body: axum::body::Bytes,
) -> Result<Response, AppError> {
    use crate::services::chat_session_file_service::{validation_error, ChatSessionFileUpload};
    use g5_internal_api::session_files::{
        decode_session_file_body, SessionFileError, SessionFileProvider,
        CHAT_SESSION_FILE_MAX_BYTES,
    };

    let provider = SessionFileProvider::parse(&query.provider).ok_or_else(|| {
        AppError::from(G5Error::ValidationError {
            details: serde_json::json!({
                "code": "session_file_provider_invalid",
                "field": "provider",
                "message": "provider must be claude_code or codex",
            }),
        })
    })?;
    // Decompress before taking the session locks; the decode stops one byte
    // past the cap, so a small body cannot expand without bound.
    let bytes = match decode_session_file_body(&body) {
        Ok(bytes) => bytes,
        Err(SessionFileError::TooLarge) => {
            return Ok((
                StatusCode::PAYLOAD_TOO_LARGE,
                Json(ErrorEnvelope {
                    error: ErrorResponse {
                        code: "SESSION_FILE_TOO_LARGE".to_string(),
                        message: SessionFileError::TooLarge.to_string(),
                        details: serde_json::json!({
                            "field": "body",
                            "limitBytes": CHAT_SESSION_FILE_MAX_BYTES,
                        }),
                        request_id: request_id
                            .map(|Extension(request_id)| request_id.0)
                            .unwrap_or_default(),
                    },
                }),
            )
                .into_response());
        }
        Err(error) => return Err(validation_error(&error, "body")),
    };
    let upload = ChatSessionFileUpload {
        provider,
        provider_session_id: &query.provider_session_id,
        relative_path: &query.path,
        sha256: &query.sha256,
        bytes: &bytes,
        base: match (query.base_size, query.base_sha256.as_deref()) {
            (None, None) => None,
            (Some(size), Some(sha256)) if query.generation.is_some() => Some((size, sha256)),
            _ => {
                return Err(AppError::from(G5Error::ValidationError {
                    details: serde_json::json!({"field":"base","message":"append requires a generation, base size and base sha256"}),
                }))
            }
        },
    };
    let mut db = scoped_db_for_session_file_incarnation(
        &state,
        chat_session_id,
        &incarnation,
        &["sessions:write"],
    )
    .await?;
    let response = state
        .chat_session_file_service
        .upsert(
            db.conn(),
            chat_session_id,
            upload,
            Some(
                crate::services::chat_session_file_service::SessionFileIncarnation {
                    vm_id: incarnation.vm_id,
                    epoch: incarnation.epoch,
                },
            ),
            query.generation,
        )
        .await?;
    db.commit().await?;
    Ok(Json(response).into_response())
}

async fn update_codex_auth(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(session_id): Path<Uuid>,
    Json(body): Json<g5_internal_api::codex_auth::CodexAuthUpdate>,
) -> Result<Json<g5_internal_api::codex_auth::CodexAuthDisposition>, AppError> {
    let mut db = scoped_db_for_session_file_incarnation(
        &state,
        session_id,
        &incarnation,
        &["workspaces:write"],
    )
    .await?;
    let result = state
        .task_service
        .update_codex_auth(
            &mut db,
            session_id,
            incarnation.owner_user_id,
            incarnation.vm_id,
            incarnation.epoch,
            body,
        )
        .await?;
    db.commit().await.map_err(AppError::from)?;
    Ok(Json(result))
}

async fn report_subscription_usage(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(session_id): Path<Uuid>,
    Json(report): Json<g5_internal_api::subscription_usage::SubscriptionUsageReport>,
) -> Result<Json<serde_json::Value>, AppError> {
    let mut db = scoped_db_for_session_file_incarnation(
        &state,
        session_id,
        &incarnation,
        &["sessions:write"],
    )
    .await?;
    let event = state
        .task_service
        .subscription_usage_event(
            &mut db,
            session_id,
            incarnation.vm_id,
            incarnation.epoch,
            report,
        )
        .await?;
    let accepted = event.is_some();
    // Admit publication under the same incarnation/attempt locks. Browser
    // delivery independently enforces the chat's current reader authority.
    if let Some(event) = event {
        state
            .event_bus
            .publish(&crate::ws::Channel::Chat(session_id), &event);
    }
    db.commit().await?;
    Ok(Json(serde_json::json!({"accepted": accepted})))
}

/// A tenant-scoped connection for the incarnation's org on which the live
/// incarnation and the session's workspace were verified. The raw system
/// pool carries no org context, so under forced row security the verifier
/// saw no `workspaces` row (and lacked the lock privilege): every native
/// session-file persist and manifest read from a workspace VM failed, and
/// each agent turn showed "API error 500" (QA, 2026-09-23). CI never saw it
/// because its API connects as the owning role.
async fn scoped_db_for_session_file_incarnation(
    state: &Arc<AppState>,
    chat_session_id: Uuid,
    claims: &WorkspaceIncarnationClaims,
    scopes: &[&str],
) -> Result<ScopedDb, AppError> {
    let mut db =
        scoped_db_for_workspace_incarnation(state, claims.workspace_id, claims, scopes).await?;
    require_session_in_workspace(db.conn(), chat_session_id, claims).await?;
    Ok(db)
}

async fn require_session_in_workspace(
    conn: &mut sqlx::PgConnection,
    chat_session_id: Uuid,
    claims: &WorkspaceIncarnationClaims,
) -> Result<(), AppError> {
    let session_workspace: Option<Uuid> =
        sqlx::query_scalar("SELECT workspace_id FROM chat_sessions WHERE id = $1")
            .bind(chat_session_id)
            .fetch_optional(&mut *conn)
            .await?;
    if session_workspace != Some(claims.workspace_id) {
        return Err(AppError::from(G5Error::Forbidden {
            missing_scopes: Vec::new(),
        }));
    }
    Ok(())
}

async fn require_live_workspace_incarnation(
    db: &mut ScopedDb,
    workspace_id: Uuid,
    claims: &WorkspaceIncarnationClaims,
) -> Result<(), AppError> {
    let current = workspace_id == claims.workspace_id
        && workspace_incarnation::current_incarnation(db, claims, IncarnationLock::Share)
            .await?
            .is_some();
    if !current {
        return Err(AppError::from(G5Error::Forbidden {
            missing_scopes: Vec::new(),
        }));
    }
    Ok(())
}

async fn scoped_db_for_workspace_incarnation(
    state: &Arc<AppState>,
    workspace_id: Uuid,
    claims: &WorkspaceIncarnationClaims,
    scopes: &[&str],
) -> Result<ScopedDb, AppError> {
    // This tag is derived only from the authenticated incarnation, never a
    // route or payload id. Ordinary workspace policies ignore it.
    let binding = g5_contracts::types::formal_review_runtime_scope(claims.workspace_id);
    let mut bound_scopes = scopes.to_vec();
    bound_scopes.push(&binding);
    let scopes = bound_scopes.as_slice();
    let mut db = scoped_db_for_system_org(state, claims.org_id, scopes).await?;
    require_live_workspace_incarnation(&mut db, workspace_id, claims).await?;
    Ok(db)
}

/// `POST /api/internal/chat-sessions/:id/attachments` (routes/internal D35)
///
/// Worker-owned agent-screenshot upload: only the worker currently running
/// the session's task may write, under the session-generation lock — the same
/// fence as the session-file upload. Persistence contract: chat_service D63
/// (`created_by` NULL, assistant-message binding, `agent-screenshots/`
/// storage-key prefix, re-feed exclusion).
async fn upload_agent_chat_attachment(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(chat_session_id): Path<Uuid>,
    Json(body): Json<g5_internal_api::dtos::UploadAgentChatAttachmentRequest>,
) -> Result<Json<g5_internal_api::dtos::UploadAgentChatAttachmentResponse>, AppError> {
    let mut db = scoped_db_for_session_file_incarnation(
        &state,
        chat_session_id,
        &incarnation,
        &["sessions:write"],
    )
    .await?;
    let message_id = Uuid::parse_str(&body.message_id).map_err(|_| {
        AppError::from_error(g5_contracts::error::G5Error::ValidationError {
            details: serde_json::json!({
                "field": "messageId",
                "message": "messageId must be a UUID",
            }),
        })
    })?;
    let attachment = state
        .chat_service
        .create_agent_attachment(
            &mut db,
            chat_session_id,
            message_id,
            &body.file_name,
            body.content_type.clone(),
            &body.content_base64,
        )
        .await?;
    db.commit().await.map_err(AppError::from)?;
    Ok(Json(
        g5_internal_api::dtos::UploadAgentChatAttachmentResponse {
            attachment_id: attachment.id.to_string(),
            sha256: attachment.sha256,
            size_bytes: attachment.size_bytes,
        },
    ))
}

/// `GET /api/internal/chat-attachments/:id/content`
///
/// Returns the bounded stored bytes for a chat attachment to the worker that
/// currently owns the running task for the attachment's chat session.
async fn download_chat_attachment_content(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(attachment_id): Path<Uuid>,
) -> Result<Json<ChatAttachmentContentResponse>, AppError> {
    #[derive(sqlx::FromRow)]
    struct AttachmentContentRow {
        session_id: Uuid,
        input_id: Option<Uuid>,
        file_name: String,
        content_type: Option<String>,
        size_bytes: i64,
        sha256: String,
    }

    // The live incarnation (owner, VM, epoch, status, lease) is verified on
    // the same scoped connection before any attachment row is read; the
    // session's workspace is checked once the row names it.
    let mut db = scoped_db_for_workspace_incarnation(
        &state,
        incarnation.workspace_id,
        &incarnation,
        &["workspaces:read"],
    )
    .await?;
    let row = sqlx::query_as::<_, AttachmentContentRow>(
        r#"
        SELECT session_id, input_id, file_name, content_type, size_bytes, sha256
        FROM chat_attachments
        WHERE id = $1
          AND org_id = $2
        "#,
    )
    .bind(attachment_id)
    .bind(incarnation.org_id)
    .fetch_optional(db.conn())
    .await
    .map_err(AppError::from)?
    .ok_or_else(|| {
        AppError::from(g5_contracts::error::G5Error::NotFound {
            resource: format!("chat attachment {attachment_id}"),
        })
    })?;

    require_session_in_workspace(db.conn(), row.session_id, &incarnation).await?;
    let content_bytes = sqlx::query_scalar::<_, Vec<u8>>(
        r#"
        SELECT content_bytes
        FROM chat_attachments
        WHERE id = $1
          AND org_id = $2
        "#,
    )
    .bind(attachment_id)
    .bind(incarnation.org_id)
    .fetch_optional(db.conn())
    .await
    .map_err(AppError::from)?
    .ok_or_else(|| {
        AppError::from(g5_contracts::error::G5Error::NotFound {
            resource: format!("chat attachment {attachment_id}"),
        })
    })?;
    db.commit().await.map_err(AppError::from)?;

    Ok(Json(ChatAttachmentContentResponse {
        attachment_id: attachment_id.to_string(),
        session_id: row.session_id.to_string(),
        input_id: row.input_id.map(|id| id.to_string()),
        file_name: row.file_name,
        content_type: row.content_type,
        size_bytes: row.size_bytes,
        sha256: row.sha256,
        content_base64: BASE64_STANDARD.encode(content_bytes),
    }))
}

// ─── Workspace-scoped Internal Endpoints ────────────────────────────────────

/// `GET /api/internal/workspaces/:id/manifest`
///
/// Returns the materialization manifest for the workspace currently leased to
/// the worker. This is the worker-safe counterpart to the public
/// `/api/v1/workspaces/:id/manifest` endpoint used by `g5d refresh`.
async fn get_workspace_manifest_internal(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(workspace_id): Path<Uuid>,
) -> Result<
    Json<crate::routes::ResponseEnvelope<crate::services::workspace_service::WorkspaceManifest>>,
    AppError,
> {
    let db = scoped_db_for_workspace_incarnation(
        &state,
        workspace_id,
        &incarnation,
        &["workspaces:read"],
    )
    .await?;

    tracing::debug!(
        %workspace_id,
        "Internal endpoint called: get_workspace_manifest_internal"
    );

    let manifest = state
        .workspace_service
        .get_workspace_manifest(
            db,
            workspace_id,
            &state.external_url,
            &state.git_external_url,
        )
        .await?;

    // The committed-cut capture releases the admission transaction (and its
    // incarnation row locks) before fencing. Replacement mints a new vm_id and
    // epoch, so rechecking after capture refuses a manifest to an incarnation
    // replaced or deleted while the cut was read.
    let recheck = scoped_db_for_workspace_incarnation(
        &state,
        workspace_id,
        &incarnation,
        &["workspaces:read"],
    )
    .await?;
    recheck.rollback().await.map_err(AppError::from)?;

    Ok(Json(crate::routes::ResponseEnvelope::ok(manifest)))
}

/// Resolve the acting human from the signed workspace incarnation for platform
/// issue intake. The token is minted for the workspace owner and the live
/// VM/epoch fence is revalidated before the report is accepted.
pub(crate) async fn resolve_internal_issue_actor(
    state: &Arc<AppState>,
    claims: &WorkspaceIncarnationClaims,
    workspace_id: Uuid,
    _action: &str,
) -> Result<(ScopedDb, Uuid, Uuid), AppError> {
    let db = scoped_db_for_workspace_incarnation(
        state,
        workspace_id,
        claims,
        &["workspaces:read", "issues:write"],
    )
    .await?;
    Ok((db, claims.org_id, claims.owner_user_id))
}

/// Compute a hex-encoded SHA-256 hash.
fn sha256_hex(data: &[u8]) -> String {
    use std::fmt::Write;
    let digest = <sha2::Sha256 as sha2::Digest>::digest(data);
    let mut hex = String::with_capacity(64);
    for byte in digest {
        write!(hex, "{:02x}", byte).unwrap();
    }
    hex
}

// ─── Internal Staleness Handler ─────────────────────────────────────────────

// ─── Internal Doc Content Handler ───────────────────────────────────────────

/// `PUT /api/internal/docs/:id/content`
///
/// Refreshes the indexed doc projection from canonical repo-backed content.
/// Called by workers after completing a view-mode task. The payload content is
/// treated as a verification hint only: Postgres is updated only when the
/// current docs-branch file already matches the provided content. This avoids
/// reintroducing `docs.content` as a shadow source of truth when a task branch
/// fails to merge.
async fn update_doc_content(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(doc_id): Path<Uuid>,
    Json(body): Json<g5_internal_api::dtos::UpdateDocContentRequest>,
) -> Result<StatusCode, AppError> {
    tracing::debug!(
        %doc_id,
        content_len = body.content.len(),
        "Internal endpoint called: update_doc_content"
    );

    let binding = g5_contracts::types::formal_review_runtime_scope(incarnation.workspace_id);
    let mut db =
        scoped_db_for_system_org(&state, incarnation.org_id, &["docs:write", &binding]).await?;
    let (workspace_id, doc_file_path, docs_repo_id) =
        sqlx::query_as::<_, (Uuid, Option<String>, Option<Uuid>)>(
            r#"
        SELECT d.workspace_id, d.file_path, w.docs_repo_id
        FROM docs d
        JOIN workspaces w ON w.id = d.workspace_id
        WHERE d.id = $1
        "#,
        )
        .bind(doc_id)
        .fetch_optional(db.conn())
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| {
            AppError::from_error(g5_contracts::error::G5Error::NotFound {
                resource: format!("doc/{doc_id}"),
            })
        })?;
    require_live_workspace_incarnation(&mut db, workspace_id, &incarnation).await?;

    // Read-only workspace guard: the agent doc-projection refresh mutates
    // workspace-owned content; incarnation authority is not a blanket bypass.
    ensure_workspace_allows_mutation(
        &mut db,
        workspace_id,
        WorkspaceMutationKind::DirectAgentWrite,
        "internal.doc_content",
    )
    .await?;

    let Some(doc_file_path) = doc_file_path else {
        tracing::warn!(%doc_id, "Skipping doc projection refresh for doc without file_path");
        return Ok(StatusCode::NO_CONTENT);
    };

    let Some(docs_repo_id) = docs_repo_id else {
        tracing::warn!(%doc_id, "Skipping doc projection refresh because no docs repository is configured");
        return Ok(StatusCode::NO_CONTENT);
    };
    let snapshot = resolve_committed_snapshot(
        &mut db,
        &state.repo_storage,
        AuthorityTarget::Workspace { workspace_id },
    )
    .await
    .map_err(AppError::from_error)?;
    let docs_view = RepoReadView::from_committed_snapshot(&db, &snapshot, docs_repo_id)
        .map_err(AppError::from_error)?;
    let canonical_content = match docs_view
        .read_committed_file(&state.repo_storage, &doc_file_path)
        .await
    {
        Ok(bytes) => String::from_utf8(bytes).ok(),
        Err(error) => {
            tracing::warn!(
                %doc_id,
                repo_id = %docs_repo_id,
                file_path = %doc_file_path,
                head_commit = %docs_view.repo.head_commit,
                error = %error,
                "Committed doc read failed during projection refresh"
            );
            None
        }
    };

    if canonical_content.as_deref() != Some(body.content.as_str()) {
        tracing::warn!(
            %doc_id,
            file_path = %doc_file_path,
            "Skipping doc projection refresh because canonical repo content differs from worker payload"
        );
        return Ok(StatusCode::NO_CONTENT);
    }

    let content_hash = sha256_hex(body.content.as_bytes());
    let linked_doc_paths = extract_linked_doc_paths_from_content(&body.content);

    sqlx::query(
        r#"
        UPDATE docs
        SET content = NULL,
            content_hash = $1,
            linked_doc_paths = $2,
            content_updated_at = now(),
            updated_at = now()
        WHERE id = $3
        "#,
    )
    .bind(content_hash)
    .bind(linked_doc_paths)
    .bind(doc_id)
    .execute(db.conn())
    .await
    .map_err(AppError::from)?;

    db.commit().await.map_err(AppError::from)?;

    tracing::info!(%doc_id, "Doc content updated via internal endpoint");

    // Notify connected clients so open doc tabs refresh
    state.event_bus.publish(
        &crate::ws::Channel::Workspace(workspace_id),
        &crate::ws::WsEvent::DocContentChanged {
            workspace_id,
            doc_id: doc_id.to_string(),
        },
    );

    Ok(StatusCode::NO_CONTENT)
}

fn extract_frontmatter_string_list(mapping: &serde_yaml::Mapping, keys: &[&str]) -> Vec<String> {
    for key_name in keys {
        let key = serde_yaml::Value::String((*key_name).to_string());
        if let Some(seq) = mapping.get(&key).and_then(|value| value.as_sequence()) {
            return seq
                .iter()
                .filter_map(|item| item.as_str().map(ToString::to_string))
                .collect();
        }
    }

    Vec::new()
}

fn extract_linked_doc_paths_from_content(content: &str) -> Option<serde_json::Value> {
    use crate::services::doc_reference_service::parse_yaml_frontmatter;

    let frontmatter = parse_yaml_frontmatter(content)?;
    let mapping = frontmatter.as_mapping()?;
    let linked_doc_paths = extract_frontmatter_string_list(mapping, &["linked_doc_paths"]);
    if linked_doc_paths.is_empty() {
        None
    } else {
        serde_json::to_value(linked_doc_paths).ok()
    }
}

// ---------------------------------------------------------------------------
// ─── Linked external issues (the owner's agent) ─────────────────────────────

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct InternalIssueLinksResponse {
    /// Primary first, each with a live summary or a typed error.
    links: Vec<crate::services::external_issues::WorkspaceIssueLinkDTO>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct InternalLinkedIssueResponse {
    link: crate::services::external_issues::WorkspaceIssueLinkDTO,
    issue: crate::services::external_issues::ExternalIssueDetailDTO,
}

#[derive(Debug, Deserialize)]
struct LinkedIssueInternalPath {
    id: Uuid,
    link_id: Uuid,
}

/// The owner's tenant scope behind the workspace incarnation fence, for an
/// agent's native issue reads and writes.
struct IncarnationDb<'a> {
    state: &'a Arc<AppState>,
    workspace_id: Uuid,
    incarnation: &'a WorkspaceIncarnationClaims,
    scopes: &'static [&'static str],
}

#[async_trait::async_trait]
impl crate::services::external_issues::TenantDbOpener for IncarnationDb<'_> {
    async fn open(&self) -> Result<ScopedDb, G5Error> {
        scoped_db_for_workspace_incarnation(
            self.state,
            self.workspace_id,
            self.incarnation,
            self.scopes,
        )
        .await
        .map_err(|error| error.inner)
    }
}

/// `GET /api/internal/workspaces/:id/issue-links`
///
/// The workspace's linked issues for its owner's agent (`g5d issue list`),
/// read inside configured issue domains exactly as the owner's Linked issues
/// view. Read-only: agents get no tracker credentials and no write path here.
async fn list_issue_links_internal(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(workspace_id): Path<Uuid>,
) -> Result<Json<ResponseEnvelope<InternalIssueLinksResponse>>, AppError> {
    use crate::services::external_issues::{
        fetch_link_summaries, list_domain_access, list_workspace_links, IssueErrorDTO,
    };
    let mut db = scoped_db_for_workspace_incarnation(
        &state,
        workspace_id,
        &incarnation,
        &["workspaces:read"],
    )
    .await?;
    crate::services::external_issues::ensure_external_issues_enabled(&mut db, incarnation.org_id)
        .await?;
    let links = list_workspace_links(&mut db, incarnation.org_id, workspace_id).await?;
    let domains = if links.is_empty() {
        Vec::new()
    } else {
        list_domain_access(&mut db, incarnation.org_id).await?
    };
    db.commit().await.map_err(AppError::from)?;
    let opener = IncarnationDb {
        state: &state,
        workspace_id,
        incarnation: &incarnation,
        scopes: &["workspaces:read"],
    };
    let backends = crate::routes::external_issues::issue_backends(
        &state,
        incarnation.org_id,
        &domains,
        &opener,
    )
    .await?;
    let outcomes = fetch_link_summaries(&backends, &domains, &links).await;
    let links = links
        .iter()
        .zip(outcomes)
        .map(|(link, outcome)| match outcome {
            Ok(summary) => link.to_dto(Some(summary), None),
            Err(error) => link.to_dto(None, Some(IssueErrorDTO::from(&error))),
        })
        .collect();
    Ok(Json(ResponseEnvelope::ok(InternalIssueLinksResponse {
        links,
    })))
}

/// `GET /api/internal/workspaces/:id/issue-links/:link_id`
///
/// One active link of the workspace with the issue's description and first
/// comment page (`g5d issue show`). A link of another workspace is not found.
async fn get_linked_issue_internal(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(params): Path<LinkedIssueInternalPath>,
) -> Result<Json<ResponseEnvelope<InternalLinkedIssueResponse>>, AppError> {
    use crate::services::external_issues::{
        fetch_linked_issue_detail, list_domain_access, list_workspace_links,
    };
    let workspace_id = params.id;
    let mut db = scoped_db_for_workspace_incarnation(
        &state,
        workspace_id,
        &incarnation,
        &["workspaces:read"],
    )
    .await?;
    crate::services::external_issues::ensure_external_issues_enabled(&mut db, incarnation.org_id)
        .await?;
    let link = list_workspace_links(&mut db, incarnation.org_id, workspace_id)
        .await?
        .into_iter()
        .find(|link| link.id == params.link_id)
        .ok_or_else(|| {
            AppError::from(G5Error::NotFound {
                resource: format!("issue link {}", params.link_id),
            })
        })?;
    let domains = list_domain_access(&mut db, incarnation.org_id).await?;
    db.commit().await.map_err(AppError::from)?;
    let opener = IncarnationDb {
        state: &state,
        workspace_id,
        incarnation: &incarnation,
        scopes: &["workspaces:read"],
    };
    let backends = crate::routes::external_issues::issue_backends(
        &state,
        incarnation.org_id,
        &domains,
        &opener,
    )
    .await?;
    let issue = fetch_linked_issue_detail(&backends, &domains, &link, true)
        .await
        .map_err(|error| AppError::from(G5Error::from(error)))?;
    Ok(Json(ResponseEnvelope::ok(InternalLinkedIssueResponse {
        link: link.to_dto(None, None),
        issue,
    })))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AgentIssueCommentRequest {
    body: String,
    /// Stable across retries of the same comment.
    idempotency_key: String,
    /// The issue to comment on: a link id, a reference, URL or number, as
    /// `g5d issue view` takes; none means the primary issue.
    #[serde(rename = "ref", default)]
    reference: Option<String>,
}

/// The issue an agent's argument names (`g5d issue view` and `comment`): a
/// link of this workspace, or any issue in the organization's configured
/// domains. A bare number names a linked issue first, then one in the
/// creation domain.
enum AgentIssueChoice<'a> {
    Link(&'a crate::services::external_issues::WorkspaceIssueLinkRecord),
    Resolved(Box<crate::services::external_issues::ResolvedIssue>),
}

async fn choose_agent_issue<'a>(
    backends: &crate::services::external_issues::IssueBackends<'_>,
    links: &'a [crate::services::external_issues::WorkspaceIssueLinkRecord],
    domains: &[crate::services::external_issues::IssueDomainAccess],
    reference: Option<&str>,
) -> Result<AgentIssueChoice<'a>, AppError> {
    use crate::services::external_issues::{
        resolve_reference, select_workspace_link, LinkSelection,
    };
    use crate::services::issue_creations::target_reference_for;
    let reference = match select_workspace_link(links, reference)? {
        LinkSelection::Link(link) => return Ok(AgentIssueChoice::Link(link)),
        LinkSelection::Unlinked(number) => {
            let target = domains
                .iter()
                .find(|access| access.domain.is_default_create_target)
                .ok_or_else(|| {
                    AppError::from(G5Error::ValidationError {
                        details: serde_json::json!({
                            "field": "ref",
                            "error": format!(
                                "#{number} is not linked to this workspace and the organization has no destination for new issues; name the issue in full (owner/name#{number} or KEY-{number})"
                            ),
                        }),
                    })
                })?;
            target_reference_for(Some(&number.to_string()), &target.domain)
        }
        LinkSelection::Reference => reference.map(str::to_string),
    }
    .ok_or_else(|| {
        AppError::from(G5Error::ValidationError {
            details: serde_json::json!({"field": "ref", "error": "name an issue"}),
        })
    })?;
    resolve_reference(backends, domains, &reference)
        .await
        .map(|resolved| AgentIssueChoice::Resolved(Box::new(resolved)))
        .map_err(|error| AppError::from(G5Error::from(error)))
}

/// The issue an agent's write names (`g5d issue comment`, `edit`, `close`,
/// `reopen`), chosen as [`choose_agent_issue`] chooses for `view`: a link of
/// this workspace, else any issue in the configured domains. Without a
/// reference, a workspace with no primary issue is refused as
/// `no_primary_issue`.
pub(crate) async fn agent_write_target(
    state: &Arc<AppState>,
    incarnation: &WorkspaceIncarnationClaims,
    links: &[crate::services::external_issues::WorkspaceIssueLinkRecord],
    domains: &[crate::services::external_issues::IssueDomainAccess],
    reference: Option<&str>,
) -> Result<crate::routes::external_issues::CommentTarget, AppError> {
    use crate::routes::external_issues::CommentTarget;
    let opener = IncarnationDb {
        state,
        workspace_id: incarnation.workspace_id,
        incarnation,
        scopes: &["workspaces:read"],
    };
    let backends =
        crate::routes::external_issues::issue_backends(state, incarnation.org_id, domains, &opener)
            .await?;
    let reference = reference
        .map(str::trim)
        .filter(|reference| !reference.is_empty());
    match choose_agent_issue(&backends, links, domains, reference).await {
        Ok(AgentIssueChoice::Link(link)) => Ok(CommentTarget::from_link(link)),
        Ok(AgentIssueChoice::Resolved(resolved)) => {
            Ok(CommentTarget::from_summary(&resolved.summary))
        }
        // Without a reference, a workspace with no primary issue keeps its
        // long-standing refusal.
        Err(_) if reference.is_none() => Err(AppError::from(
            crate::services::issue_comments::comment_conflict(
                "no_primary_issue",
                "the workspace has no primary issue; name the issue",
            ),
        )),
        Err(error) => Err(error),
    }
}

/// `POST /api/internal/workspaces/:id/issue-comments`
///
/// The owner's agent comments as the owner (`g5d issue comment`) on the
/// workspace's primary issue, or on the issue `ref` names: any issue the
/// owner could read in the organization's configured domains. On a GitHub
/// issue the same policy as a person's comment applies (the domain allows G5
/// comments, GitHub reports the repository non-public, the body passes the
/// secret screen, a retry with the key finds the earlier send by its marker)
/// and the agent never receives the GitHub token; on a G5 issue the comment
/// is written in G5, keyed by the idempotency key.
async fn post_issue_comment_internal(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(workspace_id): Path<Uuid>,
    Json(request): Json<AgentIssueCommentRequest>,
) -> Result<
    (
        StatusCode,
        Json<ResponseEnvelope<crate::routes::external_issues::CommentResultDTO>>,
    ),
    AppError,
> {
    use crate::routes::external_issues::{publish_issue_comment, IssueComment};
    use crate::services::external_issues::{list_domain_access, list_workspace_links};

    let key = request.idempotency_key.as_str();
    if key.is_empty() || key.trim() != key || key.len() > 190 {
        return Err(AppError::from(G5Error::ValidationError {
            details: serde_json::json!({
                "field": "idempotencyKey",
                "error": "must be 1-190 characters without surrounding whitespace",
            }),
        }));
    }
    // Keys share the owner's namespace with their own comments.
    let key = format!("agent:{key}");
    let open_db = || {
        scoped_db_for_workspace_incarnation(
            &state,
            workspace_id,
            &incarnation,
            &["workspaces:read"],
        )
    };
    let mut db = open_db().await?;
    crate::services::external_issues::ensure_external_issues_enabled(&mut db, incarnation.org_id)
        .await?;
    let links = list_workspace_links(&mut db, incarnation.org_id, workspace_id).await?;
    let domains = list_domain_access(&mut db, incarnation.org_id).await?;
    db.commit().await.map_err(AppError::from)?;

    let target = agent_write_target(
        &state,
        &incarnation,
        &links,
        &domains,
        request.reference.as_deref(),
    )
    .await?;

    let outcome = publish_issue_comment(
        &state,
        &IssueComment {
            org_id: incarnation.org_id,
            target: &target,
            domains: &domains,
            author: incarnation.owner_user_id,
            idempotency_key: &key,
            body: &request.body,
            origin: "agent",
        },
        open_db,
    )
    .await?;
    tracing::info!(
        %workspace_id,
        comment = %outcome.id(),
        issue = %target.display_key,
        "agent issue comment submitted"
    );
    let status = if outcome.fresh() {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((status, Json(ResponseEnvelope::ok(outcome.into_dto()))))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AgentOwnerEmailRequest {
    subject: String,
    body: String,
    /// `text` (the default) or `markdown`.
    #[serde(default)]
    format: AgentOwnerEmailFormat,
    /// Stable across retries of the same request.
    idempotency_key: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
enum AgentOwnerEmailFormat {
    #[default]
    Text,
    Markdown,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AgentOwnerEmailResponse {
    delivery_id: Uuid,
    /// `queued`, or `existing` when the idempotency key was already used.
    status: &'static str,
}

/// `POST /api/internal/workspaces/:id/owner-email`
///
/// The workspace's agent emails the workspace's current owner
/// (`g5d workspace email-owner`, `g5.workspace.owner-email`). The recipient comes from
/// the live incarnation's owner, never from the request. A retry with the
/// same key returns the earlier request instead of sending again.
async fn email_owner_internal(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(workspace_id): Path<Uuid>,
    Json(request): Json<AgentOwnerEmailRequest>,
) -> Result<(StatusCode, Json<ResponseEnvelope<AgentOwnerEmailResponse>>), AppError> {
    use crate::services::workspace_owner_email::{
        request_owner_email, BodyFormat, OwnerEmailOutcome, OwnerEmailRequest,
    };

    if workspace_id != incarnation.workspace_id {
        return Err(AppError::from(G5Error::Forbidden {
            missing_scopes: Vec::new(),
        }));
    }
    // The live-incarnation check, the owner lookup and the request commit
    // together on the system pool: the delivery table and the owner's login
    // email are system data the tenant role cannot read.
    let mut db = crate::db::scoped::scoped_db_for_system_org_with(
        state.authz.as_ref(),
        &state.system_pool,
        incarnation.org_id,
        &["workspaces:read"],
    )
    .await?;
    let outcome = request_owner_email(
        &state.email,
        &mut db,
        OwnerEmailRequest {
            claims: &incarnation,
            subject: &request.subject,
            body: &request.body,
            format: match request.format {
                AgentOwnerEmailFormat::Text => BodyFormat::Text,
                AgentOwnerEmailFormat::Markdown => BodyFormat::Markdown,
            },
            idempotency_key: &request.idempotency_key,
        },
    )
    .await?;
    db.commit().await.map_err(AppError::from)?;
    let (status, delivery_id, state_name) = match outcome {
        OwnerEmailOutcome::Queued(id) => (StatusCode::CREATED, id, "queued"),
        OwnerEmailOutcome::Existing(id) => (StatusCode::OK, id, "existing"),
    };
    Ok((
        status,
        Json(ResponseEnvelope::ok(AgentOwnerEmailResponse {
            delivery_id,
            status: state_name,
        })),
    ))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AgentIssueCreateRequest {
    title: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    assignees: Vec<String>,
    /// Stable across retries of the same creation.
    idempotency_key: String,
    /// An issue in the creation target to create the issue under: its URL,
    /// `owner/name#N` or number.
    #[serde(default)]
    parent: Option<String>,
    /// Images will be attached next (`--attach`): refused before anything
    /// is created unless new issues go to G5 issues.
    #[serde(default)]
    attachments: bool,
    /// Labels from the destination's catalog (`g5.issue.labels`).
    #[serde(default)]
    labels: Vec<String>,
}

/// `POST /api/internal/workspaces/:id/issues`
///
/// The owner's agent creates an issue in the organization's default
/// creation target (`g5d issue create`) under the owner's connected GitHub
/// identity, through the same checks as a person's creation. `@me` is the
/// owner. A retry with the same key returns the issue an earlier attempt
/// created, and puts it under the requested parent if it is not yet. The
/// issue is not linked to the workspace.
async fn create_issue_internal(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(workspace_id): Path<Uuid>,
    Json(request): Json<AgentIssueCreateRequest>,
) -> Result<
    (
        StatusCode,
        Json<ResponseEnvelope<crate::services::issue_creations::IssueCreationDTO>>,
    ),
    AppError,
> {
    use crate::routes::external_issues::{create_tracker_issue, TrackerIssueCreation};
    use crate::services::external_issues::list_domain_access;
    use crate::services::issue_creations::validate_key;

    validate_key(&request.idempotency_key, 190).map_err(AppError::from)?;
    // Keys share the owner's namespace with their own creations.
    let marker_key = format!("agent:{}", request.idempotency_key);
    let mut db = scoped_db_for_workspace_incarnation(
        &state,
        workspace_id,
        &incarnation,
        &["workspaces:read"],
    )
    .await?;
    crate::services::external_issues::ensure_external_issues_enabled(&mut db, incarnation.org_id)
        .await?;
    let domains = list_domain_access(&mut db, incarnation.org_id).await?;
    db.commit().await.map_err(AppError::from)?;
    if request.attachments
        && !crate::routes::external_issues::creation_target(&domains)
            .is_some_and(|access| access.is_native())
    {
        return Err(AppError::from(
            crate::services::issue_comments::comment_conflict(
                "uploads_unsupported",
                "new issues go to GitHub, which takes no uploads from G5: create the issue \
                 without --attach and describe the images, or attach them on GitHub",
            ),
        ));
    }

    let result = create_tracker_issue(
        &state,
        &TrackerIssueCreation {
            org_id: incarnation.org_id,
            domains: &domains,
            expected_domain: None,
            author: incarnation.owner_user_id,
            idempotency_key: &request.idempotency_key,
            marker_key: &marker_key,
            title: &request.title,
            body: &request.body,
            assignees: &request.assignees,
            labels: &request.labels,
            parent: request.parent.as_deref(),
            check_only: false,
            origin: "agent",
        },
        &IncarnationDb {
            state: &state,
            workspace_id,
            incarnation: &incarnation,
            scopes: &["workspaces:read"],
        },
    )
    .await?;
    let status = if result.sent && result.state == "created" {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((status, Json(ResponseEnvelope::ok(result))))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AgentIssueLinkRequest {
    /// An issue URL, `owner/name#N` or a number in the creation target.
    #[serde(rename = "ref")]
    reference: String,
    /// `primary` or `related`.
    role: String,
}

/// `POST /api/internal/workspaces/:id/issue-links`
///
/// The owner's agent links an existing issue to its workspace
/// (`g5d issue link`) through the same operation as the owner's Link
/// existing, with the same primary bounds and conflicts. The tracker is not
/// changed.
async fn link_issue_internal(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(workspace_id): Path<Uuid>,
    Json(request): Json<AgentIssueLinkRequest>,
) -> Result<
    (
        StatusCode,
        Json<ResponseEnvelope<crate::services::external_issues::WorkspaceIssueLinkDTO>>,
    ),
    AppError,
> {
    use crate::services::external_issues::{
        attach_primary_owners, create_workspace_link, list_domain_access, resolve_reference,
        LinkRole,
    };

    let role = LinkRole::parse(request.role.trim()).ok_or_else(|| {
        AppError::from(G5Error::ValidationError {
            details: serde_json::json!({"field": "role", "error": "must be primary or related"}),
        })
    })?;
    let owner = incarnation.owner_user_id;
    let mut db = scoped_db_for_workspace_incarnation(
        &state,
        workspace_id,
        &incarnation,
        &["workspaces:write"],
    )
    .await?;
    crate::services::external_issues::ensure_external_issues_enabled(&mut db, incarnation.org_id)
        .await?;
    let domains = list_domain_access(&mut db, incarnation.org_id).await?;
    db.commit().await.map_err(AppError::from)?;

    // A bare number, as `gh` takes, names an issue in the creation target.
    let reference =
        crate::services::issue_creations::creation_reference(&domains, &request.reference);
    let opener = IncarnationDb {
        state: &state,
        workspace_id,
        incarnation: &incarnation,
        scopes: &["workspaces:read"],
    };
    let backends = crate::routes::external_issues::issue_backends(
        &state,
        incarnation.org_id,
        &domains,
        &opener,
    )
    .await?;
    let mut summary = resolve_reference(&backends, &domains, &reference)
        .await
        .map_err(|error| AppError::from(G5Error::from(error)))?
        .summary;

    let mut db = scoped_db_for_workspace_incarnation(
        &state,
        workspace_id,
        &incarnation,
        &["workspaces:write"],
    )
    .await?;
    let link = create_workspace_link(
        &mut db,
        &state.system_pool,
        Some(owner),
        incarnation.org_id,
        workspace_id,
        owner,
        &summary,
        role,
    )
    .await?;
    attach_primary_owners(
        &mut db,
        &state.system_pool,
        Some(owner),
        incarnation.org_id,
        &mut [&mut summary],
    )
    .await?;
    db.commit().await.map_err(AppError::from)?;
    tracing::info!(
        %workspace_id,
        link_id = %link.id,
        primary = link.is_primary,
        "agent linked workspace issue"
    );
    Ok((
        StatusCode::CREATED,
        Json(ResponseEnvelope::ok(link.to_dto(Some(summary), None))),
    ))
}

/// Open the organization for the owner's agent to read beyond its own
/// workspace (`g5.external-issues.cli.reads`), behind the live incarnation
/// fence. The fence rechecks the workspace's current owner on every
/// request, and the database keeps a live workspace's owner an eligible
/// member (`workspaces_owner_same_org_fk`), so the owner is a member here.
async fn owner_read_db_for_incarnation(
    state: &Arc<AppState>,
    workspace_id: Uuid,
    claims: &WorkspaceIncarnationClaims,
) -> Result<ScopedDb, AppError> {
    scoped_db_for_workspace_incarnation(state, workspace_id, claims, &["workspaces:read"]).await
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AgentIssueViewQuery {
    /// A link id, `owner/name#N`, URL or number; none means the primary
    /// issue.
    #[serde(rename = "ref", default)]
    reference: Option<String>,
    /// Read the first comment page too.
    #[serde(default)]
    comments: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct InternalIssueViewResponse {
    /// The issue; its `links` are the issue's active links as the owner
    /// sees them.
    issue: crate::services::external_issues::ExternalIssueDetailDTO,
    /// This workspace's link to the issue, if any.
    workspace_link: Option<crate::services::external_issues::WorkspaceIssueLinkDTO>,
}

/// `GET /api/internal/workspaces/:id/issues/view?ref=&comments=`
///
/// Any issue the owner could read (`g5d issue view`): a link of this
/// workspace, or any issue in the organization's configured issue domains,
/// read with the installation token. A bare number names a linked issue
/// first, then one in the creation target.
async fn view_issue_internal(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(workspace_id): Path<Uuid>,
    Query(query): Query<AgentIssueViewQuery>,
) -> Result<Json<ResponseEnvelope<InternalIssueViewResponse>>, AppError> {
    use crate::services::external_issues::{
        fetch_issue_detail, fetch_linked_issue_detail, issue_link_refs, link_for_issue,
        list_domain_access, list_workspace_links,
    };

    let owner = incarnation.owner_user_id;
    let org_id = incarnation.org_id;
    let mut db = owner_read_db_for_incarnation(&state, workspace_id, &incarnation).await?;
    crate::services::external_issues::ensure_external_issues_enabled(&mut db, org_id).await?;
    let links = list_workspace_links(&mut db, org_id, workspace_id).await?;
    let domains = list_domain_access(&mut db, org_id).await?;
    db.commit().await.map_err(AppError::from)?;

    let issue_error = |error| AppError::from(G5Error::from(error));
    let opener = IncarnationDb {
        state: &state,
        workspace_id,
        incarnation: &incarnation,
        scopes: &["workspaces:read"],
    };
    let backends =
        crate::routes::external_issues::issue_backends(&state, org_id, &domains, &opener).await?;
    let (mut issue, link) =
        match choose_agent_issue(&backends, &links, &domains, query.reference.as_deref()).await? {
            AgentIssueChoice::Link(link) => (
                fetch_linked_issue_detail(&backends, &domains, link, query.comments)
                    .await
                    .map_err(issue_error)?,
                Some(link),
            ),
            AgentIssueChoice::Resolved(resolved) => {
                let issue = fetch_issue_detail(
                    &backends,
                    &domains[resolved.domain_index],
                    resolved.summary.locator.number,
                    query.comments,
                )
                .await
                .map_err(issue_error)?;
                let link = link_for_issue(&links, &issue.summary);
                (issue, link)
            }
        };

    let mut db = owner_read_db_for_incarnation(&state, workspace_id, &incarnation).await?;
    issue.links = issue_link_refs(
        &mut db,
        &state.system_pool,
        Some(owner),
        org_id,
        issue.summary.tracker_id,
        &issue.summary.external_issue_id,
    )
    .await?;
    crate::services::external_issues::attach_primary_owners(
        &mut db,
        &state.system_pool,
        Some(owner),
        org_id,
        &mut [&mut issue.summary],
    )
    .await?;
    db.commit().await.map_err(AppError::from)?;
    Ok(Json(ResponseEnvelope::ok(InternalIssueViewResponse {
        issue,
        workspace_link: link.map(|link| link.to_dto(None, None)),
    })))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AgentIssueAttachmentsQuery {
    /// The issue, as for `g5d issue view`; none means the primary issue.
    #[serde(rename = "ref", default)]
    reference: Option<String>,
    /// List the first comment page's attachments too.
    #[serde(default)]
    comments: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AgentIssueAttachmentQuery {
    #[serde(rename = "ref", default)]
    reference: Option<String>,
    /// The handle the listing gave.
    handle: String,
}

/// The workspace's links and the organization's domains, read as for `view`.
async fn agent_issue_scope(
    state: &Arc<AppState>,
    incarnation: &WorkspaceIncarnationClaims,
    workspace_id: Uuid,
) -> Result<
    (
        Vec<crate::services::external_issues::WorkspaceIssueLinkRecord>,
        Vec<crate::services::external_issues::IssueDomainAccess>,
    ),
    AppError,
> {
    use crate::services::external_issues::{list_domain_access, list_workspace_links};
    let org_id = incarnation.org_id;
    let mut db = owner_read_db_for_incarnation(state, workspace_id, incarnation).await?;
    crate::services::external_issues::ensure_external_issues_enabled(&mut db, org_id).await?;
    let links = list_workspace_links(&mut db, org_id, workspace_id).await?;
    let domains = list_domain_access(&mut db, org_id).await?;
    db.commit().await.map_err(AppError::from)?;
    Ok((links, domains))
}

/// The issue an attachment request names, chosen as `view` chooses it.
fn attachment_target<'a>(
    choice: &'a AgentIssueChoice<'a>,
    domains: &'a [crate::services::external_issues::IssueDomainAccess],
) -> crate::services::external_issues::IssueAttachmentTarget<'a> {
    use crate::services::external_issues::IssueAttachmentTarget;
    match choice {
        AgentIssueChoice::Link(link) => IssueAttachmentTarget::Linked(link),
        AgentIssueChoice::Resolved(resolved) => IssueAttachmentTarget::Domain {
            access: &domains[resolved.domain_index],
            number: resolved.summary.locator.number,
        },
    }
}

/// `GET /api/internal/workspaces/:id/issues/attachments?ref=&comments=`
///
/// The attachments of an issue `g5d issue view` can read, by place, without
/// their content (`g5.attachments`).
async fn list_issue_attachments_internal(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(workspace_id): Path<Uuid>,
    Query(query): Query<AgentIssueAttachmentsQuery>,
) -> Result<Json<ResponseEnvelope<crate::services::external_issues::IssueAttachmentsDTO>>, AppError>
{
    let (links, domains) = agent_issue_scope(&state, &incarnation, workspace_id).await?;
    let opener = IncarnationDb {
        state: &state,
        workspace_id,
        incarnation: &incarnation,
        scopes: &["workspaces:read"],
    };
    let backends = crate::routes::external_issues::issue_backends(
        &state,
        incarnation.org_id,
        &domains,
        &opener,
    )
    .await?;
    let choice =
        choose_agent_issue(&backends, &links, &domains, query.reference.as_deref()).await?;
    let listing = crate::services::external_issues::list_issue_attachments(
        &backends,
        &domains,
        attachment_target(&choice, &domains),
        query.comments,
    )
    .await
    .map_err(|error| AppError::from(G5Error::from(error)))?;
    Ok(Json(ResponseEnvelope::ok(listing)))
}

/// `GET /api/internal/workspaces/:id/issues/attachments/content?ref=&handle=`
///
/// One listed attachment, read through its adapter and streamed with its
/// recognized type (`g5.attachments`). GitHub's are fetched from GitHub's
/// own hosts without a credential.
async fn issue_attachment_content_internal(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(workspace_id): Path<Uuid>,
    Query(query): Query<AgentIssueAttachmentQuery>,
) -> Result<axum::response::Response, AppError> {
    let (links, domains) = agent_issue_scope(&state, &incarnation, workspace_id).await?;
    let opener = IncarnationDb {
        state: &state,
        workspace_id,
        incarnation: &incarnation,
        scopes: &["workspaces:read"],
    };
    let backends = crate::routes::external_issues::issue_backends(
        &state,
        incarnation.org_id,
        &domains,
        &opener,
    )
    .await?;
    let choice =
        choose_agent_issue(&backends, &links, &domains, query.reference.as_deref()).await?;
    let stream = crate::services::external_issues::open_issue_attachment(
        &backends,
        &domains,
        attachment_target(&choice, &domains),
        query.handle.trim(),
    )
    .await
    .map_err(AppError::from)?;
    tracing::info!(
        %workspace_id,
        kind = stream.kind.as_str(),
        length = ?stream.length,
        "agent read an issue attachment"
    );
    Ok(crate::routes::external_issues::attachment_stream_response(
        stream,
    ))
}

/// An agent upload's JSON: one image of at most 10 MiB, base64-encoded.
const AGENT_UPLOAD_BODY_LIMIT_BYTES: usize = 15 * 1024 * 1024;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AgentIssueUploadRequest {
    /// The issue, as for `g5d issue comment`; none means the primary issue.
    #[serde(rename = "ref", default)]
    reference: Option<String>,
    file_name: String,
    content_base64: String,
}

/// The image the agent added and the issue it is on.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AgentIssueUploadDTO {
    attachment: crate::services::native_issues::NativeAttachmentDTO,
    /// The issue's display key and page, to name it in the writes that show
    /// the image.
    display_key: String,
    issue_url: String,
}

/// `POST /api/internal/workspaces/:id/issues/attachments`
///
/// The owner's agent adds an image to a G5 issue as the owner
/// (`g5d issue … --attach`, `g5.attachments.cli.attach`): the issue is
/// chosen as for the agent's comments, a GitHub issue is refused before the
/// bytes are read, and the upload is keyed by the issue, the file name and
/// the content, so a retry adds it once.
async fn upload_issue_attachment_internal(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(workspace_id): Path<Uuid>,
    Json(request): Json<AgentIssueUploadRequest>,
) -> Result<(StatusCode, Json<ResponseEnvelope<AgentIssueUploadDTO>>), AppError> {
    use crate::services::external_issues::{list_domain_access, list_workspace_links};
    use crate::services::native_issues::{self, NativeReader};
    use base64::Engine as _;
    use sha2::{Digest, Sha256};

    let open_db = || {
        scoped_db_for_workspace_incarnation(
            &state,
            workspace_id,
            &incarnation,
            &["workspaces:read"],
        )
    };
    let mut db = open_db().await?;
    crate::services::external_issues::ensure_external_issues_enabled(&mut db, incarnation.org_id)
        .await?;
    crate::services::workspace_review_service::ensure_not_review_workspace(
        &mut db,
        incarnation.org_id,
        workspace_id,
        "Attach an image",
    )
    .await?;
    let links = list_workspace_links(&mut db, incarnation.org_id, workspace_id).await?;
    let domains = list_domain_access(&mut db, incarnation.org_id).await?;
    db.commit().await.map_err(AppError::from)?;
    let target = agent_write_target(
        &state,
        &incarnation,
        &links,
        &domains,
        request.reference.as_deref(),
    )
    .await?;
    let access = domains
        .iter()
        .find(|access| access.is_native() && access.domain.tracker_id == target.tracker_id)
        .filter(|_| target.is_native())
        .ok_or_else(|| {
            AppError::from(crate::services::issue_comments::comment_conflict(
                "uploads_unsupported",
                "only G5 issues take images from G5: GitHub's API accepts no uploads, so \
                 attach the image on GitHub or describe it",
            ))
        })?;
    let file_name = native_issues::normalize_file_name(&request.file_name)?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(request.content_base64.trim())
        .map_err(|_| {
            AppError::from(G5Error::ValidationError {
                details: serde_json::json!({"field": "contentBase64", "error": "not valid base64"}),
            })
        })?;
    let issue_id = native_issues::parse_issue_id(&target.external_issue_id)
        .map_err(|error| AppError::from(G5Error::from(error)))?;
    // The same file added to the same issue again is the same upload.
    let content = Sha256::digest(&bytes);
    let mut identity = Sha256::new();
    identity.update(file_name.as_bytes());
    identity.update([0]);
    identity.update(content);
    let key = format!(
        "agent:attach:{issue_id}:{}",
        hex::encode(&identity.finalize()[..16])
    );
    let site =
        native_issues::native_site(&state.system_pool, &state.external_url, incarnation.org_id)
            .await?;
    let reader = NativeReader {
        domain: &access.domain,
        site: &site,
        system_pool: &state.system_pool,
    };
    let mut db = open_db().await?;
    let (attachment, fresh) = native_issues::add_attachment(
        &mut db,
        &reader,
        &state.issue_attachments,
        &native_issues::NewNativeAttachment {
            org_id: incarnation.org_id,
            issue_id,
            uploader: incarnation.owner_user_id,
            origin: "agent",
            idempotency_key: &key,
            file_name: &file_name,
            bytes: &bytes,
        },
    )
    .await?;
    db.commit().await.map_err(AppError::from)?;
    tracing::info!(
        %workspace_id,
        issue = %target.display_key,
        attachment = attachment.id,
        fresh,
        "agent added an image to a G5 issue"
    );
    let status = if fresh {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((
        status,
        Json(ResponseEnvelope::ok(AgentIssueUploadDTO {
            attachment: attachment.to_dto(),
            issue_url: site.issue_url(target.number),
            display_key: target.display_key,
        })),
    ))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AgentPullRequestAttachmentsQuery {
    /// A GitHub PR number or URL, as for `g5d pr view NUMBER -c`.
    selector: String,
    #[serde(default)]
    repo: Option<String>,
    /// Include the conversation comments, reviews and review comments.
    #[serde(default)]
    comments: bool,
    /// For a fetch: the handle the listing gave.
    #[serde(default)]
    handle: Option<String>,
}

impl AgentPullRequestAttachmentsQuery {
    fn read_query(
        &self,
    ) -> crate::services::workspace_pull_request_provider_lifecycle::PullRequestReadQuery {
        crate::services::workspace_pull_request_provider_lifecycle::PullRequestReadQuery {
            selector: Some(self.selector.clone()),
            repo: self.repo.clone(),
            comments: self.comments,
            ..Default::default()
        }
    }
}

/// `GET /api/internal/workspaces/:id/pull-requests/github/attachments?selector=&repo=&comments=`
///
/// A readable GitHub PR's attachments for the owner's agent, as the owner
/// (`g5d pr attachments`, `g5.attachments`).
async fn list_pull_request_attachments_internal(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(claims)): Extension<WorkspaceIncarnationContext>,
    Path(workspace): Path<Uuid>,
    Query(query): Query<AgentPullRequestAttachmentsQuery>,
) -> Result<
    Json<
        ResponseEnvelope<
            crate::services::workspace_pull_request_provider_lifecycle::PullRequestAttachmentsDTO,
        >,
    >,
    AppError,
> {
    let db = scoped_db_for_workspace_incarnation(&state, workspace, &claims, &["workspaces:read"])
        .await?;
    let listing = state
        .workspace_pull_request_provider_lifecycle_service
        .list_pull_request_attachments(db, workspace, claims.owner_user_id, &query.read_query())
        .await?;
    Ok(Json(ResponseEnvelope::ok(listing)))
}

/// `GET /api/internal/workspaces/:id/pull-requests/github/attachments/content?selector=&repo=&handle=`
///
/// One listed GitHub PR attachment, fetched from GitHub's own hosts without
/// a credential and streamed with its recognized type.
async fn pull_request_attachment_content_internal(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(claims)): Extension<WorkspaceIncarnationContext>,
    Path(workspace): Path<Uuid>,
    Query(query): Query<AgentPullRequestAttachmentsQuery>,
) -> Result<axum::response::Response, AppError> {
    let handle = query.handle.clone().ok_or_else(|| {
        AppError::from(G5Error::ValidationError {
            details: serde_json::json!({"field": "handle", "error": "name the attachment's handle"}),
        })
    })?;
    let db = scoped_db_for_workspace_incarnation(&state, workspace, &claims, &["workspaces:read"])
        .await?;
    let stream = state
        .workspace_pull_request_provider_lifecycle_service
        .open_pull_request_attachment(
            db,
            workspace,
            claims.owner_user_id,
            &query.read_query(),
            &handle,
        )
        .await?;
    tracing::info!(
        %workspace,
        kind = stream.kind.as_str(),
        length = ?stream.length,
        "agent read a pull request attachment"
    );
    Ok(crate::routes::external_issues::attachment_stream_response(
        stream,
    ))
}

/// Most results one agent search returns: G5's merged page.
const AGENT_SEARCH_MAX_LIMIT: usize = 30;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AgentIssueSearchQuery {
    #[serde(default)]
    q: Option<String>,
    /// `open`, `closed` or `all` (the default here, as `gh search issues`).
    #[serde(default)]
    state: Option<String>,
    /// `me` (the owner) or `none`.
    #[serde(default)]
    assignee: Option<String>,
    /// One configured domain, `owner/name`.
    #[serde(default)]
    repo: Option<String>,
    /// 1–30, default 30.
    #[serde(default)]
    limit: Option<usize>,
}

/// `GET /api/internal/workspaces/:id/issues/search?q=&state=&assignee=&repo=&limit=`
///
/// The person's federated search (`g5d issue search`) run as the owner,
/// within the agents' search budget. A qualifier the search would drop
/// (`repo:`, `is:` …) is refused rather than ignored.
async fn search_issues_internal(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(workspace_id): Path<Uuid>,
    Query(query): Query<AgentIssueSearchQuery>,
) -> Result<
    Json<ResponseEnvelope<crate::services::external_issues::ExternalIssueSearchResultsDTO>>,
    AppError,
> {
    use crate::routes::external_issues::{parse_search_filters, run_issue_search, IssueSearch};
    use crate::services::external_issues::{list_domain_access, reserve_agent_search};
    use crate::services::github_issues::reserved_qualifier;

    let invalid = |field: &str, error: String| {
        AppError::from(G5Error::ValidationError {
            details: serde_json::json!({"field": field, "error": error}),
        })
    };
    let limit = query.limit.unwrap_or(AGENT_SEARCH_MAX_LIMIT);
    if !(1..=AGENT_SEARCH_MAX_LIMIT).contains(&limit) {
        return Err(invalid(
            "limit",
            format!("G5 search returns at most {AGENT_SEARCH_MAX_LIMIT} issues; use 1-{AGENT_SEARCH_MAX_LIMIT} and narrow the query"),
        ));
    }
    let text = query.q.as_deref().unwrap_or_default();
    if let Some((term, qualifier)) = text
        .split_whitespace()
        .find_map(|term| reserved_qualifier(term).map(|qualifier| (term, qualifier)))
    {
        return Err(invalid(
            "q",
            format!("G5 search sets {qualifier} itself and would drop {term}; use the search's own options instead"),
        ));
    }
    let filters = parse_search_filters(
        Some(query.state.as_deref().unwrap_or("all")),
        query.assignee.as_deref(),
    )?;
    let owner = incarnation.owner_user_id;
    let org_id = incarnation.org_id;
    let mut db = owner_read_db_for_incarnation(&state, workspace_id, &incarnation).await?;
    crate::services::external_issues::ensure_external_issues_enabled(&mut db, org_id).await?;
    let mut domains = list_domain_access(&mut db, org_id).await?;
    db.commit().await.map_err(AppError::from)?;
    // The budget shares GitHub's search allowance; G5 issues spend none
    // (`g5.issue.native.search`).
    let configured = domains.iter().filter(|access| !access.is_native()).count();
    if let Some(repo) = query.repo.as_deref().map(str::trim) {
        domains.retain(|access| access.domain.display_key.eq_ignore_ascii_case(repo));
        if domains.is_empty() {
            return Err(AppError::from(G5Error::from(
                crate::services::github_issues::IssueError::new(
                    g5_contracts::error::ExternalIssueErrorKind::OutsideDomain,
                    format!("{repo} is not a configured issue domain"),
                ),
            )));
        }
    }
    if domains.iter().any(|access| !access.is_native()) {
        let store =
            crate::middleware::rate_limit::PostgresRateLimitStore::new(state.system_pool.clone());
        reserve_agent_search(&store, org_id, workspace_id, configured).await?;
    }
    let mut response = run_issue_search(
        &state,
        &IssueSearch {
            org_id,
            viewer: Some(owner),
            domains: &domains,
            query: text,
            filters,
        },
        || owner_read_db_for_incarnation(&state, workspace_id, &incarnation),
    )
    .await?;
    if response.results.len() > limit {
        response.results.truncate(limit);
        response.truncated = true;
    }
    Ok(Json(ResponseEnvelope::ok(response)))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AgentIssueUnlinkRequest {
    /// A link id, the recorded `owner/name#N`, URL or number.
    #[serde(rename = "ref")]
    reference: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct InternalIssueUnlinkResponse {
    /// The link that ended.
    link: crate::services::external_issues::WorkspaceIssueLinkDTO,
}

/// `POST /api/internal/workspaces/:id/issue-links/unlink`
///
/// The owner's agent ends one of its workspace's links (`g5d issue unlink`),
/// the primary one included, as the owner's Unlink does. Matched from G5's
/// records only: no tracker is read and the issue is not changed, so it
/// works while external issues are off.
async fn unlink_issue_internal(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(workspace_id): Path<Uuid>,
    Json(request): Json<AgentIssueUnlinkRequest>,
) -> Result<Json<ResponseEnvelope<InternalIssueUnlinkResponse>>, AppError> {
    use crate::services::external_issues::{
        list_workspace_links, select_workspace_link, unlink_workspace_link, LinkSelection,
    };
    let reference = request.reference.trim();
    if reference.is_empty() {
        return Err(AppError::from(G5Error::ValidationError {
            details: serde_json::json!({"field": "ref", "error": "name the linked issue"}),
        }));
    }
    let org_id = incarnation.org_id;
    let mut db = scoped_db_for_workspace_incarnation(
        &state,
        workspace_id,
        &incarnation,
        &["workspaces:write"],
    )
    .await?;
    let links = list_workspace_links(&mut db, org_id, workspace_id).await?;
    let link = match select_workspace_link(&links, Some(reference))? {
        LinkSelection::Link(link) => link.clone(),
        LinkSelection::Unlinked(_) | LinkSelection::Reference => {
            let keys: Vec<&str> = links.iter().map(|link| link.display_key.as_str()).collect();
            return Err(AppError::from(G5Error::NotFound {
                resource: format!(
                    "{reference} among this workspace's linked issues ({})",
                    if keys.is_empty() {
                        "none".to_string()
                    } else {
                        keys.join(", ")
                    }
                ),
            }));
        }
    };
    unlink_workspace_link(
        &mut db,
        org_id,
        workspace_id,
        link.id,
        incarnation.owner_user_id,
    )
    .await?;
    db.commit().await.map_err(AppError::from)?;
    tracing::info!(
        %workspace_id,
        link_id = %link.id,
        primary = link.is_primary,
        "agent unlinked workspace issue"
    );
    Ok(Json(ResponseEnvelope::ok(InternalIssueUnlinkResponse {
        link: link.to_dto(None, None),
    })))
}

/// `POST /api/internal/workspaces/:id/pull-requests/github/comments`
///
/// The owner's agent comments on a GitHub PR in one of the workspace's
/// repositories, or replies in its review thread (`g5d pr comment`), under the
/// owner's GitHub identity. The agent never receives the token.
async fn post_pull_request_comment_internal(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(workspace_id): Path<Uuid>,
    Json(request): Json<crate::routes::pull_request_conversation::PullRequestCommentRequest>,
) -> Result<
    (
        StatusCode,
        Json<ResponseEnvelope<crate::services::issue_comments::CommentPublicationDTO>>,
    ),
    AppError,
> {
    use crate::routes::pull_request_conversation::{agent_request, publish_pull_request_comment};
    let request = agent_request(request)?;
    let (record, fresh) = publish_pull_request_comment(
        &state,
        || {
            scoped_db_for_workspace_incarnation(
                &state,
                workspace_id,
                &incarnation,
                &["workspaces:read"],
            )
        },
        workspace_id,
        incarnation.owner_user_id,
        "agent",
        &request,
    )
    .await?;
    tracing::info!(%workspace_id, publication_id = %record.id, state = %record.state, "agent pull request comment submitted");
    let status = if fresh {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((status, Json(ResponseEnvelope::ok(record.to_dto()))))
}

/// `POST /api/internal/workspaces/:id/pull-requests/github/threads/resolve`
///
/// The owner's agent resolves or unresolves a review thread (`g5d pr
/// resolve`) under the owner's GitHub identity.
async fn resolve_review_thread_internal(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(workspace_id): Path<Uuid>,
    Json(request): Json<
        crate::services::workspace_pull_request_provider_lifecycle::ReviewThreadResolutionRequest,
    >,
) -> Result<Json<ResponseEnvelope<serde_json::Value>>, AppError> {
    let db = scoped_db_for_workspace_incarnation(
        &state,
        workspace_id,
        &incarnation,
        &["workspaces:read"],
    )
    .await?;
    crate::routes::pull_request_conversation::set_review_thread_resolution(
        &state,
        db,
        workspace_id,
        incarnation.owner_user_id,
        &request,
    )
    .await
    .map(|data| Json(ResponseEnvelope::ok(data)))
}

async fn get_pull_request_info_internal(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(claims)): Extension<WorkspaceIncarnationContext>,
    Path(workspace): Path<Uuid>,
    Query(query): Query<
        crate::services::workspace_pull_request_provider_lifecycle::PullRequestReadQuery,
    >,
) -> Result<Json<ResponseEnvelope<serde_json::Value>>, AppError> {
    let db = scoped_db_for_workspace_incarnation(&state, workspace, &claims, &["workspaces:read"])
        .await?;
    super::workspaces::read_pull_request_info(&state, db, workspace, claims.owner_user_id, query)
        .await
}

/// `GET /api/internal/workspaces/:id/pull-requests/:pullRequestId/description`
///
/// The canonical description `g5d pr view` prints: the same service read as
/// the user route behind the landing (`g5.git.pr-description`).
async fn get_pull_request_description_internal(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(claims)): Extension<WorkspaceIncarnationContext>,
    Path((workspace, pull_request_id)): Path<(Uuid, Uuid)>,
) -> Result<
    Json<
        ResponseEnvelope<
            crate::services::workspace_pull_request_provider_lifecycle::PullRequestDescriptionView,
        >,
    >,
    AppError,
> {
    let db = scoped_db_for_workspace_incarnation(&state, workspace, &claims, &["workspaces:read"])
        .await?;
    let view = state
        .workspace_pull_request_provider_lifecycle_service
        .read_pull_request_description(db, workspace, pull_request_id, claims.owner_user_id)
        .await?;
    Ok(Json(ResponseEnvelope::ok(view)))
}

/// `GET /api/internal/workspaces/:id/pull-requests/descriptions?ids=…`
///
/// `g5d pr list --json body`: the batch twin of the description read.
async fn get_pull_request_descriptions_internal(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(claims)): Extension<WorkspaceIncarnationContext>,
    Path(workspace): Path<Uuid>,
    Query(query): Query<super::workspaces::PullRequestDescriptionsQuery>,
) -> Result<
    Json<
        ResponseEnvelope<
            Vec<crate::services::workspace_pull_request_provider_lifecycle::PullRequestDescriptionText>,
        >,
    >,
    AppError,
>{
    let ids = query.ids()?;
    let db = scoped_db_for_workspace_incarnation(&state, workspace, &claims, &["workspaces:read"])
        .await?;
    let read = state
        .workspace_pull_request_provider_lifecycle_service
        .read_pull_request_descriptions(db, workspace, &ids, claims.owner_user_id)
        .await?;
    Ok(Json(ResponseEnvelope::ok(read)))
}

/// `GET /api/internal/workspaces/:id/pull-requests/targets`
///
/// The targets the UI offers for Create PR, for `g5d pr create -B NAME`.
async fn list_pull_request_targets_internal(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(claims)): Extension<WorkspaceIncarnationContext>,
    Path(workspace): Path<Uuid>,
) -> Result<
    Json<ResponseEnvelope<Vec<crate::services::workspace_pull_request_create::PullRequestTarget>>>,
    AppError,
> {
    let db = scoped_db_for_workspace_incarnation(&state, workspace, &claims, &["workspaces:read"])
        .await?;
    db.rollback().await.map_err(AppError::from)?;
    let targets =
        crate::services::workspace_pull_request_create::targets_for_incarnation(&state, &claims)
            .await?;
    Ok(Json(ResponseEnvelope::ok(targets)))
}

/// `POST /api/internal/workspaces/:id/pull-requests/preview`
///
/// What `g5d pr create` would publish into the target (`g5d pr create
/// --dry-run`), admitted as create is; writes nothing.
async fn preview_pull_request_internal(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(claims)): Extension<WorkspaceIncarnationContext>,
    Path(workspace): Path<Uuid>,
    Json(body): Json<super::workspaces::PreviewWorkspacePullRequestTextRequestDTO>,
) -> Result<
    Json<
        ResponseEnvelope<
            crate::services::coordinated_repo_publication::WorkspacePullRequestTextPreview,
        >,
    >,
    AppError,
> {
    let db = scoped_db_for_workspace_incarnation(&state, workspace, &claims, &["workspaces:read"])
        .await?;
    db.rollback().await.map_err(AppError::from)?;
    let preview = crate::services::workspace_pull_request_create::preview_for_incarnation(
        &state,
        &claims,
        body.target_workspace_id,
    )
    .await?;
    Ok(Json(ResponseEnvelope::ok(preview)))
}

/// `GET /api/internal/workspaces/:id/pull-requests/github`
///
/// `g5d pr list` in repository mode for the owner's agent, as the owner
/// (`g5.git.pr-list`); a GitHub search spends the agents' search budget
/// (`g5.git.pr-list.search`).
async fn list_repository_pull_requests_internal(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(claims)): Extension<WorkspaceIncarnationContext>,
    Path(workspace): Path<Uuid>,
    Query(query): Query<
        crate::services::workspace_pull_request_provider_lifecycle::PullRequestListQuery,
    >,
) -> Result<Json<ResponseEnvelope<serde_json::Value>>, AppError> {
    let db = scoped_db_for_workspace_incarnation(&state, workspace, &claims, &["workspaces:read"])
        .await?;
    let data = state
        .workspace_pull_request_provider_lifecycle_service
        .list_repository_pull_requests(db, workspace, claims.owner_user_id, &query, true)
        .await?;
    Ok(Json(ResponseEnvelope::ok(data)))
}

async fn create_pull_request_internal(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(claims)): Extension<WorkspaceIncarnationContext>,
    Path(workspace): Path<Uuid>,
    Json(body): Json<crate::services::workspace_pull_request_create::CreatePullRequestRequest>,
) -> Result<
    Json<
        ResponseEnvelope<crate::services::workspace_pull_request_create::CreatePullRequestOutcome>,
    >,
    AppError,
> {
    let outcome = crate::services::workspace_pull_request_create::create_or_update(
        &state,
        crate::services::workspace_pull_request_create::PullRequestCaller::Workspace(&claims),
        workspace,
        body,
    )
    .await?;
    Ok(Json(ResponseEnvelope::ok(outcome)))
}

// ─── Referenced questions (g5.documents.question.agent-access) ──────────────
//
// The owner's agent reads, authors and proposes through `g5d question`. Each
// route runs the live-incarnation check; writes also run the direct-agent
// write policy. The actor is always the agent of the incarnation's owner:
// provenance is `agent` whatever the body says, an answer lands as a
// proposal, and the write stays in the live file until the agent pushes.

#[derive(Debug, Deserialize)]
struct InternalQuestionSetQuery {
    #[serde(default)]
    path: Option<String>,
}

fn question_agent(
    claims: &WorkspaceIncarnationClaims,
) -> crate::services::question_set_service::QuestionActor {
    crate::services::question_set_service::QuestionActor::Agent {
        owner_user_id: claims.owner_user_id,
        vm_id: claims.vm_id,
    }
}

async fn capture_question_docs(
    state: &Arc<AppState>,
    workspace_id: Uuid,
    claims: &WorkspaceIncarnationClaims,
    write: bool,
) -> Result<crate::services::doc_service::CapturedDocsList, AppError> {
    let scopes: &[&str] = if write {
        &["workspaces:read", "workspaces:write"]
    } else {
        &["workspaces:read"]
    };
    let mut db = scoped_db_for_workspace_incarnation(state, workspace_id, claims, scopes).await?;
    if write {
        ensure_workspace_allows_mutation(
            &mut db,
            workspace_id,
            WorkspaceMutationKind::DirectAgentWrite,
            "internal.question_set_write",
        )
        .await?;
    }
    let captured = state
        .doc_service
        .capture_docs_list(&mut db, workspace_id)
        .await?;
    db.commit().await.map_err(AppError::from)?;
    Ok(captured)
}

/// `GET /api/internal/workspaces/:id/questions[?path=]`: the workspace's own
/// sets, or one set by path, as the owner's live checkout holds them.
async fn list_question_sets_internal(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(workspace_id): Path<Uuid>,
    Query(query): Query<InternalQuestionSetQuery>,
) -> Result<Json<ResponseEnvelope<serde_json::Value>>, AppError> {
    let captured = capture_question_docs(&state, workspace_id, &incarnation, false).await?;
    let service = crate::services::question_set_service::QuestionSetService::new(
        &state.doc_service,
        &state.workspace_runtime,
    );
    let viewer = Some(incarnation.owner_user_id);
    let sets = match query.path.as_deref() {
        Some(path) => vec![
            service
                .get_set(&captured, workspace_id, viewer, path)
                .await?,
        ],
        None => service.list_sets(&captured, workspace_id, viewer).await?,
    };
    Ok(Json(ResponseEnvelope::ok(
        serde_json::json!({ "sets": sets }),
    )))
}

/// `POST /api/internal/workspaces/:id/questions`: create or append a set in
/// the workspace's own questions directory, as a live write.
async fn create_question_set_internal(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(workspace_id): Path<Uuid>,
    Json(body): Json<crate::routes::questions::CreateQuestionSetRequestDTO>,
) -> Result<Json<ResponseEnvelope<crate::services::question_set_service::CreateSetOutcome>>, AppError>
{
    let captured = capture_question_docs(&state, workspace_id, &incarnation, true).await?;
    let outcome = crate::services::question_set_service::QuestionSetService::new(
        &state.doc_service,
        &state.workspace_runtime,
    )
    .create_or_append(
        &state.system_pool,
        &captured,
        workspace_id,
        &question_agent(&incarnation),
        body.into_command(),
    )
    .await?;
    Ok(Json(ResponseEnvelope::ok(outcome)))
}

/// `POST /api/internal/workspaces/:id/questions/answer`: the agent's
/// proposal, recorded with agent provenance in the live file.
async fn answer_question_internal(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(incarnation)): Extension<WorkspaceIncarnationContext>,
    Path(workspace_id): Path<Uuid>,
    Json(body): Json<crate::routes::questions::AnswerQuestionRequestDTO>,
) -> Result<Json<ResponseEnvelope<crate::services::question_set_service::AnswerOutcome>>, AppError>
{
    let command = body.into_command(false)?;
    let captured = capture_question_docs(&state, workspace_id, &incarnation, true).await?;
    let outcome = crate::services::question_set_service::QuestionSetService::new(
        &state.doc_service,
        &state.workspace_runtime,
    )
    .answer(
        &state.system_pool,
        &captured,
        workspace_id,
        &question_agent(&incarnation),
        command,
    )
    .await?;
    Ok(Json(ResponseEnvelope::ok(outcome)))
}

async fn save_source_control_internal(
    State(state): State<Arc<AppState>>,
    Extension(WorkspaceIncarnationContext(claims)): Extension<WorkspaceIncarnationContext>,
    Path(workspace): Path<Uuid>,
) -> Result<Json<ResponseEnvelope<serde_json::Value>>, AppError> {
    let db = scoped_db_for_workspace_incarnation(
        &state,
        workspace,
        &claims,
        &["workspaces:read", "workspaces:write"],
    )
    .await?;
    super::source_control_browser::finish_save(
        &state,
        db,
        workspace,
        claims.owner_user_id,
        (claims.vm_id, claims.epoch),
    )
    .await?;
    // Revalidate the caller after the save, before publishing its checkpoint.
    let binding = g5_contracts::types::formal_review_runtime_scope(claims.workspace_id);
    let mut db =
        scoped_db_for_system_org(&state, claims.org_id, &["workspaces:write", &binding]).await?;
    // Serialize checkpoint writers before incarnation validation takes its
    // workspace/VM share locks. Concurrent lock upgrades would deadlock.
    state
        .workspace_service
        .lock_source_control_checkpoint(&mut db, workspace)
        .await?;
    require_live_workspace_incarnation(&mut db, workspace, &claims).await?;
    let result = super::source_control_browser::publish_checkpoint(&state, db, workspace).await?;
    Ok(Json(ResponseEnvelope::ok(result)))
}
