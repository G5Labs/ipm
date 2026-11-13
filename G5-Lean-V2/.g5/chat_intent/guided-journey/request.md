# Guided journey — request and interpretation

## User instructions (2026-10-11, this port)

- "Look at https://dev.g5labs.ai/internal-g5-devs/Davide-guided/doc/overlay:… (`mock-f-hover-approvals-summary.html`) and the chats in that workspace. Port the changes that were done in that workspace into this codebase and open a PR. If a mock was not decided let me know."
- "There are two repositories: G5 (`~/Desktop/G5-new/G5-Tungsten-seed`) and G5-Lean (this folder). G5-Lean was forked from G5 and then a lot of features were removed and the architecture was simplified. This workspace is from G5."
- "You can also check what changed since then using the files in `~/Desktop/G5/G5-Transcript/G5/meetings`, especially the ones starting with UI, and `~/Downloads/Tim*`, and also `~/Downloads/session-prompts.json` and `~/Downloads/ux-changes.jsonl`."

## User instructions (2026-10-12, follow-ups)

After PR #444 merged, in the same conversation that opened the follow-up PR:

- "https://github.com/G5Labs/G5-Lean/pull/444 is merged, but I don't see it in the UI. Is there
  a flag that needs to be enabled? If yes let's enable it everywhere."
- "Can we also make sure that when a workspace is open the guided flow is open by default in
  the sidebar, until someone hovers over it, then it can go back to the 'expand on hover'
  behavior"
- "in the same PR, can we also make sure that when clicking on the name of a workspace after
  clicking on the <button type="button" data-testid="workspace-tab-add", I can open the
  workspace?"

Interpretation: the `guided_journey` feature becomes a default-on kill-switch (Q3's
"default off in staging + prod" is superseded); the Guided default-open extends to the
sidebar's rail display modes (the user's sidebar is in expand-on-hover mode, where the
section-level default could not show); the "+" fork picker's workspace names open the
workspace as a tab, as Home's directory already does.

## What the source workspace holds (G5 workspace `Davide-guided`, 2026-08-31 → 2026-09-08)

The workspace's code branch never left `main`; every change is a Project Doc:
`requirement.md`, the mock series A → L, `guided_flow.jsonl` (the A → F chat) and
`guided_panel_implementation_plan.md` (`guided-panel-v1`, status
`reviewed-with-open-decisions`, four answered questions). The user's own messages,
verbatim, are the human provenance of this design.

### requirement.md (2026-08-31)

"We want to add a new left panel (like core ontology/project docs/explainers/etc..) that
is a guided experience. It needs to be like a vertical timeline with the phases defined
in the transcript [G5 Daily Standup 2026-08-26], and with the buttons that are already
in the status bar today (upload docs, generate, view changes). Generate few mocks for
this so I can choose the best one."

### Mock series A → F (`guided_flow.jsonl`, 2026-08-31)

1. "download this mock because I think the sandbox is blocking its interactivity"
2. "augment them to be interactive a minimum"
3. "can we have mock D which is mock A but with something better for the gates? And mock E with the gates removed completely (all the orange parts)"
4. "mock F: mock e + approvals between phases, rendered like in the status panel (expanded on hover) + high level summary on top like in mock c"
5. "when I said approvals in the middle I meant the little avatars we have in the status panel"
6. "hovering on Integration Engineering should also expand its menu"
7. "also hovering on completed phases (Business Engineering)"

### Phase and step definitions → Mock G (2026-09-01)

"Let's define the phases:
[Business engineering] The first step should be upload a doc/draft it in the chat. The
second step should be to generate a doc with the open questions/conflict intents (we
have this in the "Generate" button today). The third step should be to generate mocks.
[System engineering] The first step should be to generate the ontology. The second step
should be to generate a design doc and implementation plan. The third step should be to
generate an explainer. The fourth step should be to review the ontology diffs (same as
clicking "View Diff Graph" in the side panel).
[Integration engineering] The first step should be to generate the code. The second step
should be to start a preview (using the preview button). The third step should be to
start a code review (using the review skill, which today is in the
data-testid="review-workspace-button").
[Operations (basically the buttons in "Prepare to ship")] The first step should be to
rebase on the target. The second step should be to open a PR. The last step should be
merge the PR in the target.
Approvals are between phases (a bit different than the status bar) so we don't need to
define them inside the phase. When you are in a phase all the steps are always expanded
and the buttons are always visible. This side panel should be the one that opens by
default when you open/fork a workspace. Generate a new mock with all of these embedded."

"after the generations are complete show the artifact inline (even after the phase is
complete) like we do today in the chat [the `chat-message-citation` chip]. Do this only
for the question-answering artifacts, explainer, mocks but not for ontology changes or
code since it will be too many artifacts."

"read only the user messages from guided_flow.jsonl then write a plan to implement this
that includes all the requirements we refined via mocks"

### Answers to the plan's questions (2026-09-01, in `guided_panel_implementation_plan.md`)

- Q1 journey state source: "Add a button to mark a step as complete if it needs client-observed steps".
- Q2 gates versus the two-stage model: "Map onto the existing two stages. Planning stage = system gate; integration gate = finalizing stage. Business gate we'll do later".
- Q3 rollout: "New capability flag, default off in staging + prod, enabled in dev by default".
- Q4 default-open persistence: "Remember the collapse per workspace".
- Also asked (2026-09-01): "give me some mocks for Inter-phase approval gates vs the existing two-stage model (question 2)" → mocks H, I, J (Q2 options; superseded by Q2's answer and later by phase-bound approvals, below).

### Flowchart variants (2026-09-08)

"create a new mock based on mock f/g where the side panel is a flowchart" → Mock K.
"create another version of this mock, just so I can compare different directions" → Mock L
(the session ran out of credits before Mock L was reviewed). No message chooses between
G, K and L; the plan names Mock G "the accepted latest".

## What changed between the source design and this port

- **2026-09-01 standup** (meeting notes): the guided side panel was shown and taken as
  "a stepping stone"; the same meeting judged the workspace desktop "the wrong UI model",
  and Tim asked for a UX revamp on 2026-09-03.
- **2026-09-08 offsite**: Excel-style ribbons for Business / System / Integration
  Engineering, one workspace per tab, a Home that flags what needs you; Operations did not
  make it into the mock. **2026-09-09**: "dual-mode UI strategy: guided templates for
  non-technical users and flexible chat-based interactions for developers"; the UI is a
  template provider and chat stays an equal escape hatch.
- **2026-09-16 UI meetings** and the UXv2 mock session (`session-prompts.json`,
  `ux-changes.jsonl`): the workbench mock (phase chevrons with approval meters, per-phase
  ribbon, rail Activity · Project docs · Ontology · Files · Issues & tasks · Jobs, chat that
  keeps its phase). **2026-09-25** (Tim / Ben): "merge to parent" is the last step of a
  workspace; meeting notes v1 is upload only.
- **2026-10-06 UI discussion** → the workbench now on `main` ([chat intent](../ux-v2-workbench/request.md),
  [workbench page](../../ontology/features/workbench.g5.md)): three phases as explicit
  workspace state, approvals bound to phases (stages removed), ribbon actions as templated
  chats, timeline landing, the `alignment` layout gate.
- **2026-10-07 standup**: every user-experience change is reviewed by Tim and Davide before
  it is committed; a mock before the change is the preferred form.

## Agent interpretation

- The source workspace produced a design, not code, so the port is a design: the
  [guided-journey plan](../../pr_ontology_diff_design/guided-journey/ontology-plan.md) and
  the workbench ontology blocks it adds. The user then asked for the implementation in the
  same PR; the 2026-10-07 review rule still requires Tim and Davide to review the UX against
  its mock before it lands.
- The Guided panel becomes a sidebar section of the workbench rather than a replacement for
  the status band: the band is gone under the alignment layout, the three phases are already
  workspace state, and the ribbon already owns the per-phase actions. Steps bind to ribbon
  action ids; Operations' three steps fold into Integration Engineering, which ends at merge.
- Q2's stage mapping is void: approvals bind to phases now, so each phase has its own
  roster. Q3 is kept as answered: the design review found that the alignment gate alone
  (on in QA) cannot keep Guided off in staging while the workbench stays on, so a separate
  `guided_journey` flag with Q3's defaults is added (`g5.workbench.journey.gate`). Q1 and Q4
  are kept.
- Mock G (timeline, always-expanded steps, chips, rosters, summary) is the accepted
  presentation; Mocks K and L (flowchart) were asked for "to compare different directions"
  and never chosen. The design pins G and names K/L as the open presentation choice for
  the human; the model, steps, rosters and chips are the same under all three.
