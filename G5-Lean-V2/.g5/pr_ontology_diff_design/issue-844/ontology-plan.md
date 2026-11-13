# Dispatch only through a reachable host

[Issue #844](https://github.com/G5Labs/G5-Lean/issues/844) reports personal Home
Start-and-send timing out after readiness. Multiple APIs share PostgreSQL while
host tunnels are process-local. A foreign replica can reserve an input or renew
the shared two-second retry timestamp before failing runtime invocation with
`HOST_TUNNEL_UNAVAILABLE`. Active-attempt claims similarly spend the shared fair
batch on hosts they cannot reach. Qualification also exposed the separate
VM-placement defect described in #758. The continued design below includes the
placement and Start-reconciliation ownership needed to complete that qualification.

## Decision and owners

Specialize [dispatch eligibility](../../ontology/features/chat.g5.md#g5.chat.dispatch-eligibility)
with [host reachability](../../ontology/features/chat.g5.md#g5.chat.dispatch-reachability).
The existing tunnel registry owns local connection facts; WorkspaceRuntime
exposes a fresh snapshot for scheduling, and continues to enforce current
owner, VM, epoch, lease and connection authority at RPC admission. TaskService
owns reservation and retry selection under the existing database locks. Task
maintenance owns fair reconciliation selection and the commit-before-RPC sequence.

Filter queued agent, command and preview reservations and unconfirmed retries
by the VM's host before locking/advancing task or input state. Use the same
reachability source when claiming active attempts. An absent, expired, deleted
or replaced incarnation remains eligible for database-only loss settlement on
any replica; a live but disconnected incarnation is not evidence of VM loss.
Keep cancellation of ended Start consents and native-request deadline decisions
database-driven. Pending native reply selection does not advance a shared retry
timestamp or consume the reply, so it cannot cause this starvation mechanism;
its existing exact-attempt delivery and durable cancellation intent remain.

Reuse the registry and canonical execution/reservation identities. A new durable
dispatch owner, leader election, forwarding service or claim lease would duplicate
connection state and add recovery obligations. A post-claim connection loss leaves
the committed attempt intact. A fresh retry snapshot excludes the disconnected
API, and reconnect/transfer resumes the same identity after the existing bounded
retry interval. Do not reset timestamps after RPC failure: an ambiguous invoke
may have executed, and an unconditional reset could overwrite another claimant.
The snapshot is an eligibility hint, never runtime authority or a promise that
the connection survives. No database migration or unbounded work queue is added.

## Validation

Use real PostgreSQL and two independent AppStates with one controlled host RPC
peer. Force the disconnected replica to run first, then check untouched durable
input/retry/reconciliation state and immediate dispatch by the connected replica.
Exercise competing passes, reconnect/route transfer, accepted invocation with
lost response, Stop and replaced/expired epochs. Assert the retained execution
identity and one durable user message; the controlled guest models native
acceptance, so separately retain/run the real dispatcher's idempotency regressions.

Run affected API tests and normal format/lint checks. Repeat the existing personal
browser flow three times in its existing selector (each with fresh message content
and the normal stop/start fixtures) under the multi-API CI configuration and qualify the full E2E lane
on the published candidate. Preserve its 20-second post-readiness assertion and
first-attempt gate. Record exact revisions and distinguish local boundary tests,
CI observations and deployment evidence. No deployment is part of this change.

## Evidence

At `06a58e973d0bd97cc976a50ad922c309c3161bad` (tests, no production fix), the
personal wrong-replica-first and unconfirmed-retry regressions fail on unexpected
durable state changes after the disconnected replica's maintenance pass. The
connection-loss and accepted-invoke/lost-reply cases fail the same untouched-state
assertion. The additional command/Stop test fixtures required corrections (a
supported command envelope and clearing the stopped incarnation fields); those
fixture failures are not evidence of the dispatch defect.

At `472678508e1cf63cf7d3c39ec66298bba25e201b`, all six tests in
`api_dispatch_ownership` and 168 affected API regressions pass against real
PostgreSQL. The host endpoint controls native acceptance and journal replay;
the separate real-dispatcher idempotency/cancellation regression also passes.
Clippy and full Rust CI pass. In E2E build
`21a3bf05-67d5-4f63-a234-efab9e1af008`, all three personal Start-and-send
repetitions pass with retry zero, in 12.200s, 11.477s and 12.291s (whole-test
durations). Both broad browser shards, provider browser tests and Rust E2E pass.

Full qualification fails because the value-loop formal-review integration test
hits the 720-second harness timeout, leaving no phase or runtime state in its
captured output. The cause is not established by this result. The next candidate
adds periodic phase, task, input and VM snapshots to that existing test, with
bounded read-only diagnostics and unchanged assertions/deadlines. This lets CI
distinguish readiness, dispatch and later review stalls before cleanup. Archived
container-log downloads are unavailable to the supplied build-log reader; Cloud
Build logs remain readable. The local VM has no `/dev/kvm`, so full qualification
continues through CI. The PR records subsequent results on their exact candidate.

On diagnostic checkpoint `40ed52ed67ff6ea67a2ca42a8e833a8fcfc459af`, full Rust CI
passes again and the formal-review scenario passes in 350.263 seconds. Its prior
timeout remains unattributed. Review found that awaiting diagnostic SQL inside
the phase loop could delay readiness polling. The final diagnostic version runs
one bounded snapshot independently, cancels it when the phase ends, and prints
report progress immediately. Qualification is repeated on the resulting head.

## Placement follow-up from qualification

At `a326d3205806fb159d244fb5d0499468cccf85b4`, full Rust CI and all three personal
Start-and-send cases pass again without retries (10.102s, 12.229s, 12.286s whole
tests). The formal-review scenario times out at 720 seconds. Its captured runtime
states identify a pre-dispatch stall: review `b7a31d14-32c6-4937-a247-61f48fba2f4f`
has no task, while workspace `a44c26ad-1e48-59b8-8961-267b8d65cd2f` remains
queued for capacity, advancing epochs and renewing exclusions for host
`ef482c1f-5c4a-4493-b8d3-0de12a1c4616` at about 30-second intervals. This is
consistent with #758 and establishes that task dispatch has not yet been reached;
a deterministic two-service test must establish the placement mechanism.

Extend [placement](../../ontology/software_architecture/host_control_plane.g5.md#g5.runtime.placement.reachability)
using the same registry, selecting only live connections with a control client.
WorkspaceVmService passes that hint to the existing atomic placement owner before
host reservation; quota, capacity, Start consent and incarnation fences stay there.
The final control dispatch still rechecks availability. An unsent Start cannot
represent a host refusal, so its fenced release returns the existing intent to the
queue without adding or extending host cooldowns. Preserve actual host refusal
cooldowns and retain ambiguous sent incarnations for status recovery. Filter that
status-reconciliation selection before a foreign API can advance the shared retry
deadline. Database-only lease/loss handling remains available without a tunnel.

This adds no durable owner or forwarding service and does not change boot budgets,
browser concurrency, first-attempt qualification or the unrelated fixture cleanup
work in #758. Validate foreign-first placement, reconnect/control transfer,
connection loss after reservation, genuine refusal cooldown and starting-status
reconciliation with real PostgreSQL and controlled host commands; then rerun the
affected lifecycle/placement suites and full CI on the published head.

At `55e8506f1524f1339c79748db7b06f6690950ba4`, both new placement regressions fail
against the unfixed production path: the disconnected API increments the VM epoch
from 1 to 2, and its status pass advances the expired starting deadline by about
30 seconds. These are real PostgreSQL tests with independent connection registries.
The fix supplies mandatory control reachability to atomic placement, restricts
starting-status selection, and uses the existing fenced `start_sent` fact to avoid
adding a host exclusion when no Start was sent. Additional coverage exercises an
unsent reservation released after connection loss, an RPC-only connection, and
immediate placement after control transfers to the second API. The existing real
host-refusal fixture also asserts its exclusion remains in force.

All nine two-replica task/placement tests pass locally with the combined fix.
Broader lifecycle/capacity regressions and full CI are rerun on the published
candidate; the PR records their exact revision and results.
