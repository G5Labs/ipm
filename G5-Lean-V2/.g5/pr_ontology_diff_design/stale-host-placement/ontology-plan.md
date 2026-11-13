# Keep stale-runtime hosts out of placement

Design for the [human request](../../chat_intent/stale-host-placement/request.md):
a Start must never land on a worker host whose runtime the control plane no longer
matches, such hosts must be visible as stale on the Workers page, and an operator
must be able to force removal of an occupied host from the page. Baseline `970b28f6`.

## What happens today

- The host alone knows its runtime. `g5d-host` reads `<runtime_root>/modules/catalog.json`
  and rejects a `vm.start` whose `module_catalog_version`/`module_catalog_hash` or
  `dispatcher_contract_version` differ (`backend/g5d-host/src/workspace_vm.rs::validate_start`).
  The hello (`HostHello`) carries protocol version, capabilities, slots, slot size and
  inventory, nothing about the runtime. `worker_hosts.runtime_identity` (migration 003)
  exists, is in the `WorkerHost` contract and the frontend type, and has no writer.
- Placement (`WorkerHostService::place`) selects any `ready` host with a published
  sizing contract and a free slot. A `SpecRejected { RuntimeMismatch }` reply then runs
  `fail_start`: the VM row becomes `failed`, the Start is not requeued, the host is not
  excluded, and the owner sees the failure. Nothing creates capacity.
- Demand-driven capacity (`host_capacity::supply`) counts the same hosts as healthy
  supply, so queued Starts would be assigned to the stale host rather than grow.
- `DELETE /orgs/:org/worker-hosts/:host` already accepts `?force=true`; the page's
  `useDeallocateWorkerHost` never sends it and surfaces the 409 text as a banner.

## Design

### Publish the runtime identity on hello, decide at placement

Each hello publishes `runtime: { module_catalog_version, module_catalog_hash,
dispatcher_contract_version }`, the same facts `validate_start` checks, read from the
same catalog file. The control plane stores them in `worker_hosts.runtime_identity`
beside the sizing contract in the hello transaction. Its own expectation is the bundled
catalog and `DISPATCHER_CONTRACT_VERSION`, moved from the VM service's private constants
to `g5_internal_api::host_ctl` so the API, the host-side tests and the scripted test host
share one definition.

A host is **runtime-current** when its stored identity matches the control plane's.
Only runtime-current hosts are eligible for placement, count as usable supply for
demand, and qualify as the reserve that permits idle retirement of another host. A host
without a published identity is not current, like a host without a published sizing
contract: both are control-plane-first rollout cases where the host keeps custody,
control and liveness but receives nothing new. The host still performs its own check,
so the final admission stays with the host (`g5.runtime.placement.host-slots`).

Consequences for the user flow: with every host stale, `place` returns `NoCapacity`,
the Start waits with `no_ready_host`, the capacity sweep finds no usable supply and
provisions a host of the organization's machine type exactly as today for a full fleet.
The stale host, now empty, follows the existing idle-retirement and image-rotation rules;
no new removal path is added. A `RuntimeMismatch` reply remains a hard failure because
after this change it indicates a host whose hello and validation disagree, which the
design makes impossible for one catalog file, and a loud failure is better than a silent
retry loop.

Three consequences found in review (round 1, Codex and an independent Claude read)
amend the capacity sweep rather than placement:

- **A spent claim.** A host claimed for a Start can boot after a deploy and come up
  stale; the claim branch used to record `capacity_failed` and wait forever. A claim
  whose host is healthy but not current is spent, and the Start claims again.
- **A lagging image pin.** The deploy pins the nearest published worker image, which can
  lag the control plane's catalog; then every new host is stale and growth would churn
  hosts up to the resource bound. While a healthy host of the image a claim would boot
  has come up stale, growth waits with `capacity_image_stale` and that host is kept as
  the evidence. The image resolution Add Host and claims use is shared with the sweep
  (`host_provisioning::resolve_image_source`), so all three agree on the pin.
- **A stale fleet and the resource bound.** Stale automatic hosts could never retire
  (no current reserve) and held the quota-plus-one bound. An empty, healthy, stale
  managed host of any image but the pin retires at the next sweep without the idle
  delay, reserve or demand conditions (`drain_reason = automatic_stale`); it serves no
  Start, and cloud cleanup then frees the bound for a current host.

Round 2 corrected three of these:

- A spent claim is released for every healthy stale host before demand is read, since
  the quota can hide a Start from one sweep; a host retired without a classified
  bring-up failure is no longer read as a Start's failed claim.
- The pinned-image evidence host is kept past the idle delay; only runtime-current
  hosts retire on idle, stale ones only when off the pin.
- Rotation readiness now requires a runtime-current replacement. The earlier exception
  assumed a stale replacement implies a stale predecessor, which fails when an operator
  added a current host by image override while the organization's track lags: rotation
  would have drained the only placeable host for a stale one. A stale replacement of the
  tracked image waits (the predecessor keeps serving); once the track moves on, the pair
  dissolves so the predecessor is replaced from the new image and the stale replacement
  is left to stale retirement. The credential-deadline forced drain is unchanged.
- The stale-pin guard resolves the image only when a stale host exists and propagates
  resolution errors, so a broken resolver fails the sweep loudly instead of silently
  lifting the guard and retiring the evidence (CodeRabbit).

Round 3 corrected the image itself and two edge cases:

- **The runtime lives in the worker boot image, not the customer image.** The host
  reads its catalog from its own filesystem, which the GCE boot image (`boot_image_id`,
  resolved from the `g5-worker` family) supplies; the organization's tracked customer
  image is the guest OCI image and says nothing about the runtime. The guard, the
  evidence rule and rotation supersession now compare `boot_image_id` with the boot
  image the family resolves to. The capacity sweep resolves it through the cloud
  provider only while a stale host exists and before taking the admission lock (one
  family lookup per sweep in that state); the shared image-source refactor from round 1
  was reverted. Rotation already resolves the boot image once per pass and passes it in.
- **A terminated claim without exhaustion evidence releases its link** (an operator
  removed the stale claim before a sweep saw it), so the successor can take the unique
  Start link instead of failing every sweep on the index.
- **Supersession needs affirmative evidence**: both the replacement's boot image and the
  family's current one known and different. A pinned or untracked organization, whose
  tracked customer image resolves to nothing, no longer dissolves every stale pair.

Round 4 corrected two of round 3's mechanics:

- **The lookup runs before any lock.** Round 3 resolved the boot image after the
  admission lock and the host row locks, so a stalled cloud answer would have held
  placement, hello publication and heartbeats. The sweep now asks, lock-free, whether a
  stale registered host exists, resolves the family if so, and only then locks; a stale
  host that registers between the check and the lock defers the sweep by one tick.
- **Only a fulfilled claim is spent.** Round 3 released the link of any terminated claim
  without a classified error, which would have handed a fresh budget to a Start whose
  claim had exhausted its attempts on unclassified failures. A terminated claim host
  releases its link only if it had registered; one that never registered keeps its
  attempts as exhaustion evidence, as the claim driver intends.

Round 5 corrected the lookup's envelope:

- **Bounded, and outside the decision transaction.** The lookup had no timeout, and
  lifecycle maintenance awaits each organization in turn, so one hanging request could
  stall every organization's Start retries and reconciliation; and it ran inside the
  decision transaction, whose Postgres `now()` is fixed at transaction start, so a
  reserve whose heartbeat aged past the 30-second window during the lookup still read as
  healthy. `sweep_boot_image` now probes for a stale registered host in its own short
  transaction, resolves the family after it closed with a five-second bound, and the
  decision transaction opens afterwards; `reconcile_capacity` takes the result.
- The new rotation test runs serially like the other rotation tests, since rotation is
  enabled through a process-global environment variable.

Rejected alternatives:

- Marking a host stale only after it rejects a spec (lazy) keeps a protocol unchanged
  but costs every organization one failed or requeued Start per stale host per deploy,
  and needs a mark that hello must clear. The hello field is the smaller mechanism.
- Encoding the identity as capability strings avoids one `deny_unknown_fields` decode
  during a rolling control-plane deploy, at the cost of parsing attributes out of a list
  in SQL. A host whose hello an old replica cannot decode reconnects with its existing
  backoff, so the typed field is preferred.
- Treating an unknown identity as current would keep legacy hosts eligible through the
  first rollout but would also keep stale legacy hosts eligible, which is the failure the
  request removes.

### Workers page: a stale label

The host list derives `runtimeState: "current" | "stale" | "unknown"` from the stored
identity against the control plane's, like `heartbeatState`. The page shows a plain
`stale` chip under the status chip and nothing more: the owner asked (2026-10-15, after
seeing a tooltip and an `unknown` chip) that the page expose no further detail. An
unpublished identity is therefore not labelled.

### Force removal from the page

The page keeps the backend as the authority on occupancy. If the row shows occupancy,
the confirmation says the host runs N VM(s), that removal stops them, and sends
`force=true`. If the plain removal is refused with 409 (a VM arrived after the list was
read), the banner shows the backend's reason and a `Remove anyway` button that retries
with `force=true`. No drain UI is added; the drain route exists for operators who want
the gentler path and the request did not ask for it.

## Ontology changes

- `g5.runtime.placement.runtime-identity` (new, `software_architecture/runtime.g5.md`):
  the identity publication, eligibility rule and reason.
- `g5.runtime.placement`: references the new block for eligibility.
- `g5.architecture.host_demand_capacity` and `g5.architecture.host_idle_retirement`:
  usable supply and the retirement reserve count only runtime-current hosts.
- `g5.architecture.host_forced_removal` (new, `software_architecture/worker_host.g5.md`):
  refusal of an occupied host without force, what force does, and the page's two-step
  consent. Forced removal was previously only metadata under `g5.architecture.placement`.

## Implementation and verification

- Protocol: `HostHello.runtime: Option<HostRuntimeIdentity>` (`#[serde(default)]`),
  `HostRuntimeIdentity::bundled()` for the control plane's expectation,
  `HostRuntimeIdentity::read(runtime_root)` for the host.
- Host: `WorkspaceHostState::hello` publishes `HostRuntimeIdentity::read(runtime_root)`;
  a read failure publishes none (the host would reject every spec anyway).
- API: the hello transaction stores the identity; `place`, `supply` and the retirement
  reserve filter on it; `list_hosts_filtered` and the runtime-label refresh annotate
  `runtime_state`.
- Frontend: `WorkerHost.runtimeState`, the chips, the occupancy-aware confirmation and the
  `Remove anyway` retry; `useDeallocateWorkerHost` takes `force`.
- Tests (see the metadata of the blocks above once run): API placement skips a stale and
  an unpublished host and reserves the current one; capacity grows when the only hosts
  are stale and does not retire the last current host behind a stale reserve; the list
  reports `runtime_state`; the scripted host publishes the bundled identity by default;
  the page shows the chip, confirms with force for an occupied row and offers
  `Remove anyway` on 409.
