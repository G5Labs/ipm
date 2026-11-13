//! Worker-host provisioning route handlers (PR-rev-7-9b + PR-rev-7-9c).
//!
//! Endpoints (all org-scoped, `Scope::WorkersAdmin`):
//! - `POST   /api/v1/orgs/:orgId/worker-hosts`           — provision a host record
//! - `GET    /api/v1/orgs/:orgId/worker-hosts`           — list host records
//! - `DELETE /api/v1/orgs/:orgId/worker-hosts/:hostId`   — deallocate a host record
//!
//! PR-rev-7-9b implemented the DB-only control-plane surface. PR-rev-7-9c wires
//! the POST/DELETE handlers to a [`CloudProvider`] for GCP cloud bring-up/teardown
//! (`vpc_id`/`cloud_instance_id` population, instance insert/delete) — but ONLY
//! when a cloud backend is wired (`state.cloud_provider == Some`). When `None`
//! (dev/test, or GCP unconfigured) the handlers behave exactly as 9b: no cloud
//! call, `vpc_id`/`cloud_instance_id` stay NULL. Cloud bring-up runs AFTER the
//! INSERT commits so a GCP failure can never roll back the host record (the
//! cloud-call-after-db-commit failure mode, [D21]/[D22]).
//!
//! Contract: `generation/.semlift/specs/g5/g5-api/worker_hosts.semlift.md`
//! [D9]–[D15], [D22], [D23], [I6], [I7]. The handlers mirror the sibling
//! `routes/worker_tokens.rs` org-admin pattern (AuditedRouter + scoped_db_for_org).

use std::sync::Arc;
use std::time::Duration;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{delete, get, post, put},
    Json, Router,
};
use chrono::{DateTime, TimeDelta, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db::scoped::scoped_db_for_org;
use crate::errors::AppError;
use crate::middleware::AuthContext;
use crate::routes::audited::AuditedRouter;
use crate::routes::ResponseEnvelope;
use crate::services::runtime_label_resolver::RuntimeLabelError;
use crate::services::worker_host_service::{
    list_active_workspace_vms, parse_promoted_base_images, DrainRequest, VmInventoryVisibility,
    WorkspaceVmInventoryRow,
};
use crate::services::AppState;
use g5_auth::Scope;
use g5_contracts::error::G5Error;
use g5_contracts::types::worker_host::{
    validate_base_image_ref, DrainWorkerHostRequest, DrainWorkerHostResponse,
    ListWorkerHostsResponse, ProvisionWorkerHostRequest, ProvisionWorkerHostResponse,
    UpdateWorkerImagePolicyRequest, WorkerHost, WorkerHostStatus, WorkerImagePolicyResponse,
    WorkerImageTrack,
};
use g5_internal_api::host_ctl::CommandResult;

// ─── Path / Query Parameter Types ──────────────────────────────────────────

/// Path parameters for the org-collection endpoints
/// (`/api/v1/orgs/:orgId/worker-hosts`).
#[derive(Debug, Deserialize)]
pub struct OrgPathParams {
    #[serde(rename = "orgId")]
    pub org_id: Uuid,
}

/// Path parameters for the single-host endpoint
/// (`/api/v1/orgs/:orgId/worker-hosts/:hostId`).
#[derive(Debug, Deserialize)]
pub struct HostPathParams {
    #[serde(rename = "orgId")]
    pub org_id: Uuid,
    #[serde(rename = "hostId")]
    pub host_id: Uuid,
}

/// Current workspace VM assignments for the organization-wide Workers page.
/// Projection of the shared inventory row (`list_active_workspace_vms`); the
/// member-visible compute read (`routes/compute.rs`) carries the slot size
/// instead, so this wire shape stays as it was.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkerHostWorkspaceView {
    workspace_id: Uuid,
    workspace_name: String,
    agent_review: Option<crate::services::worker_host_service::AgentReviewInventoryLink>,
    host_id: Option<Uuid>,
    status: String,
    started_at: Option<DateTime<Utc>>,
    last_activity_at: Option<DateTime<Utc>>,
    owner_user_id: Option<Uuid>,
    owner_display_name: Option<String>,
    owner_email: Option<String>,
}

impl From<WorkspaceVmInventoryRow> for WorkerHostWorkspaceView {
    fn from(row: WorkspaceVmInventoryRow) -> Self {
        Self {
            workspace_id: row.workspace_id,
            workspace_name: row.workspace_name,
            agent_review: row.agent_review,
            host_id: row.host_id,
            status: row.status,
            started_at: row.started_at,
            last_activity_at: row.last_activity_at,
            owner_user_id: row.owner_user_id,
            owner_display_name: row.owner_display_name,
            owner_email: row.owner_email,
        }
    }
}

/// Query parameters for `GET …/worker-hosts` (ontology [D12]).
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListWorkerHostsQuery {
    /// Optional status filter. Absent → all statuses.
    #[serde(default)]
    pub status: Option<WorkerHostStatus>,
    /// Exclude terminated audit records before counting and paginating.
    #[serde(default)]
    pub active_only: bool,
    /// Page size; default 50, clamped to `1..=100`.
    #[serde(default)]
    pub limit: Option<i64>,
    /// Row offset; default 0, negatives coerced to 0.
    #[serde(default)]
    pub offset: Option<i64>,
}

#[derive(Debug, Default, Deserialize)]
pub struct DeleteWorkerHostQuery {
    #[serde(default)]
    pub force: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CapacityStopRequest {
    workspace_id: Uuid,
    operation_id: Uuid,
    #[serde(default)]
    deadline_at: Option<DateTime<Utc>>,
}

impl ListWorkerHostsQuery {
    const DEFAULT_LIMIT: i64 = 50;
    const MAX_LIMIT: i64 = 100;

    /// Validated page size (default 50, clamped to `1..=100`).
    fn limit_clamped(&self) -> i64 {
        self.limit
            .unwrap_or(Self::DEFAULT_LIMIT)
            .clamp(1, Self::MAX_LIMIT)
    }

    /// Validated offset (default 0, non-negative).
    fn offset_clamped(&self) -> i64 {
        self.offset.unwrap_or(0).max(0)
    }
}

// ─── Router ────────────────────────────────────────────────────────────────

/// Worker-host provisioning routes, nested under
/// `/api/v1/orgs/:orgId/worker-hosts`.
pub fn router() -> Router<Arc<AppState>> {
    AuditedRouter::new()
        .audited_route("/", post(provision_worker_host), &[Scope::WorkersAdmin])
        .audited_route("/", get(list_worker_hosts), &[Scope::WorkersAdmin])
        .audited_route(
            "/workspaces",
            get(list_host_workspaces),
            &[Scope::WorkersAdmin],
        )
        .audited_route(
            "/capacity-stop",
            post(capacity_stop_workspace),
            &[Scope::WorkersAdmin],
        )
        .audited_route(
            "/image-policy",
            put(update_worker_image_policy),
            &[Scope::WorkersAdmin],
        )
        .audited_route(
            "/:hostId",
            delete(deallocate_worker_host),
            &[Scope::WorkersAdmin],
        )
        .audited_route(
            "/:hostId/drain",
            post(drain_worker_host),
            &[Scope::WorkersAdmin],
        )
        .audited_route(
            "/:hostId/runtime-labels/refresh",
            post(refresh_worker_host_runtime_labels),
            &[Scope::WorkersAdmin],
        )
        .into_router()
}

async fn list_host_workspaces(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<OrgPathParams>,
) -> Result<Json<ResponseEnvelope<Vec<WorkerHostWorkspaceView>>>, AppError> {
    let mut db = scoped_db_for_org(&state, &auth, params.org_id, &["workers:admin"]).await?;
    let workspaces = list_active_workspace_vms(
        &mut db,
        &state.system_pool,
        state.authz.as_ref(),
        params.org_id,
        VmInventoryVisibility::OrgWide,
    )
    .await?
    .into_iter()
    .map(WorkerHostWorkspaceView::from)
    .collect();
    Ok(Json(ResponseEnvelope::ok(workspaces)))
}

async fn capacity_stop_workspace(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<OrgPathParams>,
    Json(body): Json<CapacityStopRequest>,
) -> Result<Json<ResponseEnvelope<crate::services::workspace_vm_service::StopOutcome>>, AppError> {
    let mut db = scoped_db_for_org(&state, &auth, params.org_id, &["workers:admin"]).await?;
    let belongs_to_org = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM workspaces WHERE id = $1 AND org_id = $2 AND deleted_at IS NULL)",
    )
    .bind(body.workspace_id)
    .bind(params.org_id)
    .fetch_one(db.conn())
    .await?;
    if !belongs_to_org {
        return Err(AppError::from_error(G5Error::NotFound {
            resource: format!("workspace {}", body.workspace_id),
        }));
    }
    let scope = db.reopener();
    db.commit().await.map_err(AppError::from)?;
    let minter = state.jwt_minter.clone().ok_or_else(|| {
        AppError::from_error(G5Error::ServiceUnavailable {
            message: "workspace lifecycle signing is unavailable".to_string(),
        })
    })?;
    let service =
        crate::services::WorkspaceVmService::new(crate::services::WorkspaceVmServiceConfig {
            system_pool: state.system_pool.clone(),
            scope,
            hosts: state.preview_tunnels.clone(),
            minter,
            runtime: state.workspace_runtime.clone(),
            agent_work: state.agent_work.clone(),
            repo_storage: state.repo_storage.clone(),
            git_external_url: state.git_external_url.clone(),
            ws_hub: state.event_bus.ws_hub().clone(),
        });
    let outcome = service
        .stop_for_capacity(
            body.workspace_id,
            body.operation_id,
            body.deadline_at
                .unwrap_or_else(|| Utc::now() + TimeDelta::minutes(5)),
        )
        .await?;
    Ok(Json(ResponseEnvelope::ok(outcome)))
}

/// Record an idempotent host drain before attempting delivery over the live
/// control tunnel. A missing tunnel or ambiguous timeout leaves the durable
/// request pending for the host reconciler; it is never treated as refusal.
async fn drain_worker_host(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<HostPathParams>,
    Json(body): Json<DrainWorkerHostRequest>,
) -> Result<Json<ResponseEnvelope<DrainWorkerHostResponse>>, AppError> {
    let actor_user_id = auth.require_user_id()?;
    let reason = body
        .reason
        .as_deref()
        .map(str::trim)
        .filter(|reason| !reason.is_empty())
        .unwrap_or("operator");
    if reason.len() > 256 {
        return Err(AppError::from_error(G5Error::ValidationError {
            details: serde_json::json!({
                "field": "reason",
                "error": "drain reason must be at most 256 bytes",
            }),
        }));
    }
    let proposed_drain_id = Uuid::new_v4();
    let mut db = scoped_db_for_org(&state, &auth, params.org_id, &["workers:admin"]).await?;
    let drain = match state
        .worker_host_service
        .request_drain(
            &mut db,
            params.org_id,
            params.host_id,
            proposed_drain_id,
            reason,
        )
        .await?
    {
        DrainRequest::Recorded(drain) => drain,
        refused => {
            db.rollback().await.map_err(AppError::from)?;
            return Err(AppError::from_error(match refused {
                DrainRequest::RotationOwned => G5Error::Conflict {
                    reason: format!("worker host {} is draining for rotation", params.host_id),
                },
                _ => G5Error::NotFound {
                    resource: format!("active worker host {}", params.host_id),
                },
            }));
        }
    };
    db.commit().await.map_err(AppError::from)?;

    deliver_host_drain(&state, params.host_id, &drain).await;

    tracing::info!(
        target: "g5_audit",
        event = "worker_host.drain_requested",
        host_id = %params.host_id,
        org_id = %params.org_id,
        drain_id = %drain.drain_id(),
        active_vms = drain.active_vms,
        already_draining = drain.already_draining,
        actor_user_id = %actor_user_id,
        reason,
        "Worker host drain recorded"
    );
    Ok(Json(ResponseEnvelope::ok(DrainWorkerHostResponse {
        host_id: params.host_id,
        drain_id: drain.drain_id(),
        active_vms: drain.active_vms,
        already_draining: drain.already_draining,
    })))
}

async fn deliver_host_drain(
    state: &Arc<AppState>,
    host_id: Uuid,
    drain: &crate::services::worker_host_service::HostDrainRecord,
) {
    if let Some(delivery) = state
        .worker_host_service
        .deliver_drain(&state.preview_tunnels, drain, Duration::from_secs(2), 2)
        .await
    {
        match delivery {
            Ok(CommandResult::Accepted) => {}
            Ok(result) => tracing::warn!(
                target: "g5_audit",
                event = "worker_host.drain_delivery_deferred",
                %host_id,
                drain_id = %drain.drain_id(),
                result = ?result,
                "Host returned a retryable drain delivery result"
            ),
            Err(error) => tracing::warn!(
                target: "g5_audit",
                event = "worker_host.drain_delivery_ambiguous",
                %host_id,
                drain_id = %drain.drain_id(),
                %error,
                "Drain remains durable after ambiguous control-tunnel delivery"
            ),
        }
    }
}

/// `PUT /api/v1/orgs/:orgId/worker-hosts/image-policy` — set the default
/// worker-image source used by future hosts. Resolution and validation finish
/// before the single policy write; historical host provenance is untouched.
async fn update_worker_image_policy(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<OrgPathParams>,
    Json(body): Json<UpdateWorkerImagePolicyRequest>,
) -> Result<Json<ResponseEnvelope<WorkerImagePolicyResponse>>, AppError> {
    let requested_track = body
        .image_track
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let requested_pin = body
        .base_image_ref
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if requested_track.is_some() == requested_pin.is_some() {
        return Err(AppError::from_error(G5Error::ValidationError {
            details: serde_json::json!({
                "field": "imageSource",
                "error": "exactly one of imageTrack and baseImageRef is required",
            }),
        }));
    }

    // Authorization precedes catalog access and the system-pool write.
    let _db = scoped_db_for_org(&state, &auth, params.org_id, &["workers:admin"]).await?;
    let (source_kind, source_value, image_ref, digest) = if let Some(track) = requested_track {
        let catalog = std::env::var("G5_PROMOTED_BASE_IMAGES")
            .ok()
            .map(|raw| parse_promoted_base_images(&raw))
            .unwrap_or_default();
        let image_ref = catalog
            .into_iter()
            .find_map(|(name, image_ref)| (name == track).then_some(image_ref))
            .ok_or_else(|| {
                AppError::from_error(G5Error::ValidationError {
                    details: serde_json::json!({
                        "field": "imageTrack",
                        "error": "selected promoted image track is absent from the current catalog",
                        "value": track,
                    }),
                })
            })?;
        let digest = validate_base_image_ref(&image_ref).map_err(|error| {
            AppError::from_error(G5Error::Internal {
                message: format!("validated promoted catalog drifted: {}", error.as_str()),
            })
        })?;
        ("track", track, image_ref, digest)
    } else {
        let pin = requested_pin.ok_or_else(|| {
            AppError::from_error(G5Error::Internal {
                message: "validated image policy lost its required pin".to_string(),
            })
        })?;
        let digest = validate_base_image_ref(pin).map_err(|error| {
            AppError::from_error(G5Error::ValidationError {
                details: serde_json::json!({
                    "field": "baseImageRef", "error": error.as_str(), "value": pin,
                }),
            })
        })?;
        ("pin", pin, pin.to_string(), digest)
    };

    state
        .worker_host_service
        .set_org_image_policy(
            &state.system_pool,
            params.org_id,
            (source_kind == "track").then_some(source_value),
            &image_ref,
        )
        .await?;

    Ok(Json(ResponseEnvelope::ok(WorkerImagePolicyResponse {
        source_kind: source_kind.to_string(),
        source_value: source_value.to_string(),
        resolved_image_ref: image_ref,
        resolved_digest: digest,
    })))
}

// ─── Handlers ──────────────────────────────────────────────────────────────

/// `POST /api/v1/orgs/:orgId/worker-hosts` — provision a worker-host record.
///
/// Inserts a `worker_hosts` row at `status='provisioning'` (no cloud bring-up;
/// `cloud_instance_id` stays NULL) and mints a per-host `host_jwt`. Returns
/// `201 Created`. Ontology [D10], [I6].
async fn provision_worker_host(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<OrgPathParams>,
    Json(body): Json<ProvisionWorkerHostRequest>,
) -> Result<
    (
        StatusCode,
        Json<ResponseEnvelope<ProvisionWorkerHostResponse>>,
    ),
    AppError,
> {
    let user_id = auth.require_user_id()?;
    let mut db = scoped_db_for_org(&state, &auth, params.org_id, &["workers:admin"]).await?;
    let prepared = state
        .worker_host_service
        .prepare_host(&state, &mut db, params.org_id, Some(user_id), body)
        .await?;
    let recent = state
        .worker_host_service
        .count_recent_hosts(&mut db, params.org_id)
        .await
        .ok();
    db.commit().await?;
    tracing::info!(target: "g5_audit", event = "worker_host.provisioned",
        host_id = %prepared.host.host_id, org_id = %params.org_id, actor_user_id = %user_id,
        host_count_in_org_last_hour = recent.unwrap_or(-1), "Provisioned worker host record");
    prepared.launch(state, user_id)?;
    Ok((
        StatusCode::CREATED,
        Json(ResponseEnvelope::ok(prepared.into_response()?)),
    ))
}

/// `GET /api/v1/orgs/:orgId/worker-hosts` — list worker-host records.
///
/// Admin-only in V1. Supports an optional `status` filter and `limit`/`offset`
/// paging; results are ordered by `created_at DESC, host_id DESC` (the
/// `host_id` tie-breaker keeps offset-based pagination deterministic when
/// multiple rows share `created_at`). Ontology [D12].
async fn list_worker_hosts(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<OrgPathParams>,
    Query(query): Query<ListWorkerHostsQuery>,
) -> Result<Json<ResponseEnvelope<ListWorkerHostsResponse>>, AppError> {
    let mut db = scoped_db_for_org(&state, &auth, params.org_id, &["workers:admin"]).await?;

    let limit = query.limit_clamped();
    let offset = query.offset_clamped();

    let (mut hosts, total) = state
        .worker_host_service
        .list_hosts_filtered(
            &mut db,
            params.org_id,
            query.status,
            limit,
            offset,
            query.active_only,
        )
        .await?;

    state
        .worker_host_service
        .attach_creator_profiles(&state.system_pool, &mut hosts)
        .await?;

    // The org's OWN default base image rides the envelope for the dialog
    // prefill (ontology [D51]) — resolved post-authorization on the system
    // pool; never a cross-org catalog ([D50]).
    let (org_default_base_image_track, org_default_base_image_ref) = state
        .worker_host_service
        .org_image_policy(&state.system_pool, params.org_id)
        .await?;
    let promoted_image_tracks = std::env::var("G5_PROMOTED_BASE_IMAGES")
        .ok()
        .map(|raw| parse_promoted_base_images(&raw))
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(name, image_ref)| {
            validate_base_image_ref(&image_ref)
                .ok()
                .map(|digest| WorkerImageTrack {
                    name,
                    image_ref,
                    digest,
                })
        })
        .collect();

    // Read-only path — no `db.commit()`. The scoped capability is released
    // when `db` drops at the end of the handler; nothing here needs durability.

    // Publish the boot-disk sizing policy used by provisioning, so the Add
    // worker host dialog shows the server default without its own constants
    // (ontology `g5.runtime.placement.host-disk`).
    let boot_disk_sizing =
        crate::services::org_admin_settings::disk_policy(db.conn(), params.org_id).await?;

    Ok(Json(ResponseEnvelope::ok(ListWorkerHostsResponse {
        hosts,
        total,
        org_default_base_image_ref,
        org_default_base_image_track,
        promoted_image_tracks,
        boot_disk_sizing,
    })))
}

/// `DELETE /api/v1/orgs/:orgId/worker-hosts/:hostId` — deallocate a host record.
///
/// Transitions the row to `terminated` and clears `last_heartbeat`. The row is
/// NOT deleted (audit trail); cloud teardown is best-effort when the row has a
/// `cloud_instance_id` and a cloud backend is wired. Returns `204 No Content`;
/// `404` if no such host is visible in the org. Idempotent on an
/// already-terminated host. Ontology [D9], [D11], [D23], [I7].
async fn deallocate_worker_host(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<HostPathParams>,
    Query(query): Query<DeleteWorkerHostQuery>,
) -> Result<StatusCode, AppError> {
    let user_id = auth.require_user_id()?;
    let mut db = scoped_db_for_org(&state, &auth, params.org_id, &["workers:admin"]).await?;
    // Custody check, drain, forced losses and termination commit together.
    let Some(removal) = state
        .worker_host_service
        .remove_host(
            state.agent_work.as_ref(),
            &mut db,
            params.org_id,
            params.host_id,
            user_id,
            query.force,
        )
        .await?
    else {
        db.rollback().await.map_err(AppError::from)?;
        // Defense-in-depth observability: a 404 is most often benign (stale
        // tooling re-running DELETE on a host it already terminated), but
        // bursts on unknown host IDs can also indicate enumeration probes.
        // Emit at warn level so log analysis can detect either pattern;
        // ontology [D13] makes this an observability event, not a durable
        // audit row.
        tracing::warn!(
            target: "g5_audit",
            event = "worker_host.deallocate_not_found",
            host_id = %params.host_id,
            org_id = %params.org_id,
            actor_user_id = %user_id,
            "Deallocate request targeted unknown or out-of-scope host"
        );
        return Err(AppError::from_error(G5Error::NotFound {
            resource: format!("worker host {}", params.host_id),
        }));
    };
    db.commit().await.map_err(AppError::from)?;
    if let Some(drain) = removal.drain.as_ref() {
        deliver_host_drain(&state, params.host_id, drain).await;
    }
    state.preview_tunnels.terminate(&params.host_id).await;

    tracing::info!(
        target: "g5_audit",
        event = "worker_host.deallocated",
        host_id = %params.host_id,
        org_id = %params.org_id,
        actor_user_id = %user_id,
        force = query.force,
        forced_vm_losses = removal.forced_vm_losses,
        "Deallocated worker host record (status=terminated)"
    );

    // ── Cloud teardown (PR-rev-7-9c) — best-effort, gated ──
    // The row is already `terminated` from the user's perspective; a GCP
    // teardown failure is logged but never fails the DELETE (terminate-best-
    // effort, ontology [D21]/[D23]).
    //
    // DELETE honors the same `cloud_features_enabled` gate as POST so an
    // operator can flip cloud bring-up off mid-flight without DELETE still
    // issuing live GCP calls (cross-review codex+opus47 important; ontology
    // [D23]). When the gate is OFF but the row carries a backing instance
    // (created while the gate was ON), we log a `cloud_orphan_at_delete`
    // event so an operator can manually reap it — matching the
    // POST-side `cloud_orphan` pattern. The reconciler does NOT reap these
    // either when the gate is off, so the audit event is the only signal.
    let cloud_instance_id = removal.cloud_instance_id;
    match (
        state.cloud_features_enabled,
        state.cloud_provider.clone(),
        cloud_instance_id,
    ) {
        (true, Some(cloud), Some(instance_id)) => {
            if let Err(e) = cloud.terminate_instance(&instance_id).await {
                tracing::error!(
                    target: "g5_audit",
                    event = "worker_host.cloud_teardown_failed",
                    host_id = %params.host_id,
                    org_id = %params.org_id,
                    cloud_instance_id = %instance_id,
                    error = %e,
                    "Cloud instance teardown failed; row is already terminated, leaving orphan for stale-host detector"
                );
            }
        }
        (false, _, Some(instance_id)) => {
            tracing::warn!(
                target: "g5_audit",
                event = "worker_host.cloud_orphan_at_delete",
                host_id = %params.host_id,
                org_id = %params.org_id,
                cloud_instance_id = %instance_id,
                "Skipped cloud teardown (cloud_features_enabled=false); GCE instance \
                 may still be alive — operator must reap manually"
            );
        }
        // No backing instance, OR (gate on, provider None): nothing to tear
        // down. The gate-on-but-no-provider case already 503-ed at POST
        // (Fix 2), so a row with cloud_instance_id and no provider should
        // not occur in practice; if it does, the orphan log under
        // (false, _, Some) covers it.
        _ => {}
    }

    Ok(StatusCode::NO_CONTENT)
}

/// `POST /api/v1/orgs/:orgId/worker-hosts/:hostId/runtime-labels/refresh` —
/// resolve and persist the host's OCI runtime labels (PR2, ontology [D43]).
///
/// Explicit-refresh-only: this is the sole trigger for populating
/// `runtime_schema`/`runtime_labels`/`runtime_labels_refreshed_at`; provisioning
/// is never blocked on a label fetch. The host lookup and final update each use
/// their own short org-scoped transaction (`scoped_db_for_org`, RLS); the
/// resolver runs outside any DB transaction so a slow registry cannot pin a pool
/// connection. No SECURITY DEFINER function, no system-pool write. On any
/// resolver failure the DB is left untouched (the endpoint never returns `200`
/// with `{}` on failure). Returns the updated [`WorkerHost`] DTO. No request
/// body. Label values and registry tokens are NEVER logged.
async fn refresh_worker_host_runtime_labels(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<HostPathParams>,
) -> Result<Json<ResponseEnvelope<WorkerHost>>, AppError> {
    let user_id = auth.require_user_id()?;

    let base_image_ref = {
        let mut db = scoped_db_for_org(&state, &auth, params.org_id, &["workers:admin"]).await?;

        // Fetch the org-scoped host row. RLS hides cross-org rows; the explicit
        // org_id predicate in the query is defense in depth. None → 404.
        let host = state
            .worker_host_service
            .get_host_for_runtime_refresh(&mut db, params.org_id, params.host_id)
            .await?;
        let Some(host) = host else {
            db.rollback().await.map_err(AppError::from)?;
            return Err(AppError::from_error(G5Error::NotFound {
                resource: format!("worker host {}", params.host_id),
            }));
        };

        // A host with no base image has nothing to resolve — operator-input
        // error (422), DB untouched.
        let Some(base_image_ref) = host.base_image_ref else {
            db.rollback().await.map_err(AppError::from)?;
            return Err(AppError::from_error(G5Error::ValidationError {
                details: serde_json::json!({
                    "error": "host has no base_image_ref to resolve runtime labels from",
                    "code": "no_base_image_ref",
                }),
            }));
        };

        // Explicitly close the read-only transaction before making any network
        // calls. Dropping would roll back too, but this makes the connection
        // release visible in review and avoids pinning the pool during OCI I/O.
        db.rollback().await.map_err(AppError::from)?;
        base_image_ref
    };

    // Resolve labels via the injected resolver (wiremock in tests, https in prod).
    // This runs outside any DB transaction; on error the DB is untouched and the
    // error maps to the contract's 422/503.
    let resolved = match state.runtime_label_resolver.resolve(&base_image_ref).await {
        Ok(resolved) => resolved,
        Err(err) => return Err(map_runtime_label_error(&err, params.host_id, params.org_id)),
    };

    // Fail-closed guest glibc-floor guard (provisioning_api#D57, enforcing
    // system_invariants/microvm_execution_boundary#D12). A base_image whose
    // `com.g5labs.runtime.base_image` ABI label POSITIVELY declares a glibc
    // below the floor is rejected (422) here — at the runtime-label resolution
    // point — rather than letting the host-staged dynamically linked runtime
    // crash with a raw missing-symbol error at task launch. A missing/
    // unparseable label or an at-or-above value passes (an unlabeled image is
    // the operator's responsibility, microvm_execution_boundary#D12). Never log the label value (#D43).
    if let Some(label) = resolved
        .labels
        .get("com.g5labs.runtime.base_image")
        .and_then(|v| v.as_str())
    {
        if g5_contracts::types::worker_host::base_image_below_glibc_floor(label) {
            tracing::warn!(
                target: "g5_audit",
                event = "worker_host.base_image_below_glibc_floor",
                host_id = %params.host_id,
                org_id = %params.org_id,
                "Rejected runtime-label refresh: base_image declares a sub-floor guest glibc"
            );
            return Err(AppError::from_error(G5Error::ValidationError {
                details: serde_json::json!({
                    "error": g5_contracts::types::worker_host::glibc_floor_violation_message(),
                    "code": "base_image_below_glibc_floor",
                }),
            }));
        }
    }

    let mut db = scoped_db_for_org(&state, &auth, params.org_id, &["workers:admin"]).await?;

    // Persist. None → the row vanished mid-refresh (e.g. a concurrent DELETE) →
    // 404, DB untouched (the UPDATE matched nothing).
    let updated = state
        .worker_host_service
        .set_runtime_labels(
            &mut db,
            params.org_id,
            params.host_id,
            resolved.schema.as_deref(),
            &resolved.labels,
        )
        .await?;
    let Some(updated) = updated else {
        db.rollback().await.map_err(AppError::from)?;
        return Err(AppError::from_error(G5Error::NotFound {
            resource: format!("worker host {}", params.host_id),
        }));
    };

    db.commit().await.map_err(AppError::from)?;

    // Audit event — NEVER includes label values or tokens (ontology [D43]).
    tracing::info!(
        target: "g5_audit",
        event = "worker_host.runtime_labels_refreshed",
        host_id = %params.host_id,
        org_id = %params.org_id,
        actor_user_id = %user_id,
        runtime_schema = updated.runtime_schema.as_deref().unwrap_or(""),
        "Refreshed worker host runtime labels"
    );

    Ok(Json(ResponseEnvelope::ok(updated)))
}

/// Map a [`RuntimeLabelError`] to the contract's `AppError` (ontology [D43]).
///
/// Operator-input / contract-incompatibility → 422 `ValidationError` with a
/// machine-readable code; registry / network / auth / timeout / oversize-network
/// → 503 `ServiceUnavailable` (fail loudly, retryable). Uses ONLY existing
/// `G5Error` variants. The 503 message carries no secrets.
fn map_runtime_label_error(err: &RuntimeLabelError, host_id: Uuid, org_id: Uuid) -> AppError {
    // Invariant breach: a stored ref that PR1 should have rejected at provision.
    // Surface as a 500 (not an operator 422) and log loudly for investigation.
    if err.is_internal() {
        tracing::error!(
            target: "g5_audit",
            event = "worker_host.runtime_labels_refresh_invariant_breach",
            host_id = %host_id,
            org_id = %org_id,
            reason = err.machine_code(),
            "Stored base_image_ref failed digest validation during refresh"
        );
        return AppError::from_error(G5Error::Internal {
            message: "stored base image reference is invalid".to_string(),
        });
    }

    if err.is_validation() {
        let mut details = serde_json::json!({
            "error": "runtime label metadata is incompatible or malformed",
            "code": err.machine_code(),
        });
        // List the supported schemas, but do NOT echo the registry-controlled
        // `found` value anywhere (neither the client body nor the logs): D43
        // requires secret-free errors and that the handler NEVER logs label
        // values. The machine code alone identifies the failure class.
        if matches!(err, RuntimeLabelError::UnsupportedSchema { .. }) {
            details["supported"] = serde_json::json!(["v1"]);
        }
        return AppError::from_error(G5Error::ValidationError { details });
    }

    // Registry/network failure — surface as a retryable 503 and log loudly so a
    // failed refresh is visible. The error variant is secret-free by construction.
    tracing::warn!(
        target: "g5_audit",
        event = "worker_host.runtime_labels_refresh_failed",
        host_id = %host_id,
        org_id = %org_id,
        reason = err.machine_code(),
        "Runtime-label refresh failed against the image registry"
    );
    AppError::from_error(G5Error::ServiceUnavailable {
        message: err.service_message().to_string(),
    })
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_query_clamps_limit_and_offset() {
        let q = ListWorkerHostsQuery {
            status: None,
            active_only: false,
            limit: Some(500),
            offset: Some(-10),
        };
        assert_eq!(q.limit_clamped(), 100, "limit clamps to MAX_LIMIT");
        assert_eq!(q.offset_clamped(), 0, "negative offset coerces to 0");

        let zero_limit = ListWorkerHostsQuery {
            status: None,
            active_only: false,
            limit: Some(0),
            offset: None,
        };
        assert_eq!(zero_limit.limit_clamped(), 1, "limit floors at 1");
        assert_eq!(
            zero_limit.offset_clamped(),
            0,
            "missing offset defaults to 0"
        );
    }

    #[test]
    fn list_query_uses_defaults_when_absent() {
        let q = ListWorkerHostsQuery::default();
        assert_eq!(q.limit_clamped(), 50);
        assert_eq!(q.offset_clamped(), 0);
        assert!(q.status.is_none());
    }

    #[test]
    fn list_query_deserialises_camel_case_status_filter() {
        // The query struct uses camelCase + an optional snake_case status enum;
        // prove the filter and paging fields deserialise as the wire expects.
        let q: ListWorkerHostsQuery =
            serde_json::from_str(r#"{"status":"ready","limit":10,"offset":20}"#).unwrap();
        assert_eq!(q.status, Some(WorkerHostStatus::Ready));
        assert_eq!(q.limit_clamped(), 10);
        assert_eq!(q.offset_clamped(), 20);

        let empty: ListWorkerHostsQuery = serde_json::from_str("{}").unwrap();
        assert_eq!(empty.status, None);
        assert_eq!(empty.limit_clamped(), 50);
    }
}
