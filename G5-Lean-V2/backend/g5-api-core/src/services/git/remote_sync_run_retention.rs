//! Retention for the `remote_sync_runs` attempt history
//! (`g5.governance.remote.sync-history`).
//!
//! The integration worker already folds a repeated background observation
//! into the membership's latest run. This job bounds what remains: it drops
//! background no-op runs that a later integration run superseded, then runs
//! older than the retention period beyond each membership's most recent
//! [`RUNS_TO_KEEP`]. It never deletes a membership's latest integration run,
//! which the status summary and retry gate read, or a run that opened a merge
//! run.

use std::time::Duration;

use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::errors::AppError;
use crate::maintenance::{
    maintenance_org_ids, scoped_db_for_maintenance_org, TenantMaintenanceDeps,
};

/// The first sweep runs shortly after startup so a deploy prunes the backlog
/// promptly; later sweeps follow the interval.
const STARTUP_DELAY: Duration = Duration::from_secs(5 * 60);
const SWEEP_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);
/// Runs older than this are eligible unless kept below.
pub const RETENTION_DAYS: i32 = 30;
/// Most recent runs kept per membership regardless of age.
pub const RUNS_TO_KEEP: i64 = 100;
/// Rows deleted per statement, so each delete holds its locks briefly.
const DELETE_BATCH: i64 = 5_000;

pub async fn run_remote_sync_run_retention(deps: TenantMaintenanceDeps, cancel: CancellationToken) {
    let start = tokio::time::Instant::now() + STARTUP_DELAY;
    let mut ticker = tokio::time::interval_at(start, SWEEP_INTERVAL);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            _ = ticker.tick() => {
                match sweep(&deps).await {
                    Ok(pruned) if pruned > 0 => info!(pruned, "remote sync run retention sweep completed"),
                    Ok(_) => debug!("remote sync run retention found nothing to prune"),
                    Err(error) => warn!(%error, "remote sync run retention sweep failed"),
                }
            }
            _ = cancel.cancelled() => break,
        }
    }
}

/// One pass over every organization. Returns the number of runs deleted.
pub async fn sweep(deps: &TenantMaintenanceDeps) -> Result<u64, AppError> {
    let mut pruned = 0;
    for org_id in maintenance_org_ids(&deps.system_pool).await? {
        match sweep_org(deps, org_id).await {
            Ok(org_pruned) => pruned += org_pruned,
            Err(error) => warn!(%org_id, %error, "remote sync run retention org sweep failed"),
        }
    }
    Ok(pruned)
}

/// One organization's pass, under that organization's tenant scope. Row-level
/// security limits every statement to its rows; the explicit `org_id` filters
/// restate that boundary. Returns the number of runs deleted.
pub async fn sweep_org(deps: &TenantMaintenanceDeps, org_id: Uuid) -> Result<u64, AppError> {
    let superseded =
        delete_in_batches(deps, org_id, PRUNE_SUPERSEDED_BACKGROUND_NOOPS, false).await?;
    let expired = delete_in_batches(deps, org_id, PRUNE_EXPIRED_RUNS, true).await?;
    Ok(superseded + expired)
}

/// One transaction per batch keeps each delete short.
async fn delete_in_batches(
    deps: &TenantMaintenanceDeps,
    org_id: Uuid,
    statement: &str,
    expiry: bool,
) -> Result<u64, AppError> {
    let mut pruned = 0;
    loop {
        let mut db = scoped_db_for_maintenance_org(deps, org_id).await?;
        let mut query = sqlx::query(statement).bind(org_id).bind(DELETE_BATCH);
        if expiry {
            query = query.bind(RETENTION_DAYS).bind(RUNS_TO_KEEP);
        }
        let deleted = query.execute(db.conn()).await?.rows_affected();
        db.commit().await.map_err(AppError::from)?;
        pruned += deleted;
        if deleted < DELETE_BATCH as u64 {
            return Ok(pruned);
        }
    }
}

/// A background no-op only says "nothing changed as of this pass"; a later
/// integration run of the same membership replaces that statement.
const PRUNE_SUPERSEDED_BACKGROUND_NOOPS: &str = r#"
    DELETE FROM remote_sync_runs
    WHERE id IN (
        SELECT run.id FROM remote_sync_runs run
        WHERE run.org_id = $1 AND run.kind = 'integration'
          AND run.trigger_kind = 'background_inbound' AND run.status = 'noop'
          AND run.merge_run_id IS NULL
          AND EXISTS (
              SELECT 1 FROM remote_sync_runs later
              WHERE later.workspace_id = run.workspace_id AND later.repo_id = run.repo_id
                AND later.kind = 'integration'
                AND (later.started_at, later.id) > (run.started_at, run.id)
          )
        LIMIT $2
    )
"#;

/// Age-based expiry beyond each membership's most recent runs. The latest
/// integration run is kept even when advisory runs outnumber it.
const PRUNE_EXPIRED_RUNS: &str = r#"
    DELETE FROM remote_sync_runs
    WHERE id IN (
        SELECT ranked.id FROM (
            SELECT id, kind, started_at, merge_run_id,
                   row_number() OVER (
                       PARTITION BY workspace_id, repo_id ORDER BY started_at DESC, id DESC
                   ) AS ordinal,
                   row_number() OVER (
                       PARTITION BY workspace_id, repo_id, kind ORDER BY started_at DESC, id DESC
                   ) AS kind_ordinal
            FROM remote_sync_runs
            WHERE org_id = $1
        ) ranked
        WHERE ranked.ordinal > $4
          AND ranked.started_at < now() - make_interval(days => $3)
          AND ranked.merge_run_id IS NULL
          AND NOT (ranked.kind = 'integration' AND ranked.kind_ordinal = 1)
        LIMIT $2
    )
"#;
