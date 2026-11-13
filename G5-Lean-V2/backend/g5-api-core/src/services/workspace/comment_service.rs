//! Comment service — the only writer of comment threads and their messages
//! (`g5.architecture.comment_owner`).
//!
//! - **Anchored comments** (`scope = "line"`): a line of one file, identified
//!   by workspace, repository and path; the branch is a label. Migration 090
//!   refuses any legacy node thread before retiring its readers.
//! - **Review-suggestion threads** (`scope = "suggestion"`): opened and
//!   answered through the review owner, never through the anchored-comment
//!   operations.
//!
//! The service decides who may act (`g5.documents.comments.authority`,
//! `.replies`) and publishes thread events after commit. Route handlers only
//! translate requests and errors.

use crate::services::git::repo_store::TenantRepoStorage;
use std::collections::HashMap;

use g5_authz::ScopedDb;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tracing::{error, info};
use uuid::Uuid;

use crate::db::scoped::ensure_user_workspace_access_until_commit;
use crate::errors::{AppError, G5Error};
use crate::services::user_service::{UserService, UserSummary};
use crate::services::workspace_mutation_policy::{
    ensure_workspace_allows_mutation, ensure_workspace_owner, refuse_base_workspace,
    WorkspaceMutationKind,
};
use crate::services::workspace_reader::{resolve_committed_snapshot, CommittedRepoReadView};
use crate::services::EventBus;
use crate::ws::{Channel, WsEvent};
use g5_contracts::types::AuthorityTarget;

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

// ---------------------------------------------------------------------------
// DTO Types
// ---------------------------------------------------------------------------

/// Filter parameters for listing comments.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CommentFilters {
    pub repo_id: Option<Uuid>,
    pub file_path: Option<String>,
    pub branch: Option<String>,
    pub resolved: Option<bool>,
}

/// Request to create a comment.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateCommentRequest {
    pub id: Option<Uuid>,
    pub scope: String,
    pub content: String,
    /// Required for scope = 'line'; exact repository owning the anchor.
    pub repo_id: Option<Uuid>,
    /// Required for scope = 'line'
    pub file_path: Option<String>,
    /// Required for scope = 'line'
    pub line_number: Option<i32>,
    /// Required for scope = 'line': SHA-256 of the exact UTF-8 line bytes.
    pub line_hash: Option<String>,
    /// Required for scope = 'line': SHA-256 of the exact displayed file bytes.
    pub content_hash: Option<String>,
    /// Required for scope = 'line': bounded surrounding source context.
    pub anchor_context: Option<String>,
    /// Optional: commit SHA at creation time (for line-scoped)
    pub commit_sha: Option<String>,
    /// The branch the user is currently viewing (for line-scoped)
    pub origin_branch: Option<String>,
    /// Optional: diff base SHA
    pub commit_range_base: Option<String>,
    /// Optional: diff head SHA
    pub commit_range_head: Option<String>,
}

/// Request to update a comment.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCommentRequest {
    pub content: Option<String>,
    pub resolved: Option<bool>,
}

/// A comment as returned by the API.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommentDTO {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub author_id: Uuid,
    pub author_name: String,
    pub author_avatar: Option<String>,
    pub scope: String,
    pub repo_id: Option<Uuid>,
    pub file_path: Option<String>,
    pub line_number: Option<i32>,
    pub line_hash: Option<String>,
    pub content_hash: Option<String>,
    pub anchor_context: Option<String>,
    pub commit_sha: Option<String>,
    pub commit_range_base: Option<String>,
    pub commit_range_head: Option<String>,
    pub content: String,
    pub resolved: bool,
    pub resolved_at: Option<String>,
    pub origin_branch: String,
    pub branch_deleted: bool,
    pub reply_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

/// A comment reply as returned by the API.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommentReplyDTO {
    pub id: Uuid,
    pub comment_id: Uuid,
    pub author_id: Uuid,
    pub author_name: String,
    pub author_avatar: Option<String>,
    pub content: String,
    pub created_at: String,
}

/// WebSocket event emitted when a new comment is created.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommentNewEvent {
    #[serde(rename = "type")]
    pub event_type: String, // Always "comment.new"
    pub workspace_id: Uuid,
    pub comment_id: Uuid,
    pub scope: String,
    pub origin_branch: String,
}

// ---------------------------------------------------------------------------
// Internal query row types
// ---------------------------------------------------------------------------

/// Row returned by the list_comments query.
#[derive(Debug, sqlx::FromRow)]
struct CommentRow {
    // -- comment_index columns --
    id: Uuid,
    workspace_id: Uuid,
    author_id: Uuid,
    scope: String,
    repo_id: Option<Uuid>,
    file_path: Option<String>,
    line_number: Option<i32>,
    line_hash: Option<String>,
    content_hash: Option<String>,
    anchor_context: Option<String>,
    commit_sha: Option<String>,
    commit_range_base: Option<String>,
    commit_range_head: Option<String>,
    content: String,
    resolved: bool,
    resolved_at: Option<chrono::DateTime<chrono::Utc>>,
    origin_branch: String,
    branch_deleted: bool,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    // -- subquery --
    reply_count: Option<i64>,
}

impl CommentRow {
    fn into_dto(self, author_name: String, author_avatar: Option<String>) -> CommentDTO {
        CommentDTO {
            id: self.id,
            workspace_id: self.workspace_id,
            author_id: self.author_id,
            author_name,
            author_avatar,
            scope: self.scope,
            repo_id: self.repo_id,
            file_path: self.file_path,
            line_number: self.line_number,
            line_hash: self.line_hash,
            content_hash: self.content_hash,
            anchor_context: self.anchor_context,
            commit_sha: self.commit_sha,
            commit_range_base: self.commit_range_base,
            commit_range_head: self.commit_range_head,
            content: self.content,
            resolved: self.resolved,
            resolved_at: self.resolved_at.map(|dt| dt.to_rfc3339()),
            origin_branch: self.origin_branch,
            branch_deleted: self.branch_deleted,
            reply_count: self.reply_count.unwrap_or(0),
            created_at: self.created_at.to_rfc3339(),
            updated_at: self.updated_at.to_rfc3339(),
        }
    }
}

/// Row returned by reply queries.
#[derive(Debug, sqlx::FromRow)]
struct ReplyRow {
    id: Uuid,
    comment_id: Uuid,
    author_id: Uuid,
    content: String,
    created_at: chrono::DateTime<chrono::Utc>,
}

impl ReplyRow {
    fn into_dto(self, author_name: String, author_avatar: Option<String>) -> CommentReplyDTO {
        CommentReplyDTO {
            id: self.id,
            comment_id: self.comment_id,
            author_id: self.author_id,
            author_name,
            author_avatar,
            content: self.content,
            created_at: self.created_at.to_rfc3339(),
        }
    }
}

/// The parts of a thread's root row that decide who may act on it.
#[derive(Debug, sqlx::FromRow)]
struct ThreadHead {
    org_id: Uuid,
    workspace_id: Uuid,
    author_id: Uuid,
    scope: String,
}

/// A comment author's attribution, shared with every record that names a
/// person (`UserService::load_summaries`).
type AuthorRow = UserSummary;

// ---------------------------------------------------------------------------
// CommentService
// ---------------------------------------------------------------------------

/// Comment CRUD operations, reply threading and resolution management.
///
/// Holds an `EventBus` for emitting WebSocket events after successful
/// comment creation. Database access is provided per-request via `ScopedDb`.
pub struct CommentService {
    event_bus: EventBus,
    /// Repository storage backend for git diff-based line mapping.
    repo_storage: TenantRepoStorage,
    system_pool: PgPool,
}

impl CommentService {
    /// Create a new `CommentService`.
    pub fn new(event_bus: EventBus, repo_storage: TenantRepoStorage, system_pool: PgPool) -> Self {
        Self {
            event_bus,
            repo_storage,
            system_pool,
        }
    }

    // -----------------------------------------------------------------------
    // list_comments
    // -----------------------------------------------------------------------

    /// Lists the anchored threads of a workspace, newest first, with author
    /// summaries. Review threads are listed through their suggestions.
    ///
    /// **Filters:**
    /// - No filters: every anchored thread in the workspace.
    /// - `repoId=R&filePath=X`: the threads of one file. Every surface showing
    ///   that file lists the same threads (`g5.documents.comments.identity`).
    /// - `resolved=true|false`: resolution status.
    ///
    /// `branch` is accepted for compatibility and ignored: the branch is a
    /// label that never selects or hides a thread.
    pub async fn list_comments(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
        filters: CommentFilters,
    ) -> Result<Vec<CommentDTO>, AppError> {
        if filters.file_path.is_some() && filters.repo_id.is_none() {
            return Err(AppError::from_error(G5Error::ValidationError {
                details: serde_json::json!(
                    "Line-comment file filters require an exact repository id"
                ),
            }));
        }
        if let Some(repo_id) = filters.repo_id {
            self.resolve_current_mapping_target(db, workspace_id, repo_id)
                .await?;
        }

        let rows = sqlx::query_as::<_, CommentRow>(
            r#"
            SELECT
                ci.id,
                ci.workspace_id,
                ci.author_id,
                ci.scope,
                ci.repo_id,
                ci.file_path,
                ci.line_number,
                ci.line_hash,
                ci.content_hash,
                ci.anchor_context,
                ci.commit_sha,
                ci.commit_range_base,
                ci.commit_range_head,
                ci.content,
                ci.resolved,
                ci.resolved_at,
                ci.origin_branch,
                ci.branch_deleted,
                ci.created_at,
                ci.updated_at,
                (SELECT COUNT(*) FROM comment_replies cr WHERE cr.comment_id = ci.id) AS reply_count
            FROM comment_index ci
            WHERE ci.workspace_id = $1
              AND ci.scope <> 'suggestion'
              AND ($2::UUID IS NULL OR ci.repo_id = $2)
              AND ($3::TEXT IS NULL OR ci.file_path = $3)
              AND ($4::BOOLEAN IS NULL OR ci.resolved = $4)
            ORDER BY ci.created_at DESC
            "#,
        )
        .bind(workspace_id)
        .bind(filters.repo_id)
        .bind(filters.file_path.as_deref())
        .bind(filters.resolved)
        .fetch_all(db.conn())
        .await
        .map_err(|e| {
            error!(
                workspace_id = %workspace_id,
                error = %e,
                "Failed to list comments"
            );
            AppError::from_error(G5Error::Internal {
                message: format!("Failed to list comments: {}", e),
            })
        })?;

        let authors = self
            .load_authors(rows.iter().map(|row| row.author_id))
            .await?;

        Ok(rows
            .into_iter()
            .map(|row| {
                let (author_name, author_avatar) = author_fields(&authors, row.author_id);
                row.into_dto(author_name, author_avatar)
            })
            .collect())
    }

    // -----------------------------------------------------------------------
    // get_cross_branch_comments
    // -----------------------------------------------------------------------

    // -----------------------------------------------------------------------
    // create_comment
    // -----------------------------------------------------------------------

    /// Starts an anchored thread with a line comment
    /// (`g5.documents.comments.authority`).
    ///
    /// **Validation:**
    /// - The workspace accepts comment writes and the author can read it:
    ///   anyone who can read the workspace starts a thread, with no running
    ///   VM; tenant row security alone admits any org member, so visibility
    ///   and invitations are checked here and held until the caller commits
    ///   (`g5.security.current-authority`).
    /// - `scope = "line"` with `repo_id`, `file_path`, `line_number`,
    ///   `line_hash`, `content_hash` and `anchor_context`. Node-level comments
    ///   are retired: existing ones stay readable, new ones are refused.
    /// - The text is not blank.
    ///
    /// **Side effects:** a `comment.new` event once the caller commits.
    pub async fn create_comment(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
        author_id: Uuid,
        req: CreateCommentRequest,
    ) -> Result<CommentDTO, AppError> {
        if req.content.trim().is_empty() {
            return Err(AppError::from_error(G5Error::ValidationError {
                details: serde_json::json!("Comment content cannot be empty"),
            }));
        }
        match req.scope.as_str() {
            "line" | "workspace" => {}
            "node" => {
                return Err(AppError::from_error(G5Error::ValidationError {
                    details: serde_json::json!(
                        "Node-level comments are retired; comment on a line of the file"
                    ),
                }));
            }
            _ => {
                return Err(AppError::from_error(G5Error::ValidationError {
                    details: serde_json::json!("Invalid comment scope. Must be 'line'"),
                }));
            }
        }
        if req.scope == "line"
            && (req.repo_id.is_none()
                || req.file_path.is_none()
                || req.line_number.is_none()
                || req.line_hash.is_none()
                || req.content_hash.is_none()
                || req.anchor_context.is_none())
        {
            return Err(AppError::from_error(G5Error::ValidationError {
                details: serde_json::json!(
                    "Line-scoped comments require repo_id, file_path, line_number, line_hash, content_hash, and anchor_context"
                ),
            }));
        }
        for (field, value) in [
            ("line_hash", req.line_hash.as_deref()),
            ("content_hash", req.content_hash.as_deref()),
        ] {
            if req.scope == "line" && !value.is_some_and(is_sha256_hex) {
                return Err(AppError::from_error(G5Error::ValidationError {
                    details: serde_json::json!({
                        "field": field,
                        "message": "must be a lowercase SHA-256 hex digest",
                    }),
                }));
            }
        }
        if req
            .anchor_context
            .as_deref()
            .is_some_and(|context| context.len() > 4096)
        {
            return Err(AppError::from_error(G5Error::ValidationError {
                details: serde_json::json!({
                    "field": "anchor_context",
                    "message": "must not exceed 4096 bytes",
                }),
            }));
        }

        let org_id: Uuid = sqlx::query_scalar(
            "SELECT org_id FROM workspaces WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(workspace_id)
        .fetch_optional(db.conn())
        .await
        .map_err(|e| {
            AppError::from_error(G5Error::Internal {
                message: format!("Failed to load workspace: {e}"),
            })
        })?
        .ok_or_else(|| {
            AppError::from_error(G5Error::NotFound {
                resource: format!("workspace {workspace_id}"),
            })
        })?;
        ensure_user_workspace_access_until_commit(
            &self.system_pool,
            db,
            author_id,
            org_id,
            workspace_id,
        )
        .await?;
        // Policy is read under the hold taken just above, so a read-only
        // transition cannot commit between this check and the write.
        ensure_thread_writes_allowed(db, workspace_id, "comments.create")
            .await
            .map_err(AppError::from_error)?;
        if req.scope == "workspace"
            && (req.repo_id.is_some() || req.file_path.is_some() || req.line_number.is_some())
        {
            return Err(G5Error::ValidationError {
                details: serde_json::json!("General feedback cannot carry a file anchor"),
            }
            .into());
        }
        if let Some(repo_id) = req.repo_id {
            self.resolve_current_mapping_target(db, workspace_id, repo_id)
                .await?;
            let owner: Option<Uuid> =
                sqlx::query_scalar("SELECT owner_user_id FROM workspaces WHERE id=$1 FOR SHARE")
                    .bind(workspace_id)
                    .fetch_one(db.conn())
                    .await?;
            if owner != Some(author_id) {
                // A reader can anchor only content in this workspace's published
                // history; arbitrary repository objects are not permission.
                let sha = req
                    .commit_sha
                    .as_deref()
                    .ok_or_else(|| G5Error::Forbidden {
                        missing_scopes: vec!["published_file_content".into()],
                    })?;
                let heads = crate::services::git::workspace_revision_history::published_heads(
                    db,
                    &self.repo_storage,
                    workspace_id,
                )
                .await?;
                let published = heads
                    .iter()
                    .any(|h| h.repo_id == repo_id.to_string() && h.head_commit == sha);
                let historical: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM workspace_revision_events e, jsonb_array_elements(e.heads) h WHERE e.workspace_id=$1 AND h->>'repoId'=$2 AND h->>'headCommit'=$3)")
                    .bind(workspace_id).bind(repo_id.to_string()).bind(sha).fetch_one(db.conn()).await?;
                if !published && !historical {
                    return Err(G5Error::Forbidden {
                        missing_scopes: vec!["published_file_content".into()],
                    }
                    .into());
                }
                let view = CommittedRepoReadView::from_landed_head(&*db, org_id, repo_id, sha)?;
                let bytes = view
                    .read_file(
                        &self.repo_storage,
                        req.file_path.as_deref().unwrap_or_default(),
                    )
                    .await?;
                use sha2::{Digest, Sha256};
                let text = std::str::from_utf8(&bytes).map_err(|_| G5Error::ValidationError {
                    details: serde_json::json!("Binary files cannot carry line comments"),
                })?;
                let line = req
                    .line_number
                    .filter(|n| *n > 0)
                    .and_then(|n| text.lines().nth((n - 1) as usize))
                    .ok_or_else(|| G5Error::ValidationError {
                        details: serde_json::json!("Invalid anchor line"),
                    })?;
                if req.content_hash.as_deref() != Some(hex::encode(Sha256::digest(&bytes)).as_str())
                    || req.line_hash.as_deref()
                        != Some(hex::encode(Sha256::digest(line.as_bytes())).as_str())
                {
                    return Err(G5Error::Conflict {
                        reason: "The comment anchor does not match the displayed revision".into(),
                    }
                    .into());
                }
            }
        }
        // The branch is a label (`g5.documents.comments.identity`): an
        // unlabelled comment takes the workspace's branch, which is named
        // after the workspace.
        let origin_branch = match req
            .origin_branch
            .as_deref()
            .map(str::trim)
            .filter(|branch| !branch.is_empty())
        {
            Some(branch) => branch.to_string(),
            None => sqlx::query_scalar::<_, String>(
                "SELECT name FROM workspaces WHERE id = $1 AND deleted_at IS NULL",
            )
            .bind(workspace_id)
            .fetch_one(db.conn())
            .await
            .map_err(|e| {
                AppError::from_error(G5Error::Internal {
                    message: format!("Failed to load workspace branch: {e}"),
                })
            })?,
        };

        let row = sqlx::query_as::<_, CommentRow>(
            r#"
            WITH inserted AS (
                INSERT INTO comment_index (
                    org_id, workspace_id, author_id, scope,
                    repo_id, file_path, line_number, line_hash, content_hash,
                    anchor_context, commit_sha,
                    commit_range_base, commit_range_head,
                    content, origin_branch, id, phase
                ) VALUES ((SELECT org_id FROM workspaces WHERE id = $1), $1, $2, $15, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, (SELECT phase FROM workspaces WHERE id=$1))
                ON CONFLICT (id) DO UPDATE SET id=EXCLUDED.id
                WHERE comment_index.workspace_id=EXCLUDED.workspace_id
                  AND comment_index.author_id=EXCLUDED.author_id
                  AND comment_index.scope=EXCLUDED.scope
                  AND comment_index.content=EXCLUDED.content
                  AND comment_index.repo_id IS NOT DISTINCT FROM EXCLUDED.repo_id
                  AND comment_index.file_path IS NOT DISTINCT FROM EXCLUDED.file_path
                  AND comment_index.line_number IS NOT DISTINCT FROM EXCLUDED.line_number
                  AND comment_index.content_hash IS NOT DISTINCT FROM EXCLUDED.content_hash
                  AND comment_index.commit_sha IS NOT DISTINCT FROM EXCLUDED.commit_sha
                RETURNING *
            )
            SELECT
                i.id,
                i.workspace_id,
                i.author_id,
                i.scope,
                i.repo_id,
                i.file_path,
                i.line_number,
                i.line_hash,
                i.content_hash,
                i.anchor_context,
                i.commit_sha,
                i.commit_range_base,
                i.commit_range_head,
                i.content,
                i.resolved,
                i.resolved_at,
                i.origin_branch,
                i.branch_deleted,
                i.created_at,
                i.updated_at,
                0::BIGINT AS reply_count
            FROM inserted i
            "#,
        )
        .bind(workspace_id)
        .bind(author_id)
        .bind(req.repo_id)
        .bind(req.file_path.as_deref())
        .bind(req.line_number)
        .bind(req.line_hash.as_deref())
        .bind(req.content_hash.as_deref())
        .bind(req.anchor_context.as_deref())
        .bind(req.commit_sha.as_deref())
        .bind(req.commit_range_base.as_deref())
        .bind(req.commit_range_head.as_deref())
        .bind(&req.content)
        .bind(&origin_branch)
        .bind(req.id.unwrap_or_else(Uuid::new_v4))
        .bind(&req.scope)
        .fetch_optional(db.conn())
        .await
        .map_err(|e| {
            error!(
                workspace_id = %workspace_id,
                author_id = %author_id,
                error = %e,
                "Failed to create comment"
            );
            AppError::from_error(G5Error::Internal {
                message: format!("Failed to create comment: {}", e),
            })
        })?.ok_or_else(|| AppError::from(G5Error::Conflict { reason: "This message identity was already used with different content".into() }))?;

        let authors = self.load_authors(std::iter::once(row.author_id)).await?;
        let (author_name, author_avatar) = author_fields(&authors, row.author_id);
        let dto = row.into_dto(author_name, author_avatar);

        self.publish_after_commit(
            db,
            workspace_id,
            WsEvent::CommentNew {
                workspace_id,
                comment_id: dto.id,
                scope: dto.scope.clone(),
                origin_branch: dto.origin_branch.clone(),
            },
        );

        info!(
            comment_id = %dto.id,
            workspace_id = %workspace_id,
            origin_branch = %dto.origin_branch,
            "Comment created"
        );

        Ok(dto)
    }

    // -----------------------------------------------------------------------
    // update_comment
    // -----------------------------------------------------------------------

    /// Updates an anchored comment's text and/or resolution status.
    ///
    /// Maps to `PATCH /api/v1/comments/:id`.
    ///
    /// - `content`: only the comment's author edits its text, and only while
    ///   they can still read the workspace (held until commit).
    /// - `resolved`: only the workspace's current owner resolves or reopens,
    ///   whoever wrote the thread (`g5.documents.comments.authority`).
    /// - A review thread is not reachable here (`g5.governance.review-thread`).
    ///
    /// **Side effects:** a `comment.updated` event once the caller commits.
    pub async fn update_comment(
        &self,
        db: &mut ScopedDb,
        comment_id: Uuid,
        user_id: Uuid,
        req: UpdateCommentRequest,
    ) -> Result<CommentDTO, AppError> {
        let head = anchored_thread_head(db, comment_id).await?;
        let is_decision: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM approval_statuses WHERE comment_id=$1)",
        )
        .bind(comment_id)
        .fetch_one(db.conn())
        .await?;
        if is_decision {
            return Err(G5Error::Conflict {
                reason: "Review decisions are retained as evidence. Add a reply to the discussion."
                    .into(),
            }
            .into());
        }

        if req.content.is_none() && req.resolved.is_none() {
            return Err(AppError::from_error(G5Error::ValidationError {
                details: serde_json::json!("Nothing to update: send content or resolved"),
            }));
        }
        if let Some(content) = req.content.as_deref() {
            if head.author_id != user_id {
                return Err(AppError::from_error(G5Error::Forbidden {
                    missing_scopes: vec!["comment_author".into()],
                }));
            }
            if content.trim().is_empty() {
                return Err(AppError::from_error(G5Error::ValidationError {
                    details: serde_json::json!("Comment content cannot be empty"),
                }));
            }
            ensure_user_workspace_access_until_commit(
                &self.system_pool,
                db,
                user_id,
                head.org_id,
                head.workspace_id,
            )
            .await?;
        }
        if req.resolved.is_some() {
            ensure_workspace_owner(db, head.workspace_id)
                .await
                .map_err(AppError::from_error)?;
        }
        // Policy is read under the hold each branch took above (the author's
        // read access, or the owner's share lock), so a read-only transition
        // cannot commit between this check and the write.
        ensure_thread_writes_allowed(db, head.workspace_id, "comments.update")
            .await
            .map_err(AppError::from_error)?;

        let row = sqlx::query_as::<_, CommentRow>(
            r#"
            WITH updated AS (
                UPDATE comment_index
                SET content = COALESCE($2, content),
                    resolved = COALESCE($3, resolved),
                    resolved_at = CASE
                        WHEN $3 = TRUE THEN now()
                        WHEN $3 = FALSE THEN NULL
                        ELSE resolved_at
                    END,
                    updated_at = now()
                WHERE id = $1
                RETURNING *
            )
            SELECT
                u.id,
                u.workspace_id,
                u.author_id,
                u.scope,
                u.repo_id,
                u.file_path,
                u.line_number,
                u.line_hash,
                u.content_hash,
                u.anchor_context,
                u.commit_sha,
                u.commit_range_base,
                u.commit_range_head,
                u.content,
                u.resolved,
                u.resolved_at,
                u.origin_branch,
                u.branch_deleted,
                u.created_at,
                u.updated_at,
                (SELECT COUNT(*) FROM comment_replies cr WHERE cr.comment_id = u.id) AS reply_count
            FROM updated u
            "#,
        )
        .bind(comment_id)
        .bind(req.content.as_deref())
        .bind(req.resolved)
        .fetch_one(db.conn())
        .await
        .map_err(|e| {
            error!(comment_id = %comment_id, error = %e, "Failed to update comment");
            AppError::from_error(G5Error::Internal {
                message: format!("Failed to update comment: {}", e),
            })
        })?;

        let authors = self.load_authors(std::iter::once(row.author_id)).await?;
        let (author_name, author_avatar) = author_fields(&authors, row.author_id);
        let dto = row.into_dto(author_name, author_avatar);

        self.publish_after_commit(
            db,
            head.workspace_id,
            WsEvent::CommentUpdated {
                workspace_id: head.workspace_id,
                comment_id,
                resolved: dto.resolved,
            },
        );

        info!(
            comment_id = %comment_id,
            resolved = ?req.resolved,
            "Comment updated"
        );

        Ok(dto)
    }

    // -----------------------------------------------------------------------
    // add_reply
    // -----------------------------------------------------------------------

    /// Adds a reply to an anchored thread. Anyone who can read the workspace
    /// may reply, with no running VM (`g5.documents.comments.replies`); the
    /// caller's scoped connection establishes that read access.
    ///
    /// Maps to `POST /api/v1/comments/:id/replies`. A review thread is not
    /// reachable here.
    ///
    /// **Side effects:** a `comment.reply` event once the caller commits.
    pub async fn add_reply(
        &self,
        db: &mut ScopedDb,
        comment_id: Uuid,
        author_id: Uuid,
        content: String,
    ) -> Result<CommentReplyDTO, AppError> {
        self.add_reply_with_id(db, comment_id, author_id, content, None)
            .await
    }

    pub async fn add_reply_with_id(
        &self,
        db: &mut ScopedDb,
        comment_id: Uuid,
        author_id: Uuid,
        content: String,
        id: Option<Uuid>,
    ) -> Result<CommentReplyDTO, AppError> {
        let head = anchored_thread_head(db, comment_id).await?;
        if content.trim().is_empty() {
            return Err(AppError::from_error(G5Error::ValidationError {
                details: serde_json::json!("Reply content cannot be empty"),
            }));
        }
        // Anyone who can read the workspace replies
        // (`g5.documents.comments.authority`); tenant row security alone
        // admits any org member, so the fixed workspace audience
        // is checked here and held until the caller commits.
        ensure_user_workspace_access_until_commit(
            &self.system_pool,
            db,
            author_id,
            head.org_id,
            head.workspace_id,
        )
        .await?;
        let reply_id = append_thread_reply_with_id(db, comment_id, author_id, &content, id)
            .await
            .map_err(AppError::from_error)?;
        let dto = self.load_reply(db, reply_id).await?;

        self.publish_after_commit(
            db,
            head.workspace_id,
            WsEvent::CommentReply {
                workspace_id: head.workspace_id,
                comment_id,
                reply_id: dto.id,
                author_name: dto.author_name.clone(),
            },
        );

        info!(
            reply_id = %dto.id,
            comment_id = %comment_id,
            "Reply added"
        );

        Ok(dto)
    }

    // -----------------------------------------------------------------------
    // get_replies
    // -----------------------------------------------------------------------

    /// Lists the replies of an anchored thread, oldest first. A review
    /// thread is listed through its suggestion with
    /// [`list_thread_replies`](Self::list_thread_replies).
    pub async fn get_replies(
        &self,
        db: &mut ScopedDb,
        comment_id: Uuid,
    ) -> Result<Vec<CommentReplyDTO>, AppError> {
        anchored_thread_head(db, comment_id).await?;
        self.list_thread_replies(db, comment_id).await
    }

    /// Lists the replies of any thread, oldest first, for an owner that has
    /// already authorized the read (the review owner for suggestion threads).
    pub async fn list_thread_replies(
        &self,
        db: &mut ScopedDb,
        comment_id: Uuid,
    ) -> Result<Vec<CommentReplyDTO>, AppError> {
        let rows = sqlx::query_as::<_, ReplyRow>(
            r#"
            SELECT
                cr.id,
                cr.comment_id,
                cr.author_id,
                cr.content,
                cr.created_at
            FROM comment_replies cr
            WHERE cr.comment_id = $1
            ORDER BY cr.created_at ASC, cr.id ASC
            "#,
        )
        .bind(comment_id)
        .fetch_all(db.conn())
        .await
        .map_err(|e| {
            error!(comment_id = %comment_id, error = %e, "Failed to fetch replies");
            AppError::from_error(G5Error::Internal {
                message: format!("Failed to fetch replies: {}", e),
            })
        })?;

        let authors = self
            .load_authors(rows.iter().map(|row| row.author_id))
            .await?;

        Ok(rows
            .into_iter()
            .map(|row| {
                let (author_name, author_avatar) = author_fields(&authors, row.author_id);
                row.into_dto(author_name, author_avatar)
            })
            .collect())
    }

    async fn load_reply(
        &self,
        db: &mut ScopedDb,
        reply_id: Uuid,
    ) -> Result<CommentReplyDTO, AppError> {
        let row = sqlx::query_as::<_, ReplyRow>(
            r#"
            SELECT cr.id, cr.comment_id, cr.author_id, cr.content, cr.created_at
            FROM comment_replies cr
            WHERE cr.id = $1
            "#,
        )
        .bind(reply_id)
        .fetch_one(db.conn())
        .await
        .map_err(|e| {
            error!(reply_id = %reply_id, error = %e, "Failed to load reply");
            AppError::from_error(G5Error::Internal {
                message: format!("Failed to load reply: {}", e),
            })
        })?;
        let authors = self.load_authors(std::iter::once(row.author_id)).await?;
        let (author_name, author_avatar) = author_fields(&authors, row.author_id);
        Ok(row.into_dto(author_name, author_avatar))
    }

    // -----------------------------------------------------------------------
    // delete_comment
    // -----------------------------------------------------------------------

    /// Deletes an anchored thread and its replies (FK CASCADE). Only the
    /// workspace's current owner deletes, whoever wrote the thread.
    ///
    /// Maps to `DELETE /api/v1/comments/:id`. A review thread is not
    /// reachable here.
    ///
    /// **Side effects:** a `comment.deleted` event once the caller commits.
    pub async fn delete_comment(
        &self,
        db: &mut ScopedDb,
        comment_id: Uuid,
        _user_id: Uuid,
    ) -> Result<(), AppError> {
        let head = anchored_thread_head(db, comment_id).await?;
        let is_decision: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM approval_statuses WHERE comment_id=$1)",
        )
        .bind(comment_id)
        .fetch_one(db.conn())
        .await?;
        if is_decision {
            return Err(G5Error::Conflict {
                reason: "Review decisions are retained as evidence. Add a reply to the discussion."
                    .into(),
            }
            .into());
        }

        ensure_thread_writes_allowed(db, head.workspace_id, "comments.delete")
            .await
            .map_err(AppError::from_error)?;
        ensure_workspace_owner(db, head.workspace_id)
            .await
            .map_err(AppError::from_error)?;

        sqlx::query("DELETE FROM comment_index WHERE id = $1")
            .bind(comment_id)
            .execute(db.conn())
            .await
            .map_err(|e| {
                error!(comment_id = %comment_id, error = %e, "Failed to delete comment");
                AppError::from_error(G5Error::Internal {
                    message: format!("Failed to delete comment: {}", e),
                })
            })?;

        self.publish_after_commit(
            db,
            head.workspace_id,
            WsEvent::CommentDeleted {
                workspace_id: head.workspace_id,
                comment_id,
            },
        );

        info!(comment_id = %comment_id, "Comment deleted");

        Ok(())
    }

    /// Publish a workspace event once the caller's transaction commits
    /// (`g5.documents.comments.live`, `g5.design.commit-notify`).
    fn publish_after_commit(&self, db: &mut ScopedDb, workspace_id: Uuid, event: WsEvent) {
        let event_bus = self.event_bus.clone();
        db.on_commit(move || {
            event_bus.publish(&Channel::Workspace(workspace_id), &event);
        });
    }

    // -----------------------------------------------------------------------
    // mark_branch_deleted
    // -----------------------------------------------------------------------

    /// Marks all comments from a deleted branch as `branch_deleted = true`.
    ///
    /// Called when a workspace branch is removed. This is a maintenance
    /// operation, not exposed as a REST endpoint.
    ///
    /// **Business rule:** Comments from deleted branches are preserved for
    /// 90 days. The GC job periodically deletes comments where
    /// `branch_deleted = TRUE AND updated_at < now() - interval '90 days'`.
    pub async fn mark_branch_deleted(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
        branch_name: &str,
    ) -> Result<u64, AppError> {
        let result = sqlx::query!(
            r#"
            UPDATE comment_index
            SET branch_deleted = TRUE, updated_at = now()
            WHERE workspace_id = $1 AND origin_branch = $2
            "#,
            workspace_id,
            branch_name,
        )
        .execute(db.conn())
        .await
        .map_err(|e| {
            error!(
                workspace_id = %workspace_id,
                branch_name = %branch_name,
                error = %e,
                "Failed to mark branch comments as deleted"
            );
            AppError::from_error(G5Error::Internal {
                message: format!("Failed to mark branch deleted: {}", e),
            })
        })?;

        let affected = result.rows_affected();

        info!(
            workspace_id = %workspace_id,
            branch_name = %branch_name,
            affected_count = affected,
            "Marked branch comments as deleted"
        );

        Ok(affected)
    }

    // ── private helpers ─────────────────────────────────────────────────

    /// Capture the exact repository selected by the comment anchor from one
    /// DB-authoritative committed workspace cut.
    async fn resolve_current_mapping_target(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
        repo_id: Uuid,
    ) -> Result<CommittedRepoReadView, AppError> {
        let snapshot = resolve_committed_snapshot(
            db,
            &self.repo_storage,
            AuthorityTarget::Workspace { workspace_id },
        )
        .await
        .map_err(AppError::from_error)?;
        let binding = snapshot.repo(repo_id).map_err(|message| {
            AppError::from_error(G5Error::ValidationError {
                details: serde_json::json!({
                    "message": message,
                    "workspaceId": workspace_id,
                    "repoId": repo_id,
                }),
            })
        })?;
        CommittedRepoReadView::from_binding(&*db, snapshot.org_id, binding)
            .map_err(AppError::from_error)
    }

    async fn load_authors<I>(&self, author_ids: I) -> Result<HashMap<Uuid, AuthorRow>, AppError>
    where
        I: IntoIterator<Item = Uuid>,
    {
        UserService::load_summaries(&self.system_pool, author_ids).await
    }
}

/// Load a thread's root for an anchored-comment request. A review thread is
/// reached only through its suggestion (`g5.governance.review-thread`), so it
/// reads as not found here.
async fn anchored_thread_head(db: &mut ScopedDb, comment_id: Uuid) -> Result<ThreadHead, AppError> {
    let head = sqlx::query_as::<_, ThreadHead>(
        "SELECT org_id, workspace_id, author_id, scope FROM comment_index WHERE id = $1",
    )
    .bind(comment_id)
    .fetch_optional(db.conn())
    .await
    .map_err(|e| {
        error!(comment_id = %comment_id, error = %e, "Failed to load comment");
        AppError::from_error(G5Error::Internal {
            message: format!("Failed to load comment: {}", e),
        })
    })?;
    match head {
        Some(head) if head.scope != "suggestion" => Ok(head),
        _ => Err(AppError::from_error(G5Error::NotFound {
            resource: format!("Comment {}", comment_id),
        })),
    }
}

/// Refuse a thread write on a read-only workspace: the retained mutation
/// policy, or Base, which is read-only for every user
/// (`g5.documents.comments.authority`, `g5.documents.comments.replies`).
/// Shared by anchored comments and review threads.
pub async fn ensure_thread_writes_allowed(
    db: &mut ScopedDb,
    workspace_id: Uuid,
    operation: &'static str,
) -> Result<(), G5Error> {
    ensure_workspace_allows_mutation(
        db,
        workspace_id,
        WorkspaceMutationKind::WorkspaceOwnedCommentWrite,
        operation,
    )
    .await?;
    let is_base: Option<bool> =
        sqlx::query_scalar("SELECT is_base FROM workspaces WHERE id = $1 AND deleted_at IS NULL")
            .bind(workspace_id)
            .fetch_optional(db.conn())
            .await
            .map_err(|e| G5Error::Internal {
                message: format!("Failed to load workspace: {e}"),
            })?;
    refuse_base_workspace(is_base, workspace_id, operation)
}

/// Open the thread of a review suggestion: a `comment_index` row with scope
/// `suggestion` on the source workspace, authored by the reviewer. The review
/// owner authorizes the write and calls this inside its own transaction.
#[allow(clippy::too_many_arguments)]
pub async fn open_suggestion_thread(
    db: &mut ScopedDb,
    org_id: Uuid,
    source_workspace_id: Uuid,
    source_branch: &str,
    author_id: Uuid,
    repo_id: Option<Uuid>,
    path: Option<&str>,
    line: Option<i32>,
    content: &str,
) -> Result<Uuid, G5Error> {
    ensure_thread_writes_allowed(db, source_workspace_id, "workspaces.review_thread").await?;
    Ok(sqlx::query_scalar(
        "INSERT INTO comment_index(
             workspace_id, author_id, scope, repo_id, file_path, line_number, content, origin_branch, org_id)
         VALUES ($1,$2,'suggestion',$3,$4,$5,$6,$7,$8)
         RETURNING id",
    )
    .bind(source_workspace_id)
    .bind(author_id)
    .bind(repo_id)
    .bind(path)
    .bind(line)
    .bind(content)
    .bind(source_branch)
    .bind(org_id)
    .fetch_one(db.conn())
    .await?)
}

/// Append a message to a thread of either kind and touch the thread. The
/// caller has already decided that `author_id` may write it; a read-only
/// workspace, Base included, refuses every thread message.
pub async fn append_thread_reply(
    db: &mut ScopedDb,
    comment_id: Uuid,
    author_id: Uuid,
    content: &str,
) -> Result<Uuid, G5Error> {
    append_thread_reply_with_id(db, comment_id, author_id, content, None).await
}

pub async fn append_thread_reply_with_id(
    db: &mut ScopedDb,
    comment_id: Uuid,
    author_id: Uuid,
    content: &str,
    id: Option<Uuid>,
) -> Result<Uuid, G5Error> {
    let workspace_id: Uuid =
        sqlx::query_scalar("SELECT workspace_id FROM comment_index WHERE id = $1")
            .bind(comment_id)
            .fetch_optional(db.conn())
            .await?
            .ok_or_else(|| G5Error::NotFound {
                resource: format!("Comment {comment_id}"),
            })?;
    ensure_thread_writes_allowed(db, workspace_id, "comments.thread_message").await?;
    let reply_id: Uuid = sqlx::query_scalar(
        "INSERT INTO comment_replies(org_id, comment_id, author_id, content, id)
         VALUES ((SELECT org_id FROM comment_index WHERE id = $1), $1, $2, $3, $4)
         ON CONFLICT (id) DO UPDATE SET id=EXCLUDED.id
         WHERE comment_replies.comment_id=EXCLUDED.comment_id
           AND comment_replies.author_id=EXCLUDED.author_id
           AND comment_replies.content=EXCLUDED.content
         RETURNING id",
    )
    .bind(comment_id)
    .bind(author_id)
    .bind(content)
    .bind(id.unwrap_or_else(Uuid::new_v4))
    .fetch_optional(db.conn())
    .await?
    .ok_or_else(|| G5Error::Conflict {
        reason: "This reply identity was already used with different content".into(),
    })?;
    sqlx::query("UPDATE comment_index SET updated_at = now() WHERE id = $1")
        .bind(comment_id)
        .execute(db.conn())
        .await?;
    Ok(reply_id)
}

fn author_fields(authors: &HashMap<Uuid, AuthorRow>, author_id: Uuid) -> (String, Option<String>) {
    authors
        .get(&author_id)
        .map(|author| (author.display_name.clone(), author.avatar_url.clone()))
        .unwrap_or_else(|| ("Unknown user".to_string(), None))
}
