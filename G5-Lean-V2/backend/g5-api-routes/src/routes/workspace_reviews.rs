//! Review workspace and review suggestion routes.
//!
//! Mounted at `/api/v1/workspaces/:workspaceId/review`.
//!
//! Review workspace lifecycle (`:workspaceId` is the source unless noted):
//! - `GET /` — the caller's review of this workspace (`asSource`) and, when
//!   this workspace is itself a review workspace, its origin (`asReview`).
//! - `POST /start` — fork the workspace into the caller's review workspace
//!   (or reuse the existing one) and resume its VM. The fork's base branch is
//!   the source's branch, so the in-VM Update rebases onto the parent.
//! - `POST /publish` — called on the **review workspace**: Push review. Turns
//!   the review branch's diff against the source into a publication of
//!   suggestions (hunks and new files), findings and a verdict.
//!
//! Suggestions (`:workspaceId` is the source):
//! - `POST /file-suggestions` — a reader who does not own the source pushes
//!   their FILES edit of one text file as suggestions, without a review
//!   workspace or a VM.
//! - `GET /publications`, `GET /suggestions`, `GET /suggestions/counts`
//! - `POST /suggestions/:suggestionId/accept|reject|resolve|withdraw`
//! - `GET|POST /suggestions/:suggestionId/replies`
//!
//! Ownership stays the only write authority: accept writes the change into
//! the owner's running working tree, then commits and pushes that file;
//! reviewers only ever write review rows. See
//! `workspace_review_service`.
//!
use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    extract::{DefaultBodyLimit, Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::db::scoped::{ensure_workspace_repo_id, scoped_db_for_workspace};
use crate::errors::AppError;
use crate::middleware::{AuthContext, AuthenticatedUser};
use crate::routes::audited::AuditedRouter;
use crate::routes::workspaces::{authorize_workspace_action, workspace_vm_service};
use crate::routes::ResponseEnvelope;
use crate::services::branch_commit::LandedBranchCommit;
use crate::services::comment_service::CommentReplyDTO;
use crate::services::notification_service::CreateNotificationRequest;
use crate::services::review_source_notifications::ReviewSourceNotifier;
use crate::services::workspace_fork_service::{ForkSource, ForkWorkspaceRequest};
use crate::services::workspace_review_service::{
    self as reviews, GeneratedSuggestion, HunkApplication, HunkRefusal, NewFinding,
    ReviewFindingRecord, ReviewPublicationRecord, ReviewPublicationSummary, ReviewSuggestionRecord,
    SkippedFile, SuggestionAuthor, SuggestionFilter, SuggestionPathCount, WorkspaceReviewRecord,
    MAX_REVIEW_NAME_ATTEMPTS,
};
use crate::services::workspace_vm_service::{ResumeOutcome, StartKind};
use crate::services::AppState;
use crate::ws::events::NotificationAction;
use g5_auth::Scope;
use g5_contracts::error::G5Error;

// ─── Path and body types ─────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct WorkspacePathParams {
    #[serde(rename = "workspaceId")]
    pub workspace_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct SuggestionPathParams {
    #[serde(rename = "workspaceId")]
    pub workspace_id: Uuid,
    #[serde(rename = "suggestionId")]
    pub suggestion_id: Uuid,
}

/// A review workspace as the API reports it.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceReviewDTO {
    #[serde(flatten)]
    pub review: WorkspaceReviewRecord,
    pub reviewer_display_name: String,
}

/// `GET /api/v1/workspaces/:workspaceId/review`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceReviewContextDTO {
    /// The caller's review of this workspace, when they started one.
    pub as_source: Option<WorkspaceReviewDTO>,
    /// This workspace's review origin, when it is a review workspace.
    pub as_review: Option<WorkspaceReviewDTO>,
    /// Open suggestions on this workspace (as a source), for badges.
    pub open_suggestions: i64,
}

/// `POST /api/v1/workspaces/:workspaceId/review/start`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartReviewResponseDTO {
    pub review: WorkspaceReviewDTO,
    /// `true` when this call forked the review workspace; `false` when the
    /// caller's existing review workspace was reopened.
    pub created: bool,
    /// Outcome of resuming the review workspace's VM, when it was admitted.
    pub resume: Option<ResumeOutcome>,
    /// Why the VM was not resumed. The review workspace exists regardless;
    /// the reviewer can start it from the workspace page.
    pub resume_error: Option<String>,
}

/// `POST /api/v1/workspaces/:reviewWorkspaceId/review/publish`.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishReviewRequestDTO {
    /// Summary for the owner.
    #[serde(default)]
    pub summary: Option<String>,
    /// `approve`, `request_changes` or `comment_only`.
    #[serde(default)]
    pub verdict: Option<String>,
    /// Eight-dimension review scores, free-form JSON object.
    #[serde(default)]
    pub scores: Option<Value>,
    /// Per-path notes shown as the suggestion's opening message.
    #[serde(default)]
    pub notes: HashMap<String, String>,
    /// Findings this round; each lists the paths whose changes belong to it.
    #[serde(default)]
    pub findings: Vec<NewFinding>,
    /// Changed paths to leave out of this publication (kept in the review
    /// workspace, published later or never).
    #[serde(default)]
    pub excluded_paths: Vec<String>,
    /// Notes per change, keyed by the change key the preview lists; a
    /// change's own note outranks its path's note.
    #[serde(default)]
    pub change_notes: HashMap<String, String>,
    /// Change keys (from the preview) to leave out of this publication.
    #[serde(default)]
    pub excluded_changes: Vec<String>,
    /// Files (repository and path) to leave out of this publication: like
    /// `excluded_paths`, but one repository's file only.
    #[serde(default)]
    pub excluded_files: Vec<ExcludedFileDTO>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExcludedFileDTO {
    pub repo_id: Uuid,
    pub path: String,
}

/// `POST /api/v1/workspaces/:reviewWorkspaceId/review/preview`: what Push
/// review would publish, without publishing.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewPreviewDTO {
    pub files: Vec<ReviewPreviewFileDTO>,
    pub skipped: Vec<SkippedFile>,
    /// Per repository: `sourceHead` (the source branch's tip in the mirror)
    /// and `mergeBase` (where this copy branched from it). A merge base
    /// behind the source head means the source moved on: Update to review
    /// it.
    pub base_commit_vector: Vec<Value>,
    /// How the reviewer's current work was saved before this preview: in
    /// the running review workspace (`live`), or not at all because the
    /// workspace is stopped (`stopped`: what it saved before stopping is
    /// what a push sends).
    pub saved: ReviewSaveDTO,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewSaveDTO {
    /// `live` or `stopped`.
    pub workspace: String,
    /// Whether the save made a new commit (a clean tree makes none).
    pub committed: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewPreviewFileDTO {
    pub repo_id: Uuid,
    pub path: String,
    /// `hunk` (changed file), `new_file` or `artifact` (new file, default).
    pub kind: String,
    pub hunks: usize,
    pub bytes: Option<i64>,
    /// The changes the round would carry for this file: each hunk of a
    /// changed file, or the one new-file proposal. Artifacts carry none.
    pub changes: Vec<ReviewPreviewChangeDTO>,
    /// For a new file or an artifact: the row of the reviewer's current
    /// round that already carries it (see `ReviewPreviewChangeDTO`).
    pub published_in: Option<reviews::PublishedIn>,
    /// For a new file or an artifact: what a push would carry for it.
    pub carried: Option<reviews::Carried>,
    /// For a new file or an artifact: the key of the one change it is
    /// (`ReviewPreviewChangeDTO::key`), which Discard from my copy takes. A
    /// changed file has none; its hunks carry their own.
    pub key: Option<String>,
}

/// `POST /api/v1/workspaces/:reviewWorkspaceId/review/changes/discard`.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscardReviewChangesRequestDTO {
    /// Keys of previewed changes (`ReviewPreviewChangeDTO::key`, or a new
    /// file's / artifact's `ReviewPreviewFileDTO::key`) to take back out of
    /// the review copy's working tree.
    #[serde(default)]
    pub keys: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscardedChangeDTO {
    pub key: String,
    pub repo_id: Uuid,
    pub path: String,
    /// `hunk`, `new_file` or `artifact`.
    pub kind: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkippedDiscardDTO {
    pub key: String,
    pub reason: String,
}

/// What Discard from my copy did: the copy's working tree no longer differs
/// from the source at the discarded changes; nothing on the source or on any
/// suggestion row changes.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscardReviewChangesResponseDTO {
    pub discarded: Vec<DiscardedChangeDTO>,
    pub skipped: Vec<SkippedDiscardDTO>,
}

/// One change of a previewed round, as the reviewer's own review panel
/// shows it before it is pushed.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewPreviewChangeDTO {
    /// Names this change to `change_notes` / `excluded_changes` of publish.
    pub key: String,
    /// `hunk` or `new_file`.
    pub kind: String,
    /// First line of the replaced block in the source file (hunks).
    pub anchor_start: Option<i32>,
    pub before_lines: Option<String>,
    pub after_lines: String,
    pub edit_count: usize,
    /// Units a push proves already offered, even when other units in this
    /// Git hunk are new. This does not claim the whole hunk is published.
    pub carried_edit_count: usize,
    /// The proven carried units that still require an owner decision.
    pub carried_open_edit_count: usize,
    /// The row of the reviewer's current round (the latest one, while the
    /// source is still at the commit it reviewed) that already carries this
    /// change: the panel shows it as "In round N" and counts it as pushed.
    /// `None` for a change no current round has.
    pub published_in: Option<reviews::PublishedIn>,
    /// What a push would carry for this change whatever the source's
    /// commit: the row it continues and the note it inherits when the push
    /// names none. The panel seeds its note editor from it.
    pub carried: Option<reviews::Carried>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishReviewResponseDTO {
    pub publication: ReviewPublicationRecord,
    pub findings: Vec<ReviewFindingRecord>,
    pub suggestions: Vec<ReviewSuggestionRecord>,
    /// Changed files that could not become suggestions.
    pub skipped: Vec<SkippedFile>,
}

/// `POST /api/v1/workspaces/:workspaceId/review/file-suggestions`: a
/// reader's edit of one existing text file, pushed from FILES.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSuggestionRequestDTO {
    pub repo_id: Uuid,
    /// Repository-relative path of the file.
    pub path: String,
    /// The whole text the reader started from.
    pub base_content: String,
    /// The whole edited text.
    pub content: String,
    /// Opens the thread of each suggestion the push makes.
    #[serde(default)]
    pub note: Option<String>,
    /// Push even though the reader's current file is no longer the text
    /// they started from (after `FILE_VERSION_CONFLICT`).
    #[serde(default)]
    pub allow_stale_base: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSuggestionResponseDTO {
    /// The push's own annotation publication (no review workspace).
    pub publication: ReviewPublicationRecord,
    pub suggestions: Vec<ReviewSuggestionRecord>,
}

/// Longest note a FILES push accepts, in characters.
const MAX_FILE_SUGGESTION_NOTE_CHARS: usize = 10_000;

/// Request-body limit of a FILES push: both texts at their decoded limit and
/// JSON's worst-case escaping (a control character travels as the six bytes
/// `\u0001`), plus 64 KiB for the note, the path and the field names. Each
/// decoded text is still bounded on its own by the review service.
const FILE_SUGGESTION_BODY_LIMIT_BYTES: usize =
    2 * 6 * reviews::MAX_FILE_SUGGESTION_TEXT_BYTES + 64 * 1024;

/// `GET /api/v1/workspaces/:workspaceId/review/publications`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewPublicationsDTO {
    pub publications: Vec<ReviewPublicationSummary>,
    pub findings: Vec<ReviewFindingRecord>,
    pub reviewer_display_names: HashMap<Uuid, String>,
    /// The source's current commit per repository. A publication whose
    /// `baseCommitVector` names another `sourceHead` is outdated: its rows
    /// leave the file pages and stay on the Reviews page under their commit.
    pub source_heads: HashMap<Uuid, String>,
    /// When each current source head was committed (RFC 3339), keyed like
    /// `sourceHeads`. A decision made after this time postdates the source's
    /// last commit; a repository whose commit cannot be read is left out.
    pub source_head_times: HashMap<Uuid, chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SuggestionListQuery {
    pub publication_id: Option<Uuid>,
    pub finding_id: Option<Uuid>,
    pub repo_id: Option<Uuid>,
    pub path: Option<String>,
    #[serde(default)]
    pub open_only: bool,
    /// Only rows about the source's current commit (what a file page shows).
    #[serde(default)]
    pub current_only: bool,
    /// Only rows of this kind (`artifact` with `status=accepted`: the
    /// workspace's kept review artifacts).
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SuggestionListDTO {
    pub suggestions: Vec<ReviewSuggestionRecord>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SuggestionCountsDTO {
    pub paths: Vec<SuggestionPathCount>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptSuggestionRequestDTO {
    #[serde(default)]
    pub edit_id: Option<Uuid>,
    /// The owner's edited replacement text (edit, then accept).
    #[serde(default)]
    pub after_lines: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RejectSuggestionRequestDTO {
    #[serde(default)]
    pub edit_id: Option<Uuid>,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplyRequestDTO {
    pub content: String,
    #[serde(default)]
    pub edit_id: Option<Uuid>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SuggestionDTO {
    pub suggestion: ReviewSuggestionRecord,
    /// Present after accept: the file hash the owner's working tree now has.
    pub written_hash: Option<String>,
    /// Present when accept found the change already in the file.
    pub already_applied: bool,
    /// The file was applied, but its selected commit/push did not complete.
    pub checkpoint_pending: bool,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditThreadQuery {
    #[serde(default)]
    pub edit_id: Option<Uuid>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepliesDTO {
    pub replies: Vec<CommentReplyDTO>,
}

// ─── Router ──────────────────────────────────────────────────────────────────

/// Router for `/api/v1/workspaces/:workspaceId/review`.
pub fn workspace_review_router() -> Router<Arc<AppState>> {
    AuditedRouter::new()
        .audited_route("/", get(get_review_context), &[Scope::WorkspacesRead])
        .audited_route("/start", post(start_review), &[Scope::WorkspacesWrite])
        .audited_route("/publish", post(publish_review), &[Scope::WorkspacesWrite])
        // A reader's FILES push writes review rows only, like a reply.
        .audited_route(
            "/file-suggestions",
            post(publish_file_suggestions)
                .layer(DefaultBodyLimit::max(FILE_SUGGESTION_BODY_LIMIT_BYTES)),
            &[Scope::WorkspacesRead],
        )
        .audited_route(
            "/publications",
            get(list_publications),
            &[Scope::WorkspacesRead],
        )
        .audited_route(
            "/suggestions",
            get(list_suggestions),
            &[Scope::WorkspacesRead],
        )
        .audited_route(
            "/suggestions/counts",
            get(count_suggestions),
            &[Scope::WorkspacesRead],
        )
        .audited_route(
            "/suggestions/:suggestionId/accept",
            post(accept_suggestion),
            &[Scope::WorkspacesWrite],
        )
        .audited_route(
            "/suggestions/:suggestionId/reject",
            post(reject_suggestion),
            &[Scope::WorkspacesWrite],
        )
        .audited_route(
            "/suggestions/:suggestionId/resolve",
            post(resolve_suggestion),
            &[Scope::WorkspacesWrite],
        )
        .audited_route(
            "/suggestions/:suggestionId/withdraw",
            post(withdraw_suggestion),
            &[Scope::WorkspacesRead],
        )
        .audited_route(
            "/suggestions/:suggestionId/replies",
            get(list_replies).post(add_reply),
            &[Scope::WorkspacesRead],
        )
        // The preview saves the review copy first (commit, push), so it is
        // a write like publish and Changes actions.
        .audited_route("/preview", post(preview_review), &[Scope::WorkspacesWrite])
        // Discard from my copy saves the copy like the preview, then writes
        // its running working tree (the reviewer's own workspace).
        .audited_route(
            "/changes/discard",
            post(discard_review_changes),
            &[Scope::WorkspacesWrite],
        )
        .audited_route(
            "/suggestions/:suggestionId/artifact",
            get(download_review_artifact),
            &[Scope::WorkspacesRead],
        )
        .into_router()
}

// ─── Review workspace lifecycle ──────────────────────────────────────────────

/// The caller's review context for `:workspaceId`.
async fn get_review_context(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
) -> Result<Json<ResponseEnvelope<WorkspaceReviewContextDTO>>, AppError> {
    let mut db =
        scoped_db_for_workspace(&state, &auth, params.workspace_id, &["workspaces:read"]).await?;
    let org_id = db.primary_org_id().map_err(AppError::from)?.into_uuid();
    let as_source = match reviews::find_review_for_source(
        &mut db,
        org_id,
        params.workspace_id,
        user.user_uuid(),
    )
    .await?
    {
        Some(review) => Some(review_dto(&state, review).await?),
        None => None,
    };
    let as_review =
        match reviews::find_review_by_review_workspace(&mut db, org_id, params.workspace_id).await?
        {
            Some(review) => Some(review_dto(&state, review).await?),
            None => None,
        };
    let source_heads =
        reviews::source_heads(&mut db, &state.repo_storage, params.workspace_id).await?;
    let open_suggestions =
        reviews::count_open_suggestions_by_path(&mut db, params.workspace_id, &source_heads)
            .await?
            .iter()
            .map(|row| row.open_count)
            .sum();
    db.commit().await.map_err(AppError::from)?;

    Ok(Json(ResponseEnvelope::ok(WorkspaceReviewContextDTO {
        as_source,
        as_review,
        open_suggestions,
    })))
}

/// Start (or reopen) the caller's review of `:workspaceId`.
///
/// Read access to the source is the only requirement: the review workspace
/// is a durable reader fork, so the source owner's VM is never touched. The
/// fork happens once per `(source, reviewer)`; later calls reuse it.
async fn start_review(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
) -> Result<(StatusCode, Json<ResponseEnvelope<StartReviewResponseDTO>>), AppError> {
    let reviewer_id = user.user_uuid();
    let source_id = params.workspace_id;
    // Authorize the source first (read access is the only requirement to
    // start), releasing the connection before the admission lock is taken.
    let mut access_db =
        scoped_db_for_workspace(&state, &auth, source_id, &["workspaces:read"]).await?;
    let org_id = access_db
        .primary_org_id()
        .map_err(AppError::from)?
        .into_uuid();
    // A review workspace is not itself reviewable: nested reviews would fork
    // a fork whose deliverables cannot be attributed to a source owner.
    reviews::ensure_not_review_workspace(&mut access_db, org_id, source_id, "Start review").await?;
    access_db.rollback().await.map_err(AppError::from)?;
    // Serialize lookup and fork admission per (source, reviewer) before any
    // further pool connection is taken: overlapping Starts otherwise race
    // between "no usable review yet" and the fork completing, and a
    // name-collision abort in one of them would shift the generation the
    // other computes. The lock is released when `admission` drops, before
    // the VM is resumed.
    let admission =
        reviews::lock_review_admission(&state.system_pool, source_id, reviewer_id).await?;
    let mut db = scoped_db_for_workspace(&state, &auth, source_id, &["workspaces:read"]).await?;
    tracing::debug!(
        source_workspace_id = %source_id,
        reviewer_user_id = %reviewer_id,
        "Workspace endpoint called: start_review"
    );

    let existing = reviews::find_review_for_source(&mut db, org_id, source_id, reviewer_id).await?;
    let (review_workspace_id, created) = match existing {
        Some(review) => {
            db.commit().await.map_err(AppError::from)?;
            (review.review_workspace_id, false)
        }
        None => {
            let source_name = reviews::workspace_name(&mut db, org_id, source_id).await?;
            db.commit().await.map_err(AppError::from)?;
            let reviewer_display_name =
                reviews::reviewer_display_name(&state.system_pool, reviewer_id).await?;

            let review_workspace_id = fork_review_workspace(
                &state,
                &auth,
                org_id,
                source_id,
                reviewer_id,
                &source_name,
                &reviewer_display_name,
            )
            .await?;
            (review_workspace_id, true)
        }
    };
    drop(admission);

    let (resume, resume_error) =
        match resume_review_workspace(&state, &auth, review_workspace_id, reviewer_id).await {
            Ok(outcome) => (Some(outcome), None),
            Err(error) => {
                tracing::warn!(
                    review_workspace_id = %review_workspace_id,
                    error = %error.inner,
                    "Review workspace VM was not resumed"
                );
                (None, Some(error.inner.to_string()))
            }
        };

    let mut db =
        scoped_db_for_workspace(&state, &auth, review_workspace_id, &["workspaces:read"]).await?;
    let review = reviews::find_review_by_review_workspace(&mut db, org_id, review_workspace_id)
        .await?
        .ok_or_else(|| {
            AppError::from_error(G5Error::Internal {
                message: "review workspace vanished while starting the review".to_string(),
            })
        })?;
    db.commit().await.map_err(AppError::from)?;
    let dto = review_dto(&state, review).await?;

    tracing::info!(
        source_workspace_id = %source_id,
        review_workspace_id = %review_workspace_id,
        created,
        "Workspace endpoint succeeded: start_review"
    );
    let status = if created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((
        status,
        Json(ResponseEnvelope::ok(StartReviewResponseDTO {
            review: dto,
            created,
            resume,
            resume_error,
        })),
    ))
}

/// Push review: publish the review workspace's changes as suggestions.
///
/// Called on the review workspace by its owner (the reviewer), who must
/// still have read access to the source. Only pushed commits are published:
/// the diff is read from the branch refs in the mirrors.
async fn publish_review(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
    body: Option<Json<PublishReviewRequestDTO>>,
) -> Result<(StatusCode, Json<ResponseEnvelope<PublishReviewResponseDTO>>), AppError> {
    let reviewer_id = user.user_uuid();
    let review_workspace_id = params.workspace_id;
    let body = body.map(|Json(body)| body).unwrap_or_default();
    if let Some(verdict) = body.verdict.as_deref() {
        if !matches!(verdict, "approve" | "request_changes" | "comment_only") {
            return Err(AppError::from_error(G5Error::ValidationError {
                details: json!({"verdict": "must be approve, request_changes or comment_only"}),
            }));
        }
    }

    let mut db =
        scoped_db_for_workspace(&state, &auth, review_workspace_id, &["workspaces:write"]).await?;
    crate::services::workspace_mutation_policy::ensure_workspace_owner(
        &mut db,
        review_workspace_id,
    )
    .await?;
    let org_id = db.primary_org_id().map_err(AppError::from)?.into_uuid();
    let review = reviews::find_review_by_review_workspace(&mut db, org_id, review_workspace_id)
        .await?
        .ok_or_else(|| {
            AppError::from_error(G5Error::NotFound {
                resource: format!("review workspace {review_workspace_id}"),
            })
        })?;
    db.commit().await.map_err(AppError::from)?;
    tracing::debug!(
        review_workspace_id = %review_workspace_id,
        source_workspace_id = %review.source_workspace_id,
        "Workspace endpoint called: publish_review"
    );

    // One push at a time per (source, reviewer), held from the branch
    // snapshot through the publication: two overlapping pushes could
    // otherwise diff in one order and publish in the other, so the older
    // snapshot would get the higher round and supersede the newer one.
    let _admission =
        reviews::lock_review_admission(&state.system_pool, review.source_workspace_id, reviewer_id)
            .await?;
    // The reviewer's current work is the round: committed and pushed from
    // their running review workspace right here, never by hand.
    let mut db = scoped_db_for_workspace(
        &state,
        &auth,
        review.source_workspace_id,
        &["workspaces:read"],
    )
    .await?;
    let next_round =
        reviews::next_round_number(&mut db, review.source_workspace_id, reviewer_id, "human")
            .await?;
    db.commit().await.map_err(AppError::from)?;
    save_review_round(
        &state,
        &auth,
        &review,
        reviewer_id,
        &format!(
            "Review of {}: round {next_round}",
            review.source_workspace_name
        ),
    )
    .await?;
    // Owning the review workspace does not keep source access alive: an
    // reviewer may have lost active organization membership since the fork. The
    // reviewer's read access is held until the publication commits
    // (`g5.security.current-authority`).
    let mut db = scoped_db_for_workspace(
        &state,
        &auth,
        review.source_workspace_id,
        &["workspaces:read"],
    )
    .await?;
    crate::db::scoped::ensure_user_workspace_access_until_commit(
        &state.system_pool,
        &mut db,
        reviewer_id,
        org_id,
        review.source_workspace_id,
    )
    .await?;
    let (owner_id, source_branch) =
        reviews::workspace_owner_and_branch(&mut db, org_id, review.source_workspace_id).await?;
    crate::services::workspace_mutation_policy::ensure_workspace_allows_mutation(
        &mut db,
        review.source_workspace_id,
        crate::services::workspace_mutation_policy::WorkspaceMutationKind::WorkspaceOwnedCommentWrite,
        "workspaces.review_publish",
    )
    .await?;
    let options = reviews::GenerateOptions {
        store_artifacts: true,
    };
    let generated =
        reviews::generate_review_changes(&mut db, &state.repo_storage, &review, &options).await?;
    let excluded: std::collections::HashSet<&str> =
        body.excluded_paths.iter().map(String::as_str).collect();
    let keys = reviews::change_keys(&generated.suggestions);
    // A key the preview listed that the regenerated round no longer has
    // names a change the reviewer edited since: the note or exclusion would
    // land nowhere (or on the wrong change), so the round is not published
    // until the reviewer previews again.
    let known: std::collections::HashSet<&str> = keys.iter().map(String::as_str).collect();
    let stale: Vec<&str> = body
        .excluded_changes
        .iter()
        .map(String::as_str)
        .chain(body.change_notes.keys().map(String::as_str))
        .filter(|key| !known.contains(key))
        .collect();
    if !stale.is_empty() {
        return Err(AppError::from_error(G5Error::Conflict {
            reason: format!(
                "{} of the previewed changes moved or changed since the preview; open Push review again",
                stale.len()
            ),
        }));
    }
    let excluded_changes: std::collections::HashSet<&str> =
        body.excluded_changes.iter().map(String::as_str).collect();
    let mut selected_keys: Vec<String> = Vec::new();
    let selected: Vec<GeneratedSuggestion> = generated
        .suggestions
        .into_iter()
        .zip(keys)
        .filter(|(item, key)| {
            !excluded.contains(item.path.as_str())
                && !excluded_changes.contains(key.as_str())
                && !body
                    .excluded_files
                    .iter()
                    .any(|file| file.repo_id == item.repo_id && file.path == item.path)
        })
        .map(|(item, key)| {
            selected_keys.push(key);
            item
        })
        .collect();
    let author_label = reviews::reviewer_handle(&state.system_pool, reviewer_id).await?;
    let author = SuggestionAuthor {
        reviewer_user_id: reviewer_id,
        author_kind: "human".to_string(),
        author_label,
    };
    let publication = reviews::create_publication(
        &mut db,
        reviews::NewPublication {
            org_id,
            source_workspace_id: review.source_workspace_id,
            review_workspace_id: Some(review_workspace_id),
            reviewer_user_id: reviewer_id,
            author_kind: author.author_kind.clone(),
            author_label: author.author_label.clone(),
            base_commit_vector: Value::Array(generated.base_commit_vector),
            summary: body.summary.unwrap_or_default().trim().to_string(),
            verdict: body.verdict.clone(),
            scores: body.scores.clone(),
            kind: reviews::PUBLICATION_KIND_ROUND.to_string(),
        },
    )
    .await?;
    let findings =
        reviews::insert_findings(&mut db, org_id, publication.id, &body.findings).await?;
    let finding_by_path = reviews::finding_paths(&body.findings, &findings);
    let suggestions = reviews::insert_generated_suggestions(
        &mut db,
        &state.repo_storage,
        org_id,
        review.source_workspace_id,
        &source_branch,
        &publication,
        &author,
        &selected,
        &body.notes,
        &selected_keys,
        &body.change_notes,
        &finding_by_path,
    )
    .await?;

    // Tell the owner and the other reviewers.
    let reviewer_name = reviews::reviewer_display_name(&state.system_pool, reviewer_id).await?;
    let other_reviewers = reviews::reviewer_user_ids(&mut db, org_id, review.source_workspace_id)
        .await?
        .into_iter()
        .filter(|id| *id != reviewer_id && Some(*id) != owner_id);
    let title = format!(
        "{reviewer_name} published review {} of {}",
        publication.round, review.source_workspace_name
    );
    let files_touched = suggestions
        .iter()
        .filter_map(|s| s.path.as_deref())
        .collect::<std::collections::HashSet<_>>()
        .len();
    let message = format!(
        "{} suggestion{} in {} file{}{}",
        suggestions
            .iter()
            .map(|s| s.edits.len().max(1))
            .sum::<usize>(),
        if suggestions
            .iter()
            .map(|s| s.edits.len().max(1))
            .sum::<usize>()
            == 1
        {
            ""
        } else {
            "s"
        },
        files_touched,
        if files_touched == 1 { "" } else { "s" },
        publication
            .verdict
            .as_deref()
            .map(|v| format!(" · verdict: {}", v.replace('_', " ")))
            .unwrap_or_default()
    );
    if let Some(owner_id) = owner_id {
        notify(
            &state,
            &mut db,
            review.source_workspace_id,
            owner_id,
            reviewer_id,
            "review",
            &title,
            &message,
            vec![open_reviews_action(&review)],
        )
        .await?;
    }
    for other in other_reviewers {
        notify(
            &state,
            &mut db,
            review.source_workspace_id,
            other,
            reviewer_id,
            "review",
            &title,
            &message,
            vec![open_reviews_action(&review)],
        )
        .await?;
    }
    db.commit().await.map_err(AppError::from)?;

    tracing::info!(
        review_workspace_id = %review_workspace_id,
        publication_id = %publication.id,
        round = publication.round,
        suggestions = suggestions.len(),
        skipped = generated.skipped.len(),
        "Workspace endpoint succeeded: publish_review"
    );
    Ok((
        StatusCode::CREATED,
        Json(ResponseEnvelope::ok(PublishReviewResponseDTO {
            publication,
            findings,
            suggestions,
            skipped: generated.skipped,
        })),
    ))
}

/// Push review from FILES (`g5.governance.file-suggestions`): a reader who
/// does not own the source turns their edit of one existing text file into
/// suggestions for the owner, whatever the state of the owner's VM. Like a
/// reply, it writes review rows only, never the source's content; the owner
/// decides the suggestions like any other.
async fn publish_file_suggestions(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
    Json(body): Json<FileSuggestionRequestDTO>,
) -> Result<
    (
        StatusCode,
        Json<ResponseEnvelope<FileSuggestionResponseDTO>>,
    ),
    AppError,
> {
    let reader_id = user.user_uuid();
    let source_id = params.workspace_id;
    // The sanitizer's reason, as a FILES push refusal.
    let path = crate::routes::files::sanitize_path(&body.path).map_err(|error| {
        let reason = match &error.inner {
            G5Error::ValidationError { details } => details["path"].as_str().map(str::to_string),
            _ => None,
        };
        reason.map_or(error, |reason| {
            AppError::from_error(reviews::file_suggestion_refusal("path", reason))
        })
    })?;
    let note = body.note.as_deref().unwrap_or_default().trim().to_string();
    if note.chars().count() > MAX_FILE_SUGGESTION_NOTE_CHARS {
        return Err(AppError::from_error(reviews::file_suggestion_refusal(
            "note",
            format!("The note is longer than {MAX_FILE_SUGGESTION_NOTE_CHARS} characters"),
        )));
    }

    // Refuse before reading anything: the reader must not own the source,
    // and the source must accept thread writes.
    let mut db =
        scoped_db_for_workspace(&state, &auth, params.workspace_id, &["workspaces:read"]).await?;
    ensure_workspace_repo_id(&mut db, params.workspace_id, body.repo_id, "repoId").await?;
    admit_file_suggestions(&mut db, source_id, reader_id).await?;
    db.rollback().await.map_err(AppError::from)?;

    // The current file exactly as this reader reads it: a non-owner is
    // served the durable committed or flush snapshot, never live bytes, and
    // never a commit or blob the client names (`.base`).
    let current = crate::routes::workspaces::read_workspace_file_for_actor_bounded(
        &state,
        &user,
        &auth,
        source_id,
        body.repo_id,
        &path,
        reviews::MAX_FILE_SUGGESTION_TEXT_BYTES,
    )
    .await
    .map_err(|error| {
        // Past the text limit: the reader's reason, as a FILES push refusal.
        let reason = match &error.inner {
            G5Error::FileTooLarge { .. } => Some(error.inner.to_string()),
            _ => None,
        };
        reason.map_or(error, |reason| {
            AppError::from_error(reviews::file_suggestion_refusal("path", reason))
        })
    })?;
    let current_bytes = match current.encoding.as_str() {
        "utf-8" => current.content.into_bytes(),
        "base64" => base64::engine::general_purpose::STANDARD
            .decode(&current.content)
            .map_err(|error| {
                AppError::from_error(G5Error::Internal {
                    message: format!("durable read of {path} returned bad base64: {error}"),
                })
            })?,
        // Any other encoding is not text this route can compare.
        _ => {
            return Err(AppError::from_error(reviews::file_suggestion_refusal(
                "path",
                format!("{path} is not valid UTF-8 text"),
            )))
        }
    };
    reviews::ensure_suggestion_text("path", &path, &current_bytes)?;
    // Both texts are checked and diffed before staleness is judged, so a
    // push that could never be recorded is not offered "Push anyway".
    let changes = reviews::file_suggestion_changes(
        body.repo_id,
        &path,
        body.base_content.as_bytes(),
        body.content.as_bytes(),
    )?;
    let stale = !g5_scm::GitRepo::blob_id_for_bytes(body.base_content.as_bytes())?
        .eq_ignore_ascii_case(&current.hash);
    if stale && !body.allow_stale_base {
        return Err(AppError::from_error(G5Error::FileVersionConflict {
            details: json!({"path": path, "hash": current.hash}),
        }));
    }

    // One transaction records the push: its own annotation publication, so
    // pushes neither supersede nor are superseded (`.additive`, `.record`).
    let mut db =
        scoped_db_for_workspace(&state, &auth, params.workspace_id, &["workspaces:read"]).await?;
    let org_id = db.primary_org_id().map_err(AppError::from)?.into_uuid();
    // The reader's audience grant is held until the push commits
    // (`g5.security.current-authority`); the admission below reads the
    // owner and policy under that hold.
    crate::db::scoped::ensure_user_workspace_access_until_commit(
        &state.system_pool,
        &mut db,
        reader_id,
        org_id,
        source_id,
    )
    .await?;
    let (owner_id, source_branch) = admit_file_suggestions(&mut db, source_id, reader_id).await?;
    let source_heads = reviews::source_heads(&mut db, &state.repo_storage, source_id).await?;
    let author = SuggestionAuthor {
        reviewer_user_id: reader_id,
        author_kind: "human".to_string(),
        author_label: reviews::reviewer_handle(&state.system_pool, reader_id).await?,
    };
    let publication = reviews::create_publication(
        &mut db,
        reviews::NewPublication {
            org_id,
            source_workspace_id: source_id,
            review_workspace_id: None,
            reviewer_user_id: reader_id,
            author_kind: author.author_kind.clone(),
            author_label: author.author_label.clone(),
            base_commit_vector: reviews::annotation_vector(&source_heads),
            summary: String::new(),
            verdict: Some("comment_only".to_string()),
            scores: None,
            kind: reviews::PUBLICATION_KIND_ANNOTATION.to_string(),
        },
    )
    .await?;
    let notes = if note.is_empty() {
        HashMap::new()
    } else {
        HashMap::from([(path.clone(), note)])
    };
    let suggestions = reviews::insert_generated_suggestions(
        &mut db,
        &state.repo_storage,
        org_id,
        source_id,
        &source_branch,
        &publication,
        &author,
        &changes,
        &notes,
        &[],
        &HashMap::new(),
        &reviews::FindingPaths::new(),
    )
    .await?;

    // The notification names the workspace as people see it; the branch is
    // for the suggestion commits (`g5.workspace.rename`).
    let source_name = reviews::workspace_name(&mut db, org_id, source_id).await?;
    if let Some(owner_id) = owner_id {
        let reader_name = reviews::reviewer_display_name(&state.system_pool, reader_id).await?;
        let count: usize = suggestions.iter().map(|s| s.edits.len().max(1)).sum();
        notify(
            &state,
            &mut db,
            source_id,
            owner_id,
            reader_id,
            "review",
            &format!("{reader_name} suggested changes to {path}"),
            &format!(
                "{count} suggestion{} in {source_name}",
                if count == 1 { "" } else { "s" }
            ),
            vec![open_reviews_action_for(source_id, &source_name)],
        )
        .await?;
    }
    db.commit().await.map_err(AppError::from)?;

    tracing::info!(
        source_workspace_id = %source_id,
        publication_id = %publication.id,
        suggestions = suggestions.len(),
        stale_base = stale,
        "Workspace endpoint succeeded: publish_file_suggestions"
    );
    Ok((
        StatusCode::CREATED,
        Json(ResponseEnvelope::ok(FileSuggestionResponseDTO {
            publication,
            suggestions,
        })),
    ))
}

/// The FILES push admission, checked on `db` before the file is read and
/// again in the transaction that records the push: a reader who does not
/// own the source, on a source that accepts thread writes (not Base,
/// read-only or freezing), as for a reply. Returns the owner and the
/// source's name, which is also its branch.
async fn admit_file_suggestions(
    db: &mut g5_authz::ScopedDb,
    source_id: Uuid,
    reader_id: Uuid,
) -> Result<(Option<Uuid>, String), AppError> {
    let org_id = db.primary_org_id().map_err(AppError::from)?.into_uuid();
    let (owner_id, source_branch) =
        reviews::workspace_owner_and_branch(db, org_id, source_id).await?;
    if owner_id == Some(reader_id) {
        return Err(AppError::from_error(G5Error::Conflict {
            reason: "The workspace owner edits files directly".to_string(),
        }));
    }
    crate::services::comment_service::ensure_thread_writes_allowed(
        db,
        source_id,
        "workspaces.review_file_suggestions",
    )
    .await?;
    Ok((owner_id, source_branch))
}

// ─── Reading publications and suggestions (source workspace) ─────────────────

async fn list_publications(
    State(state): State<Arc<AppState>>,
    _user: AuthenticatedUser,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
) -> Result<Json<ResponseEnvelope<ReviewPublicationsDTO>>, AppError> {
    let mut db =
        scoped_db_for_workspace(&state, &auth, params.workspace_id, &["workspaces:read"]).await?;
    let publications = reviews::list_publications(&mut db, params.workspace_id).await?;
    let findings = reviews::list_findings(&mut db, params.workspace_id, None).await?;
    let source_heads =
        reviews::source_heads(&mut db, &state.repo_storage, params.workspace_id).await?;
    let repo_storage = state.repo_storage.scoped(&db);
    let org_id = db.primary_org_id().map_err(AppError::from)?.into_uuid();
    db.commit().await.map_err(AppError::from)?;
    let source_head_times = {
        let heads = source_heads.clone();
        tokio::task::spawn_blocking(move || {
            reviews::source_head_times(&repo_storage, org_id, &heads)
        })
        .await
        .map_err(|error| {
            AppError::from_error(G5Error::Internal {
                message: format!("source head times task failed: {error}"),
            })
        })?
    };
    let mut reviewer_display_names = HashMap::new();
    for publication in &publications {
        let id = publication.publication.reviewer_user_id;
        if let std::collections::hash_map::Entry::Vacant(entry) = reviewer_display_names.entry(id) {
            entry.insert(reviews::reviewer_display_name(&state.system_pool, id).await?);
        }
    }
    Ok(Json(ResponseEnvelope::ok(ReviewPublicationsDTO {
        publications,
        findings,
        reviewer_display_names,
        source_heads,
        source_head_times,
    })))
}

async fn list_suggestions(
    State(state): State<Arc<AppState>>,
    _user: AuthenticatedUser,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
    Query(query): Query<SuggestionListQuery>,
) -> Result<Json<ResponseEnvelope<SuggestionListDTO>>, AppError> {
    let mut db =
        scoped_db_for_workspace(&state, &auth, params.workspace_id, &["workspaces:read"]).await?;
    if let Some(repo_id) = query.repo_id {
        ensure_workspace_repo_id(&mut db, params.workspace_id, repo_id, "repoId").await?;
    }
    let mut suggestions = reviews::list_suggestions(
        &mut db,
        params.workspace_id,
        &SuggestionFilter {
            publication_id: query.publication_id,
            finding_id: query.finding_id,
            repo_id: query.repo_id,
            path: query.path,
            open_only: query.open_only,
            current_only: query.current_only,
            kind: query.kind,
            status: query.status,
        },
    )
    .await?;
    if query.current_only {
        // A file page shows only what is about the source's current
        // commit: once the owner commits again, earlier rounds' rows leave
        // the files and stay on the Reviews page under their commit.
        let source_heads =
            reviews::source_heads(&mut db, &state.repo_storage, params.workspace_id).await?;
        let vectors: HashMap<Uuid, (Value, Option<Uuid>)> =
            reviews::list_publications(&mut db, params.workspace_id)
                .await?
                .into_iter()
                .map(|summary| {
                    (
                        summary.publication.id,
                        (
                            summary.publication.base_commit_vector,
                            summary.publication.superseded_by,
                        ),
                    )
                })
                .collect();
        suggestions.retain(|suggestion| {
            vectors
                .get(&suggestion.publication_id)
                .is_none_or(|(vector, superseded)| {
                    reviews::publication_is_current(vector, &source_heads)
                        || (superseded.is_none()
                            && !suggestion.edits.is_empty()
                            && !matches!(
                                suggestion.status.as_str(),
                                "withdrawn" | "obsolete" | "superseded"
                            ))
                })
        });
    }
    db.commit().await.map_err(AppError::from)?;
    Ok(Json(ResponseEnvelope::ok(SuggestionListDTO {
        suggestions,
    })))
}

async fn count_suggestions(
    State(state): State<Arc<AppState>>,
    _user: AuthenticatedUser,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
) -> Result<Json<ResponseEnvelope<SuggestionCountsDTO>>, AppError> {
    let mut db =
        scoped_db_for_workspace(&state, &auth, params.workspace_id, &["workspaces:read"]).await?;
    let source_heads =
        reviews::source_heads(&mut db, &state.repo_storage, params.workspace_id).await?;
    let paths =
        reviews::count_open_suggestions_by_path(&mut db, params.workspace_id, &source_heads)
            .await?;
    db.commit().await.map_err(AppError::from)?;
    Ok(Json(ResponseEnvelope::ok(SuggestionCountsDTO { paths })))
}

// ─── Owner decisions ─────────────────────────────────────────────────────────

/// Accept a hunk or new-file suggestion into the owner's running workspace.
/// Refused with
/// `WORKSPACE_NOT_RUNNING` while the owner's VM is stopped.
async fn accept_suggestion(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    Path(params): Path<SuggestionPathParams>,
    body: Option<Json<AcceptSuggestionRequestDTO>>,
) -> Result<Json<ResponseEnvelope<SuggestionDTO>>, AppError> {
    let body = body.map(|Json(body)| body).unwrap_or_default();
    let db =
        scoped_db_for_workspace(&state, &auth, params.workspace_id, &["workspaces:write"]).await?;
    let decided = accept_suggestion_as(
        &state,
        db,
        user.user_uuid(),
        params.workspace_id,
        params.suggestion_id,
        body,
    )
    .await?;
    Ok(Json(ResponseEnvelope::ok(decided)))
}

/// The owner's accept decision on an open connection. `db` is committed on
/// success and on the obsolete
/// outcome (which is recorded, then reported as a conflict). The caller has
/// authenticated `owner_id` as the owner of `workspace_id`; the ownership
/// and mutation policy are re-checked here on the same transaction.
pub(crate) async fn accept_suggestion_as(
    state: &Arc<AppState>,
    mut db: g5_authz::ScopedDb,
    owner_id: Uuid,
    workspace_id: Uuid,
    suggestion_id: Uuid,
    body: AcceptSuggestionRequestDTO,
) -> Result<SuggestionDTO, AppError> {
    let scope = db.reopener();
    crate::services::workspace_mutation_policy::ensure_workspace_mutable_by(
        &mut db,
        workspace_id,
        owner_id,
        "workspaces.review_accept",
    )
    .await?;
    // Decisions write thread notes: read-only workspaces and Base refuse
    // them, including an artifact kept without a thread row.
    crate::services::comment_service::ensure_thread_writes_allowed(
        &mut db,
        workspace_id,
        "workspaces.review_accept",
    )
    .await?;
    let org_id = db.primary_org_id().map_err(AppError::from)?.into_uuid();
    // Decisions on this workspace run one at a time (advisory lock for this
    // transaction), taken before any row lock: two accepts cannot both write
    // their files and then deadlock on each other's suggestion or finding
    // rows.
    reviews::lock_review_decisions(&mut db, workspace_id).await?;
    // The row stays locked in `db` until the decision commits below, after
    // the file write: a concurrent reject or withdraw waits here and then
    // finds the suggestion decided instead of relabelling an applied change.
    let (suggestion, publication) =
        reviews::lock_suggestion(&mut db, workspace_id, suggestion_id).await?;
    if suggestion.kind == "hunk" && !suggestion.edits.is_empty() {
        return accept_hunk_edits_as(
            state,
            db,
            owner_id,
            workspace_id,
            suggestion,
            publication,
            body,
        )
        .await;
    }
    if body.edit_id.is_some() || suggestion.status != "open" {
        return Err(AppError::from_error(G5Error::Conflict {
            reason: format!(
                "suggestion is {} or has no individual edits",
                suggestion.status
            ),
        }));
    }
    if suggestion.kind == "artifact" {
        // Keeping an artifact is the owner's decision alone: nothing is
        // written into the workspace (no VM needed); the row joins the
        // workspace's review artifacts, which outlive rounds and commits.
        let updated =
            reviews::set_suggestion_status(&mut db, suggestion.id, "accepted", owner_id, None)
                .await?;
        let name = updated.path.clone().unwrap_or_default();
        if let Some(comment_id) = updated.comment_id {
            reviews::add_thread_reply(
                &mut db,
                comment_id,
                owner_id,
                "Kept: added to the workspace's review artifacts.",
            )
            .await?;
        }
        if publication.reviewer_user_id != owner_id {
            let owner_name = reviews::reviewer_display_name(&state.system_pool, owner_id).await?;
            let source_name = reviews::workspace_name(&mut db, org_id, workspace_id).await?;
            notify_reviewer(
                state,
                &mut db,
                workspace_id,
                publication.reviewer_user_id,
                owner_id,
                &format!("{} kept your artifact {name}", owner_name),
                &format!("{source_name}: it is now among the workspace's review artifacts."),
                vec![open_reviews_action_for(workspace_id, &source_name)],
            )
            .await?;
        }
        db.commit().await.map_err(AppError::from)?;
        return Ok(SuggestionDTO {
            suggestion: updated,
            written_hash: None,
            already_applied: false,
            checkpoint_pending: false,
        });
    }
    let (repo_id, path) = match (suggestion.repo_id, suggestion.path.as_deref()) {
        (Some(repo_id), Some(path)) if matches!(suggestion.kind.as_str(), "hunk" | "new_file") => {
            (repo_id, path.to_string())
        }
        _ => {
            return Err(AppError::from_error(G5Error::ValidationError {
                details: json!({"kind": "only hunk, new_file and artifact suggestions can be accepted"}),
            }))
        }
    };
    let path = crate::routes::files::sanitize_path(&path)?;
    // The same policy as Save: a mutable, non-base workspace of the owner's
    // and one of its repositories.
    crate::routes::workspaces::ensure_workspace_file_mutation_allowed(
        &mut db,
        workspace_id,
        owner_id,
        repo_id,
    )
    .await?;

    // The owner's VM must be running: accept is a Save into the working tree.
    let handle = state
        .workspace_runtime
        .owner_handle(workspace_id, owner_id)
        .await?;
    let after = body
        .after_lines
        .clone()
        .or_else(|| suggestion.after_lines.clone())
        .unwrap_or_default();
    // The checkout must still be on the workspace's member branch before
    // anything is *recorded* as applied: the write path runs this guard
    // itself, but the already-applied shortcuts below never write, and a
    // terminal that switched branches would otherwise pass a checkout that
    // happens to contain the proposal off as the workspace's own.
    state
        .workspace_runtime
        .workspace_changes(&scope, workspace_id, owner_id)
        .await?;
    let (content, expected_hash, already_applied) = if suggestion.kind == "new_file" {
        match crate::routes::workspaces::read_live_workspace_file(
            state, &handle, owner_id, repo_id, &path,
        )
        .await
        {
            Ok(existing) => {
                if existing.encoding != "utf-8" {
                    return Err(AppError::from_error(G5Error::Conflict {
                        reason: format!(
                            "{path} already exists in your workspace and is not a text file"
                        ),
                    }));
                }
                if existing.content == after {
                    (None, Some(existing.hash), true)
                } else {
                    return Err(AppError::from_error(G5Error::Conflict {
                        reason: format!(
                            "{path} already exists in your workspace with different content; \
                             accept it as an edit of that file instead"
                        ),
                    }));
                }
            }
            Err(error) if matches!(error.inner, G5Error::NotFound { .. }) => {
                (Some(after.clone()), None, false)
            }
            Err(error) => return Err(error),
        }
    } else {
        let current = crate::routes::workspaces::read_live_workspace_file(
            state, &handle, owner_id, repo_id, &path,
        )
        .await?;
        if current.encoding != "utf-8" {
            return Err(AppError::from_error(G5Error::Conflict {
                reason: format!("{path} is not a text file in your workspace"),
            }));
        }
        // The published line is authoritative only while the file is still
        // the version the hunk was cut from. `content_hash` is the sha256 of
        // the raw bytes at publication; `current.hash` is the runtime's Git
        // blob id (header plus content, repository object format), which is
        // kept for the write precondition but never equals a raw digest.
        let live_digest = format!("{:x}", Sha256::digest(current.content.as_bytes()));
        let anchor_trusted = suggestion.content_hash.as_deref() == Some(live_digest.as_str());
        let context = reviews::AnchorContext::parse(suggestion.anchor_context.as_deref());
        match reviews::apply_hunk_with_context(
            &current.content,
            suggestion.before_lines.as_deref().unwrap_or_default(),
            &after,
            suggestion.anchor_start,
            &context,
            anchor_trusted,
        ) {
            Ok(HunkApplication::Applied(new_content)) => {
                (Some(new_content), Some(current.hash), false)
            }
            Ok(HunkApplication::AlreadyApplied) => (None, Some(current.hash), true),
            Err(HunkRefusal::TargetNotFound) => {
                // The owner's file moved on: the proposal cannot apply as
                // published. Retire it so the panel stops offering Accept,
                // and tell the reviewer to update their copy and push again.
                let obsolete = reviews::set_suggestion_status(
                    &mut db,
                    suggestion.id,
                    "obsolete",
                    owner_id,
                    None,
                )
                .await?;
                if let Some(finding_id) = obsolete.finding_id {
                    reviews::settle_finding_status(&mut db, finding_id).await?;
                }
                if let Some(comment_id) = obsolete.comment_id {
                    reviews::add_thread_reply(
                        &mut db,
                        comment_id,
                        owner_id,
                        &format!(
                            "Obsolete: the lines this suggestion changes were modified in {path} \
                             since it was published."
                        ),
                    )
                    .await?;
                }
                let source_name = reviews::workspace_name(&mut db, org_id, workspace_id).await?;
                // A push without a review workspace (FILES) has no copy to
                // update: the reviewer suggests the change again instead.
                let (reviewer_step, owner_step) = if publication.review_workspace_id.is_some() {
                    (
                        format!("Update your review workspace from {source_name} and push again."),
                        "Ask the reviewer to update their copy and push again",
                    )
                } else {
                    (
                        "Suggest the change again from FILES.".to_string(),
                        "Ask the reviewer to suggest the change again",
                    )
                };
                notify_reviewer(
                    state,
                    &mut db,
                    workspace_id,
                    publication.reviewer_user_id,
                    owner_id,
                    &format!("Your suggestion on {path} is obsolete"),
                    &format!(
                        "{source_name}: the lines changed since you published. {reviewer_step}"
                    ),
                    vec![open_reviews_action_for(workspace_id, &source_name)],
                )
                .await?;
                db.commit().await.map_err(AppError::from)?;
                return Err(AppError::from_error(G5Error::Conflict {
                    reason: format!(
                        "the lines this suggestion changes are no longer in {path}; it is now \
                         marked obsolete. {owner_step}"
                    ),
                }));
            }
            Err(HunkRefusal::Ambiguous) => {
                return Err(AppError::from_error(G5Error::Conflict {
                    reason: format!(
                        "the lines this suggestion changes appear more than once in {path}; \
                         apply it by hand or ask for a wider hunk"
                    ),
                }))
            }
        }
    };
    let written_hash = match content {
        Some(content) => {
            let written = crate::routes::workspaces::write_live_workspace_file(
                state,
                &scope,
                &handle,
                owner_id,
                repo_id,
                &path,
                &content,
                "utf-8",
                expected_hash.as_deref(),
            )
            .await?;
            written
                .get("hash")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    AppError::from_error(G5Error::Internal {
                        message: "accepted file did not include its blob hash".into(),
                    })
                })?
                .to_string()
        }
        None => expected_hash.ok_or_else(|| {
            AppError::from_error(G5Error::Internal {
                message: "already-applied file did not include its blob hash".into(),
            })
        })?,
    };
    let checkpoint = crate::routes::workspaces::commit_and_push_files(
        state,
        workspace_id,
        owner_id,
        &[(repo_id, path.clone(), written_hash.clone())],
        &format!("Accept review suggestion for {path}"),
    )
    .await;
    // The file mutation has already happened. A failed commit or push must
    // not leave this proposal open for Reject/Withdraw to misrepresent it as
    // unapplied. The selected path remains in Changes for explicit recovery.
    let checkpoint_pending = checkpoint.is_err();
    if let Err(error) = &checkpoint {
        tracing::warn!(source_workspace_id = %workspace_id, suggestion_id = %suggestion.id, %error, "accepted file is pending its Git checkpoint");
    }

    let edited = body
        .after_lines
        .as_deref()
        .filter(|edited| Some(*edited) != suggestion.after_lines.as_deref());
    let updated =
        reviews::set_suggestion_status(&mut db, suggestion.id, "accepted", owner_id, edited)
            .await?;
    if let Some(finding_id) = updated.finding_id {
        reviews::settle_finding_status(&mut db, finding_id).await?;
    }
    if let Some(comment_id) = updated.comment_id {
        let note = if checkpoint_pending {
            format!("Accepted: applied in the owner's workspace, but commit/push is pending for {path}.")
        } else if already_applied {
            "Accepted: the change was already in the owner's workspace.".to_string()
        } else if edited.is_some() {
            format!("Accepted with edits: committed and pushed {path}.")
        } else {
            format!("Accepted: committed and pushed {path}.")
        };
        reviews::add_thread_reply(&mut db, comment_id, owner_id, &note).await?;
    }
    let owner_name = reviews::reviewer_display_name(&state.system_pool, owner_id).await?;
    let actor_name = owner_name;
    let source_name = reviews::workspace_name(&mut db, org_id, workspace_id).await?;
    // Competing proposals for the same lines lose: they cannot apply on top
    // of the accepted one, so they are retired and their authors told. The
    // decision also applies to a retry after a write whose transaction failed,
    // so an already-applied acceptance settles competing proposals too.
    let retired = if suggestion.kind == "hunk" {
        reviews::obsolete_competing_suggestions(&mut db, workspace_id, &updated, owner_id).await?
    } else {
        Vec::new()
    };
    for other in &retired {
        if let Some(comment_id) = other.comment_id {
            reviews::add_thread_reply(
                &mut db,
                comment_id,
                owner_id,
                &format!(
                    "Obsolete: {actor_name} accepted a competing suggestion for these lines of {path}."
                ),
            )
            .await?;
        }
        let (_, other_publication) =
            reviews::get_suggestion(&mut db, workspace_id, other.id).await?;
        if other_publication.reviewer_user_id != publication.reviewer_user_id {
            notify_reviewer(
                state,
                &mut db,
                workspace_id,
                other_publication.reviewer_user_id,
                owner_id,
                &format!("A competing suggestion on {path} was accepted"),
                &format!("{source_name}: your proposal for the same lines is now obsolete."),
                vec![open_reviews_action_for(workspace_id, &source_name)],
            )
            .await?;
        }
    }
    notify_reviewer(
        state,
        &mut db,
        workspace_id,
        publication.reviewer_user_id,
        owner_id,
        &format!("{actor_name} accepted your suggestion"),
        &format!(
            "{path} in {source_name}{}",
            if edited.is_some() { " (edited)" } else { "" }
        ),
        vec![open_reviews_action_for(workspace_id, &source_name)],
    )
    .await?;
    db.commit().await.map_err(AppError::from)?;

    tracing::info!(
        source_workspace_id = %workspace_id,
        suggestion_id = %suggestion.id,
        already_applied,
        "Workspace endpoint succeeded: accept_suggestion"
    );
    Ok(SuggestionDTO {
        suggestion: updated,
        written_hash: Some(written_hash),
        already_applied,
        checkpoint_pending,
    })
}

/// Translate a legacy full-parent edit only when unchanged source between
/// selected children uniquely proves the replacement of each open child.
fn parent_edit_overrides(
    suggestion: &ReviewSuggestionRecord,
    selected: &[Uuid],
    proposed: &str,
) -> Result<HashMap<Uuid, String>, AppError> {
    let current = &suggestion.expected_before_lines;
    if suggestion
        .edits
        .iter()
        .any(|edit| selected.contains(&edit.id) && edit.source_conflict)
    {
        return Err(edit_composition_error(
            "An open edit overlaps accepted source; select a nonconflicting edit".into(),
        ));
    }
    let children: Vec<_> = suggestion
        .edits
        .iter()
        .filter(|e| selected.contains(&e.id))
        .collect();
    let mut remaining = proposed;
    let mut cursor = 0;
    let mut overrides = HashMap::new();
    for (index, edit) in children.iter().enumerate() {
        let prefix = &current[cursor..edit.current_start];
        remaining = remaining.strip_prefix(prefix).ok_or_else(|| {
            edit_composition_error(
                "A parent edit cannot modify unchanged or settled source; select one edit".into(),
            )
        })?;
        let end = children
            .get(index + 1)
            .map_or(current.len(), |next| next.current_start);
        let separator = &current[edit.current_end..end];
        let value = if index + 1 == children.len() {
            remaining.strip_suffix(separator).ok_or_else(|| {
                edit_composition_error(
                    "A parent edit cannot modify unchanged or settled source; select one edit"
                        .into(),
                )
            })?
        } else {
            let positions: Vec<_> = remaining
                .match_indices(separator)
                .map(|(pos, _)| pos)
                .collect();
            if separator.is_empty() || positions.len() != 1 {
                return Err(edit_composition_error(
                    "The full-parent replacement cannot be divided unambiguously; select one edit"
                        .into(),
                ));
            }
            &remaining[..positions[0]]
        };
        overrides.insert(edit.id, value.to_string());
        remaining = &remaining[value.len()..];
        cursor = edit.current_end;
    }
    Ok(overrides)
}

fn edit_composition_error(reason: String) -> AppError {
    AppError::from_error(G5Error::Conflict { reason })
}

async fn accept_hunk_edits_as(
    state: &Arc<AppState>,
    mut db: g5_authz::ScopedDb,
    owner_id: Uuid,
    workspace_id: Uuid,
    mut suggestion: ReviewSuggestionRecord,
    publication: ReviewPublicationRecord,
    body: AcceptSuggestionRequestDTO,
) -> Result<SuggestionDTO, AppError> {
    let scope = db.reopener();
    if let Some(id) = body.edit_id {
        let edit = suggestion
            .edits
            .iter()
            .find(|e| e.id == id)
            .ok_or_else(|| {
                AppError::from_error(G5Error::NotFound {
                    resource: format!("review edit {id}"),
                })
            })?;
        if edit.status == "accepted" {
            if body.after_lines.as_deref().is_some_and(|value| {
                value
                    != edit
                        .accepted_after_text
                        .as_deref()
                        .unwrap_or(&edit.after_text)
            }) {
                return Err(edit_composition_error(
                    "This edit was already accepted with a different replacement".into(),
                ));
            }
            let written_hash = edit.written_hash.clone();
            let checkpoint_pending = edit.checkpoint_pending;
            db.commit().await.map_err(AppError::from)?;
            return Ok(SuggestionDTO {
                suggestion,
                written_hash,
                already_applied: true,
                checkpoint_pending,
            });
        }
        if edit.status != "open" {
            return Err(edit_composition_error(format!("edit is {}", edit.status)));
        }
    }
    if suggestion.status != "open" {
        return Err(edit_composition_error(format!(
            "suggestion is {}",
            suggestion.status
        )));
    }
    let selected: Vec<_> = suggestion
        .edits
        .iter()
        .filter(|e| e.status == "open" && body.edit_id.is_none_or(|id| e.id == id))
        .map(|e| e.id)
        .collect();
    if selected.is_empty() {
        return Err(edit_composition_error("No remaining open edits".into()));
    }
    let repo_id = suggestion
        .repo_id
        .ok_or_else(|| edit_composition_error("Review edit has no repository".into()))?;
    let path = crate::routes::files::sanitize_path(suggestion.path.as_deref().unwrap_or_default())?;
    crate::routes::workspaces::ensure_workspace_file_mutation_allowed(
        &mut db,
        workspace_id,
        owner_id,
        repo_id,
    )
    .await?;
    let handle = state
        .workspace_runtime
        .owner_handle(workspace_id, owner_id)
        .await?;
    state
        .workspace_runtime
        .workspace_changes(&scope, workspace_id, owner_id)
        .await?;
    let current = crate::routes::workspaces::read_live_workspace_file(
        state, &handle, owner_id, repo_id, &path,
    )
    .await?;
    if current.encoding != "utf-8" {
        return Err(edit_composition_error(format!("{path} is not a text file")));
    }
    let digest = format!("{:x}", Sha256::digest(current.content.as_bytes()));
    let trusted = suggestion.content_hash.as_deref() == Some(digest.as_str());
    if trusted {
        reviews::use_captured_source_projection(&mut suggestion);
    } else {
        reviews::refresh_suggestion_projection_with_version(
            &mut db,
            workspace_id,
            &mut suggestion,
            Some(&current.hash),
        )
        .await?;
    }
    let overrides = match (&body.after_lines, body.edit_id) {
        (Some(value), Some(id)) => HashMap::from([(id, value.clone())]),
        (Some(value), None) => parent_edit_overrides(&suggestion, &selected, value)?,
        _ => HashMap::new(),
    };
    let (before, after) = reviews::compose_suggestion_before_and_after(
        &mut db,
        workspace_id,
        &suggestion,
        &selected,
        &overrides,
        trusted,
        &current.hash,
    )
    .await?;

    let context = reviews::AnchorContext::parse(suggestion.anchor_context.as_deref());
    let captured = suggestion.before_lines.as_deref().unwrap_or_default();
    let captured_digest = format!("{:x}", Sha256::digest(captured.as_bytes()));
    let captured_without_final_newline = captured
        .strip_suffix('\n')
        .map(|bytes| format!("{:x}", Sha256::digest(bytes.as_bytes())));
    let whole_file_parent = suggestion.anchor_start == Some(1)
        && (suggestion.content_hash.as_deref() == Some(captured_digest.as_str())
            || captured_without_final_newline
                .as_deref()
                .is_some_and(|digest| suggestion.content_hash.as_deref() == Some(digest)));
    let applied = reviews::apply_review_edit_with_context(
        &current.content,
        &before,
        &after,
        suggestion.anchor_start,
        &context,
        trusted,
        whole_file_parent,
    )
    .map_err(|refusal| {
        edit_composition_error(match refusal {
            HunkRefusal::TargetNotFound => {
                "This edit's captured source no longer matches; it remains open for attention"
                    .into()
            }
            HunkRefusal::Ambiguous => {
                "This edit's source is ambiguous; it remains open for attention".into()
            }
        })
    })?;
    let (written_hash, already_applied) = match applied {
        HunkApplication::AlreadyApplied => (current.hash, true),
        HunkApplication::Applied(content) => {
            let written = crate::routes::workspaces::write_live_workspace_file(
                state,
                &scope,
                &handle,
                owner_id,
                repo_id,
                &path,
                &content,
                "utf-8",
                Some(&current.hash),
            )
            .await?;
            let hash = written
                .get("hash")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    AppError::from_error(G5Error::Internal {
                        message: "accepted edit did not return a file hash".into(),
                    })
                })?
                .to_string();
            (hash, false)
        }
    };
    let checkpoint = crate::routes::workspaces::commit_and_push_files(
        state,
        workspace_id,
        owner_id,
        &[(repo_id, path.clone(), written_hash.clone())],
        &format!("Accept review edit for {path}"),
    )
    .await;
    let checkpoint_pending = checkpoint.is_err();
    if let Err(error) = &checkpoint {
        tracing::warn!(%workspace_id, suggestion_id = %suggestion.id, %error, "accepted edit is pending its Git checkpoint");
    }
    // Once the guest write landed, settle only selected children, including a
    // failed checkpoint. Rejected/open siblings cannot be re-labelled applied.
    let now = chrono::Utc::now();
    for edit in &mut suggestion.edits {
        if !selected.contains(&edit.id) {
            continue;
        }
        edit.status = "accepted".into();
        edit.status_by = Some(owner_id);
        edit.status_at = Some(now);
        edit.accepted_after_text = overrides.get(&edit.id).cloned();
        edit.checkpoint_pending = checkpoint_pending;
        edit.written_hash = Some(written_hash.clone());
    }
    for id in &selected {
        let thread =
            reviews::ensure_edit_thread(&mut db, workspace_id, &mut suggestion, &publication, *id)
                .await?;
        let verb = if overrides.get(id).is_some_and(|value| {
            suggestion
                .edits
                .iter()
                .any(|e| e.id == *id && e.after_text != *value)
        }) {
            "Accepted with edits"
        } else {
            "Accepted"
        };
        let note = if checkpoint_pending {
            format!(
                "{}: applied in the owner's workspace, but commit/push is pending for {path}.",
                verb
            )
        } else if already_applied {
            format!("{}: the change was already in the owner's workspace.", verb)
        } else {
            format!("{}: committed and pushed {path}.", verb)
        };
        reviews::add_thread_reply(&mut db, thread, owner_id, &note).await?;
    }
    let updated =
        reviews::save_suggestion_edits(&mut db, workspace_id, &suggestion, owner_id).await?;
    if body.edit_id.is_none() {
        if let Some(thread) = updated.comment_id {
            let note = if checkpoint_pending {
                format!("Accepted: applied in the owner's workspace, but commit/push is pending for {path}.")
            } else if updated.status == "accepted" {
                format!("Accepted: committed and pushed {path}.")
            } else {
                format!("Accepted remaining open edits: committed and pushed {path}; earlier decisions are retained.")
            };
            reviews::add_thread_reply(&mut db, thread, owner_id, &note).await?;
        }
    }
    if let Some(finding_id) = updated.finding_id {
        reviews::settle_finding_status(&mut db, finding_id).await?;
    }
    let retired =
        reviews::obsolete_competing_edits(&mut db, workspace_id, &updated, &selected, owner_id)
            .await?;
    for other in retired {
        let (_, other_publication) =
            reviews::get_suggestion(&mut db, workspace_id, other.id).await?;
        if other_publication.reviewer_user_id != publication.reviewer_user_id {
            let org_id = db.primary_org_id().map_err(AppError::from)?.into_uuid();
            let name = reviews::workspace_name(&mut db, org_id, workspace_id).await?;
            notify_reviewer(state, &mut db, workspace_id, other_publication.reviewer_user_id, owner_id, "A competing review edit was accepted", &format!("{path} in {name}: the conflicting edit is obsolete; independent siblings remain open."), vec![open_reviews_action_for(workspace_id, &name)]).await?;
        }
    }
    let owner_name = reviews::reviewer_display_name(&state.system_pool, owner_id).await?;
    let org_id = db.primary_org_id().map_err(AppError::from)?.into_uuid();
    let source_name = reviews::workspace_name(&mut db, org_id, workspace_id).await?;
    notify_reviewer(
        state,
        &mut db,
        workspace_id,
        publication.reviewer_user_id,
        owner_id,
        &format!(
            "{} accepted your {}",
            owner_name,
            if body.edit_id.is_some() {
                "review edit"
            } else {
                "suggestion"
            }
        ),
        &format!("{path} in {source_name}"),
        vec![open_reviews_action_for(workspace_id, &source_name)],
    )
    .await?;
    db.commit().await.map_err(AppError::from)?;
    Ok(SuggestionDTO {
        suggestion: updated,
        written_hash: Some(written_hash),
        already_applied,
        checkpoint_pending,
    })
}

async fn reject_suggestion(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    Path(params): Path<SuggestionPathParams>,
    body: Option<Json<RejectSuggestionRequestDTO>>,
) -> Result<Json<ResponseEnvelope<SuggestionDTO>>, AppError> {
    let body = body.map(|Json(body)| body).unwrap_or_default();
    let reason = body
        .reason
        .map(|reason| reason.trim().to_string())
        .filter(|reason| !reason.is_empty());
    let db =
        scoped_db_for_workspace(&state, &auth, params.workspace_id, &["workspaces:write"]).await?;
    let decided = reject_suggestion_as(
        &state,
        db,
        user.user_uuid(),
        params.workspace_id,
        params.suggestion_id,
        reason,
        body.edit_id,
    )
    .await?;
    Ok(Json(ResponseEnvelope::ok(decided)))
}

/// The owner's reject decision on an open connection. `db` is committed on
/// success. `reason` is already trimmed and
/// non-empty when present.
pub(crate) async fn reject_suggestion_as(
    state: &Arc<AppState>,
    mut db: g5_authz::ScopedDb,
    owner_id: Uuid,
    workspace_id: Uuid,
    suggestion_id: Uuid,
    reason: Option<String>,
    edit_id: Option<Uuid>,
) -> Result<SuggestionDTO, AppError> {
    crate::services::workspace_mutation_policy::ensure_workspace_owner_is(
        &mut db,
        workspace_id,
        owner_id,
    )
    .await?;
    // Decisions write thread notes: read-only workspaces and Base refuse them.
    crate::services::comment_service::ensure_thread_writes_allowed(
        &mut db,
        workspace_id,
        "workspaces.review_reject",
    )
    .await?;
    let org_id = db.primary_org_id().map_err(AppError::from)?.into_uuid();
    reviews::lock_review_decisions(&mut db, workspace_id).await?;
    let (suggestion, publication) =
        reviews::lock_suggestion(&mut db, workspace_id, suggestion_id).await?;
    let updated = if !suggestion.edits.is_empty() {
        if suggestion.status != "open" {
            return Err(edit_composition_error(format!(
                "suggestion is {}",
                suggestion.status
            )));
        }
        let mut suggestion = suggestion;
        let selected: Vec<_> = suggestion
            .edits
            .iter()
            .filter(|e| e.status == "open" && edit_id.is_none_or(|id| id == e.id))
            .map(|e| e.id)
            .collect();
        if selected.is_empty() {
            return Err(edit_composition_error(
                "The selected edit is not open".into(),
            ));
        }
        let now = chrono::Utc::now();
        for edit in &mut suggestion.edits {
            if selected.contains(&edit.id) {
                edit.status = "rejected".into();
                edit.status_by = Some(owner_id);
                edit.status_at = Some(now);
            }
        }
        for id in selected {
            let thread = reviews::ensure_edit_thread(
                &mut db,
                workspace_id,
                &mut suggestion,
                &publication,
                id,
            )
            .await?;
            let note = reason.as_ref().map_or_else(
                || "Rejected.".to_string(),
                |reason| format!("Rejected: {reason}"),
            );
            reviews::add_thread_reply(&mut db, thread, owner_id, &note).await?;
        }
        reviews::save_suggestion_edits(&mut db, workspace_id, &suggestion, owner_id).await?
    } else {
        if edit_id.is_some() {
            return Err(edit_composition_error(
                "This suggestion has no individual edits".into(),
            ));
        }
        reviews::set_suggestion_status(&mut db, suggestion.id, "rejected", owner_id, None).await?
    };
    if let Some(finding_id) = updated.finding_id {
        reviews::settle_finding_status(&mut db, finding_id).await?;
    }
    if let Some(comment_id) = updated.comment_id.filter(|_| edit_id.is_none()) {
        let note = match &reason {
            Some(reason) => format!("Rejected: {reason}"),
            None => "Rejected.".to_string(),
        };
        reviews::add_thread_reply(&mut db, comment_id, owner_id, &note).await?;
    }
    let owner_name = reviews::reviewer_display_name(&state.system_pool, owner_id).await?;
    let source_name = reviews::workspace_name(&mut db, org_id, workspace_id).await?;
    notify_reviewer(
        state,
        &mut db,
        workspace_id,
        publication.reviewer_user_id,
        owner_id,
        &format!("{} rejected your suggestion", owner_name),
        &format!(
            "{} in {source_name}{}",
            updated.path.as_deref().unwrap_or("(no file)"),
            reason
                .as_deref()
                .map(|r| format!(": {r}"))
                .unwrap_or_default()
        ),
        vec![open_reviews_action_for(workspace_id, &source_name)],
    )
    .await?;
    db.commit().await.map_err(AppError::from)?;
    Ok(SuggestionDTO {
        suggestion: updated,
        written_hash: None,
        already_applied: false,
        checkpoint_pending: false,
    })
}

/// Resolve a comment-kind suggestion (a comment changes nothing).
async fn resolve_suggestion(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    Path(params): Path<SuggestionPathParams>,
) -> Result<Json<ResponseEnvelope<SuggestionDTO>>, AppError> {
    let db =
        scoped_db_for_workspace(&state, &auth, params.workspace_id, &["workspaces:write"]).await?;
    let decided = resolve_suggestion_as(
        &state,
        db,
        user.user_uuid(),
        params.workspace_id,
        params.suggestion_id,
    )
    .await?;
    Ok(Json(ResponseEnvelope::ok(decided)))
}

/// The owner's resolve decision on an open connection. `db` is committed on success.
pub(crate) async fn resolve_suggestion_as(
    state: &Arc<AppState>,
    mut db: g5_authz::ScopedDb,
    owner_id: Uuid,
    workspace_id: Uuid,
    suggestion_id: Uuid,
) -> Result<SuggestionDTO, AppError> {
    crate::services::workspace_mutation_policy::ensure_workspace_owner_is(
        &mut db,
        workspace_id,
        owner_id,
    )
    .await?;
    // Decisions write thread notes: read-only workspaces and Base refuse them.
    crate::services::comment_service::ensure_thread_writes_allowed(
        &mut db,
        workspace_id,
        "workspaces.review_resolve",
    )
    .await?;
    let org_id = db.primary_org_id().map_err(AppError::from)?.into_uuid();
    reviews::lock_review_decisions(&mut db, workspace_id).await?;
    let (suggestion, publication) =
        reviews::lock_open_suggestion(&mut db, workspace_id, suggestion_id).await?;
    if suggestion.kind != "comment" {
        return Err(AppError::from_error(G5Error::ValidationError {
            details: json!({"kind": "only comments are resolved; accept or reject a change"}),
        }));
    }
    let updated =
        reviews::set_suggestion_status(&mut db, suggestion.id, "resolved", owner_id, None).await?;
    if let Some(comment_id) = updated.comment_id {
        reviews::add_thread_reply(&mut db, comment_id, owner_id, "Resolved.").await?;
    }
    let owner_name = reviews::reviewer_display_name(&state.system_pool, owner_id).await?;
    let source_name = reviews::workspace_name(&mut db, org_id, workspace_id).await?;
    notify_reviewer(
        state,
        &mut db,
        workspace_id,
        publication.reviewer_user_id,
        owner_id,
        &format!("{} resolved your comment", owner_name),
        &format!(
            "{} in {source_name}",
            suggestion.path.as_deref().unwrap_or("(no file)")
        ),
        vec![open_reviews_action_for(workspace_id, &source_name)],
    )
    .await?;
    db.commit().await.map_err(AppError::from)?;
    Ok(SuggestionDTO {
        suggestion: updated,
        written_hash: None,
        already_applied: false,
        checkpoint_pending: false,
    })
}

/// The reviewer withdraws their own open suggestion.
async fn withdraw_suggestion(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    Path(params): Path<SuggestionPathParams>,
) -> Result<Json<ResponseEnvelope<SuggestionDTO>>, AppError> {
    let actor = user.user_uuid();
    let mut db =
        scoped_db_for_workspace(&state, &auth, params.workspace_id, &["workspaces:read"]).await?;
    let org_id = db.primary_org_id().map_err(AppError::from)?.into_uuid();
    // The reviewer withdraws with read access to the source; hold it until
    // the withdrawal commits (`g5.security.current-authority`).
    crate::db::scoped::ensure_user_workspace_access_until_commit(
        &state.system_pool,
        &mut db,
        actor,
        org_id,
        params.workspace_id,
    )
    .await?;
    // Decisions write thread notes: read-only workspaces and Base refuse them.
    crate::services::comment_service::ensure_thread_writes_allowed(
        &mut db,
        params.workspace_id,
        "workspaces.review_withdraw",
    )
    .await?;
    reviews::lock_review_decisions(&mut db, params.workspace_id).await?;
    let (suggestion, publication) =
        reviews::lock_open_suggestion(&mut db, params.workspace_id, params.suggestion_id).await?;
    if publication.reviewer_user_id != actor {
        return Err(AppError::from_error(G5Error::Forbidden {
            missing_scopes: Vec::new(),
        }));
    }
    let updated =
        reviews::set_suggestion_status(&mut db, suggestion.id, "withdrawn", actor, None).await?;
    if let Some(finding_id) = updated.finding_id {
        reviews::settle_finding_status(&mut db, finding_id).await?;
    }
    if let Some(comment_id) = updated.comment_id {
        reviews::add_thread_reply(&mut db, comment_id, actor, "Withdrawn.").await?;
    }
    let (owner_id, _) =
        reviews::workspace_owner_and_branch(&mut db, org_id, params.workspace_id).await?;
    if let Some(owner_id) = owner_id {
        let reviewer_name = reviews::reviewer_display_name(&state.system_pool, actor).await?;
        let source_name = reviews::workspace_name(&mut db, org_id, params.workspace_id).await?;
        notify(
            &state,
            &mut db,
            params.workspace_id,
            owner_id,
            actor,
            "review",
            &format!("{reviewer_name} withdrew a suggestion"),
            &format!(
                "{} in {source_name}",
                suggestion.path.as_deref().unwrap_or("(no file)")
            ),
            vec![open_reviews_action_for(params.workspace_id, &source_name)],
        )
        .await?;
    }
    db.commit().await.map_err(AppError::from)?;
    Ok(Json(ResponseEnvelope::ok(SuggestionDTO {
        suggestion: updated,
        written_hash: None,
        already_applied: false,
        checkpoint_pending: false,
    })))
}

// ─── Threads ─────────────────────────────────────────────────────────────────

async fn list_replies(
    State(state): State<Arc<AppState>>,
    _user: AuthenticatedUser,
    auth: AuthContext,
    Path(params): Path<SuggestionPathParams>,
    Query(query): Query<EditThreadQuery>,
) -> Result<Json<ResponseEnvelope<RepliesDTO>>, AppError> {
    let mut db =
        scoped_db_for_workspace(&state, &auth, params.workspace_id, &["workspaces:read"]).await?;
    let (suggestion, _) =
        reviews::get_suggestion(&mut db, params.workspace_id, params.suggestion_id).await?;
    let replies = match reviews::edit_thread_id(&suggestion, query.edit_id)? {
        Some(comment_id) => {
            state
                .comment_service
                .list_thread_replies(&mut db, comment_id)
                .await?
        }
        None => Vec::new(),
    };
    db.commit().await.map_err(AppError::from)?;
    Ok(Json(ResponseEnvelope::ok(RepliesDTO { replies })))
}

/// Reply in a suggestion's thread: the owner, the reviewer, or any other
/// reader of the source. The other side is notified.
async fn add_reply(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    Path(params): Path<SuggestionPathParams>,
    Json(body): Json<ReplyRequestDTO>,
) -> Result<(StatusCode, Json<ResponseEnvelope<RepliesDTO>>), AppError> {
    let content = body.content.trim().to_string();
    if content.is_empty() {
        return Err(AppError::from_error(G5Error::ValidationError {
            details: json!({"content": "reply must not be empty"}),
        }));
    }
    let db =
        scoped_db_for_workspace(&state, &auth, params.workspace_id, &["workspaces:read"]).await?;
    let (_, replies) = add_reply_as(
        &state,
        db,
        user.user_uuid(),
        params.workspace_id,
        params.suggestion_id,
        &content,
        body.edit_id,
    )
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(ResponseEnvelope::ok(RepliesDTO { replies })),
    ))
}

/// A reply in a suggestion's thread by `actor`, any reader of the source.
/// `content` is trimmed and non-empty. `db` is committed on success; the
/// suggestion and its thread after the reply are returned.
#[allow(clippy::too_many_arguments)] // Carries both actor and edit identity.
pub(crate) async fn add_reply_as(
    state: &Arc<AppState>,
    mut db: g5_authz::ScopedDb,
    actor: Uuid,
    workspace_id: Uuid,
    suggestion_id: Uuid,
    content: &str,
    edit_id: Option<Uuid>,
) -> Result<(ReviewSuggestionRecord, Vec<CommentReplyDTO>), AppError> {
    let org_id = db.primary_org_id().map_err(AppError::from)?.into_uuid();
    // The shared reply rule (`g5.documents.comments.replies`): any reader,
    // refused only on a read-only workspace. Read access is checked here
    // for every entry path, not only by the caller's route, and held until
    // the reply commits (`g5.security.current-authority`); the policy is
    // read under that hold, so a revocation or read-only transition waits
    // behind the write instead of landing between admission and commit.
    crate::db::scoped::ensure_user_workspace_access_until_commit(
        &state.system_pool,
        &mut db,
        actor,
        org_id,
        workspace_id,
    )
    .await?;
    crate::services::comment_service::ensure_thread_writes_allowed(
        &mut db,
        workspace_id,
        "workspaces.review_reply",
    )
    .await?;
    let (mut suggestion, publication) =
        reviews::lock_suggestion(&mut db, workspace_id, suggestion_id).await?;
    let thread = if let Some(id) = edit_id {
        Some(
            reviews::ensure_edit_thread(&mut db, workspace_id, &mut suggestion, &publication, id)
                .await?,
        )
    } else {
        suggestion.comment_id
    };
    let Some(comment_id) = thread else {
        return Err(AppError::from_error(G5Error::Conflict {
            reason: "this suggestion has no thread".to_string(),
        }));
    };
    reviews::add_thread_reply(&mut db, comment_id, actor, content).await?;
    let (owner_id, _) = reviews::workspace_owner_and_branch(&mut db, org_id, workspace_id).await?;
    let source_name = reviews::workspace_name(&mut db, org_id, workspace_id).await?;
    let actor_name = reviews::reviewer_display_name(&state.system_pool, actor).await?;
    // One notice per person: a reviewer who owns the source is one party.
    let mut recipients = vec![publication.reviewer_user_id];
    if let Some(owner_id) = owner_id.filter(|owner| *owner != publication.reviewer_user_id) {
        recipients.push(owner_id);
    }
    let excerpt: String = content.chars().take(140).collect();
    let reply_title = format!("{actor_name} replied on a suggestion");
    let reply_message = format!(
        "{} in {source_name}: {excerpt}",
        suggestion.path.as_deref().unwrap_or("(no file)")
    );
    for recipient in recipients.into_iter().filter(|id| *id != actor) {
        if recipient == publication.reviewer_user_id {
            notify_reviewer(
                state,
                &mut db,
                workspace_id,
                recipient,
                actor,
                &reply_title,
                &reply_message,
                vec![open_reviews_action_for(workspace_id, &source_name)],
            )
            .await?;
        } else {
            notify(
                state,
                &mut db,
                workspace_id,
                recipient,
                actor,
                "review",
                &reply_title,
                &reply_message,
                vec![open_reviews_action_for(workspace_id, &source_name)],
            )
            .await?;
        }
    }
    let replies = state
        .comment_service
        .list_thread_replies(&mut db, comment_id)
        .await?;
    db.commit().await.map_err(AppError::from)?;
    Ok((suggestion, replies))
}

// ─── Orchestration helpers ───────────────────────────────────────────────────

async fn review_dto(
    state: &Arc<AppState>,
    review: WorkspaceReviewRecord,
) -> Result<WorkspaceReviewDTO, AppError> {
    let reviewer_display_name =
        reviews::reviewer_display_name(&state.system_pool, review.reviewer_user_id).await?;
    Ok(WorkspaceReviewDTO {
        review,
        reviewer_display_name,
    })
}

fn open_reviews_action(review: &WorkspaceReviewRecord) -> NotificationAction {
    open_reviews_action_for(review.source_workspace_id, &review.source_workspace_name)
}

pub(crate) fn open_reviews_action_for(
    source_workspace_id: Uuid,
    source_name: &str,
) -> NotificationAction {
    NotificationAction {
        label: format!("Open reviews of {source_name}"),
        action: "navigate".to_string(),
        target: format!("workspace:{source_workspace_id}?view=reviews"),
    }
}

pub(crate) use crate::services::review_source_notifications::SourceHeadMove;

/// Tell every reviewer with a live review copy of `source_workspace_id`
/// that its branch moved (the owner committed, pulled or merged): see
/// [`ReviewSourceNotifier`]. Never fails the caller.
pub(crate) async fn notify_reviewers_of_source_head(
    state: &Arc<AppState>,
    org_id: Uuid,
    source_workspace_id: Uuid,
    moves: &[SourceHeadMove],
    actor_user_id: Uuid,
) {
    ReviewSourceNotifier::from_state(state)
        .notify_reviewers_of_source_head(org_id, source_workspace_id, moves, actor_user_id)
        .await;
}

/// [`notify_reviewers_of_source_head`] for a cut the server landed (a pull,
/// a merge, a pull request): every member whose branch ref moved counts.
pub(crate) async fn notify_reviewers_after_landing(
    state: &Arc<AppState>,
    landed: &LandedBranchCommit,
    actor_user_id: Uuid,
) {
    ReviewSourceNotifier::from_state(state)
        .notify_reviewers_after_landing(landed, actor_user_id)
        .await;
}

/// Tell a reviewer about a decision on one of their proposals. The notice
/// is filed in their live review copy of the source when they hold one (the
/// copy's inbox is what a returning reviewer reads), else in the source. It
/// speaks about the source, so a reviewer who can no longer open the source
/// is told nothing.
#[allow(clippy::too_many_arguments)]
async fn notify_reviewer(
    state: &Arc<AppState>,
    db: &mut g5_authz::ScopedDb,
    source_workspace_id: Uuid,
    reviewer_user_id: Uuid,
    actor_user_id: Uuid,
    title: &str,
    message: &str,
    actions: Vec<NotificationAction>,
) -> Result<(), AppError> {
    let org_id = db.primary_org_id().map_err(AppError::from)?.into_uuid();
    if !crate::db::scoped::user_can_access_workspace(
        &state.system_pool,
        db,
        reviewer_user_id,
        org_id,
        source_workspace_id,
    )
    .await?
    {
        tracing::debug!(
            source_workspace_id = %source_workspace_id,
            reviewer_user_id = %reviewer_user_id,
            "review decision notice skipped: reviewer no longer has access to the source"
        );
        return Ok(());
    }
    let inbox = reviews::find_review_for_source(db, org_id, source_workspace_id, reviewer_user_id)
        .await?
        .map(|review| review.review_workspace_id)
        .unwrap_or(source_workspace_id);
    notify(
        state,
        db,
        inbox,
        reviewer_user_id,
        actor_user_id,
        "review",
        title,
        message,
        actions,
    )
    .await
}

/// Notify `recipient_user_id` about review activity on `workspace_id`.
/// Recipients are re-authorized against the source workspace first: a
/// reviewer whose access was revoked keeps their publication rows, but must
/// not keep receiving reply excerpts, rejection reasons or new rounds.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn notify(
    state: &Arc<AppState>,
    db: &mut g5_authz::ScopedDb,
    workspace_id: Uuid,
    recipient_user_id: Uuid,
    actor_user_id: Uuid,
    category: &str,
    title: &str,
    message: &str,
    actions: Vec<NotificationAction>,
) -> Result<(), AppError> {
    // Nobody is notified of their own action.
    if recipient_user_id == actor_user_id {
        return Ok(());
    }
    let org_id = db.primary_org_id().map_err(AppError::from)?.into_uuid();
    if !crate::db::scoped::user_can_access_workspace(
        &state.system_pool,
        db,
        recipient_user_id,
        org_id,
        workspace_id,
    )
    .await?
    {
        tracing::debug!(
            workspace_id = %workspace_id,
            recipient_user_id = %recipient_user_id,
            "review notification skipped: recipient no longer has access"
        );
        return Ok(());
    }
    state
        .notification_service
        .create_notification(
            db,
            CreateNotificationRequest {
                workspace_id,
                recipient_user_id,
                actor_user_id: Some(actor_user_id),
                category: category.to_string(),
                severity: "info".to_string(),
                title: title.to_string(),
                message: message.to_string(),
                task_id: None,
                comment_id: None,
                reply_id: None,
                actions,
                expires_at: None,
            },
        )
        .await?;
    Ok(())
}

/// Fork `source_id` into a durable, reviewer-owned review workspace whose
/// merge target is the source. A branch-name collision in any member repo is
/// retried with a numbered name; each attempt has its own idempotency key so
/// an aborted attempt never shadows the next one.
async fn fork_review_workspace(
    state: &Arc<AppState>,
    auth: &AuthContext,
    org_id: Uuid,
    source_id: Uuid,
    reviewer_id: Uuid,
    source_name: &str,
    reviewer_display_name: &str,
) -> Result<Uuid, AppError> {
    // The generation counts the requester's retired review forks of this
    // source (aborted, or whose workspace is deleted or transferred), so this
    // fork gets a fresh operation identity instead of replaying one whose
    // workspace is gone or no longer theirs, while overlapping Starts that
    // both saw no usable review share one key and converge on one fork.
    let mut db = scoped_db_for_workspace(state, auth, source_id, &["workspaces:read"]).await?;
    let generation =
        reviews::next_review_fork_generation(&mut db, org_id, source_id, reviewer_id).await?;
    db.commit().await.map_err(AppError::from)?;

    // An unfinished review fork (provider verification pending, or cleanup
    // in progress) holds a claim under its recorded key and name. Resume that
    // exact operation: re-deriving the name from the current display name
    // would change the request digest and be refused as a reused key.
    let mut db = scoped_db_for_workspace(state, auth, source_id, &["workspaces:read"]).await?;
    let unfinished =
        reviews::find_unfinished_review_fork(&mut db, org_id, source_id, reviewer_id).await?;
    db.commit().await.map_err(AppError::from)?;
    if let Some(unfinished) = unfinished {
        let db = scoped_db_for_workspace(state, auth, source_id, &["workspaces:read"]).await?;
        let fork_scope = db.reopener();
        db.commit().await.map_err(AppError::from)?;
        let request = review_fork_request(
            source_id,
            source_name,
            unfinished.proposed_name.clone(),
            unfinished.idempotency_key.clone(),
            ForkSource::Branch,
        );
        let forked = match state
            .workspace_fork_service
            .scoped(fork_scope)
            .fork_workspace(org_id, source_id, reviewer_id, request)
            .await
        {
            Ok(forked) => forked,
            // An unfinished fork admitted before review copies forked from
            // the branch recorded the durable source in its request
            // digest; resuming it means resubmitting that request.
            Err(error) if is_reused_key(&error.inner) => {
                let db =
                    scoped_db_for_workspace(state, auth, source_id, &["workspaces:read"]).await?;
                let fork_scope = db.reopener();
                db.commit().await.map_err(AppError::from)?;
                let request = review_fork_request(
                    source_id,
                    source_name,
                    unfinished.proposed_name,
                    unfinished.idempotency_key,
                    ForkSource::Durable,
                );
                state
                    .workspace_fork_service
                    .scoped(fork_scope)
                    .fork_workspace(org_id, source_id, reviewer_id, request)
                    .await?
            }
            Err(error) => return Err(error),
        };
        return Ok(forked.workspace.id);
    }

    let mut last_conflict: Option<AppError> = None;
    for attempt in 1..=MAX_REVIEW_NAME_ATTEMPTS {
        let name = reviews::review_workspace_name(source_name, reviewer_display_name, attempt);
        let key = reviews::review_fork_idempotency_key(source_id, generation, attempt);
        let db = scoped_db_for_workspace(state, auth, source_id, &["workspaces:read"]).await?;
        let fork_scope = db.reopener();
        db.commit().await.map_err(AppError::from)?;
        let request = review_fork_request(
            source_id,
            source_name,
            name.clone(),
            key,
            ForkSource::Branch,
        );
        match state
            .workspace_fork_service
            .scoped(fork_scope)
            .fork_workspace(org_id, source_id, reviewer_id, request)
            .await
        {
            Ok(forked) => return Ok(forked.workspace.id),
            Err(error) if is_name_unavailable(&error.inner) => {
                tracing::info!(
                    source_workspace_id = %source_id,
                    name = %name,
                    attempt,
                    error = %error.inner,
                    "Review workspace name conflicted; trying the next name"
                );
                last_conflict = Some(error);
            }
            Err(error) => return Err(error),
        }
    }
    Err(last_conflict.unwrap_or_else(|| {
        AppError::from_error(G5Error::Conflict {
            reason: "could not choose a free review workspace name".to_string(),
        })
    }))
}

/// Save the reviewer's current work as a round: inside the running review
/// workspace every member's working tree is committed on the review branch
/// and the branch is pushed, so Push review and its preview always see the
/// reviewer's latest edits with no git step on their part. A stopped review
/// workspace is left alone: its published branches (what it saved before
/// stopping) are what gets reviewed, and the caller says so.
async fn save_review_round(
    state: &Arc<AppState>,
    auth: &AuthContext,
    review: &WorkspaceReviewRecord,
    reviewer_id: Uuid,
    message: &str,
) -> Result<ReviewSaveDTO, AppError> {
    // The copy's own mutation policy governs this save as it governs any
    // Changes action: a read-only or freezing copy is neither committed nor
    // pushed on the reviewer's behalf.
    let mut db = scoped_db_for_workspace(
        state,
        auth,
        review.review_workspace_id,
        &["workspaces:write"],
    )
    .await?;
    crate::services::workspace_mutation_policy::ensure_workspace_mutable(
        &mut db,
        review.review_workspace_id,
        "workspaces.review_save",
    )
    .await?;
    db.rollback().await.map_err(AppError::from)?;
    let commit = match state
        .workspace_runtime
        .workspace_changes_commit(review.review_workspace_id, reviewer_id, None, message)
        .await
    {
        Ok(outcome) => outcome,
        Err(G5Error::WorkspaceNotRunning) => {
            // Not running covers a workspace on its way up, and one whose
            // lease lapsed while its VM still holds unsaved edits: pushing
            // the mirror's old branches in either case would publish a
            // stale round. Only a workspace whose durable lifecycle state
            // is final (or that never had an incarnation) is "stopped".
            let live = state
                .workspace_runtime
                .durable_status(review.review_workspace_id)
                .await?
                .is_some_and(|status| !matches!(status.as_str(), "stopped" | "failed"));
            if live {
                return Err(AppError::from_error(G5Error::Conflict {
                    reason:
                        "your review workspace is not ready to save changes; try again in a moment"
                            .to_string(),
                }));
            }
            return Ok(ReviewSaveDTO {
                workspace: "stopped".to_string(),
                committed: false,
            });
        }
        Err(G5Error::WorkspaceStopping) => {
            return Err(AppError::from_error(G5Error::Conflict {
                reason: "your review workspace is stopping; try again in a moment".to_string(),
            }));
        }
        Err(error) => return Err(AppError::from_error(error)),
    };
    ensure_changes_succeeded(&commit, "save your changes")?;
    // The runtime flattens the module outcome: members sit at the top.
    let committed = commit["members"]
        .as_array()
        .map(|members| members.iter().any(|member| member["committed"] == true))
        .unwrap_or(false);
    let push = state
        .workspace_runtime
        .workspace_changes_action(review.review_workspace_id, reviewer_id, "push", None)
        .await
        .map_err(AppError::from_error)?;
    ensure_changes_succeeded(&push, "publish your changes")?;
    Ok(ReviewSaveDTO {
        workspace: "live".to_string(),
        committed,
    })
}

/// A `workspace.changes` outcome that did not succeed, as a conflict naming
/// the failing members and their errors.
pub(crate) fn ensure_changes_succeeded(outcome: &Value, what: &str) -> Result<(), AppError> {
    if outcome["status"] == "succeeded" {
        return Ok(());
    }
    let details: Vec<String> = outcome["members"]
        .as_array()
        .map(|members| {
            members
                .iter()
                .filter(|member| !member["errors"].is_null())
                .map(|member| {
                    let errors = member["errors"]
                        .as_array()
                        .map(|errors| {
                            errors
                                .iter()
                                .filter_map(Value::as_str)
                                .collect::<Vec<_>>()
                                .join(", ")
                        })
                        .unwrap_or_default();
                    let detail = member["detail"].as_str().unwrap_or_default();
                    if detail.is_empty() {
                        errors
                    } else {
                        format!("{errors}: {detail}")
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    let hint = if details.iter().any(|d| d.contains("INTERNAL_BRANCH_MOVED")) {
        "; the review branch moved underneath your copy (Update from the source, then push again)"
    } else {
        ""
    };
    Err(AppError::from_error(G5Error::Conflict {
        reason: format!(
            "could not {what} in your review workspace ({}){hint}",
            if details.is_empty() {
                "unknown reason".to_string()
            } else {
                details.join("; ")
            }
        ),
    }))
}

/// The fixed shape of every review fork request. Everything but the name and
/// key is constant, so a resumed operation's digest matches.
/// A review copy forks with `ForkSource::Branch`: the reviewer reviews the
/// owner's last commit that reached the source's branch, never unsaved
/// edits, which is what Update from the source rebases onto and what a
/// round records as its source head. An unfinished fork from before that
/// is resumed with the source it recorded.
fn review_fork_request(
    source_id: Uuid,
    source_name: &str,
    name: String,
    idempotency_key: String,
    source: ForkSource,
) -> ForkWorkspaceRequest {
    ForkWorkspaceRequest {
        name,
        description: Some(format!("Review workspace for {source_name}")),
        idempotency_key,
        read_only: false,
        merge_target_workspace_id: Some(source_id),
        chat_session_ids: Vec::new(),
        source,
        interrupt: false,
        issue: None,
        start_chat: None,
        uniquify_name: false,
        reverification: None,
    }
}

/// The fork service's refusal of an idempotency key whose recorded request
/// differs from the one resubmitted.
fn is_reused_key(error: &G5Error) -> bool {
    match error {
        G5Error::ValidationError { details } => {
            details.get("field").and_then(|field| field.as_str()) == Some("idempotencyKey")
        }
        _ => false,
    }
}

/// A fork refused because the proposed name is taken (in any member repo).
/// The fork service reports it as a validation error on the `name` field;
/// anything else is not a name problem and is not retried.
fn is_name_unavailable(error: &G5Error) -> bool {
    match error {
        G5Error::ValidationError { details } => {
            details.get("field").and_then(|field| field.as_str()) == Some("name")
        }
        _ => false,
    }
}

async fn resume_review_workspace(
    state: &Arc<AppState>,
    auth: &AuthContext,
    review_workspace_id: Uuid,
    reviewer_id: Uuid,
) -> Result<ResumeOutcome, AppError> {
    let scope = authorize_workspace_action(state, auth, review_workspace_id).await?;
    Ok(workspace_vm_service(state, scope)?
        .resume(
            review_workspace_id,
            reviewer_id,
            Uuid::new_v4(),
            StartKind::Resume,
        )
        .await?)
}

// ─── Preview and artifacts ───────────────────────────────────────────────────

/// What Push review would publish from this review workspace, without
/// publishing (nothing is stored). Lets the Push dialog list changed files
/// and let the reviewer tick which new files should enter the repository.
/// One previewed file's changes for a discard: repository, path, and each
/// change's index in the generated round with its key, in diff order.
type FileChanges = (Uuid, String, Vec<(usize, String)>);

/// The bytes a live file read carries, whatever the guest's encoding; `None`
/// for an encoding this code does not read.
fn live_file_bytes(
    current: &crate::routes::workspaces::WorkspaceFileContentDTO,
) -> Option<Vec<u8>> {
    match current.encoding.as_str() {
        "utf-8" => Some(current.content.as_bytes().to_vec()),
        "base64" => base64::engine::general_purpose::STANDARD
            .decode(current.content.as_bytes())
            .ok(),
        _ => None,
    }
}

/// Whether the copy's live file is still the one the round's changes were
/// computed from: byte for byte the saved tree's version. An editor or an
/// agent can change the file between the save and the read; then no key
/// describes what is there, and nothing is written or deleted.
fn live_matches_saved(
    state: &Arc<AppState>,
    storage: &g5_api_core::services::git::repo_store::ScopedRepoStorage,
    org_id: Uuid,
    current: &crate::routes::workspaces::WorkspaceFileContentDTO,
    repo_id: Uuid,
    review_head: Option<&str>,
    path: &str,
) -> bool {
    let Some(review_head) = review_head else {
        return false;
    };
    let Some(live) = live_file_bytes(current) else {
        return false;
    };
    let Ok(key) = storage.key(org_id, repo_id) else {
        return false;
    };
    match reviews::saved_copy_bytes(&state.repo_storage, &key, review_head, path) {
        Ok(saved) => saved == live,
        Err(_) => false,
    }
}

/// Why one file's write or delete failed, for the reviewer: a version
/// conflict means the file moved under the discard; anything else is the
/// runtime's own word. One file's failure never hides what the others did.
fn discard_failure_reason(path: &str, error: &AppError) -> String {
    match &error.inner {
        G5Error::FileVersionConflict { .. } => {
            format!("{path} changed in your copy while discarding; open Push review again")
        }
        other => format!("{path}: {other}"),
    }
}

/// The git blob id the runtime reports for a live file, without its bytes:
/// a one-byte window carries the whole file's hash, so an artifact of any
/// size answers within one reply. A runtime that cannot window the file (an
/// empty one has no byte to window) is asked plainly instead.
async fn live_file_hash(
    state: &Arc<AppState>,
    handle: &crate::services::workspace_runtime::WorkspaceRuntimeHandle,
    request: &crate::services::workspace_runtime::RuntimeRequest,
    repo_id: Uuid,
    path: &str,
) -> Result<String, AppError> {
    let hash_of = |value: &Value| {
        value
            .get("hash")
            .and_then(Value::as_str)
            .map(str::to_string)
    };
    match state
        .workspace_runtime
        .file_read_window(handle, request, repo_id, path, 0, 1)
        .await
    {
        Ok(value) => {
            if let Some(hash) = hash_of(&value) {
                return Ok(hash);
            }
        }
        Err(error) => {
            let mapped = crate::routes::workspaces::map_workspace_file_runtime_error(error);
            if matches!(mapped.inner, G5Error::NotFound { .. }) {
                return Err(mapped);
            }
        }
    }
    let value = state
        .workspace_runtime
        .file_read(handle, request, repo_id, path)
        .await
        .map_err(crate::routes::workspaces::map_workspace_file_runtime_error)?;
    hash_of(&value).ok_or_else(|| {
        AppError::from_error(G5Error::Internal {
            message: "workspace file reply omitted hash".to_string(),
        })
    })
}

const CHANGED_SINCE_PREVIEW: &str =
    "changed in your copy since the preview; open Push review again";

/// Lines of a stored hunk block (every line newline-terminated).
fn stored_line_count(block: &str) -> i64 {
    if block.is_empty() {
        0
    } else {
        block
            .strip_suffix('\n')
            .unwrap_or(block)
            .split('\n')
            .count() as i64
    }
}

/// Take previewed changes back out of the reviewer's copy: the reverse of
/// what the round proposes, written into the copy's running working tree
/// the way the owner's Accept writes the source's. A hunk is replaced by
/// the lines it changed; a new file or an artifact is deleted. The copy is
/// saved first, like a preview, so the keys the panel holds name the
/// changes of the current tree; a key the regenerated diff no longer has
/// refuses the whole request, like Push review.
async fn discard_review_changes(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
    body: Option<Json<DiscardReviewChangesRequestDTO>>,
) -> Result<Json<ResponseEnvelope<DiscardReviewChangesResponseDTO>>, AppError> {
    let reviewer_id = user.user_uuid();
    let review_workspace_id = params.workspace_id;
    let body = body.map(|Json(body)| body).unwrap_or_default();
    let mut requested: Vec<String> = Vec::new();
    for key in body.keys.iter().map(|key| key.trim()) {
        if !key.is_empty() && !requested.iter().any(|seen| seen == key) {
            requested.push(key.to_string());
        }
    }
    if requested.is_empty() {
        return Err(AppError::from_error(G5Error::ValidationError {
            details: json!({"keys": "name at least one change of the preview"}),
        }));
    }
    let mut db =
        scoped_db_for_workspace(&state, &auth, review_workspace_id, &["workspaces:write"]).await?;
    crate::services::workspace_mutation_policy::ensure_workspace_owner(
        &mut db,
        review_workspace_id,
    )
    .await?;
    let org_id = db.primary_org_id().map_err(AppError::from)?.into_uuid();
    let review = reviews::find_review_by_review_workspace(&mut db, org_id, review_workspace_id)
        .await?
        .ok_or_else(|| {
            AppError::from_error(G5Error::NotFound {
                resource: format!("review workspace {review_workspace_id}"),
            })
        })?;
    if review.reviewer_user_id != reviewer_id {
        return Err(AppError::from_error(G5Error::Forbidden {
            missing_scopes: Vec::new(),
        }));
    }
    let scope = db.reopener();
    let storage = state.repo_storage.scoped(&db);
    db.rollback().await.map_err(AppError::from)?;
    // The copy's VM must be running: a discard is a write into its working
    // tree, refused with `WORKSPACE_NOT_RUNNING` otherwise, before anything
    // is saved or diffed.
    let handle = state
        .workspace_runtime
        .owner_handle(review_workspace_id, reviewer_id)
        .await?;
    // One save at a time per (source, reviewer), like a preview or a push.
    let _admission =
        reviews::lock_review_admission(&state.system_pool, review.source_workspace_id, reviewer_id)
            .await?;
    save_review_round(
        &state,
        &auth,
        &review,
        reviewer_id,
        &format!(
            "Review of {}: work in progress",
            review.source_workspace_name
        ),
    )
    .await?;
    let mut db = scoped_db_for_workspace(
        &state,
        &auth,
        review.source_workspace_id,
        &["workspaces:read"],
    )
    .await?;
    let generated = reviews::generate_review_changes(
        &mut db,
        &state.repo_storage,
        &review,
        &reviews::GenerateOptions::default(),
    )
    .await?;
    db.rollback().await.map_err(AppError::from)?;
    let keys = reviews::change_keys(&generated.suggestions);
    let known: std::collections::HashSet<&str> = keys.iter().map(String::as_str).collect();
    let stale = requested
        .iter()
        .filter(|key| !known.contains(key.as_str()))
        .count();
    if stale > 0 {
        return Err(AppError::from_error(G5Error::Conflict {
            reason: format!(
                "{stale} of the previewed changes moved or changed since the preview; open Push review again"
            ),
        }));
    }
    let selected: std::collections::HashSet<&str> = requested.iter().map(String::as_str).collect();
    // The same policy as Save on the copy: a mutable workspace of the
    // reviewer's and one of its repositories.
    let mut db =
        scoped_db_for_workspace(&state, &auth, review_workspace_id, &["workspaces:write"]).await?;
    let mut repos_checked: Vec<Uuid> = Vec::new();
    for (item, key) in generated.suggestions.iter().zip(&keys) {
        if selected.contains(key.as_str()) && !repos_checked.contains(&item.repo_id) {
            crate::routes::workspaces::ensure_workspace_file_mutation_allowed(
                &mut db,
                review_workspace_id,
                reviewer_id,
                item.repo_id,
            )
            .await?;
            repos_checked.push(item.repo_id);
        }
    }
    db.rollback().await.map_err(AppError::from)?;

    // Per file, in generation order (the diff's order), every change of the
    // file with its key: the copy-side line of a hunk is its source-side
    // anchor shifted by the earlier hunks of the same file.
    let mut per_file: Vec<FileChanges> = Vec::new();
    for (index, (item, key)) in generated.suggestions.iter().zip(&keys).enumerate() {
        match per_file
            .iter_mut()
            .find(|(repo_id, path, _)| *repo_id == item.repo_id && *path == item.path)
        {
            Some((_, _, entries)) => entries.push((index, key.clone())),
            None => per_file.push((item.repo_id, item.path.clone(), vec![(index, key.clone())])),
        }
    }
    // The saved tree each repository's changes were computed from: a live
    // file is discarded only while it is still that version.
    let review_heads: HashMap<Uuid, String> = generated
        .base_commit_vector
        .iter()
        .filter_map(|entry| {
            let repo_id = entry.get("repoId").and_then(Value::as_str)?.parse().ok()?;
            let head = entry.get("reviewHead").and_then(Value::as_str)?.to_string();
            Some((repo_id, head))
        })
        .collect();
    let mut discarded: Vec<DiscardedChangeDTO> = Vec::new();
    let mut skipped: Vec<SkippedDiscardDTO> = Vec::new();
    for (repo_id, raw_path, entries) in per_file {
        let review_head = review_heads.get(&repo_id).map(String::as_str);
        if !entries
            .iter()
            .any(|(_, key)| selected.contains(key.as_str()))
        {
            continue;
        }
        let path = match crate::routes::files::sanitize_path(&raw_path) {
            Ok(path) => path,
            Err(error) => {
                for (_, key) in entries
                    .iter()
                    .filter(|(_, key)| selected.contains(key.as_str()))
                {
                    skipped.push(SkippedDiscardDTO {
                        key: key.clone(),
                        reason: format!("{raw_path}: {}", error.inner),
                    });
                }
                continue;
            }
        };
        let whole_file = entries
            .iter()
            .any(|(index, _)| generated.suggestions[*index].kind != "hunk");
        if whole_file {
            // A new file or an artifact: one change, the file itself.
            let (index, key) = &entries[0];
            let item = &generated.suggestions[*index];
            // Only the file's identity is needed, never its bytes: an
            // artifact can be far larger than one runtime reply. The git
            // blob id the runtime reports for the live file is compared
            // with the saved tree's, and the delete is guarded by that
            // same id, so a file replaced after the save is left alone.
            let read_request =
                crate::routes::workspaces::workspace_runtime_request(&handle, reviewer_id, "read");
            let live_hash =
                match live_file_hash(&state, &handle, &read_request, repo_id, &path).await {
                    Ok(hash) => hash,
                    Err(error) if matches!(error.inner, G5Error::NotFound { .. }) => {
                        skipped.push(SkippedDiscardDTO {
                            key: key.clone(),
                            reason: format!("{path} is no longer in your copy"),
                        });
                        continue;
                    }
                    // A read the runtime could not serve is this file's
                    // outcome too: the files already discarded stay reported.
                    Err(error) => {
                        skipped.push(SkippedDiscardDTO {
                            key: key.clone(),
                            reason: discard_failure_reason(&path, &error),
                        });
                        continue;
                    }
                };
            let saved_hash = review_head.and_then(|head| {
                let key = storage.key(org_id, repo_id).ok()?;
                reviews::saved_copy_blob_id(&state.repo_storage, &key, head, &path).ok()
            });
            if saved_hash.as_deref() != Some(live_hash.as_str()) {
                skipped.push(SkippedDiscardDTO {
                    key: key.clone(),
                    reason: format!("{path} {CHANGED_SINCE_PREVIEW}"),
                });
                continue;
            }
            let request = crate::routes::workspaces::workspace_runtime_request(
                &handle,
                reviewer_id,
                "delete",
            );
            let deleted = match state
                .workspace_runtime
                .workspace_changes(&scope, review_workspace_id, reviewer_id)
                .await
            {
                Ok(_) => state
                    .workspace_runtime
                    .file_delete(&handle, &request, repo_id, &path, Some(&live_hash))
                    .await
                    .map_err(crate::routes::workspaces::map_workspace_file_runtime_error),
                Err(error) => Err(AppError::from(error)),
            };
            match deleted {
                Ok(_) => discarded.push(DiscardedChangeDTO {
                    key: key.clone(),
                    repo_id,
                    path: path.clone(),
                    kind: item.kind.clone(),
                }),
                Err(error) => skipped.push(SkippedDiscardDTO {
                    key: key.clone(),
                    reason: discard_failure_reason(&path, &error),
                }),
            }
            continue;
        }
        let current = match crate::routes::workspaces::read_live_workspace_file(
            &state,
            &handle,
            reviewer_id,
            repo_id,
            &path,
        )
        .await
        {
            Ok(current) => current,
            Err(error) if matches!(error.inner, G5Error::NotFound { .. }) => {
                for (_, key) in entries
                    .iter()
                    .filter(|(_, key)| selected.contains(key.as_str()))
                {
                    skipped.push(SkippedDiscardDTO {
                        key: key.clone(),
                        reason: format!("{path} is no longer in your copy"),
                    });
                }
                continue;
            }
            Err(error) => {
                let reason = discard_failure_reason(&path, &error);
                for (_, key) in entries
                    .iter()
                    .filter(|(_, key)| selected.contains(key.as_str()))
                {
                    skipped.push(SkippedDiscardDTO {
                        key: key.clone(),
                        reason: reason.clone(),
                    });
                }
                continue;
            }
        };
        if current.encoding != "utf-8" {
            for (_, key) in entries
                .iter()
                .filter(|(_, key)| selected.contains(key.as_str()))
            {
                skipped.push(SkippedDiscardDTO {
                    key: key.clone(),
                    reason: format!("{path} is not a text file in your copy"),
                });
            }
            continue;
        }
        // The copy-side anchors below are exact only on the very file the
        // diff was cut from; a file that moved since the save is left alone
        // as a whole rather than patched at a guessed place.
        if !live_matches_saved(
            &state,
            &storage,
            org_id,
            &current,
            repo_id,
            review_head,
            &path,
        ) {
            for (_, key) in entries
                .iter()
                .filter(|(_, key)| selected.contains(key.as_str()))
            {
                skipped.push(SkippedDiscardDTO {
                    key: key.clone(),
                    reason: format!("{path} {CHANGED_SINCE_PREVIEW}"),
                });
            }
            continue;
        }
        // Copy-side anchors: each hunk's source-side line plus the growth of
        // the hunks before it in the file.
        let mut planned: Vec<(usize, String, Option<i32>)> = Vec::new();
        let mut offset: i64 = 0;
        for (index, key) in &entries {
            let item = &generated.suggestions[*index];
            let anchor = item
                .anchor_start
                .map(|line| (i64::from(line) + offset) as i32);
            if selected.contains(key.as_str()) {
                planned.push((*index, key.clone(), anchor));
            }
            offset += stored_line_count(&item.after_lines)
                - stored_line_count(item.before_lines.as_deref().unwrap_or_default());
        }
        // Later hunks first, so the anchors of the earlier ones stay valid
        // as the file shrinks or grows above them.
        planned.sort_by_key(|(_, _, anchor)| std::cmp::Reverse(*anchor));
        let mut content = current.content.clone();
        let mut written_any = false;
        // This file's outcomes, final only once its write succeeded.
        let mut file_discarded: Vec<DiscardedChangeDTO> = Vec::new();
        for (index, key, anchor) in planned {
            let item = &generated.suggestions[index];
            let original = item.before_lines.as_deref().unwrap_or_default();
            let context = reviews::AnchorContext::parse(item.anchor_context.as_deref());
            // The reverse hunk: the proposed text is what is looked for, the
            // original is what goes back. The live file is byte for byte the
            // saved tree the diff was cut from (checked above), so the
            // copy-side anchor is exact and trusted the way a published
            // anchor is on an unchanged file.
            match reviews::apply_hunk_with_context(
                &content,
                &item.after_lines,
                original,
                anchor,
                &context,
                true,
            ) {
                Ok(HunkApplication::Applied(next)) => {
                    content = next;
                    written_any = true;
                    file_discarded.push(DiscardedChangeDTO {
                        key,
                        repo_id,
                        path: path.clone(),
                        kind: item.kind.clone(),
                    });
                }
                Ok(HunkApplication::AlreadyApplied) => {
                    file_discarded.push(DiscardedChangeDTO {
                        key,
                        repo_id,
                        path: path.clone(),
                        kind: item.kind.clone(),
                    });
                }
                Err(HunkRefusal::TargetNotFound) => skipped.push(SkippedDiscardDTO {
                    key,
                    reason: format!("the proposed lines are no longer in {path}"),
                }),
                Err(HunkRefusal::Ambiguous) => skipped.push(SkippedDiscardDTO {
                    key,
                    reason: format!(
                        "the proposed lines appear more than once in {path}; take them out by hand"
                    ),
                }),
            }
        }
        if written_any {
            if let Err(error) = crate::routes::workspaces::write_live_workspace_file(
                &state,
                &scope,
                &handle,
                reviewer_id,
                repo_id,
                &path,
                &content,
                "utf-8",
                Some(&current.hash),
            )
            .await
            {
                let reason = discard_failure_reason(&path, &error);
                skipped.extend(file_discarded.into_iter().map(|item| SkippedDiscardDTO {
                    key: item.key,
                    reason: reason.clone(),
                }));
                continue;
            }
        }
        discarded.extend(file_discarded);
    }
    tracing::info!(
        review_workspace_id = %review_workspace_id,
        discarded = discarded.len(),
        skipped = skipped.len(),
        "Workspace endpoint succeeded: discard_review_changes"
    );
    Ok(Json(ResponseEnvelope::ok(
        DiscardReviewChangesResponseDTO { discarded, skipped },
    )))
}

async fn preview_review(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
) -> Result<Json<ResponseEnvelope<ReviewPreviewDTO>>, AppError> {
    let reviewer_id = user.user_uuid();
    let review_workspace_id = params.workspace_id;
    let mut db =
        scoped_db_for_workspace(&state, &auth, review_workspace_id, &["workspaces:write"]).await?;
    crate::services::workspace_mutation_policy::ensure_workspace_owner(
        &mut db,
        review_workspace_id,
    )
    .await?;
    let org_id = db.primary_org_id().map_err(AppError::from)?.into_uuid();
    let review = reviews::find_review_by_review_workspace(&mut db, org_id, review_workspace_id)
        .await?
        .ok_or_else(|| {
            AppError::from_error(G5Error::NotFound {
                resource: format!("review workspace {review_workspace_id}"),
            })
        })?;
    if review.reviewer_user_id != reviewer_id {
        return Err(AppError::from_error(G5Error::Forbidden {
            missing_scopes: Vec::new(),
        }));
    }
    db.rollback().await.map_err(AppError::from)?;
    // One save at a time per (source, reviewer), like a push: two previews,
    // or a preview under a push, would otherwise stage and commit in the
    // same checkout at once.
    let _admission =
        reviews::lock_review_admission(&state.system_pool, review.source_workspace_id, reviewer_id)
            .await?;
    // The preview is of the reviewer's current work: saved (committed and
    // pushed) from the running review workspace first, so what the dialog
    // lists is what Push review sends.
    let saved = save_review_round(
        &state,
        &auth,
        &review,
        reviewer_id,
        &format!(
            "Review of {}: work in progress",
            review.source_workspace_name
        ),
    )
    .await?;
    let mut db = scoped_db_for_workspace(
        &state,
        &auth,
        review.source_workspace_id,
        &["workspaces:read"],
    )
    .await?;
    let generated = reviews::generate_review_changes(
        &mut db,
        &state.repo_storage,
        &review,
        &reviews::GenerateOptions::default(),
    )
    .await?;
    // Which of these changes the reviewer's current round already carries:
    // the panel marks them and counts only the new ones.
    let source_heads =
        reviews::source_heads(&mut db, &state.repo_storage, review.source_workspace_id).await?;
    let generated_vector = Value::Array(generated.base_commit_vector.clone());
    let published = reviews::published_in_current_round(
        &mut db,
        &state.repo_storage,
        &generated_vector,
        review.source_workspace_id,
        reviewer_id,
        &generated.suggestions,
        &source_heads,
    )
    .await?;
    // And what a push would carry for each, whatever the source's commit:
    // the note editor shows that note so a re-push never republishes text
    // the reviewer cannot see or clear.
    let (carried, carried_edit_counts, carried_open_edit_counts) = reviews::carried_by_push(
        &mut db,
        &state.repo_storage,
        &generated_vector,
        review.source_workspace_id,
        reviewer_id,
        &generated.suggestions,
    )
    .await?;
    db.rollback().await.map_err(AppError::from)?;
    let mut files: Vec<ReviewPreviewFileDTO> = Vec::new();
    let keys = reviews::change_keys(&generated.suggestions);
    for (((((item, key), published_in), carried), carried_edit_count), carried_open_edit_count) in
        generated
            .suggestions
            .into_iter()
            .zip(keys)
            .zip(published)
            .zip(carried)
            .zip(carried_edit_counts)
            .zip(carried_open_edit_counts)
    {
        let file_key = (item.kind != "hunk").then(|| key.clone());
        let change = (item.kind != "artifact").then(|| ReviewPreviewChangeDTO {
            key,
            kind: item.kind.clone(),
            anchor_start: item.anchor_start,
            before_lines: item.before_lines.clone(),
            after_lines: item.after_lines.clone(),
            carried_edit_count,
            carried_open_edit_count,
            edit_count: if item.kind == "hunk" {
                crate::services::review_edits::discrete_edits(
                    Uuid::nil(),
                    item.before_lines.as_deref().unwrap_or_default(),
                    &item.after_lines,
                )
                .len()
            } else {
                1
            },
            published_in: published_in.clone(),
            carried: carried.clone(),
        });
        if let Some(existing) = files
            .iter_mut()
            .find(|file| file.repo_id == item.repo_id && file.path == item.path)
        {
            existing.hunks += 1;
            existing.changes.extend(change);
            continue;
        }
        files.push(ReviewPreviewFileDTO {
            repo_id: item.repo_id,
            path: item.path,
            hunks: usize::from(item.kind == "hunk"),
            bytes: item.bytes,
            // A file-level answer for the one-change kinds; a changed file
            // answers per hunk.
            published_in: (item.kind != "hunk").then_some(published_in).flatten(),
            carried: (item.kind != "hunk").then_some(carried).flatten(),
            key: file_key,
            kind: item.kind,
            changes: change.into_iter().collect(),
        });
    }
    Ok(Json(ResponseEnvelope::ok(ReviewPreviewDTO {
        files,
        skipped: generated.skipped,
        base_commit_vector: generated.base_commit_vector,
        saved,
    })))
}

/// The bytes of an artifact suggestion, for the owner to open or download.
async fn download_review_artifact(
    State(state): State<Arc<AppState>>,
    _user: AuthenticatedUser,
    auth: AuthContext,
    Path(params): Path<SuggestionPathParams>,
) -> Result<axum::response::Response, AppError> {
    let mut db =
        scoped_db_for_workspace(&state, &auth, params.workspace_id, &["workspaces:read"]).await?;
    let (suggestion, _) =
        reviews::get_suggestion(&mut db, params.workspace_id, params.suggestion_id).await?;
    let storage = state.repo_storage.scoped(&db);
    let org_id = db.primary_org_id().map_err(AppError::from)?.into_uuid();
    db.rollback().await.map_err(AppError::from)?;
    let (Some(repo_id), Some(oid)) = (suggestion.repo_id, suggestion.blob_oid.as_deref()) else {
        return Err(AppError::from_error(G5Error::NotFound {
            resource: format!("artifact for suggestion {}", params.suggestion_id),
        }));
    };
    if suggestion.kind != "artifact" {
        return Err(AppError::from_error(G5Error::ValidationError {
            details: json!({"kind": "only artifact suggestions carry bytes"}),
        }));
    }
    let bytes =
        reviews::read_review_artifact(&state.repo_storage, &storage.key(org_id, repo_id)?, oid)?;
    let content_type = suggestion
        .content_type
        .clone()
        .unwrap_or_else(|| "application/octet-stream".to_string());
    // The download is named after the file, not its directory.
    let file_name = suggestion
        .path
        .as_deref()
        .and_then(|path| path.rsplit('/').next())
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("artifact-{}", params.suggestion_id));
    let safe_name: String = file_name
        .chars()
        .map(|c| if c == '"' || c.is_control() { '_' } else { c })
        .collect();
    let inline = content_type.starts_with("text/html")
        || content_type.starts_with("image/")
        || content_type.starts_with("application/pdf");
    let response = axum::response::Response::builder()
        .status(StatusCode::OK)
        .header(axum::http::header::CONTENT_TYPE, content_type)
        .header(
            axum::http::header::CONTENT_DISPOSITION,
            format!(
                "{}; filename=\"{safe_name}\"",
                if inline { "inline" } else { "attachment" }
            ),
        )
        .header(axum::http::header::CACHE_CONTROL, "private, max-age=300")
        .header("X-Content-Type-Options", "nosniff")
        .header(
            axum::http::header::CONTENT_SECURITY_POLICY,
            "sandbox; default-src 'none'; img-src data: blob:; style-src 'unsafe-inline'",
        )
        .body(axum::body::Body::from(bytes))
        .map_err(|e| {
            AppError::from_error(G5Error::Internal {
                message: format!("building artifact response: {e}"),
            })
        })?;
    Ok(response)
}
