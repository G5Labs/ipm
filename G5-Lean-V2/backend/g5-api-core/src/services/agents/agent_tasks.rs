//! Dispatcher-journal reconciliation for workspace-bound tasks.
//!
//! The API database is authoritative for logical task state.  The VM journal
//! is authoritative for process facts.  This loop claims live attempts and
//! hands each to an [attempt follower](super::attempt_follower), which
//! streams the journal live, stores it in batches and acknowledges only a
//! committed prefix so an API crash can safely repeat either side
//! (`g5.chat.live-delivery`, `g5.chat.history-batches`). Lost or replaced
//! incarnations are settled here from the database alone, and
//! acknowledgements that failed after an attempt ended are retried here
//! (`g5.chat.journal`).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use serde_json::Value;
use sqlx::Row;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::services::agents::attempt_follower::FollowedAttempt;
#[cfg(test)]
use crate::services::task_lifecycle::normalize_agent_turn_event_kind;
use crate::services::task_lifecycle::{canonical_exec_id, TaskLifecycleEvent};
#[cfg(test)]
use crate::services::task_lifecycle::{journal_event_from_replay, JournalTaskEvent};
use crate::services::workspace_runtime::RuntimeRequest;
use crate::services::AppState;

use crate::maintenance::{
    maintenance_org_ids, scoped_db_for_maintenance_org, TenantMaintenanceDeps,
};

const RECONCILE_INTERVAL: Duration = Duration::from_secs(2);
const RECONCILE_BATCH: i64 = 32;
const RPC_BUDGET: Duration = Duration::from_secs(15);
static DISPATCH_ORG_CURSOR: AtomicUsize = AtomicUsize::new(0);
static ACTIVE_ATTEMPT_ORG_CURSOR: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug)]
struct ActiveAttempt {
    org_id: Uuid,
    workspace_id: Uuid,
    task_id: Uuid,
    session_id: Option<Uuid>,
    attempt: i32,
    vm_id: Uuid,
    epoch: u64,
    acked_event_seq: u64,
    start_confirmed: bool,
    cancel_requested: bool,
    interrupt_reason: Option<String>,
}

pub fn spawn(state: Arc<AppState>, cancel: CancellationToken) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(RECONCILE_INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                _ = interval.tick() => {
                    if let Err(error) = run_once(&state).await {
                        tracing::warn!(%error, "agent task journal reconciliation failed; retrying");
                    }
                }
            }
        }
        state.followers.shutdown();
    })
}

pub async fn run_once(state: &Arc<AppState>) -> Result<u64, crate::errors::AppError> {
    let attempts = load_active_attempts(state, RECONCILE_BATCH).await?;
    let mut applied = 0_u64;
    for attempt in attempts {
        match reconcile_attempt(state, &attempt).await {
            Ok(count) => applied = applied.saturating_add(count),
            Err(error) => tracing::warn!(
                task_id = %attempt.task_id,
                attempt = attempt.attempt,
                vm_id = %attempt.vm_id,
                %error,
                "agent task attempt reconciliation remains pending"
            ),
        }
    }
    // Acknowledgement arrears are guest round trips: they run on their own
    // task, one scan at a time, so a slow guest never delays this tick's
    // native requests or dispatch (`g5.chat.journal`).
    spawn_journal_ack_retry(state);
    applied = applied.saturating_add(reconcile_native_requests(state, RECONCILE_BATCH).await?);
    applied = applied.saturating_add(dispatch_ready_tasks(state, RECONCILE_BATCH).await?);
    applied =
        applied.saturating_add(cancel_tasks_left_by_ended_starts(state, RECONCILE_BATCH).await?);
    Ok(applied)
}

/// Start one acknowledgement retry scan unless one is still running.
/// Returns whether a scan started.
pub fn spawn_journal_ack_retry(state: &Arc<AppState>) -> bool {
    if !state.followers.begin_ack_retry() {
        return false;
    }
    let state = state.clone();
    tokio::spawn(async move {
        let outcome = retry_journal_acks(&state, RECONCILE_BATCH).await;
        state.followers.end_ack_retry();
        if let Err(error) = outcome {
            tracing::warn!(%error, "journal acknowledgement retry scan failed; the next tick repeats it");
        }
    });
    true
}

/// Repeat the guest acknowledgement of a persisted prefix the guest never
/// confirmed, including for attempts that already ended, while their VM
/// incarnation lives (`g5.chat.journal`). An incarnation that is gone took
/// its journal with it, so its arrears are closed. Claimed attempts are
/// acknowledged a few at a time, so one stalled guest holds neither the
/// scan nor the reconciler.
pub async fn retry_journal_acks(
    state: &Arc<AppState>,
    limit: i64,
) -> Result<u64, crate::errors::AppError> {
    let deps = TenantMaintenanceDeps {
        system_pool: state.system_pool.clone(),
        tenant_pool: state.tenant_pool.clone(),
        authz: state.authz.clone(),
        repo_storage: state.repo_storage.clone(),
    };
    let followed = state.followers.followed_attempts();
    let (followed_tasks, followed_attempts): (Vec<Uuid>, Vec<i32>) = followed.into_iter().unzip();
    let mut retried = 0_u64;
    for org_id in maintenance_org_ids(&state.system_pool).await? {
        // The claim's connection goes back to the pool before any guest
        // round trip: a held guest answer must not hold a connection the
        // tick or a batch needs.
        let mut db = scoped_db_for_maintenance_org(&deps, org_id).await?;
        let rows = sqlx::query(
            "WITH selected AS (
                 SELECT attempt.org_id, attempt.task_id, attempt.attempt
                 FROM task_attempts AS attempt
                 JOIN tasks AS task
                   ON task.org_id = attempt.org_id AND task.id = attempt.task_id
                 WHERE attempt.org_id = $2
                   AND attempt.journal_acked_seq < attempt.acked_event_seq
                   AND attempt.vm_id IS NOT NULL
                   AND NOT EXISTS (
                       SELECT 1 FROM unnest($3::uuid[], $4::int4[]) AS followed(task_id, attempt)
                       WHERE followed.task_id = attempt.task_id
                         AND followed.attempt = attempt.attempt)
                   AND (attempt.ended_at IS NOT NULL
                        OR attempt.last_reconciled_at < now() - interval '30 seconds')
                 ORDER BY attempt.last_reconciled_at
                 FOR UPDATE OF attempt SKIP LOCKED
                 LIMIT $1
             ), claimed AS (
                 UPDATE task_attempts AS attempt
                 SET last_reconciled_at = now()
                 FROM selected
                 WHERE attempt.org_id = selected.org_id
                   AND attempt.task_id = selected.task_id
                   AND attempt.attempt = selected.attempt
                 RETURNING attempt.*
             )
             SELECT attempt.org_id, task.workspace_id, task.id AS task_id,
                    task.chat_session_id, attempt.attempt, attempt.vm_id, attempt.epoch,
                    attempt.acked_event_seq, attempt.start_confirmed
             FROM claimed AS attempt
             JOIN tasks AS task
               ON task.org_id = attempt.org_id AND task.id = attempt.task_id",
        )
        .bind(limit.clamp(1, 128))
        .bind(org_id)
        .bind(&followed_tasks)
        .bind(&followed_attempts)
        .fetch_all(db.conn())
        .await?;
        db.commit().await?;
        let mut attempts = Vec::with_capacity(rows.len());
        for row in rows {
            let raw_vm_id: String = row.try_get("vm_id")?;
            let Ok(vm_id) = Uuid::parse_str(&raw_vm_id) else {
                continue;
            };
            attempts.push(FollowedAttempt {
                org_id: row.try_get("org_id")?,
                workspace_id: row.try_get("workspace_id")?,
                task_id: row.try_get("task_id")?,
                session_id: row.try_get("chat_session_id")?,
                attempt: row.try_get("attempt")?,
                vm_id,
                epoch: u64::try_from(row.try_get::<i64, _>("epoch")?).unwrap_or(0),
                persisted_through: u64::try_from(row.try_get::<i64, _>("acked_event_seq")?)
                    .unwrap_or(0),
                start_confirmed: row.try_get("start_confirmed")?,
            });
        }
        let outcomes: Vec<Result<bool, crate::errors::AppError>> = futures::stream::iter(attempts)
            .map(|attempt| retry_journal_ack(state, &deps, attempt))
            .buffer_unordered(ACK_RETRY_CONCURRENCY)
            .collect()
            .await;
        for outcome in outcomes {
            if outcome? {
                retried = retried.saturating_add(1);
            }
        }
    }
    Ok(retried)
}

/// Acknowledgements in flight per retry scan.
const ACK_RETRY_CONCURRENCY: usize = 4;

/// Acknowledge one attempt's persisted prefix to its live incarnation and
/// record the confirmation; an incarnation that is gone is confirmed as
/// closed. Returns whether the arrears were closed.
async fn retry_journal_ack(
    state: &Arc<AppState>,
    deps: &TenantMaintenanceDeps,
    attempt: FollowedAttempt,
) -> Result<bool, crate::errors::AppError> {
    let exec_id = attempt.exec_id();
    let handle = state
        .workspace_runtime
        .get(attempt.workspace_id)
        .await
        .map_err(crate::errors::AppError::from_error)?;
    let live = handle
        .as_ref()
        .is_some_and(|h| h.vm_id == attempt.vm_id && h.epoch == attempt.epoch);
    let confirmed = if let (true, Some(handle)) = (live, handle.as_ref()) {
        let permits = state.followers.side_call_permits();
        let _permit = permits.acquire().await;
        match state
            .workspace_runtime
            .modules_events_ack(
                handle,
                &lifecycle_request_for(
                    &attempt,
                    format!("agent-ack-retry:{exec_id}:{}", attempt.persisted_through),
                ),
                &exec_id,
                attempt.persisted_through,
            )
            .await
        {
            Ok(_) => true,
            Err(error) => {
                tracing::warn!(
                    target: "g5_api::live_relay",
                    task_id = %attempt.task_id,
                    attempt = attempt.attempt,
                    error = %error.message,
                    "journal acknowledgement retry failed"
                );
                false
            }
        }
    } else {
        // The incarnation and its journal are gone; nothing to prune.
        true
    };
    if !confirmed {
        return Ok(false);
    }
    crate::services::agents::attempt_follower::record_journal_ack(
        state,
        deps,
        &attempt,
        attempt.persisted_through,
    )
    .await?;
    Ok(true)
}

fn lifecycle_request_for(attempt: &FollowedAttempt, idempotency_key: String) -> RuntimeRequest {
    RuntimeRequest::lifecycle(
        attempt.workspace_id,
        attempt.vm_id,
        attempt.epoch,
        unix_time_ms().saturating_add(RPC_BUDGET.as_millis() as u64),
        idempotency_key,
    )
}

/// Cancel agent tasks left with no input after the Start-and-send consent
/// their input was bound to ended (`g5.personal.start-and-send`). The
/// `workspace_vms` trigger already withdrew the input in that transition, so
/// such a task can never dispatch; this only settles it.
pub async fn cancel_tasks_left_by_ended_starts(
    state: &Arc<AppState>,
    limit: i64,
) -> Result<u64, crate::errors::AppError> {
    let deps = TenantMaintenanceDeps {
        system_pool: state.system_pool.clone(),
        tenant_pool: state.tenant_pool.clone(),
        authz: state.authz.clone(),
        repo_storage: state.repo_storage.clone(),
    };
    let mut cancelled = 0_u64;
    for org_id in maintenance_org_ids(&state.system_pool).await? {
        let mut db = scoped_db_for_maintenance_org(&deps, org_id).await?;
        let rows =
            crate::services::agents::agent_dispatch::cancel_tasks_left_by_ended_starts_in_org(
                db.conn(),
                org_id,
                limit,
            )
            .await?;
        db.commit().await?;
        cancelled = cancelled.saturating_add(rows);
    }
    Ok(cancelled)
}

async fn reconcile_native_requests(
    state: &Arc<AppState>,
    limit: i64,
) -> Result<u64, crate::errors::AppError> {
    let deps = TenantMaintenanceDeps {
        system_pool: state.system_pool.clone(),
        tenant_pool: state.tenant_pool.clone(),
        authz: state.authz.clone(),
        repo_storage: state.repo_storage.clone(),
    };
    let mut reconciled = 0_u64;
    for org_id in maintenance_org_ids(&state.system_pool).await? {
        if reconciled >= limit.max(1) as u64 {
            break;
        }
        let remaining = limit.saturating_sub(reconciled as i64).max(1);
        let mut timeout_db = scoped_db_for_maintenance_org(&deps, org_id).await?;
        let timeouts = state
            .task_service
            .claim_expired_native_requests(&mut timeout_db, remaining)
            .await?;
        timeout_db.commit().await?;
        for timeout in &timeouts {
            if let Err(error) = state
                .task_service
                .cancel_expired_native_request(timeout)
                .await
            {
                tracing::warn!(%error, "expired native request cancellation remains pending");
            }
        }
        reconciled = reconciled.saturating_add(timeouts.len() as u64);

        if reconciled >= limit.max(1) as u64 {
            break;
        }
        let remaining = limit.saturating_sub(reconciled as i64).max(1);
        let mut reply_db = scoped_db_for_maintenance_org(&deps, org_id).await?;
        let replies = state
            .task_service
            .claim_pending_native_replies(&mut reply_db, remaining)
            .await?;
        reply_db.commit().await?;
        for reply in &replies {
            if let Err(error) = state.task_service.deliver_claimed_native_reply(reply).await {
                tracing::warn!(%error, "accepted native reply delivery remains pending");
            }
        }
        reconciled = reconciled.saturating_add(replies.len() as u64);
    }
    Ok(reconciled)
}

async fn dispatch_ready_tasks(
    state: &Arc<AppState>,
    limit: i64,
) -> Result<u64, crate::errors::AppError> {
    let deps = TenantMaintenanceDeps {
        system_pool: state.system_pool.clone(),
        tenant_pool: state.tenant_pool.clone(),
        authz: state.authz.clone(),
        repo_storage: state.repo_storage.clone(),
    };
    let mut org_ids = maintenance_org_ids(&state.system_pool).await?;
    if org_ids.is_empty() {
        return Ok(0);
    }
    rotate_orgs(
        &mut org_ids,
        DISPATCH_ORG_CURSOR.fetch_add(1, Ordering::Relaxed),
    );
    let mut dispatched = 0_u64;
    let mut attempted = 0_u64;
    let limit = limit.max(1) as u64;
    while attempted < limit {
        let mut found_work = false;
        for &org_id in &org_ids {
            if attempted >= limit {
                break;
            }
            let mut db = scoped_db_for_maintenance_org(&deps, org_id).await?;
            let command_dispatch = match state.task_service.reserve_command_dispatch(&mut db).await
            {
                Ok(dispatch) => dispatch,
                Err(error) => {
                    tracing::warn!(%org_id, %error, "command dispatch reservation failed for one tenant");
                    break;
                }
            };
            if let Some(dispatch) = command_dispatch {
                db.commit().await?;
                found_work = true;
                attempted = attempted.saturating_add(1);
                let disposition = match state.task_service.invoke_command_dispatch(&dispatch).await
                {
                    Ok(disposition) => disposition,
                    Err(error) => {
                        tracing::warn!(
                            task_id = %dispatch.task_id,
                            attempt = dispatch.attempt,
                            vm_id = %dispatch.vm_id,
                            %error,
                            "command invoke remains unconfirmed; the same execution id will retry"
                        );
                        continue;
                    }
                };
                let mut confirmation =
                    scoped_db_for_maintenance_org(&deps, dispatch.org_id).await?;
                state
                    .task_service
                    .apply(&mut confirmation, dispatch.start_disposition(disposition))
                    .await?;
                confirmation.commit().await?;
                dispatched = dispatched.saturating_add(1);
                continue;
            }
            let agent_dispatch = match state.task_service.reserve_agent_dispatch(&mut db).await {
                Ok(dispatch) => dispatch,
                Err(error) => {
                    tracing::warn!(%org_id, %error, "agent dispatch reservation failed for one tenant");
                    break;
                }
            };
            let Some(dispatch) = agent_dispatch else {
                db.commit().await?;
                continue;
            };
            db.commit().await?;
            found_work = true;
            attempted = attempted.saturating_add(1);
            let disposition = match state.task_service.invoke_agent_dispatch(&dispatch).await {
                Ok(disposition) => disposition,
                Err(error) => {
                    tracing::warn!(
                        task_id = %dispatch.task_id,
                        attempt = dispatch.attempt,
                        vm_id = %dispatch.vm_id,
                        %error,
                        "agent invoke remains unconfirmed; the same execution id will retry"
                    );
                    continue;
                }
            };
            let mut confirmation = scoped_db_for_maintenance_org(&deps, dispatch.org_id).await?;
            let fresh_start = disposition["disposition"].as_str() == Some("started");
            state
                .task_service
                .apply(&mut confirmation, dispatch.start_disposition(disposition))
                .await?;
            if fresh_start {
                // The guest had not seen this execution, so it launched the
                // reduced request and fails the turn before the provider
                // runs; tell the owner why in the chat.
                state
                    .task_service
                    .record_secret_preflight_notice(&mut confirmation, &dispatch)
                    .await?;
            }
            confirmation.commit().await?;
            dispatched = dispatched.saturating_add(1);
        }
        if !found_work {
            break;
        }
    }
    Ok(dispatched)
}

fn rotate_orgs(org_ids: &mut [Uuid], cursor: usize) {
    let len = org_ids.len();
    if len > 1 {
        org_ids.rotate_left(cursor % len);
    }
}

async fn load_active_attempts(
    state: &Arc<AppState>,
    limit: i64,
) -> Result<Vec<ActiveAttempt>, crate::errors::AppError> {
    let deps = TenantMaintenanceDeps {
        system_pool: state.system_pool.clone(),
        tenant_pool: state.tenant_pool.clone(),
        authz: state.authz.clone(),
        repo_storage: state.repo_storage.clone(),
    };
    let mut org_ids = maintenance_org_ids(&state.system_pool).await?;
    if org_ids.is_empty() {
        return Ok(Vec::new());
    }
    rotate_orgs(
        &mut org_ids,
        ACTIVE_ATTEMPT_ORG_CURSOR.fetch_add(1, Ordering::Relaxed),
    );
    let limit = limit.clamp(1, 128);
    let org_count = i64::try_from(org_ids.len()).unwrap_or(i64::MAX);
    let fair_share = (limit - 1) / org_count + 1;
    let mut attempts: Vec<ActiveAttempt> = Vec::new();
    // Attempts a follower already streams need no claim; the batch goes to
    // attempts nobody follows yet and to incarnations settled from the
    // database alone.
    let followed = state.followers.followed_attempts();
    for &org_id in &org_ids {
        let remaining = limit.saturating_sub(i64::try_from(attempts.len()).unwrap_or(i64::MAX));
        if remaining == 0 {
            break;
        }
        let mut db = scoped_db_for_maintenance_org(&deps, org_id).await?;
        attempts.extend(
            claim_active_attempts(
                &mut db,
                org_id,
                fair_share.min(remaining),
                &followed,
                &state.workspace_runtime.reachable_host_ids(),
            )
            .await?,
        );
        db.commit().await?;
    }
    if attempts.len() < usize::try_from(limit).unwrap_or(usize::MAX) {
        for &org_id in &org_ids {
            let remaining = limit.saturating_sub(i64::try_from(attempts.len()).unwrap_or(i64::MAX));
            if remaining == 0 {
                break;
            }
            // The fair-share pass committed its claims, so the top-up must
            // exclude them or this pass reconciles the same attempt twice.
            let mut claimed: Vec<(Uuid, i32)> = attempts
                .iter()
                .filter(|attempt| attempt.org_id == org_id)
                .map(|attempt| (attempt.task_id, attempt.attempt))
                .collect();
            claimed.extend(followed.iter().copied());
            let mut db = scoped_db_for_maintenance_org(&deps, org_id).await?;
            attempts.extend(
                claim_active_attempts(
                    &mut db,
                    org_id,
                    remaining,
                    &claimed,
                    &state.workspace_runtime.reachable_host_ids(),
                )
                .await?,
            );
            db.commit().await?;
        }
    }
    Ok(attempts)
}

async fn claim_active_attempts(
    db: &mut g5_authz::ScopedDb,
    org_id: Uuid,
    limit: i64,
    exclude_attempts: &[(Uuid, i32)],
    reachable_host_ids: &[Uuid],
) -> Result<Vec<ActiveAttempt>, crate::errors::AppError> {
    let (exclude_tasks, exclude_numbers): (Vec<Uuid>, Vec<i32>) =
        exclude_attempts.iter().copied().unzip();
    // A live incarnation requires a local route before spending the shared
    // reconciliation batch. Lost/replaced/expired incarnations need only DB
    // settlement, which must still progress when no host is connected.
    let rows = sqlx::query(
        "WITH selected AS (
             SELECT attempt.org_id, attempt.task_id, attempt.attempt
             FROM task_attempts AS attempt
             JOIN tasks AS task
               ON task.org_id = attempt.org_id AND task.id = attempt.task_id
             JOIN workspaces AS workspace
               ON workspace.org_id = task.org_id AND workspace.id = task.workspace_id
             LEFT JOIN workspace_vms AS vm
               ON vm.org_id = task.org_id AND vm.workspace_id = task.workspace_id
             WHERE attempt.org_id = $2
               AND task.status = 'running' AND task.attempt = attempt.attempt
               AND task.vm_id = attempt.vm_id AND task.epoch = attempt.epoch
               AND attempt.ended_at IS NULL
               AND NOT EXISTS (
                   SELECT 1 FROM unnest($3::uuid[], $4::int4[]) AS followed(task_id, attempt)
                   WHERE followed.task_id = attempt.task_id
                     AND followed.attempt = attempt.attempt)
               AND (vm.host_id = ANY($5)
                    OR workspace.deleted_at IS NOT NULL
                    OR vm.workspace_id IS NULL
                    OR vm.status NOT IN ('starting','running','stopping')
                    OR vm.vm_id IS DISTINCT FROM attempt.vm_id
                    OR vm.epoch IS DISTINCT FROM attempt.epoch
                    OR NOT COALESCE(vm.lease_expires_at > clock_timestamp(), false))
             ORDER BY attempt.last_reconciled_at, task.updated_at, task.id
             FOR UPDATE OF attempt SKIP LOCKED
             LIMIT $1
         ), claimed AS (
             UPDATE task_attempts AS attempt
             SET last_reconciled_at = now()
             FROM selected
             WHERE attempt.org_id = selected.org_id
               AND attempt.task_id = selected.task_id
               AND attempt.attempt = selected.attempt
             RETURNING attempt.*
         )
         SELECT attempt.org_id, task.workspace_id, task.id AS task_id,
                task.chat_session_id,
                attempt.attempt, attempt.vm_id, attempt.epoch,
                attempt.acked_event_seq, attempt.start_confirmed,
                COALESCE((task.spec #>> '{lifecycle,cancel_requested}')::boolean, false)
                    AS cancel_requested,
                task.spec #>> '{lifecycle,interrupt_reason}' AS interrupt_reason
         FROM claimed AS attempt
         JOIN tasks AS task
           ON task.org_id = attempt.org_id AND task.id = attempt.task_id
         ORDER BY attempt.last_reconciled_at, task.updated_at, task.id",
    )
    .bind(limit.clamp(1, 128))
    .bind(org_id)
    .bind(&exclude_tasks)
    .bind(&exclude_numbers)
    .bind(reachable_host_ids)
    .fetch_all(db.conn())
    .await?;
    rows.into_iter()
        .map(|row| {
            let raw_vm_id: String = row.try_get("vm_id")?;
            let raw_epoch: i64 = row.try_get("epoch")?;
            let raw_acked: i64 = row.try_get("acked_event_seq")?;
            Ok(ActiveAttempt {
                org_id: row.try_get("org_id")?,
                workspace_id: row.try_get("workspace_id")?,
                task_id: row.try_get("task_id")?,
                session_id: row.try_get("chat_session_id")?,
                attempt: row.try_get("attempt")?,
                vm_id: Uuid::parse_str(&raw_vm_id).map_err(|_| {
                    sqlx::Error::Protocol("task attempt vm_id is not a UUID".to_string())
                })?,
                epoch: u64::try_from(raw_epoch).map_err(|_| {
                    sqlx::Error::Protocol("task attempt epoch is negative".to_string())
                })?,
                acked_event_seq: u64::try_from(raw_acked).map_err(|_| {
                    sqlx::Error::Protocol("task event watermark is negative".to_string())
                })?,
                start_confirmed: row.try_get("start_confirmed")?,
                cancel_requested: row.try_get("cancel_requested")?,
                interrupt_reason: row.try_get("interrupt_reason")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(Into::into)
}

async fn reconcile_attempt(
    state: &Arc<AppState>,
    attempt: &ActiveAttempt,
) -> Result<u64, crate::errors::AppError> {
    let Some(handle) = state
        .workspace_runtime
        .get(attempt.workspace_id)
        .await
        .map_err(crate::errors::AppError::from_error)?
    else {
        return apply_vm_loss(state, attempt, Value::String("incarnation_absent".into())).await;
    };
    if handle.vm_id != attempt.vm_id || handle.epoch != attempt.epoch {
        return apply_vm_loss(state, attempt, Value::String("incarnation_replaced".into())).await;
    }
    // A stopping incarnation is drained and settled by the stop itself
    // (`settle_stopped_execution`); a follower would only race it.
    if handle.status == "stopping" {
        return Ok(0);
    }
    // A live incarnation: one follower per attempt streams its journal,
    // stores it in batches and acknowledges the committed prefix
    // (`g5.chat.live-delivery`). Its control facts reach the follower too.
    let exec_id = canonical_exec_id(attempt.task_id, attempt.attempt, attempt.vm_id);
    let started = state.followers.ensure(
        state,
        FollowedAttempt {
            org_id: attempt.org_id,
            workspace_id: attempt.workspace_id,
            task_id: attempt.task_id,
            session_id: attempt.session_id,
            attempt: attempt.attempt,
            vm_id: attempt.vm_id,
            epoch: attempt.epoch,
            persisted_through: attempt.acked_event_seq,
            start_confirmed: attempt.start_confirmed,
        },
    );
    if started {
        tracing::debug!(
            target: "g5_api::live_relay",
            task_id = %attempt.task_id,
            attempt = attempt.attempt,
            vm_id = %handle.vm_id,
            "attempt follower started"
        );
    } else if attempt.cancel_requested || attempt.interrupt_reason.is_some() {
        state.followers.poke(&exec_id);
    }
    Ok(u64::from(started))
}

async fn apply_vm_loss(
    state: &Arc<AppState>,
    attempt: &ActiveAttempt,
    evidence: Value,
) -> Result<u64, crate::errors::AppError> {
    let deps = TenantMaintenanceDeps {
        system_pool: state.system_pool.clone(),
        tenant_pool: state.tenant_pool.clone(),
        authz: state.authz.clone(),
        repo_storage: state.repo_storage.clone(),
    };
    let mut db = scoped_db_for_maintenance_org(&deps, attempt.org_id).await?;
    let result = state
        .task_service
        .apply(
            &mut db,
            TaskLifecycleEvent::VmLost {
                task_id: attempt.task_id,
                attempt: attempt.attempt,
                vm_id: attempt.vm_id,
                epoch: attempt.epoch,
                evidence,
            },
        )
        .await?;
    db.commit().await?;
    Ok((!result.duplicate) as u64)
}

#[cfg(test)]
fn parse_event(
    attempt: &ActiveAttempt,
    exec_id: &str,
    value: &Value,
) -> Result<JournalTaskEvent, crate::errors::AppError> {
    journal_event_from_replay(
        attempt.task_id,
        attempt.attempt,
        attempt.vm_id,
        attempt.epoch,
        exec_id,
        value,
    )
}

fn unix_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_declared_agent_turn_events_without_rewriting_other_modules() {
        for kind in [
            "input.accepted",
            "provider.session",
            "provider_request.opened",
            "provider_request.replied",
            "provider_request.invalidated",
            "message.delta",
            "message.final",
            "thought.final",
            "tool.started",
            "tool.completed",
            "notice",
            "session.title",
        ] {
            assert_eq!(
                normalize_agent_turn_event_kind(&format!("module.{kind}")),
                kind
            );
        }
        assert_eq!(
            normalize_agent_turn_event_kind("module.preview.ready"),
            "module.preview.ready"
        );
        assert_eq!(
            normalize_agent_turn_event_kind("invocation.outcome"),
            "invocation.outcome"
        );
    }

    #[test]
    fn replay_event_is_bound_to_persisted_attempt_identity() {
        let attempt = ActiveAttempt {
            org_id: Uuid::nil(),
            workspace_id: Uuid::nil(),
            task_id: Uuid::parse_str("0199c0de-1111-7000-8000-00000000beef").unwrap(),
            session_id: None,
            attempt: 2,
            vm_id: Uuid::parse_str("0199c0de-0000-7000-8000-00000000abcd").unwrap(),
            epoch: 7,
            acked_event_seq: 0,
            start_confirmed: true,
            cancel_requested: false,
            interrupt_reason: None,
        };
        let exec = canonical_exec_id(attempt.task_id, attempt.attempt, attempt.vm_id);
        let event = parse_event(
            &attempt,
            &exec,
            &serde_json::json!({
                "seq": 1,
                "kind": "invocation.started",
                "payload": {},
                "terminal": false
            }),
        )
        .unwrap();
        event.validate_identity().unwrap();
        assert_eq!(event.epoch, 7);
    }

    #[test]
    fn dispatch_rotation_changes_the_first_tenant_without_dropping_any() {
        let first = Uuid::from_u128(1);
        let second = Uuid::from_u128(2);
        let third = Uuid::from_u128(3);
        let mut orgs = vec![first, second, third];
        rotate_orgs(&mut orgs, 4);
        assert_eq!(orgs, vec![second, third, first]);
    }
}
