//! Stale-host detector loop (ontology [D65]/[D66], issues #782/#783/#1464/#1880).
//!
//! Third background loop beside the reconciler ([D20]) and rotation ([D47]).
//! Its invariant: the set of live GCE instances and the set of live
//! `worker_hosts` rows converge — in BOTH directions.
//!
//! - **Pass 1 (#782, never-registered reap):** a `provisioning` row whose
//!   instance is linked and RUNNING but whose `g5d` never registered (no
//!   heartbeat, bring-up lease lapsed) is terminated with
//!   `provision_error_code='never_registered'` after a grace window, its
//!   instance best-effort deleted, and replaced ONCE (a second consecutive
//!   never-registered host in the same replacement chain means a broken
//!   image — no further auto-replacement; human decision 547a4e7d).
//! - **Pass 2 (#783, silent-ready reap):** a `ready` row whose
//!   `last_heartbeat` is stale past the threshold drains, finalizes, and is
//!   replaced through the [D47] rotation machinery — capacity self-heals.
//! - **Pass 3 (#1464, cloud-orphan sweep):** the REVERSE direction — list
//!   `g5-host-*` instances, join back to rows, and reap instances whose row
//!   is terminated/absent for a SUSTAINED wall-clock window
//!   (`cloud_orphan_sightings`; the #1512 "no one-shot kills" rule applied
//!   cloud-side).
//! - **Retry pass ([D66], #1880):** rows parked on
//!   `provision_error_code='zone_capacity_exhausted'` are re-provisioned at
//!   their scheduled time — same region first, then the configured
//!   North-America fallback basket — until the 30-minute budget lapses.
//!
//! Like rotation — and unlike the reconciler — the detector terminates
//! instances and provisions replacements, so double execution is not benign:
//! each tick is single-flight behind a dedicated advisory lock.
//!
//! Opt-in: `G5_STALE_HOST_DETECTOR_ENABLED` defaults to false; the retry pass
//! additionally requires `G5_PROVISION_RETRY_ENABLED`.
//!
//! Contract:
//! `.semlift/specs/software_architecture/g5-api/worker_hosts/cloud_provider.semlift.md`
//! [D65], [D66]; observability [D35]; migration 228.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::bringup::{
    bring_up_cloud_resources, host_disk_allocation, provision_retry_enabled,
    resolve_fallback_regions, resolve_provision_retry_budget_secs, BringUpCloudResourcesParams,
};
use super::rotation::{
    forced_quiesce, provision_replacement, teardown_and_finalize, ReplacementOutcome,
    RotationHostRow, ROTATION_HOST_ROW_COLUMNS,
};
use crate::db::maintenance::{
    scoped_db_for_maintenance_org, TenantMaintenanceDeps, SYSTEM_MAINTENANCE_ACTOR_UUID,
};
use crate::errors::AppError;
use crate::services::AppState;

/// Master switch — the detector is opt-in (`false` unless truthy). Enable on
/// dev first (ontology [D65]).
pub const STALE_DETECTOR_ENABLED_ENV: &str = "G5_STALE_HOST_DETECTOR_ENABLED";
/// Loop tick interval (seconds).
pub const STALE_DETECTOR_INTERVAL_ENV: &str = "G5_STALE_DETECTOR_INTERVAL_SECS";
/// Default tick: 5 minutes.
pub const DEFAULT_STALE_DETECTOR_INTERVAL_SECS: u64 = 300;
/// Floor for the tick interval.
pub const MIN_STALE_DETECTOR_INTERVAL_SECS: u64 = 60;
/// Pass 1 never-registered grace (seconds past `created_at`).
pub const STALE_PROVISIONING_GRACE_ENV: &str = "G5_STALE_PROVISIONING_GRACE_SECS";
/// Default Pass 1 grace: 45 minutes — well past healthy bring-up (2–5 min)
/// plus slow image pulls.
pub const DEFAULT_STALE_PROVISIONING_GRACE_SECS: u64 = 2_700;
/// Floor for the Pass 1 grace.
pub const MIN_STALE_PROVISIONING_GRACE_SECS: u64 = 600;
/// Pass 2 silent-ready threshold (seconds past `last_heartbeat`).
pub const STALE_READY_HEARTBEAT_ENV: &str = "G5_STALE_READY_HEARTBEAT_SECS";
/// Default Pass 2 threshold: 20 minutes — ≫ the 30s heartbeat interval and ≫
/// any observed transient control-plane blip (#961 was ~4 min).
pub const DEFAULT_STALE_READY_HEARTBEAT_SECS: u64 = 1_200;
/// Floor for the Pass 2 threshold: keeps the detector far above rotation's
/// 5-minute liveness predicate so the two loops select disjoint rows.
pub const MIN_STALE_READY_HEARTBEAT_SECS: u64 = 600;
/// Pass 3 sustained-sighting window (seconds past `first_seen`).
pub const CLOUD_ORPHAN_GRACE_ENV: &str = "G5_CLOUD_ORPHAN_GRACE_SECS";
/// Default Pass 3 grace: 15 minutes — a DELETE-races-bring-up instance is
/// legitimately row-terminated for seconds while [D61] compensation may still
/// act; a single listing observation must never kill.
pub const DEFAULT_CLOUD_ORPHAN_GRACE_SECS: u64 = 900;
/// Floor for the Pass 3 grace.
pub const MIN_CLOUD_ORPHAN_GRACE_SECS: u64 = 120;

/// Advisory lock key ("STAL") — the detector terminates instances and
/// provisions replacements, so it must be single-flight across API replicas.
const STALE_HOST_ADVISORY_LOCK_KEY: i64 = 0x5354_414C;

/// The deterministic instance-name prefix ([D44]) the orphan sweep lists.
const HOST_INSTANCE_NAME_PREFIX: &str = "g5-host-";

/// Whether the detector loop is enabled (opt-in, [D65]).
#[must_use]
pub fn stale_detector_enabled() -> bool {
    std::env::var(STALE_DETECTOR_ENABLED_ENV)
        .map(|raw| {
            let v = raw.trim().to_ascii_lowercase();
            v == "1" || v == "true"
        })
        .unwrap_or(false)
}

fn resolve_secs(env: &str, default: u64, floor: u64) -> u64 {
    std::env::var(env)
        .ok()
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .map(|configured| configured.max(floor))
        .unwrap_or(default)
}

/// Resolve the loop tick interval, floored.
#[must_use]
pub fn resolve_interval() -> Duration {
    Duration::from_secs(resolve_secs(
        STALE_DETECTOR_INTERVAL_ENV,
        DEFAULT_STALE_DETECTOR_INTERVAL_SECS,
        MIN_STALE_DETECTOR_INTERVAL_SECS,
    ))
}

/// Resolved per-pass thresholds (seconds).
#[derive(Debug, Clone, Copy)]
struct DetectorThresholds {
    provisioning_grace: u64,
    ready_heartbeat: u64,
    orphan_grace: u64,
}

impl DetectorThresholds {
    fn resolve() -> Self {
        Self {
            provisioning_grace: resolve_secs(
                STALE_PROVISIONING_GRACE_ENV,
                DEFAULT_STALE_PROVISIONING_GRACE_SECS,
                MIN_STALE_PROVISIONING_GRACE_SECS,
            ),
            ready_heartbeat: resolve_secs(
                STALE_READY_HEARTBEAT_ENV,
                DEFAULT_STALE_READY_HEARTBEAT_SECS,
                MIN_STALE_READY_HEARTBEAT_SECS,
            ),
            orphan_grace: resolve_secs(
                CLOUD_ORPHAN_GRACE_ENV,
                DEFAULT_CLOUD_ORPHAN_GRACE_SECS,
                MIN_CLOUD_ORPHAN_GRACE_SECS,
            ),
        }
    }
}

/// Outcome of a single detector pass, returned so tests (and logs) can assert
/// exactly what changed without waiting on the spawned loop.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StaleHostPass {
    /// Orgs scanned this pass.
    pub orgs_scanned: usize,
    /// Pass 1: provisioning rows reaped as `never_registered` (#782).
    pub never_registered_reaped: usize,
    /// Pass 1: replacements provisioned under the replace-once rule.
    pub never_registered_replaced: usize,
    /// Pass 2: silent `ready` rows reaped + replaced (#783).
    pub silent_reaped: usize,
    /// Pass 3: orphaned instances reaped after a sustained sighting (#1464).
    pub orphans_reaped: usize,
    /// Pass 3: sightings stamped/refreshed this pass (not yet reaped).
    pub orphan_sightings: usize,
    /// Retry pass: stockout rows re-provisioned ([D66], #1880).
    pub retried: usize,
    /// Retry pass: rows whose retry budget lapsed this pass.
    pub retry_exhausted: usize,
    /// Orgs flagged `worker_host.org_zero_capacity` this pass.
    pub zero_capacity_orgs: usize,
    /// Per-org or per-instance failures (logged, retried next pass).
    pub errors: usize,
}

/// Run a single detector pass. Pub so integration tests can drive the state
/// machine deterministically. A no-op (zeroed [`StaleHostPass`]) unless the
/// detector is enabled AND cloud features are on AND a cloud backend is wired.
pub async fn detect_once(state: &Arc<AppState>) -> StaleHostPass {
    if !stale_detector_enabled() {
        return StaleHostPass::default();
    }
    if !state.cloud_features_enabled || state.cloud_provider.is_none() {
        return StaleHostPass::default();
    }

    // Single-flight across replicas (mirrors rotation's lock discipline).
    let mut lock_conn = match state.system_pool.acquire().await {
        Ok(conn) => conn,
        Err(e) => {
            tracing::warn!(error = %e, "stale-host detector: failed to acquire lock connection");
            return StaleHostPass::default();
        }
    };
    let locked: bool = match sqlx::query_scalar("SELECT pg_try_advisory_lock($1)")
        .bind(STALE_HOST_ADVISORY_LOCK_KEY)
        .fetch_one(&mut *lock_conn)
        .await
    {
        Ok(locked) => locked,
        Err(e) => {
            tracing::warn!(error = %e, "stale-host detector: advisory-lock acquisition failed");
            return StaleHostPass::default();
        }
    };
    if !locked {
        tracing::debug!("stale-host detector: another replica holds the lock — skipping pass");
        return StaleHostPass::default();
    }

    let pass = detect_all(state).await;

    if let Err(e) = sqlx::query_scalar::<_, bool>("SELECT pg_advisory_unlock($1)")
        .bind(STALE_HOST_ADVISORY_LOCK_KEY)
        .fetch_one(&mut *lock_conn)
        .await
    {
        tracing::warn!(error = %e, "stale-host detector: advisory-lock release failed (released on connection close)");
    }

    pass
}

async fn detect_all(state: &Arc<AppState>) -> StaleHostPass {
    let thresholds = DetectorThresholds::resolve();
    let deps = TenantMaintenanceDeps {
        system_pool: state.system_pool.clone(),
        tenant_pool: state.tenant_pool.clone(),
        authz: state.authz.clone(),
        repo_storage: state.repo_storage.clone(),
    };

    let org_ids: Vec<Uuid> =
        match sqlx::query_scalar::<_, Uuid>("SELECT id FROM orgs ORDER BY created_at ASC")
            .fetch_all(&state.system_pool)
            .await
        {
            Ok(ids) => ids,
            Err(e) => {
                tracing::warn!(error = %e, "stale-host detector: failed to list orgs");
                return StaleHostPass::default();
            }
        };

    let mut pass = StaleHostPass {
        orgs_scanned: org_ids.len(),
        ..StaleHostPass::default()
    };

    for org_id in org_ids {
        if let Err(e) = detect_org(state, &deps, org_id, thresholds, &mut pass).await {
            pass.errors += 1;
            tracing::warn!(error = %e, %org_id, "stale-host detector: org pass failed; retrying next tick");
        }
    }

    // Pass 3 is cross-org by nature (an orphan's row may belong to any org or
    // no org) and runs once per tick after the org passes.
    if let Err(e) = sweep_cloud_orphans(state, thresholds, &mut pass).await {
        pass.errors += 1;
        tracing::warn!(error = %e, "stale-host detector: cloud-orphan sweep failed; retrying next tick");
    }

    pass
}

async fn detect_org(
    state: &Arc<AppState>,
    deps: &TenantMaintenanceDeps,
    org_id: Uuid,
    thresholds: DetectorThresholds,
    pass: &mut StaleHostPass,
) -> Result<(), AppError> {
    reap_never_registered(state, deps, org_id, thresholds, pass).await?;
    reap_silent_ready(state, deps, org_id, thresholds, pass).await?;
    retry_stockout_rows(state, deps, org_id, pass).await?;
    alert_zero_capacity(deps, org_id, thresholds, pass).await?;
    Ok(())
}

/// Pass 1 ([D65], #782): reap ONE never-registered provisioning host per org
/// per tick, replace once per chain.
async fn reap_never_registered(
    state: &Arc<AppState>,
    deps: &TenantMaintenanceDeps,
    org_id: Uuid,
    thresholds: DetectorThresholds,
    pass: &mut StaleHostPass,
) -> Result<(), AppError> {
    let candidate: Option<RotationHostRow> = {
        let mut db = scoped_db_for_maintenance_org(deps, org_id).await?;
        sqlx::query_as::<_, RotationHostRow>(&format!(
            "SELECT {ROTATION_HOST_ROW_COLUMNS} \
             FROM worker_hosts h \
             WHERE h.org_id = $1 \
               AND h.status = 'provisioning' \
               AND h.cloud_instance_id IS NOT NULL \
               AND NOT h.slot_size_reported \
               AND cardinality(h.reported_live_vm_ids) = 0 \
               AND NOT EXISTS (SELECT 1 FROM workspace_vms vm \
                               WHERE vm.host_id = h.host_id \
                                 AND vm.status IN ('starting','running','stopping')) \
               AND h.provision_error_code IS NULL \
               AND (h.bringup_heartbeat_at IS NULL \
                    OR h.bringup_heartbeat_at < now() - interval '90 seconds') \
               AND h.created_at < now() - make_interval(secs => $2::double precision) \
             ORDER BY h.created_at ASC \
             LIMIT 1",
        ))
        .bind(org_id)
        .bind(thresholds.provisioning_grace as f64)
        .fetch_optional(db.conn())
        .await?
    };
    let Some(host) = candidate else {
        return Ok(());
    };
    let Some(expected_linkage) = host.cloud_instance_id.clone() else {
        return Ok(()); // unreachable per the selector; defensive
    };

    // One guarded UPDATE ([D65]): status CAS + expected-linkage CAS + the
    // publication predicate re-checked atomically — a qualified registration
    // landing between SELECT and UPDATE voids the kill. Legacy heartbeats
    // alone do not complete resource registration.
    let reaped = {
        let mut db = scoped_db_for_maintenance_org(deps, org_id).await?;
        let rows = sqlx::query(
            "UPDATE worker_hosts \
             SET status = 'terminated', last_heartbeat = NULL, \
                 provision_error_code = 'never_registered' \
             WHERE host_id = $1 AND status = 'provisioning' \
               AND cloud_instance_id = $2 AND NOT slot_size_reported \
               AND cardinality(reported_live_vm_ids) = 0 \
               AND NOT EXISTS (SELECT 1 FROM workspace_vms vm \
                               WHERE vm.host_id = worker_hosts.host_id \
                                 AND vm.status IN ('starting','running','stopping'))",
        )
        .bind(host.host_id)
        .bind(&expected_linkage)
        .execute(db.conn())
        .await?
        .rows_affected();
        if rows == 1 {
            db.commit().await?;
            true
        } else {
            false
        }
    };
    if !reaped {
        return Ok(());
    }

    // Best-effort instance teardown ([D23] posture): a failure leaves the
    // terminated row + running instance for Pass 3 to sweep.
    if let Some(cloud) = state.cloud_provider.as_ref() {
        if let Err(e) = cloud.terminate_instance(&expected_linkage).await {
            tracing::warn!(
                host_id = %host.host_id,
                error = %e,
                "never-registered reap: instance teardown failed; the cloud-orphan sweep will catch it"
            );
        }
    }

    // Replace once, then stop (human decision 547a4e7d): a predecessor in the
    // replacement chain that ALSO died never-registered means a broken
    // image/config — no further auto-replacement.
    let predecessor_never_registered: bool = {
        let mut db = scoped_db_for_maintenance_org(deps, org_id).await?;
        sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM worker_hosts prev \
                            WHERE prev.org_id = $1 \
                              AND prev.rotation_replaced_by = $2 \
                              AND prev.provision_error_code = 'never_registered')",
        )
        .bind(org_id)
        .bind(host.host_id)
        .fetch_one(db.conn())
        .await?
    };
    let replaced = if predecessor_never_registered {
        false
    } else {
        // Best-effort (CodeRabbit PR #2051): the reap is already committed —
        // a failed replacement must not abort the org pass and strand the
        // org without the rest of this tick's recovery work. The org's
        // capacity gap is surfaced by the zero-capacity alert either way.
        match provision_replacement(state, deps, org_id, &host, None).await {
            Ok(ReplacementOutcome::Provisioned { .. }) => {
                pass.never_registered_replaced += 1;
                true
            }
            // Deferral is audited by provision_replacement itself; it is a
            // truthful no-replacement, not an error (issue #627 S8).
            Ok(ReplacementOutcome::Deferred { .. }) => false,
            Err(e) => {
                pass.errors += 1;
                tracing::warn!(
                    host_id = %host.host_id,
                    error = %e,
                    "never-registered reap: replacement provisioning failed; org relies on the zero-capacity alert"
                );
                false
            }
        }
    };

    pass.never_registered_reaped += 1;
    tracing::warn!(
        target: "g5_audit",
        event = "worker_host.stale_never_registered",
        host_id = %host.host_id,
        org_id = %org_id,
        age_secs = (Utc::now() - host.created_at).num_seconds(),
        replaced,
        "Provisioning host never registered within the grace window; reaped ([D65] Pass 1, #782)"
    );
    Ok(())
}

/// Pass 2 ([D65], #783): reap ONE silent `ready` host per org per tick and
/// self-heal capacity through the [D47] rotation machinery.
async fn reap_silent_ready(
    state: &Arc<AppState>,
    deps: &TenantMaintenanceDeps,
    org_id: Uuid,
    thresholds: DetectorThresholds,
    pass: &mut StaleHostPass,
) -> Result<(), AppError> {
    let candidate: Option<(RotationHostRow, DateTime<Utc>)> = {
        let mut db = scoped_db_for_maintenance_org(deps, org_id).await?;
        // `unreachable` rows are the silent population in practice: the
        // registry flips `ready → unreachable` after 30s of heartbeat silence,
        // long before this threshold — a selector pinned to `ready` could
        // never match and left unreachable hosts outside every reap lane
        // (`g5.runtime.host.rotation`, issue #627 S1). Cloud ownership is
        // required like every other reap/rotation selector: a self-hosted
        // host is never automatically deleted
        // (`g5.architecture.host_idle_retirement`), and an UNLINKED cloud row
        // belongs to the orphan machinery, not this reap.
        let row: Option<RotationHostRow> = sqlx::query_as::<_, RotationHostRow>(&format!(
            "SELECT {ROTATION_HOST_ROW_COLUMNS} \
             FROM worker_hosts h \
             WHERE h.org_id = $1 \
               AND h.status IN ('ready','unreachable') \
               AND h.cloud_instance_id IS NOT NULL \
               AND h.last_heartbeat IS NOT NULL \
               AND h.last_heartbeat < now() - make_interval(secs => $2::double precision) \
             ORDER BY h.last_heartbeat ASC \
             LIMIT 1",
        ))
        .bind(org_id)
        .bind(thresholds.ready_heartbeat as f64)
        .fetch_optional(db.conn())
        .await?;
        match row {
            None => None,
            Some(row) => {
                let last_heartbeat: Option<DateTime<Utc>> = sqlx::query_scalar(
                    "SELECT last_heartbeat FROM worker_hosts WHERE host_id = $1",
                )
                .bind(row.host_id)
                .fetch_one(db.conn())
                .await?;
                last_heartbeat.map(|hb| (row, hb))
            }
        }
    };
    let Some((host, last_heartbeat)) = candidate else {
        return Ok(());
    };

    // Forced quiesce ([D54] shape): the host is silent, so there is no
    // teardown channel to preserve — CAS ready → draining, evict all sticky
    // holds with recycle notices, terminate every worker.
    if !forced_quiesce(state, deps, org_id, &host).await? {
        return Ok(()); // raced by a DELETE / another writer
    }

    // Self-heal: replacement through the [D47] machinery — UNLESS the silent
    // host was already paired by [D67] rotation (its replacement exists; a
    // second one would double-provision).
    if host.rotation_replaced_by.is_none() {
        // Best-effort (CodeRabbit PR #2051): the forced quiesce is committed;
        // a failed replacement must not leave the draining row unfinalized.
        // The row stays recoverable either way — rotation's resumability
        // sweep re-provisions a draining row with a NULL marker. A deferral
        // is audited by provision_replacement (issue #627 S8).
        match provision_replacement(state, deps, org_id, &host, None).await {
            Ok(ReplacementOutcome::Provisioned { .. } | ReplacementOutcome::Deferred { .. }) => {}
            Err(e) => {
                pass.errors += 1;
                tracing::warn!(
                    host_id = %host.host_id,
                    error = %e,
                    "silent-ready reap: replacement provisioning failed; rotation's draining sweep will retry"
                );
            }
        }
    }
    teardown_and_finalize(state, deps, org_id, &host).await?;

    pass.silent_reaped += 1;
    tracing::warn!(
        target: "g5_audit",
        event = "worker_host.stale_reaped",
        host_id = %host.host_id,
        org_id = %org_id,
        silent_secs = (Utc::now() - last_heartbeat).num_seconds(),
        "Silent ready host reaped and replaced ([D65] Pass 2, #783)"
    );
    Ok(())
}

/// Row shape for the [D66] retry pass.
#[derive(Debug, sqlx::FromRow)]
struct RetryRow {
    host_id: Uuid,
    region: String,
    instance_type: String,
    base_image_ref: Option<String>,
    provision_retry_count: i32,
}

/// Retry pass ([D66], #1880): re-provision due stockout rows — same region
/// first, then the fallback basket — within the budget; freeze past it.
async fn retry_stockout_rows(
    state: &Arc<AppState>,
    deps: &TenantMaintenanceDeps,
    org_id: Uuid,
    pass: &mut StaleHostPass,
) -> Result<(), AppError> {
    if !provision_retry_enabled() {
        return Ok(());
    }
    let Some(minter) = state.jwt_minter.as_ref() else {
        // No minter ⇒ no fresh host_jwt for the startup script; retrying
        // would boot a credential-less host.
        return Ok(());
    };
    let budget_secs = resolve_provision_retry_budget_secs();

    // Freeze rows whose budget lapsed while still scheduled: stop retrying,
    // keep the truthful terminal badge, alert via the zero-capacity check.
    let exhausted: Vec<Uuid> = {
        let mut db = scoped_db_for_maintenance_org(deps, org_id).await?;
        let rows = sqlx::query_scalar(
            "UPDATE worker_hosts SET provision_next_retry_at = NULL \
             WHERE org_id = $1 AND status = 'provisioning' AND capacity_start_intent_id IS NULL \
               AND NOT capacity_floor_claim \
               AND cloud_instance_id IS NULL \
               AND provision_error_code = 'zone_capacity_exhausted' \
               AND provision_next_retry_at IS NOT NULL \
               AND created_at <= now() - make_interval(secs => $2::double precision) \
             RETURNING host_id",
        )
        .bind(org_id)
        .bind(budget_secs as f64)
        .fetch_all(db.conn())
        .await?;
        db.commit().await?;
        rows
    };
    for host_id in &exhausted {
        pass.retry_exhausted += 1;
        tracing::warn!(
            target: "g5_audit",
            event = "worker_host.provision_retry_exhausted",
            host_id = %host_id,
            org_id = %org_id,
            budget_secs,
            "Stockout retry budget exhausted; row keeps its terminal badge ([D66])"
        );
    }

    let due: Vec<RetryRow> = {
        let mut db = scoped_db_for_maintenance_org(deps, org_id).await?;
        sqlx::query_as::<_, RetryRow>(
            "SELECT host_id, region, instance_type, base_image_ref, provision_retry_count \
             FROM worker_hosts \
             WHERE org_id = $1 AND status = 'provisioning' AND capacity_start_intent_id IS NULL \
               AND NOT capacity_floor_claim \
               AND cloud_instance_id IS NULL \
               AND provision_error_code = 'zone_capacity_exhausted' \
               AND provision_next_retry_at IS NOT NULL \
               AND provision_next_retry_at <= now() \
               AND created_at > now() - make_interval(secs => $2::double precision) \
             ORDER BY provision_next_retry_at ASC \
             LIMIT 10",
        )
        .bind(org_id)
        .bind(budget_secs as f64)
        .fetch_all(db.conn())
        .await?
    };

    // Cross-region fallback targets only regions placement will use
    // (`g5.runtime.host.cloud-linkage`, issue #627 S30): placement serves
    // exactly the org's configured `worker_host_region`, so a host retried
    // into any other basket region would be paid capacity no Start can ever
    // land on. The basket keeps expressing the operator's jurisdiction limit;
    // placement policy filters it.
    let placement_region: String = {
        let mut db = scoped_db_for_maintenance_org(deps, org_id).await?;
        sqlx::query_scalar(
            "SELECT COALESCE((SELECT value #>> '{}' FROM configs \
                WHERE org_id = $1 AND scope = 'org' AND scope_id = $1 \
                  AND key = 'worker_host_region'), 'us-central1')",
        )
        .bind(org_id)
        .fetch_one(db.conn())
        .await?
    };
    let fallback_regions: Vec<String> = resolve_fallback_regions()
        .into_iter()
        .filter(|region| *region == placement_region)
        .collect();
    for row in due {
        // Attempt numbering: the claim below increments the count, so THIS
        // attempt is count + 1. Attempts 1–2 stay in the row's region (the
        // [D59] zone fallback already covered its siblings); attempts 3+
        // walk the (placement-filtered) basket round-robin.
        let attempt = row.provision_retry_count.saturating_add(1);
        let target_region = if attempt <= 2 || fallback_regions.is_empty() {
            row.region.clone()
        } else {
            let idx = usize::try_from(attempt - 3).unwrap_or(0) % fallback_regions.len();
            fallback_regions[idx].clone()
        };

        // Claim: ONE guarded UPDATE re-checking the whole due predicate while
        // consuming the schedule AND the classification together and
        // retargeting the region — a concurrent attach/DELETE/claim makes
        // this a no-op. Consuming `provision_error_code` with the schedule
        // (`g5.runtime.host.cloud-linkage`, issue #627 S18) keeps the row
        // coherent for EVERY retry outcome: a later stockout re-classifies
        // and re-schedules; a later unclassified failure leaves a plain
        // unlinked provisioning row that ordinary reconciliation attaches or
        // bounds, instead of a stale badge no selector covers. The bring-up
        // lease is renewed IN the claim (issue #627 Codex re-review, the
        // managed-capacity claim's shape): the prior lease lapsed ≥ the
        // backoff ago, so without it the just-unclassified row would sit in
        // the orphan-abandonment window until the detached bring-up's first
        // stamp.
        let allocation = {
            let mut db = scoped_db_for_maintenance_org(deps, org_id).await?;
            // A missing/unreadable snapshot must leave the schedule and retry
            // budget intact. The allocation is immutable once recorded.
            let allocation = match host_disk_allocation(db.conn(), row.host_id).await {
                Ok(allocation) => allocation,
                Err(error) => {
                    tracing::warn!(%error, host_id = %row.host_id,
                        "manual provisioning retry deferred without consuming its schedule");
                    continue;
                }
            };
            let rows = sqlx::query(
                "UPDATE worker_hosts \
                 SET provision_retry_count = provision_retry_count + 1, \
                     provision_next_retry_at = NULL, \
                     provision_error_code = NULL, \
                     bringup_heartbeat_at = now(), \
                     region = $2 \
                 WHERE host_id = $1 AND status = 'provisioning' AND capacity_start_intent_id IS NULL \
                   AND NOT capacity_floor_claim \
                   AND cloud_instance_id IS NULL \
                   AND provision_error_code = 'zone_capacity_exhausted' \
                   AND provision_next_retry_at IS NOT NULL \
                   AND provision_next_retry_at <= now()",
            )
            .bind(row.host_id)
            .bind(&target_region)
            .execute(db.conn())
            .await?
            .rows_affected();
            if rows == 1 {
                db.commit().await?;
                Some(allocation)
            } else {
                None
            }
        };
        let Some(allocation) = allocation else {
            continue;
        };

        // Fresh host_jwt for the same host_id: multiple valid host tokens are
        // safe — the [D5] DB-liveness check gates them all.
        let host_jwt = minter.mint_host_jwt(row.host_id, org_id, state.host_jwt_ttl)?;

        pass.retried += 1;
        tracing::info!(
            target: "g5_audit",
            event = "worker_host.provision_retry",
            host_id = %row.host_id,
            org_id = %org_id,
            attempt,
            region = %target_region,
            "Re-attempting stockout-failed provisioning ([D66], #1880)"
        );

        if let Some(cloud) = state.cloud_provider.clone() {
            let state_for_bringup = state.clone();
            let instance_type = row.instance_type.clone();
            let base_image_ref = row.base_image_ref.clone();
            let host_id = row.host_id;
            let region = target_region.clone();
            let scratch_disk_mib = allocation.vm_disk_mib as i32;
            let max_microvms = allocation.slots;
            let boot_disk_gb = allocation.disk_gib;
            // Detached like every other bring-up caller ([D22]): the [D60]
            // lease stamps while it runs; a failure re-classifies and
            // re-schedules through the shared branch in bringup.rs.
            let _bringup_handle = tokio::spawn(async move {
                bring_up_cloud_resources(BringUpCloudResourcesParams {
                    state: state_for_bringup.as_ref(),
                    cloud: cloud.as_ref(),
                    host_id,
                    org_id,
                    instance_type: &instance_type,
                    region: &region,
                    host_jwt: &host_jwt,
                    base_image_ref: base_image_ref.as_deref(),
                    created_by: SYSTEM_MAINTENANCE_ACTOR_UUID,
                    max_microvms,
                    scratch_disk_mib,
                    boot_disk_gb,
                    host_disk_gib: allocation.host_disk_gib,
                })
                .await;
            });
        }
    }
    Ok(())
}

/// Zero-capacity alert ([D65]): an org that INTENDS capacity (has any
/// non-terminated row) but has zero `ready` hosts and zero healthy in-flight
/// provisioning rows gets an error-level audit event — the pager hook the
/// Aug 6–14 incident lacked.
async fn alert_zero_capacity(
    deps: &TenantMaintenanceDeps,
    org_id: Uuid,
    thresholds: DetectorThresholds,
    pass: &mut StaleHostPass,
) -> Result<(), AppError> {
    let stuck: Option<(bool, Option<Uuid>, Option<String>)> = {
        let mut db = scoped_db_for_maintenance_org(deps, org_id).await?;
        let zero: bool = sqlx::query_scalar(
            "SELECT NOT EXISTS (SELECT 1 FROM worker_hosts h \
                                WHERE h.org_id = $1 AND h.status IN ('ready','draining')) \
                AND EXISTS (SELECT 1 FROM worker_hosts h \
                            WHERE h.org_id = $1 AND h.status <> 'terminated') \
                AND NOT EXISTS (SELECT 1 FROM worker_hosts h \
                                WHERE h.org_id = $1 AND h.status = 'provisioning' \
                                  AND (h.created_at > now() - make_interval(secs => $2::double precision) \
                                       OR h.provision_next_retry_at IS NOT NULL \
                                       OR h.bringup_heartbeat_at > now() - interval '90 seconds'))",
        )
        .bind(org_id)
        .bind(thresholds.provisioning_grace as f64)
        .fetch_one(db.conn())
        .await?;
        if !zero {
            None
        } else {
            let detail: Option<(Uuid, Option<String>)> = sqlx::query_as(
                "SELECT host_id, provision_error_code FROM worker_hosts \
                 WHERE org_id = $1 AND status = 'provisioning' \
                 ORDER BY created_at DESC LIMIT 1",
            )
            .bind(org_id)
            .fetch_optional(db.conn())
            .await?;
            Some((
                true,
                detail.as_ref().map(|(id, _)| *id),
                detail.and_then(|(_, code)| code),
            ))
        }
    };
    if let Some((_, stuck_host_id, stuck_error_code)) = stuck {
        pass.zero_capacity_orgs += 1;
        tracing::error!(
            target: "g5_audit",
            event = "worker_host.org_zero_capacity",
            org_id = %org_id,
            stuck_host_id = %stuck_host_id
                .map(|id| id.to_string())
                .unwrap_or_else(|| "<none>".to_string()),
            stuck_error_code = stuck_error_code.as_deref().unwrap_or("<none>"),
            "Org has zero worker capacity and no healthy in-flight provisioning ([D65])"
        );
    }
    Ok(())
}

/// Pass 3 ([D65], #1464): the reverse direction — list `g5-host-*` instances,
/// join back to rows, reap instances whose row stays terminated/absent for a
/// sustained wall-clock window.
async fn sweep_cloud_orphans(
    state: &Arc<AppState>,
    thresholds: DetectorThresholds,
    pass: &mut StaleHostPass,
) -> Result<(), AppError> {
    let Some(cloud) = state.cloud_provider.as_ref() else {
        return Ok(());
    };

    // Swept region set = the FULL placement allowlist ∪ every region any row
    // ever recorded ∪ the [D66] fallback basket. The allowlist base matters
    // for the hard-deleted-row corner (CodeRabbit PR #2051 round 2): an
    // orphan whose row is ABSENT contributes no region, so a db-derived set
    // could never list it — the allowlist is the complete space provisioning
    // can ever place instances in.
    let mut regions: Vec<String> = g5_contracts::types::worker_host::WORKER_HOST_REGION_ALLOWLIST
        .iter()
        .map(|r| (*r).to_string())
        .collect();
    let recorded: Vec<String> =
        sqlx::query_scalar("SELECT region FROM authz.list_worker_host_regions()")
            .fetch_all(&state.system_pool)
            .await
            .map_err(AppError::from)?;
    for region in recorded.into_iter().chain(resolve_fallback_regions()) {
        if !regions.contains(&region) {
            regions.push(region);
        }
    }

    let grace =
        chrono::Duration::seconds(i64::try_from(thresholds.orphan_grace).unwrap_or(i64::MAX));
    let mut seen: HashSet<String> = HashSet::new();

    for region in &regions {
        let listing = match cloud
            .list_instances_by_prefix(region, HOST_INSTANCE_NAME_PREFIX)
            .await
        {
            Ok(listing) => listing,
            Err(e) => {
                pass.errors += 1;
                tracing::warn!(
                    region = %region,
                    error = %e,
                    "cloud-orphan sweep: listing failed for region; skipping this pass"
                );
                continue;
            }
        };
        for inst in listing {
            if !matches!(
                inst.state,
                super::InstanceState::Running | super::InstanceState::Provisioning
            ) {
                // GCE is already tearing it down / transient — skip.
                continue;
            }
            if !seen.insert(format!("{}/{}", inst.zone, inst.name)) {
                continue; // already handled under another region's zone set
            }
            // Parse the trailing 36-char UUID from the deterministic name
            // ([D44]). A non-UUID suffix is skipped and logged — the MIG
            // fleet is retired (human decision 547a4e7d), so UUID parsing is
            // the only exclusion needed.
            let host_id = parse_host_id_suffix(&inst.name);
            let Some(host_id) = host_id else {
                tracing::debug!(
                    instance_name = %inst.name,
                    "cloud-orphan sweep: non-UUID name suffix; skipped"
                );
                continue;
            };

            // Row lookup under FORCE RLS via the [D61] definer read.
            if orphan_host_is_protected(state, host_id).await? {
                // Live control-plane property — never touched; clear any
                // stale sighting.
                sqlx::query(
                    "DELETE FROM cloud_orphan_sightings WHERE instance_name = $1 AND zone = $2",
                )
                .bind(&inst.name)
                .bind(&inst.zone)
                .execute(&state.system_pool)
                .await
                .map_err(AppError::from)?;
                continue;
            }

            // Terminated or absent row: stamp/refresh the sighting and reap
            // only after the window has been sustained.
            let first_seen: DateTime<Utc> = sqlx::query_scalar(
                "INSERT INTO cloud_orphan_sightings (instance_name, zone, host_id) \
                 VALUES ($1, $2, $3) \
                 ON CONFLICT (instance_name, zone) \
                 DO UPDATE SET last_seen = now() \
                 RETURNING first_seen",
            )
            .bind(&inst.name)
            .bind(&inst.zone)
            .bind(host_id)
            .fetch_one(&state.system_pool)
            .await
            .map_err(AppError::from)?;

            if Utc::now() - first_seen < grace {
                pass.orphan_sightings += 1;
                continue;
            }

            // Re-check status and custody immediately before deletion. A
            // protected observation clears the sighting and its grace window.
            if orphan_host_is_protected(state, host_id).await? {
                sqlx::query(
                    "DELETE FROM cloud_orphan_sightings WHERE instance_name = $1 AND zone = $2",
                )
                .bind(&inst.name)
                .bind(&inst.zone)
                .execute(&state.system_pool)
                .await
                .map_err(AppError::from)?;
                continue;
            }

            let linkage = format!("{}/{}", inst.zone, inst.name);
            match cloud.terminate_instance(&linkage).await {
                Ok(()) => {
                    sqlx::query(
                        "DELETE FROM cloud_orphan_sightings WHERE instance_name = $1 AND zone = $2",
                    )
                    .bind(&inst.name)
                    .bind(&inst.zone)
                    .execute(&state.system_pool)
                    .await
                    .map_err(AppError::from)?;
                    pass.orphans_reaped += 1;
                    tracing::warn!(
                        target: "g5_audit",
                        event = "worker_host.cloud_orphan_reaped",
                        instance_name = %inst.name,
                        zone = %inst.zone,
                        host_id = %host_id,
                        orphaned_secs = (Utc::now() - first_seen).num_seconds(),
                        "Orphaned instance with a terminated/absent row reaped ([D65] Pass 3, #1464)"
                    );
                }
                Err(e) => {
                    pass.errors += 1;
                    tracing::warn!(
                        instance_name = %inst.name,
                        zone = %inst.zone,
                        error = %e,
                        "cloud-orphan sweep: terminate failed; sighting retained for the next pass"
                    );
                }
            }
        }
    }

    // Prune sightings that stopped refreshing (instance gone on its own) so
    // the ledger stays bounded.
    sqlx::query(
        "DELETE FROM cloud_orphan_sightings \
         WHERE last_seen < now() - make_interval(secs => $1::double precision)",
    )
    .bind((thresholds.orphan_grace * 4) as f64)
    .execute(&state.system_pool)
    .await
    .map_err(AppError::from)?;

    Ok(())
}

/// A partition is not an orphan: unreachable hosts belong to the existing
/// custody-aware retirement path. Even a terminated row is protected until the
/// VM lifecycle owner has released every incarnation (including expired leases).
/// Read both predicates together at selection and again at delete admission.
/// Terminal hosts cannot accept placement/reconnect, and the VM foreign key
/// prevents a rowless host from holding custody, so no lease can appear after a
/// successful final check through the production lifecycle paths.
async fn orphan_host_is_protected(state: &Arc<AppState>, host_id: Uuid) -> Result<bool, AppError> {
    sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM authz.get_worker_host_cloud_linkage($1) \
                        WHERE status <> 'terminated') \
             OR EXISTS (SELECT 1 FROM workspace_vms WHERE host_id = $1)",
    )
    .bind(host_id)
    .fetch_one(&state.system_pool)
    .await
    .map_err(AppError::from)
}

/// Parse the trailing `-{uuid}` (or bare legacy `g5-host-{uuid}`) suffix of a
/// deterministic instance name ([D44]).
fn parse_host_id_suffix(name: &str) -> Option<Uuid> {
    if name.len() < 36 {
        return None;
    }
    let (head, tail) = name.split_at(name.len() - 36);
    if !(head.ends_with('-')) {
        return None;
    }
    Uuid::parse_str(tail).ok()
}

/// Spawn the detector loop. No-op spawn when disabled or no cloud backend is
/// wired. Honors the cancellation token for graceful shutdown.
pub fn spawn_stale_host_detector(
    state: Arc<AppState>,
    cancel: CancellationToken,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        if !stale_detector_enabled() {
            tracing::debug!(
                "stale-host detector: {STALE_DETECTOR_ENABLED_ENV} not set — not running"
            );
            return;
        }
        if state.cloud_provider.is_none() || !state.cloud_features_enabled {
            tracing::debug!("stale-host detector: no cloud backend wired — not running");
            return;
        }
        let interval_dur = resolve_interval();
        let thresholds = DetectorThresholds::resolve();
        tracing::info!(
            interval_secs = interval_dur.as_secs(),
            provisioning_grace_secs = thresholds.provisioning_grace,
            ready_heartbeat_secs = thresholds.ready_heartbeat,
            orphan_grace_secs = thresholds.orphan_grace,
            retry_enabled = provision_retry_enabled(),
            "stale-host detector loop started"
        );
        let mut ticker = tokio::time::interval(interval_dur);
        ticker.tick().await; // consume the immediate first tick
        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    tracing::info!("stale-host detector loop shutting down");
                    return;
                }
                _ = ticker.tick() => {
                    let pass = detect_once(&state).await;
                    if pass.never_registered_reaped > 0
                        || pass.silent_reaped > 0
                        || pass.orphans_reaped > 0
                        || pass.orphan_sightings > 0
                        || pass.retried > 0
                        || pass.retry_exhausted > 0
                        || pass.zero_capacity_orgs > 0
                        || pass.errors > 0
                    {
                        tracing::info!(
                            orgs_scanned = pass.orgs_scanned,
                            never_registered_reaped = pass.never_registered_reaped,
                            never_registered_replaced = pass.never_registered_replaced,
                            silent_reaped = pass.silent_reaped,
                            orphans_reaped = pass.orphans_reaped,
                            orphan_sightings = pass.orphan_sightings,
                            retried = pass.retried,
                            retry_exhausted = pass.retry_exhausted,
                            zero_capacity_orgs = pass.zero_capacity_orgs,
                            errors = pass.errors,
                            "stale-host detector pass complete"
                        );
                    }
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[serial_test::serial]
    fn detector_enabled_defaults_off_and_parses_truthy() {
        let prior = std::env::var(STALE_DETECTOR_ENABLED_ENV).ok();

        std::env::remove_var(STALE_DETECTOR_ENABLED_ENV);
        assert!(!stale_detector_enabled(), "detector must be opt-in");

        std::env::set_var(STALE_DETECTOR_ENABLED_ENV, "true");
        assert!(stale_detector_enabled());
        std::env::set_var(STALE_DETECTOR_ENABLED_ENV, "0");
        assert!(!stale_detector_enabled());

        match prior {
            Some(v) => std::env::set_var(STALE_DETECTOR_ENABLED_ENV, v),
            None => std::env::remove_var(STALE_DETECTOR_ENABLED_ENV),
        }
    }

    #[test]
    #[serial_test::serial]
    fn thresholds_default_and_floor() {
        let keys = [
            STALE_DETECTOR_INTERVAL_ENV,
            STALE_PROVISIONING_GRACE_ENV,
            STALE_READY_HEARTBEAT_ENV,
            CLOUD_ORPHAN_GRACE_ENV,
        ];
        let prior: Vec<(&str, Option<String>)> =
            keys.iter().map(|k| (*k, std::env::var(k).ok())).collect();
        for k in keys {
            std::env::remove_var(k);
        }

        let t = DetectorThresholds::resolve();
        assert_eq!(t.provisioning_grace, DEFAULT_STALE_PROVISIONING_GRACE_SECS);
        assert_eq!(t.ready_heartbeat, DEFAULT_STALE_READY_HEARTBEAT_SECS);
        assert_eq!(t.orphan_grace, DEFAULT_CLOUD_ORPHAN_GRACE_SECS);
        assert_eq!(
            resolve_interval(),
            Duration::from_secs(DEFAULT_STALE_DETECTOR_INTERVAL_SECS)
        );

        // Floors: the silent-ready threshold cannot drop into rotation's
        // 5-minute liveness band ([D65] disjointness).
        std::env::set_var(STALE_READY_HEARTBEAT_ENV, "60");
        let t = DetectorThresholds::resolve();
        assert_eq!(t.ready_heartbeat, MIN_STALE_READY_HEARTBEAT_SECS);

        for (k, v) in prior {
            match v {
                Some(v) => std::env::set_var(k, v),
                None => std::env::remove_var(k),
            }
        }
    }

    #[test]
    fn parse_host_id_suffix_handles_slugged_legacy_and_junk_names() {
        let id = Uuid::parse_str("e2be17df-df50-4471-9f4e-8bfb9ff6c5a2").unwrap();
        assert_eq!(
            parse_host_id_suffix(&format!("g5-host-some-org-{id}")),
            Some(id)
        );
        assert_eq!(parse_host_id_suffix(&format!("g5-host-{id}")), Some(id));
        assert_eq!(parse_host_id_suffix("g5-host-mig-fleet-0abc"), None);
        assert_eq!(parse_host_id_suffix("g5-host-"), None);
        assert_eq!(
            parse_host_id_suffix("g5-host-xe2be17dfdf5044719f4e8bfb9ff6c5a2"),
            None
        );
    }
}
