use std::time::Instant;
use tokio_util::sync::CancellationToken;

use crate::config::MaintenanceConfig;
use crate::errors::AppError;

use crate::maintenance::{
    maintenance_org_ids, scoped_db_for_maintenance_org, TenantMaintenanceDeps,
};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Usage aggregation reconciliation interval: 5 minutes.
///
/// This is hardcoded rather than exposed in `MaintenanceConfig` because:
/// 1. The task is not latency-critical (real-time path handles most updates)
/// 2. Running more frequently than every 5 minutes adds unnecessary DB load
/// 3. Running less frequently increases the window where stale counters are visible
const AGGREGATION_INTERVAL_SECS: u64 = 300;

// ---------------------------------------------------------------------------
// AggregationResult
// ---------------------------------------------------------------------------

/// Result of a single usage aggregation reconciliation pass.
///
/// Reports how many entities had discrepancies between their denormalized
/// counters and the aggregated source-of-truth in `usage_records`.
#[derive(Debug, Default)]
pub struct AggregationResult {
    pub tasks_reconciled: u64,
    /// Number of chat sessions where denormalized counters were corrected.
    pub sessions_reconciled: u64,
    /// Total number of discrepancies found (sum of the above).
    pub total_discrepancies: u64,
}

// ---------------------------------------------------------------------------
// Public API — aggregation loop
// ---------------------------------------------------------------------------

/// Runs the usage aggregation maintenance loop.
///
/// This function runs indefinitely until the cancellation token is triggered.
/// It periodically reconciles denormalized usage counters on `tasks`,
/// and `chat_sessions` against the source-of-truth `usage_records` table.
///
/// The loop respects the cancellation token for graceful shutdown.
/// Errors are logged but do NOT crash the task — the loop continues
/// on the next interval tick.
pub async fn run_usage_aggregation_loop(
    deps: TenantMaintenanceDeps,
    config: MaintenanceConfig,
    cancel: CancellationToken,
) {
    // Usage aggregation interval: 5 minutes (hardcoded, not in MaintenanceConfig).
    // The `config` parameter is accepted for consistency with sibling maintenance
    // tasks and for future configurability.
    let _ = &config;
    let interval_duration = std::time::Duration::from_secs(AGGREGATION_INTERVAL_SECS);
    let mut interval = tokio::time::interval(interval_duration);

    // First tick completes immediately — skip it to avoid running on startup.
    interval.tick().await;

    loop {
        tokio::select! {
            _ = interval.tick() => {
                let start = Instant::now();
                match aggregate_usage(&deps).await {
                    Ok(result) => {
                        let duration_ms = start.elapsed().as_millis();
                        if result.total_discrepancies > 0 {
                            tracing::warn!(
                                tasks_reconciled = result.tasks_reconciled,
                                sessions_reconciled = result.sessions_reconciled,
                                total_discrepancies = result.total_discrepancies,
                                duration_ms = duration_ms,
                                "usage_aggregation: reconciliation found discrepancies"
                            );
                        } else {
                            tracing::debug!(
                                duration_ms = duration_ms,
                                "usage_aggregation: reconciliation complete, no discrepancies found"
                            );
                        }
                    }
                    Err(e) => {
                        let duration_ms = start.elapsed().as_millis();
                        tracing::error!(
                            error = %e,
                            duration_ms = duration_ms,
                            "usage_aggregation: reconciliation failed"
                        );
                    }
                }
            }
            _ = cancel.cancelled() => {
                tracing::info!("usage_aggregation: shutting down");
                break;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Public API — single reconciliation pass
// ---------------------------------------------------------------------------

/// Performs a single usage aggregation reconciliation pass.
///
/// Reconciles denormalized usage counters on `tasks`, and
/// `chat_sessions` against the aggregated source-of-truth in `usage_records`.
///
/// The two reconciliation passes (tasks, sessions) are independent
/// and are NOT wrapped in a single transaction. This is intentional:
/// - Each pass is independent — there is no cross-table consistency requirement
/// - A single large transaction would hold locks on all three tables simultaneously
/// - If one pass fails, the others still succeed and their corrections are preserved
/// - The reconciliation is idempotent — running it again produces the same result
///
/// Returns the number of entities where discrepancies were found and corrected.
pub async fn aggregate_usage(deps: &TenantMaintenanceDeps) -> Result<AggregationResult, AppError> {
    let org_ids = maintenance_org_ids(&deps.system_pool)
        .await
        .map_err(AppError::from)?;

    let mut result = AggregationResult::default();

    for org_id in org_ids {
        let mut db = scoped_db_for_maintenance_org(deps, org_id).await?;
        let org_result = aggregate_usage_for_org(db.conn())
            .await
            .map_err(AppError::from)?;
        db.commit().await.map_err(AppError::from)?;
        result.tasks_reconciled += org_result.tasks_reconciled;
        result.sessions_reconciled += org_result.sessions_reconciled;
    }

    result.total_discrepancies = result.tasks_reconciled + result.sessions_reconciled;

    Ok(result)
}

async fn aggregate_usage_for_org(
    conn: &mut sqlx::PgConnection,
) -> Result<AggregationResult, sqlx::Error> {
    let mut result = AggregationResult::default();

    // Step 2: Reconcile tasks
    //
    // Tasks are reconciled regardless of status — a completed task may still
    // have drifted counters that should be corrected for accurate billing
    // reports.
    let tasks_updated = sqlx::query!(
        r#"
        UPDATE tasks t
        SET
            cost_usd = agg.sum_cost,
            input_tokens = agg.sum_input,
            output_tokens = agg.sum_output
        FROM (
            SELECT
                task_id,
                COALESCE(SUM(cost_usd), 0) AS sum_cost,
                COALESCE(SUM(input_tokens), 0) AS sum_input,
                COALESCE(SUM(output_tokens), 0) AS sum_output
            FROM usage_records
            WHERE task_id IS NOT NULL
            GROUP BY task_id
        ) agg
        WHERE t.id = agg.task_id
          AND (
              t.cost_usd IS DISTINCT FROM agg.sum_cost
              OR t.input_tokens IS DISTINCT FROM agg.sum_input
              OR t.output_tokens IS DISTINCT FROM agg.sum_output
          )
        "#,
    )
    .execute(&mut *conn)
    .await?;
    result.tasks_reconciled = tasks_updated.rows_affected();

    // Step 3: Reconcile chat sessions
    //
    // Note the different column names on chat_sessions: total_cost_usd,
    // total_input_tokens, total_output_tokens (prefixed with "total_")
    // as defined in the schema (API_AND_STORAGE.md §2.8).
    let sessions_updated = sqlx::query!(
        r#"
        UPDATE chat_sessions cs
        SET
            total_cost_usd = agg.sum_cost,
            total_input_tokens = agg.sum_input,
            total_output_tokens = agg.sum_output
        FROM (
            SELECT
                session_id,
                COALESCE(SUM(cost_usd), 0) AS sum_cost,
                COALESCE(SUM(input_tokens), 0) AS sum_input,
                COALESCE(SUM(output_tokens), 0) AS sum_output
            FROM usage_records
            WHERE session_id IS NOT NULL
            GROUP BY session_id
        ) agg
        WHERE cs.id = agg.session_id
          AND (
              cs.total_cost_usd IS DISTINCT FROM agg.sum_cost
              OR cs.total_input_tokens IS DISTINCT FROM agg.sum_input
              OR cs.total_output_tokens IS DISTINCT FROM agg.sum_output
          )
        "#,
    )
    .execute(&mut *conn)
    .await?;
    result.sessions_reconciled = sessions_updated.rows_affected();

    result.total_discrepancies = result.tasks_reconciled + result.sessions_reconciled;

    Ok(result)
}
