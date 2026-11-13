//! Doc route handlers under `/api/v1/workspaces/:workspaceId/docs`.
//!
//! This module implements the HTTP route handlers for the Docs API
//! (API_AND_STORAGE.md §3.3). Docs are markdown documents that capture
//! cross-cutting design intent. A doc may be standalone or reference SemLift
//! components through canonical repository frontmatter.
//!
//! Endpoints:
//! 1. `GET  /`            — list docs in workspace
//! 2. `GET  /:docId`      — doc detail + content
//! 3. `POST /`            — create doc
//! 4. `PUT  /:docId`      — update doc content/name
//! 7. `DELETE /:docId`      — delete a view doc (existing behavior)
//! 8. `PUT|DELETE /project-docs/:docId` — rewrite/delete a non-view Project Doc
//!
//! All handlers follow the thin handler pattern: extract request parameters,
//! delegate to the doc service on `AppState`, and format the response using
//! `ResponseEnvelope`. No business logic resides in the handlers.

use std::sync::Arc;

use axum::{
    extract::{DefaultBodyLimit, Path, Query, State},
    http::StatusCode,
    routing::{delete, get, post, put},
    Json, Router,
};
use base64::Engine;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db::scoped::{ensure_workspace_doc_id, scoped_db_for_doc, scoped_db_for_workspace};
use crate::errors::{AppError, G5Error};
use crate::middleware::{AuthContext, AuthenticatedUser};
use crate::routes::audited::AuditedRouter;
use crate::routes::ResponseEnvelope;
use crate::services::workspace_mutation_policy::ensure_workspace_mutable;
use crate::services::{AppState, DocService};
use g5_auth::Scope;

// ─── Path Parameter Types ────────────────────────────────────────────────────

/// Path parameters for workspace-scoped endpoints (`/api/v1/workspaces/:workspaceId/docs`).
#[derive(Debug, Deserialize)]
pub struct WorkspacePathParams {
    #[serde(rename = "workspaceId")]
    pub workspace_id: Uuid,
}

/// Path parameters for doc-scoped endpoints (`/api/v1/workspaces/:workspaceId/docs/:docId`).
#[derive(Debug, Deserialize)]
pub struct DocPathParams {
    #[serde(rename = "workspaceId")]
    pub workspace_id: Uuid,
    #[serde(rename = "docId")]
    pub doc_id: Uuid,
}

/// Path parameters for doc sync and current Project Doc mutations. Sync accepts
/// either a persisted doc UUID or a stable overlay doc handle; current
/// mutations require the repository handle returned by branch-scoped listing.
#[derive(Debug, Deserialize)]
pub struct DocSyncPathParams {
    #[serde(rename = "workspaceId")]
    pub workspace_id: Uuid,
    #[serde(rename = "docId")]
    pub doc_id: String,
}

/// Path parameters for view-scoped endpoints (`/api/v1/workspaces/:workspaceId/docs/views/:viewId`).
#[derive(Debug, Deserialize)]
pub struct ViewPathParams {
    #[serde(rename = "workspaceId")]
    pub workspace_id: Uuid,
    #[serde(rename = "viewId")]
    pub view_id: Uuid,
}

// ─── Request DTOs ────────────────────────────────────────────────────────────

/// Request body for `POST /api/v1/workspaces/:workspaceId/docs`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateDocRequest {
    /// Document name (required).
    pub name: String,
    /// Document type. One of: spec, design, api, guide, adr, note, view. Defaults to "design".
    #[serde(default = "default_doc_type")]
    pub doc_type: String,
    /// Optional file path (relative to repo root) for git-backed content.
    #[serde(default)]
    pub file_path: Option<String>,
    /// Optional initial markdown content.
    #[serde(default)]
    pub content: Option<String>,
}

fn default_doc_type() -> String {
    "design".to_string()
}

/// Request body for `PUT /api/v1/workspaces/:workspaceId/docs/:docId`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateDocRequest {
    /// Updated document name (optional).
    #[serde(default)]
    pub name: Option<String>,
    /// Updated markdown content (optional).
    #[serde(default)]
    pub content: Option<String>,
    /// Updated document type (optional).
    #[serde(default)]
    pub doc_type: Option<String>,
    /// Optimistic version from the workspace file read.
    #[serde(default)]
    pub expected_hash: Option<String>,
    pub expected_vm_id: Option<Uuid>,
    #[serde(default)]
    pub commit_and_push: bool,
}

// ─── Response DTOs ───────────────────────────────────────────────────────────

/// Summary DTO for the list endpoint.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocListItemDTO {
    pub id: String,
    pub name: String,
    pub doc_type: String,
    pub file_path: Option<String>,
    pub document_source: Option<String>,
    pub linked_node_count: i64,
    pub source: String,
    pub scope: String,
    pub repo_id: Option<String>,
    pub draft_dirty: bool,
    pub is_virtual: bool,
    pub is_future: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_uplifted_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_downlifted_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_updated_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Project-doc summaries plus the durable repository cut they came from.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocListResponse {
    pub docs: Vec<DocListItemDTO>,
    pub source: Option<String>,
    pub repo_oid: Option<String>,
    pub snapshot_at: Option<String>,
}

/// Full doc detail including content and canonical component references.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocDetailDTO {
    pub id: String,
    pub workspace_id: String,
    pub name: String,
    pub doc_type: String,
    pub file_path: Option<String>,
    pub document_source: Option<String>,
    pub content: Option<String>,
    /// Canonical component identities resolved from the captured repo cut.
    pub linked_component_ids: Vec<String>,
    pub source: String,
    pub scope: String,
    pub repo_id: Option<String>,
    pub draft_dirty: bool,
    pub is_virtual: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_uplifted_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_downlifted_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_updated_at: Option<String>,
    pub content_hash: Option<String>,
    pub repo_oid: Option<String>,
    pub snapshot_at: Option<String>,
    pub vm_id: Option<Uuid>,
    pub last_uplifted_commit_sha: Option<String>,
    pub last_downlifted_commit_sha: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Single item in the view list response.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewListItemResponseDTO {
    pub id: String,
    pub name: String,
    pub doc_type: String,
    pub file_path: Option<String>,
    pub is_future: bool,
}

/// Query parameters for opening a branch-scoped doc by file path.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocByPathQuery {
    pub path: String,
}

// ─── From Implementations ────────────────────────────────────────────────────

/// Service-level doc list item returned by the doc service.
///
/// The exact service type names depend on the doc service implementation.
/// These `From` conversions bridge the service DTOs to the route DTOs.
/// UUID → String, DateTime → ISO 8601 String, nullable fields → Option.
impl From<crate::services::doc_service::DocListItemDTO> for DocListItemDTO {
    fn from(doc: crate::services::doc_service::DocListItemDTO) -> Self {
        DocListItemDTO {
            id: doc.id,
            name: doc.name,
            doc_type: doc.doc_type,
            file_path: doc.file_path,
            document_source: doc.document_source,
            linked_node_count: doc.linked_node_count,
            source: doc.source,
            scope: doc.scope,
            repo_id: doc.repo_id.map(|id| id.to_string()),
            draft_dirty: doc.draft_dirty,
            is_virtual: doc.is_virtual,
            is_future: doc.is_future,
            last_uplifted_at: doc.last_uplifted_at.map(|dt| dt.to_rfc3339()),
            last_downlifted_at: doc.last_downlifted_at.map(|dt| dt.to_rfc3339()),
            content_updated_at: doc.content_updated_at.map(|dt| dt.to_rfc3339()),
            created_at: doc.created_at.to_rfc3339(),
            updated_at: doc.updated_at.to_rfc3339(),
        }
    }
}

impl From<crate::services::doc_service::DocDetailDTO> for DocDetailDTO {
    fn from(doc: crate::services::doc_service::DocDetailDTO) -> Self {
        DocDetailDTO {
            id: doc.id,
            workspace_id: doc.workspace_id.to_string(),
            name: doc.name,
            doc_type: doc.doc_type,
            file_path: doc.file_path,
            document_source: doc.document_source,
            content: doc.content,
            linked_component_ids: doc.linked_component_ids,
            source: doc.source,
            scope: doc.scope,
            repo_id: doc.repo_id.map(|id| id.to_string()),
            draft_dirty: doc.draft_dirty,
            is_virtual: doc.is_virtual,
            last_uplifted_at: doc.last_uplifted_at.map(|dt| dt.to_rfc3339()),
            last_downlifted_at: doc.last_downlifted_at.map(|dt| dt.to_rfc3339()),
            content_updated_at: doc.content_updated_at.map(|dt| dt.to_rfc3339()),
            content_hash: doc.content_hash,
            repo_oid: None,
            snapshot_at: None,
            vm_id: None,
            last_uplifted_commit_sha: doc.last_uplifted_commit_sha,
            last_downlifted_commit_sha: doc.last_downlifted_commit_sha,
            created_at: doc.created_at.to_rfc3339(),
            updated_at: doc.updated_at.to_rfc3339(),
        }
    }
}

async fn hydrate_doc_from_authorized_file_source(
    state: &Arc<AppState>,
    user: &AuthenticatedUser,
    auth: &AuthContext,
    workspace_id: Uuid,
    mut dto: DocDetailDTO,
) -> Result<DocDetailDTO, AppError> {
    let (Some(path), Some(repo_id)) = (dto.file_path.as_deref(), dto.repo_id.as_deref()) else {
        return Ok(dto);
    };
    let repo_id = repo_id.parse::<Uuid>().map_err(|_| {
        AppError::from_error(G5Error::Internal {
            message: format!("doc {} has invalid repo id {repo_id}", dto.id),
        })
    })?;
    let file = crate::routes::workspaces::read_workspace_file_for_actor(
        state,
        user,
        auth,
        workspace_id,
        repo_id,
        path,
    )
    .await?;
    dto.content = Some(file.content);
    dto.content_hash = Some(file.hash);
    dto.draft_dirty = file.source == "live";
    dto.source = file.source;
    dto.repo_oid = file.repo_oid;
    dto.snapshot_at = file.snapshot_at;
    dto.vm_id = file.vm_id;
    Ok(dto)
}

fn parse_project_doc_handle(doc_ref: &str) -> Result<((Uuid, Uuid), String), AppError> {
    let rest = doc_ref.strip_prefix("overlay:").ok_or_else(|| {
        validation_error(
            "docId",
            "current Project Docs mutation requires the repository handle returned by list_docs",
        )
    })?;
    let mut parts = rest.splitn(3, ':');
    let (Some(workspace), Some(repo), Some(encoded_path)) =
        (parts.next(), parts.next(), parts.next())
    else {
        return Err(validation_error(
            "docId",
            "repository handle requires workspace, repository and path",
        ));
    };
    let owner = (
        workspace
            .parse::<Uuid>()
            .map_err(|_| validation_error("docId", "invalid workspace identity"))?,
        repo.parse::<Uuid>()
            .map_err(|_| validation_error("docId", "invalid repository identity"))?,
    );
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(encoded_path)
        .map_err(|_| validation_error("docId", "repository document path is invalid"))?;
    let path = String::from_utf8(bytes)
        .map_err(|_| validation_error("docId", "repository document path is not UTF-8"))?;
    let path = crate::routes::files::sanitize_path(&path)?;
    Ok((owner, path))
}

fn validate_project_doc_owner(owner: (Uuid, Uuid), doc: &DocDetailDTO) -> Result<(), AppError> {
    let belongs = doc.workspace_id.parse::<Uuid>().ok() == Some(owner.0)
        && doc
            .repo_id
            .as_deref()
            .and_then(|id| id.parse::<Uuid>().ok())
            == Some(owner.1);
    if belongs {
        Ok(())
    } else {
        Err(validation_error(
            "docId",
            "repository document handle does not belong to the active workspace docs member",
        ))
    }
}

// ─── Router ──────────────────────────────────────────────────────────────────

/// Buffered-body limit for the docs router. `create_doc` and `update_doc`
/// receive the doc `content` through the Axum `Json` buffered extractor, and
/// uploaded HTML project docs are routinely several MiB — well past Axum's
/// default 2 MiB limit. Without this override an oversized request is rejected
/// by the extractor before the body is drained, leaving a browser upload
/// half-open so the client hangs instead of getting an error. Sized to match
/// the host LLM-proxy buffered-body cap (32 MiB). See routes/docs spec D21.
const DOCS_BODY_LIMIT_BYTES: usize = 32 * 1024 * 1024;

/// Doc routes nested under `/api/v1/workspaces/:workspaceId/docs`.
///
/// Endpoints:
/// - `GET  /`              — list docs in workspace
/// - `POST /`              — create doc
/// - `GET  /views`         — list retained view docs
/// - `GET  /views/:viewId` — get a retained view doc
/// - `GET  /:docId`        — get doc detail + content
/// - `PUT  /:docId`        — update doc
/// - `DELETE /:docId`      — delete view doc
/// - `PUT|DELETE /project-docs/:docId` — rewrite/delete non-view Project Doc
pub fn router() -> Router<Arc<AppState>> {
    AuditedRouter::new()
        .audited_route("/", get(list_docs), &[Scope::WorkspacesRead])
        .audited_route("/", post(create_doc), &[Scope::WorkspacesWrite])
        .audited_route("/views", get(list_views_handler), &[Scope::WorkspacesRead])
        .audited_route("/by-path", get(get_doc_by_path), &[Scope::WorkspacesRead])
        .audited_route(
            "/views/:viewId",
            get(get_view_handler),
            &[Scope::WorkspacesRead],
        )
        .audited_route(
            "/project-docs/:docId",
            put(update_project_doc),
            &[Scope::WorkspacesWrite],
        )
        .audited_route(
            "/project-docs/:docId",
            delete(delete_project_doc),
            &[Scope::WorkspacesWrite],
        )
        .audited_route("/:docId", get(get_doc), &[Scope::WorkspacesRead])
        .audited_route("/:docId", put(update_doc), &[Scope::WorkspacesWrite])
        .audited_route("/:docId", delete(delete_view), &[Scope::WorkspacesWrite])
        .into_router()
        // Uploaded HTML project docs exceed Axum's 2 MiB default body limit;
        // raise it for the whole docs router so create_doc/update_doc accept
        // realistic documents instead of stalling the client (spec D21).
        .layer(DefaultBodyLimit::max(DOCS_BODY_LIMIT_BYTES))
}

// ─── Handlers ────────────────────────────────────────────────────────────────

/// Query parameters for `GET /docs`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocListQuery {
    /// Filter to docs with these doc types (repeatable).
    #[serde(default)]
    pub doc_type: Vec<String>,
    /// Filter to future (`true`) or non-future (`false`) docs.
    pub is_future: Option<bool>,
    /// Case-insensitive substring search on doc name.
    pub search: Option<String>,
}

/// `GET /api/v1/workspaces/:workspaceId/docs`
///
/// Lists all docs in a workspace, optionally filtered by doc type, future
/// status, or name search. Returns an empty array (not an error) if no docs
/// exist. No pagination — the number of docs is typically small (< 50).
async fn list_docs(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
    Query(filters): Query<DocListQuery>,
) -> Result<Json<ResponseEnvelope<DocListResponse>>, AppError> {
    let mut db =
        scoped_db_for_workspace(&state, &auth, params.workspace_id, &["workspaces:read"]).await?;
    tracing::debug!(
        workspace_id = %params.workspace_id,
        "Doc endpoint called: list_docs"
    );
    let captured = state
        .doc_service
        .capture_docs_list(&mut db, params.workspace_id)
        .await?;
    let provenance = captured.provenance();
    db.commit().await.map_err(AppError::from)?;
    let live = live_docs_overlay(&state, &auth, params.workspace_id, &captured).await;
    let docs = state
        .doc_service
        .list_docs_with_live_overlay(captured, live)
        .await?;

    // Apply optional filters (§38.357)
    let search_lower = filters.search.as_deref().map(str::to_lowercase);
    let dtos: Vec<DocListItemDTO> = docs
        .into_iter()
        .map(DocListItemDTO::from)
        .filter(|d| {
            if !filters.doc_type.is_empty() && !filters.doc_type.iter().any(|t| t == &d.doc_type) {
                return false;
            }
            if let Some(is_future) = filters.is_future {
                let doc_is_future = d.doc_type.starts_with("future_");
                if doc_is_future != is_future {
                    return false;
                }
            }
            if let Some(ref q) = search_lower {
                if !d.name.to_lowercase().contains(q.as_str()) {
                    return false;
                }
            }
            true
        })
        .collect();
    let (source, repo_oid, snapshot_at) =
        provenance.map_or((None, None, None), |(source, repo_oid, snapshot_at)| {
            (Some(source), Some(repo_oid), Some(snapshot_at.to_rfc3339()))
        });
    Ok(Json(ResponseEnvelope::ok(DocListResponse {
        docs: dtos,
        source,
        repo_oid,
        snapshot_at,
    })))
}

/// `GET /api/v1/workspaces/:workspaceId/docs/:docId`
///
/// Retrieves a single doc by its ID, including staleness fields, linked node
/// IDs, and content (read from git via the file_path).
async fn get_doc(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    Path(params): Path<DocPathParams>,
) -> Result<Json<ResponseEnvelope<DocDetailDTO>>, AppError> {
    tracing::debug!(
        doc_id = %params.doc_id,
        workspace_id = %params.workspace_id,
        "Doc endpoint called: get_doc"
    );

    let mut db = scoped_db_for_doc(&state, &auth, params.doc_id, &["workspaces:read"]).await?;
    ensure_workspace_doc_id(&mut db, params.workspace_id, params.doc_id, "docId").await?;
    let retained = state
        .doc_service
        .get_doc(&mut db, params.workspace_id, params.doc_id)
        .await?;
    let path = retained
        .file_path
        .clone()
        .ok_or_else(|| validation_error("docId", "document has no repository path"))?;
    let captured = state
        .doc_service
        .capture_docs_list(&mut db, params.workspace_id)
        .await?;
    db.commit().await.map_err(AppError::from)?;
    let live = live_doc_read(&state, &auth, params.workspace_id, &captured, &path).await;
    // Projected rows can describe a newer private recovery cut. Public ID
    // reads use the same authorized content-derived metadata as path reads.
    let mut doc = state
        .doc_service
        .get_doc_by_path(captured, path, live)
        .await?;
    doc.id = params.doc_id.to_string();
    doc.is_virtual = false;
    // Historical run markers survive projection retirement; file-derived
    // fields above still come from the authorized source.
    doc.last_uplifted_at = retained.last_uplifted_at;
    doc.last_downlifted_at = retained.last_downlifted_at;
    doc.last_uplifted_commit_sha = retained.last_uplifted_commit_sha;
    doc.last_downlifted_commit_sha = retained.last_downlifted_commit_sha;
    let dto = hydrate_doc_from_authorized_file_source(
        &state,
        &user,
        &auth,
        params.workspace_id,
        doc.into(),
    )
    .await?;
    Ok(Json(ResponseEnvelope::ok(dto)))
}

/// `GET /api/v1/workspaces/:workspaceId/docs/by-path?path=docs/foo.md`
///
/// Retrieves a branch-scoped doc by repo-local path. This opens overlay-only
/// virtual entries returned by the list endpoint.
async fn get_doc_by_path(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
    Query(query): Query<DocByPathQuery>,
) -> Result<Json<ResponseEnvelope<DocDetailDTO>>, AppError> {
    tracing::debug!(
        workspace_id = %params.workspace_id,
        path = %query.path,
        "Doc endpoint called: get_doc_by_path"
    );

    let mut db =
        scoped_db_for_workspace(&state, &auth, params.workspace_id, &["workspaces:read"]).await?;
    let captured = state
        .doc_service
        .capture_docs_list(&mut db, params.workspace_id)
        .await?;
    db.commit().await.map_err(AppError::from)?;
    let live = live_doc_read(&state, &auth, params.workspace_id, &captured, &query.path).await;
    let doc = state
        .doc_service
        .get_doc_by_path(captured, query.path, live)
        .await?;
    let dto = hydrate_doc_from_authorized_file_source(
        &state,
        &user,
        &auth,
        params.workspace_id,
        doc.into(),
    )
    .await?;
    Ok(Json(ResponseEnvelope::ok(dto)))
}

/// `POST /api/v1/workspaces/:workspaceId/docs`
///
/// Creates a new doc in the workspace. Returns 201 Created with the newly
/// created doc wrapped in the standard envelope.
async fn create_doc(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
    Json(body): Json<CreateDocRequest>,
) -> Result<(StatusCode, Json<ResponseEnvelope<DocDetailDTO>>), AppError> {
    tracing::debug!(
        workspace_id = %params.workspace_id,
        doc_name = %body.name,
        "Doc endpoint called: create_doc"
    );

    let user_id = auth.require_user_id()?;

    // Authorize every protected request identifier before using the workspace
    // admission gate. Gate waiters must not retain a tenant-pool connection.
    let mut preflight_db =
        scoped_db_for_workspace(&state, &auth, params.workspace_id, &["workspaces:write"]).await?;
    ensure_workspace_mutable(&mut preflight_db, params.workspace_id, "docs.create").await?;
    preflight_db.rollback().await.map_err(AppError::from)?;

    // Queue only authorized same-workspace creates. Re-open and revalidate after
    // admission so authorization or mutability changes that
    // occurred while waiting fail closed. The service's transaction advisory
    // lock remains the cross-instance destination guard.
    let _request_gate = state
        .doc_service
        .acquire_create_doc_request_gate(params.workspace_id)
        .await;

    let mut db =
        scoped_db_for_workspace(&state, &auth, params.workspace_id, &["workspaces:write"]).await?;
    ensure_workspace_mutable(&mut db, params.workspace_id, "docs.create").await?;
    let initial_content = body.content.unwrap_or_default();
    let service_req = crate::services::doc_service::CreateDocRequest {
        name: body.name,
        doc_type: body.doc_type,
        file_path: body.file_path,
        content: Some(initial_content.clone()),
    };
    let mutation = state
        .doc_service
        .create_doc(&mut db, params.workspace_id, user_id, service_req)
        .await?;
    let doc_uuid = mutation.doc_id;
    let file_path = mutation.file_path.ok_or_else(|| {
        AppError::from_error(g5_contracts::error::G5Error::Internal {
            message: format!("Created doc {doc_uuid} has no canonical repository path"),
        })
    })?;
    let saved = crate::routes::workspaces::write_workspace_file_for_actor(
        &state,
        &user,
        &auth,
        params.workspace_id,
        mutation.repo_id,
        &file_path,
        &initial_content,
        "utf-8",
        None,
        None,
    )
    .await?;
    db.commit().await.map_err(AppError::from)?;

    let mut readback_db =
        scoped_db_for_workspace(&state, &auth, params.workspace_id, &["workspaces:write"]).await?;
    let doc = state
        .doc_service
        .get_doc(&mut readback_db, params.workspace_id, doc_uuid)
        .await?;
    readback_db.commit().await.map_err(AppError::from)?;
    let mut dto: DocDetailDTO = doc.into();
    dto.content = Some(initial_content);
    dto.content_hash = saved
        .get("hash")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);
    dto.draft_dirty = true;
    dto.source = "live".to_string();
    dto.repo_oid = None;
    dto.snapshot_at = None;
    tracing::info!(
        workspace_id = %params.workspace_id,
        doc_id = %dto.id,
        "Doc endpoint succeeded: create_doc"
    );
    state.event_bus.publish(
        &crate::ws::Channel::Workspace(params.workspace_id),
        &crate::ws::WsEvent::DocCreated {
            workspace_id: params.workspace_id,
            doc_id: doc_uuid,
        },
    );

    Ok((StatusCode::CREATED, Json(ResponseEnvelope::ok(dto))))
}

/// `DELETE /api/v1/workspaces/:workspaceId/docs/:docId`
///
/// Deletes a view doc through the pre-existing view-delete API surface.
/// Non-view Project Docs are rejected by the service layer.
async fn delete_view(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    Path(params): Path<DocSyncPathParams>,
) -> Result<StatusCode, AppError> {
    tracing::debug!(
        doc_id = %params.doc_id,
        workspace_id = %params.workspace_id,
        "Doc endpoint called: delete_view"
    );
    let mut db =
        scoped_db_for_workspace(&state, &auth, params.workspace_id, &["workspaces:write"]).await?;
    auth.require_user_id()?;
    ensure_workspace_mutable(&mut db, params.workspace_id, "docs.delete").await?;
    let doc = if let Ok(doc_id) = Uuid::parse_str(&params.doc_id) {
        ensure_workspace_doc_id(&mut db, params.workspace_id, doc_id, "docId").await?;
        let doc = state
            .doc_service
            .get_doc(&mut db, params.workspace_id, doc_id)
            .await?;
        db.rollback().await.map_err(AppError::from)?;
        doc.into()
    } else {
        let (owner, path) = parse_project_doc_handle(&params.doc_id)?;
        if owner.0 != params.workspace_id {
            return Err(validation_error(
                "docId",
                "document handle belongs to another workspace",
            ));
        }
        let captured = state
            .doc_service
            .capture_docs_list(&mut db, params.workspace_id)
            .await?;
        db.rollback().await.map_err(AppError::from)?;
        let live = live_doc_read(&state, &auth, params.workspace_id, &captured, &path).await;
        let doc = state
            .doc_service
            .get_doc_by_path(captured, path, live)
            .await?;
        let dto: DocDetailDTO = doc.into();
        validate_project_doc_owner(owner, &dto)?;
        dto
    };
    DocService::ensure_deletable_view_type(&doc.doc_type)?;
    let file_path = doc.file_path.ok_or_else(|| {
        AppError::from_error(G5Error::Internal {
            message: format!("view {} has no canonical repository path", params.doc_id),
        })
    })?;
    let repo_id = doc
        .repo_id
        .as_deref()
        .and_then(|id| id.parse::<Uuid>().ok())
        .ok_or_else(|| {
            AppError::from_error(G5Error::Internal {
                message: format!("view {} has no repository mapping", params.doc_id),
            })
        })?;

    // Resolve the optimistic hash from the same attached owner that performs
    // the delete. This preserves a concurrent terminal/editor write rather
    // than deleting bytes that changed after the user requested the action.
    let current = crate::routes::workspaces::read_workspace_file_for_actor(
        &state,
        &user,
        &auth,
        params.workspace_id,
        repo_id,
        &file_path,
    )
    .await?;
    crate::routes::workspaces::delete_workspace_file_for_actor(
        &state,
        &user,
        &auth,
        params.workspace_id,
        repo_id,
        &file_path,
        Some(&current.hash),
    )
    .await?;

    let mut db =
        scoped_db_for_workspace(&state, &auth, params.workspace_id, &["workspaces:write"]).await?;
    ensure_workspace_mutable(&mut db, params.workspace_id, "docs.delete").await?;
    state
        .doc_service
        .delete_view_metadata(&mut db, params.workspace_id, repo_id, &file_path)
        .await?;

    tracing::info!(
        doc_id = %params.doc_id,
        "Doc endpoint succeeded: delete_view"
    );
    db.commit().await.map_err(AppError::from)?;

    state.event_bus.publish(
        &crate::ws::Channel::Workspace(params.workspace_id),
        &crate::ws::WsEvent::DocDeleted {
            workspace_id: params.workspace_id,
            doc_id: params.doc_id.to_string(),
        },
    );

    Ok(StatusCode::NO_CONTENT)
}

/// `PUT /api/v1/workspaces/:workspaceId/docs/project-docs/:docId`
///
/// Rewrites a non-view Project Doc through the repository-derived handle
/// returned by the current Docs reader. Name and type stay derived from the
/// effective path/frontmatter; projection-only mutation fields are rejected.
async fn update_project_doc(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    Path(params): Path<DocSyncPathParams>,
    Json(body): Json<UpdateDocRequest>,
) -> Result<Json<ResponseEnvelope<DocDetailDTO>>, AppError> {
    tracing::debug!(
        doc_id = %params.doc_id,
        workspace_id = %params.workspace_id,
        "Doc endpoint called: update_project_doc"
    );
    if body.name.is_some() || body.doc_type.is_some() {
        return Err(validation_error(
            "projectDoc",
            "current Project Docs rewrite accepts content only; name and type are derived from the repository file",
        ));
    }
    let content = body.content.ok_or_else(|| {
        validation_error("content", "current Project Docs rewrite requires content")
    })?;

    let (owner, file_path) = parse_project_doc_handle(&params.doc_id)?;
    let mut db =
        scoped_db_for_workspace(&state, &auth, params.workspace_id, &["workspaces:write"]).await?;
    let _user_id = auth.require_user_id()?;
    ensure_workspace_mutable(&mut db, params.workspace_id, "docs.update_project_doc").await?;
    let captured = state
        .doc_service
        .capture_docs_list(&mut db, params.workspace_id)
        .await?;
    db.commit().await.map_err(AppError::from)?;
    let live = live_doc_read(&state, &auth, params.workspace_id, &captured, &file_path).await;
    let doc = state
        .doc_service
        .get_doc_by_path(captured, file_path.clone(), live)
        .await?;
    let mut dto: DocDetailDTO = doc.into();
    validate_project_doc_owner(owner, &dto)?;
    let repo_id = dto
        .repo_id
        .as_deref()
        .ok_or_else(|| validation_error("repoId", "document has no repository mapping"))?
        .parse::<Uuid>()
        .map_err(|_| validation_error("repoId", "document repository mapping is invalid"))?;
    let saved = crate::routes::workspaces::write_workspace_file_and_maybe_push(
        &state,
        &user,
        &auth,
        params.workspace_id,
        repo_id,
        &file_path,
        &content,
        "utf-8",
        body.expected_hash.as_deref(),
        body.expected_vm_id,
        body.commit_and_push,
    )
    .await?;
    dto.content = Some(content);
    dto.content_hash = saved
        .get("hash")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);
    dto.vm_id = saved
        .get("vmId")
        .and_then(serde_json::Value::as_str)
        .and_then(|id| id.parse().ok());
    dto.draft_dirty = true;
    dto.source = "live".to_string();
    dto.repo_oid = None;
    dto.snapshot_at = None;

    tracing::info!(
        doc_id = %params.doc_id,
        "Doc endpoint succeeded: update_project_doc"
    );
    state.event_bus.publish(
        &crate::ws::Channel::Workspace(params.workspace_id),
        &crate::ws::WsEvent::DocContentChanged {
            workspace_id: params.workspace_id,
            doc_id: params.doc_id,
        },
    );

    Ok(Json(ResponseEnvelope::ok(dto)))
}

/// `DELETE /api/v1/workspaces/:workspaceId/docs/project-docs/:docId`
///
/// Deletes a non-view Project Doc and its associated file/data.
/// View-type docs are protected from this Project Docs delete path.
async fn delete_project_doc(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    Path(params): Path<DocSyncPathParams>,
) -> Result<StatusCode, AppError> {
    tracing::debug!(
        doc_id = %params.doc_id,
        workspace_id = %params.workspace_id,
        "Doc endpoint called: delete_project_doc"
    );
    let (owner, file_path) = parse_project_doc_handle(&params.doc_id)?;
    let mut db =
        scoped_db_for_workspace(&state, &auth, params.workspace_id, &["workspaces:write"]).await?;
    let _user_id = auth.require_user_id()?;
    ensure_workspace_mutable(&mut db, params.workspace_id, "docs.delete_project_doc").await?;

    let captured = state
        .doc_service
        .capture_docs_list(&mut db, params.workspace_id)
        .await?;
    db.commit().await.map_err(AppError::from)?;
    let live = live_doc_read(&state, &auth, params.workspace_id, &captured, &file_path).await;
    let doc = state
        .doc_service
        .get_doc_by_path(captured, file_path.clone(), live)
        .await?;
    // Design v9 §9.2: a delete carries the hash of the file it removes, and
    // the guest refuses one without it. The handle names the doc, not a
    // version, so the current live hash is read through the same authorized
    // file boundary the by-path read uses.
    let dto = hydrate_doc_from_authorized_file_source(
        &state,
        &user,
        &auth,
        params.workspace_id,
        doc.into(),
    )
    .await?;
    validate_project_doc_owner(owner, &dto)?;
    let repo_id = dto
        .repo_id
        .as_deref()
        .ok_or_else(|| validation_error("repoId", "document has no repository mapping"))?
        .parse::<Uuid>()
        .map_err(|_| validation_error("repoId", "document repository mapping is invalid"))?;
    crate::routes::workspaces::delete_workspace_file_for_actor(
        &state,
        &user,
        &auth,
        params.workspace_id,
        repo_id,
        &file_path,
        dto.content_hash.as_deref(),
    )
    .await?;
    let event_doc_id = params.doc_id.clone();

    tracing::info!(
        doc_id = %params.doc_id,
        "Doc endpoint succeeded: delete_project_doc"
    );
    state.event_bus.publish(
        &crate::ws::Channel::Workspace(params.workspace_id),
        &crate::ws::WsEvent::DocDeleted {
            workspace_id: params.workspace_id,
            doc_id: event_doc_id,
        },
    );

    Ok(StatusCode::NO_CONTENT)
}

/// `GET /api/v1/workspaces/:workspaceId/docs/views`
///
/// List all view-type documents in a workspace.
async fn list_views_handler(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
) -> Result<Json<ResponseEnvelope<Vec<ViewListItemResponseDTO>>>, AppError> {
    let mut db =
        scoped_db_for_workspace(&state, &auth, params.workspace_id, &["workspaces:read"]).await?;
    let captured = state
        .doc_service
        .capture_docs_list(&mut db, params.workspace_id)
        .await?;
    db.commit().await.map_err(AppError::from)?;
    let views = state.doc_service.list_views(captured).await?;
    let dtos: Vec<ViewListItemResponseDTO> = views
        .into_iter()
        .map(|v| ViewListItemResponseDTO {
            id: v.id,
            name: v.name,
            doc_type: v.doc_type,
            file_path: v.file_path,
            is_future: v.is_future,
        })
        .collect();
    Ok(Json(ResponseEnvelope::ok(dtos)))
}

/// `GET /api/v1/workspaces/:workspaceId/docs/views/:viewId`
///
/// Get a single view document by ID. Rejects non-view docs.
async fn get_view_handler(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    Path(params): Path<ViewPathParams>,
) -> Result<Json<ResponseEnvelope<DocDetailDTO>>, AppError> {
    let mut db = scoped_db_for_doc(&state, &auth, params.view_id, &["workspaces:read"]).await?;
    ensure_workspace_doc_id(&mut db, params.workspace_id, params.view_id, "viewId").await?;
    let view = state
        .doc_service
        .get_view(&mut db, params.workspace_id, params.view_id)
        .await?;
    let path = view
        .doc
        .file_path
        .clone()
        .ok_or_else(|| validation_error("viewId", "view has no repository path"))?;
    let captured = state
        .doc_service
        .capture_docs_list(&mut db, params.workspace_id)
        .await?;
    db.commit().await.map_err(AppError::from)?;
    let live = live_doc_read(&state, &auth, params.workspace_id, &captured, &path).await;
    let mut doc = state
        .doc_service
        .get_doc_by_path(captured, path, live)
        .await?;
    doc.id = params.view_id.to_string();
    doc.is_virtual = false;
    doc.last_uplifted_at = view.doc.last_uplifted_at;
    doc.last_downlifted_at = view.doc.last_downlifted_at;
    doc.last_uplifted_commit_sha = view.doc.last_uplifted_commit_sha;
    doc.last_downlifted_commit_sha = view.doc.last_downlifted_commit_sha;
    let dto = hydrate_doc_from_authorized_file_source(
        &state,
        &user,
        &auth,
        params.workspace_id,
        doc.into(),
    )
    .await?;
    Ok(Json(ResponseEnvelope::ok(dto)))
}

/// `PUT /api/v1/workspaces/:workspaceId/docs/:docId`
///
/// Updates a doc's name and/or content. All fields are optional — this is
/// a partial update. Only the provided fields are modified.
async fn update_doc(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    Path(params): Path<DocPathParams>,
    Json(body): Json<UpdateDocRequest>,
) -> Result<Json<ResponseEnvelope<DocDetailDTO>>, AppError> {
    tracing::debug!(
        doc_id = %params.doc_id,
        workspace_id = %params.workspace_id,
        "Doc endpoint called: update_doc"
    );

    if body.name.is_some() || body.doc_type.is_some() {
        return Err(validation_error(
            "doc",
            "document name and type are derived from the repository file; save content through the mapped file",
        ));
    }
    let content = body
        .content
        .as_deref()
        .ok_or_else(|| validation_error("content", "content is required"))?;
    let mut db = scoped_db_for_doc(&state, &auth, params.doc_id, &["workspaces:read"]).await?;
    ensure_workspace_doc_id(&mut db, params.workspace_id, params.doc_id, "docId").await?;
    let doc = state
        .doc_service
        .get_doc(&mut db, params.workspace_id, params.doc_id)
        .await?;
    db.commit().await.map_err(AppError::from)?;
    let mut dto: DocDetailDTO = doc.into();
    let path = dto
        .file_path
        .as_deref()
        .ok_or_else(|| validation_error("filePath", "document has no repository file mapping"))?;
    let repo_id = dto
        .repo_id
        .as_deref()
        .ok_or_else(|| validation_error("repoId", "document has no repository mapping"))?
        .parse::<Uuid>()
        .map_err(|_| validation_error("repoId", "document repository mapping is invalid"))?;
    let saved = crate::routes::workspaces::write_workspace_file_and_maybe_push(
        &state,
        &user,
        &auth,
        params.workspace_id,
        repo_id,
        path,
        content,
        "utf-8",
        body.expected_hash.as_deref(),
        body.expected_vm_id,
        body.commit_and_push,
    )
    .await?;
    dto.content = Some(content.to_string());
    dto.content_hash = saved
        .get("hash")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);
    dto.vm_id = saved
        .get("vmId")
        .and_then(serde_json::Value::as_str)
        .and_then(|id| id.parse().ok());
    dto.draft_dirty = true;
    dto.source = "live".to_string();
    dto.repo_oid = None;
    dto.snapshot_at = None;
    tracing::info!(
        doc_id = %params.doc_id,
        "Doc endpoint succeeded: update_doc"
    );
    state.event_bus.publish(
        &crate::ws::Channel::Workspace(params.workspace_id),
        &crate::ws::WsEvent::DocContentChanged {
            workspace_id: params.workspace_id,
            doc_id: params.doc_id.to_string(),
        },
    );
    Ok(Json(ResponseEnvelope::ok(dto)))
}

fn validation_error(field: &str, message: &str) -> AppError {
    AppError::from(G5Error::ValidationError {
        details: serde_json::json!({
            "field": field,
            "message": message,
        }),
    })
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Extracts the `user_id` from `AuthContext::User`. Returns `Forbidden`
/// error if the auth context is a worker JWT (workers cannot perform
/// doc operations via the public API).
/// The attached owner's live docs surface (design v9 §9.1) for the docs
/// wrappers, or `None` for every other caller. Runs after the capture
/// transaction has committed, like every other effective read.
async fn live_docs_overlay(
    state: &Arc<AppState>,
    auth: &AuthContext,
    workspace_id: Uuid,
    captured: &crate::services::doc_service::CapturedDocsList,
) -> Option<crate::services::doc_service::LiveDocsOverlay> {
    let actor_user_id = auth.require_user_id().ok()?;
    state
        .doc_service
        .live_docs_overlay(
            &state.workspace_runtime,
            actor_user_id,
            workspace_id,
            captured,
        )
        .await
}

/// One docs-surface path as the attached owner's running checkout holds it
/// (design v9 §9.1); `Unavailable` for every other caller.
async fn live_doc_read(
    state: &Arc<AppState>,
    auth: &AuthContext,
    workspace_id: Uuid,
    captured: &crate::services::doc_service::CapturedDocsList,
    path: &str,
) -> crate::services::doc_service::LiveDocRead {
    let Ok(actor_user_id) = auth.require_user_id() else {
        return crate::services::doc_service::LiveDocRead::Unavailable;
    };
    state
        .doc_service
        .live_doc_read(
            &state.workspace_runtime,
            actor_user_id,
            workspace_id,
            captured,
            path,
        )
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_router_creates_without_panic() {
        // Verify the router can be constructed (validates route patterns)
        let _router = router();
    }
}
