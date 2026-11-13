# Stale-host placement implementation review

Self-review of the `kaihui/stale-host-placement` change against the saved
[design](pr_ontology_diff_design/stale-host-placement/ontology-plan.md) and
[chat intent](chat_intent/stale-host-placement/request.md). The same agent implemented
the change, so this section is not an independent review; independent rounds follow.

## Initial findings and evidence

| Commitment | Actual path → counterexample/check | Result |
|---|---|---|
| `g5.runtime.placement.runtime-identity` | `g5d-host` reads `<runtime_root>/modules/catalog.json` in `WorkspaceVmRuntime::runtime_identity` for every hello and every `validate_start`, so the published identity and the spec check cannot disagree; `HostHello.runtime` is `#[serde(default)]` so a hello from an older host decodes with none. The hello transaction stores it (`'{}'` when absent) beside the sizing contract. `place` adds `runtime_identity @> <bundled>`; `list_hosts_filtered` and the label refresh derive `runtime_state`. Counterexample: three ready hosts, the two older ones stale/unpublished, must lose to the youngest current one; then no free current slot must wait with `no_ready_host`, not fail; a current hello must make the host eligible again. | `api_worker_hosts.rs::placement_skips_stale_and_unpublished_runtime_hosts_and_the_list_says_why`; `g5-internal-api` unit tests `runtime_identity_read_from_the_shipped_catalog_is_current`, `hello_runtime_identity_is_optional_on_the_wire`, `module_catalog_hash_matches_shipped_catalog`. Passing. |
| `g5.architecture.host_demand_capacity` | `supply` adds `runtime_current`; `available()` is 0 for a healthy stale host, so queued demand finds no candidate and the sweep claims a new host. Counterexample: one stale 4-slot host and one queued Start must produce one claim with `preparing_capacity`, and placement must answer `NoCapacity`. | `api_host_capacity.rs::a_stale_runtime_host_covers_no_demand_so_the_fleet_grows`. Passing. |
| `g5.architecture.host_idle_retirement` | The retirement reserve requires `other.runtime_current`; candidates still only need `healthy`, so a stale host keeps its idle timer and retires behind a current host. Counterexample: one current and one stale idle host must retire exactly the stale one and then nothing. | `api_host_capacity.rs::a_stale_runtime_host_is_no_reserve_and_retires_behind_a_current_one`. Passing. |
| `g5.runtime.placement` (unchanged ordering) | Seeded hosts in the existing suites now carry the bundled identity (`helpers::current_runtime_identity`, `host()` in the capacity suite, the runtime peer helper); ordering, refusal cooldown and quota tests are unchanged. | `api_worker_hosts` 51 → 52 tests, `api_host_capacity` 30 → 32 tests passing. |
| `g5.architecture.host_forced_removal` | `useDeallocateWorkerHost` sends `?force=true` only when asked; the page confirms with the VM count for an occupied row and sends force, and shows the 409 reason with `Remove anyway` / `Keep host` after a refused plain removal. Counterexample: a 500 must still show the plain error (no force offer); a 409 must not auto-force. | `WorkerHostsPageContent.test.tsx`: `confirms with the VM count and forces removal of an occupied host`, `offers to remove anyway when the backend refuses a plain removal`, and the existing failed-removal test. Passing. |
| Workers page label | `runtimeStateChip` renders `stale runtime` (with the host's catalog version in the title) and `runtime unknown` for ready hosts only. | `labels a stale-runtime host and a ready host that published no runtime`. Passing. |

### Defects found and repaired before commit

1. The capacity growth test asserted a cloud insert after the decision pass; the pass only
   records the claim and the launch runs in the maintenance tick, so the test now asserts
   the claim row instead.
2. `warn!` was not imported in the host runtime module; the call is now `tracing::warn!`.

## Rechecks

- `cargo test -p g5-internal-api --lib`: 14 passing.
- `cargo test -p g5-api --test api_worker_hosts --test api_host_capacity`: 52 + 32 passing.
- Frontend: `tsc --project tsconfig.build.json --noEmit` clean; `vitest run` for the Workers
  page and hook files: 38 passing; eslint 0 errors (two pre-existing warnings); prettier clean.
- Full `cargo test -p g5-api` and clippy: see the rounds below.

## Independent review round 1 (Codex CLI 0.155.1, `gpt-6-astra`, effort `xhigh`, read-only sandbox; plus a fresh-context Claude read)

Verdict: changes requested. Dispositions:

| Finding | Disposition |
|---|---|
| P1 · A Start stays on its own stale capacity claim (`capacity_failed` forever when the claimed host boots after a deploy). | Fixed: a claim whose host is healthy but not current is spent; the Start claims again (`host_capacity.rs` claim lookup). Test `a_spent_stale_claim_and_a_stale_fleet_free_the_resource_bound`. |
| P1 · A stale automatic fleet holds the quota-plus-one bound: no current reserve, demand blocks retirement. | Fixed: an empty, healthy, stale managed host off the pinned image retires at the next sweep without delay, reserve or demand conditions (`automatic_stale`); cloud cleanup frees the bound. Ontology `g5.architecture.host_idle_retirement` amended. Same test. |
| High (Claude) · Runaway provisioning when the deploy's pinned image lags the catalog: every claim comes up stale, up to the bound. | Fixed: growth waits with `capacity_image_stale` while a healthy host of the pinned image is stale; that host is kept as evidence. (Round 3 moved the comparison from the customer image to the worker boot image.) Ontology `g5.architecture.host_demand_capacity` amended; `vmCapacityWait.ts` explains the wait. Test `a_stale_host_of_the_pinned_image_holds_growth_until_the_pin_moves`. |
| P1 · Rotation accepts a stale replacement as ready and drains its predecessor. | Not changed, recorded as a scoped exception in `g5.runtime.placement.runtime-identity`: a stale replacement implies a stale predecessor under monotonic images, so no placeable capacity is lost; the lagging-image wait owns that situation. |
| P2 · `api_org_admin_settings.rs` quota fixture seeds a host without an identity. | Fixed (seeded with `helpers::current_runtime_identity`). Other identity-less seeds attach VMs directly and never place (checked by the Claude read). |
| P2 · SQL containment and `is_current` disagreed on hash case / extra keys. | Fixed: `is_current` is exact equality with the bundled identity; `HostRuntimeIdentity` no longer `deny_unknown_fields`, so a later field cannot make an older control plane drop a whole hello. |
| High (Claude) · `sha2` import unused outside tests in `g5d-host` would fail clippy `-D warnings`. | Fixed (import moved into the tests module). |
| Low (Claude) · `POST /worker-hosts` answered `runtimeState: ""`. | Fixed: the field is skipped when empty. |
| Rolling-deploy hello rejection by pre-change decoders. | Accepted in the design and recorded as a known limit in the block's metadata. |

Two more defects surfaced by the new tests while repairing the above: cloud cleanup of
retired hosts only covered `automatic_idle` drains (now also `automatic_stale`), and a
spent claim kept its `capacity_start_intent_id`, so the retired host read as the Start's
failed claim (the link is now released when the claim is spent).

## Independent review round 2 (Codex, same configuration) and CodeRabbit on PR #569

Verdict: changes requested. Dispositions:

| Finding | Disposition |
|---|---|
| P1 · A spent claim's link survived when the quota hid its Start from the sweep; the retired host then read as a failed claim forever. | Fixed: every healthy stale host's claim link is released before demand is read, and a terminated host without a classified bring-up failure is not a failed claim. Test `a_spent_claim_is_released_while_the_quota_hides_its_start`. |
| P1 · Idle expiry retired the pinned-image evidence host, so the next sweep provisioned the same stale image. | Fixed: idle retirement applies to runtime-current hosts; stale hosts retire only off the pin. The pinned-image test now keeps the 61-minute idle age. Ontology amended ("past any idle delay"). |
| P1 · The rotation exception's premise fails for a supported image override (current host B, stale tracked image A: rotation drained B for A). | Fixed: replacement readiness requires a runtime-current replacement; a stale replacement of the tracked image waits, and is released from the pair once the track moves on (`stale_runtime_superseded`), leaving it to stale retirement. Ontology `g5.runtime.placement.runtime-identity` rewritten for rotation. Test `api_worker_hosts_stale.rs::a_stale_replacement_never_drains_its_predecessor_and_dissolves_once_the_track_moves`. |
| P3 · Stale tooltip promised growth and removal the settings or the guard may forbid. | Fixed: wording qualified. |
| CodeRabbit · `.ok()` on the image resolution silently lifted the stale-pin guard and let the evidence host retire on any policy or database error. | Fixed: resolved only when a stale host exists; only the "no image source" validation error maps to no pin, every other error fails the sweep. |

## Independent review round 3 (Codex) and CodeRabbit's second pass

Verdict: changes requested. Dispositions:

| Finding | Disposition |
|---|---|
| P1 · The guard compared the customer OCI image; the runtime is baked into the GCE boot image, so a boot-image advance with an unchanged guest digest was misread (growth held, evidence kept, pairs never superseded). | Fixed: the guard, the evidence rule and rotation supersession compare `worker_hosts.boot_image_id` with the boot image the worker family resolves to; the capacity sweep resolves it before the admission lock only while a stale host exists; the round-1 image-source refactor was reverted. Tests vary the guest image and the boot image independently (`a_stale_host_of_the_pinned_image_holds_growth_until_the_pin_moves`, the rotation test). Ontology wording updated in both blocks. |
| P1 · A terminated claim host still held the Start's unique link; the successor's claim violated the index every sweep. | Fixed: terminated claim hosts without a classified bring-up failure release their link in the same pre-pass. Test `a_removed_stale_claim_releases_its_link_before_the_successor_claims`. |
| P1 / CodeRabbit · A pinned or untracked organization resolves no tracked image, so every stale replacement read as superseded and re-provisioned in a loop. | Fixed: supersession needs both boot images known and different; the customer image is no longer consulted. |
| Resolver failure while a stale host exists. | Fails the sweep (`capacity_configuration`) instead of guessing. Test `an_unresolvable_boot_image_fails_the_sweep_instead_of_guessing`. |

## Independent review round 4 (Codex)

Verdict: changes requested. Dispositions:

| Finding | Disposition |
|---|---|
| P1 · The boot-image lookup ran after the admission lock and the host row locks, against the amended contract; a stalled cloud answer would hold placement, hello and heartbeats. | Fixed: a lock-free existence check decides whether to resolve, the resolution runs before `lock_admission`, and a stale host that registers between the check and the lock defers the sweep by one tick. Test `the_boot_image_lookup_never_holds_admission_locks` (a 1.5 s resolver delay; concurrent placement completes in under a second). |
| P1 · Releasing every terminated claim without a classified error wiped exhaustion evidence the claim driver keeps on purpose (unclassified failures leave the code NULL). | Fixed: a terminated claim host releases its link only if it had registered (a fulfilled claim); an unregistered one keeps it. Test `an_unregistered_terminated_claim_keeps_its_exhaustion_evidence`. Ontology `g5.architecture.host_demand_capacity` amended. |

## Independent review round 5 (Codex)

Verdict: changes requested. Dispositions:

| Finding | Disposition |
|---|---|
| P1 · The lookup had no timeout and lifecycle maintenance awaits organizations in turn; one hanging request stalls every organization's Start retries and reconciliation. | Fixed: a five-second bound maps to the configuration-error path. Test `a_hanging_boot_image_lookup_fails_the_sweep_within_its_bound`. |
| P1 · The lookup ran inside the decision transaction, whose `now()` is fixed at transaction start, so a reserve whose heartbeat aged out during the lookup still read as healthy and could let the last current host retire. | Fixed: `sweep_boot_image` probes in its own short transaction, resolves after it closed, and the decision transaction opens afterwards; `reconcile_capacity` takes the result and `maintain` and the tests' `decide` follow the same order. Test `the_decision_clock_starts_after_the_boot_image_lookup` (29-second-old reserve, 1.5-second lookup: the idle host stays). |
| P2 · The new rotation test ran without the suite's serial guard. | Fixed (`#[serial]`). |

## Independent review round 6 (Codex)

Verdict: **APPROVE** on `9ae698b8`, no P1/P2/P3 findings. Codex confirmed placement
SQL/serde, the capacity rules from rounds 1–5, the rotation readiness and supersession
rules, the unchanged stale-host detector, minimal durable state and the frontend flows,
and restated the accepted rolling-deploy limit (pre-change decoders reject a new host's
hello; lease continuity through that window is unqualified).

Rechecks after round 5: `api_host_capacity` 41 and `api_worker_hosts_stale` 18 passing;
`api_worker_hosts` 52 after round 3; the frontend suites unchanged since round 2.

Rechecks after round 1: `api_host_capacity` 34, `api_worker_hosts` 52,
`api_org_admin_settings` 8, `g5-internal-api` 61 unit tests passing; `api_home` passed on
rerun (its earlier failure was unrelated and did not recur); frontend `vmCapacityWait` 16
and Workers page 15 tests passing, `tsc` and eslint clean.
