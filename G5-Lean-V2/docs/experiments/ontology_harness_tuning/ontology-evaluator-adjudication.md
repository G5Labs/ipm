# Experiment 7 evaluator adjudication

## Decisions before further experiments

The user authorized resolving the evaluator ambiguities, making minimal necessary
corrections, and committing/pushing that checkpoint before further exploration. The
parent and two independent GPT-6-Astra reviewers inspected contracts, their source
lineage, frozen tests and composed UI paths. No new experimental worker received these
findings, and no candidate implementation was changed for this adjudication.

This document supersedes the disputed interpretations in the first and second
Experiment 7 comparisons. Original requests, ledgers, test sources, execution receipts
and historical scores remain preserved. Their preservation does not endorse a judgment
shown to be invalid. No replacement overall quality score is assigned.

| Disputed judgment | Resolution | Consequence for existing evidence |
|---|---|---|
| Timely Start cannot be followed by initial-chat allocation after its original deadline | **Withdraw the defect judgment.** Pending consent expires before attachment; valid readiness consumes it for the admitted incarnation. Current owner, generation, Stop/cancellation and runtime admission still govern later effects. | The second-only negative assertion adds an unrequested chat deadline. Ontology's allocation is not an E704 violation for this reason; plain's refusal is not evidence of superior consent enforcement. |
| Any new Fork & Start request reusing a Fork-only raw key must leave the old child stopped | **Characterization requiring a declared identity contract.** Changed action under the same logical replay scope conflicts. A separately identified, explicitly authorized compound request can compose an immutable completed fork when compatibility defines that behavior. | The probe submits a new explicit command; raw-key equality alone does not establish unauthorized replay. Do not award a pass automatically: unspecified adoption/namespace behavior remains a design/evidence gap. |
| Terminal coordinator completion at input allocation proves missing or false end-to-end progress | **Withdraw the blanket failure classification.** Assess what the user can see and do across the composed workspace and chat journey. Clearly labelled handoff completion is legitimate. | Existing VM and chat components expose relevant facts. Banner inspection alone cannot establish their absence. Browser reload/navigation and all failure states remain incompletely exercised. |

## Start consent: authoritative scope

`g5.lifecycle.start` derives from C063–C064. The VM companion's
[lifecycle admission contract](../../workspace-vm-api-v9.md#lifecycle-admission-start-intent-and-stop-durability)
states that consent expires before attachment and is consumed exactly once at
`runtime.ready`. The backend design §5.1 (line 273 at this checkpoint) explicitly says a
consumed intent authorizes its admitted incarnation, not a later VM; §5.8 checks pending
consent at readiness and requires current launch authority.

The additional second-run test (local run record `runs/2026-09-28-experiment7-second-run/consent-discriminator-ontology-final.rs`)
establishes timely consumed readiness on a current running VM, ages the original
deadline, then asserts no chat may be allocated. Its observed allocation is real; its
expected refusal is unsupported by that contract. Rejecting expired unconsumed demand,
preventing retry-based extensions and fencing replacement incarnations remain valid
requirements. Plain's separately observed extension of the original VM Start deadline
is unaffected by this correction.

The first-run ontology's extra current-time chat check is not a correctness reference
merely because it rejects more work. It may overrestrict an allowed continuation; its
behavior must be assessed against the same consumed-consent contract. A separate
end-to-end chat deadline would be a new product decision, not a recovered requirement.

**Minimal ontology clarification:** restore the attachment boundary and consumed
incarnation scope in `g5.lifecycle.start`, with the source in its metadata. No new
workflow phase or implementation check is imposed.

## Replay: identity, immutable receipt and fresh authority

`g5.fork.replay` binds requester, source and original request digest.
`g5.fork.replay.terminal` returns the historical fork outcome without new fork work.
Backend design §5.8 scopes checked-fork keys to organization/requester and rejects
changed fork input. Those facts do not prescribe one global raw-key namespace for
every future command.

The old-receipt probe (local run record `runs/2026-09-28-experiment7-second-run/common-probes-ontology-final.rs`)
first creates a Fork-only receipt, then submits a new explicit Fork & Start command
with a full initial instruction and launch selection. It checks that the original child
never gains Start consent. This conflates two possible contracts:

- Shared logical request identity: changing the accepted action must conflict.
- Separate compound identity: composing the unchanged fork can be valid when its
  receipt linkage, fresh Start consent and current child authority are explicit.

Neither namespace separation nor a new database row alone proves safe composition.
The reviewer must establish immutable fork input, current owner, distinct accepted
compound intent, stable retry mapping and absence of effects from Fork-only replay.
Silently adopting arbitrary existing children is not justified. The frozen universal
assertion cannot decide these conditions by itself.

The operation-kind wording introduced after first7 was derived from the audit itself;
it is not independent evidence validating that audit. Clarifying “within its declared
scope” preserves the general rule without imposing a universal key namespace.

## Progress: composed facts and remaining evidence

Both second-run desktop workspace pages mount the existing `WorkspaceVmStatusStrip`,
which exposes capacity/starting/running/failed states and lifecycle controls. Their
existing `ChatPanel` polls persisted session inputs and displays queued, sending and
delivery-uncertain states with the appropriate recovery actions; task/session state
also supplies agent progress. The new banners provide navigation to the allocated
chat. Thus a compound-operation record need not duplicate every downstream state.

Frozen source anchors:

- Ontology `6032975`: `frontend/src/pages/WorkspacePage.tsx:10089`,
  `frontend/src/components/workspace/ForkStartProgress.tsx:17`,
  `frontend/src/components/chat/ChatPanel.tsx:6290`.
- Plain `544a734`: `frontend/src/components/workspace/ForkStartBanner.tsx:130`
  composes with the same VM/chat surfaces in `WorkspacePage`.
- First7 ontology `a1d56fe` also mounts the VM strip and exposes Open chat through
  `ForkStartChildStatus`; first7 plain `726628d` does so through `ForkStartProgress`.

These are source-supported capabilities, not newly executed browser evidence. “First
chat is ready” can accurately describe allocation; it does not assert a delivered or
completed first turn. A bare `completed` label in first7 is less clear, but the enum
name alone still does not prove that all progress facts are absent. Verify reachable
navigation, current facts, error recovery and reload in the actual supported layout
before assigning a user-visible failure or claiming complete support.

The independent UI audit also found narrower concerns: plain's source-page reload
requires reopening the dialog to discover saved work; ontology's new banner is absent
from its early mobile layout return; and neither child banner exposes the instruction
before allocation, although the durable request retains it. These are discoverability
or scope-specific source concerns, not proof that persisted instruction or all progress
is lost. The audit covered final frozen revisions; it does not automatically reclassify
initial snapshots or validate every layout.

## What remains valid, and how future evaluation changes

- Unrelated-work cancellation remains a demonstrated violation: a separate ordinary
  chat moves onto owner-stop hold after compound cancellation.
- Public Stop's first-run queued-consent gap, its second-run repair, source-reader
  authorization behavior, stable same-action replay and original VM deadline behavior
  remain supported within their executed scopes.
- Real guest/provider outcomes, complete browser journeys and the full crash/race
  matrix remain unexecuted. Unknown evidence is not a demonstrated code defect.
- Report unique defects, supported behaviors, ambiguous decisions, unexecuted boundaries
  and effort separately. Historical broad-row scores should not decide the winner.
- Future probes must exercise both refusal and the nearby authorized continuation;
  evaluator expectations require the same contract review as implementation decisions.

The candidate brief and evaluation guide now carry these semantic corrections for new
trials. No old test receipt, frozen candidate or original worker result is rewritten.
The maintained planning/review skills receive no additional procedure at this checkpoint.
