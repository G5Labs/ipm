//! Image-aware workspace host rotation.
//!
//! Replacement hosts are paired before drain. A host reaches teardown only
//! after its workspace VM incarnations have completed the common stop path.

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::bringup::{bring_up_cloud_resources, BringUpCloudResourcesParams};
use crate::db::maintenance::{
    scoped_db_for_maintenance_org, TenantMaintenanceDeps, SYSTEM_MAINTENANCE_ACTOR_UUID,
};
use crate::errors::AppError;
use crate::services::worker_host_service::{ProvisionHostRecordParams, WorkerHostService};
use crate::services::AppState;

/// Master switch — rotation is opt-in (`false` unless the env var is a truthy
/// `1`/`true`). Enable on dev first, then prod (ontology [D47]).
pub const ROTATION_ENABLED_ENV: &str = "G5_HOST_ROTATION_ENABLED";
/// Required continuous idle window per `COALESCE(last_active_at, created_at)`.
pub const ROTATION_MIN_IDLE_ENV: &str = "G5_HOST_ROTATION_MIN_IDLE_SECS";
/// Default min idle window: 5h.
pub const DEFAULT_ROTATION_MIN_IDLE_SECS: u64 = 18_000;
/// Loop tick interval (seconds).
pub const ROTATION_INTERVAL_ENV: &str = "G5_HOST_ROTATION_INTERVAL_SECS";
/// Default tick: 5 minutes — rotation is a daily-cadence process; there is no
/// value in checking more often.
pub const DEFAULT_ROTATION_INTERVAL_SECS: u64 = 300;
/// Floor for the tick interval.
pub const MIN_ROTATION_INTERVAL_SECS: u64 = 60;

/// Fixed credential-cliff margin: once a host is older than
/// `host_jwt_ttl − CLIFF_MARGIN_SECS`, the [D54] force rotates it (idle or
/// busy, image-stale or current) — uncontrolled credential death at the TTL is
/// strictly worse (ontology [D54]/[D55]; deliberately a constant, not a knob).
const CLIFF_MARGIN_SECS: u64 = 7_200;
/// Busy hosts older than `host_jwt_ttl − OVERDUE_MARGIN_SECS` get a
/// `rotation_overdue_busy` warn. The [D54] force normally takes a busy host
/// at the cliff age (TTL − 2h), before this threshold (TTL − 1h) — the warn
/// may still fire on the same tick right before the force, and repeated
/// firings mean the forced path is unavailable or failing.
const OVERDUE_MARGIN_SECS: u64 = 3_600;

/// Advisory lock key ("ROTA") — rotation provisions instances, so unlike the
/// reconciler it must be single-flight across API replicas.
const ROTATION_ADVISORY_LOCK_KEY: i64 = 0x524F_5441;

/// Forced-lane drain deadline margin ([D67], human decision 547a4e7d:
/// TTL − 15 min): a paired past-cliff host whose replacement is still not
/// `ready` drains anyway once `created_at + host_jwt_ttl − 900s` passes — the
/// credential dies at the TTL regardless, and this trades the last 15 minutes
/// for ~105 minutes of retry runway after the TTL − 2h cliff.
const DRAIN_DEADLINE_MARGIN_SECS: u64 = 900;

/// Whether the rotation loop is enabled (opt-in, [D47]).
#[must_use]
pub fn rotation_enabled() -> bool {
    std::env::var(ROTATION_ENABLED_ENV)
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

/// Resolve the required idle window (seconds); 0 disables the window and
/// falls back to instantaneous idleness only.
#[must_use]
pub fn resolve_min_idle_secs() -> u64 {
    resolve_secs(ROTATION_MIN_IDLE_ENV, DEFAULT_ROTATION_MIN_IDLE_SECS, 0)
}

/// Resolve the loop tick interval, floored at [`MIN_ROTATION_INTERVAL_SECS`].
#[must_use]
pub fn resolve_interval() -> Duration {
    Duration::from_secs(resolve_secs(
        ROTATION_INTERVAL_ENV,
        DEFAULT_ROTATION_INTERVAL_SECS,
        MIN_ROTATION_INTERVAL_SECS,
    ))
}

/// Outcome of a single rotation pass, returned so tests (and logs) can assert
/// exactly what changed without waiting on the spawned loop.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RotationPass {
    /// Orgs scanned this pass.
    pub orgs_scanned: usize,
    /// Hosts fully rotated (quiesced + replaced + finalized).
    pub rotated: usize,
    /// Candidates dropped by the in-transaction busy re-verify.
    pub skipped_busy: usize,
    /// Interrupted rotations (rotation's own `draining` rows, which carry no
    /// drain id) finished by the resumability sweep.
    pub resumed: usize,
    /// Aged busy hosts flagged `rotation_overdue_busy`.
    pub overdue_busy: usize,
    /// Hosts past the credential cliff drained by the [D54]/[D67] forced path
    /// (replacement ready, or the TTL−15min drain deadline reached).
    pub forced: usize,
    /// Hosts newly PAIRED this pass ([D67] provision-then-drain): a
    /// replacement row + bring-up were provisioned while the old host stays
    /// `ready` and serving; the drain happens on a later tick once the
    /// replacement is `ready` (or the forced deadline passes).
    pub paired: usize,
    /// Pairings dissolved because the replacement failed terminally
    /// ([D67]): the failed replacement row was deallocated, the old row's
    /// marker cleared, and the old host re-enters candidacy.
    pub replacement_failed: usize,
    /// Pairings DEFERRED with an explicit reason (`g5.runtime.host.rotation`,
    /// issue #627): the candidate stays in service and in candidacy, nothing
    /// was provisioned, and no teardown was authorized. Surfaced as a durable
    /// `worker_host.rotation_pairing_deferred` audit warn per occurrence.
    pub pairing_deferred: usize,
    /// Per-org or per-host failures (logged, retried next pass).
    pub errors: usize,
}

/// Outcome of [`provision_replacement`] (`g5.runtime.host.rotation`, issue
/// #627 S1/S2/S8): pairing either provisioned a replacement or was deferred
/// with its reason. A deferral is a visible durable wait — callers must not
/// count it as replacement progress nor let it authorize successor-dependent
/// teardown. There is no silent no-op arm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReplacementOutcome {
    /// A replacement row + fresh host_jwt were provisioned and the pairing
    /// marker was stamped; the detached bring-up is running.
    Provisioned {
        /// The new replacement host row.
        replacement_id: Uuid,
    },
    /// Nothing was provisioned. `ineligible`: the old row left the eligible
    /// set (raced delete/retirement, an existing pairing, or a live
    /// predecessor still points at it). `cloud_cleanup_backlog`: undeleted
    /// automatic debris sits over the [`g5.architecture.host_demand_capacity`]
    /// resource bound — renewal overlap itself is NOT growth and is never
    /// deferred by retained live hosts or a lowered VM quota.
    Deferred {
        /// Machine-readable deferral reason for audit and tests.
        reason: &'static str,
    },
}

/// One rotation candidate / draining leftover pulled from `worker_hosts`.
/// `pub(crate)` so the stale-host detector ([D65], `cloud::stale_host`) can
/// reuse [`provision_replacement`] / [`teardown_and_finalize`] for its
/// silent-ready reap without duplicating the row shape.
#[derive(Debug, Clone, sqlx::FromRow)]
pub(crate) struct RotationHostRow {
    pub(crate) host_id: Uuid,
    pub(crate) region: String,
    pub(crate) instance_type: String,
    pub(crate) base_image_ref: Option<String>,
    pub(crate) base_image_digest: Option<String>,
    pub(crate) base_image_source_kind: Option<String>,
    pub(crate) base_image_source_value: Option<String>,
    /// Immutable outer boot image the host was pinned to at provision
    /// (`worker_hosts.boot_image_id`, [D56]); the boot-image-staleness key. NULL
    /// for legacy rows / resolver fallbacks — counts as stale.
    pub(crate) boot_image_id: Option<String>,
    pub(crate) worker_profile: String,
    pub(crate) slot_cpus: i32,
    pub(crate) slot_mem_mib: i32,
    /// Operator-pinned `workspaceVmLimit` budget ([provisioning_api D54]);
    /// NULL ⇒ the host tracks the current RAM-targeted default.
    pub(crate) workspace_vm_limit: Option<i32>,
    /// Operator-pinned `bootDiskGb` override (ontology
    /// `g5.runtime.placement.host-disk`); NULL ⇒ the replacement re-derives the
    /// size from its VM budget, so a sizing-rule change lands on rotation.
    pub(crate) boot_disk_gb: Option<i32>,
    pub(crate) cloud_instance_id: Option<String>,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) rotation_replaced_by: Option<Uuid>,
    /// Carried onto the replacement so the Workers page keeps naming who
    /// requested the original host (migration 061).
    pub(crate) created_by_user_id: Option<Uuid>,
}

/// The shared `SELECT` column list matching [`RotationHostRow`]'s field order.
pub(crate) const ROTATION_HOST_ROW_COLUMNS: &str =
    "host_id, region, instance_type, base_image_ref, base_image_digest, \
     base_image_source_kind, base_image_source_value, \
     boot_image_id, worker_profile, slot_cpus, slot_mem_mib, \
     workspace_vm_limit, boot_disk_gb, cloud_instance_id, \
     created_at, rotation_replaced_by, created_by_user_id";

/// Resolved per-pass thresholds (seconds).
#[derive(Debug, Clone, Copy)]
struct Thresholds {
    min_idle: u64,
    /// Age past which the [D54] force takes the host (idle path has no cliff
    /// branch since [D55]).
    cliff_age: u64,
    /// Age past which a still-busy host is warn-flagged.
    overdue_age: u64,
    /// Age past which a PAIRED host drains even with an unready replacement
    /// ([D67] forced-lane drain deadline: TTL − 15 min).
    drain_deadline_age: u64,
}

impl Thresholds {
    fn resolve(host_jwt_ttl: Duration) -> Self {
        let ttl = host_jwt_ttl.as_secs();
        Self {
            min_idle: resolve_min_idle_secs(),
            // Fixed margin ([D55]): the old `max(…, max_age)` clamp could
            // postpone the [D54] force past the TTL when misconfigured.
            cliff_age: ttl.saturating_sub(CLIFF_MARGIN_SECS),
            overdue_age: ttl.saturating_sub(OVERDUE_MARGIN_SECS),
            drain_deadline_age: ttl.saturating_sub(DRAIN_DEADLINE_MARGIN_SECS),
        }
    }
}

/// Run a single rotation pass. Pub so integration tests can drive the state
/// machine deterministically (no sleep, no spawned loop). A no-op (returns a
/// zeroed [`RotationPass`]) unless rotation is enabled AND cloud features are
/// on AND a cloud backend + JWT minter are wired.
///
/// Takes `&Arc<AppState>` (not `&AppState`) because the replacement bring-up
/// reuses the shared detached-spawn path and needs an owned handle.
pub async fn rotate_once(state: &Arc<AppState>) -> RotationPass {
    if !rotation_enabled() {
        return RotationPass::default();
    }
    if !state.cloud_features_enabled {
        return RotationPass::default();
    }
    if state.cloud_provider.is_none() {
        return RotationPass::default();
    }
    if state.jwt_minter.is_none() {
        // Without a minter no replacement credential can be issued; rotating
        // would tear hosts down with no way to bring replacements up.
        tracing::warn!("rotation: jwt_minter not configured — skipping pass");
        return RotationPass::default();
    }

    // Single-flight across replicas: rotation provisions instances, so
    // concurrent passes are not benign. Held on a dedicated system-pool
    // connection for the duration of the pass, then explicitly unlocked
    // (mirrors services/git/branch_integration_scheduler.rs).
    let mut lock_conn = match state.system_pool.acquire().await {
        Ok(conn) => conn,
        Err(e) => {
            tracing::warn!(error = %e, "rotation: failed to acquire lock connection");
            return RotationPass::default();
        }
    };
    let locked: bool = match sqlx::query_scalar("SELECT pg_try_advisory_lock($1)")
        .bind(ROTATION_ADVISORY_LOCK_KEY)
        .fetch_one(&mut *lock_conn)
        .await
    {
        Ok(locked) => locked,
        Err(e) => {
            tracing::warn!(error = %e, "rotation: advisory-lock acquisition failed");
            return RotationPass::default();
        }
    };
    if !locked {
        tracing::debug!("rotation: another replica holds the rotation lock — skipping pass");
        return RotationPass::default();
    }

    let pass = rotate_all_orgs(state).await;

    if let Err(e) = sqlx::query_scalar::<_, bool>("SELECT pg_advisory_unlock($1)")
        .bind(ROTATION_ADVISORY_LOCK_KEY)
        .fetch_one(&mut *lock_conn)
        .await
    {
        tracing::warn!(error = %e, "rotation: advisory-lock release failed (released on connection close)");
    }

    pass
}

async fn rotate_all_orgs(state: &Arc<AppState>) -> RotationPass {
    let thresholds = Thresholds::resolve(state.host_jwt_ttl);
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
                tracing::warn!(error = %e, "rotation: failed to list orgs");
                return RotationPass::default();
            }
        };

    // ── Desired outer boot image ([D56], issue #1424): resolved ONCE per pass,
    // fleet-wide — the `g5-worker` family is a per-project GCE resource, not
    // per-org. Best-effort: a resolver failure is no boot-image signal for this
    // tick (the [D55] guest-OCI trigger and the [D54] force are unaffected).
    // Reused for each organization so a pass rotates toward one fleet target.
    let desired_boot_image: Option<String> = match state.cloud_provider.as_ref() {
        Some(cloud) => match cloud.resolve_boot_image().await {
            Ok(identity) => Some(identity.image_id),
            Err(e) => {
                tracing::warn!(
                    target: "g5_audit",
                    event = "worker_host.rotation_boot_image_unresolved",
                    error = %e,
                    "Boot-image resolution failed this pass; the boot-image staleness trigger is disabled for this tick"
                );
                None
            }
        },
        None => None,
    };

    let mut pass = RotationPass {
        orgs_scanned: org_ids.len(),
        ..RotationPass::default()
    };

    for org_id in org_ids {
        if let Err(e) = rotate_org(
            state,
            &deps,
            org_id,
            thresholds,
            desired_boot_image.as_deref(),
            &mut pass,
        )
        .await
        {
            pass.errors += 1;
            tracing::warn!(error = %e, %org_id, "rotation: org pass failed; retrying next tick");
        }
    }

    pass
}

async fn rotate_org(
    state: &Arc<AppState>,
    deps: &TenantMaintenanceDeps,
    org_id: Uuid,
    thresholds: Thresholds,
    desired_boot_image: Option<&str>,
    pass: &mut RotationPass,
) -> Result<(), AppError> {
    // ── Resumability sweep ([D47]): finish rotations interrupted by a crash.
    // Rotation drains a host without a `drain_id`; an operator drain
    // (`WorkerHostService::request_drain`) always records one, and neither
    // side takes over the other's drain. So every draining row without a
    // `drain_id` is a rotation leftover, and stays one — with the replacement
    // marker set, only teardown + finalize remain; without it, the
    // replacement was never provisioned.
    let leftovers: Vec<RotationHostRow> = {
        let mut db = scoped_db_for_maintenance_org(deps, org_id).await?;
        let rows = sqlx::query_as::<_, RotationHostRow>(&format!(
            "SELECT {ROTATION_HOST_ROW_COLUMNS} \
             FROM worker_hosts \
             WHERE org_id = $1 AND status = 'draining' AND drain_id IS NULL",
        ))
        .bind(org_id)
        .fetch_all(db.conn())
        .await?;
        // Read-only: no commit needed; dropping the scoped tx releases it.
        rows
    };
    for row in leftovers {
        let active_workspace_vm: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM workspace_vms \
             WHERE host_id = $1 AND status IN ('starting','running','stopping'))",
        )
        .bind(row.host_id)
        .fetch_one(&state.system_pool)
        .await?;
        if active_workspace_vm {
            continue;
        }
        // Truthful outcome ([D67], issue #627 S8): report whether a
        // replacement really was provisioned, and count a deferral so the
        // org's capacity gap is visible rather than implied. A
        // cloud_cleanup_backlog deferral keeps the row DRAINING (CodeRabbit
        // on PR #637): finalizing without a successor would end the org's
        // one-for-one renewal with no lane left to retry it, while the
        // backlog clears within a few cleanup sweeps and this sweep then
        // provisions and finalizes. Other deferrals (ineligible: raced
        // marker/retirement state) finalize the empty leftover as before.
        let resumed_replacement = if row.rotation_replaced_by.is_none() {
            match provision_replacement(state, deps, org_id, &row, None).await? {
                ReplacementOutcome::Provisioned { .. } => true,
                ReplacementOutcome::Deferred {
                    reason: "cloud_cleanup_backlog",
                } => {
                    pass.pairing_deferred += 1;
                    continue;
                }
                ReplacementOutcome::Deferred { .. } => {
                    pass.pairing_deferred += 1;
                    false
                }
            }
        } else {
            false
        };
        teardown_and_finalize(state, deps, org_id, &row).await?;
        pass.resumed += 1;
        tracing::info!(
            target: "g5_audit",
            event = "worker_host.rotation_resumed",
            host_id = %row.host_id,
            org_id = %org_id,
            replacement_provisioned = resumed_replacement,
            "Finished a rotation interrupted by a prior crash"
        );
    }

    // ── Await sweep ([D67]): advance every paired old host — drain it once
    // its replacement is `ready` (or the forced deadline passes), or dissolve
    // the pairing when the replacement failed terminally.
    advance_paired_rotations(state, deps, org_id, thresholds, desired_boot_image, pass).await?;

    // ── Overdue-busy visibility ([D47]): aged hosts the idle gate keeps
    // skipping get a warn before the credential cliff kills them.
    let overdue: Vec<(Uuid, DateTime<Utc>)> = {
        let mut db = scoped_db_for_maintenance_org(deps, org_id).await?;
        sqlx::query_as::<_, (Uuid, DateTime<Utc>)>(
            "SELECT h.host_id, h.created_at
             FROM worker_hosts h
             WHERE h.org_id = $1
               AND h.status = 'ready'
               AND h.created_at < now() - make_interval(secs => $2::double precision)
               AND EXISTS (SELECT 1 FROM workspace_vms vm
                           WHERE vm.host_id = h.host_id
                             AND vm.status IN ('starting','running','stopping'))",
        )
        .bind(org_id)
        .bind(thresholds.overdue_age as f64)
        .fetch_all(db.conn())
        .await?
    };
    for (host_id, created_at) in overdue {
        pass.overdue_busy += 1;
        tracing::warn!(
            target: "g5_audit",
            event = "worker_host.rotation_overdue_busy",
            host_id = %host_id,
            org_id = %org_id,
            age_secs = (Utc::now() - created_at).num_seconds(),
            "Host is busy inside the host_jwt expiry window — the [D54] force will take it at the cliff"
        );
    }

    // ── Forced cliff rotation FIRST ([D54], Codex remediation e58cceec):
    // credential renewal outranks image rollout inside the
    // one-host-per-org-per-tick budget. Run before the idle path so a
    // post-deploy stream of image-stale idle candidates can never starve a
    // past-cliff host until its host_jwt dies; when the force rotates a host,
    // this tick's budget is spent and the idle path is skipped.
    if force_rotate_overdue(state, deps, org_id, thresholds, pass).await? {
        return Ok(());
    }

    // ── Desired-image resolution ([D55]): the idle path rotates only
    // image-stale hosts, so resolve the org's tracked promoted default once
    // per org per pass — one validated snapshot shared with the replacement
    // provisioning ([D52]). Pinned / no-track orgs yield no snapshot: their
    // idle path is a no-op and their 24h credential renewal falls entirely to
    // the [D54] force at the cliff. An invalid tracked ref is no-signal for
    // this pass (logged in the resolver).
    let desired_image: Option<(String, String)> = WorkerHostService::new()
        .resolve_tracked_image(&state.system_pool, org_id)
        .await?;
    let desired_digest: Option<&str> = desired_image.as_ref().map(|(_, d)| d.as_str());

    // ── Candidate selection ([D55] guest-image + [D56] boot-image predicate;
    // [D47] mechanics). One host per org per tick keeps the org's capacity dip
    // bounded to a single boot window. No age gate: a young idle host on a stale
    // guest OR outer boot image rotates; a host current on BOTH rotates only via
    // the [D54] cliff force. The idle path runs whenever EITHER desired value
    // exists this pass; each staleness disjunct is gated on its desired being
    // present ($2 / $3 NOT NULL) so a NULL desired never marks every host stale
    // (a NULL host boot_image_id vs a present desired counts as stale — legacy
    // rows converge, [D56]).
    let candidate: Option<RotationHostRow> =
        if desired_digest.is_none() && desired_boot_image.is_none() {
            None
        } else {
            let mut db = scoped_db_for_maintenance_org(deps, org_id).await?;
            sqlx::query_as::<_, RotationHostRow>(
                "SELECT h.host_id, h.region, h.instance_type, h.base_image_ref, \
                        h.base_image_digest, h.base_image_source_kind, \
                        h.base_image_source_value, h.boot_image_id, h.worker_profile, \
                        h.slot_cpus, h.slot_mem_mib, \
                        h.workspace_vm_limit, h.boot_disk_gb, h.cloud_instance_id, \
                        h.created_at, h.rotation_replaced_by, h.created_by_user_id
                 FROM worker_hosts h
                 WHERE h.org_id = $1
                   AND h.status = 'ready'
                   AND h.cloud_instance_id IS NOT NULL
                   AND h.rotation_replaced_by IS NULL
                   AND h.last_heartbeat > now() - interval '5 minutes'
                   AND (
                         ($2::text IS NOT NULL AND h.base_image_digest IS DISTINCT FROM $2)
                      OR ($3::text IS NOT NULL AND h.boot_image_id      IS DISTINCT FROM $3)
                       )
                   AND COALESCE(h.last_active_at, h.created_at)
                           < now() - make_interval(secs => $4::double precision)
                   AND NOT EXISTS (SELECT 1 FROM workspace_vms vm
                                   WHERE vm.host_id = h.host_id
                                     AND vm.status IN ('starting','running','stopping'))
                 ORDER BY h.created_at ASC
                 LIMIT 1",
            )
            .bind(org_id)
            .bind(desired_digest)
            .bind(desired_boot_image)
            .bind(thresholds.min_idle as f64)
            .fetch_optional(db.conn())
            .await?
        };

    let Some(host) = candidate else {
        // No image-stale idle candidate; the [D54] force already ran at the
        // top of this tick.
        return Ok(());
    };

    // ── Pair FIRST ([D67], issue #1880): provision the replacement — row,
    // fresh host_jwt, detached bring-up, `rotation_replaced_by` marker — while
    // the old host STAYS `ready` and serving. The drain moved into the await
    // sweep (`advance_paired_rotations`), which runs it only once the
    // replacement is `ready` (or dissolves the pairing when it fails). The
    // [D55] image snapshot still travels with the pairing so audit and
    // replacement image cannot disagree.
    match provision_replacement(state, deps, org_id, &host, desired_image.as_ref()).await? {
        ReplacementOutcome::Provisioned { .. } => {
            pass.paired += 1;
            tracing::info!(
                target: "g5_audit",
                event = "worker_host.rotation_paired",
                host_id = %host.host_id,
                org_id = %org_id,
                lane = "idle_image_stale",
                stale_digest = host.base_image_digest.as_deref().unwrap_or("<none>"),
                desired_digest = desired_digest.unwrap_or("<none>"),
                stale_boot_image = host.boot_image_id.as_deref().unwrap_or("<none>"),
                desired_boot_image = desired_boot_image.unwrap_or("<none>"),
                "Replacement provisioned; old host keeps serving until the replacement is ready ([D67])"
            );
        }
        ReplacementOutcome::Deferred { .. } => {
            // The deferral reason was already audited by provision_replacement;
            // the host stays `ready`, in service and in candidacy (issue #627).
            pass.pairing_deferred += 1;
        }
    }
    Ok(())
}

/// Replacement state probe row: (status, provision_error_code,
/// provision_next_retry_at, created_at, slot_size_reported, runtime_current,
/// boot_image_id).
type ReplacementProbe = (
    String,
    Option<String>,
    Option<DateTime<Utc>>,
    DateTime<Utc>,
    bool,
    bool,
    Option<String>,
);

/// A paired replacement's classification, shared by the await sweep's probe
/// AND the locked dissolve re-check (CodeRabbit on PR #637): the under-lock
/// re-verify is only meaningful while both decisions apply the same rule.
struct ReplacementState {
    ready: bool,
    failed: bool,
    still_provisioning: bool,
    failed_error_code: Option<String>,
}

/// Classify a replacement probe (`None` = the row was hard-deleted).
///
/// A replacement that placement would refuse cannot take over: the predecessor
/// keeps serving. While a stale replacement is still on the boot image the
/// worker family resolves to, nothing better exists and the pair waits; once
/// the family moves on (affirmative evidence: both images known and
/// different), the pair dissolves so the predecessor is replaced from the new
/// boot image, and the stale replacement retires as stale capacity.
///
/// Terminal failure: the replacement row died, or it sits in provisioning with
/// a classified error and NO retry future — retry disabled, or the [D66]
/// budget lapsed. A freshly classified row whose schedule stamp is a beat
/// behind is NOT failed (the budget check keeps this conservative).
fn classify_replacement(
    probe: Option<&ReplacementProbe>,
    desired_boot_image: Option<&str>,
) -> ReplacementState {
    let Some((
        status,
        error_code,
        next_retry_at,
        created_at,
        size_reported,
        runtime_current,
        boot_image_id,
    )) = probe
    else {
        return ReplacementState {
            ready: false,
            failed: true,
            still_provisioning: false,
            failed_error_code: None,
        };
    };
    let ready = status == "ready" && *size_reported && *runtime_current;
    let superseded_stale = status == "ready"
        && *size_reported
        && !*runtime_current
        && matches!(
            (boot_image_id.as_deref(), desired_boot_image),
            (Some(booted), Some(desired)) if booted != desired
        );
    let budget =
        i64::try_from(super::bringup::resolve_provision_retry_budget_secs()).unwrap_or(i64::MAX);
    let budget_lapsed = (Utc::now() - *created_at).num_seconds() >= budget;
    let failed = status == "terminated"
        || superseded_stale
        || (status == "provisioning"
            && error_code.is_some()
            && next_retry_at.is_none()
            && (!super::bringup::provision_retry_enabled() || budget_lapsed));
    ReplacementState {
        ready,
        failed,
        still_provisioning: status == "provisioning",
        failed_error_code: error_code
            .clone()
            .or_else(|| superseded_stale.then(|| "stale_runtime_superseded".to_string())),
    }
}

/// Await sweep ([D67], issue #1880): advance every PAIRED old host — a `ready`
/// row whose `rotation_replaced_by` marker is set. Drains it once the
/// replacement reaches `ready` (idle quiesce with busy re-verify below the
/// cliff; forced quiesce past it), forces the drain at the TTL−15min deadline
/// even with an unready replacement, and dissolves the pairing when the
/// replacement failed terminally so the old host re-enters candidacy.
async fn advance_paired_rotations(
    state: &Arc<AppState>,
    deps: &TenantMaintenanceDeps,
    org_id: Uuid,
    thresholds: Thresholds,
    desired_boot_image: Option<&str>,
    pass: &mut RotationPass,
) -> Result<(), AppError> {
    // `unreachable` old hosts stay in the lane (`g5.runtime.host.rotation`,
    // issue #627 S1): a paired host whose heartbeat died must still drain once
    // its replacement is ready (or at the deadline), or its pairing would
    // freeze and block the successor's own renewal through the chain guard.
    let paired: Vec<RotationHostRow> = {
        let mut db = scoped_db_for_maintenance_org(deps, org_id).await?;
        sqlx::query_as::<_, RotationHostRow>(&format!(
            "SELECT {ROTATION_HOST_ROW_COLUMNS} \
             FROM worker_hosts \
             WHERE org_id = $1 AND status IN ('ready','unreachable') \
               AND rotation_replaced_by IS NOT NULL",
        ))
        .bind(org_id)
        .fetch_all(db.conn())
        .await?
    };

    for old in paired {
        let Some(replacement_id) = old.rotation_replaced_by else {
            continue; // unreachable per the selector; defensive
        };
        let replacement: Option<ReplacementProbe> = {
            let mut db = scoped_db_for_maintenance_org(deps, org_id).await?;
            sqlx::query_as(
                "SELECT status, provision_error_code, provision_next_retry_at, created_at, slot_size_reported, \
                        (runtime_identity @> $3::jsonb) AS runtime_current, boot_image_id \
                 FROM worker_hosts WHERE org_id = $1 AND host_id = $2",
            )
            .bind(org_id)
            .bind(replacement_id)
            .bind(crate::services::hosts::worker_host_service::current_runtime_identity())
            .fetch_optional(db.conn())
            .await?
        };

        let past_cliff = (Utc::now() - old.created_at).num_seconds()
            >= i64::try_from(thresholds.cliff_age).unwrap_or(i64::MAX);
        let past_deadline = (Utc::now() - old.created_at).num_seconds()
            >= i64::try_from(thresholds.drain_deadline_age).unwrap_or(i64::MAX);

        let probe = classify_replacement(replacement.as_ref(), desired_boot_image);
        let (replacement_ready, replacement_failed, failed_error_code) =
            (probe.ready, probe.failed, probe.failed_error_code);

        if replacement_ready {
            // An unreachable old host has no tunnel to serve or drain
            // gracefully — it takes the forced lane even below the cliff
            // (issue #627 S1); custody is still respected (teardown readiness
            // requires no active workspace VMs). Below the cliff the
            // unreachable condition is RE-VERIFIED under the row lock
            // (issue #627 Codex re-review): the selector's status is stale
            // evidence, and a host that reconnected meanwhile re-enters the
            // idle path with its busy re-verify instead of being force-drained.
            let drained = if past_cliff {
                forced_quiesce(state, deps, org_id, &old).await?
            } else {
                below_cliff_quiesce(state, deps, org_id, &old, pass).await?
            };
            if drained {
                teardown_and_finalize(state, deps, org_id, &old).await?;
                if past_cliff {
                    pass.forced += 1;
                } else {
                    pass.rotated += 1;
                }
            }
            continue;
        }

        if replacement_failed {
            // Dissolve in ONE transaction, re-verifying the failure under the
            // same locks as the retirement (`g5.runtime.host.rotation`, issue
            // #627 S31): the probe above is stale evidence — a replacement
            // that registered (or re-entered retry) between the probe and this
            // branch must not be deallocated. Admission lock first, matching
            // provision_replacement and the capacity sweep's lock order.
            let dissolved = {
                let mut db = scoped_db_for_maintenance_org(deps, org_id).await?;
                crate::services::hosts::host_capacity::lock_admission(&mut db, org_id).await?;
                sqlx::query(
                    "SELECT host_id FROM worker_hosts WHERE org_id = $1 AND host_id = $2 FOR UPDATE",
                )
                .bind(org_id)
                .bind(old.host_id)
                .execute(db.conn())
                .await?;
                let fresh: Option<ReplacementProbe> = sqlx::query_as(
                    "SELECT status, provision_error_code, provision_next_retry_at, created_at, slot_size_reported, \
                            (runtime_identity @> $3::jsonb) AS runtime_current, boot_image_id \
                     FROM worker_hosts WHERE org_id = $1 AND host_id = $2 FOR UPDATE",
                )
                .bind(org_id)
                .bind(replacement_id)
                .bind(crate::services::hosts::worker_host_service::current_runtime_identity())
                .fetch_optional(db.conn())
                .await?;
                // Same rule as the probe above, re-applied under the lock.
                let fresh = classify_replacement(fresh.as_ref(), desired_boot_image);
                let (still_failed, still_provisioning) = (fresh.failed, fresh.still_provisioning);
                if still_failed {
                    if still_provisioning {
                        // Deallocate the dead row so failed rows do not pile up.
                        WorkerHostService::new()
                            .deallocate_host(&mut db, org_id, replacement_id)
                            .await?;
                    }
                    sqlx::query(
                        "UPDATE worker_hosts SET rotation_replaced_by = NULL \
                         WHERE host_id = $1 AND status IN ('ready','unreachable') \
                           AND rotation_replaced_by = $2",
                    )
                    .bind(old.host_id)
                    .bind(replacement_id)
                    .execute(db.conn())
                    .await?;
                    db.commit().await?;
                    true
                } else {
                    // The replacement recovered under our feet; leave the
                    // pairing intact and re-evaluate next tick.
                    false
                }
            };
            if !dissolved {
                continue;
            }
            pass.replacement_failed += 1;
            tracing::warn!(
                target: "g5_audit",
                event = "worker_host.rotation_replacement_failed",
                old_host_id = %old.host_id,
                failed_replacement_id = %replacement_id,
                org_id = %org_id,
                error_code = failed_error_code.as_deref().unwrap_or("<none>"),
                "Rotation replacement failed terminally; pairing dissolved and the old host stays in service"
            );
            // The drain deadline is independent of the replacement's fate
            // (issue #627 Codex PR review P1): persistent terminal failures
            // used to dissolve/re-pair on every pass and never reach the
            // deadline branch below, letting the old host sail past its
            // credential's death. Past the TTL−15min deadline the host drains
            // NOW — forced_quiesce re-checks state and custody under the row
            // lock, and teardown still requires the host to be empty.
            if past_deadline && forced_quiesce(state, deps, org_id, &old).await? {
                tracing::warn!(
                    target: "g5_audit",
                    event = "worker_host.rotation_deadline_drained",
                    host_id = %old.host_id,
                    org_id = %org_id,
                    replacement_host_id = %replacement_id,
                    age_secs = (Utc::now() - old.created_at).num_seconds(),
                    "Drain deadline reached with a terminally failed replacement; draining before the credential dies"
                );
                teardown_and_finalize(state, deps, org_id, &old).await?;
                pass.forced += 1;
            }
            continue;
        }

        // Replacement still in flight (bring-up or [D66] retry). Forced-lane
        // drain deadline ([D67], TTL−15min): past it, drain anyway — the
        // credential dies at the TTL regardless.
        if past_deadline && forced_quiesce(state, deps, org_id, &old).await? {
            tracing::warn!(
                target: "g5_audit",
                event = "worker_host.rotation_deadline_drained",
                host_id = %old.host_id,
                org_id = %org_id,
                replacement_host_id = %replacement_id,
                age_secs = (Utc::now() - old.created_at).num_seconds(),
                "Drain deadline reached with an unready replacement; draining before the credential dies"
            );
            teardown_and_finalize(state, deps, org_id, &old).await?;
            pass.forced += 1;
        }
    }
    Ok(())
}

/// Below-cliff quiesce for the [D67] await sweep (issue #627): decide the lane
/// from the host's CURRENT status under the row lock, not the selector's stale
/// read. Still `unreachable` ⇒ forced semantics (drain regardless of custody;
/// teardown readiness still requires no active workspace VMs). Recovered to
/// `ready` ⇒ the [D47] Tx A idle quiesce (busy ⇒ skip, the paired host retries
/// next tick). Anything else (operator drain, raced terminate) ⇒ not ours this
/// tick.
///
/// The lock comes first and the occupancy read is its own statement
/// (`g5.runtime.host.rotation`, G5-Lean#626 S22). A single `UPDATE … WHERE
/// NOT EXISTS` evaluated its subquery on the snapshot taken before the row
/// lock was granted, so a placement that committed while the statement
/// waited for the lock was invisible and the host drained under a `starting`
/// VM. Placement selects hosts `FOR UPDATE OF host SKIP LOCKED`: a placement
/// that committed before this lock is seen by the fresh read, and one that
/// arrives while the lock is held skips this host and finds it draining once
/// the commit lands.
async fn below_cliff_quiesce(
    _state: &Arc<AppState>,
    deps: &TenantMaintenanceDeps,
    org_id: Uuid,
    host: &RotationHostRow,
    pass: &mut RotationPass,
) -> Result<bool, AppError> {
    let mut db = scoped_db_for_maintenance_org(deps, org_id).await?;
    let row: Option<(String, Option<Uuid>)> = sqlx::query_as(
        "SELECT status, drain_id FROM worker_hosts \
         WHERE org_id = $1 AND host_id = $2 FOR UPDATE",
    )
    .bind(org_id)
    .bind(host.host_id)
    .fetch_optional(db.conn())
    .await?;
    let Some((status, drain_id)) = row else {
        return Ok(false);
    };
    if drain_id.is_some() {
        // An operator drained it first: that drain is the operator's.
        return Ok(false);
    }
    match status.as_str() {
        "unreachable" => {
            sqlx::query("UPDATE worker_hosts SET status = 'draining' WHERE host_id = $1")
                .bind(host.host_id)
                .execute(db.conn())
                .await?;
            let active_workspace_vm = sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM workspace_vms \
                 WHERE host_id = $1 AND status IN ('starting','running','stopping'))",
            )
            .bind(host.host_id)
            .fetch_one(db.conn())
            .await?;
            db.commit().await?;
            Ok(!active_workspace_vm)
        }
        "ready" => {
            // Fresh custody read AFTER the row lock (G5-Lean#626 S22), then a
            // plain CAS — never a single UPDATE whose subquery snapshot could
            // predate the lock.
            let active_workspace_vm = sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM workspace_vms \
                 WHERE host_id = $1 AND status IN ('starting','running','stopping'))",
            )
            .bind(host.host_id)
            .fetch_one(db.conn())
            .await?;
            if active_workspace_vm {
                pass.skipped_busy += 1;
                return Ok(false);
            }
            sqlx::query(
                "UPDATE worker_hosts SET status = 'draining' WHERE host_id = $1 AND status = 'ready'",
            )
            .bind(host.host_id)
            .execute(db.conn())
            .await?;
            db.commit().await?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

/// Test-only: drive the idle quiesce for one host, so a test can race a
/// placement against the custody re-check (`g5.runtime.host.rotation`,
/// G5-Lean#626 S22). Returns whether the host was marked draining.
#[cfg(feature = "test-helpers")]
pub async fn idle_quiesce_host_for_test(
    state: &Arc<AppState>,
    org_id: Uuid,
    host_id: Uuid,
    region: &str,
) -> Result<bool, AppError> {
    let deps = TenantMaintenanceDeps {
        system_pool: state.system_pool.clone(),
        tenant_pool: state.tenant_pool.clone(),
        authz: state.authz.clone(),
        repo_storage: state.repo_storage.clone(),
    };
    let host = RotationHostRow {
        host_id,
        region: region.to_string(),
        instance_type: String::new(),
        base_image_ref: None,
        base_image_digest: None,
        base_image_source_kind: None,
        base_image_source_value: None,
        boot_image_id: None,
        worker_profile: String::new(),
        slot_cpus: 0,
        slot_mem_mib: 0,
        workspace_vm_limit: None,
        boot_disk_gb: None,
        cloud_instance_id: None,
        created_at: Utc::now(),
        rotation_replaced_by: None,
        created_by_user_id: None,
    };
    let mut pass = RotationPass::default();
    below_cliff_quiesce(state, &deps, org_id, &host, &mut pass).await
}

/// Pair a replacement before a host credential reaches its expiry cliff.
async fn force_rotate_overdue(
    state: &Arc<AppState>,
    deps: &TenantMaintenanceDeps,
    org_id: Uuid,
    thresholds: Thresholds,
    pass: &mut RotationPass,
) -> Result<bool, AppError> {
    let candidate: Option<RotationHostRow> = {
        let mut db = scoped_db_for_maintenance_org(deps, org_id).await?;
        sqlx::query_as::<_, RotationHostRow>(
            "SELECT h.host_id, h.region, h.instance_type, h.base_image_ref, \
                    h.base_image_digest, h.base_image_source_kind, \
                    h.base_image_source_value, h.boot_image_id, h.worker_profile, \
                    h.slot_cpus, h.slot_mem_mib, \
                    h.workspace_vm_limit, h.boot_disk_gb, h.cloud_instance_id, \
                    h.created_at, h.rotation_replaced_by, h.created_by_user_id
             FROM worker_hosts h
             WHERE h.org_id = $1
               AND h.status = 'ready'
               AND h.cloud_instance_id IS NOT NULL
               AND h.rotation_replaced_by IS NULL
               AND h.created_at < now() - make_interval(secs => $2::double precision)
               AND h.last_heartbeat > now() - interval '5 minutes'
             ORDER BY h.created_at ASC
             LIMIT 1",
        )
        .bind(org_id)
        .bind(thresholds.cliff_age as f64)
        .fetch_optional(db.conn())
        .await?
    };
    let Some(host) = candidate else {
        return Ok(false);
    };

    // ── Pair only ([D67], issue #1880 — the Aug 19 pnc-cash-receipt incident
    // deleted a healthy host with ~2h of credential life 33s before learning
    // every zone was stocked out): provision the replacement while the old
    // host KEEPS SERVING. The await sweep drains it once the replacement is
    // `ready`, or at the TTL−15min deadline, whichever comes first. No image
    // snapshot here — the force rotates for credentials, so the replacement
    // re-resolves the tracked default (or copies, [D51]/[D52]).
    match provision_replacement(state, deps, org_id, &host, None).await? {
        ReplacementOutcome::Provisioned { .. } => {
            pass.paired += 1;
            tracing::warn!(
                target: "g5_audit",
                event = "worker_host.rotation_paired",
                host_id = %host.host_id,
                org_id = %org_id,
                lane = "forced_cliff",
                age_secs = (Utc::now() - host.created_at).num_seconds(),
                "Past-cliff host paired with a replacement; drains when the replacement is ready or at the TTL-15min deadline ([D54]/[D67])"
            );
            Ok(true)
        }
        ReplacementOutcome::Deferred { .. } => {
            // Nothing was provisioned (issue #627 S8): the tick's budget is
            // NOT spent, the host stays in forced candidacy, and the deferral
            // was audited by provision_replacement as the durable wait.
            pass.pairing_deferred += 1;
            Ok(false)
        }
    }
}

/// Mark a host draining and report readiness for teardown after all workspace VMs stop.
pub(crate) async fn forced_quiesce(
    _state: &Arc<AppState>,
    deps: &TenantMaintenanceDeps,
    org_id: Uuid,
    host: &RotationHostRow,
) -> Result<bool, AppError> {
    let mut db = scoped_db_for_maintenance_org(deps, org_id).await?;
    let row: Option<(String, Option<Uuid>)> = sqlx::query_as(
        "SELECT status, drain_id FROM worker_hosts \
         WHERE org_id = $1 AND host_id = $2 FOR UPDATE",
    )
    .bind(org_id)
    .bind(host.host_id)
    .fetch_optional(db.conn())
    .await?;
    let Some((status, drain_id)) = row else {
        return Ok(false);
    };
    if status == "ready" || status == "unreachable" {
        // `unreachable` takes the forced lane too (`g5.runtime.host.rotation`,
        // issue #627 S1): a heartbeat-dead host cannot drain gracefully, and
        // leaving it out froze its pair and every lane downstream of it.
        sqlx::query("UPDATE worker_hosts SET status = 'draining' WHERE host_id = $1")
            .bind(host.host_id)
            .execute(db.conn())
            .await?;
    } else if status != "draining" || drain_id.is_some() {
        // Gone, or an operator drained it first: that drain is the operator's.
        return Ok(false);
    }
    let active_workspace_vm = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM workspace_vms \
         WHERE host_id = $1 AND status IN ('starting','running','stopping'))",
    )
    .bind(host.host_id)
    .fetch_one(db.conn())
    .await?;
    db.commit().await?;
    Ok(!active_workspace_vm)
}

/// Provision the replacement host (config copied from the old row), mint a
/// fresh `host_jwt`, stamp `rotation_replaced_by` on the old row — all in one
/// transaction — then kick off the shared detached cloud bring-up ([D47] step
/// 2). The detached spawn survives this pass; a replacement whose bring-up is
/// interrupted is covered by the [D39] orphan machinery.
///
/// Returns an explicit [`ReplacementOutcome`] (`g5.runtime.host.rotation`,
/// issue #627): a deferral emits the `rotation_pairing_deferred` audit warn
/// here — the one durable, actionable wait — and callers decide what their
/// lane does with it; none may count it as progress.
pub(crate) async fn provision_replacement(
    state: &Arc<AppState>,
    deps: &TenantMaintenanceDeps,
    org_id: Uuid,
    old: &RotationHostRow,
    desired_image: Option<&(String, String)>,
) -> Result<ReplacementOutcome, AppError> {
    let minter = state.jwt_minter.as_ref().ok_or_else(|| {
        AppError::from_error(g5_contracts::error::G5Error::Internal {
            message: "rotation: jwt_minter unset past the pass gate".to_string(),
        })
    })?;

    let service = WorkerHostService::new();

    // Track-aware image resolution (ontology [D52]/[D55]): a tracking org's
    // replacement rolls onto the promoted default, which makes rotation the
    // rollout mechanism for tracking orgs. The [D55] idle path passes the
    // snapshot its candidate was selected against so audit and replacement
    // can never disagree; the force and resumability paths carry no snapshot
    // and re-resolve here. Pinned / no-track orgs (and invalid tracked refs,
    // logged in the resolver) copy the old host's image verbatim ([D51]).
    let resolved: Option<(String, String)> = match desired_image {
        Some(snapshot) => Some(snapshot.clone()),
        None => {
            service
                .resolve_tracked_image(&state.system_pool, org_id)
                .await?
        }
    };
    let (base_image_ref, base_image_digest, image_source_kind, image_source_value) = match resolved
    {
        Some((tracked_ref, digest)) => {
            let (track, _) = service.org_image_policy(&state.system_pool, org_id).await?;
            let track = track.ok_or_else(|| {
                AppError::from_error(g5_contracts::error::G5Error::Internal {
                    message: "rotation resolved a tracked image without a track policy".to_string(),
                })
            })?;
            (tracked_ref, digest, "track".to_string(), track)
        }
        None => {
            let image_ref = old.base_image_ref.clone().ok_or_else(|| {
                AppError::from_error(g5_contracts::error::G5Error::Internal {
                    message: "rotation refused a legacy host with no immutable worker image"
                        .to_string(),
                })
            })?;
            let digest = old.base_image_digest.clone().ok_or_else(|| {
                AppError::from_error(g5_contracts::error::G5Error::Internal {
                    message: "rotation host image is missing its digest".to_string(),
                })
            })?;
            (
                image_ref.clone(),
                digest,
                old.base_image_source_kind
                    .clone()
                    .unwrap_or_else(|| "pin".to_string()),
                old.base_image_source_value.clone().unwrap_or(image_ref),
            )
        }
    };

    let mut db = scoped_db_for_maintenance_org(deps, org_id).await?;
    // Serialize the pairing marker with every cold and warm assignment path.
    // This transaction advisory is acquired before provision_host_record can
    // lock/insert host rows, which preserves one global lock order.
    crate::services::hosts::host_capacity::lock_admission(&mut db, org_id).await?;
    let eligible: Option<bool> = sqlx::query_scalar(
        "SELECT automatic_capacity FROM worker_hosts
         WHERE org_id = $1 AND host_id = $2 AND rotation_replaced_by IS NULL
           AND drain_id IS NULL
           AND (status IN ('ready','unreachable','draining')
                OR (status = 'terminated' AND provision_error_code = 'never_registered'
                    AND (NOT automatic_capacity
                         OR (capacity_start_intent_id IS NULL AND NOT capacity_floor_claim))))
           AND NOT EXISTS (SELECT 1 FROM worker_hosts prev WHERE prev.rotation_replaced_by = worker_hosts.host_id
               AND prev.status <> 'terminated')
         FOR UPDATE",
    )
    .bind(org_id)
    .bind(old.host_id)
    .fetch_optional(db.conn())
    .await?;
    let Some(automatic_capacity) = eligible else {
        // A selection made before idle retirement/deletion is no authority to
        // restore that capacity. Never-registered manual recovery is explicit.
        tracing::warn!(
            target: "g5_audit",
            event = "worker_host.rotation_pairing_deferred",
            old_host_id = %old.host_id,
            org_id = %org_id,
            reason = "ineligible",
            "Pairing deferred: the host left the eligible set (raced retirement/deletion, an existing pairing, or a live predecessor still points at it)"
        );
        return Ok(ReplacementOutcome::Deferred {
            reason: "ineligible",
        });
    };
    if automatic_capacity {
        // Renewal overlap is NOT growth (`g5.runtime.host.rotation`, issue
        // #627 S2/PL-08): only undeleted automatic DEBRIS — terminated rows
        // whose cloud cleanup has not confirmed — counts against the
        // `g5.architecture.host_demand_capacity` bound here. Retained live
        // hosts and a lowered VM quota never defer a one-for-one replacement;
        // the one-overlap-per-retained-host rule is enforced by the pairing
        // marker and the predecessor chain guard above.
        let within_bound: bool = sqlx::query_scalar(
            "SELECT (SELECT count(*) FROM worker_hosts h WHERE h.org_id = $1
                AND h.automatic_capacity AND h.status = 'terminated'
                AND h.cloud_deleted_at IS NULL) <=
                GREATEST(COALESCE((SELECT (value #>> '{}')::bigint FROM configs WHERE org_id=$1
                    AND scope='org' AND scope_id=$1 AND key='max_running_vms'),10),0) + 1",
        )
        .bind(org_id)
        .fetch_one(db.conn())
        .await?;
        if !within_bound {
            tracing::warn!(
                target: "g5_audit",
                event = "worker_host.rotation_pairing_deferred",
                old_host_id = %old.host_id,
                org_id = %org_id,
                reason = "cloud_cleanup_backlog",
                "Pairing deferred: undeleted automatic cloud resources exceed the resource bound; retried every tick until cleanup confirms"
            );
            return Ok(ReplacementOutcome::Deferred {
                reason: "cloud_cleanup_backlog",
            });
        }
    }
    let provision = service
        .provision_host_record(
            &mut db,
            ProvisionHostRecordParams {
                org_id,
                region: &old.region,
                instance_type: &old.instance_type,
                base_image_ref: Some(base_image_ref.as_str()),
                base_image_digest: Some(base_image_digest.as_str()),
                base_image_source_kind: &image_source_kind,
                base_image_source_value: &image_source_value,
                worker_profile: &old.worker_profile,
                slot_cpus: old.slot_cpus,
                slot_mem_mib: old.slot_mem_mib,
                // Copied verbatim ([provisioning_api D54]): an operator-pinned
                // budget survives every rotation; NULL keeps tracking the
                // current default on this and future replacements.
                workspace_vm_limit: old.workspace_vm_limit,
                // Copied verbatim for the same reason: an operator-pinned disk
                // size survives every rotation; NULL re-derives the size from
                // this replacement's budget (g5.runtime.placement.host-disk).
                boot_disk_gb: old.boot_disk_gb,
                created_by_user_id: old.created_by_user_id,
            },
        )
        .await;
    let (replacement, allocation) = match provision {
        Ok(allocation) => allocation,
        Err(error)
            if matches!(&error.inner, g5_contracts::error::G5Error::ValidationError { details }
            if matches!(details.get("field").and_then(|v| v.as_str()), Some("bootDiskGb" | "workspaceVmLimit"))) =>
        {
            tracing::warn!(target: "g5_audit", event = "worker_host.rotation_pairing_deferred",
                old_host_id = %old.host_id, org_id = %org_id, reason = "incompatible_retained_disk_allocation",
                retained_disk_gib = ?old.boot_disk_gb, details = ?error.inner,
                "Replacement allocation is incompatible with current organization settings; retained host stays in service");
            return Ok(ReplacementOutcome::Deferred {
                reason: "incompatible_retained_disk_allocation",
            });
        }
        Err(error) => return Err(error),
    };

    sqlx::query("UPDATE worker_hosts SET automatic_capacity = $2 WHERE host_id = $1")
        .bind(replacement.host_id)
        .bind(automatic_capacity)
        .execute(db.conn())
        .await?;
    // Mint inside the still-open transaction so a mint failure rolls the
    // replacement row back rather than orphaning a credential-less host
    // (mirrors [D10]).
    let host_jwt = minter.mint_host_jwt(replacement.host_id, org_id, state.host_jwt_ttl)?;
    sqlx::query("UPDATE worker_hosts SET rotation_replaced_by = $2 WHERE host_id = $1")
        .bind(old.host_id)
        .bind(replacement.host_id)
        .execute(db.conn())
        .await?;
    db.commit().await?;

    tracing::info!(
        target: "g5_audit",
        event = "worker_host.rotation_replacement_provisioned",
        old_host_id = %old.host_id,
        new_host_id = %replacement.host_id,
        org_id = %org_id,
        "Rotation replacement host provisioned with a fresh host_jwt"
    );

    if let Some(cloud) = state.cloud_provider.clone() {
        let state_for_bringup = state.clone();
        let instance_type = replacement.instance_type.clone();
        let region = replacement.region.clone();
        let base_image_ref = replacement.base_image_ref.clone();
        let new_host_id = replacement.host_id;
        let scratch_disk_mib = replacement.scratch_disk_mib;
        let max_microvms_for_replacement = allocation.slots;
        let boot_disk_gb = allocation.disk_gib;
        let host_disk_gib = allocation.host_disk_gib;
        // Detached like the POST path ([D22]): the bring-up outlives this
        // pass; the JoinHandle is intentionally dropped ([D39] reaps rows
        // whose bring-up a process restart interrupts).
        let _bringup_handle = tokio::spawn(async move {
            bring_up_cloud_resources(BringUpCloudResourcesParams {
                state: state_for_bringup.as_ref(),
                cloud: cloud.as_ref(),
                host_id: new_host_id,
                org_id,
                instance_type: &instance_type,
                region: &region,
                host_jwt: &host_jwt,
                base_image_ref: base_image_ref.as_deref(),
                created_by: SYSTEM_MAINTENANCE_ACTOR_UUID,
                // The persisted operator budget ([provisioning_api D54], copied
                // onto the replacement row above) wins; a NULL (omitted at
                // provision time) resolves the CURRENT RAM-targeted default —
                // rotation is the rollout mechanism for default changes, the
                // same posture as [D52] image tracking.
                max_microvms: max_microvms_for_replacement,
                scratch_disk_mib,
                boot_disk_gb,
                host_disk_gib,
            })
            .await;
        });
    }

    Ok(ReplacementOutcome::Provisioned {
        replacement_id: replacement.host_id,
    })
}

/// Best-effort GCE teardown then finalize the old row to `terminated`
/// ([D47] steps 3–4). A teardown failure is warned and the finalize still
/// runs — [D23]'s terminate-best-effort posture; the row keeps its
/// `cloud_instance_id` for manual reap.
pub(crate) async fn teardown_and_finalize(
    state: &Arc<AppState>,
    deps: &TenantMaintenanceDeps,
    org_id: Uuid,
    old: &RotationHostRow,
) -> Result<(), AppError> {
    if let (Some(cloud), Some(cloud_instance_id)) = (
        state.cloud_provider.as_ref(),
        old.cloud_instance_id.as_ref(),
    ) {
        if let Err(e) = cloud.terminate_instance(cloud_instance_id).await {
            tracing::warn!(
                target: "g5_audit",
                event = "worker_host.rotation_teardown_failed",
                host_id = %old.host_id,
                org_id = %org_id,
                cloud_instance_id = %cloud_instance_id,
                error = %e,
                "Rotation teardown failed; GCE instance may be orphaned (cloud_instance_id retained on the terminated row)"
            );
        }
    }

    let mut db = scoped_db_for_maintenance_org(deps, org_id).await?;
    let service = WorkerHostService::new();
    service
        .deallocate_host(&mut db, org_id, old.host_id)
        .await?;
    db.commit().await?;

    tracing::info!(
        target: "g5_audit",
        event = "worker_host.rotation_finalized",
        host_id = %old.host_id,
        org_id = %org_id,
        "Rotated host finalized to terminated"
    );
    Ok(())
}

/// Spawn the rotation loop. No-op spawn (returns a handle that finishes
/// immediately after logging) when rotation is disabled or no cloud backend is
/// wired. Honors the cancellation token for graceful shutdown.
pub fn spawn_rotation_loop(state: Arc<AppState>, cancel: CancellationToken) -> JoinHandle<()> {
    tokio::spawn(async move {
        if !rotation_enabled() {
            tracing::debug!("rotation: {ROTATION_ENABLED_ENV} not set — not running");
            return;
        }
        if state.cloud_provider.is_none() || !state.cloud_features_enabled {
            tracing::debug!("rotation: no cloud backend wired — not running");
            return;
        }
        let interval_dur = resolve_interval();
        tracing::info!(
            interval_secs = interval_dur.as_secs(),
            min_idle_secs = resolve_min_idle_secs(),
            cliff_age_secs = state
                .host_jwt_ttl
                .as_secs()
                .saturating_sub(CLIFF_MARGIN_SECS),
            "worker-host rotation loop started"
        );
        let mut ticker = tokio::time::interval(interval_dur);
        ticker.tick().await; // consume the immediate first tick
        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    tracing::info!("worker-host idle-rotation loop shutting down");
                    return;
                }
                _ = ticker.tick() => {
                    let pass = rotate_once(&state).await;
                    if pass.rotated > 0
                        || pass.skipped_busy > 0
                        || pass.resumed > 0
                        || pass.overdue_busy > 0
                        || pass.forced > 0
                        || pass.paired > 0
                        || pass.replacement_failed > 0
                        || pass.pairing_deferred > 0
                        || pass.errors > 0
                    {
                        tracing::info!(
                            orgs_scanned = pass.orgs_scanned,
                            rotated = pass.rotated,
                            skipped_busy = pass.skipped_busy,
                            resumed = pass.resumed,
                            overdue_busy = pass.overdue_busy,
                            forced = pass.forced,
                            paired = pass.paired,
                            replacement_failed = pass.replacement_failed,
                            pairing_deferred = pass.pairing_deferred,
                            errors = pass.errors,
                            "rotation pass complete"
                        );
                    }
                }
            }
        }
    })
}
