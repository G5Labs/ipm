//! Organization Home read-only routes.

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    routing::get,
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db::scoped::scoped_db_for_org;
use crate::errors::AppError;
use crate::middleware::AuthContext;
use crate::routes::audited::AuditedRouter;
use crate::routes::orgs::OrgPathParams;
use crate::routes::ResponseEnvelope;
use crate::services::home_service::{HomeAttentionItem, HomeAttentionKind, HomeAttentionOrder};
use crate::services::AppState;
use crate::services::{base_reverification, config_service};
use g5_auth::Scope;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HomeAttentionQuery {
    pub cursor: Option<String>,
    pub limit: Option<usize>,
    pub kind: Option<HomeAttentionKind>,
    /// `newest` or `oldest` (g5.ui.home-attention-order); absent reads
    /// oldest first, as before orders existed
    /// (g5.ui.home-attention-order-api-default).
    pub order: Option<HomeAttentionOrder>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HomeAttentionItemDTO {
    pub id: String,
    pub kind: HomeAttentionKind,
    pub workspace_id: Uuid,
    pub workspace_name: String,
    /// The owning workspace's owner, so each row shows the owner avatar the
    /// Workspaces tile does; absent only when no owner could be resolved.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_user_id: Option<Uuid>,
    pub source_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub open_question_count: Option<i64>,
    pub actionable_at: DateTime<Utc>,
}

impl From<HomeAttentionItem> for HomeAttentionItemDTO {
    fn from(item: HomeAttentionItem) -> Self {
        Self {
            id: item.id,
            kind: item.kind,
            workspace_id: item.workspace_id,
            workspace_name: item.workspace_name,
            owner_user_id: item.owner_user_id,
            source_id: item.source_id,
            display_name: item.display_name,
            open_question_count: item.open_question_count,
            actionable_at: item.actionable_at,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HomeAttentionResponseDTO {
    pub items: Vec<HomeAttentionItemDTO>,
    pub next_cursor: Option<String>,
}

/// Base's re-verification status for Home (g5.ui.home-reverification). Only
/// `enabled` while the organization's verification switch is off.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BaseReverificationStatusDTO {
    pub enabled: bool,
    #[serde(flatten)]
    pub status: Option<base_reverification::Status>,
    /// The caller may fork Base, so may start a run.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub can_start: Option<bool>,
}

pub fn home_router() -> Router<Arc<AppState>> {
    AuditedRouter::new()
        .audited_route("/attention", get(get_attention), &[Scope::WorkspacesRead])
        .audited_route(
            "/base-reverification",
            get(get_base_reverification),
            &[Scope::WorkspacesRead],
        )
        .into_router()
}

async fn get_base_reverification(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<OrgPathParams>,
) -> Result<Json<ResponseEnvelope<BaseReverificationStatusDTO>>, AppError> {
    let mut db = scoped_db_for_org(&state, &auth, params.org_id, &["workspaces:read"]).await?;
    let caller_user_id = auth.require_user_id()?;
    let disabled = BaseReverificationStatusDTO {
        enabled: false,
        status: None,
        can_start: None,
    };
    let enabled = config_service::org_boolean_setting(
        &mut db,
        params.org_id,
        config_service::VERIFICATION_ACTION_ENABLED_SETTING,
    )
    .await
    .map_err(AppError::from_error)?;
    let base = if enabled {
        base_reverification::base_workspace(db.conn(), params.org_id).await?
    } else {
        None
    };
    let Some((base_workspace_id, base_workspace_name)) = base else {
        db.commit().await.map_err(AppError::from)?;
        return Ok(Json(ResponseEnvelope::ok(disabled)));
    };
    let read = base_reverification::read_status(
        &mut db,
        &state.repo_storage,
        params.org_id,
        base_workspace_id,
        base_workspace_name,
    )
    .await?;
    // Lines are counted from Git after the scoped transaction ends.
    let scope = db.reopener();
    db.commit().await.map_err(AppError::from)?;
    let status = base_reverification::complete_status(read, &scope, &state.repo_storage).await;
    let can_start =
        base_reverification::can_start(&state.system_pool, params.org_id, caller_user_id).await?;
    Ok(Json(ResponseEnvelope::ok(BaseReverificationStatusDTO {
        enabled: true,
        status: Some(status),
        can_start: Some(can_start),
    })))
}

async fn get_attention(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<OrgPathParams>,
    Query(query): Query<HomeAttentionQuery>,
) -> Result<Json<ResponseEnvelope<HomeAttentionResponseDTO>>, AppError> {
    let mut db = scoped_db_for_org(&state, &auth, params.org_id, &["workspaces:read"]).await?;
    let caller_user_id = auth.require_user_id()?;
    let captured = state
        .home_service
        .capture_attention(
            &mut db,
            &state.system_pool,
            params.org_id,
            caller_user_id,
            query.cursor.as_deref(),
            query.limit,
            query.kind,
            query.order.unwrap_or_default(),
            &state.approval_config_service,
            &state.approval_definition_service,
            &state.workspace_service,
            &state.doc_service,
        )
        .await?;
    db.commit().await.map_err(AppError::from)?;

    let page = state
        .home_service
        .materialize_attention(
            captured,
            &state.workspace_service,
            &state.doc_service,
            &state.workspace_runtime,
        )
        .await?;
    Ok(Json(ResponseEnvelope::ok(HomeAttentionResponseDTO {
        items: page.items.into_iter().map(Into::into).collect(),
        next_cursor: page.next_cursor,
    })))
}
