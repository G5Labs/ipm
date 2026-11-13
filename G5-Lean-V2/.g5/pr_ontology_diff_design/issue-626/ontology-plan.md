# Preserve VM custody and host progress

Design for [G5-Lean#626](https://github.com/G5Labs/G5-Lean/issues/626). The issue is the
fix proposal; this plan records how it is realized and which commitments govern it.
Baseline `49d0187d`. Companion recovery work (cloud linkage, stale replacement deletion)
stays in #627.

## Earlier repair (PR #634)

The following design and executed evidence describe the merged custody repair. The
remaining host-progress work is specified below; it preserves those guarantees.

### What happened before PR #634

- `resume_with` reserves an incarnation (`status = 'starting'`), builds its boot spec,
  then sends `vm.start`. A timeout keeps `starting` for the reconciler, which sends
  `vm.status` after `start_deadline_at` and releases the incarnation on `NotFound`
  (`release_refused_placement`, back to `queued_for_capacity`). The Stop path releases
  on `NotFound` too, and a hello releases every `starting` VM its inventory omits. None of
  these knows whether a Start it never heard back from is still queued on the tunnel, still
  in the host's request channel, or executing before the host has reserved the slot: the
  host registers a VM only when `start()` reaches `state.active.insert`. A released
  incarnation can therefore boot afterwards, consuming a real slot the control plane no
  longer counts (quota and placement both count `starting/running/stopping` rows).
- The tunnel registry publishes a connection's command client before its hello is
  reconciled, so a Start placed during a reconnect can be released by that hello and sent
  anyway.
- `idle_quiesce` decides emptiness and marks the host draining in one `UPDATE` whose
  `NOT EXISTS` subquery keeps the statement's snapshot while the host-row lock is awaited;
  a placement that commits meanwhile is invisible (`host = draining, VM = starting`).
- When the Ready journal write fails, `complete_boot` ignores the result of its own
  `stop`. A stop already owning cleanup answers `Busy` before exit proof, and the boot is
  reported as a definite failure, which the control plane treats as release evidence.
- Adjacent writers: a refused placement requeues the Start intent even after Stop cancelled
  it; `transfer` admits a second pending transfer; `record_start_intent` reads the
  pending-transfer predicate in the same statement that waits for the workspace lock;
  inventory, exit and failure releases leave `start_intent.status = 'starting'` on a
  released row; `release_failed_start` and `release_stopped` settle Agents work even when
  their exact-incarnation fence matched nothing.

## Ontology changes and governance

Ontology changes are in this branch's diff against `origin/main`:

- [`g5.runtime.start.settlement`](../../ontology/software_architecture/hosts.g5.md#g5.runtime.start.settlement) (new): host absence releases a placed incarnation only when its Start is settled or provably undeliverable; the control plane, the host and the tunnel each carry one obligation that makes absence decidable.
- [`g5.lifecycle.transfer.admission`](../../ontology/software_architecture/workspace_vm.g5.md#g5.lifecycle.transfer.admission) (new): one pending transfer per workspace; Start admission re-reads the predicate under the lock.
- [`g5.runtime.host.rotation`](../../ontology/software_architecture/host_control_plane.g5.md#g5.runtime.host.rotation) (metadata): the fresh, serialized idle-custody check it already requires is now implemented.

| Governing block | Why relevant | Implementation must |
| --- | --- | --- |
| [`g5.runtime.host.inventory`](../../ontology/software_architecture/hosts.g5.md#g5.runtime.host.inventory) | Hello is the complete authenticated snapshot | Hello inventory includes pending Starts; a `starting` row whose Start was never sent is not reconciled from inventory |
| [`g5.runtime.control.replay`](../../ontology/software_architecture/hosts.g5.md#g5.runtime.control.replay) | A caller timeout leaves the outcome unknown | Retries keep the Start's identity; the host's pending registry lives from request dequeue to execution end, including replayed and expired identities |
| [`g5.runtime.lease`](../../ontology/software_architecture/hosts.g5.md#g5.runtime.lease) | The backstop when nothing can be proved | No new timeout; unsettled custody waits for lease expiry |
| [`g5.lifecycle.start`](../../ontology/software_architecture/workspace_vm.g5.md#g5.lifecycle.start) | Retries cannot revive cancelled intent | A refused placement requeues only an intent still in `starting` |
| [`g5.lifecycle.stop`](../../ontology/software_architecture/workspace_vm.g5.md#g5.lifecycle.stop) | Logical release does not prove exit | A host Stop of a pending Start cancels it before answering `NotFound`; a stop of a booting VM is journaled as an exit once proven |
| [`g5.runtime.events`](../../ontology/software_architecture/hosts.g5.md#g5.runtime.events) | Effects apply to the current incarnation | Agents settlement follows the fenced release that matched |
| [`g5.runtime.placement.capacity`](../../ontology/software_architecture/host_control_plane.g5.md#g5.runtime.placement.capacity) | Count custody until release | Quota-one organizations cannot admit a second incarnation while a Start may land |

## Design

### Absence becomes decidable

The invariant: **host absence (a `NotFound` reply to status or stop, or omission from a
complete hello) releases an incarnation only if every Start the control plane sent for it
was necessarily observed by the host before the host answered, or if no Start was sent and
none can be.** Three obligations make it hold without a new timeout:

1. **Control plane: record the send before it can be observed.** `workspace_vms` gains
   `start_command_sent_at` (migration 097). `resume_with` stamps it under the incarnation's
   row lock, with an exact `(host, vm, epoch, status = 'starting')` fence, and enqueues the
   Start on the connection inside that transaction (`HostCommandClient::dispatch`, the
   enqueue half of `execute`). Commit follows the enqueue, so any later command another
   writer sends on the same connection is behind the Start, and any release that reads the
   row sees the stamp. A failed fence means the incarnation was released or changed
   meanwhile: nothing is sent. Placement and every release clear the stamp.
2. **Tunnel: publish the client only after hello reconciliation.** The registry sets a
   connection's command client once `accept_hello` committed, so a Start can no longer be
   enqueued on a connection whose hello snapshot predates it. Until then `command_client()`
   is `None` and `resume_with` releases the never-sent placement as `host_control_unavailable`.
   The host already runs control sessions one at a time (the reconnect loop awaits the old
   session), so a hello is built only after every line of the previous session was read.
3. **Host: register a Start at dequeue, before any later command.** The session loop
   registers a `vm.start`'s incarnation in the runtime (`pending_starts`) when it dequeues
   the request and before spawning its execution; the entry lives until `execute` returns,
   whatever the result. While pending, `vm.status` answers `Status { booting }`, the complete
   hello inventory lists the incarnation as booting (the partial heartbeat does not), and
   `vm.stop` cancels the pending
   Start (so `start()` refuses it at reservation with `boot_cancelled`) before answering
   `NotFound`. A cancelled pending Start is absent from inventory because nothing can
   admit it.

Release eligibility is then one rule in `workspace_vm_release.rs`: an unadmitted Start is
released on absence with a fence on whether it was sent (`start_command_sent_at IS NULL`
when the observer read it unsent, `IS NOT NULL` when it was sent and the host answered on a
hello-complete connection). The reconciler reads the stamp before asking the host; the
hello skips `starting` rows without a stamp (their sender decides); the Stop path keeps
treating `NotFound` as proof because the host now cancels a pending Start first and the Stop
command is ordered behind the Start.

Rejected: re-sending the Start to settle it (its payload carries a credential the control
plane does not keep, so the identity cannot be replayed), host-side tombstones with a
retention window (a timeout standing in for proof), and refusing Starts absent from the
roster (a roster snapshot legitimately predates a fresh placement).

### Rotation checks custody after the lock

`idle_quiesce` locks the host row (`FOR UPDATE`), re-reads its status, runs the occupancy
`EXISTS` as a fresh statement, and only then marks the host draining. Placement selects
hosts with `FOR UPDATE OF host SKIP LOCKED`, so a concurrent placement either committed
before the lock (and is seen) or skips the host (and finds it draining after commit). The
busy case rolls back and counts as `skipped_busy`, as before.

### Boot settlement joins the owning stop

After a Ready journal failure, `complete_boot` kills its own VM as today; when the kill is
refused as `Busy` because a stop already owns cleanup, it waits for that stop to remove the
reservation (exit proven) before returning `BootFailed`. A boot cancelled while completing
now journals `Exited` once its kill proved reap, so the control plane's stop completes
through the event instead of waiting for the replay window or the lease.

### Adjacent writers

- `release_refused_placement` requeues the intent only while `start_intent.status = 'starting'`;
  a cancelled or superseded intent keeps its status (host refusals are still recorded).
- `transfer` refuses a new transfer while another is pending for the workspace.
- `record_start_intent` re-reads the pending-transfer predicate after `FOR UPDATE`.
- Inventory, exit, failure and forced-loss releases of a `starting` row mark the intent
  `failed`, as lease expiry and definite boot failure already do.
- `release_failed_start` and `release_stopped` settle Agents work only when their fenced
  update matched one row.
- The process backend's waiter (S32) already refuses to manufacture a receipt: a failed
  `wait()` publishes `WaitFailed`, which every custody owner treats as unproven and
  retries. No change; recorded here so the gap stays visible.

### Assumptions

- Hosts run one control session at a time and read a connection's lines in order; the
  API's `serve_control_stream` writes commands in enqueue order. Both hold today.
- The host's `vm.status` for a pending Start reports `Booting` with the spec's epoch; the
  reconciler's existing identity match defers on it.
- `start_command_sent_at` is the only new durable state; it is cleared with custody.

## Verification

- `api_workspace_vms` (real Postgres, scripted host): a Start whose reply never arrives is
  not released by the deadline reconciler when the host reports it booting or pending; a
  Stop of a pending Start releases only after the host answers for the ordered Stop; a
  hello that omits an unsent `starting` row leaves it alone while a sent, absent one is
  released; a quota-one organization keeps one incarnation across the delayed Start.
- `api_worker_hosts_stale`/`api_host_capacity`: a placement committed while rotation waits
  for the host row keeps the host `ready` (`skipped_busy`).
- `g5d-host` unit tests: pending Start registry through status, inventory and stop;
  cancelled pending Start refused at reservation; Ready-journal failure with a concurrent
  stop returns only after exit proof; cancelled boot journals `Exited`.
- `host_ctl` unit tests: the command client is absent until hello acceptance.
- Transfer and intent tests: second pending transfer refused; refusal does not revive a
  cancelled intent; releases mark a `starting` intent failed.

## Implementation status

Implemented on this branch. Executed locally against Postgres 17 (dev stack on 5432)
with the pinned 1.97.1 toolchain:

- `g5d-host` lib: the two pending-start qualification cases
  (`a_received_start_is_booting_inventory_before_it_reserves`,
  `a_stop_cancels_a_pending_start_so_it_can_never_reserve`), the inline
  `a_failed_ready_journal_holds_custody_until_exit_is_proven` and
  `a_failed_ready_journal_joins_a_concurrent_stop_that_owns_the_kill`, and the
  extended cancelled-boot custody cases pass.
- `g5-api` integration (real Postgres): `idle_quiesce_yields_to_a_placement_that_commits_while_it_waits`,
  `hello_absence_spares_an_unsent_start_and_releases_a_sent_one` (also asserts the
  released row's intent fails with it, for the hello and the exit-event loss paths),
  `a_delayed_start_holds_its_quota_slot_against_a_second_incarnation` (quota-one),
  `a_late_refusal_does_not_revive_a_cancelled_start_intent` (D5 and the delivery fence),
  `a_second_pending_transfer_is_refused`,
  `a_transfer_admitted_while_a_start_waits_for_the_lock_fences_it`, and the kept
  `expired_start_deadline_keeps_an_incarnation_the_host_still_reports`,
  `pending_transfer_blocks_a_queued_start_intent`,
  `delayed_reconciliation_cannot_revive_a_superseded_start` pass.
- `a_settled_refusal_retracts_the_pending_start_a_hello_advertised` and
  `a_stop_answered_absent_retracts_the_pending_start_a_hello_advertised` cover the
  retraction from `worker_hosts.reported_live_vm_ids` on a settled non-admission
  (the hello-path residue from Codex round 2); the partial heartbeat intentionally
  never advertises or retracts a pending Start, as the design records.
- `a_failed_ready_journal_joins_a_concurrent_stop_that_owns_the_kill` forces the
  concurrent-Stop `Busy` interleaving with the test-only journal write barrier
  (`EventJournal::hold_writes`) and asserts no `BootFailed` until the owning Stop
  proves exit; the single-actor journal-failure and cancelled-boot tests remain.
- `migration_097_start_command_sent_at`: the migration applies on a clean database
  and on one already migrated through main's 095, and replays as a no-op.
- `cargo clippy` on `g5-api-core` and `g5d-host` (all targets) is clean.

Remaining gaps:

- S32 (process-backend waiter recovery) is unchanged: the waiter already publishes
  `WaitFailed` and never manufactures a receipt. Recorded as out of scope here.
- No Linux/KVM run: the host-side behavior is exercised against the scripted launcher
  and the in-process scripted host peer only.

## Host progress follow-up (issue #626, October 22)

Authorized branch: `issue-626-preserve-vm-custody-through-ambiguous-starts`;
starting revision `ca780054`. The user requested design, implementation, verification,
PR publication and monitoring of checks, merge conflicts and blocking review findings.
The [current issue](https://github.com/G5Labs/G5-Lean/issues/626) supplies the acceptance criteria.

The remaining defects share no need for a new durable lifecycle. Runtime already owns
VM reservations and exit evidence; control owns command identities; the API lifecycle
owns custody release. Preserve those owners and the earlier Start-settlement rules.

- **Runtime stop ownership and reconciliation.** Admit each stop under the runtime state
  lock and install its cleanup task before yielding. The task retains the handle and
  reservation until exit proof, independently of a cancelled command or reconciliation
  caller. Reconciliation applies a newer roster by admitting all stale stops under that
  lock, then returns; it does not await their exit. The generation mutex orders admission
  only, and the applied generation advances after admission. Existing boot cancellation
  continues to own cleanup when no handle has been published. Shutdown joins admitted
  cleanup within its existing deadline; process restart uses the existing incarnation
  recovery. At most one stop owner exists per active VM, bounded by host slots. Exit
  completion is fenced to the original handle so delayed work cannot remove a successor.
- **Command admission.** Reuse the process-local identity registry to reject an identical
  active retry with transient `Busy`, or changed payload with `IdempotencyConflict`, before
  consuming an execution permit. Only admitted identities register a pending Start, in
  receive order. The processor owns the existing 16-command budget across reconnects;
  neither duplicate waiter tasks nor session resets multiply it. Completed results keep
  the existing retention/replay behavior. Rejecting active retries is sufficient because
  callers already retry `Busy` with the same identity; no subscription/fan-out owner is
  needed. Truly distinct stuck commands can still consume the budget.
- **Orphan cleanup.** Reverse sweeping reaps only terminated or absent hosts after the
  sighting grace. An unreachable host is still owned and stays in the existing silent-host
  drain/retirement lane. Both candidate selection and the final delete admission also
  refuse any unreleased VM row for that host, using API-owned custody. This is deliberately
  stronger than testing lease expiry: the lifecycle owner must release expired custody.
  A terminated host cannot accept new placement or revive through hello; an absent row
  cannot hold a VM because of the foreign key. Thus no new durable delete claim or lease
  policy is necessary. Lookup failure fails closed. Protected candidates clear sightings;
  failed deletes retain them for retry. Rowless and released terminal hosts still converge.

A timeout that releases the stuck reservation violates custody. Merely spawning a whole
reconciliation still strands its later stops, and moving duplicate gate waits outside the
semaphore leaves unbounded waiters. The selected mechanisms remove those dependencies
while preserving their safety rules.

Governing changes: [host cleanup progress](../../ontology/software_architecture/hosts.g5.md#g5.runtime.cleanup.progress),
[command replay](../../ontology/software_architecture/hosts.g5.md#g5.runtime.control.replay),
and [orphan custody](../../ontology/software_architecture/host_control_plane.g5.md#g5.runtime.host.orphan-custody).
The self-review checked receive-order Start registration, concurrent Stop/boot settlement,
stale generations/completions, restart custody, placement eligibility and lease-renewal
writers. No VM waiter, transfer-history or new product policy is introduced.

Verification will exercise real mux control sessions with scripted exit evidence, stop
caller cancellation, multiple stale VMs and newer/older rosters, duplicate retries beyond
16 slots, retained replay and conflicting payloads; real Postgres detector tests will
cover a short grace, candidate/final-admission races, expiry/release and delete retry.
Run host and affected API suites, formatting/lint and the PR's Rust/E2E checks. Scripted
cloud and VM peers establish orchestration guarantees, not real provider or KVM behavior.

### Local verification of the follow-up

- `cargo test -p g5-api --test integration api_worker_hosts_stale:: -- --test-threads=1`:
  all 32 tests pass against native Postgres 16, including both new orphan-custody cases
  and the retained Start-settlement/rotation regressions.
- `cargo test -p g5d-host --lib -- --skip unready_dispatcher --skip preparing_a_retained_root --skip an_unproven_process_boot`:
  reports 103 passed, one ignored and six filtered after the review additions. The five new stop/control regressions
  execute and pass, covering real mux reconnect/heartbeat/events/commands, retries beyond
  the command budget, caller cancellation, cancelled generation retry and late completion.
  Inherited materialization qualification tests also self-skip on this filesystem.
- `cargo clippy -p g5d-host -p g5-api-core --all-targets -- -D warnings` and
  `cargo fmt --all -- --check` pass. Final-candidate checks are also recorded on the PR.

The unrestricted local host run reports five existing materialization-related failures:
three unready-dispatcher cases, retained-root preparation and the process-boot hidden-child
fixture. This MicroVM exposes only ext filesystems and its kernel has no loop/XFS support;
installing XFS tools did not make the fixture mount possible. The new stop/control tests
seed their already-running boundary so they execute here without weakening materialization.
CI must run the inherited reflink fixtures and the full required Rust/E2E lanes. Cloud
provider behavior and actual KVM exit proof remain outside these scripted boundary tests.


The 71 `api_workspace_vms` integration tests also pass. Restoring the affected pre-fix
production paths in an isolated checkout makes all seven initial regressions fail for
the intended reasons: blocked traffic/retries, abandoned cleanup/generation, stale
completion, and deletion despite custody. The added distinct-command reconnect and
cancelled-boot scheduling tests also fail against those old paths.

[PR #714 review](https://github.com/G5Labs/G5-Lean/pull/714) identified an attributable
shutdown polling issue and two coverage/reliability gaps. The cancelled-boot wait now
uses its existing polling timer once cancellation is already signalled, instead of
rescheduling continuously while proof is outstanding. The mux regression fills all 16
slots before disconnecting and verifies refusal and later recovery on the new session.
The orphan tests use the actual 120-second grace floor and a five-minute backdate, and
all three orphan tests pass again. The optional fixture-wrapper cleanup is deferred
[inline](https://github.com/G5Labs/G5-Lean/pull/714#discussion_r4345913757); it does not
change execution or acceptance. These are implementation/verification refinements
within the same design and ownership rules.
