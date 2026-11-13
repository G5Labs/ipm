# Onboarding tour — design

Request: [chat intent](../../chat_intent/onboarding-tour/request.md). Governing blocks:
[g5.user.onboarding](../../ontology/features/users.g5.md#g5.user.onboarding),
[g5.ui.onboarding-tour](../../ontology/features/ui.g5.md#g5.ui.onboarding-tour),
[g5.ui.home-workspaces](../../ontology/features/ui.g5.md#g5.ui.home-workspaces) (amended default),
[g5.ui.settings-page](../../ontology/features/ui.g5.md#g5.ui.settings-page) (replay row),
[g5.personal.home](../../ontology/features/personal_workspace.g5.md#g5.personal.home) and
[g5.personal.start-and-send](../../ontology/features/personal_workspace.g5.md#g5.personal.start-and-send)
(the scratchpad composer the tour seeds), [g5.ui.guided-panel](../../ontology/features/ui.g5.md#g5.ui.guided-panel)
(the hand-off target).

## Owners and facts

- **Progress** is a fact about the person, not the browser: the user record owns `onboarding_step`
  (smallint, null = never shown, 0–6 = highest step reached, 7 = finished) and
  `onboarding_skipped` (boolean). The existing `PATCH /users/me` carries both; the service never
  lowers the step (`GREATEST`), so a replay cannot erase the record, and clears or sets the flag as
  sent. Two columns rather than a sentinel because the user chose "resume until completed or
  skipped", which needs skip and leave told apart for the resume rule while the step still answers
  "where do people drop out".
- **The tour** is one client controller mounted above the routes (`OnboardingTour`), fed by the
  auth context's user. It decides the active step from the record, the route and the page's
  mounted controls; it never persists anything itself beyond the record. Page components only
  expose stable anchors (`data-testid`) and one seed (the personal composer's prefilled message);
  no page knows the step list.
- **Home defaults** derive from the record: `onboardingStep < 7` (null included) turns the
  Workspaces, Issues and tasks and Running compute tiles to All when the URL names no filter.
  The URL keeps omitting the person's default, so a copied link shows the recipient's default.

## Steps (copy is the contract; see the ontology block for the rule)

| # | Page | Anchor | Advance |
|---|------|--------|---------|
| 0 | Home | strip Settings button; backdrop blocks everything else except Skip | the route reaches a settings view |
| 1 | Base settings | Theme and Mode rows | Next |
| 2 | Base settings | Accounts section; 3b spotlights Add account and holds Next until a user-scope account exists | Next (tour navigates Home) |
| 3 | Home | personal chat composer | Next |
| 4 | Home | agent options row then Send; composer seeded with the issue message | an open issue whose title carries the person's marker appears after the step began (polled search, 5 s); a timeout or chat error reveals "Or create it yourself" at Create |
| 5 | Home → issue page | the issue's title link, then Start workspace | the route reaches a workspace |
| 6 | workspace | rail Guided row, then the first step's action button | that press, or Done |

Viewers: steps 0–2, then a closing card whose Done records 7. Replay (Settings → Appearance →
Tour) clears `onboarding_skipped`, sets the client cursor to 0 and navigates Home.

Resume: the controller shows the step whose index is the record's value whenever the record is
below 7 and not skipped; when the current page is not that step's page it shows a "Continue
tour" pill that navigates there (step 6 accepts any workspace page). Desktop only: the phone
layout has no strip, so nothing renders below the desktop breakpoint.

## Choices

- The welcome card blocks the page except the Settings button: the user asked for it; it is a
  forced-action step, which the cited research favours over "look here, Next".
- Issue detection polls the organization's issue search for the marker rather than parsing the
  agent's reply: the search is the same read the Issues tile uses, so the tile and the tour agree,
  and the agent's wording is not a contract. Only issues created after the step began count, so a
  replay does not match last time's issue.
- The tour does not join the dialog stack: joining it would trap focus inside the card while the
  control to press sits outside it. Instead it swallows pointer events outside the spotlit
  controls, the card and anything rendered outside the app root (the pickers' popovers portal
  to the body), cycles Tab between the card and the spotlit controls, and hides itself while a
  covering overlay (a modal) is open so the dialog stays usable.
- Progress writes count only when the server answered; a failed step write is retried by the
  next step's write, Skip and Done retry once. Replay and issue-baseline state in session
  storage is keyed by user id, so nothing of one person's run reaches the next sign-in.
- Viewers cannot add provider accounts (admission needs workspace write), so their Accounts
  step informs rather than gates.

## Codex review round 1 (2026-10-27)

Two independent read-only runs found: the Rust-side `max` merge racing concurrent writes
(fixed: `GREATEST`/`COALESCE` in the UPDATE), picker popovers blocked (fixed: portals outside the
app root pass), the Create fallback stranding the step (fixed: the issue page advances the step
and the baseline survives navigation and reload), the collapsed scratchpad opener blocked
(fixed: its own anchor), replay state leaking between people (fixed: per-user keys and a reset
on identity change), writes counted before success (fixed), viewers gated on an account they
cannot add (fixed), and missing keyboard focus handling (fixed: focus on the card, live text,
Tab ring).

## Sources consulted (user request, 2026-10-26)

Produktly 2026 benchmarks (completion by step count and start type), Chameleon "550M data
points" (cap at four steps, click-triggered and checklist-launched tours), NN/g mobile tutorials
and instructional overlays (short tips at first encounter; tutorials do not improve task
performance), Userflow and Appcues on action-driven tours. Applied as: two short chapters, an
action on every step, hand-off to the Guided checklist.

## Tests

- Backend: `api_users` covers the new fields on GET, monotonic PATCH, skip flag round trip and
  validation of out-of-range values; migration test for the columns' defaults.
- Frontend: unit tests for the step resolver (branching on viewer, org accounts, route), the
  controller's persistence calls and resume pill, and the Home tiles' default switch.
- E2E (dev stack): a fresh user sees the welcome card on Home, Settings advances, theme and
  accounts steps, back on Home the composer is seeded, Skip records the step, Home tiles show All
  before completion and Mine after; the agent-driven issue creation is exercised manually
  (dev-up inspection) because its outcome depends on a live agent.

## Codex review round 2 (2026-10-27)

Found: a progress retry could run under the next person's token (fixed: writes carry the
principal and a retry after an identity change is dropped), the Tab ring left out the pickers'
portalled popover (fixed), the GitHub title link leaves G5 (fixed: step 6 spotlights the row's
own Start workspace action, on the issue this run detected), a quick Skip restarting the welcome
card before the server answered (fixed: local dismissal per person), Create & Fork from the
fallback reaching a workspace at step 5 (fixed: that transition advances too), an owned issue's
page blocking every control (fixed: the issue page's card does not block), and a resumed step
in another tab losing the issue (fixed: the baseline and the detected issue live in local
storage per person; another device still starts its own baseline, accepted). Also fixed from
manual inspection: Replay pressed on the settings view started on the Theme step.

## Codex audit of the fix commits (2026-10-27)

Two independent audits judged the round fixes and the merge with main sound (checksum
registry, sqlx cache, ontology reference, CI seed) and left four defects, fixed here: step 6's
Home targeting now resolves the matching issue row and spotlights its own Start action (never a
title link that opens GitHub), falling back to the whole tile when the row is paged away so
Show more and search stay usable; the keyboard ring deduplicates controls reached through an
enclosing target; a progress response arriving after the person changed or after a later
write is ignored, so the client record never moves backwards; the local e2e seed marks the
shared fixture user as onboarded like the CI seed. Noted, not changed: the zod schema and msw
do not enforce the 0–7 range the backend does; a dev database that applied this branch's
former migration 117 needs its migration row reconciled before main's 117 applies.

