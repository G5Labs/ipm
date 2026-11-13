# Workspace landing for non-stacked PRs (issue #816)

Source: [G5Labs/G5-Lean#816](https://github.com/G5Labs/G5-Lean/issues/816) and its
[reviewed mocks and specification](https://review-f8e66463ff2c.hariharan-rahul.chatgpt.site/implementation-spec.md).
Stacked PRs are deferred to [#818](https://github.com/G5Labs/G5-Lean/issues/818).

## Ontology changes

- [ui](../../ontology/features/ui.g5.md): `g5.workbench.landing` now names the landing
  composition (summary line, Workspace task, Implementation description, Activity with
  pills) and leaves the phase to the chevrons. New `g5.workbench.landing.summary`,
  `.task`, `.implementation` and `.status` own the details. `g5.workbench.review-prototype`
  keeps the vector table for history and retires the landing's "PR submitted" popup;
  `g5.workbench.gates` admits the summary/Activity links to the one Approvals destination.
- [git](../../ontology/software_architecture/git.g5.md): new `g5.git.pr-description` owns
  the canonical G5 PR description and its comparison with provider bodies.

Governing blocks kept as they are: `g5.git.pr-providers`, `g5.git.pr-write-identity` and
`g5.git.save-publication` (create/update and their per-repository outcome reporting stay in
Prepare to Ship and Pull Requests), `g5.issue.primary`/`g5.issue.references` (primary and
related links), `g5.issue.read-failure`, `g5.ui.activity-current` (Current rows are never
filtered) and `g5.workspace.review-revisions` (saved-base counts).

## Owners and data

| Landing element | Source (existing unless noted) |
|---|---|
| Approval summary | `GET …/approvals` projection already loaded by the page (`outgoing.requirements`) |
| Changed-file counts | `review-flow/changes?mode=base`, the same query and kinds as before |
| Task title/body | primary issue detail, else workspace description, else docs README |
| Linked issues | `GET …/issue-links`, `role = related`, with the live summary each link carries |
| G5 PR, lifecycle, repository rows | `GET …/pull-requests?role=source&status=all`: open PR else latest; `sourceHeads` and `providerRepos` |
| Description, comparison, rendering, provider status | **new** `GET …/pull-requests/:id/description` |

The new read belongs to `WorkspacePullRequestProviderLifecycleService`, the owner of the
publication serializer, so expected bodies come from the same `build_provider_pr_payload`
used to publish (override, else Project Doc, else commit-derived, plus provenance). For each
provider repository with a PR number it reads `GET /repos/…/pulls/N` once with the
`full+json` media type (state, merged, draft, raw body, `body_html`) through the existing
repository credential path (`ReadPullRequests`), and renders the G5 description with
`POST /markdown` (`gfm`, context = the single provider repository; no context for several).
It stores nothing and writes nothing. A PR with no provider publication returns the
description without provider calls.

Rejected alternatives:

- Persisting the published body or its digest at publication time: adds a lifecycle and
  still needs a live read to notice GitHub edits.
- Reusing the general `pull-requests/info` read: it waits for GitHub's mergeability
  computation and returns no rendered HTML.
- Widening the review-flow `pr` (open only, used by approval requests) to closed/merged PRs
  would change approval-request semantics; the landing uses the PR list instead.

## Assumptions and acceptance conditions

- Non-stacked means the presentation of one G5 PR with at most one provider PR per
  repository; the model has no PR dependency yet, so nothing is classified as a stack.
- Provider statuses come from the live read; when it fails, the recorded status maps
  `open`/`merged` and anything else is Unavailable. Closed or draft comes only from the
  live read.
- A provider body G5 published through the legacy retry after GitHub refused the
  derived body (422) matches: the comparison accepts the serializer's preferred body
  and, for a derived one, its legacy retry body.
- Rendered GitHub HTML is sanitized in the browser with GitHub's own allowlist (the Files
  preview schema); links open outside the app.

## Current-main integration — October 26

Preserve the repository browser, change selection and Save checkpoint contracts
alongside the new landing. Its summary counts the complete pushed/base comparison,
independently of Source Control filters, and its total opens exactly those captured
pushed heads. Category links retain their ordinary reader destinations. This
reconciles the earlier plain Source Control link with issue #750. The implementation
keeps the recorded PR repository rows and Commit History in that PR context;
selected complete-vector history comparisons open in Source Control. Preserve route
recovery for expired persisted tabs together with same-chat reopening, and retain
both landing and Source Control preview fixtures.

The later main advance adds provider-owned frozen transport, native catalog and
shared chat rendering contracts. Activity selects the identified session without
reinterpreting its provider configuration or transport. Those type additions are
compatible with the separate PR-description DTOs. Rebuild the preview backend,
process worker and frontend using that revision's exact provider CLI pins.

Main `243598eb` also adds personal chat-result inspection, activity ordering and
per-chat prompt history. Preserve those contracts and their visibility-aware
acknowledgement when Activity reopens a chat. Its transcript DTO now exposes the
producing task and attempt, so reply waits use that authoritative task identity
after the captured input and before the next human input, including native-question
continuations, without a second journal read. No task/implementation presentation
or Activity message-box rule changes during this integration.

## Activity cleanup and filter proposal — October 25 follow-up

Status: implemented following the owner's October 25, 2026 authorization and review
of the published mocks and Project Doc. The owner explicitly confirmed the filter
interaction captions are mock-only and requested no extra redundant product text.
This continues the landing design above; the surrounding-control exclusion remains.

### Scope and human direction

The owner identified repeated links to Approvals and the primary issue, and wording
that incorrectly describes all Activity as issue discussion. They asked for a full
redundancy audit and a filter where clicking Tasks from All shows Tasks immediately.
They then requested mocks and a detailed description in Project Docs, explicitly
excluding removal of surrounding page controls.

**Excluded:** the Start buttons, toolbar actions, Merge to parent buttons, Guided
summary/steps, phase chevrons, surrounding navigation and chat controls. The full-page
mock deliberately retains their existing repetitions in both Current and Proposed.
The former audit's “surrounding page controls” recommendation is withdrawn from this
proposal. This exclusion also applies to narrow-screen behavior.

**Included:** landing content, Activity heading/filter area, activity rows and the
sample-data prose that duplicates their existing information. Substantive authored
comments and PR descriptions are preserved; the prose cleanup applies to our seeded
examples, not automatic rewriting or heading stripping of arbitrary user Markdown.

### Review artifacts

- Editable mock: [full page](../../../docs/mocks/landing-activity/index.html).
- Focused mock: [filter interaction](../../../docs/mocks/landing-activity/filters.html).
- [Mock source, scenarios and validation](../../../docs/mocks/landing-activity/README.md).
- The matching standalone HTML exports and a reader copy of this follow-up are
  published in this workspace's Project Docs under `mocks/landing-activity/` and
  `workspace-landing-activity-design.md`. These are review artifacts, not app entry points.

### Content and navigation changes

| Existing repetition | Proposed presentation | Access or information preserved |
|---|---|---|
| “Reviews and comments” beside Activity | Remove the link | Existing Approvals toolbar action and landing summary link reach the same destination; comment creation stays in its existing surface. |
| “Open DEMO-2” beside Activity | Remove the link | The task title row keeps its issue link. |
| “Shared discussion on G5 issues · DEMO-2” / GitHub equivalent and “Newest first” strip | Remove the whole strip | Activity remains a combined newest-first history; source-specific failures stay visible. |
| “Comment on DEMO-2 in Issues” | Remove the instructional link | The primary issue and existing Issues destination retain their comment entry points. No standing composer is introduced into Activity; the later contextual Reply correction is described below. |
| Repeated “Workspace context”, “G5 comment”, “G5 issue”, standalone “G5”/“GitHub” footers | Remove those repeated sentences/labels | Row type, actor and action remain. Where an external source needs identification, retain a compact source icon with an accessible label and tooltip, rather than another prose footer. |
| Separate “Open issue page”, “Open in GitHub”, “Open chat”, “Open task”, “Open commit”, “Open approvals”, “Open pull requests” and “View thread” footer actions | Use the event title for the action; use comment timestamps for the permalink | Exact destinations, native issue routes, external URLs, comment anchors and reviewed file/commit identity are retained. This changes link placement, not navigation authority. |
| Issue-opened row repeats the primary task's title and full body | A compact event such as “Alex opened DEMO-2” | Actor, issue identity and original timestamp remain; the full body stays in Workspace task and its reader. |
| Parent review card embeds replies also shown as reply events | Each reply appears once in Activity, at its own timestamp, using the same message box as its comment | The parent event opens its complete thread. Replies are not deleted from the source conversation or hidden from its reader. |
| Chat “last active …” repeats its timeline timestamp | One timestamp per row | Message count and conversation title remain. |
| Ongoing task/chat repeats Running/Waiting/Queued in title/body and Current badge | Keep the state once in “Current · Running/Waiting/Queued” | Completed tasks retain their outcome; Current rows retain their actual state and pinned position. |
| Total 0 plus Code 0 · Ontology 0 · Docs 0 | Show only “0 changed files” for a known zero total | Its Source Control link remains. A nonzero total keeps the existing category breakdown, including zero categories; loading/unavailable counts never become zero. |
| Extra heading restating the seeded PR title, “Preview exercise” implementation narration, and sentences announcing that an example has or lacks a PR | Remove those lines from the seed's sample prose | Keep the task problem, acceptance criteria, implementation behavior and review checks. Do not strip headings from arbitrary issue/PR content. |

The issue and G5 PR are separate records: each keeps its own title/status even when
their titles happen to match. Repository SHAs and provider PR statuses identify
different records and remain. Each Read more opens its own full reader. Error,
pending-publication recovery, stale-review and historical-approval notices remain
where they explain a real condition. Edit, Reply and retry actions retain their
authority checks. Activity still includes issue discussion, workspace comments and
replies, approvals, conversations, tasks, commits, PR events and guided marks; it
does not become an issue-only feed.

Owner-approved refinement, October 26, 2026: every person-written message—issue and
workspace comments, replies, and approval decisions with written comments—uses the
same box. Events stay plain, including uncommented approval decisions and the compact
issue-opened event; the box identifies a written message rather than a thread's start.

### Filter interaction

Use a single **All** state or a nonempty proper subset of category IDs. All is the
absence of a category restriction, not seven individually selected categories.
Keep the existing category vocabulary, compact pills and counts.

| Starting state | Click | Result and highlighted pills |
|---|---|---|
| All | Tasks | Tasks only; All and all other categories are neutral. |
| Tasks | Approvals | Tasks + Approvals; either category's history is included. |
| Tasks + Approvals | Tasks | Approvals only. |
| Tasks | Tasks | All; there is no accidental empty category selection. |
| Any selection | All | All; only All is highlighted. |
| Six categories | The last category | All; selecting every category is normalized to the same reset state. |
| All | All | All. |

Category groups combine with OR. Phase remains an independent AND restriction; All
resets only category selection, not the selected phase. A category with zero loaded
events remains selectable and leads to a concise no-matching-history message. A
count is the category's loaded historical event count, as today; it is not a server
total or a promise that all rows match the phase. Loading more applies the same
selection to newly loaded rows. Filtering changes neither source records nor fetching.

Keep ordinary button semantics: Tab focuses each pill; Enter or Space invokes the
same transition as a pointer click. Focus stays on the pill after the update. Selected
state is exposed with `aria-pressed`; neutral category pills under All are unselected,
not disabled. Preserve per-person/browser preferences and migrate the current stored
hidden-group set by deriving its visible subset: none hidden becomes All, a proper
visible subset is preserved, and the old hide-everything state becomes All. The view
owns this preference; no server filter or new persisted platform state is needed.

### Current work and historical records

The existing human-directed `g5.ui.activity-current` exception is preserved. Ongoing
chats and tasks stay pinned, once, above history, and are not hidden by either filter.
Thus Tasks means Tasks-only **history**; an ongoing chat can still appear above it.
The mock includes that exact case, and puts its explanation in the review controls,
not new recurring product text. Changing the exception would be a separate product
decision; this proposal does not silently change it to make the demo look simpler.

The compact issue-opened proposal applies where the row is the already-linked primary
issue. Preserve identifying titles for other historical records where necessary.
The full thread reader remains the destination for a parent discussion and replies.
The mock uses a sample reader; implementation must check paging so parent expansion
cannot reintroduce duplicate reply bodies or lose the ability to open a reply whose
parent is not on the currently loaded page.

### Ownership and implementation boundaries

This is a presentation and browser-preference change. The backend remains owner of
durable events, their timestamp/cursor order, review snapshot and publication evidence.
`WorkspaceActivityPanel` owns the combined feed's presentation; the shared filter
utility owns category mapping and preference compatibility. Source services and thread
readers keep full records. The landing summary owns zero-count simplification. The
seed script alone owns cleanup of our sample prose. Reuse existing destinations,
readers, error/recovery components and permission checks; introduce no independent composer lifecycle,
server endpoint, event copy, stored timeline or separate filter lifecycle.

The accepted ontology is amended in `ui.g5.md` at `g5.workbench.landing`, the new
`g5.workbench.landing.activity-filters`, and `g5.workbench.landing.summary`. The
issue/activity and review source owners keep their current meaning.
`g5.ui.activity-current`, comment identity, publication recovery and surrounding
controls remain governing. No mock controls, selection readout or interaction
instructions belong in the application. A workspace without a primary issue still
has valid Activity; removing the old discussion strip also removes its issue-link
prompt, with issue management kept in the existing task/Issues surfaces.

### Mock acceptance conditions and evidence

1. Current and Proposed full-page mocks show the same surrounding controls, Start
   buttons, toolbar actions and Guided content. No app component or preview fixture
   is modified by publishing these artifacts.
2. The sample page shows a task card, implementation card and Activity without the
   four redundant page-level elements in Proposed. All original issue comments remain.
3. A known zero total omits category-zero links; nonzero and unavailable states keep
   truthful counts. No-PR shows the existing implementation empty state.
4. All → Tasks takes one click and highlights Tasks alone; additions, removals,
   last-category reset and all-category normalization follow the table above.
5. Native keyboard activation and focus work. Dark/light and narrow content layouts
   remain readable; mocks do not load fonts, scripts or data from external services.
6. Phase filtering, zero results, mixed event types, source provenance, full readers,
   exact comment destinations, review replies and the Current exception are inspectable.
7. The artifacts are published as Project Docs, with links from the handoff. Verification
   applies to the prototype; it makes no claim about production implementation.

Validation details and captured mock states are maintained in the linked mock README.

### Consistent Reply correction — October 25

The owner observed Reply under a workspace parent but not its reply or the issue
comments and authorized correcting that inconsistency. Activity issue comments and
workspace comments/replies need the same Reply affordance regardless of author;
write restrictions still apply. No recurring source labels or explanatory prose are added.

Workspace parents, approval threads and reply events open the canonical full
thread with its existing reply form expanded and focused. Thread lookup remains
independent of Activity pagination. Issue-comment Reply opens the existing issue
composer in a dialog and seeds a compact Markdown reference to the original comment
permalink and author, without copying the original body into the new comment.
The reference alone cannot be posted; the person must enter a response. The tracker still stores a flat issue comment; no new reply hierarchy or backend
endpoint is introduced. Ordinary comment creation retains its current destinations.

Reuse the issue composer's person/workspace/destination draft and publication identity.
Append the selected quote to saved text without replacing it; reopening an unchanged
quote preserves the existing body and retry key. A different quote changes the text
and receives a new submission identity, like an explicit edit. Capture the selected
destination when opening, and rely on the existing server checks if links or authority
change; never retarget a reply draft. Keep provider review/connection/refusal/recovery.

Verify both authors' issue actions, parent and reply entry points, focus and full-thread
access, real posting to each original conversation, draft reopen/retry identity,
blocked writes and unchanged absence of standing composers or mock captions.

### Activity destination regression coverage — October 26

Fourteen full-page tests now follow Activity actions to their rendered destinations:
historical and Current chats, command and preview tasks, exact commit identity,
approval decisions and requests, internal PRs, native issues and comment permalinks,
and canonical discussion threads. Chat and commit routes are also restored after
remount; chat is reopened after returning through either the Workspace tab or rail.
GitHub issue, comment and PR links retain checks for their URLs and new-window targets.

The phase selector shares the Activity heading row and aligns to its right edge;
category filters remain below it. The existing phase selection and persistence
behavior is retained.

The repeated-chat tests initially failed: selecting a singleton content tab retained
the suppressed session hint. Singleton tabs now clear that hint using the same rule
as content paths, allowing a later explicit link to reopen the conversation.

## Validation

The integration base is main `243598eb`. Results below apply to the stated revision
and scope; prototype checks do not establish production behavior.

- **Local regression checks:** 548 tests passed across eight landing, Activity,
  Source Control, ChatPanel and launch-control files on `71d8408f`; three mocked-API
  Chromium Source Control journeys passed, including captured-head navigation,
  reload and independence from applied type filters. TypeScript and formatting
  passed; changed files had no lint errors. On `cdc82e935`, the 19-test landing
  suite covers canonical-description read failures with null, empty and whitespace
  overrides in both readers, plus saved-override fallback. Those cases and the
  background PR-refetch retention case failed before their fixes. The current
  harness follow-up passes all 18 reply-wait Chromium tests; the unchanged chat
  components passed 367 tests on `e4ddc2ab0`. Targeted harness TypeScript, formatting
  and lint checks pass. The reply tests cover distinct input/reply IDs through the
  default API reader for both new and existing sessions, a pair split across transcript pages, steered replies in the
  same task, delayed acceptance, stale identical replies and failure before acceptance.
  Native-question continuations require transcript task identity from the same task and
  stop at the next human input; the continuation case failed before this correction. Forty-five
  dependent browser journeys are discoverable. The stale-reply cases failed before
  binding the wait to its own turn. The compact phase-header layout passes the
  32-test Activity suite and TypeScript; Chromium checks at 1440 and 1024 pixels
  confirm vertical alignment, right-edge placement and phase persistence. The October 26
  message-box refinement passes 269 focused Activity, page-alignment and button-hook
  tests and the full frontend suite (5,180 tests across 331 files); TypeScript
  and lint have no errors. Mock verification passes with regenerated snapshots using
  bundled Playwright Chromium and the specified fallback font order.
  After the `243598eb` rebase, all 5,227 frontend tests across 333 files pass.
  The read-recovery follow-up passes 18 reply-wait Chromium tests; source and
  harness TypeScript, formatting, mock verification and targeted lint have no errors.
- **Cloud CI (`e545aa73a`, main `243598eb`):** Rust and the complete E2E check
  passed, with 340 first-attempt browser passes and 15 skips. Both Save selections,
  the four previously failing native-question continuation cases and the review
  journeys passed without retries. The wait identifies replies using the captured
  input and transcript task identity, bounded by the next human input; cleanup
  retains the exact input. Stop requests retain the server's two-minute save window
  and two one-minute host waits plus admission overhead, without replaying transport
  failures. [Build results](https://console.cloud.google.com/cloud-build/builds;region=us-central1/b7e82f63-cb16-4f58-bad0-1b6c462f1156?project=g5-qa-1ccc).
  CodeRabbit's scoped follow-up identified transient transcript HTTP reads as a
  new failure dependency. Read-only 5xx/408 failures may recover within the original
  deadline; three consecutive failures report the last HTTP error. Authorization,
  schema and confirmed provider failures remain immediate, and already identified
  replies remain inspectable during a read failure. The 18 local Chromium tests
  cover those bounds; this follow-up awaits its published-head CI qualification.
- **Full-stack preview:** worker, API and frontend were rebuilt for `e545aa73a`
  on the current main base, with health verified. Read-only Chromium checks against
  the real API verified message boxes and plain events across 13 review-workspace
  and 16 issue/PR-workspace entries. The improved local seed was reapplied after
  restart and remains outside committed source. Existing Source Control examples
  were preserved without edits or Save. Their initial seven-pushed/two-local-commit,
  dirty-file and PR states were verified through the real API on `6d9d53d7`.
- **Earlier production browser checks (October 25):** native Chromium against the
  real local preview verified both landing scenarios, additive filters, persistence,
  keyboard focus, issue permalinks, deduplicated replies and complete threads. Reply
  worked for both authors and parent/reply entries; real writes persisted in the
  original issue/thread, reference-only posts were disabled, compact links rendered
  correctly and retry payload identity stayed unchanged. No page errors or mock
  captions appeared; two Start controls, three Merge actions and phase chevrons
  remained. These checks did not exercise remote-edge owner-session authentication.
- **Prototype evidence:** the standalone mock scenarios, responsive/theme checks
  and captured states remain in the [mock README](../../../docs/mocks/landing-activity/README.md).

The full backend-connected browser suite is exercised in Cloud CI, not locally.
Backend description serializer, precedence, provider read and route-authorization
coverage stays in the existing SCM/API tests. New Activity tests follow rendered
content and external destinations; their scenario inventory is above.
