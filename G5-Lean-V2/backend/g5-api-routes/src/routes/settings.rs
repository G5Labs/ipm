//! Workspace settings route handlers.
//!
//! This module implements the HTTP route handlers for the Workspace Settings API
//! (API_AND_STORAGE.md §3.9) and the Spend History API (API_AND_STORAGE.md §3.15).
//!
//! Endpoints (nested under `/api/v1/workspaces/:workspaceId/settings`):
//! - `GET /` — get workspace settings
//! - `PUT /` — update workspace settings (partial)
//! - `GET /spend` — get current spend vs. limits
//! - `GET /spend/history` — get spend history across multiple periods
//!
//! All handlers follow the thin handler pattern: extract request parameters,
//! delegate to the appropriate service (`WorkspaceService` or `SpendService`),
//! and format the response using `ResponseEnvelope`. No business logic resides
//! in the handlers.

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    routing::{get, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use rust_decimal::prelude::ToPrimitive;

use crate::db::scoped::scoped_db_for_workspace;
use crate::errors::AppError;
use crate::middleware::AuthContext;
use crate::models::WorkspaceSettings;
use crate::routes::audited::AuditedRouter;
use crate::routes::ResponseEnvelope;
use crate::services::spend_service;
use crate::services::AppState;
use g5_auth::Scope;

pub use g5_api_foundation::contracts::settings::UpdateWorkspaceSettingsRequest;

// ─── Path Parameter Types ────────────────────────────────────────────────────

/// Path parameters for workspace-scoped endpoints (`/api/v1/workspaces/:workspaceId/*`).
#[derive(Debug, Deserialize)]
pub struct WorkspacePathParams {
    #[serde(rename = "workspaceId")]
    pub workspace_id: Uuid,
}

// ─── Query Parameter Types ───────────────────────────────────────────────────

/// Default number of periods for spend history.
fn default_periods() -> i32 {
    6
}

/// Query parameters for `GET /spend/history`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpendHistoryQueryParams {
    /// Number of past periods to return. Default: 6.
    #[serde(default = "default_periods")]
    pub periods: i32,
    /// Granularity: "hourly", "daily", "weekly", "monthly".
    /// Default: auto (match spend_period from workspace settings).
    pub granularity: Option<String>,
}

// ─── Request DTOs ────────────────────────────────────────────────────────────

// ─── Response DTOs ───────────────────────────────────────────────────────────

/// The primary settings response DTO matching the TypeScript `WorkspaceSettingsDTO`
/// interface from API_AND_STORAGE.md §3.9.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceSettingsDTO {
    pub workspace_id: String,
    pub max_spend_usd: Option<f64>,
    pub spend_period: String,
    pub spend_period_start: Option<String>,
    pub max_input_tokens: Option<i64>,
    pub max_output_tokens: Option<i64>,
    pub idle_timeout_secs: i32,
    pub response_deadline_secs: i32,
    pub bounded_execution_slots: i32,
    pub permission_policy: Option<serde_json::Value>,
    pub agent_may_modify_other_workspaces: bool,
    pub updated_by: Option<String>,
    pub updated_at: String,
}

/// Spend summary route DTO wrapping the service-level `SpendSummaryDTO`.
/// Converts UUID fields to strings and Decimal fields to f64.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpendSummaryRouteDTO {
    pub period_start: String,
    pub period_end: String,
    pub total_spend_usd: f64,
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
    pub limit_spend_usd: Option<f64>,
    pub limit_input_tokens: Option<i64>,
    pub limit_output_tokens: Option<i64>,
    pub is_spend_limit_reached: bool,
    pub unattributed_spend_usd: f64,
    pub other_task_spend_usd: f64,
    pub by_task: Vec<TaskSpendBreakdownDTO>,
}

/// Per-task spend breakdown DTO.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskSpendBreakdownDTO {
    pub task_id: String,
    pub task_name: String,
    pub task_type: String,
    pub spend_usd: f64,
    pub input_tokens: i64,
    pub output_tokens: i64,
}

/// Spend history route DTO wrapping the service-level `SpendHistoryDTO`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpendHistoryRouteDTO {
    pub granularity: String,
    pub periods: Vec<SpendPeriodRouteDTO>,
    pub daily: Vec<DailySpendRouteDTO>,
}

/// A single period in the spend history.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpendPeriodRouteDTO {
    pub period_start: String,
    pub period_end: String,
    pub total_spend_usd: f64,
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
    pub task_count: i64,
}

/// Daily spend data point for charts.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DailySpendRouteDTO {
    pub date: String,
    pub spend_usd: f64,
    pub input_tokens: i64,
    pub output_tokens: i64,
}

impl From<WorkspaceSettings> for WorkspaceSettingsDTO {
    fn from(s: WorkspaceSettings) -> Self {
        WorkspaceSettingsDTO {
            workspace_id: s.workspace_id.to_string(),
            max_spend_usd: s.max_spend_usd.map(|d| {
                d.to_f64().unwrap_or_else(|| {
                    tracing::error!(decimal = %d, "Failed to convert max_spend_usd Decimal to f64");
                    0.0
                })
            }),
            spend_period: s.spend_period,
            spend_period_start: s.spend_period_start.map(|dt| dt.to_rfc3339()),
            max_input_tokens: s.max_input_tokens,
            max_output_tokens: s.max_output_tokens,
            idle_timeout_secs: s.idle_timeout_secs,
            response_deadline_secs: s.response_deadline_secs,
            bounded_execution_slots: s.bounded_execution_slots,
            permission_policy: s.permission_policy,
            agent_may_modify_other_workspaces: s.agent_may_modify_other_workspaces,
            updated_by: s.updated_by.map(|u| u.to_string()),
            updated_at: s.updated_at.to_rfc3339(),
        }
    }
}

impl From<spend_service::SpendSummaryDTO> for SpendSummaryRouteDTO {
    fn from(dto: spend_service::SpendSummaryDTO) -> Self {
        SpendSummaryRouteDTO {
            period_start: dto.period_start,
            period_end: dto.period_end,
            total_spend_usd: dto.total_spend_usd.to_f64().unwrap_or_else(|| {
                tracing::error!(decimal = %dto.total_spend_usd, "Failed to convert total_spend_usd Decimal to f64");
                0.0
            }),
            total_input_tokens: dto.total_input_tokens,
            total_output_tokens: dto.total_output_tokens,
            limit_spend_usd: dto.limit_spend_usd.map(|d| d.to_f64().unwrap_or_else(|| {
                tracing::error!(decimal = %d, "Failed to convert limit_spend_usd Decimal to f64");
                0.0
            })),
            limit_input_tokens: dto.limit_input_tokens,
            limit_output_tokens: dto.limit_output_tokens,
            is_spend_limit_reached: dto.is_spend_limit_reached,
            unattributed_spend_usd: dto.unattributed_spend_usd.to_f64().unwrap_or_default(),
            other_task_spend_usd: dto.other_task_spend_usd.to_f64().unwrap_or_default(),
            by_task: dto
                .by_task
                .into_iter()
                .map(TaskSpendBreakdownDTO::from)
                .collect(),
        }
    }
}

impl From<spend_service::TaskSpendBreakdown> for TaskSpendBreakdownDTO {
    fn from(dto: spend_service::TaskSpendBreakdown) -> Self {
        TaskSpendBreakdownDTO {
            task_id: dto.task_id.to_string(),
            task_name: dto.task_name,
            task_type: dto.task_type,
            spend_usd: dto.spend_usd.to_f64().unwrap_or_else(|| {
                tracing::error!(decimal = %dto.spend_usd, "Failed to convert spend_usd Decimal to f64");
                0.0
            }),
            input_tokens: dto.input_tokens,
            output_tokens: dto.output_tokens,
        }
    }
}

impl From<spend_service::SpendHistoryDTO> for SpendHistoryRouteDTO {
    fn from(dto: spend_service::SpendHistoryDTO) -> Self {
        SpendHistoryRouteDTO {
            granularity: dto.granularity,
            periods: dto
                .periods
                .into_iter()
                .map(SpendPeriodRouteDTO::from)
                .collect(),
            daily: dto
                .daily
                .into_iter()
                .map(DailySpendRouteDTO::from)
                .collect(),
        }
    }
}

impl From<spend_service::SpendPeriodDTO> for SpendPeriodRouteDTO {
    fn from(dto: spend_service::SpendPeriodDTO) -> Self {
        SpendPeriodRouteDTO {
            period_start: dto.period_start,
            period_end: dto.period_end,
            total_spend_usd: dto.total_spend_usd.to_f64().unwrap_or_else(|| {
                tracing::error!(decimal = %dto.total_spend_usd, "Failed to convert period total_spend_usd Decimal to f64");
                0.0
            }),
            total_input_tokens: dto.total_input_tokens,
            total_output_tokens: dto.total_output_tokens,
            task_count: dto.task_count,
        }
    }
}

impl From<spend_service::DailySpendDTO> for DailySpendRouteDTO {
    fn from(dto: spend_service::DailySpendDTO) -> Self {
        DailySpendRouteDTO {
            date: dto.date,
            spend_usd: dto.spend_usd.to_f64().unwrap_or_else(|| {
                tracing::error!(decimal = %dto.spend_usd, "Failed to convert daily spend_usd Decimal to f64");
                0.0
            }),
            input_tokens: dto.input_tokens,
            output_tokens: dto.output_tokens,
        }
    }
}

// ─── Router ────────────────────────────────────────────────────────────────────────────

/// Settings routes nested under `/api/v1/workspaces/:workspaceId/settings`.
///
/// Endpoints:
/// - `GET /` — get workspace settings
/// - `PUT /` — update workspace settings (partial)
/// - `GET /spend` — get current spend vs. limits
/// - `GET /spend/history` — get spend history across multiple periods
pub fn router() -> Router<Arc<AppState>> {
    AuditedRouter::new()
        .audited_route("/", get(get_settings), &[Scope::WorkspacesRead])
        .audited_route("/", put(update_settings), &[Scope::WorkspacesWrite])
        .audited_route("/spend", get(get_spend_summary), &[Scope::BillingAdmin])
        .audited_route(
            "/spend/history",
            get(get_spend_history),
            &[Scope::BillingAdmin],
        )
        .into_router()
}

// ─── Handlers ────────────────────────────────────────────────────────────────

/// `GET /api/v1/workspaces/:workspaceId/settings`
///
/// Retrieves the current workspace settings. Used by the SettingsPage to
/// populate the settings form.
async fn get_settings(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
) -> Result<Json<ResponseEnvelope<WorkspaceSettingsDTO>>, AppError> {
    let mut db =
        scoped_db_for_workspace(&state, &auth, params.workspace_id, &["workspaces:read"]).await?;
    tracing::debug!(
        workspace_id = %params.workspace_id,
        "Settings endpoint called: get_settings"
    );
    let settings = state
        .workspace_service
        .get_workspace_settings(&mut db, params.workspace_id)
        .await?;
    let dto: WorkspaceSettingsDTO = settings.into();
    db.commit().await.map_err(AppError::from)?;

    Ok(Json(ResponseEnvelope::ok(dto)))
}

/// `PUT /api/v1/workspaces/:workspaceId/settings`
///
/// Partially updates workspace settings. Only provided fields are updated;
/// omitted fields retain their current values. Changes take effect immediately —
/// the scheduler reads settings on every tick.
async fn update_settings(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
    Json(body): Json<UpdateWorkspaceSettingsRequest>,
) -> Result<Json<ResponseEnvelope<WorkspaceSettingsDTO>>, AppError> {
    let mut db =
        scoped_db_for_workspace(&state, &auth, params.workspace_id, &["workspaces:write"]).await?;
    crate::services::workspace_mutation_policy::ensure_workspace_owner(
        &mut db,
        params.workspace_id,
    )
    .await?;
    tracing::debug!(
        workspace_id = %params.workspace_id,
        "Settings endpoint called: update_settings"
    );

    let user_id = auth.require_user_id()?;

    let updated = state
        .workspace_service
        .update_workspace_settings(&mut db, params.workspace_id, user_id, body)
        .await?;
    let dto: WorkspaceSettingsDTO = updated.into();
    tracing::info!(
        workspace_id = %params.workspace_id,
        "Settings endpoint succeeded: update_settings"
    );
    db.commit().await.map_err(AppError::from)?;

    Ok(Json(ResponseEnvelope::ok(dto)))
}

/// `GET /api/v1/workspaces/:workspaceId/settings/spend`
///
/// Retrieves the current spend vs. limits for the workspace, including a
/// per-task breakdown. Powers the spend summary section of the SettingsPage.
async fn get_spend_summary(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
) -> Result<Json<ResponseEnvelope<SpendSummaryRouteDTO>>, AppError> {
    tracing::debug!(
        workspace_id = %params.workspace_id,
        "Settings endpoint called: get_spend_summary"
    );

    let mut db =
        scoped_db_for_workspace(&state, &auth, params.workspace_id, &["billing:admin"]).await?;
    let summary = state
        .spend_service
        .get_spend_summary(&mut db, params.workspace_id)
        .await?;
    let dto: SpendSummaryRouteDTO = summary.into();
    Ok(Json(ResponseEnvelope::ok(dto)))
}

/// `GET /api/v1/workspaces/:workspaceId/settings/spend/history`
///
/// Retrieves historical spend data across multiple past periods with
/// configurable granularity. Powers the spend history chart on the SettingsPage.
async fn get_spend_history(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
    Query(query): Query<SpendHistoryQueryParams>,
) -> Result<Json<ResponseEnvelope<SpendHistoryRouteDTO>>, AppError> {
    tracing::debug!(
        workspace_id = %params.workspace_id,
        periods = query.periods,
        "Settings endpoint called: get_spend_history"
    );

    let mut db =
        scoped_db_for_workspace(&state, &auth, params.workspace_id, &["billing:admin"]).await?;
    let history = state
        .spend_service
        .get_spend_history(
            &mut db,
            params.workspace_id,
            query.periods,
            query.granularity,
        )
        .await?;
    let dto: SpendHistoryRouteDTO = history.into();
    Ok(Json(ResponseEnvelope::ok(dto)))
}
