# Onboarding tour — request and interpretation

## User instructions (2026-10-26)

> Let's add an onboarding flow with popups that highlight which buttons to press. Add one more
> parameter to the user objects along the lines of userHasCompletedTutorial, which stores what's
> the max step the user completed/skip, so we can also check how many users skip the tutorial.
> If !userHasCompletedTutorial, default the homepage workspaces/issues/compute panels to all
> instead of just mine. Build this on top of PR #852 assuming that's merged.

The user then asked for a web search on onboarding storytelling, rewrote the agent's 15-step
proposal into a 6-step action-driven flow (Settings first, then the personal scratchpad chat
creating the first issue, then Start workspace, then Guided), and settled the copy in chat.
Decisions given in that exchange:

- Drop "the scratchpad can run the code" from the Home step: it might suggest the personal chat
  can change the code.
- The provider-accounts instance feature is always on; do not gate the Accounts step on it.
- "Harness" is too technical: the composer step says "pick a model and account".
- Every organization will have an issue tracker; no tracker-less branch.
- Viewers have no personal workspace (agent's answer from the personal-workspace ontology), so
  viewers get the three Settings steps and a closing card.
- Resume until completed or skipped, which needs skip distinguishable from leaving.
- The welcome card auto-starts only on the organization Home.
- "when you are done, before the codex reviews, open the PR, then start the codex reviews in the
  background, then just dev-up so I can inspect manually".
- (2026-10-27, on the running stack) "can we put something to highlight the setting icon more in
  step 1? Otherwise skip tour seems the only option" — the agent searched the web (Atlassian
  spotlight pulse, Appcues beacons, Chameleon's gentle pulse) and added a pulsing ring, a card
  arrow and an "Open Settings" button on the welcome card.
- "also after login the first page shown should be the homepage".

## Agent interpretation

- Progress lives on the user record as two fields: the highest step reached (`onboarding_step`,
  null until the welcome card is shown, 7 when finished) and `onboarding_skipped`. Completion is
  step 7; the Home tiles default to All until then.
- One tour controller at the application root renders a spotlight over the current step's
  control and follows the person across Home, Base's settings view, the issue page and the new
  workspace. A paused step on another page shows a "Continue tour" pill that navigates there.
- The research the user asked for (short tours, action per step, checklist hand-off) is applied
  through the user's own rewrite; the design doc records the sources.
