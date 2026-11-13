//! Shared workspace task state and public task-history projections.
//!
//! Agent/command/preview execution is implemented by the dispatcher-backed
//! extensions in `agent_dispatch` and `task_lifecycle`. This module only owns
//! their shared dependencies and read-side compatibility projections.

use crate::services::git::repo_store::TenantRepoStorage;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use g5_authz::ScopedDb;
use g5_contracts::error::G5Error;
use g5_contracts::types::TaskLifecyclePosture;
use rust_decimal::prelude::ToPrimitive;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::errors::AppError;
use crate::models::task::Task;
pub use crate::services::git::unified_diff::parse_unified_diff as parse_task_unified_diff;
use crate::services::{ConfigService, EventBus};
use crate::ws::{Channel, WsEvent};
use g5_api_foundation::contracts::tasks::{
    SpecContextDTO, TaskDTO as TasksRouteTaskDTO, TaskExecutionAttemptDTO, TimelineEntryDTO,
};

/// A task's dependency ids as the `depends_on` column. They come from
/// `task_dependencies`, the only dependency authority
/// (`g5.agents.dependencies`); allocation leaves the legacy `tasks.depends_on`
/// array empty.
pub(crate) const TASK_DEPENDS_ON_SQL: &str = "ARRAY(SELECT edge.depends_on_task_id \
     FROM task_dependencies AS edge \
     WHERE edge.task_id = tasks.id \
     ORDER BY edge.depends_on_task_id) AS depends_on";

#[derive(Clone)]
pub struct TaskService {
    pub(crate) event_bus: EventBus,
    system_pool: PgPool,
    config_service: ConfigService,
    pub(crate) workspace_runtime:
        Arc<crate::services::workspace::workspace_runtime::WorkspaceRuntime>,
    /// Followers of live attempts, poked when a control fact changes so a
    /// cancellation the route could not deliver is redelivered.
    pub(crate) followers: Arc<super::attempt_follower::FollowerRegistry>,
}

impl TaskService {
    pub async fn find_active_task_for_session(
        &self,
        db: &mut ScopedDb,
        session_id: Uuid,
    ) -> Result<Option<(Uuid, String)>, AppError> {
        sqlx::query_as(
            "SELECT id, status FROM tasks
             WHERE chat_session_id = $1
               AND status IN ('queued','running','waiting')
             ORDER BY created_at DESC, id DESC
             LIMIT 1",
        )
        .bind(session_id)
        .fetch_optional(db.conn())
        .await
        .map_err(AppError::from)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        event_bus: EventBus,
        _repo_storage: TenantRepoStorage,
        system_pool: PgPool,
        config_service: ConfigService,
        _scm_token_manager: Option<Arc<crate::auth::ScmTokenManager>>,
        workspace_runtime: Arc<crate::services::workspace::workspace_runtime::WorkspaceRuntime>,
        _external_url: String,
    ) -> Self {
        Self {
            event_bus,
            system_pool,
            config_service,
            workspace_runtime,
            followers: Arc::default(),
        }
    }

    /// Share the process's follower registry (`AppState` construction).
    #[must_use]
    pub fn with_followers(
        mut self,
        followers: Arc<super::attempt_follower::FollowerRegistry>,
    ) -> Self {
        self.followers = followers;
        self
    }

    pub(crate) fn dispatch_system_pool(&self) -> &PgPool {
        &self.system_pool
    }

    pub(crate) fn dispatch_config_service(&self) -> &ConfigService {
        &self.config_service
    }

    /// Hint, once `db` commits, that this agent task's chat changed turn
    /// activity (chat g5.chat.activity.refresh); nothing for other kinds.
    pub(crate) async fn announce_chat_activity(
        &self,
        db: &mut ScopedDb,
        task_id: Uuid,
    ) -> Result<(), AppError> {
        super::chat_activity::announce_task(db, &self.event_bus, task_id).await
    }

    /// [`Self::announce_chat_activity`] for one chat by id.
    pub(crate) async fn announce_session_chat_activity(
        &self,
        db: &mut ScopedDb,
        session_id: Uuid,
    ) -> Result<(), AppError> {
        super::chat_activity::announce_chat(db, &self.event_bus, session_id).await
    }

    pub fn publish_task_status(&self, publish: &TaskStatusPublish) {
        let event = WsEvent::TaskStatus {
            task_id: publish.task_id,
            status: publish.status.clone(),
        };
        self.event_bus.publish_to_many(
            &[
                Channel::Task(publish.task_id),
                Channel::Workspace(publish.workspace_id),
            ],
            &event,
        );
    }

    pub async fn get_task(
        &self,
        db: &mut ScopedDb,
        task_id: Uuid,
    ) -> Result<TasksRouteTaskDTO, AppError> {
        let task = sqlx::query_as::<_, Task>(&format!(
            "SELECT id, org_id, name, kind AS task_type, status, \
             base_sha AS base_commit, retry_count, \
             {TASK_DEPENDS_ON_SQL}, input_tokens, output_tokens, cost_usd, created_at, updated_at, \
             component_id, src_files, node_level, language, parent_component_id, \
             child_component_ids, max_duration_secs, chat_session_id \
             FROM tasks WHERE id = $1",
        ))
        .bind(task_id)
        .fetch_optional(db.conn())
        .await?
        .ok_or_else(|| G5Error::NotFound {
            resource: format!("task {task_id}"),
        })?;

        let context = sqlx::query(
            "SELECT current_task.workspace_id, workspace.name AS workspace_name, \
                    current_task.kind, current_task.chat_session_id, current_task.started_at, \
                    current_task.ended_at \
             FROM tasks AS current_task \
             JOIN workspaces AS workspace ON workspace.id = current_task.workspace_id \
             WHERE current_task.id = $1",
        )
        .bind(task_id)
        .fetch_one(db.conn())
        .await?;
        let kind = context.try_get::<String, _>("kind")?;
        let session_id = context.try_get::<Option<Uuid>, _>("chat_session_id")?;
        let started_at = context.try_get::<Option<DateTime<Utc>>, _>("started_at")?;
        let ended_at = context.try_get::<Option<DateTime<Utc>>, _>("ended_at")?;
        let latest_attempt = self.latest_attempt(db, task_id).await?;
        let lifecycle_posture = lifecycle_posture(&task.status, latest_attempt.as_ref());

        Ok(TasksRouteTaskDTO {
            id: task.id.to_string(),
            name: task.name,
            task_type: kind,
            status: task.status,
            workspace_id: context.try_get::<Uuid, _>("workspace_id")?.to_string(),
            workspace_name: context.try_get("workspace_name")?,
            node_level: task.node_level,
            spec_context: task.component_id.map(|component_id| SpecContextDTO {
                component_id,
                src_files: task.src_files,
                parent_spec: task.parent_component_id,
                child_specs: task.child_component_ids,
            }),
            depends_on: task
                .depends_on
                .into_iter()
                .map(|id| id.to_string())
                .collect(),
            files_changed: Vec::new(),
            cost_usd: task.cost_usd.to_f64().unwrap_or_default(),
            session_id: session_id.map(|id| id.to_string()),
            assigned_at: started_at.map(|at| at.to_rfc3339()),
            completed_at: ended_at.map(|at| at.to_rfc3339()),
            lifecycle_posture,
            latest_attempt,
            background_wait: None,
            native_steering_available: false,
        })
    }

    /// The running attempt's latest reported background set, when non-empty
    /// (`g5.conversation.providers.claude-background.visibility`). The set is
    /// journaled only when it changes, so the newest row is authoritative.
    pub async fn live_background_wait(
        &self,
        db: &mut ScopedDb,
        task_id: Uuid,
        attempt: i32,
    ) -> Result<Option<serde_json::Value>, AppError> {
        let payload: Option<serde_json::Value> = sqlx::query_scalar(
            "SELECT metadata -> 'payload' FROM task_events
             WHERE task_id = $1 AND attempt = $2
               AND metadata ->> 'kind' = 'background_tasks.replaced'
             ORDER BY seq DESC
             LIMIT 1",
        )
        .bind(task_id)
        .bind(attempt)
        .fetch_optional(db.conn())
        .await?;
        Ok(payload.filter(|payload| payload["count"].as_u64().unwrap_or(0) > 0))
    }

    async fn latest_attempt(
        &self,
        db: &mut ScopedDb,
        task_id: Uuid,
    ) -> Result<Option<TaskExecutionAttemptDTO>, AppError> {
        let row = sqlx::query(
            "SELECT attempt, vm_id, policy_snapshot_ref, started_at, ended_at, outcome, reason \
             FROM task_attempts WHERE task_id = $1 ORDER BY attempt DESC LIMIT 1",
        )
        .bind(task_id)
        .fetch_optional(db.conn())
        .await?;
        row.map(|row| task_attempt_to_dto(task_id, &row))
            .transpose()
    }

    pub async fn list_task_attempts(
        &self,
        db: &mut ScopedDb,
        task_id: Uuid,
        before_attempt: Option<i32>,
        limit: i64,
    ) -> Result<(Vec<TaskExecutionAttemptDTO>, Option<i32>, bool), AppError> {
        let limit = limit.clamp(1, 100);
        let rows = sqlx::query(
            "SELECT attempt, vm_id, policy_snapshot_ref, started_at, ended_at, outcome, reason \
             FROM task_attempts \
             WHERE task_id = $1 AND ($2::integer IS NULL OR attempt < $2) \
             ORDER BY attempt DESC LIMIT $3",
        )
        .bind(task_id)
        .bind(before_attempt)
        .bind(limit + 1)
        .fetch_all(db.conn())
        .await?;
        let has_more = rows.len() as i64 > limit;
        let attempts = rows
            .iter()
            .take(limit as usize)
            .map(|row| task_attempt_to_dto(task_id, row))
            .collect::<Result<Vec<_>, _>>()?;
        let cursor = has_more
            .then(|| attempts.last().map(|attempt| attempt.attempt_number))
            .flatten();
        Ok((attempts, cursor, has_more))
    }

    pub async fn get_task_timeline(
        &self,
        db: &mut ScopedDb,
        task_id: Uuid,
        cursor: Option<String>,
        limit: i64,
    ) -> Result<(Vec<TimelineEntryDTO>, Option<String>, bool), AppError> {
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM tasks WHERE id = $1)")
            .bind(task_id)
            .fetch_one(db.conn())
            .await?;
        if !exists {
            return Err(G5Error::NotFound {
                resource: format!("task {task_id}"),
            }
            .into());
        }
        let limit = limit.clamp(1, 200);
        let (cursor_at, cursor_id) = parse_timeline_cursor(cursor.as_deref())?;
        let rows = sqlx::query(
            "SELECT event.id, event.entry_type, event.content,
                CASE WHEN request.id IS NULL THEN event.metadata ELSE
                    event.metadata || jsonb_build_object('nativeRequestStatus', request.status,
                        'nativeRequestEndReason', request.end_reason,
                        'nativeRequestAnswer', request.response -> 'body') END AS metadata,
                event.created_at
             FROM task_events event
             LEFT JOIN provider_requests request ON request.task_id = event.task_id
                AND request.attempt = event.attempt
                AND request.provider_request_id = event.metadata #>> '{payload,provider_request_id}'
                AND event.metadata ->> 'kind' = 'provider_request.opened'
             WHERE event.task_id = $1
               AND ($2::timestamptz IS NULL OR event.created_at > $2
                    OR (event.created_at = $2 AND event.id > $3))
             ORDER BY event.created_at, event.id LIMIT $4",
        )
        .bind(task_id)
        .bind(cursor_at)
        .bind(cursor_id.unwrap_or_else(Uuid::nil))
        .bind(limit + 1)
        .fetch_all(db.conn())
        .await?;
        let has_more = rows.len() as i64 > limit;
        let entries = rows
            .iter()
            .take(limit as usize)
            .map(|row| {
                Ok(TimelineEntryDTO {
                    id: row.try_get::<Uuid, _>("id")?.to_string(),
                    entry_type: row.try_get("entry_type")?,
                    content: row.try_get("content")?,
                    metadata: row.try_get("metadata")?,
                    created_at: row.try_get::<DateTime<Utc>, _>("created_at")?.to_rfc3339(),
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()?;
        let next_cursor = if has_more {
            let row = &rows[limit as usize - 1];
            Some(format!(
                "{}|{}",
                row.try_get::<DateTime<Utc>, _>("created_at")?
                    .to_rfc3339_opts(chrono::SecondsFormat::Micros, true),
                row.try_get::<Uuid, _>("id")?
            ))
        } else {
            None
        };
        Ok((entries, next_cursor, has_more))
    }

    pub async fn list_workspace_tasks(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
        cursor: Option<&str>,
        active_only: bool,
        limit: i64,
    ) -> Result<
        (
            Vec<g5_api_foundation::contracts::tasks::TaskListEntry>,
            Option<String>,
            bool,
        ),
        AppError,
    > {
        let (before_time, before_id) = parse_timeline_cursor(cursor)?;
        let limit = limit.clamp(1, 200);
        let mut rows = sqlx::query(
            "SELECT id, name, kind, status, workspace_id, chat_session_id, cost_usd, created_at, updated_at,
                    outcome
             FROM tasks WHERE workspace_id = $1
               AND (NOT $2 OR status IN ('queued','running','waiting'))
               AND ($3::timestamptz IS NULL OR (created_at, id) < ($3, $4))
             ORDER BY created_at DESC, id DESC LIMIT $5",
        )
        .bind(workspace_id).bind(active_only).bind(before_time).bind(before_id).bind(limit + 1)
        .fetch_all(db.conn()).await?;
        let has_more = rows.len() > limit as usize;
        rows.truncate(limit as usize);
        let cursor = if has_more {
            rows.last()
                .map(|row| -> Result<String, sqlx::Error> {
                    Ok(format!(
                        "{}|{}",
                        row.try_get::<DateTime<Utc>, _>("created_at")?.to_rfc3339(),
                        row.try_get::<Uuid, _>("id")?
                    ))
                })
                .transpose()?
        } else {
            None
        };
        let tasks = rows
            .into_iter()
            .map(|row| -> Result<_, sqlx::Error> {
                let status: String = row.try_get("status")?;
                let outcome: Option<serde_json::Value> = row.try_get("outcome")?;
                Ok(g5_api_foundation::contracts::tasks::TaskListEntry {
                    failure: g5_api_foundation::contracts::tasks::TaskFailure::from_outcome(
                        &status,
                        outcome.as_ref(),
                    ),
                    id: row.try_get("id")?,
                    name: row.try_get("name")?,
                    task_type: row.try_get("kind")?,
                    status,
                    workspace_id: row.try_get("workspace_id")?,
                    session_id: row.try_get("chat_session_id")?,
                    cost_usd: row
                        .try_get::<rust_decimal::Decimal, _>("cost_usd")?
                        .to_f64()
                        .unwrap_or_default(),
                    created_at: row.try_get::<DateTime<Utc>, _>("created_at")?.to_rfc3339(),
                    updated_at: row.try_get::<DateTime<Utc>, _>("updated_at")?.to_rfc3339(),
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok((tasks, cursor, has_more))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskStatusPublish {
    pub task_id: Uuid,
    pub workspace_id: Uuid,
    pub status: String,
}

fn parse_timeline_cursor(
    cursor: Option<&str>,
) -> Result<(Option<DateTime<Utc>>, Option<Uuid>), AppError> {
    let Some(cursor) = cursor else {
        return Ok((None, None));
    };
    let (timestamp, id) = cursor
        .split_once('|')
        .ok_or_else(|| G5Error::ValidationError {
            details: serde_json::json!({"cursor":"invalid cursor format"}),
        })?;
    let timestamp = timestamp.parse().map_err(|_| G5Error::ValidationError {
        details: serde_json::json!({"cursor":"invalid cursor timestamp"}),
    })?;
    let id = id.parse().map_err(|_| G5Error::ValidationError {
        details: serde_json::json!({"cursor":"invalid cursor id"}),
    })?;
    Ok((Some(timestamp), Some(id)))
}

fn task_attempt_to_dto(
    task_id: Uuid,
    row: &sqlx::postgres::PgRow,
) -> Result<TaskExecutionAttemptDTO, AppError> {
    let attempt = row.try_get::<i32, _>("attempt")?;
    let started_at = row.try_get::<DateTime<Utc>, _>("started_at")?;
    let ended_at = row.try_get::<Option<DateTime<Utc>>, _>("ended_at")?;
    let outcome = row.try_get::<Option<serde_json::Value>, _>("outcome")?;
    let reason = row.try_get::<Option<String>, _>("reason")?;
    let policy = row.try_get::<Option<serde_json::Value>, _>("policy_snapshot_ref")?;
    let provider = crate::services::agents::attempt_snapshot::snapshot_launch_value(
        policy.as_ref(),
        "provider",
    )
    .map(str::to_string);
    let provider_session_id = outcome
        .as_ref()
        .and_then(|value| value.get("provider_session_id"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);
    let terminal = ended_at.is_some();
    let disposition = terminal.then(|| {
        match reason.as_deref() {
            Some("cancelled") => "cancelled",
            Some("interrupted" | "suspended" | "vm_lost") => "interrupted",
            Some("exit_code") => "completed",
            _ => "failed",
        }
        .to_string()
    });
    let retry_policy = terminal.then(|| {
        if matches!(
            reason.as_deref(),
            Some("interrupted" | "suspended" | "vm_lost")
        ) {
            "manual"
        } else {
            "none"
        }
        .to_string()
    });
    let lifecycle_posture = if !terminal {
        TaskLifecyclePosture::Active
    } else if retry_policy.as_deref() == Some("manual") {
        TaskLifecyclePosture::ActionRequired
    } else {
        TaskLifecyclePosture::Terminal
    };
    let mut identity = Vec::with_capacity(20);
    identity.extend_from_slice(task_id.as_bytes());
    identity.extend_from_slice(&attempt.to_be_bytes());
    Ok(TaskExecutionAttemptDTO {
        id: Uuid::new_v5(&Uuid::NAMESPACE_OID, &identity),
        attempt_number: attempt,
        provider,
        provider_session_id,
        disposition,
        reason,
        retry_policy,
        lifecycle_posture,
        started_at: started_at.to_rfc3339(),
        ended_at: ended_at.map(|at| at.to_rfc3339()),
        continuation_summary: outcome
            .as_ref()
            .and_then(|value| value.get("continuation_summary"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
    })
}

fn lifecycle_posture(
    status: &str,
    latest: Option<&TaskExecutionAttemptDTO>,
) -> TaskLifecyclePosture {
    match status {
        "queued" | "running" => TaskLifecyclePosture::Active,
        "waiting" => TaskLifecyclePosture::ActionRequired,
        "succeeded" | "failed" | "cancelled" => TaskLifecyclePosture::Terminal,
        _ => latest
            .map(|attempt| attempt.lifecycle_posture)
            .unwrap_or(TaskLifecyclePosture::Resumable),
    }
}
