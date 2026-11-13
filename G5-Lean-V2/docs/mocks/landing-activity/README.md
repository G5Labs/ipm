# Workspace landing and Activity cleanup mocks

Standalone design mocks for [the existing landing plan](../../../.g5/pr_ontology_diff_design/issue-816-workspace-landing/ontology-plan.md#activity-cleanup-and-filter-proposal--october-25-follow-up).

- `index.html`: full landing, Current / Proposed, with the surrounding page controls retained.
- `filters.html`: focused interactive filter example, with state shortcuts.
- `mock.fragment.html`: the full-page document's editable fragment for an HTML reader.

All files are standalone and use sample data. They make no G5 or GitHub requests.
Open the HTML directly or serve this directory. Change Current / Proposed, scenario
and theme. Click the activity pills and phase selector; click an event title or
timestamp to inspect a sample reader. Mock-only controls are above the app frame.

Scenarios cover the original DEMO-2 issue/PR, mixed activity, ongoing work, review
replies, an issue without a PR and GitHub source provenance. The focused mock has
shortcuts for All, Tasks, Tasks + Approvals, no matching history and pinned ongoing
work. In Proposed, only All is highlighted in the initial state. In Current, every
category is highlighted initially and clicking Tasks excludes it, reproducing the
reported problem.

The Start buttons, Merge to parent buttons, toolbar, Guided summary and step list,
phase chevrons, sidebar and chat controls are excluded from the proposal. Their
repetitions deliberately remain in the full-page mock. These standalone mocks run
independently of the production frontend and live preview fixtures.

The owner-approved October 26 refinement boxes person-written messages: issue and
workspace comments, replies, and approval decisions with a reviewer comment. Events,
including uncommented decisions, stay plain; Current retains its historical boxed
issue-opening example.

The Project Docs exports live in the workspace's docs repository at
`docs/72ede771-1551-4407-9332-73aeafad1eb1/`. The Markdown specification is a reader
copy of the continuing design's follow-up, not an independent design plan. The HTML
exports embed all CSS and JS so they work in the Project Docs HTML reader without
relative-asset assumptions.

## Validation

Run `node docs/mocks/landing-activity/verify.mjs` from the repository root after
installing the frontend's Playwright dependencies. The check opens local HTML with
Chromium and exercises filter transitions, phase filtering, empty results, Current
pinning, review replies, readers, both themes, responsive widths and preservation of
surrounding controls. It writes PNG reference states to `snapshots/`.

These checks validate the mocks only. The approved cleanup is implemented in the
application; production regression checks and their scope are recorded in the
[continuing landing plan](../../../.g5/pr_ontology_diff_design/issue-816-workspace-landing/ontology-plan.md).

Executed with local Playwright Chromium on October 26, 2026: all checks above pass,
including keyboard focus, both themes at 320/390/900/1440 pixels, no document-level
horizontal overflow and no remote network calls. Captures: `landing-current.png`,
`landing-proposed.png` and `filters-combined.png` in `snapshots/`.
