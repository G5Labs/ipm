# Backend design v7 review — replacement draft

Reviewed 2026-09-16 in `/Users/zliu/Workspace/G5-Lean-wt-v7`, branch `lean/v7-design`, commit `abcd9082cad2f4def2d37d220aac998cd0024cda`.

This review covers the replacement [v7 design](/Users/zliu/Workspace/G5-Lean-wt-v7/docs/lean-backend-design-v7.md), its [implementation plan](/Users/zliu/Workspace/G5-Lean-wt-v7/docs/lean-implementation-plan-v7.md), the three normative API notes, and the existing provider runner that the plan carries forward. This is a design review; the proposed VM lifecycle and Agents implementation are not present yet. The design and implementation plan were not edited.

**Recommendation: address the three P1 findings before accepting this draft.** Five of the six findings from the earlier v7 review are addressed at the contract level; live-flush quiescence is partially addressed. There are five actionable findings below: three P1 and two P2.

## Findings

### R1 — [P1] Separate module launch confirmation from provider input acceptance

**Location:** [design §8.1–8.2](/Users/zliu/Workspace/G5-Lean-wt-v7/docs/lean-backend-design-v7.md:652); [plan P4.6, inbox delivery](/Users/zliu/Workspace/G5-Lean-wt-v7/docs/lean-implementation-plan-v7.md:884).

The API marks the reserved message `delivered` on the module's `started` disposition. The next step has the module restore session files and launch the provider; §8.3 additionally requires writing and closing its stdin. The generic dispatcher confirms that a module execution exists, not that the provider received its input. If session restore, provider launch or stdin delivery fails after that confirmation, a message the provider never received has already become immutable delivered transcript. The specified `delivery_unknown` recovery does not select it, because the start was confirmed.

The adopted [Agents contract explicitly distinguishes these facts](/Users/zliu/Workspace/G5-Lean-wt-v7/docs/agent-api-surface.md:390): `input.accepted` carries the reservation identity, revision and delivery generation, and CLI acceptance requires the frozen prompt's write/close boundary. The v7 event union already includes this event, but the inbox transition does not use it. Making generic `modules.invoke` wait for provider acceptance would also move Agents-specific knowledge into the dispatcher and alter the meaning of start confirmation for other modules.

**Change:** let invocation dispositions update execution start state only. Have the adapter journal `input.accepted` after the documented CLI acceptance boundary, and let Agents consume that event to transition the matching reservation to `delivered`. Define recovery separately for definitely unsent input and ambiguous delivery; a missing terminal outcome alone is not evidence that a provider did not receive or act on input.

**Validation to add:** confirm module launch, then fail session restore, provider spawn and stdin write independently. None may leave a never-sent message `delivered`. Also exercise loss after the provider acceptance boundary but before its acknowledgement, including event replay and session reconciliation. The §8.7 no-loss/no-duplication gate needs these distinctions.

### R2 — [P1] Make question suspension an explicit runner operation

**Location:** [design §8.4](/Users/zliu/Workspace/G5-Lean-wt-v7/docs/lean-backend-design-v7.md:723), [reconciler rule](/Users/zliu/Workspace/G5-Lean-wt-v7/docs/lean-backend-design-v7.md:625); [plan P4.11](/Users/zliu/Workspace/G5-Lean-wt-v7/docs/lean-implementation-plan-v7.md:1121).

The runner-wait deadline returns an MCP tool error and assumes that this ends the provider turn with `suspended`. An MCP error is a tool result, not a process-control signal; the protocol permits the model to correct or retry after seeing it. See the [MCP tool error contract](https://modelcontextprotocol.io/specification/2025-11-25/server/tools#error-handling). The current runner being retained has the same distinction: [tool completion errors do not determine its terminal status](/Users/zliu/Workspace/G5-Lean-wt-v7/backend/g5d-worker/src/runner/mod.rs:727), and its [Codex mapping fixture permits an MCP failure followed by a successful turn result](/Users/zliu/Workspace/G5-Lean-wt-v7/backend/g5d-worker/src/runner/codex.rs:534).

Consequently, the CLI can keep running, retry the question or finish normally after the timeout. In the last case a later answer cannot create a successor, because §7.2 correctly excludes `succeeded` and ordinary `failed` outcomes. The reconciler explicitly leaves this deadline to the MCP server, so no other component repairs the missing suspension decision.

**Change:** define an explicit deadline-to-runner suspension path. Persist the blocker and suspension decision, notify the supervising adapter, stop and reap the provider with a bounded signal sequence, harvest its session, and emit exactly one C3 `suspended` outcome. Serialize that decision with answer consumption and cancellation; an ordinary tool error must not be the authority that ends the execution.

**Validation to add:** use a provider fixture that receives the tool error and deliberately keeps running or emits a normal successful result. The deadline must still produce the selected suspended outcome, reap the process and leave the question answerable. Race answer consumption and cancellation against this transition and verify the appropriate single outcome and successor behavior.

### R3 — [P1] Drain file mutations as well as modules before a live flush

**Location:** [design §5.4](/Users/zliu/Workspace/G5-Lean-wt-v7/docs/lean-backend-design-v7.md:276); [plan P4.3](/Users/zliu/Workspace/G5-Lean-wt-v7/docs/lean-implementation-plan-v7.md:649).

The new live-flush rule closes module admission and waits for bounded runs. It does not close or drain `fs.write`, `fs.delete` or `fs.rename`. In particular, a Save that passed its `workspace.changes get` association check just before module admission closed can still be in flight when snapshotting begins. The shutdown path explicitly drains these mutations; the live path does not invoke that barrier. No terminal or preview is needed, so `concurrent_writers` can be empty while the snapshot is inconsistent.

**Observed:** a temporary repository running real `git add -A` against a temporary index was paused at a controlled file-read boundary. Two concurrent writes changed the worktree through `(before, before) → (after, before) → (after, after)`. Git captured `(before, after)`, which was never a worktree state. A clean-filter barrier controlled the interleaving; this demonstrates Git's lack of a snapshot boundary, not a production dispatcher test.

**Change:** use the existing private admission barrier for all file mutations during live flushes, wait for already-admitted mutations to finish, and hold it through snapshot capture. Apply this to every member repository and reopen admission on every completion/refusal/failure path. The explicit terminal/preview warning policy can remain as designed; it does not cover editor mutations.

**Validation to add:** pause an already-admitted Save after its association check, begin a fork, and release the write while the flush is trying to proceed. The fork must wait and capture a consistent cut. Cover delete/rename, multiple repositories, and reopening admission after a failed flush.

### R4 — [P2] Allow cleanup of the incarnation after releasing its authority

**Location:** [design stop-sequence release](/Users/zliu/Workspace/G5-Lean-wt-v7/docs/lean-backend-design-v7.md:358), [host command fence](/Users/zliu/Workspace/G5-Lean-wt-v7/docs/lean-backend-design-v7.md:396); [plan P4.5](/Users/zliu/Workspace/G5-Lean-wt-v7/docs/lean-implementation-plan-v7.md:787).

Release clears `vm_id` and makes the old incarnation stale, then sends `vm.stop graceful`. The host contract says every VM-scoped command for a non-current incarnation returns `stale` and is never acted on. A heartbeat can deliver the updated current-incarnation set between those two steps, so a correctly fenced host refuses the very command intended to free the slot. The same ordering occurs if the lifecycle service crashes after release and later retries the stop.

This is bounded by the fallback mechanisms: a subsequent stale lease renewal, lease expiry or reconnect can kill the old VM. It is not an indefinite resource leak. It does, however, turn a normal stop into delayed fallback cleanup and can make an immediate restart on the same host fail with `workspace_busy`, despite the API already reporting the workspace stopped.

**Change:** specify authenticated, idempotent teardown of the exact host-owned `vm_id` after its runtime authority has been revoked, with a durable retryable stop intent, or retain a lifecycle state that permits teardown until exit is acknowledged. This exception must never let an old stop affect a newer incarnation or restore the old VM's Git/proxy authority.

**Validation to add:** release → heartbeat acknowledgement excluding the VM → `vm.stop`; separately crash after release and retry after recovery. The exact old VM must stop promptly, release its slot, and preserve the retained-root policy; a replacement must remain untouched.

### R5 — [P2] Verify warm-root contents, not just HEAD and dirty paths

**Location:** [design §5.5](/Users/zliu/Workspace/G5-Lean-wt-v7/docs/lean-backend-design-v7.md:298); [plan P4.3](/Users/zliu/Workspace/G5-Lean-wt-v7/docs/lean-implementation-plan-v7.md:665).

The warm-root optimization proposes `HEAD` plus `git status --porcelain` and the snapshot's dirty set as proof that the worktree matches the receipt. Those values do not identify file contents. Flush a modified `a.txt`, modify that same file again without committing, and lose the VM: both versions have the same `HEAD` and ` M a.txt` status, so the stated fast-path check accepts a warm root that differs from the receipt. This violates the explicit policy that unflushed changes after an ungraceful loss are discarded and makes warm and cold restores disagree.

**Observed:** a real Git probe captured `a.txt = "saved"`, then replaced it with equal-length `"later"`. HEAD and porcelain status were identical; the snapshot and working-tree contents differed.

**Change:** compare a content-derived tree against `snapshot_oid` under the same capture rules, or always materialize the receipt on warm attach. A dirty-path comparison can reject a mismatch cheaply, but cannot prove equality.

**Validation to add:** keep HEAD, dirty paths and file sizes identical while changing the contents after the last receipt; simulate loss and warm attach. It must restore exactly the same content as a cold attach from that receipt.

## Disposition of the previous findings

These are contract-level conclusions, not claims that the proposed implementation has passed integration tests.

| Earlier v7 finding | Disposition in this replacement draft |
|---|---|
| V7-1: restore bypasses completion through mutable snapshot refs | Addressed: immutable per-flush refs, verified multi-repository completion, and `last_flush_id` select restore, stopped reads, fork and projections (§5.4–5.5, §9.4). R5 concerns the separate warm-root equality check. |
| V7-2: live snapshots race native writers | Partially addressed: bounded runs are waited for or interrupted, and terminals/previews are disclosed. R3 covers the remaining file-mutation path. |
| V7-3: the host lease can outlive the control-plane grant | Addressed: the host uses request-departure time, ignores obsolete renewal replies and bounds initial boot authority by the command deadline (§6.4). |
| V7-4: pruning the journal removes cancellation protection | Addressed: cancellation and finished dispositions live for the incarnation independently of journal pruning (§7.5). |
| V7-5: Save checks a stopped VM before waking it | Addressed: ensure precedes association lookup, and lookup/write bind to the same incarnation (§9.2). |
| V7-6: independent Postgres and Git backups can be inconsistent | Addressed: one backup barrier and a paired restore manifest are required (§16.3–16.4). |

The earlier v6 G1–G4 corrections also remain explicit: lost incarnations settle unconfirmed starts, cancellation is durable, successor eligibility excludes cancellation, and deduplication/watermarks include the attempt. G5 now has stronger evidence than target equality: the target update and merge receipt are one Git ref transaction (§10.6). A real Git probe verified that the receipt continues to identify the applied result after the target advances again.

## Validation scope

I traced the written lifecycle, dispatch, inbox, question and restore paths against their authoritative state owners and checked the corresponding implementation prompts. Executed checks were three isolated Git probes (live snapshot interleaving, warm-root equality and atomic merge receipt) and small literal state-transition models for release-before-stop and delivery-before-provider-launch. Temporary probe repositories were removed automatically.

The question-suspension finding is supported by the retained runner's source, its existing mapping fixture and the MCP specification; a live provider experiment was not run. Backend integration tests were not run because this review makes no implementation changes and the proposed lifecycle is not implemented. The regression cases above are acceptance requirements for that implementation, not reported passing tests.
