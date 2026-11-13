//! Chat turn activity and personal result inspection (chat
//! `g5.chat.activity.turns`, `g5.chat.activity.order`,
//! `g5.chat.inspection.acknowledge`, `g5.chat.activity.refresh`).
//!
//! Activity is derived from TaskService's task and attempt records; the only
//! state stored here is each person's per-chat inspection watermark. A chat
//! turn is one agent task. It is active while running, or queued while the
//! workspace's compute is starting, running or has a start in progress: a VM
//! stop leaves queued turns queued, and those wait for the owner. Its finished
//! results are its settled attempts, or the task's own end when it ended
//! without any attempt (an admission refusal or a cancel before dispatch).
//! A session has at most one live agent task and terminal tasks gain no
//! attempts, so the latest result belongs to the newest task that has one.
//! That live task also says whether the turn waits for the person
//! (`g5.chat.activity.awaiting-input`): a pending question or permission
//! request of its running attempt, or a deferred question holding it.

use crate::errors::{AppError, G5Error};
use crate::services::EventBus;
use crate::ws::{Channel, WsEvent};
use chrono::{DateTime, Utc};
use g5_authz::ScopedDb;
use serde::Serialize;
use uuid::Uuid;

/// A chat's turn activity as one person sees it.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionActivity {
    /// When the active turn started; `None` when no turn is active.
    pub active_turn_started_at: Option<String>,
    /// When the latest finished result ended; `None` before the first.
    pub last_finished_at: Option<String>,
    /// The latest finished result, which an acknowledgement names.
    pub latest_result: Option<SessionResult>,
    /// The latest result ended after this person's watermark and the chat's
    /// floor. Kept while a newer turn is active; the spinner takes precedence.
    pub unseen_result: bool,
    /// The turn waits for the person (`g5.chat.activity.awaiting-input`): a
    /// question or permission request of its running attempt is pending, or a
    /// deferred question holds the turn.
    pub awaiting_input: bool,
}

/// One finished result of a chat turn.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionResult {
    pub task_id: Uuid,
    /// `None` for a turn that ended without any attempt.
    pub attempt: Option<i32>,
    /// The attempt recorded assistant output the transcript can display.
    pub has_output: bool,
}

/// The activity columns [`activity_joins`] adds to a session row.
#[derive(Debug, Clone, Default, sqlx::FromRow)]
pub(crate) struct ActivityColumns {
    pub(crate) activity_started_at: Option<DateTime<Utc>>,
    pub(crate) result_task_id: Option<Uuid>,
    pub(crate) result_attempt: Option<i32>,
    pub(crate) result_ended_at: Option<DateTime<Utc>>,
    pub(crate) result_has_output: Option<bool>,
    pub(crate) result_unseen: Option<bool>,
    pub(crate) awaiting_input: Option<bool>,
}

impl ActivityColumns {
    pub(crate) fn into_activity(self) -> SessionActivity {
        let latest_result = self.result_task_id.map(|task_id| SessionResult {
            task_id,
            attempt: self.result_attempt,
            has_output: self.result_has_output.unwrap_or(false),
        });
        SessionActivity {
            active_turn_started_at: self.activity_started_at.map(|at| at.to_rfc3339()),
            last_finished_at: self.result_ended_at.map(|at| at.to_rfc3339()),
            unseen_result: latest_result.is_some() && self.result_unseen.unwrap_or(false),
            latest_result,
            awaiting_input: self.awaiting_input.unwrap_or(false),
        }
    }
}

/// Select-list expressions for [`activity_joins`], for a session aliased `s`.
pub(crate) const ACTIVITY_COLUMNS: &str = r#"
    active_turn.created_at AS activity_started_at,
    latest_result.task_id AS result_task_id,
    latest_result.attempt AS result_attempt,
    latest_result.ended_at AS result_ended_at,
    (latest_result.attempt IS NOT NULL AND EXISTS (
        SELECT 1 FROM chat_messages AS output
        WHERE output.task_id = latest_result.task_id
          AND output.attempt = latest_result.attempt
          AND output.role = 'assistant')) AS result_has_output,
    (latest_result.ended_at > s.inspection_floor_at
        AND (inspection.inspected_through IS NULL
             OR latest_result.ended_at > inspection.inspected_through)) AS result_unseen,
    EXISTS (
        SELECT 1 FROM tasks AS live
        WHERE live.chat_session_id = s.id AND live.kind = 'agent'
          AND ((live.status = 'running' AND EXISTS (
                    SELECT 1 FROM provider_requests AS request
                    WHERE request.task_id = live.id AND request.attempt = live.attempt
                      AND request.status = 'pending'))
               OR (live.status = 'waiting' AND live.hold_reason = 'awaiting_answer'))) AS awaiting_input
"#;

/// The activity order (`g5.chat.activity.order`): active chats by turn start,
/// then the others by their last finished result or creation, newest first,
/// with the session id as a deterministic tie-breaker.
pub(crate) const ACTIVITY_ORDER: &str = r#"
    (active_turn.created_at IS NULL),
    active_turn.created_at DESC,
    COALESCE(latest_result.ended_at, s.created_at) DESC,
    s.id DESC
"#;

/// Joins deriving a session's activity, for a session aliased `s`, with the
/// viewing person's inspection read from bind parameter `viewer_param`
/// (for example `"$5"`). Each chat reads its newest tasks through
/// `tasks_agent_session_recent_idx`, so ordering a workspace stays linear in
/// its chat count.
pub(crate) fn activity_joins(viewer_param: &str) -> String {
    format!(
        r#"
        LEFT JOIN LATERAL (
            SELECT turn.created_at
            FROM tasks AS turn
            WHERE turn.chat_session_id = s.id AND turn.kind = 'agent'
              AND (turn.status = 'running'
                   OR (turn.status = 'queued' AND EXISTS (
                       SELECT 1 FROM workspace_vms AS compute
                       WHERE compute.workspace_id = s.workspace_id
                         AND (compute.status IN ('starting', 'running')
                              OR compute.start_intent ->> 'status'
                                 IN ('pending', 'queued_for_capacity', 'starting')))))
            ORDER BY turn.created_at DESC, turn.id DESC
            LIMIT 1
        ) AS active_turn ON true
        LEFT JOIN LATERAL (
            SELECT turn.id AS task_id, settled.attempt,
                   COALESCE(settled.ended_at, turn.ended_at, turn.updated_at) AS ended_at
            FROM tasks AS turn
            LEFT JOIN LATERAL (
                SELECT attempt.attempt, attempt.ended_at
                FROM task_attempts AS attempt
                WHERE attempt.task_id = turn.id AND attempt.ended_at IS NOT NULL
                ORDER BY attempt.attempt DESC
                LIMIT 1
            ) AS settled ON true
            WHERE turn.chat_session_id = s.id AND turn.kind = 'agent'
              AND (settled.attempt IS NOT NULL
                   OR turn.status IN ('succeeded', 'failed', 'cancelled'))
            ORDER BY turn.created_at DESC, turn.id DESC
            LIMIT 1
        ) AS latest_result ON true
        LEFT JOIN chat_session_inspections AS inspection
            ON inspection.session_id = s.id AND inspection.user_id = {viewer_param}
        "#
    )
}

/// The chat's activity as `viewer` sees it.
pub async fn session_activity(
    db: &mut ScopedDb,
    session_id: Uuid,
    viewer: Uuid,
) -> Result<SessionActivity, AppError> {
    let columns = sqlx::query_as::<_, ActivityColumns>(&format!(
        "SELECT {ACTIVITY_COLUMNS} FROM chat_sessions AS s {joins} WHERE s.id = $1",
        joins = activity_joins("$2"),
    ))
    .bind(session_id)
    .bind(viewer)
    .fetch_optional(db.conn())
    .await?
    .ok_or_else(|| {
        AppError::from_error(G5Error::NotFound {
            resource: format!("chat session {session_id}"),
        })
    })?;
    Ok(columns.into_activity())
}

/// What an acknowledgement recorded.
#[derive(Debug, Clone)]
pub struct InspectionOutcome {
    pub workspace_id: Uuid,
    /// The watermark moved forward; an older or repeated result leaves it.
    pub advanced: bool,
    pub activity: SessionActivity,
}

/// Record that `viewer` saw one finished result of the chat. Only a result of
/// an agent task in this chat is accepted, and the watermark only moves
/// forward to that result's end, so a newer result that finished meanwhile
/// stays uninspected. Needs only read authority for the chat; it changes no
/// workspace content and no activity order.
pub async fn record_inspection(
    db: &mut ScopedDb,
    session_id: Uuid,
    viewer: Uuid,
    task_id: Uuid,
    attempt: Option<i32>,
) -> Result<InspectionOutcome, AppError> {
    // The same end time the activity projection reports for this result.
    let ended_at: Option<DateTime<Utc>> = match attempt {
        Some(attempt) => {
            sqlx::query_scalar(
                "SELECT attempt.ended_at
                 FROM task_attempts AS attempt
                 JOIN tasks AS turn ON turn.id = attempt.task_id
                 WHERE attempt.task_id = $2 AND attempt.attempt = $3
                   AND attempt.ended_at IS NOT NULL
                   AND turn.chat_session_id = $1 AND turn.kind = 'agent'",
            )
            .bind(session_id)
            .bind(task_id)
            .bind(attempt)
            .fetch_optional(db.conn())
            .await?
        }
        None => {
            sqlx::query_scalar(
                "SELECT COALESCE(turn.ended_at, turn.updated_at)
                 FROM tasks AS turn
                 WHERE turn.id = $2 AND turn.chat_session_id = $1 AND turn.kind = 'agent'
                   AND turn.status IN ('succeeded', 'failed', 'cancelled')
                   AND NOT EXISTS (
                       SELECT 1 FROM task_attempts AS attempt
                       WHERE attempt.task_id = turn.id AND attempt.ended_at IS NOT NULL)",
            )
            .bind(session_id)
            .bind(task_id)
            .fetch_optional(db.conn())
            .await?
        }
    };
    let Some(ended_at) = ended_at else {
        return Err(AppError::from_error(G5Error::Conflict {
            reason: "the named result is not a finished result of this chat".into(),
        }));
    };
    let workspace_id: Uuid =
        sqlx::query_scalar("SELECT workspace_id FROM chat_sessions WHERE id = $1")
            .bind(session_id)
            .fetch_one(db.conn())
            .await?;
    let advanced = sqlx::query(
        "INSERT INTO chat_session_inspections (
             org_id, workspace_id, session_id, user_id, inspected_through)
         SELECT org_id, workspace_id, id, $2, $3 FROM chat_sessions WHERE id = $1
         ON CONFLICT (session_id, user_id) DO UPDATE
             SET inspected_through = EXCLUDED.inspected_through, updated_at = now()
             WHERE chat_session_inspections.inspected_through < EXCLUDED.inspected_through",
    )
    .bind(session_id)
    .bind(viewer)
    .bind(ended_at)
    .execute(db.conn())
    .await?
    .rows_affected()
        > 0;
    let activity = session_activity(db, session_id, viewer).await?;
    Ok(InspectionOutcome {
        workspace_id,
        advanced,
        activity,
    })
}

/// Hint, once `db` commits, that a chat's turn activity changed. Content-free:
/// clients refetch the authoritative list, so a lost hint costs only latency.
pub(crate) fn announce_session(
    db: &mut ScopedDb,
    event_bus: &EventBus,
    workspace_id: Uuid,
    session_id: Option<Uuid>,
) {
    let event_bus = event_bus.clone();
    db.on_commit(move || {
        event_bus.publish(
            &Channel::Workspace(workspace_id),
            &WsEvent::ChatActivity {
                workspace_id,
                session_id,
            },
        );
    });
}

/// [`announce_session`] for the chat an agent task belongs to; nothing for
/// other task kinds.
pub(crate) async fn announce_task(
    db: &mut ScopedDb,
    event_bus: &EventBus,
    task_id: Uuid,
) -> Result<(), AppError> {
    let owner: Option<(Uuid, Option<Uuid>)> = sqlx::query_as(
        "SELECT workspace_id, chat_session_id FROM tasks WHERE id = $1 AND kind = 'agent'",
    )
    .bind(task_id)
    .fetch_optional(db.conn())
    .await?;
    if let Some((workspace_id, Some(session_id))) = owner {
        announce_session(db, event_bus, workspace_id, Some(session_id));
    }
    Ok(())
}

/// [`announce_session`] for one chat by id.
pub(crate) async fn announce_chat(
    db: &mut ScopedDb,
    event_bus: &EventBus,
    session_id: Uuid,
) -> Result<(), AppError> {
    let workspace_id: Option<Uuid> =
        sqlx::query_scalar("SELECT workspace_id FROM chat_sessions WHERE id = $1")
            .bind(session_id)
            .fetch_optional(db.conn())
            .await?;
    if let Some(workspace_id) = workspace_id {
        announce_session(db, event_bus, workspace_id, Some(session_id));
    }
    Ok(())
}

/// Tell only `viewer`'s own connections that their inspection of a chat
/// changed, so their other tabs and devices converge. Call after commit.
pub fn publish_inspection(
    event_bus: &EventBus,
    workspace_id: Uuid,
    session_id: Uuid,
    viewer: Uuid,
) {
    event_bus.publish_to_user(
        &Channel::Workspace(workspace_id),
        viewer,
        &WsEvent::ChatActivity {
            workspace_id,
            session_id: Some(session_id),
        },
    );
}
