# Host rotation and provisioning recovery after quota changes and cloud failures (issue #627)

Issue: G5Labs/G5-Lean#627. Audit baseline `d048743f3`; branch baseline `49d0187d` (current main).
Governing blocks: [g5.runtime.host.rotation](../../ontology/software_architecture/host_control_plane.g5.md#g5.runtime.host.rotation),
[g5.runtime.host.cloud-linkage](../../ontology/software_architecture/host_control_plane.g5.md#g5.runtime.host.cloud-linkage),
[g5.runtime.host.cloud-recovery](../../ontology/software_architecture/host_control_plane.g5.md#g5.runtime.host.cloud-recovery),
[g5.runtime.host.compensation](../../ontology/software_architecture/host_control_plane.g5.md#g5.runtime.host.compensation),
[g5.architecture.host_demand_capacity](../../ontology/software_architecture/host_control_plane.g5.md#g5.architecture.host_demand_capacity),
[g5.runtime.placement.capacity](../../ontology/software_architecture/host_control_plane.g5.md#g5.runtime.placement.capacity),
[g5.hcp.provisioning](../../ontology/software_architecture/host_control_plane.g5.md#g5.hcp.provisioning).

## Problem

Several recovery paths can leave hosts permanently outside every retry, replacement or
cleanup lane (issue findings S1/S2/PL-08, S18, S33/S20, S19/S31, S3, S7/S8/S17, S30,
S34/S35). Each defect below was confirmed against the current code; no finding was taken
on the issue's word alone.

1. **Silent pairing refusal + growth bound misapplied to renewal (S1/S2, PL-08).**
   `provision_replacement` (`cloud/rotation.rs`) returns `Ok(())` for three distinct
   outcomes — provisioned, ineligible, over the automatic-resource bound — so callers
   count `paired`, log success, and (in the resumability sweep) finalize teardown after a
   no-op. Its bound counts **live** automatic hosts against `VM quota + 1`, so lowering
   the quota below the current host count refuses every credential-deadline pairing
   forever. `g5.architecture.host_demand_capacity` bounds *undeleted uncertain or failed*
   resources, not retained live hosts; renewal overlap is separately allowed per retained
   host. The misapplied bound violates that text.
2. **Unreachable hosts fall out of every lane (S1).** `mark_unreachable` flips
   `ready → unreachable` after 30s of heartbeat silence, but: the paired-await sweep
   selects only `status='ready'` old hosts; `forced_quiesce` CASes only `ready →
   draining`; the stale-host silent reap (Pass 2) selects only `status='ready'` with a
   ≥20-min-stale heartbeat — unreachable at 30s means that selector can never match. An
   unreachable predecessor therefore freezes its pair, and the chain guard in
   `provision_replacement` (`NOT EXISTS prev … status <> 'terminated'`) then blocks the
   successor's own renewal indefinitely.
3. **Stockout retry leaves incoherent rows (S18).** The retry claim consumes
   `provision_next_retry_at` but retains `provision_error_code='zone_capacity_exhausted'`.
   If the new attempt fails unclassified (auth, transport, 5xx, persistence), the row is
   excluded from the retry selector (needs a schedule), the exhausted-freeze selector
   (same), and the orphan reconciler (`provision_error_code IS NULL` filter) — while the
   cloud-orphan sweep treats the live row as owner of any stranded instance. Nothing
   converges.
4. **Zone selection assumes `{region}-a` exists (S33).** `us-east1`/`europe-west1` have
   no `-a` zone. A non-stockout insert error (404 unknown zone) bypasses the [D59] zone
   fallback entirely, and `list_instances_by_prefix` aborts the whole region's orphan
   sweep when the configured primary zone fails to list.
5. **All 403s read as auth denial (S20).** GCP returns 403 for documented rate limits
   (`rateLimitExceeded`/`userRateLimitExceeded`); `api_error` maps every 401/403 to
   `AuthDenied` (never retried, alarming audit event).
6. **Compensation deletes live hosts (S19).** `accept_hello` flips
   `provisioning → ready` before linkage persistence can land; bring-up's `Ok(None)`
   compensation branch then sees a live row with NULL linkage, concludes `owned_by_row =
   false`, and terminates the just-registered host's instance. This directly violates
   `g5.runtime.host.compensation` ("leave uncertain ownership — including live, unlinked
   hosts — for reconciliation"; migration 073's orphan attach exists exactly for this row
   shape).
7. **Stale failure probe authorizes deallocation (S31).** The paired-await sweep's
   dissolution branch probes the replacement, then in later transactions deallocates it —
   with `deallocate_host` matching *any* live status, so a replacement that registered
   between probe and dissolve is killed from stale evidence.
8. **Queued capacity claims reaped as abandoned (S3).** A managed-capacity claim waiting
   for a launch lane (all `MAX_PARALLEL` advisory lanes busy) has no bring-up lease, so
   the orphan reconciler terminates it as `abandoned_bringup` after ~6 min — inside the
   15-min claim window its owner (the capacity sweep) is still honoring.
9. **Lost refusal detail, false success, mixed clocks (S7/S8/S17).** Later waits
   overwrite `last_error` wholesale, discarding the host's refusal `host_reason`;
   rotation logs "replacement provisioned" on no-ops (fixed by 1); `host_refusals` expiry
   is written with the DB clock but read with the app clock.
10. **Cross-region retry into unusable regions (S30).** Placement (`place()` and the
    capacity sweep) serves exactly the org's configured `worker_host_region`; the [D66]
    fallback basket retries a stocked-out row into `us-east1`/`us-west1`, producing paid
    capacity placement never uses.
11. **Delete outcomes misreported (S34/S35).** `terminate_instance` reports a delete-op
    poll timeout as failure (it is pending) and an operation-level "already gone" error as
    failure (cleanup succeeded) — the latter permanently blocks `cleanup_retired`'s
    `cloud_deleted_at` confirmation and therefore the automatic-resource bound release.

## Design

Ownership is unchanged: rotation owns pairing/drain, the stale-host detector owns reaping
and the [D66] retry, the reconciler owns linkage recovery, the capacity sweep owns its
claims, the GCP adapter owns provider classification. No new loop, table, or lifecycle is
added; every fix tightens an existing owner's contract. Per-fix:

1. **Explicit pairing outcome.** `provision_replacement` returns
   `ReplacementOutcome::{Provisioned{replacement_id}, Deferred{reason}}` (reasons:
   `ineligible`, `cloud_cleanup_backlog`). Callers count `paired`/`replaced` only on
   `Provisioned`; a deferral emits a `worker_host.rotation_pairing_deferred` audit warn
   (the durable, actionable wait) and a new `RotationPass::pairing_deferred` counter. The
   resumability sweep still finalizes its already-draining leftover (it can never serve
   again) but reports the replacement outcome truthfully.
2. **Renewal overlap is not growth.** The pairing bound counts only automatic *debris* —
   `status='terminated' AND cloud_deleted_at IS NULL` rows — against `quota + 1`
   (the `g5.architecture.host_demand_capacity` sentence as written). Retained live hosts
   no longer gate renewal, so quota reduction cannot defeat the deadline-drain policy.
   One-overlap-per-retained-host stays enforced by the `rotation_replaced_by` marker and
   the predecessor chain guard.
3. **Unreachable hosts stay in the lanes.** `forced_quiesce` CASes from
   `('ready','unreachable')`; the paired-await sweep selects old hosts in both statuses
   and uses the forced lane for unreachable ones; stale-host Pass 2 reaps silent hosts in
   both statuses (its ≥20-min heartbeat threshold is now reachable). An unreachable
   predecessor is thus drained by its own pair or reaped by Pass 2, unfreezing the
   successor's chain without weakening the chain guard.
4. **Coherent retry state.** The retry claim's guarded UPDATE also clears
   `provision_error_code` — classification and schedule are consumed together. A
   post-claim unclassified failure leaves a plain unlinked provisioning row that ordinary
   reconciliation attaches (instance accepted) or bounds (`abandoned_bringup`). Migration:
   `list_orphan_bringup_worker_hosts` additionally returns live unlinked rows with the
   stale classification shape (`zone_capacity_exhausted` + NULL schedule) so pre-existing
   strands attach; the reconciler never miss-counts rows carrying a classified code
   (freeze semantics and the zero-capacity alert are preserved);
   `attach_orphan_worker_host_cloud_refs` clears
   `provision_error_code`/`provision_next_retry_at`/`cloud_gone_since` like the 4-arg
   success setter. The zero-capacity alert's healthy-in-flight predicate also accepts a
   fresh bring-up lease so a mid-flight retry is not a false alarm.
5. **Real zones.** `provision_instance` expands its candidate set from the region's UP
   zones on `NotFound` (unknown/invalid zone) exactly as on stockout, so an invalid
   configured suffix degrades to real zones instead of failing closed.
   `list_instances_by_prefix` iterates UP zones (primary-first when the primary is UP)
   instead of unconditionally prepending a possibly-invalid primary.
6. **Reason-code classification.** `api_error` keeps 401 and non-rate-limit 403 as
   `AuthDenied`; 403 bodies carrying GCP rate-limit reasons map to `Api` and the request
   core retries them with the same backoff as 429.
7. **Compensation leaves live unlinked rows.** The `Ok(None)` branch compensates only
   when the row is terminated/absent or live with a *different* linkage; a live row with
   NULL linkage is left to the migration-073 orphan attach (deterministic name → this
   row), restoring `g5.runtime.host.compensation` conformance.
8. **Fresh re-verify before dissolution.** Dissolving a failed pair runs in one
   transaction: admission lock → old row `FOR UPDATE` → replacement `FOR UPDATE` →
   re-evaluate the terminal-failure predicate → deallocate + clear marker. A replacement
   that became ready (or re-entered retry) in the window is left alone.
9. **Claims belong to the capacity sweep.** The orphan selector and the
   miss/terminate setters exclude rows with `capacity_start_intent_id IS NOT NULL OR
   capacity_floor_claim`; `drive_claim`'s own reconciliation/abandonment (15-min window,
   needed-demand check) is the single owner, so queue delay can never become
   `capacity_failed`.
10. **Refusal diagnostics.** `host_refusals` expiry is compared against the database
    clock (passed into `merged_excluded_host_ids` by both placement and the capacity
    sweep); wait-reason overwrites in `place()` and `set_wait` carry forward an existing
    `host_reason` detail. `deallocate_host` clears `reported_live_vm_ids` on finalize
    (definite terminal event ⇒ no phantom inventory).
11. **Placement-respecting fallback.** The [D66] cross-region walk only targets basket
    regions equal to the org's configured placement region (`worker_host_region`); when
    none qualifies — the common case, since a row usually already sits in the placement
    region — retries stay in-region until the budget freezes the row. Capacity in a
    region placement cannot use is cost without benefit; the basket keeps expressing the
    jurisdiction limit.
12. **Honest delete outcomes.** On a delete-operation poll error or timeout,
    `terminate_instance` confirms absence with one GET: 404 ⇒ success (covers
    operation-level already-gone), otherwise the original error propagates (pending stays
    pending, unknown stays uncertain). This also unblocks `cleanup_retired`'s
    `cloud_deleted_at` confirmation and with it the automatic-resource bound release.

### Explicitly out of scope

- Changing [D66] freeze semantics (budget-lapsed rows keep `provisioning` + truthful
  badge; the zero-capacity alert depends on it).
- Multi-region placement; the fallback fix only filters targets by the existing
  single-region placement policy.
- Live-GCE validation: tests run through `MockCloudProvider` and wiremock against the
  real GCP adapter code; deployment/live validation is recorded separately per the issue.

### Migration

One migration (`096_recover_quota_and_cloud_failures.sql`, renumbered from 093 after main merged its own 093–095; following 073's
`SET ROLE authz_lookup_definer` snapshot/restore pattern): recreate
`list_orphan_bringup_worker_hosts` (claim exclusion + bounded stale-classification
inclusion; projection unchanged), `set_worker_host_orphan_check_miss` and
`terminate_abandoned_worker_host(uuid,int)` (claim exclusion, defense in depth), and
`attach_orphan_worker_host_cloud_refs` (clear stale classification on attach). All
changes widen/narrow WHERE clauses only; the function projections are unchanged, so
older replicas stay compatible during rolling deploy.

### Review corrections (Codex gpt-6-astra, xhigh)

These review findings were validated against the execution paths and folded in:

1. **Claim exclusion narrowed to provisioning rows.** The orphan selector's
   claim exclusion also caught a claim host that *registered* while unlinked —
   and `drive_claim` recovers `provisioning` rows only, so such a host would
   have kept a NULL linkage forever (rotation/retirement both require it).
   The exclusion now applies only while the claim is a provisioning row; a
   registered claim re-enters ordinary linkage recovery (attach-only).
2. **Zone fallback restricted to create-request failures.** The original
   combined insert+poll result let a 404 from *polling an accepted insert's
   operation* trigger the sibling-zone walk — a duplicate VM for the same
   identity. The phases are now judged separately: request-level stockout/404
   walks zones; post-acceptance uncertainty (poll timeout, operation 404)
   retains the deterministic linkage for reconciliation.
3. **Unreachable lane re-verified under the row lock.** The await sweep's
   forced-lane choice used the selector's stale status; a paired host that
   reconnected between select and quiesce would have been force-drained while
   serving. `below_cliff_quiesce` decides the lane from the current status
   under `FOR UPDATE`: still-unreachable ⇒ forced semantics, recovered-ready ⇒
   the idle busy re-verify.
4. **Retry claim renews the bring-up lease.** Clearing the classification
   while the prior lease was already stale opened an orphan-abandonment window
   until the detached bring-up's first stamp; the claim UPDATE now stamps
   `bringup_heartbeat_at` like the managed-capacity claim does.
5. **Silent reap requires cloud ownership.** The Pass 2 selector never
   required `cloud_instance_id`; latent while the lane was `ready`-only dead
   code, activating `unreachable` would have reaped self-hosted registrations
   (violating `g5.architecture.host_idle_retirement`) and provisioned
   unintended cloud replacements. The selector now requires cloud linkage;
   unlinked rows stay with the orphan machinery.
6. **Exhausted assigned claims still reconcile identity.** A demand-assigned
   claim at the attempt limit with lost linkage had no launcher
   (`retry_due=false`, assigned ⇒ skipped by the abandoned-claim sweep) once
   the orphan pass stopped covering claims; the exhaustion-evidence branch now
   launches `drive_claim` for any assigned unlinked provisioning claim —
   attach-only past the budget, no extra inserts, no attempt consumed.
7. **Failed-boot inventory repairs on PROVEN exits only** (Codex PR review,
   round 2, amended for the [mixed-version replay blocker](https://github.com/G5Labs/G5-Lean/pull/637#issuecomment-6280438603)).
   Governing [event identity](../../ontology/software_architecture/hosts.g5.md#g5.runtime.events)
   stays strict: keep the legacy `BootFailed { reason }` payload and have the
   host append a separately sequenced `Exited` after it only when cleanup
   proves exit. Journal both facts atomically so restart cannot lose the proof
   after recording the failure. The API's existing incarnation-fenced release
   makes the second fact inventory-only once the failure settled; it cannot
   overwrite diagnostics, settle work twice or release a newer incarnation.
   Unconfirmed cleanup (`CancelledChildReapUnconfirmed`) emits only BootFailed
   and retains reported occupancy, including when a Stop raced with boot.
   Proven cleanup completing a requested Stop keeps its existing exit-only outcome.
   `Exited` and confirmed-kill `StaleKilled` retract matching inventory.
   An optional proof field was insufficient: an older API silently stripped
   it before durable storage, so lost-ack replay to a newer API conflicted.
   Separate existing facts preserve proof and replay identity without adding
   negotiation or accepting changes to the payload at an existing sequence.
   This replaces the undeployed proof-field design; validation must cross the
   legacy decoder/storage boundary and then replay through the control stream
   before checking normal heartbeats, strict conflicts and unchanged custody.
8. **Classified-row recovery is bounded by the miss counter, not wall clock**
   (CodeRabbit on PR #637, two rounds). A time window either polls budget-frozen
   rows forever (no window) or abandons pre-migration strands older than the
   window — only the GCE lookup can tell the two apart. Stale-classified rows
   therefore share the ordinary miss counting: at most the orphan threshold of
   confirmed-missing lookups in total, attaching an eligible instance (live
   state, row still unlinked, bring-up lease stale) when the lookup confirms
   it, and the terminate setter keeps refusing classified rows so the [D66] badge is
   never converted to abandonment.

### Acceptance mapping (issue checklist → tests)

Tests live in the `integration` target (`api_worker_hosts_cloud.rs`,
`api_worker_hosts_stale.rs`, `api_host_capacity.rs`) through the real
reconciler/rotation/stale-host entry points over `MockCloudProvider`, plus wiremock unit
tests inside `gcp.rs` for adapter behavior:

- Quota lowered below busy-host count ⇒ cliff pairing still provisions; debris over the
  bound ⇒ `Deferred(cloud_cleanup_backlog)` audit + no paired count + no teardown.
- Unreachable paired predecessor drains via the await sweep; unreachable silent host is
  reaped by Pass 2; successor renewal unblocks.
- Stockout → auth/transport/5xx/persistence follow-ups: row either attaches (instance
  accepted) or reaches `abandoned_bringup`/frozen badge; stranded-instance row with stale
  classification attaches after the migration.
- Region fixture without `-a` (wiremock): provisioning falls back to real zones;
  prefix listing still sweeps real zones.
- Hello between insert and linkage persistence ⇒ no compensation terminate; replacement
  turned ready between probe and dissolve ⇒ not deallocated.
- Claim queued past the orphan threshold without a lease survives (not reaped).
- 403 rate-limit classifies as retryable `Api`; delete-op timeout with absent instance ⇒
  success; competing delete ⇒ success; still-present ⇒ original error.
- Refusal expiry honored under a skewed app clock (DB `now()` passed through);
  `host_reason` survives a later wait overwrite.
- Failed-boot replay across API versions passes through a frozen legacy decoder,
  PostgreSQL ingestion and the real control stream, with lost acknowledgements
  before/after the exit fact, subsequent heartbeat traffic, unchanged newer
  incarnation custody and strict conflicting-event rejection
  (`boot_failure_replay_across_api_versions_preserves_exit_evidence`). Host tests
  exercise the real boot completion and disk journal with a scripted launcher:
  proven/unproven cleanup, racing Stop, atomic append failure, restart and partial
  acknowledgement. These checks passed locally; they are not live deployment evidence.
