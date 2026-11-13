// =============================================================================
// useChat.test.ts — Chat Hook Integration Tests with MSW
// =============================================================================

import { describe, it, expect, test, vi } from "vitest";
import React from "react";
import { renderHook, waitFor, act } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { http, HttpResponse } from "msw";
import { server } from "../test/msw/server";
import { createMockChatMessage } from "../test/fixtures";
import { ApiError } from "../lib/api";
import { WebSocketManager, type WSEvent } from "../lib/ws";
import {
  refreshChatMessages,
  useChatSessions,
  useChatSession,
  useChatMessages,
  useCreateChatSession,
  useStartChatMessage,
  useSendChatMessage,
  useReplyToProviderRequest,
  useSessionTimeline,
  useStopStreaming,
  useForkSession,
  useRecordChatInspection,
  useRenameChatSession,
  useChatResultInspection,
  useCancelUnknownSessionInput,
  useSessionInputs,
} from "./useChat";
import type {
  ChatSessionDTO,
  ChatMessageDTO,
  SessionInputDTO,
  TimelineEntry,
  Page,
} from "../lib/types";
import { projectTaskTimelineEntries } from "../lib/taskTimeline";

// ---------------------------------------------------------------------------
// Test Wrapper
// ---------------------------------------------------------------------------

function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false, staleTime: Infinity, gcTime: Infinity },
      mutations: { retry: false },
    },
  });

  const wrapper = ({ children }: { children: React.ReactNode }) =>
    React.createElement(QueryClientProvider, { client: queryClient }, children);

  return { wrapper, queryClient };
}

// ---------------------------------------------------------------------------
// Mock Data
// ---------------------------------------------------------------------------

const mockSessions: ChatSessionDTO[] = [
  {
    id: "session-1",
    workspaceId: "ws-123",
    scopeType: "workspace",
    scopeRef: "ws-123",
    model: "claude-sonnet-4-20250514",
    agentProvider: "claude_code",
    title: "Architecture Discussion",
    summary: "Discussing the auth module architecture",
    tags: [],
    messageCount: 10,
    totalInputTokens: 5000,
    totalOutputTokens: 3000,
    totalCostUsd: 0.25,
    forkedFromId: null,
    linkedCommits: [],
    createdAt: "2024-11-15T10:00:00Z",
    updatedAt: "2024-11-15T10:30:00Z",
    canSend: true,
  },
];

const mockSession: ChatSessionDTO = mockSessions[0];

const mockMessages: ChatMessageDTO[] = [
  {
    id: "msg-1",
    sessionId: "session-1",
    role: "user",
    content: "How should I structure the auth module?",
    artifacts: [],
    citations: [],
    inputTokens: 50,
    outputTokens: null,
    status: "complete",
    messageIndex: 0,
    createdAt: "2024-11-15T10:00:00Z",
  },
  {
    id: "msg-2",
    sessionId: "session-1",
    role: "assistant",
    content: "I recommend a layered approach with JWT validation...",
    artifacts: [
      {
        type: "code",
        title: "Auth Module",
        content: "export function validate() {}",
        language: "typescript",
      },
    ],
    citations: [
      {
        label: "auth/validator.ts",
      },
    ],
    inputTokens: null,
    outputTokens: 200,
    status: "complete",
    messageIndex: 1,
    createdAt: "2024-11-15T10:00:30Z",
  },
];

const mockNewMessage: ChatMessageDTO = {
  id: "msg-3",
  sessionId: "session-1",
  role: "user",
  content: "Hello",
  artifacts: [],
  citations: [],
  inputTokens: null,
  outputTokens: null,
  status: "complete",
  messageIndex: 2,
  createdAt: "2024-11-15T10:01:00Z",
};

const mockForkedSession: ChatSessionDTO = {
  id: "session-forked",
  workspaceId: "ws-123",
  scopeType: "workspace",
  scopeRef: "ws-123",
  model: "claude-sonnet-4-20250514",
  agentProvider: "claude_code",
  title: "Architecture Discussion (fork)",
  summary: "Forked from Architecture Discussion",
  tags: [],
  messageCount: 3,
  totalInputTokens: 2000,
  totalOutputTokens: 1500,
  totalCostUsd: 0.1,
  forkedFromId: "session-1",
  linkedCommits: [],
  createdAt: "2024-11-15T10:02:00Z",
  updatedAt: "2024-11-15T10:02:00Z",
  canSend: true,
};

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("useChatSessions", () => {
  it("fetches chat sessions for workspace", async () => {
    server.use(
      http.get("/api/v1/workspaces/:id/sessions", () =>
        HttpResponse.json({ data: mockSessions }),
      ),
    );

    const { wrapper } = createWrapper();
    const { result } = renderHook(() => useChatSessions("ws-123"), {
      wrapper,
    });

    await waitFor(() => expect(result.current.status).toBe("ok"));

    if (result.current.status === "ok") {
      expect(result.current.data).toHaveLength(1);
      expect(result.current.data[0].id).toBe("session-1");
    }
  });

  it("fetches chat sessions with scope filter", async () => {
    let capturedUrl: URL | null = null;
    server.use(
      http.get("/api/v1/workspaces/:id/sessions", ({ request }) => {
        capturedUrl = new URL(request.url);
        return HttpResponse.json({ data: mockSessions });
      }),
    );

    const { wrapper } = createWrapper();
    renderHook(() => useChatSessions("ws-123", "task"), { wrapper });

    await waitFor(() => expect(capturedUrl).not.toBeNull());
    expect(capturedUrl!.searchParams.get("scopeType")).toBe("task");
  });
});

describe("useSessionInputs", () => {
  it("validates the camelCase wire contract and keeps the server permissions", async () => {
    server.use(
      http.get("/api/v1/sessions/:sessionId/inputs", () =>
        HttpResponse.json({
          data: [
            {
              id: "input-1",
              sessionId: "session-1",
              inputIndex: 0,
              content: "Queued text",
              artifacts: [],
              revision: 1,
              inputOrigin: "human",
              delivery: "queued",
              deliveryGeneration: 0,
              taskId: "task-1",
              attempt: null,
              createdAt: "2026-09-21T00:00:00Z",
              deliveredAt: null,
              withdrawnAt: null,
              editable: false,
              withdrawable: true,
            },
          ],
        }),
      ),
    );

    const { wrapper } = createWrapper();
    const { result } = renderHook(() => useSessionInputs("session-1"), {
      wrapper,
    });
    await waitFor(() => expect(result.current.status).toBe("ok"));
    if (result.current.status === "ok") {
      expect(result.current.data[0]).toMatchObject({
        id: "input-1",
        inputOrigin: "human",
        editable: false,
        withdrawable: true,
      });
    }
  });

  it("normalizes a draining pre-#629 pod's snake_case response", async () => {
    server.use(
      http.get("/api/v1/sessions/:sessionId/inputs", () =>
        HttpResponse.json({
          data: [
            {
              id: "input-legacy",
              session_id: "session-1",
              input_index: 0,
              content: "Queued on an older pod",
              artifacts: [],
              revision: 1,
              input_origin: "human",
              delivery: "queued",
              delivery_generation: 0,
              task_id: "task-1",
              attempt: null,
              created_at: "2026-09-21T00:00:00Z",
              delivered_at: null,
              withdrawn_at: null,
            },
          ],
        }),
      ),
    );

    const { wrapper } = createWrapper();
    const { result } = renderHook(() => useSessionInputs("session-1"), {
      wrapper,
    });
    await waitFor(() => expect(result.current.status).toBe("ok"));
    if (result.current.status === "ok") {
      expect(result.current.data[0]).toMatchObject({
        id: "input-legacy",
        sessionId: "session-1",
        inputOrigin: "human",
        deliveryGeneration: 0,
      });
      expect(result.current.data[0].editable).toBeUndefined();
    }
  });
});

describe("useCancelUnknownSessionInput", () => {
  it("posts to the explicit uncertain-delivery cancel route and updates the cached input", async () => {
    const original: SessionInputDTO = {
      id: "input-unknown",
      sessionId: "session-1",
      inputIndex: 4,
      content: "possibly delivered",
      artifacts: null,
      revision: 1,
      inputOrigin: "human",
      delivery: "delivery_unknown",
      deliveryGeneration: 1,
      taskId: "task-unknown",
      attempt: 1,
      createdAt: "2026-09-21T00:00:00Z",
      deliveredAt: null,
      withdrawnAt: null,
    };
    let requestBody: unknown;
    server.use(
      http.post(
        "/api/v1/sessions/:sessionId/inputs/:inputId/cancel",
        async ({ request, params }) => {
          expect(params).toMatchObject({
            sessionId: "session-1",
            inputId: "input-unknown",
          });
          requestBody = await request.json();
          return HttpResponse.json({
            data: {
              ...original,
              delivery: "withdrawn",
              revision: 2,
              withdrawnAt: "2026-09-21T00:01:00Z",
            },
          });
        },
      ),
    );

    const { wrapper, queryClient } = createWrapper();
    queryClient.setQueryData<SessionInputDTO[]>(
      ["session-inputs", "session-1"],
      [original],
    );
    queryClient.setQueryData(["session-task", "session-1"], {
      id: "task-unknown",
    });
    const { result } = renderHook(() => useCancelUnknownSessionInput(), {
      wrapper,
    });

    await act(async () => {
      await result.current.mutateAsync({
        sessionId: "session-1",
        inputId: "input-unknown",
      });
    });

    expect(requestBody).toEqual({});
    expect(
      queryClient.getQueryData<SessionInputDTO[]>([
        "session-inputs",
        "session-1",
      ])?.[0],
    ).toMatchObject({ delivery: "withdrawn", revision: 2 });
    expect(
      queryClient.getQueryState(["session-task", "session-1"])?.isInvalidated,
    ).toBe(true);
  });
});

describe("useChatSession", () => {
  it("receives live meters without fetching them, retains them through session reads, and isolates accounts", async () => {
    let account = "account-one";
    server.use(
      http.get("/api/v1/sessions/:sessionId", () =>
        HttpResponse.json({
          data: {
            ...mockSession,
            authMode: "chat_gpt_subscription",
            account: {
              id: account,
              kind: "chatgpt_subscription",
              scope: "user",
            },
            subscriptionUsage: {
              sevenDayUtilization: 0.99,
              capturedAt: "2020-01-01T00:00:00Z",
            },
          },
        }),
      ),
    );
    const { wrapper, queryClient } = createWrapper();
    const { result, unmount } = renderHook(() => useChatSession("session-1"), {
      wrapper,
    });
    await waitFor(() => expect(result.current.status).toBe("ok"));
    const read = () =>
      result.current.status === "ok"
        ? result.current.data.subscriptionUsage
        : undefined;
    expect(read()).toBeNull();
    const manager = new WebSocketManager({
      getBaseUrl: () => "ws://test",
      getWsTicket: async () => "ticket",
      queryClient,
    });
    const dispatch = (event: WSEvent) =>
      (manager as unknown as { handleEvent(e: WSEvent): void }).handleEvent(
        event,
      );
    const event: WSEvent = {
      type: "chat.subscription_usage",
      sessionId: "session-1",
      authMode: "chat_gpt_subscription",
      accountId: "account-one",
      usage: {
        fiveHourUtilization: 0.23,
        sevenDayUtilization: 0.45,
        capturedAt: "2026-10-30T12:00:00Z",
      },
    };
    act(() => dispatch({ ...event, sessionId: "another-session" }));
    expect(read()).toBeNull();
    act(() => dispatch(event));
    await waitFor(() => expect(read()?.sevenDayUtilization).toBe(0.45));
    await act(async () => {
      await queryClient.invalidateQueries({
        queryKey: ["chat-session", "session-1"],
      });
    });
    expect(read()?.sevenDayUtilization).toBe(0.45);
    account = "account-two";
    await act(async () => {
      await queryClient.invalidateQueries({
        queryKey: ["chat-session", "session-1"],
      });
    });
    await waitFor(() => expect(read()).toBeNull());
    act(() => dispatch({ ...event, accountId: account }));
    await waitFor(() => expect(read()?.fiveHourUtilization).toBe(0.23));
    unmount();
    await waitFor(() =>
      expect(
        queryClient.getQueryData(["subscription-usage", "session-1"]),
      ).toBeUndefined(),
    );
  });
  it("fetches single chat session", async () => {
    server.use(
      http.get("/api/v1/sessions/:sessionId", () =>
        HttpResponse.json({ data: mockSession }),
      ),
    );

    const { wrapper } = createWrapper();
    const { result } = renderHook(() => useChatSession("session-1"), {
      wrapper,
    });

    await waitFor(() => expect(result.current.status).toBe("ok"));

    if (result.current.status === "ok") {
      expect(result.current.data.id).toBe("session-1");
      expect(result.current.data.title).toBe("Architecture Discussion");
    }
  });
});

describe("useChatMessages", () => {
  it("fetches chat messages", async () => {
    server.use(
      http.get("/api/v1/sessions/:sessionId/messages", () =>
        HttpResponse.json({ data: mockMessages }),
      ),
    );

    const { wrapper } = createWrapper();
    const { result } = renderHook(() => useChatMessages("session-1"), {
      wrapper,
    });

    await waitFor(() => expect(result.current.status).toBe("ok"));

    if (result.current.status === "ok") {
      expect(result.current.data).toHaveLength(2);
      expect(result.current.data[1].artifacts).toHaveLength(1);
      expect(result.current.data[1].citations).toHaveLength(1);
    }
  });

  it("handles empty message list", async () => {
    server.use(
      http.get("/api/v1/sessions/:sessionId/messages", () =>
        HttpResponse.json({ data: [] }),
      ),
    );

    const { wrapper } = createWrapper();
    const { result } = renderHook(() => useChatMessages("empty-session"), {
      wrapper,
    });

    await waitFor(() => expect(result.current.status).toBe("ok"));

    if (result.current.status === "ok") {
      expect(result.current.data).toEqual([]);
    }
  });

  it("loads the page strictly older than the cached minimum and preserves it across a tail refresh", async () => {
    // Messages scrollback (job_chat_draft_hooks D39, chat d45027de): the tail
    // window never slides backward, so rows older than the newest 500 were
    // unreachable. `loadOlder` must read the page strictly older than the
    // cached minimum through the forward cursor (`cursor = min − limit − 1`,
    // routes/chat I6), prepend it, and the D39 window merge must then keep the
    // prepended history across the next tail refresh.
    const row = (index: number): ChatMessageDTO => ({
      id: `m-${index}`,
      sessionId: "session-scrollback",
      role: index % 2 === 0 ? "user" : "assistant",
      content: `message ${index}`,
      artifacts: null,
      citations: null,
      inputTokens: null,
      outputTokens: null,
      status: "complete",
      messageIndex: index,
      createdAt: `2026-01-01T00:00:${String(index % 60).padStart(2, "0")}Z`,
    });
    const olderRequests: Array<{
      cursor: string | null;
      limit: string | null;
    }> = [];
    let tailCalls = 0;
    server.use(
      http.get("/api/v1/sessions/:sessionId/messages", ({ request }) => {
        const url = new URL(request.url);
        if (url.searchParams.get("tail") === "true") {
          tailCalls += 1;
          return HttpResponse.json({
            data: {
              // Second tail read slides forward by one appended row.
              messages:
                tailCalls === 1
                  ? [row(600), row(601), row(602)]
                  : [row(600), row(601), row(602), row(603)],
              cursor: tailCalls === 1 ? 602 : 603,
              hasMore: true,
            },
          });
        }
        olderRequests.push({
          cursor: url.searchParams.get("cursor"),
          limit: url.searchParams.get("limit"),
        });
        return HttpResponse.json({
          data: { messages: [row(598), row(599)], cursor: 599, hasMore: true },
        });
      }),
    );

    const { wrapper, queryClient } = createWrapper();
    const { result } = renderHook(() => useChatMessages("session-scrollback"), {
      wrapper,
    });

    await waitFor(() => expect(result.current.status).toBe("ok"));
    expect(result.current.hasOlder).toBe(true);

    await act(async () => {
      // Concurrent second call must no-op (single-flight).
      const first = result.current.loadOlder();
      const second = result.current.loadOlder();
      await Promise.all([first, second]);
    });

    // Exactly one older-page request, addressed strictly below the cached
    // minimum (600): cursor = 600 − 500 − 1 = 99, forward mode (no tail).
    expect(olderRequests).toEqual([{ cursor: "99", limit: "500" }]);
    await waitFor(() => {
      if (result.current.status !== "ok") throw new Error("not ok");
      expect(result.current.data.map((m) => m.messageIndex)).toEqual([
        598, 599, 600, 601, 602,
      ]);
    });
    expect(result.current.hasOlder).toBe(true);

    // The D21-style tail refresh must keep the prepended older history.
    await act(async () => {
      await queryClient.invalidateQueries({
        queryKey: ["chat-messages", "session-scrollback"],
      });
    });
    await waitFor(() => {
      if (result.current.status !== "ok") throw new Error("not ok");
      expect(result.current.data.map((m) => m.messageIndex)).toEqual([
        598, 599, 600, 601, 602, 603,
      ]);
    });
  });

  it("a concurrent tail refresh cannot overwrite a loadOlder prepend", async () => {
    // PR #2435 review (kaihuiz blocking #2, chat 8029eea1): a tail read
    // snapshots the cache; if loadOlder's prepend lands after that snapshot
    // but before TanStack commits the tail result, the commit overwrites the
    // prepend (reproduced as [598..602] collapsing to [600..603]). The
    // prepend must CANCEL AND AWAIT the in-flight tail read on the exact
    // session key before applying the atomic updater (job_chat_draft_hooks
    // D39), making the outcome interleave-independent.
    const row = (index: number): ChatMessageDTO => ({
      id: `race-${index}`,
      sessionId: "session-race",
      role: index % 2 === 0 ? "user" : "assistant",
      content: `message ${index}`,
      artifacts: null,
      citations: null,
      inputTokens: null,
      outputTokens: null,
      status: "complete",
      messageIndex: index,
      createdAt: `2026-01-02T00:00:${String(index % 60).padStart(2, "0")}Z`,
    });
    let tailCalls = 0;
    // No-op-initialized callable (NOT a `| null` union): assignments made
    // inside the msw handler closure are invisible to TS control-flow
    // narrowing, which pins a null-initialized union to `null` at the outer
    // call site — `?.()` on the remaining `never` is TS2349 under the CI
    // `tsc -b` (which, unlike tsconfig.build.json, includes test files).
    let releaseTail: () => void = () => undefined;
    server.use(
      http.get("/api/v1/sessions/:sessionId/messages", async ({ request }) => {
        const url = new URL(request.url);
        if (url.searchParams.get("tail") === "true") {
          tailCalls += 1;
          if (tailCalls === 2) {
            // Park the poll-style tail read mid-flight.
            await new Promise<void>((resolve) => {
              releaseTail = resolve;
            });
            return HttpResponse.json({
              data: {
                messages: [row(600), row(601), row(602), row(603)],
                cursor: 603,
                hasMore: true,
              },
            });
          }
          return HttpResponse.json({
            data: {
              messages: [row(600), row(601), row(602)],
              cursor: 602,
              hasMore: true,
            },
          });
        }
        return HttpResponse.json({
          data: { messages: [row(598), row(599)], cursor: 599, hasMore: true },
        });
      }),
    );
    const { wrapper, queryClient } = createWrapper();
    const { result } = renderHook(() => useChatMessages("session-race"), {
      wrapper,
    });
    await waitFor(() => expect(result.current.status).toBe("ok"));

    // Start the D21-style tail refresh; its response is parked server-side.
    const invalidation = queryClient.invalidateQueries(
      { queryKey: ["chat-messages", "session-race"] },
      { cancelRefetch: false },
    );
    await waitFor(() => expect(tailCalls).toBe(2));
    expect(
      queryClient.isFetching({ queryKey: ["chat-messages", "session-race"] }),
    ).toBe(1);

    // loadOlder completes while the tail read is parked: it must cancel the
    // parked read before prepending.
    await act(async () => {
      await result.current.loadOlder();
    });
    expect(
      queryClient.isFetching({ queryKey: ["chat-messages", "session-race"] }),
    ).toBe(0);
    await waitFor(() => {
      if (result.current.status !== "ok") throw new Error("not ok");
      expect(result.current.data.map((m) => m.messageIndex)).toEqual([
        598, 599, 600, 601, 602,
      ]);
    });

    // Releasing the parked response must not resurrect it over the prepend.
    releaseTail();
    await act(async () => {
      await invalidation.catch(() => {});
    });
    await waitFor(() => {
      if (result.current.status !== "ok") throw new Error("not ok");
      expect(result.current.data.map((m) => m.messageIndex)).toEqual([
        598, 599, 600, 601, 602,
      ]);
    });
  });

  it("a pending older-page read for a previous session does not leak into the next session", async () => {
    // PR #2435 review (kaihuiz advisory #5 / CodeRabbit Major, chat
    // 8029eea1): in-flight ownership is keyed by session + request token
    // (job_chat_draft_hooks D39). Session A's parked older-page read must not
    // block session B's first loadOlder, and A's late completion must not
    // clear B's loading state or mark B exhausted.
    const row = (sid: string, index: number): ChatMessageDTO => ({
      id: `${sid}-${index}`,
      sessionId: sid,
      role: "user",
      content: `message ${index}`,
      artifacts: null,
      citations: null,
      inputTokens: null,
      outputTokens: null,
      status: "complete",
      messageIndex: index,
      createdAt: `2026-01-03T00:00:${String(index % 60).padStart(2, "0")}Z`,
    });
    // No-op-initialized callables (see the TS2349 narrowing note in the
    // race test above).
    let releaseOlderA: () => void = () => undefined;
    let releaseOlderB: () => void = () => undefined;
    const olderRequests: string[] = [];
    server.use(
      http.get(
        "/api/v1/sessions/:sessionId/messages",
        async ({ request, params }) => {
          const sid = String(params.sessionId);
          const url = new URL(request.url);
          if (url.searchParams.get("tail") === "true") {
            return HttpResponse.json({
              data: {
                messages: [row(sid, 600), row(sid, 601)],
                cursor: 601,
                hasMore: true,
              },
            });
          }
          olderRequests.push(sid);
          if (sid === "sess-a") {
            await new Promise<void>((resolve) => {
              releaseOlderA = resolve;
            });
            // No strictly-older rows: on the OLD code this marked the
            // CURRENT (wrong) session exhausted after the switch.
            return HttpResponse.json({
              data: { messages: [], cursor: null, hasMore: false },
            });
          }
          await new Promise<void>((resolve) => {
            releaseOlderB = resolve;
          });
          return HttpResponse.json({
            data: {
              messages: [row(sid, 598), row(sid, 599)],
              cursor: 599,
              hasMore: true,
            },
          });
        },
      ),
    );
    const { wrapper } = createWrapper();
    const { result, rerender } = renderHook(
      ({ sid }: { sid: string }) => useChatMessages(sid),
      { wrapper, initialProps: { sid: "sess-a" } },
    );
    await waitFor(() => expect(result.current.status).toBe("ok"));

    let pendingA: Promise<void> | null = null;
    act(() => {
      pendingA = result.current.loadOlder();
    });
    await waitFor(() => expect(olderRequests).toEqual(["sess-a"]));

    // Switch to session B while A's older read is parked.
    rerender({ sid: "sess-b" });
    await waitFor(() => expect(result.current.status).toBe("ok"));
    expect(result.current.hasOlder).toBe(true);

    // B's first loadOlder must FIRE (not be swallowed by A's pending lock).
    let pendingB: Promise<void> | null = null;
    act(() => {
      pendingB = result.current.loadOlder();
    });
    await waitFor(() => expect(olderRequests).toEqual(["sess-a", "sess-b"]));
    expect(result.current.isLoadingOlder).toBe(true);

    // A settles late (with zero older rows): B's loading state must stay
    // true and B must NOT be marked exhausted.
    releaseOlderA();
    await act(async () => {
      await pendingA;
    });
    expect(result.current.isLoadingOlder).toBe(true);
    expect(result.current.hasOlder).toBe(true);

    // B settles normally and prepends into B's own cache.
    releaseOlderB();
    await act(async () => {
      await pendingB;
    });
    await waitFor(() => {
      if (result.current.status !== "ok") throw new Error("not ok");
      expect(result.current.data.map((m) => m.messageIndex)).toEqual([
        598, 599, 600, 601,
      ]);
    });
    expect(result.current.isLoadingOlder).toBe(false);
  });
});

describe("useReplyToProviderRequest", () => {
  it("posts the fenced identity and native response to the task reply route", async () => {
    let capturedBody: Record<string, unknown> | undefined;
    server.use(
      http.post(
        "/api/v1/tasks/:taskId/provider-requests/:providerRequestId/reply",
        async ({ params, request }) => {
          expect(params).toMatchObject({
            taskId: "task-1",
            providerRequestId: "provider-request-1",
          });
          capturedBody = (await request.json()) as Record<string, unknown>;
          return HttpResponse.json({
            data: {
              taskId: "task-1",
              attempt: 3,
              providerRequestId: "provider-request-1",
              requestId: capturedBody.requestId,
              responseDigest: "digest-1",
              deliveryDisposition: "delivered",
            },
          });
        },
      ),
    );
    const { wrapper, queryClient } = createWrapper();
    queryClient.setQueryData(["session-timeline", "session-1"], {});
    const { result } = renderHook(() => useReplyToProviderRequest(), {
      wrapper,
    });

    await act(async () => {
      await result.current.mutateAsync({
        sessionId: "session-1",
        taskId: "task-1",
        attempt: 3,
        providerRequestId: "provider-request-1",
        validatedResponse: {
          g5AnswersByIndex: { "0": { selectedOptionIndexes: [1] } },
        },
      });
    });

    expect(capturedBody).toMatchObject({
      attempt: 3,
      validatedResponse: {
        g5AnswersByIndex: { "0": { selectedOptionIndexes: [1] } },
      },
    });
    expect(capturedBody?.requestId).toEqual(expect.any(String));
    expect(
      queryClient.getQueryState(["session-timeline", "session-1"])
        ?.isInvalidated,
    ).toBe(true);
  });

  it("reuses the native reply request id after an ambiguous failure", async () => {
    const requestIds: string[] = [];
    let attempts = 0;
    server.use(
      http.post(
        "/api/v1/tasks/:taskId/provider-requests/:providerRequestId/reply",
        async ({ request }) => {
          attempts += 1;
          const body = (await request.json()) as { requestId: string };
          requestIds.push(body.requestId);
          if (attempts === 1) {
            return HttpResponse.json(
              { error: { message: "reply outcome unknown" } },
              { status: 503 },
            );
          }
          return HttpResponse.json({
            data: {
              taskId: "task-1",
              attempt: 3,
              providerRequestId: "provider-request-1",
              requestId: body.requestId,
              responseDigest: "digest-1",
              deliveryDisposition: "delivered",
            },
          });
        },
      ),
    );
    const { wrapper } = createWrapper();
    const { result } = renderHook(() => useReplyToProviderRequest(), {
      wrapper,
    });
    const input = {
      sessionId: "session-1",
      taskId: "task-1",
      attempt: 3,
      providerRequestId: "provider-request-1",
      validatedResponse: {
        g5AnswersByIndex: { "0": { selectedOptionIndexes: [1] } },
      },
    };

    await expect(result.current.mutateAsync(input)).rejects.toThrow();
    await act(async () => {
      await result.current.mutateAsync(input);
    });

    expect(requestIds).toHaveLength(2);
    expect(requestIds[1]).toBe(requestIds[0]);
  });
});

describe("useSendChatMessage", () => {
  it("sends chat message with optimistic update", async () => {
    server.use(
      http.post("/api/v1/sessions/:sessionId/messages", () =>
        HttpResponse.json({ data: mockNewMessage }, { status: 201 }),
      ),
    );

    const { wrapper, queryClient } = createWrapper();

    // Pre-populate messages cache
    queryClient.setQueryData(["chat-messages", "session-1"], mockMessages);

    const { result } = renderHook(() => useSendChatMessage(), { wrapper });

    await act(async () => {
      await result.current.mutateAsync({
        sessionId: "session-1",
        content: "Hello",
      });
    });

    // Cache should be invalidated on settled
    await waitFor(() => {
      const state = queryClient.getQueryState(["chat-messages", "session-1"]);
      expect(state?.isInvalidated).toBe(true);
    });
  });

  it("gives each pending submission a distinct id within the same millisecond", async () => {
    // Never resolve the POST so onSettled's invalidate does not wipe the
    // optimistic rows before we inspect them.
    server.use(
      http.post(
        "/api/v1/sessions/:sessionId/messages",
        () => new Promise<Response>(() => {}),
      ),
    );
    // Freeze the clock so two sends land in the same millisecond — the exact
    // condition under which `optimistic-${Date.now()}` produced colliding
    // React keys.
    const nowSpy = vi.spyOn(Date, "now").mockReturnValue(1_700_000_000_000);

    try {
      const { wrapper, queryClient } = createWrapper();
      queryClient.setQueryData<SessionInputDTO[]>(
        ["session-inputs", "session-1"],
        [],
      );

      const { result } = renderHook(() => useSendChatMessage(), { wrapper });

      act(() => {
        result.current.mutate({ sessionId: "session-1", content: "first" });
        result.current.mutate({ sessionId: "session-1", content: "second" });
      });

      await waitFor(() => {
        const msgs = queryClient.getQueryData<SessionInputDTO[]>([
          "session-inputs",
          "session-1",
        ]);
        expect(msgs?.length).toBe(2);
      });

      const ids = (
        queryClient.getQueryData<SessionInputDTO[]>([
          "session-inputs",
          "session-1",
        ]) ?? []
      ).map((m) => m.id);
      expect(new Set(ids).size).toBe(2);
    } finally {
      nowSpy.mockRestore();
    }
  });

  it("keeps submission ids distinct without crypto.randomUUID (fallback)", async () => {
    server.use(
      http.post(
        "/api/v1/sessions/:sessionId/messages",
        () => new Promise<Response>(() => {}),
      ),
    );
    const nowSpy = vi.spyOn(Date, "now").mockReturnValue(1_700_000_000_000);
    // Force the plain-HTTP browser branch: getRandomValues remains available,
    // but randomUUID does not. Distinct deterministic bytes prove uniqueness.
    let seed = 0;
    vi.stubGlobal("crypto", {
      getRandomValues: (bytes: Uint8Array) => {
        bytes.fill(seed);
        seed += 1;
        return bytes;
      },
    });

    try {
      const { wrapper, queryClient } = createWrapper();
      queryClient.setQueryData<SessionInputDTO[]>(
        ["session-inputs", "session-1"],
        [],
      );

      const { result } = renderHook(() => useSendChatMessage(), { wrapper });

      act(() => {
        result.current.mutate({ sessionId: "session-1", content: "first" });
        result.current.mutate({ sessionId: "session-1", content: "second" });
      });

      await waitFor(() => {
        const msgs = queryClient.getQueryData<SessionInputDTO[]>([
          "session-inputs",
          "session-1",
        ]);
        expect(msgs?.length).toBe(2);
      });

      const ids = (
        queryClient.getQueryData<SessionInputDTO[]>([
          "session-inputs",
          "session-1",
        ]) ?? []
      ).map((m) => m.id);
      expect(new Set(ids).size).toBe(2);
      expect(ids.every((id) => id.startsWith("optimistic-"))).toBe(true);
      expect(ids.some((id) => id.includes("undefined"))).toBe(false);
    } finally {
      vi.unstubAllGlobals();
      nowSpy.mockRestore();
    }
  });

  it("sends correct request body for message", async () => {
    let capturedBody: unknown = null;
    server.use(
      http.post("/api/v1/sessions/:sessionId/messages", async ({ request }) => {
        capturedBody = await request.json();
        return HttpResponse.json({ data: mockNewMessage }, { status: 201 });
      }),
    );

    const { wrapper } = createWrapper();
    const { result } = renderHook(() => useSendChatMessage(), { wrapper });

    await act(async () => {
      await result.current.mutateAsync({
        sessionId: "session-1",
        content: "Hello world",
      });
    });

    expect(capturedBody).toEqual({ content: "Hello world" });
  });

  it("sends the turn's account with its model and effort (routes/chat D33)", async () => {
    let capturedBody: unknown = null;
    server.use(
      http.post("/api/v1/sessions/:sessionId/messages", async ({ request }) => {
        capturedBody = await request.json();
        return HttpResponse.json({ data: mockNewMessage }, { status: 201 });
      }),
    );

    const { wrapper } = createWrapper();
    const { result } = renderHook(() => useSendChatMessage(), { wrapper });

    await act(async () => {
      await result.current.mutateAsync({
        sessionId: "session-1",
        content: "Keep going",
        options: {
          model: "claude-opus-5",
          effort: "high",
          account: { kind: "account", accountId: "acct-claude-1st" },
        },
      });
    });

    expect(capturedBody).toEqual({
      content: "Keep going",
      model: "claude-opus-5",
      effort: "high",
      account: { kind: "account", accountId: "acct-claude-1st" },
    });
  });

  it("sends active and open file context for message", async () => {
    let capturedBody: unknown = null;
    server.use(
      http.post("/api/v1/sessions/:sessionId/messages", async ({ request }) => {
        capturedBody = await request.json();
        return HttpResponse.json({ data: mockNewMessage }, { status: 201 });
      }),
    );

    const { wrapper } = createWrapper();
    const { result } = renderHook(() => useSendChatMessage(), { wrapper });

    await act(async () => {
      await result.current.mutateAsync({
        sessionId: "session-1",
        content: "Hello world",
        context: {
          activeFilePath: "src/main.rs",
          openFilePaths: ["src/main.rs", "README.md"],
        },
      });
    });

    expect(capturedBody).toMatchObject({
      content: "Hello world",
      context: {
        activeFilePath: "src/main.rs",
        openFilePaths: ["src/main.rs", "README.md"],
      },
    });
  });

  it("includes project references in send context", async () => {
    let capturedBody: unknown = null;
    server.use(
      http.post("/api/v1/sessions/:sessionId/messages", async ({ request }) => {
        capturedBody = await request.json();
        return HttpResponse.json({ data: mockNewMessage }, { status: 201 });
      }),
    );

    const { wrapper } = createWrapper();
    const { result } = renderHook(() => useSendChatMessage(), { wrapper });

    await act(async () => {
      await result.current.mutateAsync({
        sessionId: "session-1",
        content: "Use this reference",
        context: {
          projectReferences: [
            {
              id: "source_file:src/main.rs",
              kind: "source_file",
              label: "main.rs",
              path: "src/main.rs",
            },
          ],
        },
      });
    });

    expect(capturedBody).toMatchObject({
      content: "Use this reference",
      context: {
        projectReferences: [
          {
            id: "source_file:src/main.rs",
            kind: "source_file",
            label: "main.rs",
            path: "src/main.rs",
          },
        ],
      },
    });
  });

  it("handles send message error and rolls back optimistic update", async () => {
    server.use(
      http.post("/api/v1/sessions/:sessionId/messages", () =>
        HttpResponse.json(
          {
            error: {
              code: "INTERNAL_ERROR",
              message: "Server error",
              requestId: "req-err",
            },
          },
          { status: 500 },
        ),
      ),
    );

    const { wrapper, queryClient } = createWrapper();

    // Pre-populate messages cache
    const existingMessages = [...mockMessages];
    queryClient.setQueryData(["chat-messages", "session-1"], existingMessages);

    const { result } = renderHook(() => useSendChatMessage(), { wrapper });

    await act(async () => {
      try {
        await result.current.mutateAsync({
          sessionId: "session-1",
          content: "Hello",
        });
      } catch {
        // Expected error
      }
    });

    // Cache should be rolled back to original (no optimistic message)
    await waitFor(() => {
      const cached = queryClient.getQueryData<ChatMessageDTO[]>([
        "chat-messages",
        "session-1",
      ]);
      // After rollback + invalidation, length should match original
      expect(cached).toHaveLength(existingMessages.length);
    });
  });

  it("onError removes only the optimistic row and preserves a chunk streamed mid-send", async () => {
    // A steer that fails while a prior turn is still streaming must roll back
    // ONLY the optimistic user row, not the whole pre-send snapshot — otherwise
    // every chat.chunk delta that landed after onMutate is deterministically
    // dropped (fleet review #1106 / #1115).
    const { wrapper, queryClient } = createWrapper();

    const streaming: ChatMessageDTO[] = [
      { ...mockMessages[0] },
      {
        ...mockMessages[1],
        id: "assistant-live",
        content: "partial",
        status: "streaming",
      },
    ];
    queryClient.setQueryData(["chat-messages", "session-1"], streaming);

    server.use(
      http.post("/api/v1/sessions/:sessionId/messages", () => {
        // Simulate a chat.chunk landing in the same cache while the send is in
        // flight (after onMutate appended the optimistic user row).
        queryClient.setQueryData<ChatMessageDTO[]>(
          ["chat-messages", "session-1"],
          (old) =>
            (old ?? []).map((message) =>
              message.id === "assistant-live"
                ? { ...message, content: message.content + " delta" }
                : message,
            ),
        );
        return HttpResponse.json(
          {
            error: {
              code: "INTERNAL_ERROR",
              message: "Server error",
              requestId: "req-err",
            },
          },
          { status: 500 },
        );
      }),
    );

    const { result } = renderHook(() => useSendChatMessage(), { wrapper });
    await act(async () => {
      try {
        await result.current.mutateAsync({
          sessionId: "session-1",
          content: "steer",
        });
      } catch {
        // Expected error
      }
    });

    await waitFor(() => {
      const cached =
        queryClient.getQueryData<ChatMessageDTO[]>([
          "chat-messages",
          "session-1",
        ]) ?? [];
      // The failed optimistic user row is gone...
      expect(
        cached.some((m) => m.content === "steer" && m.status === "pending"),
      ).toBe(false);
      // ...but the mid-send streamed delta survives the rollback.
      const live = cached.find((m) => m.id === "assistant-live");
      expect(live?.content).toBe("partial delta");
    });
  });

  it("onSuccess clears the optimistic row's pending state so a lost terminal event can't leave it stuck", async () => {
    // onSettled refetchType "none" defers id reconciliation to the terminal WS
    // event; onSuccess must at least clear the transient pending state on POST
    // success so a dropped chat.done doesn't leave the row spinning forever
    // (fleet review #1106 / #1115).
    const { wrapper, queryClient } = createWrapper();
    queryClient.setQueryData(["chat-messages", "session-1"], [...mockMessages]);
    server.use(
      http.post("/api/v1/sessions/:sessionId/messages", () =>
        HttpResponse.json({
          data: { messageId: "assistant-srv", status: "streaming" },
        }),
      ),
    );

    const { result } = renderHook(() => useSendChatMessage(), { wrapper });
    await act(async () => {
      await result.current.mutateAsync({
        sessionId: "session-1",
        content: "hi there",
      });
    });

    await waitFor(() => {
      const cached =
        queryClient.getQueryData<ChatMessageDTO[]>([
          "chat-messages",
          "session-1",
        ]) ?? [];
      const optimistic = cached.find((m) => m.content === "hi there");
      expect(optimistic).toBeTruthy();
      // No longer stuck pending after the server accepted the send.
      expect(optimistic?.status).not.toBe("pending");
    });
  });

  it("onSuccess inserts the authoritative assistant placeholder so the turn appears streaming locally", async () => {
    // The settle-time invalidation is deliberately refetch-free, so without
    // this insert the client never learns the pending assistant row exists —
    // the session never appears streaming locally, the Working… indicator and
    // the polling it gates never start, and the reply pops in all at once at
    // chat.done (job_chat_draft_hooks D38, issue #1173).
    const { wrapper, queryClient } = createWrapper();
    queryClient.setQueryData(["chat-messages", "session-1"], [...mockMessages]);
    server.use(
      http.post("/api/v1/sessions/:sessionId/messages", () =>
        HttpResponse.json({
          data: { messageId: "assistant-srv", status: "pending" },
        }),
      ),
    );

    const { result } = renderHook(() => useSendChatMessage(), { wrapper });
    await act(async () => {
      await result.current.mutateAsync({
        sessionId: "session-1",
        content: "hi there",
      });
    });

    await waitFor(() => {
      const cached =
        queryClient.getQueryData<ChatMessageDTO[]>([
          "chat-messages",
          "session-1",
        ]) ?? [];
      const last = cached[cached.length - 1];
      // The server-persisted assistant placeholder is the tail row, by its
      // real id, so isChatStreaming derivation sees an assistant pending row.
      expect(last?.id).toBe("assistant-srv");
      expect(last?.role).toBe("assistant");
      expect(last?.status).toBe("pending");
      expect(last?.content).toBe("");
      expect(last?.messageIndex).toBe(
        cached[cached.length - 2]!.messageIndex + 1,
      );
    });
  });

  it("onSuccess stamps the assistant placeholder with the SERVER createdAt, not the browser clock", async () => {
    // Regression for PR #2343 review: the assistant row's createdAt must be the
    // server-authored timestamp — NEVER new Date(), whose client clock skew can
    // push the row ahead of valid current-turn activity. A past server value
    // proves the server timestamp is used (a browser clock would read ~now).
    const { wrapper, queryClient } = createWrapper();
    queryClient.setQueryData(["chat-messages", "session-1"], [...mockMessages]);
    const SERVER_TS = "2020-01-01T00:00:00.000Z";
    server.use(
      http.post("/api/v1/sessions/:sessionId/messages", () =>
        HttpResponse.json({
          data: {
            messageId: "assistant-srv",
            status: "pending",
            createdAt: SERVER_TS,
          },
        }),
      ),
    );
    const { result } = renderHook(() => useSendChatMessage(), { wrapper });
    await act(async () => {
      await result.current.mutateAsync({
        sessionId: "session-1",
        content: "hi there",
      });
    });
    await waitFor(() => {
      const cached =
        queryClient.getQueryData<ChatMessageDTO[]>([
          "chat-messages",
          "session-1",
        ]) ?? [];
      expect(cached.find((m) => m.id === "assistant-srv")?.createdAt).toBe(
        SERVER_TS,
      );
    });
  });

  it("onSuccess falls back to the optimistic timestamp boundary when the API omits createdAt", async () => {
    // Rolling-upgrade path: an older API can omit createdAt, so the assistant
    // row retains the pre-request optimistic boundary (the user row's own
    // createdAt) rather than a fresh browser-clock stamp (PR #2343 review).
    const { wrapper, queryClient } = createWrapper();
    queryClient.setQueryData(["chat-messages", "session-1"], [...mockMessages]);
    server.use(
      http.post("/api/v1/sessions/:sessionId/messages", () =>
        HttpResponse.json({
          data: { messageId: "assistant-srv", status: "pending" },
        }),
      ),
    );
    const { result } = renderHook(() => useSendChatMessage(), { wrapper });
    await act(async () => {
      await result.current.mutateAsync({
        sessionId: "session-1",
        content: "hi there",
      });
    });
    await waitFor(() => {
      const cached =
        queryClient.getQueryData<ChatMessageDTO[]>([
          "chat-messages",
          "session-1",
        ]) ?? [];
      const optimisticUser = cached.find(
        (m) => m.content === "hi there" && m.role === "user",
      );
      const assistant = cached.find((m) => m.id === "assistant-srv");
      expect(optimisticUser?.createdAt).toBeTruthy();
      // The assistant boundary is the retained optimistic value verbatim, which
      // a browser-clock stamp taken at onSuccess time would not equal.
      expect(assistant?.createdAt).toBe(optimisticUser?.createdAt);
    });
  });

  it("onSuccess invalidates the session-task projection after an accepted send", async () => {
    // Regression for PR #2343 review: the accepted send must invalidate
    // ["session-task", sessionId] so a follow-up's empty timeline cannot inherit
    // the preceding turn's lifecycle label until the periodic poll runs
    // (chat_panel_runtime D24, issue #1050).
    const { wrapper, queryClient } = createWrapper();
    queryClient.setQueryData(["chat-messages", "session-1"], [...mockMessages]);
    const invalidateSpy = vi.spyOn(queryClient, "invalidateQueries");
    server.use(
      http.post("/api/v1/sessions/:sessionId/messages", () =>
        HttpResponse.json({
          data: { messageId: "assistant-srv", status: "pending" },
        }),
      ),
    );
    const { result } = renderHook(() => useSendChatMessage(), { wrapper });
    await act(async () => {
      await result.current.mutateAsync({
        sessionId: "session-1",
        content: "hi there",
      });
    });
    await waitFor(() => {
      expect(invalidateSpy).toHaveBeenCalledWith({
        queryKey: ["session-task", "session-1"],
      });
    });
  });

  it("onSuccess does not duplicate the assistant placeholder when the row already reached the cache", async () => {
    // A concurrent refetch or WS-driven repopulation can deliver the assistant
    // row before onSuccess runs; the insert is id-deduplicated (D38).
    const { wrapper, queryClient } = createWrapper();
    queryClient.setQueryData(["chat-messages", "session-1"], [...mockMessages]);
    server.use(
      http.post("/api/v1/sessions/:sessionId/messages", () => {
        // Simulate the row landing in the cache while the send is in flight.
        queryClient.setQueryData<ChatMessageDTO[]>(
          ["chat-messages", "session-1"],
          (old) => [
            ...(old ?? []),
            {
              id: "assistant-srv",
              sessionId: "session-1",
              role: "assistant",
              content: "already here",
              artifacts: null,
              citations: null,
              inputTokens: null,
              outputTokens: null,
              status: "streaming",
              messageIndex: (old?.length ?? 0) + 1,
              createdAt: new Date().toISOString(),
            },
          ],
        );
        return HttpResponse.json({
          data: { messageId: "assistant-srv", status: "pending" },
        });
      }),
    );

    const { result } = renderHook(() => useSendChatMessage(), { wrapper });
    await act(async () => {
      await result.current.mutateAsync({
        sessionId: "session-1",
        content: "hi there",
      });
    });

    await waitFor(() => {
      const cached =
        queryClient.getQueryData<ChatMessageDTO[]>([
          "chat-messages",
          "session-1",
        ]) ?? [];
      const rows = cached.filter((m) => m.id === "assistant-srv");
      // Exactly one row, and the already-delivered content/status is preserved.
      expect(rows).toHaveLength(1);
      expect(rows[0]!.content).toBe("already here");
      expect(rows[0]!.status).toBe("streaming");
    });
  });

  // Shared server model for the transcript regressions: a mutable row store
  // served through the real messages contract — tail=true returns the latest
  // `limit` rows ascending (routes/chat I6), forward-cursor requests keep the
  // keyset behavior. `stats.gets` counts requests so tests can assert the
  // one-request-per-refresh cost bound (D39).
  function paginatedTranscript(initialLength: number) {
    const rows: ChatMessageDTO[] = Array.from(
      { length: initialLength },
      (_, i) => ({
        id: `msg-${i}`,
        sessionId: "session-1",
        role: i % 2 === 0 ? "user" : "assistant",
        content: `message ${i}`,
        artifacts: null,
        citations: null,
        inputTokens: null,
        outputTokens: null,
        status: "complete",
        messageIndex: i,
        createdAt: new Date(1700000000000 + i * 1000).toISOString(),
      }),
    );
    const stats = { gets: 0 };
    const getHandler = http.get(
      "/api/v1/sessions/:sessionId/messages",
      ({ request }) => {
        stats.gets += 1;
        const search = new URL(request.url).searchParams;
        const limitParam = search.get("limit");
        const limit = limitParam === null ? 50 : Number(limitParam);
        if (search.get("tail") === "true" && search.get("cursor") === null) {
          const window = rows.slice(-limit);
          return HttpResponse.json({
            data: {
              messages: window,
              cursor: window.length
                ? window[window.length - 1].messageIndex
                : null,
              hasMore: rows.length > limit,
            },
          });
        }
        const cursorParam = search.get("cursor");
        const cursor = cursorParam === null ? null : Number(cursorParam);
        const after =
          cursor === null ? rows : rows.filter((m) => m.messageIndex > cursor);
        const page = after.slice(0, limit);
        return HttpResponse.json({
          data: {
            messages: page,
            cursor: page.length ? page[page.length - 1].messageIndex : null,
            hasMore: after.length > limit,
          },
        });
      },
    );
    // The POST persists the user row and the pending assistant placeholder
    // server-side (chat_agent_session D9) before returning the assistant id.
    const postHandler = http.post(
      "/api/v1/sessions/:sessionId/messages",
      async ({ request }) => {
        const body = (await request.json()) as { content: string };
        const base = rows[rows.length - 1]!.messageIndex;
        rows.push(
          {
            id: "user-srv",
            sessionId: "session-1",
            role: "user",
            content: body.content,
            artifacts: null,
            citations: null,
            inputTokens: null,
            outputTokens: null,
            status: "complete",
            messageIndex: base + 1,
            createdAt: new Date().toISOString(),
          },
          {
            id: "assistant-srv",
            sessionId: "session-1",
            role: "assistant",
            content: "",
            artifacts: null,
            citations: null,
            inputTokens: null,
            outputTokens: null,
            status: "pending",
            messageIndex: base + 2,
            createdAt: new Date().toISOString(),
          },
        );
        return HttpResponse.json({
          data: { messageId: "assistant-srv", status: "pending" },
        });
      },
    );
    return { rows, stats, getHandler, postHandler };
  }

  it("fetches the latest window so an active refetch reaches the seeded turn on a >50-message transcript", async () => {
    // Once the placeholder makes isChatStreaming true, WorkspacePage's
    // refreshActiveChatTurn actively refetches this query. The tail read must
    // cover the persisted current turn — main's first-page fetch truncated the
    // cache to rows 0–49 and deleted the just-seeded turn
    // (job_chat_draft_hooks D39, PR #1174 review rounds 1–2).
    const { wrapper, queryClient } = createWrapper();
    const { getHandler, postHandler } = paginatedTranscript(120);
    server.use(getHandler, postHandler);

    const { result } = renderHook(
      () => ({
        messages: useChatMessages("session-1"),
        send: useSendChatMessage(),
      }),
      { wrapper },
    );
    await waitFor(() => expect(result.current.messages.status).toBe("ok"));
    // The initial mount already sees the whole transcript, not one page.
    const initial = result.current.messages;
    expect(initial.status === "ok" && initial.data).toHaveLength(120);

    await act(async () => {
      await result.current.send.mutateAsync({
        sessionId: "session-1",
        content: "follow-up on a long transcript",
      });
    });

    // Placeholder seeded at the tail.
    await waitFor(() => {
      const cached =
        queryClient.getQueryData<ChatMessageDTO[]>([
          "chat-messages",
          "session-1",
        ]) ?? [];
      expect(cached[cached.length - 1]?.id).toBe("assistant-srv");
    });

    // The same active invalidation refreshActiveChatTurn performs.
    await act(async () => {
      await queryClient.invalidateQueries({
        queryKey: ["chat-messages", "session-1"],
      });
    });

    await waitFor(() => {
      const cached =
        queryClient.getQueryData<ChatMessageDTO[]>([
          "chat-messages",
          "session-1",
        ]) ?? [];
      // Full persisted transcript: 120 history rows + user + placeholder,
      // with the optimistic user row replaced by the persisted one.
      expect(cached).toHaveLength(122);
      expect(
        cached.filter((m) => m.content === "follow-up on a long transcript"),
      ).toHaveLength(1);
      const last = cached[cached.length - 1];
      expect(last?.id).toBe("assistant-srv");
      expect(last?.role).toBe("assistant");
      expect(last?.status).toBe("pending");
    });
  });

  it("converges the seeded placeholder to the persisted completed row when chat.done is missed", async () => {
    // ws_event_envelope D5: WS delivery is best-effort; REST is the recovery
    // source. If every terminal frame for the turn is dropped, the D21
    // recovery poll's refetch must still observe the persisted row's terminal
    // state on a >50-message transcript (PR #1174 second-pass review).
    const { wrapper, queryClient } = createWrapper();
    const { rows, getHandler, postHandler } = paginatedTranscript(120);
    server.use(getHandler, postHandler);

    const { result } = renderHook(
      () => ({
        messages: useChatMessages("session-1"),
        send: useSendChatMessage(),
      }),
      { wrapper },
    );
    await waitFor(() => expect(result.current.messages.status).toBe("ok"));

    await act(async () => {
      await result.current.send.mutateAsync({
        sessionId: "session-1",
        content: "question with a lost chat.done",
      });
    });
    await waitFor(() => {
      const cached =
        queryClient.getQueryData<ChatMessageDTO[]>([
          "chat-messages",
          "session-1",
        ]) ?? [];
      expect(cached[cached.length - 1]?.id).toBe("assistant-srv");
    });

    // The turn completes server-side, but every terminal WS frame is dropped.
    const assistantRow = rows.find((m) => m.id === "assistant-srv")!;
    assistantRow.status = "complete";
    assistantRow.content = "final answer";

    // Recovery poll: the active authoritative refetch alone must converge.
    await act(async () => {
      await queryClient.invalidateQueries({
        queryKey: ["chat-messages", "session-1"],
      });
    });

    await waitFor(() => {
      const cached =
        queryClient.getQueryData<ChatMessageDTO[]>([
          "chat-messages",
          "session-1",
        ]) ?? [];
      const assistant = cached.find((m) => m.id === "assistant-srv");
      expect(assistant?.status).toBe("complete");
      expect(assistant?.content).toBe("final answer");
      // Nothing stale survives past the persisted end.
      expect(cached[cached.length - 1]?.id).toBe("assistant-srv");
    });
  });

  it("converges a missed chat.done beyond any page boundary with a single latest-window request", async () => {
    // Round-3/4 review boundary case: the completed terminal row lives deep in
    // a 2000+-row transcript. The tail read covers the current turn by
    // construction, so one request converges the seeded placeholder to the
    // persisted completed row — no page cap to strand it, no multi-page drain
    // for the 3s invalidation to starve, and cost stays one request per
    // refresh (job_chat_draft_hooks D39).
    const { wrapper, queryClient } = createWrapper();
    const { rows, stats, getHandler } = paginatedTranscript(2050);
    // The turn completed server-side at index 2050; its terminal WS frames
    // were all dropped.
    rows.push({
      id: "assistant-srv",
      sessionId: "session-1",
      role: "assistant",
      content: "final answer",
      artifacts: null,
      citations: null,
      inputTokens: null,
      outputTokens: null,
      status: "complete",
      messageIndex: 2050,
      createdAt: new Date().toISOString(),
    });
    server.use(getHandler);

    // The client had previously loaded some older history and seeded the
    // placeholder for the live turn.
    const seeded: ChatMessageDTO[] = [
      ...rows.slice(0, 50),
      {
        id: "assistant-srv",
        sessionId: "session-1",
        role: "assistant",
        content: "",
        artifacts: null,
        citations: null,
        inputTokens: null,
        outputTokens: null,
        status: "pending",
        messageIndex: 2050,
        createdAt: new Date().toISOString(),
      },
    ];
    queryClient.setQueryData(["chat-messages", "session-1"], seeded);

    renderHook(() => useChatMessages("session-1"), { wrapper });

    const getsBefore = stats.gets;
    await act(async () => {
      await queryClient.invalidateQueries({
        queryKey: ["chat-messages", "session-1"],
      });
    });

    await waitFor(() => {
      // One bounded request per refresh — the D39 cost/starvation guarantee.
      expect(stats.gets - getsBefore).toBe(1);
      const cached =
        queryClient.getQueryData<ChatMessageDTO[]>([
          "chat-messages",
          "session-1",
        ]) ?? [];
      // REST recovery alone converged the placeholder to the persisted
      // completed row.
      const assistant = cached.find((m) => m.id === "assistant-srv");
      expect(assistant?.status).toBe("complete");
      expect(assistant?.content).toBe("final answer");
      expect(cached[cached.length - 1]?.id).toBe("assistant-srv");
      // Window merge preserved the previously loaded older history (rows
      // 0..49 sit below the 500-row window) without duplicating window rows.
      expect(cached.filter((m) => m.id === "msg-0")).toHaveLength(1);
      expect(cached).toHaveLength(50 + 500);
    });
  });

  it("recovery refresh is single-flight: a slow window read survives repeating poll ticks", async () => {
    // chat_panel_runtime D21 / PR #1174 round-5 review: the D21 poll
    // invalidates on a repeating interval. With TanStack's default
    // cancelRefetch: true (and the queryFn consuming the abort signal), a
    // window read slower than the tick is aborted and restarted by every tick
    // — the persisted terminal row never reaches the cache. The production
    // path (refreshChatMessages) passes cancelRefetch: false, so ticks reuse
    // the in-flight read and one slow request converges.
    const { wrapper, queryClient } = createWrapper();
    const { rows, stats, getHandler } = paginatedTranscript(120);
    rows.push({
      id: "assistant-srv",
      sessionId: "session-1",
      role: "assistant",
      content: "final answer",
      artifacts: null,
      citations: null,
      inputTokens: null,
      outputTokens: null,
      status: "complete",
      messageIndex: 120,
      createdAt: new Date().toISOString(),
    });
    // Every response is slower (200ms) than the poll tick (50ms).
    server.use(
      http.get("/api/v1/sessions/:sessionId/messages", async () => {
        await new Promise((resolve) => setTimeout(resolve, 200));
      }),
      getHandler,
    );

    // The client had seeded the placeholder; every terminal WS frame dropped.
    queryClient.setQueryData(
      ["chat-messages", "session-1"],
      [
        ...rows.slice(0, 120),
        {
          id: "assistant-srv",
          sessionId: "session-1",
          role: "assistant",
          content: "",
          artifacts: null,
          citations: null,
          inputTokens: null,
          outputTokens: null,
          status: "pending",
          messageIndex: 120,
          createdAt: new Date().toISOString(),
        },
      ],
    );

    renderHook(() => useChatMessages("session-1"), { wrapper });

    // The D21 poll analogue: a repeating tick far faster than the request.
    const interval = setInterval(() => {
      void refreshChatMessages(queryClient, "session-1");
    }, 50);
    try {
      await waitFor(
        () => {
          const cached =
            queryClient.getQueryData<ChatMessageDTO[]>([
              "chat-messages",
              "session-1",
            ]) ?? [];
          const assistant = cached.find((m) => m.id === "assistant-srv");
          expect(assistant?.status).toBe("complete");
          expect(assistant?.content).toBe("final answer");
        },
        { timeout: 3000 },
      );
    } finally {
      clearInterval(interval);
    }
    // Bounded requests: in-flight reads were reused across ticks, not
    // aborted-and-restarted once per tick (~15+ starts in 750ms would mean
    // cancellation churn).
    expect(stats.gets).toBeLessThanOrEqual(4);
  });

  it("onSettled marks chat-messages stale without a mid-stream refetch that clobbers live chunks", async () => {
    // On a successful steer, the POST settles mid-stream. An active refetch here
    // would resolve to the server-behind snapshot and drop the client's live
    // chat.chunk deltas; refetchType "none" defers reconvergence to the terminal
    // WS events instead (fleet review #1106 / #1115).
    const { wrapper, queryClient } = createWrapper();

    // The server GET is deliberately behind the live stream.
    const serverBehind: ChatMessageDTO[] = [
      { ...mockMessages[0] },
      {
        ...mockMessages[1],
        id: "assistant-live",
        content: "partial",
        status: "streaming",
      },
    ];
    server.use(
      http.get("/api/v1/sessions/:sessionId/messages", () =>
        HttpResponse.json({ data: serverBehind }),
      ),
      http.post("/api/v1/sessions/:sessionId/messages", () =>
        HttpResponse.json({
          data: { messageId: "srv-user", status: "pending" },
        }),
      ),
    );

    const { result } = renderHook(
      () => ({
        messages: useChatMessages("session-1"),
        send: useSendChatMessage(),
      }),
      { wrapper },
    );
    await waitFor(() => expect(result.current.messages.status).toBe("ok"));

    // The live stream advances the assistant message beyond the server snapshot.
    act(() => {
      queryClient.setQueryData<ChatMessageDTO[]>(
        ["chat-messages", "session-1"],
        (old) =>
          (old ?? []).map((message) =>
            message.id === "assistant-live"
              ? { ...message, content: "partial delta" }
              : message,
          ),
      );
    });

    await act(async () => {
      await result.current.send.mutateAsync({
        sessionId: "session-1",
        content: "steer",
      });
    });

    await waitFor(() => {
      const cached =
        queryClient.getQueryData<ChatMessageDTO[]>([
          "chat-messages",
          "session-1",
        ]) ?? [];
      // The live delta must survive: onSettled marked the query stale rather
      // than refetching the server-behind snapshot mid-stream.
      const live = cached.find((m) => m.id === "assistant-live");
      expect(live?.content).toBe("partial delta");
      expect(
        queryClient.getQueryState(["chat-messages", "session-1"])
          ?.isInvalidated,
      ).toBe(true);
    });
  });
});

describe("useStartChatMessage", () => {
  const agentLaunch = {
    preferences: {
      schemaVersion: 1 as const,
      provider: { source: "inherit_configured" as const },
      model: { source: "inherit_configured" as const },
      permissionMode: { source: "inherit_configured" as const },
      effort: { source: "inherit_configured" as const },
    },
    confirmedResolutionId: "alr_chat_test",
  };

  it("warms exact scoped session caches after first-turn session start", async () => {
    const startedSession: ChatSessionDTO = {
      ...mockSession,
      id: "started-session-1",
      scopeType: "workspace",
      scopeRef: null,
      agentProvider: "claude_code",
      messageCount: 2,
    };
    server.use(
      http.post(
        "/api/v1/workspaces/:id/sessions/start",
        async ({ request }) => {
          expect(await request.json()).toMatchObject({
            content: "Hello",
            scopeType: "workspace",
            agentLaunch,
            context: {
              activeFilePath: "src/lib.rs",
              openFilePaths: ["src/lib.rs", "docs/plan.md"],
            },
          });
          return HttpResponse.json(
            {
              data: {
                session: startedSession,
                message: { messageId: "msg-user-start", status: "queued" },
                assistantMessageId: "msg-assistant-start",
                taskId: "task-start",
              },
            },
            { status: 201 },
          );
        },
      ),
    );

    const { wrapper, queryClient } = createWrapper();
    queryClient.setQueryData(
      ["chat-sessions", "ws-123", "workspace"],
      mockSessions,
    );
    queryClient.setQueryData<unknown[]>(["workspace-tasks", "ws-123"], []);

    const { result } = renderHook(() => useStartChatMessage(), { wrapper });

    await act(async () => {
      const response = await result.current.mutateAsync({
        workspaceId: "ws-123",
        content: "Hello",
        scopeType: "workspace",
        agentLaunch,
        context: {
          activeFilePath: "src/lib.rs",
          openFilePaths: ["src/lib.rs", "docs/plan.md"],
        },
      });
      expect(response.session.id).toBe("started-session-1");
    });

    expect(
      queryClient
        .getQueryData<
          ChatSessionDTO[]
        >(["chat-sessions", "ws-123", "workspace"])
        ?.map((session) => session.id),
    ).toEqual(["started-session-1", "session-1"]);
    expect(
      queryClient.getQueryData<ChatSessionDTO>([
        "chat-session",
        "started-session-1",
      ])?.id,
    ).toBe("started-session-1");
    expect(
      queryClient.getQueryState(["workspace-tasks", "ws-123"])?.isInvalidated,
    ).toBe(true);
  });

  it("includes project references in first-turn start context", async () => {
    let capturedBody: unknown = null;
    const startedSession: ChatSessionDTO = {
      ...mockSession,
      id: "started-session-reference",
      scopeType: "workspace",
      scopeRef: null,
      agentProvider: "claude_code",
      messageCount: 2,
    };
    server.use(
      http.post(
        "/api/v1/workspaces/:id/sessions/start",
        async ({ request }) => {
          capturedBody = await request.json();
          return HttpResponse.json(
            {
              data: {
                session: startedSession,
                message: { messageId: "msg-user-start", status: "queued" },
                assistantMessageId: "msg-assistant-start",
                taskId: "task-start",
              },
            },
            { status: 201 },
          );
        },
      ),
    );

    const { wrapper } = createWrapper();
    const { result } = renderHook(() => useStartChatMessage(), { wrapper });

    await act(async () => {
      await result.current.mutateAsync({
        workspaceId: "ws-123",
        content: "Start with this reference",
        scopeType: "workspace",
        agentLaunch,
        context: {
          projectReferences: [
            {
              id: "project_doc:doc-1",
              kind: "project_doc",
              label: "Requirements",
              path: "docs/requirements.md",
              docId: "doc-1",
            },
          ],
        },
      });
    });

    expect(capturedBody).toMatchObject({
      content: "Start with this reference",
      context: {
        projectReferences: [
          {
            id: "project_doc:doc-1",
            kind: "project_doc",
            label: "Requirements",
            path: "docs/requirements.md",
            docId: "doc-1",
          },
        ],
      },
    });
  });
});

describe("useCreateChatSession", () => {
  it("creates a scoped session without a private branch", async () => {
    let capturedBody: unknown = null;
    server.use(
      http.post("/api/v1/workspaces/:id/sessions", async ({ request }) => {
        capturedBody = await request.json();
        return HttpResponse.json(
          {
            data: {
              ...mockSession,
              id: "session-task-1",
              scopeType: "task",
              scopeRef: "task-123",
              title: null,
            },
          },
          { status: 201 },
        );
      }),
    );

    const { wrapper } = createWrapper();
    const { result } = renderHook(() => useCreateChatSession(), { wrapper });

    await act(async () => {
      const session = await result.current.mutateAsync({
        workspaceId: "ws-123",
        scopeType: "task",
        scopeRef: "task-123",
      });
      expect(session.id).toBe("session-task-1");
      expect(session.scopeType).toBe("task");
    });

    expect(capturedBody).toEqual({
      scopeType: "task",
      scopeRef: "task-123",
    });
  });

  it("updates only the matching scope cache and the unfiltered cache", async () => {
    server.use(
      http.post("/api/v1/workspaces/:id/sessions", () =>
        HttpResponse.json(
          {
            data: {
              ...mockSession,
              id: "session-task-2",
              scopeType: "task",
              scopeRef: "task-999",
              title: null,
            },
          },
          { status: 201 },
        ),
      ),
    );

    const { wrapper, queryClient } = createWrapper();
    const workspaceSessions = [{ ...mockSession, id: "workspace-session-1" }];
    const taskSessions = [
      {
        ...mockSession,
        id: "task-session-existing",
        scopeType: "task",
        scopeRef: "task-123",
      },
    ];

    queryClient.setQueryData(
      ["chat-sessions", "ws-123", "workspace"],
      workspaceSessions,
    );
    queryClient.setQueryData(["chat-sessions", "ws-123", "task"], taskSessions);
    queryClient.setQueryData(
      ["chat-sessions", "ws-123", undefined],
      workspaceSessions,
    );

    const { result } = renderHook(() => useCreateChatSession(), { wrapper });

    await act(async () => {
      await result.current.mutateAsync({
        workspaceId: "ws-123",
        scopeType: "task",
        scopeRef: "task-999",
      });
    });

    const updatedWorkspaceSessions = queryClient.getQueryData<ChatSessionDTO[]>(
      ["chat-sessions", "ws-123", "workspace"],
    );
    const updatedTaskSessions = queryClient.getQueryData<ChatSessionDTO[]>([
      "chat-sessions",
      "ws-123",
      "task",
    ]);
    const updatedUnfilteredSessions = queryClient.getQueryData<
      ChatSessionDTO[]
    >(["chat-sessions", "ws-123", undefined]);

    expect(updatedWorkspaceSessions?.map((session) => session.id)).toEqual([
      "workspace-session-1",
    ]);
    expect(updatedTaskSessions?.map((session) => session.id)).toEqual([
      "session-task-2",
      "task-session-existing",
    ]);
    expect(updatedUnfilteredSessions?.map((session) => session.id)).toEqual([
      "session-task-2",
      "workspace-session-1",
    ]);
  });
});

describe("useStopStreaming", () => {
  it("returns the runtime's disposition and refreshes the turn's history and task", async () => {
    server.use(
      http.post("/api/v1/sessions/:sessionId/messages/:messageId/stop", () =>
        HttpResponse.json({
          data: { stopped: true, disposition: "already_finished" },
        }),
      ),
    );
    const { wrapper, queryClient } = createWrapper();
    const invalidateSpy = vi.spyOn(queryClient, "invalidateQueries");
    const { result } = renderHook(() => useStopStreaming(), { wrapper });
    let outcome: { stopped: boolean; disposition?: string } | undefined;
    await act(async () => {
      outcome = await result.current.mutateAsync({
        sessionId: "session-1",
        messageId: "msg-1",
      });
    });
    expect(outcome).toEqual({ stopped: true, disposition: "already_finished" });
    expect(invalidateSpy).toHaveBeenCalledWith({
      queryKey: ["session-timeline", "session-1"],
    });
    expect(invalidateSpy).toHaveBeenCalledWith({
      queryKey: ["session-task", "session-1"],
    });
  });

  it("sends POST to correct URL", async () => {
    let capturedUrl = "";
    server.use(
      http.post(
        "/api/v1/sessions/:sessionId/messages/:messageId/stop",
        ({ request }) => {
          capturedUrl = new URL(request.url).pathname;
          return new HttpResponse(null, { status: 204 });
        },
      ),
    );

    const { wrapper, queryClient } = createWrapper();
    const invalidateSpy = vi.spyOn(queryClient, "invalidateQueries");
    queryClient.setQueryData(["chat-messages", "session-1"], mockMessages);

    const { result } = renderHook(() => useStopStreaming(), { wrapper });

    await act(async () => {
      await result.current.mutateAsync({
        sessionId: "session-1",
        messageId: "msg-1",
      });
    });

    expect(capturedUrl).toBe("/api/v1/sessions/session-1/messages/msg-1/stop");

    // Verify cache invalidation
    await waitFor(() => {
      const state = queryClient.getQueryState(["chat-messages", "session-1"]);
      expect(state?.isInvalidated).toBe(true);
    });
    expect(invalidateSpy).toHaveBeenCalledWith({
      queryKey: ["chat-messages", "session-1"],
    });
    expect(invalidateSpy).toHaveBeenCalledWith({
      queryKey: ["workspace-tasks"],
    });
    expect(invalidateSpy).toHaveBeenCalledWith({
      queryKey: ["workspace-tasks"],
    });
  });
});

describe("useForkSession", () => {
  it("forks session at index and invalidates sessions cache", async () => {
    server.use(
      http.post("/api/v1/sessions/:sessionId/fork", () =>
        HttpResponse.json(
          { data: { sessionId: "session-forked", messagesCopied: 3 } },
          { status: 201 },
        ),
      ),
    );

    const { wrapper, queryClient } = createWrapper();

    // Pre-populate sessions cache
    queryClient.setQueryData(
      ["chat-sessions", "ws-123", undefined],
      mockSessions,
    );

    const { result } = renderHook(() => useForkSession(), { wrapper });

    await act(async () => {
      const forked = await result.current.mutateAsync({
        sessionId: "session-1",
        workspaceId: "ws-123",
        atIndex: 3,
      });
      expect(forked.sessionId).toBe("session-forked");
    });

    // Verify sessions cache was invalidated
    await waitFor(() => {
      const state = queryClient.getQueryState([
        "chat-sessions",
        "ws-123",
        undefined,
      ]);
      expect(state?.isInvalidated).toBe(true);
    });
  });
});

describe("useRenameChatSession", () => {
  it("patches the session title and updates cached lists", async () => {
    server.use(
      http.patch("/api/v1/sessions/:sessionId", async ({ request }) => {
        const body = (await request.json()) as { title: string | null };
        expect(body.title).toBe("Renamed");
        return HttpResponse.json({
          data: {
            ...mockSession,
            title: "Renamed",
            updatedAt: "2024-11-15T10:40:00Z",
          },
        });
      }),
    );

    const { wrapper, queryClient } = createWrapper();
    queryClient.setQueryData(
      ["chat-sessions", "ws-123", undefined],
      mockSessions,
    );
    queryClient.setQueryData(["chat-session", "session-1"], mockSession);

    const { result } = renderHook(() => useRenameChatSession(), { wrapper });

    await act(async () => {
      await result.current.mutateAsync({
        sessionId: "session-1",
        title: "Renamed",
      });
    });

    expect(
      queryClient.getQueryData<ChatSessionDTO>(["chat-session", "session-1"])
        ?.title,
    ).toBe("Renamed");
    expect(
      queryClient.getQueryData<ChatSessionDTO[]>([
        "chat-sessions",
        "ws-123",
        undefined,
      ])?.[0]?.title,
    ).toBe("Renamed");
  });
});

describe("chat activity (g5.chat.activity)", () => {
  const activity = {
    activeTurnStartedAt: null,
    lastFinishedAt: "2024-11-15T10:30:00Z",
    latestResult: { taskId: "task-1", attempt: 1, hasOutput: true },
    unseenResult: true,
  };

  it("keeps a chat's activity when a rename response carries none", async () => {
    server.use(
      http.patch("/api/v1/sessions/:sessionId", () =>
        HttpResponse.json({ data: { ...mockSession, title: "Renamed" } }),
      ),
    );
    const { wrapper, queryClient } = createWrapper();
    queryClient.setQueryData(
      ["chat-sessions", "ws-123", undefined],
      mockSessions.map((session) =>
        session.id === "session-1" ? { ...session, activity } : session,
      ),
    );
    queryClient.setQueryData(["chat-session", "session-1"], {
      ...mockSession,
      activity,
    });
    const { result } = renderHook(() => useRenameChatSession(), { wrapper });
    await act(async () => {
      await result.current.mutateAsync({
        sessionId: "session-1",
        title: "Renamed",
      });
    });
    const listed = queryClient.getQueryData<ChatSessionDTO[]>([
      "chat-sessions",
      "ws-123",
      undefined,
    ]);
    expect(listed?.[0]).toMatchObject({ title: "Renamed", activity });
    expect(
      queryClient.getQueryData<ChatSessionDTO>(["chat-session", "session-1"]),
    ).toMatchObject({ title: "Renamed", activity });
  });

  it("records an inspection and replaces the cached activity", async () => {
    const inspected = { ...activity, unseenResult: false };
    let body: unknown;
    server.use(
      http.put(
        "/api/v1/sessions/:sessionId/inspection",
        async ({ request }) => {
          body = await request.json();
          return HttpResponse.json({ data: inspected });
        },
      ),
    );
    const { wrapper, queryClient } = createWrapper();
    queryClient.setQueryData(
      ["chat-sessions", "ws-123", undefined],
      [
        { ...mockSession, activity },
        { ...mockSession, id: "session-2", activity },
      ],
    );
    const { result } = renderHook(() => useRecordChatInspection(), { wrapper });
    await act(async () => {
      await result.current.mutateAsync({
        sessionId: "session-1",
        result: activity.latestResult,
      });
    });
    expect(body).toEqual({ taskId: "task-1", attempt: 1 });
    const listed = queryClient.getQueryData<ChatSessionDTO[]>([
      "chat-sessions",
      "ws-123",
      undefined,
    ]);
    expect(
      listed?.find((session) => session.id === "session-1")?.activity,
    ).toEqual(inspected);
    expect(
      listed?.find((session) => session.id !== "session-1")?.activity,
    ).toEqual(activity);
  });

  it("re-reads a transcript that cannot show the unseen result yet, once", async () => {
    const { wrapper, queryClient } = createWrapper();
    const invalidate = vi.spyOn(queryClient, "invalidateQueries");
    const { rerender } = renderHook(
      ({ messages }) =>
        useChatResultInspection("session-1", activity, messages),
      { wrapper, initialProps: { messages: [] as ChatMessageDTO[] } },
    );
    await waitFor(() =>
      expect(invalidate).toHaveBeenCalledWith({
        queryKey: ["chat-messages", "session-1"],
      }),
    );
    rerender({ messages: [] });
    await act(async () => {});
    expect(invalidate).toHaveBeenCalledTimes(1);
  });

  it("reads the transcript again when its first read is still out", async () => {
    const { wrapper, queryClient } = createWrapper();
    let reads = 0;
    // The refresh cancels this read; its rejection is expected.
    const firstRead = queryClient
      .fetchQuery({
        queryKey: ["chat-messages", "session-1"],
        queryFn: () => {
          reads += 1;
          return new Promise<never>(() => {});
        },
      })
      .catch(() => {});
    const cancel = vi.spyOn(queryClient, "cancelQueries");
    renderHook(() => useChatResultInspection("session-1", activity, []), {
      wrapper,
    });
    await waitFor(() =>
      expect(cancel).toHaveBeenCalledWith({
        queryKey: ["chat-messages", "session-1"],
        exact: true,
      }),
    );
    expect(reads).toBe(1);
    await firstRead;
  });

  it("re-reads a transcript that holds only the turn's steered user row", async () => {
    const { wrapper, queryClient } = createWrapper();
    const invalidate = vi.spyOn(queryClient, "invalidateQueries");
    const steered = createMockChatMessage({
      sessionId: "session-1",
      role: "user",
      taskId: "task-1",
      attempt: 1,
    });
    renderHook(
      () => useChatResultInspection("session-1", activity, [steered]),
      { wrapper },
    );
    await waitFor(() =>
      expect(invalidate).toHaveBeenCalledWith({
        queryKey: ["chat-messages", "session-1"],
      }),
    );
  });
});

// ---------------------------------------------------------------------------
// useSessionTimeline — held history, refresh merge and older reads
// (g5.chat.history-window, g5.chat.history-refresh; issue #847)
// ---------------------------------------------------------------------------

describe("useSessionTimeline", () => {
  const SESSION_ID = "s-timeline-1";

  function timelineEntry(id: string, createdAt: string): TimelineEntry {
    return { id, entryType: "tool_call", content: id, metadata: {}, createdAt };
  }

  /** Entry `e<n>` created at second n of the minute. */
  function numbered(n: number): TimelineEntry {
    return timelineEntry(
      `e${n}`,
      `2026-01-01T00:${String(Math.floor(n / 60)).padStart(2, "0")}:${String(
        n % 60,
      ).padStart(2, "0")}.000Z`,
    );
  }

  function windowOf(from: number, to: number, hasMore = from > 0) {
    const entries: TimelineEntry[] = [];
    for (let n = from; n <= to; n += 1) entries.push(numbered(n));
    return {
      entries,
      cursor: hasMore ? `below-e${from}` : null,
      hasMore,
    };
  }

  function ids(state: ReturnType<typeof useSessionTimeline>): string[] {
    if (state.status !== "ok") throw new Error(`status ${state.status}`);
    return state.data.items.map((entry) => entry.id);
  }

  function range(from: number, to: number): string[] {
    const out: string[] = [];
    for (let n = from; n <= to; n += 1) out.push(`e${n}`);
    return out;
  }

  /**
   * A server whose history is e0..e<newest>, paged `size` rows newest-first;
   * `below-e<n>` reads the page ending just below e<n>.
   */
  function installHistory(options: {
    newest: () => number;
    size?: number;
    onRequest?: (cursor: string | null) => void | Promise<void>;
    fail?: (cursor: string | null) => boolean;
  }) {
    const size = options.size ?? 3;
    const requests: (string | null)[] = [];
    server.use(
      http.get("/api/v1/sessions/:id/timeline", async ({ request }) => {
        const cursor = new URL(request.url).searchParams.get("cursor");
        requests.push(cursor);
        await options.onRequest?.(cursor);
        if (options.fail?.(cursor)) {
          return new HttpResponse(null, { status: 500 });
        }
        const top =
          cursor == null
            ? options.newest()
            : Number(cursor.replace("below-e", "")) - 1;
        const from = Math.max(0, top - size + 1);
        return HttpResponse.json({ data: windowOf(from, top) });
      }),
    );
    return requests;
  }

  async function refresh(queryClient: QueryClient) {
    await act(async () => {
      await queryClient.invalidateQueries({
        queryKey: ["session-timeline", SESSION_ID],
      });
    });
  }

  it("loads the newest window, then prepends older pages in order", async () => {
    installHistory({ newest: () => 5 });
    const { wrapper } = createWrapper();
    const { result } = renderHook(() => useSessionTimeline(SESSION_ID), {
      wrapper,
    });

    await waitFor(() => expect(result.current.status).toBe("ok"));
    expect(ids(result.current)).toEqual(["e3", "e4", "e5"]);
    expect(result.current.hasOlder).toBe(true);
    expect(result.current.coverage).toEqual({
      status: "ready",
      floorCreatedAt: numbered(3).createdAt,
      hasOlder: true,
    });

    await act(async () => {
      await result.current.loadOlder();
    });
    expect(ids(result.current)).toEqual(range(0, 5));
    expect(result.current.hasOlder).toBe(false);
    expect(result.current.coverage.hasOlder).toBe(false);
    expect(result.current.isLoadingOlder).toBe(false);
  });

  it("reports unknown coverage until the first read lands", async () => {
    const gate: { release?: () => void } = {};
    installHistory({
      newest: () => 5,
      onRequest: () =>
        new Promise<void>((resolve) => {
          gate.release = resolve;
        }),
    });
    const { wrapper } = createWrapper();
    const { result } = renderHook(() => useSessionTimeline(SESSION_ID), {
      wrapper,
    });
    expect(result.current.coverage).toEqual({
      status: "loading",
      floorCreatedAt: null,
      hasOlder: false,
    });
    await waitFor(() => expect(gate.release).toBeDefined());
    gate.release?.();
    await waitFor(() => expect(result.current.coverage.status).toBe("ready"));
  });

  it("refreshes with one newest-window read and never re-reads held pages", async () => {
    let newest = 8;
    const requests = installHistory({ newest: () => newest });
    const { wrapper, queryClient } = createWrapper();
    const { result } = renderHook(() => useSessionTimeline(SESSION_ID), {
      wrapper,
    });
    await waitFor(() => expect(result.current.status).toBe("ok"));
    await act(async () => {
      await result.current.loadOlder();
    });
    await act(async () => {
      await result.current.loadOlder();
    });
    expect(ids(result.current)).toEqual(range(0, 8));
    requests.length = 0;

    // A poll while a turn streams: one new row slid the newest window.
    newest = 9;
    await refresh(queryClient);

    expect(requests).toEqual([null]);
    await waitFor(() => expect(ids(result.current)).toEqual(range(0, 9)));
    expect(result.current.hasOlder).toBe(false);
  });

  it("keeps the held floor cursor when a slid window overlaps the held rows", async () => {
    let newest = 5;
    const requests = installHistory({ newest: () => newest });
    const { wrapper, queryClient } = createWrapper();
    const { result } = renderHook(() => useSessionTimeline(SESSION_ID), {
      wrapper,
    });
    await waitFor(() => expect(result.current.status).toBe("ok"));
    newest = 6;
    await refresh(queryClient);
    await waitFor(() => expect(ids(result.current)).toEqual(range(3, 6)));
    if (result.current.status === "ok") {
      expect(result.current.data.cursor).toBe("below-e3");
    }

    requests.length = 0;
    await act(async () => {
      await result.current.loadOlder();
    });
    // The next older read continues below the held floor, not the slid one.
    expect(requests).toEqual(["below-e3"]);
    expect(ids(result.current)).toEqual(range(0, 6));
  });

  it("reads back across more than five pages instead of dropping held rows", async () => {
    let newest = 5;
    const requests = installHistory({ newest: () => newest });
    const { wrapper, queryClient } = createWrapper();
    const { result } = renderHook(() => useSessionTimeline(SESSION_ID), {
      wrapper,
    });
    await waitFor(() => expect(result.current.status).toBe("ok"));
    await act(async () => {
      await result.current.loadOlder();
    });
    expect(ids(result.current)).toEqual(range(0, 5));
    requests.length = 0;

    // Long turns ran while this view was not refreshing: 24 new rows, so the
    // refreshed window shares nothing with the held rows until the ninth page.
    newest = 29;
    await refresh(queryClient);

    await waitFor(() => expect(ids(result.current)).toEqual(range(0, 29)));
    expect(requests).toEqual([
      null,
      "below-e27",
      "below-e24",
      "below-e21",
      "below-e18",
      "below-e15",
      "below-e12",
      "below-e9",
      "below-e6",
    ]);
    // The held floor is kept: nothing remains below it.
    expect(result.current.hasOlder).toBe(false);
  });

  it("resumes an interrupted read-back instead of restarting it", async () => {
    let newest = 5;
    let interrupt: (() => void) | null = null;
    const requests = installHistory({
      newest: () => newest,
      onRequest: async (cursor) => {
        if (cursor === "below-e18" && interrupt) {
          const cancel = interrupt;
          interrupt = null;
          cancel();
          // Let the cancellation abort this request before it answers.
          await new Promise((resolve) => setTimeout(resolve, 20));
        }
      },
    });
    const { wrapper, queryClient } = createWrapper();
    const { result } = renderHook(() => useSessionTimeline(SESSION_ID), {
      wrapper,
    });
    await waitFor(() => expect(result.current.status).toBe("ok"));
    requests.length = 0;

    newest = 26;
    // The next poll cancels the read mid-way, as a 3 s poll does.
    interrupt = () =>
      void queryClient.invalidateQueries({
        queryKey: ["session-timeline", SESSION_ID],
      });
    await refresh(queryClient);
    await waitFor(() => expect(ids(result.current)).toEqual(range(3, 26)));

    // The second read met the saved progress and continued from it; pages
    // already read back were not requested again.
    expect(requests.filter((cursor) => cursor === "below-e24")).toHaveLength(1);
    expect(requests.filter((cursor) => cursor === "below-e21")).toHaveLength(1);
    expect(requests.filter((cursor) => cursor === null)).toHaveLength(2);
  });

  describe("held requests that are still open", () => {
    // History e0..e8 paged three rows at a time; e1 opens a native permission
    // request of task-1 attempt 1, whose status the server joins on read.
    function requestHistory(extra: (n: number) => TimelineEntry | null) {
      let status = "pending";
      let newest = 5;
      const rowAt = (n: number): TimelineEntry => {
        if (n === 1)
          return {
            ...numbered(1),
            entryType: "checkpoint",
            metadata: {
              schema_version: "g5.task-event.v1",
              kind: "provider_request.opened",
              taskId: "task-1",
              attempt: 1,
              sessionId: SESSION_ID,
              payload: {
                provider_request_id: "request-1",
                request_kind: "permission",
                display: { input: { command: "npm test" } },
              },
              nativeRequestStatus: status,
            },
          };
        return extra(n) ?? numbered(n);
      };
      const requests: (string | null)[] = [];
      server.use(
        http.get("/api/v1/sessions/:id/timeline", ({ request }) => {
          const cursor = new URL(request.url).searchParams.get("cursor");
          requests.push(cursor);
          const top =
            cursor == null ? newest : Number(cursor.replace("below-e", "")) - 1;
          const from = Math.max(0, top - 2);
          const entries: TimelineEntry[] = [];
          for (let n = from; n <= top; n += 1) entries.push(rowAt(n));
          return HttpResponse.json({
            data: {
              entries,
              cursor: from > 0 ? `below-e${from}` : null,
              hasMore: from > 0,
            },
          });
        }),
      );
      return {
        requests,
        setStatus: (next: string) => {
          status = next;
        },
        setNewest: (next: number) => {
          newest = next;
        },
      };
    }

    function requestStatus(state: ReturnType<typeof useSessionTimeline>) {
      if (state.status !== "ok") throw new Error(state.status);
      return state.data.items.find((entry) => entry.id === "e1")?.metadata
        ?.nativeRequestStatus;
    }

    async function loadAll(result: {
      current: ReturnType<typeof useSessionTimeline>;
    }) {
      await waitFor(() => expect(result.current.status).toBe("ok"));
      await act(async () => {
        await result.current.loadOlder();
      });
      await waitFor(() =>
        expect(requestStatus(result.current)).toBe("pending"),
      );
    }

    it("reads back to an open request below the window and refreshes its status", async () => {
      const server = requestHistory(() => null);
      const { wrapper, queryClient } = createWrapper();
      const { result } = renderHook(() => useSessionTimeline(SESSION_ID), {
        wrapper,
      });
      await loadAll(result);

      // The owner replied; nothing new was journaled yet.
      server.setStatus("reply_pending");
      server.setNewest(6);
      server.requests.length = 0;
      await refresh(queryClient);

      await waitFor(() =>
        expect(requestStatus(result.current)).toBe("reply_pending"),
      );
      expect(server.requests).toEqual([null, "below-e4"]);
      expect(ids(result.current)).toEqual(range(0, 6));
      expect(result.current.hasOlder).toBe(false);
    });

    it("stops reading back once a reply row settles the request", async () => {
      const server = requestHistory((n) =>
        n === 5
          ? {
              ...numbered(5),
              entryType: "checkpoint",
              metadata: {
                schema_version: "g5.task-event.v1",
                kind: "provider_request.replied",
                taskId: "task-1",
                attempt: 1,
                payload: {
                  provider_request_id: "request-1",
                  disposition: "allowed",
                },
              },
            }
          : null,
      );
      const { wrapper, queryClient } = createWrapper();
      const { result } = renderHook(() => useSessionTimeline(SESSION_ID), {
        wrapper,
      });
      await loadAll(result);
      server.setNewest(6);
      server.requests.length = 0;
      await refresh(queryClient);
      await waitFor(() => expect(ids(result.current)).toEqual(range(0, 6)));
      expect(server.requests).toEqual([null]);
    });

    it("stops reading back once the request's attempt has an outcome", async () => {
      const server = requestHistory((n) =>
        n === 4
          ? {
              ...numbered(4),
              entryType: "checkpoint",
              metadata: {
                schema_version: "g5.task-event.v1",
                kind: "invocation.outcome",
                taskId: "task-1",
                attempt: 1,
                payload: {},
              },
            }
          : null,
      );
      const { wrapper, queryClient } = createWrapper();
      const { result } = renderHook(() => useSessionTimeline(SESSION_ID), {
        wrapper,
      });
      await loadAll(result);
      server.setNewest(6);
      server.requests.length = 0;
      await refresh(queryClient);
      await waitFor(() => expect(ids(result.current)).toEqual(range(0, 6)));
      expect(server.requests).toEqual([null]);
    });
  });

  it("drops held rows only when history ends without them", async () => {
    let history: "old" | "new" = "old";
    server.use(
      http.get("/api/v1/sessions/:id/timeline", ({ request }) => {
        const cursor = new URL(request.url).searchParams.get("cursor");
        if (history === "old") {
          return HttpResponse.json({ data: windowOf(3, 5, false) });
        }
        // The held rows no longer exist server-side.
        if (cursor == null) {
          return HttpResponse.json({
            data: {
              entries: [numbered(10), numbered(11)],
              cursor: "below-e10",
              hasMore: true,
            },
          });
        }
        return HttpResponse.json({
          data: {
            entries: [numbered(8), numbered(9)],
            cursor: null,
            hasMore: false,
          },
        });
      }),
    );
    const { wrapper, queryClient } = createWrapper();
    const { result } = renderHook(() => useSessionTimeline(SESSION_ID), {
      wrapper,
    });
    await waitFor(() => expect(result.current.status).toBe("ok"));
    history = "new";
    await refresh(queryClient);
    await waitFor(() =>
      expect(ids(result.current)).toEqual(["e8", "e9", "e10", "e11"]),
    );
    expect(result.current.hasOlder).toBe(false);
  });

  it("keeps the loaded rows and offers a retry when an older read fails", async () => {
    let failing = true;
    installHistory({
      newest: () => 5,
      fail: (cursor) => cursor != null && failing,
    });
    const { wrapper } = createWrapper();
    const { result } = renderHook(() => useSessionTimeline(SESSION_ID), {
      wrapper,
    });
    await waitFor(() => expect(result.current.status).toBe("ok"));

    await act(async () => {
      await result.current.loadOlder();
    });
    expect(result.current.appendError).not.toBeNull();
    expect(result.current.status).toBe("ok");
    expect(ids(result.current)).toEqual(["e3", "e4", "e5"]);
    expect(result.current.hasOlder).toBe(true);

    failing = false;
    await act(async () => {
      await result.current.loadOlder();
    });
    expect(result.current.appendError).toBeNull();
    expect(ids(result.current)).toEqual(range(0, 5));
  });

  it("keeps a prepend that races a refresh and reads the refresh again", async () => {
    let newest = 5;
    let releaseRefresh: () => void = () => undefined;
    let holdRefresh = false;
    const requests = installHistory({
      newest: () => newest,
      onRequest: (cursor) =>
        cursor == null && holdRefresh
          ? new Promise<void>((resolve) => {
              holdRefresh = false;
              releaseRefresh = resolve;
            })
          : undefined,
    });
    const { wrapper, queryClient } = createWrapper();
    const { result } = renderHook(() => useSessionTimeline(SESSION_ID), {
      wrapper,
    });
    await waitFor(() => expect(result.current.status).toBe("ok"));
    requests.length = 0;

    // A refresh is in flight (it read the held rows e3..e5) when the older
    // page lands.
    newest = 6;
    holdRefresh = true;
    let pendingRefresh: Promise<void> = Promise.resolve();
    act(() => {
      pendingRefresh = queryClient.invalidateQueries({
        queryKey: ["session-timeline", SESSION_ID],
      });
    });
    await waitFor(() => expect(requests).toEqual([null]));
    await act(async () => {
      await result.current.loadOlder();
    });
    releaseRefresh();
    await act(async () => {
      await pendingRefresh.catch(() => undefined);
    });

    await waitFor(() => expect(ids(result.current)).toEqual(range(0, 6)));
    // The cancelled refresh was read again rather than lost.
    expect(requests.filter((cursor) => cursor === null).length).toBe(2);
  });

  it("loadOlder is single-flight", async () => {
    const requests = installHistory({ newest: () => 5 });
    const { wrapper } = createWrapper();
    const { result } = renderHook(() => useSessionTimeline(SESSION_ID), {
      wrapper,
    });
    await waitFor(() => expect(result.current.status).toBe("ok"));
    requests.length = 0;

    await act(async () => {
      const first = result.current.loadOlder();
      const second = result.current.loadOlder();
      await Promise.all([first, second]);
    });
    expect(requests).toEqual(["below-e3"]);
    expect(ids(result.current)).toEqual(range(0, 5));
  });

  it("loadOlder is not blocked by a previous session's pending read", async () => {
    let releaseOlderA: () => void = () => undefined;
    const olderRequestsBySession: Record<string, number> = {};
    server.use(
      http.get("/api/v1/sessions/:id/timeline", async ({ request, params }) => {
        const sid = String(params.id);
        const cursor = new URL(request.url).searchParams.get("cursor");
        if (cursor == null) {
          return HttpResponse.json({
            data: {
              entries: [
                timelineEntry(`${sid}-newest`, "2026-01-01T00:05:00.000Z"),
              ],
              cursor: `cur-${sid}`,
              hasMore: true,
            },
          });
        }
        olderRequestsBySession[sid] = (olderRequestsBySession[sid] ?? 0) + 1;
        if (sid === "sess-tl-a") {
          await new Promise<void>((resolve) => {
            releaseOlderA = resolve;
          });
        }
        return HttpResponse.json({
          data: {
            entries: [
              timelineEntry(`${sid}-older`, "2026-01-01T00:01:00.000Z"),
            ],
            cursor: null,
            hasMore: false,
          },
        });
      }),
    );
    const { wrapper } = createWrapper();
    const { result, rerender } = renderHook(
      ({ sid }: { sid: string }) => useSessionTimeline(sid),
      { wrapper, initialProps: { sid: "sess-tl-a" } },
    );

    await waitFor(() => expect(result.current.status).toBe("ok"));
    let pendingA: Promise<void> | null = null;
    act(() => {
      pendingA = result.current.loadOlder();
    });
    await waitFor(() => expect(olderRequestsBySession["sess-tl-a"]).toBe(1));

    rerender({ sid: "sess-tl-b" });
    await waitFor(() => expect(result.current.status).toBe("ok"));
    expect(result.current.isLoadingOlder).toBe(false);

    await act(async () => {
      await result.current.loadOlder();
    });
    expect(olderRequestsBySession["sess-tl-b"]).toBe(1);
    expect(ids(result.current)).toEqual([
      "sess-tl-b-older",
      "sess-tl-b-newest",
    ]);

    releaseOlderA();
    await act(async () => {
      await pendingA;
    });
    // A's late completion leaves B's state alone.
    expect(result.current.isLoadingOlder).toBe(false);
    expect(result.current.appendError).toBeNull();
  });
});

describe("useSessionTimeline live rows (chat g5.chat.replay)", () => {
  const SESSION_ID = "s-live-rows";

  function journalRow(seq: number, live: boolean): TimelineEntry {
    return {
      id: `task-x:1:${seq}`,
      entryType: "output",
      content: `delta ${seq}`,
      createdAt: `2026-10-30T03:00:${String(seq).padStart(2, "0")}.000Z`,
      metadata: {
        schema_version: "g5.task-event.v1",
        kind: "message.delta",
        payload: { block_id: "b", text_delta: `delta ${seq}` },
        taskId: "task-x",
        attempt: 1,
        seq,
        sessionId: SESSION_ID,
        messageId: "m1",
        ...(live ? { live: true } : {}),
      },
    };
  }

  function serve(saved: () => TimelineEntry[]) {
    server.use(
      http.get("/api/v1/sessions/:id/timeline", () =>
        HttpResponse.json({
          data: { entries: saved(), cursor: null, hasMore: false },
        }),
      ),
    );
  }

  it("a refresh keeps live rows beyond the saved prefix and replaces the ones it covers", async () => {
    let saved = [journalRow(1, false), journalRow(2, false)];
    serve(() => saved);
    const { wrapper, queryClient } = createWrapper();
    const { result } = renderHook(() => useSessionTimeline(SESSION_ID), {
      wrapper,
    });
    await waitFor(() => expect(result.current.status).toBe("ok"));
    // Live rows 3 and 4 arrive ahead of saved history.
    act(() => {
      queryClient.setQueryData(
        ["session-timeline", SESSION_ID],
        (old: Page<TimelineEntry> | undefined) =>
          old && {
            ...old,
            items: [...old.items, journalRow(3, true), journalRow(4, true)],
          },
      );
    });
    // Persistence catches up with row 3 only.
    saved = [journalRow(1, false), journalRow(2, false), journalRow(3, false)];
    await act(async () => {
      await queryClient.invalidateQueries({
        queryKey: ["session-timeline", SESSION_ID],
      });
    });
    await waitFor(() => {
      if (result.current.status !== "ok") throw new Error("not ready");
      expect(
        result.current.data.items.map((item) => [
          item.id,
          item.metadata.live ?? false,
        ]),
      ).toEqual([
        ["task-x:1:1", false],
        ["task-x:1:2", false],
        ["task-x:1:3", false],
        ["task-x:1:4", true],
      ]);
    });
    // Two reads of saved history completed; the placement in between was a
    // cache write and did not count.
    expect(result.current.historyReads).toBe(2);
  });

  it("historyReads counts completed server reads only, never cache writes (g5.chat.live-tail-recovery)", async () => {
    let saved = [journalRow(1, false)];
    serve(() => saved);
    const { wrapper, queryClient } = createWrapper();
    const { result } = renderHook(() => useSessionTimeline(SESSION_ID), {
      wrapper,
    });
    expect(result.current.historyReads).toBe(0);
    await waitFor(() => expect(result.current.status).toBe("ok"));
    expect(result.current.historyReads).toBe(1);

    // Live rows placed into the window, as the tail read and the WebSocket
    // path do, are updates to the query but not reads of saved history.
    for (const seq of [2, 3, 4]) {
      act(() => {
        queryClient.setQueryData(
          ["session-timeline", SESSION_ID],
          (old: Page<TimelineEntry> | undefined) =>
            old && { ...old, items: [...old.items, journalRow(seq, true)] },
        );
      });
    }
    // An unchanged write bumps the query's own `dataUpdatedAt` as well.
    act(() => {
      queryClient.setQueryData(
        ["session-timeline", SESSION_ID],
        (old: Page<TimelineEntry> | undefined) => old,
      );
    });
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(result.current.historyReads).toBe(1);
    expect(
      result.current.status === "ok" && result.current.data.items,
    ).toHaveLength(4);

    saved = [journalRow(1, false), journalRow(2, false)];
    await act(async () => {
      await queryClient.invalidateQueries({
        queryKey: ["session-timeline", SESSION_ID],
      });
    });
    await waitFor(() => expect(result.current.historyReads).toBe(2));
    // Loading older pages writes the cache without reading the newest window.
    expect(result.current.historyReads).toBe(2);
  });

  /**
   * Saved rows 1..`persisted` paged backward by cursor, like the API;
   * `rowAt` replaces the row of a sequence (a request among the deltas).
   */
  function servePaged(
    persisted: number,
    pageSize: number,
    rowAt?: (seq: number) => TimelineEntry | undefined,
  ) {
    const reads: string[] = [];
    const saved = Array.from(
      { length: persisted },
      (_, index) => rowAt?.(index + 1) ?? journalRow(index + 1, false),
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

  function seqs(state: ReturnType<typeof useSessionTimeline>): number[] {
    if (state.status !== "ok") throw new Error(`status ${state.status}`);
    return state.data.items.map((entry) => entry.metadata.seq as number);
  }

  it("a refresh reads back until it meets a held saved row, not a held live row (g5.chat.live-tail-recovery)", async () => {
    const reads = servePaged(10, 3);
    const { wrapper, queryClient } = createWrapper();
    // Held: saved 1..3 and live 8..9; rows 4..7 were dropped for this
    // connection and persisted meanwhile.
    queryClient.setQueryData<Page<TimelineEntry>>(
      ["session-timeline", SESSION_ID],
      {
        items: [
          journalRow(1, false),
          journalRow(2, false),
          journalRow(3, false),
          journalRow(8, true),
          journalRow(9, true),
        ],
        cursor: null,
        hasMore: false,
      },
    );
    const { result } = renderHook(() => useSessionTimeline(SESSION_ID), {
      wrapper,
    });
    await waitFor(() => expect(result.current.status).toBe("ok"));
    await act(async () => {
      await queryClient.invalidateQueries({
        queryKey: ["session-timeline", SESSION_ID],
      });
    });
    await waitFor(() => expect(result.current.historyReads).toBe(1));
    // The newest page (8..10) carries the ids of the held live rows; that
    // proves nothing about the saved prefix, so the read went on to 5..7 and
    // then 2..4, which met the held saved rows.
    expect(reads).toEqual([
      "",
      `${journalRow(8, false).createdAt}|task-x:1:8`,
      `${journalRow(5, false).createdAt}|task-x:1:5`,
    ]);
    expect(seqs(result.current)).toEqual([1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
    expect(
      result.current.status === "ok" &&
        result.current.data.items.filter((row) => row.metadata.live === true),
    ).toEqual([]);
  });

  it("a completed read that still leaves a covered hole has it filled by sequence, as a cache write", async () => {
    const reads = servePaged(10, 3);
    const { wrapper, queryClient } = createWrapper();
    // Held: saved 1..3, live 5..6, and the stored row of a boundary at 8
    // (published after its batch committed); 4 and 7 never arrived.
    queryClient.setQueryData<Page<TimelineEntry>>(
      ["session-timeline", SESSION_ID],
      {
        items: [
          journalRow(1, false),
          journalRow(2, false),
          journalRow(3, false),
          journalRow(5, true),
          journalRow(6, true),
          journalRow(8, false),
        ],
        cursor: null,
        hasMore: false,
      },
    );
    const { result } = renderHook(() => useSessionTimeline(SESSION_ID), {
      wrapper,
    });
    await waitFor(() => expect(result.current.status).toBe("ok"));
    await act(async () => {
      await queryClient.invalidateQueries({
        queryKey: ["session-timeline", SESSION_ID],
      });
    });
    await waitFor(() => expect(result.current.historyReads).toBe(1));
    // The newest page (8..10) met the held saved row 8, so the refresh
    // stopped there; live rows 5 and 6 stay (their saved copies were not
    // read), leaving 4 and 7 missing. The fill read each from the row after
    // it, oldest first. Its placements are cache writes: no further read.
    await waitFor(() =>
      expect(seqs(result.current)).toEqual([1, 2, 3, 4, 5, 6, 7, 8, 9, 10]),
    );
    await new Promise((resolve) => setTimeout(resolve, 100));
    expect(reads).toEqual([
      "",
      `${journalRow(5, false).createdAt}|task-x:1:5`,
      `${journalRow(8, false).createdAt}|task-x:1:8`,
    ]);
    expect(result.current.historyReads).toBe(1);
    // The live rows the fill did not read stand in for their saved copies.
    expect(
      result.current.status === "ok" &&
        result.current.data.items
          .filter((item) => item.metadata.live === true)
          .map((item) => item.metadata.seq),
    ).toEqual([5, 6]);
  });

  /**
   * A provider question at `seq`: the live copy carries no request status
   * (that is joined onto the saved row when it is read), the saved copy is
   * `pending`.
   */
  function questionRow(seq: number, live: boolean): TimelineEntry {
    const base = journalRow(seq, live);
    return {
      ...base,
      entryType: "checkpoint",
      content: "Choose",
      metadata: {
        ...base.metadata,
        kind: "provider_request.opened",
        payload: {
          provider_request_id: "request-q",
          request_kind: "question",
          display: {
            questions: [{ question: "Choose", options: [{ label: "A" }] }],
          },
        },
        ...(live ? {} : { nativeRequestStatus: "pending" }),
      },
    };
  }

  it("a refresh reads back to a live provider question until its saved copy arrives (g5.chat.history-refresh)", async () => {
    const reads = servePaged(8, 3, (seq) =>
      seq === 2 ? questionRow(2, false) : undefined,
    );
    const { wrapper, queryClient } = createWrapper();
    // Held: saved 1; the question live at 2, its stored republish missed;
    // live output 3..7; the stored row of a boundary at 8. Everything is
    // saved server-side. Held rows are projected as the placement path
    // projects them.
    queryClient.setQueryData<Page<TimelineEntry>>(
      ["session-timeline", SESSION_ID],
      {
        items: projectTaskTimelineEntries([
          journalRow(1, false),
          questionRow(2, true),
          journalRow(3, true),
          journalRow(4, true),
          journalRow(5, true),
          journalRow(6, true),
          journalRow(7, true),
          journalRow(8, false),
        ]),
        cursor: null,
        hasMore: false,
      },
    );
    const { result } = renderHook(() => useSessionTimeline(SESSION_ID), {
      wrapper,
    });
    await waitFor(() => expect(result.current.status).toBe("ok"));
    await act(async () => {
      await queryClient.invalidateQueries({
        queryKey: ["session-timeline", SESSION_ID],
      });
    });
    await waitFor(() => expect(result.current.historyReads).toBe(1));
    // The newest page (6..8) met the held saved row 8, but the question
    // below it is held live, without a status, so its request is open: the
    // read went on to 3..5 and then 1..2, where its saved copy was.
    expect(reads).toEqual([
      "",
      `${journalRow(6, false).createdAt}|task-x:1:6`,
      `${journalRow(3, false).createdAt}|task-x:1:3`,
    ]);
    expect(seqs(result.current)).toEqual([1, 2, 3, 4, 5, 6, 7, 8]);
    const question =
      result.current.status === "ok"
        ? result.current.data.items.find((row) => row.metadata.seq === 2)
        : undefined;
    expect(question?.metadata.live).toBeUndefined();
    expect(question?.metadata.nativeRequestStatus).toBe("pending");
    expect(
      result.current.status === "ok" &&
        result.current.data.items.filter((row) => row.metadata.live === true),
    ).toEqual([]);
    // No further read: the request's status arrived with its row.
    await new Promise((resolve) => setTimeout(resolve, 100));
    expect(reads).toHaveLength(3);
    expect(result.current.historyReads).toBe(1);
  });

  it("a refresh that returns no overlap still keeps the running turn's live rows", async () => {
    let saved: TimelineEntry[] = [];
    serve(() => saved);
    const { wrapper, queryClient } = createWrapper();
    const { result } = renderHook(() => useSessionTimeline(SESSION_ID), {
      wrapper,
    });
    await waitFor(() => expect(result.current.status).toBe("ok"));
    act(() => {
      queryClient.setQueryData(["session-timeline", SESSION_ID], {
        items: [journalRow(5, true)],
        cursor: null,
        hasMore: false,
      });
    });
    saved = [journalRow(1, false)];
    await act(async () => {
      await queryClient.invalidateQueries({
        queryKey: ["session-timeline", SESSION_ID],
      });
    });
    await waitFor(() => {
      if (result.current.status !== "ok") throw new Error("not ready");
      expect(result.current.data.items.map((item) => item.id)).toEqual([
        "task-x:1:1",
        "task-x:1:5",
      ]);
    });
  });
});
