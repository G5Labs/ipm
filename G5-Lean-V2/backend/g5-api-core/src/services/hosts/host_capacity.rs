//! Demand-driven managed capacity. The existing Start queue remains the demand
//! authority; host rows are durable claims, and cloud work follows commit.
use std::collections::HashSet;
use std::sync::Arc;

use g5_authz::{ScopedDb, ScopedDbReopener};
use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;

use crate::cloud::bringup::{
    bring_up_cloud_resources, default_workspace_vm_limit, host_disk_allocation,
    parse_vcpus_from_instance_type, BringUpCloudResourcesParams,
};
use crate::cloud::{host_instance_name, InstanceState};
use crate::errors::AppError;
use crate::models::DEFAULT_MAX_RUNNING_VMS;
use crate::services::org_admin_settings;
use crate::services::{AppState, WorkerHostService};
use g5_contracts::types::worker_host::ProvisionWorkerHostRequest;

const MAX_PARALLEL: usize = 4;
/// Bound on the worker boot image lookup a sweep may wait for.
const BOOT_IMAGE_LOOKUP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
const MAX_ATTEMPTS: i32 = 3;

pub fn enabled(state: &AppState) -> bool {
    state.cloud_features_enabled
        && state.cloud_provider.is_some()
        && state.jwt_minter.is_some()
        && crate::cloud::stale_host::stale_detector_enabled()
}

#[derive(sqlx::FromRow)]
struct Demand {
    workspace_id: Uuid,
    intent_id: Uuid,
    owner_user_id: Uuid,
    region: String,
    intent: Value,
}

#[derive(sqlx::FromRow)]
struct Supply {
    host_id: Uuid,
    region: String,
    instance_type: String,
    status: String,
    slots_total: i32,
    workspace_vm_limit: Option<i32>,
    occupancy: i64,
    /// Published a sizing contract: the host registered at least once.
    registered: bool,
    healthy: bool,
    /// Placement-eligible runtime (`g5.runtime.placement.runtime-identity`):
    /// a healthy but stale host keeps its idle timer and may retire, yet
    /// covers no demand and is no reserve for retiring another host.
    runtime_current: bool,
    inflight: bool,
    paired: bool,
    replacing: bool,
    replacement_reserved: i64,
    replacement_overlap: bool,
    idle_expired: bool,
    cloud_instance_id: Option<String>,
    boot_image_id: Option<String>,
    automatic_capacity: bool,
    capacity_start_intent_id: Option<Uuid>,
    /// A claim the sweep made: for a queued Start (`capacity_start_intent_id`)
    /// or for the free-slot floor (`capacity_floor_claim`). Rotation
    /// replacements of automatic hosts are automatic but never claims.
    claim: bool,
    /// A claim made for the floor rather than for a Start.
    floor_claim: bool,
    /// Removed by automatic retirement while the floor was set
    /// (`capacity_floor_presence`), with its cloud cleanup unconfirmed or
    /// confirmed within the last hour. Such a host keeps its region present
    /// for the floor: retiring the region's last established host (an empty
    /// stale one) must not end the floor's authority to replace it, which the
    /// resource bound may have held back until that cleanup. An operator's
    /// removal clears the marker for the region (`remove_host`).
    retired_recently: bool,
    /// A young claim that has no cloud instance yet: in flight, retryable, or
    /// cooling down after a failed attempt. It covers the floor for a sweep.
    claim_pending: bool,
    cloud_deleted_at: Option<chrono::DateTime<chrono::Utc>>,
    provision_retry_count: i32,
    retry_due: bool,
    provision_error_code: Option<String>,
}

/// The org lock placement and quota writes share
/// (`org_admin_settings::lock_org_vm_quota`).
pub(crate) async fn lock_admission(db: &mut ScopedDb, org_id: Uuid) -> Result<(), AppError> {
    org_admin_settings::lock_org_vm_quota(db.conn(), org_id).await?;
    Ok(())
}

/// The region every Start of the organization is placed in, and the only
/// region the free-slot floor keeps supply in.
async fn demand_region(db: &mut ScopedDb, org_id: Uuid) -> Result<String, AppError> {
    sqlx::query_scalar(
        "SELECT COALESCE((SELECT value #>> '{}' FROM configs
            WHERE org_id = $1 AND scope = 'org' AND scope_id = $1 AND key = 'worker_host_region'),
            'us-central1')",
    )
    .bind(org_id)
    .fetch_one(db.conn())
    .await
    .map_err(Into::into)
}

async fn demand(
    db: &mut ScopedDb,
    org_id: Uuid,
    region: &str,
) -> Result<(i64, Vec<Demand>), AppError> {
    let quota: i64 = sqlx::query_scalar(
        "SELECT COALESCE((SELECT (value #>> '{}')::bigint FROM configs
         WHERE org_id = $1 AND scope = 'org' AND scope_id = $1 AND key = $2), $3)",
    )
    .bind(org_id)
    .bind(org_admin_settings::MAX_RUNNING_VMS_KEY)
    .bind(DEFAULT_MAX_RUNNING_VMS)
    .fetch_one(db.conn())
    .await?;
    let active: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM workspace_vms WHERE org_id = $1
        AND status IN ('starting','running','stopping')",
    )
    .bind(org_id)
    .fetch_one(db.conn())
    .await?;
    // Lock workspaces before VMs, as placement/Stop/transfer do. Recheck consent
    // after those locks; Stop cannot be overtaken by a claim based on an old read.
    sqlx::query(
        "SELECT w.id FROM workspaces w WHERE w.org_id = $1
        AND EXISTS (SELECT 1 FROM workspace_vms vm WHERE vm.workspace_id = w.id
            AND vm.start_intent ->> 'status' IN ('pending','queued_for_capacity'))
        ORDER BY w.id FOR UPDATE OF w",
    )
    .bind(org_id)
    .fetch_all(db.conn())
    .await?;
    let rows = sqlx::query_as::<_, Demand>(
        "SELECT vm.workspace_id, (vm.start_intent ->> 'id')::uuid AS intent_id,
            w.owner_user_id, $3::text AS region,
            vm.start_intent AS intent
         FROM workspace_vms vm JOIN workspaces w ON w.id = vm.workspace_id AND w.org_id = vm.org_id
         WHERE vm.org_id = $1 AND vm.status IN ('stopped','failed')
           AND vm.start_intent ->> 'status' IN ('pending','queued_for_capacity')
           AND (vm.start_intent ->> 'expires_at')::timestamptz > now()
           AND (vm.start_intent ->> 'owner_user_id')::uuid = w.owner_user_id
           AND (vm.start_intent ->> 'lifecycle_generation')::bigint = w.lifecycle_generation
           AND w.deleted_at IS NULL
           AND NOT EXISTS (SELECT 1 FROM workspace_ownership_transfers t
                           WHERE t.workspace_id = w.id AND t.status = 'pending')
         ORDER BY (vm.start_intent ->> 'created_at')::timestamptz, vm.workspace_id
         LIMIT $2 FOR UPDATE OF vm",
    )
    .bind(org_id)
    .bind((quota - active).max(0))
    .bind(region)
    .fetch_all(db.conn())
    .await?;
    Ok((quota.max(0), rows))
}

async fn supply(
    db: &mut ScopedDb,
    org_id: Uuid,
    queued: &[Demand],
    idle_host_retire_secs: i32,
) -> Result<Vec<Supply>, AppError> {
    // Lock relevant rows in identity order. Keep unconfirmed resources for the
    // bound and exhausted claims for live Starts, without locking all historical
    // audit rows on every sweep. No cloud calls under these locks.
    let intent_ids: Vec<_> = queued.iter().map(|d| d.intent_id).collect();
    sqlx::query_as::<_, Supply>(
        "SELECT h.host_id, h.region, h.instance_type, h.status, h.slots_total, h.workspace_vm_limit,
            (SELECT count(*) FROM workspace_vms vm WHERE vm.host_id = h.host_id
                AND vm.status IN ('starting','running','stopping')) +
            (SELECT count(*) FROM unnest(h.reported_live_vm_ids) live(id)
                WHERE NOT EXISTS (SELECT 1 FROM workspace_vms vm WHERE vm.host_id = h.host_id
                    AND vm.vm_id = live.id::text AND vm.status IN ('starting','running','stopping'))) AS occupancy,
            h.slot_size_reported AS registered,
            COALESCE(h.status = 'ready' AND h.slot_size_reported
                AND h.last_heartbeat >= now() - interval '30 seconds'
                AND h.reported_live_vm_ids IS NOT NULL AND h.drain_id IS NULL, false) AS healthy,
            (h.runtime_identity @> $4::jsonb) AS runtime_current,
            COALESCE((h.status = 'provisioning' AND h.provision_error_code IS NULL
                AND h.created_at > now() - interval '15 minutes'
                AND (h.cloud_instance_id IS NOT NULL OR h.bringup_heartbeat_at > now() - interval '90 seconds'
                    OR h.created_at > now() - interval '30 seconds'
                    OR ((h.capacity_start_intent_id IS NOT NULL OR h.capacity_floor_claim)
                        AND h.provision_retry_count = 0))), false) AS inflight,
            (h.rotation_replaced_by IS NOT NULL OR EXISTS (SELECT 1 FROM worker_hosts prev
                WHERE prev.rotation_replaced_by = h.host_id AND prev.status <> 'terminated')) AS paired,
            (h.rotation_replaced_by IS NOT NULL) AS replacing,
            (SELECT count(*) FROM workspace_vms vm JOIN worker_hosts prev ON prev.host_id = vm.host_id
                WHERE prev.rotation_replaced_by = h.host_id AND prev.status <> 'terminated'
                    AND vm.status IN ('starting','running','stopping')) AS replacement_reserved,
            EXISTS (SELECT 1 FROM worker_hosts prev WHERE prev.rotation_replaced_by = h.host_id
                AND prev.status <> 'terminated') AS replacement_overlap,
            COALESCE(h.idle_since < now() - make_interval(secs => $3), false) AS idle_expired,
            h.cloud_instance_id, h.boot_image_id, h.automatic_capacity, h.capacity_start_intent_id,
            (h.capacity_start_intent_id IS NOT NULL OR h.capacity_floor_claim) AS claim,
            h.capacity_floor_claim AS floor_claim,
            COALESCE(h.status = 'terminated' AND h.capacity_floor_presence
                AND (h.cloud_deleted_at IS NULL OR h.cloud_deleted_at > now() - interval '1 hour'), false) AS retired_recently,
            ((h.capacity_start_intent_id IS NOT NULL OR h.capacity_floor_claim)
                AND h.status = 'provisioning' AND h.cloud_instance_id IS NULL
                AND h.created_at > now() - interval '15 minutes') AS claim_pending,
            h.cloud_deleted_at,
            h.provision_retry_count, h.provision_error_code,
            (h.status = 'provisioning' AND h.cloud_instance_id IS NULL
                AND h.created_at > now() - interval '15 minutes'
                AND h.provision_retry_count < 3
                AND (h.provision_retry_count = 0 OR h.bringup_heartbeat_at IS NULL
                    OR h.bringup_heartbeat_at < now() - interval '90 seconds')
                AND (h.provision_error_code IS NULL OR h.provision_error_code = 'zone_capacity_exhausted')) AS retry_due
         FROM worker_hosts h WHERE h.org_id = $1
            AND (h.status <> 'terminated'
                OR (h.automatic_capacity AND h.cloud_deleted_at IS NULL)
                OR (h.capacity_floor_presence
                    AND (h.cloud_deleted_at IS NULL OR h.cloud_deleted_at > now() - interval '1 hour'))
                OR h.capacity_start_intent_id = ANY($2::uuid[]))
         ORDER BY h.host_id FOR UPDATE OF h")
        .bind(org_id).bind(intent_ids).bind(f64::from(idle_host_retire_secs))
        .bind(super::worker_host_service::current_runtime_identity())
        .fetch_all(db.conn()).await.map_err(Into::into)
}

fn available(h: &Supply) -> i64 {
    if h.replacing {
        return 0;
    }
    if h.healthy {
        if !h.runtime_current {
            return 0;
        }
        (i64::from(h.slots_total) - h.occupancy - h.replacement_reserved).max(0)
    } else if h.inflight {
        i64::from(h.workspace_vm_limit.unwrap_or_else(|| {
            default_workspace_vm_limit(
                parse_vcpus_from_instance_type(&h.instance_type).unwrap_or(4),
            ) as i32
        })) - h.replacement_reserved
    } else {
        return 0;
    }
    .max(0)
}

/// `now` is the sweep transaction's database clock: refusal expiries are
/// written with SQL `now()`, so they are judged on the same clock
/// (`g5.runtime.placement.capacity`, issue #627 S17).
fn accepts(h: &Supply, d: &Demand, now: chrono::DateTime<chrono::Utc>) -> bool {
    h.region == d.region
        && !super::worker_host_service::merged_excluded_host_ids(&[], &d.intent, now)
            .contains(&h.host_id)
}

/// Whether `region` holds fewer than `floor` free slots, counting `free` per
/// host (aligned with `hosts`) plus `extra` slots not in the snapshot, and
/// ignoring `exclude` entirely. With `require_present`, the region must also
/// hold an established host, one with a cloud instance or a registration, or
/// one automatic retirement removed within the last hour: the floor tops up a
/// region that already has a host and never creates a region's first host,
/// a claim that never launched is not one, and an operator removing every
/// host ends the floor there, pending claims included
/// (`g5.architecture.host_demand_capacity`). Automatic retirement of the host
/// a floor claim was made beside keeps the region present through the marker,
/// so recovery requires presence like creation does.
fn floor_short(
    hosts: &[Supply],
    free: &[i64],
    region: &str,
    floor: i32,
    extra: i64,
    exclude: Option<Uuid>,
    require_present: bool,
) -> bool {
    if floor <= 0 {
        return false;
    }
    let mut present = false;
    let mut total = extra;
    for (h, free) in hosts.iter().zip(free) {
        if h.region != region || Some(h.host_id) == exclude {
            continue;
        }
        present |= (h.status != "terminated" && (h.cloud_instance_id.is_some() || h.registered))
            || h.retired_recently;
        total += free;
    }
    (present || !require_present) && total < i64::from(floor)
}

/// Slots a claim of the organization's machine type brings once in flight,
/// as `available` will count it.
fn claim_slots(instance_type: &str) -> i64 {
    i64::from(default_workspace_vm_limit(
        parse_vcpus_from_instance_type(instance_type).unwrap_or(4),
    ))
}

#[derive(Default, Debug)]
pub struct CapacityPass {
    pub provisioned: usize,
    pub retired: usize,
    pub launches: Vec<Uuid>,
}

impl WorkerHostService {
    /// Decide demand and retirement atomically using placement's organization
    /// admission lock. Returned launches must run only after commit.
    /// `desired_boot_image` comes from [`sweep_boot_image`], resolved before
    /// this transaction opened so no lock and no transaction clock spans the
    /// cloud lookup.
    pub async fn reconcile_capacity(
        &self,
        state: &AppState,
        db: &mut ScopedDb,
        org_id: Uuid,
        desired_boot_image: Option<String>,
    ) -> Result<CapacityPass, AppError> {
        let mut pass = CapacityPass::default();
        if !enabled(state) {
            return Ok(pass);
        }
        lock_admission(db, org_id).await?;
        // One snapshot of the org's managed-capacity settings per sweep; their
        // writer takes the same admission lock.
        let settings = org_admin_settings::managed_capacity(db.conn(), org_id).await?;
        let region = demand_region(db, org_id).await?;
        let (quota, queued) = demand(db, org_id, &region).await?;
        let hosts = supply(db, org_id, &queued, settings.idle_host_retire_secs).await?;
        // One database clock for every refusal-expiry judgment this sweep
        // (issue #627 S17).
        let db_now: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("SELECT now()")
            .fetch_one(db.conn())
            .await?;
        if desired_boot_image.is_none() && hosts.iter().any(|h| h.healthy && !h.runtime_current) {
            // A stale host registered between the lock-free probe and the lock.
            // Decide nothing on a guess; the next sweep resolves the family first.
            tracing::debug!(%org_id, "stale host appeared after the boot-image check; deferring the sweep");
            return Ok(pass);
        }
        let pin_stale = hosts.iter().any(|h| {
            h.healthy
                && !h.runtime_current
                && h.boot_image_id.is_some()
                && h.boot_image_id == desired_boot_image
        });
        // A claim fulfilled by a host that came up stale is spent, whether or
        // not its Start is in this sweep's demand (the quota can hide it):
        // release the link so the Start claims again later and the host, once
        // retired, never reads as that Start's failed claim. A claim host that
        // registered and was then terminated (an operator removed it, or stale
        // retirement did) was fulfilled and releases its link the same way, so
        // the Start's successor can take the unique link. A claim that never
        // registered keeps its link: its attempts are exhaustion evidence.
        for h in hosts.iter().filter(|h| {
            h.capacity_start_intent_id.is_some()
                && ((h.healthy && !h.runtime_current) || (h.status == "terminated" && h.registered))
        }) {
            sqlx::query(
                "UPDATE worker_hosts SET capacity_start_intent_id = NULL WHERE host_id = $1",
            )
            .bind(h.host_id)
            .execute(db.conn())
            .await?;
        }
        let mut remaining: Vec<_> = hosts.iter().map(available).collect();
        // Count all unconfirmed resources, including failed/terminated rows.
        // Successors of live predecessors are separately bounded overlap.
        let mut resources = hosts
            .iter()
            .filter(|h| {
                h.automatic_capacity && h.cloud_deleted_at.is_none() && !h.replacement_overlap
            })
            .count() as i64;
        let mut launch_set = HashSet::new();
        let mut assigned = HashSet::new();
        // Free slots the claims of this sweep bring beyond the Starts they
        // serve; they are not in the snapshot but count toward the floor.
        let mut claimed_spare = 0i64;
        for d in &queued {
            let candidate = hosts
                .iter()
                .enumerate()
                .filter(|(i, h)| remaining[*i] > 0 && accepts(h, d, db_now))
                .min_by_key(|(_, h)| (!h.healthy, h.host_id));
            if let Some((i, h)) = candidate {
                remaining[i] -= 1;
                assigned.insert(h.host_id);
                if h.claim && h.retry_due {
                    launch_set.insert(h.host_id);
                }
                if !h.healthy {
                    set_wait(db, d, "preparing_capacity").await?;
                }
                continue;
            }
            // A live claim, or exhaustion evidence of one. A spent claim (released
            // above) and a host retired without a classified bring-up failure are
            // neither.
            if let Some(h) = hosts.iter().find(|h| {
                h.capacity_start_intent_id == Some(d.intent_id)
                    && !(h.healthy && !h.runtime_current)
                    && !(h.status == "terminated" && h.registered)
            }) {
                assigned.insert(h.host_id);
                // Identity reconciliation is independent of retry eligibility
                // (issue #627 Codex review): an assigned claim whose LAST
                // attempt created an instance but lost linkage persistence has
                // retry_due=false, yet its deterministic-name lookup must
                // still run — drive_claim attaches without consuming an
                // attempt and never inserts past the budget. A claim still
                // IN FLIGHT (fresh lease / just created) is left to its
                // running attempt.
                if accepts(h, d, db_now)
                    && h.status == "provisioning"
                    && h.cloud_instance_id.is_none()
                    && !h.inflight
                {
                    launch_set.insert(h.host_id);
                }
                let reason = if h.provision_error_code.as_deref() == Some("zone_capacity_exhausted")
                    && h.provision_retry_count < MAX_ATTEMPTS
                {
                    "capacity_stockout"
                } else if h.provision_error_code.is_some() || !h.retry_due && !h.inflight {
                    "capacity_failed"
                } else {
                    "preparing_capacity"
                };
                set_wait(db, d, reason).await?;
                continue;
            }
            if pin_stale {
                set_wait(db, d, "capacity_image_stale").await?;
                continue;
            }
            // Administrators turned growth off: no new claim; claims already
            // made above continue.
            if !settings.add_hosts {
                set_wait(db, d, "capacity_growth_disabled").await?;
                continue;
            }
            if resources >= quota.saturating_add(1) {
                set_wait(db, d, "capacity_limit").await?;
                continue;
            }
            // A bounded batch; later polls can claim the rest without serial boots.
            if pass.provisioned >= MAX_PARALLEL {
                continue;
            }
            let body: ProvisionWorkerHostRequest = serde_json::from_value(json!({
                "region": d.region, "instanceType": settings.instance_type
            }))
            .map_err(|e| {
                AppError::from_error(g5_contracts::error::G5Error::Internal {
                    message: e.to_string(),
                })
            })?;
            let prepared = self
                .prepare_host(state, db, org_id, Some(d.owner_user_id), body)
                .await?;
            let id = prepared.host.host_id;
            sqlx::query(
                "UPDATE worker_hosts SET automatic_capacity = true, capacity_start_intent_id = $2
                WHERE host_id = $1",
            )
            .bind(id)
            .bind(d.intent_id)
            .execute(db.conn())
            .await?;
            resources += 1;
            pass.provisioned += 1;
            assigned.insert(id);
            launch_set.insert(id);
            claimed_spare += claim_slots(&settings.instance_type) - 1;
            set_wait(db, d, "preparing_capacity").await?;
        }
        // The free-slot floor (`g5.architecture.host_demand_capacity`): once
        // the demand region has a host, keep `min_free_slots` current slots
        // free there so the next Start finds a slot instead of a boot. One
        // claim per sweep; the next sweep sees it in flight. A young claim
        // without an instance covers the floor meanwhile: relaunched while it
        // can still make progress, otherwise cooldown after a failed attempt,
        // so one failure never becomes a claim per sweep.
        if floor_short(
            &hosts,
            &remaining,
            &region,
            settings.min_free_slots,
            claimed_spare,
            None,
            true,
        ) {
            // A young claim not serving a Start this sweep covers the floor.
            if let Some(h) = hosts
                .iter()
                .find(|h| h.region == region && h.claim_pending && !assigned.contains(&h.host_id))
            {
                if h.retry_due {
                    launch_set.insert(h.host_id);
                }
            } else if pin_stale {
                tracing::debug!(%org_id, "free-slot floor waits: boot image pinned stale");
            } else if !settings.add_hosts {
                tracing::debug!(%org_id, "free-slot floor waits: automatic growth is off");
            } else if resources >= quota.saturating_add(1) {
                tracing::debug!(%org_id, "free-slot floor waits: resource bound reached");
            } else if pass.provisioned < MAX_PARALLEL {
                let body: ProvisionWorkerHostRequest = serde_json::from_value(json!({
                    "region": region, "instanceType": settings.instance_type
                }))
                .map_err(|e| {
                    AppError::from_error(g5_contracts::error::G5Error::Internal {
                        message: e.to_string(),
                    })
                })?;
                let prepared = self.prepare_host(state, db, org_id, None, body).await?;
                let id = prepared.host.host_id;
                sqlx::query(
                    "UPDATE worker_hosts SET automatic_capacity = true, capacity_floor_claim = true
                    WHERE host_id = $1",
                )
                .bind(id)
                .execute(db.conn())
                .await?;
                pass.provisioned += 1;
                launch_set.insert(id);
                tracing::info!(target: "g5_audit", event = "worker_host.floor_claimed", host_id = %id, %org_id, %region, floor = settings.min_free_slots);
            }
        }
        // Abandoned claims still need deterministic-identity reconciliation. This
        // does not launch cloud inserts without live demand or a short floor.
        for h in &hosts {
            if h.claim
                && h.status == "provisioning"
                && h.cloud_instance_id.is_none()
                && !assigned.contains(&h.host_id)
            {
                launch_set.insert(h.host_id);
            }
        }
        pass.launches = launch_set.into_iter().collect();
        pass.launches.sort_unstable();
        // Timer begins only at a healthy empty observation. Invalid observations
        // reset it; database guards also cover writers between these sweeps.
        for h in &hosts {
            let empty = h.healthy && h.occupancy == 0 && !h.paired;
            sqlx::query(
                "UPDATE worker_hosts SET idle_since = CASE WHEN $2
                THEN COALESCE(idle_since, now()) ELSE NULL END WHERE host_id = $1
                AND (($2 AND idle_since IS NULL) OR (NOT $2 AND idle_since IS NOT NULL))",
            )
            .bind(h.host_id)
            .bind(empty)
            .execute(db.conn())
            .await?;
        }
        let mut retired = HashSet::new();
        // Prefer retaining smaller hosts; leave the smallest qualifying reserve.
        // With removal off the idle timers above still run, so turning it back
        // on decides from accurate empty periods. An empty stale host of an
        // image other than the pin is no capacity at all: it retires at once,
        // without the delay, the reserve or the demand check, so a stale fleet
        // frees the resource bound for current hosts. A stale host of the
        // current boot image stays, past any idle delay, as the evidence that
        // holds growth above.
        let stale_now = |h: &&Supply| !h.runtime_current && h.boot_image_id != desired_boot_image;
        let mut candidates: Vec<_> = hosts
            .iter()
            .filter(|h| {
                settings.retire_idle_hosts
                    && h.healthy
                    && h.occupancy == 0
                    && ((h.runtime_current && h.idle_expired) || stale_now(h))
                    && !h.paired
                    && h.cloud_instance_id.is_some()
            })
            .collect();
        candidates.sort_by_key(|h| std::cmp::Reverse((h.slots_total, h.host_id)));
        for h in candidates {
            if h.runtime_current {
                // Do not shrink a region with queued demand, even refused demand.
                if queued.iter().any(|d| d.region == h.region) {
                    continue;
                }
                // The others must keep the floor (at least one free slot)
                // without this host, or removal and the floor would churn.
                let reserve: i64 = hosts
                    .iter()
                    .filter(|other| {
                        other.host_id != h.host_id
                            && other.region == h.region
                            && other.healthy
                            && other.runtime_current
                            && !other.paired
                            && !retired.contains(&other.host_id)
                    })
                    .map(available)
                    .sum();
                if reserve < i64::from(settings.min_free_slots.max(1)) {
                    continue;
                }
            }
            if self
                .remove_host(
                    state.agent_work.as_ref(),
                    db,
                    org_id,
                    h.host_id,
                    Uuid::nil(),
                    false,
                )
                .await?
                .is_none()
            {
                continue;
            }
            let drain_reason = if h.runtime_current {
                "automatic_idle"
            } else {
                "automatic_stale"
            };
            // While a floor is set, the retired host keeps its region present
            // for the floor (`floor_short`); an operator's removal ends that.
            sqlx::query(
                "UPDATE worker_hosts SET drain_reason = $2, capacity_floor_presence = $3
                WHERE host_id = $1",
            )
            .bind(h.host_id)
            .bind(drain_reason)
            .bind(settings.min_free_slots > 0)
            .execute(db.conn())
            .await?;
            retired.insert(h.host_id);
            pass.retired += 1;
            tracing::info!(target: "g5_audit", event = "worker_host.idle_retired", host_id = %h.host_id, %org_id, drain_reason);
        }
        Ok(pass)
    }
}

/// The boot image the fleet's worker family resolves to now: what a claim
/// would boot, and therefore the runtime it would carry. Not under any lock.
async fn resolve_desired_boot_image(state: &AppState) -> Result<String, AppError> {
    let cloud = state.cloud_provider.as_ref().ok_or_else(|| {
        AppError::from_error(g5_contracts::error::G5Error::Internal {
            message: "managed capacity needs a cloud provider to resolve the boot image".into(),
        })
    })?;
    // Lifecycle maintenance awaits each organization in turn, so a hanging
    // lookup must fail this sweep rather than stall every organization's
    // Start retries and reconciliation behind it.
    let identity = tokio::time::timeout(BOOT_IMAGE_LOOKUP_TIMEOUT, cloud.resolve_boot_image())
        .await
        .map_err(|_| {
            AppError::from_error(g5_contracts::error::G5Error::Internal {
                message: format!(
                    "resolve the worker boot image: no answer within {}s",
                    BOOT_IMAGE_LOOKUP_TIMEOUT.as_secs()
                ),
            })
        })?
        .map_err(|e| {
            AppError::from_error(g5_contracts::error::G5Error::Internal {
                message: format!("resolve the worker boot image: {e}"),
            })
        })?;
    Ok(identity.image_id)
}

/// The boot image the capacity sweep decides with, or `None` when no stale
/// registered host exists and the family decides nothing
/// (`g5.runtime.placement.runtime-identity`). The probe is its own short
/// transaction and the cloud lookup runs after it closed: no admission or
/// host lock is held, and the decision transaction opens afterwards with a
/// fresh clock for its freshness windows. An unresolvable family is an error
/// the caller maps to `capacity_configuration`.
pub async fn sweep_boot_image(
    state: &AppState,
    scope: &ScopedDbReopener,
    org_id: Uuid,
) -> Result<Option<String>, AppError> {
    let probe = scope.open().await?;
    let mut probe = probe;
    let stale_present: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM worker_hosts WHERE org_id = $1 AND status = 'ready'
            AND slot_size_reported AND NOT (runtime_identity @> $2::jsonb))",
    )
    .bind(org_id)
    .bind(super::worker_host_service::current_runtime_identity())
    .fetch_one(probe.conn())
    .await?;
    probe.rollback().await?;
    if !stale_present {
        return Ok(None);
    }
    resolve_desired_boot_image(state).await.map(Some)
}

async fn set_wait(db: &mut ScopedDb, d: &Demand, reason: &str) -> Result<(), AppError> {
    // The current wait reason replaces a stale one, but the host's actionable
    // refusal detail is carried forward (`g5.runtime.placement.capacity`,
    // issue #627 S7).
    sqlx::query(
        "UPDATE workspace_vms SET last_error = jsonb_build_object('reason', $2::text)
            || CASE WHEN last_error ? 'host_reason'
               THEN jsonb_build_object('host_reason', last_error -> 'host_reason')
               ELSE '{}'::jsonb END
        WHERE workspace_id = $1 AND start_intent ->> 'id' = $3",
    )
    .bind(d.workspace_id)
    .bind(reason)
    .bind(d.intent_id.to_string())
    .execute(db.conn())
    .await?;
    Ok(())
}

/// Called after ordinary queue retries. Errors are visible on the existing wait
/// surface and never change or renew Start consent.
pub async fn maintain(
    state: &Arc<AppState>,
    scope: ScopedDbReopener,
    org_id: Uuid,
) -> Result<CapacityPass, AppError> {
    if !enabled(state) {
        return Ok(CapacityPass::default());
    }
    let decided = match sweep_boot_image(state, &scope, org_id).await {
        Ok(desired_boot_image) => {
            let mut db = scope.open().await?;
            match state
                .worker_host_service
                .reconcile_capacity(state, &mut db, org_id, desired_boot_image)
                .await
            {
                Ok(pass) => {
                    db.commit().await?;
                    Ok(pass)
                }
                Err(error) => {
                    db.rollback().await?;
                    Err(error)
                }
            }
        }
        Err(error) => Err(error),
    };
    let pass = match decided {
        Ok(pass) => pass,
        Err(error) => {
            let mut db = scope.open().await?;
            sqlx::query("UPDATE workspace_vms SET last_error = jsonb_build_object('reason', 'capacity_configuration')
                WHERE org_id = $1 AND start_intent ->> 'status' = 'queued_for_capacity'
                AND last_error ->> 'reason' IS DISTINCT FROM 'org_vm_quota'")
                .bind(org_id).execute(db.conn()).await?;
            db.commit().await?;
            return Err(error);
        }
    };
    // Detached tasks outlive individual lifecycle polls. DB locks fence replicas
    // and cap parallelism; each claim survives a process restart before launch.
    for host_id in pass.launches.iter().copied() {
        let state = state.clone();
        let scope = scope.clone();
        let _launch = tokio::spawn(async move {
            if let Err(error) = drive_claim(&state, &scope, org_id, host_id).await {
                tracing::warn!(%error, %host_id, "automatic host claim recovery deferred");
            }
        });
    }
    let cleanup_state = state.clone();
    let _cleanup = tokio::spawn(async move {
        if let Err(error) = cleanup_retired(&cleanup_state, &scope, org_id).await {
            tracing::warn!(%error, %org_id, "automatic retirement cleanup deferred");
        }
    });
    Ok(pass)
}

async fn drive_claim(
    state: &Arc<AppState>,
    scope: &ScopedDbReopener,
    org_id: Uuid,
    host_id: Uuid,
) -> Result<(), AppError> {
    // Detached physical connection: cancellation closes it, releasing session locks
    // instead of returning a still-locked connection to the pool.
    let mut lock = state.system_pool.acquire().await?.detach();
    let acquired: bool =
        sqlx::query_scalar("SELECT pg_try_advisory_lock(hashtextextended($1, 821))")
            .bind(host_id.to_string())
            .fetch_one(&mut lock)
            .await?;
    if !acquired {
        return Ok(());
    }
    let mut lane = None;
    for n in 0..MAX_PARALLEL {
        let key = format!("{org_id}:{n}");
        let acquired: bool =
            sqlx::query_scalar("SELECT pg_try_advisory_lock(hashtextextended($1, 822))")
                .bind(key)
                .fetch_one(&mut lock)
                .await?;
        if acquired {
            lane = Some(n);
            break;
        }
    }
    if lane.is_none() {
        return Ok(());
    }
    let cloud = state
        .cloud_provider
        .as_ref()
        .expect("enabled managed capacity");
    let mut db = scope.open().await?;
    let row = sqlx::query(
        "SELECT region, instance_type, base_image_ref, scratch_disk_mib,
        workspace_vm_limit, boot_disk_gb, created_by_user_id, provision_retry_count,
        cloud_instance_id, status FROM worker_hosts WHERE org_id = $1 AND host_id = $2
        AND (capacity_start_intent_id IS NOT NULL OR capacity_floor_claim)",
    )
    .bind(org_id)
    .bind(host_id)
    .fetch_optional(db.conn())
    .await?;
    db.rollback().await?;
    let Some(row) = row else {
        return Ok(());
    };
    if row.try_get::<String, _>("status")? != "provisioning"
        || row
            .try_get::<Option<String>, _>("cloud_instance_id")?
            .is_some()
    {
        return Ok(());
    }
    let region: String = row.try_get("region")?;
    let slug: String = sqlx::query_scalar("SELECT slug FROM orgs WHERE id = $1")
        .bind(org_id)
        .fetch_one(&state.system_pool)
        .await?;
    // Always reconcile first, including crashes before the first launch. Both
    // historical names are searched, and lookup errors never mean absence.
    let mut name = host_instance_name(host_id, &slug);
    let mut found = cloud.get_instance_by_name(&region, &name).await;
    if matches!(found, Ok(None)) {
        name = host_instance_name(host_id, "");
        found = cloud.get_instance_by_name(&region, &name).await;
    }
    let found = match found {
        Ok(found) => found,
        Err(error) => {
            tracing::warn!(%error, %host_id, "capacity identity lookup deferred");
            return Ok(());
        }
    };
    if let Some(found) = found {
        if matches!(
            found.state,
            InstanceState::Running | InstanceState::Provisioning
        ) {
            let vpc = match cloud.ensure_vpc_for_org(org_id).await {
                Ok(vpc) => vpc,
                Err(_) => return Ok(()),
            };
            sqlx::query("SELECT authz.set_worker_host_cloud_refs($1, $2, $3)")
                .bind(host_id)
                .bind(vpc.vpc_id)
                .bind(format!("{}/{}", found.zone, name))
                .execute(&state.system_pool)
                .await?;
            return Ok(());
        }
        // A stopped GCE object still owns its name. Remove it before retrying,
        // but never infer deletion from its stopped state or a failed lookup.
        // Stopping/unknown states retain the existing conservative wait policy.
        if !matches!(found.state, InstanceState::Terminated) {
            return Ok(());
        }
        let instance = format!("{}/{}", found.zone, name);
        if let Err(error) = cloud.terminate_instance(&instance).await {
            tracing::warn!(%error, %host_id, "stopped automatic claim deletion deferred");
            return Ok(());
        }
        for name in [
            host_instance_name(host_id, &slug),
            host_instance_name(host_id, ""),
        ] {
            if !matches!(cloud.get_instance_by_name(&region, &name).await, Ok(None)) {
                return Ok(());
            }
        }
    }
    let mut db = scope.open().await?;
    lock_admission(&mut db, org_id).await?;
    // An existing claim proceeds even if growth was turned off since: only new
    // claims follow `add_hosts`.
    let settings = org_admin_settings::managed_capacity(db.conn(), org_id).await?;
    // The organization's demand region decides need; `region` above stays the
    // claim's own, which names its cloud instance and drives its bring-up.
    let floor_region = demand_region(&mut db, org_id).await?;
    let (_, queued) = demand(&mut db, org_id, &floor_region).await?;
    let hosts = supply(&mut db, org_id, &queued, settings.idle_host_retire_secs).await?;
    // Refusal expiries on this transaction's database clock (issue #627 S17).
    let db_now: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("SELECT now()")
        .fetch_one(db.conn())
        .await?;
    let Some(h) = hosts.iter().find(|h| h.host_id == host_id) else {
        return Ok(());
    };
    // Match FIFO demand against all OTHER supply. This claim may proceed only
    // if a compatible Start is still uncovered, including its original
    // refusal, or if the floor would be short without it while the claim is
    // still young (whatever it was claimed for).
    let mut free: Vec<_> = hosts
        .iter()
        .map(|h| {
            if h.host_id == host_id {
                0
            } else {
                available(h)
            }
        })
        .collect();
    let mut needed = false;
    for d in &queued {
        if let Some(i) = hosts
            .iter()
            .enumerate()
            .position(|(i, other)| free[i] > 0 && accepts(other, d, db_now))
        {
            free[i] -= 1;
        } else if accepts(h, d, db_now) {
            needed = true;
            break;
        }
    }
    // A Start's exhausted claim stays as that Start's exhaustion evidence
    // until the Start goes. A floor claim has no Start to be evidence for:
    // past its window, with no cloud instance, it is terminated even while
    // Starts wait, so it stops holding a resource against the bound and the
    // next sweep claims afresh for them.
    if needed && h.floor_claim && !h.claim_pending {
        needed = false;
    }
    if !needed && h.claim_pending && h.region == floor_region {
        needed = floor_short(
            &hosts,
            &free,
            &floor_region,
            settings.min_free_slots,
            0,
            Some(host_id),
            true,
        );
    }
    if !needed {
        sqlx::query(
            "UPDATE worker_hosts SET status = 'terminated', cloud_deleted_at = now(),
                capacity_start_intent_id = CASE WHEN provision_retry_count = 0
                    THEN NULL ELSE capacity_start_intent_id END
            WHERE host_id = $1 AND status = 'provisioning' AND cloud_instance_id IS NULL",
        )
        .bind(host_id)
        .execute(db.conn())
        .await?;
        db.commit().await?;
        return Ok(());
    }
    if !h.retry_due {
        return Ok(());
    }
    // Read the immutable allocation before consuming the attempt, in the
    // same transaction, so a lookup failure preserves retry eligibility.
    let allocation = host_disk_allocation(db.conn(), host_id).await?;
    // Consume an attempt before cloud work; a crash cannot reset its budget.
    sqlx::query(
        "UPDATE worker_hosts SET provision_retry_count = provision_retry_count + 1,
        bringup_heartbeat_at = now(), provision_error_code = NULL WHERE host_id = $1",
    )
    .bind(host_id)
    .execute(db.conn())
    .await?;
    db.commit().await?;
    let instance_type: String = row.try_get("instance_type")?;
    let max_microvms = allocation.slots;
    let scratch_disk_mib = allocation.vm_disk_mib as i32;
    let boot_disk_gb = allocation.disk_gib;
    let base_image_ref: Option<String> = row.try_get("base_image_ref")?;
    let actor = row
        .try_get::<Option<Uuid>, _>("created_by_user_id")?
        .unwrap_or(Uuid::nil());
    let host_jwt = state
        .jwt_minter
        .as_ref()
        .expect("enabled managed capacity")
        .mint_host_jwt(host_id, org_id, state.host_jwt_ttl)?;
    bring_up_cloud_resources(BringUpCloudResourcesParams {
        state,
        cloud: cloud.as_ref(),
        host_id,
        org_id,
        instance_type: &instance_type,
        region: &region,
        host_jwt: &host_jwt,
        base_image_ref: base_image_ref.as_deref(),
        created_by: actor,
        max_microvms,
        scratch_disk_mib,
        boot_disk_gb,
        host_disk_gib: allocation.host_disk_gib,
    })
    .await;
    Ok(())
}

async fn cleanup_retired(
    state: &AppState,
    scope: &ScopedDbReopener,
    org_id: Uuid,
) -> Result<(), AppError> {
    let mut cleanup_lock = state.system_pool.acquire().await?.detach();
    let acquired: bool =
        sqlx::query_scalar("SELECT pg_try_advisory_lock(hashtextextended($1, 823))")
            .bind(org_id.to_string())
            .fetch_one(&mut cleanup_lock)
            .await?;
    if !acquired {
        return Ok(());
    }
    let mut db = scope.open().await?;
    let rows = sqlx::query(
        "WITH selected AS (
            SELECT host_id FROM worker_hosts
            WHERE org_id = $1 AND status = 'terminated' AND cloud_deleted_at IS NULL
              AND (automatic_capacity OR drain_reason IN ('automatic_idle', 'automatic_stale'))
            ORDER BY cloud_delete_attempted_at NULLS FIRST, created_at, host_id
            LIMIT 4 FOR UPDATE SKIP LOCKED
        )
        UPDATE worker_hosts h SET cloud_delete_attempted_at = now()
        FROM selected WHERE h.org_id = $1 AND h.host_id = selected.host_id
        RETURNING h.host_id, h.cloud_instance_id, h.region",
    )
    .bind(org_id)
    .fetch_all(db.conn())
    .await?;
    db.commit().await?;
    let cloud = state
        .cloud_provider
        .as_ref()
        .expect("enabled managed capacity");
    for row in rows {
        let host_id: Uuid = row.try_get("host_id")?;
        state.preview_tunnels.terminate(&host_id).await;
        let instance: Option<String> = row.try_get("cloud_instance_id")?;
        let mut lock = state.system_pool.acquire().await?.detach();
        let acquired: bool =
            sqlx::query_scalar("SELECT pg_try_advisory_lock(hashtextextended($1, 821))")
                .bind(host_id.to_string())
                .fetch_one(&mut lock)
                .await?;
        if !acquired {
            continue;
        }
        let region: String = row.try_get("region")?;
        let slug: String = sqlx::query_scalar("SELECT slug FROM orgs WHERE id = $1")
            .bind(org_id)
            .fetch_one(&state.system_pool)
            .await?;
        let mut instances = HashSet::new();
        if let Some(instance) = instance {
            instances.insert(instance);
        }
        let mut confirmed = true;
        for name in [
            host_instance_name(host_id, &slug),
            host_instance_name(host_id, ""),
        ] {
            match cloud.get_instance_by_name(&region, &name).await {
                Ok(Some(found)) => {
                    instances.insert(format!("{}/{}", found.zone, name));
                }
                Ok(None) => {}
                Err(_) => {
                    confirmed = false;
                }
            }
        }
        for instance in instances {
            if let Err(error) = cloud.terminate_instance(&instance).await {
                tracing::warn!(%error, %host_id, "idle cloud deletion will retry");
                confirmed = false;
            } else if !matches!(
                cloud.get_instance_status(&instance).await,
                Ok(InstanceState::Terminated)
            ) {
                confirmed = false;
            }
        }
        if confirmed {
            let mut db = scope.open().await?;
            sqlx::query(
                "UPDATE worker_hosts SET cloud_deleted_at = now()
                WHERE host_id = $1 AND status = 'terminated'",
            )
            .bind(host_id)
            .execute(db.conn())
            .await?;
            db.commit().await?;
            tracing::info!(target: "g5_audit", event = "worker_host.idle_cloud_deleted", %host_id, %org_id);
        }
    }
    Ok(())
}
