# Host Control Plane

<a id="g5.hcp.ownership"></a>
<a id="g5.hcp"></a>
[I · g5.hcp]

The Host Control Plane (HCP) provisions and manages an organization's hosts.

<a id="g5.hcp.api"></a>
[I · g5.hcp.api]

HCP exposes an API to list, provision, drain and de-provision hosts, to allocate and report workspace VMs, to reclaim idle VM slots, and to manage host images.

<a id="g5.hcp.observability"></a>
[I · g5.hcp.observability]

HCP is the central place to query workspace VM state, so users and administrators can see the state of the system.

<a id="g5.hcp.administrator"></a>
[I · g5.hcp.administrator]

Organization administrators can provision new hosts.

<a id="g5.hcp.provisioning"></a>
[I · g5.hcp.provisioning]

Provisioning a Cloud Host requests cloud resources. A failure keeps enough error detail to tell the administrator what went wrong, and transient failures are retried.

<a id="g5.hcp.autoscale"></a>
[I · g5.hcp.autoscale]

With auto-scaling limits (min, max) configured, a workspace VM Start that existing resources cannot satisfy can trigger host provisioning.

<a id="g5.hcp.scheduling"></a>
[I · g5.hcp.scheduling]

Any schedulable workspace VM request should be scheduled on an appropriate host.

## Provisioning and retirement

<a id="g5.runtime.host"></a>
[I · g5.runtime.host]

A host record carries a cloud instance through provisioning, recovery and retirement, independently of the VMs it holds.

<a id="g5.runtime.host.rotation"></a>
[D · g5.runtime.host.rotation]

Replace a host by pairing a new one while the old keeps serving, and retire the old only after the replacement is ready with reported slots, under a fresh idle-custody check serialized with placement.

- A credential deadline may instead force a drain, releasing all VM custody before teardown.
- A failed replacement never silently retires a healthy old host; retiring a failed replacement re-verifies its failure under the retirement locks.
- A replacement that fails for good dissolves the pairing, and the old host keeps serving until a later pass pairs it again. It fails for good when it is terminated or removed, its bring-up has no retry left, or it is ready only on a stale runtime a newer boot image supersedes.
- Pairing reports an explicit outcome: provisioned, or deferred with a reason. A deferral is a visible, durable wait that never counts as replacement progress and never authorizes retiring the old host.
- Only automatic cloud-cleanup debris beyond the [resource bound](#g5.architecture.host_demand_capacity) defers pairing; renewal overlap, retained live hosts and a lowered VM quota do not.
- Unreachable hosts stay owned in the rotation and reaping lanes, so an unreachable predecessor cannot freeze its pair or block the successor's renewal.
- [Empty-host retirement](#g5.architecture.host_idle_retirement) needs no replacement; each path rechecks whether the other already retired the host.
- A chat turn a drain interrupts continues under [automatic continuation](workspace_vm.g5.md#g5.lifecycle.start.automatic-continuation) once the workspace is placed again.

<a id="g5.runtime.host.cloud-linkage"></a>
[D · g5.runtime.host.cloud-linkage]

Create a durable host record before cloud provisioning, and keep classified failures for retry or diagnosis.

- Every instance insert carries a `requestId` its resends reuse, so the provider recognizes a resend whose first reply was lost instead of refusing it as already existing; each new attempt, such as a later zone or stockout retry, is a new insert with its own `requestId`.
- If an accepted insert times out, keep its deterministic reference and boot image for reconciliation, and never insert again in another zone.
- Each retry persists a coherent row, consuming the prior classification and schedule together, so a later unclassified failure stays in ordinary reconciliation.
- Candidate zones come from the provider's available zones; an invalid configured preference degrades to real zones, never to a closed failure. Cross-region stockout fallback targets only regions the organization's [placement](#g5.runtime.placement.capacity) will use.
- An ambiguous 403 on an initial insert permits zone fallback only when a successful regional lookup excludes the configured preference and offers other UP zones; otherwise keep the original authorization failure. Classify provider reason codes too: a rate-limited 403 is retryable.
- An asynchronous delete that times out is pending, not failed; declare deletion complete only on confirmed absence, and keep unknown provider state uncertain.

<a id="g5.runtime.host.cloud-recovery"></a>
[D · g5.runtime.host.cloud-recovery]

Repair the cloud link of a live, unlinked host only when its bring-up lease is absent or stale, preserving its lifecycle state.

- Abandon an unlinked provisioning host only after repeated confirmed absence and with no live bring-up lease. A transient observation resets the missing streak; a lookup error does not advance it.
- Reconciliation attaches a found instance to a live unlinked row that carries a stale retry classification and clears that classification; it never abandons a row that keeps a classified terminal badge.
- The one exception is a manually added host still stocked out when its retry budget lapses (30 minutes from creation by default): it becomes a failed bring-up that keeps its stockout cause, and after a further [cloud-orphan grace](#g5.runtime.host.orphan-custody) it is terminated, so a host that can never come up neither shows Provisioning forever nor waits for an administrator.
- Only the [capacity owner](#g5.architecture.host_demand_capacity) reconciles or abandons unlaunched managed-capacity claims; launch-queue delay is not abandonment. A claim host that registered while unlinked re-enters ordinary linkage recovery.

<a id="g5.runtime.host.orphan-custody"></a>
[D · g5.runtime.host.orphan-custody]

Reverse cloud-orphan sweeping deletes only an instance whose host is terminated or absent through the configured sighting grace and that holds no unreleased VM custody.

- Check both conditions at candidate selection and at final deletion admission; a failed lookup authorizes no deletion.
- Unreachable hosts remain owned and use [custody-aware retirement](#g5.runtime.host.rotation). The VM lifecycle owner reconciles lease expiry before orphan cleanup proceeds.

<a id="g5.architecture.host_forced_removal"></a>
[D · g5.architecture.host_forced_removal]

Removing a host that still holds starting, running or stopping workspace VMs is refused unless the operator explicitly forces it; the Workers page asks for that consent with the VM count, or after a refused plain removal.

- A forced removal records each of those VMs as lost (`host_force_terminated`, with the actor and incarnation), releases their custody without exit proof, and terminates the host in one transaction, then best-effort asks a reachable host to drain and deletes the cloud instance.
- Like lease expiry, it is a [logical release](workspace_vm.g5.md#g5.vm.physical-uniqueness-question) and never proves the VMs stopped.

<a id="g5.runtime.host.compensation"></a>
[D · g5.runtime.host.compensation]

Late cloud results cannot relink a retired host. Keep compensation exclusive with [recovery attachment](#g5.runtime.host.cloud-recovery) through deletion, preserve instances already owned, and leave uncertain ownership — including live, unlinked hosts — for reconciliation.

<a id="g5.runtime.host.image-policy"></a>
[D · g5.runtime.host.image-policy]

An organization administrator sets the worker-image policy that new and replacement hosts run ([the host's versioned image](hosts.g5.md#g5.host.image)): exactly one of a promoted image track or a digest-pinned image reference.

- Provisioning is refused when no track or pin is given and the organization has no policy.
- An administrator can refresh a host's runtime labels on demand, independent of provisioning; a base image declaring a guest glibc below the supported floor is refused.

## Placement capacity and disk provisioning

<a id="g5.runtime.placement.capacity"></a>
[D · g5.runtime.placement.capacity]

Placement requires both quota in the organization and a free slot on a ready, eligible host. The VM quota defaults to 10; administrators may set 0–100. Hold the organization admission lock across quota reads, host eligibility and reservation so competing Starts cannot overbook.

- Count starting, running and stopping custody until release. Lowering the quota does not stop existing VMs, and this quota sets no host-count autoscaling limit.
- Insufficient quota or capacity preserves eligible Start consent for retry.
- Judge host-refusal expiry on the database clock that recorded it, and let a later wait reason keep the host's actionable refusal detail rather than overwrite it.
- Finalizing a host to terminated clears its reported live-VM inventory, so phantom inventory from a dead host never counts.

<a id="g5.runtime.placement.reachability"></a>
[D · g5.runtime.placement.reachability]

Expands [placement capacity](#g5.runtime.placement.capacity): an API may reserve only hosts reachable through its live control connections. The tunnel registry supplies a fresh eligibility snapshot; placement still owns quota, slot and incarnation admission, and command delivery rechecks the connection.

- A disconnected API preserves queued Start consent without allocating an incarnation or recording a host refusal. Losing the connection before Start is sent returns the reservation to that consent without adding or renewing a shared host exclusion.
- An actual host refusal keeps its cooldown; an ambiguous sent command retains its incarnation for recovery.
- Starting-VM status reconciliation requires the same local control eligibility before advancing its shared retry deadline. Lease expiry and database-only loss settlement stay independent of connectivity.

<a id="g5.runtime.placement.owner-limit"></a>
[D · g5.runtime.placement.owner-limit]

Placement also enforces the [owner limit](../features/organizations.g5.md#g5.organization.owner-limits) on running VMs: by default an owner may hold 5 in the organization, and administrators may set 0–100.

- Under the same admission lock and before the [organization quota](#g5.runtime.placement.capacity), count the owner's other starting, running and stopping VMs plus their valid Starts that have waited since before this one.
- A Start at the limit is refused and its consent ends with the reason, rather than waiting, so a runaway owner's agents get a definite answer and no owner's queued demand exceeds their limit.
- Lowering the limit stops no VMs. Starts already waiting beyond it are refused when retried, and [managed growth](#g5.architecture.host_demand_capacity) may already have claimed a host that [idle retirement](#g5.architecture.host_idle_retirement) later removes.

<a id="g5.runtime.placement.host-disk"></a>
[D · g5.runtime.placement.host-disk]

Administrators configure a host disk allowance H (default 120 GiB) and a VM disk allowance V (default 80 GiB); a new managed host with S slots requires H + V × S GiB. This is both the default and the minimum: reject an unsupported calculated minimum or a smaller override rather than clamp, while allowing larger explicit disks within supported limits.

- Manual creation, automatic growth and replacement use the same policy.
- Persist H, V, S and the resolved disk size when creating the host, and return that allocation to launchers; provisioning retries and launchers reuse that snapshot.
- A replacement uses current organization settings and keeps a compatible explicit override; report an incompatible override before disturbing existing VMs.
- Settings edits neither resize existing hosts nor change their advertised capacity. Publish current organization sizing to the administrator UI in binary units.

<a id="g5.runtime.placement.vm-disk"></a>
[D · g5.runtime.placement.vm-disk]

Each managed VM's virtual disk capacity is its host's recorded V, and registration, boot specifications, helper arguments and prepared-image identity all agree on that value; deployment-global disk settings cannot override it.

- Start admission requires available host space of at least V, alongside independent slot, CPU, memory and authorization checks. Cold or changed images, seed misses and bootstrap add no multiple of V or fixed margin.
- The disk is a sparse clone whose physical allocation grows with the guest's writes: nothing unshares or preallocates it, and admission reserves no storage for later writes.

## Managed host capacity

<a id="g5.architecture.host_demand_capacity"></a>
[D · g5.architecture.host_demand_capacity]

Managed growth is enabled by default: add hosts for queued Starts whose owner- and generation-bound consent is valid and unexpired and whose demand is below the [VM quota](#g5.runtime.placement.capacity), allocating headroom FIFO across requested regions. Subtract usable ready supply and healthy in-flight supply while respecting refusals and replacement obligations.

- Also keep the organization's free-slot floor (default one slot; administrators may set 0–100, where 0 disables it) in its demand region: after queued Starts are served, when current healthy and in-flight supply there has fewer free slots than the floor, claim one host per sweep. Free slots on a stale-runtime host do not count.
- The floor tops up a region that already has a host and never creates a region's first host, so a dormant organization pays for none and removing every host ends it there.
- A region a floor host left keeps its presence for an hour after cloud cleanup, so retiring the last empty stale host while the resource bound holds the floor's claim back does not end the floor; an operator's later removal that terminates a live host in that region does.
- A floor claim obeys the same image, growth, resource-bound and batch gates as a Start's claim and carries no Start; a young claim without a cloud instance covers the floor until it registers, is retried, or is abandoned after the claim window.
- While the organization is at its [VM quota](#g5.runtime.placement.capacity), the floor makes and keeps no claim, so lowering the quota to cut spending never yields a host placement refuses.
- Use the organization's allowlisted machine type (default `c3-standard-4`) through the existing Add Host validation, image, credential and sizing operation. Automatic maintenance leaves manual sizing overrides unset and cannot create Start consent.
- Disabling growth blocks new claims but lets existing claims continue. Capacity maintenance owns host supply and wait diagnostics; workspace lifecycle owns Start consent and VM transitions.
- Persist claims before cloud work, and apply [cloud reconciliation](#g5.runtime.host.cloud-linkage) with deterministic identity and bounded retries on the same row. Net growth requires continuing demand or a short floor in the region; a claim neither needs is abandoned, and cancellation or expiry cannot create Start authority.
- Delete confirmed-stopped cloud objects and confirm their names absent before retrying, and keep observing unknown states. Count undeleted uncertain or failed automatic resources against `VM quota + 1`, allowing at most one replacement overlap per retained host.
- Visible waits distinguish preparation, quota, disabled growth, configuration, stockout and exhaustion.

<a id="g5.architecture.host_idle_retirement"></a>
[D · g5.architecture.host_idle_retirement]

Automatic removal is enabled by default: retire an API-managed cloud host only while it stays healthy and continuously empty longer than the organization's delay (default one hour; range 15 minutes–24 hours), and only while the other healthy current hosts in the same organization and region together keep the [free-slot floor](#g5.architecture.host_demand_capacity). Active reservations, reported live inventory and starting/running/stopping VMs block retirement, and valid queued demand is served first.

- Unknown custody or a heartbeat gap over 30 seconds resets idle continuity. A fresh, empty Hello preserves it only for an already-ready host with unchanged published sizing.
- Serialize retirement with placement, and exclude drains and both ends of replacement pairs.
- Retire without replacement and keep the audit record. Retry failed cloud cleanup fairly so one failure cannot starve later hosts.
- Disabling removal skips retirement without discarding idle evidence. Never automatically delete self-hosted hosts.

## Metadata

### g5.hcp

- human-provided
- Source: [Ben, line 152](../../../docs/ben_new_ontology.md#L152).
- Former ID: `g5.hcp.ownership`, restated as the concept that defines the `g5.hcp` level ([writing principles](https://github.com/G5Labs/G5-Lean/issues/1119)).
- Children: [API](#g5.hcp.api), [observability](#g5.hcp.observability), [administrator](#g5.hcp.administrator), [provisioning](#g5.hcp.provisioning), [autoscale](#g5.hcp.autoscale), [scheduling](#g5.hcp.scheduling).

### g5.hcp.api

- human-provided
- Expands: [HCP](#g5.hcp).
- Source: [Ben, line 153](../../../docs/ben_new_ontology.md#L153).

### g5.hcp.observability

- human-provided
- Expands: [HCP](#g5.hcp).
- Source: [Ben, line 154](../../../docs/ben_new_ontology.md#L154).

### g5.hcp.administrator

- human-provided
- Expands: [HCP](#g5.hcp).
- Source: [Ben, line 155](../../../docs/ben_new_ontology.md#L155).

### g5.hcp.provisioning

- human-provided
- Expands: [HCP](#g5.hcp).
- Source: [Ben, line 156](../../../docs/ben_new_ontology.md#L156).

### g5.hcp.autoscale

- human-provided
- Expands: [HCP](#g5.hcp).
- Source: [Ben, line 157](../../../docs/ben_new_ontology.md#L157).
- [RUNTIME-005](../../../docs/code_gaps.md) now concerns configurable min/max host counts: [demand-driven growth](#g5.architecture.host_demand_capacity) and empty-host retirement exist, but VM quota and automatic-resource bounds do not provide those settings. The [earlier audit](../../uplift/ben-ontology/investigations/runtime.md) at `20e393c0` predates that implementation.

### g5.hcp.scheduling

- human-provided
- Expands: [HCP](#g5.hcp).
- Source: [Ben, line 158](../../../docs/ben_new_ontology.md#L158).

### g5.runtime.host

- agent
- Concept added for the [writing principles](https://github.com/G5Labs/G5-Lean/issues/1119) ID-level rule; it groups the host-record lifecycle blocks below.
- Children: [rotation](#g5.runtime.host.rotation), [cloud linkage](#g5.runtime.host.cloud-linkage), [cloud recovery](#g5.runtime.host.cloud-recovery), [orphan custody](#g5.runtime.host.orphan-custody), [compensation](#g5.runtime.host.compensation), [image policy](#g5.runtime.host.image-policy), [inventory](hosts.g5.md#g5.runtime.host.inventory).

### g5.runtime.host.rotation

- agent
- Expands: [host record](#g5.runtime.host).
- Source: [runtime reconstruction](../../uplift/ben-ontology/investigations/runtime.md) and [rotation/retirement decision](https://github.com/G5Labs/G5-Lean/blob/3d17915df39bcf4cbe0cabc08500de63ff46fbb2/.g5/chat_intent/idle-host-recycle/request.md).
- HostRotation exposed RUNTIME-008 under its Read Committed snapshot premise; a hypothetical fresh, serialized custody check passed. The database schedule was not reproduced, and cloud deletion does not prove physical VM death.
- The explicit pairing outcome, renewal-is-not-growth bound, unreachable-host lanes and pre-retirement re-verification are agent design from [issue #627](https://github.com/G5Labs/G5-Lean/issues/627) (findings S1/S2/S8/S31; [design](../../pr_ontology_diff_design/issue-627/ontology-plan.md)); the prior code silently refused pairing under a lowered quota and froze pairs whose predecessor went unreachable.
- Implemented ([G5-Lean#626](https://github.com/G5Labs/G5-Lean/issues/626) S22): the below-cliff quiesce (`rotation.rs::below_cliff_quiesce`, which absorbed `idle_quiesce` on the #627 branch) locks the host row, re-reads occupancy in a fresh statement and only then marks the host draining; the earlier single `UPDATE … NOT EXISTS` read its snapshot before the lock. Covered by a real-Postgres test that commits a placement while the quiesce waits.
- Amendment: [supplied proposal](../../pr_ontology_diff_design/ontology-gap-fill/ontology-plan.md#source-and-scope), item 45 (Q7).
- Amendment ([G5-Lean#984](https://github.com/G5Labs/G5-Lean/issues/984)): interrupted chat turns follow [automatic continuation](workspace_vm.g5.md#g5.lifecycle.start.automatic-continuation); rotation and drain mechanics are unchanged.

### g5.runtime.host.cloud-linkage

- agent
- Expands: [host record](#g5.runtime.host).
- Source: [runtime reconstruction](../../uplift/ben-ontology/investigations/runtime.md) and [full review](../../uplift/ben-ontology/reviews/runtime-full.md).
- CloudRecovery checked retention of an accepted result's reference/image and refusal of a second accepted insert, with classified zone fallback. Provider-response parsing and arbitrary repeated inserts remain outside the check.
- Retry-state coherence, placement-respecting fallback, real-zone candidacy, reason-code classification and delete-outcome confirmation are agent design from [issue #627](https://github.com/G5Labs/G5-Lean/issues/627) (S18/S20/S30/S33/S34/S35; [design](../../pr_ontology_diff_design/issue-627/ontology-plan.md)).
- The guarded initial-403 fallback addresses GCE's ambiguous unknown-zone denial recorded in [issue #1000](https://github.com/G5Labs/G5-Lean/issues/1000), while preserving authorization failures when zone discovery cannot justify fallback.
- Amendment: [supplied proposal](../../pr_ontology_diff_design/ontology-gap-fill/ontology-plan.md#source-and-scope), item 46 (S21).

### g5.runtime.host.cloud-recovery

- agent
- Expands: [host record](#g5.runtime.host).
- Source: [full runtime review](../../uplift/ben-ontology/reviews/runtime-full.md), migration 073; split from `g5.runtime.host.cloud-linkage` to own recovery.
- CloudRecovery checked lifecycle preservation, missing thresholds, transient/error distinctions and bring-up lease guards. Cloud discovery completeness and real clock behavior remain unproved.
- Stale-classification inclusion and capacity-claim ownership are agent design from [issue #627](https://github.com/G5Labs/G5-Lean/issues/627) (S18/S3; [design](../../pr_ontology_diff_design/issue-627/ontology-plan.md)); the prior orphan pass reaped queued claims their capacity owner was still honoring.
- Amendment: [supplied proposal](../../pr_ontology_diff_design/ontology-gap-fill/ontology-plan.md#source-and-scope), item 47 (S30, PL-cell-09).

### g5.runtime.host.orphan-custody

- agent
- Expands: [host record](#g5.runtime.host).
- Source: [issue #626](https://github.com/G5Labs/G5-Lean/issues/626) and [continued design](../../pr_ontology_diff_design/issue-626/ontology-plan.md#host-progress-follow-up-issue-626-october-22).
- The stronger unreleased-custody guard reuses lifecycle release instead of duplicating lease policy.
- Implemented by the shared selection/final-admission predicate in `stale_host.rs`; real-Postgres regressions cover short-grace partitioning, changed custody before deletion, expiry/release and provider-delete retry.

### g5.architecture.host_forced_removal

- human-provided
- Reason: operators need a way out for a dead host, and the loss is recorded rather than hidden.
- Amendment: [supplied proposal](../../pr_ontology_diff_design/ontology-gap-fill/ontology-plan.md#source-and-scope), item 48 (Q5, L1B-Q1).

### g5.runtime.host.compensation

- agent
- Expands: [host record](#g5.runtime.host).
- Source: [full runtime review](../../uplift/ben-ontology/reviews/runtime-full.md); split from `g5.runtime.host.cloud-linkage`.
- CloudLinkage exposed [RUNTIME-007](../../../docs/code_gaps.md); reusing a terminal host row preserved live ownership in the bounded repair comparison, while an independent claim alone did not. The repair is unimplemented, and multiple results or failed cloud deletion remain outside the check.

### g5.runtime.host.image-policy

- agent
- Expands: [host record](#g5.runtime.host).
- Inferred from code for the writing-rules [coverage review](https://github.com/G5Labs/G5-Lean/issues/1119); the owner should confirm the intent and whether it belongs here or with deployment. Refines [the host image](hosts.g5.md#g5.host.image).
- Implementation: `backend/g5-api-routes/src/routes/worker_hosts.rs` (image-policy PUT, on-demand runtime-label refresh), `backend/g5-api-core/src/services/hosts/worker_host_service.rs` (`set_org_image_policy`), `backend/g5-api-core/src/services/hosts/host_provisioning.rs` (image-source-required refusal), `backend/g5-contracts/src/types/worker_host.rs` (glibc floor).

### g5.runtime.placement.capacity

- agent
- Source: [worker-host API](../../../docs/worker-host-api-v9.md) and [quota configuration decision](https://github.com/G5Labs/G5-Lean/blob/3d17915df39bcf4cbe0cabc08500de63ff46fbb2/.g5/chat_intent/admin-page/request.md); migration 083 preserves existing values.
- [Placement](https://github.com/G5Labs/G5-Lean/blob/072a12d3e99032ee3a7356837542c5edeb825984/.g5/TLA_verification/ben-coverage/runtime/README.md) checked serialized quota/slot reservation and custody counting. It did not establish the later numerical settings, packing policy or managed growth.
- The single refusal-expiry clock, refusal-detail retention and terminal inventory clearing are agent design from [issue #627](https://github.com/G5Labs/G5-Lean/issues/627) (S5/S7/S17; [design](../../pr_ontology_diff_design/issue-627/ontology-plan.md)).

### g5.runtime.placement.reachability

- agent
- Expands: [placement capacity](#g5.runtime.placement.capacity); reuses the registry also governing [task dispatch](../features/chat.g5.md#g5.chat.dispatch-reachability).
- Source: [issue #758](https://github.com/G5Labs/G5-Lean/issues/758), required qualification for [issue #844](https://github.com/G5Labs/G5-Lean/issues/844), and the [continued design](../../pr_ontology_diff_design/issue-844/ontology-plan.md). CI diagnostics at `a326d3205806fb159d244fb5d0499468cccf85b4` show repeated placement exclusions before a formal-review task exists; these observations do not attribute every historical timeout.

### g5.runtime.placement.owner-limit

- human-verified
- Source: [owner request and decisions](../../chat_intent/owner-resource-limits/request.md) (defaults, refusal instead of queueing, waiting Starts count); [design](../../pr_ontology_diff_design/owner-resource-limits/ontology-plan.md#running-vms-per-owner).
- `WorkerHostService::place` counts and refuses with `owner_vm_limit`; the setting is `max_running_vms_per_owner` (migration 118).

### g5.runtime.placement.host-disk

- human-provided
- H/V policy, replacing the earlier fixed formula and permissive overrides.
- Source: [issue #1049](https://github.com/G5Labs/G5-Lean/issues/1049); [implementation design and qualification](../../pr_ontology_diff_design/issue-1049/ontology-plan.md).

### g5.runtime.placement.vm-disk

- human-provided
- Host-recorded V, admission at V and sparse disks. The [owner's 2026-11-04 correction](../../chat_intent/issue-1049/sparse-disk-correction.md) replaced the earlier physical-reservation and shared-H quota requirements.
- Source: [issue #1049](https://github.com/G5Labs/G5-Lean/issues/1049); [implementation design and evidence](../../pr_ontology_diff_design/issue-1049/ontology-plan.md).

### g5.architecture.host_demand_capacity

- human-verified
- Source: [human request and decisions](https://github.com/G5Labs/G5-Lean/blob/3d17915df39bcf4cbe0cabc08500de63ff46fbb2/.g5/chat_intent/idle-host-recycle/request.md); [human request and decisions](https://github.com/G5Labs/G5-Lean/blob/3d17915df39bcf4cbe0cabc08500de63ff46fbb2/.g5/chat_intent/admin-page/request.md); [free-slot floor request](../../chat_intent/host-min-free-slot/request.md) and its [design](../../pr_ontology_diff_design/host-min-free-slot/ontology-plan.md).
- Operational bounds and recovery are retained agent design. The policies do not satisfy the separately retained [min/max host-limit intent](#g5.hcp.autoscale). Upstream real-DB tests use a cloud double; live GCE qualification is separate.
- The floor's "top up, never create" scope, its default of one and its 15-minute claim window are agent design under the owner's "always a free slot". `host_capacity.rs` decides it after demand in the same sweep; a floor claim is `worker_hosts.capacity_floor_claim` and shares the Start claim's launch, retry and abandonment (`drive_claim`); presence (an established host, or the `capacity_floor_presence` marker automatic retirement leaves) gates both creation and recovery, and an operator's removal clears the marker for the region; the 082 capacity guard (replaced in 086) and `stale_host.rs` keep the manual cross-region stockout retry off floor claims. `api_host_capacity.rs` covers the floor with the cloud double.
- Amendment: [supplied proposal](../../pr_ontology_diff_design/ontology-gap-fill/ontology-plan.md#source-and-scope), item 49 (L1B-Q4, L1B-Q3).

### g5.architecture.host_idle_retirement

- human-verified
- Source: [human request and decisions](https://github.com/G5Labs/G5-Lean/blob/3d17915df39bcf4cbe0cabc08500de63ff46fbb2/.g5/chat_intent/idle-host-recycle/request.md); [human request and decisions](https://github.com/G5Labs/G5-Lean/blob/3d17915df39bcf4cbe0cabc08500de63ff46fbb2/.g5/chat_intent/admin-page/request.md).
- The 15-minute floor and 30-second freshness bound are agent design recorded upstream. A currently empty host after a long outage does not prove continuous emptiness. This is separate from [rotation](#g5.runtime.host.rotation).
- The reserve is the summed free slots of the other current hosts against `max(1, min_free_slots)` ([design](../../pr_ontology_diff_design/host-min-free-slot/ontology-plan.md)); a sweep therefore never retires a current host and claims a floor host in the same region; an empty stale host retires regardless and may be replaced in the same sweep.
