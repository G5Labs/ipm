# Design votes · 2026-10-09

A vote page for the week's design alternatives, built from three explainers
written by Claude sessions between 2026-10-06 and 2026-10-09 (the workbench
header styles, what a workspace tab switch must preserve, the six places where
the UX v2 plan contradicts the prompts), the real-app screenshots of PR #406
from the phase 8–9 page, and the four questions the 2026-10-09 standup left
open (review experience layout, center pane tabs, workspace tabs, previews on
another person's workspace). Every decision has a mock, a wireframe or a
screenshot beside its card; items marked *Real* are screenshots of the app.

- `design-votes-2026-10-09.html` — the page. Each decision is a question card
  (`data-g5-question` placeholder) that G5 renders inside the sandboxed
  explainer; the mocks and the context sit beside it. `__WORKSPACE_ID__` is
  replaced on publish.
- `questions.json` — the question set body (`POST /api/v1/workspaces/:id/questions`
  or `g5d question create --file`), with fixed question and option ids so the
  page can reference them.
- `publish.sh` — creates the set and commits the page as
  `explainers/design-votes-2026-10-09.html` in the workspace's docs repository.

## Publish

Run as the workspace owner while the workspace is running:

```sh
G5_JWT=... docs/design-votes/2026-10-09/publish.sh \
  --base https://dev.g5labs.ai --workspace <workspace-id>
```

Then open the document from Project docs → Explainers. Anyone who can read the
workspace votes in the cards with one press of Vote: the voter's name appears
under the option at once, and a vote can be changed or withdrawn any time; the
tally and the voters' names show on every card. The owner answers each
question from the same cards when the tally is in (`g5.documents.question.votes`).
