// ---------------------------------------------------------------------------
// g5-api/src/services/identity/spend_service.rs — Spend Service
//
// Business logic for spend tracking, limit enforcement, usage record
// ingestion, and historical spend queries per API_AND_STORAGE.md §3.9
// (Settings/Spend API) and §3.15 (Spend History API).
//
// This service is the financial control plane for workspace-level cost
// management. The scheduler calls `check_spend_limit` on every tick to
// gate task scheduling. Workers call `record_usage` after LLM calls.
// ---------------------------------------------------------------------------

use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc};
use g5_authz::ScopedDb;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::errors::G5Error;
use crate::models::workspace::{WorkspaceSettings, DEFAULT_BOUNDED_EXECUTION_SLOTS};
use crate::ws::{Channel, WsEvent};

use crate::services::EventBus;

// ---------------------------------------------------------------------------
// DTO Types
// ---------------------------------------------------------------------------

/// Spend summary for a workspace within the current billing period.
/// Matches the SpendSummaryDTO from API_AND_STORAGE.md §3.9.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpendSummaryDTO {
    pub period_start: String,
    pub period_end: String,
    pub total_spend_usd: Decimal,
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
    pub limit_spend_usd: Option<Decimal>,
    pub limit_input_tokens: Option<i64>,
    pub limit_output_tokens: Option<i64>,
    pub is_spend_limit_reached: bool,
    pub by_task: Vec<TaskSpendBreakdown>,
    /// Usage with no matching retained task in this workspace.
    pub unattributed_spend_usd: Decimal,
    /// Usage for retained tasks omitted from the bounded breakdown.
    pub other_task_spend_usd: Decimal,
}

/// Per-task spend breakdown within the current period.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct TaskSpendBreakdown {
    pub task_id: Uuid,
    pub task_name: String,
    pub task_type: String,
    pub spend_usd: Decimal,
    pub input_tokens: i64,
    pub output_tokens: i64,
}

/// Result of a spend limit check — used by the scheduler.
#[derive(Debug, Clone)]
pub enum SpendLimitStatus {
    /// Workspace is within all configured spend limits.
    WithinLimits,
    /// Workspace has exceeded a spend limit.
    Exceeded {
        /// The specific limit that was exceeded.
        reason: String,
    },
}

/// Historical spend data across multiple periods.
/// Matches SpendHistoryDTO from API_AND_STORAGE.md §3.15.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpendHistoryDTO {
    pub granularity: String,
    pub periods: Vec<SpendPeriodDTO>,
    pub daily: Vec<DailySpendDTO>,
}

/// A single period in the spend history.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpendPeriodDTO {
    pub period_start: String,
    pub period_end: String,
    pub total_spend_usd: Decimal,
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
    pub task_count: i64,
}

/// Daily spend data point for charts.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DailySpendDTO {
    pub date: String,
    pub spend_usd: Decimal,
    pub input_tokens: i64,
    pub output_tokens: i64,
}

/// Request to record a single usage entry (from worker).
/// Matches the POST /api/internal/usage request body items.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordUsageRequest {
    pub org_id: Uuid,
    pub workspace_id: Uuid,
    pub task_id: Option<Uuid>,
    pub session_id: Option<Uuid>,
    pub provider: String,
    pub model: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub total_tokens: Option<i64>,
    pub cache_creation_tokens: Option<i64>,
    pub cache_read_tokens: Option<i64>,
    pub cost_usd: Decimal,
    pub request_type: String,
    pub finish_reason: Option<String>,
    pub duration_ms: Option<i32>,
}

/// Result of batch usage record insertion.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordUsageResult {
    pub inserted: usize,
}

/// Aggregated usage data for billing.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageAggregateDTO {
    pub total_cost_usd: Decimal,
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
    pub total_cache_creation_tokens: i64,
    pub total_cache_read_tokens: i64,
    pub record_count: i64,
    pub by_provider_model: Vec<ProviderModelBreakdown>,
}

/// Per-provider/model usage breakdown.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ProviderModelBreakdown {
    pub provider: String,
    pub model: String,
    pub cost_usd: Decimal,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub request_count: i64,
}

// ---------------------------------------------------------------------------
// Allowed request types for validation
// ---------------------------------------------------------------------------

const VALID_REQUEST_TYPES: &[&str] = &["chat", "completion", "summarization", "embedding"];

// ---------------------------------------------------------------------------
// SpendService
// ---------------------------------------------------------------------------

/// Service for spend tracking, limit enforcement, usage record ingestion,
/// and historical spend queries.
///
/// Holds an `EventBus` for publishing spend limit events
/// to WebSocket subscribers. Database access is provided per-request
/// via `ScopedDb`.
pub struct SpendService {
    event_bus: EventBus,
}

impl SpendService {
    /// Create a new `SpendService` with the given event bus.
    #[must_use]
    pub fn new(event_bus: EventBus) -> Self {
        Self { event_bus }
    }

    // -----------------------------------------------------------------------
    // Spend Summary
    // -----------------------------------------------------------------------

    /// Returns the current spend vs. limits for a workspace, including a
    /// per-task breakdown. Powers `GET /api/v1/workspaces/:workspaceId/settings/spend`.
    ///
    /// # Errors
    ///
    /// - `G5Error::NotFound` — workspace does not exist.
    /// - `G5Error::Internal` — database error.
    pub async fn get_spend_summary(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
    ) -> Result<SpendSummaryDTO, G5Error> {
        // 0. Verify workspace exists.
        self.verify_workspace_exists(db, workspace_id).await?;

        // 1. Fetch workspace settings.
        let settings = self.fetch_workspace_settings(db, workspace_id).await?;

        // 2. Compute the current period window.
        let (period_start, period_end) =
            compute_period_window(&settings.spend_period, settings.spend_period_start);

        // Read totals and the bounded breakdown in one snapshot, including usage
        // from tasks created before this period and conversations without a task.
        let rows = sqlx::query_as::<_, (Option<Uuid>, String, String, Decimal, i64, i64, Decimal)>(
            "WITH period_usage AS MATERIALIZED (
                SELECT * FROM usage_records
                WHERE workspace_id = $1 AND created_at >= $2 AND created_at < $3
             ), task_spend AS MATERIALIZED (
                SELECT t.id AS task_id, t.name AS task_name, t.kind AS task_type,
                       SUM(ur.cost_usd) AS spend_usd,
                       SUM(ur.input_tokens)::bigint AS input_tokens,
                       SUM(ur.output_tokens)::bigint AS output_tokens
                FROM period_usage ur
                JOIN tasks t ON t.id = ur.task_id AND t.org_id = ur.org_id AND t.workspace_id = ur.workspace_id
                GROUP BY t.id, t.name, t.kind
             )
             SELECT * FROM (
                SELECT NULL::uuid AS task_id, ''::text AS task_name, ''::text AS task_type,
                       COALESCE(SUM(cost_usd), 0) AS spend_usd,
                       COALESCE(SUM(input_tokens), 0)::bigint AS input_tokens,
                       COALESCE(SUM(output_tokens), 0)::bigint AS output_tokens,
                       COALESCE(SUM(cost_usd), 0) - COALESCE((SELECT SUM(spend_usd) FROM task_spend), 0)
                           AS unattributed_spend_usd
                FROM period_usage
                UNION ALL
                SELECT limited.*, 0::numeric FROM (
                    SELECT * FROM task_spend ORDER BY spend_usd DESC, task_id LIMIT 100
                ) limited
             ) summary ORDER BY task_id IS NULL DESC, spend_usd DESC, task_id",
        ).bind(workspace_id).bind(period_start).bind(period_end).fetch_all(db.conn()).await?;
        let (mut total_spend_usd, mut total_input_tokens, mut total_output_tokens) =
            (Decimal::ZERO, 0, 0);
        let mut unattributed_spend_usd = Decimal::ZERO;
        let mut by_task = Vec::new();
        for (task_id, task_name, task_type, spend_usd, input_tokens, output_tokens, unattributed) in
            rows
        {
            if let Some(task_id) = task_id {
                by_task.push(TaskSpendBreakdown {
                    task_id,
                    task_name,
                    task_type,
                    spend_usd,
                    input_tokens,
                    output_tokens,
                });
            } else {
                (total_spend_usd, total_input_tokens, total_output_tokens) =
                    (spend_usd, input_tokens, output_tokens);
                unattributed_spend_usd = unattributed;
            }
        }
        let other_task_spend_usd = total_spend_usd
            - unattributed_spend_usd
            - by_task.iter().map(|row| row.spend_usd).sum::<Decimal>();

        // 5. Determine is_spend_limit_reached.
        let is_spend_limit_reached = is_limit_exceeded(
            &settings,
            total_spend_usd,
            total_input_tokens,
            total_output_tokens,
        );

        // 6. Construct and return SpendSummaryDTO.
        Ok(SpendSummaryDTO {
            period_start: period_start.to_rfc3339(),
            period_end: period_end.to_rfc3339(),
            total_spend_usd,
            total_input_tokens,
            total_output_tokens,
            limit_spend_usd: settings.max_spend_usd,
            limit_input_tokens: settings.max_input_tokens,
            limit_output_tokens: settings.max_output_tokens,
            is_spend_limit_reached,
            by_task,
            unattributed_spend_usd,
            other_task_spend_usd,
        })
    }

    // -----------------------------------------------------------------------
    // Spend Limit Check (Scheduler Hot Path)
    // -----------------------------------------------------------------------

    /// Lightweight check used by the scheduler on every tick to determine if
    /// a workspace has exceeded any spend limit. Optimized for speed.
    ///
    /// # Fast Path
    ///
    /// If all limits are NULL, returns `WithinLimits` immediately without
    /// executing any SQL query.
    ///
    /// # Errors
    ///
    /// - `G5Error::NotFound` — workspace does not exist.
    /// - `G5Error::Internal` — database error.
    pub async fn check_spend_limit(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
    ) -> Result<SpendLimitStatus, G5Error> {
        // 1. Fetch workspace settings.
        let settings = self.fetch_workspace_settings(db, workspace_id).await?;

        // 2. Fast path: if all limits are NULL, return immediately.
        if settings.max_spend_usd.is_none()
            && settings.max_input_tokens.is_none()
            && settings.max_output_tokens.is_none()
        {
            return Ok(SpendLimitStatus::WithinLimits);
        }

        // 3. Compute the current period window.
        let (period_start, period_end) =
            compute_period_window(&settings.spend_period, settings.spend_period_start);

        // 4. Execute the spend aggregation query.
        let (total_spend_usd, total_input_tokens, total_output_tokens) =
            Self::period_spend_totals(db, workspace_id, period_start, period_end).await?;

        // 5. Check each configured limit.
        if let Some(max_usd) = settings.max_spend_usd {
            if total_spend_usd >= max_usd {
                return Ok(SpendLimitStatus::Exceeded {
                    reason: "max_spend_usd".to_string(),
                });
            }
        }

        if let Some(max_input) = settings.max_input_tokens {
            if total_input_tokens >= max_input {
                return Ok(SpendLimitStatus::Exceeded {
                    reason: "max_input_tokens".to_string(),
                });
            }
        }

        if let Some(max_output) = settings.max_output_tokens {
            if total_output_tokens >= max_output {
                return Ok(SpendLimitStatus::Exceeded {
                    reason: "max_output_tokens".to_string(),
                });
            }
        }

        Ok(SpendLimitStatus::WithinLimits)
    }

    // -----------------------------------------------------------------------
    // Event Emission
    // -----------------------------------------------------------------------

    /// Publishes a `workspace.spend_limit_reached` WebSocket event.
    /// Called by the scheduler when a limit transition is detected.
    ///
    /// # Errors
    ///
    /// - `G5Error::Internal` — event bus failure.
    pub async fn emit_spend_limit_reached(
        &self,
        workspace_id: Uuid,
        period_spend_usd: Decimal,
        limit_spend_usd: Decimal,
        reason: &str,
    ) -> Result<(), G5Error> {
        let event = WsEvent::SpendLimitReached {
            workspace_id,
            period_spend_usd: period_spend_usd.to_string().parse::<f64>().unwrap_or_else(|e| {
                tracing::error!(error = %e, decimal = %period_spend_usd, "Failed to convert period_spend_usd Decimal to f64");
                0.0
            }),
            limit_spend_usd: limit_spend_usd.to_string().parse::<f64>().unwrap_or_else(|e| {
                tracing::error!(error = %e, decimal = %limit_spend_usd, "Failed to convert limit_spend_usd Decimal to f64");
                0.0
            }),
            reason: reason.to_string(),
        };

        let channel = Channel::Workspace(workspace_id);
        self.event_bus.publish(&channel, &event);

        tracing::warn!(
            workspace_id = %workspace_id,
            reason = %reason,
            spend = %period_spend_usd,
            limit = %limit_spend_usd,
            "Spend limit reached"
        );

        Ok(())
    }

    /// Publishes a `workspace.spend_limit_cleared` WebSocket event.
    /// Called when a spend limit is raised or a billing period resets.
    ///
    /// # Errors
    ///
    /// - `G5Error::Internal` — event bus failure.
    pub async fn emit_spend_limit_cleared(
        &self,
        workspace_id: Uuid,
        reason: &str,
    ) -> Result<(), G5Error> {
        let event = WsEvent::SpendLimitCleared {
            workspace_id,
            reason: reason.to_string(),
        };

        let channel = Channel::Workspace(workspace_id);
        self.event_bus.publish(&channel, &event);

        tracing::info!(
            workspace_id = %workspace_id,
            reason = %reason,
            "Spend limit cleared"
        );

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Spend History
    // -----------------------------------------------------------------------

    /// Returns historical spend data across multiple past periods.
    /// Powers `GET /api/v1/workspaces/:workspaceId/settings/spend/history`.
    ///
    /// # Parameters
    ///
    /// - `workspace_id` — Target workspace.
    /// - `periods` — Number of past periods to return (default: 6).
    /// - `granularity` — `"hourly"`, `"daily"`, `"weekly"`, `"monthly"`, or
    ///   `None` (auto = match `spend_period` from workspace settings).
    ///
    /// # Errors
    ///
    /// - `G5Error::NotFound` — workspace does not exist.
    /// - `G5Error::Internal` — database error.
    pub async fn get_spend_history(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
        periods: i32,
        granularity: Option<String>,
    ) -> Result<SpendHistoryDTO, G5Error> {
        // 0. Verify workspace exists.
        self.verify_workspace_exists(db, workspace_id).await?;

        // 1. Fetch workspace settings.
        let settings = self.fetch_workspace_settings(db, workspace_id).await?;

        // 2. Determine effective granularity.
        let effective_granularity = match granularity {
            Some(g) => g,
            None => {
                if settings.spend_period == "total" {
                    "monthly".to_string()
                } else {
                    settings.spend_period.clone()
                }
            }
        };

        // 3. Compute time boundaries for the requested number of periods,
        //    working backwards from now.
        let now = Utc::now();
        let mut period_boundaries: Vec<(DateTime<Utc>, DateTime<Utc>)> =
            Vec::with_capacity(periods as usize);

        let mut current_end = now;
        for _ in 0..periods {
            let current_start = subtract_period(&effective_granularity, current_end);
            period_boundaries.push((current_start, current_end));
            current_end = current_start;
        }

        // Reverse so periods are in chronological order (oldest first).
        period_boundaries.reverse();

        let overall_start = period_boundaries.first().map(|(s, _)| *s).unwrap_or(now);
        let overall_end = period_boundaries.last().map(|(_, e)| *e).unwrap_or(now);

        // 4. For each period, query aggregated spend from usage_records.
        let mut period_dtos: Vec<SpendPeriodDTO> = Vec::with_capacity(periods as usize);

        for (p_start, p_end) in &period_boundaries {
            let row = sqlx::query!(
                "SELECT \
                     COALESCE(SUM(ur.cost_usd), 0) AS \"total_spend_usd!\", \
                     COALESCE(SUM(ur.input_tokens), 0)::bigint AS \"total_input_tokens!\", \
                     COALESCE(SUM(ur.output_tokens), 0)::bigint AS \"total_output_tokens!\", \
                     COUNT(DISTINCT ur.task_id) AS \"task_count!\" \
                 FROM usage_records ur \
                 WHERE ur.workspace_id = $1 \
                   AND ur.created_at >= $2 \
                   AND ur.created_at < $3",
                workspace_id,
                p_start,
                p_end,
            )
            .fetch_one(db.conn())
            .await?;

            period_dtos.push(SpendPeriodDTO {
                period_start: p_start.to_rfc3339(),
                period_end: p_end.to_rfc3339(),
                total_spend_usd: row.total_spend_usd,
                total_input_tokens: row.total_input_tokens,
                total_output_tokens: row.total_output_tokens,
                task_count: row.task_count,
            });
        }

        // 5. Compute daily breakdown across the entire requested range.
        let daily = sqlx::query!(
            "SELECT \
                 DATE(ur.created_at) AS \"date!: NaiveDate\", \
                 COALESCE(SUM(ur.cost_usd), 0) AS \"spend_usd!\", \
                 COALESCE(SUM(ur.input_tokens), 0)::bigint AS \"input_tokens!\", \
                 COALESCE(SUM(ur.output_tokens), 0)::bigint AS \"output_tokens!\" \
             FROM usage_records ur \
             WHERE ur.workspace_id = $1 \
               AND ur.created_at >= $2 \
               AND ur.created_at < $3 \
             GROUP BY DATE(ur.created_at) \
             ORDER BY DATE(ur.created_at)",
            workspace_id,
            overall_start,
            overall_end,
        )
        .fetch_all(db.conn())
        .await?
        .into_iter()
        .map(|row| DailySpendDTO {
            date: row.date.format("%Y-%m-%d").to_string(),
            spend_usd: row.spend_usd,
            input_tokens: row.input_tokens,
            output_tokens: row.output_tokens,
        })
        .collect();

        // 6. Construct and return SpendHistoryDTO.
        Ok(SpendHistoryDTO {
            granularity: effective_granularity,
            periods: period_dtos,
            daily,
        })
    }

    // -----------------------------------------------------------------------
    // Usage Record Ingestion
    // -----------------------------------------------------------------------

    /// Batch inserts usage records and updates denormalized cost counters.
    /// Powers `POST /api/internal/usage` called by workers after LLM calls.
    ///
    /// All inserts and updates are performed in a SINGLE transaction to
    /// ensure denormalized counters stay consistent with `usage_records`.
    ///
    /// # Errors
    ///
    /// - `G5Error::ValidationError` — invalid record (missing required fields,
    ///   invalid `request_type`, or negative `cost_usd`).
    /// - `G5Error::Internal` — database error.
    pub async fn record_usage(
        &self,
        db: &mut ScopedDb,
        records: Vec<RecordUsageRequest>,
    ) -> Result<RecordUsageResult, G5Error> {
        // Validate all records before starting the transaction.
        for (idx, record) in records.iter().enumerate() {
            // Validate request_type.
            if !VALID_REQUEST_TYPES.contains(&record.request_type.as_str()) {
                return Err(G5Error::ValidationError {
                    details: serde_json::json!({
                        "message": format!(
                            "Record at index {} has invalid request_type: '{}'",
                            idx, record.request_type
                        ),
                        "valid_types": VALID_REQUEST_TYPES,
                    }),
                });
            }

            // Validate cost_usd is non-negative.
            if record.cost_usd < Decimal::ZERO {
                return Err(G5Error::ValidationError {
                    details: serde_json::json!({
                        "message": format!(
                            "Record at index {} has negative cost_usd: {}",
                            idx, record.cost_usd
                        ),
                    }),
                });
            }
        }

        let count = records.len();

        // Begin a database transaction.

        for record in &records {
            // Generate UUIDv7 for the new usage record.
            let record_id = Uuid::now_v7();

            let total_tokens = record
                .total_tokens
                .unwrap_or(record.input_tokens + record.output_tokens);
            let cache_creation_tokens = record.cache_creation_tokens.unwrap_or(0);
            let cache_read_tokens = record.cache_read_tokens.unwrap_or(0);

            sqlx::query(
                "INSERT INTO usage_records (
                     id, org_id, workspace_id, task_id, session_id,
                     provider, model, input_tokens, output_tokens, total_tokens,
                     cache_creation_tokens, cache_read_tokens, cost_usd,
                     request_type, finish_reason, duration_ms
                 ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16)",
            )
            .bind(record_id)
            .bind(record.org_id)
            .bind(record.workspace_id)
            .bind(record.task_id)
            .bind(record.session_id)
            .bind(&record.provider)
            .bind(&record.model)
            .bind(record.input_tokens)
            .bind(record.output_tokens)
            .bind(total_tokens)
            .bind(cache_creation_tokens)
            .bind(cache_read_tokens)
            .bind(record.cost_usd)
            .bind(&record.request_type)
            .bind(&record.finish_reason)
            .bind(record.duration_ms)
            .execute(db.conn())
            .await?;

            // b. If task_id is set, update the task's denormalized counters.
            if let Some(task_id) = record.task_id {
                sqlx::query!(
                    "UPDATE tasks SET \
                         input_tokens = input_tokens + $1, \
                         output_tokens = output_tokens + $2, \
                         cost_usd = cost_usd + $3, \
                         updated_at = now() \
                     WHERE id = $4",
                    record.input_tokens,
                    record.output_tokens,
                    record.cost_usd,
                    task_id,
                )
                .execute(db.conn())
                .await?;
            }

            // d. If session_id is set, update the chat session's denormalized counters.
            if let Some(session_id) = record.session_id {
                sqlx::query!(
                    "UPDATE chat_sessions SET \
                         total_input_tokens = total_input_tokens + $1, \
                         total_output_tokens = total_output_tokens + $2, \
                         total_cost_usd = total_cost_usd + $3 \
                     WHERE id = $4",
                    record.input_tokens,
                    record.output_tokens,
                    record.cost_usd,
                    session_id,
                )
                .execute(db.conn())
                .await?;
            }
        }

        // Commit the transaction.

        // Log the batch at workspace level (use first record's workspace_id).
        if let Some(first) = records.first() {
            tracing::info!(
                workspace_id = %first.workspace_id,
                count = count,
                "Usage records inserted"
            );
        }

        Ok(RecordUsageResult { inserted: count })
    }

    // -----------------------------------------------------------------------
    // Usage Aggregation
    // -----------------------------------------------------------------------

    /// Aggregates usage records for a workspace within an arbitrary time
    /// window. Used for billing reports and reconciliation.
    ///
    /// # Errors
    ///
    /// - `G5Error::Internal` — database error.
    pub async fn aggregate_usage(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
        period_start: DateTime<Utc>,
        period_end: DateTime<Utc>,
    ) -> Result<UsageAggregateDTO, G5Error> {
        // 1. Query aggregated totals.
        let totals = sqlx::query!(
            "SELECT \
                 COALESCE(SUM(cost_usd), 0) AS \"total_cost_usd!\", \
                 COALESCE(SUM(input_tokens), 0)::bigint AS \"total_input_tokens!\", \
                 COALESCE(SUM(output_tokens), 0)::bigint AS \"total_output_tokens!\", \
                 COALESCE(SUM(cache_creation_tokens), 0)::bigint AS \"total_cache_creation_tokens!\", \
                 COALESCE(SUM(cache_read_tokens), 0)::bigint AS \"total_cache_read_tokens!\", \
                 COUNT(*) AS \"record_count!\" \
             FROM usage_records \
             WHERE workspace_id = $1 \
               AND created_at >= $2 \
               AND created_at < $3",
            workspace_id,
            period_start,
            period_end,
        )
        .fetch_one(db.conn())
        .await?;

        // 2. Query per-provider/model breakdown.
        let breakdown = sqlx::query_as!(
            ProviderModelBreakdown,
            "SELECT \
                 provider AS \"provider!\", \
                 model AS \"model!\", \
                 COALESCE(SUM(cost_usd), 0) AS \"cost_usd!\", \
                 COALESCE(SUM(input_tokens), 0)::bigint AS \"input_tokens!\", \
                 COALESCE(SUM(output_tokens), 0)::bigint AS \"output_tokens!\", \
                 COUNT(*) AS \"request_count!\" \
             FROM usage_records \
             WHERE workspace_id = $1 \
               AND created_at >= $2 \
               AND created_at < $3 \
             GROUP BY provider, model \
             ORDER BY COALESCE(SUM(cost_usd), 0) DESC",
            workspace_id,
            period_start,
            period_end,
        )
        .fetch_all(db.conn())
        .await?;

        // 3. Construct and return UsageAggregateDTO.
        Ok(UsageAggregateDTO {
            total_cost_usd: totals.total_cost_usd,
            total_input_tokens: totals.total_input_tokens,
            total_output_tokens: totals.total_output_tokens,
            total_cache_creation_tokens: totals.total_cache_creation_tokens,
            total_cache_read_tokens: totals.total_cache_read_tokens,
            record_count: totals.record_count,
            by_provider_model: breakdown,
        })
    }

    // -----------------------------------------------------------------------
    // Private Helpers
    // -----------------------------------------------------------------------

    async fn period_spend_totals(
        db: &mut ScopedDb,
        workspace_id: Uuid,
        period_start: DateTime<Utc>,
        period_end: DateTime<Utc>,
    ) -> Result<(Decimal, i64, i64), G5Error> {
        Ok(sqlx::query_as(
            "SELECT COALESCE(SUM(ur.cost_usd), 0),
                    COALESCE(SUM(ur.input_tokens), 0)::bigint,
                    COALESCE(SUM(ur.output_tokens), 0)::bigint
             FROM usage_records ur
             WHERE ur.workspace_id = $1
               AND ur.created_at >= $2 AND ur.created_at < $3",
        )
        .bind(workspace_id)
        .bind(period_start)
        .bind(period_end)
        .fetch_one(db.conn())
        .await?)
    }

    /// Fetch workspace settings for a workspace.
    ///
    /// If the workspace has no explicit settings row, returns sensible defaults
    /// matching the SQL table `DEFAULT` values — a missing row is expected for
    /// newly-created workspaces and should **not** produce a 404.
    async fn fetch_workspace_settings(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
    ) -> Result<WorkspaceSettings, G5Error> {
        match sqlx::query_as::<_, WorkspaceSettings>(
            r#"
            SELECT workspace_id, org_id, max_spend_usd,
                   spend_period, spend_period_start, max_input_tokens,
                   max_output_tokens, idle_timeout_secs, response_deadline_secs,
                   bounded_execution_slots, permission_policy,
                   agent_may_modify_other_workspaces,
                   updated_by, updated_at
            FROM workspace_settings
            WHERE workspace_id = $1
            "#,
        )
        .bind(workspace_id)
        .fetch_optional(db.conn())
        .await?
        {
            Some(settings) => Ok(settings),
            None => Ok(WorkspaceSettings {
                workspace_id,
                org_id: Uuid::nil(),
                max_spend_usd: None,
                spend_period: "monthly".to_string(),
                spend_period_start: None,
                max_input_tokens: None,
                max_output_tokens: None,
                idle_timeout_secs: 28800,
                response_deadline_secs: 900,
                bounded_execution_slots: DEFAULT_BOUNDED_EXECUTION_SLOTS,
                permission_policy: None,
                agent_may_modify_other_workspaces: true,
                updated_by: None,
                updated_at: Utc::now(),
            }),
        }
    }

    /// Quick existence check for a workspace. Returns `NotFound` if the
    /// workspace does not exist. Used before returning default settings.
    async fn verify_workspace_exists(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
    ) -> Result<(), G5Error> {
        let exists: bool = sqlx::query_scalar!(
            r#"SELECT EXISTS(SELECT 1 FROM workspaces WHERE id = $1) AS "exists!""#,
            workspace_id,
        )
        .fetch_one(db.conn())
        .await?;

        if !exists {
            return Err(G5Error::NotFound {
                resource: format!("workspace {}", workspace_id),
            });
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Period Computation — Pure Functions
// ---------------------------------------------------------------------------

/// Compute the current billing period window based on the spend period
/// type and the anchor timestamp.
///
/// Returns `(period_start, period_end)`.
///
/// # Period Types
///
/// - `"total"` — Period start = anchor, period end = far future.
/// - `"hourly"` — Most recent hour boundary relative to anchor.
/// - `"daily"` — Most recent day boundary relative to anchor.
/// - `"weekly"` — Most recent 7-day boundary relative to anchor.
/// - `"monthly"` — Most recent month boundary relative to anchor,
///   using calendar months.
fn compute_period_window(
    spend_period: &str,
    spend_period_start: Option<DateTime<Utc>>,
) -> (DateTime<Utc>, DateTime<Utc>) {
    let now = Utc::now();

    // Determine the anchor timestamp.
    // If spend_period_start is not set, use the epoch so that the first
    // period window covers all historical data through the current period.
    let epoch = DateTime::UNIX_EPOCH;
    let anchor = spend_period_start.unwrap_or(epoch);

    match spend_period {
        "total" => {
            // Period never resets. Start = anchor, end = far future.
            // SAFETY: constant string, parse is infallible.
            let far_future = DateTime::parse_from_rfc3339("9999-12-31T23:59:59Z")
                .expect("constant RFC3339 string")
                .with_timezone(&Utc);
            (anchor, far_future)
        }
        "hourly" => {
            // Find the most recent hour boundary relative to anchor.
            let duration_since_anchor = now.signed_duration_since(anchor);
            let hours_elapsed = duration_since_anchor.num_hours();
            let period_start = anchor + Duration::hours(hours_elapsed);
            let period_end = period_start + Duration::hours(1);
            (period_start, period_end)
        }
        "daily" => {
            // Find the most recent day boundary relative to anchor.
            let duration_since_anchor = now.signed_duration_since(anchor);
            let days_elapsed = duration_since_anchor.num_days();
            let period_start = anchor + Duration::days(days_elapsed);
            let period_end = period_start + Duration::days(1);
            (period_start, period_end)
        }
        "weekly" => {
            // Find the most recent 7-day boundary relative to anchor.
            let duration_since_anchor = now.signed_duration_since(anchor);
            let weeks_elapsed = duration_since_anchor.num_weeks();
            let period_start = anchor + Duration::weeks(weeks_elapsed);
            let period_end = period_start + Duration::weeks(1);
            (period_start, period_end)
        }
        "monthly" => {
            // Find the most recent month boundary relative to anchor.
            // Calendar months: if anchor is Jan 15, periods are
            // Jan 15–Feb 14, Feb 15–Mar 14, etc.
            let period_start = find_monthly_period_start(anchor, now);
            let period_end = add_one_month(period_start);
            (period_start, period_end)
        }
        _ => {
            // Unknown period type — treat as "total" for safety.
            tracing::warn!(
                spend_period = %spend_period,
                "Unknown spend_period type, treating as 'total'"
            );
            // SAFETY: constant string, parse is infallible.
            let far_future = DateTime::parse_from_rfc3339("9999-12-31T23:59:59Z")
                .expect("constant RFC3339 string")
                .with_timezone(&Utc);
            (anchor, far_future)
        }
    }
}

/// Find the most recent monthly period start relative to the anchor.
///
/// Starting from the anchor, advance by calendar months until we find
/// the period that contains `now`.
fn find_monthly_period_start(anchor: DateTime<Utc>, now: DateTime<Utc>) -> DateTime<Utc> {
    if now < anchor {
        // If now is before anchor, return anchor (period hasn't started yet).
        return anchor;
    }

    let mut period_start = anchor;
    loop {
        let next_start = add_one_month(period_start);
        if next_start > now {
            return period_start;
        }
        period_start = next_start;
    }
}

/// Add one calendar month to a DateTime, handling month-end overflow.
///
/// If the anchor day doesn't exist in the target month (e.g., Jan 31 → Feb),
/// clamp to the last day of the target month.
fn add_one_month(dt: DateTime<Utc>) -> DateTime<Utc> {
    let year = dt.year();
    let month = dt.month();
    let day = dt.day();

    let (new_year, new_month) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };

    // Compute the last day of the new month.
    let last_day_of_month = last_day_of_month(new_year, new_month);
    let clamped_day = day.min(last_day_of_month);

    // Reconstruct the datetime with the new date but same time.
    // Fall back to the 1st if clamped_day is somehow invalid (shouldn't happen
    // given last_day_of_month, but avoids a panic).
    let new_date = NaiveDate::from_ymd_opt(new_year, new_month, clamped_day)
        .or_else(|| NaiveDate::from_ymd_opt(new_year, new_month, 1))
        .unwrap_or(NaiveDate::MIN);

    let time = dt.time();
    new_date.and_time(time).and_utc()
}

/// Returns the last day of a given month in a given year.
fn last_day_of_month(year: i32, month: u32) -> u32 {
    // Use the first day of the next month and subtract one day.
    let (next_year, next_month) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };

    // Fall back to the 28th (always valid) if from_ymd_opt somehow fails.
    let first_of_next = NaiveDate::from_ymd_opt(next_year, next_month, 1)
        .or_else(|| NaiveDate::from_ymd_opt(year, month, 28))
        .unwrap_or(NaiveDate::MIN);

    // pred_opt returns None only for NaiveDate::MIN, which can't happen
    // with any real calendar date.
    first_of_next.pred_opt().map(|d| d.day()).unwrap_or(28)
}

/// Subtract one period of the given granularity from a DateTime.
fn subtract_period(granularity: &str, dt: DateTime<Utc>) -> DateTime<Utc> {
    match granularity {
        "hourly" => dt - Duration::hours(1),
        "daily" => dt - Duration::days(1),
        "weekly" => dt - Duration::weeks(1),
        "monthly" => subtract_one_month(dt),
        _ => dt - Duration::days(30), // fallback
    }
}

/// Subtract one calendar month from a DateTime, handling month-end overflow.
fn subtract_one_month(dt: DateTime<Utc>) -> DateTime<Utc> {
    let year = dt.year();
    let month = dt.month();
    let day = dt.day();

    let (new_year, new_month) = if month == 1 {
        (year - 1, 12)
    } else {
        (year, month - 1)
    };

    let last_day = last_day_of_month(new_year, new_month);
    let clamped_day = day.min(last_day);

    let new_date = NaiveDate::from_ymd_opt(new_year, new_month, clamped_day)
        .or_else(|| NaiveDate::from_ymd_opt(new_year, new_month, 1))
        .unwrap_or(NaiveDate::MIN);

    let time = dt.time();
    new_date.and_time(time).and_utc()
}

/// Check whether any configured spend limit is exceeded.
fn is_limit_exceeded(
    settings: &WorkspaceSettings,
    total_spend_usd: Decimal,
    total_input_tokens: i64,
    total_output_tokens: i64,
) -> bool {
    if let Some(max_usd) = settings.max_spend_usd {
        if total_spend_usd >= max_usd {
            return true;
        }
    }
    if let Some(max_input) = settings.max_input_tokens {
        if total_input_tokens >= max_input {
            return true;
        }
    }
    if let Some(max_output) = settings.max_output_tokens {
        if total_output_tokens >= max_output {
            return true;
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// The usage one settled agent turn reports.
pub(crate) struct AgentTurnUsage<'a> {
    pub org_id: Uuid,
    pub workspace_id: Uuid,
    pub task_id: Uuid,
    pub session_id: Uuid,
    pub provider: &'a str,
    pub model: &'a str,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_creation_tokens: i64,
    pub cache_read_tokens: i64,
    pub cost_usd: f64,
    pub finish_reason: Option<&'a str>,
    pub duration_ms: Option<i32>,
}

/// Record a settled agent turn's usage in the caller's transaction: one
/// `usage_records` row and the task and session counters. It keeps the
/// turn writer's rules, which differ from [`SpendService::record_usage`]:
/// total tokens include cache tokens, cost is a float, and the session's cost
/// counter is left alone.
pub(crate) async fn record_agent_turn_usage(
    conn: &mut sqlx::PgConnection,
    usage: AgentTurnUsage<'_>,
) -> Result<(), sqlx::Error> {
    let total_tokens = usage
        .input_tokens
        .saturating_add(usage.output_tokens)
        .saturating_add(usage.cache_creation_tokens)
        .saturating_add(usage.cache_read_tokens);
    sqlx::query(
        "INSERT INTO usage_records(
             id, org_id, workspace_id, task_id, session_id,
             provider, model, input_tokens, output_tokens, total_tokens,
             cache_creation_tokens, cache_read_tokens, cost_usd,
             request_type, finish_reason, duration_ms)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,
                 $13::double precision::numeric,'chat',$14,$15)",
    )
    .bind(Uuid::now_v7())
    .bind(usage.org_id)
    .bind(usage.workspace_id)
    .bind(usage.task_id)
    .bind(usage.session_id)
    .bind(usage.provider)
    .bind(usage.model)
    .bind(usage.input_tokens)
    .bind(usage.output_tokens)
    .bind(total_tokens)
    .bind(usage.cache_creation_tokens)
    .bind(usage.cache_read_tokens)
    .bind(usage.cost_usd)
    .bind(usage.finish_reason)
    .bind(usage.duration_ms)
    .execute(&mut *conn)
    .await?;
    sqlx::query(
        "UPDATE tasks
         SET input_tokens = input_tokens + $2,
             output_tokens = output_tokens + $3,
             cost_usd = cost_usd + $4::double precision::numeric,
             updated_at = now()
         WHERE id = $1",
    )
    .bind(usage.task_id)
    .bind(usage.input_tokens)
    .bind(usage.output_tokens)
    .bind(usage.cost_usd)
    .execute(&mut *conn)
    .await?;
    sqlx::query(
        "UPDATE chat_sessions
         SET total_input_tokens = total_input_tokens + $3,
             total_output_tokens = total_output_tokens + $4
         WHERE org_id = $1 AND id = $2",
    )
    .bind(usage.org_id)
    .bind(usage.session_id)
    .bind(usage.input_tokens)
    .bind(usage.output_tokens)
    .execute(conn)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    // -- Period computation tests --

    #[test]
    fn test_compute_period_window_total() {
        let anchor = Utc.with_ymd_and_hms(2024, 1, 15, 0, 0, 0).unwrap();
        let (start, end) = compute_period_window("total", Some(anchor));
        assert_eq!(start, anchor);
        assert!(end.year() >= 9999);
    }

    #[test]
    fn test_compute_period_window_total_no_anchor() {
        let (start, end) = compute_period_window("total", None);
        // With no anchor, start defaults to epoch so the window covers all history.
        assert_eq!(start, DateTime::UNIX_EPOCH);
        assert!(end.year() >= 9999);
    }

    #[test]
    fn test_compute_period_window_daily() {
        let anchor = Utc.with_ymd_and_hms(2024, 1, 15, 14, 30, 0).unwrap();
        let (start, end) = compute_period_window("daily", Some(anchor));
        // Period should be 24 hours long.
        let duration = end.signed_duration_since(start);
        assert_eq!(duration.num_hours(), 24);
    }

    #[test]
    fn test_compute_period_window_weekly() {
        let anchor = Utc.with_ymd_and_hms(2024, 1, 15, 0, 0, 0).unwrap();
        let (start, end) = compute_period_window("weekly", Some(anchor));
        // Period should be 7 days long.
        let duration = end.signed_duration_since(start);
        assert_eq!(duration.num_days(), 7);
    }

    #[test]
    fn test_compute_period_window_hourly() {
        let anchor = Utc.with_ymd_and_hms(2024, 1, 15, 14, 30, 0).unwrap();
        let (start, end) = compute_period_window("hourly", Some(anchor));
        // Period should be 1 hour long.
        let duration = end.signed_duration_since(start);
        assert_eq!(duration.num_minutes(), 60);
    }

    #[test]
    fn test_compute_period_window_monthly() {
        let anchor = Utc.with_ymd_and_hms(2024, 1, 15, 0, 0, 0).unwrap();
        let (start, end) = compute_period_window("monthly", Some(anchor));
        // Start should be Jan 15 or a later month boundary.
        assert_eq!(start.day(), 15);
        // End should be one month after start.
        assert_eq!(end.day(), 15);
    }

    // -- Calendar month edge case tests --

    #[test]
    fn test_add_one_month_normal() {
        let dt = Utc.with_ymd_and_hms(2024, 1, 15, 12, 0, 0).unwrap();
        let result = add_one_month(dt);
        assert_eq!(result.year(), 2024);
        assert_eq!(result.month(), 2);
        assert_eq!(result.day(), 15);
    }

    #[test]
    fn test_add_one_month_end_overflow() {
        // Jan 31 → Feb: should clamp to Feb 28 (or 29 in leap year).
        let dt = Utc.with_ymd_and_hms(2024, 1, 31, 12, 0, 0).unwrap();
        let result = add_one_month(dt);
        assert_eq!(result.year(), 2024);
        assert_eq!(result.month(), 2);
        assert_eq!(result.day(), 29); // 2024 is a leap year
    }

    #[test]
    fn test_add_one_month_end_overflow_non_leap() {
        // Jan 31 → Feb in non-leap year: should clamp to Feb 28.
        let dt = Utc.with_ymd_and_hms(2023, 1, 31, 12, 0, 0).unwrap();
        let result = add_one_month(dt);
        assert_eq!(result.year(), 2023);
        assert_eq!(result.month(), 2);
        assert_eq!(result.day(), 28);
    }

    #[test]
    fn test_add_one_month_december_to_january() {
        let dt = Utc.with_ymd_and_hms(2024, 12, 15, 12, 0, 0).unwrap();
        let result = add_one_month(dt);
        assert_eq!(result.year(), 2025);
        assert_eq!(result.month(), 1);
        assert_eq!(result.day(), 15);
    }

    #[test]
    fn test_subtract_one_month_normal() {
        let dt = Utc.with_ymd_and_hms(2024, 3, 15, 12, 0, 0).unwrap();
        let result = subtract_one_month(dt);
        assert_eq!(result.year(), 2024);
        assert_eq!(result.month(), 2);
        assert_eq!(result.day(), 15);
    }

    #[test]
    fn test_subtract_one_month_january_to_december() {
        let dt = Utc.with_ymd_and_hms(2024, 1, 15, 12, 0, 0).unwrap();
        let result = subtract_one_month(dt);
        assert_eq!(result.year(), 2023);
        assert_eq!(result.month(), 12);
        assert_eq!(result.day(), 15);
    }

    #[test]
    fn test_subtract_one_month_end_overflow() {
        // Mar 31 → Feb: should clamp to Feb 29 (2024 is leap year).
        let dt = Utc.with_ymd_and_hms(2024, 3, 31, 12, 0, 0).unwrap();
        let result = subtract_one_month(dt);
        assert_eq!(result.year(), 2024);
        assert_eq!(result.month(), 2);
        assert_eq!(result.day(), 29);
    }

    #[test]
    fn test_last_day_of_month_february_leap() {
        assert_eq!(last_day_of_month(2024, 2), 29);
    }

    #[test]
    fn test_last_day_of_month_february_non_leap() {
        assert_eq!(last_day_of_month(2023, 2), 28);
    }

    #[test]
    fn test_last_day_of_month_december() {
        assert_eq!(last_day_of_month(2024, 12), 31);
    }

    #[test]
    fn test_last_day_of_month_april() {
        assert_eq!(last_day_of_month(2024, 4), 30);
    }

    // -- Limit check tests --

    #[test]
    fn test_is_limit_exceeded_no_limits() {
        let settings = make_test_settings(None, None, None);
        assert!(!is_limit_exceeded(
            &settings,
            Decimal::new(100, 0),
            1000,
            500,
        ));
    }

    #[test]
    fn test_is_limit_exceeded_usd_exceeded() {
        let settings = make_test_settings(Some(Decimal::new(50, 0)), None, None);
        assert!(is_limit_exceeded(&settings, Decimal::new(50, 0), 0, 0,));
    }

    #[test]
    fn test_is_limit_exceeded_usd_within() {
        let settings = make_test_settings(Some(Decimal::new(50, 0)), None, None);
        assert!(!is_limit_exceeded(&settings, Decimal::new(49, 0), 0, 0,));
    }

    #[test]
    fn test_is_limit_exceeded_input_tokens_exceeded() {
        let settings = make_test_settings(None, Some(1000), None);
        assert!(is_limit_exceeded(&settings, Decimal::ZERO, 1000, 0,));
    }

    #[test]
    fn test_is_limit_exceeded_output_tokens_exceeded() {
        let settings = make_test_settings(None, None, Some(500));
        assert!(is_limit_exceeded(&settings, Decimal::ZERO, 0, 500,));
    }

    #[test]
    fn test_is_limit_exceeded_all_within() {
        let settings = make_test_settings(Some(Decimal::new(100, 0)), Some(10000), Some(5000));
        assert!(!is_limit_exceeded(
            &settings,
            Decimal::new(50, 0),
            5000,
            2500,
        ));
    }

    // -- Validation tests --

    #[test]
    fn test_valid_request_types() {
        assert!(VALID_REQUEST_TYPES.contains(&"chat"));
        assert!(VALID_REQUEST_TYPES.contains(&"completion"));
        assert!(VALID_REQUEST_TYPES.contains(&"summarization"));
        assert!(VALID_REQUEST_TYPES.contains(&"embedding"));
        assert!(!VALID_REQUEST_TYPES.contains(&"unknown"));
    }

    // -- DTO serialization tests --

    #[test]
    fn test_spend_summary_dto_serialization() {
        let dto = SpendSummaryDTO {
            period_start: "2024-01-01T00:00:00Z".to_string(),
            period_end: "2024-02-01T00:00:00Z".to_string(),
            total_spend_usd: Decimal::new(5050, 2), // 50.50
            total_input_tokens: 100000,
            total_output_tokens: 50000,
            limit_spend_usd: Some(Decimal::new(10000, 2)), // 100.00
            limit_input_tokens: None,
            limit_output_tokens: None,
            is_spend_limit_reached: false,
            by_task: vec![],
            unattributed_spend_usd: Decimal::new(5050, 2),
            other_task_spend_usd: Decimal::ZERO,
        };
        let json = serde_json::to_value(&dto).unwrap();
        assert!(json.get("periodStart").is_some());
        assert!(json.get("periodEnd").is_some());
        assert!(json.get("totalSpendUsd").is_some());
        assert!(json.get("totalInputTokens").is_some());
        assert!(json.get("totalOutputTokens").is_some());
        assert!(json.get("limitSpendUsd").is_some());
        assert!(json.get("isSpendLimitReached").is_some());
        assert!(json.get("byTask").is_some());
    }

    #[test]
    fn test_task_spend_breakdown_serialization() {
        let breakdown = TaskSpendBreakdown {
            task_id: Uuid::nil(),
            task_name: "Test Task".into(),
            task_type: "compile".into(),
            spend_usd: Decimal::new(2550, 2), // 25.50
            input_tokens: 50000,
            output_tokens: 25000,
        };
        let json = serde_json::to_value(&breakdown).unwrap();
        assert!(json.get("taskId").is_some());
        assert!(json.get("taskName").is_some());
        assert!(json.get("taskType").is_some());
        assert!(json.get("spendUsd").is_some());
        assert!(json.get("inputTokens").is_some());
        assert!(json.get("outputTokens").is_some());
    }

    #[test]
    fn test_spend_history_dto_serialization() {
        let dto = SpendHistoryDTO {
            granularity: "monthly".to_string(),
            periods: vec![],
            daily: vec![],
        };
        let json = serde_json::to_value(&dto).unwrap();
        assert_eq!(json["granularity"], "monthly");
        assert!(json.get("periods").is_some());
        assert!(json.get("daily").is_some());
    }

    #[test]
    fn test_daily_spend_dto_serialization() {
        let dto = DailySpendDTO {
            date: "2024-01-15".to_string(),
            spend_usd: Decimal::new(1234, 2), // 12.34
            input_tokens: 5000,
            output_tokens: 2500,
        };
        let json = serde_json::to_value(&dto).unwrap();
        assert_eq!(json["date"], "2024-01-15");
        assert!(json.get("spendUsd").is_some());
        assert!(json.get("inputTokens").is_some());
        assert!(json.get("outputTokens").is_some());
    }

    #[test]
    fn test_record_usage_request_deserialization() {
        let json = r#"{
            "orgId": "550e8400-e29b-41d4-a716-446655440000",
            "workspaceId": "550e8400-e29b-41d4-a716-446655440001",
            "taskId": "550e8400-e29b-41d4-a716-446655440002",
            "provider": "anthropic",
            "model": "claude-sonnet-4-20250514",
            "inputTokens": 1000,
            "outputTokens": 500,
            "costUsd": "0.0150",
            "requestType": "chat"
        }"#;
        let req: RecordUsageRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.provider, "anthropic");
        assert_eq!(req.model, "claude-sonnet-4-20250514");
        assert_eq!(req.input_tokens, 1000);
        assert_eq!(req.output_tokens, 500);
        assert_eq!(req.request_type, "chat");
        assert!(req.task_id.is_some());
        assert!(req.session_id.is_none());
    }

    #[test]
    fn test_record_usage_result_serialization() {
        let result = RecordUsageResult { inserted: 5 };
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["inserted"], 5);
    }

    #[test]
    fn test_usage_aggregate_dto_serialization() {
        let dto = UsageAggregateDTO {
            total_cost_usd: Decimal::new(10050, 2), // 100.50
            total_input_tokens: 500000,
            total_output_tokens: 250000,
            total_cache_creation_tokens: 10000,
            total_cache_read_tokens: 50000,
            record_count: 42,
            by_provider_model: vec![ProviderModelBreakdown {
                provider: "anthropic".into(),
                model: "claude-sonnet-4-20250514".into(),
                cost_usd: Decimal::new(10050, 2),
                input_tokens: 500000,
                output_tokens: 250000,
                request_count: 42,
            }],
        };
        let json = serde_json::to_value(&dto).unwrap();
        assert!(json.get("totalCostUsd").is_some());
        assert!(json.get("totalInputTokens").is_some());
        assert!(json.get("totalOutputTokens").is_some());
        assert!(json.get("totalCacheCreationTokens").is_some());
        assert!(json.get("totalCacheReadTokens").is_some());
        assert!(json.get("recordCount").is_some());
        assert!(json.get("byProviderModel").is_some());

        let providers = json["byProviderModel"].as_array().unwrap();
        assert_eq!(providers.len(), 1);
        assert!(providers[0].get("provider").is_some());
        assert!(providers[0].get("model").is_some());
        assert!(providers[0].get("costUsd").is_some());
        assert!(providers[0].get("requestCount").is_some());
    }

    #[test]
    fn test_spend_period_dto_serialization() {
        let dto = SpendPeriodDTO {
            period_start: "2024-01-01T00:00:00Z".to_string(),
            period_end: "2024-02-01T00:00:00Z".to_string(),
            total_spend_usd: Decimal::new(5050, 2),
            total_input_tokens: 100000,
            total_output_tokens: 50000,
            task_count: 25,
        };
        let json = serde_json::to_value(&dto).unwrap();
        assert!(json.get("periodStart").is_some());
        assert!(json.get("periodEnd").is_some());
        assert!(json.get("totalSpendUsd").is_some());
        assert!(json.get("taskCount").is_some());
    }

    // -- Helper for tests --

    fn make_test_settings(
        max_spend_usd: Option<Decimal>,
        max_input_tokens: Option<i64>,
        max_output_tokens: Option<i64>,
    ) -> WorkspaceSettings {
        WorkspaceSettings {
            workspace_id: Uuid::nil(),
            org_id: Uuid::nil(),
            max_spend_usd,
            spend_period: "monthly".to_string(),
            spend_period_start: None,
            max_input_tokens,
            max_output_tokens,
            idle_timeout_secs: 1800,
            response_deadline_secs: 900,
            bounded_execution_slots: DEFAULT_BOUNDED_EXECUTION_SLOTS,
            permission_policy: None,
            agent_may_modify_other_workspaces: true,
            updated_by: None,
            updated_at: Utc::now(),
        }
    }
}
