//! Review workspaces and review suggestions.
//!
//! The single-writer rule (`workspace_mutation_policy`) means a reviewer who
//! does not own a workspace cannot edit it, run its VM or send chat in it. The
//! Approvals page's Review Workspace action therefore forks the source into a
//! reviewer-owned review workspace: an ordinary durable checked fork whose
//! merge target and parent are the source, so Update merges the source's
//! branch through the in-VM `workspace.changes.update` operation.
//! Later reviews reopen that same workspace (files, flush receipt and chat
//! history persist across Stop/Resume).
//!
//! The review's deliverable is not a pull request. *Push review* records a
//! **publication** (one round of one reviewer's review of the source) holding
//! **findings** (the organising unit of a fix-first review), **suggestions**
//! (a hunk, a new file, a pure comment or an artifact) and
//! their threads (`comment_index` rows with scope `suggestion`). The source
//! owner acts on suggestions from the source workspace: accept writes the
//! change into the owner's running working tree, commits and pushes that file;
//! reject and reply record the decision in the thread.
//!
//! The review link itself is still the fork operation: a review fork is the
//! completed `workspace_forks` row whose idempotency key carries
//! [`REVIEW_FORK_KEY_PREFIX`]; VM state comes from `workspace_vms`.

use std::collections::HashMap;

use crate::services::git::repo_store::{RepoKey, ScopedRepoStorage, TenantRepoStorage};
use chrono::{DateTime, Utc};
use g5_authz::ScopedDb;
use g5_contracts::error::G5Error;
use g5_scm::{CommitDiffLineType, CommitFileDiff, CommitFileDiffStatus, GitRepo};

use crate::services::review_edits::{self, ReviewSuggestionEdit};
use crate::services::workspace_reader::{
    blob_id_at_commit_sync, file_size_at_commit_sync, read_file_at_commit_sync,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

pub use crate::services::identity::workspace_mutation_policy::{
    ensure_not_review_workspace, REVIEW_FORK_KEY_PREFIX,
};

/// How many `<source>-review-<reviewer>[-n]` names Start tries before giving
/// up on a branch-name collision in a member repository.
pub const MAX_REVIEW_NAME_ATTEMPTS: u32 = 5;

/// Longest workspace name the fork service accepts (`g5_git_branch_name_is_valid`).
pub const MAX_WORKSPACE_NAME_BYTES: usize = 255;

/// Idempotency key of the `attempt`-th name attempt in the `generation`-th
/// review fork of `source_workspace_id` by the requester. The generation is
/// the number of review forks the requester already made of this source
/// (see [`next_review_fork_generation`]), so a deleted or transferred review
/// workspace never replays its completed operation; attempts are distinct so
/// an aborted name attempt never shadows the next one.
pub fn review_fork_idempotency_key(
    source_workspace_id: Uuid,
    generation: i64,
    attempt: u32,
) -> String {
    format!("{REVIEW_FORK_KEY_PREFIX}{source_workspace_id}:g{generation}:{attempt}")
}

/// One reviewer's review workspace for one source workspace, derived from
/// the completed review fork.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceReviewRecord {
    /// The `workspace_forks` row that created the review workspace.
    pub fork_id: Uuid,
    pub org_id: Uuid,
    pub source_workspace_id: Uuid,
    pub source_workspace_name: String,
    pub review_workspace_id: Uuid,
    pub review_workspace_name: String,
    pub reviewer_user_id: Uuid,
    /// Lifecycle status of the review workspace's VM (`workspace_vms.status`),
    /// `stopped` when no incarnation row exists.
    pub vm_status: String,
    pub vm_started_at: Option<DateTime<Utc>>,
    pub vm_last_activity_at: Option<DateTime<Utc>>,
    /// When the review workspace's state was last flushed durably (the
    /// receipt Stop leaves behind and Reopen restores from).
    pub last_flushed_at: Option<DateTime<Utc>>,
    /// When the review workspace was forked.
    pub created_at: DateTime<Utc>,
}

const REVIEW_SELECT: &str = r#"
SELECT f.id AS fork_id,
       f.org_id,
       f.source_workspace_id,
       s.name AS source_workspace_name,
       f.target_workspace_id AS review_workspace_id,
       w.name AS review_workspace_name,
       f.requested_by AS reviewer_user_id,
       COALESCE(vm.status, 'stopped') AS vm_status,
       vm.started_at AS vm_started_at,
       vm.last_activity_at AS vm_last_activity_at,
       fl.complete_at AS last_flushed_at,
       COALESCE(f.completed_at, f.created_at) AS created_at
FROM workspace_forks f
JOIN workspaces s ON s.org_id = f.org_id AND s.id = f.source_workspace_id
JOIN workspaces w ON w.org_id = f.org_id AND w.id = f.target_workspace_id AND w.deleted_at IS NULL
LEFT JOIN workspace_vms vm ON vm.workspace_id = f.target_workspace_id
LEFT JOIN workspace_flushes fl ON fl.org_id = vm.org_id AND fl.flush_id = vm.last_flush_id
WHERE f.org_id = $1
  AND f.status = 'completed'
  AND left(f.idempotency_key, $2) = $3
  AND f.idempotency_key NOT LIKE 'review-workspace:%:platform:%'
"#;

/// Build the review workspace name for `attempt` (1-based). The source name is
/// already a valid Git branch name and the suffix keeps it valid. The result
/// is never a slash-boundary ancestor or descendant of the source name, so it
/// cannot collide with it under the checked-fork prefix rule. A source name
/// too long to carry the suffix is shortened so the result stays within
/// [`MAX_WORKSPACE_NAME_BYTES`] and still valid.
pub fn review_workspace_name(
    source_name: &str,
    reviewer_display_name: &str,
    attempt: u32,
) -> String {
    let mut slug = String::new();
    let mut last_dash = true;
    for ch in reviewer_display_name.chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch);
            last_dash = false;
        } else if !last_dash {
            slug.push('-');
            last_dash = true;
        }
        if slug.len() >= 32 {
            break;
        }
    }
    let slug = slug.trim_matches('-');
    let slug = if slug.is_empty() { "reviewer" } else { slug };
    let mut suffix = format!("-review-{slug}");
    if attempt > 1 {
        suffix.push_str(&format!("-{attempt}"));
    }
    let budget = MAX_WORKSPACE_NAME_BYTES.saturating_sub(suffix.len());
    let mut base = source_name;
    if base.len() > budget {
        let mut cut = budget;
        while cut > 0 && !base.is_char_boundary(cut) {
            cut -= 1;
        }
        base = &base[..cut];
        // A cut can leave a trailing `.`, `/`, `-` or a `.lock` segment end,
        // which the branch-name rule rejects; trim until it is valid again.
        loop {
            let trimmed = base.trim_end_matches(['.', '/', '-']);
            let trimmed = trimmed.strip_suffix(".lock").unwrap_or(trimmed);
            if trimmed.len() == base.len() {
                break;
            }
            base = trimmed;
        }
    }
    let base = if base.is_empty() { "workspace" } else { base };
    format!("{base}{suffix}")
}

/// The generation of the requester's next review fork of
/// `source_workspace_id`: how many of their earlier review forks are retired
/// (aborted, or completed with a workspace since deleted or transferred).
///
/// Only retired forks count, so two overlapping Starts that both find no
/// usable review compute the same generation and the same idempotency key;
/// the fork service then converges them on one operation instead of
/// creating two review workspaces. A usable review never reaches the fork.
pub async fn next_review_fork_generation(
    db: &mut ScopedDb,
    org_id: Uuid,
    source_workspace_id: Uuid,
    reviewer_user_id: Uuid,
) -> Result<i64, G5Error> {
    Ok(sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*)
         FROM workspace_forks f
         LEFT JOIN workspaces w ON w.org_id = f.org_id AND w.id = f.target_workspace_id
         WHERE f.org_id = $1 AND f.source_workspace_id = $2 AND f.requested_by = $3
           AND left(f.idempotency_key, $4) = $5
           AND (
                f.status = 'aborted'
             OR (f.status = 'completed'
                 AND (w.id IS NULL
                      OR w.deleted_at IS NOT NULL
                      OR w.owner_user_id IS DISTINCT FROM f.requested_by))
           )",
    )
    .bind(org_id)
    .bind(source_workspace_id)
    .bind(reviewer_user_id)
    .bind(REVIEW_FORK_KEY_PREFIX.len() as i32)
    .bind(REVIEW_FORK_KEY_PREFIX)
    .fetch_one(db.conn())
    .await?)
}

/// The caller's review workspace of `source_workspace_id`, if they forked one
/// that still exists and still own it. A review workspace transferred to
/// someone else is no longer the caller's review: the fork requester is
/// immutable, so ownership is checked on the workspace row.
pub async fn find_review_for_source(
    db: &mut ScopedDb,
    org_id: Uuid,
    source_workspace_id: Uuid,
    reviewer_user_id: Uuid,
) -> Result<Option<WorkspaceReviewRecord>, G5Error> {
    let sql = format!(
        "{REVIEW_SELECT}
           AND f.source_workspace_id = $4
           AND f.requested_by = $5
           AND w.owner_user_id = $5
         ORDER BY COALESCE(f.completed_at, f.created_at) DESC
         LIMIT 1"
    );
    Ok(sqlx::query_as::<_, WorkspaceReviewRecord>(&sql)
        .bind(org_id)
        .bind(REVIEW_FORK_KEY_PREFIX.len() as i32)
        .bind(REVIEW_FORK_KEY_PREFIX)
        .bind(source_workspace_id)
        .bind(reviewer_user_id)
        .fetch_optional(db.conn())
        .await?)
}

/// The review that `review_workspace_id` belongs to, if it is a review
/// workspace.
pub async fn find_review_by_review_workspace(
    db: &mut ScopedDb,
    org_id: Uuid,
    review_workspace_id: Uuid,
) -> Result<Option<WorkspaceReviewRecord>, G5Error> {
    let sql = format!(
        "{REVIEW_SELECT}
           AND f.target_workspace_id = $4
         ORDER BY COALESCE(f.completed_at, f.created_at) DESC
         LIMIT 1"
    );
    Ok(sqlx::query_as::<_, WorkspaceReviewRecord>(&sql)
        .bind(org_id)
        .bind(REVIEW_FORK_KEY_PREFIX.len() as i32)
        .bind(REVIEW_FORK_KEY_PREFIX)
        .bind(review_workspace_id)
        .fetch_optional(db.conn())
        .await?)
}

/// Every live review workspace of `source_workspace_id` (any reviewer).
pub async fn list_reviews_of_source(
    db: &mut ScopedDb,
    org_id: Uuid,
    source_workspace_id: Uuid,
) -> Result<Vec<WorkspaceReviewRecord>, G5Error> {
    let sql = format!(
        "{REVIEW_SELECT}
           AND f.source_workspace_id = $4
           AND w.owner_user_id = f.requested_by
         ORDER BY COALESCE(f.completed_at, f.created_at) DESC"
    );
    Ok(sqlx::query_as::<_, WorkspaceReviewRecord>(&sql)
        .bind(org_id)
        .bind(REVIEW_FORK_KEY_PREFIX.len() as i32)
        .bind(REVIEW_FORK_KEY_PREFIX)
        .bind(source_workspace_id)
        .fetch_all(db.conn())
        .await?)
}

/// The reviewer's display name. `users` is readable only through the system
/// pool (it carries no tenant grant), so this is looked up separately from
/// the tenant-scoped review query.
pub async fn reviewer_display_name(
    system_pool: &PgPool,
    reviewer_user_id: Uuid,
) -> Result<String, G5Error> {
    let name: Option<String> = sqlx::query_scalar("SELECT display_name FROM users WHERE id = $1")
        .bind(reviewer_user_id)
        .fetch_optional(system_pool)
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to resolve reviewer display name: {e}"),
        })?;
    Ok(name.unwrap_or_default())
}

/// The reviewer's short handle used as the publication's author label.
pub async fn reviewer_handle(
    system_pool: &PgPool,
    reviewer_user_id: Uuid,
) -> Result<String, G5Error> {
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT email, display_name FROM users WHERE id = $1")
            .bind(reviewer_user_id)
            .fetch_optional(system_pool)
            .await
            .map_err(|e| G5Error::Internal {
                message: format!("Failed to resolve reviewer handle: {e}"),
            })?;
    let (email, display) = row.unwrap_or_default();
    let local_part = email.split('@').next().unwrap_or("").trim().to_string();
    let raw = if local_part.is_empty() {
        display
    } else {
        local_part
    };
    let raw = if raw.trim().is_empty() {
        "reviewer".to_string()
    } else {
        raw
    };
    let mut handle = String::new();
    let mut last_dash = true;
    for ch in raw.chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            handle.push(ch);
            last_dash = false;
        } else if !last_dash {
            handle.push('-');
            last_dash = true;
        }
        if handle.len() >= 32 {
            break;
        }
    }
    let handle = handle.trim_matches('-');
    Ok(if handle.is_empty() {
        "reviewer".to_string()
    } else {
        handle.to_string()
    })
}

/// Name of a live workspace in `org_id`, or `NotFound`.
pub async fn workspace_name(
    db: &mut ScopedDb,
    org_id: Uuid,
    workspace_id: Uuid,
) -> Result<String, G5Error> {
    sqlx::query_scalar::<_, String>(
        "SELECT name FROM workspaces WHERE org_id = $1 AND id = $2 AND deleted_at IS NULL",
    )
    .bind(org_id)
    .bind(workspace_id)
    .fetch_optional(db.conn())
    .await?
    .ok_or_else(|| G5Error::NotFound {
        resource: format!("workspace {workspace_id}"),
    })
}

/// Owner and branch name of a live workspace.
/// The workspace's current owner and the branch it owns in its repositories
/// (`g5.workspace.branch`), which a rename never changes. Base owns each
/// repository's default branch and reports its name instead; review sources
/// are never Base. For a label, use [`workspace_name`].
pub async fn workspace_owner_and_branch(
    db: &mut ScopedDb,
    org_id: Uuid,
    workspace_id: Uuid,
) -> Result<(Option<Uuid>, String), G5Error> {
    sqlx::query_as::<_, (Option<Uuid>, String)>(
        "SELECT owner_user_id, COALESCE(branch_name, name)
         FROM workspaces WHERE org_id = $1 AND id = $2 AND deleted_at IS NULL",
    )
    .bind(org_id)
    .bind(workspace_id)
    .fetch_optional(db.conn())
    .await?
    .ok_or_else(|| G5Error::NotFound {
        resource: format!("workspace {workspace_id}"),
    })
}

/// How long an overlapping Start waits for the one in progress before it is
/// refused; the fork itself takes seconds, so a wait this long means the
/// other request is stuck and the caller should retry later.
pub const REVIEW_ADMISSION_WAIT: std::time::Duration = std::time::Duration::from_secs(60);

/// At most this many review admissions per replica hold a system-pool
/// connection at once. One is enough: a Start's fork phase takes seconds
/// and the system pool is small; the others wait without a connection.
const MAX_REVIEW_ADMISSIONS_IN_FLIGHT: usize = 1;

fn review_admissions() -> &'static tokio::sync::Semaphore {
    static ADMISSIONS: std::sync::OnceLock<tokio::sync::Semaphore> = std::sync::OnceLock::new();
    ADMISSIONS.get_or_init(|| tokio::sync::Semaphore::new(MAX_REVIEW_ADMISSIONS_IN_FLIGHT))
}

/// Holds the per-`(source, reviewer)` advisory lock (a transaction on the
/// system pool) and the per-replica admission permit for the duration of a
/// Start's lookup-and-fork phase. Dropping it rolls the transaction back,
/// releasing both, so a cancelled request never leaks the lock.
pub struct ReviewAdmission {
    _tx: sqlx::Transaction<'static, sqlx::Postgres>,
    _permit: tokio::sync::SemaphorePermit<'static>,
}

/// Serialize Start per `(source, reviewer)`. The advisory lock is only ever
/// *tried*: a blocking wait would pin a pool connection while another
/// request holds the lock and needs the pool itself, so a loser releases its
/// transaction and permit and polls instead. Both the permit wait and the
/// lock wait share [`REVIEW_ADMISSION_WAIT`] and expire into a retryable
/// conflict.
pub async fn lock_review_admission(
    pool: &PgPool,
    source_workspace_id: Uuid,
    reviewer_user_id: Uuid,
) -> Result<ReviewAdmission, G5Error> {
    let key = format!("review-workspace:{source_workspace_id}:{reviewer_user_id}");
    let deadline = tokio::time::Instant::now() + REVIEW_ADMISSION_WAIT;
    let busy = || G5Error::Conflict {
        reason: "another review start for this workspace is still in progress; try again"
            .to_string(),
    };
    loop {
        let permit = match tokio::time::timeout_at(deadline, review_admissions().acquire()).await {
            Ok(Ok(permit)) => permit,
            Ok(Err(_)) => {
                return Err(G5Error::Internal {
                    message: "review admission semaphore closed".to_string(),
                })
            }
            Err(_elapsed) => return Err(busy()),
        };
        let mut tx = pool.begin().await?;
        let locked: bool = sqlx::query_scalar("SELECT pg_try_advisory_xact_lock(hashtext($1))")
            .bind(&key)
            .fetch_one(&mut *tx)
            .await?;
        if locked {
            return Ok(ReviewAdmission {
                _tx: tx,
                _permit: permit,
            });
        }
        drop(tx);
        drop(permit);
        if tokio::time::Instant::now() >= deadline {
            return Err(busy());
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
}

/// A review fork operation that is still pending or aborting: the claim it
/// holds must be resumed with its original inputs, never re-planned.
#[derive(Debug, Clone)]
pub struct UnfinishedReviewFork {
    pub idempotency_key: String,
    pub proposed_name: String,
}

/// The requester's unfinished review fork of `source_workspace_id`, if any.
/// A retry must resubmit the recorded key and name: the fork service binds
/// the request digest to the key, so re-deriving the name from a display
/// name that changed since would be refused as a reused key forever.
pub async fn find_unfinished_review_fork(
    db: &mut ScopedDb,
    org_id: Uuid,
    source_workspace_id: Uuid,
    reviewer_user_id: Uuid,
) -> Result<Option<UnfinishedReviewFork>, G5Error> {
    let row = sqlx::query_as::<_, (String, String)>(
        "SELECT idempotency_key, proposed_name
         FROM workspace_forks
         WHERE org_id = $1 AND source_workspace_id = $2 AND requested_by = $3
           AND left(idempotency_key, $4) = $5
           AND idempotency_key NOT LIKE 'review-workspace:%:platform:%'
           AND status IN ('pending', 'aborting')
         ORDER BY created_at DESC
         LIMIT 1",
    )
    .bind(org_id)
    .bind(source_workspace_id)
    .bind(reviewer_user_id)
    .bind(REVIEW_FORK_KEY_PREFIX.len() as i32)
    .bind(REVIEW_FORK_KEY_PREFIX)
    .fetch_optional(db.conn())
    .await?;
    Ok(
        row.map(|(idempotency_key, proposed_name)| UnfinishedReviewFork {
            idempotency_key,
            proposed_name,
        }),
    )
}

// ─── Publications ────────────────────────────────────────────────────────────

/// One round of one reviewer's review of a source workspace.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ReviewPublicationRecord {
    pub id: Uuid,
    pub org_id: Uuid,
    pub source_workspace_id: Uuid,
    /// `None` for a comments-only publication made without a review workspace.
    pub review_workspace_id: Option<Uuid>,
    pub reviewer_user_id: Uuid,
    /// `human` or `platform`.
    pub author_kind: String,
    /// The reviewer's short handle.
    pub author_label: String,
    pub round: i32,
    /// `[{repoId, sourceHead, reviewHead}]` the publication was computed from.
    pub base_commit_vector: Value,
    pub summary: String,
    /// `approve`, `request_changes`, `comment_only` or none.
    pub verdict: Option<String>,
    /// Eight-dimension review scores, when the reviewer supplied them.
    pub scores: Option<Value>,
    pub superseded_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    /// `round` or `annotation` (see `NewPublication::kind`).
    pub kind: String,
}

/// A publication with its suggestion counts by status.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ReviewPublicationSummary {
    #[sqlx(flatten)]
    #[serde(flatten)]
    pub publication: ReviewPublicationRecord,
    pub open_count: i64,
    pub accepted_count: i64,
    pub rejected_count: i64,
    pub resolved_count: i64,
    pub superseded_count: i64,
    pub artifact_count: i64,
}

/// One review finding: the organising unit of a fix-first review.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ReviewFindingRecord {
    pub id: Uuid,
    pub publication_id: Uuid,
    pub title: String,
    pub severity: String,
    pub dimension: String,
    pub evidence: String,
    /// Structured grounding review actions; never a ledger diff.
    pub actions: Value,
    pub status: String,
    pub position: i32,
    pub created_at: DateTime<Utc>,
    /// Where the finding points: the repository (when named) and the
    /// repository-relative paths, as submitted.
    pub repo_id: Option<Uuid>,
    pub paths: Value,
}

// Keep prepared result shapes independent of retired columns. In particular,
// blob_id can be dropped only after these explicit projections have shipped
// and connections prepared by the previous release have been drained (#903).
pub const REVIEW_SUGGESTION_COLUMNS: &str = "id, publication_id, finding_id, repo_id, path, kind, before_lines, after_lines, anchor_start, content_hash, anchor_context, author_kind, author_label, note, comment_id, status, status_by, status_at, supersedes, adopted_from, created_at, blob_oid, blob_sha256, bytes, content_type, edits";
const QUALIFIED_REVIEW_SUGGESTION_COLUMNS: &str = "s.id, s.publication_id, s.finding_id, s.repo_id, s.path, s.kind, s.before_lines, s.after_lines, s.anchor_start, s.content_hash, s.anchor_context, s.author_kind, s.author_label, s.note, s.comment_id, s.status, s.status_by, s.status_at, s.supersedes, s.adopted_from, s.created_at, s.blob_oid, s.blob_sha256, s.bytes, s.content_type, s.edits";

/// One suggestion: a hunk, a new file, a comment or an artifact.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ReviewSuggestionRecord {
    pub id: Uuid,
    pub publication_id: Uuid,
    pub finding_id: Option<Uuid>,
    pub repo_id: Option<Uuid>,
    pub path: Option<String>,
    pub kind: String,
    pub before_lines: Option<String>,
    pub after_lines: Option<String>,
    pub anchor_start: Option<i32>,
    pub content_hash: Option<String>,
    pub anchor_context: Option<String>,
    pub author_kind: String,
    pub author_label: String,
    pub note: String,
    pub comment_id: Option<Uuid>,
    pub status: String,
    pub status_by: Option<Uuid>,
    pub status_at: Option<DateTime<Utc>>,
    pub supersedes: Option<Uuid>,
    pub adopted_from: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    /// artifact: Git blob id of the bytes in the repository's review
    /// artifact store (`TenantRepoStorage::artifact_dir`, outside the
    /// repository and its blob namespace).
    pub blob_oid: Option<String>,
    pub blob_sha256: Option<String>,
    pub bytes: Option<i64>,
    pub content_type: Option<String>,
    /// Stable independently decided source edits; parent source/proposal stay immutable.
    #[sqlx(json)]
    pub edits: Vec<ReviewSuggestionEdit>,
    /// Common current source projection. Child offsets refer to these bytes;
    /// it is derived from captured source and accepted receipts, never stored.
    #[sqlx(skip)]
    #[serde(default)]
    pub expected_before_lines: String,
    /// The reviewer who published it, so the file card can offer them
    /// Withdraw (`g5.governance.review-suggestions.display`). Filled by the
    /// source listing; other reads leave it out.
    #[sqlx(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reviewer_user_id: Option<Uuid>,
}

/// A finding as submitted with Push review.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewFinding {
    pub title: String,
    #[serde(default = "default_severity")]
    pub severity: String,
    #[serde(default)]
    pub dimension: String,
    #[serde(default)]
    pub evidence: String,
    #[serde(default)]
    pub actions: Value,
    /// Repository-relative paths whose suggestions belong to this finding.
    #[serde(default)]
    pub paths: Vec<String>,
    /// The repository those paths live in. Without it the finding claims
    /// the path in every repository of the workspace, which is only
    /// ambiguous when the same relative path changed in two of them.
    #[serde(default)]
    pub repo_id: Option<Uuid>,
}

/// Which finding a generated suggestion belongs to, keyed by repository
/// and path; a `None` repository is a finding submitted without one.
pub type FindingPaths = HashMap<(Option<Uuid>, String), Uuid>;

/// Builds the association from the submitted findings, in the order the
/// findings were stored (`stored[i]` is `submitted[i]`).
pub fn finding_paths(submitted: &[NewFinding], stored: &[ReviewFindingRecord]) -> FindingPaths {
    let mut out = FindingPaths::new();
    for (finding, input) in stored.iter().zip(submitted.iter()) {
        for path in &input.paths {
            out.insert((input.repo_id, path.clone()), finding.id);
        }
    }
    out
}

/// The finding for a suggestion in `repo_id` at `path`: a finding that
/// names the repository wins over one that only names the path.
pub fn finding_for(paths: &FindingPaths, repo_id: Uuid, path: &str) -> Option<Uuid> {
    paths
        .get(&(Some(repo_id), path.to_string()))
        .or_else(|| paths.get(&(None, path.to_string())))
        .copied()
}

fn default_severity() -> String {
    "P2".to_string()
}

/// A hunk or new file computed from the review branch against the source.
/// The key a reviewer's tools use to name one change of a round before it
/// is published: a digest of the repository, the path, the change's before
/// and after text and its anchor context (the lines around it), plus its
/// occurrence among changes identical in all of those. Two identical hunks
/// at different places differ by context, so removing one never hands its
/// key to the other; only hunks identical text and context alike (which
/// nothing could tell apart) share the occurrence tiebreak. Stable across
/// previews as long as the change itself is, so a note written or an
/// exclusion ticked on a preview still names the same change when Push
/// review regenerates the round.
pub fn change_key(
    repo_id: Uuid,
    path: &str,
    before_lines: Option<&str>,
    after_lines: &str,
    anchor_context: Option<&str>,
    blob_digest: Option<&str>,
    occurrence: u32,
) -> String {
    let mut digest = Sha256::new();
    digest.update(repo_id.as_bytes());
    digest.update(path.as_bytes());
    digest.update([0u8]);
    digest.update(before_lines.unwrap_or_default().as_bytes());
    digest.update([0u8]);
    digest.update(after_lines.as_bytes());
    digest.update([0u8]);
    digest.update(anchor_context.unwrap_or_default().as_bytes());
    // An artifact has no text of its own: its bytes are its identity, so a
    // file replaced after the preview no longer answers to the old key
    // (publish and discard then refuse it until the reviewer previews
    // again). A hunk or a new file adds nothing here, so their keys are as
    // they were.
    if let Some(blob_digest) = blob_digest {
        digest.update([0u8]);
        digest.update(blob_digest.as_bytes());
    }
    digest.update([0u8]);
    digest.update(occurrence.to_le_bytes());
    format!("{:x}", digest.finalize())[..32].to_string()
}

/// The digest that identifies an artifact's bytes (`blob_sha256`, else the
/// blob id); `None` for a hunk or a new file, whose text identifies them.
fn artifact_blob_digest(item: &GeneratedSuggestion) -> Option<&str> {
    if item.kind != "artifact" {
        return None;
    }
    item.blob_sha256.as_deref().or(item.blob_oid.as_deref())
}

/// What tells one change of a round from another: repository, path, before
/// and after text, anchor context, and an artifact's bytes.
type ChangeIdentity<'a> = (
    Uuid,
    &'a str,
    Option<&'a str>,
    &'a str,
    Option<&'a str>,
    Option<&'a str>,
);

/// The keys of a generated round's changes, in generation order (the order
/// the preview lists and publish regenerates).
pub fn change_keys(items: &[GeneratedSuggestion]) -> Vec<String> {
    let mut seen: HashMap<ChangeIdentity<'_>, u32> = HashMap::new();
    items
        .iter()
        .map(|item| {
            let blob_digest = artifact_blob_digest(item);
            let identity = (
                item.repo_id,
                item.path.as_str(),
                item.before_lines.as_deref(),
                item.after_lines.as_str(),
                item.anchor_context.as_deref(),
                blob_digest,
            );
            let occurrence = seen.entry(identity).or_insert(0);
            let key = change_key(
                item.repo_id,
                &item.path,
                item.before_lines.as_deref(),
                &item.after_lines,
                item.anchor_context.as_deref(),
                blob_digest,
                *occurrence,
            );
            *occurrence += 1;
            key
        })
        .collect()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratedSuggestion {
    pub repo_id: Uuid,
    pub path: String,
    /// `hunk`, `new_file` or `artifact`.
    pub kind: String,
    pub before_lines: Option<String>,
    pub after_lines: String,
    pub anchor_start: Option<i32>,
    pub content_hash: Option<String>,
    pub anchor_context: Option<String>,
    /// artifact: where the bytes live and what they are.
    pub blob_oid: Option<String>,
    pub blob_sha256: Option<String>,
    pub bytes: Option<i64>,
    pub content_type: Option<String>,
}

/// Largest artifact Push review or an upload stores.
pub const MAX_ARTIFACT_BYTES: usize = 10 * 1024 * 1024;

/// How Push review materializes the review's changes.
#[derive(Debug, Clone, Default)]
pub struct GenerateOptions {
    /// Write artifact bytes into the artifact store (false for a preview).
    pub store_artifacts: bool,
}

/// The bytes of `path` in a member repository of a review copy, as published
/// on its review branch (what Push review sends; the caller saves the copy
/// first). Read from the mirror, so an artifact of any size up to
/// `MAX_ARTIFACT_BYTES` is attachable; larger is refused before the read.
pub async fn read_review_copy_file(
    db: &mut ScopedDb,
    repo_storage: &TenantRepoStorage,
    review_workspace_id: Uuid,
    repo_id: Uuid,
    path: &str,
) -> Result<Vec<u8>, G5Error> {
    let member = crate::services::git::workspace_repositories::load_workspace_repositories(
        db.conn(),
        review_workspace_id,
        false,
    )
    .await?
    .into_iter()
    .find(|member| member.repo_id == repo_id)
    .ok_or_else(|| G5Error::NotFound {
        resource: format!("review repository {repo_id}"),
    })?;
    let key = TenantRepoStorage::key(&*db, member.org_id, repo_id)?;
    let branch = member.branch_name;
    let head = crate::services::workspace_reader::read_branch_head_oid(
        &repo_storage.scoped(&*db),
        member.org_id,
        repo_id,
        &branch,
    )
    .await?
    .ok_or_else(|| G5Error::NotFound {
        resource: format!("review branch {branch}"),
    })?;
    let size = file_size_at_commit_sync(repo_storage, &key, &head, path)?;
    if size > MAX_ARTIFACT_BYTES as u64 {
        return Err(G5Error::ValidationError {
            details: json!({"path": format!(
                "artifact must be at most {} bytes", MAX_ARTIFACT_BYTES
            )}),
        });
    }
    read_file_at_commit_sync(repo_storage, &key, &head, path)
}

/// The bytes of `path` in the review copy's saved tree (`review_head` of a
/// generated round's `base_commit_vector`): what the round's changes were
/// computed from. A discard compares the copy's live file with this before
/// writing, so a save-to-read race (an editor or agent changing the file in
/// between) never deletes or rewrites bytes the preview did not describe.
pub fn saved_copy_bytes(
    repo_storage: &TenantRepoStorage,
    key: &RepoKey,
    review_head: &str,
    path: &str,
) -> Result<Vec<u8>, G5Error> {
    read_file_at_commit_sync(repo_storage, key, review_head, path)
}

/// The git blob id of `path` in the review copy's saved tree, from the
/// object header alone: what a discard compares the copy's live file
/// against when only the file's identity matters (a new file or an
/// artifact, which can be far larger than a runtime reply).
pub fn saved_copy_blob_id(
    repo_storage: &TenantRepoStorage,
    key: &RepoKey,
    review_head: &str,
    path: &str,
) -> Result<String, G5Error> {
    blob_id_at_commit_sync(repo_storage, key, review_head, path)
}

/// The media type for an artifact file name, when the reviewer gave none.
pub fn guess_content_type(path: &str) -> &'static str {
    let ext = path
        .rsplit('.')
        .next()
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    match ext.as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "md" | "markdown" => "text/markdown; charset=utf-8",
        "txt" => "text/plain; charset=utf-8",
        "json" => "application/json",
        "csv" => "text/csv; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "pdf" => "application/pdf",
        _ => "application/octet-stream",
    }
}

/// Where an artifact's bytes live: the repository's review artifact
/// directory, content-addressed by git blob id. That directory is outside
/// the repository root, so nothing that reads a repository's Git objects
/// can reach it: an artifact of a restricted workspace is served only by
/// `download_review_artifact`, which checks access to the workspace.
fn artifact_path(
    repo_storage: &TenantRepoStorage,
    key: &RepoKey,
    oid: &str,
) -> Result<std::path::PathBuf, G5Error> {
    let dir = repo_storage
        .artifact_dir(key)
        .ok_or_else(|| G5Error::ServiceUnavailable {
            message: format!("Repository {} has no review artifact store", key.repo_id()),
        })?;
    Ok(dir.join(&oid[..2]).join(&oid[2..4]).join(oid))
}

/// Store artifact bytes in the repository's review artifact store and
/// return `(git blob id, sha256)`. Idempotent: the same bytes map to the
/// same path.
pub fn store_review_artifact(
    repo_storage: &TenantRepoStorage,
    key: &RepoKey,
    bytes: &[u8],
) -> Result<(String, String), G5Error> {
    if bytes.len() > MAX_ARTIFACT_BYTES {
        return Err(G5Error::ValidationError {
            details: json!({"bytes": format!("artifact exceeds {} bytes", MAX_ARTIFACT_BYTES)}),
        });
    }
    let oid = g5_scm::hash_blob(bytes).map_err(|e| G5Error::Internal {
        message: format!("hashing artifact: {e}"),
    })?;
    let sha256 = format!("{:x}", Sha256::digest(bytes));
    let target = artifact_path(repo_storage, key, &oid)?;
    if !target.exists() {
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| G5Error::Internal {
                message: format!("creating artifact store: {e}"),
            })?;
        }
        // Two writers of the same bytes (the same artifact uploaded twice,
        // or pushed by two reviewers) must not share a temporary file: each
        // writes its own and the rename that loses the race finds the
        // content already published, which is the outcome it wanted.
        let tmp = target.with_extension(format!("{}.tmp", Uuid::new_v4().simple()));
        std::fs::write(&tmp, bytes).map_err(|e| G5Error::Internal {
            message: format!("writing artifact: {e}"),
        })?;
        if let Err(e) = std::fs::rename(&tmp, &target) {
            let _ = std::fs::remove_file(&tmp);
            if !target.exists() {
                return Err(G5Error::Internal {
                    message: format!("publishing artifact: {e}"),
                });
            }
        }
    }
    Ok((oid, sha256))
}

/// Read artifact bytes back from the repository's blob store.
pub fn read_review_artifact(
    repo_storage: &TenantRepoStorage,
    key: &RepoKey,
    oid: &str,
) -> Result<Vec<u8>, G5Error> {
    if oid.len() != 40 || !oid.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(G5Error::ValidationError {
            details: json!({"blobOid": "must be a 40-hex Git blob id"}),
        });
    }
    std::fs::read(artifact_path(repo_storage, key, oid)?).map_err(|_| G5Error::NotFound {
        resource: format!("artifact {oid}"),
    })
}

/// A changed file that could not become a suggestion (binary, too large, not UTF-8).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkippedFile {
    pub repo_id: Uuid,
    pub path: String,
    pub reason: String,
}

/// What Push review computed from the review branch.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratedReview {
    pub suggestions: Vec<GeneratedSuggestion>,
    pub skipped: Vec<SkippedFile>,
    pub base_commit_vector: Vec<Value>,
}

/// Longest anchor context stored with a suggestion (`review_suggestions_anchor_context_size`).
const MAX_ANCHOR_CONTEXT_BYTES: usize = 4096;
/// A new file proposed *into the repository* travels inline as text in the
/// suggestion row and on every poll; the same 100 KiB guardrail as a
/// commit file diff (`g5_scm` `MAX_COMMIT_FILE_DIFF_BYTES`). Larger files
/// stay artifacts.
const MAX_NEW_FILE_SUGGESTION_BYTES: usize = 100 * 1024;
/// Context lines kept on each side of a hunk for relocation.
const ANCHOR_CONTEXT_LINES: usize = 3;
/// Format of `anchor_context`: 2 = lines outside the replaced block
/// (`anchor_context_for`); 1 (the block's own edge rows) is ignored.
const ANCHOR_CONTEXT_VERSION: u64 = 2;

/// Compute the review's changes: every member repository's review branch
/// against the source's branch, read from the published refs in the mirrors
/// (only pushed work is reviewed; the reviewer is warned about unpushed work
/// before publishing).
pub async fn generate_review_changes(
    db: &mut ScopedDb,
    repo_storage: &TenantRepoStorage,
    review: &WorkspaceReviewRecord,
    options: &GenerateOptions,
) -> Result<GeneratedReview, G5Error> {
    let review_members = crate::services::git::workspace_repositories::load_workspace_repositories(
        db.conn(),
        review.review_workspace_id,
        false,
    )
    .await?;
    let source_members = crate::services::git::workspace_repositories::load_workspace_repositories(
        db.conn(),
        review.source_workspace_id,
        false,
    )
    .await?;
    let pairs = review_members
        .into_iter()
        .map(|member| {
            let source = source_members
                .iter()
                .find(|source| source.repo_id == member.repo_id)
                .ok_or_else(|| G5Error::Conflict {
                    reason: "Review and source repository membership differ".into(),
                })?;
            Ok((
                member.repo_id,
                member.branch_name,
                source.branch_name.clone(),
            ))
        })
        .collect::<Result<Vec<_>, G5Error>>()?;

    let mut generated = GeneratedReview {
        suggestions: Vec::new(),
        skipped: Vec::new(),
        base_commit_vector: Vec::new(),
    };
    for (repo_id, review_branch, source_branch) in pairs {
        let key = TenantRepoStorage::key(&*db, review.org_id, repo_id)?;
        let repo_path = repo_storage.path(&key)?;
        let repo = GitRepo::open(&repo_path)?;
        let source_head =
            repo.read_branch_oid(&source_branch)?
                .ok_or_else(|| G5Error::Conflict {
                    reason: format!("repository {repo_id} has no branch {source_branch}"),
                })?;
        let review_head =
            repo.read_branch_oid(&review_branch)?
                .ok_or_else(|| G5Error::Conflict {
                    reason: format!("repository {repo_id} has no branch {review_branch}"),
                })?;
        // Only the reviewer's own commits are proposed: the review branch is
        // compared with its common ancestor with the source, never with the
        // source head itself. Comparing heads directly would turn every
        // commit the owner made since the fork into an inverse suggestion
        // that applies cleanly and undoes it.
        let merge_base = if source_head == review_head {
            review_head.clone()
        } else {
            repo.merge_base(&source_head, &review_head)?
        };
        generated.base_commit_vector.push(json!({
            "repoId": repo_id,
            "sourceBranch": source_branch,
            "sourceHead": source_head,
            "reviewBranch": review_branch,
            "reviewHead": review_head,
            "mergeBase": merge_base,
        }));
        if merge_base == review_head {
            continue;
        }
        for diff in repo.file_diffs_between_commits(&merge_base, &review_head)? {
            let path = diff.path.clone();
            // Classify by the tree entry first: a deletion has no entry, and
            // a gitlink, symlink or executable cannot be reproduced by the
            // plain text write that accept performs. None of these may
            // reach a blob read (a submodule's object is not in this repo).
            match repo.file_mode(&review_head, &path) {
                Err(G5Error::NotFound { .. }) => {
                    generated.skipped.push(SkippedFile {
                        repo_id,
                        path,
                        reason: "file deleted in the review (deletions are not suggested yet)"
                            .to_string(),
                    });
                    continue;
                }
                Err(error) => return Err(error),
                Ok(0o160000) => {
                    generated.skipped.push(SkippedFile {
                        repo_id,
                        path,
                        reason: "submodule (not supported)".to_string(),
                    });
                    continue;
                }
                Ok(_) => {}
            }
            match repo.file_mode(&review_head, &path)? {
                0o120000 => {
                    generated.skipped.push(SkippedFile {
                        repo_id,
                        path,
                        reason: "symbolic link (not supported)".to_string(),
                    });
                    continue;
                }
                0o100755 => {
                    generated.skipped.push(SkippedFile {
                        repo_id,
                        path,
                        reason: "executable file (mode not preserved; not suggested)".to_string(),
                    });
                    continue;
                }
                _ => {}
            }
            // Blob reads go through the API's one committed-snapshot reader
            // (authoritative_workspace_reader_guard), never the raw handle.
            // Whether the base has the file is answered from the object
            // header, so an unsupported or oversized existing file is skipped
            // below without its base blob ever being read.
            let base_present =
                match file_size_at_commit_sync(repo_storage, &key, &merge_base, &path) {
                    Ok(size) => Some(size),
                    Err(G5Error::NotFound { .. }) => None,
                    Err(error) => return Err(error),
                };
            let Some(_base_size) = base_present else {
                // New file. A text file the repository can take becomes a
                // `new_file` suggestion: it shows in Reviews and the owner
                // decides whether it enters the workspace (accept) or not.
                // What cannot be proposed as a file (binary, over 100 KiB)
                // still reaches the owner, as an artifact to open or
                // download; only an oversized artifact is skipped. The size
                // comes from the object header first, so an oversized file
                // is never materialized.
                let size = file_size_at_commit_sync(repo_storage, &key, &review_head, &path)?;
                if size > MAX_ARTIFACT_BYTES as u64 {
                    generated.skipped.push(SkippedFile {
                        repo_id,
                        path,
                        reason: "artifact too large".to_string(),
                    });
                    continue;
                }
                let bytes = read_file_at_commit_sync(repo_storage, &key, &review_head, &path)?;
                // git's binary verdict and a NUL byte (UTF-16 passes
                // `from_utf8` on ASCII and then fails the text column) both
                // keep a file out of the repository proposal.
                let proposable = !matches!(
                    diff.status,
                    CommitFileDiffStatus::Binary | CommitFileDiffStatus::TooLarge
                ) && !bytes.contains(&0)
                    && bytes.len() <= MAX_NEW_FILE_SUGGESTION_BYTES;
                if proposable {
                    if let Ok(text) = String::from_utf8(bytes.clone()) {
                        generated.suggestions.push(GeneratedSuggestion {
                            repo_id,
                            path,
                            kind: "new_file".to_string(),
                            before_lines: None,
                            after_lines: text,
                            anchor_start: None,
                            content_hash: None,
                            anchor_context: None,
                            blob_oid: None,
                            blob_sha256: None,
                            bytes: None,
                            content_type: None,
                        });
                        continue;
                    }
                }
                let (blob_oid, blob_sha256) = if options.store_artifacts {
                    store_review_artifact(repo_storage, &key, &bytes)?
                } else {
                    (
                        g5_scm::hash_blob(&bytes).map_err(|e| G5Error::Internal {
                            message: format!("hashing artifact: {e}"),
                        })?,
                        format!("{:x}", Sha256::digest(&bytes)),
                    )
                };
                let content_type = guess_content_type(&path).to_string();
                generated.suggestions.push(GeneratedSuggestion {
                    repo_id,
                    path,
                    kind: "artifact".to_string(),
                    before_lines: None,
                    after_lines: String::new(),
                    anchor_start: None,
                    content_hash: None,
                    anchor_context: None,
                    blob_oid: Some(blob_oid),
                    blob_sha256: Some(blob_sha256),
                    bytes: Some(bytes.len() as i64),
                    content_type: Some(content_type),
                });
                continue;
            };
            match diff.status {
                CommitFileDiffStatus::Binary => {
                    generated.skipped.push(SkippedFile {
                        repo_id,
                        path,
                        reason: "binary file".to_string(),
                    });
                    continue;
                }
                CommitFileDiffStatus::TooLarge => {
                    generated.skipped.push(SkippedFile {
                        repo_id,
                        path,
                        reason: "diff too large".to_string(),
                    });
                    continue;
                }
                CommitFileDiffStatus::Text => {}
            }
            let base_bytes = read_file_at_commit_sync(repo_storage, &key, &merge_base, &path)?;
            let proposed_bytes = read_file_at_commit_sync(repo_storage, &key, &review_head, &path)?;
            // Hunks carry text: both versions must be valid UTF-8, or the
            // lossy diff would propose replacement characters instead of the
            // reviewed bytes.
            if std::str::from_utf8(&base_bytes).is_err()
                || std::str::from_utf8(&proposed_bytes).is_err()
            {
                generated.skipped.push(SkippedFile {
                    repo_id,
                    path,
                    reason: "not UTF-8".to_string(),
                });
                continue;
            }
            let hunks = hunk_suggestions(repo_id, &path, &base_bytes, &proposed_bytes, &diff);
            if hunks.is_empty() {
                generated.skipped.push(SkippedFile {
                    repo_id,
                    path,
                    reason: "only the final newline changed".to_string(),
                });
            } else {
                generated.suggestions.extend(hunks);
            }
        }
    }
    Ok(generated)
}

/// The hunk suggestions of one changed text file, shared by review rounds
/// and FILES pushes: one per Git hunk of `diff` (cut from `base_bytes`, the
/// UTF-8 file it changes), except a hunk whose only change is the final
/// newline, which proposes nothing. Each is anchored on its first base line
/// and carries the base file's digest and the base lines around it.
fn hunk_suggestions(
    repo_id: Uuid,
    path: &str,
    base_bytes: &[u8],
    proposed_bytes: &[u8],
    diff: &CommitFileDiff,
) -> Vec<GeneratedSuggestion> {
    let content_hash = format!("{:x}", Sha256::digest(base_bytes));
    let base_text = std::str::from_utf8(base_bytes).unwrap_or("");
    let base_lines = split_lines(base_text);
    let proposed_text = std::str::from_utf8(proposed_bytes).unwrap_or("");
    let proposed_lines = split_lines(proposed_text);
    let mut suggestions = Vec::new();
    for hunk in &diff.hunks {
        let mut before = Vec::new();
        let mut after = Vec::new();
        let mut anchor_start: Option<i32> = None;
        for line in &hunk.lines {
            match line.line_type {
                CommitDiffLineType::Context => {
                    before.push(line.content.as_str());
                    after.push(line.content.as_str());
                }
                CommitDiffLineType::Deletion => before.push(line.content.as_str()),
                CommitDiffLineType::Addition => after.push(line.content.as_str()),
            }
            if anchor_start.is_none() {
                if let Some(old) = line.old_line {
                    anchor_start = Some(old as i32);
                }
            }
        }
        if before == after {
            // Only the final newline changed: nothing to propose.
            continue;
        }
        if anchor_start.is_none() {
            // A pure insertion (for example into an empty file) has no
            // old line to anchor on; git's `@@ -N,0` says it goes
            // after line N of the base.
            anchor_start = hunk_old_start(&hunk.header).map(|start| start + 1);
        }
        // The diff's own context rows are part of `before`, so they
        // could never tell two occurrences of the block apart; the
        // published context is the base file's lines around the block.
        let mut context = anchor_context_for(&base_lines, anchor_start, before.len()).to_json();
        if context.len() > MAX_ANCHOR_CONTEXT_BYTES {
            context = AnchorContext::default().to_json();
        }
        // Interior hunk rows end at a line boundary. An EOF row retains the
        // actual file terminator, rather than introducing a synthetic byte.
        let exact_rows = |rows: &[&str], source: &str, line_count: usize, old: bool| {
            let mut text = join_lines(rows);
            if !source.ends_with('\n')
                && hunk.lines.iter().any(|line| {
                    (if old { line.old_line } else { line.new_line }) == Some(line_count as u32)
                })
            {
                text.pop();
            }
            text
        };
        suggestions.push(GeneratedSuggestion {
            repo_id,
            path: path.to_string(),
            kind: "hunk".to_string(),
            before_lines: Some(exact_rows(&before, base_text, base_lines.len(), true)),
            after_lines: exact_rows(&after, proposed_text, proposed_lines.len(), false),
            anchor_start,
            content_hash: Some(content_hash.clone()),
            anchor_context: Some(context),
            blob_oid: None,
            blob_sha256: None,
            bytes: None,
            content_type: None,
        });
    }
    suggestions
}

/// Largest text a FILES push compares, decoded: the FILES editor's limit
/// (`g5.documents.files-view.text-only`).
pub const MAX_FILE_SUGGESTION_TEXT_BYTES: usize = 2 * 1024 * 1024;

/// Refuse `bytes` unless a file suggestion can carry them as text: valid
/// UTF-8 without a NUL byte, at most [`MAX_FILE_SUGGESTION_TEXT_BYTES`]
/// (`g5.documents.files-view.text-only`). `subject` names the text in the
/// message FILES shows (`details.message`); `field` names the request field.
pub fn ensure_suggestion_text(field: &str, subject: &str, bytes: &[u8]) -> Result<(), G5Error> {
    let problem = if bytes.len() > MAX_FILE_SUGGESTION_TEXT_BYTES {
        "is larger than 2 MiB"
    } else if std::str::from_utf8(bytes).is_err() {
        "is not valid UTF-8 text"
    } else if bytes.contains(&0) {
        "contains a NUL byte, so it is not text"
    } else {
        return Ok(());
    };
    Err(file_suggestion_refusal(
        field,
        format!("{subject} {problem}"),
    ))
}

/// A FILES push refusal: `details.message` says why for the field it names,
/// and the response's own message repeats it (`FILE_SUGGESTION_REFUSED`).
pub fn file_suggestion_refusal(field: &str, message: impl Into<String>) -> G5Error {
    G5Error::ValidationError {
        details: json!({
            "code": "FILE_SUGGESTION_REFUSED",
            "field": field,
            "message": message.into(),
        }),
    }
}

/// The hunk suggestions a FILES push proposes for one existing text file:
/// `proposed` against `base`, the text the reader started from, cut and
/// converted exactly as a review round cuts a changed file
/// (`g5.governance.file-suggestions.record`). Refused unless both are text,
/// when nothing but the final newline changed, and past the review diff
/// limits.
pub fn file_suggestion_changes(
    repo_id: Uuid,
    path: &str,
    base: &[u8],
    proposed: &[u8],
) -> Result<Vec<GeneratedSuggestion>, G5Error> {
    ensure_suggestion_text("baseContent", "The text you started from", base)?;
    ensure_suggestion_text("content", "Your edited text", proposed)?;
    let diff = g5_scm::text_file_diff(path, base, proposed)?;
    match diff.status {
        CommitFileDiffStatus::Text => {}
        CommitFileDiffStatus::TooLarge => {
            return Err(file_suggestion_refusal(
                "content",
                "This change is too large to suggest",
            ))
        }
        CommitFileDiffStatus::Binary => {
            return Err(G5Error::Internal {
                message: format!("the text diff of {path} came back binary"),
            })
        }
    }
    let changes = hunk_suggestions(repo_id, path, base, proposed, &diff);
    if changes.is_empty() {
        return Err(file_suggestion_refusal("content", "No changes to suggest"));
    }
    Ok(changes)
}

/// The base start line of a unified hunk header (`@@ -N[,M] +… @@`).
fn hunk_old_start(header: &str) -> Option<i32> {
    let rest = header.strip_prefix("@@ -")?;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

/// Inputs of a new publication.
#[derive(Debug, Clone)]
pub struct NewPublication {
    pub org_id: Uuid,
    pub source_workspace_id: Uuid,
    pub review_workspace_id: Option<Uuid>,
    pub reviewer_user_id: Uuid,
    pub author_kind: String,
    pub author_label: String,
    pub base_commit_vector: Value,
    pub summary: String,
    pub verdict: Option<String>,
    pub scores: Option<Value>,
    /// `round` (Push review: supersedes the
    /// author's previous round of the same shape) or `annotation` (the
    /// publication a stand-alone comment or upload belongs to).
    pub kind: String,
}

/// The round number the author's next publication would get (for the
/// commit that records it; the allocation itself happens under the
/// publication lock in `create_publication`).
pub async fn next_round_number(
    db: &mut ScopedDb,
    source_workspace_id: Uuid,
    reviewer_user_id: Uuid,
    author_kind: &str,
) -> Result<i32, G5Error> {
    Ok(sqlx::query_scalar(
        "SELECT COALESCE(MAX(round), 0) + 1 FROM review_publications
         WHERE source_workspace_id = $1 AND reviewer_user_id = $2 AND author_kind = $3",
    )
    .bind(source_workspace_id)
    .bind(reviewer_user_id)
    .bind(author_kind)
    .fetch_one(db.conn())
    .await?)
}

/// The transaction-scoped lock every publication of one author on one
/// source is created under (`create_publication` takes it too; advisory
/// transaction locks re-enter). A caller that decides something about the
/// round it is about to create from current state takes it first, so that
/// state cannot change between the decision and the write.
pub async fn lock_publications(
    db: &mut ScopedDb,
    source_workspace_id: Uuid,
    reviewer_user_id: Uuid,
    author_kind: &str,
) -> Result<(), G5Error> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
        .bind(format!(
            "review_publication:{source_workspace_id}:{reviewer_user_id}:{author_kind}"
        ))
        .execute(db.conn())
        .await?;
    Ok(())
}

pub const PUBLICATION_KIND_ROUND: &str = "round";
pub const PUBLICATION_KIND_ANNOTATION: &str = "annotation";

/// Record a publication and retire the reviewer's previous round of the same
/// kind: the previous publication is marked superseded and its open hunk and
/// new-file suggestions are superseded (a new round is a full
/// snapshot of the review branch against the source). Comments and artifacts
/// stay open: they are conversation, not a snapshot.
pub async fn create_publication(
    db: &mut ScopedDb,
    input: NewPublication,
) -> Result<ReviewPublicationRecord, G5Error> {
    // Every path that changes suggestion rows of this source runs under the
    // decisions lock (`lock_review_decisions`), taken before any row lock:
    // a round's bulk retirement of its predecessor's rows would otherwise
    // interleave with an accept that locks competing rows one by one, each
    // waiting for the other's row. The lock re-enters, so a caller that
    // already holds it is unaffected; the publication lock comes after it
    // everywhere (see the internal publish route).
    lock_review_decisions(db, input.source_workspace_id).await?;
    // Round numbers are allocated under a transaction-scoped advisory lock
    // on the (source, reviewer, kind) tuple: two concurrent pushes (or a
    // push racing a first comment) otherwise both read the same maximum and
    // one of them dies on the unique constraint.
    lock_publications(
        db,
        input.source_workspace_id,
        input.reviewer_user_id,
        &input.author_kind,
    )
    .await?;
    let round: i32 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(round), 0) + 1 FROM review_publications
         WHERE source_workspace_id = $1 AND reviewer_user_id = $2 AND author_kind = $3",
    )
    .bind(input.source_workspace_id)
    .bind(input.reviewer_user_id)
    .bind(&input.author_kind)
    .fetch_one(db.conn())
    .await?;
    let record = sqlx::query_as::<_, ReviewPublicationRecord>(
        "INSERT INTO review_publications(
             org_id, source_workspace_id, review_workspace_id, reviewer_user_id,
             author_kind, author_label, round, base_commit_vector, summary, verdict, scores,
             kind)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
         RETURNING *",
    )
    .bind(input.org_id)
    .bind(input.source_workspace_id)
    .bind(input.review_workspace_id)
    .bind(input.reviewer_user_id)
    .bind(&input.author_kind)
    .bind(&input.author_label)
    .bind(round)
    .bind(&input.base_commit_vector)
    .bind(&input.summary)
    .bind(&input.verdict)
    .bind(&input.scores)
    .bind(&input.kind)
    .fetch_one(db.conn())
    .await?;
    // A new round retires the same author's previous rounds of the same
    // shape: rounds pushed from a review workspace replace earlier ones from
    // a review workspace; an in-place round (reviewing the
    // workspace it runs in, no fork; findings alone count) replaces earlier
    // in-place rounds. Annotation publications (the home of stand-alone
    // comments and uploads) are never rounds and neither supersede nor get
    // superseded.
    if input.kind == PUBLICATION_KIND_ROUND {
        let previous: Vec<Uuid> = sqlx::query_scalar(
            "UPDATE review_publications SET superseded_by = $1
             WHERE source_workspace_id = $2 AND reviewer_user_id = $3 AND author_kind = $4
               AND superseded_by IS NULL AND id <> $1 AND kind = 'round'
               AND (review_workspace_id IS NOT NULL) = $5
             RETURNING id",
        )
        .bind(record.id)
        .bind(input.source_workspace_id)
        .bind(input.reviewer_user_id)
        .bind(&input.author_kind)
        .bind(input.review_workspace_id.is_some())
        .fetch_all(db.conn())
        .await?;
        if !previous.is_empty() {
            sqlx::query(
                "UPDATE review_suggestions SET status = 'superseded', status_at = now(),
                    edits = COALESCE((SELECT jsonb_agg(CASE WHEN e->>'status' = 'open' THEN
                        e || jsonb_build_object('status','superseded','statusAt',now()) ELSE e END)
                        FROM jsonb_array_elements(edits) e), '[]'::jsonb)
                 WHERE publication_id = ANY($1) AND status = 'open'
                   AND kind IN ('hunk', 'new_file')",
            )
            .bind(&previous)
            .execute(db.conn())
            .await?;
            sqlx::query(
                "UPDATE review_findings SET status = 'superseded'
                 WHERE publication_id = ANY($1) AND status = 'open'",
            )
            .bind(&previous)
            .execute(db.conn())
            .await?;
        }
    }
    Ok(record)
}

/// Record the findings of a publication in submission order.
pub async fn insert_findings(
    db: &mut ScopedDb,
    org_id: Uuid,
    publication_id: Uuid,
    findings: &[NewFinding],
) -> Result<Vec<ReviewFindingRecord>, G5Error> {
    let mut out = Vec::with_capacity(findings.len());
    for (position, finding) in findings.iter().enumerate() {
        let title = finding.title.trim();
        if title.is_empty() {
            return Err(G5Error::ValidationError {
                details: json!({"findings": "each finding needs a title"}),
            });
        }
        if !matches!(finding.severity.as_str(), "P0" | "P1" | "P2" | "P3") {
            return Err(G5Error::ValidationError {
                details: json!({"findings": "severity must be P0, P1, P2 or P3"}),
            });
        }
        let actions = if finding.actions.is_null() {
            json!([])
        } else {
            finding.actions.clone()
        };
        let paths: Vec<&str> = finding
            .paths
            .iter()
            .map(|path| path.trim())
            .filter(|path| !path.is_empty())
            .collect();
        out.push(
            sqlx::query_as::<_, ReviewFindingRecord>(
                "INSERT INTO review_findings(org_id, publication_id, title, severity, dimension, evidence, actions, position, repo_id, paths)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
                 RETURNING id, publication_id, title, severity, dimension, evidence, actions, status, position, created_at, repo_id, paths",
            )
            .bind(org_id)
            .bind(publication_id)
            .bind(title)
            .bind(&finding.severity)
            .bind(finding.dimension.trim())
            .bind(finding.evidence.trim())
            .bind(actions)
            .bind(position as i32)
            .bind(finding.repo_id)
            .bind(json!(paths))
            .fetch_one(db.conn())
            .await?,
        );
    }
    Ok(out)
}

/// Authorship carried by every suggestion of a publication.
#[derive(Debug, Clone)]
pub struct SuggestionAuthor {
    pub reviewer_user_id: Uuid,
    pub author_kind: String,
    pub author_label: String,
}

/// A matched predecessor of a generated change: its id, the thread it
/// carries, the decision on it (`status`, `status_by`, `status_at`), for an
/// artifact the bytes it stood for (`blob_oid`), for a hunk the place it
/// was anchored at (`anchor_start`), and its `note`.
type PredecessorRow = (
    Uuid,
    Option<Uuid>,
    String,
    Option<Uuid>,
    Option<DateTime<Utc>>,
    Option<String>,
    Option<i32>,
    String,
);

/// Git may align an accepted identical block with another occurrence in the
/// complete proposal. Only the original ordered group and exact source effects
/// can establish which pending identities continue across that boundary.
fn identical_insert_group_matches(
    previous: &ReviewSuggestionRecord,
    source: &str,
    old_base: usize,
    old_locations: &[(usize, usize, &ReviewSuggestionEdit)],
    new_base: usize,
    next_edits: &[ReviewSuggestionEdit],
    point: usize,
) -> Vec<(usize, ReviewSuggestionEdit)> {
    let next: Vec<_> = next_edits
        .iter()
        .enumerate()
        .filter(|(_, edit)| edit.start == edit.end && new_base + edit.start == point)
        .collect();
    let Some((_, first)) = next.first() else {
        return Vec::new();
    };
    if first.after_text.is_empty()
        || next
            .iter()
            .any(|(_, edit)| !edit.before_text.is_empty() || edit.after_text != first.after_text)
    {
        return Vec::new();
    }
    let effect = |placements: &[(usize, &ReviewSuggestionEdit)]| -> Option<String> {
        let mut output = String::new();
        let mut cursor = 0;
        for (offset, edit) in placements {
            output.push_str(source.get(cursor..*offset)?);
            output.push_str(&edit.after_text);
            cursor = *offset;
        }
        output.push_str(source.get(cursor..)?);
        Some(output)
    };
    let new_placements: Vec<_> = next.iter().map(|(_, edit)| (point, *edit)).collect();
    let Some(new_effect) = effect(&new_placements) else {
        return Vec::new();
    };
    let points: std::collections::HashSet<_> = old_locations
        .iter()
        .filter(|(_, _, edit)| edit.start == edit.end)
        .map(|(_, _, edit)| edit.start)
        .collect();
    let mut proven = Vec::new();
    for original in points {
        let old: Vec<_> = old_locations
            .iter()
            .filter(|(_, _, edit)| edit.start == original && edit.end == original)
            .collect();
        if old.len() != next.len()
            || old.iter().zip(&next).any(|(old, (_, next))| {
                old.2.before_text != next.before_text || old.2.after_text != next.after_text
            })
        {
            continue;
        }
        let siblings: Vec<_> = previous
            .edits
            .iter()
            .filter(|edit| edit.start == original && edit.end == original)
            .collect();
        if siblings
            .iter()
            .any(|edit| edit.source_conflict || edit.after_text != first.after_text)
        {
            continue;
        }
        let accepted: Vec<_> = siblings
            .iter()
            .filter(|edit| edit.status == "accepted")
            .collect();
        if accepted.is_empty()
            || accepted.iter().any(|edit| {
                edit.accepted_after_text
                    .as_ref()
                    .unwrap_or(&edit.after_text)
                    != &first.after_text
                    || source.get(old_base + edit.current_start..old_base + edit.current_end)
                        != Some(first.after_text.as_str())
            })
        {
            continue;
        }
        let start = accepted
            .iter()
            .map(|edit| old_base + edit.current_start)
            .min()
            .unwrap();
        let end = accepted
            .iter()
            .map(|edit| old_base + edit.current_end)
            .max()
            .unwrap();
        if point < start || point > end {
            continue;
        }
        let old_placements: Vec<_> = old.iter().map(|(start, _, edit)| (*start, *edit)).collect();
        if effect(&old_placements).as_ref() == Some(&new_effect) {
            proven.push(old);
        }
    }
    if proven.len() != 1 {
        return Vec::new();
    }
    next.iter()
        .zip(&proven[0])
        .map(|((index, _), old)| (*index, old.2.clone()))
        .collect()
}

/// Ephemeral correspondence proof. Published coordinates describe the new
/// capture while the prior record supplies durable identity and decisions.
struct MatchedReviewEdit {
    edit: ReviewSuggestionEdit,
    capture_start: usize,
    capture_end: usize,
}

fn child_source_matches(
    previous: &ReviewSuggestionRecord,
    item: &GeneratedSuggestion,
    source: &str,
    next_edits: &[ReviewSuggestionEdit],
) -> Vec<(usize, MatchedReviewEdit)> {
    if previous.kind != "hunk"
        || previous.repo_id != Some(item.repo_id)
        || previous.path.as_deref() != Some(item.path.as_str())
        || previous.edits.is_empty()
    {
        return Vec::new();
    }
    let digest = format!("{:x}", Sha256::digest(source.as_bytes()));
    if item.content_hash.as_deref() != Some(digest.as_str()) {
        return Vec::new();
    }
    let source_lines = split_lines(source);
    let Some(new_line) = item
        .anchor_start
        .and_then(|line| usize::try_from(line - 1).ok())
    else {
        return Vec::new();
    };
    let new_before = split_lines(item.before_lines.as_deref().unwrap_or_default());
    if new_line + new_before.len() > source_lines.len()
        || source_lines[new_line..new_line + new_before.len()] != new_before[..]
    {
        return Vec::new();
    }
    // Publication and preview have the same exact captured-source witness as
    // decisions. A deliberate revert cannot be located in historical accepted
    // text or in the offsets derived from that text.
    let trusted = previous.content_hash.as_deref() == Some(digest.as_str());
    let mut captured_previous;
    let previous = if trusted {
        captured_previous = previous.clone();
        use_captured_source_projection(&mut captured_previous);
        &captured_previous
    } else {
        previous
    };
    let old_before = split_lines(&previous.expected_before_lines);
    let preferred = previous
        .anchor_start
        .and_then(|line| usize::try_from(line - 1).ok());
    let context = AnchorContext::parse(previous.anchor_context.as_deref());
    let Ok(old_line) =
        find_block_with_context(&source_lines, &old_before, preferred, &context, trusted)
    else {
        return Vec::new();
    };
    let byte_at_line = |line: usize| {
        source
            .split_inclusive('\n')
            .take(line)
            .map(str::len)
            .sum::<usize>()
    };
    let old_base = byte_at_line(old_line);
    let new_base = byte_at_line(new_line);
    let mut old_locations = Vec::new();
    for edit in &previous.edits {
        if edit.source_conflict
            || !(matches!(edit.status.as_str(), "open" | "superseded" | "rejected")
                || (trusted && edit.status == "accepted"))
        {
            continue;
        }
        old_locations.push((
            old_base + edit.current_start,
            old_base + edit.current_end,
            edit,
        ));
    }
    let mut matches = Vec::new();
    let mut equivalent_groups = HashMap::new();
    for (index, next) in next_edits.iter().enumerate() {
        let point = new_base + next.start;
        let candidates: Vec<_> = old_locations
            .iter()
            .filter(|(start, end, edit)| {
                *start == point
                    && *end == new_base + next.end
                    && edit.before_text == next.before_text
                    && edit.after_text == next.after_text
            })
            .collect();
        let reverse_count = next_edits
            .iter()
            .filter(|other| {
                other.start == next.start
                    && other.end == next.end
                    && other.before_text == next.before_text
                    && other.after_text == next.after_text
            })
            .count();
        if candidates.len() == 1 && reverse_count == 1 {
            matches.push((
                index,
                MatchedReviewEdit {
                    edit: candidates[0].2.clone(),
                    capture_start: next.start,
                    capture_end: next.end,
                },
            ));
            continue;
        }
        if next.start == next.end {
            let old_group: Vec<_> = old_locations
                .iter()
                .filter(|(start, end, _)| *start == point && *end == point)
                .collect();
            let new_group: Vec<_> = next_edits
                .iter()
                .enumerate()
                .filter(|(_, edit)| new_base + edit.start == point && edit.start == edit.end)
                .collect();
            if old_group.len() > 1 || new_group.len() > 1 {
                // The owner already established the order of this complete
                // same-point group. Equal paragraphs are distinct units, but
                // a missing, added or reordered unit cannot prove that order.
                if old_group.len() == new_group.len()
                    && old_group.iter().zip(&new_group).all(|(old, (_, new))| {
                        old.2.before_text == new.before_text && old.2.after_text == new.after_text
                    })
                {
                    let order = new_group.iter().position(|(i, _)| *i == index).unwrap();
                    matches.push((
                        index,
                        MatchedReviewEdit {
                            edit: old_group[order].2.clone(),
                            capture_start: next.start,
                            capture_end: next.end,
                        },
                    ));
                }
                if matches.last().is_some_and(|(matched, _)| *matched == index) {
                    continue;
                }
            }
            if !trusted {
                let group = equivalent_groups.entry(point).or_insert_with(|| {
                    identical_insert_group_matches(
                        previous,
                        source,
                        old_base,
                        &old_locations,
                        new_base,
                        next_edits,
                        point,
                    )
                });
                if let Some((_, edit)) = group.iter().find(|(matched, _)| *matched == index) {
                    // Equal immutable insertions cannot exchange logical
                    // positions: a later Edit may give this unit new text.
                    if let (Some(start), Some(end)) = (
                        (old_base + edit.current_start).checked_sub(new_base),
                        (old_base + edit.current_end).checked_sub(new_base),
                    ) {
                        if item
                            .before_lines
                            .as_deref()
                            .unwrap_or_default()
                            .get(start..end)
                            == Some(edit.before_text.as_str())
                        {
                            matches.push((
                                index,
                                MatchedReviewEdit {
                                    edit: edit.clone(),
                                    capture_start: start,
                                    capture_end: end,
                                },
                            ));
                        }
                    }
                }
            }
        }
    }
    // Different generated coordinates must not reuse one prior identity even
    // if separate equivalent-boundary probes each appeared locally valid.
    let mut counts = HashMap::new();
    for (_, matched) in &matches {
        *counts.entry(matched.edit.id).or_insert(0usize) += 1;
    }
    matches.retain(|(_, matched)| counts[&matched.edit.id] == 1);
    let moved = |index: usize, matched: &MatchedReviewEdit| {
        matched.capture_start != next_edits[index].start
            || matched.capture_end != next_edits[index].end
    };
    if matches
        .iter()
        .any(|(index, matched)| moved(*index, matched))
    {
        let mut proposed = next_edits.to_vec();
        for edit in &mut proposed {
            edit.status = "open".into();
            edit.accepted_after_text = None;
        }
        for (index, matched) in &matches {
            proposed[*index].start = matched.capture_start;
            proposed[*index].end = matched.capture_end;
        }
        proposed.sort_by_key(|edit| (edit.start, edit.end));
        // A group alone may be equivalent while another newly proposed edit
        // makes the moved boundary alter the full published document.
        if review_edits::compose_proposed(
            item.before_lines.as_deref().unwrap_or_default(),
            &proposed,
        )
        .ok()
        .as_deref()
            != Some(item.after_lines.as_str())
        {
            matches.retain(|(index, matched)| !moved(*index, matched));
        }
    }
    matches
}

fn captured_base_source(
    repo_storage: &ScopedRepoStorage,
    org_id: Uuid,
    vector: &Value,
    repo_id: Uuid,
    path: &str,
) -> Result<Option<String>, G5Error> {
    let Some(head) = captured_base_head(vector, repo_id) else {
        return Ok(None);
    };
    let key = repo_storage.key(org_id, repo_id)?;
    let bytes = read_file_at_commit_sync(repo_storage, &key, head, path)?;
    Ok(String::from_utf8(bytes).ok())
}

fn captured_base_head(vector: &Value, repo_id: Uuid) -> Option<&str> {
    vector
        .as_array()
        .and_then(|entries| {
            entries.iter().find(|entry| {
                entry.get("repoId").and_then(Value::as_str) == Some(repo_id.to_string().as_str())
            })
        })
        .and_then(|entry| entry.get("mergeBase").and_then(Value::as_str))
}

struct CapturedReviewSource {
    content: String,
    /// Repository Git blob OID, the runtime file-version domain. This is not
    /// the raw SHA256 digest used by the captured proposal's content_hash.
    version: String,
}

type CapturedReviewSources = HashMap<(Uuid, String), CapturedReviewSource>;

fn captured_review_sources(
    repo_storage: &ScopedRepoStorage,
    org_id: Uuid,
    vector: &Value,
    generated: &[GeneratedSuggestion],
) -> Result<CapturedReviewSources, G5Error> {
    let mut sources = HashMap::new();
    for item in generated.iter().filter(|item| item.kind == "hunk") {
        if let std::collections::hash_map::Entry::Vacant(entry) =
            sources.entry((item.repo_id, item.path.clone()))
        {
            if let Some(content) =
                captured_base_source(repo_storage, org_id, vector, item.repo_id, &item.path)?
            {
                let head = captured_base_head(vector, item.repo_id)
                    .expect("captured source requires a base head");
                let key = repo_storage.key(org_id, item.repo_id)?;
                let version = blob_id_at_commit_sync(repo_storage, &key, head, &item.path)?;
                entry.insert(CapturedReviewSource { content, version });
            }
        }
    }
    Ok(sources)
}

async fn get_suggestion_for_capture(
    db: &mut ScopedDb,
    source_workspace_id: Uuid,
    suggestion_id: Uuid,
    sources: &CapturedReviewSources,
) -> Result<ReviewSuggestionRecord, G5Error> {
    let (mut suggestion, _) = get_suggestion(db, source_workspace_id, suggestion_id).await?;
    if let (Some(repo), Some(path)) = (suggestion.repo_id, suggestion.path.as_ref()) {
        if let Some(source) = sources.get(&(repo, path.clone())) {
            // Exact immutable Git capture is a publication source witness.
            // It makes no claim about current VM content and never persists
            // or relabels the decisions used to reconstruct this projection.
            refresh_suggestion_projection_with_version(
                db,
                source_workspace_id,
                &mut suggestion,
                Some(&source.version),
            )
            .await?;
        }
    }
    Ok(suggestion)
}

/// A changed Git context is the same complete proposal only when every
/// generated edit has a unique correspondence to the predecessor's source.
/// Partial carry must not label newly introduced edits as already published.
fn all_hunk_children_match(
    previous: &ReviewSuggestionRecord,
    item: &GeneratedSuggestion,
    source: &str,
) -> bool {
    let edits = review_edits::discrete_edits(
        Uuid::nil(),
        item.before_lines.as_deref().unwrap_or_default(),
        &item.after_lines,
    );
    if edits.is_empty() {
        return false;
    }
    let matched = child_source_matches(previous, item, source, &edits);
    matched.len() == edits.len()
        && matched
            .iter()
            .map(|(_, matched)| matched.edit.id)
            .collect::<std::collections::HashSet<_>>()
            .len()
            == edits.len()
}

fn projected_proposal(row: &ReviewSuggestionRecord) -> Option<String> {
    if row.edits.iter().any(|edit| edit.source_conflict) {
        return None;
    }
    let mut proposed = row.expected_before_lines.clone();
    for edit in row
        .edits
        .iter()
        .rev()
        .filter(|edit| edit.status != "accepted")
    {
        if proposed.get(edit.current_start..edit.current_end) != Some(edit.before_text.as_str()) {
            return None;
        }
        proposed.replace_range(edit.current_start..edit.current_end, &edit.after_text);
    }
    Some(proposed)
}

fn hunk_proposal_matches(row: &ReviewSuggestionRecord, item: &GeneratedSuggestion) -> bool {
    row.kind == "hunk"
        && item.kind == "hunk"
        && row.repo_id == Some(item.repo_id)
        && row.path.as_deref() == Some(item.path.as_str())
        && row.anchor_start == item.anchor_start
        && ((row.before_lines == item.before_lines
            && row.after_lines.as_deref() == Some(item.after_lines.as_str()))
            || (!row.edits.is_empty()
                && row.anchor_context == item.anchor_context
                && Some(row.expected_before_lines.as_str()) == item.before_lines.as_deref()
                && projected_proposal(row).as_deref() == Some(item.after_lines.as_str())))
}

/// Record the generated hunks and new files of a publication. A hunk that
/// equals one from the reviewer's previous round (same path and text) keeps
/// that thread: the old suggestion is linked through `supersedes` and its
/// `comment_id` is reused, so the conversation continues across rounds. A
/// predecessor the owner already accepted or rejected hands that decision
/// to the new row (`status`, `status_by`, `status_at`): a re-push of an
/// unchanged proposal never reopens it.
#[allow(clippy::too_many_arguments)]
pub async fn insert_generated_suggestions(
    db: &mut ScopedDb,
    repo_storage: &TenantRepoStorage,
    org_id: Uuid,
    source_workspace_id: Uuid,
    source_branch: &str,
    publication: &ReviewPublicationRecord,
    author: &SuggestionAuthor,
    generated: &[GeneratedSuggestion],
    notes: &HashMap<String, String>,
    change_keys: &[String],
    change_notes: &HashMap<String, String>,
    finding_by_path: &FindingPaths,
) -> Result<Vec<ReviewSuggestionRecord>, G5Error> {
    let mut out = Vec::with_capacity(generated.len());
    let base_sources = captured_review_sources(
        &repo_storage.scoped(&*db),
        org_id,
        &publication.base_commit_vector,
        generated,
    )?;
    // Each predecessor continues at most one thread: two identical hunks in
    // one file (same before/after text at different places) must not share
    // a discussion, so matched ids are excluded and the nearest anchor wins.
    let mut matched_previous: Vec<Uuid> = Vec::new();
    let mut settled_findings: std::collections::BTreeSet<Uuid> = std::collections::BTreeSet::new();
    // Pass one: every hunk and new file first reserves the predecessor at
    // its exact anchor. Matching in generation order by nearest anchor
    // alone would let an identical edit added earlier in the file take the
    // row that belongs to the unchanged one further down, which would then
    // arrive open, its decision and note lost.
    let mut reserved: HashMap<usize, PredecessorRow> = HashMap::new();
    if publication.review_workspace_id.is_some() {
        for (index, item) in generated.iter().enumerate() {
            if item.kind == "artifact" {
                continue;
            }
            let mut exact: Option<PredecessorRow> = sqlx::query_as(
                "SELECT s.id, s.comment_id, s.status, s.status_by, s.status_at, s.blob_oid,
                        s.anchor_start, s.note
                 FROM review_suggestions s
                 JOIN review_publications p ON p.id = s.publication_id
                 WHERE p.source_workspace_id = $1 AND p.reviewer_user_id = $2
                   AND p.author_kind = $3 AND p.superseded_by = $4
                   AND s.kind = $5 AND s.repo_id = $6 AND s.path = $7
                   AND s.before_lines IS NOT DISTINCT FROM $8 AND s.after_lines = $9
                   AND s.anchor_start IS NOT DISTINCT FROM $10
                   AND s.status NOT IN ('withdrawn', 'obsolete')
                   AND NOT (s.id = ANY($11::uuid[]))
                 ORDER BY s.created_at DESC
                 LIMIT 1",
            )
            .bind(source_workspace_id)
            .bind(author.reviewer_user_id)
            .bind(&author.author_kind)
            .bind(publication.id)
            .bind(&item.kind)
            .bind(item.repo_id)
            .bind(&item.path)
            .bind(&item.before_lines)
            .bind(&item.after_lines)
            .bind(item.anchor_start)
            .bind(&matched_previous)
            .fetch_optional(db.conn())
            .await?;
            if exact.is_none() && item.kind == "hunk" {
                let candidates: Vec<Uuid> = sqlx::query_scalar(
                    "SELECT s.id FROM review_suggestions s JOIN review_publications p ON p.id = s.publication_id
                     WHERE p.source_workspace_id = $1 AND p.reviewer_user_id = $2 AND p.author_kind = $3
                       AND p.superseded_by = $4 AND s.kind = 'hunk' AND s.repo_id = $5 AND s.path = $6
                       AND s.anchor_start IS NOT DISTINCT FROM $7 AND s.anchor_context IS NOT DISTINCT FROM $8
                       AND s.status NOT IN ('withdrawn','obsolete') AND NOT (s.id = ANY($9::uuid[]))")
                    .bind(source_workspace_id).bind(author.reviewer_user_id).bind(&author.author_kind).bind(publication.id)
                    .bind(item.repo_id).bind(&item.path).bind(item.anchor_start).bind(&item.anchor_context).bind(&matched_previous)
                    .fetch_all(db.conn()).await?;
                let mut equivalent = Vec::new();
                for id in candidates {
                    let row =
                        get_suggestion_for_capture(db, source_workspace_id, id, &base_sources)
                            .await?;
                    if !row.edits.is_empty() && hunk_proposal_matches(&row, item) {
                        equivalent.push(row);
                    }
                }
                if equivalent.len() == 1 {
                    let row = equivalent.pop().unwrap();
                    exact = Some((
                        row.id,
                        row.comment_id,
                        row.status,
                        row.status_by,
                        row.status_at,
                        row.blob_oid,
                        row.anchor_start,
                        row.note,
                    ));
                }
            }
            if let Some(row) = exact {
                matched_previous.push(row.0);
                reserved.insert(index, row);
            }
        }
    }
    for (index, item) in generated.iter().enumerate() {
        // The same artifact pushed again (same repository, path and blob)
        // stays the one open row the earlier round created; the flat
        // Artifacts list is the review's deliverables, not its history.
        // Decided before any thread lookup or creation, so a repeated push
        // never leaves a discussion row that no suggestion references.
        if item.kind == "artifact"
            && find_open_artifact(
                db,
                source_workspace_id,
                author.reviewer_user_id,
                item.repo_id,
                &item.path,
                item.blob_oid.as_deref(),
                &vector_heads(&publication.base_commit_vector),
            )
            .await?
            .is_some()
        {
            continue;
        }
        let previous: Option<PredecessorRow> = match publication.review_workspace_id {
            // An artifact is identified by its path alone (its text columns
            // are empty), and its row stays in the round that first pushed
            // it: the predecessor is the reviewer's latest row at that path
            // in any of their rounds, so a changed file continues its thread
            // and the decision on it is compared by bytes below.
            Some(_) if item.kind == "artifact" => {
                sqlx::query_as(
                    "SELECT s.id, s.comment_id, s.status, s.status_by, s.status_at, s.blob_oid,
                            s.anchor_start, s.note
                     FROM review_suggestions s
                     JOIN review_publications p ON p.id = s.publication_id
                     WHERE p.source_workspace_id = $1 AND p.reviewer_user_id = $2
                       AND p.author_kind = $3 AND p.review_workspace_id IS NOT NULL
                       AND s.kind = 'artifact' AND s.repo_id = $4 AND s.path = $5
                       AND s.status NOT IN ('withdrawn', 'obsolete')
                       AND NOT (s.id = ANY($6::uuid[]))
                     ORDER BY s.created_at DESC
                     LIMIT 1",
                )
                .bind(source_workspace_id)
                .bind(author.reviewer_user_id)
                .bind(&author.author_kind)
                .bind(item.repo_id)
                .bind(&item.path)
                .bind(&matched_previous)
                .fetch_optional(db.conn())
                .await?
            }
            // Pass one reserved the row at the exact anchor; what is left
            // continues the nearest remaining thread, as before.
            Some(_) if reserved.contains_key(&index) => reserved.remove(&index),
            Some(_) => {
                sqlx::query_as(
                    "SELECT s.id, s.comment_id, s.status, s.status_by, s.status_at, s.blob_oid,
                            s.anchor_start, s.note
                     FROM review_suggestions s
                     JOIN review_publications p ON p.id = s.publication_id
                     WHERE p.source_workspace_id = $1 AND p.reviewer_user_id = $2
                       AND p.author_kind = $3 AND p.superseded_by = $4
                       AND s.kind = $5 AND s.repo_id = $6 AND s.path = $7
                       AND s.before_lines IS NOT DISTINCT FROM $8 AND s.after_lines = $9
                       AND s.status NOT IN ('withdrawn', 'obsolete')
                       AND NOT (s.id = ANY($11::uuid[]))
                     ORDER BY (s.anchor_start IS NOT DISTINCT FROM $10) DESC,
                              abs(COALESCE(s.anchor_start, 0) - COALESCE($10, 0)) ASC,
                              s.created_at DESC
                     LIMIT 1",
                )
                .bind(source_workspace_id)
                .bind(author.reviewer_user_id)
                .bind(&author.author_kind)
                .bind(publication.id)
                .bind(&item.kind)
                .bind(item.repo_id)
                .bind(&item.path)
                .bind(&item.before_lines)
                .bind(&item.after_lines)
                .bind(item.anchor_start)
                .bind(&matched_previous)
                .fetch_optional(db.conn())
                .await?
            }
            None => None,
        };
        // An unchanged proposal the owner already accepted or rejected keeps
        // that decision: the new row is born decided, so the owner is not
        // asked twice and the round card shows it settled. An open one
        // starts open as before; a withdrawn or obsolete one says nothing.
        // A hunk is the same proposal only at the same place: identical text
        // at another anchor is the same edit made to another occurrence of a
        // repeated block, which the owner never decided on (the thread may
        // still continue). An artifact only when its bytes are the ones
        // decided on.
        let same_proposal =
            previous
                .as_ref()
                .is_some_and(|(_, _, _, _, _, blob_oid, anchor, _)| {
                    if item.kind == "artifact" {
                        item.blob_oid.is_some() && *blob_oid == item.blob_oid
                    } else {
                        *anchor == item.anchor_start
                    }
                });
        let carried_decision = previous.as_ref().filter(|_| same_proposal).and_then(
            |(_, _, status, by, at, _, _, _)| {
                (status == "accepted" || status == "rejected").then(|| (status.clone(), *by, *at))
            },
        );
        // The note travels with the proposal too: after a push the panel's
        // drafts are cleared, so a re-push names no note, and the row would
        // otherwise lose the one it had. An explicit note in the payload,
        // an empty one included, is the reviewer's word and replaces it.
        let carried_note = previous
            .as_ref()
            .filter(|_| same_proposal)
            .map(|(_, _, _, _, _, _, _, note)| note.clone());
        let previous: Option<(Uuid, Option<Uuid>)> =
            previous.map(|(id, comment_id, _, _, _, _, _, _)| (id, comment_id));
        if let Some((previous_id, _)) = previous {
            matched_previous.push(previous_id);
        }
        // A note on the change itself (the reviewer's panel, by the key the
        // preview listed) outranks a note on its path (the dialog).
        let note = change_keys
            .get(index)
            .and_then(|key| change_notes.get(key))
            .or_else(|| notes.get(&item.path))
            .cloned()
            .or(carried_note)
            .unwrap_or_default();
        let (supersedes, comment_id) = match previous {
            Some((id, Some(comment_id))) => (Some(id), comment_id),
            other => {
                let default_text = if item.kind == "new_file" {
                    format!("Proposes a new file {}", item.path)
                } else {
                    format!("Suggested change to {}", item.path)
                };
                let comment_id = create_suggestion_thread(
                    db,
                    org_id,
                    source_workspace_id,
                    source_branch,
                    author.reviewer_user_id,
                    Some(item.repo_id),
                    Some(&item.path),
                    item.anchor_start,
                    if note.is_empty() {
                        &default_text
                    } else {
                        &note
                    },
                )
                .await?;
                (other.map(|(id, _)| id), comment_id)
            }
        };
        let finding_id = finding_for(finding_by_path, item.repo_id, &item.path);
        let (status, status_by, status_at) = carried_decision
            .clone()
            .unwrap_or_else(|| ("open".to_string(), None, None));
        if carried_decision.is_some() {
            if let Some(finding_id) = finding_id {
                settled_findings.insert(finding_id);
            }
        }
        out.push(
            sqlx::query_as::<_, ReviewSuggestionRecord>(&format!(
                "INSERT INTO review_suggestions(
                     org_id, publication_id, finding_id, repo_id, path, kind,
                     before_lines, after_lines, anchor_start, content_hash, anchor_context,
                     author_kind, author_label, note, comment_id, supersedes,
                     blob_oid, blob_sha256, bytes, content_type, status, status_by, status_at)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,
                         $18,$19,$20,$21,$22,$23)
                 RETURNING {REVIEW_SUGGESTION_COLUMNS}"
            ))
            .bind(org_id)
            .bind(publication.id)
            .bind(finding_id)
            .bind(item.repo_id)
            .bind(&item.path)
            .bind(&item.kind)
            .bind(&item.before_lines)
            .bind(&item.after_lines)
            .bind(item.anchor_start)
            .bind(&item.content_hash)
            .bind(&item.anchor_context)
            .bind(&author.author_kind)
            .bind(&author.author_label)
            .bind(&note)
            .bind(comment_id)
            .bind(supersedes)
            .bind(&item.blob_oid)
            .bind(&item.blob_sha256)
            .bind(item.bytes)
            .bind(&item.content_type)
            .bind(&status)
            .bind(status_by)
            .bind(status_at)
            .fetch_one(db.conn())
            .await?,
        );
    }
    let predecessor_ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT s.id FROM review_suggestions s JOIN review_publications p ON p.id = s.publication_id
         WHERE p.source_workspace_id = $1 AND p.reviewer_user_id = $2 AND p.author_kind = $3
           AND p.superseded_by = $4 AND s.kind = 'hunk' AND s.status NOT IN ('withdrawn','obsolete')")
        .bind(source_workspace_id).bind(author.reviewer_user_id).bind(&author.author_kind).bind(publication.id)
        .fetch_all(db.conn()).await?;
    let mut predecessor_records = Vec::new();
    for id in predecessor_ids {
        predecessor_records
            .push(get_suggestion_for_capture(db, source_workspace_id, id, &base_sources).await?);
    }
    let mut taken_children = std::collections::HashSet::new();
    // Keep child identities, decisions and threads when the exact source proposal
    // continues. The immutable parent text remains the publication identity.
    for row in &mut out {
        if row.kind != "hunk" {
            continue;
        }
        if let Some(previous_id) = row.supersedes {
            let previous =
                get_suggestion_for_capture(db, source_workspace_id, previous_id, &base_sources)
                    .await?;
            let exact = previous.before_lines == row.before_lines
                && previous.after_lines == row.after_lines;
            let composed = Some(previous.expected_before_lines.as_str())
                == row.before_lines.as_deref()
                && projected_proposal(&previous).as_deref() == row.after_lines.as_deref()
                && previous.anchor_context == row.anchor_context;
            if (exact || composed)
                && previous.anchor_start == row.anchor_start
                && !previous.edits.is_empty()
            {
                row.edits = previous.edits;
                for edit in &mut row.edits {
                    if edit.status == "superseded" {
                        edit.status = "open".into();
                        edit.status_at = None;
                        edit.status_by = None;
                    }
                }
                row.before_lines = previous.before_lines;
                row.after_lines = previous.after_lines;
                row.content_hash = previous.content_hash;
                row.status = edit_summary_status(&row.edits).to_string();
                sqlx::query("UPDATE review_suggestions SET edits = $2, status = $3, before_lines = $4, after_lines = $5, content_hash = $6 WHERE id = $1")
                    .bind(row.id).bind(sqlx::types::Json(durable_edit_ledger(&row.edits))).bind(&row.status).bind(&row.before_lines).bind(&row.after_lines).bind(&row.content_hash).execute(db.conn()).await?;
            }
        }
        ensure_suggestion_edits(db, source_workspace_id, row).await?;
        let generated_item = generated.iter().enumerate().find(|(_, item)| {
            item.repo_id == row.repo_id.unwrap_or_default()
                && row.path.as_deref() == Some(item.path.as_str())
                && item.anchor_start == row.anchor_start
                && (item.before_lines == row.before_lines || hunk_proposal_matches(row, item))
        });
        if let Some((generated_index, item)) = generated_item {
            if let Some(source) = base_sources.get(&(item.repo_id, item.path.clone())) {
                let mut matches: HashMap<usize, Vec<(Uuid, MatchedReviewEdit)>> = HashMap::new();
                for previous in &predecessor_records {
                    for (index, matched) in
                        child_source_matches(previous, item, &source.content, &row.edits)
                    {
                        if !taken_children.contains(&matched.edit.id) {
                            matches
                                .entry(index)
                                .or_default()
                                .push((previous.id, matched));
                        }
                    }
                }
                let mut parents = std::collections::HashSet::new();
                for (index, candidates) in matches {
                    if candidates.len() != 1 {
                        continue;
                    }
                    let (parent, matched) = &candidates[0];
                    let old = &matched.edit;
                    let next = &mut row.edits[index];
                    // New bounds describe this publication's capture; durable edit
                    // identity/disposition/thread describe the proven same source unit.
                    next.id = old.id;
                    next.start = matched.capture_start;
                    next.end = matched.capture_end;
                    next.status = if old.status == "superseded" {
                        "open".into()
                    } else {
                        old.status.clone()
                    };
                    next.status_by = if old.status == "superseded" {
                        None
                    } else {
                        old.status_by
                    };
                    next.status_at = if old.status == "superseded" {
                        None
                    } else {
                        old.status_at
                    };
                    next.comment_id = old.comment_id;
                    next.accepted_after_text = old.accepted_after_text.clone();
                    next.checkpoint_pending = old.checkpoint_pending;
                    next.written_hash = old.written_hash.clone();
                    taken_children.insert(old.id);
                    parents.insert(*parent);
                }
                if !parents.is_empty() {
                    row.edits.sort_by_key(|edit| (edit.start, edit.end));
                    row.status = edit_summary_status(&row.edits).into();
                    if row.supersedes.is_none() && parents.len() == 1 {
                        row.supersedes = parents.iter().next().copied();
                    }
                    let explicit_note = change_keys
                        .get(generated_index)
                        .is_some_and(|key| change_notes.contains_key(key))
                        || notes.contains_key(&item.path);
                    if !explicit_note && parents.len() == 1 {
                        if let Some(previous) = predecessor_records.iter().find(|previous| {
                            parents.contains(&previous.id)
                                && all_hunk_children_match(previous, item, &source.content)
                        }) {
                            row.note = previous.note.clone();
                        }
                    }
                    sqlx::query("UPDATE review_suggestions SET edits = $2, status = $3, supersedes = $4, note = $5 WHERE id = $1")
                        .bind(row.id).bind(sqlx::types::Json(durable_edit_ledger(&row.edits))).bind(&row.status).bind(row.supersedes).bind(&row.note).execute(db.conn()).await?;
                    ensure_suggestion_edits(db, source_workspace_id, row).await?;
                }
            }
        }
        taken_children.extend(row.edits.iter().map(|e| e.id));
        let ids: Vec<_> = row.edits.iter().map(|e| e.id).collect();
        for id in ids {
            ensure_edit_thread(db, source_workspace_id, row, publication, id).await?;
        }
        if let Some(id) = row.finding_id {
            settled_findings.insert(id);
        }
    }
    // A finding whose fixes arrived already decided settles right away, the
    // way it would after the owner's decision.
    for finding_id in settled_findings {
        settle_finding_status(db, finding_id).await?;
    }
    Ok(out)
}

/// The reviewer's open artifact on the source with these bytes at this
/// path, if any: a re-push or a re-upload of an unchanged file reuses it.
pub async fn find_open_artifact(
    db: &mut ScopedDb,
    source_workspace_id: Uuid,
    reviewer_user_id: Uuid,
    repo_id: Uuid,
    path: &str,
    blob_oid: Option<&str>,
    source_heads: &HashMap<Uuid, String>,
) -> Result<Option<ReviewSuggestionRecord>, G5Error> {
    let Some(blob_oid) = blob_oid else {
        return Ok(None);
    };
    // Only an artifact of a publication about the source's current commit
    // is reused (every repository of its vector, the way the Reviews page
    // decides): one left on an outdated round would keep the reattached
    // file out of the current deliverables.
    let candidates: Vec<(Uuid, Value)> = sqlx::query_as(
        "SELECT s.id, p.base_commit_vector
         FROM review_suggestions s
         JOIN review_publications p ON p.id = s.publication_id
         WHERE p.source_workspace_id = $1 AND p.reviewer_user_id = $2
           AND s.kind = 'artifact' AND s.status = 'open'
           AND s.repo_id = $3 AND s.path = $4 AND s.blob_oid = $5
         ORDER BY s.created_at",
    )
    .bind(source_workspace_id)
    .bind(reviewer_user_id)
    .bind(repo_id)
    .bind(path)
    .bind(blob_oid)
    .fetch_all(db.conn())
    .await?;
    let Some((id, _)) = candidates
        .into_iter()
        .find(|(_, vector)| publication_is_current(vector, source_heads))
    else {
        return Ok(None);
    };
    let (record, _) = get_suggestion(db, source_workspace_id, id).await?;
    Ok(Some(record))
}

/// Where a previewed change already is: the reviewer's current round on
/// the source and the row of it that carries the change.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishedIn {
    pub publication_id: Uuid,
    pub round: i32,
    pub suggestion_id: Uuid,
    /// The row's status (`open`, `accepted`, `rejected`, ...).
    pub status: String,
}

#[derive(Debug, sqlx::FromRow)]
struct PublishedArtifactRow {
    id: Uuid,
    repo_id: Option<Uuid>,
    path: Option<String>,
    blob_oid: Option<String>,
    status: String,
    #[allow(dead_code)]
    created_at: DateTime<Utc>,
    publication_id: Uuid,
    round: i32,
    base_commit_vector: Value,
}

#[derive(Debug, sqlx::FromRow)]
struct PublishedRow {
    id: Uuid,
    kind: String,
    repo_id: Option<Uuid>,
    path: Option<String>,
    before_lines: Option<String>,
    after_lines: Option<String>,
    anchor_start: Option<i32>,
    #[allow(dead_code)]
    blob_oid: Option<String>,
    status: String,
    created_at: DateTime<Utc>,
}

/// For each generated change of a preview, the row of the reviewer's
/// current round that already carries it, or `None` for a change the round
/// does not have. The current round is the reviewer's latest round pushed
/// from a review workspace, and only while it is about the source's
/// current commit: after the owner commits again nothing is "in round N"
/// any more, the way the Reviews page retires that round. A change is
/// matched the way a push carries a decision: exact proposal identity or
/// complete source-proven child correspondence; each parent row taken once.
///
/// An artifact is matched the way a push treats it: an open row with the
/// same blob at the same path in a publication about the source's current
/// commit is reused (`find_open_artifact`), else the reviewer's latest row
/// at that path is the predecessor and counts only with the same bytes.
/// A round that retains an unchanged artifact leaves its row in the round
/// that first pushed it, so that earlier round is what the preview names.
pub async fn published_in_current_round(
    db: &mut ScopedDb,
    repo_storage: &TenantRepoStorage,
    generated_vector: &Value,
    source_workspace_id: Uuid,
    reviewer_user_id: Uuid,
    generated: &[GeneratedSuggestion],
    source_heads: &HashMap<Uuid, String>,
) -> Result<Vec<Option<PublishedIn>>, G5Error> {
    let latest: Option<(Uuid, i32, Value)> = sqlx::query_as(
        "SELECT id, round, base_commit_vector FROM review_publications
         WHERE source_workspace_id = $1 AND reviewer_user_id = $2 AND author_kind = 'human'
           AND kind = 'round' AND review_workspace_id IS NOT NULL AND superseded_by IS NULL
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(source_workspace_id)
    .bind(reviewer_user_id)
    .fetch_optional(db.conn())
    .await?;
    let Some((publication_id, round, vector)) = latest else {
        return Ok(vec![None; generated.len()]);
    };
    if !publication_is_current(&vector, source_heads) {
        return Ok(vec![None; generated.len()]);
    }
    // A withdrawn row is no longer the reviewer's proposal, and an obsolete
    // one was retired with a request to push again: neither marks a change
    // as already in the round, so the reviewer can propose it once more.
    let rows: Vec<PublishedRow> = sqlx::query_as(
        "SELECT id, kind, repo_id, path, before_lines, after_lines, anchor_start, blob_oid,
                status, created_at
         FROM review_suggestions
         WHERE publication_id = $1 AND status NOT IN ('withdrawn', 'obsolete')",
    )
    .bind(publication_id)
    .fetch_all(db.conn())
    .await?;
    // Every artifact row of the reviewer on the source, newest first: an
    // artifact's row lives in the round that first pushed it, and a push
    // looks at the latest row at its path, not at the current round. The
    // same rows a push considers (`insert_generated_suggestions`,
    // `carried_by_push`): a withdrawn or obsolete row is skipped before the
    // latest one is picked, so a withdrawn replacement does not hide the
    // decided row behind it.
    let artifact_rows: Vec<PublishedArtifactRow> = sqlx::query_as(
        "SELECT s.id, s.repo_id, s.path, s.blob_oid, s.status, s.created_at,
                p.id AS publication_id, p.round, p.base_commit_vector
         FROM review_suggestions s
         JOIN review_publications p ON p.id = s.publication_id
         WHERE p.source_workspace_id = $1 AND p.reviewer_user_id = $2
           AND s.kind = 'artifact' AND s.status NOT IN ('withdrawn', 'obsolete')
         ORDER BY s.created_at DESC",
    )
    .bind(source_workspace_id)
    .bind(reviewer_user_id)
    .fetch_all(db.conn())
    .await?;
    let org_id =
        crate::services::workspace_reader::workspace_org(db.conn(), source_workspace_id).await?;
    let base_sources = captured_review_sources(
        &repo_storage.scoped(&*db),
        org_id,
        generated_vector,
        generated,
    )?;
    let mut full_hunks = HashMap::new();
    for row in &rows {
        if row.kind == "hunk" {
            full_hunks.insert(
                row.id,
                get_suggestion_for_capture(db, source_workspace_id, row.id, &base_sources).await?,
            );
        }
    }
    let mut taken: std::collections::HashSet<Uuid> = std::collections::HashSet::new();
    let published = generated
        .iter()
        .map(|item| {
            if item.kind == "artifact" {
                // The way a push treats it: an open row with these bytes in
                // a publication about the current commit is reused as it is;
                // otherwise the latest row at the path is the predecessor,
                // and only the same bytes make it the same proposal (a kept
                // or rejected one carries its decision). Anything else, a
                // historical row with these bytes included, is a new push.
                let blob_oid = item.blob_oid.as_deref()?;
                let at_path = |row: &&PublishedArtifactRow| {
                    !taken.contains(&row.id)
                        && row.repo_id == Some(item.repo_id)
                        && row.path.as_deref() == Some(item.path.as_str())
                };
                let reused = artifact_rows.iter().filter(at_path).find(|row| {
                    row.status == "open"
                        && row.blob_oid.as_deref() == Some(blob_oid)
                        && publication_is_current(&row.base_commit_vector, source_heads)
                });
                let row = match reused {
                    Some(row) => row,
                    None => {
                        // An open row is reused only through the rule above
                        // (a push refuses an outdated one and creates a new
                        // open row); a decided row carries its decision when
                        // the bytes are the ones decided on.
                        let latest = artifact_rows.iter().find(at_path)?;
                        let decided = matches!(latest.status.as_str(), "accepted" | "rejected");
                        (decided && latest.blob_oid.as_deref() == Some(blob_oid))
                            .then_some(latest)?
                    }
                };
                taken.insert(row.id);
                return Some(PublishedIn {
                    publication_id: row.publication_id,
                    round: row.round,
                    suggestion_id: row.id,
                    status: row.status.clone(),
                });
            }
            // The same text at another anchor is the same edit made to
            // another occurrence of a repeated block: a push would create a
            // different proposal, so it is not "in the round".
            let best = rows
                .iter()
                .filter(|row| {
                    !taken.contains(&row.id)
                        && row.kind == item.kind
                        && row.repo_id == Some(item.repo_id)
                        && row.path.as_deref() == Some(item.path.as_str())
                        && (if item.kind == "hunk" {
                            full_hunks.get(&row.id).is_some_and(|row| {
                                hunk_proposal_matches(row, item)
                                    || base_sources
                                        .get(&(item.repo_id, item.path.clone()))
                                        .is_some_and(|source| {
                                            all_hunk_children_match(row, item, &source.content)
                                        })
                            })
                        } else {
                            row.anchor_start == item.anchor_start
                                && row.before_lines == item.before_lines
                                && row.after_lines.as_deref() == Some(item.after_lines.as_str())
                        })
                })
                .max_by_key(|row| row.created_at)?;
            taken.insert(best.id);
            Some(PublishedIn {
                publication_id,
                round,
                suggestion_id: best.id,
                status: best.status.clone(),
            })
        })
        .collect();
    Ok(published)
}

/// What a push would carry for a previewed change: the row of the
/// reviewer's rounds it would continue and the note the new row inherits
/// when the push names none (see `insert_generated_suggestions`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Carried {
    pub suggestion_id: Uuid,
    pub round: i32,
    pub note: String,
}

#[derive(Debug, sqlx::FromRow)]
struct CarriedRow {
    id: Uuid,
    kind: String,
    repo_id: Option<Uuid>,
    path: Option<String>,
    before_lines: Option<String>,
    after_lines: Option<String>,
    anchor_start: Option<i32>,
    blob_oid: Option<String>,
    note: String,
    created_at: DateTime<Utc>,
    round: i32,
}

/// For each generated change, what a push would carry over, regardless of
/// the source's current commit: the predecessor `insert_generated_suggestions`
/// carries a decision and note from the reviewer's latest round: a complete
/// proposal at its original source location, including uniquely proven
/// child correspondence after Git context changes, each row once. An
/// artifact uses the latest row at its path with the same bytes; withdrawn
/// and obsolete rows never carry. A nearest-anchor match
/// only continues a thread and carries nothing, so it is not reported. `published_in_current_round` says whether the change is
/// already in the review; this says what a push does with it either way,
/// so the panel shows the note a push would republish and can clear it.
pub async fn carried_by_push(
    db: &mut ScopedDb,
    repo_storage: &TenantRepoStorage,
    generated_vector: &Value,
    source_workspace_id: Uuid,
    reviewer_user_id: Uuid,
    generated: &[GeneratedSuggestion],
) -> Result<(Vec<Option<Carried>>, Vec<usize>, Vec<usize>), G5Error> {
    let latest: Option<(Uuid, i32)> = sqlx::query_as(
        "SELECT id, round FROM review_publications
         WHERE source_workspace_id = $1 AND reviewer_user_id = $2 AND author_kind = 'human'
           AND kind = 'round' AND review_workspace_id IS NOT NULL AND superseded_by IS NULL
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(source_workspace_id)
    .bind(reviewer_user_id)
    .fetch_optional(db.conn())
    .await?;
    let mut carried: Vec<Option<Carried>> = vec![None; generated.len()];
    let mut carried_edit_counts = vec![0; generated.len()];
    let mut carried_open_edit_counts = vec![0; generated.len()];
    let Some((publication_id, round)) = latest else {
        return Ok((carried, carried_edit_counts, carried_open_edit_counts));
    };
    let rows: Vec<CarriedRow> = sqlx::query_as(
        "SELECT s.id, s.kind, s.repo_id, s.path, s.before_lines, s.after_lines, s.anchor_start,
                s.blob_oid, s.note, s.created_at, $2::int AS round
         FROM review_suggestions s
         WHERE s.publication_id = $1 AND s.kind IN ('hunk', 'new_file')
           AND s.status NOT IN ('withdrawn', 'obsolete')",
    )
    .bind(publication_id)
    .bind(round)
    .fetch_all(db.conn())
    .await?;
    let artifact_rows: Vec<CarriedRow> = sqlx::query_as(
        "SELECT s.id, s.kind, s.repo_id, s.path, s.before_lines, s.after_lines, s.anchor_start,
                s.blob_oid, s.note, s.created_at, p.round
         FROM review_suggestions s
         JOIN review_publications p ON p.id = s.publication_id
         WHERE p.source_workspace_id = $1 AND p.reviewer_user_id = $2
           AND p.author_kind = 'human' AND p.review_workspace_id IS NOT NULL
           AND s.kind = 'artifact' AND s.status NOT IN ('withdrawn', 'obsolete')
         ORDER BY s.created_at DESC",
    )
    .bind(source_workspace_id)
    .bind(reviewer_user_id)
    .fetch_all(db.conn())
    .await?;
    let org_id =
        crate::services::workspace_reader::workspace_org(db.conn(), source_workspace_id).await?;
    let base_sources = captured_review_sources(
        &repo_storage.scoped(&*db),
        org_id,
        generated_vector,
        generated,
    )?;
    let mut full_hunks = HashMap::new();
    for row in &rows {
        if row.kind == "hunk" {
            full_hunks.insert(
                row.id,
                get_suggestion_for_capture(db, source_workspace_id, row.id, &base_sources).await?,
            );
        }
    }
    let mut taken_children = std::collections::HashSet::new();
    for (index, item) in generated
        .iter()
        .enumerate()
        .filter(|(_, item)| item.kind == "hunk")
    {
        let next = review_edits::discrete_edits(
            Uuid::nil(),
            item.before_lines.as_deref().unwrap_or_default(),
            &item.after_lines,
        );
        if let Some(previous) = full_hunks
            .values()
            .filter(|row| hunk_proposal_matches(row, item))
            .max_by_key(|row| row.created_at)
        {
            carried_edit_counts[index] = next.len();
            carried_open_edit_counts[index] = previous
                .edits
                .iter()
                .filter(|edit| matches!(edit.status.as_str(), "open" | "superseded"))
                .count()
                .min(next.len());
            continue;
        }
        let Some(source) = base_sources.get(&(item.repo_id, item.path.clone())) else {
            continue;
        };
        let mut candidates: HashMap<usize, HashMap<Uuid, bool>> = HashMap::new();
        for previous in full_hunks.values() {
            for (unit, matched) in child_source_matches(previous, item, &source.content, &next) {
                let edit = matched.edit;
                if !taken_children.contains(&edit.id) {
                    let pending = matches!(edit.status.as_str(), "open" | "superseded");
                    candidates
                        .entry(unit)
                        .or_default()
                        .entry(edit.id)
                        .and_modify(|open| *open &= pending)
                        .or_insert(pending);
                }
            }
        }
        let mut frequencies = HashMap::new();
        for ids in candidates.values().filter(|ids| ids.len() == 1) {
            *frequencies.entry(*ids.keys().next().unwrap()).or_insert(0) += 1;
        }
        for ids in candidates.values().filter(|ids| ids.len() == 1) {
            let (id, pending) = ids.iter().next().unwrap();
            let id = *id;
            if frequencies.get(&id) == Some(&1) {
                taken_children.insert(id);
                carried_edit_counts[index] += 1;
                carried_open_edit_counts[index] += usize::from(*pending);
            }
        }
    }
    let mut taken: std::collections::HashSet<Uuid> = std::collections::HashSet::new();
    let same_text = |row: &CarriedRow, item: &GeneratedSuggestion| {
        row.kind == item.kind
            && row.repo_id == Some(item.repo_id)
            && row.path.as_deref() == Some(item.path.as_str())
            && (if item.kind == "hunk" {
                full_hunks.get(&row.id).is_some_and(|row| {
                    hunk_proposal_matches(row, item)
                        || base_sources
                            .get(&(item.repo_id, item.path.clone()))
                            .is_some_and(|source| {
                                all_hunk_children_match(row, item, &source.content)
                            })
                })
            } else {
                row.anchor_start == item.anchor_start
                    && row.before_lines == item.before_lines
                    && row.after_lines.as_deref() == Some(item.after_lines.as_str())
            })
    };
    let to_carried = |row: &CarriedRow| Carried {
        suggestion_id: row.id,
        round: row.round,
        note: row.note.clone(),
    };
    // Reserve a complete proposal before considering artifact history.
    for (index, item) in generated.iter().enumerate() {
        if item.kind == "artifact" {
            continue;
        }
        let exact = rows
            .iter()
            .filter(|row| !taken.contains(&row.id) && same_text(row, item))
            .max_by_key(|row| row.created_at);
        if let Some(row) = exact {
            taken.insert(row.id);
            carried[index] = Some(to_carried(row));
        }
    }
    // A change left over may still continue the nearest remaining thread
    // when pushed, but a push carries its decision and note only for the
    // same complete proposal at its proven source location. An artifact
    // carries the latest row at
    // its path when the bytes match.
    for (index, item) in generated.iter().enumerate() {
        if carried[index].is_some() || item.kind != "artifact" {
            continue;
        }
        let Some(blob_oid) = item.blob_oid.as_deref() else {
            continue;
        };
        let latest_at_path = artifact_rows.iter().find(|row| {
            !taken.contains(&row.id)
                && row.repo_id == Some(item.repo_id)
                && row.path.as_deref() == Some(item.path.as_str())
        });
        if let Some(row) = latest_at_path {
            if row.blob_oid.as_deref() == Some(blob_oid) {
                taken.insert(row.id);
                carried[index] = Some(to_carried(row));
            }
        }
    }
    Ok((carried, carried_edit_counts, carried_open_edit_counts))
}

/// The source heads a publication's base vector names, per repository.
pub fn vector_heads(vector: &Value) -> HashMap<Uuid, String> {
    vector
        .as_array()
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| {
                    let repo_id = entry
                        .get("repoId")
                        .and_then(Value::as_str)
                        .and_then(|id| id.parse::<Uuid>().ok())?;
                    let head = entry.get("sourceHead").and_then(Value::as_str)?;
                    Some((repo_id, head.to_string()))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The source's current commit per repository: the tip of each member
/// branch in the mirror, which is what a round records as `sourceHead`
/// and what Update from the source rebases onto.
pub async fn source_heads(
    db: &mut ScopedDb,
    repo_storage: &TenantRepoStorage,
    source_workspace_id: Uuid,
) -> Result<HashMap<Uuid, String>, G5Error> {
    let org_id =
        crate::services::workspace_reader::workspace_org(db.conn(), source_workspace_id).await?;
    let members = crate::services::git::workspace_repositories::load_workspace_repositories(
        db.conn(),
        source_workspace_id,
        false,
    )
    .await?
    .into_iter()
    .map(|member| (member.repo_id, member.branch_name))
    .collect::<Vec<_>>();
    let heads = crate::services::workspace_reader::read_branch_head_oids(
        &repo_storage.scoped(&*db),
        org_id,
        &members,
    )
    .await?;
    Ok(heads
        .into_iter()
        .filter_map(|((repo_id, _), head)| head.map(|head| (repo_id, head)))
        .collect())
}

/// The committer time of each current source head, keyed like
/// [`source_heads`]. A repository whose mirror or commit cannot be read is
/// left out rather than failing the listing.
pub fn source_head_times(
    repo_storage: &ScopedRepoStorage,
    org_id: Uuid,
    source_heads: &HashMap<Uuid, String>,
) -> HashMap<Uuid, DateTime<Utc>> {
    source_heads
        .iter()
        .filter_map(|(repo_id, head)| {
            let repo_path = repo_storage.repo_path(org_id, *repo_id).ok()?;
            let repo = GitRepo::open(&repo_path).ok()?;
            let seconds = repo.commit_committed_at(head).ok()?;
            DateTime::<Utc>::from_timestamp(seconds, 0).map(|at| (*repo_id, at))
        })
        .collect()
}

/// The base vector of an annotation publication: the source heads it was
/// made against, one entry per repository.
pub fn annotation_vector(source_heads: &HashMap<Uuid, String>) -> Value {
    let mut entries: Vec<(&Uuid, &String)> = source_heads.iter().collect();
    entries.sort();
    Value::Array(
        entries
            .into_iter()
            .map(|(repo_id, head)| json!({"repoId": repo_id, "sourceHead": head}))
            .collect(),
    )
}

/// Whether a publication is about the source's current commit: every
/// repository its base vector names is still at that `sourceHead`. A
/// vector naming no repository (older rows) counts as current.
pub fn publication_is_current(vector: &Value, source_heads: &HashMap<Uuid, String>) -> bool {
    let Some(entries) = vector.as_array() else {
        return true;
    };
    entries.iter().all(|entry| {
        let repo_id = entry
            .get("repoId")
            .and_then(Value::as_str)
            .and_then(|id| id.parse::<Uuid>().ok());
        let head = entry.get("sourceHead").and_then(Value::as_str);
        match (repo_id, head) {
            (Some(repo_id), Some(head)) => source_heads
                .get(&repo_id)
                .is_some_and(|current| current.eq_ignore_ascii_case(head)),
            _ => true,
        }
    })
}

/// Open the thread of a suggestion: a `comment_index` row with scope
/// `suggestion` on the source workspace, authored by the reviewer. The comment
/// owner stores it; replies go through the same owner.
#[allow(clippy::too_many_arguments)]
async fn create_suggestion_thread(
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
    crate::services::comment_service::open_suggestion_thread(
        db,
        org_id,
        source_workspace_id,
        source_branch,
        author_id,
        repo_id,
        path,
        line,
        content,
    )
    .await
}

/// Append a person's reply or decision note to a suggestion's thread.
pub async fn add_thread_reply(
    db: &mut ScopedDb,
    comment_id: Uuid,
    author_id: Uuid,
    content: &str,
) -> Result<Uuid, G5Error> {
    crate::services::comment_service::append_thread_reply(db, comment_id, author_id, content).await
}

/// Publications of a source workspace with suggestion counts, newest first.
pub async fn list_publications(
    db: &mut ScopedDb,
    source_workspace_id: Uuid,
) -> Result<Vec<ReviewPublicationSummary>, G5Error> {
    normalize_open_suggestion_edits(db, source_workspace_id).await?;
    Ok(sqlx::query_as::<_, ReviewPublicationSummary>(
        "SELECT p.*,
                COALESCE(SUM(CASE WHEN jsonb_array_length(s.edits) > 0 THEN (SELECT count(*) FROM jsonb_array_elements(s.edits) e WHERE e->>'status' = 'open') WHEN s.status = 'open' THEN 1 ELSE 0 END), 0)::bigint AS open_count,
                COALESCE(SUM(CASE WHEN jsonb_array_length(s.edits) > 0 THEN (SELECT count(*) FROM jsonb_array_elements(s.edits) e WHERE e->>'status' = 'accepted') WHEN s.status = 'accepted' THEN 1 ELSE 0 END), 0)::bigint AS accepted_count,
                COALESCE(SUM(CASE WHEN jsonb_array_length(s.edits) > 0 THEN (SELECT count(*) FROM jsonb_array_elements(s.edits) e WHERE e->>'status' = 'rejected') WHEN s.status = 'rejected' THEN 1 ELSE 0 END), 0)::bigint AS rejected_count,
                COALESCE(SUM(CASE WHEN jsonb_array_length(s.edits) > 0 THEN (SELECT count(*) FROM jsonb_array_elements(s.edits) e WHERE e->>'status' = 'resolved') WHEN s.status = 'resolved' THEN 1 ELSE 0 END), 0)::bigint AS resolved_count,
                COALESCE(SUM(CASE WHEN jsonb_array_length(s.edits) > 0 THEN (SELECT count(*) FROM jsonb_array_elements(s.edits) e WHERE e->>'status' = 'superseded') WHEN s.status = 'superseded' THEN 1 ELSE 0 END), 0)::bigint AS superseded_count,
                COALESCE(SUM(CASE WHEN s.kind = 'artifact' THEN 1 ELSE 0 END), 0)::bigint AS artifact_count
         FROM review_publications p
         LEFT JOIN review_suggestions s ON s.publication_id = p.id
         WHERE p.source_workspace_id = $1
         GROUP BY p.id
         ORDER BY p.created_at DESC",
    )
    .bind(source_workspace_id)
    .fetch_all(db.conn())
    .await?)
}

/// One publication of `source_workspace_id`; NotFound when the id belongs to
/// another workspace.
pub async fn get_publication(
    db: &mut ScopedDb,
    source_workspace_id: Uuid,
    publication_id: Uuid,
) -> Result<ReviewPublicationRecord, G5Error> {
    sqlx::query_as::<_, ReviewPublicationRecord>(
        "SELECT p.* FROM review_publications p
         WHERE p.source_workspace_id = $1 AND p.id = $2",
    )
    .bind(source_workspace_id)
    .bind(publication_id)
    .fetch_optional(db.conn())
    .await?
    .ok_or_else(|| G5Error::NotFound {
        resource: format!("review publication {publication_id}"),
    })
}

/// Findings of a source workspace's publications, newest publication first.
pub async fn list_findings(
    db: &mut ScopedDb,
    source_workspace_id: Uuid,
    publication_id: Option<Uuid>,
) -> Result<Vec<ReviewFindingRecord>, G5Error> {
    Ok(sqlx::query_as::<_, ReviewFindingRecord>(
        "SELECT f.id, f.publication_id, f.title, f.severity, f.dimension, f.evidence, f.actions,
                f.status, f.position, f.created_at, f.repo_id, f.paths
         FROM review_findings f
         JOIN review_publications p ON p.id = f.publication_id
         WHERE p.source_workspace_id = $1 AND ($2::uuid IS NULL OR f.publication_id = $2)
         ORDER BY p.created_at DESC, f.position ASC",
    )
    .bind(source_workspace_id)
    .bind(publication_id)
    .fetch_all(db.conn())
    .await?)
}

/// Filters for listing suggestions.
#[derive(Debug, Clone, Default)]
pub struct SuggestionFilter {
    pub publication_id: Option<Uuid>,
    pub finding_id: Option<Uuid>,
    pub repo_id: Option<Uuid>,
    pub path: Option<String>,
    /// Only `open` (and `resolved` comments are excluded too).
    pub open_only: bool,
    /// Only rows whose publication is about the source's current commit
    /// (`publication_is_current`); what a file page shows.
    pub current_only: bool,
    /// Only rows of this kind (`hunk`, `new_file`, `artifact`, `comment`).
    pub kind: Option<String>,
    /// Only rows in this status (`open`, `accepted`, ...).
    pub status: Option<String>,
}

/// Suggestions of a source workspace, oldest first within a file so the
/// in-file overlay renders them in order.
pub async fn list_suggestions(
    db: &mut ScopedDb,
    source_workspace_id: Uuid,
    filter: &SuggestionFilter,
) -> Result<Vec<ReviewSuggestionRecord>, G5Error> {
    let mut rows = sqlx::query_as::<_, ReviewSuggestionRecord>(
        "SELECT s.id, s.publication_id, s.finding_id, s.repo_id, s.path, s.kind,
                s.before_lines, s.after_lines, s.anchor_start, s.content_hash, s.anchor_context,
                s.author_kind, s.author_label, s.note, s.comment_id,
                s.status, s.status_by, s.status_at, s.supersedes, s.adopted_from, s.created_at, s.blob_oid, s.blob_sha256, s.bytes, s.content_type, s.edits,
                p.reviewer_user_id
         FROM review_suggestions s
         JOIN review_publications p ON p.id = s.publication_id
         WHERE p.source_workspace_id = $1
           AND ($2::uuid IS NULL OR s.publication_id = $2)
           AND ($3::uuid IS NULL OR s.finding_id = $3)
           AND ($4::uuid IS NULL OR s.repo_id = $4)
           AND ($5::text IS NULL OR s.path = $5)
           AND (NOT $6 OR s.status = 'open')
           AND ($7::text IS NULL OR s.kind = $7)
           AND ($8::text IS NULL OR s.status = $8)
         ORDER BY s.path NULLS LAST, s.anchor_start NULLS LAST, s.created_at ASC",
    )
    .bind(source_workspace_id)
    .bind(filter.publication_id)
    .bind(filter.finding_id)
    .bind(filter.repo_id)
    .bind(&filter.path)
    .bind(filter.open_only)
    .bind(&filter.kind)
    .bind(&filter.status)
    .fetch_all(db.conn())
    .await?;
    for row in &mut rows {
        materialize_suggestion_edits(db, source_workspace_id, row).await?;
    }
    let keys: Vec<_> = rows
        .iter()
        .filter(|row| row.kind == "hunk" && !row.edits.is_empty())
        .filter_map(|row| Some((row.repo_id?, row.path.clone()?)))
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    let context_records = load_context_records(db, source_workspace_id, &keys).await?;
    for row in &mut rows {
        if row.kind == "hunk" && !row.edits.is_empty() {
            let context = accepted_context_from_records(row, None, &context_records);
            apply_suggestion_projection(row, context)?;
        }
    }
    Ok(rows)
}

/// Metadata readers need durable units and counts, never derived source text.
async fn normalize_open_suggestion_edits(
    db: &mut ScopedDb,
    source_workspace_id: Uuid,
) -> Result<(), G5Error> {
    let rows: Vec<ReviewSuggestionRecord> = sqlx::query_as(
        &format!("SELECT {QUALIFIED_REVIEW_SUGGESTION_COLUMNS} FROM review_suggestions s JOIN review_publications p ON p.id = s.publication_id
         WHERE p.source_workspace_id = $1 AND s.kind = 'hunk' AND s.status = 'open'
           AND jsonb_array_length(s.edits) = 0
         ORDER BY s.path NULLS LAST, s.anchor_start NULLS LAST, s.created_at, s.id"),
    )
    .bind(source_workspace_id)
    .fetch_all(db.conn())
    .await?;
    for mut row in rows {
        materialize_suggestion_edits(db, source_workspace_id, &mut row).await?;
    }
    Ok(())
}

/// Open suggestion counts per file of a source workspace, for tree badges.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct SuggestionPathCount {
    pub repo_id: Uuid,
    pub path: String,
    pub open_count: i64,
}

pub async fn count_open_suggestions_by_path(
    db: &mut ScopedDb,
    source_workspace_id: Uuid,
    source_heads: &HashMap<Uuid, String>,
) -> Result<Vec<SuggestionPathCount>, G5Error> {
    let publications = list_publications(db, source_workspace_id).await?;
    // Group/count durable statuses directly. Tree badges must not reconstruct
    // a source frame for every child or capture in the workspace's history.
    let rows: Vec<(Uuid, String, Uuid, i64, i64)> = sqlx::query_as(
        "SELECT s.repo_id, s.path, s.publication_id,
                SUM(CASE WHEN jsonb_array_length(s.edits) > 0 THEN
                    (SELECT count(*) FROM jsonb_array_elements(s.edits) e WHERE e->>'status' = 'open')
                    ELSE 1 END)::bigint AS open_count,
                SUM(CASE WHEN jsonb_array_length(s.edits) > 0 THEN
                    (SELECT count(*) FROM jsonb_array_elements(s.edits) e WHERE e->>'status' = 'open')
                    ELSE 0 END)::bigint AS continuing_open_count
         FROM review_suggestions s JOIN review_publications p ON p.id = s.publication_id
         WHERE p.source_workspace_id = $1 AND s.status = 'open'
           AND s.repo_id IS NOT NULL AND s.path IS NOT NULL
         GROUP BY s.repo_id, s.path, s.publication_id",
    )
    .bind(source_workspace_id)
    .fetch_all(db.conn())
    .await?;
    let mut counts: Vec<SuggestionPathCount> = Vec::new();
    for (repo_id, path, publication_id, total_open_count, continuing_open_count) in rows {
        let publication = publications
            .iter()
            .find(|p| p.publication.id == publication_id);
        let current = publication.is_none_or(|p| {
            publication_is_current(&p.publication.base_commit_vector, source_heads)
        });
        // Only each ledger's own units continue across a source-head change;
        // unrelated comments or empty legacy parents do not become current.
        let open_count = if current {
            total_open_count
        } else if publication.is_some_and(|p| p.publication.superseded_by.is_none()) {
            continuing_open_count
        } else {
            0
        };
        if open_count == 0 {
            continue;
        }
        if let Some(count) = counts
            .iter_mut()
            .find(|c| c.repo_id == repo_id && c.path == path)
        {
            count.open_count += open_count;
        } else {
            counts.push(SuggestionPathCount {
                repo_id,
                path,
                open_count,
            });
        }
    }
    Ok(counts)
}

/// Materialize an old open parent once. Read-only viewing creates no discussion
/// or content write; threads are opened lazily by an admitted action.
fn durable_edit_ledger(edits: &[ReviewSuggestionEdit]) -> Vec<ReviewSuggestionEdit> {
    edits
        .iter()
        .cloned()
        .map(|mut edit| {
            edit.source_conflict = false;
            edit.current_start = 0;
            edit.current_end = 0;
            edit.expected_before_lines.clear();
            edit.expected_after_lines.clear();
            edit
        })
        .collect()
}

/// A source-only composition view. Context receipts never become this
/// parent's decisions or escape into its persisted/serialized child ledger.
struct AcceptedContextLedger {
    edits: Vec<ReviewSuggestionEdit>,
    blocked: std::collections::HashSet<Uuid>,
}

fn captured_point(row: &ReviewSuggestionRecord, offset: usize) -> Option<(usize, usize)> {
    let before = row.before_lines.as_deref()?;
    let prefix = before.get(..offset)?;
    let anchor = usize::try_from(row.anchor_start?).ok()?;
    Some((
        anchor + prefix.bytes().filter(|byte| *byte == b'\n').count(),
        prefix.rsplit('\n').next()?.len(),
    ))
}

fn local_captured_point(row: &ReviewSuggestionRecord, point: (usize, usize)) -> Option<usize> {
    let before = row.before_lines.as_deref()?;
    let anchor = usize::try_from(row.anchor_start?).ok()?;
    let line = point.0.checked_sub(anchor)?;
    let mut offset = 0;
    let mut lines = before.split_inclusive('\n');
    for _ in 0..line {
        offset += lines.next()?.len();
    }
    let remaining = lines.next().unwrap_or_default().trim_end_matches('\n');
    (point.1 <= remaining.len() && remaining.is_char_boundary(point.1)).then_some(offset + point.1)
}

struct AcceptedContextReceipt {
    parent: Uuid,
    order: usize,
    edit: ReviewSuggestionEdit,
}

fn context_parent_root(roots: &HashMap<Uuid, Uuid>, mut parent: Uuid) -> Uuid {
    while let Some(next) = roots.get(&parent).filter(|next| **next != parent) {
        parent = *next;
    }
    parent
}

fn context_receipts_compete(a: &AcceptedContextReceipt, b: &AcceptedContextReceipt) -> bool {
    review_edits::byte_spans_overlap((a.edit.start, a.edit.end), (b.edit.start, b.edit.end))
        && !(a.parent == b.parent && a.edit.start == a.edit.end && b.edit.start == b.edit.end)
}

async fn load_context_records(
    db: &mut ScopedDb,
    source_workspace_id: Uuid,
    keys: &[(Uuid, String)],
) -> Result<Vec<ReviewSuggestionRecord>, G5Error> {
    if keys.is_empty() {
        return Ok(Vec::new());
    }
    let repos: Vec<_> = keys.iter().map(|(repo, _)| *repo).collect();
    let paths: Vec<_> = keys.iter().map(|(_, path)| path.clone()).collect();
    Ok(sqlx::query_as(
        &format!("SELECT {QUALIFIED_REVIEW_SUGGESTION_COLUMNS} FROM review_suggestions s JOIN review_publications p ON p.id = s.publication_id
         WHERE p.source_workspace_id = $1 AND s.kind = 'hunk' AND jsonb_array_length(s.edits) > 0
           AND EXISTS (SELECT 1 FROM unnest($2::uuid[], $3::text[]) AS files(repo_id, path)
                       WHERE s.repo_id = files.repo_id AND s.path = files.path)
         ORDER BY s.created_at, s.id"),
    )
    .bind(source_workspace_id)
    .bind(&repos)
    .bind(&paths)
    .fetch_all(db.conn())
    .await?)
}

async fn accepted_context_ledger(
    db: &mut ScopedDb,
    source_workspace_id: Uuid,
    suggestion: &ReviewSuggestionRecord,
    current_version: Option<&str>,
) -> Result<AcceptedContextLedger, G5Error> {
    let keys = suggestion
        .repo_id
        .zip(suggestion.path.clone())
        .into_iter()
        .collect::<Vec<_>>();
    let records = load_context_records(db, source_workspace_id, &keys).await?;
    Ok(accepted_context_from_records(
        suggestion,
        current_version,
        &records,
    ))
}

fn accepted_context_from_records(
    suggestion: &ReviewSuggestionRecord,
    current_version: Option<&str>,
    records: &[ReviewSuggestionRecord],
) -> AcceptedContextLedger {
    let mut context = AcceptedContextLedger {
        edits: durable_edit_ledger(&suggestion.edits),
        blocked: std::collections::HashSet::new(),
    };
    let (Some(repo), Some(path), Some(hash)) = (
        suggestion.repo_id,
        suggestion.path.as_deref(),
        suggestion.content_hash.as_deref(),
    ) else {
        return context;
    };
    // Captures supply coordinates, not decisions. A continued child can be
    // accepted in a later capture: only its durable UUID and exact immutable
    // before/proposed bytes connect that outcome to this original frame.
    let mut captures: Vec<&ReviewSuggestionRecord> = records
        .iter()
        .filter(|row| {
            row.repo_id == Some(repo)
                && row.path.as_deref() == Some(path)
                && row.content_hash.as_deref() == Some(hash)
                && row.id != suggestion.id
        })
        .collect();
    captures.push(suggestion);
    let child_ids: std::collections::HashSet<_> = captures
        .iter()
        .flat_map(|capture| capture.edits.iter().map(|edit| edit.id))
        .collect();
    let outcomes: Vec<_> = records
        .iter()
        .filter(|row| {
            row.repo_id == Some(repo)
                && row.path.as_deref() == Some(path)
                && row.edits.iter().any(|edit| {
                    child_ids.contains(&edit.id)
                        && edit.status == "accepted"
                        && edit.written_hash.is_some()
                })
        })
        .collect();
    let mut accepted_by_id: HashMap<Uuid, Vec<&ReviewSuggestionEdit>> = HashMap::new();
    for outcome in &outcomes {
        for edit in outcome
            .edits
            .iter()
            .filter(|edit| edit.status == "accepted" && edit.written_hash.is_some())
        {
            accepted_by_id.entry(edit.id).or_default().push(edit);
        }
    }
    let before = suggestion.before_lines.as_deref().unwrap_or_default();
    let mut external: Vec<AcceptedContextReceipt> = Vec::new();
    let mut parent_roots = HashMap::new();
    for receipt in captures {
        for (order, captured) in receipt.edits.iter().enumerate() {
            let Some(outcomes) = accepted_by_id.get(&captured.id) else {
                continue;
            };
            let accepted_text = |edit: &ReviewSuggestionEdit| {
                edit.accepted_after_text
                    .clone()
                    .unwrap_or_else(|| edit.after_text.clone())
            };
            let replacement = accepted_text(outcomes[0]);
            // A UUID alone cannot authorize changed proposal meaning or an
            // inconsistent settled replacement. Refuse contradictory lineage
            // instead of borrowing coordinates from a different capture.
            if outcomes.iter().any(|edit| {
                edit.before_text != captured.before_text
                    || edit.after_text != captured.after_text
                    || accepted_text(edit) != replacement
            }) {
                continue;
            }
            let edit = outcomes
                .iter()
                .copied()
                .find(|edit| {
                    current_version.is_some() && edit.written_hash.as_deref() == current_version
                })
                .unwrap_or(outcomes[0]);
            if suggestion
                .edits
                .iter()
                .any(|own| own.id == edit.id && own.status == "accepted")
            {
                continue;
            }
            let Some(start) = captured_point(receipt, captured.start)
                .and_then(|point| local_captured_point(suggestion, point))
            else {
                continue;
            };
            let Some(end) = captured_point(receipt, captured.end)
                .and_then(|point| local_captured_point(suggestion, point))
            else {
                continue;
            };
            if before.get(start..end) != Some(edit.before_text.as_str()) {
                continue;
            }
            let mut mapped = edit.clone();
            mapped.start = start;
            mapped.end = end;
            if let Some(old) = external.iter_mut().find(|old| {
                old.edit.id == mapped.id
                    && old.edit.start == start
                    && old.edit.end == end
                    && old.edit.before_text == mapped.before_text
                    && old
                        .edit
                        .accepted_after_text
                        .as_ref()
                        .unwrap_or(&old.edit.after_text)
                        == mapped
                            .accepted_after_text
                            .as_ref()
                            .unwrap_or(&mapped.after_text)
            }) {
                // A carried identity with unchanged source-array order proves
                // the same ordered parent ledger, independent of its display parent.
                if old.order == order {
                    let a = context_parent_root(&parent_roots, old.parent);
                    let b = context_parent_root(&parent_roots, receipt.id);
                    parent_roots.insert(a.max(b), a.min(b));
                }
                if current_version.is_some() && mapped.written_hash.as_deref() == current_version {
                    old.edit = mapped;
                }
            } else {
                external.push(AcceptedContextReceipt {
                    parent: receipt.id,
                    order,
                    edit: mapped,
                });
            }
        }
    }
    for receipt in &mut external {
        receipt.parent = context_parent_root(&parent_roots, receipt.parent);
    }
    // Source capture order governs compatible same-point blocks. Different
    // parents at the same point remain competing, including equal text.
    external.sort_by_key(|receipt| {
        (
            receipt.edit.start,
            receipt.edit.end,
            receipt.parent,
            receipt.order,
        )
    });
    // Only exact equality with the file version already read for this action
    // proves a current effect. Metadata-only projections have no such witness.
    let witnessed: Vec<_> = external
        .iter()
        .enumerate()
        .filter(|(_, receipt)| {
            current_version.is_some() && receipt.edit.written_hash.as_deref() == current_version
        })
        .map(|(index, _)| index)
        .collect();
    let disproven: std::collections::HashSet<_> = external
        .iter()
        .enumerate()
        .filter(|(_, receipt)| {
            receipt.edit.written_hash.as_deref() != current_version
                && witnessed
                    .iter()
                    .any(|index| context_receipts_compete(receipt, &external[*index]))
        })
        .map(|(index, _)| index)
        .collect();
    external = external
        .into_iter()
        .enumerate()
        .filter(|(index, _)| !disproven.contains(index))
        .map(|(_, receipt)| receipt)
        .collect();
    // Conflicting receipts do not establish which replacement is canonical.
    // Do not pick a newer/nearer proposal or manufacture a combined outcome.
    let ambiguous: std::collections::HashSet<_> = external
        .iter()
        .enumerate()
        .filter(|(index, receipt)| {
            external.iter().enumerate().any(|(other, candidate)| {
                other != *index && context_receipts_compete(receipt, candidate)
            })
        })
        .map(|(index, _)| index)
        .collect();
    let mut applied_context = Vec::new();
    for (index, receipt) in external.into_iter().enumerate() {
        if ambiguous.contains(&index) {
            continue;
        }
        let edit = receipt.edit;
        let overlaps_accepted = suggestion.edits.iter().any(|own| {
            own.status == "accepted"
                && review_edits::byte_spans_overlap((own.start, own.end), (edit.start, edit.end))
        });
        if overlaps_accepted
            && !(current_version.is_some() && edit.written_hash.as_deref() == current_version)
        {
            continue;
        }
        for own in &suggestion.edits {
            if review_edits::byte_spans_overlap((own.start, own.end), (edit.start, edit.end))
                || own.id == edit.id
            {
                context.blocked.insert(own.id);
            }
        }
        applied_context.push(edit);
    }
    context
        .edits
        .retain(|edit| !context.blocked.contains(&edit.id));
    context.edits.extend(applied_context);
    // Stable sort preserves the source order of a parent's same-point inserted blocks.
    context.edits.sort_by_key(|edit| (edit.start, edit.end));
    context
}

async fn refresh_suggestion_projection(
    db: &mut ScopedDb,
    source_workspace_id: Uuid,
    suggestion: &mut ReviewSuggestionRecord,
) -> Result<(), G5Error> {
    refresh_suggestion_projection_with_version(db, source_workspace_id, suggestion, None).await
}

/// Explicit selected-source/file-version evidence from an admitted action or
/// immutable publication capture. Metadata-only reads neither fetch a VM nor
/// infer a current effect from the age of an accepted receipt.
pub async fn refresh_suggestion_projection_with_version(
    db: &mut ScopedDb,
    source_workspace_id: Uuid,
    suggestion: &mut ReviewSuggestionRecord,
    current_version: Option<&str>,
) -> Result<(), G5Error> {
    let context =
        accepted_context_ledger(db, source_workspace_id, suggestion, current_version).await?;
    apply_suggestion_projection(suggestion, context)
}

fn apply_suggestion_projection(
    suggestion: &mut ReviewSuggestionRecord,
    mut context: AcceptedContextLedger,
) -> Result<(), G5Error> {
    let captured = suggestion.before_lines.as_deref().unwrap_or_default();
    suggestion.expected_before_lines = review_edits::compose_current(captured, &context.edits)
        .map_err(|message| G5Error::Internal { message })?;
    review_edits::refresh_edit_projection(captured, &mut context.edits)
        .map_err(|message| G5Error::Internal { message })?;
    for edit in &mut suggestion.edits {
        edit.source_conflict = context.blocked.contains(&edit.id);
        if let Some(projected) = context
            .edits
            .iter()
            .find(|projected| projected.id == edit.id)
        {
            edit.current_start = projected.current_start;
            edit.current_end = projected.current_end;
            edit.expected_before_lines = projected.expected_before_lines.clone();
            edit.expected_after_lines = projected.expected_after_lines.clone();
        } else {
            edit.current_start = 0;
            edit.current_end = 0;
            edit.expected_before_lines.clear();
            edit.expected_after_lines.clear();
        }
    }
    Ok(())
}

/// Exact live raw identity restores the captured coordinate frame for presentation
/// and parent replacement parsing. Decision receipts remain immutable history.
pub fn use_captured_source_projection(suggestion: &mut ReviewSuggestionRecord) {
    suggestion.expected_before_lines = suggestion.before_lines.clone().unwrap_or_default();
    for edit in &mut suggestion.edits {
        edit.source_conflict = false;
        edit.current_start = edit.start;
        edit.current_end = edit.end;
        edit.expected_before_lines.clear();
        edit.expected_after_lines.clear();
    }
}

/// Reads and decisions reconstruct the same source-only context. Selected
/// actual children retain their own status and captured bytes throughout.
pub async fn compose_suggestion_before_and_after(
    db: &mut ScopedDb,
    source_workspace_id: Uuid,
    suggestion: &ReviewSuggestionRecord,
    selected: &[Uuid],
    overrides: &HashMap<Uuid, String>,
    captured_basis_trusted: bool,
    current_version: &str,
) -> Result<(String, String), G5Error> {
    let context = if captured_basis_trusted {
        AcceptedContextLedger {
            // Previously accepted bytes may have been deliberately reverted.
            // Exact current identity permits only this request's open deltas.
            edits: durable_edit_ledger(&suggestion.edits)
                .into_iter()
                .filter(|edit| edit.status != "accepted")
                .collect(),
            blocked: std::collections::HashSet::new(),
        }
    } else {
        accepted_context_ledger(db, source_workspace_id, suggestion, Some(current_version)).await?
    };
    if selected.iter().any(|id| context.blocked.contains(id)) {
        return Err(G5Error::Conflict { reason: "This edit overlaps an accepted captured-source change; it remains visible for attention".into() });
    }
    review_edits::compose_before_and_after(
        suggestion.before_lines.as_deref().unwrap_or_default(),
        &context.edits,
        selected,
        overrides,
    )
    .map_err(|reason| G5Error::Conflict { reason })
}

pub async fn ensure_suggestion_edits(
    db: &mut ScopedDb,
    source_workspace_id: Uuid,
    suggestion: &mut ReviewSuggestionRecord,
) -> Result<(), G5Error> {
    if suggestion.kind != "hunk" {
        return Ok(());
    }
    materialize_suggestion_edits(db, source_workspace_id, suggestion).await?;
    if !suggestion.edits.is_empty() {
        refresh_suggestion_projection(db, source_workspace_id, suggestion).await?;
    }
    Ok(())
}

async fn materialize_suggestion_edits(
    db: &mut ScopedDb,
    source_workspace_id: Uuid,
    suggestion: &mut ReviewSuggestionRecord,
) -> Result<(), G5Error> {
    if suggestion.kind != "hunk" {
        return Ok(());
    }
    if suggestion.edits.is_empty() && suggestion.status == "open" {
        let proposed = review_edits::discrete_edits(
            suggestion.id,
            suggestion.before_lines.as_deref().unwrap_or_default(),
            suggestion.after_lines.as_deref().unwrap_or_default(),
        );
        // An empty/no-op legacy proposal has no identities to persist.
        // Repeated viewing must not lock against a VM-backed decision or write [].
        if proposed.is_empty() {
            return Ok(());
        }
        lock_review_decisions(db, source_workspace_id).await?;
        let (edits, status): (sqlx::types::Json<Vec<ReviewSuggestionEdit>>, String) =
            sqlx::query_as("SELECT edits, status FROM review_suggestions WHERE id = $1 FOR UPDATE")
                .bind(suggestion.id)
                .fetch_one(db.conn())
                .await?;
        suggestion.status = status;
        suggestion.edits = edits.0;
        if suggestion.edits.is_empty() && suggestion.status == "open" {
            suggestion.edits = proposed;
            sqlx::query("UPDATE review_suggestions SET edits = $2 WHERE id = $1")
                .bind(suggestion.id)
                .bind(sqlx::types::Json(durable_edit_ledger(&suggestion.edits)))
                .execute(db.conn())
                .await?;
        }
    }
    Ok(())
}

/// Aggregate only child outcomes. No parent may claim acceptance with an open
/// or rejected child; unrelated terminal retirement statuses remain explicit.
pub fn edit_summary_status(edits: &[ReviewSuggestionEdit]) -> &'static str {
    if edits.iter().any(|e| e.status == "open") {
        "open"
    } else if edits.iter().all(|e| e.status == "accepted") {
        "accepted"
    } else if edits.iter().all(|e| e.status == "rejected") {
        "rejected"
    } else if edits.iter().all(|e| e.status == "obsolete") {
        "obsolete"
    } else if edits.iter().all(|e| e.status == "withdrawn") {
        "withdrawn"
    } else if edits.iter().all(|e| e.status == "superseded") {
        "superseded"
    } else {
        "partial"
    }
}

pub async fn save_suggestion_edits(
    db: &mut ScopedDb,
    source_workspace_id: Uuid,
    suggestion: &ReviewSuggestionRecord,
    by: Uuid,
) -> Result<ReviewSuggestionRecord, G5Error> {
    let mut updated = sqlx::query_as::<_, ReviewSuggestionRecord>(
        &format!("UPDATE review_suggestions SET edits = $2, status = $3, status_by = $4, status_at = now() WHERE id = $1 RETURNING {REVIEW_SUGGESTION_COLUMNS}"))
        .bind(suggestion.id).bind(sqlx::types::Json(durable_edit_ledger(&suggestion.edits))).bind(edit_summary_status(&suggestion.edits)).bind(by)
        .fetch_one(db.conn()).await?;
    refresh_suggestion_projection(db, source_workspace_id, &mut updated).await?;
    Ok(updated)
}

pub async fn ensure_edit_thread(
    db: &mut ScopedDb,
    source_workspace_id: Uuid,
    suggestion: &mut ReviewSuggestionRecord,
    publication: &ReviewPublicationRecord,
    edit_id: Uuid,
) -> Result<Uuid, G5Error> {
    let index = suggestion
        .edits
        .iter()
        .position(|e| e.id == edit_id)
        .ok_or_else(|| G5Error::NotFound {
            resource: format!("review edit {edit_id}"),
        })?;
    if let Some(id) = suggestion.edits[index].comment_id {
        return Ok(id);
    }
    let org_id = db
        .primary_org_id()
        .map_err(|error| G5Error::Internal {
            message: error.to_string(),
        })?
        .into_uuid();
    let (_, branch) = workspace_owner_and_branch(db, org_id, source_workspace_id).await?;
    let comment_id = create_suggestion_thread(
        db,
        org_id,
        source_workspace_id,
        &branch,
        publication.reviewer_user_id,
        suggestion.repo_id,
        suggestion.path.as_deref(),
        suggestion.anchor_start,
        &format!(
            "Suggested edit to {}",
            suggestion.path.as_deref().unwrap_or("document")
        ),
    )
    .await?;
    suggestion.edits[index].comment_id = Some(comment_id);
    sqlx::query("UPDATE review_suggestions SET edits = $2 WHERE id = $1")
        .bind(suggestion.id)
        .bind(sqlx::types::Json(durable_edit_ledger(&suggestion.edits)))
        .execute(db.conn())
        .await?;
    Ok(comment_id)
}

pub fn edit_thread_id(
    suggestion: &ReviewSuggestionRecord,
    edit_id: Option<Uuid>,
) -> Result<Option<Uuid>, G5Error> {
    match edit_id {
        Some(id) => suggestion
            .edits
            .iter()
            .find(|e| e.id == id)
            .map(|e| e.comment_id)
            .ok_or_else(|| G5Error::NotFound {
                resource: format!("review edit {id}"),
            }),
        None => Ok(suggestion.comment_id),
    }
}

pub async fn lock_suggestion(
    db: &mut ScopedDb,
    source_workspace_id: Uuid,
    suggestion_id: Uuid,
) -> Result<(ReviewSuggestionRecord, ReviewPublicationRecord), G5Error> {
    lock_review_decisions(db, source_workspace_id).await?;
    let (_, publication) = get_suggestion(db, source_workspace_id, suggestion_id).await?;
    let mut suggestion = sqlx::query_as::<_, ReviewSuggestionRecord>(&format!(
        "SELECT {REVIEW_SUGGESTION_COLUMNS} FROM review_suggestions WHERE id = $1 FOR UPDATE"
    ))
    .bind(suggestion_id)
    .fetch_one(db.conn())
    .await?;
    ensure_suggestion_edits(db, source_workspace_id, &mut suggestion).await?;
    Ok((suggestion, publication))
}

/// A suggestion of `source_workspace_id`, with its publication.
/// Serialize review decisions (accept, reject, withdraw, resolve) on one
/// source workspace for the rest of the transaction. Taken before any
/// suggestion or finding row is locked and before an accept writes the
/// file, so two decisions never lock rows in opposite orders (a finding
/// groups suggestions across files, and retiring competitors touches
/// other findings) and then deadlock, which PostgreSQL resolves by
/// aborting one transaction whose file change already landed.
pub async fn lock_review_decisions(
    db: &mut ScopedDb,
    source_workspace_id: Uuid,
) -> Result<(), G5Error> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
        .bind(format!("review-decide:{source_workspace_id}"))
        .execute(db.conn())
        .await?;
    Ok(())
}

pub async fn get_suggestion(
    db: &mut ScopedDb,
    source_workspace_id: Uuid,
    suggestion_id: Uuid,
) -> Result<(ReviewSuggestionRecord, ReviewPublicationRecord), G5Error> {
    let publication = sqlx::query_as::<_, ReviewPublicationRecord>(
        "SELECT p.* FROM review_publications p
         JOIN review_suggestions s ON s.publication_id = p.id
         WHERE p.source_workspace_id = $1 AND s.id = $2",
    )
    .bind(source_workspace_id)
    .bind(suggestion_id)
    .fetch_optional(db.conn())
    .await?
    .ok_or_else(|| G5Error::NotFound {
        resource: format!("review suggestion {suggestion_id}"),
    })?;
    let mut suggestion = sqlx::query_as::<_, ReviewSuggestionRecord>(
        "SELECT id, publication_id, finding_id, repo_id, path, kind, before_lines, after_lines,
                anchor_start, content_hash, anchor_context, author_kind, author_label,
                note, comment_id, status, status_by, status_at, supersedes,
                adopted_from, created_at, blob_oid, blob_sha256, bytes, content_type, edits
         FROM review_suggestions WHERE id = $1",
    )
    .bind(suggestion_id)
    .fetch_one(db.conn())
    .await?;
    ensure_suggestion_edits(db, source_workspace_id, &mut suggestion).await?;
    Ok((suggestion, publication))
}

/// Load an open suggestion and lock its row for the rest of `db`'s
/// transaction, so competing decisions (accept, reject, withdraw, resolve)
/// queue behind this one and then see its terminal status instead of
/// overwriting it. Accept holds the lock across its file write, so a reject
/// cannot land on a change that was just written.
pub async fn lock_open_suggestion(
    db: &mut ScopedDb,
    source_workspace_id: Uuid,
    suggestion_id: Uuid,
) -> Result<(ReviewSuggestionRecord, ReviewPublicationRecord), G5Error> {
    let (suggestion, publication) = lock_suggestion(db, source_workspace_id, suggestion_id).await?;
    if suggestion.status != "open" {
        return Err(G5Error::Conflict {
            reason: format!("suggestion is {}", suggestion.status),
        });
    }
    Ok((suggestion, publication))
}

/// Move an open suggestion to a terminal status, recording who and when.
/// Refuses (409) when another decision landed first.
pub async fn set_suggestion_status(
    db: &mut ScopedDb,
    suggestion_id: Uuid,
    status: &str,
    by: Uuid,
    edited_after_lines: Option<&str>,
) -> Result<ReviewSuggestionRecord, G5Error> {
    set_suggestion_status_if_open(db, suggestion_id, status, by, edited_after_lines)
        .await?
        .ok_or_else(|| G5Error::Conflict {
            reason: "suggestion was already decided".to_string(),
        })
}

/// [`set_suggestion_status`] for a write that tolerates a suggestion decided
/// meanwhile: `None` when it was no longer open.
pub async fn set_suggestion_status_if_open(
    db: &mut ScopedDb,
    suggestion_id: Uuid,
    status: &str,
    by: Uuid,
    edited_after_lines: Option<&str>,
) -> Result<Option<ReviewSuggestionRecord>, G5Error> {
    Ok(sqlx::query_as::<_, ReviewSuggestionRecord>(
        "UPDATE review_suggestions
         SET status = $2, status_by = $3, status_at = now(),
             after_lines = COALESCE($4, after_lines),
             edits = CASE WHEN $2 IN ('withdrawn','obsolete','superseded') THEN
                 COALESCE((SELECT jsonb_agg(CASE WHEN e->>'status' = 'open' THEN
                     e || jsonb_build_object('status',$2::text,'statusBy',$3::uuid,'statusAt',now()) ELSE e END)
                     FROM jsonb_array_elements(edits) e), '[]'::jsonb) ELSE edits END
         WHERE id = $1 AND status = 'open'
         RETURNING id, publication_id, finding_id, repo_id, path, kind, before_lines, after_lines,
                   anchor_start, content_hash, anchor_context, author_kind, author_label,
                   note, comment_id, status, status_by, status_at, supersedes,
                   adopted_from, created_at, blob_oid, blob_sha256, bytes, content_type, edits",
    )
    .bind(suggestion_id)
    .bind(status)
    .bind(by)
    .bind(edited_after_lines)
    .fetch_optional(db.conn())
    .await?)
}

/// The base line range `[start, end)` a hunk suggestion replaces, 1-based.
fn hunk_edited_spans_of(suggestion: &ReviewSuggestionRecord) -> Option<Vec<(i32, i32)>> {
    hunk_edited_spans(
        suggestion.anchor_start?,
        suggestion.before_lines.as_deref().unwrap_or(""),
        suggestion.after_lines.as_deref().unwrap_or(""),
    )
}

/// The base lines a hunk actually edits, from a line diff of the replaced
/// block against the replacement: replaced lines as `[start, end)` spans
/// (1-based) and insertion points as empty spans `(p, p)` (the insertion
/// goes before base line `p`). A line the replacement keeps (a diff's
/// context rows, and unchanged lines between two nearby edits git folded
/// into one hunk) is not edited. `None` when the blocks are too large for
/// an exact diff: an approximate answer would retire or group unrelated
/// proposals, so such a hunk takes no part in competition at all.
pub fn hunk_edited_spans(anchor_start: i32, before: &str, after: &str) -> Option<Vec<(i32, i32)>> {
    let before = split_lines(before);
    let after = split_lines(after);
    let (n, m) = (before.len(), after.len());
    if n.saturating_mul(m) > MAX_EDIT_DIFF_CELLS {
        return None;
    }
    let mut spans: Vec<(usize, usize)> = Vec::new();
    let replace = |spans: &mut Vec<(usize, usize)>, at: usize| {
        if let Some(last) = spans.last_mut() {
            if last.0 != last.1 && last.1 >= at {
                last.1 = last.1.max(at + 1);
                return;
            }
            // The walk emits a replacement as its inserted lines first, then
            // the removed line: that insertion point becomes the interval.
            if *last == (at, at) {
                *last = (at, at + 1);
                return;
            }
        }
        spans.push((at, at + 1));
    };
    let insert = |spans: &mut Vec<(usize, usize)>, at: usize| {
        if spans.last() != Some(&(at, at)) {
            spans.push((at, at));
        }
    };
    // Longest common subsequence of lines; everything off it is an edit.
    let mut table = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            table[i][j] = if before[i] == after[j] {
                table[i + 1][j + 1] + 1
            } else {
                table[i + 1][j].max(table[i][j + 1])
            };
        }
    }
    let (mut i, mut j) = (0, 0);
    while i < n || j < m {
        if i < n && j < m && before[i] == after[j] {
            i += 1;
            j += 1;
        } else if j < m && (i >= n || table[i][j + 1] >= table[i + 1][j]) {
            // `after[j]` is inserted before `before[i]`: a point, unless a
            // replacement of the lines just before it is in progress (the
            // inserted line then belongs to that replacement).
            match spans.last() {
                Some(last) if last.0 != last.1 && last.1 == i => {}
                _ => insert(&mut spans, i),
            }
            j += 1;
        } else {
            // `before[i]` is removed (or replaced).
            replace(&mut spans, i);
            i += 1;
        }
    }
    Some(
        spans
            .into_iter()
            .map(|(s, e)| (anchor_start + s as i32, anchor_start + e as i32))
            .collect(),
    )
}

/// The largest `before × after` line table diffed exactly (4 MiB of
/// counters); a hunk beyond it has no edited spans (`hunk_edited_spans`).
const MAX_EDIT_DIFF_CELLS: usize = 1_000_000;

/// Whether the edits of one hunk touch the edits of another: two replaced
/// intervals that intersect, two insertions at the same point, or an
/// insertion strictly inside a replaced interval. An insertion before a
/// line and a replacement of that line do not touch: the replacement's
/// block is still whole after the insertion and re-anchors.
pub fn spans_overlap(a: &[(i32, i32)], b: &[(i32, i32)]) -> bool {
    let touches = |(s1, e1): (i32, i32), (s2, e2): (i32, i32)| match (s1 == e1, s2 == e2) {
        (false, false) => s1 < e2 && s2 < e1,
        (true, true) => s1 == s2,
        (true, false) => s2 < s1 && s1 < e2,
        (false, true) => s1 < s2 && s2 < e1,
    };
    a.iter().any(|x| b.iter().any(|y| touches(*x, *y)))
}

/// Open hunk suggestions on the same file whose replaced lines overlap
/// `accepted`'s: once the owner chose one proposal for those lines, the
/// competing ones can no longer apply and are marked obsolete. Returns the
/// suggestions that were retired.
pub async fn obsolete_competing_suggestions(
    db: &mut ScopedDb,
    source_workspace_id: Uuid,
    accepted: &ReviewSuggestionRecord,
    by: Uuid,
) -> Result<Vec<ReviewSuggestionRecord>, G5Error> {
    let (Some(repo_id), Some(path), Some(spans)) = (
        accepted.repo_id,
        accepted.path.as_deref(),
        hunk_edited_spans_of(accepted),
    ) else {
        return Ok(Vec::new());
    };
    // A repository can be a member of several workspaces; only proposals on
    // *this* source compete (its publications), never another workspace's.
    let candidates = sqlx::query_as::<_, ReviewSuggestionRecord>(
        "SELECT s.id, s.publication_id, s.finding_id, s.repo_id, s.path, s.kind, s.before_lines,
                s.after_lines, s.anchor_start, s.content_hash, s.anchor_context,
                s.author_kind, s.author_label, s.note, s.comment_id, s.status,
                s.status_by, s.status_at, s.supersedes, s.adopted_from, s.created_at, s.blob_oid,
                s.blob_sha256, s.bytes, s.content_type, s.edits
         FROM review_suggestions s
         JOIN review_publications p ON p.id = s.publication_id
         WHERE p.source_workspace_id = $4
           AND s.repo_id = $1 AND s.path = $2 AND s.kind = 'hunk' AND s.status = 'open'
           AND s.id <> $3
         ORDER BY s.created_at",
    )
    .bind(repo_id)
    .bind(path)
    .bind(accepted.id)
    .bind(source_workspace_id)
    .fetch_all(db.conn())
    .await?;
    let mut retired = Vec::new();
    for candidate in candidates {
        // Line numbers only compare within one published version of the
        // file: a proposal computed against an older or newer source (the
        // reviewers forked or updated at different times) may name the same
        // lines while meaning different code, so it stays open and is judged
        // by re-anchoring when the owner turns to it.
        if candidate.content_hash != accepted.content_hash {
            continue;
        }
        let Some(other) = hunk_edited_spans_of(&candidate) else {
            continue;
        };
        if spans_overlap(&spans, &other) {
            // A candidate the reviewer withdrew (or the owner decided) between
            // the read above and this write is simply no longer competing;
            // the acceptance already wrote the owner's file and must commit.
            let Some(updated) =
                set_suggestion_status_if_open(db, candidate.id, "obsolete", by, None).await?
            else {
                continue;
            };
            if let Some(finding_id) = updated.finding_id {
                settle_finding_status(db, finding_id).await?;
            }
            retired.push(updated);
        }
    }
    Ok(retired)
}

/// Compare captured edit coordinates only within the same raw source identity.
/// Sharing a line is not competition, and children of one publication parent
/// never retire one another (ordered paragraph insertions may share a point).
pub async fn obsolete_competing_edits(
    db: &mut ScopedDb,
    source_workspace_id: Uuid,
    accepted: &ReviewSuggestionRecord,
    selected: &[Uuid],
    by: Uuid,
) -> Result<Vec<ReviewSuggestionRecord>, G5Error> {
    let (Some(repo_id), Some(path), Some(hash)) = (
        accepted.repo_id,
        accepted.path.as_deref(),
        accepted.content_hash.as_deref(),
    ) else {
        return Ok(Vec::new());
    };
    fn coordinates(
        row: &ReviewSuggestionRecord,
        edit: &ReviewSuggestionEdit,
    ) -> Option<((usize, usize), (usize, usize))> {
        let text = row.before_lines.as_deref()?;
        let anchor = usize::try_from(row.anchor_start?).ok()?;
        let point = |offset: usize| {
            let prefix = text.get(..offset)?;
            Some((
                anchor + prefix.bytes().filter(|byte| *byte == b'\n').count(),
                prefix.rsplit('\n').next()?.len(),
            ))
        };
        Some((point(edit.start)?, point(edit.end)?))
    }
    let touched: Vec<_> = accepted
        .edits
        .iter()
        .filter(|e| selected.contains(&e.id))
        .filter_map(|e| coordinates(accepted, e))
        .collect();
    let mut candidates = list_suggestions(
        db,
        source_workspace_id,
        &SuggestionFilter {
            repo_id: Some(repo_id),
            path: Some(path.to_string()),
            open_only: true,
            ..Default::default()
        },
    )
    .await?;
    let mut retired = Vec::new();
    for candidate in &mut candidates {
        if candidate.id == accepted.id
            || candidate.kind != "hunk"
            || candidate.content_hash.as_deref() != Some(hash)
        {
            continue;
        }
        let mut ids = Vec::new();
        for edit in &candidate.edits {
            if edit.status != "open" {
                continue;
            }
            let Some((b0, b1)) = coordinates(candidate, edit) else {
                continue;
            };
            if touched.iter().any(|(a0, a1)| match (a0 == a1, b0 == b1) {
                (true, true) => *a0 == b0,
                (true, false) => b0 < *a0 && *a0 < b1,
                (false, true) => *a0 < b0 && b0 < *a1,
                (false, false) => *a0 < b1 && b0 < *a1,
            }) {
                ids.push(edit.id);
            }
        }
        if ids.is_empty() {
            continue;
        }
        let (_, publication) = get_suggestion(db, source_workspace_id, candidate.id).await?;
        for id in &ids {
            let thread =
                ensure_edit_thread(db, source_workspace_id, candidate, &publication, *id).await?;
            add_thread_reply(db, thread, by,
                "Obsolete: a competing edit of the same captured source was accepted. Independent edits remain open.").await?;
        }
        for edit in &mut candidate.edits {
            if ids.contains(&edit.id) {
                edit.status = "obsolete".into();
                edit.status_by = Some(by);
                edit.status_at = Some(Utc::now());
            }
        }
        let updated = save_suggestion_edits(db, source_workspace_id, candidate, by).await?;
        if let Some(id) = updated.finding_id {
            settle_finding_status(db, id).await?;
        }
        retired.push(updated);
    }
    Ok(retired)
}

/// Settle a finding's status from its suggestions: accepted once every
/// non-superseded suggestion is accepted, rejected once none is open and at
/// least one was rejected.
pub async fn settle_finding_status(db: &mut ScopedDb, finding_id: Uuid) -> Result<(), G5Error> {
    // Serialize settlement on the finding: two decisions on sibling
    // suggestions each hold only their own suggestion row, so without this
    // both could aggregate a snapshot in which the other's row is still
    // open. The UPDATE below runs after the lock with a fresh snapshot.
    sqlx::query("SELECT id FROM review_findings WHERE id = $1 FOR UPDATE")
        .bind(finding_id)
        .execute(db.conn())
        .await?;
    // Only the actionable children (hunks and new files) settle a finding.
    // An artifact attached to it is supporting material with no accept:
    // an open one must not keep the finding open once its fixes are in,
    // and a withdrawn one says nothing about the fixes.
    sqlx::query(
        "UPDATE review_findings f SET status = CASE
             WHEN NOT EXISTS (SELECT 1 FROM review_suggestions s WHERE s.finding_id = f.id AND s.kind <> 'artifact' AND s.status = 'open')
                  AND EXISTS (SELECT 1 FROM review_suggestions s WHERE s.finding_id = f.id AND s.kind <> 'artifact' AND (s.status = 'accepted' OR (s.status = 'partial' AND EXISTS (SELECT 1 FROM jsonb_array_elements(s.edits) e WHERE e->>'status' = 'accepted'))))
                  AND NOT EXISTS (SELECT 1 FROM review_suggestions s WHERE s.finding_id = f.id AND s.kind <> 'artifact' AND (s.status = 'rejected' OR (s.status = 'partial' AND EXISTS (SELECT 1 FROM jsonb_array_elements(s.edits) e WHERE e->>'status' = 'rejected'))))
                 THEN 'accepted'
             WHEN NOT EXISTS (SELECT 1 FROM review_suggestions s WHERE s.finding_id = f.id AND s.kind <> 'artifact' AND s.status = 'open')
                  AND EXISTS (SELECT 1 FROM review_suggestions s WHERE s.finding_id = f.id AND s.kind <> 'artifact' AND (s.status = 'rejected' OR (s.status = 'partial' AND EXISTS (SELECT 1 FROM jsonb_array_elements(s.edits) e WHERE e->>'status' = 'rejected'))))
                 THEN 'rejected'
             ELSE f.status END
         WHERE f.id = $1",
    )
    .bind(finding_id)
    .execute(db.conn())
    .await?;
    Ok(())
}

/// Everyone who reviews `source_workspace_id`: reviewers with a live review
/// workspace plus authors of its publications (comments-only reviewers).
pub async fn reviewer_user_ids(
    db: &mut ScopedDb,
    org_id: Uuid,
    source_workspace_id: Uuid,
) -> Result<Vec<Uuid>, G5Error> {
    let mut ids: Vec<Uuid> = list_reviews_of_source(db, org_id, source_workspace_id)
        .await?
        .into_iter()
        .map(|review| review.reviewer_user_id)
        .collect();
    let authors: Vec<Uuid> = sqlx::query_scalar(
        "SELECT DISTINCT reviewer_user_id FROM review_publications WHERE source_workspace_id = $1",
    )
    .bind(source_workspace_id)
    .fetch_all(db.conn())
    .await?;
    for id in authors {
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    Ok(ids)
}

// ─── Applying a hunk to the owner's file ─────────────────────────────────────

/// Outcome of applying a hunk to the owner's current file content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HunkApplication {
    /// The hunk applied; here is the new content.
    Applied(String),
    /// The file already contains the proposed text where the hunk points.
    AlreadyApplied,
}

/// Why a hunk could not be applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HunkRefusal {
    /// The lines the hunk replaces are not in the file (changed or removed).
    TargetNotFound,
    /// The lines the hunk replaces appear more than once and the anchor
    /// does not disambiguate them.
    Ambiguous,
}

/// Lines of a hunk block. The stored form ends every line with `\n`, so
/// `""` is no line at all while `"\n"` is one empty line; a block without a
/// final newline (an owner's edit box) is read the same way.
fn split_lines(text: &str) -> Vec<&str> {
    if text.is_empty() {
        return Vec::new();
    }
    let body = text.strip_suffix('\n').unwrap_or(text);
    body.split('\n').collect()
}

/// The stored form of a hunk block: every line newline-terminated, so one
/// empty line (`"\n"`) differs from no line (`""`).
fn join_lines(lines: &[&str]) -> String {
    let mut out = lines.join("\n");
    if !lines.is_empty() {
        out.push('\n');
    }
    out
}

/// Every start index at which `block` occurs in `lines` (empty for an empty
/// block).
fn all_positions(lines: &[&str], block: &[&str]) -> Vec<usize> {
    if block.is_empty() || block.len() > lines.len() {
        return Vec::new();
    }
    (0..=lines.len() - block.len())
        .filter(|start| lines[*start..*start + block.len()] == *block)
        .collect()
}

/// The context lines a hunk was published with (`anchor_context` JSON:
/// `{"version":2,"leading":[...],"trailing":[...]}`), used to tell apart
/// several occurrences of the same replaced block after the file moved.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct AnchorContext {
    pub leading: Vec<String>,
    pub trailing: Vec<String>,
}

impl AnchorContext {
    pub fn parse(raw: Option<&str>) -> Self {
        let Some(raw) = raw else {
            return Self::default();
        };
        let Ok(value) = serde_json::from_str::<Value>(raw) else {
            return Self::default();
        };
        // Version 1 carried the hunk's own edge rows, which frame every
        // occurrence of the block and, read as outside lines, could single
        // out the wrong one; only version 2 (lines outside the block) is
        // trusted, an older context counts as none and duplicates refuse.
        if value.get("version").and_then(Value::as_u64) != Some(ANCHOR_CONTEXT_VERSION) {
            return Self::default();
        }
        let lines = |key: &str| -> Vec<String> {
            value
                .get(key)
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default()
        };
        Self {
            leading: lines("leading"),
            trailing: lines("trailing"),
        }
    }

    fn is_empty(&self) -> bool {
        self.leading.is_empty() && self.trailing.is_empty()
    }

    /// The stored `anchor_context` form.
    pub fn to_json(&self) -> String {
        json!({"version": ANCHOR_CONTEXT_VERSION, "leading": self.leading, "trailing": self.trailing})
            .to_string()
    }

    /// Whether `lines[start..start+len]` sits inside the published context:
    /// the leading lines end right before it and the trailing lines start
    /// right after it. The context is never part of the block itself (see
    /// `anchor_context_for`), so a block matching in place says nothing.
    fn surrounds(&self, lines: &[&str], start: usize, len: usize) -> bool {
        let leading_ok = self.leading.is_empty() || {
            let want: Vec<&str> = self.leading.iter().map(String::as_str).collect();
            start >= want.len() && lines[start - want.len()..start] == want[..]
        };
        let trailing_ok = self.trailing.is_empty() || {
            let want: Vec<&str> = self.trailing.iter().map(String::as_str).collect();
            let end = start + len;
            end + want.len() <= lines.len() && lines[end..end + want.len()] == want[..]
        };
        leading_ok && trailing_ok
    }
}

/// The context a hunk is published with: up to `ANCHOR_CONTEXT_LINES` base
/// lines before the replaced block (`anchor_start`, 1-based, `before_len`
/// lines long; a pure insertion has none) and after it. Lines of the block
/// itself are never context, so two occurrences of the same block are told
/// apart by what surrounds them.
pub fn anchor_context_for(
    base_lines: &[&str],
    anchor_start: Option<i32>,
    before_len: usize,
) -> AnchorContext {
    let Some(start) = anchor_start
        .filter(|start| *start >= 1)
        .map(|start| (start as usize - 1).min(base_lines.len()))
    else {
        return AnchorContext::default();
    };
    let end = (start + before_len).min(base_lines.len());
    let to_owned = |lines: &[&str]| lines.iter().map(|line| line.to_string()).collect();
    AnchorContext {
        leading: to_owned(&base_lines[start.saturating_sub(ANCHOR_CONTEXT_LINES)..start]),
        trailing: to_owned(&base_lines[end..(end + ANCHOR_CONTEXT_LINES).min(base_lines.len())]),
    }
}

/// Locate `block`. `anchor_trusted` says whether the file is still the
/// version the hunk was published against: then the published line wins
/// outright. Otherwise the line is only a hint, and a match there is taken
/// only when it is the sole occurrence or the published context confirms
/// it, so shifted duplicate blocks never receive someone else's change.
/// Several occurrences away from the anchor are told apart by the context
/// when it frames exactly one of them.
fn find_block_with_context(
    lines: &[&str],
    block: &[&str],
    preferred_start: Option<usize>,
    context: &AnchorContext,
    anchor_trusted: bool,
) -> Result<usize, HunkRefusal> {
    if block.is_empty() {
        // A pure insertion has only its line to go by; on a changed file
        // the published context must still frame that line.
        let start = preferred_start
            .filter(|start| *start <= lines.len())
            .ok_or(HunkRefusal::TargetNotFound)?;
        if anchor_trusted {
            return Ok(start);
        }
        if context.is_empty() {
            return Err(HunkRefusal::TargetNotFound);
        }
        let framed: Vec<_> = (0..=lines.len())
            .filter(|index| context.surrounds(lines, *index, 0))
            .collect();
        return match framed.as_slice() {
            [only] if *only == start => Ok(start),
            [_, _, ..] => Err(HunkRefusal::Ambiguous),
            _ => Err(HunkRefusal::TargetNotFound),
        };
    }
    let matches_at = |start: usize| {
        start + block.len() <= lines.len() && lines[start..start + block.len()] == *block
    };
    let positions = all_positions(lines, block);
    // Occurrences the published context frames. Context that frames more
    // than one occurrence tells nothing apart and confirms nothing.
    let framed: Vec<usize> = if context.is_empty() {
        Vec::new()
    } else {
        positions
            .iter()
            .copied()
            .filter(|start| context.surrounds(lines, *start, block.len()))
            .collect()
    };
    if let Some(start) = preferred_start {
        if matches_at(start)
            && (anchor_trusted || positions.len() == 1 || framed.as_slice() == [start])
        {
            return Ok(start);
        }
    }
    match positions.as_slice() {
        [] => Err(HunkRefusal::TargetNotFound),
        [only] => Ok(*only),
        _ => match framed.as_slice() {
            [only] => Ok(*only),
            _ => Err(HunkRefusal::Ambiguous),
        },
    }
}

/// Edit acceptance needs an exact target or a proven post-write frame. A
/// replacement found elsewhere after the original disappeared is not a receipt.
#[allow(clippy::too_many_arguments)]
pub fn apply_review_edit_with_context(
    content: &str,
    before_lines: &str,
    after_lines: &str,
    anchor_start: Option<i32>,
    context: &AnchorContext,
    anchor_trusted: bool,
    whole_file_parent: bool,
) -> Result<HunkApplication, HunkRefusal> {
    let lines = split_lines(content);
    let before = split_lines(before_lines);
    let after = split_lines(after_lines);
    let preferred = anchor_start.and_then(|line| usize::try_from(line - 1).ok());
    let raw_line_start = |line: usize| {
        lines[..line]
            .iter()
            .map(|row| row.len() + 1)
            .sum::<usize>()
            .min(content.len())
    };
    if !anchor_trusted && before != after {
        if whole_file_parent && content == after_lines {
            return Ok(HunkApplication::AlreadyApplied);
        }
        if !context.is_empty() {
            let candidates: Vec<usize> = if after.is_empty() {
                (0..=lines.len()).collect()
            } else {
                all_positions(&lines, &after)
            };
            let framed: Vec<_> = candidates
                .into_iter()
                .filter(|start| context.surrounds(&lines, *start, after.len()))
                .collect();
            if let [start] = framed.as_slice() {
                // A matching line frame is not a byte receipt when the final
                // newline differs. Compare the entire framed source window.
                if &content[raw_line_start(*start)..raw_line_start(*start + after.len())]
                    == after_lines
                {
                    return Ok(HunkApplication::AlreadyApplied);
                }
            }
        }
    }
    if !anchor_trusted && !before.is_empty() && !after.is_empty() {
        let replacements = all_positions(&lines, &after);
        let originals = all_positions(&lines, &before);
        if originals.iter().any(|original| {
            replacements.iter().any(|replacement| {
                *original >= *replacement && *original + before.len() <= *replacement + after.len()
            })
        }) {
            return Err(HunkRefusal::Ambiguous);
        }
    }
    let start = find_block_with_context(&lines, &before, preferred, context, anchor_trusted)?;
    // New captures retain actual EOF termination. Apply their exact bytes,
    // including an owner's edited replacement; never round-trip through lines.
    let raw_start = raw_line_start(start);
    let raw_end = raw_start + before_lines.len();
    if content.get(raw_start..raw_end) == Some(before_lines)
        && (!before_lines.is_empty()
            || raw_start == 0
            || content.as_bytes()[raw_start - 1] == b'\n')
        && (before_lines.is_empty() || before_lines.ends_with('\n') || raw_end == content.len())
    {
        return Ok(HunkApplication::Applied(format!(
            "{}{}{}",
            &content[..raw_start],
            after_lines,
            &content[raw_end..]
        )));
    }
    // Old captures normalized every final line. Preserve their established
    // application policy when that synthetic terminator is absent in the file.
    let mut output = Vec::new();
    output.extend_from_slice(&lines[..start]);
    output.extend_from_slice(&after);
    output.extend_from_slice(&lines[start + before.len()..]);
    let mut content_after = output.join("\n");
    if !output.is_empty() && (content.ends_with('\n') || lines.is_empty()) {
        content_after.push('\n');
    }
    Ok(HunkApplication::Applied(content_after))
}

/// Apply `before_lines` → `after_lines` to `content`, preferring the line
/// the hunk was published at (1-based) and otherwise the unique occurrence
/// of the replaced block. Trailing-newline handling: the file's own final
/// newline is preserved.
pub fn apply_hunk(
    content: &str,
    before_lines: &str,
    after_lines: &str,
    anchor_start: Option<i32>,
) -> Result<HunkApplication, HunkRefusal> {
    apply_hunk_with_trust(content, before_lines, after_lines, anchor_start, true)
}

/// [`apply_hunk`] with `anchor_trusted` false when the file is no longer
/// the version the hunk was published against (its stored content hash
/// differs from the live file): the published line is then a hint, not a
/// fact, and only an unambiguous match is applied.
pub fn apply_hunk_with_trust(
    content: &str,
    before_lines: &str,
    after_lines: &str,
    anchor_start: Option<i32>,
    anchor_trusted: bool,
) -> Result<HunkApplication, HunkRefusal> {
    apply_hunk_with_context(
        content,
        before_lines,
        after_lines,
        anchor_start,
        &AnchorContext::default(),
        anchor_trusted,
    )
}

/// [`apply_hunk_with_trust`] plus the published surrounding context, which
/// re-anchors a hunk whose replaced block now appears more than once.
pub fn apply_hunk_with_context(
    content: &str,
    before_lines: &str,
    after_lines: &str,
    anchor_start: Option<i32>,
    context: &AnchorContext,
    anchor_trusted: bool,
) -> Result<HunkApplication, HunkRefusal> {
    // `split_lines` strips exactly one terminal newline itself, so a file
    // ending with a blank line keeps that blank line as its last element.
    let had_trailing_newline = content.ends_with('\n');
    let lines = split_lines(content);
    let before = split_lines(before_lines);
    let after = split_lines(after_lines);
    let preferred = anchor_start.and_then(|line| usize::try_from(line - 1).ok());
    // A hunk that empties the file, on a file that is already empty: the
    // end state is reached (a retry after the write landed, or a
    // republished round), and nothing else could apply anyway.
    if after.is_empty() && !before.is_empty() && lines.is_empty() {
        return Ok(HunkApplication::AlreadyApplied);
    }
    let block_at = |start: usize, block: &[&str]| {
        start + block.len() <= lines.len() && lines[start..start + block.len()] == *block
    };
    if !after.is_empty() && before != after {
        // Already applied at the anchor: the proposed text sits exactly where
        // the hunk points. Checked before any relocation, so an identical
        // original block elsewhere in the file is never rewritten instead.
        // (An append keeps the original as a prefix of the replacement, so
        // the original being present there is not evidence against it.)
        // A shrinking hunk (deletion of trailing lines) leaves the
        // replacement as a prefix of the untouched original, so for those
        // the original must also be gone from the anchor.
        // Only a trusted anchor is evidence on its own: on a changed file the
        // owner may have added a block equal to the replacement right where
        // the stale line points while the original target still waits below,
        // so the whole-file check further down decides instead.
        if let Some(start) = preferred.filter(|_| anchor_trusted) {
            let applied_here = block_at(start, &after)
                && (after.len() >= before.len() || !block_at(start, &before));
            if applied_here {
                return Ok(HunkApplication::AlreadyApplied);
            }
        }
        // Already applied elsewhere (the file moved since): either the
        // original is gone and the replacement is present, or both occur
        // exactly once and the original sits inside the replacement (an
        // append, a prepend or a wrap keeps the original text inside the
        // new text, at the start, the end or the middle of it). Repeated
        // text is never proof: a block that recurs (identical lines, a
        // common snippet) still needs the anchor above or a unique match
        // below.
        // The containment reading is a heuristic for a file that moved; on
        // the published file with the original still at the anchor it is
        // overruled, or a hunk prepending a copy of the line above it (the
        // original then sits inside text that already occurs once) would be
        // recorded as accepted without writing anything.
        let original_at_anchor =
            anchor_trusted && preferred.is_some_and(|start| block_at(start, &before));
        let after_positions = all_positions(&lines, &after);
        let before_positions = all_positions(&lines, &before);
        let applied_elsewhere = !original_at_anchor
            && match (after_positions.as_slice(), before_positions.as_slice()) {
                ([_, ..], []) => true,
                ([only_after], [only_before]) => {
                    let offset = only_before.wrapping_sub(*only_after);
                    only_before >= only_after
                        && offset + before.len() <= after.len()
                        && after[offset..offset + before.len()] == before[..]
                }
                _ => false,
            };
        if applied_elsewhere {
            return Ok(HunkApplication::AlreadyApplied);
        }
        // On a changed file, the replacement sitting somewhere while the
        // original also remains is not enough to choose: either the owner
        // already applied it to one of two identical blocks (and the other
        // must stay), or they wrote the same text elsewhere on their own.
        // Refuse rather than rewrite a block that may be unrelated.
        if !anchor_trusted && !after_positions.is_empty() && !before_positions.is_empty() {
            return Err(HunkRefusal::Ambiguous);
        }
    }
    let start = find_block_with_context(&lines, &before, preferred, context, anchor_trusted)?;
    let mut out: Vec<&str> = Vec::with_capacity(lines.len() + after.len());
    out.extend_from_slice(&lines[..start]);
    out.extend_from_slice(&after);
    out.extend_from_slice(&lines[start + before.len()..]);
    let mut joined = out.join("\n");
    // A hunk that removes every line empties the file; the original's final
    // newline is restored only while there is still a line to end. An empty
    // original has no termination of its own: the proposal's lines end with
    // a newline, as every stored block does.
    if !out.is_empty() && (had_trailing_newline || lines.is_empty()) {
        joined.push('\n');
    }
    Ok(HunkApplication::Applied(joined))
}

#[cfg(test)]
mod tests {
    #[test]
    fn edit_recovery_proofs_include_final_newline_bytes() {
        use super::*;
        for whole_file in [true, false] {
            let prefix = if whole_file { "" } else { "Header\n" };
            let context = if whole_file {
                AnchorContext::default()
            } else {
                AnchorContext {
                    leading: vec!["Header".into()],
                    trailing: vec![],
                }
            };
            let anchor = Some(if whole_file { 1 } else { 2 });
            for (before, after, near_match) in
                [("LGTM", " LGTM", " LGTM\n"), ("LGTM\n", " LGTM\n", " LGTM")]
            {
                assert_eq!(
                    apply_review_edit_with_context(
                        &format!("{prefix}{near_match}"),
                        before,
                        after,
                        anchor,
                        &context,
                        false,
                        whole_file
                    ),
                    Err(HunkRefusal::TargetNotFound)
                );
                assert_eq!(
                    apply_review_edit_with_context(
                        &format!("{prefix}{after}"),
                        before,
                        after,
                        anchor,
                        &context,
                        false,
                        whole_file
                    ),
                    Ok(HunkApplication::AlreadyApplied)
                );
            }
        }
    }

    #[test]
    fn whitespace_hunks_keep_eof_bytes_in_both_publication_and_edit_acceptance() {
        use super::*;
        for (before, after) in [
            ("LGTM", " LGTM"),
            ("LGTM", "LGTM\n\n"),
            ("LGTM\n\n", "LGTM"),
            ("LGTM\n", "LGTM\n\n"),
            ("LGTM\n\n", "LGTM\n"),
            ("", "\n"),
            ("\n", ""),
            ("LGTM\r\n", " LGTM\r\n"),
        ] {
            let hunks = file_suggestion_changes(
                Uuid::nil(),
                "notes.md",
                before.as_bytes(),
                after.as_bytes(),
            )
            .unwrap();
            assert_eq!(hunks.len(), 1, "{before:?} -> {after:?}");
            let hunk = &hunks[0];
            assert_eq!(hunk.before_lines.as_deref(), Some(before));
            assert_eq!(hunk.after_lines, after);
            assert_eq!(
                apply_review_edit_with_context(
                    before,
                    before,
                    after,
                    hunk.anchor_start,
                    &AnchorContext::parse(hunk.anchor_context.as_deref()),
                    true,
                    true
                ),
                Ok(HunkApplication::Applied(after.into())),
                "{before:?} -> {after:?}"
            );
        }
        // A final-newline-only publication keeps its existing explicit refusal.
        assert!(file_suggestion_changes(Uuid::nil(), "notes.md", b"LGTM", b"LGTM\n").is_err());
    }

    #[test]
    fn changed_source_repeated_insertion_context_is_not_a_location_proof() {
        use super::*;
        let lines = split_lines("A.\nB.\nA.\nB.\n");
        let context = AnchorContext {
            leading: vec!["A.".into()],
            trailing: vec!["B.".into()],
        };
        assert_eq!(
            find_block_with_context(&lines, &[], Some(1), &context, false),
            Err(HunkRefusal::Ambiguous)
        );
        assert_eq!(
            find_block_with_context(&lines, &[], Some(1), &context, true),
            Ok(1)
        );
        assert_eq!(
            find_block_with_context(&split_lines("A.\nB.\n"), &[], Some(1), &context, false),
            Ok(1)
        );
        assert_eq!(
            find_block_with_context(
                &split_lines("Intro.\nA.\nB.\n"),
                &[],
                Some(1),
                &context,
                false
            ),
            Err(HunkRefusal::TargetNotFound)
        );
    }

    use super::{
        apply_hunk, apply_hunk_with_trust, finding_for, finding_paths, review_fork_idempotency_key,
        review_workspace_name, HunkApplication, HunkRefusal, NewFinding, ReviewFindingRecord,
        MAX_WORKSPACE_NAME_BYTES, REVIEW_FORK_KEY_PREFIX,
    };
    use chrono::Utc;
    use serde_json::Value;
    use uuid::Uuid;

    #[test]
    fn identical_group_equivalence_requires_the_complete_unmodified_accepted_boundary() {
        let before = "Intro.\n\nQuiet.\n\nTail.\n";
        let after = "Intro.\n\nSame added block.\n\nSame added block.\n\nQuiet.\n\nTail.\n";
        let mut edits = super::review_edits::discrete_edits(Uuid::new_v4(), before, after);
        edits[0].status = "accepted".into();
        edits[0].accepted_after_text = Some(edits[0].after_text.clone());
        let mut row = super::ReviewSuggestionRecord {
            id: Uuid::new_v4(),
            publication_id: Uuid::new_v4(),
            finding_id: None,
            repo_id: Some(Uuid::nil()),
            path: Some("docs/test.md".into()),
            kind: "hunk".into(),
            before_lines: Some(before.into()),
            after_lines: Some(after.into()),
            anchor_start: Some(1),
            content_hash: None,
            anchor_context: None,
            author_kind: "human".into(),
            author_label: "test".into(),
            note: String::new(),
            comment_id: None,
            status: "open".into(),
            status_by: None,
            status_at: None,
            supersedes: None,
            adopted_from: None,
            created_at: Utc::now(),
            blob_oid: None,
            blob_sha256: None,
            bytes: None,
            content_type: None,
            edits,
            expected_before_lines: String::new(),
            reviewer_user_id: None,
        };
        super::review_edits::refresh_edit_projection(before, &mut row.edits).unwrap();
        let source = super::review_edits::compose_current(before, &row.edits).unwrap();
        let mut next = row.edits[1].clone();
        next.start = row.edits[0].current_start;
        next.end = next.start;
        let point = next.start;
        let locations = vec![(
            row.edits[1].current_start,
            row.edits[1].current_end,
            &row.edits[1],
        )];
        let matched = super::identical_insert_group_matches(
            &row,
            &source,
            0,
            &locations,
            0,
            &[next.clone()],
            point,
        );
        assert_eq!(
            matched.len(),
            1,
            "both identical-boundary insertions have the exact same complete source effect"
        );
        assert_eq!(matched[0].1.id, row.edits[1].id);
        assert!(
            super::identical_insert_group_matches(
                &row,
                &source,
                0,
                &locations,
                0,
                &[next.clone(), next.clone()],
                point
            )
            .is_empty(),
            "changed pending cardinality cannot prove a correspondence"
        );
        let outside = source.find("Tail.").unwrap();
        let mut moved = next.clone();
        moved.start = outside;
        moved.end = outside;
        assert!(
            super::identical_insert_group_matches(
                &row,
                &source,
                0,
                &locations,
                0,
                &[moved],
                outside
            )
            .is_empty(),
            "another source point cannot borrow this group's ordinal identity"
        );
        assert!(
            super::identical_insert_group_matches(
                &row,
                before,
                0,
                &locations,
                0,
                &[next.clone()],
                point
            )
            .is_empty(),
            "historically accepted text absent from restored bytes is no boundary witness"
        );

        // Construct the owner's fresh child ledger directly to exercise the
        // full-ledger guard independently of Git's current hunk segmentation.
        let mut reverse_row = row.clone();
        reverse_row.edits[0].status = "open".into();
        reverse_row.edits[0].accepted_after_text = None;
        reverse_row.edits[1].status = "accepted".into();
        reverse_row.edits[1].accepted_after_text = Some(reverse_row.edits[1].after_text.clone());
        super::review_edits::refresh_edit_projection(before, &mut reverse_row.edits).unwrap();
        reverse_row.expected_before_lines = source.clone();
        let mut insertion = reverse_row.edits[0].clone();
        insertion.id = Uuid::new_v4();
        insertion.start = source.find("Quiet.").unwrap();
        insertion.end = insertion.start;
        let generated = |fresh: &[super::ReviewSuggestionEdit]| super::GeneratedSuggestion {
            repo_id: Uuid::nil(),
            path: "docs/test.md".into(),
            kind: "hunk".into(),
            before_lines: Some(source.clone()),
            after_lines: super::review_edits::compose_proposed(&source, fresh).unwrap(),
            anchor_start: Some(1),
            content_hash: Some(format!(
                "{:x}",
                <super::Sha256 as super::Digest>::digest(source.as_bytes())
            )),
            anchor_context: None,
            blob_oid: None,
            blob_sha256: None,
            bytes: None,
            content_type: None,
        };
        let plain = std::slice::from_ref(&insertion);
        let matched = super::child_source_matches(&reverse_row, &generated(plain), &source, plain);
        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].1.edit.id, reverse_row.edits[0].id);
        assert_eq!(
            matched[0].1.capture_start, reverse_row.edits[0].current_start,
            "equivalent raw bytes must retain the original pending logical boundary"
        );
        assert_ne!(matched[0].1.capture_start, insertion.start);
        let mut canonical_change = insertion.clone();
        canonical_change.id = Uuid::new_v4();
        canonical_change.start = source.find("Same").unwrap();
        canonical_change.end = canonical_change.start + "Same".len();
        canonical_change.before_text = "Same".into();
        canonical_change.after_text = "Other".into();
        let changed = [canonical_change, insertion.clone()];
        assert!(
            super::child_source_matches(&reverse_row, &generated(&changed), &source, &changed)
                .is_empty(),
            "group-only equivalence must not carry a moved boundary when another delta changes the complete proposal"
        );
        let mut prefix_change = insertion.clone();
        prefix_change.id = Uuid::new_v4();
        prefix_change.start = 0;
        prefix_change.end = "Intro".len();
        prefix_change.before_text = "Intro".into();
        prefix_change.after_text = "Context".into();
        let disjoint = [prefix_change, insertion];
        let matched =
            super::child_source_matches(&reverse_row, &generated(&disjoint), &source, &disjoint);
        assert_eq!(
            matched.len(),
            1,
            "a disjoint new edit whose full proposal remains exact preserves the pending unit"
        );
        assert_eq!(matched[0].1.edit.id, reverse_row.edits[0].id);

        row.edits[0].accepted_after_text = Some(row.edits[0].after_text.repeat(2));
        super::review_edits::refresh_edit_projection(before, &mut row.edits).unwrap();
        let overridden = super::review_edits::compose_current(before, &row.edits).unwrap();
        let locations = vec![(
            row.edits[1].current_start,
            row.edits[1].current_end,
            &row.edits[1],
        )];
        assert!(super::identical_insert_group_matches(&row,&overridden,0,&locations,0,&[next],point).is_empty(),"an owner override cannot justify a moved-boundary fallback even when inserting another copy yields equal text");
    }

    #[test]
    fn names_are_git_valid_and_derived_from_reviewer() {
        assert_eq!(
            review_workspace_name("kaihui-test3", "Alex Chen", 1),
            "kaihui-test3-review-alex-chen"
        );
        assert_eq!(
            review_workspace_name("kaihui-test3", "Alex Chen", 2),
            "kaihui-test3-review-alex-chen-2"
        );
        assert_eq!(
            review_workspace_name("feature/x", "  ", 1),
            "feature/x-review-reviewer"
        );
        assert_eq!(
            review_workspace_name("s", "Élodie Ünal-Ög", 1),
            "s-review-lodie-nal-g"
        );
    }

    #[test]
    fn long_source_names_are_cut_to_a_valid_name_within_the_limit() {
        let long = format!("{}.lock", "a".repeat(300));
        let name = review_workspace_name(&long, "Bo Li", 3);
        assert!(name.len() <= MAX_WORKSPACE_NAME_BYTES);
        assert!(name.ends_with("-review-bo-li-3"));
        assert!(!name.contains(".lock-review"));
        let dotted = format!("{}...", "b".repeat(250));
        let name = review_workspace_name(&dotted, "x", 1);
        assert!(name.len() <= MAX_WORKSPACE_NAME_BYTES);
        assert!(!name.contains("..-review"));
    }

    #[test]
    fn idempotency_keys_carry_the_review_marker_generation_and_attempt() {
        let source = Uuid::nil();
        let key = review_fork_idempotency_key(source, 2, 3);
        assert!(key.starts_with(REVIEW_FORK_KEY_PREFIX));
        assert_eq!(key, format!("review-workspace:{source}:g2:3"));
    }

    #[test]
    fn apply_hunk_replaces_at_the_anchor_and_keeps_the_trailing_newline() {
        let content = "a\nb\nc\nd\n";
        let applied = apply_hunk(content, "b\nc", "b\nX\nc", Some(2)).unwrap();
        assert_eq!(
            applied,
            HunkApplication::Applied("a\nb\nX\nc\nd\n".to_string())
        );
        let moved = apply_hunk("z\na\nb\nc\nd\n", "b\nc", "b\nX\nc", Some(2)).unwrap();
        assert_eq!(
            moved,
            HunkApplication::Applied("z\na\nb\nX\nc\nd\n".to_string())
        );
    }

    #[test]
    fn apply_hunk_reports_already_applied_missing_and_ambiguous_targets() {
        assert_eq!(
            apply_hunk("a\nb\nX\nc\n", "b\nc", "b\nX\nc", Some(2)),
            Ok(HunkApplication::AlreadyApplied)
        );
        assert_eq!(
            apply_hunk("a\nq\n", "b\nc", "b\nX\nc", Some(2)),
            Err(HunkRefusal::TargetNotFound)
        );
        assert_eq!(
            apply_hunk("b\nc\nb\nc\n", "b\nc", "b\nX\nc", None),
            Err(HunkRefusal::Ambiguous)
        );
        // With an anchor that still matches, it wins even when the block
        // also appears elsewhere.
        assert_eq!(
            apply_hunk("b\nc\nz\nb\nc\n", "b\nc", "Q", Some(4)),
            Ok(HunkApplication::Applied("b\nc\nz\nQ\n".to_string()))
        );
        // With an anchor that no longer matches and several candidates, no
        // guess is made.
        assert_eq!(
            apply_hunk("x\nb\nc\nz\nb\nc\n", "b\nc", "Q", Some(9)),
            Err(HunkRefusal::Ambiguous)
        );
        // Applied at the anchor while an identical original block remains
        // elsewhere: reported as already applied, the other block untouched.
        assert_eq!(
            apply_hunk("a\nnew\nsep\na\nold\n", "a\nold", "a\nnew", Some(1)),
            Ok(HunkApplication::AlreadyApplied)
        );
        // The proposed text exists elsewhere but the original still sits at
        // the anchor (twice in the file): apply at the anchor, not "already
        // applied".
        assert_eq!(
            apply_hunk("old\nnew\nold\n", "old", "new", Some(1)),
            Ok(HunkApplication::Applied("new\nnew\nold\n".to_string()))
        );
        // An append is idempotent: applying it twice never duplicates.
        let once = apply_hunk("a\nb\nc\n", "a\nb\nc", "a\nb\nc\nnew", Some(1)).unwrap();
        assert_eq!(once, HunkApplication::Applied("a\nb\nc\nnew\n".to_string()));
        assert_eq!(
            apply_hunk("a\nb\nc\nnew\n", "a\nb\nc", "a\nb\nc\nnew", Some(1)),
            Ok(HunkApplication::AlreadyApplied)
        );
        // Even when the file moved so the anchor is stale.
        assert_eq!(
            apply_hunk("z\na\nb\nc\nnew\n", "a\nb\nc", "a\nb\nc\nnew", Some(1)),
            Ok(HunkApplication::AlreadyApplied)
        );
        // A prepend leaves the original as the tail of the replacement (the
        // file's hash changed, so the anchor is not trusted): a retry after
        // a partial failure reports done instead of refusing as ambiguous.
        assert_eq!(
            apply_hunk_with_trust(
                "header\na\nb\nc\n",
                "a\nb\nc",
                "header\na\nb\nc",
                Some(1),
                false
            ),
            Ok(HunkApplication::AlreadyApplied)
        );
        assert_eq!(
            apply_hunk_with_trust(
                "z\nheader\na\nb\nc\n",
                "a\nb\nc",
                "header\na\nb\nc",
                Some(1),
                false
            ),
            Ok(HunkApplication::AlreadyApplied)
        );
        // A wrap keeps the original in the middle.
        assert_eq!(
            apply_hunk_with_trust(
                "open\na\nb\nclose\n",
                "a\nb",
                "open\na\nb\nclose",
                Some(1),
                false
            ),
            Ok(HunkApplication::AlreadyApplied)
        );
        // On the published file (trusted anchor) with the original still at
        // the anchor, the hunk is applied even though the replacement text
        // already occurs once with the original inside it: prepending a copy
        // of the line above writes that copy.
        assert_eq!(
            apply_hunk("a\nb\nc\n", "b", "a\nb", Some(2)),
            Ok(HunkApplication::Applied("a\na\nb\nc\n".to_string()))
        );
        // The original lying outside the replacement is not "already
        // applied": both are present once, so an untrusted anchor refuses.
        assert_eq!(
            apply_hunk_with_trust(
                "header\nx\na\nb\nc\n",
                "a\nb\nc",
                "header\nx",
                Some(1),
                false
            ),
            Err(HunkRefusal::Ambiguous)
        );
        // A deletion of trailing lines is applied, not mistaken for done.
        assert_eq!(
            apply_hunk("a\nb\nc\nd\n", "a\nb\nc\nd", "a\nb\nc", Some(1)),
            Ok(HunkApplication::Applied("a\nb\nc\n".to_string()))
        );
        assert_eq!(
            apply_hunk("a\nb\nc\n", "a\nb\nc\nd", "a\nb\nc", Some(1)),
            Ok(HunkApplication::AlreadyApplied)
        );
    }

    #[test]
    fn competing_hunks_are_decided_by_their_edited_lines() {
        use super::{hunk_edited_spans, spans_overlap};
        let spans = |anchor: i32, before: &str, after: &str| {
            hunk_edited_spans(anchor, before, after).expect("exact diff")
        };
        // Edits to lines 5 and 9 with three lines of context each: hunks
        // 2..=8 and 6..=12 share only context and never compete.
        let first = spans(2, "a\nb\nc\nX\nd\ne\nf\n", "a\nb\nc\nY\nd\ne\nf\n");
        let second = spans(6, "d\ne\nf\nP\ng\nh\ni\n", "d\ne\nf\nQ\ng\nh\ni\n");
        assert_eq!(first, vec![(5, 6)]);
        assert_eq!(second, vec![(9, 10)]);
        assert!(!spans_overlap(&first, &second));
        // Two nearby edits folded into one hunk edit lines 1 and 7 only; a
        // proposal for the untouched line 4 does not compete with them.
        let folded = spans(1, "a\nb\nc\nd\ne\nf\ng\n", "A\nb\nc\nd\ne\nf\nG\n");
        assert_eq!(folded, vec![(1, 2), (7, 8)]);
        assert!(!spans_overlap(&folded, &spans(4, "d\n", "D\n")));
        assert!(spans_overlap(&folded, &spans(7, "g\n", "gg\n")));
        // A pure insertion is a point before the line it goes before: it
        // does not compete with a replacement of that line (the block stays
        // whole and re-anchors), but two insertions at one point do, and an
        // insertion strictly inside a replaced interval does.
        let insertion = spans(1, "one\ntwo\n", "added\none\ntwo\n");
        assert_eq!(insertion, vec![(1, 1)]);
        assert!(!spans_overlap(&insertion, &spans(1, "one\n", "ONE\n")));
        assert!(spans_overlap(
            &insertion,
            &spans(1, "one\ntwo\n", "other\none\ntwo\n")
        ));
        assert!(spans_overlap(
            &spans(2, "b\n", "b\nNEW\n"),
            &spans(1, "a\nb\nc\n", "z\n")
        ));
        assert!(!spans_overlap(
            &spans(2, "b\n", "b\nNEW\n"),
            &spans(3, "c\n", "C\n")
        ));
        assert_eq!(spans(4, "c\nd\n", "c\nNEW\nd\n"), vec![(5, 5)]);
        // A hunk without context is its whole block; replacing two lines by
        // one is one span, and a replacement that grows keeps one interval.
        assert_eq!(spans(3, "x\ny\n", "z\n"), vec![(3, 5)]);
        assert_eq!(spans(3, "x\n", "y\nz\n"), vec![(3, 4)]);
        // Growing a block: the kept prefix is not edited, the tail is a
        // point after it.
        assert_eq!(spans(1, "a\nb\n", "a\nb\nc\n"), vec![(3, 3)]);
        // Beyond the exact-diff bound there are no spans: such a hunk never
        // retires or groups another proposal on an approximation.
        let huge = (0..1100)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(hunk_edited_spans(1, &huge, &format!("{huge}\nmore")), None);
    }

    #[test]
    fn findings_are_matched_by_repository_and_path() {
        fn finding(title: &str, repo_id: Option<Uuid>, paths: &[&str]) -> NewFinding {
            NewFinding {
                title: title.to_string(),
                severity: "P2".to_string(),
                dimension: String::new(),
                evidence: String::new(),
                actions: Value::Null,
                paths: paths.iter().map(|p| p.to_string()).collect(),
                repo_id,
            }
        }
        fn stored(id: Uuid) -> ReviewFindingRecord {
            ReviewFindingRecord {
                id,
                publication_id: Uuid::nil(),
                title: String::new(),
                severity: "P2".to_string(),
                dimension: String::new(),
                evidence: String::new(),
                actions: Value::Null,
                status: "open".to_string(),
                position: 0,
                created_at: Utc::now(),
                repo_id: None,
                paths: Value::Array(Vec::new()),
            }
        }
        let api = Uuid::new_v4();
        let web = Uuid::new_v4();
        let (api_finding, web_finding, any_finding) =
            (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        // Two repositories each changed `src/main.rs`; a finding per
        // repository keeps its own suggestions.
        let paths = finding_paths(
            &[
                finding("api greeting", Some(api), &["src/main.rs"]),
                finding("web greeting", Some(web), &["src/main.rs"]),
                finding("readme", None, &["README.md"]),
            ],
            &[
                stored(api_finding),
                stored(web_finding),
                stored(any_finding),
            ],
        );
        assert_eq!(finding_for(&paths, api, "src/main.rs"), Some(api_finding));
        assert_eq!(finding_for(&paths, web, "src/main.rs"), Some(web_finding));
        // A finding without a repository claims the path in every one.
        assert_eq!(finding_for(&paths, api, "README.md"), Some(any_finding));
        assert_eq!(finding_for(&paths, web, "README.md"), Some(any_finding));
        assert_eq!(finding_for(&paths, api, "src/lib.rs"), None);
        // The qualified finding wins over an unqualified one for its repository.
        let paths = finding_paths(
            &[
                finding("everywhere", None, &["src/main.rs"]),
                finding("api only", Some(api), &["src/main.rs"]),
            ],
            &[stored(any_finding), stored(api_finding)],
        );
        assert_eq!(finding_for(&paths, api, "src/main.rs"), Some(api_finding));
        assert_eq!(finding_for(&paths, web, "src/main.rs"), Some(any_finding));
    }

    #[test]
    fn apply_hunk_keeps_carriage_returns_and_fills_empty_files() {
        // CRLF lines round-trip: the hunk carries the `\r`, the live split keeps it.
        assert_eq!(
            apply_hunk("a\r\nb\r\nc\r\n", "b\r", "B\r", Some(2)),
            Ok(HunkApplication::Applied("a\r\nB\r\nc\r\n".to_string()))
        );
        // Insertion into an empty existing file anchors at line 1 and ends
        // its lines like any stored block; one blank line stays a line.
        assert_eq!(
            apply_hunk("", "", "hello\nworld", Some(1)),
            Ok(HunkApplication::Applied("hello\nworld\n".to_string()))
        );
        assert_eq!(
            apply_hunk("", "", "\n", Some(1)),
            Ok(HunkApplication::Applied("\n".to_string()))
        );
        assert_eq!(super::hunk_old_start("@@ -0,0 +1,2 @@"), Some(0));
        assert_eq!(
            super::hunk_old_start("@@ -17,3 +17,5 @@ fn main()"),
            Some(17)
        );
        assert_eq!(super::hunk_old_start("garbage"), None);
    }

    #[test]
    fn apply_hunk_distrusts_the_anchor_once_the_file_changed() {
        use super::apply_hunk_with_trust;
        // The owner inserted lines above, so the published line now points
        // at the *other* identical block: refused rather than misapplied.
        let shifted = "fn two() {\n    x\n}\nfn one() {\n    x\n}\n";
        assert_eq!(
            apply_hunk_with_trust(shifted, "    x", "    y", Some(5), false),
            Err(HunkRefusal::Ambiguous)
        );
        // Same file version: the line is authoritative.
        assert_eq!(
            apply_hunk_with_trust(shifted, "    x", "    y", Some(5), true),
            Ok(HunkApplication::Applied(
                "fn two() {\n    x\n}\nfn one() {\n    y\n}\n".to_string()
            ))
        );
        // Changed file but a unique block: applied wherever it moved to.
        assert_eq!(
            apply_hunk_with_trust("z\na\nb\nc\n", "b", "B", Some(2), false),
            Ok(HunkApplication::Applied("z\na\nB\nc\n".to_string()))
        );
        // A pure insertion on a changed file has nothing to anchor on.
        assert_eq!(
            apply_hunk_with_trust("a\nc\n", "", "b", Some(2), false),
            Err(HunkRefusal::TargetNotFound)
        );
        // The owner prepended a block equal to the replacement where the
        // stale line now points, while the original target still waits
        // below: neither "already applied" nor safe to guess (the same
        // shape arises when one of two identical blocks was already
        // changed and the other must stay), so it is refused.
        assert_eq!(
            apply_hunk_with_trust("new\nz\nold\n", "old", "new", Some(1), false),
            Err(HunkRefusal::Ambiguous)
        );
        // Two identical blocks, one already changed: the other is left alone.
        assert_eq!(
            apply_hunk_with_trust("b\nX\nc\nz\nb\nc\n", "b\nc", "b\nX\nc", Some(1), false),
            Err(HunkRefusal::Ambiguous)
        );
    }

    #[test]
    fn apply_hunk_keeps_repeated_text_honest() {
        // Appending one more identical line: the original three lines recur
        // inside the four-line replacement at every offset, which must not
        // read as "already applied" — the trusted anchor applies it.
        let three = "x\nx\nx\n";
        assert_eq!(
            apply_hunk(three, "x\nx\nx\n", "x\nx\nx\nx\n", Some(1)),
            Ok(HunkApplication::Applied("x\nx\nx\nx\n".to_string()))
        );
        // ...and once applied, the same hunk is recognised as done.
        assert_eq!(
            apply_hunk("x\nx\nx\nx\n", "x\nx\nx\n", "x\nx\nx\nx\n", Some(1)),
            Ok(HunkApplication::AlreadyApplied)
        );
        // One blank line is a line: replacing `hello` with an empty line.
        assert_eq!(
            apply_hunk("hello\n", "hello\n", "\n", Some(1)),
            Ok(HunkApplication::Applied("\n".to_string()))
        );
        assert_eq!(super::join_lines(&[]), "");
        assert_eq!(super::join_lines(&[""]), "\n");
        assert_eq!(super::split_lines("\n"), vec![""]);
        assert!(super::split_lines("").is_empty());
    }

    #[test]
    fn apply_hunk_keeps_a_trailing_blank_line() {
        // A file ending with a blank line: a hunk elsewhere leaves it, and
        // a hunk covering it can remove it.
        assert_eq!(
            apply_hunk("a\nb\n\n", "b\n", "B\n", Some(2)),
            Ok(HunkApplication::Applied("a\nB\n\n".to_string()))
        );
        assert_eq!(
            apply_hunk("a\n\n", "a\n\n", "a\n", Some(1)),
            Ok(HunkApplication::Applied("a\n".to_string()))
        );
    }

    #[test]
    fn apply_hunk_can_empty_a_file() {
        assert_eq!(
            apply_hunk("hello\n", "hello", "", Some(1)),
            Ok(HunkApplication::Applied(String::new()))
        );
        assert_eq!(
            apply_hunk("a\nb\n", "a\nb", "", Some(1)),
            Ok(HunkApplication::Applied(String::new()))
        );
        // ...and once the file is empty, the same hunk is done.
        assert_eq!(
            apply_hunk("", "a\nb", "", Some(1)),
            Ok(HunkApplication::AlreadyApplied)
        );
    }

    #[test]
    fn generated_context_lies_outside_the_block_and_tells_twins_apart() {
        use super::{anchor_context_for, apply_hunk_with_context, split_lines, AnchorContext};
        let base = "fn one() {\n    x\n}\nfn two() {\n    x\n}\n";
        let lines = split_lines(base);
        // The context of each twin is what surrounds it, never its own lines.
        let second = anchor_context_for(&lines, Some(5), 1);
        assert_eq!(second.leading, vec!["    x", "}", "fn two() {"]);
        assert_eq!(second.trailing, vec!["}"]);
        let first = anchor_context_for(&lines, Some(2), 1);
        assert_eq!(first.leading, vec!["fn one() {"]);
        assert_eq!(first.trailing, vec!["}", "fn two() {", "    x"]);
        // A pure insertion is framed by the lines it slips between.
        let insertion = anchor_context_for(&lines, Some(4), 0);
        assert_eq!(insertion.leading, vec!["fn one() {", "    x", "}"]);
        assert_eq!(insertion.trailing, vec!["fn two() {", "    x", "}"]);
        assert_eq!(
            anchor_context_for(&lines, None, 1),
            AnchorContext::default()
        );
        // The file grew at the top (hash changed, anchor untrusted): each
        // published context still frames exactly its own twin.
        let shifted = "// new\nfn one() {\n    x\n}\nfn two() {\n    x\n}\n";
        assert_eq!(
            apply_hunk_with_context(shifted, "    x", "    y", Some(5), &second, false),
            Ok(HunkApplication::Applied(
                "// new\nfn one() {\n    x\n}\nfn two() {\n    y\n}\n".to_string()
            ))
        );
        assert_eq!(
            apply_hunk_with_context(shifted, "    x", "    y", Some(2), &first, false),
            Ok(HunkApplication::Applied(
                "// new\nfn one() {\n    y\n}\nfn two() {\n    x\n}\n".to_string()
            ))
        );
        // A context made of the block's own edge lines (what the diff's
        // context rows would give) frames no occurrence, so twins stay
        // ambiguous instead of both counting as framed.
        let inside = AnchorContext::parse(Some(
            &AnchorContext {
                leading: vec!["    x".to_string()],
                trailing: Vec::new(),
            }
            .to_json(),
        ));
        assert_eq!(
            apply_hunk_with_context(shifted, "    x", "    y", Some(2), &inside, false),
            Err(HunkRefusal::Ambiguous)
        );
        assert_eq!(
            second.to_json(),
            r#"{"leading":["    x","}","fn two() {"],"trailing":["}"],"version":2}"#
        );
    }

    #[test]
    fn apply_hunk_re_anchors_with_the_published_context() {
        use super::{apply_hunk_with_context, AnchorContext};
        let context = AnchorContext::parse(Some(
            r#"{"version":2,"leading":["fn two() {"],"trailing":["}"]}"#,
        ));
        // The replaced block appears twice; only one occurrence sits under
        // the published leading line.
        let content = "fn one() {\n    x\n}\nfn two() {\n    x\n}\n";
        assert_eq!(
            apply_hunk_with_context(content, "    x", "    y", Some(40), &context, true),
            Ok(HunkApplication::Applied(
                "fn one() {\n    x\n}\nfn two() {\n    y\n}\n".to_string()
            ))
        );
        // Context that matches both occurrences still refuses.
        let vague = AnchorContext::parse(Some(r#"{"version":2,"leading":[],"trailing":["}"]}"#));
        // A version-1 context (the block's own edge rows) is not trusted.
        assert_eq!(
            AnchorContext::parse(Some(
                r#"{"version":1,"leading":["fn two() {"],"trailing":["}"]}"#
            )),
            AnchorContext::default()
        );
        assert_eq!(
            apply_hunk_with_context(content, "    x", "    y", Some(40), &vague, true),
            Err(HunkRefusal::Ambiguous)
        );
        // Untrusted anchor pointing at the wrong twin: the context decides.
        let shifted = "fn two() {\n    x\n}\nfn one() {\n    x\n}\n";
        assert_eq!(
            apply_hunk_with_context(shifted, "    x", "    y", Some(5), &vague, false),
            Err(HunkRefusal::Ambiguous)
        );
        assert_eq!(
            apply_hunk_with_context(shifted, "    x", "    y", Some(5), &context, false),
            Ok(HunkApplication::Applied(
                "fn two() {\n    y\n}\nfn one() {\n    x\n}\n".to_string()
            ))
        );
        assert_eq!(
            AnchorContext::parse(Some("not json")),
            AnchorContext::default()
        );
    }

    #[test]
    fn apply_hunk_can_delete_and_insert() {
        assert_eq!(
            apply_hunk("a\nb\nc", "b", "", Some(2)),
            Ok(HunkApplication::Applied("a\nc".to_string()))
        );
        assert_eq!(
            apply_hunk("a\nc\n", "", "b", Some(2)),
            Ok(HunkApplication::Applied("a\nb\nc\n".to_string()))
        );
    }

    fn refusal_message(error: g5_contracts::error::G5Error) -> (String, String) {
        match error {
            g5_contracts::error::G5Error::ValidationError { details } => {
                assert_eq!(details["code"], "FILE_SUGGESTION_REFUSED", "{details}");
                (
                    details["field"].as_str().unwrap_or_default().to_string(),
                    details["message"].as_str().unwrap_or_default().to_string(),
                )
            }
            other => panic!("expected a validation refusal, got {other:?}"),
        }
    }

    /// Commit `content` as `path` without index filters (no autocrlf).
    fn commit_raw(repo: &git2::Repository, parent: Option<git2::Oid>, content: &str) -> git2::Oid {
        let blob = repo.blob(content.as_bytes()).unwrap();
        let mut builder = repo.treebuilder(None).unwrap();
        builder.insert("notes.md", blob, 0o100644).unwrap();
        let tree = repo.find_tree(builder.write().unwrap()).unwrap();
        let signature = git2::Signature::now("Test", "test@example.com").unwrap();
        let parents: Vec<git2::Commit<'_>> = parent
            .map(|oid| repo.find_commit(oid).unwrap())
            .into_iter()
            .collect();
        let parents: Vec<&git2::Commit<'_>> = parents.iter().collect();
        repo.commit(None, &signature, &signature, "raw", &tree, &parents)
            .unwrap()
    }

    #[test]
    fn file_suggestions_are_the_hunks_a_round_cuts_from_the_same_versions() {
        use sha2::Digest as _;
        let base = "# Notes\r\n\r\nalpha\r\nbeta\r\ngamma\r\ndelta\r\nepsilon\r\nzeta\r\neta\r\n\
                    theta\r\niota\r\nkappa\r\n";
        let proposed =
            "# Notes\r\n\r\nalpha\r\nBETA\r\ngamma\r\ndelta\r\nepsilon\r\nzeta\r\neta\r\n\
                        theta\r\niota\r\nkappa\r\nlambda\r\n";
        let dir = tempfile::tempdir().unwrap();
        let git = git2::Repository::init(dir.path()).unwrap();
        let old = commit_raw(&git, None, base);
        let new = commit_raw(&git, Some(old), proposed);
        let round = g5_scm::GitRepo::open(dir.path())
            .unwrap()
            .file_diffs_between_commits(&old.to_string(), &new.to_string())
            .unwrap()
            .remove(0);
        let repo_id = Uuid::new_v4();

        let changes = super::file_suggestion_changes(
            repo_id,
            "notes.md",
            base.as_bytes(),
            proposed.as_bytes(),
        )
        .unwrap();

        assert_eq!(
            serde_json::to_value(&changes).unwrap(),
            serde_json::to_value(super::hunk_suggestions(
                repo_id,
                "notes.md",
                base.as_bytes(),
                proposed.as_bytes(),
                &round
            ))
            .unwrap(),
            "a FILES push proposes exactly what a round proposes for the same two versions"
        );
        assert_eq!(changes.len(), 2, "{changes:?}");
        let first = &changes[0];
        assert_eq!(first.kind, "hunk");
        assert_eq!(first.anchor_start, Some(1));
        assert_eq!(
            first.before_lines.as_deref(),
            Some("# Notes\r\n\r\nalpha\r\nbeta\r\ngamma\r\ndelta\r\nepsilon\r\n"),
            "carriage returns are kept"
        );
        assert_eq!(
            first.after_lines,
            "# Notes\r\n\r\nalpha\r\nBETA\r\ngamma\r\ndelta\r\nepsilon\r\n"
        );
        let digest = format!("{:x}", sha2::Sha256::digest(base.as_bytes()));
        assert_eq!(first.content_hash.as_deref(), Some(digest.as_str()));
        assert_eq!(
            first.anchor_context.as_deref(),
            Some(r#"{"leading":[],"trailing":["zeta\r","eta\r","theta\r"],"version":2}"#),
            "the published context is the base lines outside the block"
        );
        let second = &changes[1];
        assert_eq!(second.anchor_start, Some(10));
        assert_eq!(
            second.before_lines.as_deref(),
            Some("theta\r\niota\r\nkappa\r\n")
        );
        assert_eq!(second.after_lines, "theta\r\niota\r\nkappa\r\nlambda\r\n");
        assert_eq!(
            second.anchor_context.as_deref(),
            Some(r#"{"leading":["epsilon\r","zeta\r","eta\r"],"trailing":[],"version":2}"#)
        );
    }

    #[test]
    fn a_file_suggestion_into_an_empty_file_is_a_pure_insertion_after_line_zero() {
        let changes =
            super::file_suggestion_changes(Uuid::new_v4(), "empty.md", b"", b"first\nsecond\n")
                .unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].before_lines.as_deref(), Some(""));
        assert_eq!(changes[0].after_lines, "first\nsecond\n");
        assert_eq!(
            changes[0].anchor_start,
            Some(1),
            "`@@ -0,0` goes after line 0 of the base"
        );
        assert_eq!(
            changes[0].anchor_context.as_deref(),
            Some(r#"{"leading":[],"trailing":[],"version":2}"#)
        );
    }

    #[test]
    fn file_suggestions_refuse_no_change_and_a_final_newline_change() {
        for (base, proposed) in [
            ("same\n", "same\n"),
            ("", ""),
            ("line\n", "line"),
            ("line", "line\n"),
            ("one\ntwo", "one\ntwo\n"),
        ] {
            let error = super::file_suggestion_changes(
                Uuid::new_v4(),
                "a.md",
                base.as_bytes(),
                proposed.as_bytes(),
            )
            .expect_err("nothing to propose");
            assert_eq!(
                refusal_message(error),
                ("content".to_string(), "No changes to suggest".to_string()),
                "{base:?} -> {proposed:?}"
            );
        }
    }

    #[test]
    fn file_suggestions_refuse_a_change_past_the_review_diff_limits() {
        let base = "line\n".repeat(10);
        let wide = format!(
            "{}\n{}",
            "x".repeat(g5_scm::repo::MAX_COMMIT_FILE_DIFF_BYTES),
            base
        );
        let long = format!(
            "{}{base}",
            "new\n".repeat(g5_scm::repo::MAX_COMMIT_FILE_DIFF_LINES)
        );
        for proposed in [wide, long] {
            let error = super::file_suggestion_changes(
                Uuid::new_v4(),
                "a.md",
                base.as_bytes(),
                proposed.as_bytes(),
            )
            .expect_err("too large");
            assert_eq!(
                refusal_message(error),
                (
                    "content".to_string(),
                    "This change is too large to suggest".to_string()
                )
            );
        }
    }

    #[test]
    fn file_suggestions_refuse_texts_that_are_not_text() {
        let big = "x".repeat(super::MAX_FILE_SUGGESTION_TEXT_BYTES + 1);
        let cases: [(&[u8], &[u8], &str, &str); 5] = [
            (
                b"ok\n",
                b"bad \xff byte\n",
                "content",
                "Your edited text is not valid UTF-8 text",
            ),
            (
                b"bad \xc3\x28\n",
                b"ok\n",
                "baseContent",
                "The text you started from is not valid UTF-8 text",
            ),
            (
                b"ok\n",
                b"nul \0 byte\n",
                "content",
                "Your edited text contains a NUL byte, so it is not text",
            ),
            (
                b"nul \0 byte\n",
                b"ok\n",
                "baseContent",
                "The text you started from contains a NUL byte, so it is not text",
            ),
            (
                b"ok\n",
                big.as_bytes(),
                "content",
                "Your edited text is larger than 2 MiB",
            ),
        ];
        for (base, proposed, field, message) in cases {
            let error = super::file_suggestion_changes(Uuid::new_v4(), "a.md", base, proposed)
                .expect_err("not text");
            assert_eq!(
                refusal_message(error),
                (field.to_string(), message.to_string())
            );
        }
        let limit = "y".repeat(super::MAX_FILE_SUGGESTION_TEXT_BYTES);
        assert!(
            super::ensure_suggestion_text("content", "Text", limit.as_bytes()).is_ok(),
            "exactly 2 MiB is still text"
        );
    }
}
