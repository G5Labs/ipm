//! Worker-host provisioning and lifecycle database operations, and delivery
//! of recorded drains to a host's control tunnel.
//!
//! Route handlers own authentication, request validation, response envelopes,
//! and transaction commit/rollback. This service owns the SQL against
//! `worker_hosts`, keeping route handlers free of direct database queries.

use std::time::Duration;

use chrono::{DateTime, Utc};
use g5_authz::ScopedDb;
use g5_internal_api::host_ctl::{
    method, CommandParams, CommandResult, HostDrain, HostRuntimeIdentity,
};
use serde_json::Value;
use sqlx::Row;
use uuid::Uuid;

use crate::errors::AppError;
use crate::models::DEFAULT_MAX_RUNNING_VMS;
use crate::services::org_admin_settings;
use crate::ws::host_ctl::HostCtlError;
use crate::ws::preview_tunnel::TunnelRegistry;
use g5_contracts::types::worker_host::{
    validate_base_image_ref, HostDiskAllocation, WorkerHost, WorkerHostStatus,
};
// The promoted-image list is organization seed input, parsed where the
// organization defaults are seeded; hosts reuse the parser.
pub use crate::services::identity::org_service::parse_promoted_base_images;

/// Minimal projection of a worker-host row needed to drive a runtime-label
/// refresh (ontology [D43]). Only `base_image_ref` is required to decide whether
/// the host has an image to resolve; the full updated row is returned later by
/// [`WorkerHostService::set_runtime_labels`].
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RuntimeRefreshHostRow {
    /// The host's digest-pinned base image ref, or `None` when it provisioned
    /// without one (refresh then returns a 422 `no_base_image_ref`).
    pub base_image_ref: Option<String>,
}

/// Worker-host management service.
#[derive(Clone, Default)]
pub struct WorkerHostService;

/// One workspace with a non-stopped VM: the org running-compute inventory
/// row shared by the Workers page (`workers:admin`) and the member-visible
/// Home compute tile (`workspaces:read`, workbench Phase 6). Slot size and
/// instance type come from the placed host; `None` before placement.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct WorkspaceVmInventoryRow {
    pub workspace_id: Uuid,
    pub workspace_name: String,
    #[sqlx(skip)]
    pub agent_review: Option<AgentReviewInventoryLink>,
    pub host_id: Option<Uuid>,
    pub status: String,
    pub started_at: Option<DateTime<Utc>>,
    pub last_activity_at: Option<DateTime<Utc>>,
    pub owner_user_id: Option<Uuid>,
    pub slot_cpus: Option<i32>,
    pub slot_mem_mib: Option<i32>,
    pub instance_type: Option<String>,
    #[sqlx(default)]
    pub owner_display_name: Option<String>,
    #[sqlx(default)]
    pub owner_email: Option<String>,
}

/// Which workspaces the inventory may name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmInventoryVisibility {
    /// Every workspace of the organization (workers admins, org admins — the same org-wide access the workspace list grants them).
    OrgWide,
    /// Ordinary workspaces and this member's personal workspace, under the
    /// same fixed audience used by the workspace list.
    ReadableBy(Uuid),
}

impl VmInventoryVisibility {
    /// What a member of the organization may see: org admins see
    /// every workspace, as in the workspace list; everyone else their
    /// readable workspaces. The role is read from the system pool because
    /// `org_memberships` sits outside the tenant grant; callers authorize the
    /// org scope first.
    pub async fn for_member(
        system_pool: &sqlx::PgPool,
        org_id: Uuid,
        user_id: Uuid,
    ) -> Result<Self, AppError> {
        let org_role: Option<String> = sqlx::query_scalar(
            "SELECT role FROM org_memberships WHERE org_id = $1 AND user_id = $2 AND deactivated_at IS NULL",
        )
        .bind(org_id)
        .bind(user_id)
        .fetch_optional(system_pool)
        .await?;
        Ok(if matches!(org_role.as_deref(), Some("admin")) {
            Self::OrgWide
        } else {
            Self::ReadableBy(user_id)
        })
    }
}

/// Summary only: source readers discover formal compute without receiving the
/// private workspace capability. All joins are organization-bound; callers
/// have already authorized their inventory scope before this separately scoped server read.
#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct AgentReviewInventoryLink {
    pub review_id: Uuid,
    pub source_workspace_id: Uuid,
    pub source_workspace_name: String,
}

#[derive(sqlx::FromRow)]
pub struct FormalReviewWorkspaceInventory {
    pub workspace_id: Uuid,
    pub workspace_name: String,
    #[sqlx(flatten)]
    pub agent_review: AgentReviewInventoryLink,
    pub read_only: bool,
    pub created_by: Option<Uuid>,
    pub cost: f64,
    pub owner_user_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub phase: Option<String>,
    pub total_tasks: i64,
    pub tasks_completed: i64,
    pub concurrent_tasks: i64,
    pub host_id: Option<Uuid>,
    pub status: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub last_activity_at: Option<DateTime<Utc>>,
    pub slot_cpus: Option<i32>,
    pub slot_mem_mib: Option<i32>,
    pub instance_type: Option<String>,
}

pub async fn list_formal_review_workspaces(
    authz: &dyn g5_authz::AuthzService,
    system_pool: &sqlx::PgPool,
    org_id: Uuid,
    visibility: VmInventoryVisibility,
) -> Result<Vec<FormalReviewWorkspaceInventory>, g5_contracts::G5Error> {
    let mut db =
        crate::db::scoped::scoped_db_for_system_org_with(authz, system_pool, org_id, &["*"])
            .await
            .map_err(|error| error.inner)?;
    let viewer = match visibility {
        VmInventoryVisibility::OrgWide => None,
        VmInventoryVisibility::ReadableBy(user) => Some(user),
    };
    let rows = sqlx::query_as(
        r#"SELECT w.id AS workspace_id, w.name AS workspace_name,
            r.id AS review_id, source.id AS source_workspace_id, source.name AS source_workspace_name,
            w.owner_user_id, w.created_by,
            COALESCE((SELECT SUM(CAST(u.cost_usd AS DOUBLE PRECISION)) FROM usage_records u WHERE u.org_id=r.org_id AND u.workspace_id=w.id),0.0) AS cost,
            (w.mutation_policy='read_only') AS read_only, w.created_at, w.updated_at, w.phase,
            CASE WHEN t.id IS NULL THEN 0::bigint ELSE 1 END AS total_tasks,
            CASE WHEN t.status = 'succeeded' THEN 1::bigint ELSE 0 END AS tasks_completed,
            CASE WHEN t.status = 'running' THEN 1::bigint ELSE 0 END AS concurrent_tasks,
            vm.host_id, vm.status,
            CASE WHEN vm.vm_id IS NOT NULL THEN vm.started_at END AS started_at,
            CASE WHEN vm.vm_id IS NOT NULL THEN vm.last_activity_at END AS last_activity_at,
            host.slot_cpus, host.slot_mem_mib, host.instance_type
        FROM agent_approval_reviews r
        JOIN workspaces w ON w.id=r.review_workspace_id AND w.org_id=r.org_id
            AND w.agent_approval_review_id=r.id AND w.deleted_at IS NULL
        JOIN workspaces source ON source.id=r.source_workspace_id AND source.org_id=r.org_id
            AND source.deleted_at IS NULL
        LEFT JOIN tasks t ON t.id=r.task_id AND t.org_id=r.org_id AND t.workspace_id=w.id
        LEFT JOIN workspace_vms vm ON vm.workspace_id=w.id AND vm.org_id=r.org_id
        LEFT JOIN worker_hosts host ON host.host_id=vm.host_id AND host.org_id=r.org_id
        WHERE r.org_id=$1 AND authz.workspace_audience_allows(source.personal_user_id,
            source.owner_user_id, source.agent_approval_review_id, $2, $2::uuid IS NULL)
        ORDER BY w.updated_at DESC, w.id DESC"#,
    ).bind(org_id).bind(viewer).fetch_all(db.conn()).await?;
    db.rollback()
        .await
        .map_err(|error| g5_contracts::G5Error::Internal {
            message: error.to_string(),
        })?;
    Ok(rows)
}

/// Current VM assignments of an organization, excluding stopped VMs and
/// deleted workspaces, ordered by host then workspace name. Owner profiles
/// live outside tenant-table grants: only the owner ids of the authorized
/// rows are resolved from the system pool, never a cross-org projection.
pub async fn list_active_workspace_vms(
    db: &mut ScopedDb,
    system_pool: &sqlx::PgPool,
    authz: &dyn g5_authz::AuthzService,
    org_id: Uuid,
    visibility: VmInventoryVisibility,
) -> Result<Vec<WorkspaceVmInventoryRow>, AppError> {
    let viewer = match visibility {
        VmInventoryVisibility::OrgWide => None,
        VmInventoryVisibility::ReadableBy(user_id) => Some(user_id),
    };
    let mut rows = sqlx::query_as::<_, WorkspaceVmInventoryRow>(
        r#"
        SELECT workspace.id AS workspace_id, workspace.name AS workspace_name,
               vm.host_id, vm.status,
               CASE WHEN vm.vm_id IS NOT NULL THEN vm.started_at END AS started_at,
               CASE WHEN vm.vm_id IS NOT NULL THEN vm.last_activity_at END AS last_activity_at,
               workspace.owner_user_id,
               host.slot_cpus, host.slot_mem_mib, host.instance_type
        FROM workspace_vms vm
        JOIN workspaces workspace
          ON workspace.org_id = vm.org_id AND workspace.id = vm.workspace_id
        LEFT JOIN worker_hosts host
          ON host.org_id = vm.org_id AND host.host_id = vm.host_id
        WHERE vm.org_id = $1 AND vm.status <> 'stopped'
          AND workspace.deleted_at IS NULL
          AND workspace.agent_approval_review_id IS NULL
          AND authz.workspace_audience_allows(workspace.personal_user_id, workspace.owner_user_id,
              workspace.agent_approval_review_id, $2, $2::uuid IS NULL)
        ORDER BY vm.host_id ASC NULLS LAST, workspace.name, workspace.id
        "#,
    )
    .bind(org_id)
    .bind(viewer)
    .fetch_all(db.conn())
    .await?;
    for review in list_formal_review_workspaces(authz, system_pool, org_id, visibility).await? {
        let Some(status) = review.status.filter(|status| status != "stopped") else {
            continue;
        };
        rows.push(WorkspaceVmInventoryRow {
            workspace_id: review.workspace_id,
            workspace_name: review.workspace_name,
            agent_review: Some(review.agent_review),
            host_id: review.host_id,
            status,
            started_at: review.started_at,
            last_activity_at: review.last_activity_at,
            owner_user_id: review.owner_user_id,
            slot_cpus: review.slot_cpus,
            slot_mem_mib: review.slot_mem_mib,
            instance_type: review.instance_type,
            owner_display_name: None,
            owner_email: None,
        });
    }
    rows.sort_by(|a, b| {
        a.host_id
            .is_none()
            .cmp(&b.host_id.is_none())
            .then(a.host_id.cmp(&b.host_id))
            .then(a.workspace_name.cmp(&b.workspace_name))
            .then(a.workspace_id.cmp(&b.workspace_id))
    });
    let owner_ids: Vec<Uuid> = rows.iter().filter_map(|row| row.owner_user_id).collect();
    if owner_ids.is_empty() {
        return Ok(rows);
    }
    let owners = sqlx::query_as::<_, (Uuid, String, String)>(
        "SELECT id, display_name, email FROM users WHERE id = ANY($1)",
    )
    .bind(&owner_ids)
    .fetch_all(system_pool)
    .await?
    .into_iter()
    .map(|(id, name, email)| (id, (name, email)))
    .collect::<std::collections::HashMap<_, _>>();
    for row in &mut rows {
        if let Some((name, email)) = row.owner_user_id.and_then(|id| owners.get(&id)) {
            row.owner_display_name = Some(name.clone());
            row.owner_email = Some(email.clone());
        }
    }
    Ok(rows)
}

/// Parameters for inserting a provisioning worker-host row.
pub struct ProvisionHostRecordParams<'a> {
    pub org_id: Uuid,
    pub region: &'a str,
    pub instance_type: &'a str,
    pub base_image_ref: Option<&'a str>,
    pub base_image_digest: Option<&'a str>,
    pub base_image_source_kind: &'a str,
    pub base_image_source_value: &'a str,
    pub worker_profile: &'a str,
    /// Equal-slot runtime sizes persisted before the host may become ready.
    /// `vm.start` specs are built from these columns and the supervisor rejects
    /// any size that differs from its own configuration.
    pub slot_cpus: i32,
    pub slot_mem_mib: i32,
    /// The operator's EXPLICIT `workspaceVmLimit` choice (ontology [D54]) — the
    /// raw request value on the POST path, the copied `workspace_vm_limit` column
    /// on the rotation path. `None` means "track the current default": the
    /// resolved default is never persisted, so default changes roll out to
    /// omitted-value hosts through rotation.
    pub workspace_vm_limit: Option<i32>,
    /// The operator's EXPLICIT `bootDiskGb` override (ontology
    /// `g5.runtime.placement.host-disk`) — the raw request value on the POST
    /// path, the copied `boot_disk_gb` column on the rotation path. `None`
    /// means "track the current sizing rule": the derived size is never
    /// persisted, so a rule change reaches omitted-value hosts through
    /// replacement.
    pub boot_disk_gb: Option<i32>,
    /// The acting user on the POST path; the replaced host's creator on the
    /// rotation path. `None` only for callers without a user.
    pub created_by_user_id: Option<Uuid>,
}

#[derive(Debug, Clone)]
pub struct PlacementRequest<'a> {
    pub org_id: Uuid,
    pub workspace_id: Uuid,
    pub region: &'a str,
    /// Process-local control reachability; quota and incarnation authority stay here.
    pub reachable_host_ids: &'a [Uuid],
    pub excluded_host_ids: &'a [Uuid],
    pub start_deadline_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlacementReservation {
    pub host_id: Uuid,
    pub vm_id: Uuid,
    pub epoch: u64,
    pub occupancy_before: u32,
    pub previous_activity_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct PlacementBootRecord<'a> {
    pub image_ref: &'a str,
    pub image_digest: &'a str,
    pub image_source: &'a Value,
    pub image_oid_vector: &'a Value,
    pub module_catalog_version: &'a str,
    pub boot_spec: &'a Value,
    pub lease_ttl_secs: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlacementOutcome {
    Reserved(PlacementReservation),
    NoCapacity,
    QuotaExceeded { limit: u32 },
    OwnerLimitExceeded { limit: i64 },
    StartIntentUnavailable,
    WorkspaceBusy,
}

/// The outcome of [`WorkerHostService::request_drain`].
#[derive(Debug)]
pub enum DrainRequest {
    /// The operator drain is recorded, or was already.
    Recorded(HostDrainRecord),
    /// Rotation is draining the host and finishes that drain itself.
    RotationOwned,
    /// No active host matched.
    Absent,
}

/// What [`WorkerHostService::remove_host`] did: the drain to deliver after
/// commit, the VMs released as forced losses, and the terminated row's cloud
/// instance.
#[derive(Debug)]
pub struct HostRemoval {
    pub drain: Option<HostDrainRecord>,
    pub forced_vm_losses: i64,
    pub cloud_instance_id: Option<String>,
}

/// A drain that `request_drain` recorded. Its host and drain id are private,
/// so only this crate creates one and a route delivers exactly the drain it
/// recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostDrainRecord {
    drain_id: Uuid,
    pub active_vms: u32,
    pub already_draining: bool,
    host_id: Uuid,
}

impl HostDrainRecord {
    /// The drain id stored in `worker_hosts`.
    #[must_use]
    pub fn drain_id(&self) -> Uuid {
        self.drain_id
    }
}

/// The runtime identity this control plane places on, as the JSON a host's
/// hello stores in `worker_hosts.runtime_identity`
/// (`g5.runtime.placement.runtime-identity`).
pub(crate) fn current_runtime_identity() -> Value {
    serde_json::to_value(HostRuntimeIdentity::bundled())
        .expect("the bundled runtime identity serializes")
}

/// `current`, `stale` or `unknown` for a host's stored runtime identity, as
/// the Workers page shows it.
pub(crate) fn runtime_state(identity: &Value) -> &'static str {
    match serde_json::from_value::<HostRuntimeIdentity>(identity.clone()) {
        Ok(identity) if identity.is_current() => "current",
        Ok(_) => "stale",
        Err(_) => "unknown",
    }
}

impl WorkerHostService {
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Atomically enforce the org VM quota, lock a concrete host slot, and
    /// bind the workspace's current start intent to a fresh incarnation.
    /// Every caller must use this path so the locked org config row serializes
    /// quota decisions and `workspace_vms` remains the occupancy authority.
    pub async fn place(
        &self,
        db: &mut ScopedDb,
        request: PlacementRequest<'_>,
    ) -> Result<PlacementOutcome, AppError> {
        // The org config row is optional, so it cannot be the serialization
        // authority for quota checks. Lock the org identity itself before
        // reading either the optional override or current occupancy.
        org_admin_settings::lock_org_vm_quota(db.conn(), request.org_id).await?;
        let quota = sqlx::query_scalar::<_, i64>(
            "SELECT (value #>> '{}')::bigint
             FROM configs
             WHERE org_id = $1 AND scope = 'org' AND scope_id = $1
               AND key = $2
             FOR UPDATE",
        )
        .bind(request.org_id)
        .bind(org_admin_settings::MAX_RUNNING_VMS_KEY)
        .fetch_optional(db.conn())
        .await?
        .unwrap_or(DEFAULT_MAX_RUNNING_VMS)
        .max(0);

        // Resume admission and ownership transfer lock the workspace before its
        // VM. A joined FOR UPDATE may lock the VM first, deadlocking placement
        // against a concurrent resume that already owns the workspace row.
        let live_workspace: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM workspaces
             WHERE org_id = $1 AND id = $2 AND deleted_at IS NULL
               AND deletion_operation_id IS NULL
             FOR UPDATE",
        )
        .bind(request.org_id)
        .bind(request.workspace_id)
        .fetch_optional(db.conn())
        .await?;
        if live_workspace.is_none() {
            return Ok(PlacementOutcome::StartIntentUnavailable);
        }
        let owner: Uuid = sqlx::query_scalar("SELECT owner_user_id FROM workspaces WHERE id = $1")
            .bind(request.workspace_id)
            .fetch_one(db.conn())
            .await?;
        crate::services::PermissionService::require_active_member_on_conn(
            db.conn(),
            owner,
            request.org_id,
            true,
        )
        .await
        .map_err(AppError::from_error)?;
        let workspace = sqlx::query(
            "SELECT status, start_intent, last_activity_at FROM workspace_vms
             WHERE org_id = $1 AND workspace_id = $2 FOR UPDATE",
        )
        .bind(request.org_id)
        .bind(request.workspace_id)
        .fetch_optional(db.conn())
        .await?;
        let Some(workspace) = workspace else {
            return Ok(PlacementOutcome::StartIntentUnavailable);
        };
        let status = workspace.try_get::<String, _>("status")?;
        if matches!(status.as_str(), "starting" | "running" | "stopping") {
            return Ok(PlacementOutcome::WorkspaceBusy);
        }
        let start_intent = workspace.try_get::<Option<Value>, _>("start_intent")?;
        let Some(start_intent) = start_intent else {
            return Ok(PlacementOutcome::StartIntentUnavailable);
        };
        if !matches!(
            start_intent.get("status").and_then(Value::as_str),
            Some("pending" | "queued_for_capacity")
        ) || start_intent
            .get("expires_at")
            .and_then(Value::as_str)
            .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
            .is_none_or(|expires_at| expires_at <= Utc::now())
        {
            return Ok(PlacementOutcome::StartIntentUnavailable);
        }
        // Refusal expiry on the clock that recorded it (issue #627 S17): the
        // writer stamps SQL `now()`, so read the same transaction's clock.
        let db_now: DateTime<Utc> = sqlx::query_scalar("SELECT now()")
            .fetch_one(db.conn())
            .await?;
        let excluded_host_ids =
            merged_excluded_host_ids(request.excluded_host_ids, &start_intent, db_now);

        // The owner's limit precedes the organization quota
        // (`g5.runtime.placement.owner-limit`), so every Start that waits for
        // the quota or a host already fits it. Count the owner's other
        // custody and their valid Starts that have waited since before this
        // one, in the start queue's order, so concurrent Starts admit the
        // earliest and queued demand never exceeds the limit.
        let owner_limit = org_admin_settings::max_running_vms_per_owner(db.conn(), request.org_id)
            .await
            .map_err(AppError::from_error)?
            .max(0);
        let owner_claims = sqlx::query_scalar::<_, i64>(
            "SELECT count(*)
             FROM workspace_vms AS vm
             JOIN workspaces AS workspace
               ON workspace.id = vm.workspace_id AND workspace.org_id = vm.org_id
             CROSS JOIN (
                 SELECT (start_intent ->> 'created_at')::timestamptz AS created_at
                 FROM workspace_vms WHERE workspace_id = $3
             ) AS this
             WHERE vm.org_id = $1 AND workspace.owner_user_id = $2
               AND vm.workspace_id <> $3
               AND (
                   vm.status IN ('starting', 'running', 'stopping')
                   OR (
                       vm.status IN ('stopped', 'failed')
                       AND workspace.deleted_at IS NULL
                       AND vm.start_intent ->> 'status' IN ('pending', 'queued_for_capacity')
                       AND (vm.start_intent ->> 'expires_at')::timestamptz > now()
                       AND (vm.start_intent ->> 'owner_user_id')::uuid = workspace.owner_user_id
                       AND (vm.start_intent ->> 'lifecycle_generation')::bigint
                           = workspace.lifecycle_generation
                       AND ((vm.start_intent ->> 'created_at')::timestamptz, vm.workspace_id)
                           < (this.created_at, $3)
                   )
               )",
        )
        .bind(request.org_id)
        .bind(owner)
        .bind(request.workspace_id)
        .fetch_one(db.conn())
        .await?;
        if owner_claims >= owner_limit {
            // Refused, not queued: the Start's consent ends here, so neither
            // the start queue nor managed growth serves it again.
            sqlx::query(
                "UPDATE workspace_vms
                 SET start_intent = jsonb_set(start_intent, '{status}', '\"failed\"'),
                     last_error = jsonb_build_object('reason', 'owner_vm_limit', 'limit', $3::bigint),
                     updated_at = now()
                 WHERE org_id = $1 AND workspace_id = $2",
            )
            .bind(request.org_id)
            .bind(request.workspace_id)
            .bind(owner_limit)
            .execute(db.conn())
            .await?;
            return Ok(PlacementOutcome::OwnerLimitExceeded { limit: owner_limit });
        }

        let active = sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM workspace_vms
             WHERE org_id = $1 AND status IN ('starting', 'running', 'stopping')",
        )
        .bind(request.org_id)
        .fetch_one(db.conn())
        .await?;
        if active >= quota {
            // Record why the start waits, so the owner sees the quota rather
            // than a missing host.
            sqlx::query(
                "UPDATE workspace_vms
                 SET start_intent = jsonb_set(start_intent, '{status}', '\"queued_for_capacity\"'),
                     last_error = jsonb_build_object('reason', 'org_vm_quota', 'limit', $3::bigint)
                         || CASE WHEN last_error ? 'host_reason'
                            THEN jsonb_build_object('host_reason', last_error -> 'host_reason')
                            ELSE '{}'::jsonb END,
                     updated_at = now()
                 WHERE org_id = $1 AND workspace_id = $2",
            )
            .bind(request.org_id)
            .bind(request.workspace_id)
            .bind(quota)
            .execute(db.conn())
            .await?;
            return Ok(PlacementOutcome::QuotaExceeded {
                limit: u32::try_from(quota).unwrap_or(u32::MAX),
            });
        }

        let candidate = sqlx::query(
            "SELECT host.host_id,
                    (SELECT count(*) FROM workspace_vms AS occupied
                     WHERE occupied.host_id = host.host_id
                       AND occupied.status IN ('starting', 'running', 'stopping')) AS occupancy
             FROM worker_hosts AS host
             WHERE host.org_id = $1 AND host.region = $2 AND host.status = 'ready' AND host.slot_size_reported
               AND NOT (host.host_id = ANY($3::uuid[]))
               AND host.runtime_identity @> $4::jsonb
               AND host.host_id = ANY($5::uuid[])
               AND (SELECT count(*) FROM workspace_vms AS occupied
                    WHERE occupied.host_id = host.host_id
                      AND occupied.status IN ('starting', 'running', 'stopping')) < host.slots_total
             ORDER BY EXISTS (SELECT 1 FROM workspace_vms running
                              WHERE running.host_id = host.host_id AND running.status = 'running') DESC,
                      occupancy DESC, host.created_at ASC, host.host_id ASC
             FOR UPDATE OF host SKIP LOCKED
             LIMIT 1",
        )
        .bind(request.org_id)
        .bind(request.region)
        .bind(&excluded_host_ids)
        .bind(current_runtime_identity())
        .bind(request.reachable_host_ids)
        .fetch_optional(db.conn())
        .await?;
        let Some(candidate) = candidate else {
            // No ready, runtime-current host has a free slot. The CURRENT wait
            // reason replaces a stale one, while the host's actionable refusal
            // detail (`host_reason`, e.g. `disk`) is carried forward so the
            // owner keeps seeing WHY the last host refused
            // (`g5.runtime.placement.capacity`, issue #627 S7); the capacity
            // sweep grows the fleet when nothing usable remains
            // (`g5.runtime.placement.runtime-identity`).
            sqlx::query(
                "UPDATE workspace_vms
                 SET start_intent = jsonb_set(start_intent, '{status}', '\"queued_for_capacity\"'),
                     last_error = jsonb_build_object('reason', 'no_ready_host')
                         || CASE WHEN last_error ? 'host_reason'
                            THEN jsonb_build_object('host_reason', last_error -> 'host_reason')
                            ELSE '{}'::jsonb END,
                     updated_at = now()
                 WHERE org_id = $1 AND workspace_id = $2",
            )
            .bind(request.org_id)
            .bind(request.workspace_id)
            .execute(db.conn())
            .await?;
            return Ok(PlacementOutcome::NoCapacity);
        };

        let host_id = candidate.try_get::<Uuid, _>("host_id")?;
        let occupancy = candidate.try_get::<i64, _>("occupancy")?;
        let vm_id = Uuid::new_v4();
        let epoch = sqlx::query_scalar::<_, i64>(
            "UPDATE workspace_vms
             SET host_id = $3,
                 vm_id = $4,
                 epoch = epoch + 1,
                 status = 'starting',
                 lease_sequence = 0,
                 lease_expires_at = now() + make_interval(secs => 300),
                 start_deadline_at = $5,
                 start_command_sent_at = NULL,
                 started_at = NULL,
                 last_activity_at = NULL,
                 idle_retry_after = NULL,
                 start_intent = jsonb_set(
                     jsonb_set(start_intent, '{status}', '\"starting\"'),
                     '{attempts}',
                     to_jsonb(COALESCE((start_intent ->> 'attempts')::integer, 0) + 1)
                 ),
                 last_error = NULL,
                 updated_at = now()
             WHERE org_id = $1 AND workspace_id = $2
               AND status IN ('stopped', 'failed')
             RETURNING epoch",
        )
        .bind(request.org_id)
        .bind(request.workspace_id)
        .bind(host_id)
        .bind(vm_id.to_string())
        .bind(request.start_deadline_at)
        .fetch_one(db.conn())
        .await?;
        Ok(PlacementOutcome::Reserved(PlacementReservation {
            host_id,
            vm_id,
            epoch: u64::try_from(epoch).map_err(|_| {
                AppError::from_error(g5_contracts::error::G5Error::Internal {
                    message: "workspace VM epoch became negative".to_string(),
                })
            })?,
            occupancy_before: u32::try_from(occupancy).unwrap_or(u32::MAX),
            previous_activity_at: workspace.try_get("last_activity_at")?,
        }))
    }

    /// Bind the immutable boot decision to an already reserved incarnation.
    /// This exact-incarnation CAS closes the interval before command delivery:
    /// a replacement reservation can never inherit an earlier boot spec.
    pub async fn bind_boot_record(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        workspace_id: Uuid,
        reservation: &PlacementReservation,
        boot: PlacementBootRecord<'_>,
    ) -> Result<bool, AppError> {
        let updated = sqlx::query(
            "UPDATE workspace_vms
             SET image_ref = $6,
                 image_digest = $7,
                 image_source = $8,
                 image_oid_vector = $9,
                 module_catalog_version = $10,
                 boot_spec = $11,
                 lease_expires_at = now() + make_interval(secs => $12),
                 updated_at = now()
             WHERE org_id = $1 AND workspace_id = $2 AND host_id = $3
               AND vm_id = $4 AND epoch = $5 AND status = 'starting'
               AND boot_spec IS NULL",
        )
        .bind(org_id)
        .bind(workspace_id)
        .bind(reservation.host_id)
        .bind(reservation.vm_id.to_string())
        .bind(i64::try_from(reservation.epoch).unwrap_or(i64::MAX))
        .bind(boot.image_ref)
        .bind(boot.image_digest)
        .bind(boot.image_source)
        .bind(boot.image_oid_vector)
        .bind(boot.module_catalog_version)
        .bind(boot.boot_spec)
        .bind(boot.lease_ttl_secs)
        .execute(db.conn())
        .await?;
        Ok(updated.rows_affected() == 1)
    }

    /// Release an unadmitted placement back to the durable FIFO queue: the
    /// host refused it, or it is absent from a host that saw every Start sent
    /// for it. The exact incarnation fence prevents a delayed refusal from
    /// clearing a replacement reservation; `start_sent` is the Start-delivery
    /// state the caller observed, which the release fences on
    /// (`g5.runtime.start.settlement`). `host_reason` records why the host
    /// refused (for example `disk`), so the owner sees the real cause.
    #[allow(clippy::too_many_arguments)]
    pub async fn release_placement(
        &self,
        db: &mut ScopedDb,
        reservation: &PlacementReservation,
        org_id: Uuid,
        workspace_id: Uuid,
        start_sent: bool,
        reason: &str,
        host_reason: Option<&str>,
    ) -> Result<bool, AppError> {
        Ok(
            crate::services::hosts::workspace_vm_release::release_refused_placement(
                db.conn(),
                org_id,
                workspace_id,
                reservation.host_id,
                reservation.vm_id,
                reservation.epoch,
                start_sent,
                reason,
                host_reason,
            )
            .await?,
        )
    }

    /// Persist one idempotent operator drain request. A repeated request while
    /// already draining returns the original drain id. A host rotation is
    /// draining (it records no drain id) stays rotation's, so an operator
    /// drain never takes over a host rotation may replace and tear down.
    pub async fn request_drain(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        host_id: Uuid,
        proposed_drain_id: Uuid,
        reason: &str,
    ) -> Result<DrainRequest, AppError> {
        let row = sqlx::query(
            "UPDATE worker_hosts
             SET status = 'draining',
                 drain_id = COALESCE(drain_id, $3),
                 drain_requested_at = COALESCE(drain_requested_at, now()),
                 drain_reason = COALESCE(drain_reason, $4)
             WHERE org_id = $1 AND host_id = $2
               AND status IN ('ready', 'unreachable', 'draining')
               AND (status <> 'draining' OR drain_id IS NOT NULL)
             RETURNING drain_id,
                 (drain_id IS DISTINCT FROM $3) AS already_draining,
                 (SELECT count(*) FROM workspace_vms AS vm
                  WHERE vm.host_id = worker_hosts.host_id
                    AND vm.status IN ('starting', 'running', 'stopping')) AS active_vms",
        )
        .bind(org_id)
        .bind(host_id)
        .bind(proposed_drain_id)
        .bind(reason)
        .fetch_optional(db.conn())
        .await?;
        if let Some(row) = row {
            return Ok(DrainRequest::Recorded(HostDrainRecord {
                drain_id: row.try_get("drain_id")?,
                active_vms: u32::try_from(row.try_get::<i64, _>("active_vms")?).unwrap_or(u32::MAX),
                already_draining: row.try_get("already_draining")?,
                host_id,
            }));
        }
        let rotation_draining = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM worker_hosts
             WHERE org_id = $1 AND host_id = $2
               AND status = 'draining' AND drain_id IS NULL)",
        )
        .bind(org_id)
        .bind(host_id)
        .fetch_one(db.conn())
        .await?;
        Ok(if rotation_draining {
            DrainRequest::RotationOwned
        } else {
            DrainRequest::Absent
        })
    }

    /// Send `host.drain` for a drain `request_drain` recorded, over the host's
    /// live control tunnel, trying up to `attempts` times while the send times
    /// out without an answer. Delivery is best effort: the drain stays durable
    /// for the host reconciler whatever this returns. `None` means the host has
    /// no command tunnel in this process.
    pub async fn deliver_drain(
        &self,
        tunnels: &TunnelRegistry,
        drain: &HostDrainRecord,
        wait: Duration,
        attempts: usize,
    ) -> Option<Result<CommandResult, HostCtlError>> {
        self.deliver_recorded_drain(tunnels, drain.host_id, drain.drain_id, wait, attempts)
            .await
    }

    /// [`Self::deliver_drain`] for a drain read back from `worker_hosts`, as
    /// the host reconciler does.
    pub(crate) async fn deliver_recorded_drain(
        &self,
        tunnels: &TunnelRegistry,
        host_id: Uuid,
        drain_id: Uuid,
        wait: Duration,
        attempts: usize,
    ) -> Option<Result<CommandResult, HostCtlError>> {
        let client = tunnels.get(&host_id)?.command_client()?;
        let command = CommandParams {
            idempotency_key: format!("host-drain:{drain_id}"),
            budget_ms: 5_000,
            command: HostDrain { drain_id },
        };
        Some(
            client
                .execute_with_retry(method::HOST_DRAIN, &command, wait, attempts)
                .await,
        )
    }

    pub async fn active_vm_count(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        host_id: Uuid,
    ) -> Result<i64, AppError> {
        Ok(sqlx::query_scalar(
            "SELECT count(*) FROM workspace_vms
             WHERE org_id = $1 AND host_id = $2
               AND status IN ('starting', 'running', 'stopping')",
        )
        .bind(org_id)
        .bind(host_id)
        .fetch_one(db.conn())
        .await?)
    }

    /// Three missed default ten-second control heartbeats make a ready host
    /// unavailable for placement without releasing its durable VM custody.
    pub async fn mark_unreachable(&self, db: &mut ScopedDb) -> Result<u64, AppError> {
        let updated = sqlx::query(
            "UPDATE worker_hosts SET status = 'unreachable'
             WHERE status = 'ready'
               AND last_heartbeat < now() - interval '30 seconds'",
        )
        .execute(db.conn())
        .await?;
        Ok(updated.rows_affected())
    }

    /// Insert a DB-only worker-host record at status `provisioning`.
    ///
    /// `base_image_ref`/`base_image_digest` are `None` when the request omitted a
    /// digest-pinned base image (columns stay NULL → the startup script keeps the
    /// `buildpack-deps:resolute` default). `worker_profile` is always supplied by the route
    /// (defaulting to `fat-default`). Slot sizes are provisional until the host
    /// publishes its measured resource contract; only published sizes govern
    /// placement and `VmSpec`. The forward-compat `runtime_*` columns are NOT
    /// bound — they take their DB defaults (NULL / `{}`) for PR2 to populate.
    pub async fn provision_host_record(
        &self,
        db: &mut ScopedDb,
        params: ProvisionHostRecordParams<'_>,
    ) -> Result<(WorkerHost, HostDiskAllocation), AppError> {
        org_admin_settings::lock_org_vm_quota(db.conn(), params.org_id).await?;
        let policy = org_admin_settings::disk_policy(db.conn(), params.org_id).await?;
        let vcpus = crate::cloud::bringup::parse_vcpus_from_instance_type(params.instance_type)
            .unwrap_or(4);
        let slots = match params.workspace_vm_limit {
            Some(n) if n >= 1 && n as u32 <= vcpus => n as u32,
            None => crate::cloud::bringup::default_workspace_vm_limit(vcpus),
            Some(_) => {
                return Err(AppError::from_error(
                    g5_contracts::error::G5Error::ValidationError {
                        details: serde_json::json!({"field": "workspaceVmLimit", "error": "Retained slot override exceeds the machine's supported capacity"}),
                    },
                ))
            }
        };
        let allocation = HostDiskAllocation {
            host_disk_gib: policy.base_gb,
            vm_disk_mib: policy.per_slot_gb * 1024,
            slots,
            disk_gib: policy.resolve(params.boot_disk_gb, slots)?,
        };
        let host = sqlx::query_as::<_, WorkerHost>(
            r#"
            INSERT INTO worker_hosts
                (org_id, region, instance_type, base_image_ref, base_image_digest,
                 base_image_source_kind, base_image_source_value, worker_profile,
                 workspace_vm_limit, slot_cpus, slot_mem_mib, scratch_disk_mib,
                 boot_disk_gb, reported_live_vm_ids, created_by_user_id,
                 host_disk_gib, provisioned_slots, resolved_disk_gib, slots_total)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, '{}'::uuid[], $14, $15, $16, $17, $16)
            RETURNING host_id, org_id, vpc_id, cloud_instance_id, instance_type, region,
                      slots_total, slot_size_reported, slot_cpus, slot_mem_mib, scratch_disk_mib,
                      kernel_version,
                      active_microvm_count, cache_disk_used_bytes, cache_disk_total_bytes,
                      cache_state, attestation_state, status, created_at, last_heartbeat,
                      provision_error_code, provision_retry_count, provision_next_retry_at,
                      base_image_ref, base_image_digest, base_image_source_kind,
                      base_image_source_value, runtime_schema, runtime_labels,
                      runtime_labels_refreshed_at, worker_profile, workspace_vm_limit,
                      created_by_user_id
            "#,
        )
        .bind(params.org_id)
        .bind(params.region)
        .bind(params.instance_type)
        .bind(params.base_image_ref)
        .bind(params.base_image_digest)
        .bind(params.base_image_source_kind)
        .bind(params.base_image_source_value)
        .bind(params.worker_profile)
        .bind(params.workspace_vm_limit)
        .bind(params.slot_cpus)
        .bind(params.slot_mem_mib)
        .bind(allocation.vm_disk_mib as i32)
        .bind(params.boot_disk_gb)
        .bind(params.created_by_user_id)
        .bind(allocation.host_disk_gib as i32)
        .bind(allocation.slots as i32)
        .bind(allocation.disk_gib as i32)
        .fetch_one(db.conn())
        .await?;

        Ok((host, allocation))
    }

    /// Fetch the org's default base image ref (`orgs.default_base_image_ref`,
    /// ontology [D51]).
    ///
    /// Runs on the privileged system pool: `orgs` carries no RLS, and callers
    /// invoke this only AFTER `scoped_db_for_org` has authorized the request
    /// against the path org. `None` when the org has no default (or the org id
    /// is unknown — impossible post-authorization).
    pub async fn org_default_base_image_ref(
        &self,
        pool: &sqlx::PgPool,
        org_id: Uuid,
    ) -> Result<Option<String>, AppError> {
        let default_ref = sqlx::query_scalar::<_, Option<String>>(
            "SELECT default_base_image_ref FROM orgs WHERE id = $1",
        )
        .bind(org_id)
        .fetch_optional(pool)
        .await?
        .flatten();

        Ok(default_ref)
    }

    /// Calling org's image policy snapshot `(track, immutable pin)`.
    pub async fn org_image_policy(
        &self,
        pool: &sqlx::PgPool,
        org_id: Uuid,
    ) -> Result<(Option<String>, Option<String>), AppError> {
        let policy = sqlx::query_as::<_, (Option<String>, Option<String>)>(
            "SELECT default_base_image_track, default_base_image_ref FROM orgs WHERE id = $1",
        )
        .bind(org_id)
        .fetch_optional(pool)
        .await?
        .unwrap_or((None, None));
        Ok(policy)
    }

    /// Atomically replace the org's source policy after the route has
    /// authorized the caller and resolved a validated immutable image ref.
    pub async fn set_org_image_policy(
        &self,
        pool: &sqlx::PgPool,
        org_id: Uuid,
        image_track: Option<&str>,
        image_ref: &str,
    ) -> Result<(), AppError> {
        let updated = sqlx::query(
            "UPDATE orgs \
             SET default_base_image_track = $2, default_base_image_ref = $3 \
             WHERE id = $1",
        )
        .bind(org_id)
        .bind(image_track)
        .bind(image_ref)
        .execute(pool)
        .await?;
        if updated.rows_affected() != 1 {
            return Err(AppError::from_error(
                g5_contracts::error::G5Error::NotFound {
                    resource: "organization".to_string(),
                },
            ));
        }
        Ok(())
    }

    /// Fetch the org's default base image ref ONLY when the org opted into
    /// promoted-image tracking (`default_base_image_track` set, ontology [D52]).
    ///
    /// Used by the idle-rotation replacement path: tracking orgs' replacements
    /// resolve the CURRENT default instead of copying the old host's image;
    /// pinned / no-track orgs return `None` here and keep [D51] copy semantics.
    pub async fn tracked_org_default_base_image_ref(
        &self,
        pool: &sqlx::PgPool,
        org_id: Uuid,
    ) -> Result<Option<String>, AppError> {
        let tracked_ref = sqlx::query_scalar::<_, String>(
            "SELECT default_base_image_ref FROM orgs \
             WHERE id = $1 \
               AND default_base_image_track IS NOT NULL \
               AND default_base_image_ref IS NOT NULL",
        )
        .bind(org_id)
        .fetch_optional(pool)
        .await?;

        Ok(tracked_ref)
    }

    /// Resolve the org's tracked promoted default to a validated
    /// `(image_ref, digest)` snapshot (ontology [D52]/[D55]).
    ///
    /// Pinned / no-track orgs resolve to `None` ([D51] copy semantics). A
    /// tracked ref that fails digest validation (config drift; unreachable
    /// under the migration-133 CHECK) logs
    /// `worker_host.rotation_tracked_ref_invalid` and also resolves to `None`
    /// — rotation is background hygiene and must not wedge on drift. The one
    /// shared resolver keeps candidate selection and replacement provisioning
    /// on the same snapshot semantics ([D55]).
    pub async fn resolve_tracked_image(
        &self,
        pool: &sqlx::PgPool,
        org_id: Uuid,
    ) -> Result<Option<(String, String)>, AppError> {
        let Some(tracked_ref) = self
            .tracked_org_default_base_image_ref(pool, org_id)
            .await?
        else {
            return Ok(None);
        };
        match validate_base_image_ref(&tracked_ref) {
            Ok(digest) => Ok(Some((tracked_ref, digest))),
            Err(e) => {
                tracing::error!(
                    target: "g5_audit",
                    event = "worker_host.rotation_tracked_ref_invalid",
                    org_id = %org_id,
                    tracked_ref,
                    error = e.as_str(),
                    "Tracked org default failed digest validation; treated as no tracked image"
                );
                Ok(None)
            }
        }
    }

    /// Refresh tracking orgs' `default_base_image_ref` from the digests this
    /// environment's deploy promoted (ontology [D52]).
    ///
    /// `entries` come from [`parse_promoted_base_images`]. Each `(name, ref)`
    /// pair updates every org whose `default_base_image_track` equals `name`
    /// and whose ref differs. Idempotent and race-safe across replicas.
    pub async fn seed_tracked_org_defaults(
        &self,
        pool: &sqlx::PgPool,
        entries: &[(String, String)],
    ) -> Result<u64, AppError> {
        let mut updated = 0u64;
        for (name, image_ref) in entries {
            let result = sqlx::query(
                "UPDATE orgs SET default_base_image_ref = $1 \
                 WHERE default_base_image_track = $2 \
                   AND default_base_image_ref IS DISTINCT FROM $1",
            )
            .bind(image_ref)
            .bind(name)
            .execute(pool)
            .await?;
            updated += result.rows_affected();
        }
        Ok(updated)
    }

    /// Adopt the environment's configured default for organizations that have
    /// never selected an image policy. An operator pin or track always wins;
    /// the conditional update also preserves a concurrent policy write.
    pub async fn reconcile_unconfigured_org_defaults(
        &self,
        pool: &sqlx::PgPool,
        image_track: &str,
        image_ref: &str,
    ) -> Result<u64, AppError> {
        let result = sqlx::query(
            "UPDATE orgs \
             SET default_base_image_track = $1, default_base_image_ref = $2 \
             WHERE default_base_image_track IS NULL \
               AND default_base_image_ref IS NULL",
        )
        .bind(image_track)
        .bind(image_ref)
        .execute(pool)
        .await?;
        Ok(result.rows_affected())
    }

    /// Best-effort trailing-hour host count for provisioning burst observability.
    pub async fn count_recent_hosts(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
    ) -> Result<i64, AppError> {
        let count = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*)
            FROM worker_hosts
            WHERE org_id = $1 AND created_at > now() - interval '1 hour'
            "#,
        )
        .bind(org_id)
        .fetch_one(db.conn())
        .await?;

        Ok(count)
    }

    /// List worker hosts and return the unpaginated total for the applied filter.
    pub async fn list_hosts(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        status: Option<WorkerHostStatus>,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<WorkerHost>, i64), AppError> {
        self.list_hosts_filtered(db, org_id, status, limit, offset, false)
            .await
    }

    /// Apply the active-host filter before counting and paginating, so audit
    /// history cannot crowd running hosts off the Workers page.
    pub async fn list_hosts_filtered(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        status: Option<WorkerHostStatus>,
        limit: i64,
        offset: i64,
        active_only: bool,
    ) -> Result<(Vec<WorkerHost>, i64), AppError> {
        let status_filter = status.map(|s| s.as_str().to_string());

        let total = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*)
            FROM worker_hosts
            WHERE org_id = $1 AND ($2::text IS NULL OR status = $2)
              AND (NOT $3::boolean OR status <> 'terminated')
            "#,
        )
        .bind(org_id)
        .bind(status_filter.as_deref())
        .bind(active_only)
        .fetch_one(db.conn())
        .await?;

        let mut hosts = sqlx::query_as::<_, WorkerHost>(
            r#"
            SELECT host_id, org_id, vpc_id, cloud_instance_id, instance_type, region,
                   kind, slots_total, slot_size_reported, slot_cpus, slot_mem_mib, scratch_disk_mib,
                   runtime_identity, capabilities,
                   (SELECT count(*) FROM workspace_vms AS vm
                    WHERE vm.host_id = worker_hosts.host_id
                      AND vm.status IN ('starting', 'running', 'stopping')) AS occupancy,
                   CASE
                     WHEN last_heartbeat IS NULL THEN 'never'
                     WHEN last_heartbeat < now() - interval '30 seconds' THEN 'stale'
                     ELSE 'live'
                   END AS heartbeat_state,
                   CASE WHEN last_heartbeat IS NULL THEN NULL
                     ELSE extract(epoch FROM (now() - last_heartbeat))::bigint
                   END AS heartbeat_age_secs,
                   kernel_version,
                   active_microvm_count, cache_disk_used_bytes, cache_disk_total_bytes,
                   cache_state, attestation_state, status, created_at, last_heartbeat,
                   NULL::timestamptz AS credential_expires_at,
                   0::bigint AS credential_generation,
                   drain_requested_at, drain_reason, drain_id, last_error,
                   provision_error_code, provision_retry_count, provision_next_retry_at,
                   base_image_ref, base_image_digest, base_image_source_kind,
                   base_image_source_value, runtime_schema, runtime_labels,
                   runtime_labels_refreshed_at, worker_profile, workspace_vm_limit,
                      created_by_user_id
            FROM worker_hosts
            WHERE org_id = $1 AND ($2::text IS NULL OR status = $2)
              AND (NOT $3::boolean OR status <> 'terminated')
            ORDER BY created_at DESC, host_id DESC
            LIMIT $4 OFFSET $5
            "#,
        )
        .bind(org_id)
        .bind(status_filter.as_deref())
        .bind(active_only)
        .bind(limit)
        .bind(offset)
        .fetch_all(db.conn())
        .await?;
        for host in &mut hosts {
            host.runtime_state = runtime_state(&host.runtime_identity).to_string();
        }

        Ok((hosts, total))
    }

    /// Fill each host's creator display name and email from `users`.
    ///
    /// Runs on the privileged system pool: user profiles live outside the
    /// tenant-table grants, so callers pass only hosts they already listed
    /// under an authorized org scope. Hosts without a recorded creator, or
    /// whose creator no longer exists, keep `None`.
    pub async fn attach_creator_profiles(
        &self,
        pool: &sqlx::PgPool,
        hosts: &mut [WorkerHost],
    ) -> Result<(), AppError> {
        let creator_ids: Vec<Uuid> = hosts
            .iter()
            .filter_map(|host| host.created_by_user_id)
            .collect();
        if creator_ids.is_empty() {
            return Ok(());
        }
        let creators = sqlx::query_as::<_, (Uuid, String, String)>(
            "SELECT id, display_name, email FROM users WHERE id = ANY($1)",
        )
        .bind(&creator_ids)
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|(id, name, email)| (id, (name, email)))
        .collect::<std::collections::HashMap<_, _>>();
        for host in hosts {
            if let Some((name, email)) = host.created_by_user_id.and_then(|id| creators.get(&id)) {
                host.created_by_display_name = Some(name.clone());
                host.created_by_email = Some(email.clone());
            }
        }
        Ok(())
    }

    /// Remove a host in the caller's transaction: refuse while it owns VMs
    /// unless forced, record the operator drain, release its VMs as forced
    /// losses when forced, and mark it terminated. The caller commits, then
    /// delivers the drain. `None` when no such host is visible.
    pub async fn remove_host(
        &self,
        agent_work: &dyn crate::services::hosts::workspace_vm_release::AgentWorkSettlement,
        db: &mut ScopedDb,
        org_id: Uuid,
        host_id: Uuid,
        actor_user_id: Uuid,
        force: bool,
    ) -> Result<Option<HostRemoval>, AppError> {
        // Removal changes what the capacity sweep decides from (supply, and
        // the floor's presence in the region), so it takes the organization
        // admission lock the sweep and placement take before any host row,
        // keeping one lock order with them (the sweep already holds it).
        super::host_capacity::lock_admission(db, org_id).await?;
        // Placement holds the host row while it puts a VM on the host, so with
        // the row locked first the custody count sees every VM placed here,
        // and no placement picks the host until removal commits.
        let visible =
            sqlx::query("SELECT 1 FROM worker_hosts WHERE org_id = $1 AND host_id = $2 FOR UPDATE")
                .bind(org_id)
                .bind(host_id)
                .fetch_optional(db.conn())
                .await?;
        if visible.is_none() {
            return Ok(None);
        }
        let active_vms = self.active_vm_count(db, org_id, host_id).await?;
        if active_vms > 0 && !force {
            return Err(AppError::from_error(g5_contracts::error::G5Error::Conflict {
                reason: format!(
                    "worker host {host_id} still owns {active_vms} active workspace VM(s); drain it or retry with force=true"
                ),
            }));
        }
        // A host rotation is draining records no operator drain; removal goes on.
        let drain = match self
            .request_drain(
                db,
                org_id,
                host_id,
                Uuid::new_v4(),
                if force {
                    "operator_force"
                } else {
                    "operator_delete"
                },
            )
            .await?
        {
            DrainRequest::Recorded(drain) => Some(drain),
            DrainRequest::RotationOwned | DrainRequest::Absent => None,
        };
        if force && active_vms > 0 {
            let recorded = crate::services::hosts::workspace_vm_release::record_forced_vm_loss(
                agent_work,
                db,
                org_id,
                host_id,
                actor_user_id,
            )
            .await?;
            if i64::try_from(recorded).unwrap_or(i64::MAX) != active_vms {
                return Err(AppError::from_error(
                    g5_contracts::error::G5Error::Conflict {
                        reason: "worker host occupancy changed during forced termination"
                            .to_string(),
                    },
                ));
            }
        }
        let Some((_, cloud_instance_id)) = self.deallocate_host(db, org_id, host_id).await? else {
            return Ok(None);
        };
        // An operator's removal (automatic retirement passes the nil actor)
        // ends the free-slot floor's standing in the region: hosts automatic
        // retirement removed no longer count as the region's presence, so the
        // floor cannot re-create a host an administrator just removed
        // (`g5.architecture.host_demand_capacity`).
        if actor_user_id != Uuid::nil() {
            sqlx::query(
                "UPDATE worker_hosts SET capacity_floor_presence = false
                 WHERE org_id = $1 AND capacity_floor_presence
                   AND region = (SELECT region FROM worker_hosts WHERE host_id = $2)",
            )
            .bind(org_id)
            .bind(host_id)
            .execute(db.conn())
            .await?;
        }
        Ok(Some(HostRemoval {
            drain,
            forced_vm_losses: if force { active_vms } else { 0 },
            cloud_instance_id,
        }))
    }

    /// Transition a visible worker host to `terminated`. Finalizing is a
    /// definite terminal event, so the host's reported inventory ends with it
    /// (`g5.runtime.placement.capacity`, issue #627 S5): the migration-048
    /// liveness trigger invalidates `reported_live_vm_ids` on the
    /// `last_heartbeat` write here, and the gauge is zeroed explicitly —
    /// phantom inventory from a dead host must never count toward occupancy
    /// or the free-slot floor.
    pub async fn deallocate_host(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        host_id: Uuid,
    ) -> Result<Option<(Uuid, Option<String>)>, AppError> {
        let updated = sqlx::query_as::<_, (Uuid, Option<String>)>(
            r#"
            UPDATE worker_hosts
            SET status = 'terminated', last_heartbeat = NULL,
                active_microvm_count = 0
            WHERE host_id = $1
              AND org_id = $2
              AND status IN ('provisioning', 'ready', 'unreachable', 'draining', 'terminated')
            RETURNING host_id, cloud_instance_id
            "#,
        )
        .bind(host_id)
        .bind(org_id)
        .fetch_optional(db.conn())
        .await?;

        Ok(updated)
    }

    /// Fetch the org-scoped host row needed to drive a runtime-label refresh
    /// (ontology [D43]). Returns `None` when no such host is visible in the org
    /// (RLS hides cross-org rows, and the explicit `org_id` predicate is defense
    /// in depth → the handler maps `None` to 404). Keyed on `(host_id, org_id)`.
    pub async fn get_host_for_runtime_refresh(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        host_id: Uuid,
    ) -> Result<Option<RuntimeRefreshHostRow>, AppError> {
        let row = sqlx::query_as::<_, RuntimeRefreshHostRow>(
            r#"
            SELECT base_image_ref
            FROM worker_hosts
            WHERE host_id = $1 AND org_id = $2
            "#,
        )
        .bind(host_id)
        .bind(org_id)
        .fetch_optional(db.conn())
        .await?;

        Ok(row)
    }

    /// Persist resolved runtime labels on the org-scoped host row and return the
    /// full updated [`WorkerHost`] (ontology [D43]). `schema` becomes
    /// `runtime_schema` (NULL when `None`); `labels` becomes `runtime_labels`;
    /// `runtime_labels_refreshed_at` is set to Postgres `now()` (not a client
    /// clock). Returns `None` when no matching host is visible in the org (→ 404).
    /// Keyed on `(host_id, org_id)`; the RETURNING column list is copied verbatim
    /// from `list_hosts`.
    pub async fn set_runtime_labels(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        host_id: Uuid,
        schema: Option<&str>,
        labels: &Value,
    ) -> Result<Option<WorkerHost>, AppError> {
        let updated = sqlx::query_as::<_, WorkerHost>(
            r#"
            UPDATE worker_hosts
            SET runtime_schema = $3,
                runtime_labels = $4,
                runtime_labels_refreshed_at = now()
            WHERE host_id = $1 AND org_id = $2
            RETURNING host_id, org_id, vpc_id, cloud_instance_id, instance_type, region,
                      kind, slots_total, slot_size_reported, slot_cpus, slot_mem_mib, scratch_disk_mib,
                      runtime_identity, capabilities,
                      (SELECT count(*) FROM workspace_vms AS vm
                       WHERE vm.host_id = worker_hosts.host_id
                         AND vm.status IN ('starting', 'running', 'stopping')) AS occupancy,
                      CASE
                        WHEN last_heartbeat IS NULL THEN 'never'
                        WHEN last_heartbeat < now() - interval '30 seconds' THEN 'stale'
                        ELSE 'live'
                      END AS heartbeat_state,
                      CASE WHEN last_heartbeat IS NULL THEN NULL
                        ELSE extract(epoch FROM (now() - last_heartbeat))::bigint
                      END AS heartbeat_age_secs,
                      kernel_version,
                      active_microvm_count, cache_disk_used_bytes, cache_disk_total_bytes,
                      cache_state, attestation_state, status, created_at, last_heartbeat,
                      NULL::timestamptz AS credential_expires_at,
                      0::bigint AS credential_generation,
                      drain_requested_at, drain_reason, drain_id, last_error,
                      provision_error_code, provision_retry_count, provision_next_retry_at,
                      base_image_ref, base_image_digest, base_image_source_kind,
                      base_image_source_value, runtime_schema, runtime_labels,
                      runtime_labels_refreshed_at, worker_profile, workspace_vm_limit,
                      created_by_user_id
            "#,
        )
        .bind(host_id)
        .bind(org_id)
        .bind(schema)
        .bind(labels)
        .fetch_optional(db.conn())
        .await?
        .map(|mut host: WorkerHost| {
            host.runtime_state = runtime_state(&host.runtime_identity).to_string();
            host
        });

        Ok(updated)
    }
}

/// `now` is the DATABASE clock of the caller's transaction, not the app
/// clock: refusal expiries are written with SQL `now()`, so judging them
/// against a skewed app clock could resurrect or extend a refusal
/// (`g5.runtime.placement.capacity`, issue #627 S17).
pub(crate) fn merged_excluded_host_ids(
    requested: &[Uuid],
    start_intent: &Value,
    now: DateTime<Utc>,
) -> Vec<Uuid> {
    let mut excluded = requested.to_vec();
    if let Some(persisted) = start_intent
        .get("excluded_host_ids")
        .and_then(Value::as_array)
    {
        for value in persisted {
            if let Some(host_id) = value.as_str().and_then(|value| Uuid::parse_str(value).ok()) {
                if !excluded.contains(&host_id) {
                    excluded.push(host_id);
                }
            }
        }
    }
    if let Some(refusals) = start_intent.get("host_refusals").and_then(Value::as_object) {
        for (id, until) in refusals {
            if until
                .as_str()
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .is_some_and(|until| until > now)
            {
                if let Ok(id) = Uuid::parse_str(id) {
                    if !excluded.contains(&id) {
                        excluded.push(id);
                    }
                }
            }
        }
    }
    excluded
}

#[cfg(test)]
mod promoted_base_image_tests {
    use super::{merged_excluded_host_ids, parse_promoted_base_images};
    use chrono::Utc;
    use serde_json::json;
    use uuid::Uuid;

    fn digest_ref(name: &str) -> String {
        format!("reg.example.com/g5/{name}@sha256:{}", "a".repeat(64))
    }

    #[test]
    fn parses_names_from_valid_refs_and_skips_invalid_entries() {
        let raw = format!(
            " {} ,not-a-ref, ,{},reg.example.com/img:tag-only",
            digest_ref("g5d-pairsoft-preview"),
            digest_ref("g5d-pnc-bankworkflow"),
        );
        let entries = parse_promoted_base_images(&raw);
        assert_eq!(
            entries.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(),
            vec!["g5d-pairsoft-preview", "g5d-pnc-bankworkflow"],
        );
        assert!(entries.iter().all(|(_, r)| r.contains("@sha256:")));
    }

    #[test]
    fn parses_explicit_tracks_and_keeps_first_duplicate() {
        let first = digest_ref("g5d-stable");
        let duplicate = digest_ref("g5d-next");
        let raw =
            format!("current={first},CURRENT={duplicate},current={duplicate},next_1={duplicate}");

        assert_eq!(
            parse_promoted_base_images(&raw),
            vec![
                ("current".to_string(), first),
                ("next_1".to_string(), duplicate),
            ]
        );
    }

    #[test]
    fn empty_and_blank_inputs_yield_no_entries() {
        assert!(parse_promoted_base_images("").is_empty());
        assert!(parse_promoted_base_images(" , ,, ").is_empty());
    }

    #[test]
    fn placement_combines_request_and_persisted_exclusions_without_duplicates() {
        let requested = Uuid::new_v4();
        let persisted = Uuid::new_v4();
        let exclusions = merged_excluded_host_ids(
            &[requested],
            &json!({
                "excluded_host_ids": [
                    requested.to_string(),
                    persisted.to_string(),
                    "invalid-host-id"
                ]
            }),
            Utc::now(),
        );
        assert_eq!(exclusions, vec![requested, persisted]);
    }

    #[test]
    fn host_refusals_expire_on_the_supplied_clock() {
        // Refusal expiry is judged on the DATABASE clock passed by the caller
        // (issue #627 S17): the same intent reads as refused or expired purely
        // by the clock, never by the app host's skew.
        let refused = Uuid::new_v4();
        let until = Utc::now();
        let intent = json!({
            "host_refusals": { refused.to_string(): until.to_rfc3339() }
        });
        let before = until - chrono::TimeDelta::seconds(10);
        let after = until + chrono::TimeDelta::seconds(10);
        assert_eq!(
            merged_excluded_host_ids(&[], &intent, before),
            vec![refused],
            "a refusal still in the future on the recording clock excludes the host"
        );
        assert!(
            merged_excluded_host_ids(&[], &intent, after).is_empty(),
            "a refusal past expiry on the recording clock no longer excludes the host"
        );
    }
}
