//! Timeline cleanup maintenance task.
//!
//! Prunes old `timeline_entries` rows for tasks in terminal states
//! (`completed`, `failed`, `cancelled`). Without periodic cleanup, the
//! `timeline_entries` table grows unbounded — a single task can produce
//! hundreds or thousands of entries during execution, and completed tasks
//! accumulate indefinitely.
//!
//! The cleanup preserves the most recent N entries per task (configurable,
//! default: 100) to maintain a useful summary even for old tasks.

use std::time::Duration;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::errors::AppError;

use crate::maintenance::{
    maintenance_org_ids, scoped_db_for_maintenance_org, TenantMaintenanceDeps,
};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for the timeline cleanup maintenance task.
///
/// The current `MaintenanceConfig` (in `config.rs`) does not include
/// timeline-specific fields. This struct uses hardcoded defaults for M0.
/// When `MaintenanceConfig` is extended with timeline cleanup fields
/// (e.g., `G5_TIMELINE_RETENTION_DAYS`, `G5_TIMELINE_ENTRIES_TO_KEEP`),
/// the config construction should be updated to read from those fields.
#[derive(Debug, Clone)]
pub struct TimelineCleanupConfig {
    /// How often the cleanup runs. Default: 24 hours (86400 seconds).
    pub interval: Duration,
    /// Only prune entries for tasks succeeded/failed/cancelled longer than
    /// this. Default: 30 days.
    pub retention_period: Duration,
    /// Keep the most recent N entries per task. Default: 100.
    pub entries_to_keep: i64,
    /// Maximum rows to delete per DELETE statement. Default: 1000.
    pub delete_batch_size: i64,
}

impl Default for TimelineCleanupConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(86400),              // 24 hours
            retention_period: Duration::from_secs(30 * 86400), // 30 days
            entries_to_keep: 100,
            delete_batch_size: 1000,
        }
    }
}

// ---------------------------------------------------------------------------
// Stats
// ---------------------------------------------------------------------------

/// Statistics returned by a single cleanup pass.
#[derive(Debug, Default)]
pub struct CleanupStats {
    /// Number of tasks whose timeline entries were examined.
    pub tasks_processed: u64,
    /// Total number of timeline entries deleted across all tasks.
    pub entries_pruned: u64,
    /// Number of tasks that had entries pruned (subset of tasks_processed).
    pub tasks_pruned: u64,
    /// Number of tasks that were skipped due to errors.
    pub errors: u64,
}

// ---------------------------------------------------------------------------
// Public loop entry point
// ---------------------------------------------------------------------------

/// Runs the timeline cleanup loop indefinitely until cancelled.
///
/// Spawned as a Tokio task by `MaintenanceRunner::spawn_all()`.
pub async fn run_timeline_cleanup_loop(
    deps: TenantMaintenanceDeps,
    config: TimelineCleanupConfig,
    cancel: CancellationToken,
) {
    tracing::info!(
        interval_secs = config.interval.as_secs(),
        retention_days = config.retention_period.as_secs() / 86400,
        entries_to_keep = config.entries_to_keep,
        "starting timeline cleanup loop"
    );

    let mut interval = tokio::time::interval(config.interval);
    // Consume the first immediate tick so the first cleanup runs after one
    // full interval, not immediately on startup.
    interval.tick().await;

    loop {
        tokio::select! {
            _ = interval.tick() => {
                let start = std::time::Instant::now();
                match cleanup_old_timeline_entries(&deps, &config).await {
                    Ok(stats) => {
                        tracing::info!(
                            tasks_processed = stats.tasks_processed,
                            tasks_pruned = stats.tasks_pruned,
                            entries_pruned = stats.entries_pruned,
                            errors = stats.errors,
                            duration_ms = start.elapsed().as_millis() as u64,
                            "timeline cleanup complete"
                        );
                    }
                    Err(e) => {
                        tracing::error!(
                            error = %e,
                            "timeline cleanup failed"
                        );
                    }
                }
            }
            _ = cancel.cancelled() => {
                tracing::info!("timeline cleanup task shutting down");
                break;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Core cleanup logic
// ---------------------------------------------------------------------------

/// Executes a single cleanup pass: identifies eligible tasks and prunes
/// their old timeline entries.
///
/// Returns stats about the cleanup operation.
async fn cleanup_old_timeline_entries(
    deps: &TenantMaintenanceDeps,
    config: &TimelineCleanupConfig,
) -> Result<CleanupStats, AppError> {
    let org_ids = maintenance_org_ids(&deps.system_pool)
        .await
        .map_err(AppError::from)?;

    let mut stats = CleanupStats::default();

    for org_id in org_ids {
        let mut db = scoped_db_for_maintenance_org(deps, org_id).await?;
        let org_stats = cleanup_old_timeline_entries_for_org(db.conn(), config)
            .await
            .map_err(AppError::from)?;
        db.commit().await.map_err(AppError::from)?;
        stats.tasks_processed += org_stats.tasks_processed;
        stats.entries_pruned += org_stats.entries_pruned;
        stats.tasks_pruned += org_stats.tasks_pruned;
        stats.errors += org_stats.errors;
    }

    Ok(stats)
}

async fn cleanup_old_timeline_entries_for_org(
    conn: &mut sqlx::PgConnection,
    config: &TimelineCleanupConfig,
) -> Result<CleanupStats, sqlx::Error> {
    // Convert retention_period to a PostgreSQL interval string.
    // We pass it as seconds and use `make_interval`.
    let retention_secs = config.retention_period.as_secs() as f64;

    // An ended chat turn's ontology record (`g5.chat.ontology-card`): its card
    // is on its reply. Settlement drops it; this catches turns that ended
    // another way, such as a held turn ended after its VM was lost.
    sqlx::query(
        "DELETE FROM chat_turn_ontology AS record USING tasks AS task
         WHERE task.id = record.task_id AND task.status IN ('succeeded', 'failed', 'cancelled')",
    )
    .execute(&mut *conn)
    .await?;

    // Step 1 — Identify eligible tasks: terminal state + older than
    // retention period + actually have timeline entries.
    let eligible_tasks: Vec<Uuid> = sqlx::query_scalar!(
        r#"
        SELECT t.id AS "id!"
        FROM tasks t
        WHERE t.status IN ('succeeded', 'failed', 'cancelled')
          AND t.updated_at < now() - make_interval(secs => $1)
          AND EXISTS (
            SELECT 1 FROM timeline_entries te
            WHERE te.task_id = t.id
          )
        "#,
        retention_secs,
    )
    .fetch_all(&mut *conn)
    .await?;

    if eligible_tasks.is_empty() {
        tracing::debug!("no eligible tasks for timeline cleanup");
        return Ok(CleanupStats::default());
    }

    let mut stats = CleanupStats::default();

    // Step 2 — For each eligible task, prune old entries (batched).
    for task_id in &eligible_tasks {
        stats.tasks_processed += 1;

        match prune_task_entries(
            conn,
            *task_id,
            config.entries_to_keep,
            config.delete_batch_size,
        )
        .await
        {
            Ok(deleted) => {
                if deleted > 0 {
                    stats.entries_pruned += deleted;
                    stats.tasks_pruned += 1;
                }
            }
            Err(e) => {
                tracing::warn!(
                    task_id = %task_id,
                    error = %e,
                    "failed to prune entries for task"
                );
                stats.errors += 1;
            }
        }
    }

    Ok(stats)
}

/// Prunes timeline entries for a single task, keeping the most recent
/// `entries_to_keep` entries. Deletes in batches of `batch_size` to avoid
/// long-running transactions.
///
/// Returns the number of entries deleted.
async fn prune_task_entries(
    conn: &mut sqlx::PgConnection,
    task_id: Uuid,
    entries_to_keep: i64,
    batch_size: i64,
) -> Result<u64, sqlx::Error> {
    let mut total_deleted: u64 = 0;

    loop {
        let result = sqlx::query!(
            r#"
            DELETE FROM timeline_entries
            WHERE id IN (
                SELECT te.id FROM timeline_entries te
                WHERE te.task_id = $1
                  AND te.id NOT IN (
                    SELECT id FROM timeline_entries
                    WHERE task_id = $1
                    ORDER BY created_at DESC
                    LIMIT $2
                  )
                LIMIT $3
            )
            "#,
            task_id,
            entries_to_keep,
            batch_size,
        )
        .execute(&mut *conn)
        .await?;

        let deleted = result.rows_affected();
        total_deleted += deleted;

        if deleted < batch_size as u64 {
            break; // No more rows to delete
        }
    }

    Ok(total_deleted)
}
