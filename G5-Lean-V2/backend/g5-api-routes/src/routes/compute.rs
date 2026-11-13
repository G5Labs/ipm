//! Organization running-compute read for members (workbench Phase 6).
//!
//! `GET /api/v1/orgs/:orgId/compute/running` — the Workers-page VM inventory
//! (`worker_hosts.rs::list_host_workspaces`, same shared query) exposed under
//! `workspaces:read` and filtered to the workspaces the caller may read: every
//! ordinary workspace, plus personal workspaces for their owner or an org
//! administrator — the
//! workspace list's own predicate (security tenant isolation; a member read
//! never names a workspace the list would hide). The response never branches
//! on the UI layout (g5.workbench.gate): the Home tile decides visibility.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    routing::get,
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::db::scoped::scoped_db_for_org;
use crate::errors::AppError;
use crate::middleware::AuthContext;
use crate::routes::audited::AuditedRouter;
use crate::routes::orgs::OrgPathParams;
use crate::routes::ResponseEnvelope;
use crate::services::worker_host_service::{
    list_active_workspace_vms, VmInventoryVisibility, WorkspaceVmInventoryRow,
};
use crate::services::AppState;
use g5_auth::Scope;

/// One running (non-stopped) workspace VM the caller may see.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunningComputeDTO {
    pub workspace_id: Uuid,
    pub workspace_name: String,
    pub agent_review: Option<crate::services::worker_host_service::AgentReviewInventoryLink>,
    /// `starting`, `running`, `stopping` or `failed` (stopped VMs are excluded).
    pub status: String,
    pub started_at: Option<DateTime<Utc>>,
    pub last_activity_at: Option<DateTime<Utc>>,
    pub owner_user_id: Option<Uuid>,
    pub owner_display_name: Option<String>,
    pub host_id: Option<Uuid>,
    /// Slot size of the placed host; `null` before placement.
    pub slot_cpus: Option<i32>,
    pub slot_mem_mib: Option<i32>,
    pub instance_type: Option<String>,
}

impl From<WorkspaceVmInventoryRow> for RunningComputeDTO {
    fn from(row: WorkspaceVmInventoryRow) -> Self {
        Self {
            workspace_id: row.workspace_id,
            workspace_name: row.workspace_name,
            agent_review: row.agent_review,
            status: row.status,
            started_at: row.started_at,
            last_activity_at: row.last_activity_at,
            owner_user_id: row.owner_user_id,
            owner_display_name: row.owner_display_name,
            host_id: row.host_id,
            slot_cpus: row.slot_cpus,
            slot_mem_mib: row.slot_mem_mib,
            instance_type: row.instance_type,
        }
    }
}

pub fn router() -> Router<Arc<AppState>> {
    AuditedRouter::new()
        .audited_route(
            "/running",
            get(list_running_compute),
            &[Scope::WorkspacesRead],
        )
        .into_router()
}

async fn list_running_compute(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<OrgPathParams>,
) -> Result<Json<ResponseEnvelope<Vec<RunningComputeDTO>>>, AppError> {
    let mut db = scoped_db_for_org(&state, &auth, params.org_id, &["workspaces:read"]).await?;
    let caller_user_id = auth.require_user_id()?;
    // Org admins see every workspace, as in the workspace list;
    // the service reads the role only after the org-scoped authorization
    // above succeeded.
    let visibility =
        VmInventoryVisibility::for_member(&state.system_pool, params.org_id, caller_user_id)
            .await?;
    let rows = list_active_workspace_vms(
        &mut db,
        &state.system_pool,
        state.authz.as_ref(),
        params.org_id,
        visibility,
    )
    .await?;
    db.commit().await.map_err(AppError::from)?;
    Ok(Json(ResponseEnvelope::ok(
        rows.into_iter().map(RunningComputeDTO::from).collect(),
    )))
}
