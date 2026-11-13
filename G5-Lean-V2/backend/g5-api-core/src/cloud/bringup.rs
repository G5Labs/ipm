//! Shared detached cloud bring-up for worker hosts (ontology [D22], [D34],
//! [D44]).
//!
//! Historically private to `routes/worker_hosts.rs`; extracted here so the
//! idle-rotation loop ([D47], `cloud::rotation`) can provision replacement
//! hosts through the exact same best-effort path the POST handler uses —
//! same startup script, same operator-visibility labels, same classifier and
//! orphan-reconciler failure machinery. Behavior is identical to the
//! pre-extraction routes version.
//!
//! Cloud linkage and recovery (formerly [D39]/[D60]/[D61]) are governed by
//! `.g5/ontology/software_architecture/worker_host.g5.md`:
//! `g5.architecture.host_cloud_linkage` / `g5.architecture.host_insert_acceptance`.

use g5_contracts::types::worker_host::HostDiskAllocation;
pub use g5_contracts::types::worker_host::{
    default_workspace_vm_limit, parse_vcpus_from_instance_type,
};
use uuid::Uuid;

use super::startup_script::build_startup_script;
use super::{CloudError, CloudProvider, ProvisioningActor};
use crate::services::AppState;

/// Bring up the GCP cloud resources backing a just-provisioned host record and
/// persist `vpc_id`/`cloud_instance_id` (PR-rev-7-9c, ontology [D22]).
///
/// Best-effort: every step logs and returns on error rather than propagating,
/// so the POST handler always returns `201` once the DB record exists (the
/// cloud-call-after-db-commit failure mode, [D21]). The persistence write uses
/// the **system pool** via the `authz.set_worker_host_cloud_refs` SECURITY
/// DEFINER function (migration 081) — the scoped INSERT transaction has already
/// committed and dropped, and `worker_hosts` FORCE RLS would filter a direct
/// system-pool UPDATE to zero rows without an org binding (see [I9]).
pub struct BringUpCloudResourcesParams<'a> {
    pub state: &'a AppState,
    pub cloud: &'a dyn CloudProvider,
    pub host_id: Uuid,
    pub org_id: Uuid,
    pub instance_type: &'a str,
    pub region: &'a str,
    pub host_jwt: &'a str,
    pub base_image_ref: Option<&'a str>,
    /// The actor that initiated provisioning — the authenticated user on the
    /// POST path, or `SYSTEM_MAINTENANCE_ACTOR_UUID` on the rotation path —
    /// stamped as the `g5_created_by_id`/`g5_created_by` GCE labels (ontology
    /// [D44]). A UUID with no `users` row degrades to the UUID-only label.
    pub created_by: Uuid,
    /// Resolved host VM budget → `G5D_HOST_MAX_MICROVMS` in the startup script
    /// (ontology [D53]/[D54]). The POST handler resolves this from the request's
    /// `workspaceVmLimit` (validated `1..=vCPUs`); the rotation path from the
    /// persisted `worker_hosts.workspace_vm_limit`. When neither is set, the
    /// RAM-targeted [`default_workspace_vm_limit`] (~16 GB/VM) applies. Per-VM
    /// CPU/RAM sizing is measured by the supervisor and published at registration.
    pub max_microvms: u32,
    pub scratch_disk_mib: i32,
    pub host_disk_gib: u32,
    /// Resolved boot disk in GiB from the immutable creation-time allocation.
    /// GCE retains its historical `diskSizeGb` field name.
    pub boot_disk_gb: u32,
}

/// Read the creation-time snapshot, never current organization/deployment policy.
pub async fn host_disk_allocation(
    conn: &mut sqlx::PgConnection,
    host_id: Uuid,
) -> Result<HostDiskAllocation, crate::errors::AppError> {
    let (h, v, slots, disk): (Option<i32>, i32, Option<i32>, Option<i32>) = sqlx::query_as(
        "SELECT host_disk_gib, scratch_disk_mib, provisioned_slots, resolved_disk_gib FROM worker_hosts WHERE host_id = $1"
    ).bind(host_id).fetch_one(conn).await?;
    match (h, slots, disk) {
        (Some(h), Some(slots), Some(disk)) if h > 0 && v > 0 && slots > 0 && disk > 0 =>
            Ok(HostDiskAllocation { host_disk_gib: h as u32, vm_disk_mib: v as u32, slots: slots as u32, disk_gib: disk as u32 }),
        _ => Err(crate::errors::AppError::from_error(g5_contracts::error::G5Error::ServiceUnavailable {
            message: format!("host {host_id} has no complete recorded disk allocation; provisioning cannot safely infer it"),
        })),
    }
}

/// Interval between bring-up liveness stamps (ontology [D60]). The orphan
/// selector's staleness window in migration 171 is 3× this value (90s) so a
/// single missed stamp (transient DB error) cannot open the mid-provision
/// attach window that caused the #1512 race.
pub(crate) const BRINGUP_HEARTBEAT_INTERVAL_SECS: u64 = 30;

// ─── Stockout retry configuration (ontology [D66], issue #1880) ─────────────

/// Opt-in master switch for bounded stockout retry (`false` unless truthy).
pub const PROVISION_RETRY_ENABLED_ENV: &str = "G5_PROVISION_RETRY_ENABLED";
/// Total retry window per row, measured from `worker_hosts.created_at`
/// (standup 2026-08-17: "keep retrying for 30 minutes").
pub const PROVISION_RETRY_BUDGET_ENV: &str = "G5_PROVISION_RETRY_BUDGET_SECS";
/// Default retry budget: 30 minutes.
pub const DEFAULT_PROVISION_RETRY_BUDGET_SECS: u64 = 1_800;
/// Floor for the retry budget.
pub const MIN_PROVISION_RETRY_BUDGET_SECS: u64 = 60;
/// Ordered comma-separated fallback-region basket for retry attempts 3+
/// ([D66]; human decision 547a4e7d: managed default `us-east1,us-west1`,
/// North-America-only per the 2026-08-17 standup jurisdiction stance).
pub const FALLBACK_REGIONS_ENV: &str = "G5_WORKER_FALLBACK_REGIONS";

/// Whether stockout retry is enabled (opt-in, [D66]).
#[must_use]
pub fn provision_retry_enabled() -> bool {
    std::env::var(PROVISION_RETRY_ENABLED_ENV)
        .map(|raw| {
            let v = raw.trim().to_ascii_lowercase();
            v == "1" || v == "true"
        })
        .unwrap_or(false)
}

/// Resolve the per-row retry budget (seconds), floored.
#[must_use]
pub fn resolve_provision_retry_budget_secs() -> u64 {
    std::env::var(PROVISION_RETRY_BUDGET_ENV)
        .ok()
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .map(|v| v.max(MIN_PROVISION_RETRY_BUDGET_SECS))
        .unwrap_or(DEFAULT_PROVISION_RETRY_BUDGET_SECS)
}

/// The managed default basket when `G5_WORKER_FALLBACK_REGIONS` is UNSET
/// ([D66], human decision 547a4e7d; North-America-only per the 2026-08-17
/// standup). An EXPLICITLY empty value disables cross-region retry.
pub const DEFAULT_FALLBACK_REGIONS: &str = "us-east1,us-west1";

/// Resolve the validated fallback-region basket ([D66]): comma-separated,
/// order-preserving, deduplicated; entries outside
/// `WORKER_HOST_REGION_ALLOWLIST` are dropped with a warn (a typo must degrade
/// the basket, never fail the pass). Unset ⇒ [`DEFAULT_FALLBACK_REGIONS`];
/// explicitly empty ⇒ same-region retry only.
#[must_use]
pub fn resolve_fallback_regions() -> Vec<String> {
    let raw = std::env::var(FALLBACK_REGIONS_ENV)
        .unwrap_or_else(|_| DEFAULT_FALLBACK_REGIONS.to_string());
    let mut out: Vec<String> = Vec::new();
    for entry in raw.split(',') {
        let region = entry.trim();
        if region.is_empty() || out.iter().any(|r| r == region) {
            continue;
        }
        if g5_contracts::types::worker_host::WORKER_HOST_REGION_ALLOWLIST.contains(&region) {
            out.push(region.to_string());
        } else {
            tracing::warn!(
                region,
                "{FALLBACK_REGIONS_ENV} entry is not in WORKER_HOST_REGION_ALLOWLIST; dropped"
            );
        }
    }
    out
}

pub async fn bring_up_cloud_resources(params: BringUpCloudResourcesParams<'_>) {
    let state = params.state;
    let host_id = params.host_id;

    // A worker host must always boot from an image resolved by the API
    // preflight. Refuse to make any cloud calls for legacy or malformed
    // callers instead of silently substituting the startup-script fallback.
    if params.base_image_ref.is_none() {
        tracing::error!(
            host_id = %host_id,
            "worker host bring-up refused: immutable base image was not resolved"
        );
        return;
    }

    // [D60] liveness ticker: stamp `bringup_heartbeat_at` at t=0 and every
    // interval while the bring-up body runs, so the orphan pass can tell an
    // in-flight provision (possibly deep in a [D59] multi-zone stockout
    // retry) from a dead handler. Best-effort: a failed stamp only warns —
    // a task that stops stamping is exactly what the orphan pass exists to
    // reap. The ticker future is dropped when the body completes.
    let heartbeat = async {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(
            BRINGUP_HEARTBEAT_INTERVAL_SECS,
        ));
        loop {
            tick.tick().await;
            let stamped: Result<Option<Uuid>, sqlx::Error> =
                sqlx::query_scalar::<_, Option<Uuid>>("SELECT authz.touch_worker_host_bringup($1)")
                    .bind(host_id)
                    .fetch_one(&state.system_pool)
                    .await;
            if let Err(e) = stamped {
                tracing::warn!(
                    host_id = %host_id,
                    error = %e,
                    "bring-up liveness stamp failed; orphan pass may engage if this persists"
                );
            }
        }
    };
    tokio::select! {
        biased;
        _ = bring_up_cloud_resources_inner(params) => {}
        _ = heartbeat => {}
    }
}

async fn bring_up_cloud_resources_inner(params: BringUpCloudResourcesParams<'_>) {
    let BringUpCloudResourcesParams {
        state,
        cloud,
        host_id,
        org_id,
        instance_type,
        region,
        host_jwt,
        base_image_ref,
        created_by,
        max_microvms,
        scratch_disk_mib,
        host_disk_gib,
        boot_disk_gb,
    } = params;

    // Fail-closed guest glibc-floor guard at provisioning bring-up
    // (provisioning_api#D57, enforcing system_invariants/microvm_execution_boundary#D12).
    // Resolve the base_image's ABI label BEFORE creating the VM; if it POSITIVELY
    // declares a glibc below the floor, abort bring-up so no cloud VM is ever
    // created — no task can then launch on a sub-floor guest (fail-closed),
    // instead of the host-staged dynamically linked runtime crashing with a raw
    // missing-symbol error at task launch. Aborting = no cloud refs persisted =
    // the [D21] best-effort/reaper model already handles the record. Only a
    // POSITIVELY-declared sub-floor label aborts: a missing/unparseable label or
    // a registry-resolution failure proceeds (unlabeled/unresolvable images are
    // the operator's responsibility, microvm_execution_boundary#D12), matching this function's best-effort
    // posture. Never log the label value (#D43).
    if let Some(base_ref) = base_image_ref {
        match state.runtime_label_resolver.resolve(base_ref).await {
            Ok(resolved) => {
                if resolved
                    .labels
                    .get("com.g5labs.runtime.base_image")
                    .and_then(|v| v.as_str())
                    .is_some_and(g5_contracts::types::worker_host::base_image_below_glibc_floor)
                {
                    tracing::error!(
                        target: "g5_audit",
                        event = "worker_host.base_image_below_glibc_floor",
                        host_id = %host_id,
                        org_id = %org_id,
                        "Aborted bring-up: base_image declares a sub-floor guest glibc; no VM created (repin to a 26.04 base)"
                    );
                    // Persist the dedicated code (migration 264) so the row
                    // surfaces an actionable badge instead of aging into the
                    // generic abandoned_bringup disposition. Best-effort like
                    // every other bring-up persistence: a failed stamp only
                    // warns, and the [D21] reaper still governs the record.
                    let stamped: Result<Option<Uuid>, sqlx::Error> = sqlx::query_scalar(
                        "SELECT authz.set_worker_host_provision_error_code($1, $2)",
                    )
                    .bind(host_id)
                    .bind("base_image_below_glibc_floor")
                    .fetch_one(&state.system_pool)
                    .await;
                    if let Err(err) = stamped {
                        tracing::warn!(
                            target: "g5_audit",
                            event = "worker_host.provision_error_persist_failed",
                            host_id = %host_id,
                            org_id = %org_id,
                            error = %err,
                            "Failed to persist base_image_below_glibc_floor provision error code"
                        );
                    }
                    return;
                }
            }
            Err(_) => {
                tracing::warn!(
                    target: "g5_audit",
                    event = "worker_host.glibc_floor_precheck_unresolved",
                    host_id = %host_id,
                    org_id = %org_id,
                    "glibc-floor pre-check could not resolve base_image labels; proceeding (unlabeled/unresolvable = operator responsibility, microvm_execution_boundary#D12)"
                );
            }
        }
    }

    // Cache door (ontology g5.runtime.cache.door): also per bring-up, and
    // scoped to this host's organization for the fleet sccache prefix.
    let cache_door =
        super::startup_script::worker_cache_door(|key| std::env::var(key).ok(), org_id);
    let startup_script = build_startup_script(
        host_id,
        host_jwt,
        &state.external_url,
        region,
        HostDiskAllocation {
            host_disk_gib,
            vm_disk_mib: scratch_disk_mib as u32,
            slots: max_microvms,
            disk_gib: boot_disk_gb,
        },
        base_image_ref,
        cache_door.as_ref(),
        // Nested virtualization (ontology g5.architecture.microvm_backend.nested):
        // per bring-up from deployment environment, QA first.
        super::startup_script::worker_nested_virtualization(|key| std::env::var(key).ok()),
    );

    // Operator-visibility lookups (ontology [D44]) — both best-effort: a
    // failed slug read provisions under the legacy `g5-host-{host_id}` name,
    // a failed email read degrades to the UUID-only creator label. Neither
    // failure may block bring-up. `orgs`/`users` carry no RLS, so the system
    // pool reads them without a definer function. Each lookup is bounded by
    // a short timeout so a starved system pool delays bring-up by at most
    // ~1s instead of the pool's full acquire timeout (CodeRabbit on PR #365,
    // chat intent C4); timeout degrades exactly like a query error.
    const METADATA_LOOKUP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(1);
    // The two lookups run concurrently so the whole metadata phase shares one
    // ~1s wall-clock bound even when both time out (codex on PR #365).
    let (slug_result, email_result) = tokio::join!(
        tokio::time::timeout(
            METADATA_LOOKUP_TIMEOUT,
            sqlx::query_scalar::<_, String>("SELECT slug FROM orgs WHERE id = $1")
                .bind(org_id)
                .fetch_optional(&state.system_pool),
        ),
        tokio::time::timeout(
            METADATA_LOOKUP_TIMEOUT,
            sqlx::query_scalar::<_, String>("SELECT email FROM users WHERE id = $1")
                .bind(created_by)
                .fetch_optional(&state.system_pool),
        )
    );
    let org_slug: String = match slug_result {
        Ok(Ok(Some(slug))) => slug,
        Ok(Ok(None)) => String::new(),
        Ok(Err(e)) => {
            tracing::warn!(
                host_id = %host_id,
                org_id = %org_id,
                error = %e,
                "org slug lookup failed; provisioning under legacy instance name"
            );
            String::new()
        }
        Err(_) => {
            tracing::warn!(
                host_id = %host_id,
                org_id = %org_id,
                "org slug lookup timed out; provisioning under legacy instance name"
            );
            String::new()
        }
    };
    let actor_email: Option<String> = match email_result {
        Ok(Ok(email)) => email,
        Ok(Err(e)) => {
            tracing::warn!(
                host_id = %host_id,
                user_id = %created_by,
                error = %e,
                "creator email lookup failed; stamping UUID-only creator label"
            );
            None
        }
        Err(_) => {
            tracing::warn!(
                host_id = %host_id,
                user_id = %created_by,
                "creator email lookup timed out; stamping UUID-only creator label"
            );
            None
        }
    };
    let actor = ProvisioningActor {
        user_id: created_by,
        email: actor_email,
    };

    let vpc = match cloud.ensure_vpc_for_org(org_id).await {
        Ok(vpc) => vpc,
        Err(e) => {
            log_cloud_bringup_error(&e, host_id, org_id, "ensure_vpc", None);
            persist_provision_error_if_classified(state, host_id, org_id, &e).await;
            return;
        }
    };

    let image = cloud.worker_image().to_string();
    let instance = match cloud
        .provision_instance(
            host_id,
            &org_slug,
            Some(&actor),
            &vpc,
            instance_type,
            region,
            &image,
            &startup_script,
            boot_disk_gb,
        )
        .await
    {
        Ok(instance) => instance,
        Err(e) => {
            log_cloud_bringup_error(&e, host_id, org_id, "provision_instance", Some(&vpc.vpc_id));
            persist_provision_error_if_classified(state, host_id, org_id, &e).await;
            return;
        }
    };

    // 4-arg overload ([D56], migration 168): also persists the resolved-and-pinned
    // outer boot image so idle rotation can detect boot-image staleness. NULL
    // boot_image_id (resolver fell back) is fine — the row converges on a later
    // rotation. The 3-arg overload is retained for the orphan-reconciler attach
    // path (no boot image known there) and rolling-deploy safety.
    let persisted: Result<Option<Uuid>, sqlx::Error> = sqlx::query_scalar::<_, Option<Uuid>>(
        "SELECT authz.set_worker_host_cloud_refs($1, $2, $3, $4)",
    )
    .bind(host_id)
    .bind(&vpc.vpc_id)
    .bind(&instance.cloud_instance_id)
    .bind(instance.boot_image_id.as_deref())
    .fetch_one(&state.system_pool)
    .await;
    match persisted {
        Ok(Some(_)) => {}
        Ok(None) => {
            // The host left `provisioning` (a DELETE, rotation, or [D61]
            // gone-confirmation raced bring-up) during the cloud round-trip.
            // The guarded UPDATE declined to re-link the row, so the instance
            // we just created has no DB linkage and nothing will ever tear it
            // down — compensate by best-effort terminating it now ([D61],
            // issue #1512: this exact shape leaked two RUNNING c3-standard-4
            // VMs on build fa1034a2). The row's own teardown path could not
            // have deleted this instance (it ran with a NULL or stale
            // linkage).
            //
            // Ownership check first (Codex round-2 blocker 2): Ok(None) only
            // proves the row is not in `provisioning` NOW — it does NOT prove
            // nothing owns this instance. If an orphan attach linked the SAME
            // deterministic instance and the row already advanced (e.g. to
            // `ready`), terminating it would kill a live host. Only compensate
            // when the row's current linkage is absent or points elsewhere;
            // on a read failure stay conservative and leave the VM (the
            // pre-171 posture: log for manual cleanup).
            let current: Result<Option<(String, Option<String>)>, sqlx::Error> =
                sqlx::query_as::<_, (String, Option<String>)>(
                    "SELECT status, cloud_instance_id \
                     FROM authz.get_worker_host_cloud_linkage($1)",
                )
                .bind(host_id)
                .fetch_optional(&state.system_pool)
                .await;
            match &current {
                Ok(Some((_, Some(linkage)))) if linkage == &instance.cloud_instance_id => {
                    // The row converged onto this instance through the orphan
                    // attach — it is live and owned; nothing to compensate.
                    tracing::info!(
                        target: "g5_audit",
                        event = "worker_host.cloud_bringup_converged",
                        host_id = %host_id,
                        org_id = %org_id,
                        cloud_instance_id = %instance.cloud_instance_id,
                        "Row already owns the instance this bring-up created (orphan attach converged first); no compensation"
                    );
                    return;
                }
                Ok(Some((status, None))) if status != "terminated" => {
                    // A LIVE row with no linkage: hello can promote the row to
                    // `ready` before this persistence lands (issue #627 S19) —
                    // the instance this bring-up created IS that host's VM,
                    // just not linked yet. Terminating it would kill a live,
                    // just-registered host from stale evidence. Leave it for
                    // reconciliation (`g5.runtime.host.compensation`): the
                    // orphan pass attaches a live unlinked row by its
                    // deterministic instance name once this task's bring-up
                    // lease goes stale.
                    tracing::info!(
                        target: "g5_audit",
                        event = "worker_host.cloud_bringup_unlinked_live",
                        host_id = %host_id,
                        org_id = %org_id,
                        cloud_instance_id = %instance.cloud_instance_id,
                        status = %status,
                        "Row is live but unlinked after bring-up persistence declined; leaving the instance for orphan reconciliation (no compensation)"
                    );
                    return;
                }
                // Terminated/absent row, or a live row that owns a DIFFERENT
                // instance: this instance is surplus with no owner — the
                // row's own teardown path can never delete it, and the
                // cloud-orphan sweep would treat a live row's name as live
                // property. Compensate below.
                Ok(_) => {}
                Err(e) => {
                    tracing::error!(
                        target: "g5_audit",
                        event = "worker_host.cloud_orphan",
                        host_id = %host_id,
                        org_id = %org_id,
                        vpc_id = %vpc.vpc_id,
                        cloud_instance_id = %instance.cloud_instance_id,
                        error = %e,
                        "GCE instance alive with unconfirmed DB linkage (ownership check failed); manual cleanup may be required"
                    );
                    return;
                }
            }
            let compensation = cloud.terminate_instance(&instance.cloud_instance_id).await;
            match compensation {
                Ok(()) => {
                    tracing::warn!(
                        target: "g5_audit",
                        event = "worker_host.cloud_orphan_compensated",
                        host_id = %host_id,
                        org_id = %org_id,
                        vpc_id = %vpc.vpc_id,
                        cloud_instance_id = %instance.cloud_instance_id,
                        "Host left provisioning during bring-up; compensating terminate of the just-created instance succeeded"
                    );
                }
                Err(e) => {
                    tracing::error!(
                        target: "g5_audit",
                        event = "worker_host.cloud_orphan",
                        host_id = %host_id,
                        org_id = %org_id,
                        vpc_id = %vpc.vpc_id,
                        cloud_instance_id = %instance.cloud_instance_id,
                        error = %e,
                        "GCE instance alive with no DB linkage and the compensating terminate failed; manual cleanup required until stale-host detector lands"
                    );
                }
            }
            return;
        }
        Err(e) => {
            tracing::error!(
                target: "g5_audit",
                event = "worker_host.cloud_bringup_failed",
                stage = "persist_refs",
                host_id = %host_id,
                org_id = %org_id,
                error = %e,
                "Persisting vpc_id/cloud_instance_id failed; instance exists but row is unlinked"
            );
            return;
        }
    }

    tracing::info!(
        target: "g5_audit",
        event = "worker_host.cloud_brought_up",
        host_id = %host_id,
        org_id = %org_id,
        vpc_id = %vpc.vpc_id,
        cloud_instance_id = %instance.cloud_instance_id,
        boot_image_id = instance.boot_image_id.as_deref().unwrap_or("<none>"),
        "Worker host cloud resources brought up; awaiting heartbeat for ready transition"
    );
}

/// Log a cloud bring-up error, splitting the dedicated `CloudError::AuthDenied`
/// variant onto its own audit event so operators can distinguish bad IAM /
/// missing credentials from generic API failures (codex round-2 important;
/// ontology [D17]). All other CloudError variants share `cloud_bringup_failed`.
fn log_cloud_bringup_error(
    err: &CloudError,
    host_id: Uuid,
    org_id: Uuid,
    stage: &'static str,
    vpc_id: Option<&str>,
) {
    match err {
        CloudError::AuthDenied {
            status,
            scope,
            detail,
        } => {
            tracing::error!(
                target: "g5_audit",
                event = "worker_host.cloud_auth_denied",
                stage,
                host_id = %host_id,
                org_id = %org_id,
                vpc_id = vpc_id.unwrap_or(""),
                status = *status,
                scope = %scope,
                detail = %detail,
                "GCP rejected credentials (401/403); check service-account IAM grants"
            );
        }
        other => {
            tracing::error!(
                target: "g5_audit",
                event = "worker_host.cloud_bringup_failed",
                stage,
                host_id = %host_id,
                org_id = %org_id,
                vpc_id = vpc_id.unwrap_or(""),
                error = %other,
                "Cloud bring-up failed; host left provisioning"
            );
        }
    }
}

/// Classify a [`CloudError`] into a `worker_hosts.provision_error_code` value
/// the UI can render as a precise badge (migration 102, ontology [D34]).
///
/// Returns `None` when the failure is not in the known codeset; the caller then
/// leaves the column NULL and the UI keeps showing "Provisioning" (the
/// reconciler will eventually flip the row to `terminated`).
///
/// Detection looks at the **provider error message** rather than the HTTP
/// status: GCP returns HTTP 503 for both transient unavailability AND
/// `ZONE_RESOURCE_POOL_EXHAUSTED`, but the message differs. Substring matches
/// are case-insensitive against the canonical GCP reason
/// (`ZONE_RESOURCE_POOL_EXHAUSTED`, with or without `_WITH_DETAILS`) and the
/// human phrase GCP returns when the reason is folded into `error.message`
/// only ("does not have enough resources"). Extend in lockstep with the column
/// CHECK and the UI badge map when a new code is added.
pub(crate) fn classify_provision_error(err: &CloudError) -> Option<&'static str> {
    // Shared with `provision_instance`'s multi-zone fallback so the retry and
    // the terminal classification never drift ([D34]/[D59]). Reaching here means
    // EVERY candidate zone was exhausted (the fallback already retried siblings).
    if crate::cloud::is_zone_capacity_exhausted(err) {
        return Some("zone_capacity_exhausted");
    }
    None
}

/// Persist the classified provision error code on the host row, if any. Uses
/// the SECURITY DEFINER function added in migration 102 so the system-pool
/// UPDATE reaches the row under `worker_hosts` FORCE RLS. Runs **post-commit**
/// (the scoped INSERT transaction has already closed at `db.commit()`) but
/// **pre-response** — `provision_worker_host` still awaits this branch before
/// constructing the 201, so the classifier write lands before the user sees
/// the response. Deliberate: the next GET should observe the classified code
/// immediately. Best-effort: a sqlx failure is logged at error level but
/// never propagated, so a write failure cannot turn the 201 into a 5xx.
pub(crate) async fn persist_provision_error_if_classified(
    state: &AppState,
    host_id: Uuid,
    org_id: Uuid,
    err: &CloudError,
) {
    let Some(code) = classify_provision_error(err) else {
        return;
    };
    let result: Result<Option<Uuid>, sqlx::Error> = sqlx::query_scalar::<_, Option<Uuid>>(
        "SELECT authz.set_worker_host_provision_error_code($1, $2)",
    )
    .bind(host_id)
    .bind(code)
    .fetch_one(&state.system_pool)
    .await;
    match result {
        Ok(Some(_)) => {
            tracing::warn!(
                target: "g5_audit",
                event = "worker_host.provision_error_classified",
                host_id = %host_id,
                org_id = %org_id,
                provision_error_code = %code,
                "Classified cloud bring-up failure and persisted to worker_hosts.provision_error_code"
            );
            // [D66] (issue #1880): an exhausted-everywhere stockout is
            // RETRYABLE, not terminal, when retry is enabled — schedule the
            // next attempt with SQL-computed backoff. Every bring-up caller
            // (POST, rotation replacement, retry pass) self-schedules through
            // this one branch. Best-effort: a failed stamp only warns; the
            // row keeps its truthful badge and the [D65] zero-capacity alert
            // covers the org.
            if code == "zone_capacity_exhausted" && provision_retry_enabled() {
                let scheduled: Result<Option<chrono::DateTime<chrono::Utc>>, sqlx::Error> =
                    sqlx::query_scalar("SELECT authz.schedule_worker_host_provision_retry($1)")
                        .bind(host_id)
                        .fetch_one(&state.system_pool)
                        .await;
                match scheduled {
                    Ok(Some(next_retry_at)) => {
                        tracing::info!(
                            target: "g5_audit",
                            event = "worker_host.provision_retry_scheduled",
                            host_id = %host_id,
                            org_id = %org_id,
                            next_retry_at = %next_retry_at,
                            "Stockout classified; next provisioning retry scheduled with backoff"
                        );
                    }
                    Ok(None) => {
                        // The row left provisioning or acquired a linkage
                        // between classification and this stamp — nothing to
                        // retry.
                    }
                    Err(e) => {
                        tracing::warn!(
                            host_id = %host_id,
                            error = %e,
                            "Failed to schedule provisioning retry; row keeps its terminal badge until the next classification"
                        );
                    }
                }
            }
        }
        Ok(None) => {
            // The row left `provisioning` between bring-up failure and this
            // write (e.g. a concurrent DELETE). The classified code is moot
            // for a terminated row; no further action.
            tracing::info!(
                target: "g5_audit",
                event = "worker_host.provision_error_skipped",
                host_id = %host_id,
                org_id = %org_id,
                provision_error_code = %code,
                "Skipped persisting provision_error_code; host already left provisioning"
            );
        }
        Err(e) => {
            tracing::error!(
                target: "g5_audit",
                event = "worker_host.provision_error_persist_failed",
                host_id = %host_id,
                org_id = %org_id,
                provision_error_code = %code,
                error = %e,
                "Failed to persist provision_error_code; UI will continue to show Provisioning until reconciler advances the row"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[serial_test::serial]
    fn fallback_regions_default_when_unset_and_disable_when_explicitly_empty() {
        let prior = std::env::var(FALLBACK_REGIONS_ENV).ok();

        // Unset ⇒ the managed NA default ([D66], human decision 547a4e7d):
        // a default deployment must not silently lose cross-region recovery.
        std::env::remove_var(FALLBACK_REGIONS_ENV);
        assert_eq!(resolve_fallback_regions(), vec!["us-east1", "us-west1"]);

        // Explicitly empty ⇒ deliberate same-region-only opt-out.
        std::env::set_var(FALLBACK_REGIONS_ENV, "");
        assert!(resolve_fallback_regions().is_empty());

        // Invalid entries drop; order and dedup hold.
        std::env::set_var(
            FALLBACK_REGIONS_ENV,
            "us-west1, not-a-region ,us-east1,us-west1",
        );
        assert_eq!(resolve_fallback_regions(), vec!["us-west1", "us-east1"]);

        match prior {
            Some(v) => std::env::set_var(FALLBACK_REGIONS_ENV, v),
            None => std::env::remove_var(FALLBACK_REGIONS_ENV),
        }
    }

    #[test]
    fn parse_vcpus_reads_last_segment() {
        assert_eq!(parse_vcpus_from_instance_type("n2-standard-8"), Some(8));
        assert_eq!(parse_vcpus_from_instance_type("c3-standard-44"), Some(44));
        assert_eq!(parse_vcpus_from_instance_type("bogus"), None);
    }

    #[test]
    fn default_workspace_vm_limit_targets_16gb_per_job() {
        // 4 GB/vCPU standard shapes ⇒ vcpus/4 workspace VMs ≈ 16 GB each ([D54]).
        assert_eq!(default_workspace_vm_limit(22), 5, "c3-standard-22 (88 GB)");
        assert_eq!(default_workspace_vm_limit(8), 2, "n2-standard-8 (32 GB)");
        assert_eq!(
            default_workspace_vm_limit(44),
            11,
            "c3-standard-44 (176 GB)"
        );
        // Small shapes floor at one VM rather than zero.
        assert_eq!(default_workspace_vm_limit(4), 1, "c3-standard-4 (16 GB)");
        assert_eq!(default_workspace_vm_limit(1), 1);
    }

    #[test]
    fn classify_provision_error_matches_canonical_gcp_reason() {
        // GCP can fold the reason into `error.message` ("...
        // ZONE_RESOURCE_POOL_EXHAUSTED ...") or use the human phrase only
        // ("does not have enough resources"). Both must classify as
        // zone_capacity_exhausted regardless of casing.
        let canonical = CloudError::Api {
            status: 503,
            message: "The zone us-central1-a does not have enough resources \
                       available to fulfill the request."
                .into(),
        };
        assert_eq!(
            classify_provision_error(&canonical),
            Some("zone_capacity_exhausted")
        );

        let reason_in_message = CloudError::Api {
            status: 503,
            message: "Operation failed: ZONE_RESOURCE_POOL_EXHAUSTED_WITH_DETAILS".into(),
        };
        assert_eq!(
            classify_provision_error(&reason_in_message),
            Some("zone_capacity_exhausted")
        );

        let mixed_case = CloudError::Api {
            status: 503,
            message: "Zone_Resource_Pool_Exhausted (mixed case)".into(),
        };
        assert_eq!(
            classify_provision_error(&mixed_case),
            Some("zone_capacity_exhausted")
        );
    }

    #[test]
    fn classify_provision_error_returns_none_for_unrelated_failures() {
        // Generic 5xx, auth-denied, timeout, transport — none of these are
        // "no capacity", so the column must stay NULL and the UI must keep
        // showing "Provisioning" until the reconciler advances the row.
        assert!(classify_provision_error(&CloudError::Api {
            status: 500,
            message: "internal server error".into(),
        })
        .is_none());
        assert!(classify_provision_error(&CloudError::AuthDenied {
            status: 403,
            scope: "compute".into(),
            detail: "permission denied".into(),
        })
        .is_none());
        assert!(classify_provision_error(&CloudError::Timeout {
            operation: "insert instance".into(),
        })
        .is_none());
        assert!(classify_provision_error(&CloudError::Transport {
            message: "tcp reset".into(),
        })
        .is_none());
    }
}
