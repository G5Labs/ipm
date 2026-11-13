# Activity order, current work and the Activity rail panel

Source: [chat intent](../../chat_intent/activity-current-rail/request.md) (2026-10-20).
Mock: [mocks/activity-current-rail.html](mocks/activity-current-rail.html) (fake Current jobs, important and
unimportant events; toggles for the rail overlay, light mode, nothing running and a failed rail read), rendered in
`screenshots/`. Mock B [mocks/activity-current-rail-b-inline.html](mocks/activity-current-rail-b-inline.html)
(2026-10-20, user request) shows the same ongoing work as ordinary rows at the top of the record and of the
rail list, each badged Current, with no separate box or group headings; `screenshots/mock-b-*`.

Three bounded changes to the workspace Activity surface: the record reads newest first by
last activity; ongoing conversations and tasks form a live Current group pinned above it;
and the rail's Activity row becomes a collapsible section that shows Current and the most
important recent events with a link to the full page.

## Ontology changes

| Block | Change | Why |
|---|---|---|
| `g5.workbench.landing` (amended, `ui.g5.md`) | The record reads newest first by last activity; the composer sits above it. | User instruction; UX research favoured newest first for activity logs. |
| `g5.ui.activity-current` (new D, `ui.g5.md`) | Ongoing conversations and tasks are a Current group above the record: live, outside the filters, shown once, nothing persisted. | "mark them as current and keep them always at the top". |
| `g5.ui.activity-rail` (new D, `ui.g5.md`) | The rail's Activity section is a panel: Open full activity on top, Current, then the latest comments, approvals, pull requests and guided steps from the first loaded page. | "an actual panel like the others, showing only the most important events". |

## Governing ontology

- [g5.workbench.landing](../../ontology/features/ui.g5.md#g5.workbench.landing): every
  row stays a projection of a durable record; phase changes stay hidden; filters stay
  local, per person and browser.
- [g5.issue.activity](../../ontology/features/issues.g5.md#g5.issue.activity) and
  [g5.issue.read-failure](../../ontology/features/issues.g5.md#g5.issue.read-failure): the
  issue discussion stays interleaved; a failed GitHub read still leaves workspace context.
  Current never depends on a GitHub read.
- [g5.ui.guided-panel](../../ontology/features/ui.g5.md#g5.ui.guided-panel): "running is
  never completion"; Current reuses the same non-terminal task predicate and claims nothing
  about outcomes.
- [g5.ui.sidebar-default](../../ontology/features/ui.g5.md#g5.ui.sidebar-default) and
  [g5.workbench.tabs](../../ontology/features/ui.g5.md#g5.workbench.tabs): the rail modes
  and the landing-versus-return rule are unchanged; the full page keeps `?view=activity`.
- [g5.ui.unavailable-actions](../../ontology/features/ui.g5.md#g5.ui.unavailable-actions):
  a failed Current or record read is shown as unavailable with Retry, never as "nothing
  running" or "no activity".

## Design

### Order: newest first by last activity

**Owner.** `workspace_activity.rs` (the activity projection) owns the order; the page
renders what it is given.

- A conversation's time becomes `chat_sessions.updated_at` (bumped on every turn by
  `chat_service.rs`), so the chat query orders and pages on `(updated_at, 'chat:'||id)`.
  Tasks already use end-else-start; the other kinds keep the time they carry.
- `compose_page` returns the page newest first and takes `earlier` from its last entry;
  `WorkspaceActivityDTO.entries` is documented "newest first". The client flattens pages
  in load order (page 0 first) with the existing key dedup, which already handles an entry
  whose time moved between loads (a conversation that continued).
- `WorkspaceActivityPanel.tsx`: the scroller starts at the top; "Load earlier activity"
  sits under the last entry and appends below, so the prepend scroll restoration goes
  away; the "Oldest first" note becomes "Newest first"; the composer and the viewer's
  pending comments render above the list (under Current), still gated on the Issue
  discussion group.

### Current: ongoing conversations and tasks

**Owner.** The same projection, through a second read that carries no cursor and no
GitHub call: `GET /api/v1/workspaces/:workspaceId/activity/current` →
`{ entries: ActivityEntryDTO[] }`, newest last activity first.

- Ongoing = a `chat_sessions` row with a task where `chat_session_id = id`, `kind =
  'agent'` and `status IN ('queued','running','waiting')` (at most one per session by
  `tasks_one_active_agent_per_session_idx`), plus `tasks` rows with `kind <> 'agent'` in
  those states. Both reuse the row-to-entry mapping of `workspace_entries` (new shared
  helpers; no duplicate SQL for the entry shape). Bounded by the workspace's execution
  slots, so no page.
- Route in `external_issues::activity_router` under `WorkspacesRead`, handler
  `get_workspace_activity_current` with its `route_auth_static` manifest arm.
- Client: `useWorkspaceActivityCurrent(workspaceId)` in `useExternalIssues.ts`, polled at
  `ACTIVE_TASK_POLL_INTERVAL` (5 s) while a consumer is mounted; react-query shares one
  request between the page and the rail. When the set of keys shrinks, the hook invalidates
  the record query so the finished entry reappears there at its last-activity position.
  Send and Stop in `useChat.ts` invalidate it as they invalidate the task list today.
- Page (mock B, chosen 2026-10-20): the Current entries render as the first rows of the
  record list (`activity-current-<key>`), each the existing `ActivityEntry` with a
  "Current · Running / Waiting for input / Queued" badge and a left accent; no box, no
  heading. They are not a filter kind and are never hidden by the filter row; their keys are
  left out of the record below. Nothing running renders nothing extra. A failed Current read
  shows one line with Retry above the list.

### The Activity rail section

**Owner.** `WorkspacePage.tsx` composes the section like Guided (`activitySection` prop);
`components/external-issues/ActivityRailSection.tsx` renders it; `TreeNode.tsx` treats
`activity` as an ordinary section.

- `TreeNode.tsx`: the activity entry of `sectionConfig` renders the section; `headerOnly`,
  the forced collapse and the three `onOpenActivity` short-circuits (toggle, rail activate,
  activate request, hover/focus) go away, so the row collapses and expands like the others
  and the rail icon opens the overlay on it. The header keeps the "Collapse sidebar"
  trailing action in the stacked panel. It starts collapsed like every section but Guided
  (`defaultCollapsedSectionsFor` is unchanged).
- Body, top to bottom: an "Open full activity" link (`?view=activity` through the page's
  `openWorkspaceActivityTab`); then one list (mock B): the Current rows first (same source
  as the page, left accent, state word on the right), then up to eight rows of kinds `issue_comment`, `approval_decision`, `pull_request`, `journey_step` from
  page 0 of the shared record query, newest first. Each row is one line: kind icon, text
  (comment author and first line; decision and phase; PR event and title; step and
  done/undone/skipped), short time. A Current chat row opens its session (`?session=`),
  any other row opens the full page. Loading shows flat placeholders; a failed read shows
  one line with Retry; no qualifying rows shows "No comments, approvals, pull requests or
  guided steps yet" under the link. The section never pages: the full page does.
- Shown only under the workbench preset (`sectionOrderPreset === "workbench"`), as the
  row is today; the classic layouts keep no Activity row.

### Alternatives considered

- **Current derived in the browser** from the active-only task list the page already polls
  (`useWorkspaceActiveSessionIds`) joined to chat titles from the loaded record: no new
  route, but a conversation older than the loaded page has no title, two sources must agree,
  and the rail would synthesize rows the page does not. Rejected for the one read.
- **Current as a field on page 0** of the record: live only if page 0 is polled, which
  repeats the GitHub issue read every few seconds. Rejected.
- **Oldest-first issue discussion kept inside a newest-first record**: mixed directions in
  one list; the user asked for the whole view descending. Rejected.

### Assumptions and acceptance conditions

- Current in the rail is an interpretation of "always at the top" (the user named only the
  four event kinds for the rail); drop the group from the rail if unwanted.
- The rail section starts collapsed like the other sections; Guided keeps its default-open
  rule. Under expand-on-hover the Activity icon opens the overlay on the section.
- Eight rail rows and page 0 as the rail's source are agent picks; a workspace whose first
  fifty entries hold none of the four kinds shows the empty line, not older ones.
- A conversation's last activity is `updated_at`; a title-only edit also bumps it in
  `chat_service.rs` (title source update), which is accepted as activity.

## Tests

- `backend/g5-api/tests/api_workspace_timeline.rs`: newest first within and across pages;
  a conversation pages by its last message time and moves after a new turn; `current`
  lists a session with a queued/running/waiting agent task and a running preview, omits
  finished ones, is empty with nothing running, and reads with `WorkspacesRead`;
  `route_auth_static` manifest arm.
- `workspace_activity.rs` unit tests: `compose_page` newest first, cursor from the last.
- `WorkspaceActivityPanel.test.tsx`: order, Load earlier appends below, composer above the
  list, Current group above the composer and unaffected by filters, a current key absent
  from the record, Current error line with Retry, "Newest first".
- `hooks/useExternalIssues.test.tsx`: Current polls and invalidates the record when a key
  leaves.
- `ActivityRailSection.test.tsx`: link, Current rows, the four kinds only, cap of eight,
  empty and error lines, row targets.
- `TreeNode.sectionOrder.test.tsx` and `Sidebar.test.tsx`: Activity collapses and expands,
  the rail icon opens the overlay, the collapse-sidebar action stays.
- `pages/WorkspacePageAlignment.test.tsx`: the section renders under the workbench preset
  and its link opens the Activity tab.
- `e2e/flows/alignment-shell.spec.ts`: the landing's first entry is the newest; the rail
  section expands and its link opens the full page.

## Implementation (2026-10-20, mock B)

Delivered on `feat/activity-current-rail` as designed, with these adjustments found
while building:

- The chat entry carries `taskStatus` only on Current entries (serde skips it in the
  record), so one `ActivityEntryDTO` shape serves both reads and the badge's state word.
- `compose_page` returns the page newest first and takes the cursor from its last entry;
  the client flattens pages in load order and no longer restores scroll on load.
- The rail section is passed as `activitySection` (like `guidedSection`); `LeftSidebar`
  lost `onOpenActivity` and every Activity special case, so the row collapses, expands
  and opens the overlay like the others. Base gets the section too (its record exists).
- Send, session start and Stop in `useChat.ts` invalidate the Current read with the task
  list, so the badge changes without waiting for the 5 s poll.

Evidence: the test files listed under the three blocks' metadata in `ui.g5.md`; backend
`cargo test -p g5-api --test integration` modules `api_workspace_timeline`,
`api_external_issues::workspace_activity*`, `route_auth_static`, `api_task_lifecycle::preview*`,
`api_workspace_journey`; frontend `vitest` for the changed files and the page suite. The
Playwright flows are updated but were not executed here.

### Review round (2026-10-20)

Two independent Codex reviews (correctness, duplication) and two Claude reviews on the first
commit. Applied: Current rows render independently of the record read on both surfaces; the
rail reports a GitHub discussion read failure instead of "nothing yet"; the Current hook forgets
its key memory on a workspace switch; the rail reuses `firstParagraph`, `TaskStatusIndicator`
and `ACTIVE_TASK_POLL_INTERVAL`; the Current test fixture is shared. Recorded as limits, not
fixed: a conversation that starts and ends between two polls moves above a loaded page's cursor
until the record refetches; expand-on-hover keyboard behaviour (focus opens the overlay, Enter
toggles it) is the sidebar's existing rule for every section; a first rail hover now opens
Activity because it leads the rail.

Second independent Codex round: applied the panel's loading branch (Current rows and their
notice render while the record still loads, as the rail already did). Its two other findings
are the limits above: a renamed chat does not move (a rename never bumps `updated_at`;
only turns and turn-start bookkeeping do, which Current covers), and the expand-on-hover
keyboard rule is the sidebar's for every section. Stopped here per the review policy.
