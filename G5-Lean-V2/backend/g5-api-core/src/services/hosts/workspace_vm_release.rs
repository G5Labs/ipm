//! Releasing a workspace VM: the one home of the SQL that clears a VM's
//! custody columns.
//!
//! Every release clears the same columns ([`CLEAR_CUSTODY`]). Each cause
//! keeps its own status, loss, error and start-intent expressions. A release
//! that ends a VM's lifecycle settles Agents through [`AgentWorkSettlement`]
//! in the same transaction, and only when its fenced update matched the
//! incarnation it meant; a refused placement, whose VM never started, does not.
//!
//! Host absence is release evidence only for a settled Start
//! (`g5.runtime.start.settlement`): a `starting` row whose Start was never
//! recorded as sent (`start_command_sent_at IS NULL`) is left to its sender,
//! and a release on absence fences on the stamp the observer read.

use chrono::{DateTime, Utc};
use g5_authz::ScopedDb;
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::errors::AppError;

/// How a VM release ends the workspace's queued Agents work.
#[derive(Clone, Copy, Debug)]
pub enum ReleaseSettlement {
    /// A loss or failure: queued previews are cancelled and undelivered
    /// notices withdrawn, both with this reason.
    Lost(&'static str),
    /// A completed stop: undelivered notices are withdrawn.
    Stopped,
}

/// The Agents work a VM fact changes, settled on the releasing connection
/// (`g5.runtime.events.org-scope`): a release ends the workspace's queued
/// work and withdraws its notices, and a Ready returns work a stop held.
/// Hosts owns the custody columns; Agents owns the task and notice rows and
/// is installed at composition.
#[async_trait::async_trait]
pub trait AgentWorkSettlement: Send + Sync {
    async fn settle_after_release(
        &self,
        conn: &mut PgConnection,
        workspace_id: Uuid,
        settlement: ReleaseSettlement,
    ) -> Result<(), sqlx::Error>;

    async fn release_tasks_after_resume(
        &self,
        conn: &mut PgConnection,
        workspace_id: Uuid,
    ) -> Result<(), sqlx::Error>;
}

/// Columns every release clears: host custody, the boot record and stop
/// state.
const CLEAR_CUSTODY: &str = "host_id = NULL, vm_id = NULL, lease_expires_at = NULL,
     start_command_sent_at = NULL,
     boot_spec = NULL, source_manifest_digest = NULL, image_oid_vector = NULL,
     stop_reason = NULL, stop_operation_id = NULL, stop_generation = NULL,
     stop_retry_at = NULL, stop_deadline_at = NULL, stop_command_sent_at = NULL,
     pending_stop_reason = NULL, pending_stop_deadline_at = NULL,
     nudged_at = NULL, updated_at = now()";

/// A `starting` row released by loss or failure takes its live Start intent
/// with it, as lease expiry and definite boot failure already do; an intent
/// Stop or transfer already cancelled keeps its status.
const FAIL_STARTING_INTENT: &str = "start_intent = CASE
                 WHEN status = 'starting' AND start_intent ->> 'status' = 'starting'
                 THEN jsonb_set(start_intent, '{status}', '\"failed\"')
                 ELSE start_intent END";

/// A release that settles Agents work first locks its workspace rows
/// (`KEY SHARE`, in workspace order); its fenced update then locks only the VM
/// rows it releases. Owner operations lock a workspace row before its VM row
/// and tasks, and settlement locks tasks and notices, so a release must never
/// hold a VM row while it waits for a workspace row
/// (`g5.runtime.events.org-scope`). A batch releases only the candidates
/// whose workspaces it locked.
async fn lock_for_release(
    conn: &mut PgConnection,
    workspace_ids: &[Uuid],
) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT 1 FROM workspaces WHERE id = ANY($1) ORDER BY id FOR KEY SHARE")
        .bind(workspace_ids)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Lock the host row before any VM row a release touches. Placement and
/// hello both take the host row first, so a release that also writes the
/// host's reported inventory keeps their order and cannot deadlock against
/// them.
async fn lock_host(conn: &mut PgConnection, host_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT 1 FROM worker_hosts WHERE host_id = $1 FOR UPDATE")
        .bind(host_id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// A settled non-admission retracts the incarnation from the host's reported
/// inventory (`g5.runtime.start.settlement`). A reconnect hello advertises a
/// pending Start as booting; if that Start is then refused, cancelled by a
/// Stop, or found absent once every sent Start was observed, no VM ever
/// existed to emit exit evidence, and the control plane otherwise removes a
/// reported id only on `Exited`. Left in place, the id would count as
/// occupancy and block idle retirement until the next hello rewrote the set.
/// Only exit evidence or this settled release retracts an id; a partial
/// heartbeat never does.
async fn retract_unadmitted_inventory(
    conn: &mut PgConnection,
    host_id: Uuid,
    vm_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE worker_hosts
         SET reported_live_vm_ids = array_remove(reported_live_vm_ids, $2::uuid)
         WHERE host_id = $1 AND $2::uuid = ANY(reported_live_vm_ids)",
    )
    .bind(host_id)
    .bind(vm_id)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// The host's authenticated inventory no longer lists these VMs. A stop
/// whose command reached the host completes; any other VM is lost. A
/// `starting` VM whose Start was never recorded as sent is not this host's
/// to report yet: its sender either sends under the fence or releases it
/// (`g5.runtime.start.settlement`).
pub(crate) async fn release_missing_from_inventory(
    agents: &dyn AgentWorkSettlement,
    conn: &mut PgConnection,
    host_id: Uuid,
    present_vm_ids: &[String],
) -> Result<Vec<Uuid>, sqlx::Error> {
    let candidates = sqlx::query_scalar::<_, Uuid>(
        "SELECT workspace_id FROM workspace_vms
         WHERE host_id = $1 AND status IN ('starting','running','stopping')
           AND NOT (vm_id = ANY($2::text[]))
           AND NOT (status = 'starting' AND start_command_sent_at IS NULL)",
    )
    .bind(host_id)
    .bind(present_vm_ids)
    .fetch_all(&mut *conn)
    .await?;
    if candidates.is_empty() {
        return Ok(candidates);
    }
    lock_for_release(conn, &candidates).await?;
    let workspace_ids = sqlx::query_scalar::<_, Uuid>(&format!(
        "UPDATE workspace_vms
         SET status = CASE
                 WHEN status = 'stopping' AND stop_command_sent_at IS NOT NULL
                 THEN 'stopped' ELSE 'failed' END,
             start_deadline_at = NULL,
             {FAIL_STARTING_INTENT},
             unflushed_loss_at = CASE
                 WHEN status = 'starting'
                   OR (status = 'stopping' AND stop_command_sent_at IS NOT NULL)
                 THEN unflushed_loss_at ELSE COALESCE(unflushed_loss_at, now()) END,
             session_harvest_loss_at = CASE
                 WHEN status = 'starting'
                   OR (status = 'stopping' AND stop_command_sent_at IS NOT NULL)
                 THEN session_harvest_loss_at
                 ELSE COALESCE(session_harvest_loss_at, now()) END,
             last_error = CASE
                 WHEN status = 'stopping' AND stop_command_sent_at IS NOT NULL
                 THEN last_error
                 ELSE jsonb_build_object(
                     'reason', 'host_inventory_missing', 'host_id', $1::text)
             END,
             {CLEAR_CUSTODY}
         WHERE host_id = $1 AND status IN ('starting','running','stopping')
           AND NOT (vm_id = ANY($2::text[])) AND workspace_id = ANY($3)
           AND NOT (status = 'starting' AND start_command_sent_at IS NULL)
         RETURNING workspace_id"
    ))
    .bind(host_id)
    .bind(present_vm_ids)
    .bind(&candidates)
    .fetch_all(&mut *conn)
    .await?;
    for workspace_id in &workspace_ids {
        agents
            .settle_after_release(
                conn,
                *workspace_id,
                ReleaseSettlement::Lost("host_inventory_missing"),
            )
            .await?;
    }
    Ok(workspace_ids)
}

/// An exit-proven capacity refusal reuses the ordinary placement queue and
/// preserves its byte/phase diagnostic. Independent of event delivery order.
pub(crate) async fn release_disk_capacity(
    conn: &mut PgConnection,
    org_id: Uuid,
    workspace_id: Uuid,
    host_id: Uuid,
    vm_id: Uuid,
    diagnostic: &str,
) -> Result<bool, sqlx::Error> {
    let epoch: Option<i64> = sqlx::query_scalar("SELECT epoch FROM workspace_vms WHERE org_id=$1 AND workspace_id=$2 AND host_id=$3 AND vm_id=$4 AND status='starting'")
        .bind(org_id).bind(workspace_id).bind(host_id).bind(vm_id.to_string()).fetch_optional(&mut *conn).await?;
    let Some(epoch) = epoch else {
        return Ok(false);
    };
    let released = release_refused_placement(
        conn,
        org_id,
        workspace_id,
        host_id,
        vm_id,
        epoch as u64,
        true,
        "host_capacity_refused",
        Some("disk"),
    )
    .await?;
    if released {
        sqlx::query("UPDATE workspace_vms SET last_error=last_error || jsonb_build_object('diagnostic',$3::text) WHERE org_id=$1 AND workspace_id=$2 AND epoch=$4 AND status='stopped'")
            .bind(org_id).bind(workspace_id).bind(diagnostic).bind(epoch).execute(conn).await?;
    }
    Ok(released)
}

/// The host reported that one VM exited. A stop whose command reached the
/// host completes; any other exit is a loss.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn release_exited(
    agents: &dyn AgentWorkSettlement,
    conn: &mut PgConnection,
    org_id: Uuid,
    workspace_id: Uuid,
    host_id: Uuid,
    vm_id: Uuid,
    last_error: &Value,
    at: DateTime<Utc>,
) -> Result<bool, sqlx::Error> {
    // A capacity failure is retryable only after exit evidence, and only for
    // this still-starting incarnation. The reply and journal use one queue owner.
    let capacity_diagnostic: Option<String> = sqlx::query_scalar(
        "SELECT last_error ->> 'diagnostic' FROM workspace_vms WHERE org_id = $1 AND workspace_id = $2
         AND host_id = $3 AND vm_id = $4 AND status = 'starting'
         AND last_error ->> 'reason' = 'host_capacity_refused' AND last_error ->> 'host_reason' = 'disk'
         AND last_error ->> 'diagnostic' IS NOT NULL"
    ).bind(org_id).bind(workspace_id).bind(host_id).bind(vm_id.to_string()).fetch_optional(&mut *conn).await?;
    if let Some(diagnostic) = capacity_diagnostic {
        return release_disk_capacity(conn, org_id, workspace_id, host_id, vm_id, &diagnostic)
            .await;
    }
    lock_for_release(conn, &[workspace_id]).await?;
    let updated = sqlx::query(&format!(
        "UPDATE workspace_vms
         SET status = CASE
                 WHEN status = 'stopping' AND stop_command_sent_at IS NOT NULL
                 THEN 'stopped' ELSE 'failed' END,
             start_deadline_at = NULL,
             {FAIL_STARTING_INTENT},
             unflushed_loss_at = CASE
                 WHEN status = 'starting'
                   OR (status = 'stopping' AND stop_command_sent_at IS NOT NULL)
                 THEN unflushed_loss_at ELSE COALESCE(unflushed_loss_at, $6) END,
             session_harvest_loss_at = CASE
                 WHEN status = 'starting'
                   OR (status = 'stopping' AND stop_command_sent_at IS NOT NULL)
                 THEN session_harvest_loss_at
                 ELSE COALESCE(session_harvest_loss_at, $6) END,
             last_error = CASE
                 WHEN status = 'stopping' AND stop_command_sent_at IS NOT NULL
                 THEN last_error ELSE $5 END,
             {CLEAR_CUSTODY}
         WHERE org_id = $1 AND workspace_id = $2 AND host_id = $3 AND vm_id = $4
           AND status IN ('starting', 'running', 'stopping')"
    ))
    .bind(org_id)
    .bind(workspace_id)
    .bind(host_id)
    .bind(vm_id.to_string())
    .bind(last_error)
    .bind(at)
    .execute(&mut *conn)
    .await?
    .rows_affected()
        == 1;
    if updated {
        agents
            .settle_after_release(conn, workspace_id, ReleaseSettlement::Lost("vm_lost"))
            .await?;
    }
    Ok(updated)
}

/// The host reported that one VM ran out of memory, was killed as stale or
/// failed to boot.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn release_failed(
    agents: &dyn AgentWorkSettlement,
    conn: &mut PgConnection,
    org_id: Uuid,
    workspace_id: Uuid,
    host_id: Uuid,
    vm_id: Uuid,
    last_error: &Value,
    at: DateTime<Utc>,
) -> Result<bool, sqlx::Error> {
    lock_for_release(conn, &[workspace_id]).await?;
    let updated = sqlx::query(&format!(
        "UPDATE workspace_vms
         SET status = 'failed',
             start_deadline_at = NULL,
             {FAIL_STARTING_INTENT},
             unflushed_loss_at = CASE WHEN status = 'starting'
                 THEN unflushed_loss_at ELSE COALESCE(unflushed_loss_at, $6) END,
             session_harvest_loss_at = CASE WHEN status = 'starting'
                 THEN session_harvest_loss_at
                 ELSE COALESCE(session_harvest_loss_at, $6) END,
             last_error = $5,
             {CLEAR_CUSTODY}
         WHERE org_id = $1 AND workspace_id = $2 AND host_id = $3 AND vm_id = $4
           AND status IN ('starting', 'running', 'stopping')"
    ))
    .bind(org_id)
    .bind(workspace_id)
    .bind(host_id)
    .bind(vm_id.to_string())
    .bind(last_error)
    .bind(at)
    .execute(&mut *conn)
    .await?
    .rows_affected()
        == 1;
    if updated {
        agents
            .settle_after_release(conn, workspace_id, ReleaseSettlement::Lost("vm_lost"))
            .await?;
    }
    Ok(updated)
}

/// Return an unadmitted placement to the durable capacity queue: the host
/// refused it, or it is absent from a host that necessarily observed every
/// Start sent for it. The exact incarnation fence keeps a delayed refusal
/// from clearing a replacement reservation, and `start_sent` fences the
/// release on the Start-delivery state the caller observed: an unsent Start
/// cannot be released by a caller that saw it sent, nor the reverse, so a
/// sender stamping `start_command_sent_at` concurrently makes one of the two
/// lose (`g5.runtime.start.settlement`). The intent returns to the queue only
/// while it is still the one being started; an intent Stop or transfer
/// cancelled keeps its status, so a late refusal cannot revive it
/// (`g5.lifecycle.start`). The VM never started, so Agents work stays queued
/// for the next placement. `host_reason` records why the host refused (for
/// example `disk`), so the owner sees the real cause of the wait.
/// An unsent Start lost local admission, not host capacity: preserve existing
/// exclusions without adding a cooldown that would block a connected API.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn release_refused_placement(
    conn: &mut PgConnection,
    org_id: Uuid,
    workspace_id: Uuid,
    host_id: Uuid,
    vm_id: Uuid,
    epoch: u64,
    start_sent: bool,
    reason: &str,
    host_reason: Option<&str>,
) -> Result<bool, sqlx::Error> {
    lock_host(conn, host_id).await?;
    let released = sqlx::query(&format!(
        "UPDATE workspace_vms
         SET status = 'stopped',
             start_deadline_at = NULL,
             start_intent = CASE WHEN start_intent ->> 'status' = 'starting'
                 THEN jsonb_set(start_intent, '{{status}}', '\"queued_for_capacity\"')
                 ELSE start_intent END
               || CASE WHEN $8 THEN jsonb_build_object('host_refusals',
                 COALESCE(start_intent -> 'host_refusals', '{{}}'::jsonb) ||
                 jsonb_build_object($3::text, now() + interval '30 seconds'))
                 ELSE '{{}}'::jsonb END,
             last_error = jsonb_strip_nulls(
                 jsonb_build_object('reason', $6, 'host_reason', $7::text)
             ),
             {CLEAR_CUSTODY}
         WHERE org_id = $1 AND workspace_id = $2 AND host_id = $3
           AND vm_id = $4 AND epoch = $5 AND status = 'starting'
           AND (start_command_sent_at IS NOT NULL) = $8"
    ))
    .bind(org_id)
    .bind(workspace_id)
    .bind(host_id)
    .bind(vm_id.to_string())
    .bind(i64::try_from(epoch).unwrap_or(i64::MAX))
    .bind(reason)
    .bind(host_reason)
    .bind(start_sent)
    .execute(&mut *conn)
    .await?
    .rows_affected()
        == 1;
    if released {
        retract_unadmitted_inventory(conn, host_id, vm_id).await?;
    }
    Ok(released)
}

/// Record forced loss of every active VM on a host an operator removes by
/// force. The prior incarnation is embedded in `last_error` before the
/// custody columns clear, so later reconciliation can tell force from a
/// clean exit. Returns the number of released VMs.
pub(crate) async fn record_forced_vm_loss(
    agents: &dyn AgentWorkSettlement,
    db: &mut ScopedDb,
    org_id: Uuid,
    host_id: Uuid,
    actor_user_id: Uuid,
) -> Result<u64, AppError> {
    let candidates = sqlx::query_scalar::<_, Uuid>(
        "SELECT workspace_id FROM workspace_vms
         WHERE org_id = $1 AND host_id = $2
           AND status IN ('starting', 'running', 'stopping')",
    )
    .bind(org_id)
    .bind(host_id)
    .fetch_all(db.conn())
    .await?;
    if candidates.is_empty() {
        return Ok(0);
    }
    lock_for_release(db.conn(), &candidates).await?;
    for workspace_id in &candidates {
        let pending: bool = sqlx::query_scalar("SELECT authz.lock_workspace_cleanup_pending($1)")
            .bind(workspace_id)
            .fetch_one(db.conn())
            .await?;
        if pending {
            return Err(AppError::from_error(g5_contracts::error::G5Error::Conflict {
                reason: "Membership shutdown requires confirmed VM termination before releasing host custody".into(),
            }));
        }
    }
    let workspace_ids = sqlx::query_scalar::<_, Uuid>(&format!(
        "UPDATE workspace_vms
         SET status = 'failed',
             last_error = jsonb_build_object(
                 'reason', 'host_force_terminated',
                 'host_id', host_id,
                 'vm_id', vm_id,
                 'epoch', epoch,
                 'actor_user_id', $3,
                 'recorded_at', now()
             ),
             unflushed_loss_at = COALESCE(unflushed_loss_at, now()),
             session_harvest_loss_at = COALESCE(session_harvest_loss_at, now()),
             start_deadline_at = NULL,
             {FAIL_STARTING_INTENT},
             {CLEAR_CUSTODY}
         WHERE org_id = $1 AND host_id = $2
           AND status IN ('starting', 'running', 'stopping')
           AND workspace_id = ANY($4)
         RETURNING workspace_id"
    ))
    .bind(org_id)
    .bind(host_id)
    .bind(actor_user_id)
    .bind(&candidates)
    .fetch_all(db.conn())
    .await?;
    for workspace_id in &workspace_ids {
        agents
            .settle_after_release(
                db.conn(),
                *workspace_id,
                ReleaseSettlement::Lost("host_force_terminated"),
            )
            .await?;
    }
    Ok(workspace_ids.len() as u64)
}

/// Release every incarnation whose lease expired: expiry proves the old
/// incarnation no longer has custody. A stop that had not reached the host
/// command records the resulting unflushed loss; one that had reached it
/// completes normally; other active incarnations fail closed. Unlike every
/// other release, this one leaves `start_deadline_at` as it was.
pub(crate) async fn release_expired_leases(
    agents: &dyn AgentWorkSettlement,
    conn: &mut PgConnection,
) -> Result<Vec<Uuid>, sqlx::Error> {
    let candidates = sqlx::query_scalar::<_, Uuid>(
        "SELECT workspace_id FROM workspace_vms
         WHERE status IN ('starting', 'running', 'stopping')
           AND lease_expires_at <= now()",
    )
    .fetch_all(&mut *conn)
    .await?;
    if candidates.is_empty() {
        return Ok(candidates);
    }
    lock_for_release(conn, &candidates).await?;
    let mut releasable = Vec::new();
    for workspace_id in candidates {
        let pending: bool = sqlx::query_scalar("SELECT authz.lock_workspace_cleanup_pending($1)")
            .bind(workspace_id)
            .fetch_one(&mut *conn)
            .await?;
        if !pending {
            releasable.push(workspace_id);
        }
    }
    let candidates = releasable;
    let workspace_ids = sqlx::query_scalar::<_, Uuid>(&format!(
        "UPDATE workspace_vms
         SET status = CASE WHEN status = 'stopping' THEN 'stopped' ELSE 'failed' END,
             unflushed_loss_at = CASE WHEN status = 'running'
                   OR (status = 'stopping' AND started_at IS NOT NULL
                       AND stop_command_sent_at IS NULL)
                 THEN COALESCE(unflushed_loss_at, now()) ELSE unflushed_loss_at END,
             session_harvest_loss_at = CASE WHEN status = 'running'
                   OR (status = 'stopping' AND started_at IS NOT NULL
                       AND stop_command_sent_at IS NULL)
                 THEN COALESCE(session_harvest_loss_at, now())
                 ELSE session_harvest_loss_at END,
             start_intent = CASE WHEN start_intent IS NULL THEN NULL
                 ELSE jsonb_set(start_intent, '{{status}}', '\"failed\"') END,
             last_error = jsonb_build_object(
                 'reason', CASE
                     WHEN status = 'stopping' AND started_at IS NOT NULL
                          AND stop_command_sent_at IS NULL
                     THEN 'lease_expired_during_stop'
                     ELSE 'lease_expired'
                 END),
             {CLEAR_CUSTODY}
         WHERE status IN ('starting', 'running', 'stopping')
           AND lease_expires_at <= now() AND workspace_id = ANY($1)
         RETURNING workspace_id"
    ))
    .bind(&candidates)
    .fetch_all(&mut *conn)
    .await?;
    for workspace_id in &workspace_ids {
        agents
            .settle_after_release(conn, *workspace_id, ReleaseSettlement::Lost("vm_lost"))
            .await?;
    }
    Ok(workspace_ids)
}

/// A start whose boot failed definitely. Agents settle only when the fenced
/// update released this incarnation: a reply for an incarnation Stop or
/// transfer already released must not cancel the work a newer one queued
/// (`g5.runtime.start.settlement`).
#[allow(clippy::too_many_arguments)]
pub(crate) async fn release_failed_start(
    agents: &dyn AgentWorkSettlement,
    conn: &mut PgConnection,
    org_id: Uuid,
    workspace_id: Uuid,
    host_id: Uuid,
    vm_id: Uuid,
    epoch: u64,
    reason: &str,
) -> Result<bool, sqlx::Error> {
    lock_host(conn, host_id).await?;
    lock_for_release(conn, &[workspace_id]).await?;
    let released = sqlx::query(&format!(
        "UPDATE workspace_vms
         SET status = 'failed',
             start_deadline_at = NULL,
             start_intent = jsonb_set(start_intent, '{{status}}', '\"failed\"'),
             last_error = jsonb_build_object('reason', $6),
             {CLEAR_CUSTODY}
         WHERE org_id = $1 AND workspace_id = $2 AND host_id = $3
           AND vm_id = $4 AND epoch = $5 AND status = 'starting'"
    ))
    .bind(org_id)
    .bind(workspace_id)
    .bind(host_id)
    .bind(vm_id.to_string())
    .bind(i64::try_from(epoch).unwrap_or(i64::MAX))
    .bind(reason)
    .execute(&mut *conn)
    .await?
    .rows_affected()
        == 1;
    if released {
        retract_unadmitted_inventory(conn, host_id, vm_id).await?;
        agents
            .settle_after_release(
                conn,
                workspace_id,
                ReleaseSettlement::Lost("vm_start_failed"),
            )
            .await?;
    }
    Ok(released)
}

/// A stop that captured the workspace and proved the VM exited. The host
/// deleted the root at release. Agents settle only when the fenced update
/// released this incarnation and operation; a stop whose exit event already
/// released it settled there (`g5.runtime.start.settlement`).
pub(crate) async fn release_stopped(
    agents: &dyn AgentWorkSettlement,
    conn: &mut PgConnection,
    host_id: Uuid,
    workspace_id: Uuid,
    vm_id: Uuid,
    epoch: u64,
    stop_operation_id: Uuid,
) -> Result<bool, sqlx::Error> {
    lock_host(conn, host_id).await?;
    lock_for_release(conn, &[workspace_id]).await?;
    let released = sqlx::query(&format!(
        "UPDATE workspace_vms
         SET status = 'stopped',
             start_deadline_at = NULL,
             {CLEAR_CUSTODY}
         WHERE workspace_id = $1 AND vm_id = $2 AND epoch = $3
           AND stop_operation_id = $4 AND status = 'stopping'"
    ))
    .bind(workspace_id)
    .bind(vm_id.to_string())
    .bind(i64::try_from(epoch).unwrap_or(i64::MAX))
    .bind(stop_operation_id)
    .execute(&mut *conn)
    .await?
    .rows_affected()
        == 1;
    if released {
        // A stop the host answered absent for a Start it never admitted ends
        // the incarnation without exit evidence; retract it like a refusal.
        retract_unadmitted_inventory(conn, host_id, vm_id).await?;
        agents
            .settle_after_release(conn, workspace_id, ReleaseSettlement::Stopped)
            .await?;
    }
    Ok(released)
}
