# Center surface URLs

Source: [issue #561](https://github.com/G5Labs/G5-Lean/issues/561) and
[chat intent](../../chat_intent/center-surface-urls/request.md) (2026-10-15). Follows
PR #559 (tab return) and PR #567 (org scope).

Every center surface gets a URL form, so a reload, a shared link or a new browser tab lands
on it. The tab strip's session snapshot (PR #559) already restores these surfaces within a
session; this design makes the address bar sufficient on its own.

## Ontology changes

| Block | Change | Why |
|---|---|---|
| `g5.workbench.tabs` (amended, UI) | Every center surface has a URL: content with an identity by path, singleton surfaces by `?view=`, Chat keeps the content URL with its conversation query. A job surface's route is written once the job is listed; until then the opener's route stands. Replaces "a center tab without a route uses the workspace root, except job outputs keep the current URL". | Issue #561: the previous sentence named the surfaces that could not be deep-linked. |

## Governing ontology

- [g5.workbench.tabs](../../ontology/features/ui.g5.md#g5.workbench.tabs): the route selects
  the active tab; a fresh open or reload follows the named route; a return restores the
  snapshot while the returned-to path and branch hint stay unchanged. This design gives
  the remaining surfaces a route to follow; the return and guard rules are unchanged.
- [g5.workbench.landing](../../ontology/features/ui.g5.md#g5.workbench.landing): a URL that
  selects a surface wins over the timeline landing. The new routes and values count as
  deep links there.
- [g5.ui.terminology](../../ontology/features/ui.g5.md#g5.ui.terminology): `?view=` values
  follow the tab labels.
- [g5.workspace.base-ui.chat](../../ontology/features/ui.g5.md#g5.workspace.base-ui.chat) and
  the Base gates: a `?view=` or path that names a surface the workspace does not offer
  (Diff Graph on Base, an unlisted job, an unknown guide or repository) opens nothing and the
  URL falls back to the workspace root, as unknown doc links do today.

## URL forms

| Surface | Tab type | URL (after `/:org/:workspace`) | Today |
|---|---|---|---|
| Pull Requests | `pull_requests` | `?view=pull-requests` | none |
| Progress overview | `workspace_plan` | `?view=progress` | none |
| Source Control (dirty diff) | `source_control_diff` | `?view=source-control` | none |
| Diff Graph | `source_control_change_graph` | `?view=diff-graph` | none |
| Guide doc | `guide_doc` | `/guide/:guideId` | none |
| External issue | `external_issue` | `/issue/:repositoryId/:number` | none |
| Commit diff | `source_control_commit_diff` | `/commit/:repoId/:sha` | none |
| Job output | `job_output` | `/job/:jobId[/task/:taskId]` | written only by the job or task click |
| Explainer output | `explainer_output` | `/job/:jobId` | written only by a task click or link |
| Job progress | `job_progress` | `/job/:jobId/progress` | none |

Path-backed (`spec`, `doc`, `file`, `intent-log`), `?view=` (Activity, Details, Approvals,
Review, Terminal, Settings, Workers, Members, approval definitions, GitHub app, admin) and
`?session=` (Chat) surfaces are unchanged. The Source Control tab's selected file and the
Workspace changes surface (a task without a job) stay without a URL: the first is a
transient selection inside the view, the second is opened from a task row whose route the
click already writes.

## High-level design

All changes are in `frontend/src/pages/WorkspacePage.tsx` and `App.tsx` routes, plus
`tabToPath` tests.

### Singleton surfaces: `?view=`

- The URL→tab consumer effect gains `pull-requests`, `progress`, `source-control` and
  `diff-graph`, each calling the existing open helper (`openPullRequestsTab`,
  `openWorkspaceImplementationTab`, `openSourceControlDiffTab()`,
  `openSourceControlChangeGraphTab`). `diff-graph` waits, unconsumed, until the workspace
  detail is loaded, since its helper refuses Base; on Base the hint opens nothing and the
  tab→URL mirror clears it.
- The tab→URL mirror maps the four tab types to those values and lists them among the
  center-view values it may clear. `syncUrlToTab` already writes the workspace root for
  these tabs, so the mirror produces `/:org/:workspace?view=...`.
- The Progress overview stays out of the reload record (`g5:tabs:<id>`); the URL is its
  reload path, the same as for Activity and Details.

### Identity surfaces: paths

- `tabToPath` returns `/guide/:guideId`, `/issue/:repositoryId/:number`,
  `/commit/:repoId/:sha`, `/job/:jobId[/task/:taskId]` (job output), `/job/:jobId`
  (explainer output) and `/job/:jobId/progress` (job progress). `syncUrlToTab`'s content
  branch then writes them with the tab-neutral query (branch kept, stale `session` and
  `tab` dropped) as it does for doc and file tabs.
- `App.tsx` adds the `/:org/:workspace/guide/:guideId`, `/issue/:repositoryId/:number`,
  `/commit/:repoId/:sha` and `/job/:jobId/progress` routes on `WorkspacePage`; the page
  reads them with `useMatch` like the spec/doc/file routes.
- The deep-link effect resolves each new route once per route key, like doc links:
  - guide: `guideDocTab(guideId)`; an unknown id falls back to the workspace root.
  - issue: an `external_issue` tab named by the workspace's matching link display key
    (waits while the links load), otherwise `#<number>`; a non-numeric number falls back
    to the root. Authorization stays with the panel's live read.
  - commit: waits for the branch list; the tab takes `repoName` from the branch with that
    `repoId` and is named by the short SHA; an unknown repository falls back to the root.
  - job progress: the existing `/job/:jobId` branch opens Job Progress instead of Job
    Output when the progress route matched. Stale and pending resolution are unchanged.
- The landing's deep-link check and the first-workspace getting-started auto-open treat
  the new routes as deep links, like a job path.

### Job surfaces: one writer, on request, once the job is listed

Writing a job route from `openTab`, `selectTab` or a close would race the job and task
clicks, whose own navigation carries history state (`taskQueueActivationJobId`) and hints,
and would reset an explainer launched from a document to the root while its job is not yet
listed (the `/job/:id` hydration treats an unlisted job as stale). The router location can
also lag a click by a render, so a writer keyed on the URL cannot tell a click's route from
a stale one. So:

- `syncUrlToTab` keeps a job tab's current path (unchanged), unless its caller asks for the
  tab's own route.
- The entry points that write no route of their own ask for it, by tab id
  (`jobRouteRequestRef`): a selection from the tab bar, the neighbour selected after a
  close or bulk close, every Job Progress open (`openTab` asks for that type, so an
  explainer's transcript control counts), a strip return that restores a job selection
  (the earlier request lived in the unmounted page) and an explainer open from the job list (the
  "explainer opened from a document" case: that open wrote no route, so the document's URL
  stayed). The job and task clicks never ask; they write `/job/:id` with state and refine
  it to the task route themselves.
- A small effect, beside the `?view=` mirror, is the only writer: when the selected tab is
  the requested job tab, Chat is not in the center and the job is in the loaded job list, it
  calls `syncUrlToTab` for the tab's own route (dropping a stale center-view hint) and
  clears the request. A request for an unlisted job stands until the list has it or
  another tab is selected, so an unlisted job never bounces to the root. A navigation away
  from the job route with the tab still selected is not rewritten back.
- A request records the path it was made on and bumps a serial so the effect runs even
  when nothing else changes (the selected job tab clicked again after a navigation to the
  root). A navigation to another path before the job list serves it (a link to another
  job or content) withdraws it, so the newer route stands. A query-only link meanwhile (a
  center-view hint, a conversation, the bare root) does not: the job route is then written
  once the job lists. Finer rules (withdraw on any new location key except the mirror's
  own hint clearing, ignore the page's own landings) handled those rare sequences but
  added three fields and a landing-key record to the request; simplicity was preferred
  (2026-10-15).
- The `/job/...` hydration then re-opens the same tab idempotently (same tab id, already
  selected), as it does after a job click.
- A content route resolves only once the workspace's tabs are restored
  (`tabStateWorkspaceId`): before that the selection still belongs to the previous
  workspace, and a route it already shows there (the same guide in another workspace)
  would resolve as shown while the restoration then clears it.
- The commit hydration keeps the opener's own tab (titled by the commit summary) when it
  already shows that repository and SHA, and replaces the singleton's identity otherwise.
- An explainer job's `/job/:id` is its Explainer Output. Its retained transcript (the
  job's Job Output tab) shares the job and so has no route of its own: its selection writes
  nothing, and the explainer is not opened over it.
- A mirrored `?view=source-control` names the Source Control tab already selected, with
  its selected file: the consumer shows it (Chat leaves the center) and opens it afresh
  only when it is not the selected tab.
- A content path owns the surface: a center-view hint beside a spec, doc, file,
  intent-log, guide, issue or commit path is stale (the mirror writes hints at the
  workspace root only). The `?view=` consumer drops it instead of opening that tab over
  the path's content, which also left the path's own navigation waiting for a URL that
  never landed. Like the mirror, that drop waits while a tab navigation is in flight: the
  path's own write (a doc route drops the branch hint too) is the URL that stands. Job
  paths keep their hints as before.
- A content tab's own URL write (`syncUrlToTab`) drops a center view hint itself, and the
  `?view=` mirror defers while a tab navigation is in flight (`pendingTabUrlRef` not yet
  landed): the router can lag a click by a render, and a mirror written from the stale
  route replaced the content URL (seen as a guide selection landing on the root).
- `/issue/:repositoryId/:number` parses the number like the issue page (decimal digits
  only) and finds the workspace's link with the same helper as the issue page controls
  (`lib/issuePage.ts`).
### Same-path links and refused singleton links

- A link to the content path the page already shows, minus the conversation hint it had
  (a query-only navigation), names the content: Chat leaves the center. An in-page action
  that drops the hint (closing Chat, a new chat, a tab selection) has suppressed that hint
  itself and keeps its own Chat placement; the route resolution key is unchanged, so this
  is a transition rule beside it, for every content route.
- A refused `?view=diff-graph` on Base and a stale `/job/...` route (job output, task or
  progress) land like a fresh open of the root (the timeline, naming its own hint), like
  the identity-route fallbacks: the landing counted the visit as deep-linked, and a
  persisted selection must not stand in for the link. The stale-job branch opens it once:
  it runs again until the root lands, and an open on every run re-renders faster than the
  navigation (a React transition) can land.

### Pending tab URLs and superseding navigations

A tab-URL write records the location key it was issued under. The deep-link effect waits
for it, and the `?view=` consumer and mirror defer to it, only until a different
navigation lands (a review copy's Review landing over a guide link, a link); that
supersedes the record instead of leaving the page waiting for a URL that never comes. A
center-view hint beside a content path counts as consumed at once, so a write that carries
it to the root meanwhile does not apply it there. The Review landing names its own
`?view=review` in its write, since it can open its tab and leave a content path in one
render, where the mirror holds back from a URL that just changed. Explicit links to a
singleton surface target the workspace root (the credentials banner's Settings link now
takes the page's root href; a relative `?view=` beside a content path is dropped).

### Return from the strip (PR #559)

Unchanged. The snapshot's remembered URL now names these surfaces, so the restored
selection and the path agree more often; where the job effect rewrites a remembered
content path to the selected job's route, the restored-selection guard expires on a path
the hydration resolves to that same tab.

### Shell chrome and the route registry

`shellRoutes.ts` holds the workspace page's routes by name (`WORKSPACE_ROUTES`): `App.tsx`
registers the page under every pattern from it and the page matches its named patterns, so
a route is declared once. The same registry classifies the new paths as workspace shell
routes, so the tab strip stays mounted on them and records them as the workspace's last URL
for the return (PR #559); `isWorkspaceShellRoute` keeps the bug-report trigger's workspace
context on them too.

## Known limits (reviewed, out of scope)

- A review copy's Review tab is selected once per page load on any fresh visit, over a
  doc, file, job or the new routes alike (pre-existing landing rule); only a strip return
  keeps the linked surface. Changing the review-copy landing is a separate decision.
- A `?session=` hint beside a `?view=` value or a `/job/...` route shows that surface
  with Chat hidden and the session selected behind it, as every `?view=` value and
  `/job/:id` already did; only a content path (spec, doc, file, guide, issue, commit) or
  the bare root keeps a `?session=` hint's Chat in the center. Parity is kept here.
- A job tab whose job is beyond the loaded page of the job list keeps the opener's URL
  until the list reaches it; writing it earlier would bounce to the root as stale. A
  reload of such a link would not resolve either.
- A linked issue or commit the live read rejects (404, revoked access) keeps its tab with
  the panel's own error and its URL, like a stale file or doc path.
- A query-only link at the root (a `?view=` or `?session=` hint) while a job route request
  waits for the job list: when the list arrives the job route is written over the link's
  hint, and the hinted tab (opened by the consumer meanwhile) may be re-selected by the
  job hydration. Contrived sequence; the simpler path-only withdrawal is kept.
- A link to the content path the page already shows is a no-op by the route key (as for
  doc and file paths): while a job tab picked on that path waits for its route, the link
  neither re-selects the content nor withdraws the request. Re-resolving on a new location
  key regressed the page's own same-path writes (every tab open re-writes its path with a
  new key) and was withdrawn.

## Tests

- `tabToPath` unit tests: new paths; the existing "returns null" assertions for the issue,
  commit, diff and Progress tabs are replaced.
- `WorkspacePageV2.test.tsx`: the Chat ⇄ guide keyboard test now expects the guide's path
  without the conversation hint while the guide is in the center; Chat's return re-attaches
  it (`handleSelectCenterChat`), as for any content tab.
- `shellRoutes.test.ts`: the new paths classify as workspace shell routes; the alignment
  suite renders a guide link with the strip mounted.
- `WorkspacePageAlignment.test.tsx` (route probe): each
  `?view=` value opens its tab and the mirror writes it on selection; Diff Graph on Base
  opens nothing and clears the hint; a guide, issue and commit deep link opens its tab and
  wins over the timeline landing; selecting a Job Output tab from the bar writes
  `/job/:id`; a Job Progress tab picked from the bar writes `/job/:id/progress` and a
  reload on that route opens Job Progress; an explainer's transcript control opens Job
  Progress with its route while the explainer's own transcript tab writes nothing; a job
  surface whose job is not listed leaves the URL alone (no bounce to the root) and gets
  its route after a strip return once the job is listed; a commit link under
  `?view=activity` still follows a later link; a mirrored Source Control hint keeps the
  selected file.
- e2e `workspace.spec.ts`: a `?view=pull-requests` deep link and a `/guide/getting-started`
  link land on their surface after `page.goto`.

## Acceptance conditions

- A reload on any center surface shows that surface (Base excepted for surfaces Base does
  not offer); a stale link falls back to the workspace root, never a wrong surface.
- Job and task clicks keep their current URL, history state and hints.
