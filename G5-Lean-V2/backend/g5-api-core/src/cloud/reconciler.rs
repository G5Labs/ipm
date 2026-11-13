//! Background worker-host status reconciler (PR-rev-7-9c).
//!
//! Cloud bring-up ([D22]) is fire-and-forget: the POST issues the instance
//! insert and returns `201` immediately, leaving the host at
//! `status='provisioning'`. This task closes the loop. Every
//! `G5_RECONCILER_INTERVAL_SECS` (default 30s) it runs two passes:
//!
//! 1. [`reconcile_once`] — the standard pass: every `provisioning` host that
//!    has a `cloud_instance_id` is polled via `get_instance_status` and
//!    transitioned per ontology [D20]:
//!
//!    - cloud `Running` AND `last_heartbeat IS NOT NULL` → `ready`
//!    - cloud `Terminated`/404 → the [D61] fallback-aware gone path: by-name
//!      re-search (repoint if the [D59] fallback moved the instance), then
//!      `terminated` only after `GONE_CONFIRMATION_SECS` of wall-clock
//!      sustained gone-ness
//!
//! 2. [`reconcile_orphans_once`] — every non-terminated host with no classified
//!    provisioning error whose `cloud_instance_id IS NULL` after the 60s grace
//!    window and whose bring-up lease is absent or 90s stale is looked up by
//!    deterministic instance name (`host_instance_name`, [D44]), with a
//!    fallback lookup under the legacy `g5-host-{host_id}` shape. Found
//!    alive → attach the cloud_instance_id, including for already-ready hosts.
//!    Not found under either name → only provisioning rows increment a
//!    persistent miss counter and terminate at `ORPHAN_TERMINATE_MISS_THRESHOLD`
//!    with `provision_error_code='abandoned_bringup'`; other live rows are left alone.
//!
//! Readiness is heartbeat-gated: a booted VM that has not reported in stays
//! `provisioning`. The reconciler is a no-op when no cloud backend is wired
//! (`AppState.cloud_provider == None`).
//!
//! Recovery contracts (formerly [D39]/[D60]/[D61]):
//! `.g5/ontology/software_architecture/worker_host.g5.md`,
//! `g5.architecture.host_cloud_linkage` and `g5.architecture.host_insert_acceptance`.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::InstanceState;
use crate::services::AppState;

/// Number of consecutive `get_instance_by_name` → `Ok(None)` observations
/// before the orphan-reconciler terminates a row with
/// `provision_error_code='abandoned_bringup'` (ontology [D39]). With the
/// default 30s tick, 10 misses ≥ 5 minutes of confirmed-missing observation.
///
/// **Known limitation (deferred follow-up per [D39]).** With N>1 API replicas
/// each running `spawn_reconciler`, every replica's tick increments the
/// miss counter independently — so the effective threshold becomes "10 obs
/// across replicas" instead of "10 ticks". For N=2 the row terminates in
/// ~2.5 min instead of ~5 min, still past the 60s grace window but without
/// a full five-minute observation window. Non-provisioning rows never
/// count misses or terminate through this path.
pub const ORPHAN_TERMINATE_MISS_THRESHOLD: i32 = 10;

/// Wall-clock seconds a provisioning row's cloud instance must be
/// continuously confirmed gone (direct GET says `Terminated`/404 AND the
/// by-name re-search across the region's UP zones finds nothing alive) before
/// the row transitions to `terminated` (ontology [D61], issue #1512). One
/// observation is not enough: during a [D59] stockout fallback the linked
/// (failed) primary-zone attempt reads as gone while the sibling-zone insert
/// is still in flight. Wall-clock — not an observation counter — so the
/// confirmation window is identical under any API replica count (Codex
/// design-review blocker 2); the value is passed into
/// `authz.terminate_gone_worker_host` so SQL and Rust share one source of
/// truth. 90s = three default ticks of sustained gone-ness.
pub const GONE_CONFIRMATION_SECS: i32 = 90;

/// Env var overriding the reconciler poll interval (seconds).
pub const RECONCILER_INTERVAL_ENV: &str = "G5_RECONCILER_INTERVAL_SECS";
/// Default reconciler poll interval (ontology [D20]).
pub const DEFAULT_RECONCILER_INTERVAL_SECS: u64 = 30;
/// Floor for the poll interval — a misconfigured tiny value would hammer both
/// the DB and the cloud API.
pub const MIN_RECONCILER_INTERVAL_SECS: u64 = 5;

/// Outcome of a single reconcile pass, returned so tests (and logs) can assert
/// exactly what changed without waiting on the spawned loop.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReconcilePass {
    /// Provisioning hosts with a `cloud_instance_id` examined this pass.
    pub checked: usize,
    /// Hosts transitioned `provisioning → ready`.
    pub marked_ready: usize,
    /// Hosts transitioned `provisioning → terminated`.
    pub marked_terminated: usize,
    /// Hosts whose linkage was repointed to the zone the by-name re-search
    /// found the instance in ([D61] — the [D59] fallback placed it there).
    pub repointed: usize,
    /// Hosts observed confirmed-gone this pass whose wall-clock confirmation
    /// window ([`GONE_CONFIRMATION_SECS`]) has not lapsed yet ([D61]).
    pub gone_pending: usize,
    /// `get_instance_status` calls that errored (left unchanged, retried next).
    pub errors: usize,
}

/// Row shape projected by `authz.list_provisioning_worker_hosts()` (migration
/// 171): `(host_id, org_id, region, cloud_instance_id, has_heartbeat,
/// cloud_gone_since)`.
type ProvisioningHostRow = (
    Uuid,
    Uuid,
    String,
    Option<String>,
    bool,
    Option<DateTime<Utc>>,
);

/// One candidate row pulled from `worker_hosts`.
struct Candidate {
    host_id: Uuid,
    org_id: Uuid,
    region: String,
    cloud_instance_id: String,
    has_heartbeat: bool,
    cloud_gone_since: Option<DateTime<Utc>>,
}

/// Resolve the poll interval from the environment, flooring at
/// [`MIN_RECONCILER_INTERVAL_SECS`].
#[must_use]
pub fn resolve_interval() -> Duration {
    let secs = std::env::var(RECONCILER_INTERVAL_ENV)
        .ok()
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .map(|configured| configured.max(MIN_RECONCILER_INTERVAL_SECS))
        .unwrap_or(DEFAULT_RECONCILER_INTERVAL_SECS);
    Duration::from_secs(secs)
}

/// Run a single reconcile pass. Pub so integration tests can drive the state
/// machine deterministically (no sleep, no spawned loop). A no-op (returns a
/// zeroed [`ReconcilePass`]) when no cloud backend is wired.
pub async fn reconcile_once(state: &AppState) -> ReconcilePass {
    let Some(cloud) = state.cloud_provider.clone() else {
        return ReconcilePass::default();
    };

    // Provisioning hosts that have a backing cloud instance to poll. Runs on
    // the system pool through a SECURITY DEFINER function (migration 081):
    // `worker_hosts` uses FORCE RLS and the system pool is NOBYPASSRLS, so a
    // direct cross-org SELECT would be filtered to zero rows (see [I9]).
    let rows: Vec<ProvisioningHostRow> = match sqlx::query_as(
        "SELECT host_id, org_id, region, cloud_instance_id, has_heartbeat, cloud_gone_since \
             FROM authz.list_provisioning_worker_hosts()",
    )
    .fetch_all(&state.system_pool)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!(error = %e, "reconciler: failed to query provisioning hosts");
            return ReconcilePass::default();
        }
    };

    let candidates: Vec<Candidate> = rows
        .into_iter()
        .filter_map(
            |(host_id, org_id, region, cloud_instance_id, has_heartbeat, cloud_gone_since)| {
                cloud_instance_id.map(|cloud_instance_id| Candidate {
                    host_id,
                    org_id,
                    region,
                    cloud_instance_id,
                    has_heartbeat,
                    cloud_gone_since,
                })
            },
        )
        .collect();

    let mut pass = ReconcilePass {
        checked: candidates.len(),
        ..ReconcilePass::default()
    };

    for c in candidates {
        match cloud.get_instance_status(&c.cloud_instance_id).await {
            Ok(InstanceState::Running) if c.has_heartbeat => {
                // [D61]: ANY alive direct observation clears a pending gone
                // window — including the ready arm. If `transition` fails
                // transiently the row stays `provisioning`; a retained stale
                // window would let one later Terminated observation kill it
                // without a fresh confirmation period.
                if c.cloud_gone_since.is_some() {
                    reset_gone_window(state, &c).await;
                }
                if transition(state, c.host_id, "ready").await {
                    pass.marked_ready += 1;
                    tracing::info!(
                        target: "g5_audit",
                        event = "worker_host.reconciled_ready",
                        host_id = %c.host_id,
                        org_id = %c.org_id,
                        cloud_instance_id = %c.cloud_instance_id,
                        "Worker host activated (cloud Running + heartbeat present)"
                    );
                }
            }
            Ok(InstanceState::Terminated) => {
                handle_gone_observation(state, cloud.as_ref(), &c, &mut pass).await;
            }
            // Running-without-heartbeat, Provisioning, Stopping, Unknown → wait.
            // An alive observation breaks any prior confirmed-gone streak
            // ([D61]) — misses must be consecutive to terminate.
            Ok(alive_or_transient) => {
                if c.cloud_gone_since.is_some()
                    && matches!(
                        alive_or_transient,
                        InstanceState::Running | InstanceState::Provisioning
                    )
                {
                    reset_gone_window(state, &c).await;
                }
            }
            Err(e) => {
                pass.errors += 1;
                tracing::warn!(
                    error = %e,
                    host_id = %c.host_id,
                    cloud_instance_id = %c.cloud_instance_id,
                    "reconciler: get_instance_status failed; will retry next pass"
                );
            }
        }
    }

    pass
}

/// Standard-pass handling of a `Terminated`/404 observation on a provisioning
/// row (ontology [D61], issue #1512). NOT a one-shot kill: during a [D59]
/// stockout fallback the linked (failed) primary-zone attempt reads as gone
/// while the sibling-zone insert is still in flight — so first re-search the
/// instance by name across the region's UP zones (alive-preferred), repoint
/// the linkage if the instance is alive elsewhere, and otherwise terminate
/// only after [`GONE_CONFIRMATION_SECS`] of wall-clock sustained gone-ness.
/// Every write is CAS-guarded on status + the expected linkage, so no replica
/// interleaving can kill a repointed/alive row.
async fn handle_gone_observation(
    state: &AppState,
    cloud: &dyn super::CloudProvider,
    c: &Candidate,
    pass: &mut ReconcilePass,
) {
    // The linkage is the `{zone}/{name}` encoding ([D19]); the name half keys
    // the by-name re-search. A malformed linkage (never produced by
    // `encode_instance_id`, but defend anyway) skips the re-search and falls
    // through to the wall-clock confirmation — still no one-shot kill.
    let name = c.cloud_instance_id.split_once('/').map(|(_, name)| name);

    if let Some(name) = name {
        match cloud.get_instance_by_name(&c.region, name).await {
            Ok(Some(found))
                if matches!(
                    found.state,
                    InstanceState::Running | InstanceState::Provisioning
                ) =>
            {
                let new_linkage = format!("{}/{}", found.zone, name);
                if new_linkage == c.cloud_instance_id {
                    // Contradiction: the direct GET said gone, the by-name
                    // probe of the SAME zone says alive — a flapping read.
                    // Treat as transient; clear any pending window and retry.
                    if c.cloud_gone_since.is_some() {
                        reset_gone_window(state, c).await;
                    }
                    return;
                }
                let repointed: Option<Uuid> = sqlx::query_scalar::<_, Option<Uuid>>(
                    "SELECT authz.repoint_worker_host_cloud_instance($1, $2, $3)",
                )
                .bind(c.host_id)
                .bind(&c.cloud_instance_id)
                .bind(&new_linkage)
                .fetch_one(&state.system_pool)
                .await
                .unwrap_or_else(|e| {
                    tracing::warn!(error = %e, %c.host_id, "reconciler: repoint setter failed");
                    None
                });
                if repointed.is_some() {
                    pass.repointed += 1;
                    tracing::info!(
                        target: "g5_audit",
                        event = "worker_host.reconciled_repointed",
                        host_id = %c.host_id,
                        org_id = %c.org_id,
                        old_cloud_instance_id = %c.cloud_instance_id,
                        new_cloud_instance_id = %new_linkage,
                        instance_state = ?found.state,
                        "Worker host linkage repointed to the zone the instance was found in ([D59] fallback moved it)"
                    );
                }
                return;
            }
            Ok(Some(transient)) if !matches!(transient.state, InstanceState::Terminated) => {
                // Stopping / Unknown / future variants: the instance exists in
                // some zone but is not attachably alive — transient, no stamp.
                tracing::info!(
                    target: "g5_audit",
                    event = "worker_host.reconciled_gone_transient",
                    host_id = %c.host_id,
                    org_id = %c.org_id,
                    instance_state = ?transient.state,
                    "Gone re-search found a transient-state instance; retry next pass"
                );
                return;
            }
            // `Ok(None)` / found-Terminated: confirmed gone everywhere — fall
            // through to the wall-clock confirmation.
            Ok(_) => {}
            Err(e) => {
                // Lookup failure: we could not confirm the instance is gone —
                // do not open/advance the window on unverified evidence.
                pass.errors += 1;
                tracing::warn!(
                    error = %e,
                    host_id = %c.host_id,
                    region = %c.region,
                    "reconciler: gone re-search failed; will retry next pass (window not stamped)"
                );
                return;
            }
        }
    }

    // Confirmed gone everywhere: stamp (or keep) the wall-clock window start,
    // CAS-guarded on the expected linkage.
    let gone_since: Option<DateTime<Utc>> = sqlx::query_scalar::<_, Option<DateTime<Utc>>>(
        "SELECT authz.mark_worker_host_cloud_gone($1, $2)",
    )
    .bind(c.host_id)
    .bind(&c.cloud_instance_id)
    .fetch_one(&state.system_pool)
    .await
    .unwrap_or_else(|e| {
        tracing::warn!(error = %e, %c.host_id, "reconciler: gone-window stamp failed");
        None
    });
    // NULL → the row raced out of provisioning or was repointed; drop it.
    let Some(gone_since) = gone_since else { return };

    // Terminate only through the atomic CAS terminate: status + expected
    // linkage + lapsed window are all re-checked inside one UPDATE, so a
    // repoint/reset by any replica between the stamp and this call makes the
    // terminate a no-op.
    let confirmed: Option<DateTime<Utc>> = sqlx::query_scalar::<_, Option<DateTime<Utc>>>(
        "SELECT authz.terminate_gone_worker_host($1, $2, $3)",
    )
    .bind(c.host_id)
    .bind(&c.cloud_instance_id)
    .bind(GONE_CONFIRMATION_SECS)
    .fetch_one(&state.system_pool)
    .await
    .unwrap_or_else(|e| {
        tracing::warn!(error = %e, %c.host_id, "reconciler: gone terminate failed");
        None
    });
    match confirmed {
        Some(since) => {
            pass.marked_terminated += 1;
            tracing::info!(
                target: "g5_audit",
                event = "worker_host.reconciled_terminated",
                host_id = %c.host_id,
                org_id = %c.org_id,
                cloud_instance_id = %c.cloud_instance_id,
                gone_since = %since,
                "Worker host terminated (cloud instance gone; confirmed over the wall-clock window)"
            );
        }
        None => {
            pass.gone_pending += 1;
            tracing::info!(
                target: "g5_audit",
                event = "worker_host.reconciled_gone_observed",
                host_id = %c.host_id,
                org_id = %c.org_id,
                cloud_instance_id = %c.cloud_instance_id,
                gone_since = %gone_since,
                confirmation_secs = GONE_CONFIRMATION_SECS,
                "Cloud instance observed gone; confirmation window not lapsed yet"
            );
        }
    }
}

/// Clear the [D61] confirmed-gone window after an alive/contradictory
/// observation. Best-effort; the setter is CAS-guarded on status + the
/// expected linkage and skip-when-NULL.
async fn reset_gone_window(state: &AppState, c: &Candidate) {
    let reset: Option<Uuid> =
        sqlx::query_scalar::<_, Option<Uuid>>("SELECT authz.reset_worker_host_cloud_gone($1, $2)")
            .bind(c.host_id)
            .bind(&c.cloud_instance_id)
            .fetch_one(&state.system_pool)
            .await
            .unwrap_or_else(|e| {
                tracing::warn!(error = %e, %c.host_id, "reconciler: gone-window reset failed");
                None
            });
    if reset.is_some() {
        tracing::info!(
            target: "g5_audit",
            event = "worker_host.reconciled_gone_window_reset",
            host_id = %c.host_id,
            org_id = %c.org_id,
            "Confirmed-gone window cleared after alive observation"
        );
    }
}

/// Outcome of a single orphan-reconcile pass ([D39]).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OrphanReconcilePass {
    /// Live unlinked candidates past creation grace and bring-up lease expiry.
    pub checked: usize,
    /// Rows whose missing `cloud_instance_id` was re-attached because the GCE
    /// instance was found alive under its deterministic name.
    pub reattached: usize,
    /// Rows whose miss counter was bumped (instance reported missing this
    /// pass, but below the terminate threshold).
    pub miss_incremented: usize,
    /// Rows transitioned to `terminated` with
    /// `provision_error_code='abandoned_bringup'` after reaching the miss
    /// threshold.
    pub terminated_abandoned: usize,
    /// `get_instance_by_name` lookups that errored (treated as "wait" — the
    /// miss counter is NOT incremented and the row is retried next pass).
    pub errors: usize,
}

/// One orphan-candidate row pulled from `worker_hosts`.
struct OrphanCandidate {
    host_id: Uuid,
    org_id: Uuid,
    region: String,
    status: String,
    #[allow(dead_code)] // Surfaced in audit events; reserved for future timing diagnostics.
    created_at: DateTime<Utc>,
    #[allow(dead_code)]
    // Not consumed by the loop today; the SECURITY DEFINER setter is the source of truth.
    orphan_check_misses: i32,
}

/// Run a single orphan-reconcile pass ([D39]). Pub so integration tests can
/// drive the orphan loop deterministically (no sleep, no spawned loop). A no-op
/// when no cloud backend is wired.
///
/// The pass complements [`reconcile_once`]: where the standard pass handles
/// `cloud_instance_id IS NOT NULL` rows, this pass closes the orphan gap that
/// the pre-async-spawn `bring_up_cloud_resources` left behind — even after the
/// D22 async-spawn fix, a hard crash mid-bring-up can still leak a row whose
/// GCE instance is alive but whose `cloud_instance_id` column is NULL.
pub async fn reconcile_orphans_once(state: &AppState) -> OrphanReconcilePass {
    // Defence in depth: when cloud features are deliberately disabled (operator
    // set `ENABLE_CLOUD_FEATURES=false`), POST `/worker-hosts` legitimately
    // creates `provisioning` rows with `cloud_instance_id IS NULL` and emits
    // `worker_host.cloud_bringup_skipped` ([D24]'s 9b regression fence). Those
    // rows are NOT orphans — sweeping them would terminate every DB-only
    // smoke-test row after the grace window. Gate on the same flag the POST
    // handler checks. Follow-up to chatgpt-codex-connector's P1 finding on the
    // round-3 patch.
    if !state.cloud_features_enabled {
        return OrphanReconcilePass::default();
    }
    let Some(cloud) = state.cloud_provider.clone() else {
        return OrphanReconcilePass::default();
    };

    // Orphan candidates older than the 60s grace window. SECURITY DEFINER read
    // (migration 073) so the system pool can SELECT
    // across orgs under FORCE RLS.
    let rows: Vec<(Uuid, Uuid, String, DateTime<Utc>, i32, String)> = match sqlx::query_as(
        "SELECT host_id, org_id, region, created_at, orphan_check_misses, status \
         FROM authz.list_orphan_bringup_worker_hosts()",
    )
    .fetch_all(&state.system_pool)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!(error = %e, "reconciler: failed to query orphan-bringup hosts");
            return OrphanReconcilePass::default();
        }
    };

    let candidates: Vec<OrphanCandidate> = rows
        .into_iter()
        .map(
            |(host_id, org_id, region, created_at, orphan_check_misses, status)| OrphanCandidate {
                host_id,
                org_id,
                region,
                status,
                created_at,
                orphan_check_misses,
            },
        )
        .collect();

    // Org slugs feed the deterministic instance name ([D44]). One batch read;
    // `orgs` has no RLS. A query error skips the whole pass for this tick —
    // looking candidates up under a possibly-wrong name would miscount toward
    // the terminate threshold (mirrors the candidates-query error handling).
    let org_slugs: HashMap<Uuid, String> = if candidates.is_empty() {
        HashMap::new()
    } else {
        let mut org_ids: Vec<Uuid> = candidates.iter().map(|c| c.org_id).collect();
        org_ids.sort_unstable();
        org_ids.dedup();
        match sqlx::query_as::<_, (Uuid, String)>("SELECT id, slug FROM orgs WHERE id = ANY($1)")
            .bind(&org_ids)
            .fetch_all(&state.system_pool)
            .await
        {
            Ok(rows) => rows.into_iter().collect(),
            Err(e) => {
                tracing::warn!(error = %e, "reconciler: failed to resolve org slugs for orphan pass");
                return OrphanReconcilePass::default();
            }
        }
    };

    let mut pass = OrphanReconcilePass {
        checked: candidates.len(),
        ..OrphanReconcilePass::default()
    };

    for c in candidates {
        // Deterministic instance name via the shared `host_instance_name`
        // helper — the same function `gcp::provision_instance` calls, so the
        // two sides cannot drift ([D44]; replaces the old duplicated
        // `g5-host-{host_id}` string template). Pre-[D44] instances and
        // provisions whose slug fetch failed live under the legacy
        // `g5-host-{host_id}` name, so a primary-name 404 retries once with
        // the legacy shape before counting toward the terminate threshold.
        let slug = org_slugs.get(&c.org_id).map(String::as_str).unwrap_or("");
        let primary = super::host_instance_name(c.host_id, slug);
        let legacy = format!("g5-host-{}", c.host_id);
        let (name, lookup) = match cloud.get_instance_by_name(&c.region, &primary).await {
            Ok(None) if primary != legacy => (
                legacy.as_str(),
                cloud.get_instance_by_name(&c.region, &legacy).await,
            ),
            other => (primary.as_str(), other),
        };

        // Per D39 contract, partition the `InstanceState` reply three ways:
        //
        // * `Running | Provisioning` → "found alive" → attach + emit
        //   `worker_host.orphan_reattached`.
        // * `Terminated | Ok(None)` (GCE 404) → "missing" → only provisioning
        //   rows bump the miss counter / terminate. A `Some(Terminated)` reply
        //   (GCE 200 OK for a stopped instance) is operationally equivalent
        //   to "missing" — attaching it to a provisioning row would cause
        //   the standard pass to take the row to `terminated` WITHOUT
        //   `provision_error_code='abandoned_bringup'`, losing the
        //   orphan-pass classification. (codex-connector P2.)
        // * `Stopping | Unknown` → "transient" → no bump, no attach; retry
        //   next pass. Stopping is mid-shutdown and could still go either
        //   way; Unknown is "the backend returned a status we don't map"
        //   and is conservative-retry. `InstanceState` is `#[non_exhaustive]`,
        //   so any future variant defaults into this transient arm — a
        //   future variant that should attach or terminate must be added to
        //   the explicit arms. (round-4-subagent P2.)
        // Pre-project the FoundInstance reply onto `(state, zone)` ([D61]) so
        // the D39 three-way partition below stays intact: an alive hit carries
        // the zone it was FOUND in, a found-Terminated folds into "missing",
        // and other states stay transient.
        let lookup = lookup.map(|found| found.map(|f| (f.state, f.zone)));
        match lookup {
            Ok(Some((
                instance_state @ (InstanceState::Running | InstanceState::Provisioning),
                found_zone,
            ))) => {
                // GCE knows the instance — restore linkage for rotation and
                // teardown. Provisioning rows also rejoin the standard pass.
                // The cloud_instance_id is the deterministic `{zone}/{name}` encoding that
                // `gcp::encode_instance_id` produces, using the zone the
                // by-name search actually FOUND the instance in ([D61] —
                // re-deriving the configured primary zone here stamped a
                // wrong-zone linkage whenever the [D59] stockout fallback had
                // placed the instance elsewhere, the #1512 race). The setter
                // is guarded to `status<>'terminated'` so a raced DELETE
                // does not reattach a terminated row.
                let cloud_instance_id = format!("{found_zone}/{name}");
                let vpc_id = format!("g5-org-{}", c.org_id);
                let attached: Result<Option<Uuid>, sqlx::Error> =
                    sqlx::query_scalar::<_, Option<Uuid>>(
                        // Lease-guarded attach ([D60]): re-checks the bring-up
                        // liveness lease INSIDE the guard, so a bring-up that
                        // renewed between the orphan SELECT and this attach is
                        // never preempted (list-then-attach TOCTOU closure).
                        "SELECT authz.attach_orphan_worker_host_cloud_refs($1, $2, $3)",
                    )
                    .bind(c.host_id)
                    .bind(&vpc_id)
                    .bind(&cloud_instance_id)
                    .fetch_one(&state.system_pool)
                    .await;
                match attached {
                    Ok(Some(_)) => {
                        pass.reattached += 1;
                        tracing::info!(
                            target: "g5_audit",
                            event = "worker_host.orphan_reattached",
                            host_id = %c.host_id,
                            org_id = %c.org_id,
                            cloud_instance_id = %cloud_instance_id,
                            instance_state = ?instance_state,
                            "Orphan-reconciler attached cloud_instance_id for a row whose POST handler failed to persist it"
                        );
                    }
                    Ok(None) => {
                        // Termination, linkage persistence or lease renewal
                        // raced the SELECT; nothing to do.
                        tracing::info!(
                            target: "g5_audit",
                            event = "worker_host.orphan_skipped_raced",
                            host_id = %c.host_id,
                            org_id = %c.org_id,
                            "Orphan-reconciler attach declined (row terminated, linkage persisted, or bring-up lease renewed mid-pass)"
                        );
                    }
                    Err(e) => {
                        pass.errors += 1;
                        tracing::error!(
                            target: "g5_audit",
                            event = "worker_host.orphan_attach_failed",
                            host_id = %c.host_id,
                            org_id = %c.org_id,
                            error = %e,
                            "Orphan-reconciler attach setter failed; will retry next pass"
                        );
                    }
                }
            }
            Ok(None) | Ok(Some((InstanceState::Terminated, _))) => {
                // A stale stockout classification (issue #627 S18) flows
                // through the SAME miss counting: the counter ages it out of
                // the selector after a bounded number of confirmed-missing
                // lookups, while `terminate_abandoned_worker_host` keeps
                // refusing classified rows — the truthful [D66] badge is
                // never converted to `abandoned_bringup`.
                if c.status != "provisioning" {
                    tracing::warn!(
                        target: "g5_audit",
                        event = "worker_host.orphan_instance_missing",
                        host_id = %c.host_id,
                        org_id = %c.org_id,
                        status = %c.status,
                        "Orphan-reconciler could not find a live instance; leaving registered host unchanged"
                    );
                    continue;
                }
                // GCE 404 OR the instance is explicitly Terminated. Bump
                // the miss counter; on threshold, terminate. Both SQL setters
                // recheck provisioning status to protect a concurrent hello.
                let post_increment: Option<i32> = sqlx::query_scalar::<_, Option<i32>>(
                    "SELECT authz.set_worker_host_orphan_check_miss($1)",
                )
                .bind(c.host_id)
                .fetch_one(&state.system_pool)
                .await
                .unwrap_or_else(|e| {
                    tracing::warn!(error = %e, %c.host_id, "reconciler: orphan miss-counter setter failed");
                    None
                });
                match post_increment {
                    None => {
                        // Row no longer qualifies as orphan (handler caught up,
                        // or DELETE raced). Skip — the next tick reconciles
                        // whatever state the row is in now.
                        tracing::info!(
                            target: "g5_audit",
                            event = "worker_host.orphan_skipped_raced",
                            host_id = %c.host_id,
                            org_id = %c.org_id,
                            "Orphan-reconciler skipped row that left orphan predicate between SELECT and increment"
                        );
                    }
                    Some(misses) if misses >= ORPHAN_TERMINATE_MISS_THRESHOLD => {
                        // 2-arg since migration 171: the threshold is
                        // re-checked atomically inside the UPDATE alongside
                        // the [D60] lease fence, so a lease revival or a
                        // transient counter reset between the increment above
                        // and this call makes the terminate a no-op.
                        let terminated: Option<Uuid> = sqlx::query_scalar::<_, Option<Uuid>>(
                            "SELECT authz.terminate_abandoned_worker_host($1, $2)",
                        )
                        .bind(c.host_id)
                        .bind(ORPHAN_TERMINATE_MISS_THRESHOLD)
                        .fetch_one(&state.system_pool)
                        .await
                        .unwrap_or_else(|e| {
                            tracing::warn!(error = %e, %c.host_id, "reconciler: terminate_abandoned setter failed");
                            None
                        });
                        if terminated.is_some() {
                            pass.terminated_abandoned += 1;
                            tracing::warn!(
                                target: "g5_audit",
                                event = "worker_host.orphan_terminated",
                                host_id = %c.host_id,
                                org_id = %c.org_id,
                                misses,
                                "Orphan-reconciler terminated row after threshold confirmed-missing observations (provision_error_code=abandoned_bringup)"
                            );
                        }
                    }
                    Some(misses) => {
                        pass.miss_incremented += 1;
                        tracing::info!(
                            target: "g5_audit",
                            event = "worker_host.orphan_observed",
                            host_id = %c.host_id,
                            org_id = %c.org_id,
                            misses,
                            threshold = ORPHAN_TERMINATE_MISS_THRESHOLD,
                            "Orphan-reconciler observed missing GCE instance; below terminate threshold"
                        );
                    }
                }
            }
            Ok(Some((transient_state, _))) => {
                // Stopping / Unknown / any future #[non_exhaustive] variant.
                // The instance reply is non-terminal — do NOT attach (would
                // race a parallel terminate) and do NOT bump the miss
                // counter (the instance IS there, just in a transient
                // state). Retry next pass.
                //
                // **Round-5 follow-up (codex-connector P2):** a live-but-
                // transient observation MUST break any prior missing streak.
                // Otherwise N-1 missing ticks + 1 transient tick + 1 missing
                // tick = N misses → terminate, but those misses weren't
                // consecutive. Reset the counter to 0 via the migration-106
                // SECURITY DEFINER setter. Setter is skip-when-zero so the
                // common "transient on first tick" case is a no-op write.
                let reset: Option<i32> = sqlx::query_scalar::<_, Option<i32>>(
                    "SELECT authz.reset_worker_host_orphan_check_miss($1)",
                )
                .bind(c.host_id)
                .fetch_one(&state.system_pool)
                .await
                .unwrap_or_else(|e| {
                    tracing::warn!(error = %e, %c.host_id, "reconciler: orphan miss-counter reset setter failed");
                    None
                });
                if reset.is_some() {
                    tracing::info!(
                        target: "g5_audit",
                        event = "worker_host.orphan_miss_streak_reset",
                        host_id = %c.host_id,
                        org_id = %c.org_id,
                        instance_state = ?transient_state,
                        "Orphan-reconciler reset miss counter after transient cloud-state observation"
                    );
                }
                tracing::info!(
                    target: "g5_audit",
                    event = "worker_host.orphan_skipped_transient",
                    host_id = %c.host_id,
                    org_id = %c.org_id,
                    instance_state = ?transient_state,
                    "Orphan-reconciler skipped row whose GCE instance is in a transient state; retry next pass"
                );
            }
            Err(e) => {
                // Transport / auth / other — the row IS there, we just
                // couldn't reach the API. Don't increment; retry next pass.
                pass.errors += 1;
                tracing::warn!(
                    error = %e,
                    host_id = %c.host_id,
                    region = %c.region,
                    "reconciler: orphan-pass get_instance_by_name failed; will retry next pass (miss counter not bumped)"
                );
            }
        }
    }

    pass
}

/// Apply a guarded transition out of `provisioning` via the SECURITY DEFINER
/// function (migration 081). The function's `status = 'provisioning'` predicate
/// makes it a no-op if the host was concurrently deleted/terminated — a
/// terminated host is never resurrected. Returns `true` if a row was updated.
async fn transition(state: &AppState, host_id: Uuid, to_status: &str) -> bool {
    // `query_scalar` returns `Result<Option<T>>`, where `Option<T>` reflects
    // row presence — but the function's column is itself NULLable (no row
    // matched the guarded UPDATE → the function returns `NULL`). So the inner
    // type is `Option<Uuid>` and the resolved value is `Option<Option<Uuid>>`.
    // We flatten the outer Result via `unwrap_or_else` and rely on
    // `Option::is_some` collapsing both layers: `Some(Some(_))` → updated.
    let updated: Option<Uuid> = sqlx::query_scalar::<_, Option<Uuid>>(
        "SELECT authz.transition_provisioning_worker_host($1, $2)",
    )
    .bind(host_id)
    .bind(to_status)
    .fetch_one(&state.system_pool)
    .await
    .unwrap_or_else(|e| {
        tracing::warn!(error = %e, %host_id, to_status, "reconciler: transition failed");
        None
    });
    updated.is_some()
}

/// Spawn the reconciler loop. No-op spawn (returns a handle that finishes
/// immediately after logging) when no cloud backend is wired, so callers can
/// unconditionally spawn without an `is_some()` guard at the call site if they
/// prefer — but `main.rs` guards anyway to avoid an idle task. Honors the
/// cancellation token for graceful shutdown.
pub fn spawn_reconciler(state: Arc<AppState>, cancel: CancellationToken) -> JoinHandle<()> {
    tokio::spawn(async move {
        if state.cloud_provider.is_none() {
            tracing::debug!("reconciler: no cloud backend wired — not running");
            return;
        }
        let interval_dur = resolve_interval();
        tracing::info!(
            interval_secs = interval_dur.as_secs(),
            "worker-host reconciler started"
        );
        let mut ticker = tokio::time::interval(interval_dur);
        ticker.tick().await; // consume the immediate first tick
        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    tracing::info!("worker-host reconciler shutting down");
                    return;
                }
                _ = ticker.tick() => {
                    let pass = reconcile_once(&state).await;
                    if pass.marked_ready > 0 || pass.marked_terminated > 0 || pass.errors > 0 {
                        tracing::info!(
                            checked = pass.checked,
                            marked_ready = pass.marked_ready,
                            marked_terminated = pass.marked_terminated,
                            errors = pass.errors,
                            "reconciler pass complete"
                        );
                    }
                    // Orphan-pass ([D39], round-3, chat intent C2). Runs on
                    // every tick so a row whose cloud_instance_id was never
                    // persisted (process crash mid-bring-up, even after the
                    // D22 async-spawn fix) is either re-attached or
                    // terminated within `ORPHAN_TERMINATE_MISS_THRESHOLD`
                    // ticks past the 60s grace window.
                    let orphan = reconcile_orphans_once(&state).await;
                    if orphan.reattached > 0
                        || orphan.miss_incremented > 0
                        || orphan.terminated_abandoned > 0
                        || orphan.errors > 0
                    {
                        tracing::info!(
                            checked = orphan.checked,
                            reattached = orphan.reattached,
                            miss_incremented = orphan.miss_incremented,
                            terminated_abandoned = orphan.terminated_abandoned,
                            errors = orphan.errors,
                            "reconciler orphan pass complete"
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
    fn interval_defaults_and_floors() {
        let prior = std::env::var(RECONCILER_INTERVAL_ENV).ok();

        std::env::remove_var(RECONCILER_INTERVAL_ENV);
        assert_eq!(
            resolve_interval(),
            Duration::from_secs(DEFAULT_RECONCILER_INTERVAL_SECS)
        );

        std::env::set_var(RECONCILER_INTERVAL_ENV, "1");
        assert_eq!(
            resolve_interval(),
            Duration::from_secs(MIN_RECONCILER_INTERVAL_SECS),
            "tiny interval floors at the minimum"
        );

        std::env::set_var(RECONCILER_INTERVAL_ENV, "120");
        assert_eq!(resolve_interval(), Duration::from_secs(120));

        match prior {
            Some(v) => std::env::set_var(RECONCILER_INTERVAL_ENV, v),
            None => std::env::remove_var(RECONCILER_INTERVAL_ENV),
        }
    }

    #[test]
    fn reconcile_pass_default_is_zeroed() {
        let p = ReconcilePass::default();
        assert_eq!(p.checked, 0);
        assert_eq!(p.marked_ready, 0);
        assert_eq!(p.marked_terminated, 0);
        assert_eq!(p.errors, 0);
    }
}
