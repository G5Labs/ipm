//! Reconcile automatic provider pushes and enabled passive pulls.
//! Recover accepted Git writes even when a queue receipt was lost.
//! Candidate policy rows are locked with nonblocking key-share so a busy commit
//! or integration cycle is skipped until the next sweep. Existing manual/push
//! requests retain precedence. The worker owns all Git reads and publication.

use std::time::Duration;

use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::errors::AppError;

use crate::maintenance::{scoped_db_for_maintenance_org, TenantMaintenanceDeps};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Global advisory-lock key — only one replica runs the scheduler at a
/// time. "BISC" = Branch Integration SCheduler.
const ADVISORY_LOCK_KEY_GLOBAL: i64 = 0x4249_5343;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Runs the scheduler loop until `cancel` fires.
///
/// `interval` comes from `MaintenanceConfig::remote_tracking_scheduler_interval`
/// (default 30s). The interval ticker uses `MissedTickBehavior::Skip` so a
/// slow tick does not stack backlog.
pub async fn run_branch_integration_scheduler(
    deps: TenantMaintenanceDeps,
    interval: Duration,
    passive_auto_pull_enabled: bool,
    cancel: CancellationToken,
) {
    let mut ticker = tokio::time::interval(interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                match do_scheduler_sweep(&deps, passive_auto_pull_enabled).await {
                    Ok(enqueued) if enqueued > 0 => {
                        info!(
                            enqueued,
                            "branch_integration_scheduler: sweep completed"
                        );
                    }
                    Ok(_) => {
                        debug!("branch_integration_scheduler: no unpaused remote-tracking branches");
                    }
                    Err(e) => {
                        error!(error = %e, "branch_integration_scheduler: sweep failed");
                    }
                }
            }
            _ = cancel.cancelled() => {
                info!("branch_integration_scheduler: shutting down");
                break;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Sweep — replica gate + per-org enqueue
// ---------------------------------------------------------------------------

async fn do_scheduler_sweep(
    deps: &TenantMaintenanceDeps,
    passive_pull_enabled: bool,
) -> Result<usize, AppError> {
    // Resolve the org list BEFORE taking the advisory-lock connection. Holding a
    // `system_pool` connection for the replica lock while `maintenance_org_ids`
    // acquires a SECOND `system_pool` connection deadlocks a small pool: the
    // maintenance runner spawns this alongside the advisory pinger, which holds
    // its own lock connection, so two held + two needed exhausts a 2-connection
    // pool (the worker shutdown then never returns). The per-org enqueue work
    // below uses `tenant_pool`, so the lock connection is the only `system_pool`
    // connection this task holds.
    let org_ids = crate::maintenance::maintenance_org_ids(&deps.system_pool)
        .await
        .map_err(AppError::from)?;

    let mut system_conn = deps.system_pool.acquire().await.map_err(AppError::from)?;

    // Replica gate: skip the sweep if another replica is already
    // scheduling. Two replicas double-stamping `requested_at` is
    // harmless (the worker collapses bursts) but ls-noise is
    // undesirable.
    let acquired: bool = sqlx::query_scalar(r#"SELECT pg_try_advisory_lock($1)"#)
        .bind(ADVISORY_LOCK_KEY_GLOBAL)
        .fetch_one(&mut *system_conn)
        .await
        .map_err(AppError::from)?;

    if !acquired {
        return Ok(0);
    }

    let result = enqueue_orgs(deps, org_ids, passive_pull_enabled).await;

    let _: bool = sqlx::query_scalar(r#"SELECT pg_advisory_unlock($1)"#)
        .bind(ADVISORY_LOCK_KEY_GLOBAL)
        .fetch_one(&mut *system_conn)
        .await
        .map_err(AppError::from)?;

    result
}

async fn enqueue_orgs(
    deps: &TenantMaintenanceDeps,
    org_ids: Vec<Uuid>,
    passive_pull_enabled: bool,
) -> Result<usize, AppError> {
    let mut enqueued = 0;
    for org_id in org_ids {
        match enqueue_one_org(deps, org_id, passive_pull_enabled).await {
            Ok(n) => enqueued += n,
            Err(e) => {
                warn!(
                    org_id = %org_id,
                    error = %e,
                    "branch_integration_scheduler: org enqueue failed"
                );
            }
        }
    }
    Ok(enqueued)
}

async fn enqueue_one_org(
    deps: &TenantMaintenanceDeps,
    org_id: Uuid,
    passive_pull_enabled: bool,
) -> Result<usize, AppError> {
    let mut db = scoped_db_for_maintenance_org(deps, org_id).await?;
    let n = enqueue_background_inbound_rows(db.conn(), passive_pull_enabled).await?;
    db.commit().await.map_err(AppError::from)?;
    Ok(n)
}

use crate::services::remote_integrations::enqueue_background_inbound_rows;
