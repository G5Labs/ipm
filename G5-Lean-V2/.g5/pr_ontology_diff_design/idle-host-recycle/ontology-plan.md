# Managed host packing and demand-driven capacity

Packing, idle removal and demand-driven provisioning are feasible within the
current control plane. Prefer existing occupied hosts, remove an empty host after
over one hour only when another healthy host has free slots, and create default
`c3-standard-4` hosts when valid queued Starts lack capacity. Placement is a small
policy change; automatic capacity management is moderate work because admission,
provisioning, replacement, idle evidence and cloud cleanup must agree.

Scope: implementation of the [human request](../../chat_intent/idle-host-recycle/request.md),
authorized after the assessment. The organization VM quota remains ten. Inspected
baseline: `4eff3a2b`. The initially checked-out, authorized branch is
`idle-host-recycle`. The initial assessment came from source inspection.
Implementation and validation evidence below distinguish local execution from
live cloud qualification.

## Inspected baseline and reusable mechanisms

- [WorkerHostService::place](../../../backend/g5-api-core/src/services/hosts/worker_host_service.rs)
  ordered eligible hosts by occupancy ascending, spreading VMs. Admission
  already filters by organization, region, ready status, published slot sizing,
  available slots and previously refused hosts. Starting/running/stopping VMs occupy
  slots. An organization advisory lock and host row lock protect reservations.
- [Host control](../../../backend/g5-api-core/src/services/hosts/host_control_state.rs)
  persists authenticated live inventory separately from VM reservations. A partial
  heartbeat cannot establish empty inventory; legacy control writes invalidate it.
  [Migrations 047](../../../backend/g5-api/src/db/migrations/047_host_reported_vm_custody.sql)
  and [048](../../../backend/g5-api/src/db/migrations/048_host_custody_legacy_writers.sql)
  deliberately preserve unknown custody.
- [Rotation](../../../backend/g5-api-core/src/cloud/rotation.rs) replaces image-stale
  idle hosts and hosts approaching credential expiry. It preserves host capacity;
  calling its ordinary rotation operation would recreate the host being removed.
  Its five-hour idle predicate reads `last_active_at`, but repository search found
  no current production writer of that field. Its fallback to `created_at` does not
  prove an hour of continuous idleness after recent use.
- [Non-forced removal](../../../backend/g5-api-core/src/services/hosts/worker_host_service.rs)
  locks the host, checks durable reservations and marks it terminated. The
  [DELETE route](../../../backend/g5-api-routes/src/routes/worker_hosts.rs) commits
  before best-effort cloud deletion. [Stale-host cleanup](../../../backend/g5-api-core/src/cloud/stale_host.rs)
  retries orphan cleanup across restarts using existing cloud-instance linkage and
  sightings. These facilities can be reused, but ordinary removal alone checks
  neither reported inventory, the idle duration nor the free-host reserve.
- [Workspace lifecycle](../../../backend/g5-api-core/src/services/workspace/workspace_lifecycle.rs)
  already polls the durable Start queue on a two-second loop. The
  [VM service](../../../backend/g5-api-core/src/services/workspace/workspace_vm_service.rs)
  retries valid owner/generation-bound Starts and expires them after 15 minutes.
  Placement distinguishes an organization VM quota refusal from no eligible host;
  only the latter can be repaired by adding hosts.
  The quota is implemented today: `configs` at organization scope stores
  `max_running_vms`; placement defaults to ten if the setting is absent, counts
  starting/running/stopping workspace VMs across every host and region in the
  organization, and returns `QuotaExceeded` before selecting a host. It records
  `org_vm_quota` and the limit on the queued Start. The existing
  [placement test](../../../backend/g5-api/tests/api_worker_hosts.rs) sets the limit
  to two and checks refusal, and the
  [waiting UI](../../../frontend/src/components/workspace/vmCapacityWait.ts) explains
  the quota condition. This is a G5 VM concurrency limit, separate from host count,
  physical free slots and cloud-provider quotas. No live organization's override
  was queried for this assessment.
- [Cloud bring-up](../../../backend/g5-api-core/src/cloud/bringup.rs) already provisions
  from a persisted host record, keeps a bring-up heartbeat and resolves default
  concurrency and disk size. Add Host resolves the organization's image policy and
  mints a host credential before committing the record. Automatic growth can reuse
  these operations through WorkerHostService rather than duplicating the route.

## Capacity policy

| Term | Meaning |
|---|---|
| Prefer a used host | Among otherwise eligible hosts, prefer those with a running VM; within that group fill the most occupied first. If none has a running VM, prefer occupied hosts before empty ones, counting starting/stopping reservations too. Retain deterministic ties and refusal fallback. |
| Idle | No starting, running or stopping reservations **and** authenticated inventory known empty, continuously for over one hour. Unknown inventory or lost liveness is not empty. |
| Removal reserve, confirmed by the human | At least two healthy hosts with free slots including the candidate permits removing it, leaving one other qualifying host with free slots. Apply within the same organization and region. |
| Remove | Retire an API-managed cloud host and delete its backing instance; retain its database audit record and create no replacement for this action. |
| Grow on demand | For valid queued Starts below the organization VM quota, create enough default small hosts to cover capacity missing after accounting for usable slots and healthy provisioning already underway. |

The human confirmed that the reserve is **one other host with at least one usable
slot**, and accepted that a running VM prevents removal. The implementation uses
organization-and-region pool scope because placement cannot consume free slots
outside that boundary.
Provisioning, draining, unreachable, unknown-inventory and rotation-reserved
capacity should not satisfy the reserve.
External/self-hosted hosts are outside automatic deletion. An operator-added host
is eligible after its initial empty hour under the requested policy; a keep-host
override would be an additional product choice.

For example, with four-slot hosts occupied `[3, 0, 0, 0]`, a new start goes to the
first host. After the other hosts have been empty over an hour, two may be removed
in successive rechecked decisions; `[4, 0]` remains. `[0, 0]` can become `[0]`,
while `[4, 0]` cannot lose its empty host because the full host has no free slots.

## Grow from the existing Start queue

Keep the durable Start queue as the demand authority. The existing maintenance
loop should invoke one WorkerHostService capacity operation after ordinary
placement retries; an immediate wake-up can reduce delay but must not be the only
trigger. No new queue or separately deployed autoscaling service is needed.

1. Recheck current Start consent, owner, lifecycle generation, expiry and organization
   VM quota. Stop, Delete, transfer, expired consent and quota-only waits must not
   cause growth. Allocate available quota headroom to queued Starts in FIFO order
   across regions so the same headroom is not promised to multiple pools.
2. For each requested region, calculate the uncovered eligible demand after usable
   ready slots and healthy, already-provisioning capacity. Include manual additions
   where suitable. Count a rotation pair only once and account for capacity needed
   by the retiring host; its successor is not automatically extra free capacity.
   Published slot counts still require normal final host admission, including disk
   checks and the Start's refused-host exclusions.
3. Under the existing organization admission lock, recheck demand and capacity and
   persist a bounded batch of host records for the deficit. Those records are the
   durable claims other API replicas observe. With the current one-slot default,
   five eligible queued VMs, zero free slots and two healthy one-slot provisions
   need three more hosts. A poll or duplicate Start must not create five more.
4. Commit before cloud calls and launch bring-up with bounded parallelism. Reuse
   shared Add Host preflight, credentials, default sizing, cloud linkage and retry
   operations. Do not make queued users wait through a strictly serial host-boot
   chain: several such boots could consume the 15-minute Start budget.
5. Once hosts publish their measured slots and become ready, the existing queue
   retries place the VMs automatically. Preserve the original Start consent and
   deadline; host creation alone does not authorize a VM start after cancellation
   or expiry. Show "Preparing capacity" while provisioning, and distinguish quota,
   configuration, stockout and timeout outcomes in the existing waiting UI.

**Defaults stay owned by the existing resolvers.** `c3-standard-4` is already the
[Add Host default](../../../backend/g5-contracts/src/types/worker_host.rs).
Leave `concurrent_jobs` and `boot_disk_gb` overrides unset so provisioning and later
replacement follow the normal rules. Today this gives **one VM slot** and a
**200 GB host boot disk** with the built-in scratch default (also with QA's 64 GiB
scratch); environment changes can alter the derived disk size. Use the normal
worker profile, scratch configuration, boot image and organization image pin/track.
An absent or invalid organization image policy is a configuration failure, not
permission to invent a fallback image. Actual VM CPU/RAM remain host-measured.

**Share the Workers page Add Host operation, as requested.** Today
[`AddWorkerHostDialog`](../../../frontend/src/components/org/AddWorkerHostDialog.tsx)
uses [`useProvisionWorkerHost`](../../../frontend/src/hooks/useWorkerHosts.ts) to
POST to `/api/v1/orgs/:orgId/worker-hosts`. Its backend handler previously owned
common input validation, default/image resolution, host-record creation, credential
minting and detached bring-up orchestration; the lower-level cloud bring-up and
sizing functions were already shared. The common provisioning operation now lives
in WorkerHostService, called by the existing POST handler and automatic
capacity maintenance. Automatic input is the requested region and
`instanceType: c3-standard-4`, with concurrency, disk and other overrides omitted.
Preserve Add Host's public response, validation, explicit overrides, audit actor
and existing cloud-feature behavior while sharing the implementation.

Keep HTTP authorization/response handling at the route. Automatic maintenance
adds only demand eligibility, capacity accounting, duplicate prevention and its
bounded retry policy around the same provisioning operation. Compose its durable
host claim with the shared operation in the admission-locked transaction, then
launch cloud work after commit; wrapping the route in an internal HTTP call would
not provide that transaction boundary. Regression coverage must prove both callers
resolve identical defaults and produce the same cloud request for identical inputs,
while the manual path still accepts its supported overrides.

**Authorization and bounds.** Existing authorized Start consent in an enabled
managed-cloud organization triggers a maintenance-owned capacity operation; users
do not need the operator-only `workers:admin` grant to obtain a VM. Manual Add Host
retains that grant. Derive growth from demand capped by the existing VM quota
(default ten), and add a bounded automatic-host policy covering provisioning,
ready, unreachable and deletion-pending cloud resources. Quota alone does not cap
cloud instances during failed deletion or replacement. The initial automatic
capacity ceiling is the VM quota plus one retained spare, with separately
bounded replacement overlap. This is a ceiling, not a provisioning target: the
retained spare is capacity left by the removal rule, never proactively replenished.
Operator-managed capacity also reduces demand. Keep
failed attempts visible and within their retry budget instead of treating every
failure as permission to create a new row. Repeated failures should back off and
surface an actionable wait, not grow indefinitely. These operational bounds leave
the human-selected VM quota unchanged.

**Reuse recovery with explicit automatic-host policy.** Existing host rows and
retry metadata should own provisioning progress. A small automatic-origin marker
is justified to distinguish demand-conditioned retry and accounting from manual
capacity and its replacement; copy the marker on replacement. Do not add a second
durable job system or persist a redundant desired-host count. A row committed just
before an API crash must be discovered by maintenance, and uncertain cloud inserts
must be resolved by deterministic instance identity before launching a substitute.
Retry decisions must recheck live demand and retain bounded exhaustion/backoff
evidence so a poll cannot evade a failed attempt's limits by creating a new row.
Origin alone must not suppress renewal of retained capacity: an active replacement
pair identifies rotation work, whose existing replacement obligation continues
even without queued Starts. Demand-conditioned retry applies to net growth. When
demand disappears before any cloud insert is accepted, retire the unused growth
claim; an uncertain insert needs reconciliation before it can be treated as absent.

Two current recovery behaviors cannot be copied unchanged. Stockout retries can
continue for 30 minutes and later move a host to another region, but Start consent
lasts 15 minutes and placement uses the workspace's requested region. For automatic
growth, reuse same-region zone fallback and backoff, stop launching new attempts
when no eligible demand remains, and keep regional retry fixed. A future explicit
cross-region Start policy would need to move demand and capacity together. An
already accepted cloud insert may still finish after its original demand ends;
it can serve other valid Starts or age through the ordinary idle rule. Never
resurrect expired Start consent. Existing orphan/never-registered recovery is
useful but its minutes-long grace periods do not guarantee this Start succeeds
before its deadline; present timeout truthfully and let the user retry.

## Recommendation and alternatives

Start with the human's demand-driven proposal and normal small-host defaults. It
minimizes the amount added for one more VM and lets one VM finishing make a whole
small host eligible for removal. Packing still improves use of existing larger
hosts. On automatically created one-slot hosts, there is no second slot to pack
another VM into; this is an intentional consequence of keeping current defaults.

During cleanup, prefer retaining the smallest suitable empty host when either
choice preserves the same admission guarantee. Do not create a small replacement
solely to evict the last large empty host in the first version: that adds a separate
capacity-conversion policy and cloud churn beyond the requested idle rule.

**Proactive spare replenishment is excluded by the human.** Automatic growth
requires valid queued VM Starts whose capacity is not covered by usable or
already-provisioning hosts. Consuming the last free slot alone creates no host.
The one-free-host rule constrains idle removal; it does not continuously replenish
free capacity after users consume it. Existing replacement of retained hosts
continues under the rotation policy.

Larger automatic hosts are another future option for sustained demand: for example,
the supported `c3-standard-8` gets two slots from today's default resolver. That
permits packing and sharing a host disk, but scales down in larger increments.
Do not change concurrent-job defaults or claim a cost winner without measuring the
workload and actual cloud costs. No automatic VM migration or shutdown of running
VMs is needed for the proposed first version.

## Minimal sufficient approach and main difficulty

Keep policy in WorkerHostService and use existing API host maintenance, cloud
termination and orphan recovery. A periodic pass can claim one eligible removal
at a time and re-evaluate the reserve and outstanding eligible demand for the next
candidate. Serve queued demand before retiring capacity it could use; otherwise
growth and shrinkage could undo each other's work.

**Track elapsed emptiness durably.** Prefer one nullable host `idle_since` field,
initially unknown. Begin it at the first healthy, known-empty observation; clear it
in the reservation transaction and on nonempty/unknown custody or a liveness gap.
This may conservatively start the hour one maintenance interval late. Use database
time and preserve the timer across API restarts, while restarting the interval when
continuity cannot be established. A sweep alone misses a VM that starts and stops
between passes. Reusing current `last_active_at` as-is fails because it is not
maintained and means last observed activity, not first proven empty time. Deriving
the interval only from current VM rows also fails: release clears `host_id`.
Historical events do not provide a complete history of reservations, unknown
inventory and liveness. These gaps justify the small amount of new durable state.
Enable deletion only after every API writer maintains the timer, or supply database
compatibility guards; a mixed-version deployment must not trust an old timer.

**Make selection and retirement atomic.** Share placement's organization admission
lock, acquire relevant host row locks in a consistent order, then recheck custody,
timer, eligibility and the one remaining host before committing retirement. Reuse
existing termination state and cloud linkage. Do not hold database locks during
cloud calls. Concurrent passes must not all act on the same earlier count; merely
adding `COUNT(*) >= 2` to a polling query is insufficient. The reserve holds at the
automatic retirement decision, not against later VM starts, host failures or explicit
operator deletions. Starting/stopping reservations and host-reported orphan VMs
must prevent removal even when the running-VM count is zero.

**Coordinate with replacement.** Exclude both ends of an active replacement pair
and hosts already owned by a drain. Do not create a rotation-style drain without a
`drain_id`: rotation's recovery sweep interprets that state as work needing a
replacement. Current `provision_replacement` locks the old host but does not reread
its status before creating its successor; candidate exclusion alone therefore
does not fence a previously selected rotation/stale-host operation. Revalidate
eligibility under that lock in the shared replacement path. Reuse the existing
non-forced retirement mechanics with an automatic-idle reason and no VM-loss path.

**Complete cloud cleanup after failure.** Commit retirement, attempt idempotent
cloud termination, and leave failed deletion discoverable through retained linkage
and the existing orphan sweep. Require that recovery to be enabled when enabling
automatic removal; it is independently opt-in today. With its current defaults,
an orphan needs a sustained 15-minute observation window plus polling before reap.
Log logical retirement and cloud deletion success/failure separately: removing a
database row from the active list does not establish that its instance is gone.

## Operational impact

Current Start queues when there is no capacity; the proposal makes that demand
provision hosts without a human adding them. A cold Start still includes host
provisioning and image readiness, with no measured latency guarantee here. One
host with a free slot may mean only one immediately available VM slot, and published
slots do not guarantee host disk admission will succeed.

Packing does not migrate existing VMs. Hosts already spread across the fleet become
empty only as their VMs stop. An unused but still-running VM blocks removal; the
human explicitly accepts this behavior. Existing workspace idle-stop settings stay
unchanged. Packing also concentrates the impact of a host failure. Actual savings
require fleet occupancy and instance-cost measurements.

## Contracts and acceptance evidence for implementation

Implementation updates the placement and bounded-pool contracts in runtime and
WorkerHostService's capacity contracts. Start consent, custody proof, image policy,
cloud linkage and retained-capacity replacement continue to govern.

### Implementation decisions

Health uses the existing 30-second control-heartbeat boundary. Database guards clear
idle evidence on every reservation and legacy liveness write; current control restores
continuity only from the locked previous healthy, empty observation. Complete hello
starts a fresh interval. Database pairing guards also reject successors for idle-retired
hosts and carry automatic origin onto replacements made by older replicas. Retirement
excludes both ends of active replacement pairs,
serves queued demand first, and rechecks the remaining free host under admission and
host locks. Managed cloud operations require cloud features/provider and the enabled
stale-host detector, so orphan cleanup is available (already enabled in QA).

Automatic growth records its origin and original Start ID on the ordinary host row.
At most one growth claim reserves a Start ID. A never-attempted claim retired because
other supply covers demand releases that reservation; attempted claims preserve
their retry history, and a new explicit Start is needed after exhaustion.
Existing provisioning retry count and bring-up lease
bound net growth to three attempts within fifteen minutes, with same-region fallback
and demand revalidation. A per-host database lock fences concurrent launches; recovery
looks up deterministic cloud identity before reissuing an uncertain insert. Accepted
inserts finish normally even if consent later ends; they never authorize a VM Start.
At most four net-growth bring-ups per organization run concurrently. Pending durable
claims count as capacity and are recovered after restart. The automatic resource
ceiling is the organization VM quota plus one, counting unconfirmed deletions and
uncertain inserts; replacement permits one overlapping successor per retained host,
subject to the existing bounded replacement-chain policy. Confirmed cloud absence is
recorded separately from logical termination so successful cleanup releases the bound.
These operational bounds do not change the organization VM quota.

PR #514 review confirmed three recovery gaps in these decisions. A stopped GCE
object still occupies its deterministic name: delete a confirmed-Terminated hit
and require both name lookups to prove absence before the existing demand/retry
checks permit an insert. Stopping and unknown states remain transient. Retiring
unused, never-attempted claims releases their Start reservation as described above.
For retired hosts, persist the last cleanup attempt and select the least recently
attempted batch, so four repeatedly failing deletions cannot starve later resources.
These changes preserve the existing three-attempt budget and four-host batch bound.
The initial Rust CI run also caught missing static audit updates: declare the
capacity module's host writes and VM wait diagnostics under their existing owner,
and point the route authorization proof at shared `prepare_host`. The route's
`workers:admin` authorization still precedes all preparation and credential minting.

Inspection found that existing placement accepts persisted exclusions but refusal
release did not write any. Packing would repeatedly choose the same full-disk host.
Persist a 30-second per-host refusal cooldown in the existing Start intent and use
it in both placement and capacity accounting. This lets other hosts serve the Start
while allowing temporarily busy or repaired hosts to become eligible again.

Extend the existing `api_worker_hosts`, `api_worker_hosts_stale` and
`api_worker_hosts_cloud` coverage with real database/control transactions: used-host
preference and fallback; quota/region/refusal isolation; the one-hour boundary and
timer resets including a short VM between sweeps; one versus two free hosts;
full/heterogeneous/unqualified hosts; unknown or reported-only custody; API restart
and mixed-version rollout; concurrent start/removal and two removal passes;
replacement selection racing retirement; interrupted and failed cloud deletion
with eventual reap and no replacement. Test the additive migration under the real
migration role on fresh and populated databases. A controlled cloud E2E should
prove the last VM exits, the host ages empty, another Start still succeeds, the
selected instance is actually deleted, and no successor is provisioned. Do not
describe mock-cloud tests as proof of real cloud deletion.

For scale-up, cover zero-host bootstrap; one and burst queued Starts; shared quota
headroom across regions; quota-only waits; consuming the last slot without a queued
Start creates no new host; exact default sizing/image policy;
manual/in-flight/rotation capacity accounting; concurrent API replicas; accepted
inserts with delayed persistence; crash after the durable claim before cloud launch;
stockout, terminal configuration failure and retry exhaustion; Stop/Delete/transfer
or expiry while provisioning; no demand after a failed attempt; same-region retries;
resource ceilings including deletion failure; and growth racing shrinkage. A real
cloud E2E should click Start without a free slot, observe automatic host creation,
reach a running VM through the existing queue, then later remove excess empty
capacity while retaining one qualifying host. Test the UI's provisioning, quota,
timeout and failure distinctions. A serialized-boot implementation that causes a
normal quota-sized burst to expire is not sufficient.

## Implementation and verification

The implementation is on the authorized `idle-host-recycle` branch. The organization
VM quota remains ten. `WorkerHostService::prepare_host` owns the former Add Host
validation, image/default resolution, record and credential preparation; both the
HTTP route and capacity maintenance use it, then use shared cloud bring-up after
commit. `host_capacity::maintain` runs after ordinary Start queue reconciliation,
recovers durable claims, and retries logically retired hosts' cloud cleanup without
blocking the queue on cloud operations. The existing stale-host sweep remains a
second cleanup path. Automatic capacity requires cloud features, a configured cloud
provider/signing key, and `G5_STALE_HOST_DETECTOR_ENABLED=true`; QA already supplies
these settings. No proactive spare creation was added.

Migration 082 adds unknown-by-default idle evidence, automatic origin, the unique
original Start claim, and confirmed cloud-deletion evidence. Its triggers protect
reservation/liveness continuity and mixed-version replacement pairing. Net growth
uses the existing attempt counter and a 90-second bring-up-lease delay between at
most three attempts, with a fifteen-minute claim age bound and live demand rechecks.
No extra Start queue or deployment was introduced.

Repeated sweeps omit completed historical host rows unless they hold an exhausted
claim for a currently queued Start. Unconfirmed cloud deletions still count against
the resource bound, and unchanged idle timers are not rewritten on each poll.

Executed locally against PostgreSQL 17: all 51 worker-host API tests and all 57
workspace VM lifecycle tests pass, including existing authorization, manual defaults
and overrides, custody, Stop, transfer and stale-Start regressions. All 14 waiting-message
tests pass. The production migration executable applied all 82 migrations to a fresh
database as NOSUPERUSER/NOBYPASSRLS and replayed them without changes. Migration
version uniqueness, the reviewed checksum catalog and changed-file formatting pass.
All 22 managed-capacity tests pass after the PR recovery fixes, including
the HTTP Start-to-running journey, burst recovery, cancellation, failed deletion,
rotation races, lost linkage and mixed-version pairing guards. All 39 cloud-recovery
tests, 17 stale-host/rotation tests, 11 module-boundary checks and the populated-database
migration 082 upgrade/replay test pass.

For the PR #514 fix batch, the three added recovery regressions pass: a never-used
claim can be replaced after its covering host fails; a confirmed-stopped instance
is deleted in its found zone and is not retried while its name remains visible;
and five failed deletions all receive attempts through a four-host batch. Exhausted
claims still block unbounded retries, and stopping/unknown instances remain untouched.
The eight core-writer checks, thirteen route-authorization checks and eleven module
boundary checks pass. Fresh migration/replay under NOSUPERUSER/NOBYPASSRLS was
repeated successfully after adding the cleanup-attempt timestamp.

The cloud provider double supplies deterministic successful inserts, stockouts,
delays, identity lookups and deletion failures. Database transactions, tenant scope,
HTTP Start/Add Host requests, queue retries and host-control framing are real. Live
GCE boot latency, instance deletion and rollout behavior have not been qualified;
no deployment or live cloud resources were created by this implementation session.

## Hello reconnect follow-up

The human's QA report after PR #514 identifies a continuity bug: migration 048
invalidates inventory on the hello liveness write, then migration 082 clears idle
evidence before hello restores its complete inventory. Hourly LB reconnects repeat
this reset even though the host stays empty. Preserve the prior timer under hello's
existing organization admission lock and host row lock, only for a fresh, already
ready host with unchanged published slot sizing. Restore it with the complete
inventory inside the same transaction, letting the existing guard reject nonempty
or unqualified reports. Keep connection publication through commit and reservation
serialization unchanged. No schema or load-balancer change is needed.

**Freshness decision:** retain the existing 30-second bound. A short tunnel detach
does not change the database heartbeat or inventory; capacity health remains true
within that bound, so an intervening sweep preserves idle evidence. After a gap
over 30 seconds, hello and the sweep must discard it even if the host returns empty
and no reservation is visible. An empty current snapshot does not prove continuous
empty custody throughout a longer outage. This deliberately starts a new full idle interval
after a longer API restart; accepting that conservative delay avoids
silently broadening the existing healthy-continuity policy.

Verify the production host-control and database boundary: first demonstrate the
unchanged empty hello regression on unfixed code; then cover every sizing dimension,
live inventory, legacy/unknown publication, readiness promotion, and a real elapsed
31-second freshness boundary with and without a sweep. Exercise a disconnected
registry tunnel and sweep during a short gap, a delayed old-carrier heartbeat after
new hello, and a real reservation/release while hello waits on admission. The latter
must not resurrect the timer reset by the reservation trigger. Reuse the worker-host
suite's concurrent sizing/publication tests and stale-host/rotation regressions.
The merged organization-settings work (PR #515) makes the removal delay
configurable, with one hour still the default. Its settings and admission-lock
behavior are retained; reconnect preserves evidence independently of that delay.
Verified locally against PostgreSQL 16.15 after integrating main at `a95ae38d`.
Before the implementation change, `empty_reconnect_hello_preserves_continuous_idle_evidence`
failed at its exact timestamp assertion: the second empty hello returned `NULL`
instead of the seeded idle timestamp. With the fix, all 30 capacity tests passed.
After aligning the six new regressions with the repository's harness hang guard,
Nextest passed those six again alongside all 51 worker-host, 39 cloud-recovery and
17 stale-host/rotation tests (113 passed; the other 24 unchanged capacity tests
were already green). The stale-gap case waited through a full 31 seconds of real
database-clock aging, both without and with an intervening sweep.

`cargo clippy --workspace --all-targets -- -D warnings` and
`cargo fmt --all -- --check` passed on the final source. The full workspace check
used real installed libkrun/libkrunfw dependencies. Code inspection and the tests
confirm the existing admission/host lock order, connection publication guard and
reservation trigger remain intact; sweeps cannot see hello's intermediate
inventory invalidation. The host-control backend, PostgreSQL and tunnel-registry
path are real; cloud operations use the existing provider double. Live QA
load-balancer reconnects and deployment of the fix were not exercised locally.
