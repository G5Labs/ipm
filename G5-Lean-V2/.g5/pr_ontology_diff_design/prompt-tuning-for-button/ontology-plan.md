# Code review loop option and Re-run Verification — plan

Governing: [g5.workbench.review-prompts](../../ontology/features/ui.g5.md#g5.workbench.review-prompts) (new), [g5.workbench.ship-actions](../../ontology/features/ui.g5.md#g5.workbench.ship-actions) (amended), [g5.workbench.chat-actions](../../ontology/features/ui.g5.md#g5.workbench.chat-actions) (unchanged; seed lifecycle). Intent: [chat intent](../../chat_intent/prompt-tuning-for-button/request.md).

## Design

- **Prompts.** `components/workspace/reviewWorkspace.ts` already owns the one review prompt for every review entry point, and it gains the verification prompt beside it. Keeping both in the frontend leaves the move to a shared prompt store to [issue #1034](https://github.com/G5Labs/G5-Lean/issues/1034).
  - The review prompt takes the iteration count. A pure helper rewrites the count in an existing draft, so the person's other edits survive.
- **Header on the seed.** The composer seed (`{token, text}`) gains an optional typed `header`, either `review-loop` or `verification`.
  - The header lives on `ChatComposerDraft` next to `seedPulse` and follows the same lifecycle: set when the seed is applied, cleared when the draft is sent or emptied, restored with the draft when a send is refused, and kept across remounts (Dock, Pop out).
  - This reuses the existing seed owner instead of adding page state.
  - `ComposerSeedHeader` renders it at the top of the fresh chat: above the landing in the center layout, under the panel header in the side layout.
- **Re-run Verification.** It is a new `open` target, `verification`, mirroring `review`, so no persona or templated start is involved.
  - The backend enum (`WorkbenchActionTarget::Verification`), the ribbon's known targets, an icon, and the IE Release entry before Create PR in `actions.yaml` (regenerated fallback).
  - `WorkspacePage` seeds it like `review` and gates it the same way.
- **Rejected alternative:** a `kind: chat` action keyed by its id. It would prepend the IE persona and would tie the notice to an id that a committed `.g5/workbench/actions.yaml` can drop or reuse.

## Assumptions

- `$HOME/.agents/guidance/TLC/pr_incremental_TLA_reverification.md` exists in every workspace VM. The harness installs the whole guidance directory into the agent's home (`backend/g5d-agent/src/harness.rs`, `write_guidance`).
- A workspace's committed `actions.yaml` that leaves out `rerun-verification` simply has no such button, as for any built-in.

## Verification

- **Unit tests:** the prompt and rewrite helpers.
- **ChatPanel:** header lifecycle (shown with the seed, N rewrites the draft, removed by send or empty, restored on a refused send).
- **ActionRibbon / WorkspacePage alignment:** the button order and seeding for both targets.
- **Backend:** the `workbench_config` and `api_workbench` tests, plus the regenerated-fallback drift check.
- **End to end:** a real-browser check of both buttons against the local preview where it can run.

## Results (2026-11-01)

- **Passed:**
  - Unit and page tests: `reviewPrompts`, `ChatPanel`, `WorkspacePageAlignment`, `WorkspacePageV2`, `ActionRibbon`.
  - Backend: `workbench_config`, including the fallback drift test.
- **Real Chromium check** against the in-browser mock app (`VITE_MOCK=true`):
  - Code review seeds the prompt with the option at the top. Choosing 3 rewrites "loop of 3 iterations", and emptying the draft removes the option.
  - Integration Engineering's ribbon reads … Review meeting, Re-run Verification, Create PR, Merge to parent. Its seed shows the bold notice above the prompt.
  - The mock ignores phase changes, so the check pinned the phase in its own harness. The mock also cannot send, because its launch-settings route returns 404.
- **Not run:** a send from the seeded chat against the full-stack preview. The send path itself is unchanged.
- **Label width (owner choice: a wider button):** the icon ribbon wraps labels anywhere at 76 px (60 px compact), which split "Verification". Re-run Verification alone is 96 px (80 px compact). In Chromium it reads "Re-run / Verification" at 1440 px and at the compact size (1024 px); every other button keeps its width.

## Organization switch for Re-run Verification (2026-11-01 follow-up)

Governing: the Re-run Verification bullet of [g5.workbench.review-prompts](../../ontology/features/ui.g5.md#g5.workbench.review-prompts) (amended) and the Admin order in [g5.ui.settings-page](../../ontology/features/ui.g5.md#g5.ui.settings-page) (amended). Owner decisions: greyed, off by default, a Workbench actions section on the Admin page.

- **Reuse.** The switch is a third entry in `ORG_ONLY_BOOLEAN_SETTINGS`, beside `external_issues_enabled` and `external_checks_block_merge`. The existing org config route already gives the required authority: members read it (`workspaces:read`) and only current organization administrators write it (`org:admin`, rechecked against the current role). The registry refuses it at workspace and personal scopes and refuses a non-boolean value. No new table, column or route.
- **Client.** `lib/verificationActionFlags.ts` reads the switch as on only when it is exactly `true`. `WorkspacePage` reads the organization's config once the session has adopted that organization, the same rule as the Issues switch, and the `verification` action state greys the button with the off reason ahead of the writable/chat reason. Loading or a failed read leaves it greyed, which matches off by default.
- **Admin.** `OrgWorkbenchActionsSection` adds a Workbench actions section, after Issues and before Defaults, with one Re-run Verification row and an On/Off switch modelled on the Issues switch.
- **Assumption.** The switch is a UI gate. It is not a policy on agent work: the guidance file still ships to every VM and anyone may ask for the same run in their own words. Enforcing that would be a separate, much larger design.
- **Not changed.** Code review, the other ribbon actions, and the committed `.g5/workbench/actions.yaml` override all behave as before. An action dropped by a repository's own configuration still has no button at all.

### Verification

- Unit tests for the flag, the Admin section (default off, writes on and off, error state), and the Admin index order.
- `WorkspacePageAlignment`: greyed with the reason by default while Code review is available, and seeds when the organization has it on.
- Backend: the `config_service` registry test, and an HTTP test against real Postgres. In that test, a member's write is forbidden, an administrator's write is stored, a member reads it, a non-boolean is refused, and personal and workspace scopes are refused.
- Results (2026-11-01): all of the above pass, and the full frontend suite passes (5,635 tests). In Chromium against the `frontend-only` preview (mocked API), Re-run Verification starts greyed with the off reason. Turning the switch on in Admin → Workbench actions enables it without a reload, and clicking it seeds the prompt under the bold notice. Full-stack end-to-end runs were skipped by owner decision: the change is one existing-route setting plus UI, and the HTTP test covers its authority.
- Preview fix (owner request, 2026-11-01): the `frontend-only` preview's mocked API echoed a phase change without keeping it, so the phase chevrons snapped back to Business Engineering after the refetch (unchanged from `main`). The shared MSW handlers now store an accepted phase per workspace and return it on later detail reads, reset after every test (`test/msw/workspacePhase.test.ts`). With it, the restarted preview switches all three phases with no harness shim. Code review seeds its loop option, Re-run Verification starts greyed (96 px) and enables once the Admin switch is on, and the full suite passes (5,637 tests).
