//! Durable routing of ordinary chat input to an already running Claude attempt.
//! The inbox stays queued until the worker journals a completed stdin flush.
use chrono::Utc;
use g5_authz::ScopedDb;
use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;

use super::{chat_service, task_lifecycle};
use crate::errors::AppError;
use crate::services::workspace::workspace_runtime::RuntimeRequest;
use crate::services::TaskService;

#[derive(Debug, Clone)]
pub struct ClaimedSteer {
    pub org_id: Uuid,
    pub workspace_id: Uuid,
    pub session_id: Uuid,
    pub actor_user_id: Uuid,
    pub task_id: Uuid,
    pub attempt: i32,
    pub vm_id: Uuid,
    pub epoch: u64,
    pub input_id: Uuid,
    pub reservation_id: Uuid,
    pub revision: i32,
    pub delivery_generation: i32,
    pub content: String,
}

impl ClaimedSteer {
    pub(super) fn binding(&self) -> Value {
        json!({"input_id":self.input_id,"reservation_id":self.reservation_id,
            "revision":self.revision,"delivery_generation":self.delivery_generation})
    }
}

#[derive(Clone, Copy, Debug)]
pub enum SteerDisposition {
    Shipped,
    NotSteerable,
    Unknown,
}

const CLAIM_SELECT: &str = "SELECT input.org_id, task.workspace_id, input.session_id,
    (SELECT (audit ->> 'actor_user_id')::uuid FROM jsonb_array_elements(input.delivery_audit) WITH ORDINALITY AS entries(audit, n)
     WHERE audit ->> 'event' = 'steering.reserved'
       AND (audit ->> 'delivery_generation')::integer = input.delivery_generation ORDER BY n DESC LIMIT 1) AS actor_user_id,
    input.task_id, input.attempt, task.vm_id::uuid AS vm_id, task.epoch,
    input.id, input.reservation_id, input.revision, input.delivery_generation, input.content
    FROM session_inputs input JOIN tasks task ON task.org_id = input.org_id AND task.id = input.task_id";

fn claim(row: sqlx::postgres::PgRow) -> Result<ClaimedSteer, sqlx::Error> {
    Ok(ClaimedSteer {
        org_id: row.try_get("org_id")?,
        workspace_id: row.try_get("workspace_id")?,
        session_id: row.try_get("session_id")?,
        actor_user_id: row.try_get("actor_user_id")?,
        task_id: row.try_get("task_id")?,
        attempt: row.try_get("attempt")?,
        vm_id: row.try_get("vm_id")?,
        epoch: u64::try_from(row.try_get::<i64, _>("epoch")?)
            .map_err(|_| sqlx::Error::Protocol("negative epoch".into()))?,
        input_id: row.try_get("id")?,
        reservation_id: row.try_get("reservation_id")?,
        revision: row.try_get("revision")?,
        delivery_generation: row.try_get("delivery_generation")?,
        content: row.try_get("content")?,
    })
}

impl TaskService {
    /// Complete an accepted steering send independently of its HTTP caller.
    /// Retrying database persistence never repeats the provider write.
    pub async fn deliver_claimed_steer(
        &self,
        deps: &crate::db::maintenance::TenantMaintenanceDeps,
        claim: &ClaimedSteer,
    ) -> Result<(), AppError> {
        let disposition = self.invoke_agent_steer(claim).await;
        for retry in 0..3 {
            let result = async {
                let mut db =
                    crate::db::maintenance::scoped_db_for_maintenance_org(deps, claim.org_id)
                        .await?;
                self.finish_agent_steer(&mut db, claim, disposition).await?;
                db.commit().await.map_err(AppError::from)
            }
            .await;
            match result {
                Ok(()) => return Ok(()),
                Err(error) if retry == 2 => return Err(error),
                Err(_) => tokio::time::sleep(std::time::Duration::from_millis(100)).await,
            }
        }
        unreachable!("the final persistence attempt returns")
    }

    /// Read-only composer hint. Admission is rechecked atomically on send.
    pub async fn native_steering_available(
        &self,
        db: &mut ScopedDb,
        task: Uuid,
    ) -> Result<bool, AppError> {
        let eligible = sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM tasks task
            JOIN task_attempts attempt ON attempt.task_id=task.id AND attempt.attempt=task.attempt
            WHERE task.id=$1 AND task.status='running' AND task.start_confirmed AND task.hold_reason IS NULL
              AND NOT COALESCE((task.spec #>> '{lifecycle,cancel_requested}')::boolean, false)
              AND task.spec #>> '{lifecycle,interrupt_reason}' IS NULL
              AND attempt.ended_at IS NULL AND task.vm_id IS NOT NULL AND task.epoch IS NOT NULL
              AND attempt.policy_snapshot_ref -> 'native_mid_turn_steering'='true'::jsonb
              AND EXISTS(SELECT 1 FROM session_inputs input WHERE input.task_id=task.id AND input.attempt=task.attempt
                  AND NOT input.steered AND input.delivery='delivered')
              AND NOT EXISTS(SELECT 1 FROM session_inputs input WHERE input.session_id=task.chat_session_id
                  AND input.delivery IN ('queued','reserved','delivery_unknown'))
              AND NOT EXISTS(SELECT 1 FROM provider_requests request WHERE request.task_id=task.id
                  AND request.status IN ('pending','reply_pending')))")
            .bind(task).fetch_one(db.conn()).await?;
        Ok(eligible)
    }
    /// Called after inline attachments have been persisted, in the same send
    /// transaction and under the same session allocation lock as enqueue.
    pub async fn reserve_agent_steer(
        &self,
        db: &mut ScopedDb,
        actor: Uuid,
        session: Uuid,
        input: Uuid,
    ) -> Result<Option<ClaimedSteer>, AppError> {
        chat_service::lock_session_turn_allocation(db.conn(), session).await?;
        let row = sqlx::query("SELECT task.id, task.org_id, task.attempt, attempt.policy_snapshot_ref
            FROM session_inputs input
            JOIN tasks task ON task.org_id=input.org_id AND task.id=input.task_id
            JOIN task_attempts attempt ON attempt.org_id=task.org_id AND attempt.task_id=task.id AND attempt.attempt=task.attempt
            JOIN workspaces workspace ON workspace.org_id=task.org_id AND workspace.id=task.workspace_id
            WHERE input.id=$1 AND input.session_id=$2 AND input.delivery='queued' AND NOT input.steered AND input.input_origin='human'
              AND input.artifacts='[]'::jsonb AND workspace.owner_user_id=$3
              AND NOT workspace.is_base AND workspace.deleted_at IS NULL
              AND task.kind='agent' AND task.status='running' AND task.hold_reason IS NULL
              AND NOT COALESCE((task.spec #>> '{lifecycle,cancel_requested}')::boolean, false)
              AND task.spec #>> '{lifecycle,interrupt_reason}' IS NULL
              AND task.start_confirmed AND task.vm_id IS NOT NULL AND task.epoch IS NOT NULL
              AND attempt.ended_at IS NULL AND attempt.vm_id=task.vm_id AND attempt.epoch=task.epoch
              AND attempt.policy_snapshot_ref -> 'native_mid_turn_steering' = 'true'::jsonb
              AND EXISTS (SELECT 1 FROM session_inputs initial WHERE initial.task_id=task.id
                  AND initial.attempt=task.attempt AND NOT initial.steered AND initial.delivery='delivered')
              AND NOT EXISTS (SELECT 1 FROM session_inputs older WHERE older.session_id=input.session_id
                  AND older.input_index<input.input_index AND older.delivery IN ('queued','reserved','delivery_unknown'))
              AND NOT EXISTS (SELECT 1 FROM provider_requests request WHERE request.task_id=task.id
                  AND request.status IN ('pending','reply_pending'))
            FOR UPDATE OF input, task, attempt FOR SHARE OF workspace")
            .bind(input).bind(session).bind(actor).fetch_optional(db.conn()).await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let snapshot: Value = row.try_get("policy_snapshot_ref")?;
        if super::attempt_snapshot::resolved_launch(&snapshot).is_none_or(|launch| {
            launch.provider.value != g5_contracts::types::ChatAgentProvider::ClaudeCode
        }) || self
            .live_background_wait(db, row.try_get("id")?, row.try_get("attempt")?)
            .await?
            .is_some()
        {
            return Ok(None);
        }
        sqlx::query("UPDATE session_inputs SET steered=true,
            attempt=$2, reservation_id=$3, reservation_revision=revision, delivery_generation=delivery_generation+1,
            delivery_audit=delivery_audit || jsonb_build_array(jsonb_build_object(
                'event','steering.reserved','actor_user_id',$4::uuid,'delivery_generation',delivery_generation+1,
                'task_id',task_id,'attempt',$2::integer,'reservation_id',$3::uuid,'revision',revision))
            WHERE id=$1 AND delivery='queued'")
            .bind(input).bind(row.try_get::<i32,_>("attempt")?).bind(Uuid::new_v4()).bind(actor)
            .execute(db.conn()).await?;
        let row = sqlx::query(&format!("{CLAIM_SELECT} WHERE input.id=$1"))
            .bind(input)
            .fetch_one(db.conn())
            .await?;
        Ok(Some(claim(row)?))
    }

    pub async fn invoke_agent_steer(&self, claim: &ClaimedSteer) -> SteerDisposition {
        tokio::time::timeout(
            std::time::Duration::from_secs(45),
            self.send_agent_steer(claim),
        )
        .await
        .unwrap_or(SteerDisposition::Unknown)
    }

    async fn send_agent_steer(&self, claim: &ClaimedSteer) -> SteerDisposition {
        let Ok(handle) = self
            .workspace_runtime
            .owner_handle(claim.workspace_id, claim.actor_user_id)
            .await
        else {
            return SteerDisposition::Unknown;
        };
        if handle.org_id != claim.org_id
            || handle.vm_id != claim.vm_id
            || handle.epoch != claim.epoch
        {
            return SteerDisposition::Unknown;
        }
        let request_id = format!("{}:{}", claim.input_id, claim.delivery_generation);
        let reply = self.workspace_runtime.modules_input_steer(&handle,
            &RuntimeRequest::lifecycle(claim.workspace_id, claim.vm_id, claim.epoch,
                Utc::now().timestamp_millis().max(0) as u64 + 45_000, request_id.clone()),
            json!({"exec_id":task_lifecycle::canonical_exec_id(claim.task_id, claim.attempt, claim.vm_id),
                "task_id":claim.task_id,"attempt":claim.attempt,"request_id":request_id,
                "input_id":claim.input_id,"reservation_id":claim.reservation_id,"revision":claim.revision,
                "delivery_generation":claim.delivery_generation,"content":claim.content})).await;
        match reply {
            Ok(reply) if reply["disposition"] == "shipped" => SteerDisposition::Shipped,
            Ok(reply) if reply["disposition"] == "not_steerable" => SteerDisposition::NotSteerable,
            // An older guest does not expose this method and wrote nothing.
            Err(error) if error.code == -32601 => SteerDisposition::NotSteerable,
            _ => SteerDisposition::Unknown,
        }
    }

    pub async fn finish_agent_steer(
        &self,
        db: &mut ScopedDb,
        claim: &ClaimedSteer,
        disposition: SteerDisposition,
    ) -> Result<String, AppError> {
        chat_service::lock_session_turn_allocation(db.conn(), claim.session_id).await?;
        if matches!(disposition, SteerDisposition::Shipped) {
            task_lifecycle::record_steering_flush(db, claim).await?;
        }
        apply_disposition(
            db,
            claim.task_id,
            claim.attempt,
            &json!({
                "input_id":claim.input_id,"reservation_id":claim.reservation_id,
                "revision":claim.revision,"delivery_generation":claim.delivery_generation,
            }),
            disposition,
        )
        .await?;
        Ok(
            sqlx::query_scalar("SELECT delivery FROM session_inputs WHERE id=$1")
                .bind(claim.input_id)
                .fetch_one(db.conn())
                .await?,
        )
    }
}

pub(super) async fn apply_disposition(
    db: &mut ScopedDb,
    task: Uuid,
    attempt: i32,
    payload: &Value,
    disposition: SteerDisposition,
) -> Result<(), AppError> {
    let id = payload["input_id"]
        .as_str()
        .and_then(|s| Uuid::parse_str(s).ok());
    let reservation = payload["reservation_id"]
        .as_str()
        .and_then(|s| Uuid::parse_str(s).ok());
    let revision = payload["revision"]
        .as_i64()
        .and_then(|n| i32::try_from(n).ok());
    let generation = payload["delivery_generation"]
        .as_i64()
        .and_then(|n| i32::try_from(n).ok());
    // finish_agent_steer retains positive RPC evidence. Journal order or
    // settlement projects it; a negative completion cannot erase it.
    // Every other result leaves ordinary queued work; a late ambiguous write
    // may duplicate it, as accepted by the owner, but never claims a new binding.
    if matches!(disposition, SteerDisposition::Shipped) {
        return Ok(());
    }
    sqlx::query("UPDATE session_inputs SET steered=false,
        attempt=NULL, reservation_id=NULL, reservation_revision=NULL,
        delivery_audit=delivery_audit || jsonb_build_array(jsonb_build_object('event','steering.queued'))
        WHERE id=$1 AND task_id=$2 AND attempt=$3 AND reservation_id=$4
          AND revision=$5 AND reservation_revision=$5 AND delivery_generation=$6
          AND steered AND delivery='queued' AND steering_flushed_at IS NULL")
        .bind(id).bind(task).bind(attempt).bind(reservation).bind(revision).bind(generation)
        .execute(db.conn()).await?;
    Ok(())
}
