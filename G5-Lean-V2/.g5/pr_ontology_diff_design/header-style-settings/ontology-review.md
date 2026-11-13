# Review — workbench header styles

Scope: the branch `feat/header-style-settings` against the [design](ontology-plan.md),
[chat intent](../../chat_intent/header-style-settings/request.md) and
`g5.workbench.header-styles`, `g5.workbench.gate`, `g5.workbench.actions`.

## Self-review (agent)

- The vertical phase column left the ribbon content-sized, so its bottom border stopped
  short; the ribbon now fills the row (and Base, which has no ribbon, gets a bordered
  filler). Fixed before the independent rounds.
- Suites that build their own instance-info payload assumed the old absent-value fallback
  (`both`); they now pin `defaultWorkspaceLayout: "both"` explicitly, as CI does, and the
  `alignment` fallback is covered by `lib/topPanelFlags.test.ts`.

## Independent review, round 1 (Codex, two parallel runs, no prior findings)

Correctness run — CHANGES_REQUESTED:

- major · `ActionRibbon.tsx` extended-icons row used `overflow-x-auto`, which clipped the
  absolutely positioned dropdown menus (reproduced in a browser). Fixed: every style wraps;
  no row scrolls.
- major · `workbenchIcons.ts` read `BY_ID[action.id]`, so a committed action id such as
  `constructor` resolved to an inherited property and crashed the render. Fixed: both
  lookups are `Map`s; `workbenchIcons.test.ts` covers prototype-named ids and the
  id → target → kind fallback order.

Duplication run — APPROVE with one note: the settings page repeated the select class list;
consolidated into `settingsSelectClass(width)`.

## Independent review, round 2 (Codex, same prompt, fresh state) — CHANGES_REQUESTED

- major · uncaptioned ribbon groups did not wrap internally, so one large committed group
  could push actions past the viewport (reproduced at 1024px with IE's actions in one
  group). Fixed: the group container wraps too.
- minor · the feature-off settings test pinned the instance default to `both`, so the
  design's acceptance condition (a valid option when the default is `alignment` without
  `alignment_ui`) had resolver coverage only. Fixed: a settings component case advertises
  default `alignment` without the feature and asserts `advanced`, no `alignment` option and
  no style selects.

## Independent review, round 3 (Codex, same prompt, fresh state) — CHANGES_REQUESTED

- minor · in `minimal-icons` a disabled action's tooltip showed only the reason, hiding the
  action's name (same for the preview trigger). Fixed: icon-only tooltips read
  `label — reason`.

## Independent review, round 4 (Codex, same prompt, fresh state) — APPROVE

No actionable defects; browser checks across every style combination, phase and three
viewport widths passed. The approvals-tab timing case failed once under load and passed in
isolation (see "Flake observed").

## Flake observed

`WorkspacePageAlignment.test.tsx` "the ribbon's Approvals action opens the tab narrowed
to the ribbon phase" failed once under the full-suite load (the `?phase=be` navigation
landed after `waitFor`'s window) and passed three consecutive isolated runs and the full
alignment suite; the independent reviewer saw the same pattern. The action's handler and
test id are unchanged by this branch.

## Unexecuted

- `e2e/flows/alignment-shell.spec.ts` (new scenario "Phase bar and Action bar settings
  restyle the live workbench header"): no e2e stack was available locally; CI `g5-ci-e2e`
  runs it with the alignment gate on.
- Backend: only `routes/instance.rs` changed; its unit test ran green. The full backend
  suite was not run.
