# Center surface URLs — request and interpretation

## Human instructions (2026-10-15)

- "Let's solve https://github.com/G5Labs/G5-Lean/issues/561": every center surface that
  today has no URL form (Pull Requests, Progress overview, guide docs, external issues,
  source-control diff, diff graph, commit diff, job progress; job output re-selected from
  the tab bar; an explainer opened from a document) must be reachable from a reload, a
  shared link or a new browser tab. The issue proposes `?view=` forms for all of them and
  `/job/:id` for job output and progress selected from the tab bar.

## Agent interpretation

- The issue's URL forms are examples ("e.g."). Surfaces with an identity (guide, issue,
  commit, job output, job progress, explainer output) take a path, like spec, doc, file
  and job routes do today; singleton surfaces (Pull Requests, Progress, Source Control,
  Diff Graph) take a `?view=` value, like Activity, Approvals and the admin tabs. The
  path/query split already exists; the `?view=` sync compares one string, and extending it
  with identity parameters would add cleanup rules the path route machinery already has.
- Job surfaces write their route once the job is in the workspace's job list, because the
  `/job/:id` hydration resets an unlisted job to the workspace root. An explainer launched
  from a document therefore keeps the document's URL until its job is listed, then names
  the job.
- `?view=` values follow the tab labels (`source-control`, `diff-graph`,
  `pull-requests`, `progress`) rather than the issue's shorthand (`diff`, `graph`).
- A deep-linked commit tab is labelled by its short SHA (the summary is not in the URL);
  a deep-linked issue tab is labelled by its link's display key when the workspace links
  it, otherwise `#<number>`.
