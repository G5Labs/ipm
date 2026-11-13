//! Org-wide approval activity feed (`g5.governance.approval.activity-feed`).
//!
//! One read projection over `approval_requests` and `approval_statuses`, scoped
//! to every workspace the reader may read under the shared workspace audience
//! (`services/git/workspace_audience.rs`, the same predicate `g5d pr list --all`
//! applies, in its live form so feed membership equals the per-workspace
//! drill-in audience). The approval tables carry only org-level RLS, so this
//! query applies the reader filter itself. It starts no compute, writes nothing
//! and is not audited. User display is resolved off the system pool because the
//! tenant role cannot read `users`.

use crate::services::git::workspace_audience::{readable, AudienceReader};
use chrono::{DateTime, Duration, Utc};
use g5_authz::ScopedDb;
use g5_contracts::G5Error;
use serde::Serialize;
use serde_json::Value;
use sqlx::PgPool;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

/// The longest window one call may scan (`g5.governance.approval.activity-feed`).
const MAX_WINDOW_DAYS: i64 = 31;
/// The most events one page returns.
const MAX_LIMIT: i64 = 200;
const DEFAULT_LIMIT: i64 = 50;
/// The default window when `--since` is omitted.
const DEFAULT_WINDOW_HOURS: i64 = 24;

const KINDS: [&str; 3] = ["requested", "decided", "closed"];
const OUTCOMES: [&str; 3] = ["approved", "rejected", "deferred"];

/// A read request for the feed. Strings arrive from the agent route's query and
/// are resolved against the reader's organization here.
#[derive(Debug, Default)]
pub struct ActivityQuery {
    pub since: Option<String>,
    pub until: Option<String>,
    pub workspaces: Vec<String>,
    pub definitions: Vec<String>,
    pub reviewer: Option<String>,
    pub requester: Option<String>,
    pub kinds: Vec<String>,
    pub actor: Option<String>,
    pub outcomes: Vec<String>,
    pub limit: Option<i64>,
    pub after: Option<String>,
    pub pending: bool,
    pub summary: bool,
}

fn validation(message: impl Into<String>) -> G5Error {
    G5Error::ValidationError {
        details: serde_json::json!({ "message": message.into() }),
    }
}

fn internal(context: &'static str) -> impl Fn(sqlx::Error) -> G5Error {
    move |error| G5Error::Internal {
        message: format!("{context}: {error}"),
    }
}

/// Text G5 did not write (`g5.email.authored-content`): labelled untrusted in
/// every output and never an instruction.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UntrustedText {
    pub text: String,
    /// Always `false`; present so every consumer sees the label.
    pub content_trusted: bool,
}

impl UntrustedText {
    fn new(text: String) -> Self {
        Self {
            text,
            content_trusted: false,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceRef {
    pub id: Uuid,
    pub name: String,
    pub owner: Uuid,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DefinitionRef {
    pub id: Uuid,
    pub name: String,
    pub version: i32,
}

/// A human user or the agent. `type` is `"human"` or `"agent"`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActorRef {
    #[serde(rename = "type")]
    pub actor_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestRef {
    pub id: Uuid,
    /// The G5 name, `PR######` (`g5.git.pr-local-number`).
    pub name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityEvent {
    pub kind: String,
    pub at: DateTime<Utc>,
    /// Cursor anchor: the request id (`requested`/`closed`) or status id
    /// (`decided`).
    pub id: Uuid,
    pub workspace: WorkspaceRef,
    pub workspace_link: String,
    pub approvals_link: String,
    pub definition: DefinitionRef,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recipient: Option<ActorRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requester: Option<ActorRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pull_request: Option<PullRequestRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<UntrustedText>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reviewer: Option<ActorRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<UntrustedText>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reviewed_heads: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub review_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_seconds: Option<i64>,
    /// Terminal state of a `closed` event: cancelled, expired, superseded or
    /// bypassed. No closer identity is recorded on the request row.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingRequest {
    pub request_id: Uuid,
    pub at: DateTime<Utc>,
    pub age_seconds: i64,
    pub workspace: WorkspaceRef,
    pub workspace_link: String,
    pub approvals_link: String,
    pub definition: DefinitionRef,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recipient: Option<ActorRef>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityWindow {
    pub since: DateTime<Utc>,
    pub until: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivitySummary {
    pub requested: i64,
    pub decided: i64,
    pub approved: i64,
    pub rejected: i64,
    pub deferred: i64,
    pub closed_without_decision: i64,
    pub pending_now: i64,
    pub by_definition: Vec<DefinitionRollup>,
    pub by_reviewer: Vec<ReviewerRollup>,
    pub by_workspace: Vec<WorkspaceRollup>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DefinitionRollup {
    pub definition: Uuid,
    pub name: String,
    pub requested: i64,
    pub approved: i64,
    pub rejected: i64,
    pub pending_now: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub median_decision_seconds: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub p90_decision_seconds: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oldest_pending_seconds: Option<i64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewerRollup {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reviewer: Option<Uuid>,
    #[serde(rename = "type")]
    pub actor_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    pub requested: i64,
    pub decided: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub median_decision_seconds: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub p90_decision_seconds: Option<i64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceRollup {
    pub workspace: Uuid,
    pub name: String,
    pub requested: i64,
    pub decided: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalActivityResponse {
    pub window: ActivityWindow,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub events: Option<Vec<ActivityEvent>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending: Option<Vec<PendingRequest>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<ActivitySummary>,
}

/// Owns no durable state: the feed is a read projection over records the
/// approvals domain owns.
#[derive(Clone, Copy, Debug, Default)]
pub struct ApprovalActivityService;

#[derive(Clone)]
struct UserInfo {
    email: String,
    display_name: String,
}

type UserMap = HashMap<Uuid, UserInfo>;

/// The cursor is `at_rfc3339|kind|id`. It encodes the exact keyset tuple, so a
/// page neither repeats nor skips an event as new ones arrive.
fn encode_cursor(event: &ActivityEvent) -> String {
    format!(
        "{}|{}|{}",
        event.at.to_rfc3339_opts(chrono::SecondsFormat::Nanos, true),
        event.kind,
        event.id
    )
}

fn decode_cursor(cursor: &str) -> Result<(DateTime<Utc>, String, Uuid), G5Error> {
    let mut parts = cursor.splitn(3, '|');
    let at = parts.next().unwrap_or_default();
    let kind = parts.next().unwrap_or_default();
    let id = parts.next().unwrap_or_default();
    let at: DateTime<Utc> = DateTime::parse_from_rfc3339(at)
        .map_err(|_| validation("Invalid --after cursor"))?
        .with_timezone(&Utc);
    let id: Uuid = id
        .parse()
        .map_err(|_| validation("Invalid --after cursor"))?;
    if kind.is_empty() {
        return Err(validation("Invalid --after cursor"));
    }
    Ok((at, kind.to_string(), id))
}

fn parse_time(label: &str, raw: &str) -> Result<DateTime<Utc>, G5Error> {
    DateTime::parse_from_rfc3339(raw)
        .map(|t| t.with_timezone(&Utc))
        .map_err(|_| validation(format!("{label} must be an RFC 3339 time")))
}

impl ApprovalActivityService {
    /// The activity feed over the reader's readable workspaces.
    pub async fn activity(
        &self,
        db: &mut ScopedDb,
        system_pool: &PgPool,
        reader: AudienceReader,
        query: &ActivityQuery,
    ) -> Result<ApprovalActivityResponse, G5Error> {
        let now = Utc::now();
        let until = match &query.until {
            Some(raw) => parse_time("--until", raw)?,
            None => now,
        };
        let since = match &query.since {
            Some(raw) => parse_time("--since", raw)?,
            None => until - Duration::hours(DEFAULT_WINDOW_HOURS),
        };
        if since >= until {
            return Err(validation("--since must be before --until"));
        }
        if until - since > Duration::days(MAX_WINDOW_DAYS) {
            return Err(validation(format!(
                "window exceeds the {MAX_WINDOW_DAYS}-day cap; narrow --since/--until"
            )));
        }
        let limit = match query.limit {
            None => DEFAULT_LIMIT,
            Some(value) if (1..=MAX_LIMIT).contains(&value) => value,
            Some(_) => return Err(validation(format!("-L must be between 1 and {MAX_LIMIT}"))),
        };

        let kinds = self.validate_kinds(query)?;
        let outcomes = self.validate_outcomes(query)?;
        let actor = self.validate_actor(query)?;
        let workspaces = self
            .resolve_workspaces(db, reader.org_id, &query.workspaces)
            .await?;
        let definitions = self
            .resolve_definitions(db, reader.org_id, &query.definitions)
            .await?;
        let reviewer = self
            .resolve_user(system_pool, reader, query.reviewer.as_deref())
            .await?;
        let requester = self
            .resolve_user(system_pool, reader, query.requester.as_deref())
            .await?;

        let window = ActivityWindow { since, until };
        let mut response = ApprovalActivityResponse {
            window: ActivityWindow { since, until },
            events: None,
            next_cursor: None,
            pending: None,
            summary: None,
        };

        if query.summary {
            response.summary = Some(
                self.summary(
                    db,
                    system_pool,
                    reader,
                    &window,
                    &workspaces,
                    &definitions,
                    reviewer,
                    requester,
                    &actor,
                    &outcomes,
                )
                .await?,
            );
        } else {
            let (events, next_cursor) = self
                .events(
                    db,
                    system_pool,
                    reader,
                    &window,
                    &workspaces,
                    &definitions,
                    reviewer,
                    requester,
                    &kinds,
                    &actor,
                    &outcomes,
                    limit,
                    query.after.as_deref(),
                )
                .await?;
            response.next_cursor = next_cursor;
            response.events = Some(events);
        }

        if query.pending {
            response.pending = Some(
                self.pending(
                    db,
                    system_pool,
                    reader,
                    until,
                    &workspaces,
                    &definitions,
                    reviewer,
                )
                .await?,
            );
        }

        Ok(response)
    }

    fn validate_kinds(&self, query: &ActivityQuery) -> Result<Vec<String>, G5Error> {
        for kind in &query.kinds {
            if !KINDS.contains(&kind.as_str()) {
                return Err(validation(format!(
                    "--kind must be one of requested, decided, closed (got {kind})"
                )));
            }
        }
        Ok(query.kinds.clone())
    }

    fn validate_outcomes(&self, query: &ActivityQuery) -> Result<Vec<String>, G5Error> {
        for outcome in &query.outcomes {
            if !OUTCOMES.contains(&outcome.as_str()) {
                return Err(validation(format!(
                    "--outcome must be one of approved, rejected, deferred (got {outcome})"
                )));
            }
        }
        Ok(query.outcomes.clone())
    }

    fn validate_actor(&self, query: &ActivityQuery) -> Result<Option<String>, G5Error> {
        match query.actor.as_deref() {
            None => Ok(None),
            Some(actor @ ("human" | "agent")) => Ok(Some(actor.to_string())),
            Some(other) => Err(validation(format!(
                "--actor must be human or agent (got {other})"
            ))),
        }
    }

    /// Resolve workspace ids and names to ids. Names that do not resolve are
    /// dropped; the main query re-applies readability, so resolution never
    /// leaks an unreadable workspace.
    async fn resolve_workspaces(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        values: &[String],
    ) -> Result<Option<Vec<Uuid>>, G5Error> {
        if values.is_empty() {
            return Ok(None);
        }
        let (mut resolved, names) = partition_ids(values);
        if !names.is_empty() {
            let found: Vec<Uuid> = sqlx::query_scalar(
                "SELECT id FROM workspaces WHERE org_id = $1 AND name = ANY($2)",
            )
            .bind(org_id)
            .bind(&names)
            .fetch_all(db.conn())
            .await
            .map_err(internal("resolve workspaces"))?;
            resolved.extend(found);
        }
        Ok(Some(resolved))
    }

    async fn resolve_definitions(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        values: &[String],
    ) -> Result<Option<Vec<Uuid>>, G5Error> {
        if values.is_empty() {
            return Ok(None);
        }
        let (mut resolved, names) = partition_ids(values);
        if !names.is_empty() {
            let found: Vec<Uuid> = sqlx::query_scalar(
                "SELECT DISTINCT id FROM approval_definitions WHERE org_id = $1 AND name = ANY($2)",
            )
            .bind(org_id)
            .bind(&names)
            .fetch_all(db.conn())
            .await
            .map_err(internal("resolve definitions"))?;
            resolved.extend(found);
        }
        Ok(Some(resolved))
    }

    /// Resolve a `--reviewer`/`--requester` selector (id, email, email local
    /// part or `@me`) to a user id, off the system pool.
    async fn resolve_user(
        &self,
        system_pool: &PgPool,
        reader: AudienceReader,
        value: Option<&str>,
    ) -> Result<Option<Uuid>, G5Error> {
        let Some(value) = value else {
            return Ok(None);
        };
        if value == "@me" {
            return Ok(Some(reader.actor));
        }
        if let Ok(id) = value.parse::<Uuid>() {
            return Ok(Some(id));
        }
        let id: Option<Uuid> = sqlx::query_scalar(
            "SELECT u.id FROM users u \
             JOIN org_memberships m ON m.user_id = u.id AND m.org_id = $1 \
             WHERE lower(u.email) = lower($2) OR lower(split_part(u.email, '@', 1)) = lower($2) \
             LIMIT 1",
        )
        .bind(reader.org_id)
        .bind(value)
        .fetch_optional(system_pool)
        .await
        .map_err(internal("resolve user"))?;
        match id {
            Some(id) => Ok(Some(id)),
            None => Err(validation(format!("no user matches '{value}'"))),
        }
    }

    /// Batch-resolve display info for a set of user ids off the system pool.
    async fn resolve_users(
        &self,
        system_pool: &PgPool,
        ids: HashSet<Uuid>,
    ) -> Result<UserMap, G5Error> {
        if ids.is_empty() {
            return Ok(UserMap::new());
        }
        let ids: Vec<Uuid> = ids.into_iter().collect();
        let rows: Vec<(Uuid, String, String)> =
            sqlx::query_as("SELECT id, email, display_name FROM users WHERE id = ANY($1)")
                .bind(&ids)
                .fetch_all(system_pool)
                .await
                .map_err(internal("resolve user names"))?;
        Ok(rows
            .into_iter()
            .map(|(id, email, display_name)| {
                (
                    id,
                    UserInfo {
                        email,
                        display_name,
                    },
                )
            })
            .collect())
    }

    #[allow(clippy::too_many_arguments)]
    async fn events(
        &self,
        db: &mut ScopedDb,
        system_pool: &PgPool,
        reader: AudienceReader,
        window: &ActivityWindow,
        workspaces: &Option<Vec<Uuid>>,
        definitions: &Option<Vec<Uuid>>,
        reviewer: Option<Uuid>,
        requester: Option<Uuid>,
        kinds: &[String],
        actor: &Option<String>,
        outcomes: &[String],
        limit: i64,
        after: Option<&str>,
    ) -> Result<(Vec<ActivityEvent>, Option<String>), G5Error> {
        let cursor = after.map(decode_cursor).transpose()?;
        let kinds_filter: Option<Vec<String>> = (!kinds.is_empty()).then(|| kinds.to_vec());
        let outcomes_filter: Option<Vec<String>> =
            (!outcomes.is_empty()).then(|| outcomes.to_vec());

        let sql = format!(
            r#"
            WITH ev AS (
                SELECT 'requested' AS kind, req.created_at AS at, req.id AS event_id,
                       req.id AS request_id, req.source_workspace_id AS workspace_id,
                       req.approval_definition_id AS definition_id,
                       req.approval_definition_version AS definition_version,
                       req.actor_type AS actor_type, req.actor_user_id AS actor_user_id,
                       req.requested_by AS requested_by, NULL::text AS state,
                       NULL::text AS outcome, NULL::text AS comment, req.note AS note,
                       NULL::jsonb AS reviewed_heads, NULL::uuid AS review_id,
                       req.pull_request_id AS pull_request_id, NULL::bigint AS latency_seconds
                FROM approval_requests req
                WHERE req.org_id = $1 AND req.created_at >= $4 AND req.created_at < $5
                UNION ALL
                SELECT 'decided', s.created_at, s.id,
                       COALESCE(s.request_id, (SELECT r.approval_request_id FROM agent_approval_reviews r WHERE r.id = s.agent_review_id)) AS request_id,
                       s.source_workspace_id, s.approval_definition_id, s.approval_definition_version,
                       s.actor_type, s.actor_user_id, dreq.requested_by, NULL::text,
                       s.outcome, s.comment, NULL::text, s.reviewed_git_heads, s.agent_review_id,
                       s.pull_request_id,
                       CASE WHEN dreq.created_at IS NOT NULL
                            THEN EXTRACT(EPOCH FROM (s.created_at - dreq.created_at))::bigint END
                FROM approval_statuses s
                LEFT JOIN approval_requests dreq ON dreq.org_id = s.org_id
                     AND dreq.id = COALESCE(s.request_id, (SELECT r.approval_request_id FROM agent_approval_reviews r WHERE r.id = s.agent_review_id))
                WHERE s.org_id = $1 AND s.created_at >= $4 AND s.created_at < $5
                UNION ALL
                SELECT 'closed', req.closed_at, req.id, req.id, req.source_workspace_id,
                       req.approval_definition_id, req.approval_definition_version,
                       req.actor_type, req.actor_user_id, req.requested_by, req.state,
                       NULL::text, NULL::text, NULL::text, NULL::jsonb, NULL::uuid,
                       req.pull_request_id, NULL::bigint
                FROM approval_requests req
                WHERE req.org_id = $1 AND req.state <> 'settled' AND req.closed_at IS NOT NULL
                      AND req.closed_at >= $4 AND req.closed_at < $5
            )
            SELECT ev.kind, ev.at, ev.event_id, ev.request_id, ev.workspace_id,
                   w.name AS workspace_name, w.owner_user_id AS workspace_owner,
                   ev.definition_id, ev.definition_version, d.name AS definition_name,
                   ev.actor_type, ev.actor_user_id, ev.requested_by,
                   ev.state, ev.outcome, ev.comment, ev.note, ev.reviewed_heads,
                   ev.review_id, ev.pull_request_id, pr.number AS pr_number, ev.latency_seconds
            FROM ev
            JOIN workspaces w ON w.org_id = $1 AND w.id = ev.workspace_id
            JOIN approval_definitions d ON d.org_id = $1 AND d.id = ev.definition_id
                 AND d.version = ev.definition_version
            LEFT JOIN workspace_pull_requests pr ON pr.org_id = $1 AND pr.id = ev.pull_request_id
            WHERE {audience}
              AND ($6::uuid[] IS NULL OR ev.workspace_id = ANY($6))
              AND ($7::uuid[] IS NULL OR ev.definition_id = ANY($7))
              AND ($8::uuid IS NULL OR ev.actor_user_id = $8)
              AND ($9::uuid IS NULL OR ev.requested_by = $9)
              AND ($10::text[] IS NULL OR ev.kind = ANY($10))
              AND ($11::text IS NULL OR ev.actor_type = $11)
              AND ($12::text[] IS NULL OR ev.kind <> 'decided' OR ev.outcome = ANY($12))
              AND ($13::timestamptz IS NULL OR (ev.at, ev.kind, ev.event_id) < ($13, $14, $15))
            ORDER BY ev.at DESC, ev.kind DESC, ev.event_id DESC
            LIMIT $16
            "#,
            audience = readable("w", 2, 3),
        );

        let rows: Vec<EventRow> = sqlx::query_as(&sql)
            .bind(reader.org_id)
            .bind(reader.org_wide)
            .bind(reader.actor)
            .bind(window.since)
            .bind(window.until)
            .bind(workspaces.clone())
            .bind(definitions.clone())
            .bind(reviewer)
            .bind(requester)
            .bind(kinds_filter)
            .bind(actor.clone())
            .bind(outcomes_filter)
            .bind(cursor.as_ref().map(|c| c.0))
            .bind(cursor.as_ref().map(|c| c.1.clone()))
            .bind(cursor.as_ref().map(|c| c.2))
            .bind(limit + 1)
            .fetch_all(db.conn())
            .await
            .map_err(internal("approval activity events"))?;

        let has_more = rows.len() as i64 > limit;
        let rows: Vec<EventRow> = rows.into_iter().take(limit as usize).collect();
        let mut ids = HashSet::new();
        for row in &rows {
            ids.extend(row.actor_user_id);
            ids.extend(row.requested_by);
        }
        let users = self.resolve_users(system_pool, ids).await?;
        let events: Vec<ActivityEvent> =
            rows.into_iter().map(|row| row.into_event(&users)).collect();
        let next_cursor = if has_more {
            events.last().map(encode_cursor)
        } else {
            None
        };
        Ok((events, next_cursor))
    }

    #[allow(clippy::too_many_arguments)]
    async fn pending(
        &self,
        db: &mut ScopedDb,
        system_pool: &PgPool,
        reader: AudienceReader,
        until: DateTime<Utc>,
        workspaces: &Option<Vec<Uuid>>,
        definitions: &Option<Vec<Uuid>>,
        reviewer: Option<Uuid>,
    ) -> Result<Vec<PendingRequest>, G5Error> {
        let sql = format!(
            r#"
            SELECT req.id AS event_id, req.created_at AS at, req.source_workspace_id AS workspace_id,
                   w.name AS workspace_name, w.owner_user_id AS workspace_owner,
                   req.approval_definition_id AS definition_id,
                   req.approval_definition_version AS definition_version, d.name AS definition_name,
                   req.actor_type, req.actor_user_id
            FROM approval_requests req
            JOIN workspaces w ON w.org_id = $1 AND w.id = req.source_workspace_id
            JOIN approval_definitions d ON d.org_id = $1 AND d.id = req.approval_definition_id
                 AND d.version = req.approval_definition_version
            WHERE req.org_id = $1 AND req.state = 'pending' AND req.created_at < $4
              AND {audience}
              AND ($5::uuid[] IS NULL OR req.source_workspace_id = ANY($5))
              AND ($6::uuid[] IS NULL OR req.approval_definition_id = ANY($6))
              AND ($7::uuid IS NULL OR req.actor_user_id = $7)
            ORDER BY req.created_at ASC, req.id ASC
            LIMIT {MAX_LIMIT}
            "#,
            audience = readable("w", 2, 3),
        );
        let rows: Vec<PendingRow> = sqlx::query_as(&sql)
            .bind(reader.org_id)
            .bind(reader.org_wide)
            .bind(reader.actor)
            .bind(until)
            .bind(workspaces.clone())
            .bind(definitions.clone())
            .bind(reviewer)
            .fetch_all(db.conn())
            .await
            .map_err(internal("approval activity pending"))?;
        let ids: HashSet<Uuid> = rows.iter().filter_map(|row| row.actor_user_id).collect();
        let users = self.resolve_users(system_pool, ids).await?;
        Ok(rows
            .into_iter()
            .map(|row| row.into_pending(until, &users))
            .collect())
    }

    #[allow(clippy::too_many_arguments)]
    async fn summary(
        &self,
        db: &mut ScopedDb,
        system_pool: &PgPool,
        reader: AudienceReader,
        window: &ActivityWindow,
        workspaces: &Option<Vec<Uuid>>,
        definitions: &Option<Vec<Uuid>>,
        reviewer: Option<Uuid>,
        requester: Option<Uuid>,
        actor: &Option<String>,
        outcomes: &[String],
    ) -> Result<ActivitySummary, G5Error> {
        // Walk the full filtered feed once and fold it into the rollups, so the
        // summary equals a page walk of the same filter by construction. The
        // window cap bounds the row count.
        let mut events = Vec::new();
        let mut cursor: Option<String> = None;
        let outcomes_vec = outcomes.to_vec();
        loop {
            let (page, next) = self
                .events(
                    db,
                    system_pool,
                    reader,
                    window,
                    workspaces,
                    definitions,
                    reviewer,
                    requester,
                    &[],
                    actor,
                    &outcomes_vec,
                    MAX_LIMIT,
                    cursor.as_deref(),
                )
                .await?;
            events.extend(page);
            match next {
                Some(c) => cursor = Some(c),
                None => break,
            }
        }
        let pending = self
            .pending(
                db,
                system_pool,
                reader,
                window.until,
                workspaces,
                definitions,
                reviewer,
            )
            .await?;
        Ok(fold_summary(events, pending))
    }
}

/// Split selectors into UUIDs and names.
fn partition_ids(values: &[String]) -> (Vec<Uuid>, Vec<String>) {
    let mut ids = Vec::new();
    let mut names = Vec::new();
    for value in values {
        match value.parse::<Uuid>() {
            Ok(id) => ids.push(id),
            Err(_) => names.push(value.clone()),
        }
    }
    (ids, names)
}

fn workspace_link(id: Uuid) -> String {
    crate::services::email::link_path(&["workspaces", &id.to_string()])
}

fn approvals_link(id: Uuid) -> String {
    crate::services::email::link_path(&["workspaces", &id.to_string(), "approvals"])
}

fn actor_ref(actor_type: &str, id: Option<Uuid>, users: &UserMap) -> ActorRef {
    match (actor_type, id) {
        ("human", Some(id)) => {
            let info = users.get(&id);
            ActorRef {
                actor_type: "human".to_string(),
                id: Some(id),
                email: info.map(|u| u.email.clone()),
                display_name: info.map(|u| u.display_name.clone()),
            }
        }
        _ => ActorRef {
            actor_type: "agent".to_string(),
            id: None,
            email: None,
            display_name: None,
        },
    }
}

#[derive(sqlx::FromRow)]
struct EventRow {
    kind: String,
    at: DateTime<Utc>,
    event_id: Uuid,
    request_id: Option<Uuid>,
    workspace_id: Uuid,
    workspace_name: String,
    workspace_owner: Uuid,
    definition_id: Uuid,
    definition_version: i32,
    definition_name: String,
    actor_type: String,
    actor_user_id: Option<Uuid>,
    requested_by: Option<Uuid>,
    state: Option<String>,
    outcome: Option<String>,
    comment: Option<String>,
    note: Option<String>,
    reviewed_heads: Option<Value>,
    review_id: Option<Uuid>,
    pull_request_id: Option<Uuid>,
    pr_number: Option<i64>,
    latency_seconds: Option<i64>,
}

impl EventRow {
    fn into_event(self, users: &UserMap) -> ActivityEvent {
        let workspace = WorkspaceRef {
            id: self.workspace_id,
            name: self.workspace_name,
            owner: self.workspace_owner,
        };
        let definition = DefinitionRef {
            id: self.definition_id,
            name: self.definition_name,
            version: self.definition_version,
        };
        let pull_request = match (self.pull_request_id, self.pr_number) {
            (Some(id), Some(number)) => Some(PullRequestRef {
                id,
                name: crate::services::git::pull_request_adapters::g5_pull_request_name(number),
            }),
            _ => None,
        };
        let actor = actor_ref(&self.actor_type, self.actor_user_id, users);
        let requester = self.requested_by.map(|id| {
            let info = users.get(&id);
            ActorRef {
                actor_type: "human".to_string(),
                id: Some(id),
                email: info.map(|u| u.email.clone()),
                display_name: info.map(|u| u.display_name.clone()),
            }
        });
        let mut event = ActivityEvent {
            kind: self.kind.clone(),
            at: self.at,
            id: self.event_id,
            workspace_link: workspace_link(workspace.id),
            approvals_link: approvals_link(workspace.id),
            workspace,
            definition,
            request_id: self.request_id,
            recipient: None,
            requester: None,
            pull_request: None,
            note: None,
            reviewer: None,
            outcome: None,
            comment: None,
            reviewed_heads: None,
            review_id: None,
            latency_seconds: None,
            state: None,
        };
        match self.kind.as_str() {
            "requested" => {
                event.recipient = Some(actor);
                event.requester = requester;
                event.pull_request = pull_request;
                event.note = self.note.map(UntrustedText::new);
            }
            "decided" => {
                event.reviewer = Some(actor);
                event.outcome = self.outcome;
                event.comment = self.comment.map(UntrustedText::new);
                event.reviewed_heads = self.reviewed_heads;
                event.review_id = self.review_id;
                event.latency_seconds = self.latency_seconds;
                event.pull_request = pull_request;
            }
            _ => {
                event.recipient = Some(actor);
                event.state = self.state;
                event.pull_request = pull_request;
            }
        }
        event
    }
}

#[derive(sqlx::FromRow)]
struct PendingRow {
    event_id: Uuid,
    at: DateTime<Utc>,
    workspace_id: Uuid,
    workspace_name: String,
    workspace_owner: Uuid,
    definition_id: Uuid,
    definition_version: i32,
    definition_name: String,
    actor_type: String,
    actor_user_id: Option<Uuid>,
}

impl PendingRow {
    fn into_pending(self, until: DateTime<Utc>, users: &UserMap) -> PendingRequest {
        let workspace = WorkspaceRef {
            id: self.workspace_id,
            name: self.workspace_name,
            owner: self.workspace_owner,
        };
        PendingRequest {
            request_id: self.event_id,
            at: self.at,
            age_seconds: (until - self.at).num_seconds().max(0),
            workspace_link: workspace_link(workspace.id),
            approvals_link: approvals_link(workspace.id),
            workspace,
            definition: DefinitionRef {
                id: self.definition_id,
                name: self.definition_name,
                version: self.definition_version,
            },
            recipient: Some(actor_ref(&self.actor_type, self.actor_user_id, users)),
        }
    }
}

fn percentile(sorted: &[i64], q: f64) -> Option<i64> {
    if sorted.is_empty() {
        return None;
    }
    if sorted.len() == 1 {
        return Some(sorted[0]);
    }
    let rank = q * (sorted.len() - 1) as f64;
    let low = rank.floor() as usize;
    let high = rank.ceil() as usize;
    let weight = rank - low as f64;
    let value = sorted[low] as f64 + (sorted[high] as f64 - sorted[low] as f64) * weight;
    Some(value.round() as i64)
}

/// Fold the full filtered feed and the pending set into the summary.
fn fold_summary(events: Vec<ActivityEvent>, pending: Vec<PendingRequest>) -> ActivitySummary {
    use std::collections::BTreeMap;

    let mut summary = ActivitySummary {
        requested: 0,
        decided: 0,
        approved: 0,
        rejected: 0,
        deferred: 0,
        closed_without_decision: 0,
        pending_now: pending.len() as i64,
        by_definition: Vec::new(),
        by_reviewer: Vec::new(),
        by_workspace: Vec::new(),
    };

    struct DefAgg {
        name: String,
        requested: i64,
        approved: i64,
        rejected: i64,
        latencies: Vec<i64>,
    }
    struct RevAgg {
        actor_type: String,
        email: Option<String>,
        requested: i64,
        decided: i64,
        latencies: Vec<i64>,
    }
    struct WsAgg {
        name: String,
        requested: i64,
        decided: i64,
    }

    let mut defs: BTreeMap<Uuid, DefAgg> = BTreeMap::new();
    let mut revs: BTreeMap<Option<Uuid>, RevAgg> = BTreeMap::new();
    let mut wss: BTreeMap<Uuid, WsAgg> = BTreeMap::new();

    for event in &events {
        let def = defs.entry(event.definition.id).or_insert_with(|| DefAgg {
            name: event.definition.name.clone(),
            requested: 0,
            approved: 0,
            rejected: 0,
            latencies: Vec::new(),
        });
        let ws = wss.entry(event.workspace.id).or_insert_with(|| WsAgg {
            name: event.workspace.name.clone(),
            requested: 0,
            decided: 0,
        });
        match event.kind.as_str() {
            "requested" => {
                summary.requested += 1;
                def.requested += 1;
                ws.requested += 1;
                let actor = event.recipient.as_ref();
                let rev = revs
                    .entry(actor.and_then(|a| a.id))
                    .or_insert_with(|| RevAgg {
                        actor_type: actor
                            .map(|a| a.actor_type.clone())
                            .unwrap_or_else(|| "agent".to_string()),
                        email: actor.and_then(|a| a.email.clone()),
                        requested: 0,
                        decided: 0,
                        latencies: Vec::new(),
                    });
                rev.requested += 1;
            }
            "decided" => {
                summary.decided += 1;
                ws.decided += 1;
                match event.outcome.as_deref() {
                    Some("approved") => {
                        summary.approved += 1;
                        def.approved += 1;
                    }
                    Some("rejected") => {
                        summary.rejected += 1;
                        def.rejected += 1;
                    }
                    Some("deferred") => summary.deferred += 1,
                    _ => {}
                }
                if let Some(latency) = event.latency_seconds {
                    def.latencies.push(latency);
                }
                let actor = event.reviewer.as_ref();
                let rev = revs
                    .entry(actor.and_then(|a| a.id))
                    .or_insert_with(|| RevAgg {
                        actor_type: actor
                            .map(|a| a.actor_type.clone())
                            .unwrap_or_else(|| "agent".to_string()),
                        email: actor.and_then(|a| a.email.clone()),
                        requested: 0,
                        decided: 0,
                        latencies: Vec::new(),
                    });
                rev.decided += 1;
                if let Some(latency) = event.latency_seconds {
                    rev.latencies.push(latency);
                }
            }
            _ => summary.closed_without_decision += 1,
        }
    }

    let mut def_pending: BTreeMap<Uuid, (i64, i64)> = BTreeMap::new();
    for request in &pending {
        let entry = def_pending.entry(request.definition.id).or_insert((0, 0));
        entry.0 += 1;
        entry.1 = entry.1.max(request.age_seconds);
    }

    for (id, agg) in defs {
        let mut latencies = agg.latencies;
        latencies.sort_unstable();
        let (pending_now, oldest) = def_pending.get(&id).copied().unwrap_or((0, 0));
        summary.by_definition.push(DefinitionRollup {
            definition: id,
            name: agg.name,
            requested: agg.requested,
            approved: agg.approved,
            rejected: agg.rejected,
            pending_now,
            median_decision_seconds: percentile(&latencies, 0.5),
            p90_decision_seconds: percentile(&latencies, 0.9),
            oldest_pending_seconds: (oldest > 0).then_some(oldest),
        });
    }
    for (id, agg) in revs {
        let mut latencies = agg.latencies;
        latencies.sort_unstable();
        summary.by_reviewer.push(ReviewerRollup {
            reviewer: id,
            actor_type: agg.actor_type,
            email: agg.email,
            requested: agg.requested,
            decided: agg.decided,
            median_decision_seconds: percentile(&latencies, 0.5),
            p90_decision_seconds: percentile(&latencies, 0.9),
        });
    }
    for (id, agg) in wss {
        summary.by_workspace.push(WorkspaceRollup {
            workspace: id,
            name: agg.name,
            requested: agg.requested,
            decided: agg.decided,
        });
    }
    summary
}
