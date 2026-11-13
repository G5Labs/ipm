# Issue 233: agent execution lifecycle and effective limits

## Ontology changes and governance

**Ontology changes.** Comparison base: `origin/main` at `67d2dbcc`. Added `g5.agents.execution-limits`, `g5.agents.output-bounds`, and `g5.agents.execution-deadlines` in [task execution](ontology/features/agents.g5.md), and `g5.architecture.agent_lifecycle` in [Agents architecture](ontology/software_architecture/agents.g5.md). Review also updated the footers for `g5.agents.outcome` and `g5.conversation.delivery.unknown` without changing their governing text. The requested behavior is recorded in [issue 233 intent](chat_intent/issue-233/request.md). Actual ontology and metadata diff:

```diff
diff --git a/.g5/ontology/features/agents.g5.md b/.g5/ontology/features/agents.g5.md
index 12762e6d..2adcbe0b 100644
--- a/.g5/ontology/features/agents.g5.md
+++ b/.g5/ontology/features/agents.g5.md
@@ -226,0 +227,30 @@ Declare execution success only after terminal provider outcome, output closure a
+### Freeze and enforce effective execution limits
+
+[D · g5.agents.execution-limits]
+
+Resolve each supported limit from the tightest applicable bounds that measure the same
+resource, and freeze it for the attempt. Reject an unsupported requested limit before
+delivery; every accepted limit must govern the actual provider execution. An absent
+request cannot relax a catalog or policy bound. The catalog's serialized module-output
+cap remains an independent bound from the provider's raw-output limit.
+
+### Bound output before framing and parsing
+
+[D · g5.agents.output-bounds]
+
+Count raw provider stdout bytes before accumulating frames or parsing protocol
+content; bound stdout and stderr frames independently, with stderr also subject to
+its log-byte cap. On a limit, terminate execution through its owner, retain already
+accepted partial output and settle the attempt as limited.
+
+### Distinguish attempt and provider deadlines
+
+[D · g5.agents.execution-deadlines]
+
+The attempt-wide deadline starts before authentication and restore and covers initial
+input delivery, provider execution and harvesting. A terminal result or blocked write
+does not reset it. At expiry, execution cannot succeed; cleanup and any remaining
+harvest have bounded grace before settlement. V1 accepts no separate provider-only
+timeout; any future provider budget starts at launch, covers input, output and exit,
+and cannot exceed the remaining attempt budget.
+
@@ -396 +426 @@ Dependency state follows the [logical task lifecycle](agents.g5.md#separate-logi
-- ~90% · task_lifecycle cancellation; full cancellation/continue race matrix unexecuted.
+- ~90% · task_lifecycle cancellation and focused ambiguous-write settlement pass; full cancellation/continue race matrix unexecuted.
@@ -468 +498,19 @@ Dependency state follows the [logical task lifecycle](agents.g5.md#separate-logi
-- ~85% · native adapter settlement; complete late-output/process-exit matrix unexecuted.
+- Partial · `g5d-agent-turn/src/runtime.rs` requires a terminal result, output closure and successful process exit; active dispatcher regressions cover early exit, terminal-before-EOF timeout and harvest, malformed frames and Claude's max-turn result. The full late-output/process-exit matrix and live provider/VM path remain unexecuted.
+
+### g5.agents.execution-limits
+
+- agent
+- [Issue 233](https://github.com/G5Labs/G5-Lean/issues/233) requires explicit catalog/policy/request limits and rejection of unsupported limits.
+- Partial · `agent_dispatch.rs` rejects resolved Codex max-turn requests; `g5d-agent-turn` rejects Codex policy caps, applies a policy-only Claude turn cap and shares the raw stdout budget with the optional title call; `g5d-dispatch` applies the request/policy/catalog attempt deadline. Focused synthetic dispatcher tests pass. Live provider/VM behavior is unexecuted.
+
+### g5.agents.output-bounds
+
+- agent
+- [Issue 233](https://github.com/G5Labs/G5-Lean/issues/233) requires byte counting before unbounded allocation and bounded frames.
+- Implemented on the inspected supervisor path · `runtime.rs` counts raw stdout and stderr bytes before bounded line assembly, retains complete in-budget frames and bounds the optional title child; `task_lifecycle.rs` persists a distinct limit reason. Reader, active dispatcher and API settlement regressions pass; live provider/VM behavior is unexecuted.
+
+### g5.agents.execution-deadlines
+
+- agent
+- [Issue 233](https://github.com/G5Labs/G5-Lean/issues/233) asks to define attempt-wide and provider-only budgets across setup and cleanup.
+- Partial · `g5d-dispatch::modules::runner` starts the effective clock before Git refresh and passes its deadline into `g5d-agent-turn::runtime`; the supervisor holds a fixed success deadline while bounded cleanup/harvest has grace. Active blocked-input and terminal-before-EOF regressions reach timeout and harvest; the API settlement test distinguishes unknown and proven pre-write delivery. Native auth and stalled harvest paths remain unexecuted; Linux cleanup proof awaits Cloud CI.
diff --git a/.g5/ontology/features/conversations.g5.md b/.g5/ontology/features/conversations.g5.md
index 3c0e39ff..ff705152 100644
--- a/.g5/ontology/features/conversations.g5.md
+++ b/.g5/ontology/features/conversations.g5.md
@@ -480 +480 @@ Automatic first-request or best-effort summary naming uses the existing turn’s
-- ~85% · delivery settlement and input status; historical late-acceptance reconciliation remains incomplete.
+- Partial · supervisor and dispatcher record ambiguous initial writes; TaskService holds only still-reserved input as `delivery_unknown`, preserves accepted input, and keeps cancellation terminal with an uncertainty audit. Focused dispatcher and API settlement cover these paths; historical late-acceptance reconciliation remains incomplete.
diff --git a/.g5/ontology/software_architecture/agents.g5.md b/.g5/ontology/software_architecture/agents.g5.md
index 4c67a8bf..938f88bd 100644
--- a/.g5/ontology/software_architecture/agents.g5.md
+++ b/.g5/ontology/software_architecture/agents.g5.md
@@ -96,0 +97,15 @@ and the [provider adapter](#provider-protocol-adapter) boundary.
+### One production execution lifecycle
+
+[D · g5.architecture.agent_lifecycle]
+
+The active `agent.turn` supervisor owns provider launch, bounded input and output,
+deadlines, cancellation, event emission and process outcome under dispatcher execution
+tree custody. Production and its test seams use this lifecycle. Older runner entry
+points may only adapt compatible inputs and outputs; they cannot maintain independent
+launch or settlement behavior. Provider wire semantics remain in the adapters.
+
+Preserves [effective limits](../features/agents.g5.md#freeze-and-enforce-effective-execution-limits),
+[output bounds](../features/agents.g5.md#bound-output-before-framing-and-parsing),
+[deadlines](../features/agents.g5.md#distinguish-attempt-and-provider-deadlines),
+and [outcome evidence](../features/agents.g5.md#provider-output-is-not-the-final-execution-decision).
+
@@ -161,0 +177,6 @@ workspace owner using ProviderAccountService's eligibility decision.
+### g5.architecture.agent_lifecycle
+
+- agent
+- [Issue 233](https://github.com/G5Labs/G5-Lean/issues/233) calls for one authoritative production lifecycle and a narrow compatibility facade; [issue 229](https://github.com/G5Labs/G5-Lean/issues/229) already owns execution-tree containment.
+- Implemented on the inspected code path · `g5d-dispatch::modules::runner` and `g5d-agent-turn::runtime` own the production lifecycle and active synthetic regressions exercise it. The unused `g5d-worker::runner::Runner::run_turn` process lifecycle was removed; only provider wire adapters and session-file helpers remain there. Live provider/VM behavior is unexecuted.
+
```

**Relevant and governing ontology.**

| Commitment | Obligation, owner, allowed case and counterexample | Conclusion |
|---|---|---|
| [`g5.agents.execution-limits`](ontology/features/agents.g5.md) | Freeze the tightest bounds for the same resource; allow a tighter request, reject unsupported limits and keep the independent serialized module-output cap. | New governing behavior. |
| [`g5.agents.output-bounds`](ontology/features/agents.g5.md) | Count raw provider stdout bytes and bound stdout/stderr frames before allocation; a flood fails with partial output retained. | New governing behavior. |
| [`g5.agents.execution-deadlines`](ontology/features/agents.g5.md) | Attempt clock includes auth, restore, prompt delivery and harvest, with bounded teardown grace; v1 has no separate provider-only clock. A blocked write cannot reset the attempt. | New governing behavior; existing device-login subdeadlines remain ceilings. |
| [`g5.architecture.agent_lifecycle`](ontology/software_architecture/agents.g5.md) | `agent.turn` owns one lifecycle used by production and tests; the older runner cannot independently launch or settle. Provider protocol stays in adapters. | New governing ownership. |
| [`g5.architecture.agent_execution_tree`](ontology/software_architecture/agents.g5.md) | Dispatcher owns and proves cleanup of supervisor/provider/tools, including separate process groups; sibling ownership stays separate. | Reuse the issue 229 implementation and issue 168 primitives. |
| [`g5.agents.outcome`](ontology/features/agents.g5.md) | Success requires terminal result, stream closure and process exit. | A terminal frame before EOF is not success. |
| [`g5.conversation.delivery`](ontology/features/conversations.g5.md), [`g5.conversation.delivery.unknown`](ontology/features/conversations.g5.md) | Acceptance requires actual provider handoff; ambiguous write is not requeued as proven failure. | Bounded input must preserve delivery evidence. |
| [`g5.provider.device-login.deadline`](ontology/features/provider_accounts.g5.md), [`g5.conversation.bundle.harvest`](ontology/features/conversations.g5.md) | Auth substeps and session harvest are bounded; expiry cannot turn incomplete harvest into success. | Existing specific bounds remain applicable. |
| [`g5.design.cleanup-proof`](ontology/concepts/system_design_principles.g5.md) | Signaling, closed pipes and timeouts alone cannot release capacity. | Preserve positive descendant reap evidence. |

## Requested outcome and scope

Deliver two reviewable PRs as issue 233 requests. PR 1 fixes effective limits, bounded IO and deadlines through the active dispatcher and `agent.turn` boundary with focused regressions. PR 2 reduces the older Runner to a compatibility facade or removes unused operations after tracing callers and replay requirements. Issue 229 owns the already merged descendant containment and issue 234 owns provider protocol/capability adaptation. Preserve invocation identity and replay compatibility.

## Design

1. Trace the active invocation from frozen task request through dispatch catalog policy, supervisor and provider adapters. Produce one effective-limits decision per attempt. Resolve bounds that measure the same resource to the tightest supported value; distinguish unsupported from absent. Validate before prompt delivery and enforce at the lowest owner that observes the resource. Treat dispatcher catalog bounds as an outer backstop, not a substitute for a tighter turn bound.
2. Start the attempt deadline before setup, with explicit subordinate budgets for auth operations, provider execution, cleanup and harvest. Preserve a fixed absolute deadline across all awaits. V1 has no separate provider-only timeout; if added, it begins at child launch and cannot exceed remaining attempt time. A terminal result does not waive output closure or process exit. Limit-triggered cleanup reuses the established dispatcher execution owner; no new process-tree helper.
3. Read provider stdout/stderr in bounded chunks. Account raw stdout bytes before line/frame accumulation and apply a separate per-frame cap plus a stderr log-byte cap. The catalog cap on serialized module events remains independent. Keep provider-specific parsing and max-turn interpretation in their adapters, while the lifecycle owns the shared bytes, clocks, cancellation, events and outcome evidence. Preserve accepted partial output and accurate delivery status on failure.
4. Exercise the real supervisor via deterministic providers for tighter output and turn limits, oversized/malformed frames, blocked write, output flood, early exit, terminal-before-EOF, setup timeout and cancellation. Run the smallest changed-boundary regression first, then affected crate/process suites and architecture guards. The full Docker lane is reserved for cross-service or CI diagnosis.
5. In PR 2, trace every old Runner entry point and test seam. Migrate applicable callers to the shared lifecycle or delete unused code, leaving only compatibility translation where needed. Do not replace provider semantics with an artificially common wire format. Keep mechanical movement separate from behavior changes.

## Risks and open assumptions

- Source inspection must establish which declared fields the pinned Claude and Codex CLIs can enforce, especially `max_turns`, and which request shapes are already accepted. Reject any field without an enforcement route; do not silently ignore it.
- Source inspection must establish whether legacy Runner callers have behavior needed outside the active supervisor, and whether catalog limits are on raw bytes or emitted events. Reuse only operations whose actual policy and effects match the calling commitment.
- Device authorization has a five-minute displayed wait. Effective attempt deadlines may be shorter; the shorter bound wins without claiming prompt delivery.
- Linux descendant ownership is already specified. macOS local tests cannot prove the Linux process-tree invariant; required Cloud CI and focused Linux evidence distinguish the claim.

## Design review

The alternate route through [provider accounts](ontology/features/provider_accounts.g5.md), [delivery](ontology/features/conversations.g5.md), [outcome](ontology/features/agents.g5.md) and [cleanup proof](ontology/concepts/system_design_principles.g5.md) found no conflicting authority. The attempt deadline caps auth without changing its own substep ceilings; it does not turn pre-delivery failure into accepted input. The architecture boundary leaves native protocol details with adapters and process custody with the dispatcher. The two-PR split prevents mechanical deletion from obscuring changed limit behavior. The remaining assumptions concern implementation facts and will be resolved by source inspection before dependent code.

## Implementation amendment

Source inspection showed that the catalog `max_output_bytes` counts serialized module events, while the turn request and policy govern raw provider stdout. Their numbers cannot be intersected as though they measure identical bytes. The dispatcher retains its catalog channel bound and the supervisor enforces the tighter raw-output bound. The v1 input has only one `wall_clock_ms`, so it governs the attempt; no separate provider-only timeout is accepted. Cleanup can require bounded grace after the deadline while success remains forbidden. These findings refined the four added ontology blocks without changing the two-PR scope.

## Review amendment

Independent review exposed a policy-only Claude turn cap, the optional title child, a read that crosses the output cap after a complete frame, and a timed-out partial provider write. The accepted policy cap now reaches the Claude CLI; title stdout uses the remaining turn allowance; complete in-budget frames remain journaled; byte and frame limits keep a distinct attempt reason. The dispatcher passes its absolute attempt deadline to the supervisor and reserves bounded settlement grace, while an ambiguous provider write carries `delivery_unknown` to TaskService. These repairs implement the existing limits, deadlines and delivery commitments rather than changing their intent. The older Runner lifecycle remains assigned to PR 2.

The second review found that missing final module output cannot override a durably accepted input, an interrupted dispatcher invocation write can be ambiguous, and owner cancellation must remain terminal even then. The dispatcher now reports initial-write ambiguity, while TaskService uses the persisted reservation to choose the hold and records a cancellation audit before withdrawal. These are repairs under existing delivery and cancellation commitments.

## Mechanical consolidation resolution

A workspace-wide Rust caller trace found no production call to `Runner::run_turn`, its `ProcessLauncher` seam, liveness policy, or process outcome types. Their callers were tests inside the legacy module. The active supervisor uses `claude` and `codex` frame mappers, checked Claude stdin construction, event types, and session-file helpers. `TurnSpec` and the old argv builders have only test callers; the active supervisor builds argv directly. PR 2 deletes those unused builders and the launch/settlement/process path, retaining the active adapters without changing their protocol semantics. Focused mapper/stdin and active dispatcher tests check the retained seam.
