/**
 * Holes in the held timeline that saved history already covers are closed
 * by sequence from saved history, read backward from the row after the hole
 * (chat g5.chat.live-tail-recovery); a hole above the persisted prefix is
 * left to the tail read; reads per run are bounded.
 */
import { describe, expect, it, vi } from "vitest";
import type { QueryClient } from "@tanstack/react-query";
import { http, HttpResponse } from "msw";
import { server } from "../test/msw/server";
import type { TimelineEntry } from "./types";
import type { TimelineWindow } from "./liveTimeline";
import {
  fillTimelineHoles,
  findCoveredHoles,
  requestTimelineHoleFill,
  timelineHoleFillInFlight,
} from "./timelineHoles";
import { timelineCursorBefore } from "./sessionTimeline";

const SESSION_ID = "session-holes";
const PAGE = 200;

function row(seq: number, live: boolean, task = "task-h"): TimelineEntry {
  const minute = Math.floor(seq / 60);
  const second = seq % 60;
  return {
    id: `${task}:1:${seq}`,
    entryType: "output",
    content: `w${String(seq).padStart(5, "0")} `,
    createdAt: `2026-10-30T03:${String(minute).padStart(2, "0")}:${String(second).padStart(2, "0")}.000000Z`,
    metadata: {
      schema_version: "g5.task-event.v1",
      kind: "message.delta",
      payload: { block_id: "b", text_delta: `w${seq} ` },
      taskId: task,
      attempt: 1,
      seq,
      sessionId: SESSION_ID,
      messageId: "m1",
      ...(live ? { live: true } : {}),
    },
  };
}

function rows(from: number, to: number, live = false): TimelineEntry[] {
  const out: TimelineEntry[] = [];
  for (let seq = from; seq <= to; seq += 1) out.push(row(seq, live));
  return out;
}

/** A server holding saved rows 1..`persisted`, paging backward by cursor. */
function pagedServer(persisted: number) {
  const saved = rows(1, persisted);
  const reads: string[] = [];
  const readPage = vi.fn(async (cursor: string): Promise<TimelineWindow> => {
    reads.push(cursor);
    const [, id] = cursor.split("|");
    const index = saved.findIndex((entry) => entry.id === id);
    const end = index === -1 ? saved.length : index;
    const start = Math.max(0, end - PAGE);
    const page = saved.slice(start, end);
    return {
      items: page,
      cursor: start > 0 && page.length ? timelineCursorBefore(page[0]) : null,
      hasMore: start > 0,
    };
  });
  return { readPage, reads };
}

function cache(window: TimelineWindow) {
  const store = new Map<string, unknown>([
    [JSON.stringify(["session-timeline", SESSION_ID]), window],
  ]);
  const setQueryData = vi.fn((key: unknown[], next: unknown) => {
    store.set(JSON.stringify(key), next);
    return next;
  });
  const queryClient = {
    getQueryData: (key: unknown[]) => store.get(JSON.stringify(key)),
    setQueryData,
  } as unknown as QueryClient;
  const held = () =>
    store.get(
      JSON.stringify(["session-timeline", SESSION_ID]),
    ) as TimelineWindow;
  return { queryClient, setQueryData, held };
}

function seqs(window: TimelineWindow): number[] {
  return window.items.map((item) => item.metadata.seq as number);
}

describe("findCoveredHoles", () => {
  it("reports a skipped range below a held saved row, oldest first, and ignores the window floor", () => {
    const items = [...rows(50, 113), ...rows(170, 200, true), row(400, false)];
    expect(findCoveredHoles(items)).toEqual([
      expect.objectContaining({ from: 114, to: 169, after: row(170, true) }),
      expect.objectContaining({ from: 201, to: 399, after: row(400, false) }),
    ]);
  });

  it("a hole above the persisted prefix is not covered unless the tail's watermark reaches it", () => {
    const items = [...rows(1, 5), ...rows(8, 9, true)];
    expect(findCoveredHoles(items)).toEqual([]);
    expect(findCoveredHoles(items, new Map([["task-h:1", 5]]))).toEqual([]);
    expect(
      findCoveredHoles(items, new Map([["task-h:1", 6]])).map((hole) => [
        hole.from,
        hole.to,
      ]),
    ).toEqual([[6, 7]]);
  });

  it("a contiguous window has no holes", () => {
    expect(findCoveredHoles([...rows(1, 20), ...rows(21, 25, true)])).toEqual(
      [],
    );
  });
});

describe("fillTimelineHoles", () => {
  it("fills a hole below the saved floor by reading backward from the row after it", async () => {
    const { queryClient, setQueryData, held } = cache({
      items: [...rows(1, 113), ...rows(170, 200, true), row(400, false)],
      cursor: null,
      hasMore: true,
    });
    const { readPage, reads } = pagedServer(400);
    const outcome = await fillTimelineHoles({
      queryClient,
      sessionId: SESSION_ID,
      readPage,
    });
    // One page per hole: the rows older than 170 cover 114..169, the rows
    // older than 400 cover 201..399 (and replace the live 200).
    expect(outcome).toEqual({ pages: 2, remaining: 0 });
    expect(reads).toEqual([
      timelineCursorBefore(row(170, true)),
      timelineCursorBefore(row(400, false)),
    ]);
    expect(setQueryData).toHaveBeenCalledTimes(2);
    const window = held();
    expect(seqs(window)).toEqual(
      Array.from({ length: 400 }, (_, index) => index + 1),
    );
    // Rows the server answered outside the holes were not placed: the
    // window's floor and its cursor are untouched.
    expect(window.cursor).toBeNull();
    expect(window.hasMore).toBe(true);
    // Live rows the holes did not cover stay live until a read brings
    // their saved copies.
    const live = window.items.filter((item) => item.metadata.live === true);
    expect(live.map((item) => item.metadata.seq)).toEqual(
      Array.from({ length: 31 }, (_, index) => 170 + index),
    );
  });

  it("a hole above the saved floor is left to the tail read; the tail's watermark covers it", async () => {
    const { queryClient, setQueryData, held } = cache({
      items: [...rows(1, 5), ...rows(8, 9, true)],
      cursor: null,
      hasMore: false,
    });
    const { readPage } = pagedServer(7);
    expect(
      await fillTimelineHoles({ queryClient, sessionId: SESSION_ID, readPage }),
    ).toEqual({ pages: 0, remaining: 0 });
    expect(readPage).not.toHaveBeenCalled();
    expect(setQueryData).not.toHaveBeenCalled();
    expect(
      await fillTimelineHoles({
        queryClient,
        sessionId: SESSION_ID,
        readPage,
        persisted: new Map([["task-h:1", 7]]),
      }),
    ).toEqual({ pages: 1, remaining: 0 });
    expect(seqs(held())).toEqual([1, 2, 3, 4, 5, 6, 7, 8, 9]);
  });

  it("reads nothing for a contiguous window", async () => {
    const { queryClient } = cache({
      items: [...rows(1, 20), ...rows(21, 25, true)],
      cursor: null,
      hasMore: false,
    });
    const { readPage } = pagedServer(20);
    expect(
      await fillTimelineHoles({ queryClient, sessionId: SESSION_ID, readPage }),
    ).toEqual({ pages: 0, remaining: 0 });
    expect(readPage).not.toHaveBeenCalled();
  });

  it("a wide hole is read page by page, bounded per run, and finished by the next run", async () => {
    const { queryClient, held } = cache({
      items: [...rows(1, 10), row(1000, false)],
      cursor: null,
      hasMore: false,
    });
    const { readPage } = pagedServer(1000);
    const first = await fillTimelineHoles({
      queryClient,
      sessionId: SESSION_ID,
      readPage,
      maxPages: 3,
    });
    expect(first).toEqual({ pages: 3, remaining: 1 });
    expect(seqs(held()).length).toBe(11 + 3 * PAGE);
    const second = await fillTimelineHoles({
      queryClient,
      sessionId: SESSION_ID,
      readPage,
      maxPages: 3,
    });
    expect(second).toEqual({ pages: 2, remaining: 0 });
    expect(seqs(held())).toEqual(
      Array.from({ length: 1000 }, (_, index) => index + 1),
    );
  });

  it("a hole the server does not return stops after the bound without a busy loop", async () => {
    const { queryClient, setQueryData } = cache({
      items: [...rows(1, 10), row(50, false)],
      cursor: null,
      hasMore: false,
    });
    // The server answers rows of another task only, with more behind.
    const readPage = vi.fn(
      async (): Promise<TimelineWindow> => ({
        items: [row(1, false, "task-other")],
        cursor: "older",
        hasMore: true,
      }),
    );
    const outcome = await fillTimelineHoles({
      queryClient,
      sessionId: SESSION_ID,
      readPage,
      maxPages: 4,
    });
    expect(outcome).toEqual({ pages: 4, remaining: 1 });
    expect(readPage).toHaveBeenCalledTimes(4);
    expect(setQueryData).not.toHaveBeenCalled();
  });

  it("one fill runs per session; a request during a run schedules one more", async () => {
    const { queryClient, held } = cache({
      items: [...rows(1, 10), row(50, false)],
      cursor: null,
      hasMore: false,
    });
    const requests: string[] = [];
    server.use(
      http.get("/api/v1/sessions/:id/timeline", ({ request }) => {
        const cursor = new URL(request.url).searchParams.get("cursor") ?? "";
        requests.push(cursor);
        // The first run finds the rows missing still; the next one finds them.
        const entries = requests.length === 1 ? [] : rows(11, 49);
        return HttpResponse.json({
          data: { entries, cursor: null, hasMore: false },
        });
      }),
    );
    const first = requestTimelineHoleFill(queryClient, SESSION_ID);
    expect(timelineHoleFillInFlight(SESSION_ID)).toBe(true);
    const second = requestTimelineHoleFill(queryClient, SESSION_ID);
    const third = requestTimelineHoleFill(queryClient, SESSION_ID);
    expect(second).toBe(first);
    expect(third).toBe(first);
    await first;
    expect(timelineHoleFillInFlight(SESSION_ID)).toBe(false);
    // One run, then one more for the requests made during it: two reads,
    // both from the row after the hole, and the hole is closed.
    expect(requests).toEqual([
      timelineCursorBefore(row(50, false)),
      timelineCursorBefore(row(50, false)),
    ]);
    expect(seqs(held())).toEqual(
      Array.from({ length: 50 }, (_, index) => index + 1),
    );
  });
});
