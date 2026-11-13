# Task Deep Dive — End-to-End Design

> Closes the wiring gaps in the Bottom Drawer execution panel. Covers diff
> review, agent timeline, message injection, branch-aware draft switching,
> session attachment, intervention controls (pause/cancel/resume), and the
> takeover flow. Each section specifies the data path from backend API
> through WebSocket events, React Query hooks, component props, and UI
> interactions, followed by the test plan.

---

## §0 Status Quo & Gaps

The Bottom Drawer (`BottomDrawer.tsx`) has five tabs—DAG, Changes, Timeline,
Terminal, Intervene—and a task header with action buttons. An audit identified
nine wiring gaps:

| # | Gap | Severity | Section |
|---|-----|----------|---------|
| 1 | `onAttach` is a no-op — "View Dialog" link does nothing | Medium | §2 |
| 2 | Timeline accepts `onPause`/`onResume`/`onAbort` but never renders them | Low | §3 |
| 3 | No branch picker — `setActiveBranchId` never exposed in UI; task selection doesn't sync branch | High | §4 |
| 4 | No infinite-scroll trigger for timeline pagination | Medium | §5 |
| 5 | No loading/error states in the Changes tab | Low | §6 |
| 6 | No optimistic UI or loading state on pause/cancel/resume buttons | Low | §7 |
| 7 | Take Over only fires `pauseTask` — no attach, no terminal, no hand-back | High | §8 |
| 8 | "Open in IDE" button is a no-op | Low | §9 |
| 9 | URL carries no view state — shared links don't preserve active tab, branch, takeover mode, or chat session | Medium | §13 |

This design addresses all nine by specifying the exact changes needed at each
layer and the test coverage for each.

---

## §1 Architecture Context

### §1.1 Relevant Backend APIs

| Endpoint | Method | Purpose |
|----------|--------|---------|
| `/api/v1/tasks/:id` | GET | Task detail (includes `branchName`, `sessionId`, `workerId`) |
| `/api/v1/tasks/:id/timeline` | GET | Cursor-paginated timeline entries |
| `/api/v1/tasks/:id/diff` | GET | Per-file diff with hunks |
| `/api/v1/tasks/:id/checkpoints` | GET | Checkpoint commits |
| `/api/v1/tasks/:id/pause` | POST | Pause agent at next tool boundary |
| `/api/v1/tasks/:id/resume` | POST | Resume paused agent |
| `/api/v1/tasks/:id/abort` | POST | Cancel task |
| `/api/v1/tasks/:id/complete` | POST | User marks blocked task as done |
| `/api/v1/sessions/:id/inject` | POST | Inject message into agent session |
| `/api/v1/sessions/:id/attach` | POST | Attach user to agent session (Mode 2) |

### §1.2 Relevant WebSocket Events

| Event | Cache Effect |
|-------|-------------|
| `task.status` | Invalidates `['task', id]`, `['job-tasks']`, `['jobs']` |
| `task.timeline` | Optimistic append to `['task-timeline', id]` |
| `task.diff` | Invalidates `['task-diff', id]` |
| `chat.done` | Invalidates `['session-timeline', sessionId]` |

### §1.3 Frontend Data Path

```
Backend API → React Query hook → WorkspacePage state → BottomDrawer props → Tab component
     ↑                                                                            │
     └──── WebSocket cache invalidation / optimistic update ◄─────────────────────┘
```

---

## §2 Fix 1 — Session Attachment (`onAttach`) — Open Task Dialog in Chat Panel

### §2.1 Problem

`TaskTimelineView` has an `onAttach` prop and a "View Dialog" link in the
footer, but `BottomDrawer` passes `onAttach={() => {}}`. Clicking the link
does nothing.

### §2.2 Design

**Goal:** Clicking "View Dialog" opens the task's chat session in the right
panel (chat sidebar), showing the full chronological conversation **and**
agent execution timeline for the running task.

**Why this works end-to-end:** Every task has a `sessionId` which is a
first-class chat session. The chat panel already merges two data sources
chronologically via `buildUnifiedTimeline()`:

1. **Messages** from `useChatMessages(sessionId)` — user/assistant conversation turns.
2. **Timeline traces** from `useSessionTimeline(sessionId)` — agent thoughts,
   tool calls/results, outputs, errors, and checkpoints.

When a task is running, the backend's `GET /sessions/:id/timeline` returns
the task-scoped execution traces because the task session **is** the
session. Live updates arrive via two mechanisms:

- **`chat.chunk` / `chat.done` events** on the `chat:<sessionId>` WS channel
  handle streaming messages and trigger `['session-timeline', sessionId]`
  cache invalidation on completion.
- **`task.timeline` events** arrive globally (listened via
  `useWSEventListener('task.timeline', ...)` in WorkspacePage) and are
  accumulated into `liveTraceEntries`, then merged with API-fetched history
  into `chatTimelineEntries`. These are passed into `ChatPanel` as
  `timelineEntries={chatTimelineEntries}`.

So switching `activeChatSessionId` to the task's `sessionId` is sufficient
to show the full running-task dialog — both conversation and trace — with
live streaming. No additional WS channel subscription is needed because:
- `useWSChannel(\`chat:${activeChatSessionId}\`)` is already called when the
  session ID changes.
- The global `task.timeline` listener captures live entries regardless of
  which channel is subscribed.

**Data path:**
1. `TaskDTO.sessionId` contains the chat session ID for the task.
2. `BottomDrawer` receives a new prop `onViewSession: (sessionId: string) => void`.
3. `BottomDrawer` passes `onAttach={() => onViewSession(taskData.sessionId)}` to
   `TaskTimelineView` (guarded: only when `sessionId` is non-null).
4. `WorkspacePage` implements `onViewSession` by:
   - Setting `activeChatSessionId` to the given session ID.
   - Opening the right panel if not already open (`setRightPanelOpen(true)`).
5. The existing `useSessionTimeline(activeChatSessionId)` refetches the
   API-persisted timeline. The existing `handleTimelineEvent` listener merges
   live traces into `chatTimelineEntries`. Both flows converge into `ChatPanel`
   via `timelineEntries={chatTimelineEntries}`.

**No new hooks needed.** The chat panel already renders the unified timeline
for whichever session is active.

### §2.3 Component Changes

| File | Change |
|------|--------|
| `BottomDrawer.tsx` | Add `onViewSession` prop. Replace `onAttach={() => {}}` with `onAttach={() => { if (taskData?.sessionId) onViewSession(taskData.sessionId); }}`. |
| `WorkspacePage.tsx` | Pass `onViewSession` to `BottomDrawer`. Handler: `setActiveChatSessionId(sid); setRightPanelOpen(true);`. |

### §2.4 Tests

| Test | Assertion |
|------|-----------|
| `BottomDrawer` unit: View Dialog calls `onViewSession` | Click "View Dialog" → `onViewSession` mock called with `sessionId`. |
| `BottomDrawer` unit: no link when `sessionId` is null | No "View Dialog" link rendered when task has no session. |
| Chat panel shows task timeline after session switch | After `onViewSession(taskSessionId)`, `ChatPanel` receives matching `timelineEntries`. |

---

## §3 Fix 2 — Intervention Controls in Timeline (was gap #7)

### §3.1 Problem

`TaskTimelineView` accepts `onPause`, `onResume`, `onAbort` props but never
renders any buttons using them. The user must switch to the Intervene tab
to access controls.

### §3.2 Design

**Goal:** Add a floating action bar at the bottom of the timeline (above the
footer stats) when the task is active. This mirrors the Intervene tab controls
in a compact inline form.

**UI spec:**
```
┌─────────────────────────────────────────────────────────┐
│ [timeline entries scroll area]                           │
│                                                          │
│ ┌──────────────────────────────────────────────────────┐ │
│ │  ⏸ Pause   ⏹ Cancel         (when running)          │ │
│ │  ▶ Resume   ⏹ Cancel         (when paused/blocked)   │ │
│ └──────────────────────────────────────────────────────┘ │
│ ┌──────────────────────────────────────────────────────┐ │
│ │ Cost: $0.12  Duration: 3m 12s  Files: 3  Session: …  │ │
│ └──────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────┘
```

### §3.3 Component Changes

| File | Change |
|------|--------|
| `Timeline.tsx` | Add a `TimelineActionBar` section between the scroll area and the footer stats bar. Render Pause/Cancel when `taskStatus === 'running'`, Resume/Cancel when `taskStatus === 'blocked'`. Call `onPause`/`onResume`/`onAbort` on click. |

### §3.4 Tests

| Test | Assertion |
|------|-----------|
| Running task shows Pause + Cancel | Buttons visible, click calls `onPause`/`onAbort`. |
| Paused task shows Resume + Cancel | Buttons visible, click calls `onResume`/`onAbort`. |
| Completed task shows no action bar | No buttons rendered. |

---

## §4 Fix 3 — Branch Picker & Task→Branch Sync (was gaps #3 + #4)

### §4.1 Problem

1. `setActiveBranchId` is never exposed in UI — users cannot switch branches.
2. Selecting a task doesn't switch the editor to the task's branch, causing
   the file tree and diff view to show stale/wrong context.

### §4.2 Design

**§4.2.1 Branch Picker in Repos Bar**

**Current state:** The `ReposBar` shows `[org/repo : branch]` chips, one per
workspace branch. Clicking a chip fires `onSelectRepo(repoId)`, but
`WorkspacePage` passes `onSelectRepo={() => {}}` — a no-op. There is no way
for the user to switch branches.

**Design:** Each repo chip becomes a **click-to-expand dropdown** that lists
all branches for that repo. The chip always displays the **active** branch
for that repo (i.e., the branch whose `id === activeBranchId`). Clicking the
chip toggles a dropdown showing all branches for that repo.

```
┌─────────────────────────────────────────────────────────────────┐
│ Repos  [🗂 acme/api : main ▾]  [🗂 acme/frontend : feat-x ▾]  │
│                   ┌──────────────────┐                          │
│                   │ ● main           │  ← base branch (default) │
│                   │   task/abc-123   │  ← task branch           │
│                   │   task/def-456   │  ← task branch           │
│                   │   feat-auth      │  ← feature branch        │
│                   └──────────────────┘                          │
└─────────────────────────────────────────────────────────────────┘
```

**Data path:**
1. `sortedBranches` already contains all workspace branches with `repoName`,
   `branchName`, `baseBranch`, and `id`.
2. `ReposBar` receives a new `branches` prop (grouped by repo) instead of
   or in addition to the current `repos` prop.
3. Each repo chip groups its branches. The base branch (where
   `branchName === baseBranch`) is shown first with a "●" indicator.
4. Task branches (matching `task/*` pattern) are labeled with a task icon.
5. Clicking a branch in the dropdown calls `onSelectBranch(branchId)`.
6. `WorkspacePage` wires `onSelectBranch={setActiveBranchId}`.

**The active branch is highlighted** with the `--g5-info` accent color. This
gives the user a clear way to see which branch they're on and flip back to
`main` at any time.

**§4.2.2 Task Selection → Branch Auto-Switch (with Return-to-Main)**

When the user selects a task from the DAG or left sidebar:

1. `handleSelectTask(taskId)` fires.
2. Save the **previous** `activeBranchId` as `previousBranchId` state
   (only if not already viewing a task branch).
3. Resolve the task's `branchName` from `tasksByJob`.
4. Find the matching branch ID in `sortedBranches` where
   `branch.branchName === task.branchName`.
5. If found, call `setActiveBranchId(matchingBranch.id)`.
6. If not found, keep current branch and show an info toast.

**Returning to main:** The user has three ways to switch back:

1. **Branch picker dropdown** — click the repo chip, select `main` (or any
   other branch). Always available, always visible.
2. **"Back to main" breadcrumb** — when `activeBranchId !== previousBranchId`
   due to a task auto-switch, a small breadcrumb appears in the BottomDrawer
   header or the repos bar: `← Back to main`. Clicking it restores
   `previousBranchId`.
3. **Deselect the task** — closing the bottom drawer or clicking elsewhere
   in the DAG clears `selectedTaskId`. A `useEffect` watches
   `selectedTaskId`: when it becomes null and `previousBranchId` is set,
   restore `setActiveBranchId(previousBranchId)` and clear
   `previousBranchId`.

**This ensures:** the file tree, draft session, ontology, and CRDT all
reflect the task's branch while reviewing, and the user can always return
to their prior context.

### §4.3 Component Changes

| File | Change |
|------|--------|
| `ReposBar.tsx` | Add `branches` prop (grouped by repo), `activeBranchId`, and `onSelectBranch`. Render each chip as a dropdown. Highlight active branch. |
| `WorkspacePage.tsx` | Pass `branches`, `activeBranchId`, `onSelectBranch={setActiveBranchId}` to `ReposBar`. Wire `onSelectRepo` to filter/focus the repo. |
| `WorkspacePage.tsx` | Add `previousBranchId` state. In `handleSelectTask`, save previous branch before switching. |
| `WorkspacePage.tsx` | Add `useEffect` to restore `previousBranchId` when `selectedTaskId` clears. |
| `BottomDrawer.tsx` | When task branch differs from `previousBranchId`, show "← Back to `main`" breadcrumb. Add `onRestoreBranch` prop. |

### §4.4 Tests

| Test | Assertion |
|------|-----------|
| Branch picker renders branches grouped by repo | Dropdown lists branches; base branch first with indicator. |
| Selecting a branch updates `activeBranchId` | Click `task/abc` → `onSelectBranch` called with correct ID. |
| Active branch is highlighted | Current branch shows `--g5-info` accent. |
| Task selection auto-switches branch | Selecting task with `branchName: "task/abc"` sets active branch. |
| "Back to main" breadcrumb appears | After task auto-switch, breadcrumb visible. Click restores previous. |
| Deselecting task restores previous branch | Clear `selectedTaskId` → branch returns to `previousBranchId`. |
| Missing branch shows toast | Task with unknown branch shows info message, branch unchanged. |

---

## §5 Fix 4 — Timeline Infinite Scroll

### §5.1 Problem

`useTaskTimeline` uses `useInfiniteQuery` with cursor-based pagination, but
`TaskTimelineView` has no mechanism to trigger `fetchNextPage()`. Old entries
beyond the first page are unreachable.

### §5.2 Design

**Option chosen:** Intersection observer on a sentinel element at the **top**
of the scroll area (since the timeline scrolls newest-at-bottom, older entries
load above).

**Data path:**
1. `BottomDrawer` already calls `useTaskTimeline(taskId)` internally.
2. The infinite query exposes `fetchNextPage` and `hasNextPage`.
3. Pass `onLoadMore` and `hasMore` props to `TaskTimelineView`.
4. `TaskTimelineView` renders a sentinel `<div ref={sentinelRef} />` at the
   top of the entries list.
5. An `IntersectionObserver` watches the sentinel. When it enters the
   viewport (user scrolled to top), call `onLoadMore()`.
6. Show a "Loading older entries…" spinner while fetching.
7. After insertion, restore scroll position so the view doesn't jump.

### §5.3 Component Changes

| File | Change |
|------|--------|
| `BottomDrawer.tsx` | Extract `fetchNextPage` and `hasNextPage` from the `useTaskTimeline` infinite query result. Pass `onLoadMore` and `hasMore` to `TaskTimelineView`. |
| `Timeline.tsx` | Add `onLoadMore?: () => void` and `hasMore?: boolean` props. Add sentinel + IntersectionObserver. Spinner at top. Scroll position restoration. |

### §5.4 Tests

| Test | Assertion |
|------|-----------|
| Sentinel triggers `onLoadMore` | Scrolling to top calls `onLoadMore` when `hasMore=true`. |
| No sentinel when `hasMore=false` | Sentinel not rendered when all pages loaded. |
| Loading spinner visible during fetch | "Loading older entries…" visible while fetching. |

---

## §6 Fix 5 — Changes Tab Loading & Error States

### §6.1 Problem

The Changes tab silently shows "No file changes yet" or "Select a file to
view changes" when the diff API is loading or has errored. The user has no
feedback that data is being fetched or that something failed.

### §6.2 Design

Pass the `AsyncState` for both `taskDiffState` and `taskCheckpoints` into
`BottomDrawer` (not just the resolved data arrays). The Changes tab renders:

| State | UI |
|-------|----|
| Loading | Skeleton shimmer in file list + "Loading changes…" in diff pane |
| Error | Error message with retry button in diff pane |
| OK, empty | "No file changes yet" (current behavior) |
| OK, with data | File list + diff viewer (current behavior) |

### §6.3 Component Changes

| File | Change |
|------|--------|
| `BottomDrawer.tsx` props | Add `diffState: AsyncState<DiffData[]>` and `checkpointsState: AsyncState<TaskCheckpoint[]>` (replacing the resolved arrays). |
| `BottomDrawer.tsx` Changes tab | Check `diffState.status` before rendering. Show shimmer/error as appropriate. |
| `WorkspacePage.tsx` | Pass raw `taskDiffState` and `taskCheckpoints` instead of extracting `.data`. |

### §6.4 Tests

| Test | Assertion |
|------|-----------|
| Loading state shows shimmer | Changes tab with `diffState.status='loading'` shows loading UI. |
| Error state shows message | Changes tab with `diffState.status='error'` shows error + retry. |
| Retry button refetches | Click retry → diff query re-fired. |

---

## §7 Fix 6 — Optimistic UI on Pause/Cancel/Resume

### §7.1 Problem

After clicking Pause, the button stays enabled until the mutation round-trips
and the cache is invalidated. Users can double-click, and there's no visual
feedback that the action was received.

### §7.2 Design

**§7.2.1 Button Loading State**

The mutation hooks (`usePauseTask`, etc.) return `isPending` on the mutation
object. Thread this through as `isPausing`, `isResuming`, `isAborting` props.

When any mutation is pending:
- The active button shows a spinner icon replacing its text.
- All action buttons are disabled.

**§7.2.2 Optimistic Cache Update**

In each mutation hook's `onMutate`, optimistically update the `['task', taskId]`
cache entry:
- `pauseTask.onMutate` → set `status: 'blocked'`
- `resumeTask.onMutate` → set `status: 'running'`
- `abortTask.onMutate` → set `status: 'cancelled'`

On error, `onError` rolls back via the snapshot saved in `onMutate`.

### §7.3 Component Changes

| File | Change |
|------|--------|
| `useJobs.ts` | Add `onMutate` optimistic updates + `onError` rollback to `usePauseTask`, `useResumeTask`, `useAbortTask`. |
| `BottomDrawer.tsx` props | Add `isPausing`, `isResuming`, `isAborting` boolean props. |
| `BottomDrawer.tsx` header | Disable action buttons when any mutation pending. Show spinner. |
| `IntervenePanel.tsx` props | Add same loading props. Disable buttons + spinner. |
| `Timeline.tsx` action bar | Disable buttons when mutation pending. |
| `WorkspacePage.tsx` | Pass `isPausing={pauseTask.isPending}` etc. to `BottomDrawer`. |

### §7.4 Tests

| Test | Assertion |
|------|-----------|
| Pause button disables during mutation | Click Pause → button disabled + spinner visible. |
| All buttons disabled during any mutation | While pausing, Resume/Cancel also disabled. |
| Optimistic update changes status chip | After Pause click, chip immediately shows "blocked". |
| Rollback on error | Mutation error → status reverts to "running". |

---

## §8 Fix 7 — Take Over Mode (Pause + Attach)

### §8.1 Problem

The "Activate Take Over" button only fires `pauseTask`. There is no mechanism
to give the user interactive access to the worker or track takeover state.

### §8.2 Design

**Key insight:** The backend already supports `POST /sessions/:id/attach`
(Mode 2 — direct attachment). True "takeover" is **Pause + Attach** — the
agent stops, the user joins the session with full draft access, and can inject
messages or manually edit files before resuming.

**Takeover is scoped to a single task.** The task ID is already in the URL
path (`/:org/:workspace/job/:jobId/task/:taskId`) and is reinforced by the
`?takeover=1` query param (§13). This means:

- The URL unambiguously identifies *which* task is taken over.
- A shared link like `/acme/ws/job/j1/task/t1?tab=intervene&takeover=1`
  tells a coworker exactly what's happening.
- Only **one task** can be in takeover at a time (per user/browser tab).

**Takeover is a composite UI flow, not a new API:**

```
User clicks "Activate Take Over"
  │
  ├─ 1. POST /tasks/:id/pause           → agent pauses at next tool boundary
  ├─ 2. POST /sessions/:id/attach       → user joins the session
  ├─ 3. Switch active branch             → editor shows the task's branch
  ├─ 4. Open right panel (chat)          → user sees agent's conversation
  ├─ 5. Set takeoverTaskId state         → UI enters "takeover mode"
  └─ 6. Update URL                       → ?takeover=1 added
```

**Takeover mode UI changes:**
- A persistent amber banner in the BottomDrawer header: "You are in control of
  this task. The agent is paused."
- The Intervene tab shows an expanded terminal-like interface (send messages,
  see timeline, edit files in the task branch context).
- A "Return Control" button that resumes the agent (`POST /tasks/:id/resume`)
  and clears `takeoverTaskId`.

### §8.3 Edge-Case Guards

**§8.3.1 Task status changes underneath the user**

A `useEffect` watches the taken-over task's status. If the task leaves a
user-controlled state (`blocked`, `paused`) — e.g., it gets cancelled by an
admin, times out, or completes — takeover is automatically released:

```typescript
// In WorkspacePage:
useEffect(() => {
  if (!takeoverTaskId) return;
  if (taskDetail.status !== 'ok') return;
  const { status } = taskDetail.data;
  // Auto-clear takeover if the task is no longer in a pausable state
  if (status !== 'blocked' && status !== 'paused' && status !== 'running') {
    setTakeoverTaskId(null);
    updateViewParams({ takeover: null });
    // Optionally show a toast: "Task completed/cancelled — takeover ended"
  }
}, [takeoverTaskId, taskDetail]);
```

**§8.3.2 Multiple tasks in the same job**

Because takeover **pauses** the task, it cannot conflict with other running
tasks even if they share a branch (which shouldn't happen — each task gets
its own branch). The URL identifies the specific task, so there's no ambiguity.

If the user tries to take over a second task while already controlling one:
- Show a confirmation dialog: "You're currently controlling task X. Return
  control before taking over task Y?"
- On confirm: call `handleReturnControl(currentTaskId)` first, then proceed
  with `handleTakeOver(newTaskId)`.

```typescript
const handleTakeOver = useCallback(async (taskId: string) => {
  // Guard: already controlling another task?
  if (takeoverTaskId && takeoverTaskId !== taskId) {
    const confirmed = window.confirm(
      'You are currently controlling another task. Return control and take over this one?'
    );
    if (!confirmed) return;
    await handleReturnControl(takeoverTaskId);
  }

  // Resolve the task — use selectedTaskId's cached data if it matches,
  // otherwise fetch fresh from the API.
  const task = (selectedTaskId === taskId && taskDetail.status === 'ok')
    ? taskDetail.data
    : await api.get<TaskDTO>(`/api/v1/tasks/${taskId}`);
  if (!task) return;

  try {
    // 1. Pause the task
    await pauseTask.mutateAsync(taskId);

    // 2. Attach to the session
    if (task.sessionId) {
      await attachSession.mutateAsync(task.sessionId);
    }
  } catch (err) {
    // If pause succeeded but attach failed, the task is paused without
    // attachment. Show an error toast but leave the task paused — the user
    // can still resume via the normal Resume button.
    console.error('Takeover failed:', err);
    // TODO: toast notification: "Takeover failed — task is paused but not attached"
    return;
  }

  // 3. Switch branch
  const taskBranch = sortedBranches.find(b => b.branchName === task.branchName);
  if (taskBranch) {
    setPreviousBranchId(activeBranchId);
    setActiveBranchId(taskBranch.id);
  }
  // 4. Open chat panel
  if (task.sessionId) {
    setActiveChatSessionId(task.sessionId);
    setRightPanelOpen(true);
  }
  // 5. Set takeover state + URL
  setTakeoverTaskId(taskId);
  updateViewParams({ takeover: '1', tab: 'intervene' });
}, [takeoverTaskId, selectedTaskId, taskDetail, pauseTask, attachSession, ...]);
```

**§8.3.3 User navigates away**

Closing the bottom drawer or selecting a different task does **not**
implicitly release takeover. The agent stays paused. This is intentional:
the user may be editing files in the task branch and not looking at the
drawer. Takeover ends in any of these ways:

1. **"Return Control" button** — resumes the agent and clears takeover state.
2. **"Resume" button** (in the Intervene tab, Timeline action bar, or
   BottomDrawer header) — resumes the agent, which transitions the task to
   `running`. The §8.3.1 `useEffect` guard detects the status change and
   auto-clears `takeoverTaskId` + removes `?takeover` from the URL.
3. **"Cancel" button** — aborts the task (`cancelled` status), also caught
   by §8.3.1 auto-clear.
4. **External status change** — admin cancel, timeout, completion — same
   §8.3.1 guard.

In other words, any action that moves the task out of `paused`/`blocked`
ends takeover. "Return Control" is just "Resume" with a friendlier label.

If the user selects a different task while in takeover, the takeover banner
remains visible with a link back: "You are controlling task X — [Return
Control] [Resume] or [Switch back]".

### §8.4 New Hooks

| Hook | File | API | Purpose |
|------|------|-----|---------|
| `useAttachSession()` | `useChat.ts` | `POST /sessions/:id/attach` | Attach user to a task session. Returns confirmation + WS subscription setup. |

### §8.5 Component Changes

| File | Change |
|------|--------|
| `useChat.ts` | Add `useAttachSession()` mutation hook. |
| `WorkspacePage.tsx` | Add `takeoverTaskId` state. Implement `handleTakeOver(taskId)` with guard for existing takeover (§8.3.2). Add `handleReturnControl(taskId)` that resumes + clears state + clears URL param. Add `useEffect` guard for task status changes (§8.3.1). |
| `BottomDrawer.tsx` | Add `isTakeover: boolean`, `takeoverTaskId: string | null`, `onTakeOver: (taskId) => void`, `onReturnControl: (taskId) => void` props. Show amber takeover banner when active. When viewing a different task during takeover, show "controlling task X" link. Pass `onTakeOver` to IntervenePanel. |
| `IntervenePanel.tsx` | Replace the Take Over card's `onClick` from `onPauseTask` to `onTakeOver`. Show "Return Control" button when `isTakeover=true`. |

### §8.6 Tests

| Test | Assertion |
|------|-----------|
| Take Over button calls `onTakeOver` | Click "Activate Take Over" → `onTakeOver(taskId)` called. |
| Takeover mode shows banner | When `isTakeover=true`, amber banner visible with "You are in control". |
| Return Control calls handler | Click "Return Control" → `onReturnControl(taskId)` called. |
| `useAttachSession` calls correct endpoint | Mock: `POST /sessions/:id/attach` → 200. |
| Takeover sequences correctly | `handleTakeOver` calls pause → attach → branch switch → panel open in order. |
| Attach failure leaves task paused | Pause succeeds, attach fails → task stays paused, `takeoverTaskId` not set, error shown. |
| Auto-clear on task completion | Task status changes to `completed` → `takeoverTaskId` cleared, `?takeover` removed from URL. |
| Auto-clear on external cancel | Task status changes to `cancelled` → takeover released. |
| Second takeover prompts confirmation | User controls task A, clicks Take Over on task B → confirmation dialog shown. |
| Confirm second takeover returns control of first | Accept dialog → `handleReturnControl(A)` then `handleTakeOver(B)`. |
| Navigate away preserves takeover | Close drawer during takeover → `takeoverTaskId` unchanged, agent stays paused. |
| Banner links back when viewing other task | Viewing task B while controlling task A → banner shows "Controlling task A" with link. |

---

## §9 Fix 8 — "Open in IDE" Button

### §9.1 Problem

The "Open in IDE" button has `onClick={() => {}}`.

### §9.2 Design

**Goal:** Open the task's branch in the user's local IDE (VS Code) via the
`vscode://` protocol handler, or at minimum deep-link to the task view in a
new browser tab.

**Two options (in priority order):**

1. **Protocol handler** (when available): The IDE plugin design (IDE_PLUGIN.md)
   specifies a `vscode://g5.open?workspace=<id>&branch=<branch>&task=<taskId>`
   URL scheme. The button constructs this URL and calls `window.open()`.

2. **Fallback — new tab**: Open `/:org/:workspace/job/:jobId/task/:taskId` in
   a new browser tab via `window.open(url, '_blank')`.

Since the IDE plugin is not yet implemented, use option 2 as the initial
implementation.

### §9.3 Component Changes

| File | Change |
|------|--------|
| `BottomDrawer.tsx` | Replace `onClick={() => {}}` for the ExternalLink button with `onClick` that calls `window.open(taskUrl, '_blank')`. Compute `taskUrl` from current route params. Add `onOpenExternal?: (taskId: string) => void` prop as an alternative injection point. |
| `WorkspacePage.tsx` | Pass `onOpenExternal` that constructs the deep-link URL using `org`, `workspaceSlug`, and the task's job ID. |

### §9.4 Tests

| Test | Assertion |
|------|-----------|
| Open in IDE calls `onOpenExternal` | Click button → `onOpenExternal(taskId)` called. |
| URL constructed correctly | `onOpenExternal` receives expected deep-link format. |

---

## §10 Implementation Plan

### §10.1 Phase 1 — Core Wiring (Fixes 1, 2, 4, 5, 6, 8)

These fixes require no new API endpoints and can be done in parallel:

| Fix | Section | Effort | Files Modified |
|-----|---------|--------|----------------|
| Fix 1 (onAttach → chat) | §2 | S | `BottomDrawer.tsx`, `WorkspacePage.tsx` |
| Fix 2 (timeline action bar) | §3 | S | `Timeline.tsx` |
| Fix 4 (timeline infinite scroll) | §5 | M | `BottomDrawer.tsx`, `Timeline.tsx` |
| Fix 5 (Changes loading/error) | §6 | S | `BottomDrawer.tsx`, `WorkspacePage.tsx` |
| Fix 6 (optimistic UI) | §7 | M | `useJobs.ts`, `BottomDrawer.tsx`, `IntervenePanel.tsx`, `WorkspacePage.tsx` |
| Fix 8 (Open in IDE) | §9 | S | `BottomDrawer.tsx`, `WorkspacePage.tsx` |

### §10.2 Phase 2 — Branch Switching (Fix 3)

Branch switching is the highest-impact change. It touches the RepoBar
subcomponent and the task selection flow:

| Fix | Section | Effort | Files Modified |
|-----|---------|--------|----------------|
| Fix 3 (branch picker + task→branch sync) | §4 | L | `ReposBar.tsx`, `WorkspacePage.tsx`, `BottomDrawer.tsx` |

### §10.3 Phase 3 — Takeover Mode (Fix 7)

Takeover mode requires a new hook and careful sequencing. Implement after
Phase 1/2 are stable:

| Fix | Section | Effort | Files Modified |
|-----|---------|--------|----------------|
| Fix 7 (takeover = pause + attach) | §8 | L | `useChat.ts`, `WorkspacePage.tsx`, `BottomDrawer.tsx`, `IntervenePanel.tsx` |

### §10.4 Phase 4 — URL Deep-Linking (Fix 9)

Shareable URLs with view state. Can be implemented in parallel with Phase 2/3
since the URL sync is purely additive:

| Fix | Section | Effort | Files Modified |
|-----|---------|--------|----------------|
| Fix 9 (URL deep-link view params) | §13 | M | `WorkspacePage.tsx`, `BottomDrawer.tsx` |

---

## §11 Test Plan Summary

### §11.1 Unit Tests (vitest + @testing-library/react)

| Suite | File | Tests |
|-------|------|-------|
| BottomDrawer wiring | `TaskQueue.test.tsx` (extend) | View Dialog, Open in IDE, loading/error states, optimistic UI, takeover banner |
| Timeline controls | `Timeline.test.tsx` (new) | Action bar rendering, infinite scroll sentinel, onLoadMore calls |
| IntervenePanel | `IntervenePanel.test.tsx` (new) | Takeover button, Return Control, message send, button disable states |
| useJobs mutations | `useJobs.test.ts` (extend) | Optimistic updates, rollback on error, cache invalidation |
| useAttachSession | `useChat.test.ts` (extend) | Attach endpoint call, error handling |
| URL deep-linking | `WorkspacePage.test.tsx` (extend) | Tab/branch/session/takeover params restore view, tab clicks update URL, Copy Link copies URL |

### §11.2 Integration Tests

| Test | Purpose |
|------|---------|
| Task deep-link renders full drawer | Navigate to `/:org/:ws/job/:jid/task/:tid` → drawer opens, correct tab data. |
| Branch switch on task select | Select task → active branch changes → file tree updates. |
| Takeover flow end-to-end | Click Take Over → pause mutation + attach mutation + branch switch + chat open. |

### §11.3 Contract Tests

| Test | Purpose |
|------|---------|
| Task pause/resume/abort response shape | Validates `TaskActionResponse` DTO. |
| Session attach response shape | Validates `AttachSessionResponseDTO`. |
| Timeline pagination cursor contract | Validates `{ entries, cursor, hasMore }` shape. |

---

## §12 Data Type Changes

### §12.1 New Props on BottomDrawer

```typescript
export interface BottomDrawerProps {
  // ... existing props ...
  
  // Fix 1: Session attachment
  onViewSession?: (sessionId: string) => void;
  
  // Fix 5: Changes tab states
  diffState?: AsyncState<DiffData[]>;
  checkpointsState?: AsyncState<TaskCheckpoint[]>;
  
  // Fix 6: Optimistic UI
  isPausing?: boolean;
  isResuming?: boolean;
  isAborting?: boolean;
  
  // Fix 7: Takeover
  isTakeover?: boolean;
  takeoverTaskId?: string | null;
  onTakeOver?: (taskId: string) => void;
  onReturnControl?: (taskId: string) => void;
  
  // Fix 8: Open in IDE
  onOpenExternal?: (taskId: string) => void;
  
  // Fix 9: URL deep-linking
  initialTab?: TabId;
  onTabChange?: (tab: TabId) => void;
}
```

### §12.2 New Props on TaskTimelineView

```typescript
export interface TaskTimelineViewProps {
  // ... existing props ...
  
  // Fix 4 (§5): Infinite scroll
  onLoadMore?: () => void;
  hasMore?: boolean;
  isLoadingMore?: boolean;
}
```

### §12.3 New Props on IntervenePanel

```typescript
export interface IntervenePanelProps {
  // ... existing props ...
  
  // Fix 6 (§7): Loading states
  isPausing?: boolean;
  isResuming?: boolean;
  isAborting?: boolean;
  
  // Fix 7 (§8): Takeover
  isTakeover?: boolean;
  onTakeOver?: (taskId: string) => void;
  onReturnControl?: (taskId: string) => void;
}
```

### §12.4 New Hook

```typescript
// In useChat.ts
export function useAttachSession(): MutationHook<string, AttachSessionResponse> {
  // POST /api/v1/sessions/:id/attach
  // On success: subscribe to WS channel `chat:<sessionId>`
}

interface AttachSessionResponse {
  attached: boolean;
  sessionId: string;
}
```

---

## §13 URL Deep-Linking — Shareable View State

### §13.1 Problem

Currently, navigating to `/:org/:workspace/job/:jobId/task/:taskId` opens the
bottom drawer and selects the task, but:

1. The **active drawer tab** (`dag`|`changes`|`timeline`|`terminal`|`intervene`)
   is pure `useState('dag')` — always resets to DAG. A colleague receiving the
   link cannot see the same tab the sender was viewing.
2. **Takeover mode** is not reflected in the URL. A link cannot say "look at
   my takeover state."
3. **Active branch** is local state. A link doesn't say "look at branch
   `task/abc-123`."
4. The **chat session** viewed in the right panel is not in the URL.

Users should be able to copy a URL and share it with a coworker who then sees
the same panel configuration (assuming they have workspace access).

### §13.2 Design — Query-Parameter-Based View State

Encode transient view state as **query parameters** on the existing route.
This keeps route paths clean (resource identity) while allowing view hints to
be appended:

```
/:org/:workspace/job/:jobId/task/:taskId?tab=intervene&branch=branch-abc&session=sess-xyz&takeover=1
```

**Parameters:**

| Param | Values | Default | Purpose |
|-------|--------|---------|---------|
| `tab` | `dag`, `changes`, `timeline`, `terminal`, `intervene` | `dag` | Active drawer tab |
| `branch` | branch ID | (auto from task) | Active workspace branch |
| `session` | session ID | (none) | Chat session open in right panel |
| `takeover` | `1` | (absent) | Whether takeover mode is active |

All parameters are **optional hints**. If absent, the UI falls back to
defaults. If the referenced resource doesn't exist or the user lacks access,
the hint is silently ignored (same as how invalid `taskId` currently works).

### §13.3 Sync Strategy

**URL → State (on mount / navigation):**

The deep-link resolution `useEffect` in `WorkspacePage` (around line 1660)
already handles `jobId` and `routeTaskId`. Extend it:

```typescript
const [searchParams] = useSearchParams();

useEffect(() => {
  // ... existing jobId / routeTaskId handling ...
  
  if (jobId) {
    setBottomDrawerOpen(true);
    if (routeTaskId) setSelectedTaskId(routeTaskId);
    
    // Tab hint
    const tabHint = searchParams.get('tab');
    if (tabHint && VALID_TABS.includes(tabHint)) {
      setInitialDrawerTab(tabHint as TabId);     // new prop on BottomDrawer
    }
    
    // Branch hint
    const branchHint = searchParams.get('branch');
    if (branchHint) setActiveBranchId(branchHint);
    
    // Session hint
    const sessionHint = searchParams.get('session');
    if (sessionHint) {
      setActiveChatSessionId(sessionHint);
      setRightPanelOpen(true);
    }
    
    // Takeover hint — does NOT auto-enter takeover for the link recipient.
    // It only opens the Intervene tab so the coworker can see the task state.
    // The actual takeoverTaskId is set only by handleTakeOver() (which calls
    // pause + attach). If the original user is still attached, the banner
    // reflects the task's paused status naturally via task.status. 
    const isTakeoverHint = searchParams.get('takeover') === '1';
    if (isTakeoverHint) {
      setInitialDrawerTab('intervene');
    }
  }
}, [jobId, routeTaskId, searchParams, ...]);
```

**State → URL (on user interaction):**

When the user changes view state, update query params with `replace: true`
so Back/Forward still work but don't create excessive history entries:

```typescript
const updateViewParams = useCallback(
  (updates: Record<string, string | null>) => {
    if (!org || !workspaceSlug) return;
    const params = new URLSearchParams(searchParams);
    for (const [key, value] of Object.entries(updates)) {
      if (value === null) params.delete(key);
      else params.set(key, value);
    }
    const qs = params.toString();
    const basePath = selectedTaskId
      ? /* find jobId for this task, build /job/:jid/task/:tid */
        currentTaskPath
      : `/${org}/${workspaceSlug}`;
    navigate(`${basePath}${qs ? '?' + qs : ''}`, { replace: true });
  },
  [org, workspaceSlug, searchParams, navigate, selectedTaskId],
);
```

**Trigger points:**

| User Action | URL Update |
|-------------|-----------|
| Click drawer tab | `updateViewParams({ tab: newTab })` |
| Select branch in picker | `updateViewParams({ branch: newBranchId })` |
| "View Dialog" opens chat session | `updateViewParams({ session: sessionId })` |
| Activate Take Over | `updateViewParams({ takeover: '1' })` |
| Return Control | `updateViewParams({ takeover: null })` |
| Close drawer | Navigate to `/:org/:workspace` (strip job/task/params) |

### §13.4 BottomDrawer Tab Initialization

Currently `BottomDrawer` initializes with `useState<TabId>('dag')`. Change
this to accept an `initialTab` prop:

```typescript
interface BottomDrawerProps {
  // ...
  initialTab?: TabId;
  onTabChange?: (tab: TabId) => void;
}

// Inside BottomDrawer:
const [activeTab, setActiveTab] = useState<TabId>(initialTab ?? 'dag');

// Notify parent on tab change (for URL sync):
const handleTabChange = useCallback((tab: TabId) => {
  setActiveTab(tab);
  onTabChange?.(tab);
}, [onTabChange]);
```

### §13.5 Component Changes

| File | Change |
|------|--------|
| `WorkspacePage.tsx` | Add `useSearchParams()`. Read `tab`, `branch`, `session`, `takeover` params on mount. Add `updateViewParams()` helper. Wire `onTabChange` from BottomDrawer to URL sync. |
| `BottomDrawer.tsx` | Add `initialTab?: TabId` and `onTabChange?: (tab: TabId) => void` props. Initialize state from `initialTab`. Call `onTabChange` on tab clicks. |
| `WorkspacePage.tsx` | In `handleSelectTask`, `handleTakeOver`, `handleReturnControl`, and `onViewSession` — call `updateViewParams` with appropriate updates. |

### §13.6 "Copy Link" Button

Add a small **"Copy Link"** icon button in the BottomDrawer header (next to
the existing close/maximize buttons). Clicking it copies the current URL
(which already includes all view params thanks to the sync above) to the
clipboard with a brief tooltip confirmation.

```
┌────────────────────────────────────────────────────────┐
│ Task: Uplift auth module  [⏸] [⏹]  …  [🔗] [↕] [✕]  │
│                                        ^^^^            │
│                                  Copy Link button      │
└────────────────────────────────────────────────────────┘
```

### §13.7 Tests

| Test | Assertion |
|------|-----------|
| Deep-link with `?tab=intervene` opens Intervene tab | Navigate to URL → drawer opens on Intervene tab, not DAG. |
| Deep-link with `?session=X` opens chat panel | Navigate → right panel opens with session X active. |
| Deep-link with `?takeover=1` opens Intervene tab | Navigate → drawer opens on Intervene tab (does NOT auto-enter takeover for link recipient). |
| Tab click updates URL query param | Click Timeline tab → URL contains `?tab=timeline`. |
| Branch switch updates URL query param | Select branch → URL contains `?branch=<id>`. |
| Copy Link copies current URL | Click 🔗 → clipboard contains full URL with params. |
| Invalid `?tab=bogus` falls back to DAG | Navigate with bad param → drawer opens on DAG. |
| Missing params use defaults | Navigate without query params → normal behavior unchanged. |

---

## §14 Backward Compatibility

All new props are optional with sensible defaults:
- `onViewSession` — defaults to no-op (link hidden when absent)
- `diffState` / `checkpointsState` — falls back to resolved arrays if provided
- `isPausing` / `isResuming` / `isAborting` — defaults to `false`
- `isTakeover` — defaults to `false`
- `onTakeOver` / `onReturnControl` — defaults to no-op
- `onOpenExternal` — defaults to no-op (button hidden when absent)
- `onLoadMore` / `hasMore` — defaults to no pagination (no sentinel rendered)

Existing tests continue to pass without modification. New tests extend
existing suites or create new focused test files.
