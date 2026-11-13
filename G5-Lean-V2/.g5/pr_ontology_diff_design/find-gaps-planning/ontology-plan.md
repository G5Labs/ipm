# Find gaps plans before it asks

Source: [chat intent](../../chat_intent/find-gaps-planning/request.md) (2026-11-03).
Governing intent: [Find gaps](../../ontology/features/ui.g5.md#g5.workbench.find-gaps) (new),
[chat actions](../../ontology/features/ui.g5.md#g5.workbench.chat-actions),
[workbench configuration](../../ontology/features/ui.g5.md#g5.ui.workbench-config).

## Design

Find gaps used to read the requirements and list open decisions without designing anything,
so it missed the gaps that only show when requirements are turned into commitments; ontology
planning ran only later, in System Engineering's Create plan.

- **Owner of the behaviour: the built-in prompts.** Find gaps' prompt now runs the
  repository's ontology-planning skill first, then records as questions every decision the
  documents or the plan leave open and every contradiction between the documents.
  No platform mechanism changes: the action still seeds the composer and pulses Send
  ([chat actions](../../ontology/features/ui.g5.md#g5.workbench.chat-actions)), and the
  Guided step's evidence is still `open-questions.md`.
- **One plan, two writers in sequence.** Find gaps and Create plan use the same
  `.g5/pr_ontology_diff_design/<workspace>/ontology-plan.md`; each continues it if present.
  Create plan reads the human-answered questions with the requirements, so their decisions
  are settled; its prompt otherwise keeps main's wording and relies on the skill and the
  shared path to continue the plan. A rerun of Find gaps extends the
  question sets and leaves answered questions alone. Considered and rejected: a throwaway
  planning pass inside Find gaps (simpler, but Create plan would redo the work).
- **Amendment limit.** Find gaps amends the ontology only where requirements or decided
  answers settle intent (owner decision 1) and keeps the rest open assumptions. Where the
  requirement documents disagree it writes neither side until the question is answered: a
  live retest (2026-11-04) committed one side of an open contradiction, which then made
  Create plan reluctant to reverse Business Engineering's statement. It still
  changes no requirements or code. The prompt relies on the skill for reading the governing
  blocks and continuing an existing plan, but names the plan path so both writers use the
  same file.

## Changes

- `backend/g5-api-core/src/services/agents/workbench_builtins/actions.yaml`: the
  `open-questions` and `create-implementation-plan` prompts; the client fallback
  `frontend/src/lib/workbenchBuiltins.generated.ts` regenerated from it.
- Ontology: new `g5.workbench.find-gaps` in `features/ui.g5.md` with its metadata.

## Risks and assumptions

- Find gaps becomes slower and costs more tokens: it now does a full planning pass.
- BE chats now edit the ontology, so a Find gaps run can end with an ontology changes card.
- A workspace whose committed `.g5/workbench/actions.yaml` overrides the `open-questions`
  prompt keeps its own prompt (inheritance fills omitted fields only).
- The plan does not link its open assumptions to their questions (owner choice,
  2026-11-03), so Create plan matches decided answers to assumptions by their content.
- Both writers are sequential chats by the same owner; concurrent Find gaps and Create plan
  runs on one plan are not coordinated beyond Git (the ordinary shared-checkout rule).

## Acceptance

- The generated-fallback test and the workbench chat API test pass.
- Live: in a workspace with a requirement document, Find gaps produces a plan whose open
  assumptions have questions in `open-questions.md`; a later Create plan continues that plan.
  Passed in full-stack preview runs on 2026-11-03 and, with two contradicting documents,
  2026-11-04 (both sides stayed out of the ontology; Create plan applied the owner's
  `decision: true` answer). Screenshots: Project Docs `find-gaps-ui-screenshots.html`.
  Those runs used Opus 4.8, the preview's default when the browser holds no model choice;
  in 2 of 3 the new statements omitted their metadata entry. A rerun on Opus 5.5 (1M)
  (2026-11-04) wrote every metadata entry and kept both sides out as well.
