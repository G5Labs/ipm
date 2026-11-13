/**
 * `useLiveTail` (chat g5.chat.live-tail-recovery): after saved history is
 * read while a turn is active, the unsaved tail is read and placed into the
 * held window; a catching-up tail is retried with backoff and never shown as
 * complete; persistence moving past the held prefix triggers one refresh.
 * Only reads of saved history, activation and gap signals trigger a tail
 * read; placing rows into the window never does.
 */
import { afterEach, describe, expect, it } from "vitest";
import React from "react";
import { act, renderHook, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { http, HttpResponse } from "msw";
import { server } from "../test/msw/server";
import type {
  ChatMessageDTO,
  LiveTailResponseDTO,
  TimelineEntry,
} from "../lib/types";
import { requestLiveTail, type TimelineWindow } from "../lib/liveTimeline";
import {
  WebSocketManager,
  applyLiveTimelineRow,
  flushLiveTimelineRows,
} from "../lib/ws";
import {
  useChatMessages,
  useSendChatMessage,
  useSessionTimeline,
} from "./useChat";
import { resetLiveTailRefreshMemory, useLiveTail } from "./useLiveTail";

const SESSION_ID = "session-tail";

function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false, staleTime: Infinity, gcTime: Infinity },
    },
  });
  const wrapper = ({ children }: { children: React.ReactNode }) =>
    React.createElement(QueryClientProvider, { client: queryClient }, children);
  return { wrapper, queryClient };
}

function entry(seq: number, live: boolean): TimelineEntry {
  return {
    id: `task-tail:1:${seq}`,
    entryType: "output",
    content: `delta ${seq}`,
    createdAt: `2026-10-30T02:00:${String(seq).padStart(2, "0")}.000Z`,
    metadata: {
      schema_version: "g5.task-event.v1",
      kind: "message.delta",
      payload: { block_id: "b", text_delta: `delta ${seq}` },
      taskId: "task-tail",
      attempt: 1,
      seq,
      sessionId: SESSION_ID,
      messageId: "m1",
      ...(live ? { live: true } : {}),
    },
  };
}

function installTail(answer: () => LiveTailResponseDTO | number) {
  const requests: number[] = [];
  server.use(
    http.get("/api/v1/sessions/:id/live-tail", () => {
      requests.push(Date.now());
      const result = answer();
      if (typeof result === "number")
        return new HttpResponse(null, { status: result });
      return HttpResponse.json({ data: result });
    }),
  );
  return requests;
}

afterEach(() => {
  resetLiveTailRefreshMemory();
});

describe("useLiveTail", () => {
  it("places a live tail into the held window and reports live", async () => {
    const { wrapper, queryClient } = createWrapper();
    queryClient.setQueryData<TimelineWindow>(["session-timeline", SESSION_ID], {
      items: [entry(1, false), entry(2, false)],
      cursor: null,
      hasMore: false,
    });
    const requests = installTail(() => ({
      state: "live",
      taskId: "task-tail",
      attempt: 1,
      persistedThrough: 2,
      fetchedThrough: 4,
      entries: [entry(3, true), entry(4, true)],
    }));
    const { result } = renderHook(() => useLiveTail(SESSION_ID, true, 1), {
      wrapper,
    });
    await waitFor(() => expect(result.current).toBe("live"));
    const held = queryClient.getQueryData<TimelineWindow>([
      "session-timeline",
      SESSION_ID,
    ]);
    expect(held?.items.map((item) => item.id)).toEqual([
      "task-tail:1:1",
      "task-tail:1:2",
      "task-tail:1:3",
      "task-tail:1:4",
    ]);
    expect(held?.items[2].metadata.live).toBe(true);
    expect(requests).toHaveLength(1);
    // The saved prefix matched what is held: no refresh was requested.
    expect(
      queryClient.getQueryState(["session-timeline", SESSION_ID])
        ?.isInvalidated,
    ).toBe(false);
  });

  it("re-reads saved history once when persistence moved past the held prefix", async () => {
    const { wrapper, queryClient } = createWrapper();
    queryClient.setQueryData<TimelineWindow>(["session-timeline", SESSION_ID], {
      items: [entry(1, false)],
      cursor: null,
      hasMore: false,
    });
    installTail(() => ({
      state: "live",
      taskId: "task-tail",
      attempt: 1,
      persistedThrough: 5,
      fetchedThrough: 6,
      entries: [entry(6, true)],
    }));
    const { result, rerender } = renderHook(
      ({ at }: { at: number }) => useLiveTail(SESSION_ID, true, at),
      { wrapper, initialProps: { at: 1 } },
    );
    await waitFor(() => expect(result.current).toBe("live"));
    await waitFor(() =>
      expect(
        queryClient.getQueryState(["session-timeline", SESSION_ID])
          ?.isInvalidated,
      ).toBe(true),
    );
    // The same watermark read again does not request another refresh.
    queryClient.setQueryData<TimelineWindow>(["session-timeline", SESSION_ID], {
      items: [entry(1, false)],
      cursor: null,
      hasMore: false,
    });
    rerender({ at: 2 });
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(
      queryClient.getQueryState(["session-timeline", SESSION_ID])
        ?.isInvalidated,
    ).toBe(false);
  });

  it("retries a catching-up tail with backoff until it is live", async () => {
    const { wrapper, queryClient } = createWrapper();
    queryClient.setQueryData<TimelineWindow>(["session-timeline", SESSION_ID], {
      items: [],
      cursor: null,
      hasMore: false,
    });
    let answers = 0;
    const requests = installTail(() => {
      answers += 1;
      return answers < 3
        ? { state: "catching_up", entries: [] }
        : {
            state: "live",
            taskId: "task-tail",
            attempt: 1,
            persistedThrough: 0,
            fetchedThrough: 1,
            entries: [entry(1, true)],
          };
    });
    const { result } = renderHook(
      () => useLiveTail(SESSION_ID, true, 1, { retryDelaysMs: [20, 40, 80] }),
      { wrapper },
    );
    await waitFor(() => expect(result.current).toBe("catching_up"));
    await waitFor(() => expect(result.current).toBe("live"), {
      timeout: 3000,
    });
    expect(requests).toHaveLength(3);
    // Backoff grew between the retries (20 ms, then 40 ms).
    expect(requests[2] - requests[1]).toBeGreaterThanOrEqual(
      requests[1] - requests[0] - 5,
    );
  });

  it("stops retrying when the turn is no longer active and reports idle", async () => {
    const { wrapper, queryClient } = createWrapper();
    queryClient.setQueryData<TimelineWindow>(["session-timeline", SESSION_ID], {
      items: [],
      cursor: null,
      hasMore: false,
    });
    const requests = installTail(() => ({
      state: "catching_up",
      entries: [],
    }));
    const { result, rerender } = renderHook(
      ({ active }: { active: boolean }) =>
        useLiveTail(SESSION_ID, active, 1, { retryDelaysMs: [20] }),
      { wrapper, initialProps: { active: true } },
    );
    await waitFor(() => expect(result.current).toBe("catching_up"));
    rerender({ active: false });
    expect(result.current).toBe("idle");
    const seen = requests.length;
    await new Promise((resolve) => setTimeout(resolve, 80));
    expect(requests.length).toBe(seen);
  });

  it("a gap signal causes one debounced tail read that fills the hole without duplicates", async () => {
    const { wrapper, queryClient } = createWrapper();
    queryClient.setQueryData<TimelineWindow>(["session-timeline", SESSION_ID], {
      items: [entry(1, false), entry(2, false)],
      cursor: null,
      hasMore: false,
    });
    const requests = installTail(() => ({
      state: "live",
      taskId: "task-tail",
      attempt: 1,
      persistedThrough: 2,
      fetchedThrough: 6,
      entries: [entry(3, true), entry(4, true), entry(5, true), entry(6, true)],
    }));
    const { result } = renderHook(() => useLiveTail(SESSION_ID, true, 1), {
      wrapper,
    });
    await waitFor(() => expect(result.current).toBe("live"));
    expect(requests).toHaveLength(1);
    // Rows 3 and 6 arrived live, 4 and 5 were dropped: the placement path
    // signals the gap (twice, for a burst); one read follows.
    queryClient.setQueryData<TimelineWindow>(
      ["session-timeline", SESSION_ID],
      (old) =>
        old && {
          ...old,
          items: [
            entry(1, false),
            entry(2, false),
            entry(3, true),
            entry(6, true),
          ],
        },
    );
    requestLiveTail(SESSION_ID);
    requestLiveTail(SESSION_ID);
    await waitFor(() => expect(requests).toHaveLength(2));
    await waitFor(() => {
      const held = queryClient.getQueryData<TimelineWindow>([
        "session-timeline",
        SESSION_ID,
      ]);
      expect(held?.items.map((item) => item.id)).toEqual([
        "task-tail:1:1",
        "task-tail:1:2",
        "task-tail:1:3",
        "task-tail:1:4",
        "task-tail:1:5",
        "task-tail:1:6",
      ]);
    });
    // No further read without a new signal or a saved-history read.
    await new Promise((resolve) => setTimeout(resolve, 350));
    expect(requests).toHaveLength(2);
  });

  it("reports none for a session without a running turn", async () => {
    const { wrapper, queryClient } = createWrapper();
    queryClient.setQueryData<TimelineWindow>(["session-timeline", SESSION_ID], {
      items: [],
      cursor: null,
      hasMore: false,
    });
    installTail(() => ({ state: "none", entries: [] }));
    const { result } = renderHook(() => useLiveTail(SESSION_ID, true, 1), {
      wrapper,
    });
    await waitFor(() => expect(result.current).toBe("none"));
  });

  it("an unreachable tail is retried and does not touch the held window", async () => {
    const { wrapper, queryClient } = createWrapper();
    const held: TimelineWindow = {
      items: [entry(1, false)],
      cursor: null,
      hasMore: false,
    };
    queryClient.setQueryData(["session-timeline", SESSION_ID], held);
    const requests = installTail(() => 503);
    const { result } = renderHook(
      () => useLiveTail(SESSION_ID, true, 1, { retryDelaysMs: [20] }),
      { wrapper },
    );
    await waitFor(() => expect(result.current).toBe("error"));
    await waitFor(() => expect(requests.length).toBeGreaterThanOrEqual(2));
    expect(
      queryClient.getQueryData<TimelineWindow>([
        "session-timeline",
        SESSION_ID,
      ]),
    ).toBe(held);
  });
});

/**
 * The tail reader composed with the saved-history reader under one
 * QueryClient, as the chat surfaces mount them: one tail read per read of
 * saved history, none for cache writes (the tail's own placements, WebSocket
 * rows), one per gap burst.
 */
describe("useLiveTail with useSessionTimeline", () => {
  function installSavedHistory(entries: () => TimelineEntry[]) {
    let reads = 0;
    server.use(
      http.get("/api/v1/sessions/:id/timeline", () => {
        reads += 1;
        return HttpResponse.json({
          data: { entries: entries(), cursor: null, hasMore: false },
        });
      }),
    );
    return { savedReads: () => reads };
  }

  function useComposed(active: boolean) {
    const timeline = useSessionTimeline(SESSION_ID);
    const tail = useLiveTail(SESSION_ID, active, timeline.historyReads);
    return { timeline, tail };
  }

  const settle = () => new Promise((resolve) => setTimeout(resolve, 300));

  it("does not repeatedly read the same tail after a single saved timeline fetch", async () => {
    const { wrapper } = createWrapper();
    const history = installSavedHistory(() => [entry(1, false)]);
    const requests = installTail(() => ({
      state: "live",
      taskId: "task-tail",
      attempt: 1,
      persistedThrough: 1,
      fetchedThrough: 1,
      entries: [],
    }));
    const { result } = renderHook(() => useComposed(true), { wrapper });
    await waitFor(() => expect(result.current.tail).toBe("live"));
    await settle();
    expect(history.savedReads()).toBe(1);
    expect(result.current.timeline.historyReads).toBe(1);
    expect(requests).toHaveLength(1);
  });

  it("a tail that repeats held rows and WebSocket rows do not trigger reads; a gap does once", async () => {
    const { wrapper, queryClient } = createWrapper();
    const history = installSavedHistory(() => [entry(1, false)]);
    const requests = installTail(() => ({
      state: "live",
      taskId: "task-tail",
      attempt: 1,
      persistedThrough: 1,
      fetchedThrough: 2,
      entries: [entry(2, true)],
    }));
    const { result } = renderHook(() => useComposed(true), { wrapper });
    await waitFor(() => expect(result.current.tail).toBe("live"));
    await waitFor(() =>
      expect(
        result.current.timeline.status === "ok" &&
          result.current.timeline.data.items.map((item) => item.id),
      ).toEqual(["task-tail:1:1", "task-tail:1:2"]),
    );
    // The tail placed row 2: a cache write, not a read.
    await settle();
    expect(requests).toHaveLength(1);
    expect(history.savedReads()).toBe(1);

    // Ordinary live rows arriving over the WebSocket are placed, nothing more.
    act(() => {
      applyLiveTimelineRow(queryClient, SESSION_ID, entry(3, true));
      applyLiveTimelineRow(queryClient, SESSION_ID, entry(3, true));
      applyLiveTimelineRow(queryClient, SESSION_ID, entry(4, true));
    });
    await settle();
    expect(requests).toHaveLength(1);
    expect(
      result.current.timeline.status === "ok" &&
        result.current.timeline.data.items.map((item) => item.id),
    ).toEqual([
      "task-tail:1:1",
      "task-tail:1:2",
      "task-tail:1:3",
      "task-tail:1:4",
    ]);

    // A row that skips past the held sequence asks for exactly one read.
    act(() => {
      applyLiveTimelineRow(queryClient, SESSION_ID, entry(7, true));
    });
    await waitFor(() => expect(requests).toHaveLength(2));
    await settle();
    expect(requests).toHaveLength(2);
    expect(history.savedReads()).toBe(1);
  });

  /** Saved rows 1..`persisted`, paged backward by cursor like the API. */
  function installPagedHistory(persisted: number, pageSize: number) {
    const reads: string[] = [];
    const saved = Array.from({ length: persisted }, (_, index) =>
      entry(index + 1, false),
    );
    server.use(
      http.get("/api/v1/sessions/:id/timeline", ({ request }) => {
        const cursor = new URL(request.url).searchParams.get("cursor");
        reads.push(cursor ?? "");
        const end = cursor
          ? saved.findIndex((row) => row.id === cursor.split("|")[1])
          : saved.length;
        const stop = end === -1 ? saved.length : end;
        const start = Math.max(0, stop - pageSize);
        const page = saved.slice(start, stop);
        return HttpResponse.json({
          data: {
            entries: page,
            cursor:
              start > 0 && page.length
                ? `${page[0].createdAt}|${page[0].id}`
                : null,
            hasMore: start > 0,
          },
        });
      }),
    );
    return reads;
  }

  it("a stored row above a hole persistence covers has the hole read by sequence (no tail signal involved)", async () => {
    const { wrapper, queryClient } = createWrapper();
    queryClient.setQueryData<TimelineWindow>(["session-timeline", SESSION_ID], {
      items: [entry(1, false), entry(2, false), entry(3, false)],
      cursor: null,
      hasMore: false,
    });
    const reads = installPagedHistory(8, 4);
    const requests = installTail(() => ({
      state: "live",
      taskId: "task-tail",
      attempt: 1,
      persistedThrough: 3,
      fetchedThrough: 3,
      entries: [],
    }));
    const { result } = renderHook(() => useComposed(true), { wrapper });
    await waitFor(() => expect(result.current.tail).toBe("live"));
    await settle();
    expect(reads).toEqual([]);
    // Rows 4..7 were dropped for this connection and persisted; the stored
    // row of a boundary at 8 arrives after its batch committed.
    act(() => {
      applyLiveTimelineRow(queryClient, SESSION_ID, entry(8, false));
      flushLiveTimelineRows(SESSION_ID);
    });
    await waitFor(() =>
      expect(
        result.current.timeline.status === "ok" &&
          result.current.timeline.data.items.map((item) => item.metadata.seq),
      ).toEqual([1, 2, 3, 4, 5, 6, 7, 8]),
    );
    await settle();
    // One page, read from the row after the hole; no tail read, no refresh.
    expect(reads).toEqual([`${entry(8, false).createdAt}|task-tail:1:8`]);
    expect(requests).toHaveLength(1);
    expect(result.current.timeline.historyReads).toBe(0);
  });

  it("a hole below the tail's persisted watermark is closed from saved history, within a bounded number of reads", async () => {
    const { wrapper, queryClient } = createWrapper();
    queryClient.setQueryData<TimelineWindow>(["session-timeline", SESSION_ID], {
      items: [entry(1, false), entry(2, false), entry(3, false)],
      cursor: null,
      hasMore: false,
    });
    const reads = installPagedHistory(10, 3);
    let tailReads = 0;
    const requests = installTail(() => {
      tailReads += 1;
      return {
        state: "live",
        taskId: "task-tail",
        attempt: 1,
        persistedThrough: 8,
        fetchedThrough: 10,
        entries: [entry(9, true), entry(10, true)],
      };
    });
    const { result } = renderHook(() => useComposed(true), { wrapper });
    await waitFor(() => expect(result.current.tail).toBe("live"));
    // Saved history and the tail together hold every row exactly once.
    await waitFor(() =>
      expect(
        result.current.timeline.status === "ok" &&
          result.current.timeline.data.items.map((item) => item.metadata.seq),
      ).toEqual([1, 2, 3, 4, 5, 6, 7, 8, 9, 10]),
    );
    await settle();
    // Rows 4..8 came from saved history, read by sequence from the row after
    // the hole, while the refresh the moved watermark asked for read back
    // until it met a held saved row. Reads stay bounded either way.
    const held =
      result.current.timeline.status === "ok"
        ? result.current.timeline.data.items
        : [];
    expect(held.filter((item) => item.metadata.live === true)).toEqual([]);
    expect(reads.length).toBeLessThanOrEqual(5);
    expect(requests.length).toBeLessThanOrEqual(3);
    expect(tailReads).toBe(requests.length);
  });

  /** A row of the attempt with the journal kind `kind`, live or saved. */
  function journalRow(seq: number, kind: string, live: boolean): TimelineEntry {
    const payload =
      kind === "thought.final"
        ? { content: `thought ${seq}` }
        : kind === "tool.started"
          ? { call_id: `call-${seq}`, name: "Read", input: { path: "x" } }
          : kind === "tool.completed"
            ? { call_id: `call-${seq - 1}`, name: "Read", output: "ok" }
            : kind === "invocation.outcome"
              ? { status: "succeeded" }
              : { block_id: "b", text_delta: `delta ${seq} ` };
    return {
      id: `task-tail:1:${seq}`,
      entryType: kind === "invocation.outcome" ? "status" : "output",
      content: `${kind} ${seq}`,
      createdAt: `2026-11-01T02:${String(Math.floor(seq / 60)).padStart(2, "0")}:${String(seq % 60).padStart(2, "0")}.000Z`,
      metadata: {
        schema_version: "g5.task-event.v1",
        kind,
        payload,
        terminal: kind === "invocation.outcome",
        taskId: "task-tail",
        attempt: 1,
        seq,
        sessionId: SESSION_ID,
        messageId: "asst-1",
        ...(live ? { live: true } : {}),
      },
    };
  }

  /** Saved rows that grow while the test runs, paged backward by cursor. */
  function installGrowingHistory(saved: TimelineEntry[], pageSize: number) {
    const reads: string[] = [];
    server.use(
      http.get("/api/v1/sessions/:id/timeline", ({ request }) => {
        const cursor = new URL(request.url).searchParams.get("cursor");
        reads.push(cursor ?? "");
        const end = cursor
          ? saved.findIndex((row) => row.id === cursor.split("|")[1])
          : saved.length;
        const stop = end === -1 ? saved.length : end;
        const start = Math.max(0, stop - pageSize);
        const page = saved.slice(start, stop);
        return HttpResponse.json({
          data: {
            entries: page,
            cursor:
              start > 0 && page.length
                ? `${page[0].createdAt}|${page[0].id}`
                : null,
            hasMore: start > 0,
          },
        });
      }),
    );
    return reads;
  }

  function message(
    id: string,
    role: string,
    status: string,
    index: number,
  ): ChatMessageDTO {
    return {
      id,
      sessionId: SESSION_ID,
      role,
      content: role === "user" ? "Write the report" : "",
      artifacts: null,
      citations: null,
      inputTokens: null,
      outputTokens: null,
      status,
      messageIndex: index,
      createdAt: `2026-11-01T01:00:0${index}.000Z`,
    };
  }

  /**
   * The production composition: the tail is active while the transcript's
   * last message is a pending or streaming assistant (the workspace chat
   * controller's rule), which a steer's optimistic user row flips off.
   */
  function useSteeredTurn() {
    const messages = useChatMessages(SESSION_ID);
    const list = messages.status === "ok" ? messages.data : [];
    const last = list[list.length - 1];
    const active =
      !!last &&
      last.role === "assistant" &&
      (last.status === "pending" || last.status === "streaming");
    const timeline = useSessionTimeline(SESSION_ID);
    const tail = useLiveTail(SESSION_ID, active, timeline.historyReads);
    const send = useSendChatMessage();
    return { active, timeline, tail, send };
  }

  it("a refresh after a republished boundary row keeps the turn's earlier live rows (steer mid-turn, chat.done)", async () => {
    const { wrapper, queryClient } = createWrapper();
    const PAGE = 20;
    const kinds = (seq: number) =>
      seq === 41
        ? "invocation.outcome"
        : seq % 20 === 1
          ? "thought.final"
          : seq % 20 === 2
            ? "tool.started"
            : seq % 20 === 3
              ? "tool.completed"
              : "message.delta";
    const saved: TimelineEntry[] = [];
    const reads = installGrowingHistory(saved, PAGE);
    let turnEnded = false;
    const tailReads = installTail(() =>
      turnEnded
        ? { state: "none", entries: [] }
        : {
            state: "live",
            taskId: "task-tail",
            attempt: 1,
            persistedThrough: 0,
            fetchedThrough: 0,
            entries: [],
          },
    );
    let transcript = [
      message("user-1", "user", "complete", 0),
      message("asst-1", "assistant", "streaming", 1),
    ];
    server.use(
      http.get("/api/v1/sessions/:id/messages", () =>
        HttpResponse.json({ data: transcript }),
      ),
    );
    // The steer's POST is still in flight when the turn ends. Its queue
    // representation must leave the running transcript and live tail alone.
    let releaseSend: () => void = () => {};
    const sendGate = new Promise<void>((resolve) => {
      releaseSend = resolve;
    });
    server.use(
      http.post("/api/v1/sessions/:id/messages", async () => {
        await sendGate;
        return HttpResponse.json(
          {
            data: {
              messageId: "asst-1",
              status: "streaming",
              inputId: "input-steer",
              taskId: "task-tail",
            },
          },
          { status: 201 },
        );
      }),
    );

    const { result } = renderHook(() => useSteeredTurn(), { wrapper });
    await waitFor(() => expect(result.current.timeline.status).toBe("ok"));
    await waitFor(() => expect(result.current.active).toBe(true));
    await waitFor(() => expect(result.current.tail).toBe("live"));

    // A long turn streams well over one history page of live rows,
    // thoughts and tool calls among them.
    act(() => {
      for (let seq = 1; seq <= 40; seq += 1)
        applyLiveTimelineRow(
          queryClient,
          SESSION_ID,
          journalRow(seq, kinds(seq), true),
        );
      flushLiveTimelineRows(SESSION_ID);
    });
    const heldSeqs = () =>
      result.current.timeline.status === "ok"
        ? result.current.timeline.data.items.map((item) => item.metadata.seq)
        : [];
    await waitFor(() =>
      expect(heldSeqs()).toEqual(
        Array.from({ length: 40 }, (_, index) => index + 1),
      ),
    );

    // A steer mid-turn stays in the queue and keeps live activity running.
    act(() => {
      result.current.send.mutate({
        sessionId: SESSION_ID,
        content: "Also cover the tests",
      });
    });
    await waitFor(() => expect(result.current.send.isLoading).toBe(true));
    expect(result.current.active).toBe(true);
    expect(result.current.tail).toBe("live");

    // The batch holding the terminal outcome commits rows 1..41; the
    // outcome, a boundary, is republished as its stored row and sits in the
    // newest saved page, whose older pages the browser has not read.
    for (let seq = 1; seq <= 41; seq += 1)
      saved.push(journalRow(seq, kinds(seq), false));
    turnEnded = true;
    act(() => {
      applyLiveTimelineRow(
        queryClient,
        SESSION_ID,
        journalRow(41, "invocation.outcome", false),
      );
      flushLiveTimelineRows(SESSION_ID);
    });
    await waitFor(() =>
      expect(heldSeqs()).toEqual(
        Array.from({ length: 41 }, (_, index) => index + 1),
      ),
    );

    // chat.done: the transcript and the timeline are read again.
    transcript = [
      message("user-1", "user", "complete", 0),
      message("asst-1", "assistant", "complete", 1),
    ];
    const manager = new WebSocketManager({
      getBaseUrl: () => "http://localhost",
      getWsTicket: async () => "test-ticket",
      queryClient,
    });
    await act(async () => {
      (manager as unknown as { handleEvent(e: unknown): void }).handleEvent({
        type: "chat.done",
        sessionId: SESSION_ID,
        messageId: "asst-1",
      });
    });
    await waitFor(() =>
      expect(result.current.timeline.historyReads).toBeGreaterThanOrEqual(2),
    );
    await settle();

    // Every row of the turn is still held exactly once: the rows the newest
    // page returned as saved, the earlier ones as the live rows whose saved
    // copies no read has brought yet.
    const held =
      result.current.timeline.status === "ok"
        ? result.current.timeline.data.items
        : [];
    expect(new Set(held.map((item) => item.id)).size).toBe(held.length);
    expect(held.map((item) => item.metadata.seq)).toEqual(
      Array.from({ length: 41 }, (_, index) => index + 1),
    );
    expect(
      held
        .filter((item) =>
          ["thought.final", "tool.started", "tool.completed"].includes(
            String(item.metadata.kind),
          ),
        )
        .map((item) => item.metadata.seq),
    ).toEqual([1, 2, 3, 21, 22, 23]);
    expect(
      held
        .filter((item) => item.metadata.live !== true)
        .map((item) => item.metadata.seq),
    ).toEqual(Array.from({ length: 20 }, (_, index) => index + 22));
    // The refresh was one newest-page read; no fill was needed.
    expect(reads.length).toBeLessThanOrEqual(3);
    expect(tailReads.length).toBeLessThanOrEqual(2);

    // The steer answers: it remains in the queue and the turn's transcript
    // and timeline rows are untouched.
    releaseSend();
    await settle();
    expect(heldSeqs()).toEqual(
      Array.from({ length: 41 }, (_, index) => index + 1),
    );
  });

  it("reads the tail again after saved history is read again", async () => {
    const { wrapper, queryClient } = createWrapper();
    const history = installSavedHistory(() => [entry(1, false)]);
    const requests = installTail(() => ({
      state: "live",
      taskId: "task-tail",
      attempt: 1,
      persistedThrough: 1,
      fetchedThrough: 1,
      entries: [],
    }));
    const { result } = renderHook(() => useComposed(true), { wrapper });
    await waitFor(() => expect(result.current.tail).toBe("live"));
    await act(async () => {
      await queryClient.invalidateQueries({
        queryKey: ["session-timeline", SESSION_ID],
      });
    });
    await waitFor(() => expect(result.current.timeline.historyReads).toBe(2));
    await waitFor(() => expect(requests).toHaveLength(2));
    await settle();
    expect(history.savedReads()).toBe(2);
    expect(requests).toHaveLength(2);
  });

  it("reads the tail when the turn becomes active over a held window", async () => {
    const { wrapper } = createWrapper();
    installSavedHistory(() => [entry(1, false)]);
    const requests = installTail(() => ({
      state: "live",
      taskId: "task-tail",
      attempt: 1,
      persistedThrough: 1,
      fetchedThrough: 1,
      entries: [],
    }));
    const { result, rerender } = renderHook(
      ({ active }: { active: boolean }) => useComposed(active),
      { wrapper, initialProps: { active: false } },
    );
    await waitFor(() => expect(result.current.timeline.status).toBe("ok"));
    await settle();
    expect(requests).toHaveLength(0);
    rerender({ active: true });
    await waitFor(() => expect(result.current.tail).toBe("live"));
    await settle();
    expect(requests).toHaveLength(1);
  });
});
