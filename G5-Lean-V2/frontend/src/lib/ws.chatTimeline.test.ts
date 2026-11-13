/**
 * The real `WebSocketManager` placing `chat.timeline` live rows into the
 * session timeline window (chat g5.chat.live-delivery, g5.chat.replay). Rows
 * are placed once per animation frame; the tests flush the frame.
 */
import { afterEach, describe, expect, it, vi } from "vitest";
import type { QueryClient } from "@tanstack/react-query";
import { WebSocketManager, flushLiveTimelineRows, type WSEvent } from "./ws";
import type { TimelineEntry } from "./types";
import * as liveTimeline from "./liveTimeline";
import { onLiveTailRequested, type TimelineWindow } from "./liveTimeline";

const SESSION_ID = "session-live";

function liveEntry(seq: number, overrides: Record<string, unknown> = {}) {
  return {
    id: `task-live:1:${seq}`,
    entryType: "output",
    content: `delta ${seq}`,
    createdAt: `2026-10-30T01:00:${String(seq).padStart(2, "0")}.000Z`,
    metadata: {
      schema_version: "g5.task-event.v1",
      kind: "message.delta",
      payload: { block_id: "b1", text_delta: `delta ${seq}`, message_id: "m1" },
      live: true,
      seq,
      attempt: 1,
      taskId: "task-live",
      sessionId: SESSION_ID,
      messageId: "m1",
      ...overrides,
    },
  };
}

function savedEntry(seq: number): TimelineEntry {
  const { metadata, ...rest } = liveEntry(seq);
  const saved = { ...metadata } as Record<string, unknown>;
  delete saved.live;
  return { ...rest, metadata: saved };
}

function makeManager(seed: TimelineWindow | undefined) {
  const store = new Map<string, unknown>();
  if (seed) store.set(JSON.stringify(["session-timeline", SESSION_ID]), seed);
  const setQueryData = vi.fn((key: unknown[], updater: unknown) => {
    const k = JSON.stringify(key);
    const prev = store.get(k);
    const next =
      typeof updater === "function"
        ? (updater as (old: unknown) => unknown)(prev)
        : updater;
    store.set(k, next);
    return next;
  });
  const queryClient = {
    invalidateQueries: vi.fn(),
    getQueryData: (key: unknown[]) => store.get(JSON.stringify(key)),
    setQueryData,
  } as unknown as QueryClient;
  const manager = new WebSocketManager({
    getBaseUrl: () => "ws://test",
    getWsTicket: () => Promise.resolve("ticket"),
    queryClient,
  });
  const deliver = (event: WSEvent) =>
    (manager as unknown as { handleEvent: (e: WSEvent) => void }).handleEvent(
      event,
    );
  /** Deliver one event and place it, as the next frame would. */
  const dispatch = (event: WSEvent) => {
    deliver(event);
    flushLiveTimelineRows(SESSION_ID);
  };
  const window = () =>
    store.get(JSON.stringify(["session-timeline", SESSION_ID])) as
      | TimelineWindow
      | undefined;
  return { deliver, dispatch, window, queryClient, setQueryData };
}

afterEach(() => {
  flushLiveTimelineRows();
  vi.restoreAllMocks();
});

/** A provider request row, live or stored (the stored row carries status). */
function requestEntry(seq: number, stored: boolean) {
  const base = liveEntry(seq, {
    kind: "provider_request.opened",
    payload: {
      provider_request_id: `req-${seq}`,
      request_kind: "question",
      tool_name: "AskUserQuestion",
      display: { questions: [{ question: "Color?", options: [] }] },
    },
  });
  if (!stored) return base;
  const metadata = { ...base.metadata } as Record<string, unknown>;
  delete metadata.live;
  return {
    ...base,
    metadata: { ...metadata, nativeRequestStatus: "pending" },
  };
}

function chatTimeline(entry: unknown): WSEvent {
  return {
    type: "chat.timeline",
    sessionId: SESSION_ID,
    taskId: "task-live",
    entry,
  };
}

describe("WebSocketManager chat.timeline live rows", () => {
  it("places live rows into the held window, projected like saved rows, deduplicated and ordered", () => {
    const { dispatch, window, queryClient } = makeManager({
      items: [savedEntry(1)],
      cursor: null,
      hasMore: false,
    });
    dispatch(chatTimeline(liveEntry(3)));
    dispatch(chatTimeline(liveEntry(2)));
    dispatch(chatTimeline(liveEntry(3)));
    const items = window()!.items;
    expect(items.map((item) => item.id)).toEqual([
      "task-live:1:1",
      "task-live:1:2",
      "task-live:1:3",
    ]);
    // Projected through the shared journal projection: presentation fields
    // and the ordering sequence are present, the live mark is retained.
    expect(items[2].metadata.providerEvent).toBe("provided_agent.output.delta");
    expect(items[2].metadata.live).toBe(true);
    expect(items[2].sequence).toBe(3);
    // No refetch: the row is placed, not invalidated.
    expect(queryClient.invalidateQueries).not.toHaveBeenCalled();
  });

  it("a live row below the held saved rows is placed until its saved copy is read", () => {
    // Saved row 4 proves row 2 is saved server-side; the browser has not
    // read it, so the live row stands in for it rather than being dropped.
    const { dispatch, window } = makeManager({
      items: [savedEntry(1), savedEntry(4)],
      cursor: null,
      hasMore: false,
    });
    dispatch(chatTimeline(liveEntry(2)));
    expect(window()!.items.map((item) => item.id)).toEqual([
      "task-live:1:1",
      "task-live:1:2",
      "task-live:1:4",
    ]);
    dispatch(chatTimeline(savedEntry(2)));
    expect(
      window()!.items.map((item) => [item.id, item.metadata.live ?? false]),
    ).toEqual([
      ["task-live:1:1", false],
      ["task-live:1:2", false],
      ["task-live:1:4", false],
    ]);
  });

  it("a window not loaded yet is left to the tail read", () => {
    const { dispatch, window } = makeManager(undefined);
    dispatch(chatTimeline(liveEntry(1)));
    expect(window()).toBeUndefined();
  });

  it("a row that skips past the held sequence is placed and asks for one tail read", () => {
    const { dispatch, window } = makeManager({
      items: [savedEntry(1), savedEntry(2)],
      cursor: null,
      hasMore: false,
    });
    let requests = 0;
    const stop = onLiveTailRequested(SESSION_ID, () => {
      requests += 1;
    });
    try {
      dispatch(chatTimeline(liveEntry(3)));
      dispatch(chatTimeline(liveEntry(4)));
      expect(requests).toBe(0);
      dispatch(chatTimeline(liveEntry(7)));
      expect(requests).toBe(1);
      expect(window()!.items.map((item) => item.id)).toEqual([
        "task-live:1:1",
        "task-live:1:2",
        "task-live:1:3",
        "task-live:1:4",
        "task-live:1:7",
      ]);
      // The late rows fill the hole without another request.
      dispatch(chatTimeline(liveEntry(5)));
      dispatch(chatTimeline(liveEntry(6)));
      expect(requests).toBe(1);
      expect(window()!.items).toHaveLength(7);
    } finally {
      stop();
    }
  });

  it("an unchanged window is not written back to the cache", () => {
    const { dispatch, setQueryData } = makeManager({
      items: [savedEntry(1), liveEntry(2) as unknown as TimelineEntry],
      cursor: null,
      hasMore: false,
    });
    dispatch(chatTimeline(liveEntry(2)));
    dispatch(chatTimeline(savedEntry(1)));
    dispatch(chatTimeline(liveEntry(1)));
    expect(setQueryData).not.toHaveBeenCalled();
    dispatch(chatTimeline(liveEntry(3)));
    expect(setQueryData).toHaveBeenCalledTimes(1);
  });

  it("the stored row of a provider request replaces its live row by id (chat g5.chat.live-delivery)", () => {
    const { dispatch, window, setQueryData } = makeManager({
      items: [savedEntry(1)],
      cursor: null,
      hasMore: false,
    });
    dispatch(chatTimeline(requestEntry(2, false)));
    dispatch(chatTimeline(liveEntry(3)));
    let items = window()!.items;
    expect(items.map((item) => item.id)).toEqual([
      "task-live:1:1",
      "task-live:1:2",
      "task-live:1:3",
    ]);
    expect(items[1].metadata.live).toBe(true);
    expect(items[1].metadata.nativeRequestStatus).toBeUndefined();
    expect(items[1].metadata.providerEvent).toBe(
      "provided_agent.approval.requested",
    );

    // After its history transaction committed, the stored row arrives on the
    // same channel with the same id and without the live mark.
    dispatch(chatTimeline(requestEntry(2, true)));
    items = window()!.items;
    expect(items.map((item) => item.id)).toEqual([
      "task-live:1:1",
      "task-live:1:2",
      "task-live:1:3",
    ]);
    expect(items[1].metadata.live).toBeUndefined();
    expect(items[1].metadata.nativeRequestStatus).toBe("pending");
    expect(items[1].metadata.providerEvent).toBe(
      "provided_agent.approval.requested",
    );
    // Later live rows still stay; the stored row proved only its own prefix.
    expect(items[2].metadata.live).toBe(true);

    // The stored row again (both channels publish it) changes nothing.
    const writes = setQueryData.mock.calls.length;
    dispatch(chatTimeline(requestEntry(2, true)));
    expect(setQueryData).toHaveBeenCalledTimes(writes);
    expect(window()!.items).toBe(items);
  });

  it("a request rejected by its history transaction replaces the live row with its error", () => {
    const { dispatch, window } = makeManager({
      items: [savedEntry(1)],
      cursor: null,
      hasMore: false,
    });
    dispatch(chatTimeline(requestEntry(2, false)));
    const rejected = requestEntry(2, true);
    dispatch(
      chatTimeline({
        ...rejected,
        metadata: {
          ...rejected.metadata,
          nativeRequestStatus: undefined,
          projection_error: "provider request rejected: duplicate",
        },
      }),
    );
    const items = window()!.items;
    expect(items.map((item) => item.id)).toEqual([
      "task-live:1:1",
      "task-live:1:2",
    ]);
    expect(items[1].metadata.live).toBeUndefined();
    expect(items[1].metadata.providerEvent).toBe("provided_agent.error");
    expect(items[1].metadata.approvalId).toBeUndefined();
  });

  it("rows delivered within one frame are placed together: one placement, one cache write, in order", () => {
    const { deliver, window, setQueryData } = makeManager({
      items: [savedEntry(1)],
      cursor: null,
      hasMore: false,
    });
    const insert = vi.spyOn(liveTimeline, "insertLiveRows");
    for (const seq of [2, 3, 4, 5, 6, 7, 8, 9, 10, 11]) {
      deliver(chatTimeline(liveEntry(seq)));
    }
    // Nothing is placed until the frame.
    expect(setQueryData).not.toHaveBeenCalled();
    expect(insert).not.toHaveBeenCalled();
    expect(window()!.items).toHaveLength(1);
    flushLiveTimelineRows(SESSION_ID);
    expect(insert).toHaveBeenCalledTimes(1);
    expect(insert.mock.calls[0][1].map((row) => row.metadata.seq)).toEqual([
      2, 3, 4, 5, 6, 7, 8, 9, 10, 11,
    ]);
    expect(setQueryData).toHaveBeenCalledTimes(1);
    expect(window()!.items.map((item) => item.metadata.seq)).toEqual([
      1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11,
    ]);
    // A frame with nothing queued writes nothing.
    flushLiveTimelineRows(SESSION_ID);
    expect(setQueryData).toHaveBeenCalledTimes(1);
  });

  it("a batch with a hole asks for one tail read; a contiguous batch asks for none", () => {
    const { deliver, window } = makeManager({
      items: [savedEntry(1), savedEntry(2)],
      cursor: null,
      hasMore: false,
    });
    let requests = 0;
    const stop = onLiveTailRequested(SESSION_ID, () => {
      requests += 1;
    });
    try {
      deliver(chatTimeline(liveEntry(3)));
      deliver(chatTimeline(liveEntry(4)));
      flushLiveTimelineRows(SESSION_ID);
      expect(requests).toBe(0);
      // Rows 5 and 6 never arrive; 7, 8 and 9 come in one frame.
      deliver(chatTimeline(liveEntry(7)));
      deliver(chatTimeline(liveEntry(8)));
      deliver(chatTimeline(liveEntry(9)));
      flushLiveTimelineRows(SESSION_ID);
      expect(requests).toBe(1);
      expect(window()!.items.map((item) => item.metadata.seq)).toEqual([
        1, 2, 3, 4, 7, 8, 9,
      ]);
    } finally {
      stop();
    }
  });

  it("malformed rows are ignored", () => {
    const { dispatch, window } = makeManager({
      items: [],
      cursor: null,
      hasMore: false,
    });
    dispatch(chatTimeline(null));
    dispatch(chatTimeline({ entryType: "output" }));
    expect(window()!.items).toEqual([]);
  });
});
