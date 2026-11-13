# Guided journey — the guided panel ported onto the workbench

Source: the G5 workspace `Davide-guided` (2026-08-31 → 2026-09-08): `requirement.md`, mocks
A → L and `guided_panel_implementation_plan.md` with its four answered questions, recorded
verbatim in the [chat intent](../../chat_intent/guided-journey/request.md) together with what
the meetings and the UXv2 mock session decided afterwards. That workspace produced a design,
not code; this document is the design for G5 Lean and the record of its implementation.

**Status:** reviewed (two independent Codex rounds, see [Review summary](#review-summary));
implemented in this PR; one presentation choice open for the human (timeline G versus
flowchart K/L, see [Open decision](#open-decision-for-the-human)). The 2026-10-07 standup
asks Tim and Davide to review every UX change against its mock before it lands; the mocks in
[`mocks/`](mocks/) and the real-browser screenshots in [`screenshots/`](screenshots/) (Playwright
over the app in mock-data mode: the whole workspace, and the SE roster popover open) are that
review's material.

## Ontology changes

| Block | Change | Why |
|---|---|---|
| `g5.story.follow-the-guided-journey` (new I, workbench) | A workspace owner new to G5 follows the phases as a vertical timeline with the next step, its button, the produced documents and the pending approvals. | The requirement.md ask and the Jada persona. |
| `g5.workbench.journey` (new D) | The Guided sidebar section under the alignment layout: timeline of the three phases, active phase with all steps expanded and buttons rendered and gated as the ribbon renders them, actual step states elsewhere, summary with one next action, explicit owner move, none on Base; Mock G's form, K/L not chosen. | Mock-series messages 3–7, the 2026-09-01 definitions. |
| `g5.workbench.journey.default-open` (new D) | Expanded on open and fork, collapse remembered per workspace in the browser; amended 2026-10-12: in the rail display modes the overlay opens on Guided until the first hover-and-leave or dismissal. | R16 and Q4; the 2026-10-12 follow-up. |
| `g5.workbench.journey.steps` (new D) | A fixed client catalog of steps bound to ribbon action ids; four built-in actions added; Operations folds into IE; ontology generation consolidated into the plan step. | The step definitions; `g5.workbench.actions`; minimal mechanism. |
| `g5.workbench.journey.state` (new D) | Done by owner mark or by outcome evidence from documents and pull requests; activity is never completion; a mark cannot hide evidence. | Q1; `g5.data.outcome-evidence` (design review). |
| `g5.workbench.journey.marks` (new D) | Marks are owner-only durable events; the latest event is the state; the timeline shows them. | Q1; the landing contract's "every durable kind". |
| `g5.workbench.journey.gates` (new D) | Phase rosters between phases, the IE roster before the merge step, popover with View approvals; decisions keep their authority on the approvals tab. | Mock F; phase-bound approvals replace Q2's stage mapping. |
| `g5.workbench.journey.artifacts` (new D) | The outcome documents of the open-questions, mock and explainer steps render as citation chips; never ontology, plan or code. | The artifact-chip message. |
| `g5.workbench.journey.gate` (new D) | `guided_journey` instance feature from `G5_GUIDED_JOURNEY_ENABLED`; amended 2026-10-12 from fail-closed (on in dev/test/CI, off in QA and production) to a default-on kill-switch, off only on an explicit `false`. | Q3's answer; superseded by "enable it everywhere" (2026-10-12). |
| `g5.workbench.issues` (amended) | The rail reads Activity · Guided · Project docs · …; expansion is owned by `.default-open`. | R16 reconciled with "Activity at the very top" (2026-10-09). |
| Story index | Link added. | — |

The diff of the ontology files at the handoff is at the [end of this document](#ontology-diff-at-the-handoff).

## Amendments after the merge (2026-10-12)

The [follow-ups](../../chat_intent/guided-journey/request.md#user-instructions-2026-10-12-follow-ups)
asked after PR #444 merged, carried by one follow-up PR:

| Commitment | Change | Observable consequence |
|---|---|---|
| `g5.workbench.journey.gate` | Default-on kill-switch (`!is_env_flag_disabled`), like `top_panel`, instead of the fail-closed gate. | QA and production advertise `guided_journey` with no values change; `G5_GUIDED_JOURNEY_ENABLED=false` is the only way off. The e2e specs skip only on that value. |
| `g5.workbench.journey.default-open` | The default extends to the rail display modes, as the amended block states (each open of a workspace, until the first hover-and-leave or a dismissal; the overlay header is the remembered collapse). | In expand-on-hover the panel shows Guided until the person hovers the sidebar and leaves it, or clicks elsewhere; in rail until a click outside, Escape or the icon. Changing the display mode later never reopens it. |
| `g5.workbench.tabs` | The fork picker's workspace name is a link to the workspace route (`fork-picker-open`). | Clicking a name in the "+" picker opens that workspace as the new tab; Fork is unchanged. The workspace page's own route-driven organization switch covers other organizations. |

## Governing and relevant ontology

| Commitment | Consequence for this design |
|---|---|
| [g5.workbench.phases](../../ontology/features/workbench.g5.md#three-phases-organize-a-workspace), [.phases.state](../../ontology/features/workbench.g5.md#phase-is-explicit-workspace-state) | Three phases, IE ends at merge, phase is owner-moved state. The journey's active phase is the workspace phase; the source design's Operations folds into IE; "Move to <next phase>" is the existing owner move, never an automatic advance. |
| [g5.workbench.gates](../../ontology/features/workbench.g5.md#bind-approvals-to-phases), [g5.governance.approval](../../ontology/features/governance.g5.md#approval-is-a-human-authority), [.approval.request](../../ontology/features/governance.g5.md#let-an-addressed-reviewer-decide-their-request), [.approval.requirements](../../ontology/features/governance.g5.md#organization-administrators-administer-approval-requirements) | Approvals bind to phases and only IE requirements gate the merge. Each phase's roster is `approvalRosterForPhase`; the IE roster precedes the merge step; the popover links to the approvals tab where an addressed reviewer, owner or not, decides. Q2's planning/finalizing mapping is void. |
| [g5.workbench.actions](../../ontology/features/workbench.g5.md#ribbon-actions-are-templated-conversations) | Per-phase actions are data merged from built-ins and a committed file. Steps name action ids; the steps without an action today get four built-in actions (`open-questions` chat, `explainer` and `code-review` open targets, a `view-diff-graph` view), not bespoke handlers; a committed file that removes an action disables the step's button. |
| [g5.workspace.ui.gates](../../ontology/features/workspaces.g5.md#offer-an-action-by-what-its-operation-needs), [.ui.unavailable](../../ontology/features/workspaces.g5.md#keep-a-waiting-action-visible-with-its-reason-and-start) | Step buttons reuse the ribbon's enablement (`actionState`) and renderer (`renderAction` for preview), so a reader, a stopped VM or Base sees each action as the ribbon shows it; marks are the only owner-only operation the panel adds. |
| [g5.workbench.gate](../../ontology/features/workbench.g5.md#offer-the-alignment-ui-as-a-workspace-panels-layout) | The section lives under the alignment layout; its own `guided_journey` feature adds the independent switch Q3 asked for — fail-closed at the handoff, a default-on kill-switch since the 2026-10-12 amendment. |
| [g5.workbench.issues](../../ontology/features/workbench.g5.md#the-rail-lists-the-workspaces-tracker-issues) | Activity stays first and opens the timeline; Guided is second. |
| [g5.workbench.landing](../../ontology/features/workbench.g5.md#the-project-timeline-is-the-landing-page) | Every durable activity kind is on the timeline, so journey mark events are a feed kind (`journey_step`) with their own filter group. |
| [g5.design.authority](../../ontology/concepts/system_invariants.g5.md#each-fact-has-one-source-of-truth), [g5.data.outcome-evidence](../../ontology/concepts/system_invariants.g5.md#report-outcomes-from-their-authoritative-owners), [g5.design.minimal-mechanism](../../ontology/concepts/system_design_principles.g5.md#justify-each-new-mechanism) | Step state is a projection over owned facts (documents, pull requests) plus the owner's mark; a completed agent turn is activity, not evidence. The only new durable record is the mark event; no projection route, no journey override file, no session column. |
| [g5.workspace.base-ui.chat](../../ontology/features/workspaces.g5.md#omit-chat-from-base) | Base has no journey, as it has no chat. |
| [g5.documents.question.referenced](../../ontology/features/documents.g5.md#reference-a-question-from-its-set-file), [.agent-access](../../ontology/features/documents.g5.md#agents-read-and-propose-through-the-workspace-cli), [g5.documents.explainer.location](../../ontology/features/documents.g5.md#keep-generated-explainers-discoverable), [g5.workbench.mock](../../ontology/features/workbench.g5.md#mock-preview-is-a-preview-profile) | The outcome evidence the state reads: question sets under `docs/<ws>/questions/`, mocks under `mocks/`, explainers under the managed `explainers/` root — all in the Project Docs listing the sidebar already loads. |

## Requested outcome

Under the alignment layout, with the feature on, a workspace opens with a Guided section
expanded in the rail: the three phases on a vertical timeline, the current phase with its
steps always expanded and their buttons always visible, a summary on top naming the next step
with one button, rosters between phases showing who approved and who is still missing, and
under the question, mock and explainer steps the documents they produced as clickable chips.
Every button is a ribbon action the workspace already has; nothing in the panel is inert.

## Design

```text
LeftSidebar (workbench preset)            Guided section
  Activity                                 ┌ summary: "<Phase> · n of m done · Next: <step>"  [primary action]
  Guided  ◀ expanded on open               ├ ① Business Engineering
  Project docs · Ontology · Files · …      │     intent · open questions · mock          [ribbon actions]
                                           ├ roster BE  (approvalRosterForPhase "be") → popover → View approvals
                                           ├ ② System Engineering  (active: all steps, all buttons)
                                           │     plan · explainer · diff review · code · preview · code review
                                           ├ roster SE
                                           ├ ③ Integration Engineering
                                           │     rebase · pull request · [roster IE] · merge to parent
                                           └ chips = CitationChips(documents in the step's evidence folder)

inputs (already loaded by the page)         new
  phase, isBase, owner       workspace DTO   GET /workspaces/:id/journey      → latest mark per step
  ribbon actions + gates     workbench       PUT /workspaces/:id/journey/steps/:stepId {done}  (owner)
  documents                  doc list        workspace_journey_events (migration 080) → timeline kind journey_step
  pull requests              PR list (source, all)
  approvals                  approvals projection (the chevron meters' data)
```

**Owners and boundaries.** The workbench owns the step catalog (`lib/journeySteps.ts`) and
the mark events (`workspace_journey_events`, written and read by `WorkspaceService`);
governance owns approvals; documents own the evidence files; the pull-request owner owns the
PR facts. The Guided components (`components/guided/`) render data and hand every click to
the page's `handleWorkbenchAction`, `renderWorkbenchAction` and `actionState` — the same three
the ribbon uses — or to the phase move and the marks mutation; they own no behaviour. Failure
modes are the owners' own: a template that fails to start shows the chat error; a refused
move rolls the chevron back; a refused mark shows the API error and keeps the prior state.

**Step state** is a pure function `journeyStepState(step, facts)`: `done` when the latest
mark says done or the evidence rule holds, else `todo`. The summary's next step is the first
`todo` step of the active phase; when none remains the primary button is "Move to <next
phase>" (owner) and, in IE, the journey reads complete once the merge step is done.

**Steps and actions.** The catalog names, per phase, the step ids, labels, notes, action ids
and evidence rules listed in `g5.workbench.journey.steps`. Four built-in actions are added
to `actions.yaml` so every step has a real button: `open-questions` (BE Analysis group,
chat: read the requirement documents and the intent, create referenced question sets with
`g5d question create` and a Project Doc `open-questions.md` that references them),
`explainer` (SE Design group, open target: the Create Explainer modal), `view-diff-graph`
(SE Analysis group, `view` of the Diff Graph tab) and `code-review` (SE Build group, open
target: seeds the existing Review Workspace prompt into the composer). The first three also
appear on the ribbon, a visible change for the same review.

**Rosters.** `approvalRosterForPhase` feeds `ShipApprovalsRoster`, exported unchanged from
the status band with the journey's own test ids; its popover's View approvals opens the
approvals tab on that phase. A phase without requirements shows "no approvals required" with
the same link.

**Presentation.** Mock G, drawn with the workbench's phase hues: numbered nodes on a vertical
line, the active node in the phase hue, done nodes green with a check; step rows with a dot,
label, note, buttons and Mark done; chips under evidence rows; the rosters as overlapping
24px avatars with the count. Rail modes show Guided as an ordinary section.

## Reconciliation with the source design

| Source decision (Aug 31 – Sep 8) | Later record | Here |
|---|---|---|
| Four phases, Operations last | 2026-10-06: three phases, IE ends at merge | Operations' rebase, PR and merge are IE's steps |
| Steps bound to the status band's Generate presets | 2026-10-06: band gone under alignment; ribbon actions are templated chats | Steps bind to ribbon action ids; four built-in actions added |
| SE: generate the ontology, then design doc and plan | AGENTS.md step 1 authors ontology within the design | One plan step; recorded as a consolidation |
| Q2: gates mapped onto planning/finalizing stages | 2026-10-06: approvals bind to phases, stages removed | One roster per phase from the same projection; IE's before the merge |
| Mock F: approve in place from the popover | `g5.governance.approval`: decisions on the approvals surface | Popover shows Still missing / Approved and links to the approvals tab |
| Q3: new capability flag, off in staging/prod, on in dev | `alignment_ui` gate exists; 2026-10-12: "enable it everywhere" | `guided_journey` feature, default-on kill-switch since 2026-10-12 |
| Q1: derived projection plus a mark button | Design review: a turn is not evidence; sessions are not loaded | Owner marks plus document and pull-request evidence |
| Q4: remember the collapse per workspace | — | Browser storage like the section order |
| Default-open panel on open/fork | 2026-10-09: Activity at the very top | Guided second in the rail, the only section expanded |
| Hover-peek of up-next and done phases | Superseded by Mock G | Always-visible lists |
| Chips for question docs, mocks, explainers only | — | The evidence documents of those three steps |
| Mock G accepted; K and L to compare | No choice recorded | G built; K/L open (below) |

## Open decision for the human

**Timeline (Mock G) or flowchart (Mock K or L)?** The user asked for K ("the side panel is a
flowchart") and L ("another version … to compare different directions") on 2026-09-08 and
never chose. The plan of record calls G the accepted latest, so G is built; K and L keep the
same phases, steps, buttons, rosters and chips and change only how the panel body is drawn
(K: one tall flowchart with decision diamonds and a "request changes" loop edge; L: a compact
overview flowchart pinned on top with only the focused phase drawn in detail). K's loop would
add a rejection path to `g5.workbench.journey.gates`; nothing else in the ontology moves. The
four mocks are in [`mocks/`](mocks/).

## Decision notes

- **Evidence over activity.** The first draft derived chat steps from a completed assistant
  turn and the session's citations; the review showed a turn can be a clarification and that
  the page loads neither messages nor historical sessions. Documents and pull requests are
  owned facts the page already has; everything else is the owner's call.
- **No session column, no projection route, no override file.** Each was in the first draft
  and each duplicated a fact or a mechanism that exists; the review's minimal alternatives are
  taken: a fixed catalog over configurable actions, a client derivation, one event table.
- **Marks are events, on the timeline.** A mark can be the only durable trace of "I reviewed
  the diff graph", so it joins the feed rather than being excepted from the landing contract.
- **Rosters link instead of approving in place.** `approvalRosterForPhase` flattens a
  person's requirements; the decision needs the request identity the approvals tab has.
- **Code review stays one prompt.** `code-review` seeds `reviewWorkspaceChatPrompt()` through
  a `review` open target instead of copying the prompt into `actions.yaml`; `open-questions`
  is a new template like `build-mock`, because the classic Generate modal is not reachable
  under alignment and its prompt builder needs selected files.
- **Rebase step is a mark.** The behind count the page shows is relative to the branch base,
  not the merge target; Prepare to Ship's preflight is the authority, and it is a modal.
- **Acceptance.** Under alignment with the feature on, a fork of Base opens with Guided
  expanded on BE and every BE step's buttons visible; a reader sees the same steps with the
  ribbon's reasons; Mark done settles the diff-review step, Not done clears it, and a step with
  a merged PR cannot be undone; the mark appears on the timeline under its own filter; a mock
  document in `mocks/` completes the mock step and renders as a chip that opens the document;
  the BE roster shows the bound requirement's approvers and View approvals opens the tab on
  BE; the summary offers "Move to System Engineering" when BE's steps are done and the move
  persists; a collapsed Guided stays collapsed for that workspace and expanded for another;
  Base shows no Guided section; with the feature off, or under a classic layout, nothing
  changes.

## Implementation notes (2026-10-11)

- Built as designed; two routine adjustments during implementation: `view-diff-graph` sits in
  SE's Design group rather than Analysis, because the Author and Analysis groups are shared
  verbatim across the three ribbons (`workbench_config.rs` pins SE[..2] == IE[..2]); the
  summary's primary button carries its own test id (`guided-summary-action-<id>`) so a step's
  button stays unique on the page.
- Verification run here: `cargo test -p g5-api --test api_workspace_journey` (5 passed, against
  the dev Postgres) and `--test api_workspace_timeline` (6), `cargo test -p g5-api-core --lib
  workbench_config` and the activity serde test, `g5-contracts` `question_set`, `g5-api-routes`
  `instance`; vitest for the journey, Guided section, hook, sidebar, ribbon, activity and
  alignment-page suites. `frontend/e2e/flows/guided-journey.spec.ts` (default-open and remembered
  collapse; Mark done persisted, on the timeline, undone; a mock document completes its step and
  its chip opens the document; the summary's move; a roster link to the approvals tab) is written
  for the CI e2e stack, which sets both gates; it was not executed locally because the local
  validation preflight needs bash 4 (`mapfile`) and this Mac ships bash 3.2.

## Implementation review (2026-10-11)

Two independent read-only Codex reviews of the implementation (correctness and conformance;
duplication and reuse) returned REQUEST_CHANGES; every finding was triaged and the code above
reflects the fixes:

| Finding | Disposition |
|---|---|
| Question-set files (`.g5q.json`) are not Project Docs, so the open-questions step could never derive | Fixed: evidence is the `open-questions.md` document (or a document under `questions/`); `g5.workbench.journey.state` reworded |
| `FOR SHARE` from the owner check upgraded to `FOR UPDATE` in the service: deadlock under two concurrent marks | Fixed: a per-workspace transaction advisory lock serializes writes; concurrent same-step and different-step API test |
| `created_at` is the transaction start, so a late-landing earlier transaction could sort behind the mark it read | Fixed: identity `seq` column orders events; reads use `seq DESC` |
| Guided called `renderAction` for disabled actions (readers lost the visible preview button) | Fixed: the panel uses the ribbon's `RibbonButton` and `resolveRibbonActionState`; custom controls only for enabled actions, with their own test-id prefix |
| A PUT response replaced the cached list; overlapping marks or an in-flight GET could regress it | Fixed: cancel the read on mutate, refetch the authoritative list on settle |
| A loading or failed approvals projection read as "No approvals required" | Fixed: `approvalsStatus` renders "Loading approvals…" / "Approvals unavailable" |
| A merged pull request still recommended the rebase step | Fixed: the merge step's completion ends the journey |
| The mark route checked ownership but not the mutation policy; the client gated on `readOnly` only | Fixed: `ensure_workspace_mutable` on the route (read-only and freezing refused, tested); the client gates on the owner's durable authority |
| Metadata overclaimed (100% without concurrency or e2e execution) | Fixed: limits stated in each entry; concurrency and policy tests added; Playwright marked not-run here |
| A queued mark captured the hook's current workspace id (re-review) | Fixed: the mark carries its workspace id in the mutation variables, like the phase move; hook test |
| A transient refresh failure emptied marks and pull requests, turning done steps to-do (re-review) | Fixed: both reads keep loaded data through a transient failure (`toRefreshingState`); hook test |
| `open-questions.md` also completed the intent step; the timeline stamped transaction start (re-review) | Fixed: the intent rule excludes question evidence; events are stamped with `clock_timestamp()` under the lock, asserted in the concurrency test |
| Chat-intent interpretation said Q3's flag was unnecessary | Fixed: the interpretation now records the adopted flag |
| Duplicated ribbon renderer, slug validator, approval and document fixtures, e2e layout helper | Fixed: shared `RibbonButton`/`resolveRibbonActionState`, `is_slug_segment` from the contracts crate, `test/approvalFixtures.ts`, `createMockDocNode`, `openWorkspaceLayoutSelect` in the e2e helpers; `installWorkspace` gained a `detail` override |
| React-refresh warnings for exporting helpers from the ribbon file | Accepted: the file already exports its shared constants the same way |

## Risks and unresolved assumptions

- **Ribbon growth.** Three actions join the ribbon (open questions, explainer, diff graph);
  if Tim and Davide prefer the ribbon unchanged, the step can keep the action while the ribbon
  group hides it — a built-ins change, not a design change.
- **Evidence folders.** `questions/`, `mocks/` and `explainers/` are the conventions of the
  question service, the `build-mock` template and the explainer root; a team that files mocks
  elsewhere marks the step by hand.
- **UXv2 mock drift.** The mock pages under `mocks/` use the older dark editor tokens; the
  implementation uses the workbench tokens.

## Review summary

Two independent read-only Codex reviews of the first draft (2026-10-11, `codex exec
--sandbox read-only --ephemeral`, gpt-5.6) returned REQUEST_CHANGES; every finding was
triaged and the design above is the result:

| Finding | Disposition |
|---|---|
| Completion from a completed assistant turn, a preview task or a PR's existence exceeds the evidence (`g5.data.outcome-evidence`) | Fixed: documents and merged/open pull requests are the only derived evidence; other steps are owner marks; a mark never hides evidence |
| Next-step selection skipped running steps; recap showed skipped steps as done | Fixed: first unfinished step; every phase shows actual states |
| The page does not load session messages, historical sessions or merged PRs; `behindBase` is not the merge-target count | Fixed: no session-derived state; PR query is `source, all`; rebase is a mark |
| Blanket owner gating contradicted `g5.governance.approval`; rosters are display-only | Fixed: ribbon gates per action; popover links to the approvals tab |
| Preview dispatch bypassed `renderWorkbenchAction`; no Diff Graph action existed | Fixed: the panel uses `renderAction` and `actionState`; `view-diff-graph` built-in |
| The explainer modal cannot carry an action identity | Moot: no session identity is used |
| Path-based chip whitelist admitted every Project Doc | Fixed: evidence folders per step |
| "Generate the ontology" dropped silently | Fixed: recorded consolidation into the plan step |
| Q3's independent flag replaced without authority | Fixed: `guided_journey` feature |
| Configurable `journey.yaml` lacked a completion contract | Fixed: fixed catalog, no override file |
| Marks excluded from the timeline against the landing contract | Fixed: `journey_step` feed kind |
| Blocks bundled commitments; provenance and metadata format | Fixed: eight blocks, human-verified/agent tags, `unassessed` where no denominator |
| New prompts duplicate `reviewWorkspaceChatPrompt` / `generateChatPrompt` | Partly: code review reuses the prompt; open questions is a template like `build-mock` (reason above) |
| Journey blocks restated governance policy; default-open had two homes | Fixed: references only; `.default-open` owns expansion |

## Ontology diff at the handoff

The diff of the ontology files at the implementation handoff (`git diff origin/main -- .g5/ontology/`); the metadata entries carry the implementation and test locations.

```diff
diff --git a/.g5/ontology/concepts/core_user_stories.g5.md b/.g5/ontology/concepts/core_user_stories.g5.md
index 32d94e58..eba9c307 100644
--- a/.g5/ontology/concepts/core_user_stories.g5.md
+++ b/.g5/ontology/concepts/core_user_stories.g5.md
@@ -54,6 +54,7 @@ These links add no separate commitments or implementation percentages.
 - [Act with one click in the current phase](../features/workbench.g5.md#act-with-one-click-in-the-current-phase).
 - [Turn a meeting into decisions](../features/workbench.g5.md#turn-a-meeting-into-decisions).
 - [Keep several workspaces open](../features/workbench.g5.md#keep-several-workspaces-open).
+- [Follow the guided journey](../features/workbench.g5.md#follow-the-guided-journey).
 
 ## Files, documents and comments
 
diff --git a/.g5/ontology/features/workbench.g5.md b/.g5/ontology/features/workbench.g5.md
index e017cf00..e93462ec 100644
--- a/.g5/ontology/features/workbench.g5.md
+++ b/.g5/ontology/features/workbench.g5.md
@@ -49,6 +49,16 @@ As a workspace owner, I see the tracker issues attached to my workspace in the l
 the primary one first, import more from the backlog with one picker, and act on one from its
 page: implement it here, fork it into its own workspace, detach it, or request approval.
 
+### Follow the guided journey
+
+[I · g5.story.follow-the-guided-journey]
+
+As a workspace owner new to G5, I open a workspace and a Guided panel walks me through
+the three phases as a vertical timeline: what each phase asks me to do, which steps are
+done, the one thing to do next with its button, the documents a step produced, and who
+still has to approve before the work moves on. I never compose a prompt or hunt for a
+button to follow the journey.
+
 ## Behavioral contracts and design
 
 ### Three phases organize a workspace
@@ -229,8 +239,9 @@ icon-plus-label format, and the global icons sit on the one bar every page share
 
 [D · g5.workbench.issues]
 
-Under the alignment layout the left rail reads Activity · Project docs · Ontology · Files ·
-Issues & tasks · Jobs, then the remaining sections. Activity opens the timeline in the work
+Under the alignment layout the left rail reads Activity · Guided · Project docs · Ontology ·
+Files · Issues & tasks · Jobs, then the remaining sections ([Guided](#guided-opens-by-default-and-remembers-a-collapse)
+owns its default expansion). Activity opens the timeline in the work
 area rather than a drawer and carries the rail's own collapse control; the collapsed rail shows
 an expand control at its edge and the title bar carries no sidebar toggle under this layout.
 Jobs is the existing task queue under its workbench name. Issues &
@@ -249,6 +260,131 @@ controls stay visible and disabled with that reason until the broker exists. Rea
 mock session asked for one rail item for issues and tasks with Activity on top; every listed
 action already has an owner, and no second issue store or write path is justified.
 
+### The Guided panel is the journey through the phases
+
+[D · g5.workbench.journey]
+
+Under the alignment layout the sidebar has a Guided section: a vertical timeline of the
+three [phases](#three-phases-organize-a-workspace) with a journey summary on top. The
+workspace's current phase is the active one: it lists every one of its
+[steps](#journey-steps-are-a-fixed-catalog-over-ribbon-actions), always expanded, each with
+its buttons always visible; a button is the ribbon action it names, rendered and enabled
+exactly as the [ribbon](#ribbon-actions-are-templated-conversations) renders and enables it
+(the [page's gates](workspaces.g5.md#offer-an-action-by-what-its-operation-needs) decide,
+per action, what a reader, a stopped VM or Base may do). The other phases list their steps
+with their actual [state](#a-step-is-done-by-its-outcome-or-by-the-owner), dimmed when
+later. The summary names the phase, how many steps are done and the first unfinished step,
+and carries one primary button: that step's primary action, or "Move to <next phase>" once
+every step of the phase is done — the ordinary owner
+[move](#phase-is-explicit-workspace-state), never an automatic advance — and it reads complete
+once the merge step is done. Base has no Guided section. Reason: the Jada persona needs one
+place that says what to do next (2026-08-26 standup; 2026-09-09 "guided templates for
+non-technical users"); the user's mock series settled on the timeline of Mock G
+(always-expanded steps, roster approvals, summary, chips, default-open) over the wizard gates
+of Mock A and the hover-revealed steps of Mock F. The flowchart variants (Mocks K and L) were
+made to compare directions and were not chosen; they would change only how the panel body is
+drawn, not this contract.
+
+### Guided opens by default and remembers a collapse
+
+[D · g5.workbench.journey.default-open]
+
+Guided is the one section expanded when a workspace is opened or forked. An explicit collapse
+is remembered for that workspace in the person's browser, like the section order, until the
+person expands it again. Reason: "this side panel should be the one that opens by default when
+you open/fork a workspace", and the user chose to remember the collapse per workspace (Q4).
+
+### Journey steps are a fixed catalog over ribbon actions
+
+[D · g5.workbench.journey.steps]
+
+The steps are a built-in catalog in the client: per phase, ordered steps with a stable id, a
+label, an optional note, the ids of the ribbon actions that are its buttons (the first is
+primary) and its evidence rule. Business Engineering: bring in the business intent (New doc,
+Upload file); generate the open questions and conflict intents (`open-questions`, a built-in
+chat template that creates [referenced question sets](documents.g5.md#reference-a-question-from-its-set-file));
+build the mock (Build mock, Preview mock). System Engineering: create the implementation
+plan (Create implementation plan — the plan carries its ontology changes, so the source
+design's separate "generate the ontology" step is consolidated here: the planning workflow
+authors ontology within the design); generate an explainer (`explainer`, an open target for
+the existing Create Explainer modal); review the ontology diffs (`view-diff-graph`, the Diff
+Graph tab); generate the code (Generate code); start a preview (Preview); start a code review
+(`code-review`, an open target that seeds the existing Review Workspace prompt). Integration
+Engineering: rebase on the parent; open a pull request (both bound to Create PR and rebase);
+merge to parent (Merge to parent). The three source-design Operations steps are Integration
+Engineering's because [IE ends at merge](#three-phases-organize-a-workspace). A step whose
+action is absent from the phase's resolved ribbon (a committed `actions.yaml` removed it)
+renders disabled with that reason and never invalidates the ribbon. Reason: the user bound
+every step to a button the workspace already has; a fixed catalog over the configurable
+actions needs no second configuration file or override mechanism
+([minimal mechanism](../concepts/system_design_principles.g5.md#justify-each-new-mechanism)).
+
+### A step is done by its outcome or by the owner
+
+[D · g5.workbench.journey.state]
+
+A step is done when the owner marked it done, or when the owner of its outcome shows that
+outcome ([outcome evidence](../concepts/system_invariants.g5.md#report-outcomes-from-their-authoritative-owners)):
+the open-questions document for the open-questions step (`open-questions.md` under
+`docs/<workspace-id>/`, or a document under its `questions/` folder — the question set files it
+references are not Project Docs), a document under `mocks/` for the mock step, an [explainer](documents.g5.md#keep-generated-explainers-discoverable)
+for the explainer step, any other Project Doc for the intent step, a pull request from this
+workspace for the pull-request step and a merged one for the merge step. The plan, diff
+review, code, preview, code review and rebase steps have no outcome the page can read, so the
+owner settles them. Every step offers "Mark done" and "Not done"; a mark never hides an
+outcome, so a step with evidence cannot be undone. A started conversation, task or preview is
+activity, not completion. Reason: the user chose "a button to mark a step as complete if it
+needs client-observed steps" over a persisted journey record (Q1), and a completed agent turn
+proves nothing about the artifact it was asked for.
+
+### Marks are durable owner events
+
+[D · g5.workbench.journey.marks]
+
+A mark is an owner-only [platform record](../concepts/data_model.g5.md): one event naming
+the workspace, the step, done or not, the actor and the time. A step's current mark is its
+latest event, so there is no second copy of the state; setting the state it already has
+records nothing. The [timeline](#the-project-timeline-is-the-landing-page) shows each event as a
+"Guided step" entry. Reason: the mark is a person's durable decision like a phase move, and the
+timeline promises every durable activity kind.
+
+### Approvals sit between phases as rosters
+
+[D · g5.workbench.journey.gates]
+
+Between two phases the timeline shows the finished phase's roster — the status band's
+overlapping avatars with state rings and badges, the approved count, and the Still missing /
+Approved popover whose View approvals opens the approvals tab on that phase, where decisions
+keep [their own authority](governance.g5.md#approval-is-a-human-authority). The Integration
+Engineering roster sits before the merge step, because
+[those requirements gate the merge](governance.g5.md#organization-administrators-administer-approval-requirements).
+A phase with no requirement shows "no approvals required" with the same link. The rosters read
+the same approvals projection as the chevron meters. Reason: "approvals between phases,
+rendered like the little avatars in the status panel" (Mock F) maps one-to-one onto
+[phase-bound approvals](#bind-approvals-to-phases); the source plan's mapping onto the removed
+planning/finalizing stages is void, and no second decision surface is added.
+
+### Completed steps show their documents as chips
+
+[D · g5.workbench.journey.artifacts]
+
+Under the open-questions, mock and explainer steps, the documents that are their outcome
+evidence render as the chat's citation chips (same component; a click opens the document), in
+the active phase and in finished phases alike. Ontology, plan and code artifacts never render
+as chips. Reason: the user asked for the artifacts inline "like we do today in the chat",
+limited to question docs, explainers and mocks "since it will be too many artifacts" otherwise.
+
+### Guided has its own fail-closed gate
+
+[D · g5.workbench.journey.gate]
+
+The section renders only while the resolved layout is `alignment` and the instance
+advertises `guided_journey` — exactly `G5_GUIDED_JOURNEY_ENABLED=true`, fail-closed like
+[`alignment_ui`](#offer-the-alignment-ui-as-a-workspace-panels-layout): on in the dev and
+test stacks and CI, off in QA and production until a deployment turns it on. Reason: the
+user asked for "a new capability flag, default off in staging + prod, enabled in dev by
+default" (Q3); an independent switch keeps the workbench while Guided is reviewed.
+
 ### Offer the alignment UI as a workspace-panels layout
 
 [D · g5.workbench.gate]
@@ -408,8 +544,90 @@ Deferral neither establishes completion nor cancels intent.
 
 - human-verified
 - User instruction 2026-10-09 in the [chat intent](../../chat_intent/ux-v2-workbench/request.md) (mock session prompts #24, #26, #29, #30, #52; change log W08, W10, W12); the disabled Done / quick-add controls follow `g5.external-issues.completion` and `g5.external-issues.broker`.
+- Amended 2026-10-11 by the [guided-journey plan](../../pr_ontology_diff_design/guided-journey/ontology-plan.md): the rail gains Guided after Activity; the rest of this entry is the prior assessed baseline.
 - 100% · V1 contract implemented: `frontend/src/components/tree/TreeNode.tsx` has Activity · Project docs · Ontology · Files · Issues & tasks · Jobs, then Explainers · Archive · Source Control; both presets retain the new IDs, classic hides them, and outgoing canonical orders migrate. `components/external-issues/WorkspaceIssuesSection.tsx` projects primary-first links, Subtask labels, native closed state and shared typed read errors; org-off cleanup reuses `WorkspaceIssuesPanel.tsx`. `ImportTaskPicker.tsx` searches open issues, imports related links sequentially, preserves row results, and uses `IssueLinkConflictNotice.tsx`. `pages/WorkspacePage.tsx` supplies linked-issue actions under resolved alignment only: bare persona start through the ribbon launch/revision handling, issue-prefilled `CreateWorkspaceDialog`, shared primary-aware unlink confirmation, and current-phase approvals. Read failures and confirmation are shared in `WorkspaceIssueLinkParts.tsx`; Done/Reopen and quick-add remain disabled with the tracker-write reason. Tests: adjacent issue suites, `components/tree/TreeNode.sectionOrder.test.tsx`, `components/Sidebar.test.tsx`, `pages/WorkspacePageAlignment.test.tsx`; browser rail assertions in `e2e/flows/alignment-shell.spec.ts`. Remaining: provider close/reopen/create, completion mapping and enforcing broker are deferred V2 work, not implemented by these controls. Verification limits: Playwright and live tracker/agent effects unexecuted here; [self-review](../../pr_ontology_diff_design/ux-v2-workbench/ontology-review.md#phase-8--implementation-self-review-2026-10-09).
 
+### g5.story.follow-the-guided-journey
+
+- human-verified
+- [requirement.md and the mock series](../../chat_intent/guided-journey/request.md) of the G5 workspace `Davide-guided` (2026-08-31 → 2026-09-08); the 2026-08-26 standup names the phases, the 2026-09-09 notes the guided mode for non-technical users; [design](../../pr_ontology_diff_design/guided-journey/ontology-plan.md).
+- Status: active, implemented.
+- Implementation: 100% of the V1 story — the Guided section (`frontend/src/components/guided/GuidedJourneySection.tsx`) rendered by `LeftSidebar` from `pages/WorkspacePage.tsx` under the alignment layout; the journey's steps, state, marks, rosters, chips and gate are the blocks below.
+- Tests: 100% of the inventoried unit and page scenarios — `pages/WorkspacePageAlignment.test.tsx` ("guided journey": expanded after Activity, buttons dispatch, documents and marks complete steps, reader reasons, move, rosters), `components/guided/GuidedJourneySection.test.tsx`, `lib/journeySteps.test.ts`. Browser: `e2e/flows/guided-journey.spec.ts` (default-open and remembered collapse; Mark done persisted, on the timeline, undone; documents created through the API complete their steps, the mock's chip opens the document, the summary's move persists; a roster link opens the approvals tab on its phase) is written for the CI e2e stack and was not executed here; the real-browser screenshots in the design folder come from the app in mock-data mode.
+- Verification: test-verified (vitest, current); Playwright not-run here.
+
+### g5.workbench.journey
+
+- human-verified
+- Mock-series messages 3–7 and the 2026-09-01 phase definitions in the [chat intent](../../chat_intent/guided-journey/request.md); Mock G and Mocks F, K, L in the [design folder](../../pr_ontology_diff_design/guided-journey/mocks/).
+- Status: active, implemented.
+- Implementation: 100% — `frontend/src/components/guided/GuidedJourneySection.tsx` (summary with one primary action, vertical timeline, active phase with every step and its buttons through the ribbon's own `RibbonButton` and `resolveRibbonActionState`, `renderAction` for an enabled preview only, actual states elsewhere, dimmed later phases, owner move through `useUpdateWorkspacePhase`, complete after merge); `pages/WorkspacePage.tsx` passes it as `guidedSection` only when the layout is `alignment`, the feature is on and the workspace is not Base; `components/tree/TreeNode.tsx` `guided` section (title, icon, hidden without a body or under the classic preset). Not built: the flowchart presentations of Mocks K and L (not chosen).
+- Tests: 100% of the inventoried scenarios — `components/guided/GuidedJourneySection.test.tsx` (expansion and dimming, dispatch and gates, missing action, summary and move, complete), `pages/WorkspacePageAlignment.test.tsx` ("guided journey"), `components/tree/TreeNode.sectionOrder.test.tsx` ("Guided section").
+- Verification: test-verified (vitest, current).
+
+### g5.workbench.journey.default-open
+
+- human-verified
+- "This side panel should be the one that opens by default" and Q4's answer in the [chat intent](../../chat_intent/guided-journey/request.md).
+- Status: active, implemented.
+- Implementation: 100% — `components/tree/TreeNode.tsx`: `defaultCollapsedSectionsFor` seeds every section collapsed except Guided unless `g5-sidebar-guided-collapsed-<workspaceId>` is set; `toggleCollapse` records or clears that key for Guided; the seed re-runs on a workspace switch like the other per-workspace state. A fork is a new workspace id, so it opens expanded.
+- Tests: 100% — `components/tree/TreeNode.sectionOrder.test.tsx` ("Guided section": expanded by default, collapse remembered, another workspace expanded), `pages/WorkspacePageAlignment.test.tsx` (aria-expanded on open); `e2e/flows/guided-journey.spec.ts` (reload keeps the collapse).
+- Verification: test-verified (vitest, current).
+
+### g5.workbench.journey.steps
+
+- human-verified
+- The 2026-09-01 step definitions (human-provided text in the [chat intent](../../chat_intent/guided-journey/request.md)); the binding to ribbon action ids, the four built-in actions and the ontology-step consolidation are the agent's mapping, recorded in the [plan](../../pr_ontology_diff_design/guided-journey/ontology-plan.md).
+- Status: active, implemented.
+- Implementation: 100% — the catalog `JOURNEY_STEPS` and `findWorkbenchAction` in `frontend/src/lib/journeySteps.ts`; built-ins `open-questions` (BE Analysis), `explainer` and `view-diff-graph` (SE Design) and `code-review` (SE Build) in `backend/g5-api-core/src/services/agents/workbench_builtins/actions.yaml`, regenerated into `frontend/src/lib/workbenchBuiltins.generated.ts`; the `explainer` and `review` open targets in `backend/g5-api-foundation/src/contracts/workbench.rs`, `lib/types.ts`, `ActionRibbon.tsx` (`KNOWN_TARGETS`) and `workbenchIcons.ts`; `pages/WorkspacePage.tsx` dispatches `explainer` to the Create Explainer modal, `review` to `reviewWorkspaceChatPrompt()` and the Diff Graph through `WORKBENCH_VIEW_TABS`; a missing action renders disabled in `GuidedJourneySection.tsx`.
+- Tests: 100% — `lib/journeySteps.test.ts` (every step's actions exist in its phase's built-ins; stable ids), `components/guided/GuidedJourneySection.test.tsx` (missing action disabled), `components/workbench/ActionRibbon.test.tsx` (built-in sets), `workbench_config.rs` unit tests (built-ins validate, shared SE/IE groups, generated fallback drift).
+- Verification: test-verified (vitest and cargo unit tests, current).
+
+### g5.workbench.journey.state
+
+- human-verified
+- Q1's answer in the [chat intent](../../chat_intent/guided-journey/request.md); the evidence sources are the agent's reading of the documents and pull-request records, after the design review found that a completed turn is not outcome evidence ([plan](../../pr_ontology_diff_design/guided-journey/ontology-plan.md)).
+- Status: active, implemented.
+- Implementation: 100% — `journeyStepState`, `journeyEvidenceDocs`, `journeyStates` and `journeyNextAction` in `frontend/src/lib/journeySteps.ts` over the docs inventory (`docsFolderPath` + `questions/`, `mocks/`, workspace explainers, other Project Docs), the source pull requests (`useWorkspacePullRequests(id, "source", "all")` in `pages/WorkspacePage.tsx`) and the marks; the Mark done / Not done control in `GuidedJourneySection.tsx` is absent on a step with evidence.
+- Tests: 100% of the inventoried scenarios — `lib/journeySteps.test.ts` (evidence folders, the open-questions document, unknown docs folder, pull requests, mark and evidence precedence, next step, move, completion after the merge without earlier marks), `components/guided/GuidedJourneySection.test.tsx`, `pages/WorkspacePageAlignment.test.tsx`. Limit: evidence over the real docs inventory is exercised through the API contract shape in MSW, not against a running stack here.
+- Verification: test-verified (vitest, current).
+
+### g5.workbench.journey.marks
+
+- agent
+- Q1's "button to mark a step as complete" needs a durable home; the event shape follows `workspace_phase_events` and the landing contract ([plan](../../pr_ontology_diff_design/guided-journey/ontology-plan.md)).
+- Status: active, implemented.
+- Implementation: 100% — `backend/g5-api/src/db/migrations/080_workspace_journey_events.sql` (`workspace_journey_events` with an identity `seq` that orders events, step-id shape check, tenant policy); `WorkspaceService::list_journey_marks` (latest event per step by `seq`) and `set_journey_mark` (ownership and mutation policy checked by the route, Base refused, id validated with the shared slug predicate, writes serialized on a per-workspace advisory lock, stamped at insertion so the timeline order agrees with `seq`, no event when the state is unchanged) in `g5-api-core/src/services/workspace/workspace_service.rs`; `GET /api/v1/workspaces/:id/journey` and owner-only `PUT …/journey/steps/:stepId` in `g5-api-routes/src/routes/workspaces.rs`; DTOs in `g5-api-foundation/src/contracts/workbench.rs` and `lib/types.ts`; the timeline's `JourneyStep` entry (`workspace_activity.rs`, `lib/activityFilters.ts` group `guided_steps`, `WorkspaceActivityPanel.tsx`); client `hooks/useJourney.ts` (read, put, activity invalidation).
+- Tests: 100% of the inventoried scenarios — `backend/g5-api/tests/api_workspace_journey.rs` (owner marks and undoes, idempotent, latest reported, timeline entries, non-owner refused but reads, invalid ids refused, Base refused, read-only and freezing refused, concurrent marks on different and the same step land without a deadlock and never repeat a state), `workspace_activity.rs` serde test, `pages/WorkspacePageAlignment.test.tsx` (PUT body and settled state), `components/external-issues/WorkspaceActivityPanel.test.tsx` (entry and filter group), `hooks/useJourney.test.tsx` (a mark writes to the workspace bound at mutate time; loaded marks survive a transient refresh failure). Limit: the client's reconciliation of overlapping marks relies on query cancellation and refetch, exercised sequentially only.
+- Verification: test-verified (cargo integration tests against Postgres and vitest, current).
+
+### g5.workbench.journey.gates
+
+- human-verified
+- Mock-series messages 4–5 (roster approvals between phases) and Q2's answer, superseded by [bind approvals to phases](#bind-approvals-to-phases); see the [chat intent](../../chat_intent/guided-journey/request.md).
+- Status: active, implemented.
+- Implementation: 100% — `GuidedJourneySection.tsx` renders `ShipApprovalsRoster` (exported from `components/workspace/WorkspaceStatusBand.tsx`, with `guided-roster-<phase>` test ids) over `approvalRosterForPhase` after BE and SE and before the IE merge step, "No approvals required" with a Request approval link only when the projection loaded, "Loading approvals…" / "Approvals unavailable" otherwise (`approvalsStatus` from the page); View approvals opens `openWorkspaceApprovalsTab(phase)` from the page.
+- Tests: 100% — `components/guided/GuidedJourneySection.test.tsx` (roster, popover, link per phase, IE placement, loading and unavailable projection never read as "none required"), `pages/WorkspacePageAlignment.test.tsx` (projection-fed roster opens the approvals tab).
+- Verification: test-verified (vitest, current).
+
+### g5.workbench.journey.artifacts
+
+- human-verified
+- The 2026-09-01 artifact-chip message in the [chat intent](../../chat_intent/guided-journey/request.md); "documents that are the outcome evidence" is the agent's reading after the design review.
+- Status: active, implemented.
+- Implementation: 100% — `evidenceDocs` of `journeyStepState` (question sets, mocks, explainers; never the intent step) rendered by `CitationChips` under `guided-step-<id>-artifacts` in `GuidedJourneySection.tsx`; a click opens the document tab from `pages/WorkspacePage.tsx`.
+- Tests: 100% — `lib/journeySteps.test.ts`, `components/guided/GuidedJourneySection.test.tsx` (chips in a finished phase open the document; none for the intent step), `pages/WorkspacePageAlignment.test.tsx`.
+- Verification: test-verified (vitest, current).
+
+### g5.workbench.journey.gate
+
+- human-verified
+- Q3's answer in the [chat intent](../../chat_intent/guided-journey/request.md); pattern: `alignment_ui` in `backend/g5-api-routes/src/routes/instance.rs` and `frontend/src/lib/topPanelFlags.ts`.
+- Status: active, implemented.
+- Implementation: 100% — `guided_journey` from `G5_GUIDED_JOURNEY_ENABLED` in `backend/g5-api-routes/src/routes/instance.rs`; `isGuidedJourneyEnabled` in `frontend/src/lib/topPanelFlags.ts` and `useGuidedJourneyEnabled` in `hooks/useSettings.ts`; `pages/WorkspacePage.tsx` renders the section only with the resolved `alignment` layout and the feature; `infra/docker/docker-compose.{dev,test}.yml` and `infra/ci/cloudbuild-ci-e2e.yaml` set it, `infra/helm/g5/values-qa.yaml` and production do not.
+- Tests: 100% — `instance.rs` unit test (exact `true`, fail-closed), `pages/WorkspacePageAlignment.test.tsx` (absent without the feature and under a classic layout).
+- Verification: test-verified (cargo unit tests and vitest, current).
+
 ### g5.workbench.gate
 
 - human-verified
```

## Mock U implementation (2026-10-20)

Follow-up to the merged panel: the user asked to implement Mock U
(`mocks/mock-u-combined.html`), the combination of Q (row and mark microinteraction),
R (motion spec), S (honest states) and T (keyboard and screen reader) from the
UI-skills study (`mocks/README-mocks-m-t.md`). P (ledger), N (role-aware summary) and
O (default-open rules) stay out by design.

### Ontology changes

- New `g5.ui.guided-panel` (D): the panel's reading contract (one next thing, folded
  rows and phases, status words, ring control with undo and skip, running from page
  knowledge, Unknown over to-do, placeholders, inline failures, reader without
  controls, motion tokens and reduced motion, landmark and live region).
- `g5.ui.journey-evidence` amended: a skip is the owner's third mark, out of the count,
  reversible, and a result still completes a skipped step.

### Decisions and amendments

| Decision | Why | Consequence |
|---|---|---|
| Skip is a durable mark kind (`skipped` column), not a client flag | A skip is a decision the team should see on the timeline and that survives reload, like a mark | Additive migration `098`, DTO fields default to false, older clients keep working; the timeline wording "Skipped: …" |
| Done phases and the upcoming phase fold by default | The shipped panel ran to ~1,400px; the next thing must sit above the other sidebar sections | Chips of a finished phase are one click away; tests and the e2e flow expand first |
| Running comes from the page's own memory of the session it started for an action, checked against the active-task view | Sessions do not persist the launching action; adding that is a chat-contract change out of this scope | A reload forgets a running step; recorded as a limit in the block metadata |
| Reader rows have no controls; a read-only or freezing owner keeps disabled controls with the reason | `g5.ui.unavailable-actions` wants the reason explained; a reader cannot enable anything, an owner can (unfreeze, transfer) | Two props: `isOwner` and `markBlockedReason` |
| Step labels become outcomes (copy deck); ids unchanged | Labels narrated features ("Generate an explainer") | Activity entries name the new labels; e2e text updated |
| The ring is the mark control; "Mark done" and "Skip" text buttons remain in the open row | Two signifiers for one state keep the control discoverable without a tooltip | `guided-step-<id>-mark` is the ring (kept for the e2e flow), `-mark-done` and `-skip` the text buttons |
| Mark failures stay inline (`role="alert"`, Retry) instead of a toast | Nielsen error recovery; the failure belongs where the action happened | `markFailure` state in the page; retry replays the same mark |

### Evidence

- Backend: `tests/api_workspace_journey.rs` adds the skip scenario (skip, idempotent,
  done-and-skipped refused, undo, timeline flag); existing mark tests unchanged.
- Frontend: `GuidedJourneySection.test.tsx` rewritten (fold, ring and undo, skip,
  reader, blocked owner, chips in a folded phase, move with a skipped step, complete,
  approvals unknown, marks and pull requests unknown with Retry, skeleton, running,
  failed and saving rows, live region and arrow keys, custom controls, rosters);
  `journeySteps.test.ts` (skip state, count, next, evidence wins);
  `WorkspacePageAlignment.test.tsx` (PUT body carries `skipped`, folded BE phase,
  reader without controls); `WorkspaceActivityPanel.test.tsx` (new labels, skipped
  entry); `e2e/flows/guided-journey.spec.ts` (toggle before the folded row, new
  activity label, `skipped` in the API read).

### Real panel after implementation

`screenshots/real-panel.png` (SE phase, a mark, a skip, folded BE and IE) and
`screenshots/real-panel-expanded.png` (BE expanded, a folded row opened), taken in
mock mode with `mocks/shot-real-panel.mjs` (Vite `VITE_MOCK=true --port 5199`, the
script patches the journey and detail responses). The gate row's "No approvals
required · Request approval" is kept on one line at the sidebar's width.
