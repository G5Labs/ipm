//! File tree route handlers for the File Tree API.
//!
//! This module implements the HTTP route handlers for the File Tree API
//! (API_AND_STORAGE.md §3.14). The branch routes are compatibility aliases for
//! the workspace-owned file facade:
//!
//! 1. `GET /api/v1/workspaces/:workspaceId/repos/:repoId/files` — List the effective exact-cut tree
//! 2. `GET /api/v1/workspaces/:workspaceId/repos/:repoId/files/*path` — Get effective exact-cut content
//! 3. `PUT /api/v1/workspaces/:workspaceId/repos/:repoId/files/*path` — Save live workspace content
//! 4. `DELETE /api/v1/workspaces/:workspaceId/repos/:repoId/files/*path` — Delete live workspace content
//!
//! Current reads capture the branch's typed, complete authority cut, whose
//! membership comes from the database and whose head SHA for every member is
//! the branch ref `refs/heads/<branch>` in that repository's internal mirror.
//! Ordinary workspace reads delegate to the owner-aware workspace facade.
//! Retained transient output branches use their captured committed binding;
//! checkouts and mutable `head_commit` columns never select the version.
//!
//! All handlers follow the thin handler pattern: extract request parameters,
//! delegate to the workspace service (for branch lookup) and a git utility
//! layer (for file tree/content retrieval), and format the response using
//! `ResponseEnvelope`. No business logic resides in the handlers.

use std::collections::BTreeSet;
use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{delete, get, put},
    Json, Router,
};
use g5_authz::ScopedDb;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db::scoped::scoped_db_for_workspace_repo;
use crate::errors::AppError;
use crate::middleware::{AuthContext, AuthenticatedUser};
use crate::models::workspace::WorkspaceRepositoryState;
use crate::routes::audited::AuditedRouter;
use crate::routes::ResponseEnvelope;
use crate::services::workspace_reader::RepoReadView;
use crate::services::workspace_runtime::{runtime_to_g5, RuntimeRequest, WorkspaceRuntimeHandle};
use crate::services::AppState;
use g5_auth::Scope;
use g5_contracts::error::G5Error;

pub use g5_api_foundation::contracts::files::{
    CommitDiffDTO, CommitHistoryEntryDTO, CommitHistoryPageDTO, CommitSummaryDTO, FileTreeEntry,
};

// ─── Path Parameter Types ────────────────────────────────────────────────────

/// Path parameters for `GET /api/v1/workspaces/:workspaceId/repos/:repoId/files`.
#[derive(Debug, Deserialize)]
pub struct BranchPathParams {
    #[serde(rename = "workspaceId")]
    pub workspace_id: Uuid,
    #[serde(rename = "repoId")]
    pub repo_id: Uuid,
}

/// Path parameters for `GET /api/v1/workspaces/:workspaceId/repos/:repoId/files/*path`.
#[derive(Debug, Deserialize)]
pub struct FilePathParams {
    #[serde(rename = "workspaceId")]
    pub workspace_id: Uuid,
    #[serde(rename = "repoId")]
    pub repo_id: Uuid,
    pub path: String,
}

#[derive(Debug, Deserialize)]
pub struct CommitPathParams {
    #[serde(rename = "workspaceId")]
    pub workspace_id: Uuid,
    #[serde(rename = "repoId")]
    pub repo_id: Uuid,
    pub sha: String,
}

#[derive(Debug, Deserialize)]
pub struct CommitFilePathParams {
    #[serde(rename = "workspaceId")]
    pub workspace_id: Uuid,
    #[serde(rename = "repoId")]
    pub repo_id: Uuid,
    pub sha: String,
    pub path: String,
}

// ─── Query Parameters ────────────────────────────────────────────────────────

/// Default path for the file tree query (repo root).
fn default_path() -> String {
    "/".to_string()
}

/// Default recursion depth (immediate children only).
fn default_depth() -> i32 {
    1
}

/// Query parameters for `GET /api/v1/workspaces/:workspaceId/repos/:repoId/files`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileTreeQuery {
    /// Subdirectory to list (e.g., "src/engine"). Defaults to "/" (repo root).
    #[serde(default = "default_path")]
    pub path: String,

    /// Max depth of recursion. 1 = immediate children only, -1 = full recursive.
    /// Defaults to 1.
    #[serde(default = "default_depth")]
    pub depth: i32,

    #[serde(default)]
    pub task_id: Option<Uuid>,
}

/// File-tree entries plus the exact source cut used to enumerate them.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileTreeResponse {
    pub entries: Vec<FileTreeEntry>,
    /// The listing stopped at the tree's entry or byte cap
    /// (`FILE_TREE_ENTRY_CAP`, `FILE_TREE_BYTE_CAP`): `entries` holds what
    /// fit, and the panel says the subtree has more.
    pub truncated: bool,
    pub source: String,
    /// The commit the entries belong to: the committed cut's head, or the
    /// checkout HEAD the VM read beside a live listing. `null` for a live
    /// listing whose checkout HEAD the VM could not read: no OID rather
    /// than a wrong one (`g5.documents.read.labels`).
    pub repo_oid: Option<String>,
    pub snapshot_at: String,
    /// Why the attached owner got the committed cut instead of the live
    /// checkout (`"entry_cap"`), so the panel can say so; `null` otherwise.
    pub live_fallback: Option<String>,
}

/// Default number of commits returned by the branch-commits endpoint.
fn default_commit_limit() -> usize {
    30
}

/// Maximum number of commits the branch-commits endpoint will return.
const MAX_COMMIT_LIMIT: usize = 100;

/// Query parameters for `GET /api/v1/workspaces/:workspaceId/repos/:repoId/commits`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchCommitsQuery {
    /// Max number of commits to return. Defaults to 30, capped at 100.
    #[serde(default = "default_commit_limit")]
    pub limit: usize,
}

fn default_history_limit() -> usize {
    10
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitHistoryQuery {
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default = "default_history_limit")]
    pub limit: usize,
}

/// Request body for `PUT /api/v1/workspaces/:workspaceId/repos/:repoId/files/*path`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileWriteRequest {
    pub content: String,
    #[serde(default = "default_file_encoding")]
    pub encoding: String,
    #[serde(default)]
    pub expected_hash: Option<String>,
}

fn default_file_encoding() -> String {
    "utf-8".to_string()
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDeleteRequest {
    #[serde(default)]
    pub expected_hash: Option<String>,
}

// ─── Response DTOs ───────────────────────────────────────────────────────────

/// File content response for `GET /api/v1/workspaces/:workspaceId/repos/:repoId/files/*path`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileContentResponse {
    /// The file path relative to the repo root.
    pub path: String,

    /// The UTF-8 file content. For binary files, this is base64-encoded.
    pub content: String,

    /// File size in bytes.
    pub size: i64,

    /// Content encoding: "utf-8" for text files, "base64" for binary files.
    pub encoding: String,

    /// `true` when the retained output reader returned live, uncommitted bytes.
    /// Ordinary workspace reads expose the more precise `source` field.
    pub draft_dirty: bool,
    pub hash: Option<String>,
    pub source: String,
    pub repo_oid: Option<String>,
    pub snapshot_at: Option<String>,
}

// ─── Router ──────────────────────────────────────────────────────────────────

/// File tree routes nested under `/api/v1/workspaces/:workspaceId/repos/:repoId/files`.
///
/// Endpoints:
/// - `GET /` — list file tree at HEAD commit
/// - `GET /*path` — get file content at HEAD commit
/// - `PUT /*path` — save file content through the attached-owner facade
/// - `DELETE /*path` — delete a file or folder through the attached-owner facade
pub fn router() -> Router<Arc<AppState>> {
    AuditedRouter::new()
        .audited_route("/", get(list_file_tree), &[Scope::WorkspacesRead])
        .audited_route("/*path", get(get_file_content), &[Scope::WorkspacesRead])
        .audited_route("/*path", put(put_file_content), &[Scope::WorkspacesWrite])
        .audited_route(
            "/*path",
            delete(delete_file_or_folder),
            &[Scope::WorkspacesWrite],
        )
        .into_router()
}

/// Branch commits route nested under `/api/v1/workspaces/:workspaceId/repos/:repoId/commits`.
///
/// Endpoint:
/// - `GET /` — list commits on the branch (newest first)
pub fn commits_router() -> Router<Arc<AppState>> {
    AuditedRouter::new()
        .audited_route("/", get(list_branch_commits), &[Scope::WorkspacesRead])
        .audited_route(
            "/history",
            get(list_branch_commit_history),
            &[Scope::WorkspacesRead],
        )
        .audited_route("/:sha/diff", get(get_commit_diff), &[Scope::WorkspacesRead])
        .audited_route(
            "/:sha/diff/files/*path",
            get(get_commit_file_diff),
            &[Scope::WorkspacesRead],
        )
        .into_router()
}

// ─── Handlers ────────────────────────────────────────────────────────────────

async fn resolve_branch_repo_read(
    state: &AppState,
    db: &mut ScopedDb,
    branch: &WorkspaceRepositoryState,
) -> Result<RepoReadView, AppError> {
    let target = g5_contracts::types::AuthorityTarget::Workspace {
        workspace_id: branch.workspace_id,
    };
    let snapshot = crate::services::workspace_reader::resolve_published_snapshot(
        db,
        &state.repo_storage,
        target,
    )
    .await
    .map_err(AppError::from_error)?;
    RepoReadView::from_committed_snapshot(&*db, &snapshot, branch.repo_id)
        .map_err(AppError::from_error)
}

/// `GET /api/v1/workspaces/:workspaceId/repos/:repoId/files`
///
/// Lists the file tree at the branch's HEAD commit. Returns a tree structure
/// of files and directories, optionally filtered by subdirectory path and
/// recursion depth. Used by the UI file explorer panel to browse repository
/// contents.
///
/// The attached owner reads the running checkout instead (`source: "live"`),
/// which lists untracked work as well. The VM lists the subtree in one
/// `fs.tree` call that honors the checkout's `.gitignore` files, so dependency
/// and build output (`node_modules/`, `target/`, …) is neither listed nor
/// walked, while tracked paths stay visible as in Git
/// (`g5.documents.read.live-tree`). A live checkout that still exceeds
/// [`LIVE_FILE_TREE_ENTRY_CAP`] entries, or a VM whose dispatcher predates
/// the call, is served from the committed cut with the reason in
/// `liveFallback` rather than failing the panel.
///
/// Returns an empty `data` array if the path is empty or doesn't exist
/// (not an error — an empty directory returns `[]`).
async fn list_file_tree(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<BranchPathParams>,
    Query(query): Query<FileTreeQuery>,
) -> Result<Json<ResponseEnvelope<FileTreeResponse>>, AppError> {
    // Ontology is optional in each repository. Its authorized discovery root
    // may be absent in a live checkout, just as in the committed file tree.
    // Other missing paths retain the normal Files error behavior.
    let missing_root_is_empty = query.path.trim_matches('/') == ".g5/ontology";
    read_file_tree_for_actor(&state, &auth, params, query, missing_root_is_empty).await
}

/// Shared actor-aware listing. Ontology discovery uses exactly the Files source policy.
pub(crate) async fn read_file_tree_for_actor(
    state: &Arc<AppState>,
    auth: &AuthContext,
    params: BranchPathParams,
    query: FileTreeQuery,
    missing_root_is_empty: bool,
) -> Result<Json<ResponseEnvelope<FileTreeResponse>>, AppError> {
    let db = scoped_db_for_workspace_repo(
        state,
        auth,
        params.workspace_id,
        params.repo_id,
        &["workspaces:read"],
    )
    .await?;
    tracing::debug!(
        workspace_id = %params.workspace_id,
        repo_id = %params.repo_id,
        path = %query.path,
        depth = query.depth,
        "File tree endpoint called: list_file_tree"
    );

    // 1. Validate auth — reject service tokens
    let user_claims = extract_user_claims(auth)?;
    let actor_user_id = Uuid::parse_str(&user_claims.sub)
        .map_err(|_| AppError::from_error(G5Error::Unauthorized))?;
    read_file_tree_as(
        state,
        db,
        actor_user_id,
        params,
        query,
        missing_root_is_empty,
    )
    .await
}

/// The listing of [`read_file_tree_for_actor`] on a read scope the caller
/// opened, for `actor_user_id`: the live checkout when it is that user's
/// running workspace, otherwise the committed cut. The workspace agent lists
/// through it as its owner (`g5.security.agent-read-scope`).
pub(crate) async fn read_file_tree_as(
    state: &Arc<AppState>,
    mut db: ScopedDb,
    actor_user_id: Uuid,
    params: BranchPathParams,
    query: FileTreeQuery,
    missing_root_is_empty: bool,
) -> Result<Json<ResponseEnvelope<FileTreeResponse>>, AppError> {
    // 2. Look up branch
    let branch = state
        .workspace_service
        .get_workspace_repository(&mut db, params.workspace_id, params.repo_id)
        .await?;
    let tree_path = query.path.trim_start_matches('/').to_string();
    let live_handle = state
        .workspace_runtime
        .get(branch.workspace_id)
        .await?
        .filter(|handle| handle.status == "running" && handle.owner_user_id == actor_user_id);
    // The committed cut is captured in the authorization transaction for
    // every reader: it is what everyone but the attached owner sees, the
    // owner's tracked-path evidence, and the owner's fallback.
    let read = resolve_branch_repo_read(state.as_ref(), &mut db, &branch).await?;
    db.commit().await.map_err(AppError::from)?;
    let mut live_fallback = None;
    if let Some(handle) = live_handle {
        // Git never ignores a tracked path, so the committed cut's paths stay
        // visible when a `.gitignore` rule matches them. Paths committed in
        // the VM but not yet flushed are unknown here and follow the rules.
        let tracked = state
            .workspace_service
            .committed_tracked_paths(&read, &tree_path, LIVE_FILE_TREE_ENTRY_CAP)
            .await?;
        let listed = list_live_file_tree(
            state,
            &handle,
            actor_user_id,
            branch.repo_id,
            &tracked,
            LiveFileTreeOptions {
                path: &tree_path,
                depth: query.depth,
                missing_root_is_empty,
            },
        )
        .await?;
        match listed {
            LiveFileTree::TooLarge => {
                tracing::warn!(
                    workspace_id = %branch.workspace_id,
                    repo_id = %branch.repo_id,
                    "live file tree exceeds {LIVE_FILE_TREE_ENTRY_CAP} entries; serving the committed cut"
                );
                live_fallback = Some(LIVE_FALLBACK_ENTRY_CAP);
            }
            LiveFileTree::RuntimeOutdated => {
                tracing::warn!(
                    workspace_id = %branch.workspace_id,
                    vm_id = %handle.vm_id,
                    "workspace runtime predates fs.tree; serving the committed cut"
                );
                live_fallback = Some(LIVE_FALLBACK_RUNTIME_OUTDATED);
            }
            LiveFileTree::Listed { entries, head_oid } => {
                // The VM read its checkout's HEAD with the listing, so the
                // tree never waits on a Changes run (git status) that an
                // agent's modules may be starving. A checkout whose `.git`
                // the VM could not read is labeled with no OID: the durable
                // head may not describe the live checkout.
                if head_oid.is_none() {
                    tracing::debug!(
                        workspace_id = %branch.workspace_id,
                        repo_id = %branch.repo_id,
                        "live file tree carried no checkout HEAD; served without an OID label"
                    );
                }
                let repo_oid = head_oid;
                return Ok(Json(ResponseEnvelope::ok(FileTreeResponse {
                    entries,
                    truncated: false,
                    source: "live".to_string(),
                    repo_oid,
                    snapshot_at: chrono::Utc::now().to_rfc3339(),
                    live_fallback: None,
                })));
            }
        }
    }
    committed_file_tree(state, read, &tree_path, query.depth, live_fallback).await
}

/// Enumerate the captured committed cut. Repository enumeration begins only
/// after the authorization/cut/overlay capture transaction has closed. The
/// walk keeps to the live tree's caps and the reply says when it did.
async fn committed_file_tree(
    state: &AppState,
    read: RepoReadView,
    tree_path: &str,
    depth: i32,
    live_fallback: Option<&str>,
) -> Result<Json<ResponseEnvelope<FileTreeResponse>>, AppError> {
    let listing = state
        .workspace_service
        .list_effective_file_tree(&read, tree_path, depth)
        .await?;
    if listing.truncated {
        tracing::debug!(
            workspace_id = %read.repo.workspace_id,
            repo_id = %read.repo.repo_id,
            path = %tree_path,
            "committed file tree exceeds {LIVE_FILE_TREE_ENTRY_CAP} entries or {LIVE_FILE_TREE_BYTE_CAP} entry bytes; serving the listed part"
        );
    }
    Ok(Json(ResponseEnvelope::ok(FileTreeResponse {
        entries: listing.entries,
        truncated: listing.truncated,
        source: read.snapshot_source().as_str().to_string(),
        repo_oid: Some(read.repo.head_commit.clone()),
        snapshot_at: read.snapshot_at().to_rfc3339(),
        live_fallback: live_fallback.map(str::to_string),
    })))
}

/// The most entries one live tree carries. A checkout beyond it is served
/// from the committed cut instead, which keeps to the same cap
/// (`workspace_reader::FILE_TREE_LIMITS`) and reports `truncated`.
const LIVE_FILE_TREE_ENTRY_CAP: usize = crate::services::workspace_reader::FILE_TREE_ENTRY_CAP;

/// The most bytes of entries one live tree reply is budgeted at
/// (`fs.tree` `max_bytes`). The runtime accepts replies up to 4 MiB
/// (`MAX_RUNTIME_REPLY_BYTES`); a budget of 3 MiB leaves the envelope and
/// the per-entry estimate's slack under it, so a checkout with long paths
/// still gets the committed cut instead of a transport failure.
const LIVE_FILE_TREE_BYTE_CAP: usize = crate::services::workspace_reader::FILE_TREE_BYTE_CAP;

/// `FileTreeResponse::live_fallback` when a cap replaced the live tree.
const LIVE_FALLBACK_ENTRY_CAP: &str = "entry_cap";

/// `FileTreeResponse::live_fallback` when the VM's dispatcher predates
/// `fs.tree` and cannot list the checkout in one call.
const LIVE_FALLBACK_RUNTIME_OUTDATED: &str = "runtime_outdated";

/// The JSON-RPC code a dispatcher answers for a method it does not know.
const RPC_METHOD_NOT_FOUND: i64 = -32601;

/// A live tree listing's result.
enum LiveFileTree {
    Listed {
        entries: Vec<FileTreeEntry>,
        /// The checkout's HEAD as the VM read it; `None` when its `.git`
        /// layout is not an ordinary clone.
        head_oid: Option<String>,
    },
    /// The checkout has more than [`LIVE_FILE_TREE_ENTRY_CAP`] entries Git
    /// does not ignore.
    TooLarge,
    /// The VM's dispatcher does not implement `fs.tree` yet.
    RuntimeOutdated,
}

fn live_tree_request(
    handle: &WorkspaceRuntimeHandle,
    actor_user_id: Uuid,
    path: &str,
) -> RuntimeRequest {
    RuntimeRequest::owner(
        handle,
        actor_user_id,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .saturating_add(30_000) as u64,
        format!("file-tree:{}:{}", Uuid::new_v4(), path),
    )
}

struct LiveFileTreeOptions<'a> {
    path: &'a str,
    depth: i32,
    missing_root_is_empty: bool,
}

/// List the attached owner's checkout from `path` in one `fs.tree` call.
/// The VM honors the checkout's `.gitignore` files, keeps `tracked` paths as
/// Git does, and stops past [`LIVE_FILE_TREE_ENTRY_CAP`] entries. A tracked
/// set already past the cap cannot fit either, so no call is made for it.
async fn list_live_file_tree(
    state: &Arc<AppState>,
    handle: &WorkspaceRuntimeHandle,
    actor_user_id: Uuid,
    repo_id: Uuid,
    tracked: &g5_scm::TreePaths,
    options: LiveFileTreeOptions<'_>,
) -> Result<LiveFileTree, AppError> {
    let LiveFileTreeOptions {
        path,
        depth,
        missing_root_is_empty,
    } = options;
    if tracked.truncated {
        return Ok(LiveFileTree::TooLarge);
    }
    let tracked = tracked.paths.iter().cloned().collect::<BTreeSet<_>>();
    let request = live_tree_request(handle, actor_user_id, path);
    let value = match state
        .workspace_runtime
        .file_tree(
            handle,
            &request,
            repo_id,
            path,
            depth,
            &tracked,
            LIVE_FILE_TREE_ENTRY_CAP,
            LIVE_FILE_TREE_BYTE_CAP,
        )
        .await
    {
        Ok(value) => value,
        Err(error) if missing_root_is_empty && error.message == "FILE_MISSING" => {
            return Ok(LiveFileTree::Listed {
                entries: Vec::new(),
                head_oid: None,
            });
        }
        Err(error) if error.code == RPC_METHOD_NOT_FOUND => {
            return Ok(LiveFileTree::RuntimeOutdated);
        }
        // The watch on the root lists every directory below it; one wider
        // than the guest lists is past the tree's cap just as the walk
        // reports it (`TooWide`).
        Err(error) if error.message == "DIRECTORY_LIMIT_EXCEEDED" => {
            return Ok(LiveFileTree::TooLarge);
        }
        Err(error) => return Err(AppError::from_error(runtime_to_g5(error))),
    };
    if value["truncated"].as_bool() == Some(true) {
        return Ok(LiveFileTree::TooLarge);
    }
    let listed = value["entries"].as_array().ok_or_else(|| {
        AppError::from_error(G5Error::Internal {
            message: "workspace fs.tree returned an invalid entry vector".to_string(),
        })
    })?;
    let mut entries = Vec::with_capacity(listed.len());
    for entry in listed {
        let entry_path = entry["path"].as_str().ok_or_else(|| {
            AppError::from_error(G5Error::Internal {
                message: "workspace fs.tree returned an entry without a path".to_string(),
            })
        })?;
        entries.push(g5_storage::StorageEntry {
            path: entry_path.to_string(),
            is_dir: entry["type"].as_str() == Some("directory"),
            size: entry["size"].as_u64(),
            last_modified: None,
        });
    }
    Ok(LiveFileTree::Listed {
        entries: crate::services::WorkspaceService::render_file_tree_entries(entries, path),
        head_oid: value["head_oid"].as_str().map(str::to_owned),
    })
}

/// `GET /api/v1/workspaces/:workspaceId/repos/:repoId/commits`
///
/// Lists the commits on the branch's working history, newest first. Walks from
/// the branch's HEAD commit and stops at the branch's divergence point from its
/// base branch, so the result is the branch's own commits. Used by the preview
/// popover to show "New commits in this workspace" when previewing in unmerged
/// mode.
///
/// Accepts an optional `?limit=` query (default 30, capped at 100).
async fn list_branch_commits(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<BranchPathParams>,
    Query(query): Query<BranchCommitsQuery>,
) -> Result<Json<ResponseEnvelope<Vec<CommitSummaryDTO>>>, AppError> {
    let mut db = scoped_db_for_workspace_repo(
        &state,
        &auth,
        params.workspace_id,
        params.repo_id,
        &["workspaces:read"],
    )
    .await?;
    tracing::debug!(
        workspace_id = %params.workspace_id,
        repo_id = %params.repo_id,
        limit = query.limit,
        "Branch commits endpoint called: list_branch_commits"
    );

    // 1. Validate auth — reject service tokens
    let _user_claims = extract_user_claims(&auth)?;

    // 2. Look up branch
    let branch = state
        .workspace_service
        .get_workspace_repository(&mut db, params.workspace_id, params.repo_id)
        .await?;

    // 3. Clamp the requested limit to the allowed maximum.
    let limit = query.limit.clamp(1, MAX_COMMIT_LIMIT);

    // 4. Delegate to the workspace service to walk the branch history.
    let commits = state
        .workspace_service
        .list_branch_commits(&mut db, &branch, limit)
        .await?;

    db.commit().await.map_err(AppError::from)?;

    Ok(Json(ResponseEnvelope::ok(commits)))
}

async fn list_branch_commit_history(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<BranchPathParams>,
    Query(query): Query<CommitHistoryQuery>,
) -> Result<Json<ResponseEnvelope<CommitHistoryPageDTO>>, AppError> {
    if let Some(cursor) = query.cursor.as_deref() {
        validate_full_sha(cursor, "cursor")?;
    }
    let mut db = scoped_db_for_workspace_repo(
        &state,
        &auth,
        params.workspace_id,
        params.repo_id,
        &["workspaces:read"],
    )
    .await?;
    let _user_claims = extract_user_claims(&auth)?;
    let branch = state
        .workspace_service
        .get_workspace_repository(&mut db, params.workspace_id, params.repo_id)
        .await?;
    let page = state
        .workspace_service
        .list_branch_commit_history(
            &mut db,
            &branch,
            query.cursor.as_deref(),
            query.limit.clamp(1, MAX_COMMIT_LIMIT),
        )
        .await?;
    db.commit().await.map_err(AppError::from)?;
    Ok(Json(ResponseEnvelope::ok(page)))
}

async fn get_commit_diff(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<CommitPathParams>,
) -> Result<Json<ResponseEnvelope<CommitDiffDTO>>, AppError> {
    validate_full_sha(&params.sha, "sha")?;
    let mut db = scoped_db_for_workspace_repo(
        &state,
        &auth,
        params.workspace_id,
        params.repo_id,
        &["workspaces:read"],
    )
    .await?;
    let _user_claims = extract_user_claims(&auth)?;
    let branch = state
        .workspace_service
        .get_workspace_repository(&mut db, params.workspace_id, params.repo_id)
        .await?;
    let diff = state
        .workspace_service
        .get_branch_commit_diff(&mut db, &branch, &params.sha)
        .await?;
    db.commit().await.map_err(AppError::from)?;
    Ok(Json(ResponseEnvelope::ok(diff)))
}

async fn get_commit_file_diff(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<CommitFilePathParams>,
) -> Result<Json<ResponseEnvelope<g5_scm::CommitFileDiff>>, AppError> {
    validate_full_sha(&params.sha, "sha")?;
    let path = sanitize_path(&params.path)?;
    let mut db = scoped_db_for_workspace_repo(
        &state,
        &auth,
        params.workspace_id,
        params.repo_id,
        &["workspaces:read"],
    )
    .await?;
    let _user_claims = extract_user_claims(&auth)?;
    let branch = state
        .workspace_service
        .get_workspace_repository(&mut db, params.workspace_id, params.repo_id)
        .await?;
    let diff = state
        .workspace_service
        .get_branch_commit_file_diff(&mut db, &branch, &params.sha, &path)
        .await?;
    db.commit().await.map_err(AppError::from)?;
    Ok(Json(ResponseEnvelope::ok(diff)))
}

fn validate_full_sha(value: &str, field: &str) -> Result<(), AppError> {
    if value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(AppError::from_error(G5Error::ValidationError {
            details: serde_json::json!({
                "field": field,
                "message": "must be a full 40-character SHA"
            }),
        }))
    }
}

/// `GET /api/v1/workspaces/:workspaceId/repos/:repoId/files/*path`
///
/// Retrieves the raw content of a single file at the branch's HEAD commit.
/// Used by the UI to display file contents in the code viewer/editor, and
/// by the diff view to show file content alongside diffs.
async fn get_file_content(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    Path(params): Path<FilePathParams>,
) -> Result<Json<ResponseEnvelope<FileContentResponse>>, AppError> {
    let mut db = scoped_db_for_workspace_repo(
        &state,
        &auth,
        params.workspace_id,
        params.repo_id,
        &["workspaces:read"],
    )
    .await?;
    tracing::debug!(
        workspace_id = %params.workspace_id,
        repo_id = %params.repo_id,
        path = %params.path,
        "File tree endpoint called: get_file_content"
    );

    // 1. Validate auth — reject service tokens
    let _user_claims = extract_user_claims(&auth)?;

    // 2. Look up branch
    let branch = state
        .workspace_service
        .get_workspace_repository(&mut db, params.workspace_id, params.repo_id)
        .await?;
    db.rollback().await.map_err(AppError::from)?;
    let file = crate::routes::workspaces::read_workspace_file_for_actor(
        &state,
        &user,
        &auth,
        branch.workspace_id,
        branch.repo_id,
        &params.path,
    )
    .await?;
    Ok(Json(ResponseEnvelope::ok(FileContentResponse {
        path: file.path,
        content: file.content,
        size: i64::try_from(file.size).unwrap_or(i64::MAX),
        encoding: file.encoding,
        draft_dirty: file.source == "live",
        hash: Some(file.hash),
        source: file.source,
        repo_oid: file.repo_oid,
        snapshot_at: file.snapshot_at,
    })))
}

/// `PUT /api/v1/workspaces/:workspaceId/repos/:repoId/files/*path`
///
/// Compatibility surface for saving one workspace file. It uses the same
/// attached-owner runtime and association fence as the workspace file API.
async fn put_file_content(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    Path(params): Path<FilePathParams>,
    Json(body): Json<FileWriteRequest>,
) -> Result<StatusCode, AppError> {
    let mut db = scoped_db_for_workspace_repo(
        &state,
        &auth,
        params.workspace_id,
        params.repo_id,
        &["workspaces:write"],
    )
    .await?;
    let _user_claims = extract_user_claims(&auth)?;
    let branch = state
        .workspace_service
        .get_workspace_repository(&mut db, params.workspace_id, params.repo_id)
        .await?;
    db.rollback().await.map_err(AppError::from)?;
    crate::routes::workspaces::write_workspace_file_for_actor(
        &state,
        &user,
        &auth,
        branch.workspace_id,
        branch.repo_id,
        &params.path,
        &body.content,
        &body.encoding,
        body.expected_hash.as_deref(),
        None,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /api/v1/workspaces/:workspaceId/repos/:repoId/files/*path`
///
/// Compatibility surface for deleting one workspace file. It uses the same
/// attached-owner runtime and association fence as the workspace file API.
async fn delete_file_or_folder(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    Path(params): Path<FilePathParams>,
    body: Option<Json<FileDeleteRequest>>,
) -> Result<StatusCode, AppError> {
    let mut db = scoped_db_for_workspace_repo(
        &state,
        &auth,
        params.workspace_id,
        params.repo_id,
        &["workspaces:write"],
    )
    .await?;
    let _user_claims = extract_user_claims(&auth)?;
    let branch = state
        .workspace_service
        .get_workspace_repository(&mut db, params.workspace_id, params.repo_id)
        .await?;
    db.rollback().await.map_err(AppError::from)?;
    let expected_hash = body
        .as_ref()
        .and_then(|Json(body)| body.expected_hash.as_deref());
    crate::routes::workspaces::delete_workspace_file_for_actor(
        &state,
        &user,
        &auth,
        branch.workspace_id,
        branch.repo_id,
        &params.path,
        expected_hash,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Extracts user claims from `AuthContext::User`. Returns `Forbidden`
/// error if the auth context is a service token (service tokens cannot
/// access file tree endpoints).
fn extract_user_claims(auth: &AuthContext) -> Result<&g5_auth::JwtClaims, AppError> {
    match auth {
        AuthContext::User(claims) => Ok(claims),
        _ => Err(AppError::from_error(
            g5_contracts::error::G5Error::Forbidden {
                missing_scopes: vec!["user_auth_required".into()],
            },
        )),
    }
}

/// Sanitizes a file path to prevent path traversal attacks.
///
/// Axum 0.7 wildcard captures (`/*path`) include the leading `/`, so we
/// must strip it first before applying security checks.
///
/// Rejects:
/// - Path traversal sequences (`..`)
/// - Empty paths (after trimming)
///
/// Returns the sanitized path with leading `/` trimmed.
pub(crate) fn sanitize_path(path: &str) -> Result<String, AppError> {
    // Normalize: trim leading slashes (Axum 0.7 wildcard captures include them)
    let trimmed = path.trim_start_matches('/');

    // Reject empty paths
    if trimmed.is_empty() {
        return Err(AppError::from_error(
            g5_contracts::error::G5Error::ValidationError {
                details: serde_json::json!({
                    "path": "File path must not be empty"
                }),
            },
        ));
    }

    // Reject path traversal
    if trimmed.contains("..") {
        return Err(AppError::from_error(
            g5_contracts::error::G5Error::ValidationError {
                details: serde_json::json!({
                    "path": "Path traversal is not allowed"
                }),
            },
        ));
    }

    Ok(trimmed.to_string())
}
