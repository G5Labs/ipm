//! Authorize a live meter against the attempt that owns the provider process.
use g5_authz::ScopedDb;
use g5_contracts::types::{AgentAuthModeV1, ChatAgentProvider};
use g5_internal_api::subscription_usage::SubscriptionUsageReport;
use serde_json::Value;
use uuid::Uuid;

use crate::{errors::AppError, services::TaskService, ws::WsEvent};

impl TaskService {
    /// No persistence: a stale or non-subscription attempt produces no event.
    pub async fn subscription_usage_event(
        &self,
        db: &mut ScopedDb,
        session_id: Uuid,
        vm_id: Uuid,
        epoch: u64,
        report: SubscriptionUsageReport,
    ) -> Result<Option<WsEvent>, AppError> {
        if !report.usage.valid() {
            return Ok(None);
        }
        let snapshot: Option<Value> = sqlx::query_scalar(
            "SELECT attempt.policy_snapshot_ref
             FROM tasks task JOIN task_attempts attempt
               ON attempt.task_id=task.id AND attempt.attempt=task.attempt
             WHERE task.id=$1 AND task.attempt=$2 AND task.chat_session_id=$3
               AND task.kind='agent' AND task.status='running'
               AND task.vm_id=$4 AND task.epoch=$5 AND attempt.ended_at IS NULL
               AND attempt.vm_id=task.vm_id AND attempt.epoch=task.epoch
               AND COALESCE(task.spec #>> '{lifecycle,cancel_requested}', 'false') <> 'true'
               AND task.spec #>> '{lifecycle,interrupt_reason}' IS NULL
             FOR SHARE OF task, attempt",
        )
        .bind(report.task_id)
        .bind(i64::from(report.attempt))
        .bind(session_id)
        .bind(vm_id.to_string())
        .bind(epoch as i64)
        .fetch_optional(db.conn())
        .await?
        .flatten();
        let Some(launch) = snapshot
            .as_ref()
            .and_then(super::attempt_snapshot::resolved_launch)
        else {
            return Ok(None);
        };
        if !matches!(
            (launch.provider.value, launch.auth_mode),
            (
                ChatAgentProvider::ClaudeCode,
                AgentAuthModeV1::AnthropicOauth
            ) | (
                ChatAgentProvider::Codex,
                AgentAuthModeV1::ChatGptSubscription
            )
        ) {
            return Ok(None);
        }
        Ok(Some(WsEvent::ChatSubscriptionUsage {
            session_id,
            auth_mode: launch.auth_mode,
            account_id: launch.account_id,
            usage: report.usage,
        }))
    }
}
