# PR candidates for testing the ontology skills

The first three candidates are a calibration set covering a new feature, a bug fix,
and a refactor. Candidates 4–6 are larger repairs across module boundaries; experiment
7 proposes a larger new feature. They draw on recorded ontology gaps, open issues and
product designs; confirm the starting capability against current code before a trial.
Each brief defines goals and success criteria, with the linked ontology supplying the
governing commitments.

The first three are known-gap closure tasks, including a missing capability, rather than
evidence of discovering previously unknown defects. The [evaluation appendix](ontology-skills-evaluation.md)
freezes equivalent inputs, cases, evidence and costs for those three only. Before
trialing a stretch candidate, freeze an equivalent obligation/evidence matrix for it as
well. Preparing these briefs does not run or authorize a trial. The original order is
stale Save, allocation ownership, then recovery.

## Experiment setup

A **GPT-6-Astra master session** acts as the human supervisor and evaluator. Each
condition runs in a separate **GPT-6-sol, extra-high reasoning child session**, with
its own branch and isolated worktree, starting from the same application code snapshot.
The ontology condition receives the repository's ontology and maintained skills;
the plain condition removes that context and explicitly prohibits ontology/skill use.
Check effective repository, parent and global instructions, skill discovery and memory
before dispatch: a prohibition alone does not establish a skill-free context. Record
any inherited skill descriptions or instructions as a limitation of the plain condition.
Separate sessions and worktrees isolate conversation and edits, not filesystem access;
record any exposure to other runs or inherited skill descriptions.

Whenever a child pauses for a question or design approval, the master reads its saved
artifacts and, for the ontology condition, the relevant governing commitments. It
mimics a human by choosing the most production-ready option within the agreed goal
and answering only the decision needed to continue. The child owns implementation,
verification and review. Plain runs receive the generic review-and-fix request below
after implementation.

The minimal-answer strategy below avoids turning supervision into extra solution
input. Experiments 1–2 nevertheless received additional assistance in the ontology
condition; the [results](ontology-experiment-results.md) disclose those deviations.

### Minimal worker instructions and supervision

Give workers only the agreed minimal Recorded gap and Goal. Keep the success criteria,
design review focus and evaluation appendix for the evaluator; do not pass them or
other runs' findings into the worker's context.

Keep follow-ups high-level, for example:

- "Approved; continue with implementation and review."
- "Choose the smallest production-ready option."
- "Follow the skills/instructions to do this." (Only for a condition with skills enabled.)
- "Fix all remaining gaps."

Do not leak specific defects, expected solutions, extra invariants, targeted test cases
or implementation details through approval or review messages. Let the worker discover
them from its assigned context. Keep the same minimal task brief, code snapshot, model,
environment and feedback policy across comparison conditions, with ontology/skill
availability as the intended difference. Preserve outputs before common evaluator checks.
Record exact follow-ups; if extra information is supplied to one condition, report that
run as additionally assisted and do not attribute the difference to ontology alone.

### Parallel execution and coordination

Run the ontology and plain conditions **in parallel by default** once the experiment
is authorized and both environments are ready. Freeze the common application snapshot
and brief first; neither condition starts from the other's implementation. Keep these
boundaries throughout the run:

- Separate fresh sessions, branches and worktrees; no shared conversation history or
  access to the other run's plans, code, results or evaluator artifacts. Do not merge,
  cherry-pick or change the harness during the comparison.
- Separate disposable databases, ports, containers/volumes, temporary files and mutable
  build/test outputs. Check effective connection settings before running tests; never
  fall back to shared development services. Scope cleanup to the owning run.
- Equivalent dependencies and resource budgets. If a resource cannot be isolated,
  serialize the affected checks and record waiting/contention separately from worker
  effort; parallel elapsed time alone is not an efficiency measure.

[Worktrees share Git metadata](https://learn.chatgpt.com/docs/environments/git-worktrees)
and do not prevent reading another checkout. Where enforced access separation is
needed, use separate containers/VMs with only each condition's permitted snapshot and
context available. Otherwise describe isolation as procedural and record observed
cross-run access; do not claim that different directories guarantee no interference.

The same master may coordinate and answer both conditions using the minimal-answer
policy, with separate decision logs. Answer from the common brief and that condition's
allowed context, never from the other implementation or evaluator findings. A genuinely
missing product requirement must be clarified consistently for both, without conveying
a discovered solution. The master knows both runs, so this is not blind supervision;
subtle guidance leakage remains possible. For a comparison requiring stronger
independence, use separate supervisor contexts, each seeing only its assigned condition,
and let the master synthesize after both outputs are frozen.

### Post-run analysis and comparison

After the ontology worker finishes implementation and review, the master examines the
saved design, ontology diff, code changes, checks and public session history, then answers:

1. **Harness reliability:** Did the agent follow the intended workflow? What rough
   edges, missed steps or master interventions occurred?
2. **Design readability and fidelity:** Are the ontology diff and high-level design
   human-readable? How well do they reflect the actual code changes? Identify omissions,
   drift and consequential decisions made during implementation.
3. **Value and cost:** Was ontology helpful, and how? Did pre-generation design and
   post-generation review materially improve quality? Separate their observed effects
   from master assistance. Compare planning, coding, review and repair effort, including
   time spent understanding/editing ontology versus delivering working code. Report
   unavailable timing or token evidence as unknown.
4. **Missing knowledge:** What was missing from the ontology for a good plan? Did design
   review help? Recommend concrete changes to principles, invariants or harness guidance
   only where the evidence shows a useful gap.

Keep this analysis out of the still-running plain worker's context. If the plain run
has not already started in parallel, launch it from the same frozen snapshot in its own
session and branch/worktree, without ontology or skills and with only the same Recorded
gap and Goal. It directly implements the goal using its native coding workflow. Save
its implementation before sending this exact follow-up:

> Do a code review, making sure what you implemented is correct and fix anything if needed.

Let it finish review and repairs under the same minimal supervision policy. Preserve
the ontology run's initial design and available pre-review implementation as well as
both final deliveries; keep any later evaluator-directed repairs separate.

Once both conditions finish, give the user a comprehensive comparison of their steps,
implementation quality, design choices, review corrections, remaining defects and
cost. Use the common [evaluation criteria](ontology-skills-evaluation.md), with explicit
score denominators and evidence gaps; apply the same evaluator checks to both frozen
outputs and distinguish retrospective probes from checks selected before the run.
Explain why outcomes differ using concrete evidence, and separate the combined
ontology/harness benefit from extra supervision and verification effort. This practical
comparison does not isolate the causal contribution of ontology, planning or review.

Save a **concise result under a new experiment section** in
[ontology-experiment-results.md](ontology-experiment-results.md): setup, quality scores,
key differences and their likely causes, time/ontology overhead, limitations and
actionable takeaways. Keep detailed artifacts in the run directory and link them.

## 1. New feature — Recover conversations when native restore fails

**Recorded gap:** Native-session restore selection exists, but deterministic
delivered-history recovery is absent.

**Goal:** When native restore is absent or refused, start a fresh native session
with bounded, deterministic history of positively delivered inputs plus the current
input. Exclude input without positive delivery evidence, including uncertain or withdrawn
input; proven late acceptance makes it eligible for a later reservation. Exclude
undelivered device-auth-failed input and old native answers. Freeze recovery selection
with the prompt and tell the user when context was truncated or lost.

**Success criteria:** Both missing and refused restore enter the fallback; the
same eligible history produces the same bounded recovery context; excluded inputs
never enter that context; successful native restore continues to work.

**Governing ontology:**
[Recovery](../../../.g5/ontology/features/conversations.g5.md#recover-a-conversation-from-known-delivered-history)
(`g5.conversation.recovery`) and
[exclusions](../../../.g5/ontology/features/conversations.g5.md#exclude-undelivered-inputs-from-recovery)
(`g5.conversation.recovery.exclude`),
[evidence cutoff](../../../.g5/ontology/features/conversations.g5.md#freeze-recovery-evidence-with-the-reserved-prompt)
(`g5.conversation.recovery.snapshot`), and
[frozen prompt](../../../.g5/ontology/features/conversations.g5.md#freeze-the-bounded-provider-prompt)
(`g5.conversation.inbox.prompt`).

**Design review focus:** recovery and data authority, provider encapsulation and change
locality. Name the history-selection and rendering owners; explain eligible transcript
types, ordering, truncation and the distinction between replayed context and new delivery.
Preserve both provider transports, bundle completeness and private-state exclusions.

## 2. Bug fix — Prevent stale editor saves after workspace replacement

**Recorded gap:** Editor Save checks file versions but lacks the observed-VM
precondition.

**Goal:** Bind Save to the VM incarnation the editor observed, alongside the
existing file-version check. If Stop followed by Start replaces that VM, reject a
stale save and preserve the user's buffer, even if the replacement file has the
same hash.

**Success criteria:** A save from the previous incarnation cannot modify the
replacement VM; a save against the current incarnation and expected file version
succeeds; existing version-conflict protection remains effective.

**Governing ontology:**
[Observed incarnation](../../../.g5/ontology/features/documents.g5.md#save-binds-bytes-and-the-attachment-they-came-from)
(`g5.documents.save`),
[Source editing](../../../.g5/ontology/features/documents.g5.md#retain-source-editing),
[file versions](../../../.g5/ontology/features/documents.g5.md#require-a-file-version-before-mutation),
and [runtime admission](../../../.g5/ontology/software_architecture/workspace.g5.md#one-live-runtime-facade)
(`g5.documents.editor.source`, `g5.documents.files.version`,
`g5.architecture.runtime_facade`).

**Design review focus:** identity, complete mediation and concurrent state changes.
Trace the incarnation observed at content load through the actual write boundary,
including [reload](../../../.g5/ontology/features/documents.g5.md#reload-before-editing-a-new-attachment)
and [dirty-buffer handling](../../../.g5/ontology/features/documents.g5.md#expose-concurrent-disk-changes).
Select all applicable save adapters without expanding into unrelated file features.

## 3. Refactor / gap closure — Restore TaskService allocation ownership

**Recorded gap:** JobService directly allocates task/session/input records outside
TaskService ownership.

**Goal:** Route task and initial-input allocation through TaskService, composing
ChatService's session-creation operation when needed, while preserving existing job
behavior. Let the owning operations participate in the caller's
transaction so bootstrap receipt, sessions, tasks and dependency edges commit
atomically.

**Success criteria:** JobService delegates the affected allocations; successful
job creation preserves its observable behavior; a failed bootstrap graph leaves
no partial records; delegation does not introduce separate commits per task.

**Governing ontology:**
[Task ownership](../../../.g5/ontology/features/agents.g5.md#separate-logical-outcome-from-execution-attempts),
[bootstrap atomicity](../../../.g5/ontology/features/agents.g5.md#commit-the-whole-bootstrap-graph-atomically),
and [transaction composition](../../../.g5/ontology/concepts/system_design_principles.g5.md#compose-owners-without-splitting-atomic-effects)
(`g5.agents.lifecycle`, `g5.agents.bootstrap.atomic`,
`g5.design.transaction-composition`).

The [allocation composition](../../../.g5/ontology/software_architecture/agents.g5.md#compose-allocation-through-the-owning-services)
(`g5.architecture.allocation_composition`) owns the TaskService/ChatService boundary.

**Design review focus:** complete operations, information hiding, minimal APIs and
atomic composition. Move validation and protected writes under their owner, preserve
caller rollback and replay, and prohibit dispatch before the transaction commits.
An extra forwarding method alone does not establish ownership.

## Three stretch candidates — cross-module integrations

Target roughly **1,000–2,000 changed lines including production code and tests** for
each candidate. The estimate is a scope check, not a quota: confirm the gap and the
available fixture/dependency path first, then keep one candidate to one reviewable PR.
These remain recorded-gap closure tasks; they test difficult integration and design
work, not independent defect discovery.

### 4. Bug fix — Retire flushes owned by a released VM incarnation ([#146](https://github.com/G5Labs/G5-Lean/issues/146))

**Recorded gap:** A flush started by an incarnation that dies before completion remains
open. Its partial unique index then blocks every later flush for that workspace, so a
new incarnation's Stop and a transfer can fail indefinitely. A duplicate start from the
same incarnation currently surfaces as a database conflict rather than a typed API
conflict.

**Goal:** Make flush ownership follow the workspace VM incarnation through start,
completion, failure and release. Releasing an incarnation must close any unfinished
flush as lost/incomplete, while refusing late writes or completion from that released
incarnation. A successor must be able to start a fresh flush without mistaking the old
capture for a complete recovery point.

**Integration scope:** Coordinate the flush service and its SQL uniqueness/state model
with every VM release path (ordinary Stop, host loss/forced loss, transfer and host
drain), API error mapping, and the callers that retry or replay flush operations. Keep
the workspace's last verified complete receipt as the recovery cut. Concurrent start,
release and late completion must settle against one durable outcome, including after an
API restart.

**Success criteria:**

- A second distinct flush start for the same live incarnation returns a stable typed
  conflict; retrying the same request key remains replayable.
- Release atomically terminalizes open flushes owned by that incarnation, without
  changing completed receipts or recording an incomplete capture as success.
- A late completion or Git write from the released incarnation is refused, even after
  a successor is attached; the successor can start and complete its own flush.
- Stop, transfer, host-loss and drain flows recover from an abandoned flush without
  leaving a 503 loop or a partially advanced repository vector.
- Real Postgres concurrency/failure cases and production-boundary lifecycle checks
  prove the result; a mocked transaction alone is insufficient.

**Design review focus:** durable state-machine ownership, transaction boundaries,
idempotency and incarnation fencing. Trace every release path and late callback to the
same persistence authority. Explain how retry identity differs from a new attempt and
why a lost flush cannot advance restore selection.

**Governing ontology:**
[verified receipt selection](../../../.g5/ontology/software_architecture/durability.g5.md#only-verified-complete-capture-advances-recovery)
(`g5.git.receipt`),
[unfinished flush write admission](../../../.g5/ontology/software_architecture/durability.g5.md#admit-flush-writes-only-for-their-unfinished-receipt)
(`g5.git.receipt.write-admission`),
[incarnation release](../../../.g5/ontology/software_architecture/lifecycle.g5.md#release-only-the-stopped-incarnation)
(`g5.lifecycle.stop.release`), and
[durability failure](../../../.g5/ontology/software_architecture/durability.g5.md#a-durability-failure-preserves-recoverable-work)
(`g5.git.flush-failure`).

**PR size estimate:** 1,000–1,600 changed lines, including migration/state handling,
service and lifecycle integration, and real database/API regression coverage.

### 5. Feature / lifecycle fix — Drain a worker host before removing it ([#155](https://github.com/G5Labs/G5-Lean/issues/155))

**Recorded gap:** Removing a host with active VMs returns 409 and leaves the operator to
manually coordinate drain, per-VM Stop, polling and a second delete request. The UI
surfaces the raw conflict. The existing host rotation flow already has a related
drain-and-teardown sequence, but operator deletion does not use it.

**Goal:** Turn one ordinary host-delete request into a durable, replayable operation:
mark the host draining, stop its VMs through the API-owned per-VM lifecycle primitive,
and let reconciliation terminate cloud capacity once custody is empty. Empty-host
deletion stays immediate. `force=true` remains the explicit destructive escape hatch
and records loss per affected VM.

**Integration scope:** Compose operator API admission, host drain state, workspace Stop
and flush policy, host custody/exit proof, cloud teardown, the background reconciler,
and the Worker Hosts UI. Reuse or extract the credential-rotation sequence where the
ownership contracts align; do not add another direct host-command path. Proposed
deadline rule: if a VM cannot stop by the drain deadline, retain the host in a visible
blocked/draining state and require an explicit force action rather than silently
recording work loss.

**Success criteria:**

- Busy-host delete returns an accepted drain identity and immediately excludes the
  host from new placement; repeated delete requests return the same in-flight result.
- Each VM follows the normal Stop flush, notice, retry and release rules. A blocked
  flush or uncertain exit proof cannot be bypassed by cloud teardown.
- After all VMs have positive release/exit proof, reconciliation terminates the host
  and cloud resource once, including after API restart. Empty-host delete remains
  immediate and idempotent.
- Deadline expiry is visible and nondestructive until the operator explicitly chooses
  force; force records per-VM loss before termination.
- The UI explains the drain, shows progress/blockers, and offers a separately labelled
  force action. API, reconciler and browser tests cover busy, empty, retry, restart,
  blocked-stop, force and partial-cloud-failure cases.

**Design review focus:** operation identity and replay, Stop/host-custody ownership,
cloud side-effect ordering, and recovery after process failure. Draw the state machine
from request through terminal host state; identify which component owns each retry and
prove that a stale reconciler cannot terminate a replacement host generation.

**Governing ontology:**
[host placement and drain ownership](../../../.g5/ontology/software_architecture/worker_host.g5.md#capacity-and-runtime-placement)
(`g5.architecture.placement`),
[Stop before release](../../../.g5/ontology/software_architecture/lifecycle.g5.md#stop-fences-future-work-before-releasing-custody)
(`g5.lifecycle.stop`),
[release only the stopped incarnation](../../../.g5/ontology/software_architecture/lifecycle.g5.md#release-only-the-stopped-incarnation)
(`g5.lifecycle.stop.release`),
[prove host exit](../../../.g5/ontology/software_architecture/runtime.g5.md#release-capacity-only-with-exit-proof)
(`g5.runtime.custody`), and
[record loss at an authorized deadline](../../../.g5/ontology/software_architecture/lifecycle.g5.md#report-state-lost-at-an-authorized-deadline)
(`g5.lifecycle.deadline.loss`).

**PR size estimate:** 1,200–1,900 changed lines across API/service/reconciliation,
frontend flow, and integration coverage.

### 6. Security fix — Keep repository-controlled Git config outside privileged capture ([#176](https://github.com/G5Labs/G5-Lean/issues/176))

**Recorded gap:** Dispatcher Git operations run with elevated identity while reading
workspace-controlled Git configuration. A clean filter or hook can execute as that
identity during flush; URL rewrites or proxy settings may also redirect a temporary
credential-bearing transport lease. Clearing environment variables or repairing file
ownership afterward does not prove these paths safe.

**Goal:** Ensure repository-controlled Git behavior cannot execute with dispatcher
privilege or access/redirect a temporary transport credential during snapshot, fetch,
pack or push. Preserve legitimate workspace Git behavior and exact multi-repository
flush/fork results across success, retry and partial failure.

**Integration scope:** Rework the dispatcher workspace-Git boundary together with
`GitTransport` lease handling, UID/filesystem ownership, temporary-index and object
transfer behavior, and the real host/guest integration fixture. The design must cover
local filters/hooks and config-selected helpers as well as network URL rewrites,
redirects and proxies. Keep required product files and filter-produced content usable;
do not “solve” the issue by silently dropping user changes or disabling all Git
features without defining the supported behavior.

**Success criteria:**

- For every supported snapshot/fetch/pack/push path, hostile repo-local filters,
  hooks, fsmonitor/signing helpers and config cannot run as the trusted dispatcher;
  tests observe actual process identity and filesystem effects.
- A temporary internal-Git lease reaches only its exact authorized origin/repository
  and operation. Repository rewrites, encoded-path tricks, redirects and proxy config
  cannot forward the secret externally; diagnostics redact the lease.
- Repository/provider descendants receive no host or dispatcher credential/capability.
- Normal flush, live fork and shutdown capture still preserve exact expected code and
  docs bytes/OIDs, working tree/index state, and retry behavior, including partial
  failures and cleanup of temporary state.
- A real dispatcher fixture exercises privileged and guest identities, with positive
  and negative security probes; unit-only command-construction tests are not sufficient.

**Design review focus:** privilege separation, complete mediation and secret lifetime.
Map every Git subprocess and filesystem handoff, identify which identity owns the
index/worktree/object stores at each stage, and show that every outbound transport
uses an authority that cannot be rewritten by the repository being processed.

**Governing ontology:**
[host Git isolation](../../../.g5/ontology/software_architecture/runtime.g5.md#host-git-never-trusts-retained-guest-control-files)
(`g5.runtime.materialization`),
[preserve working state during flush](../../../.g5/ontology/software_architecture/durability.g5.md#preserve-the-users-git-working-state-during-flush)
(`g5.git.durability.working-state`),
[complete flush receipt](../../../.g5/ontology/software_architecture/durability.g5.md#only-verified-complete-capture-advances-recovery)
(`g5.git.receipt`),
[restrict Git network effects](../../../.g5/ontology/software_architecture/durability.g5.md#restrict-network-ref-effects)
(`g5.git.network.ref-scope`), and
[guest root is not isolation](../../../.g5/ontology/concepts/security.g5.md#treat-guest-user-setup-as-compatibility)
(`g5.security.mvp-exposure.root`).

**PR size estimate:** 1,200–2,000 changed lines for the execution/transport boundary,
ownership and cleanup changes, and adversarial dispatcher integration coverage.

## 7. New feature — Fork & Start with a recoverable first agent turn

**Recorded gap:** Checked fork creates a stopped child workspace. Starting its compute
and creating a new agent chat are separate operations. The fork request has no durable
follow-on action that coordinates these steps through reloads, failures and retries.

**Goal:** Add **Fork & Start** alongside **Fork only**. The user chooses the child name,
enters an initial instruction, selects an agent through the existing launch controls,
and explicitly authorizes starting the child's compute. One submission creates the
workspace and starts one new chat for that instruction. Show durable progress and
retry/cancel controls that survive browser reload and API restart. Retries must preserve
the same work without duplicate children, chats or initial turns. A completed child
remains usable when launch fails; delayed work must respect current ownership, Stop,
cancellation and the existing consent lifetime. Scope this PR to a single initial text
instruction, the existing providers and fork modes, and the fork dialog/child workspace
UI; external issue integration is outside this experiment.

**Why this is challenging:** Fork publication, VM startup and input delivery have
different owners and completion boundaries. Git effects, SQL commits and runtime
commands cannot share one transaction. A crash after one boundary but before its
acknowledgement must neither repeat work nor lose the next step. Meanwhile, the owner
can withdraw consent, stop the child or transfer it. The feature needs to compose these
existing operations into a recoverable user journey.

**Product basis and scope:** This is the issue-less `after_fork` slice of the
[external issue design](../../external-issue-integration-design.md#forking-and-starting-work),
which explicitly allows starting a chat without an issue. Deliver a complete vertical
slice using the existing checked fork, VM lifecycle and agent transports. Keep tracker
adapters, issue publication, multi-task workflows and new runtime/provider backends out
of this PR. Selected source conversations may still be carried by the existing fork
option; the requested initial instruction belongs to a new chat in the child.

**Integration scope:**

| Boundary | Required composition |
|---|---|
| Fork / Git → durable follow-on | Admit the fork and requested follow-on together; include the normalized launch intent in retry identity. Preserve the existing complete repository vector and stopped-child publication boundary. Make follow-on work recoverably discoverable after publication. |
| Follow-on → Workspace lifecycle / placement | Start only the published child through the normal consent, capacity queue and VM admission owners. Preserve the originally accepted consent expiry across retries and API restarts. |
| Follow-on → ChatService / TaskService | Allocate the new session, logical task and initial input through their owning operations. Give the allocation a stable replay identity; dispatch only after the relevant commit and runtime admission. |
| Agent launch → configuration / provider account | Resolve the child's effective launch configuration and current owner's eligible account through existing services; preserve their confirmation and permission rules. |
| Lifecycle changes → pending follow-on | Fence stale work when consent expires, ownership changes, the child is stopped/deleted, or the follow-on is cancelled. A delayed worker cannot recreate authority. |
| Public API → browser | Persist the submitted retry identity and expose separate workspace-creation, compute and chat states. Reload and recovery use the same operation; a successful fork response alone cannot imply a running agent. |

Likely entry points include
[WorkspaceForkService](../../../backend/g5-api-core/src/services/workspace_fork_service.rs),
[WorkspaceVmService](../../../backend/g5-api-core/src/services/workspace_vm_service.rs),
the [Agents owners](../../../.g5/ontology/software_architecture/agents.g5.md#public-operations),
and [CloneDialog](../../../frontend/src/components/workspace/CloneDialog.tsx), plus a forward
migration, API contracts and reconciliation wiring. The coordinator owns the requested
sequence and its recovery; each existing service retains its domain state and decisions.

**Success criteria:**

1. **Complete user outcome.** From a source with code and Project Docs, the browser
   action creates a child with the correct member vector and starts the requested chat
   in that child. A deterministic agent writes observable child code/docs changes and
   produces the normal task/timeline result. Source files and branch heads retain their
   expected state. Fork only still creates a stopped child without launch consent.
2. **Replay the whole accepted request.** Repeated submissions, double clicks and
   recovery after a lost response resolve to the same child, follow-on, session, task
   and initial input. Within the same declared replay scope, changing instruction,
   launch selection or action conflicts. Define compatibility with existing fork
   receipts: a separately identified compound request may compose an unchanged receipt
   only with fresh explicit launch authority and current child authorization. Equal
   raw keys in distinct scopes alone do not prove replay; stored fork intent stays fixed.
3. **Recover between commits and effects.** Inject crashes after fork publication,
   after chat/task allocation and around VM-start or agent-invoke acknowledgement.
   Concurrent reconcilers converge on the original logical work. Existing command
   replay and uncertain-delivery handling remain authoritative; an unknown provider
   handoff cannot trigger blind submission of another initial turn.
4. **Honor bounded start consent.** Capacity shortages remain a visible queued state.
   The existing 15-minute consent lifetime survives process restart without being
   refreshed by a retry. Expired or withdrawn consent requires a fresh explicit owner
   action; a queued chat or pending follow-on cannot boot the VM by itself. Timely
   readiness consumes Start consent for that incarnation; its original deadline is
   not a separate deadline for chat allocation on still-authorized running compute.
5. **Recheck authority at delayed effects.** Source read permission can authorize an
   ordinary reader fork into that reader's own child, without borrowing the source
   owner's compute or credentials. Wrong-tenant callers, former owners, revoked child
   access and stale lifecycle generations cannot use recovery to start or send work.
   Provider selection continues through the normal current-owner launch boundary.
6. **Cancellation and lifecycle changes win durably.** Cancelling pending launch
   withdraws its unconsumed consent and prevents its undispatched input. Stop, deletion
   or transfer fences delayed follow-on callbacks. Once a turn has begun, use the
   ordinary task cancellation path and retain its evidence. Cancelling launch preserves
   the completed child and cannot cancel unrelated work subsequently created there.
7. **Keep creation and launch failures separate.** An aborted fork starts no child VM
   or agent. A failed launch leaves a completed fork and accessible child, with a useful
   error and recoverable action. Retrying launch never reacquires fork name claims,
   creates another child or repeats an already accepted initial input.
8. **Show truthful progress after reload.** Distinguish workspace creation, waiting for
   capacity, VM startup, input queued/delivery uncertain, agent running and action
   required using the owning records. Reloading the browser or restarting the API
   preserves the instruction, identities and correct recovery controls. Evaluate the
   composed journey, including existing VM and chat surfaces. A clearly labelled
   allocation handoff may complete before agent execution; it must not claim that the
   initial turn has run. These clarifications do not retroactively replace frozen ledgers;
   see the [evaluator adjudication](ontology-evaluator-adjudication.md).

**Design review focus:** Explain the operation/child/consent/session/task/input identity
mapping, which records commit together, and how each external effect is rediscovered
after a crash. Show the fork and follow-on state machines alongside the existing Start
and delivery lifecycles. Define cancellation races and the distinction between accepted
launch intent, an accepted initial input and actual provider delivery. Avoid holding a
database transaction open across Git capture, VM startup or provider invocation.

**Governing ontology and design:**

- [Fork replay](../../../.g5/ontology/features/workspaces.g5.md#freeze-one-recoverable-fork-operation)
  and [complete publication](../../../.g5/ontology/features/workspaces.g5.md#publish-all-members-or-recover-safely)
  (`g5.fork.replay`, `g5.fork.replay.terminal`, `g5.fork.proof`).
- [Separate setup from completed creation](../../../.g5/ontology/features/external_issues.g5.md#separate-creation-from-optional-external-setup)
  (`g5.external-issues.setup`) and the proposed
  [follow-on execution/recovery contract](../../external-issue-integration-design.md#execution-and-recovery).
- [Start consent](../../../.g5/ontology/software_architecture/lifecycle.g5.md#explicit-consent-owns-each-start),
  [compound Start-and-launch](../../../.g5/ontology/software_architecture/lifecycle.g5.md#make-compound-start-consent-explicit)
  and [Stop fencing](../../../.g5/ontology/software_architecture/lifecycle.g5.md#stop-fences-future-work-before-releasing-custody)
  (`g5.lifecycle.start`, `g5.lifecycle.start.compound`, `g5.lifecycle.stop`).
- [Allocation composition](../../../.g5/ontology/software_architecture/agents.g5.md#compose-allocation-through-the-owning-services)
  and [dispatch phases](../../../.g5/ontology/software_architecture/agents.g5.md#dispatch-transaction-phases)
  (`g5.architecture.allocation_composition`, `g5.architecture.dispatch_phases`).
- [Uncertain delivery](../../../.g5/ontology/features/conversations.g5.md#keep-uncertain-delivery-explicit)
  and [owner-authorized redelivery](../../../.g5/ontology/features/conversations.g5.md#require-an-owner-choice-for-uncertain-redelivery)
  (`g5.conversation.delivery.unknown`, `g5.conversation.delivery.redelivery`).

**Evaluation evidence:** Use real Postgres and Git, the public fork/recovery APIs,
controlled clock/failure points, and a browser journey that reaches the existing
dispatcher with a deterministic provider fixture. Assert stable identities, exact
repository bytes/OIDs, current owner/incarnation and observed invocation/input counts.
Exercise both supported provider adapters where launch composition differs. A browser
mock, inserted success rows or a returned chat ID alone does not establish completion.
Freeze these criteria for this experiment; the original appendix's eight-obligation
ledgers apply to experiments 1–3.

**PR size estimate:** **2,800–4,600 changed lines**, including meaningful integration
tests and excluding generated files, lockfiles and moved code. This is a sizing estimate;
the implementation should remain as small as the complete behavior permits.

| Work | Estimated changed lines |
|---|---:|
| Persistent follow-on state, migration and request/replay contracts | 350–600 |
| Recoverable coordination and integration with lifecycle/agent owners | 800–1,400 |
| API recovery/status surface and fork/child UI | 650–1,000 |
| Database, runtime and browser regression coverage | 1,000–1,600 |
