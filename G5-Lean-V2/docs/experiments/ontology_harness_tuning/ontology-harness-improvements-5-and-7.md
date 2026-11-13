# Targeted ontology and harness improvements after experiments 5 and 7

## Decision

Apply a small revision now. [Experiment 5](ontology-experiment-5-history-audit.md) and
[experiment 7](ontology-experiment-7-history-audit.md) repeatedly show agents retrieving
correct rules but failing to validate their application at a reused boundary. More
experiments are not needed to identify these mechanisms. Whether the revision reliably
reduces failures and overhead remains unmeasured.

The change uses the existing design and review artifacts. It adds no workflow phase,
approval, required subagent, new skill or general-purpose checklist. Source-free planning
remains intact; implementation validates assumptions against code after authorization.

## Changes and evidence

| Repeated observation | Applied change | When it matters |
|---|---|---|
| Exp. 5 reused a deadline policy with incompatible loss authority; exp. 7 relied on Stop without checking its queued-state behavior. | Planning retains consequential reuse assumptions; the coding handoff requires inspection of actual policy/effects against the governing commitment before reliance. | First use of an existing operation during implementation. |
| Exp. 5 missed a terminal-state callback and broke a valid helper caller; exp. 7 hardened an incorrect source-owner requirement. | Review selects investigations by changed concern: reuse/guards, shared state/completion, or deferred effects. It searches existing writers, callers and consumers across modules, preserving a valid variant. | A shared rule, helper or state meaning changes. |
| Exp. 5 seeded release or tested behind an unrelated busy-host guard; exp. 7 directly changed the generation that public Stop was supposed to change. | Ask whether the check would fail without the relevant protection. A fixture may establish preconditions, but cannot substitute for the transition being claimed. Reassess changed assertions against intent. | Choosing checks, interpreting failures and closing a consequential claim. |
| Both runs confused shared resource state with operation purpose or effect scope. | Add `g5.data.identity.operation`; clarify policy reuse in `g5.architecture.policy` and operation-kind/accepted-intent binding in `g5.architecture.recovery`. | Composing, replaying, recovering or cancelling work on shared resources. |
| Exp. 5 treated host acknowledgement as provider completion; exp. 7 treated input allocation as execution progress. | Add `g5.data.outcome-evidence` at the data owner and link it from the existing fact-authority principle. | Persisting or displaying a composed outcome. |
| Exp. 7's queued Start survived public Stop. | Clarify the already-governing Stop fence for stopped compute with queued consent; record the current source gap in its footer. | Lifecycle Stop, including early returns. |
| Both reviews spent effort on broad metadata estimates despite unverified central behavior. | Assess relevant blocks, edit only changed assessments, and make percentages optional with an explicit obligation basis. Preserve known gaps and accurate existing entries. | Review reconciliation; no broad metadata rewrite is required. |

The canonical homes are [data and authority](../../../.g5/ontology/concepts/data_model.g5.md),
[design principles](../../../.g5/ontology/concepts/system_design_principles.g5.md),
[Stop](../../../.g5/ontology/software_architecture/lifecycle.g5.md#stop-fences-future-work-before-releasing-custody)
and [review metadata policy](../../../.g5/ontology/concepts/coding_principles.g5.md#update-reviewed-ontology-metadata).
The operational changes live in [AGENTS](../../../AGENTS.md), the maintained
[planning](../../../.agents/skills/ontology-planning/SKILL.md) and
[review](../../../.agents/skills/ontology-review/SKILL.md) skills, and the
[authoring format](../../../.agents/skills/ontology-authoring/references/format.md).

## Scope and overhead

The harness replaces broad review prose with concrete triggers and keeps evidence in
existing artifacts. The combined AGENTS, planning, review and format text decreases from
3,617 to 3,592 whitespace-delimited words against the working-tree version before this revision; ontology adds two compact
commitments and clarifies existing owners. These counts describe text size, not measured
agent effort. Accurate footers need no edit; no existing percentages were bulk-converted.

Pre-existing edits are preserved. Generated Codex, Claude and experiment-harness skill
copies follow the maintained `.agents/skills` source. Product code and frozen experiment
candidates are unchanged. The current Stop early return was source-inspected; application
behavior was not tested in this documentation change. Structural checks of links,
block/metadata pairing and generated copies cannot establish future agent reliability.
Those structural checks and the three affected skills' frontmatter validation passed.

No experiment-specific source paths, known defects or evaluator cases were placed in
the skills. Detailed fault injection, independent review and evaluator tooling remain
conditional tools, rather than mandatory overhead for every change.

## Next validation

Use **experiment 4** as the next held-out paired trial under the existing isolation and
minimal-supervision protocol. Freeze the revised harness/context and common evaluator
obligations before dispatch; keep these audits and their solutions out of worker input.
Inspect whether the worker resolves reuse assumptions, follows alternate writers and
uses checks that exercise the actual owner transition. Report unique remaining defects,
repairs and unexecuted outcomes separately, alongside wall time, output usage and document
work. An ontology/plain pair measures the bundle; it does not isolate this revision's
causal effect from ontology or task differences.

Use experiment 6 afterward if broader security-boundary evidence is needed. Neither
trial was launched as part of this revision. Replaying experiments 5 or 7 with these exact
findings would measure assisted repair rather than independent discovery.
