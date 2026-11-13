# Human intent: shared native agent memory

Source: this conversation, 2026-10-01. The user first requested a concise design
in `docs/experiments/agent_memory`, then requested the minimal implementation plan
and design, then authorized implementation: “ok go ahead and implement the feature”.

## Human direction

- Start very simply by reusing the agent's existing memory mechanism, with no
  additional harness. The native agent decides how and when to read and save memory.
- Populate memory during microVM setup, alongside the existing agent files, and
  checkpoint changed memory into the actual codebase's Git so later work can reuse it.
- Use the real native memory paths, filenames and directory structures. The supplied
  `codex_mem.md` and `claude_mem.md` names were examples only.
- Use the existing merge/rebase operation to combine workspace changes. Acknowledge
  that Git does not provide semantic consolidation and leave that problem unaddressed.
- Keep Codex and Claude memory separate initially. Cross-provider consistency and
  additional pruning for multi-user growth are deferred; initially rely on native behavior.
- Document Codex's delayed/background capture and G5 execution-mode compatibility
  as a gap rather than adding a custom capture process.
- For multiple code repositories: “Use the agent’s launch repository when
  identifiable; skip shared memory when ambiguous (Recommended)”.

## Wording clarifications

- Describe repository selection plainly: memory belongs to the code repository
  where the agent starts. The earlier phrase “workspace members” referred to
  repositories and was confusing.
- Whether and when a lesson is saved depends on the native agent's behavior,
  which is outside G5's control.
- State behavior and current gaps directly in the relevant ontology blocks.
  Avoid release or phase framing such as “V1 adds”, “phase 0” or “MVP did”.
  Add this rule concisely to the ontology authoring skill.

## Review request

The user explicitly accepts “Codex shared memory is unsupported” as an ontology gap
and requests code review and E2E tests to check this implementation. The review found
a separate risk that a repository writer can remain active when Stop captures Git.

The user then asked what “cancellation” and a surviving native shell writer mean,
whether that behavior is necessarily a bug, whether this feature introduced it, and
what the ontology says. This raised an unresolved lifetime/ownership decision for
ordinary detached agent-launched processes; the Stop/fork capture rule remains in
force.

The user then asked how a surviving writer relates to memory loss at Git capture.
The causal risk requires a write after Stop's snapshot and before VM shutdown;
survival alone does not show a lost write. A later change after a live fork snapshot
may be expected to remain only in the source workspace.

The user then observed that a delayed memory write would require a still-running
writer authorized to perform it, and asked whether that is only an edge case.
The isolated review probe deliberately launched a Bash background writer; it did
not observe Claude's native auto-memory making a post-cancellation save.

The user directed recording this as an edge-case gap in the relevant ontology
metadata, without a process-lifetime repair now, and requested a seeded dev
workspace for manual memory verification.

## Agent interpretation and design decisions

- Treat shared lessons as repository content under the existing Git durability
  boundaries. Making a native write visible in the working tree does not create a
  new automatic commit or establish crash durability.
- Prefer a writable native mapping to the repository memory tree so bootstrap or
  teardown copies cannot overwrite newer memory. Exact provider compatibility is
  an implementation check, not established by this plan.
- Keep native credentials, raw resume files and runtime bookkeeping outside Git;
  checkpoint only the verified portable memory content. This preserves existing
  private-session and Git-exclusion contracts.
- Keep work inside the current Agents and Git/lifecycle owners, without a new
  service, database schema, UI or memory-maintenance job.

## Codex memory decision

The user proposed enabling native Codex memory and copying `$CODEX_HOME/memories`
through repository Git at VM startup/shutdown, and requested isolated local exploration.
After the [exploration findings](../../../docs/experiments/agent_memory/design.md#codex-memory-evidence),
the user accepted keeping Codex native memory disabled and directed: “let's have a
concise D block in the ontology for this and the reason, don't have a code gap
meta-data now. This is not a gap anymore”. This supersedes the earlier gap
classification; it requests no runtime change.

## Implementation findings

- Claude Code's native directory setting supports a direct repository binding.
- Codex `exec` sessions are excluded from automatic extraction, and process exit
  can interrupt background consolidation. The [Codex decision](../../ontology/features/agents.g5.md#keep-codex-native-memory-disabled)
  keeps native memory disabled.
- Review found an existing native process-drain defect. The implementation keeps
  the requested minimal scope and documents interrupted-capture risk; broader worker
  process containment has not been implemented or accepted as completed.

The [implementation design](../../../docs/experiments/agent_memory/design.md)
records the scoped ontology diff, acceptance outcomes and remaining assumptions.

## Ontology concision

The user requires agent memory to live in `features/agents.g5.md` as two or three
concise blocks, including a D block for its storage path. The standalone feature
page is removed. `g5.memory.native` is folded into `g5.story.reuse-native-memory`;
`g5.memory.persistence` and `g5.architecture.native_memory` are folded into
`g5.memory.repository`. Existing review records retain their historical IDs.

## Minimal merge-readiness repairs

The user approves allowing ignored memory already tracked in HEAD and sharing
provider tool-selection validation between API admission and the adapter. Unsupported
saved selections fail admission explicitly; exact MCP selection remains unsupported,
and restrictions must never be dropped or widened.

Agent repair decision: retain pending input on policy rejection without creating a
follow-up task, because the existing allocator copies the same unsupported policy.
