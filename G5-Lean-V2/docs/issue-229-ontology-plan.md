# Agent execution ownership

## Ontology changes and governance

**Ontology changes:** compared with origin/main at 412cccae, add
`g5.architecture.agent_execution_tree` in
[Agents](../.g5/ontology/software_architecture/agents.g5.md#execution-tree-ownership).
The exact ontology patch follows below.

**Relevant and governing ontology:**
- `g5.architecture.agent_execution_tree`: dispatcher-owned cleanup must include orphaned
  descendants; a sibling is never part of that ownership tree.
- `g5.architecture.provider_adapter`: adapter owns provider translation, dispatcher owns
  cancellation; do not move logical task outcomes into the process owner.
- `g5.design.cleanup-proof`: private reap receipt permits settlement; missing receipt
  retains capacity and reports cleanup uncertainty.
- `g5.architecture.dispatcher`: preserve journal and admission ordering.
- `g5.agents.lifecycle.cancel`: retain cancellation as the logical decision.

## Outcome and design

Launch Linux agent.turn through a private, single-threaded child-subreaper in the existing shared
process-termination module before
starting its supervisor. The owner inherits module I/O and holds a separate private
control/receipt socket that the supervisor and tools cannot inherit. Parent disconnect
also triggers cleanup. On cancellation, allow the supervisor up to two seconds to
interrupt the provider and harvest; then kill and reap all owner children, repeating
as orphan descendants are adopted. Enumerate direct children using the existing
PTY proc-stat parser; do not depend on optional /proc children support. Only waitpid ECHILD permits a cleanup receipt.
The dispatcher bounds the initial wait and keeps capacity quarantined if cleanup
cannot be proved, while continuing to wait for late proof.

Register supervisor SIGTERM handling before provider launch. On interruption stop the
provider, bound session harvest to one second, skip optional title work, and exit.
The external owner deadline also covers setup, blocked I/O and failed harvest.

No cgroup hierarchy is provisioned by the guest today. A private subreaper works
without delegation and survives the supervisor's failure and tools' setsid calls.
This is cooperative product lifecycle containment, not hostile-process security.
macOS keeps its development fallback and is excluded from Linux ownership claims.

## Acceptance and review

Exercise the real dispatcher with synthetic provider/tool descendants, including
setsid and ignored signals; cancel, pause/interrupt reasons, outer wall/output limits
and supervisor failure must reap recorded PIDs before outcome/capacity release.
A sibling must keep writing; the stopped tree must produce no late writes/events.
Run explicit Linux process tests. Native tests do not prove VM or live-provider behavior.

Design review: rechecked shared cleanup-proof and runtime custody references through
the system-invariants index; ownership must outlive the supervisor and remain distinct
from its task outcome. Failure to read /proc or reap is uncertainty, never success.
Private owner failure retains capacity. No source/API redesign outside this boundary.

## Ontology patch

```diff
diff --git a/.g5/ontology/software_architecture/agents.g5.md b/.g5/ontology/software_architecture/agents.g5.md
index 00e1090f..bb626600 100644
--- a/.g5/ontology/software_architecture/agents.g5.md
+++ b/.g5/ontology/software_architecture/agents.g5.md
@@ -80,0 +81,16 @@ The dispatcher owns admission/cancellation; TaskService decides logical completi
+### Execution tree ownership
+
+[D · g5.architecture.agent_execution_tree]
+
+The dispatcher owns each agent supervisor, provider and tool descendant through a
+private Linux child-subreaper. Cancellation, interruption, limits and supervisor
+exit start bounded cooperative cleanup followed by forced descendant reap, including
+separate process groups and sessions. Execution capacity remains held until the owner
+proves that no children remain; signalling, pipe closure or owner failure is not proof.
+Sibling executions retain independent owners. This is lifecycle containment, not a
+security boundary against malicious processes. macOS development does not provide the
+Linux descendant-reap guarantee.
+
+Preserves [cleanup proof](../concepts/system_design_principles.g5.md#prove-cleanup-before-releasing-resources)
+and the [provider adapter](#provider-protocol-adapter) boundary.
+
@@ -139,0 +156,6 @@ workspace owner using ProviderAccountService's eligibility decision.
+### g5.architecture.agent_execution_tree
+
+- agent
+- [Issue 229](https://github.com/G5Labs/G5-Lean/issues/229) and [implementation request](../../chat_intent/issue-229/request.md).
+- Implemented in `g5d-foundation::process_termination::execution_owner` and the dispatcher/agent adapter. Linux real-dispatcher synthetic-provider tests cover cancellation, limits, supervisor failure, sibling progress and reap before settlement. Live-provider/booted-VM qualification remains unexecuted; macOS retains the group-only fallback.
+
```
