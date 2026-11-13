# Question votes: design request

Branch `feat/question-vote-page`; comparison base `origin/main` at `9efa4c27` (after PR #409, "Add the question store").

## Human instructions

Request (2026-10-09):

> Now that PR #409 merged, we have a mechanism to have questions and answers. Take a look at all the html files written by claude sessions in this folder this week. They are showing some kind of alternative between options. Let's create a html page, that we can upload in the platform, where everyone can vote on the options in the platform itself. If we need to augment the platform let's also do that in addition to the html page

## Agent interpretation

- "Everyone can vote" means every person who can read the workspace, not only
  its owner. PR #409's store lets only the attached owner answer and shows
  readers committed cards read-only (`g5.documents.question.readers`), so the
  platform needs a vote record beside the owner's answer. The agent chose a
  durable platform record outside the repository, on the precedent of thread
  replies (`g5.documents.comments.replies`): a reader write that changes no
  repository content and needs no running VM.
- A vote is not a decision. The owner's Answer stays the one decision
  (`g5.documents.question.decision`); the tally informs it.
- The HTML page is a managed explainer (`explainers/<name>.html` in the docs
  repository) holding `data-g5-question` placeholders, so the platform's own
  cards render inside it and votes are confirmed in the host bar like answers
  (`g5.documents.question.html`). Superseded for votes by the 2026-10-11
  follow-up below: one press records a vote.
- The HTML files inventoried (scratchpad pages written by Claude sessions
  between 2026-10-06 and 2026-10-09): `workbench-header-options.html` (phase bar
  and action bar styles), `one-mounted-page.html` (one mounted workspace page
  versus every tab mounted), `ux-v2-plan-vs-prompts.html` (six contradictions
  and five gaps between the UX v2 plan and the prompts), `gallery/index.html`
  (Aero Glass mock, already shipped as a theme in PR #384, so no question),
  `home.html` (empty "Checklist Design" stub, no question) and
  `ux-v2-phase8-9-screenshots.html` (screenshots, no alternatives). The vote
  page carries the first three as question cards with their visuals.

Follow-up (2026-10-09):

> can you read also the latest files in ~/Downloads and add mocks or actual screenshots about all the decisions we need to take?
> only the most recent file in ~/Downloads

Agent interpretation: the most recent file is the Gemini notes of the 2026-10-09
G5 Daily Standup. Its decided items (drop Redis, minimise the ribbon for
non-owners, investigate cloud worker performance) are not questions; its open
items became four more cards (review experience layout, center pane document
tabs, workspace tabs, running previews on another person's workspace). Every
card gained a visual: the real-app screenshots from the phase 8–9 page (PR #406
against the mock API) where the app already shows the thing, and HTML
wireframes where it does not.


Follow-up (2026-10-11):

> b) avoid confirmations in the host bar so you need to click only once, even if there are platform changes

Agent interpretation: one press on an explainer card's Vote records the vote.
That is a platform change to the host bridge (`g5.documents.question.votes`,
`g5.documents.question.html`): the host still validates the frame's
`propose-question-vote` message and then applies it with no confirmation bar;
the bar appears only for a refused vote, with the reason and a retry. Answers
keep the host-bar confirmation. Part a of the same request (one decision per
page on the vote page, with previous and next buttons) is deferred to a later
change.
