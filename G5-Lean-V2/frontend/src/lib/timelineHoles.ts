// =============================================================================
// G5 Client — holes in the held timeline that saved history already covers
// =============================================================================
// A journaled row carries a dense per-attempt sequence (chat g5.chat.replay).
// When the held window skips sequences of one attempt between two rows it
// holds, and the API has persisted the skipped rows (a saved row of that
// attempt at or beyond them is held, or the tail read reported persistence
// past them), the rows exist on the server but reached neither the live
// channel nor a history read: a burst the fan-out dropped for a slow client
// was persisted before the tail was read, and the newest-window refresh
// stopped at the first row it recognized. Such a hole is closed by reading
// saved history backward from the row after it until the hole's rows are
// held, bounded per run (chat g5.chat.live-tail-recovery). A hole above the
// persisted prefix is the tail read's, not this module's.
// =============================================================================

import type { QueryClient } from "@tanstack/react-query";
import type { TimelineEntry } from "./types";
import {
  insertLiveRows,
  isLiveTimelineRow,
  persistedSequenceCeilings,
  timelineRowExecution,
  type TimelineWindow,
} from "./liveTimeline";
import {
  readSessionTimelinePage,
  timelineCursorBefore,
} from "./sessionTimeline";

/** Missing sequences `from..to` of one attempt, below the held row `after`. */
export type SequenceHole = {
  attemptKey: string;
  from: number;
  to: number;
  after: TimelineEntry;
};

/** Pages one fill run reads at most; a later trigger continues. */
export const HOLE_FILL_MAX_PAGES = 8;

type HoleQueryClient = Pick<QueryClient, "getQueryData" | "setQueryData">;

/**
 * The holes in `items` that persistence covers at least in part: between two
 * held rows of one attempt whose sequences are not consecutive, when the
 * attempt's persisted prefix (its saved rows held here, or `persisted`, the
 * tail read's watermark) reaches the hole. Ordered oldest first within an
 * attempt. A window's first row above sequence 1 is its floor, not a hole.
 */
export function findCoveredHoles(
  items: readonly TimelineEntry[],
  persisted?: ReadonlyMap<string, number>,
): SequenceHole[] {
  const byAttempt = new Map<string, { seq: number; row: TimelineEntry }[]>();
  for (const row of items) {
    const execution = timelineRowExecution(row);
    if (!execution) continue;
    let rows = byAttempt.get(execution.attemptKey);
    if (!rows) {
      rows = [];
      byAttempt.set(execution.attemptKey, rows);
    }
    rows.push({ seq: execution.seq, row });
  }
  const ceilings = persistedSequenceCeilings(items);
  const holes: SequenceHole[] = [];
  for (const [attemptKey, rows] of byAttempt) {
    rows.sort((a, b) => a.seq - b.seq);
    const covered = Math.max(
      ceilings.get(attemptKey) ?? 0,
      persisted?.get(attemptKey) ?? 0,
    );
    for (let index = 1; index < rows.length; index += 1) {
      const previous = rows[index - 1].seq;
      const next = rows[index].seq;
      if (next > previous + 1 && covered > previous) {
        holes.push({
          attemptKey,
          from: previous + 1,
          to: next - 1,
          after: rows[index].row,
        });
      }
    }
  }
  return holes;
}

function holeKey(hole: SequenceHole): string {
  return `${hole.attemptKey}:${hole.from}`;
}

/** The saved rows of `page` that belong to `hole`. */
function rowsOfHole(
  page: TimelineWindow,
  hole: SequenceHole,
): { inside: TimelineEntry[]; passed: boolean } {
  const inside: TimelineEntry[] = [];
  let passed = false;
  for (const row of page.items) {
    if (isLiveTimelineRow(row)) continue;
    const execution = timelineRowExecution(row);
    if (!execution || execution.attemptKey !== hole.attemptKey) continue;
    if (execution.seq >= hole.from && execution.seq <= hole.to)
      inside.push(row);
    else if (execution.seq < hole.from) passed = true;
  }
  return { inside, passed };
}

export type HoleFillOutcome = {
  /** Pages of saved history read. */
  pages: number;
  /** Covered holes still open when the run ended (bound reached, or the
   * server did not return their rows). */
  remaining: number;
};

/**
 * Close the covered holes of the session's held window: for each, read saved
 * history backward from the row after it, place the hole's saved rows (a saved
 * row replaces the live row of its id), and go on until no covered hole
 * remains, `maxPages` were read, or a hole's rows were not returned. Rows the
 * server answers outside the hole are not placed: the window's floor and its
 * older-pages cursor are untouched.
 */
export async function fillTimelineHoles(options: {
  queryClient: HoleQueryClient;
  sessionId: string;
  readPage?: (cursor: string) => Promise<TimelineWindow>;
  persisted?: ReadonlyMap<string, number>;
  maxPages?: number;
}): Promise<HoleFillOutcome> {
  const { queryClient, sessionId, persisted } = options;
  const maxPages = options.maxPages ?? HOLE_FILL_MAX_PAGES;
  const readPage =
    options.readPage ??
    ((cursor: string) => readSessionTimelinePage(sessionId, cursor));
  const key = ["session-timeline", sessionId];
  const unfilled = new Set<string>();
  let pages = 0;
  for (;;) {
    const held = queryClient.getQueryData<TimelineWindow>(key);
    if (!held) return { pages, remaining: 0 };
    const holes = findCoveredHoles(held.items, persisted).filter(
      (hole) => !unfilled.has(holeKey(hole)),
    );
    if (!holes.length) return { pages, remaining: 0 };
    if (pages >= maxPages) return { pages, remaining: holes.length };
    const hole = holes[0];
    let cursor = timelineCursorBefore(hole.after);
    let placed = false;
    while (pages < maxPages) {
      const page = await readPage(cursor);
      pages += 1;
      const { inside, passed } = rowsOfHole(page, hole);
      if (inside.length) {
        const current = queryClient.getQueryData<TimelineWindow>(key);
        if (!current) return { pages, remaining: 0 };
        const next = insertLiveRows(current, inside);
        if (next !== current) queryClient.setQueryData(key, next);
        placed = true;
        break;
      }
      // Rows of other tasks may lie between the hole and the row after it;
      // read on while the server has older rows and the hole is not yet
      // behind us.
      if (passed || !page.hasMore || !page.cursor || !page.items.length) break;
      cursor = page.cursor;
    }
    if (!placed) {
      // The bound ended the run with this hole open; a later signal goes on.
      if (pages >= maxPages) return { pages, remaining: holes.length };
      unfilled.add(holeKey(hole));
    }
  }
}

// ---------------------------------------------------------------------------
// One fill in flight per session
// ---------------------------------------------------------------------------

const fillsInFlight = new Map<
  string,
  {
    run: Promise<void>;
    again: boolean;
    persisted?: ReadonlyMap<string, number>;
  }
>();

/**
 * Ask for the session's covered holes to be filled. One run per session is in
 * flight at a time; a request during a run schedules one more run after it,
 * so a signal is never lost and never piles up.
 */
export function requestTimelineHoleFill(
  queryClient: HoleQueryClient,
  sessionId: string,
  persisted?: ReadonlyMap<string, number>,
): Promise<void> {
  const inFlight = fillsInFlight.get(sessionId);
  if (inFlight) {
    inFlight.again = true;
    inFlight.persisted = persisted ?? inFlight.persisted;
    return inFlight.run;
  }
  const state = { again: false, persisted, run: Promise.resolve() };
  state.run = (async () => {
    try {
      do {
        state.again = false;
        try {
          await fillTimelineHoles({
            queryClient,
            sessionId,
            persisted: state.persisted,
          });
        } catch {
          // A failed read leaves the hole for the next signal.
        }
      } while (state.again);
    } finally {
      fillsInFlight.delete(sessionId);
    }
  })();
  fillsInFlight.set(sessionId, state);
  return state.run;
}

/** Test hook: whether a fill is in flight for the session. */
export function timelineHoleFillInFlight(sessionId: string): boolean {
  return fillsInFlight.has(sessionId);
}
