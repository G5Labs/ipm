# Reorder workspace tabs — issue #820

Request: [G5-Lean#820](https://github.com/G5Labs/G5-Lean/issues/820), "Allow dragging workspace
tabs to reorder them, with Home fixed on the left" (opened 2026-10-25). The issue is the durable
intent source; no separate chat intent is recorded.

## Ontology changes

| ID | Change |
|---|---|
| [`g5.workbench.tabs.reorder`](../../ontology/features/ui.g5.md#g5.workbench.tabs.reorder) | New (human-provided, issue #820). Drag reorder from right after Home to last; Home fixed under overflow; selection, URL and return state kept; stored order and organization scope; latest-list moves that never resurrect a tab; cancellable drags. |
| [`g5.workbench.tabs.reorder.keyboard`](../../ontology/features/ui.g5.md#g5.workbench.tabs.reorder.keyboard) | New (agent). Ctrl/Cmd+Shift+Left/Right moves the focused tab. |
| [`g5.workbench.tabs.reorder.anchoring`](../../ontology/features/ui.g5.md#g5.workbench.tabs.reorder.anchoring) | New (agent). Before the first still-listed tab shown to the right, else last in the organization; unlisted tabs never written. |
| [`g5.workbench.tabs.reorder.cancel`](../../ontology/features/ui.g5.md#g5.workbench.tabs.reorder.cancel) | New (agent). Escape, pointer cancel or a cross-window list change ends a drag without a move. |
| [`g5.workbench.tabs.reorder.touch`](../../ontology/features/ui.g5.md#g5.workbench.tabs.reorder.touch) | New (agent). Touch keeps scrolling and does not drag. |
| [`g5.workbench.tabs.reorder.eviction`](../../ontology/features/ui.g5.md#g5.workbench.tabs.reorder.eviction) | New (agent). The cap still evicts the leftmost tab. |
| [`g5.workbench.tabs`](../../ontology/features/ui.g5.md#g5.workbench.tabs) | Links the new block; otherwise unchanged. |

Governing and unchanged: `g5.workbench.tabs` (per-person/browser list, case-insensitive identity,
organization scope, transient tab for a workspace closed elsewhere, left-neighbour close, return
memory), [`g5.workbench.tabs.rename`](../../ontology/features/ui.g5.md#g5.workbench.tabs.rename)
(position kept through a rename) and [`g5.ui.accessibility`](../../ontology/features/ui.g5.md#g5.ui.accessibility).

## Design

**Owner.** The existing provider (`useWorkspaceTabs`) stays the single owner of the ordered list,
its storage and its cross-window synchronization. It gains one operation, `moveTab(tab, index)`;
the strip owns only the gesture (pointer, keyboard, preview, auto-scroll). No backend, schema or
new storage: order is the stored list's order, which already defines strip order, survives reload
and is shared by the person's browser tabs through the storage event.

**Applying a move.** The strip knows positions in the *displayed* list, which can hold a transient
tab that is not stored and omits other organizations' tabs. Persisting the displayed list would
resurrect a tab closed elsewhere and drop other organizations' tabs, so the provider translates the
drop into an anchor and applies it to the list as stored at the drop, read afresh rather than taken
from its in-memory copy, which lags another browser tab's write until the storage event arrives
(a CodeRabbit finding on PR #821):

- the moved entry goes before the first tab shown to its right that is still stored in its
  organization, else after that organization's last stored tab;
- the stored entry itself moves, keeping its route kind (personal routes) and spelling;
- a tab that is no longer stored (closed or renamed in another browser tab, or the transient tab)
  leaves the list unchanged.

The pure step is `moveWorkspaceTab` in `lib/workspaceTabs.ts`. Considered and rejected: an index
into the stored list (wrong when the displayed and stored lists differ) and a single "before" anchor
(a transient or concurrently closed right neighbour would abort or misplace a valid move).

**Gesture.** A press on a workspace tab's button becomes a drag past 5 px. The tabs are measured
once; the dragged tab follows the pointer within the tabs' span and passed tabs shift aside with
CSS transforms, so the DOM is not reordered mid-drag and the pressed button keeps its pointer
capture. It passes a neighbour when its leading edge crosses the neighbour's midpoint, which lets it
reach the first and last places. The release commits one move; the click that follows a drag is
swallowed so it never selects. Escape, `pointercancel`, a lost capture or a change of the list's
keys (another browser tab) end the drag without a move. Holding the pointer near either edge of the
scroller scrolls it every frame. Touch is excluded so swiping still scrolls the strip.
Ctrl/Cmd+Shift+Left/Right moves the focused tab one place and keeps its focus, an accessible
alternative to dragging; plain arrows still only move focus.

**Home.** Home moves out of the scroller into the tablist beside it, so it never scrolls away and is
outside the sortable range. The New tab placeholder and "+" stay in the scroller after the workspace
tabs and are never measured as drop targets.

## Limits and acceptance conditions

- Leftmost eviction at the 30-tab cap now evicts a tab the person may have placed first.
- Concurrent reorders in two browser tabs are last-writer-wins on the whole list, as for every
  other tab mutation; a close in one is never undone by a move in the other.
- Moving tabs between organizations, detaching tabs and moving them between browser windows are out
  of scope (issue).
