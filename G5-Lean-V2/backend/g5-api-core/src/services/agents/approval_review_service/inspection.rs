//! Retained inspection uses source-history authority, never a live guest view.
use super::ApprovalReviewService;
use crate::{db::scoped, errors::AppError, services::AppState};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{DateTime, Utc};
use g5_authz::ScopedDb;
use g5_contracts::{types::FormalApprovalInvocation, G5Error};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{sync::Arc, time::Duration};
use uuid::Uuid;

const PAGE_BYTES: usize = 1024 * 1024;
const ITEM_CONTENT_BYTES: usize = 64 * 1024;
const TARGET_HIDDEN: &str = "[Requires read access to the merge target]";

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewCommitQuery {
    pub repo_id: Uuid,
    #[serde(default)]
    pub side: CommitSide,
    #[serde(default)]
    pub operation: CommitOperation,
    pub path: Option<String>,
}
#[derive(Debug, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CommitSide {
    #[default]
    Source,
    Base,
    #[serde(rename = "mergeBase")]
    MergeBase,
    Parent,
    #[serde(rename = "workspaceBase")]
    WorkspaceBase,
}
#[derive(Debug, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CommitOperation {
    #[default]
    Paths,
    File,
    Diff,
}
#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ReviewTranscriptQuery {
    pub cursor: Option<String>,
    pub limit: Option<u16>,
}

#[derive(sqlx::FromRow)]
struct Assignment {
    org_id: Uuid,
    target_workspace_id: Uuid,
    review_workspace_id: Uuid,
    task_id: Option<Uuid>,
    invocation: Option<Value>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    version: u8,
    review: Uuid,
    kind: i16,
    position: i64,
    sequence: i64,
    id: Uuid,
}
#[derive(sqlx::FromRow)]
struct TranscriptRow {
    kind: i16,
    position: i64,
    sequence: i64,
    id: Uuid,
    created_at: DateTime<Utc>,
    entry_type: String,
    content: String,
    metadata: Value,
    content_truncated: bool,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewTranscriptItem {
    pub id: Uuid,
    pub kind: &'static str,
    pub created_at: DateTime<Utc>,
    pub entry_type: String,
    pub content: String,
    pub content_truncated: bool,
    pub metadata: Value,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewTranscriptPage {
    pub items: Vec<ReviewTranscriptItem>,
    pub next_cursor: Option<String>,
    pub events_trimmed: bool,
}

impl ApprovalReviewService {
    #[allow(clippy::too_many_arguments)]
    pub async fn read_commits(
        &self,
        state: &Arc<AppState>,
        db: &mut ScopedDb,
        source: Uuid,
        review: Uuid,
        caller: Uuid,
        query: ReviewCommitQuery,
    ) -> Result<Value, AppError> {
        use g5_scm::native::ReviewSnapshotRead;
        let assignment = assignment(db, source, review).await?;
        if (query.side != CommitSide::Source || query.operation == CommitOperation::Diff)
            && !scoped::user_can_access_workspace(
                &state.system_pool,
                db,
                caller,
                assignment.org_id,
                assignment.target_workspace_id,
            )
            .await?
        {
            return Err(G5Error::Forbidden {
                missing_scopes: vec!["Read access to the merge target is required".into()],
            }
            .into());
        }
        let invocation: FormalApprovalInvocation = serde_json::from_value(
            assignment
                .invocation
                .ok_or_else(|| unavailable("The review has not captured its assignment"))?,
        )?;
        invocation.validate().map_err(unavailable)?;
        let selected = |heads: &[g5_contracts::types::MergeSourceHead]| {
            heads
                .iter()
                .find(|head| head.repo_id == query.repo_id.to_string())
                .map(|head| head.head_commit.clone())
                .ok_or_else(|| unavailable("Repository is not part of this review's assignment"))
        };
        let source_oid = selected(&invocation.resolution.source_heads)?;
        let base_oid = invocation
            .resolution
            .comparison_sha(&query.repo_id.to_string())
            .ok_or_else(|| unavailable("Comparison member unavailable"))?
            .to_owned();
        let context_oid;
        let commit = match query.side {
            CommitSide::Source => &source_oid,
            CommitSide::Base | CommitSide::MergeBase => &base_oid,
            CommitSide::Parent => {
                context_oid = selected(&invocation.resolution.target_heads)?;
                &context_oid
            }
            CommitSide::WorkspaceBase => {
                context_oid = selected(
                    invocation
                        .resolution
                        .workspace_base_heads
                        .as_deref()
                        .ok_or_else(|| unavailable("Workspace base unavailable"))?,
                )?;
                &context_oid
            }
        };
        let read = match query.operation {
            CommitOperation::Paths => ReviewSnapshotRead::Paths { commit },
            CommitOperation::File => ReviewSnapshotRead::File {
                commit,
                path: query
                    .path
                    .as_deref()
                    .ok_or_else(|| unavailable("A file path is required"))?,
            },
            CommitOperation::Diff => ReviewSnapshotRead::Diff {
                base: &base_oid,
                source: &source_oid,
                path: query.path.as_deref(),
            },
        };
        let repository = state
            .repo_storage
            .repo_path(&*db, assignment.org_id, query.repo_id)
            .map_err(|_| unavailable("The assigned repository is unavailable"))?;
        let git = g5_scm::native::NativeGit::api(&state.git_bin, Duration::from_secs(15))
            .map_err(git_error)?;
        let bytes = git
            .read_review_snapshot(&repository, read)
            .await
            .map_err(git_error)?;
        Ok(
            json!({"repoId":query.repo_id,"commit":commit,"content":String::from_utf8_lossy(&bytes)}),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn transcript(
        &self,
        state: &Arc<AppState>,
        db: &mut ScopedDb,
        source: Uuid,
        review: Uuid,
        caller: Uuid,
        query: ReviewTranscriptQuery,
    ) -> Result<ReviewTranscriptPage, AppError> {
        let assignment = assignment(db, source, review).await?;
        let target_access = scoped::user_can_access_workspace(
            &state.system_pool,
            db,
            caller,
            assignment.org_id,
            assignment.target_workspace_id,
        )
        .await?;
        let after = match query.cursor {
            Some(cursor) => {
                let bytes = URL_SAFE_NO_PAD
                    .decode(cursor)
                    .map_err(|_| unavailable("Invalid transcript cursor"))?;
                let cursor: Cursor = serde_json::from_slice(&bytes)
                    .map_err(|_| unavailable("Invalid transcript cursor"))?;
                if cursor.version != 1 || cursor.review != review || !(0..=2).contains(&cursor.kind)
                {
                    return Err(unavailable("Cursor does not belong to this review"));
                }
                cursor
            }
            None => Cursor {
                version: 1,
                review,
                kind: -1,
                position: 0,
                sequence: 0,
                id: Uuid::nil(),
            },
        };
        let limit = query.limit.unwrap_or(50).clamp(1, 200) as usize;
        let Some(task) = assignment.task_id else {
            return Ok(ReviewTranscriptPage {
                items: vec![],
                next_cursor: None,
                events_trimmed: false,
            });
        };
        let scope = g5_contracts::types::formal_review_read_scope(assignment.review_workspace_id);
        let mut reader =
            scoped::scoped_db_for_system_org(state, assignment.org_id, &[&scope]).await?;
        // set_ctx initialized the capability; all subsequent inspection queries
        // are physically read-only. Rollback avoids clear_ctx's commit-time write.
        sqlx::query(
            r#"
            SET TRANSACTION READ ONLY
            "#,
        )
        .execute(reader.conn())
        .await?;
        let rows: Vec<TranscriptRow> = sqlx::query_as(
            r#"
            WITH entries AS (
                SELECT 0::smallint AS kind, input_index AS position, 0::bigint AS sequence, id,
                    created_at, 'prompt'::text AS entry_type, left(content,65536) AS content,
                    '{}'::jsonb AS metadata, octet_length(content) > 65536 AS content_truncated
                FROM session_inputs WHERE task_id = $1
                UNION ALL
                SELECT 1::smallint, message_index::bigint, 0::bigint, id, created_at, role,
                    CASE WHEN role = 'tool' AND NOT $6 THEN $7 ELSE left(content,65536) END,
                    '{}'::jsonb, octet_length(content) > 65536
                FROM chat_messages WHERE session_id = (SELECT chat_session_id FROM tasks WHERE id = $1)
                UNION ALL
                SELECT 2::smallint, COALESCE(e.attempt,0)::bigint, COALESCE(e.seq,0), e.id,
                    e.created_at, e.entry_type,
                    -- Tool progress can carry raw content even when its timeline type
                    -- is checkpoint. Unknown tool kinds also require a safe call.
                    CASE WHEN $6 OR (e.entry_type <> 'tool_result'
                        AND e.metadata->>'kind' NOT LIKE 'tool.%') OR EXISTS (
                        SELECT 1 FROM task_events call
                        WHERE call.task_id = e.task_id AND call.attempt IS NOT DISTINCT FROM e.attempt
                            AND call.seq < e.seq AND call.metadata->>'kind' = 'tool.started'
                            AND call.metadata->'payload'->>'call_id' = e.metadata->'payload'->>'call_id'
                            AND (call.metadata->'payload'->>'name' LIKE '%write_report'
                                OR (call.metadata->'payload'->>'name' LIKE '%read_review'
                                    AND COALESCE(call.metadata->'payload'->'input'->>'cut','source') = 'source'
                                    AND call.metadata->'payload'->'input'->>'operation' IN ('file','paths')))
                    ) THEN left(e.content,65536) ELSE $7 END,
                    jsonb_strip_nulls(jsonb_build_object(
                        'kind', left(e.metadata->>'kind',128), 'attempt', e.attempt, 'seq', e.seq,
                        'callId', left(e.metadata->'payload'->>'call_id',512),
                        'toolName', left(e.metadata->'payload'->>'name',512),
                        'status', left(e.metadata->'payload'->>'status',128))),
                    octet_length(e.content) > 65536
                FROM task_events e WHERE e.task_id = $1
            )
            SELECT * FROM entries
            WHERE (kind,position,sequence,id) > ($2,$3,$4,$5)
            ORDER BY kind,position,sequence,id LIMIT $8
            "#,
        )
        .bind(task)
        .bind(after.kind)
        .bind(after.position)
        .bind(after.sequence)
        .bind(after.id)
        .bind(target_access)
        .bind(TARGET_HIDDEN)
        .bind((limit + 1) as i64)
        .fetch_all(reader.conn())
        .await?;
        let events_trimmed: bool = sqlx::query_scalar(
            r#"
            SELECT EXISTS (
                SELECT attempt FROM task_events WHERE task_id = $1 AND seq IS NOT NULL
                GROUP BY attempt HAVING min(seq) > 1 OR count(DISTINCT seq) <> max(seq)-min(seq)+1
            )
            "#,
        )
        .bind(task)
        .fetch_one(reader.conn())
        .await?;
        reader.rollback().await?;
        let mut page = ReviewTranscriptPage {
            items: vec![],
            next_cursor: None,
            events_trimmed,
        };
        let mut used = 1024; // Envelope, cursor and page metadata allowance.
        for row in rows {
            let cursor = Cursor {
                version: 1,
                review,
                kind: row.kind,
                position: row.position,
                sequence: row.sequence,
                id: row.id,
            };
            let (content, truncated) = bounded_content(row.content);
            let item = ReviewTranscriptItem {
                id: row.id,
                kind: match row.kind {
                    0 => "input",
                    1 => "message",
                    _ => "event",
                },
                created_at: row.created_at,
                entry_type: row.entry_type,
                content,
                content_truncated: row.content_truncated || truncated,
                metadata: row.metadata,
            };
            let bytes = serde_json::to_vec(&item)?.len() + 1;
            if page.items.len() == limit || used + bytes > PAGE_BYTES {
                // The cursor already names the last INCLUDED row. Retrying it
                // reads this row next, including after a byte-limited page.
                return Ok(page);
            }
            used += bytes;
            page.items.push(item);
            page.next_cursor = Some(URL_SAFE_NO_PAD.encode(serde_json::to_vec(&cursor)?));
        }
        page.next_cursor = None;
        Ok(page)
    }
}

async fn assignment(db: &mut ScopedDb, source: Uuid, review: Uuid) -> Result<Assignment, AppError> {
    sqlx::query_as(
        r#"
        SELECT org_id, target_workspace_id, review_workspace_id, task_id, invocation
        FROM agent_approval_reviews WHERE id = $1 AND source_workspace_id = $2
        "#,
    )
    .bind(review)
    .bind(source)
    .fetch_optional(db.conn())
    .await?
    .ok_or_else(|| {
        G5Error::NotFound {
            resource: "formal review inspection".into(),
        }
        .into()
    })
}
fn bounded_content(mut value: String) -> (String, bool) {
    if value.len() <= ITEM_CONTENT_BYTES {
        return (value, false);
    }
    let mut end = ITEM_CONTENT_BYTES;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value.truncate(end);
    (value, true)
}
fn unavailable(message: impl std::fmt::Display) -> AppError {
    G5Error::ValidationError {
        details: json!({"message":message.to_string()}),
    }
    .into()
}
fn git_error(error: g5_scm::native::NativeGitError) -> AppError {
    G5Error::GitError {
        message: error.to_string(),
    }
    .into()
}
