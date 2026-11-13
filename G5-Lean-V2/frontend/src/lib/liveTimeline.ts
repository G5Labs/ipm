// =============================================================================
// G5 Client — live timeline rows ahead of saved history
// =============================================================================
// A running turn's output reaches the browser as `chat.timeline` rows before
// the API has saved them (chat g5.chat.live-delivery). A live row carries the
// same id, identity and shape as the row the API later saves, so live and
// saved rows are one fact (g5.chat.replay): the saved row replaces the live
// one, whether it arrives by a history read or as the stored row published
// after its history transaction committed; duplicates and reordered
// deliveries change nothing; and a live row is dropped only by its own saved
// copy, never because a saved row beyond its sequence exists, so a refresh
// that read the newest page alone keeps every earlier live row of the turn
// until a read brings its saved copy (g5.chat.live-tail-recovery). Every
// placement returns the held window itself when nothing changed, so callers
// never write an unchanged window back to the cache.
// =============================================================================

import type { Page, TimelineEntry } from "./types";
import { compareTimelineEntries } from "./providerOutput";

export type TimelineWindow = Page<TimelineEntry>;

/** A row delivered live, not yet read from saved history. */
export function isLiveTimelineRow(
  entry: Pick<TimelineEntry, "metadata">,
): boolean {
  return entry.metadata?.live === true;
}

/** The execution a journaled row belongs to and its dense sequence there. */
export function timelineRowExecution(
  entry: Pick<TimelineEntry, "metadata">,
): { attemptKey: string; seq: number } | null {
  const metadata = entry.metadata ?? {};
  const taskId = metadata.taskId;
  const attempt = metadata.attempt;
  const seq = metadata.seq;
  if (
    typeof taskId !== "string" ||
    typeof attempt !== "number" ||
    typeof seq !== "number"
  )
    return null;
  return { attemptKey: `${taskId}:${attempt}`, seq };
}

/**
 * The highest saved sequence held per execution. Persistence is contiguous,
 * so a saved row at sequence N proves every row at or below N is saved.
 */
export function persistedSequenceCeilings(
  rows: readonly TimelineEntry[],
): Map<string, number> {
  const ceilings = new Map<string, number>();
  for (const row of rows) {
    if (isLiveTimelineRow(row)) continue;
    const execution = timelineRowExecution(row);
    if (!execution) continue;
    const held = ceilings.get(execution.attemptKey) ?? -1;
    if (execution.seq > held) ceilings.set(execution.attemptKey, execution.seq);
  }
  return ceilings;
}

/**
 * A live row is superseded by its own saved copy alone. A saved row of the
 * same execution beyond its sequence proves the row is saved server-side,
 * not that the browser read it: the newest-page refresh stops at the first
 * held saved row, so the copies of earlier live rows may never arrive, and
 * dropping the rows would empty the turn's earlier output from the view.
 */
function liveRowSuperseded(
  entry: TimelineEntry,
  savedIds: Set<string>,
): boolean {
  return isLiveTimelineRow(entry) && savedIds.has(entry.id);
}

function savedIdsOf(rows: readonly TimelineEntry[]): Set<string> {
  const ids = new Set<string>();
  for (const row of rows) if (!isLiveTimelineRow(row)) ids.add(row.id);
  return ids;
}

/** Insert `entry` keeping chronological order; cheap for the common append. */
function insertOrdered(items: TimelineEntry[], entry: TimelineEntry): void {
  let index = items.length;
  while (index > 0 && compareTimelineEntries(items[index - 1], entry) > 0)
    index -= 1;
  items.splice(index, 0, entry);
}

/**
 * Drop the live rows whose saved copies `items` holds. Returns the same array
 * when nothing changes.
 */
export function reconcileLiveRows(items: TimelineEntry[]): TimelineEntry[] {
  const savedIds = savedIdsOf(items);
  if (!items.some((row) => liveRowSuperseded(row, savedIds))) return items;
  return items.filter((row) => !liveRowSuperseded(row, savedIds));
}

/**
 * Place rows into the held window. A live row whose id is held, live or
 * saved, is a duplicate, and so is a saved row whose id is held saved. A saved
 * row whose id is held live replaces that live row: it is the same fact, now
 * committed (the stored row of a provider request confirms it, chat
 * g5.chat.live-delivery). The rest take their chronological place, a live row
 * below the held saved rows included: it stands for its saved copy until a
 * read brings that copy. Returns the same window when nothing changes, so
 * callers can skip the cache write.
 */
export function insertLiveRows(
  window: TimelineWindow,
  incoming: readonly TimelineEntry[],
): TimelineWindow {
  const heldSaved = savedIdsOf(window.items);
  const heldLive = new Set<string>();
  for (const row of window.items)
    if (isLiveTimelineRow(row)) heldLive.add(row.id);
  let items: TimelineEntry[] | undefined;
  for (const row of incoming) {
    if (heldSaved.has(row.id)) continue;
    const live = isLiveTimelineRow(row);
    if (heldLive.has(row.id)) {
      if (live) continue;
      items ??= [...window.items];
      items.splice(
        items.findIndex((held) => held.id === row.id),
        1,
      );
      heldLive.delete(row.id);
    }
    items ??= [...window.items];
    if (live) heldLive.add(row.id);
    else heldSaved.add(row.id);
    insertOrdered(items, row);
  }
  return items ? { ...window, items } : window;
}

/**
 * After saved history was merged into `merged`, keep the live rows `held`
 * carried that `merged` did not return, and drop the ones whose saved copies
 * it did. A refresh therefore never removes live output, ahead of the page it
 * read or before it.
 */
export function retainLiveRows(
  held: TimelineWindow | undefined,
  merged: TimelineWindow,
): TimelineWindow {
  const mergedIds = new Set(merged.items.map((row) => row.id));
  const extras = (held?.items ?? []).filter(
    (row) => isLiveTimelineRow(row) && !mergedIds.has(row.id),
  );
  const reconciled = reconcileLiveRows(merged.items);
  const base =
    reconciled === merged.items ? merged : { ...merged, items: reconciled };
  return extras.length ? insertLiveRows(base, extras) : base;
}

/** The highest sequence held for one execution, saved or live; -1 when none. */
export function highestHeldSequence(
  rows: readonly TimelineEntry[],
  attemptKey: string,
): number {
  let highest = -1;
  for (const row of rows) {
    const execution = timelineRowExecution(row);
    if (execution?.attemptKey === attemptKey && execution.seq > highest)
      highest = execution.seq;
  }
  return highest;
}

/**
 * Whether a live row arriving at `entry` skips past the rows held for its
 * execution: its sequence is more than one above the highest held, so rows
 * in between were not delivered (a dropped burst, a late subscription). The
 * row is still placed; the caller resynchronizes from the tail read instead
 * of rendering past the hole (chat g5.chat.live-tail-recovery).
 */
export function liveRowOpensGap(
  held: readonly TimelineEntry[],
  entry: TimelineEntry,
): boolean {
  return liveRowsOpenGap(held, [entry]);
}

/**
 * Whether any live row of `incoming`, placed in order after `held`, skips
 * past the rows held for its execution (see `liveRowOpensGap`). Rows earlier
 * in the batch count as held for the later ones, so a contiguous batch opens
 * no gap and one batch with a hole signals once.
 */
export function liveRowsOpenGap(
  held: readonly TimelineEntry[],
  incoming: readonly TimelineEntry[],
): boolean {
  const highest = new Map<string, number>();
  let gap = false;
  for (const row of incoming) {
    const execution = timelineRowExecution(row);
    if (!execution) continue;
    let top = highest.get(execution.attemptKey);
    if (top === undefined)
      top = highestHeldSequence(held, execution.attemptKey);
    // A first row above 1 may also follow rows older than the window; one
    // tail read settles either case.
    if (isLiveTimelineRow(row) && execution.seq > Math.max(top, 0) + 1)
      gap = true;
    highest.set(execution.attemptKey, Math.max(top, execution.seq));
  }
  return gap;
}

// ---------------------------------------------------------------------------
// Tail read requests from outside React (the WebSocket placement path)
// ---------------------------------------------------------------------------

const liveTailListeners = new Map<string, Set<() => void>>();

/** Ask the session's tail reader for one resynchronizing read. */
export function requestLiveTail(sessionId: string): void {
  for (const listener of liveTailListeners.get(sessionId) ?? []) listener();
}

/** Subscribe the session's tail reader; returns the unsubscribe. */
export function onLiveTailRequested(
  sessionId: string,
  listener: () => void,
): () => void {
  let listeners = liveTailListeners.get(sessionId);
  if (!listeners) {
    listeners = new Set();
    liveTailListeners.set(sessionId, listeners);
  }
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
    if (!listeners.size) liveTailListeners.delete(sessionId);
  };
}
