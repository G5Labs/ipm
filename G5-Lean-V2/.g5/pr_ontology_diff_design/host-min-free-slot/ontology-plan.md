# Managed capacity: keep a free slot ready

Request: [chat intent](../../chat_intent/host-min-free-slot/request.md). The owner wants a third managed-capacity rule beside demand-driven growth and empty-host removal: there is always a free, current (not stale) slot.

## Ontology changes and governance

Ontology changes are in this branch's diff against `origin/main`:

- [`g5.architecture.host_demand_capacity`](../../ontology/software_architecture/host_control_plane.g5.md#g5.architecture.host_demand_capacity) (amended): "Filling the last slot alone does not create a spare host" is replaced by the free-slot floor. Growth has two sources of authority now: a valid queued Start, and the organization's floor in its demand region while that region already has a host.
- [`g5.architecture.host_idle_retirement`](../../ontology/software_architecture/host_control_plane.g5.md#g5.architecture.host_idle_retirement) (amended): the reserve other hosts must keep is the floor (at least one free slot), not just one free slot.
- [`g5.data.configuration.org-admin-settings`](../../ontology/features/organizations.g5.md#g5.data.configuration.org-admin-settings) (amended): a seventh registered key, `min_free_slots`.

| Governing block | Why relevant | Implementation must |
| --- | --- | --- |
| [`g5.runtime.placement.capacity`](../../ontology/software_architecture/host_control_plane.g5.md#g5.runtime.placement.capacity) | Floor decisions share the admission lock with placement and the quota | Decide the floor inside `reconcile_capacity`, under `lock_admission`, from the same supply snapshot |
| [`g5.runtime.placement.runtime-identity`](../../ontology/software_architecture/hosts.g5.md) | A stale host's free slots are no capacity | Count only `runtime_current` healthy supply and in-flight claims toward the floor; refuse a floor claim while the boot image is pinned stale, as demand claims are |
| [`g5.runtime.host.cloud-linkage`](../../ontology/software_architecture/host_control_plane.g5.md#g5.runtime.host.cloud-linkage) | A floor host is a durable claim before cloud work | Reuse `prepare_host` and `drive_claim`; no second launch path |
| [`g5.data.configuration.typed`](https://github.com/G5Labs/G5-Lean/blob/3d17915df39bcf4cbe0cabc08500de63ff46fbb2/.g5/ontology/concepts/data_model.g5.md#keep-typed-configuration-limited-to-its-declared-keys) | `configs` validates registered keys | Migration 086 registers `min_free_slots` (integer 0–100), schema only |
| [`g5.security.org-admin-settings`](../../ontology/features/organizations.g5.md#g5.security.org-admin-settings) | Same administrator authority as the other knobs | The existing GET/PUT route carries the field; no new route |

## Outcome and design

An administrator sees "Free slots to keep ready" in the Admin page's Managed capacity group (default 1). When a Start takes the last free slot in the organization's region, the next sweep claims a host so a slot is free again before anyone waits. Removal never drops the region below the floor.

### The floor is one more growth reason inside the existing sweep

`WorkerHostService::reconcile_capacity` already assigns queued Starts to supply (healthy current hosts and in-flight claims) and claims hosts for the rest. After that assignment, the sweep adds one check for the demand region (`worker_host_region`, default `us-central1`):

- `free` = the unassigned slots on healthy current unpaired hosts plus in-flight claims in that region. Stale hosts and hosts being replaced contribute nothing, as in `available`.
- The floor is **active** only when the region has an established host: one that is not terminated and has a cloud instance or a registration, or one that automatic retirement removed while the floor was set (`worker_hosts.capacity_floor_presence`) whose cloud cleanup is unconfirmed or confirmed within the last hour. An operator's removal of any host in the region (`remove_host` with a real actor) clears that marker for the region, so the floor never re-creates a host an administrator just removed. `remove_host` takes the organization admission lock before its first host row, keeping one lock order with the sweep, placement and rotation. This is the "top up, never create" rule from the request record: a claim that never launched does not make a region "present", so a dead Start claim cannot seed a region's first host through the floor; an operator removing every host ends the floor; and automatic retirement of the last established host (an empty stale one, retired while the `quota + 1` bound still held the floor's claim back) hands its presence to the floor until the bound frees.
- If the floor is active and `free < min_free_slots`:
  1. A young unlaunched claim in the region (provisioning, no cloud instance, created within 15 minutes) that is not serving a Start this sweep covers the floor: it is relaunched if `retry_due`, and otherwise it is cooldown evidence after a failed attempt, so one failed floor attempt cannot turn into a claim per sweep. A claim assigned to a Start never covers the floor, so a Start's claim from an earlier sweep cannot suppress the floor's own claim; a fleet whose every slot is taken by Starts therefore gains a floor host beyond them, within the `quota + 1` bound.
  2. Otherwise the same gates as a demand claim apply, in the same order: the stale image pin, `auto_add_hosts`, the `quota + 1` resource bound and the per-sweep batch. Each refusal is logged; there is no Start to carry a wait reason.
  3. Otherwise one host is claimed through `prepare_host` with the organization's machine type, marked `automatic_capacity` with no `capacity_start_intent_id`, and launched after commit. One floor claim per sweep; the next sweep re-evaluates with it in flight.

Rejected: a separate "spare host" loop. The floor host differs from a demand claim only in what makes it necessary, so it reuses the claim's row, launch, retry and cleanup; the added state is the setting and the `worker_hosts.capacity_floor_claim` marker that identifies a claim with no Start behind it (rotation replacements of automatic hosts are automatic too, so "automatic without a Start" cannot identify a floor claim).

### Floor hosts share the claim lifecycle

`drive_claim` recovers any claim (previously only claims bound to a Start). Its "still needed" test gains the floor: a claim proceeds if an uncovered compatible Start remains **or** it sits in the demand region, is still young, and the floor would be short without it. The presence rule ("the region already has a host") gates claim creation and recovery alike. The host a floor claim was made beside may retire in the same sweep (an empty stale host does); that retirement sets the presence marker, so the claim still recovers, while an operator's removal clears the marker and a pending floor claim is abandoned with the rest. A Start's claim in a region with no established host is abandoned once its Start is gone: the floor never turns a dead claim of either kind into a region's first host. A claim that is needed by neither is terminated as unnecessary, exactly as before. The claim's own persisted region names its cloud instance and drives its bring-up; the demand region only decides need. The manual cross-region stockout retry (`provision_next_retry_at`, `stale_host.rs`) never adopts a claim of either kind: the 082 capacity guard, replaced in 086, clears that field for floor claims too. Hence:

- a demand claim whose Start was cancelled is kept when it would fill the floor anyway, instead of being terminated and re-created;
- a floor claim is abandoned when another host registers, the floor is lowered, the demand region moves, or it is older than 15 minutes without a cloud instance, even while Starts wait: unlike a Start's exhausted claim it is nobody's exhaustion evidence, so it must not hold a resource against the bound that those Starts need;
- a claim that demand is assigned to in a later sweep (for example a multi-slot floor claim that a restart left unlaunched) is launched like a Start's claim;
- a stockout retries on the same row up to three times; a non-retryable failure sits as cooldown until the 15-minute window closes, then is terminated and the floor may try again. The `quota + 1` bound still counts every undeleted automatic resource.

### Removal keeps the floor

The retirement reserve becomes: the other healthy current unpaired hosts in the region, not retired in this sweep, together have at least `max(1, min_free_slots)` free slots. With the floor at 1 this is today's rule. The retirement candidate order (retire larger hosts first, keep the smallest reserve) is unchanged. A sweep cannot both retire a current host and claim a floor host in the same region: retirement of a current host requires the others to hold the floor, and a floor claim requires the whole region to be below it. An empty stale host is the exception: it retires at once regardless of the reserve, so a stale fleet is replaced and retired in one sweep.

### Setting

| API field / `configs` key | Default | Validation | Effect |
| --- | --- | --- | --- |
| `minFreeSlots` / `min_free_slots` | 1 | integer 0–100 (API and DB) | Floor for growth and the removal reserve. 0: no floor; removal keeps one free slot as before. |

`ManagedCapacitySettings` carries it; the sweep reads it under the admission lock with the other capacity settings, and the writer takes the same lock. Migration 086 re-creates `g5_config_value_is_registered` with the new branch and rewrites no rows.

### UI

A number input "Free slots to keep ready" (0–100) in the Managed capacity fieldset, with the hint that G5 adds a host when fewer slots are free in a region that already has hosts and never removes a host that would leave fewer. The DTO field is optional so the card keeps working during a rolling deploy; the fallback is 1.

### Assumptions

- One demand region per organization (`worker_host_region`); hosts in other regions neither count toward nor receive floor capacity, since no Start goes there.
- The floor's unit is a slot, not a host; a floor larger than one host's slots is met across hosts.
- Default 1 is the owner's "always a free slot". Organizations that prefer today's behavior set 0.

## Verification

- `api_host_capacity` (real Postgres, cloud double):
  - taking the last free slot claims a floor host with no Start behind it, launched through `tick`; while it is in flight no second claim is made; with the floor at 0 the last slot is consumed without a claim (today's behavior);
  - a region with no host is not grown by the floor;
  - growth off refuses the floor claim; a stale fleet off the pin gets a current floor host and the empty stale host retires;
  - a floor claim is abandoned when a manual host registers before it launches, and a cancelled Start's claim is kept by the floor;
  - with the floor at 2 the retirement reserve keeps two free slots: of two empty hosts the small one retires and the large one stays.
- `api_org_admin_settings`: the field round-trips and 101 / -1 are rejected without writing.
- `migration_086_min_free_slots`: applied by the real migrator, accepts 0 and 100, rejects -1, 101 and a fraction, replays as a no-op.
- `org_admin_settings` unit tests cover the bounds and the fallback.
- Frontend: the card shows the default, saves the field, rejects out-of-range input; the contract test covers the schema.
- Existing growth tests whose subject is a Start's own claim pin the floor to 0 (`set_capacity(... minFreeSlots: 0)`), because with the default machine type (one slot) a Start on a full fleet now claims its host and a floor host in the same sweep.

## Implementation status

Implemented as designed on this branch and reviewed to APPROVE over six independent Codex rounds plus CodeRabbit ([review record](../../ontology-review-host-min-free-slot.md)). Executed locally against Postgres 17 on the head merged with main (2026-10-16):

- `api_host_capacity`, `api_org_admin_settings`, `migration_086_min_free_slots`, `migration_082_managed_host_capacity`, `api_worker_hosts`, `api_worker_hosts_stale` and the source-download tests: 138 passing (13 new floor tests; ten existing Start-growth tests pin the floor to 0)
- `g5-api-routes` workspace unit tests (8), `org_admin_settings` unit tests (4)
- frontend: `OrgAdminSettingsCard` (31), contract tests (31), all `components/org` and `hooks` suites; `tsc`, eslint and prettier clean
- clippy (touched crates, `nonminimal_bool` pre-existing elsewhere allowed) and `cargo deny check advisories` clean

Remaining gaps:

- No browser E2E drives the new Admin field.
- No live GCE boot of a floor claim; the QA check (take the last free slot, see a host appear before the next Start) waits for deployment.
