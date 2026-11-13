//! Workspace Activity: the primary external issue's discussion interleaved
//! with labelled G5 workspace context (docs/external-issue-integration-design.md,
//! "Activity and workspace review").
//!
//! Nothing new is persisted. Each read composes the issue's timeline (from
//! the tracker) with existing G5 records: chats, preview and
//! command tasks, approval decisions, phase moves, pull-request events and
//! each workspace branch's commits since its base as they stand in G5's git.
//! Entries are ordered by last activity — a conversation's last message, a
//! task's end else its start, the time every other record carries — with
//! their key as a deterministic tie-breaker; a page holds the most recent
//! entries before an optional cursor and is returned newest first.
//!
//! Ongoing work (ui `g5.ui.activity-current`) is the same projection read
//! without a cursor: conversations with a queued, running or waiting agent
//! task and preview/command tasks in those states.
//!
//! Every entry is a projection of a durable record (workbench
//! `g5.workbench.landing`): the feed never derives a time or an event the
//! database or Git does not hold.

use std::collections::HashMap;

use chrono::{DateTime, SecondsFormat, Utc};
use g5_authz::ScopedDb;
use g5_contracts::error::G5Error;
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::services::workspace::preview_run::{
    project_preview_run, PreviewRunEvents, PreviewRunProjection,
};

use crate::services::git::external_issues::{
    user_dto, ExternalIssueUserDTO, IssueDiscussion, LinkedIssueTimeline,
};
use crate::services::git::github_issues::{GithubIssue, GithubTimelineItem};
use crate::services::git::native_issues::NativeTimeline;
use crate::services::workspace::workspace_service::WorkspaceService;

/// Entries per page.
pub const ACTIVITY_PAGE_SIZE: usize = 50;

/// Position in the activity order: entries strictly before it are older.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivityCursor {
    pub at: DateTime<Utc>,
    pub key: String,
}

impl ActivityCursor {
    /// `<RFC 3339 time>|<entry key>`.
    #[must_use]
    pub fn encode(&self) -> String {
        format!(
            "{}|{}",
            self.at.to_rfc3339_opts(SecondsFormat::Micros, true),
            self.key
        )
    }

    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        let (at, key) = value.split_once('|')?;
        if key.is_empty() {
            return None;
        }
        Some(Self {
            at: DateTime::parse_from_rfc3339(at).ok()?.with_timezone(&Utc),
            key: key.to_string(),
        })
    }

    fn is_after(&self, entry: &ActivityEntryDTO) -> bool {
        (entry.at, entry.key.as_str()) < (self.at, self.key.as_str())
    }
}

/// Where an entry comes from: the shared tracker discussion or G5 context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivitySource {
    Github,
    /// G5's own tracker (`g5.issue.native`).
    G5,
    Workspace,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum ActivityEntryBody {
    /// The issue's description, which opens its discussion.
    IssueOpened {
        author: Option<ExternalIssueUserDTO>,
        title: String,
        body: Option<String>,
        url: String,
    },
    IssueComment {
        /// GitHub's comment id, for editing one's own comment.
        comment_id: String,
        author: Option<ExternalIssueUserDTO>,
        body: String,
        url: String,
        /// When GitHub last changed the comment; an edit must match it.
        updated_at: String,
    },
    IssueEvent {
        actor: Option<ExternalIssueUserDTO>,
        event: String,
        detail: Option<String>,
        state_reason: Option<String>,
    },
    Chat {
        session_id: Uuid,
        title: Option<String>,
        message_count: i32,
        last_activity_at: DateTime<Utc>,
        /// The agent task's status while the conversation is ongoing
        /// (`queued`, `running` or `waiting`); absent in the record.
        #[serde(skip_serializing_if = "Option::is_none")]
        task_status: Option<String>,
    },
    Task {
        task_id: Uuid,
        name: String,
        task_kind: String,
        status: String,
        /// Why the task ended, when its outcome says (for example a preview's
        /// `idle_stop`); absent while it runs or for an ordinary completion.
        reason: Option<String>,
        /// A preview run's profile, port, stage while live and structured
        /// ending once ended (ui `g5.ui.activity.preview-row`); absent for
        /// other task kinds.
        #[serde(skip_serializing_if = "Option::is_none")]
        preview: Option<PreviewRunProjection>,
    },
    /// A commit on a workspace branch since its base, as it stands in G5's
    /// git.
    Commit {
        repo_id: Uuid,
        repo_name: String,
        branch_name: String,
        sha: String,
        short_sha: String,
        summary: String,
        author_name: String,
    },
    WorkspaceComment {
        comment: serde_json::Value,
    },
    WorkspaceReply {
        comment_id: Uuid,
        content: String,
        actor_name: Option<String>,
        phase: Option<String>,
    },
    ApprovalRequested {
        cancelled: bool,
        request_id: Uuid,
        definition_name: String,
        note: Option<String>,
        phase: Option<String>,
        actor_name: Option<String>,
        reviewer_name: Option<String>,
    },
    WorkspaceCreated {
        actor_name: Option<String>,
    },
    ApprovalDecision {
        pull_request_id: Option<Uuid>,
        reviewed_git_heads: serde_json::Value,
        thread: Option<serde_json::Value>,
        definition_name: Option<String>,
        outcome: String,
        /// Phase the decision settled (`be` | `se` | `ie`, g5.workbench.gates).
        phase: String,
        actor_type: String,
        actor_name: Option<String>,
        /// A human's comment; always null for agent decisions, whose stored
        /// text is never shown as a report clip (ui
        /// `g5.workbench.landing.approval-events`).
        comment: Option<String>,
        /// The agent's authored summary, for schema 5+ reports only
        /// (`g5.agent-approval.evidence`); null otherwise.
        summary: Option<String>,
        /// A legacy human decision recorded without a request.
        historical: bool,
    },
    /// An owner moved the workspace's engineering phase: one row of
    /// `workspace_phase_events`, written in the PATCH transaction
    /// (workbench g5.workbench.phases.state).
    PhaseChanged {
        from_phase: String,
        to_phase: String,
        actor: Option<WorkspaceActorDTO>,
    },
    /// An owner marked a guided-journey step done or not done: one row of
    /// `workspace_journey_events` (workbench g5.workbench.journey.marks).
    JourneyStep {
        step_id: String,
        done: bool,
        /// The owner skipped the step (ui g5.ui.guided-panel); additive.
        #[serde(default)]
        skipped: bool,
        actor: Option<WorkspaceActorDTO>,
    },
    /// A lifecycle event of a pull request this workspace is the source of
    /// (`workspace_pull_request_events` joined to its pull request). Only
    /// reader-meaningful event types are projected; see
    /// [`PULL_REQUEST_EVENT_TYPES`].
    PullRequest {
        pull_request_id: Uuid,
        event_type: String,
        title: String,
        target_workspace_name: Option<String>,
        source_workspace_name: Option<String>,
        /// The pull request merges into this workspace rather than out of it:
        /// the target's history shows how incoming work landed
        /// ([`TARGET_PULL_REQUEST_EVENT_TYPES`]).
        incoming: bool,
        /// The event's own message (why a merge was blocked, a provider error).
        message: Option<String>,
        /// Repositories the pull request publishes to (0 while local only).
        repository_count: i64,
        /// Its provider PR, only when it spans exactly one repository:
        /// provider events name the aggregate PR, not the repository.
        provider_repo_full_name: Option<String>,
        provider_pr_number: Option<i64>,
        provider_pr_url: Option<String>,
        actor: Option<WorkspaceActorDTO>,
        /// For `provider_failed`: the failures since the PR's last completed
        /// publication, of which this entry is the newest (see
        /// [`PULL_REQUEST_EVENT_TYPES`]), and when the first of them happened.
        failure_count: Option<i64>,
        first_failed_at: Option<DateTime<Utc>>,
    },
}

/// The G5 user behind a workspace record, when it still resolves.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceActorDTO {
    pub user_id: Uuid,
    pub display_name: String,
}

/// Pull-request event types a reader of the timeline cares about. `refreshed`
/// is written on every Create-PR re-run and preflight recomputation and
/// `provider_updated` on every completed provider sync
/// (`workspace_pull_request_create::record_completed_sync`); both record that
/// a check ran, not that the pull request changed state, so they are left out.
///
/// `provider_failed` is shown only while unresolved: one entry per open pull
/// request, its newest failure since the last completed publication (a
/// `provider_updated` carrying source heads), with the count of those failures.
pub const PULL_REQUEST_EVENT_TYPES: [&str; 13] = [
    "created",
    "source_rebased",
    "merge_blocked",
    "merged",
    "closed",
    "provider_created",
    "provider_merged",
    "provider_action_required",
    "provider_failed",
    "auto_merge_enabled",
    "auto_merge_disabled",
    "merge_overridden",
    "provider_merged_outside",
];

/// Pull-request event types the target workspace's timeline shows for
/// pull requests merging into it: how incoming work landed, including a
/// merge with override (g5.workspace.merge.admin-override.formal). The rest
/// of an incoming pull request's lifecycle stays on its source.
pub const TARGET_PULL_REQUEST_EVENT_TYPES: [&str; 2] = ["merged", "merge_overridden"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityEntryDTO {
    /// Stable identity and ordering tie-breaker, e.g. `chat:<uuid>`.
    pub key: String,
    pub at: DateTime<Utc>,
    pub source: ActivitySource,
    #[serde(flatten)]
    pub body: ActivityEntryBody,
}

// ---------------------------------------------------------------------------
// Tracker entries
// ---------------------------------------------------------------------------

fn parse_time(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|time| time.with_timezone(&Utc))
}

/// Activity entries of a linked issue: its description, then its timeline.
#[must_use]
pub fn issue_entries(timeline: &LinkedIssueTimeline) -> Vec<ActivityEntryDTO> {
    match &timeline.discussion {
        IssueDiscussion::Github { issue, items } => github_issue_entries(issue, items),
        IssueDiscussion::Native(native) => native_issue_entries(native),
    }
}

fn github_issue_entries(
    issue: &GithubIssue,
    items: &[GithubTimelineItem],
) -> Vec<ActivityEntryDTO> {
    let opened = parse_time(&issue.created_at).map(|at| ActivityEntryDTO {
        key: format!("github-issue:{}", issue.id),
        at,
        source: ActivitySource::Github,
        body: ActivityEntryBody::IssueOpened {
            author: issue.author.clone().map(user_dto),
            title: issue.title.clone(),
            body: issue.body.clone().filter(|body| !body.trim().is_empty()),
            url: issue.html_url.clone(),
        },
    });
    opened
        .into_iter()
        .chain(items.iter().filter_map(|item| match item {
            GithubTimelineItem::Comment(comment) => Some(ActivityEntryDTO {
                key: format!("github-comment:{}", comment.id),
                at: parse_time(&comment.created_at)?,
                source: ActivitySource::Github,
                body: ActivityEntryBody::IssueComment {
                    comment_id: comment.id.clone(),
                    author: comment.author.clone().map(user_dto),
                    body: comment.body.clone(),
                    url: comment.html_url.clone(),
                    updated_at: comment.updated_at.clone(),
                },
            }),
            GithubTimelineItem::Event(event) => Some(ActivityEntryDTO {
                key: format!("github-event:{}", event.key),
                at: parse_time(&event.created_at)?,
                source: ActivitySource::Github,
                body: ActivityEntryBody::IssueEvent {
                    actor: event.actor.clone().map(user_dto),
                    event: event.event.clone(),
                    detail: event.detail.clone(),
                    state_reason: event.state_reason.clone(),
                },
            }),
        }))
        .collect()
}

/// The same entry kinds for a native issue, from G5's own records.
fn native_issue_entries(timeline: &NativeTimeline) -> Vec<ActivityEntryDTO> {
    let issue = &timeline.summary;
    let opened = parse_time(&issue.created_at).map(|at| ActivityEntryDTO {
        key: format!("g5-issue:{}", issue.external_issue_id),
        at,
        source: ActivitySource::G5,
        body: ActivityEntryBody::IssueOpened {
            author: issue.author.clone(),
            title: issue.title.clone(),
            body: timeline.body.clone(),
            url: issue.canonical_url.clone(),
        },
    });
    let comments = timeline.comments.iter().map(|comment| ActivityEntryDTO {
        key: format!("g5-comment:{}", comment.id),
        at: comment.created_at,
        source: ActivitySource::G5,
        body: ActivityEntryBody::IssueComment {
            comment_id: comment.id.to_string(),
            author: Some(comment.author.clone()),
            body: comment.body.clone(),
            url: comment.url.clone(),
            updated_at: crate::services::git::native_issues::wire_time(comment.updated_at),
        },
    });
    let events = timeline.events.iter().map(|event| ActivityEntryDTO {
        key: format!("g5-event:{}", event.id),
        at: event.created_at,
        source: ActivitySource::G5,
        body: ActivityEntryBody::IssueEvent {
            actor: Some(event.actor.clone()),
            event: event.event.clone(),
            detail: event.detail.clone(),
            state_reason: event.state_reason.clone(),
        },
    });
    opened.into_iter().chain(comments).chain(events).collect()
}

// ---------------------------------------------------------------------------
// Workspace context
// ---------------------------------------------------------------------------

fn db_error(context: &str, error: &sqlx::Error) -> G5Error {
    G5Error::Internal {
        message: format!("workspace activity {context}: {error}"),
    }
}

#[derive(sqlx::FromRow)]
struct CommentReplyEventRow {
    id: Uuid,
    created_at: DateTime<Utc>,
    author_id: Uuid,
    comment_id: Uuid,
    content: String,
    phase: Option<String>,
}

#[derive(sqlx::FromRow)]
struct ApprovalRequestEventRow {
    id: Uuid,
    at: DateTime<Utc>,
    requested_by: Option<Uuid>,
    actor_user_id: Option<Uuid>,
    name: String,
    note: Option<String>,
    cancelled: bool,
}

/// Up to `limit` of each G5 source before `before`, newest first per source.
/// The caller's scoped transaction has already authorized the workspace.
pub async fn workspace_entries(
    db: &mut ScopedDb,
    system_pool: &PgPool,
    workspace_id: Uuid,
    before: Option<&ActivityCursor>,
    limit: usize,
) -> Result<Vec<ActivityEntryDTO>, G5Error> {
    // Row comparison `(at, key) < (cursor)` keeps pages exact when entries
    // share a timestamp.
    let (before_at, before_key) = match before {
        Some(cursor) => (Some(cursor.at), Some(cursor.key.clone())),
        None => (None, None),
    };
    let limit = i64::try_from(limit).unwrap_or(i64::MAX);
    let mut entries = Vec::new();

    // Canonical discussion records are projected, never copied into a timeline
    // store. The same immutable identity opens the thread in the file view.
    let comments: Vec<(Uuid, DateTime<Utc>, Uuid, serde_json::Value)> = sqlx::query_as(
        "SELECT id,created_at,author_id,jsonb_build_object('id',id,'workspaceId',workspace_id,'authorId',author_id,'scope',scope,'repoId',repo_id,'filePath',file_path,'lineNumber',line_number,'commitSha',commit_sha,'content',content,'resolved',resolved,'phase',phase,'createdAt',created_at) FROM comment_index WHERE workspace_id=$1 AND scope IN ('line','workspace') AND NOT EXISTS(SELECT 1 FROM approval_statuses s WHERE s.comment_id=comment_index.id) AND ($2::timestamptz IS NULL OR (created_at,'comment:'||id::text)<($2,$3)) ORDER BY created_at DESC,id DESC LIMIT $4"
    ).bind(workspace_id).bind(before_at).bind(&before_key).bind(limit).fetch_all(db.conn()).await?;
    let replies: Vec<CommentReplyEventRow> = sqlx::query_as(
        "SELECT r.id,r.created_at,r.author_id,r.comment_id,r.content,c.phase FROM comment_replies r JOIN comment_index c ON c.id=r.comment_id WHERE c.workspace_id=$1 AND c.scope IN ('line','workspace') AND ($2::timestamptz IS NULL OR (r.created_at,'reply:'||r.id::text)<($2,$3)) ORDER BY r.created_at DESC,r.id DESC LIMIT $4"
    ).bind(workspace_id).bind(before_at).bind(&before_key).bind(limit).fetch_all(db.conn()).await?;
    let requests: Vec<ApprovalRequestEventRow> = sqlx::query_as(
        "SELECT r.id,e.at,r.requested_by,r.actor_user_id,d.name,r.note,e.cancelled FROM approval_requests r JOIN approval_definitions d ON d.org_id=r.org_id AND d.id=r.approval_definition_id AND d.version=r.approval_definition_version CROSS JOIN LATERAL (VALUES(r.created_at,false),(CASE WHEN r.state='cancelled' THEN r.closed_at END,true)) e(at,cancelled) WHERE r.source_workspace_id=$1 AND (r.pull_request_id IS NOT NULL OR r.without_pull_request) AND r.actor_type='human' AND e.at IS NOT NULL AND ($2::timestamptz IS NULL OR (e.at,(CASE WHEN e.cancelled THEN 'cancel:' ELSE 'request:' END)||r.id::text)<($2,$3)) ORDER BY e.at DESC,r.id DESC LIMIT $4"
    ).bind(workspace_id).bind(before_at).bind(&before_key).bind(limit).fetch_all(db.conn()).await?;
    let identities = comments
        .iter()
        .map(|c| c.2)
        .chain(replies.iter().map(|r| r.author_id))
        .chain(
            requests
                .iter()
                .flat_map(|r| [r.requested_by, r.actor_user_id].into_iter().flatten()),
        )
        .collect();
    let discussion_names = display_names(system_pool, identities).await?;
    for (id, at, author, mut comment) in comments {
        comment["authorName"] = serde_json::json!(discussion_names.get(&author));
        entries.push(ActivityEntryDTO {
            key: format!("comment:{id}"),
            at,
            source: ActivitySource::Workspace,
            body: ActivityEntryBody::WorkspaceComment { comment },
        });
    }
    for CommentReplyEventRow {
        id,
        created_at: at,
        author_id: author,
        comment_id,
        content,
        phase,
    } in replies
    {
        entries.push(ActivityEntryDTO {
            key: format!("reply:{id}"),
            at,
            source: ActivitySource::Workspace,
            body: ActivityEntryBody::WorkspaceReply {
                comment_id,
                content,
                actor_name: discussion_names.get(&author).cloned(),
                phase,
            },
        });
    }
    for ApprovalRequestEventRow {
        id: request_id,
        at,
        requested_by: author,
        actor_user_id: recipient,
        name: definition_name,
        note,
        cancelled,
    } in requests
    {
        entries.push(ActivityEntryDTO {
            key: format!(
                "{}:{request_id}",
                if cancelled { "cancel" } else { "request" }
            ),
            at,
            source: ActivitySource::Workspace,
            body: ActivityEntryBody::ApprovalRequested {
                cancelled,
                request_id,
                definition_name,
                phase: None,
                note,
                actor_name: author.and_then(|id| discussion_names.get(&id).cloned()),
                reviewer_name: recipient.and_then(|id| discussion_names.get(&id).cloned()),
            },
        });
    }
    let created: Option<(DateTime<Utc>,Option<Uuid>)> = sqlx::query_as("SELECT created_at,created_by FROM workspaces WHERE id=$1 AND ($2::timestamptz IS NULL OR (created_at,'created:'||id::text)<($2,$3))")
        .bind(workspace_id).bind(before_at).bind(&before_key).fetch_optional(db.conn()).await?;
    if let Some((at, author)) = created {
        let names = display_names(system_pool, author.into_iter().collect()).await?;
        entries.push(ActivityEntryDTO {
            key: format!("created:{workspace_id}"),
            at,
            source: ActivitySource::Workspace,
            body: ActivityEntryBody::WorkspaceCreated {
                actor_name: author.and_then(|id| names.get(&id).cloned()),
            },
        });
    }

    // A conversation sits at its last message (`updated_at`, bumped on every
    // turn), so a continued conversation moves up the record.
    let chats: Vec<ChatRow> = sqlx::query_as(
        r#"
        SELECT id, title, message_count, updated_at, NULL::text AS task_status
        FROM chat_sessions
        WHERE workspace_id = $1
          AND ($2::timestamptz IS NULL OR (updated_at, 'chat:' || id::text) < ($2, $3))
        ORDER BY updated_at DESC, id DESC
        LIMIT $4
        "#,
    )
    .bind(workspace_id)
    .bind(before_at)
    .bind(&before_key)
    .bind(limit)
    .fetch_all(db.conn())
    .await
    .map_err(|error| db_error("chat query", &error))?;
    entries.extend(chats.into_iter().map(chat_entry));

    // Preview and command tasks; agent turns are represented by their chats.
    let tasks: Vec<TaskRow> = sqlx::query_as(&format!(
        r#"
        SELECT id, name, kind, status, reason, at, spec, outcome, started, ready FROM (
            {TASK_ROW_SELECT}
            WHERE workspace_id = $1 AND kind <> 'agent'
        ) AS task
        WHERE $2::timestamptz IS NULL OR (at, 'task:' || id::text) < ($2, $3)
        ORDER BY at DESC, id DESC
        LIMIT $4
        "#
    ))
    .bind(workspace_id)
    .bind(before_at)
    .bind(&before_key)
    .bind(limit)
    .fetch_all(db.conn())
    .await
    .map_err(|error| db_error("task query", &error))?;
    entries.extend(tasks.into_iter().map(task_entry));

    let decisions: Vec<ApprovalDecisionRow> = sqlx::query_as(
        r#"
        SELECT decision.id, decision.created_at, decision.outcome, decision.phase, decision.pull_request_id,
               decision.actor_type, decision.actor_user_id, decision.comment, decision.reviewed_git_heads,
               definition.name AS definition_name,
               -- Only a schema 5+ report's stored text is an authored summary.
               (decision.actor_type = 'agent'
                AND (review.invocation #>> '{resolution,schemaVersion}')::int >= 5) AS authored_summary,
               (decision.actor_type = 'human' AND decision.request_id IS NULL) AS historical,
               CASE WHEN c.id IS NOT NULL THEN jsonb_build_object('id',c.id,'workspaceId',c.workspace_id,'authorId',c.author_id,'scope',c.scope,'content',c.content,'phase',c.phase,'createdAt',c.created_at,'resolved',c.resolved) END AS thread
        FROM approval_statuses AS decision
        LEFT JOIN comment_index c ON c.id=decision.comment_id
        LEFT JOIN agent_approval_reviews AS review ON review.id = decision.agent_review_id
        LEFT JOIN approval_definitions AS definition
          ON definition.id = decision.approval_definition_id
         AND definition.version = decision.approval_definition_version
        WHERE decision.source_workspace_id = $1
          AND ($2::timestamptz IS NULL
               OR (decision.created_at, 'approval:' || decision.id::text) < ($2, $3))
        ORDER BY decision.created_at DESC, decision.id DESC
        LIMIT $4
        "#,
    )
    .bind(workspace_id)
    .bind(before_at)
    .bind(&before_key)
    .bind(limit)
    .fetch_all(db.conn())
    .await
    .map_err(|error| db_error("approval decision query", &error))?;

    // Phase moves (workbench g5.workbench.phases.state), one entry per
    // durable event row.
    let phase_events: Vec<PhaseEventRow> = sqlx::query_as(
        r#"
        SELECT id, from_phase, to_phase, actor_user_id, created_at
        FROM workspace_phase_events
        WHERE workspace_id = $1
          AND ($2::timestamptz IS NULL OR (created_at, 'phase:' || id::text) < ($2, $3))
        ORDER BY created_at DESC, id DESC
        LIMIT $4
        "#,
    )
    .bind(workspace_id)
    .bind(before_at)
    .bind(&before_key)
    .bind(limit)
    .fetch_all(db.conn())
    .await
    .map_err(|error| db_error("phase event query", &error))?;

    // Pull requests this workspace is the source of. Provider lifecycle
    // events are written per repository but carry only the aggregate PR
    // identity, so a provider PR can be named on the row only when the PR
    // spans exactly one repository; a multi-repository PR shows its
    // repository count and leaves the provider fields null. The GitHub
    // adapter names them (`g5.git.pr-adapter`).
    let mut pull_request_events: Vec<PullRequestEventRow> = sqlx::query_as(
        r#"
        SELECT event.id, event.event_type, event.actor_id, event.message, event.created_at,
               pr.id AS pull_request_id, pr.title,
               target.name AS target_workspace_name,
               source.name AS source_workspace_name,
               pr.source_workspace_id <> $1 AS incoming,
               failures.failure_count, failures.first_failed_at
        FROM workspace_pull_request_events AS event
        JOIN workspace_pull_requests AS pr ON pr.id = event.pull_request_id
        LEFT JOIN workspaces AS target ON target.id = pr.target_workspace_id
        LEFT JOIN workspaces AS source ON source.id = pr.source_workspace_id
        -- The pull request's last completed publication bounds its failure run.
        LEFT JOIN LATERAL (
            SELECT completed.created_at, completed.id
            FROM workspace_pull_request_events AS completed
            WHERE event.event_type = 'provider_failed'
              AND completed.pull_request_id = event.pull_request_id
              AND completed.event_type = 'provider_updated'
              AND completed.source_heads IS NOT NULL
            ORDER BY completed.created_at DESC, completed.id DESC
            LIMIT 1
        ) AS last_completed ON TRUE
        LEFT JOIN LATERAL (
            SELECT COUNT(*) AS failure_count, MIN(failed.created_at) AS first_failed_at
            FROM workspace_pull_request_events AS failed
            WHERE event.event_type = 'provider_failed'
              AND failed.pull_request_id = event.pull_request_id
              AND failed.event_type = 'provider_failed'
              AND (last_completed.id IS NULL
                   OR (failed.created_at, failed.id)
                      > (last_completed.created_at, last_completed.id))
        ) AS failures ON TRUE
        WHERE ((pr.source_workspace_id = $1 AND event.event_type = ANY($5))
               OR (pr.target_workspace_id = $1 AND event.event_type = ANY($6)))
          -- A later failure supersedes a failure; a completed publication or
          -- the pull request's end resolves it.
          AND (event.event_type <> 'provider_failed' OR (
              pr.status = 'open'
              AND NOT EXISTS (
                  SELECT 1 FROM workspace_pull_request_events AS later
                  WHERE later.pull_request_id = event.pull_request_id
                    AND (later.created_at, later.id) > (event.created_at, event.id)
                    AND (later.event_type = 'provider_failed'
                         OR (later.event_type = 'provider_updated'
                             AND later.source_heads IS NOT NULL))
              )
          ))
          AND ($2::timestamptz IS NULL
               OR (event.created_at, 'pull-request:' || event.id::text) < ($2, $3))
        ORDER BY event.created_at DESC, event.id DESC
        LIMIT $4
        "#,
    )
    .bind(workspace_id)
    .bind(before_at)
    .bind(&before_key)
    .bind(limit)
    .bind(&PULL_REQUEST_EVENT_TYPES[..])
    .bind(&TARGET_PULL_REQUEST_EVENT_TYPES[..])
    .fetch_all(db.conn())
    .await
    .map_err(|error| db_error("pull request event query", &error))?;
    let pull_request_ids = pull_request_events
        .iter()
        .map(|event| event.pull_request_id)
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let labels = crate::services::git::pull_request_adapters::github_publication_labels(
        db.conn(),
        &pull_request_ids,
    )
    .await?;
    for event in &mut pull_request_events {
        let label = labels
            .get(&event.pull_request_id)
            .cloned()
            .unwrap_or_default();
        event.repository_count = label.repository_count;
        event.provider_repo_full_name = label.repository;
        event.provider_pr_number = label.number;
        event.provider_pr_url = label.url;
    }

    // Guided-journey marks (workbench g5.workbench.journey.marks), one entry
    // per durable event row.
    let journey_events: Vec<JourneyEventRow> = sqlx::query_as(
        r#"
        SELECT id, step_id, done, skipped, actor_user_id, created_at
        FROM workspace_journey_events
        WHERE workspace_id = $1
          AND ($2::timestamptz IS NULL OR (created_at, 'journey:' || id::text) < ($2, $3))
        ORDER BY created_at DESC, id DESC
        LIMIT $4
        "#,
    )
    .bind(workspace_id)
    .bind(before_at)
    .bind(&before_key)
    .bind(limit)
    .fetch_all(db.conn())
    .await
    .map_err(|error| db_error("journey event query", &error))?;

    let names = display_names(
        system_pool,
        decisions
            .iter()
            .filter_map(|decision| decision.actor_user_id)
            .chain(phase_events.iter().filter_map(|event| event.actor_user_id))
            .chain(
                journey_events
                    .iter()
                    .filter_map(|event| event.actor_user_id),
            )
            .chain(
                pull_request_events
                    .iter()
                    .filter_map(|event| event.actor_id),
            )
            .collect(),
    )
    .await?;
    let actor = |id: Option<Uuid>| {
        let user_id = id?;
        Some(WorkspaceActorDTO {
            user_id,
            display_name: names.get(&user_id)?.clone(),
        })
    };

    entries.extend(phase_events.into_iter().map(|event| ActivityEntryDTO {
        key: format!("phase:{}", event.id),
        at: event.created_at,
        source: ActivitySource::Workspace,
        body: ActivityEntryBody::PhaseChanged {
            from_phase: event.from_phase,
            to_phase: event.to_phase,
            actor: actor(event.actor_user_id),
        },
    }));
    entries.extend(journey_events.into_iter().map(|event| ActivityEntryDTO {
        key: format!("journey:{}", event.id),
        at: event.created_at,
        source: ActivitySource::Workspace,
        body: ActivityEntryBody::JourneyStep {
            step_id: event.step_id,
            done: event.done,
            skipped: event.skipped,
            actor: actor(event.actor_user_id),
        },
    }));
    entries.extend(
        pull_request_events
            .into_iter()
            .map(|event| ActivityEntryDTO {
                key: format!("pull-request:{}", event.id),
                at: event.created_at,
                source: ActivitySource::Workspace,
                body: ActivityEntryBody::PullRequest {
                    pull_request_id: event.pull_request_id,
                    event_type: event.event_type,
                    title: event.title,
                    target_workspace_name: event.target_workspace_name,
                    source_workspace_name: event.source_workspace_name,
                    incoming: event.incoming,
                    message: event.message,
                    repository_count: event.repository_count,
                    provider_repo_full_name: event.provider_repo_full_name,
                    provider_pr_number: event.provider_pr_number,
                    provider_pr_url: event.provider_pr_url,
                    actor: actor(event.actor_id),
                    failure_count: event.first_failed_at.and(Some(event.failure_count)),
                    first_failed_at: event.first_failed_at,
                },
            }),
    );
    entries.extend(decisions.into_iter().map(|decision| {
        let agent = decision.actor_type == "agent";
        let summary = decision
            .comment
            .clone()
            .filter(|_| agent && decision.authored_summary == Some(true));
        ActivityEntryDTO {
            key: format!("approval:{}", decision.id),
            at: decision.created_at,
            source: ActivitySource::Workspace,
            body: ActivityEntryBody::ApprovalDecision {
                pull_request_id: decision.pull_request_id,
                reviewed_git_heads: decision.reviewed_git_heads,
                thread: decision.thread,
                definition_name: decision.definition_name,
                outcome: decision.outcome,
                phase: decision.phase,
                actor_name: decision
                    .actor_user_id
                    .and_then(|id| names.get(&id).cloned()),
                actor_type: decision.actor_type,
                comment: decision.comment.filter(|_| !agent),
                summary,
                historical: decision.historical.unwrap_or(false),
            },
        }
    }));

    Ok(entries)
}

/// `(id, title, message_count, updated_at, task_status)`.
type ChatRow = (Uuid, Option<String>, i32, DateTime<Utc>, Option<String>);
/// id, name, kind, status, outcome reason, at, spec, outcome, and for a
/// preview whether its module started and reported ready.
type TaskRow = (
    Uuid,
    String,
    String,
    String,
    Option<String>,
    DateTime<Utc>,
    Option<serde_json::Value>,
    Option<serde_json::Value>,
    bool,
    bool,
);

/// The task columns both task reads select: the outcome's reason and the
/// end-else-start time for every kind, and for previews the two lifecycle
/// facts the preview dialog also reads (`g5.ui.activity.preview-row`).
const TASK_ROW_SELECT: &str = r#"
            SELECT id, name, kind, status, outcome ->> 'reason' AS reason,
                   COALESCE(ended_at, created_at) AS at, spec, outcome,
                   (kind = 'preview' AND EXISTS(
                       SELECT 1 FROM task_events event
                       WHERE event.task_id = tasks.id
                         AND (event.metadata ->> 'kind' = 'invocation.started'
                              OR event.metadata ->> 'kind' LIKE 'module.%'))) AS started,
                   (kind = 'preview' AND EXISTS(
                       SELECT 1 FROM task_events event
                       WHERE event.task_id = tasks.id
                         AND event.metadata ->> 'kind' = 'module.preview.ready')) AS ready
            FROM tasks"#;

fn chat_entry(
    (session_id, title, message_count, updated_at, task_status): ChatRow,
) -> ActivityEntryDTO {
    ActivityEntryDTO {
        key: format!("chat:{session_id}"),
        at: updated_at,
        source: ActivitySource::Workspace,
        body: ActivityEntryBody::Chat {
            session_id,
            title,
            message_count,
            last_activity_at: updated_at,
            task_status,
        },
    }
}

fn task_entry(
    (task_id, name, task_kind, status, reason, at, spec, outcome, started, ready): TaskRow,
) -> ActivityEntryDTO {
    let preview = (task_kind == "preview").then(|| {
        project_preview_run(
            &status,
            spec.as_ref().unwrap_or(&serde_json::Value::Null),
            outcome.as_ref(),
            PreviewRunEvents { started, ready },
        )
    });
    ActivityEntryDTO {
        key: format!("task:{task_id}"),
        at,
        source: ActivitySource::Workspace,
        body: ActivityEntryBody::Task {
            task_id,
            name,
            task_kind,
            status,
            reason,
            preview,
        },
    }
}

/// Ongoing work (ui `g5.ui.activity-current`), newest last activity first:
/// every conversation whose agent task is queued, running or waiting (at
/// most one per session, `tasks_one_active_agent_per_session_idx`) and every
/// preview or command task in one of those states. No cursor: the set is
/// bounded by the workspace's execution slots. The caller's scoped
/// transaction has already authorized the workspace.
pub async fn current_entries(
    db: &mut ScopedDb,
    workspace_id: Uuid,
) -> Result<Vec<ActivityEntryDTO>, G5Error> {
    let chats: Vec<ChatRow> = sqlx::query_as(
        r#"
        SELECT session.id, session.title, session.message_count, session.updated_at,
               task.status AS task_status
        FROM chat_sessions AS session
        JOIN tasks AS task
          ON task.chat_session_id = session.id
         AND task.kind = 'agent'
         AND task.status IN ('queued', 'running', 'waiting')
        WHERE session.workspace_id = $1
        ORDER BY session.updated_at DESC, session.id DESC
        "#,
    )
    .bind(workspace_id)
    .fetch_all(db.conn())
    .await
    .map_err(|error| db_error("current chat query", &error))?;
    let tasks: Vec<TaskRow> = sqlx::query_as(&format!(
        r#"
        SELECT id, name, kind, status, reason, at, spec, outcome, started, ready FROM (
            {TASK_ROW_SELECT}
            WHERE workspace_id = $1 AND kind <> 'agent'
              AND status IN ('queued', 'running', 'waiting')
        ) AS task
        ORDER BY at DESC, id DESC
        "#
    ))
    .bind(workspace_id)
    .fetch_all(db.conn())
    .await
    .map_err(|error| db_error("current task query", &error))?;
    let mut entries: Vec<ActivityEntryDTO> = chats
        .into_iter()
        .map(chat_entry)
        .chain(tasks.into_iter().map(task_entry))
        .collect();
    entries.sort_by(|left, right| (right.at, &right.key).cmp(&(left.at, &left.key)));
    Ok(entries)
}

/// Display names of the given users, read outside the tenant scope because
/// `users` is not tenant-scoped; ids that no longer resolve are absent.
async fn display_names(
    system_pool: &PgPool,
    mut ids: Vec<Uuid>,
) -> Result<HashMap<Uuid, String>, G5Error> {
    ids.sort_unstable();
    ids.dedup();
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    sqlx::query_as::<_, (Uuid, String)>("SELECT id, display_name FROM users WHERE id = ANY($1)")
        .bind(&ids)
        .fetch_all(system_pool)
        .await
        .map(|rows| rows.into_iter().collect())
        .map_err(|error| db_error("actor names", &error))
}

#[derive(Debug, sqlx::FromRow)]
struct PhaseEventRow {
    id: Uuid,
    from_phase: String,
    to_phase: String,
    actor_user_id: Option<Uuid>,
    created_at: DateTime<Utc>,
}

#[derive(Debug, sqlx::FromRow)]
struct JourneyEventRow {
    id: Uuid,
    step_id: String,
    done: bool,
    skipped: bool,
    actor_user_id: Option<Uuid>,
    created_at: DateTime<Utc>,
}

#[derive(Debug, sqlx::FromRow)]
struct PullRequestEventRow {
    id: Uuid,
    event_type: String,
    actor_id: Option<Uuid>,
    message: Option<String>,
    created_at: DateTime<Utc>,
    pull_request_id: Uuid,
    title: String,
    target_workspace_name: Option<String>,
    source_workspace_name: Option<String>,
    incoming: bool,
    /// Filled from the GitHub adapter's labels after the query.
    #[sqlx(skip)]
    repository_count: i64,
    #[sqlx(skip)]
    provider_repo_full_name: Option<String>,
    #[sqlx(skip)]
    provider_pr_number: Option<i64>,
    #[sqlx(skip)]
    provider_pr_url: Option<String>,
    failure_count: i64,
    first_failed_at: Option<DateTime<Utc>>,
}

/// Commits per branch read for Activity; older commits are not shown.
pub const ACTIVITY_COMMITS_PER_BRANCH: usize = 200;

/// Each materialized workspace branch's commits since its parent merge base, read from
/// G5's git: whatever reached the branch (pushes, the Commit action, agent
/// landings) appears at its commit time. A branch whose history cannot be
/// read (for example one without an available parent) contributes nothing.
pub async fn commit_entries(
    workspace_service: &WorkspaceService,
    db: &mut ScopedDb,
    workspace_id: Uuid,
    limit_per_branch: usize,
) -> Result<Vec<ActivityEntryDTO>, G5Error> {
    let mut entries = Vec::new();
    for detail in workspace_service.list_branches(db, workspace_id).await? {
        if detail.materialization_status != "materialized" {
            continue;
        }
        let branch = workspace_service
            .get_workspace_repository(db, workspace_id, detail.repo_id)
            .await?;
        let commits = match workspace_service
            .list_branch_commits(db, &branch, limit_per_branch)
            .await
        {
            Ok(commits) => commits,
            Err(error) => {
                tracing::debug!(
                    workspace_id = %branch.workspace_id,
                    repo_id = %branch.repo_id,
                    %error,
                    "activity skipped a branch whose commits cannot be listed"
                );
                continue;
            }
        };
        for commit in commits {
            let Some(at) = parse_time(&commit.committed_at) else {
                continue;
            };
            entries.push(ActivityEntryDTO {
                key: format!("commit:{}:{}", branch.repo_id, commit.sha),
                at,
                source: ActivitySource::Workspace,
                body: ActivityEntryBody::Commit {
                    repo_id: branch.repo_id,
                    repo_name: detail.repo_name.clone(),
                    branch_name: branch.branch_name.clone(),
                    sha: commit.sha,
                    short_sha: commit.short_sha,
                    summary: commit.summary,
                    author_name: commit.author_name,
                },
            });
        }
    }
    Ok(entries)
}

#[derive(Debug, sqlx::FromRow)]
struct ApprovalDecisionRow {
    pull_request_id: Option<Uuid>,
    reviewed_git_heads: serde_json::Value,
    thread: Option<serde_json::Value>,
    id: Uuid,
    created_at: DateTime<Utc>,
    outcome: String,
    phase: String,
    actor_type: String,
    actor_user_id: Option<Uuid>,
    comment: Option<String>,
    definition_name: Option<String>,
    authored_summary: Option<bool>,
    historical: Option<bool>,
}

// ---------------------------------------------------------------------------
// Page composition
// ---------------------------------------------------------------------------

/// The most recent `page_size` entries before `before`, newest first, and
/// the cursor for the entries before them when there are any.
#[must_use]
pub fn compose_page(
    entries: Vec<ActivityEntryDTO>,
    before: Option<&ActivityCursor>,
    page_size: usize,
) -> (Vec<ActivityEntryDTO>, Option<ActivityCursor>) {
    let mut page: Vec<ActivityEntryDTO> = entries
        .into_iter()
        .filter(|entry| before.is_none_or(|cursor| cursor.is_after(entry)))
        .collect();
    page.sort_by(|left, right| (right.at, &right.key).cmp(&(left.at, &left.key)));
    page.dedup_by(|left, right| left.key == right.key);
    let has_earlier = page.len() > page_size;
    page.truncate(page_size);
    let earlier = if has_earlier {
        page.last().map(|oldest| ActivityCursor {
            at: oldest.at,
            key: oldest.key.clone(),
        })
    } else {
        None
    };
    (page, earlier)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn entry(key: &str, minute: u32) -> ActivityEntryDTO {
        ActivityEntryDTO {
            key: key.to_string(),
            at: Utc.with_ymd_and_hms(2026, 9, 1, 10, minute, 0).unwrap(),
            source: ActivitySource::Workspace,
            body: ActivityEntryBody::Task {
                task_id: Uuid::nil(),
                name: key.to_string(),
                task_kind: "command".to_string(),
                reason: None,
                status: "succeeded".to_string(),
                preview: None,
            },
        }
    }

    fn preview_row(
        status: &str,
        spec: serde_json::Value,
        outcome: Option<serde_json::Value>,
        started: bool,
        ready: bool,
    ) -> ActivityEntryDTO {
        task_entry((
            Uuid::nil(),
            "Preview · full-stack".to_string(),
            "preview".to_string(),
            status.to_string(),
            outcome
                .as_ref()
                .and_then(|o| o["reason"].as_str().map(str::to_string)),
            Utc.with_ymd_and_hms(2026, 9, 1, 10, 0, 0).unwrap(),
            Some(spec),
            outcome,
            started,
            ready,
        ))
    }

    #[test]
    fn a_preview_row_carries_its_stage_while_live_and_its_ending_afterwards() {
        let spec = serde_json::json!({"preview": {"profile": "full-stack", "port": 3000}});
        let live =
            serde_json::to_value(preview_row("running", spec.clone(), None, true, true)).unwrap();
        assert_eq!(live["preview"]["stage"], "serving");
        assert_eq!(live["preview"]["profile"], "full-stack");
        assert_eq!(live["preview"]["port"], 3000);
        assert_eq!(live["preview"].get("ending"), None);

        let outcome = serde_json::json!({"status": "failed", "result": {"status": "app_exited", "app_exit_code": 7, "stderr_tail": "boom"}});
        let ended =
            serde_json::to_value(preview_row("failed", spec, Some(outcome), true, false)).unwrap();
        assert_eq!(ended["preview"].get("stage"), None);
        assert_eq!(ended["preview"]["ending"]["kind"], "failed");
        assert_eq!(ended["preview"]["ending"]["cause"], "app_exited");
        assert_eq!(ended["preview"]["ending"]["exitCode"], 7);
        assert_eq!(
            ended["preview"]["ending"].get("stderrTail"),
            None,
            "the tail stays in the run log"
        );

        let command = serde_json::to_value(entry("task:c", 1)).unwrap();
        assert_eq!(command.get("preview"), None);
    }

    fn keys(entries: &[ActivityEntryDTO]) -> Vec<&str> {
        entries.iter().map(|entry| entry.key.as_str()).collect()
    }

    #[test]
    fn cursor_round_trips_and_rejects_malformed_values() {
        let cursor = ActivityCursor {
            at: Utc.with_ymd_and_hms(2026, 9, 1, 10, 5, 0).unwrap(),
            key: "github-event:cross-referenced:2026|x".to_string(),
        };
        assert_eq!(ActivityCursor::parse(&cursor.encode()), Some(cursor));
        for bad in [
            "",
            "no-separator",
            "2026-09-01T10:05:00Z|",
            "yesterday|task:1",
        ] {
            assert_eq!(ActivityCursor::parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn pages_hold_the_most_recent_entries_newest_first() {
        let entries = vec![
            entry("task:c", 3),
            entry("task:a", 1),
            entry("chat:b", 2),
            entry("github-comment:1", 2),
            entry("task:d", 4),
        ];
        let (page, earlier) = compose_page(entries.clone(), None, 3);
        // Ties on time order by key; the cursor is the page's oldest entry.
        assert_eq!(keys(&page), ["task:d", "task:c", "github-comment:1"]);
        let earlier = earlier.expect("older entries remain");
        assert_eq!(earlier.key, "github-comment:1");

        let (older, none) = compose_page(entries, Some(&earlier), 3);
        assert_eq!(keys(&older), ["chat:b", "task:a"]);
        assert_eq!(none, None);
    }

    #[test]
    fn a_current_chat_serializes_its_task_status_and_a_record_chat_omits_it() {
        let at = Utc.with_ymd_and_hms(2026, 9, 1, 10, 0, 0).unwrap();
        let current = serde_json::to_value(chat_entry((
            Uuid::nil(),
            Some("Plan".to_string()),
            3,
            at,
            Some("running".to_string()),
        )))
        .unwrap();
        assert_eq!(current["kind"], "chat");
        assert_eq!(current["taskStatus"], "running");
        assert_eq!(current["at"], current["lastActivityAt"]);
        let record = serde_json::to_value(chat_entry((Uuid::nil(), None, 3, at, None))).unwrap();
        assert_eq!(record.get("taskStatus"), None);
    }

    #[test]
    fn duplicate_keys_appear_once() {
        let (page, _) = compose_page(vec![entry("task:a", 1), entry("task:a", 1)], None, 10);
        assert_eq!(keys(&page), ["task:a"]);
    }

    #[test]
    fn entries_serialize_with_their_kind() {
        let value = serde_json::to_value(entry("task:a", 1)).unwrap();
        assert_eq!(value["kind"], "task");
        assert_eq!(value["source"], "workspace");
        assert_eq!(value["taskKind"], "command");
        assert_eq!(value["key"], "task:a");
    }

    /// The frontend union mirrors these field names (`lib/types.ts`).
    #[test]
    fn workbench_entries_serialize_in_camel_case() {
        let actor = WorkspaceActorDTO {
            user_id: Uuid::nil(),
            display_name: "Ada".to_string(),
        };
        let phase = serde_json::to_value(ActivityEntryBody::PhaseChanged {
            from_phase: "be".to_string(),
            to_phase: "se".to_string(),
            actor: Some(actor.clone()),
        })
        .unwrap();
        assert_eq!(phase["kind"], "phase_changed");
        assert_eq!(phase["fromPhase"], "be");
        assert_eq!(phase["toPhase"], "se");
        assert_eq!(phase["actor"]["displayName"], "Ada");

        let journey = serde_json::to_value(ActivityEntryBody::JourneyStep {
            step_id: "se-diff-review".to_string(),
            done: true,
            skipped: false,
            actor: Some(actor.clone()),
        })
        .unwrap();
        assert_eq!(journey["kind"], "journey_step");
        assert_eq!(journey["stepId"], "se-diff-review");
        assert_eq!(journey["done"], true);
        assert_eq!(journey["skipped"], false);
        assert_eq!(journey["actor"]["displayName"], "Ada");

        let pull_request = serde_json::to_value(ActivityEntryBody::PullRequest {
            pull_request_id: Uuid::nil(),
            event_type: "merge_blocked".to_string(),
            title: "Ship it".to_string(),
            target_workspace_name: Some("base".to_string()),
            source_workspace_name: Some("feature".to_string()),
            incoming: false,
            message: Some("conflict".to_string()),
            repository_count: 2,
            provider_repo_full_name: None,
            provider_pr_number: None,
            provider_pr_url: None,
            actor: None,
            failure_count: None,
            first_failed_at: None,
        })
        .unwrap();
        assert_eq!(pull_request["kind"], "pull_request");
        assert_eq!(pull_request["eventType"], "merge_blocked");
        assert_eq!(pull_request["targetWorkspaceName"], "base");
        assert_eq!(pull_request["sourceWorkspaceName"], "feature");
        assert_eq!(pull_request["incoming"], false);
        assert_eq!(pull_request["repositoryCount"], 2);
        assert_eq!(pull_request["providerPrUrl"], serde_json::Value::Null);
        assert_eq!(pull_request["actor"], serde_json::Value::Null);
    }
}
