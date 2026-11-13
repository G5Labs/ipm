//! TaskService owns the login gate; the VM owns the CLI and credential cache.
//! Ready serializes under the task lock, before any provider input. A message
//! sent during login never touches the gate: it queues behind the original
//! prompt (`g5.chat.login-before-prompt`). The `superseded` phase is only
//! read, for attempts a message cancelled before follow-ups queued.
use chrono::Utc;
use g5_authz::ScopedDb;
use g5_contracts::error::G5Error;
use g5_internal_api::codex_auth::{
    auth_notice, valid_device_code, CodexAuthDisposition, CodexAuthPhase, CodexAuthUpdate,
    DEVICE_AUTH_URL, DEVICE_AUTH_WAIT_MS,
};
use serde::Serialize;
use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;

use crate::errors::AppError;
use crate::services::TaskService;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexAuthView {
    task_id: Uuid,
    attempt: i32,
    phase: String,
    deadline_unix_ms: Option<u64>,
    verification_url: Option<&'static str>,
    user_code: Option<String>,
    message: Option<&'static str>,
}

impl TaskService {
    #[allow(clippy::too_many_arguments)]
    pub async fn update_codex_auth(
        &self,
        db: &mut ScopedDb,
        session_id: Uuid,
        owner_id: Uuid,
        vm_id: Uuid,
        epoch: u64,
        input: CodexAuthUpdate,
    ) -> Result<CodexAuthDisposition, AppError> {
        let row = sqlx::query(
            "SELECT task.spec, attempt.policy_snapshot_ref, attempt.codex_auth_state
             FROM tasks task
             JOIN task_attempts attempt ON attempt.task_id = task.id AND attempt.attempt = task.attempt
             JOIN workspaces workspace ON workspace.id = task.workspace_id
             WHERE task.id = $1 AND task.attempt = $2 AND task.chat_session_id = $3
               AND task.kind = 'agent' AND task.status = 'running'
               AND task.vm_id = $4 AND task.epoch = $5 AND attempt.ended_at IS NULL
               AND attempt.vm_id = task.vm_id AND attempt.epoch = task.epoch
               AND workspace.owner_user_id = $6 AND workspace.deleted_at IS NULL
               AND NOT EXISTS (SELECT 1 FROM workspace_ownership_transfers transfer
                   WHERE transfer.workspace_id = workspace.id AND transfer.status = 'pending')
             FOR UPDATE OF task, attempt",
        )
        .bind(input.task_id).bind(input.attempt).bind(session_id)
        .bind(vm_id.to_string()).bind(epoch as i64).bind(owner_id)
        .fetch_optional(db.conn()).await?;
        let Some(row) = row else {
            return Ok(disposition(false, &Value::Null));
        };
        let policy: Value = row.try_get("policy_snapshot_ref")?;
        if crate::services::agents::attempt_snapshot::resolved_launch(&policy)
            .map(|launch| launch.auth_mode)
            != Some(g5_contracts::types::AgentAuthModeV1::ChatGptSubscription)
        {
            return Err(conflict(
                "device login requires the frozen ChatGPT subscription route",
            ));
        }
        let spec: Value = row.try_get("spec")?;
        let previous: Value = row
            .try_get::<Option<Value>, _>("codex_auth_state")?
            .unwrap_or(Value::Null);
        // `superseded`: a historical attempt that a message cancelled before
        // follow-ups queued during login; it never resumes.
        if spec.pointer("/lifecycle/interrupt_reason").is_some()
            || spec
                .pointer("/lifecycle/cancel_requested")
                .and_then(Value::as_bool)
                == Some(true)
            || matches!(previous["phase"].as_str(), Some("ended" | "superseded"))
        {
            return Ok(disposition(false, &previous));
        }
        let now = Utc::now().timestamp_millis() as u64;
        let expired = previous["phase"] == "waiting"
            && previous["deadline_unix_ms"]
                .as_u64()
                .is_none_or(|deadline| deadline <= now);
        let (state, secret, proceed) = if expired {
            (
                json!({"phase":"ended", "code":"codex_auth_timeout"}),
                None,
                false,
            )
        } else {
            match input.update {
                CodexAuthPhase::Checking if previous.is_null() => {
                    (json!({"phase":"checking"}), None, true)
                }
                CodexAuthPhase::Checking => return Ok(disposition(true, &previous)),
                CodexAuthPhase::Waiting {
                    user_code,
                    deadline_unix_ms,
                } => {
                    if !valid_device_code(&user_code)
                        || deadline_unix_ms <= now
                        || deadline_unix_ms > now.saturating_add(DEVICE_AUTH_WAIT_MS + 15_000)
                    {
                        return Err(conflict("invalid device authorization challenge"));
                    }
                    if previous["phase"] == "waiting" {
                        // A retried POST may acknowledge the same challenge, but cannot
                        // replace it or extend the original deadline.
                        let existing: Option<Vec<u8>> = sqlx::query_scalar(
                            "SELECT codex_auth_challenge_enc FROM task_attempts WHERE task_id=$1 AND attempt=$2"
                        ).bind(input.task_id).bind(input.attempt).fetch_one(db.conn()).await?;
                        let same = previous["deadline_unix_ms"] == deadline_unix_ms
                            && existing
                                .as_deref()
                                .map(|bytes| {
                                    self.dispatch_config_service()
                                        .decrypt_task_credential(bytes)
                                })
                                .transpose()?
                                .as_deref()
                                == Some(user_code.as_str());
                        return Ok(disposition(same, &previous));
                    }
                    if previous["phase"] != "checking" {
                        return Ok(disposition(false, &previous));
                    }
                    let encrypted = self
                        .dispatch_config_service()
                        .encrypt_task_credential(&user_code)?;
                    (
                        json!({"phase":"waiting", "deadline_unix_ms":deadline_unix_ms}),
                        Some(encrypted),
                        true,
                    )
                }
                CodexAuthPhase::Ready => {
                    if !matches!(
                        previous["phase"].as_str(),
                        Some("checking" | "waiting" | "ready")
                    ) {
                        return Ok(disposition(false, &previous));
                    }
                    (json!({"phase":"ready"}), None, true)
                }
                CodexAuthPhase::Ended { code } => {
                    if terminal_auth_code(&code).is_none() {
                        return Err(conflict("invalid device authorization outcome"));
                    }
                    (json!({"phase":"ended", "code":code}), None, false)
                }
            }
        };
        sqlx::query("UPDATE task_attempts SET codex_auth_state=$3, codex_auth_challenge_enc=$4 WHERE task_id=$1 AND attempt=$2")
            .bind(input.task_id).bind(input.attempt).bind(&state).bind(secret).execute(db.conn()).await?;
        Ok(disposition(proceed, &state))
    }

    pub async fn session_codex_auth(
        &self,
        db: &mut ScopedDb,
        session_id: Uuid,
        actor_id: Uuid,
    ) -> Result<Option<CodexAuthView>, AppError> {
        // Fence the owner check and subsequent private challenge read against
        // ownership transfer. A weaker key lock does not protect owner_user_id.
        // The HTTP route explicitly closes this short transaction before reply.
        let owner: Option<Uuid> = sqlx::query_scalar(
            "SELECT workspace.owner_user_id FROM chat_sessions session
             JOIN workspaces workspace ON workspace.id=session.workspace_id
             WHERE session.id=$1 FOR SHARE OF workspace",
        )
        .bind(session_id)
        .fetch_one(db.conn())
        .await?;
        if owner != Some(actor_id) {
            return Err(AppError::from_error(G5Error::Forbidden {
                missing_scopes: vec![],
            }));
        }
        // clock_timestamp(): this transaction may have waited for the workspace
        // row (in its authorization or the owner check above), and now() is
        // when it began.
        let row = sqlx::query(
            "SELECT task.id, attempt.attempt, attempt.codex_auth_state, attempt.codex_auth_challenge_enc,
                    task.status = 'running' AND attempt.ended_at IS NULL
                    AND vm.vm_id = attempt.vm_id AND vm.epoch = attempt.epoch
                    AND vm.status = 'running' AND vm.lease_expires_at > clock_timestamp()
                    AND task.created_by = $2 AS live
             FROM tasks task JOIN task_attempts attempt ON attempt.task_id=task.id AND attempt.attempt=task.attempt
             LEFT JOIN workspace_vms vm ON vm.workspace_id=task.workspace_id
             WHERE task.chat_session_id=$1
             ORDER BY task.created_at DESC, task.id DESC LIMIT 1"
        ).bind(session_id).bind(actor_id).fetch_optional(db.conn()).await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let Some(state) = row.try_get::<Option<Value>, _>("codex_auth_state")? else {
            return Ok(None);
        };
        let mut phase = state["phase"].as_str().unwrap_or("ended").to_string();
        if phase == "ready" {
            return Ok(None);
        }
        let deadline = state["deadline_unix_ms"].as_u64();
        let mut code = state["code"].as_str();
        if matches!(phase.as_str(), "waiting" | "checking") {
            if row.try_get::<Option<bool>, _>("live")? != Some(true) {
                phase = "ended".into();
                code = Some("codex_auth_interrupted");
            } else if deadline
                .is_some_and(|deadline| deadline <= Utc::now().timestamp_millis() as u64)
            {
                phase = "ended".into();
                code = Some("codex_auth_timeout");
            }
        }
        let user_code = if phase == "waiting" {
            row.try_get::<Option<Vec<u8>>, _>("codex_auth_challenge_enc")?
                .as_deref()
                .map(|bytes| {
                    self.dispatch_config_service()
                        .decrypt_task_credential(bytes)
                })
                .transpose()?
        } else {
            None
        };
        Ok(Some(CodexAuthView {
            task_id: row.try_get("id")?,
            attempt: row.try_get("attempt")?,
            verification_url: user_code.as_ref().map(|_| DEVICE_AUTH_URL),
            user_code,
            message: code.map(auth_notice),
            phase,
            deadline_unix_ms: deadline,
        }))
    }
}

pub(crate) fn terminal_auth_code(code: &str) -> Option<&'static str> {
    match code {
        "codex_auth_timeout" => Some("codex_auth_timeout"),
        "codex_auth_failed" => Some("codex_auth_failed"),
        "codex_auth_disabled" => Some("codex_auth_disabled"),
        "codex_auth_busy" => Some("codex_auth_busy"),
        "codex_auth_interrupted" => Some("codex_auth_interrupted"),
        _ => None,
    }
}

fn conflict(reason: &str) -> AppError {
    AppError::from_error(G5Error::Conflict {
        reason: reason.into(),
    })
}

fn disposition(proceed: bool, state: &Value) -> CodexAuthDisposition {
    CodexAuthDisposition {
        proceed,
        code: (!proceed).then(|| {
            state["code"]
                .as_str()
                .unwrap_or("codex_auth_interrupted")
                .to_string()
        }),
    }
}
