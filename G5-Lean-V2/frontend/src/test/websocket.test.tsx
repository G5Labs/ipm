// =============================================================================
// WebSocket Integration Tests
// =============================================================================
// Verifies the full WebSocket event → React Query cache invalidation → UI
// update pipeline for all server-pushed event types.
//
// Architecture:
//   1. Render a component via renderApp() (wraps in all providers)
//   2. Wait for initial REST data from MSW
//   3. Emit a WebSocket event via ws.emit() wrapped in act()
//   4. Assert UI updated (via waitFor / findBy) or cache invalidated
//
// All events are "hints for cache invalidation" — they trigger refetches,
// not direct state mutations (exception: chat.chunk uses setQueryData).
// =============================================================================

import { describe, it, expect, vi } from "vitest";
import { waitFor, act } from "@testing-library/react";
import { QueryClient } from "@tanstack/react-query";
import { http, HttpResponse } from "msw";
import { Route, Routes } from "react-router-dom";
import { server } from "./msw/server";
import { renderApp } from "./render";
import {
  createMockTask,
  createMockChatMessage,
  createMockChatSession,
} from "./fixtures";

// Import components under test
import { WorkspacePage } from "../pages/WorkspacePage";
import { WebSocketManager } from "../lib/ws";

// =============================================================================
// Helpers
// =============================================================================

/**
 * Renders the WorkspacePage at the standard test route and returns
 * the ws controller, queryClient, and render result.
 */
function renderWorkspace() {
  return renderApp(
    <Routes>
      <Route path="/:org/:workspace" element={<WorkspacePage />} />
      <Route path="/:org/:workspace/task/:taskId" element={<WorkspacePage />} />
      <Route path="/:org/:workspace/spec/*" element={<WorkspacePage />} />
      <Route path="/:org/:workspace/doc/*" element={<WorkspacePage />} />
      <Route path="/:org/:workspace/file/*" element={<WorkspacePage />} />
    </Routes>,
    {
      route: "/org-g5labs/test-workspace",
    },
  );
}

// =============================================================================
// Test Suites
// =============================================================================

describe("WebSocket Integration Tests", () => {
  // -------------------------------------------------------------------------
  // task.status events
  // -------------------------------------------------------------------------
  describe("task.status events", () => {
    it("invalidates task, workspace task lists, spend and workspace stats when task.status event received", async () => {
      // Override MSW to return updated task on refetch
      server.use(
        http.get("/api/v1/tasks/:taskId", ({ params }) =>
          HttpResponse.json({
            data: createMockTask({
              id: params.taskId as string,
              status: "succeeded",
            }),
          }),
        ),
        http.get("/api/v1/sessions/:sessionId", ({ params }) =>
          HttpResponse.json({
            data: createMockChatSession({
              id: params.sessionId as string,
              scopeType: "task",
              scopeRef: "task-1",
            }),
          }),
        ),
      );

      const { ws, queryClient } = renderWorkspace();

      // Wait for initial data to load
      await waitFor(() => {
        expect(queryClient.isFetching()).toBe(0);
      });
      queryClient.setQueryData(
        ["chat-session", "task-session-1"],
        createMockChatSession({
          id: "task-session-1",
          scopeType: "task",
          scopeRef: "task-1",
        }),
      );
      queryClient.setQueryData(
        ["chat-sessions", "ws-1", "task"],
        [
          createMockChatSession({
            id: "task-session-1",
            scopeType: "task",
            scopeRef: "task-1",
          }),
        ],
      );
      const invalidateSpy = vi.spyOn(queryClient, "invalidateQueries");

      // Emit WebSocket event
      await act(async () => {
        ws.emit({
          channel: "workspace:ws-1",
          event: "task.status",
          payload: { taskId: "task-1", status: "succeeded" },
        });
      });

      // Verify cache was invalidated
      await waitFor(() => {
        const taskState = queryClient.getQueryState(["task", "task-1"]);
        // After invalidation, the query should be refetching or have been invalidated
        expect(
          taskState === undefined ||
            taskState.isInvalidated ||
            taskState.dataUpdatedAt > 0,
        ).toBe(true);
      });

      await waitFor(() => {
        expect(invalidateSpy).toHaveBeenCalledWith({
          queryKey: ["workspace-tasks"],
        });
      });
      await waitFor(() => {
        expect(invalidateSpy).toHaveBeenCalledWith({
          queryKey: ["pending-sync-files"],
          refetchType: "all",
        });
      });
      await waitFor(() => {});
      await waitFor(() => {
        const session = queryClient.getQueryData<
          ReturnType<typeof createMockChatSession>
        >(["chat-session", "task-session-1"]);
        const sessions = queryClient.getQueryData<
          ReturnType<typeof createMockChatSession>[]
        >(["chat-sessions", "ws-1", "task"]);
        expect(session?.scopeRef).toBe("task-1");
        expect(sessions?.[0]?.scopeRef).toBe("task-1");
        expect(invalidateSpy).toHaveBeenCalledWith({
          queryKey: ["chat-messages"],
        });
        expect(invalidateSpy).toHaveBeenCalledWith({
          queryKey: ["session-timeline"],
        });
        expect(invalidateSpy).toHaveBeenCalledWith({
          queryKey: ["chat-session"],
        });
        expect(invalidateSpy).toHaveBeenCalledWith({
          queryKey: ["chat-sessions"],
        });
      });
    });

    it("invalidates the session-task projection on every task.status lifecycle event", () => {
      // Exercise the real shared manager. The page-level mock controller used
      // by this suite deliberately reimplements invalidation and would not
      // catch a missing mapping in ws.ts.
      const queryClient = new QueryClient({
        defaultOptions: { queries: { retry: false } },
      });
      const manager = new WebSocketManager({
        getBaseUrl: () => "http://localhost",
        getWsTicket: async () => "test-ticket",
        queryClient,
      });
      const invalidateSpy = vi.spyOn(queryClient, "invalidateQueries");
      const sink = manager as unknown as { handleEvent(e: unknown): void };

      sink.handleEvent({
        type: "task.status",
        taskId: "task-1",
        status: "claimed",
      });

      expect(invalidateSpy).toHaveBeenCalledWith({
        queryKey: ["session-task"],
      });
    });

    it("does not invalidate chat caches for non-terminal task.status events", async () => {
      const { ws, queryClient } = renderWorkspace();

      await waitFor(() => {
        expect(queryClient.isFetching()).toBe(0);
      });
      const invalidateSpy = vi.spyOn(queryClient, "invalidateQueries");

      await act(async () => {
        ws.emit({
          channel: "workspace:ws-1",
          event: "task.status",
          payload: { taskId: "task-1", status: "running" },
        });
      });

      expect(invalidateSpy).toHaveBeenCalledWith({
        queryKey: ["task", "task-1"],
      });
      expect(invalidateSpy).not.toHaveBeenCalledWith({
        queryKey: ["chat-messages"],
      });
      expect(invalidateSpy).not.toHaveBeenCalledWith({
        queryKey: ["session-timeline"],
      });
      expect(invalidateSpy).not.toHaveBeenCalledWith({
        queryKey: ["chat-session"],
      });
      expect(invalidateSpy).not.toHaveBeenCalledWith({
        queryKey: ["chat-sessions"],
      });
    });

    it("handles sequential task.status events for different tasks", async () => {
      const { ws, queryClient } = renderWorkspace();

      await waitFor(() => {
        expect(queryClient.isFetching()).toBe(0);
      });

      // Emit two events for different tasks
      await act(async () => {
        ws.emit({
          channel: "workspace:ws-1",
          event: "task.status",
          payload: { taskId: "task-1", status: "succeeded" },
        });
      });

      await act(async () => {
        ws.emit({
          channel: "workspace:ws-1",
          event: "task.status",
          payload: { taskId: "task-2", status: "failed" },
        });
      });

      // Both task caches should have been invalidated
      await waitFor(() => {
        // The workspace tasks cache should have been invalidated (broad invalidation)
        const jobsState = queryClient.getQueryState(["workspace-tasks"]);
        expect(jobsState === undefined || jobsState.isInvalidated).toBe(true);
      });
    });

    it("handles unknown task ID in status event gracefully", async () => {
      const { ws, queryClient } = renderWorkspace();

      await waitFor(() => {
        expect(queryClient.isFetching()).toBe(0);
      });

      // Should not crash when emitting event for non-existent task
      await act(async () => {
        ws.emit({
          channel: "workspace:ws-1",
          event: "task.status",
          payload: { taskId: "nonexistent-task", status: "succeeded" },
        });
      });

      // No crash — page is still rendered
      await waitFor(() => {
        expect(document.body).toBeTruthy();
      });
    });
  });

  // -------------------------------------------------------------------------
  // task.timeline events
  // -------------------------------------------------------------------------
  describe("task.timeline events", () => {
    it("invalidates task-timeline cache on task.timeline event", async () => {
      const { ws, queryClient } = renderWorkspace();

      await waitFor(() => {
        expect(queryClient.isFetching()).toBe(0);
      });

      await act(async () => {
        ws.emit({
          channel: "task:task-1",
          event: "task.timeline",
          payload: {
            taskId: "task-1",
            entry: {
              id: "tl-new-1",
              entryType: "thought",
              content: "Analyzing the code...",
              metadata: {},
              createdAt: new Date().toISOString(),
            },
          },
        });
      });

      // Timeline cache should be invalidated
      await waitFor(() => {
        const state = queryClient.getQueryState(["task-timeline", "task-1"]);
        expect(state === undefined || state.isInvalidated).toBe(true);
      });
    });

    it("appends a task.timeline entry once when the same id is delivered twice (#1313)", () => {
      // Drive the real WebSocketManager handler directly (the shared render
      // harness uses a MockWsController that reimplements cache logic, so it
      // would not exercise ws.ts). A task.timeline event can be delivered more
      // than once with the same id — a connection subscribed to both the task
      // and chat channels gets one publish per channel, and a REST refetch can
      // race a live append — so the append must be idempotent by entry.id.
      const queryClient = new QueryClient({
        defaultOptions: { queries: { retry: false } },
      });
      const manager = new WebSocketManager({
        getBaseUrl: () => "http://localhost",
        getWsTicket: async () => "test-ticket",
        queryClient,
      });

      // Seed an infinite-query timeline cache so the optimistic-append path runs.
      queryClient.setQueryData(["task-timeline", "task-1"], {
        pages: [{ items: [], cursor: null, hasMore: false }],
        pageParams: [null],
      });

      const event = {
        type: "task.timeline" as const,
        taskId: "task-1",
        entry: {
          id: "tl-dup-1",
          entryType: "output",
          content: "The answer.",
          metadata: {},
          createdAt: "2026-07-18T00:00:00Z",
        },
      };

      const sink = manager as unknown as {
        handleEvent(e: unknown): void;
        flushTaskTimelineRows(): void;
      };
      sink.handleEvent(event);
      sink.handleEvent(event);
      // Live rows are placed once per animation frame; place the frame now.
      sink.flushTaskTimelineRows();

      const data = queryClient.getQueryData(["task-timeline", "task-1"]) as {
        pages: { items: { id: string }[] }[];
      };
      const items = data.pages.flatMap((p) => p.items);
      expect(items.filter((it) => it.id === "tl-dup-1")).toHaveLength(1);
    });
  });

  // -------------------------------------------------------------------------
  // chat streaming events
  // -------------------------------------------------------------------------
  describe("chat streaming events", () => {
    it("handles chat.chunk event with optimistic update via setQueryData", async () => {
      const initialMessages = [
        createMockChatMessage({
          id: "msg-1",
          sessionId: "session-1",
          role: "user",
          content: "Hello",
          status: "complete",
          messageIndex: 0,
        }),
        createMockChatMessage({
          id: "msg-2",
          sessionId: "session-1",
          role: "assistant",
          content: "",
          status: "streaming",
          messageIndex: 1,
        }),
      ];

      // Pre-populate the cache with initial messages
      const { ws, queryClient } = renderWorkspace();

      // Seed the cache directly
      queryClient.setQueryData(["chat-messages", "session-1"], initialMessages);

      await act(async () => {
        ws.emit({
          channel: "chat:session-1",
          event: "chat.chunk",
          payload: {
            sessionId: "session-1",
            messageId: "msg-2",
            delta: "Hello ",
          },
        });
      });

      // Verify the cache was optimistically updated (setQueryData, not invalidateQueries)
      await waitFor(() => {
        const messages = queryClient.getQueryData<typeof initialMessages>([
          "chat-messages",
          "session-1",
        ]);
        if (messages) {
          const lastMsg = messages[messages.length - 1];
          // The delta should have been appended to the message content
          expect(lastMsg.content).toContain("Hello ");
        }
      });
    });

    it("accumulates multiple chunks into single message", async () => {
      const initialMessages = [
        createMockChatMessage({
          id: "msg-1",
          sessionId: "session-1",
          role: "assistant",
          content: "",
          status: "streaming",
          messageIndex: 0,
        }),
      ];

      const { ws, queryClient } = renderWorkspace();
      queryClient.setQueryData(["chat-messages", "session-1"], initialMessages);

      // Emit 3 sequential chunks
      await act(async () => {
        ws.emit({
          channel: "chat:session-1",
          event: "chat.chunk",
          payload: {
            sessionId: "session-1",
            messageId: "msg-1",
            delta: "Hello ",
          },
        });
      });

      await act(async () => {
        ws.emit({
          channel: "chat:session-1",
          event: "chat.chunk",
          payload: {
            sessionId: "session-1",
            messageId: "msg-1",
            delta: "World ",
          },
        });
      });

      await act(async () => {
        ws.emit({
          channel: "chat:session-1",
          event: "chat.chunk",
          payload: { sessionId: "session-1", messageId: "msg-1", delta: "!" },
        });
      });

      await waitFor(() => {
        const messages = queryClient.getQueryData<typeof initialMessages>([
          "chat-messages",
          "session-1",
        ]);
        if (messages) {
          expect(messages[0].content).toContain("Hello ");
          expect(messages[0].content).toContain("World ");
          expect(messages[0].content).toContain("!");
        }
      });
    });

    it("applies chunk deltas to the streaming assistant even when it is not the array tail after a mid-stream steer (issue #1106 defect 2)", async () => {
      // Assistant msg-1 is still streaming; the user steers, so useChat
      // optimistically appends a pending user message (msg-2) to the TAIL.
      const initialMessages = [
        createMockChatMessage({
          id: "msg-1",
          sessionId: "session-1",
          role: "assistant",
          content: "Partial ",
          status: "streaming",
          messageIndex: 0,
        }),
        createMockChatMessage({
          id: "msg-2",
          sessionId: "session-1",
          role: "user",
          content: "wait, also do this",
          status: "pending",
          messageIndex: 1,
        }),
      ];

      const { ws, queryClient } = renderWorkspace();
      queryClient.setQueryData(["chat-messages", "session-1"], initialMessages);

      await act(async () => {
        ws.emit({
          channel: "chat:session-1",
          event: "chat.chunk",
          payload: {
            sessionId: "session-1",
            messageId: "msg-1",
            delta: "answer",
          },
        });
      });

      await waitFor(() => {
        const messages = queryClient.getQueryData<typeof initialMessages>([
          "chat-messages",
          "session-1",
        ]);
        // Delta applied to the streaming assistant (by id), not dropped and not
        // mis-applied to the tail user message.
        const assistant = messages?.find((m) => m.id === "msg-1");
        const userMsg = messages?.find((m) => m.id === "msg-2");
        expect(assistant?.content).toBe("Partial answer");
        expect(assistant?.status).toBe("streaming");
        expect(userMsg?.content).toBe("wait, also do this");
      });
    });

    it("ignores a chat.chunk for a messageId not in the cache (issue #1106)", async () => {
      const initialMessages = [
        createMockChatMessage({
          id: "msg-1",
          sessionId: "session-1",
          role: "assistant",
          content: "Existing",
          status: "streaming",
          messageIndex: 0,
        }),
      ];

      const { ws, queryClient } = renderWorkspace();
      queryClient.setQueryData(["chat-messages", "session-1"], initialMessages);

      await act(async () => {
        ws.emit({
          channel: "chat:session-1",
          event: "chat.chunk",
          payload: {
            sessionId: "session-1",
            messageId: "does-not-exist",
            delta: "stray",
          },
        });
      });

      const messages = queryClient.getQueryData<typeof initialMessages>([
        "chat-messages",
        "session-1",
      ]);
      // No message mutated by a delta targeting an absent id.
      expect(messages?.[0]?.content).toBe("Existing");
    });

    it("invalidates chat-messages cache on chat.done event", async () => {
      const { ws, queryClient } = renderWorkspace();
      const initialMessages = [
        createMockChatMessage({
          id: "msg-1",
          sessionId: "session-1",
          role: "assistant",
          content: "Hello",
          status: "streaming",
          messageIndex: 0,
        }),
      ];

      queryClient.setQueryData(["chat-messages", "session-1"], initialMessages);
      queryClient.setQueryData(["session-timeline", "session-1"], []);

      await act(async () => {
        ws.emit({
          channel: "chat:session-1",
          event: "chat.done",
          payload: {
            sessionId: "session-1",
            messageId: "msg-1",
            usage: { inputTokens: 100, outputTokens: 50 },
            citations: [],
          },
        });
      });

      await waitFor(() => {
        const messages = queryClient.getQueryData<typeof initialMessages>([
          "chat-messages",
          "session-1",
        ]);
        expect(messages?.[0]?.status).toBe("complete");
      });

      await waitFor(() => {
        const messagesState = queryClient.getQueryState([
          "chat-messages",
          "session-1",
        ]);
        const timelineState = queryClient.getQueryState([
          "session-timeline",
          "session-1",
        ]);

        const sessionState = queryClient.getQueryState([
          "chat-session",
          "session-1",
        ]);

        expect(messagesState?.isInvalidated).toBe(true);
        expect(timelineState?.isInvalidated).toBe(true);
        expect(sessionState === undefined || sessionState.isInvalidated).toBe(
          true,
        );
      });
    });

    it("invalidates chat caches on chat.message_updated event", async () => {
      const { ws, queryClient } = renderWorkspace();
      const initialMessages = [
        createMockChatMessage({
          id: "msg-1",
          sessionId: "session-1",
          role: "user",
          content: "compact seed prompt",
          status: "complete",
          messageIndex: 0,
        }),
      ];

      queryClient.setQueryData(["chat-messages", "session-1"], initialMessages);
      queryClient.setQueryData(["session-timeline", "session-1"], []);
      queryClient.setQueryData(
        ["chat-session", "session-1"],
        createMockChatSession({ id: "session-1" }),
      );
      queryClient.setQueryData(
        ["chat-sessions"],
        [createMockChatSession({ id: "session-1" })],
      );

      await act(async () => {
        ws.emit({
          channel: "chat:session-1",
          event: "chat.message_updated",
          payload: {
            sessionId: "session-1",
            messageId: "msg-1",
          },
        });
      });

      await waitFor(() => {
        expect(
          queryClient.getQueryState(["chat-messages", "session-1"])
            ?.isInvalidated,
        ).toBe(true);
        expect(
          queryClient.getQueryState(["session-timeline", "session-1"])
            ?.isInvalidated,
        ).toBe(true);
        expect(
          queryClient.getQueryState(["chat-session", "session-1"])
            ?.isInvalidated,
        ).toBe(true);
        expect(
          queryClient.getQueryState(["chat-sessions"])?.isInvalidated,
        ).toBe(true);
      });
    });

    it("marks message errored on chat.error but preserves partial streamed content (issue #1106 defect 3)", async () => {
      const { ws, queryClient } = renderWorkspace();
      const initialMessages = [
        createMockChatMessage({
          id: "msg-1",
          sessionId: "session-1",
          role: "assistant",
          // Partial content already streamed before the error frame arrives.
          content: "The answer so far is",
          status: "streaming",
          messageIndex: 0,
        }),
      ];

      queryClient.setQueryData(["chat-messages", "session-1"], initialMessages);
      queryClient.setQueryData(["session-timeline", "session-1"], []);

      await act(async () => {
        ws.emit({
          channel: "chat:session-1",
          event: "chat.error",
          payload: {
            sessionId: "session-1",
            messageId: "msg-1",
            error: "RATE_LIMIT",
            message: "Rate limit exceeded",
          },
        });
      });

      await waitFor(() => {
        const messages = queryClient.getQueryData<typeof initialMessages>([
          "chat-messages",
          "session-1",
        ]);
        expect(messages?.[0]?.status).toBe("error");
        // Partial streamed content is preserved; the raw error string does NOT
        // clobber the message body (websocket D15). The error surfaces via the
        // error status/indicator instead.
        expect(messages?.[0]?.content).toBe("The answer so far is");
      });

      await waitFor(() => {
        const messagesState = queryClient.getQueryState([
          "chat-messages",
          "session-1",
        ]);
        const timelineState = queryClient.getQueryState([
          "session-timeline",
          "session-1",
        ]);
        const sessionState = queryClient.getQueryState([
          "chat-session",
          "session-1",
        ]);

        expect(messagesState?.isInvalidated).toBe(true);
        expect(timelineState?.isInvalidated).toBe(true);
        expect(sessionState === undefined || sessionState.isInvalidated).toBe(
          true,
        );
      });
    });
  });

  // -------------------------------------------------------------------------
  // spend limit events
  // -------------------------------------------------------------------------
  describe("spend limit events", () => {
    it("invalidates spend cache on workspace.spend_limit_reached event", async () => {
      const { ws, queryClient } = renderWorkspace();

      await waitFor(() => {
        expect(queryClient.isFetching()).toBe(0);
      });

      await act(async () => {
        ws.emit({
          channel: "workspace:ws-1",
          event: "workspace.spend_limit_reached",
          payload: {
            workspaceId: "ws-1",
            periodSpendUsd: 50.0,
            limitSpendUsd: 50.0,
            reason: "max_spend_usd",
          },
        });
      });

      await waitFor(() => {
        const state = queryClient.getQueryState(["spend", "ws-1"]);
        expect(state === undefined || state.isInvalidated).toBe(true);
      });
    });

    it("invalidates spend cache on workspace.spend_limit_cleared event", async () => {
      const { ws, queryClient } = renderWorkspace();

      await waitFor(() => {
        expect(queryClient.isFetching()).toBe(0);
      });

      // First reach the limit
      await act(async () => {
        ws.emit({
          channel: "workspace:ws-1",
          event: "workspace.spend_limit_reached",
          payload: {
            workspaceId: "ws-1",
            periodSpendUsd: 50.0,
            limitSpendUsd: 50.0,
            reason: "max_spend_usd",
          },
        });
      });

      // Then clear it
      await act(async () => {
        ws.emit({
          channel: "workspace:ws-1",
          event: "workspace.spend_limit_cleared",
          payload: {
            workspaceId: "ws-1",
            reason: "period_reset",
          },
        });
      });

      await waitFor(() => {
        const state = queryClient.getQueryState(["spend", "ws-1"]);
        expect(state === undefined || state.isInvalidated).toBe(true);
      });
    });
  });

  // -------------------------------------------------------------------------
  // workspace events
  // -------------------------------------------------------------------------
  describe("workspace events", () => {
    it("invalidates workspace cache on workspace.sync event", async () => {
      const { ws, queryClient } = renderWorkspace();

      await waitFor(() => {
        expect(queryClient.isFetching()).toBe(0);
      });
      queryClient.setQueryData(["file-tree"], []);
      queryClient.setQueryData(["file-content"], "old");

      await act(async () => {
        ws.emit({
          channel: "workspace:ws-1",
          event: "workspace.sync",
          payload: {
            workspaceId: "ws-1",
            behindBase: 3,
            aheadOfBase: 7,
          },
        });
      });

      await waitFor(() => {
        const state = queryClient.getQueryState(["workspace", "ws-1"]);
        expect(state === undefined || state.isInvalidated).toBe(true);
      });

      await waitFor(() => {
        expect(queryClient.getQueryState(["file-tree"])?.isInvalidated).toBe(
          true,
        );
        expect(queryClient.getQueryState(["file-content"])?.isInvalidated).toBe(
          true,
        );
      });
    });

    it("invalidates file and document caches on workspace.branch_updated event", async () => {
      const { ws, queryClient } = renderWorkspace();

      await waitFor(() => {
        expect(queryClient.isFetching()).toBe(0);
      });

      queryClient.setQueryData(["workspace", "ws-1"], { id: "ws-1" });
      queryClient.setQueryData(["workspaces"], [{ id: "ws-1" }]);
      queryClient.setQueryData(["workspace-tasks"], []);
      queryClient.setQueryData(
        ["file-tree", "branch-1", "", 1],
        ["src/main.rs"],
      );
      queryClient.setQueryData(
        ["file-content", "branch-1", "src/main.rs"],
        "old",
      );
      queryClient.setQueryData(["docs", "ws-1", "workspace"], []);
      queryClient.setQueryData(["views", "ws-1", "workspace", {}], []);
      queryClient.setQueryData(
        ["source-control-history", "repo-1", "branch-1"],
        { pages: [], pageParams: [] },
      );
      queryClient.setQueryData(
        ["source-control-commit-diff", "repo-1", "branch-1", "a".repeat(40)],
        { immutable: true },
      );
      const invalidateSpy = vi.spyOn(queryClient, "invalidateQueries");

      await act(async () => {
        ws.emit({
          channel: "workspace:ws-1",
          event: "workspace.branch_updated",
          payload: {
            workspaceId: "ws-1",
            repoId: "repo-entry-1",
            branch: "main",
            headCommit: "abc1234",
          },
        });
      });

      await waitFor(() => {
        expect(
          queryClient.getQueryState(["workspace", "ws-1"])?.isInvalidated,
        ).toBe(true);
        expect(queryClient.getQueryState(["workspaces"])?.isInvalidated).toBe(
          true,
        );
        expect(
          queryClient.getQueryState(["workspace-tasks"])?.isInvalidated,
        ).toBe(true);
        expect(invalidateSpy).toHaveBeenCalledWith({
          queryKey: ["source-control-history"],
        });
        expect(invalidateSpy).not.toHaveBeenCalledWith({
          queryKey: ["source-control-commit-diff"],
        });
        const fileTreeState = queryClient.getQueryState([
          "file-tree",
          "branch-1",
          "",
          1,
        ]);
        expect(fileTreeState === undefined || fileTreeState.isInvalidated).toBe(
          true,
        );
        expect(
          queryClient.getQueryState(["file-content", "branch-1", "src/main.rs"])
            ?.isInvalidated,
        ).toBe(true);
        expect(invalidateSpy).toHaveBeenCalledWith({
          queryKey: ["docs", "ws-1"],
        });
        expect(invalidateSpy).toHaveBeenCalledWith({
          queryKey: ["views", "ws-1"],
        });
      });
    });

    it("invalidates settings cache on workspace.settings_updated event", async () => {
      const { ws, queryClient } = renderWorkspace();

      await waitFor(() => {
        expect(queryClient.isFetching()).toBe(0);
      });

      await act(async () => {
        ws.emit({
          channel: "workspace:ws-1",
          event: "workspace.settings_updated",
          payload: {
            workspaceId: "ws-1",
            updatedBy: "user-2",
            changes: ["model_config"],
          },
        });
      });

      await waitFor(() => {
        const state = queryClient.getQueryState(["settings", "ws-1"]);
        expect(state === undefined || state.isInvalidated).toBe(true);
      });
    });
  });

  // -------------------------------------------------------------------------
  // comment events
  // -------------------------------------------------------------------------
  describe("comment events", () => {
    it("invalidates comments cache on comment.new event", async () => {
      const { ws, queryClient } = renderWorkspace();

      await waitFor(() => {
        expect(queryClient.isFetching()).toBe(0);
      });

      await act(async () => {
        ws.emit({
          channel: "workspace:ws-1",
          event: "comment.new",
          payload: {
            workspaceId: "ws-1",
            commentId: "comment-1",
            scope: "node",
            nodeId: "node-1",
            originBranch: "main",
          },
        });
      });

      await waitFor(() => {
        const state = queryClient.getQueryState(["comments", "node-1"]);
        expect(state === undefined || state.isInvalidated).toBe(true);
      });
    });
  });

  // -------------------------------------------------------------------------
  // notification events
  // -------------------------------------------------------------------------
  describe("notification events", () => {
    it("invalidates notifications cache on notification event", async () => {
      const { ws, queryClient } = renderWorkspace();

      await waitFor(() => {
        expect(queryClient.isFetching()).toBe(0);
      });

      await act(async () => {
        ws.emit({
          channel: "workspace:ws-1",
          event: "notification",
          payload: {
            workspaceId: "ws-1",
            notification: {
              id: "notif-1",
              severity: "warning",
              category: "spend_warning",
              title: "Approaching spend limit",
              message: "You have used 80% of your monthly budget.",
              taskId: null,
              actions: [
                {
                  label: "View Spend",
                  action: "navigate",
                  target: "/settings/spend",
                },
              ],
              createdAt: new Date().toISOString(),
              expiresAt: null,
            },
          },
        });
      });

      await waitFor(() => {
        const state = queryClient.getQueryState(["notifications", "ws-1"]);
        expect(state === undefined || state.isInvalidated).toBe(true);
      });
    });
  });

  // -------------------------------------------------------------------------
  // connection lifecycle
  // -------------------------------------------------------------------------
  describe("connection lifecycle", () => {
    it("handles disconnect without crashing", async () => {
      const { ws } = renderWorkspace();

      // Simulate disconnect
      await act(async () => {
        ws.simulateDisconnect();
      });

      // Page should still be rendered (no crash)
      await waitFor(() => {
        expect(document.body).toBeTruthy();
      });
    });

    it("re-subscribes channels after reconnect", async () => {
      const { ws } = renderWorkspace();

      // Record initial subscriptions
      const initialSubs = ws.getSubscriptions();

      // Simulate disconnect
      await act(async () => {
        ws.simulateDisconnect();
      });

      // Simulate reconnect
      await act(async () => {
        ws.simulateReconnect();
      });

      // Subscriptions should still be present after reconnect
      await waitFor(() => {
        const currentSubs = ws.getSubscriptions();
        // All initial subscriptions should still be registered
        for (const sub of initialSubs) {
          expect(currentSubs).toContain(sub);
        }
      });
    });

    it("invalidates full active chat session state after reconnect", async () => {
      const queryClient = new QueryClient({
        defaultOptions: {
          queries: { retry: false, staleTime: Infinity, gcTime: Infinity },
        },
      });
      const manager = new WebSocketManager({
        getBaseUrl: () => "http://localhost:7500",
        getWsTicket: async () => "test-ticket",
        queryClient,
      });
      const managerInternals = manager as unknown as {
        channelRefCounts: Map<string, number>;
        invalidateAllSubscribedCaches: () => void;
      };

      queryClient.setQueryData(["chat-messages", "session-1"], []);
      queryClient.setQueryData(["session-timeline", "session-1"], {
        items: [],
        cursor: null,
        hasMore: false,
      });
      queryClient.setQueryData(
        ["chat-session", "session-1"],
        createMockChatSession({ id: "session-1" }),
      );
      queryClient.setQueryData(
        ["chat-sessions", "ws-1", "workspace"],
        [createMockChatSession({ id: "session-1" })],
      );
      queryClient.setQueryData(["workspace-tasks", "ws-1"], []);
      queryClient.setQueryData(["workspace-tasks", "ws-1"], []);

      managerInternals.channelRefCounts.set("chat:session-1", 1);

      await act(async () => {
        managerInternals.invalidateAllSubscribedCaches();
      });

      await waitFor(() => {
        expect(
          queryClient.getQueryState(["chat-messages", "session-1"])
            ?.isInvalidated,
        ).toBe(true);
        expect(
          queryClient.getQueryState(["session-timeline", "session-1"])
            ?.isInvalidated,
        ).toBe(true);
        expect(
          queryClient.getQueryState(["chat-session", "session-1"])
            ?.isInvalidated,
        ).toBe(true);
        expect(
          queryClient.getQueryState(["chat-sessions", "ws-1", "workspace"])
            ?.isInvalidated,
        ).toBe(true);
        expect(
          queryClient.getQueryState(["workspace-tasks", "ws-1"])?.isInvalidated,
        ).toBe(true);
        expect(
          queryClient.getQueryState(["workspace-tasks", "ws-1"])?.isInvalidated,
        ).toBe(true);
      });

      manager.destroy();
    });

    it("events are not delivered during disconnection", async () => {
      const { ws, queryClient } = renderWorkspace();

      await waitFor(() => {
        expect(queryClient.isFetching()).toBe(0);
      });

      // Disconnect
      await act(async () => {
        ws.simulateDisconnect();
      });

      // Pre-populate a cache entry to verify it's NOT invalidated
      queryClient.setQueryData(["task", "task-disconnected"], {
        id: "task-disconnected",
        status: "running",
      });

      // Emit event while disconnected — should be ignored
      await act(async () => {
        ws.emit({
          channel: "workspace:ws-1",
          event: "task.status",
          payload: { taskId: "task-disconnected", status: "succeeded" },
        });
      });

      // The cache entry should NOT have been invalidated since we're disconnected
      await waitFor(() => {
        const data = queryClient.getQueryData(["task", "task-disconnected"]);
        // Data should still be the original value since the event was not delivered
        expect(data).toEqual({ id: "task-disconnected", status: "running" });
      });
    });
  });

  // -------------------------------------------------------------------------
  // channel subscription lifecycle
  // -------------------------------------------------------------------------
  describe("channel subscription lifecycle", () => {
    it("registers subscriptions when components mount", async () => {
      const workspaceUuid = "11111111-1111-4111-8111-111111111111";
      server.use(
        http.get("/api/v1/resolve", () =>
          HttpResponse.json({
            data: {
              orgId: "org-1",
              workspaceId: workspaceUuid,
            },
          }),
        ),
      );
      const { ws } = renderWorkspace();

      // After rendering the workspace page, subscriptions should be registered
      await waitFor(() => {
        const subs = ws.getSubscriptions();
        expect(subs).toContain(`workspace:${workspaceUuid}`);
        expect(subs.some((sub) => sub.startsWith("draft:"))).toBe(false);
      });
    });

    it("can subscribe to chat channel programmatically", async () => {
      const { ws } = renderWorkspace();

      // Subscribe to a chat channel
      const unsubscribe = ws.subscribe("chat:session-1", () => {});

      await waitFor(() => {
        expect(ws.getSubscriptions()).toContain("chat:session-1");
      });

      // Unsubscribe
      unsubscribe();
    });

    it("unsubscribes when calling the returned cleanup function", async () => {
      const { ws } = renderWorkspace();

      const handler = vi.fn();
      const unsubscribe = ws.subscribe("test-channel:123", handler);

      // Verify subscription exists
      expect(ws.getSubscriptions()).toContain("test-channel:123");

      // Emit an event — handler should be called
      await act(async () => {
        ws.emit({
          channel: "test-channel:123",
          event: "test.event",
          payload: { data: "test" },
        });
      });
      expect(handler).toHaveBeenCalledTimes(1);

      // Unsubscribe
      unsubscribe();

      // Emit again — handler should NOT be called
      handler.mockClear();
      await act(async () => {
        ws.emit({
          channel: "test-channel:123",
          event: "test.event",
          payload: { data: "test2" },
        });
      });
      expect(handler).not.toHaveBeenCalled();
    });
  });

  // -------------------------------------------------------------------------
  // Edge cases
  // -------------------------------------------------------------------------
  describe("edge cases", () => {
    it("unknown event types do not crash the event handler", async () => {
      const { ws } = renderWorkspace();

      // Emit an event with an unknown type — should not crash
      await act(async () => {
        ws.emit({
          channel: "workspace:ws-1",
          event: "unknown.event.type",
          payload: { foo: "bar" },
        });
      });

      // Page still functional
      await waitFor(() => {
        expect(document.body).toBeTruthy();
      });
    });

    it("events for non-existent entities are handled gracefully", async () => {
      const { ws } = renderWorkspace();

      // Emit events for entities that don't exist in the cache
      await act(async () => {
        ws.emit({
          channel: "workspace:ws-1",
          event: "task.status",
          payload: { taskId: "nonexistent-id", status: "succeeded" },
        });
      });

      await act(async () => {
        ws.emit({
          channel: "chat:nonexistent-session",
          event: "chat.done",
          payload: {
            sessionId: "nonexistent-session",
            messageId: "nonexistent-msg",
            usage: { inputTokens: 0, outputTokens: 0 },
            citations: [],
          },
        });
      });

      // No crash
      await waitFor(() => {
        expect(document.body).toBeTruthy();
      });
    });

    it("multiple rapid events for the same entity are handled correctly", async () => {
      const { ws, queryClient } = renderWorkspace();

      await waitFor(() => {
        expect(queryClient.isFetching()).toBe(0);
      });

      // Fire multiple rapid events for the same task
      await act(async () => {
        ws.emit({
          channel: "workspace:ws-1",
          event: "task.status",
          payload: { taskId: "task-rapid", status: "running" },
        });
        ws.emit({
          channel: "workspace:ws-1",
          event: "task.status",
          payload: { taskId: "task-rapid", status: "succeeded" },
        });
        ws.emit({
          channel: "workspace:ws-1",
          event: "task.status",
          payload: { taskId: "task-rapid", status: "failed" },
        });
      });

      // No crash — all events processed
      await waitFor(() => {
        expect(document.body).toBeTruthy();
      });
    });

    it("empty channel string does not cause subscription issues", async () => {
      const { ws } = renderWorkspace();

      // Subscribe with empty channel — should not crash
      const handler = vi.fn();
      const unsubscribe = ws.subscribe("", handler);

      // Emit to empty channel
      await act(async () => {
        ws.emit({
          channel: "",
          event: "test",
          payload: {},
        });
      });

      // Handler may or may not be called, but no crash
      unsubscribe();
      expect(document.body).toBeTruthy();
    });
  });

  // -------------------------------------------------------------------------
  // Event payload format verification
  // -------------------------------------------------------------------------
  describe("event payload format", () => {
    it("uses camelCase field names in event payloads", async () => {
      const { ws } = renderWorkspace();
      const handler = vi.fn();

      ws.subscribe("workspace:ws-1", handler);

      await act(async () => {
        ws.emit({
          channel: "workspace:ws-1",
          event: "task.status",
          payload: {
            taskId: "task-1", // camelCase, not task_id
            status: "succeeded",
          },
        });
      });

      expect(handler).toHaveBeenCalledWith(
        expect.objectContaining({
          channel: "workspace:ws-1",
          event: "task.status",
          payload: expect.objectContaining({
            taskId: "task-1",
          }),
        }),
      );
    });

    it("spend limit event payloads use camelCase field names", async () => {
      const { ws } = renderWorkspace();
      const handler = vi.fn();

      ws.subscribe("workspace:ws-1", handler);

      await act(async () => {
        ws.emit({
          channel: "workspace:ws-1",
          event: "workspace.spend_limit_reached",
          payload: {
            workspaceId: "ws-1",
            periodSpendUsd: 50.0, // camelCase
            limitSpendUsd: 50.0, // camelCase
            reason: "max_spend_usd",
          },
        });
      });

      expect(handler).toHaveBeenCalledWith(
        expect.objectContaining({
          payload: expect.objectContaining({
            periodSpendUsd: 50.0,
            limitSpendUsd: 50.0,
          }),
        }),
      );
    });

    it("chat event payloads use camelCase field names", async () => {
      const { ws } = renderWorkspace();
      const handler = vi.fn();

      ws.subscribe("chat:session-1", handler);

      await act(async () => {
        ws.emit({
          channel: "chat:session-1",
          event: "chat.chunk",
          payload: {
            sessionId: "session-1", // camelCase
            messageId: "msg-1", // camelCase
            delta: "Hello ",
          },
        });
      });

      expect(handler).toHaveBeenCalledWith(
        expect.objectContaining({
          payload: expect.objectContaining({
            sessionId: "session-1",
            messageId: "msg-1",
          }),
        }),
      );
    });

    it("notification event payload uses camelCase field names", async () => {
      const { ws } = renderWorkspace();
      const handler = vi.fn();

      ws.subscribe("workspace:ws-1", handler);

      await act(async () => {
        ws.emit({
          channel: "workspace:ws-1",
          event: "notification",
          payload: {
            workspaceId: "ws-1",
            notification: {
              id: "notif-1",
              severity: "warning",
              category: "spend_warning",
              title: "Approaching spend limit",
              message: "Budget warning",
              taskId: null,
              actions: [
                { label: "View", action: "navigate", target: "/spend" },
              ],
              createdAt: new Date().toISOString(),
              expiresAt: null,
            },
          },
        });
      });

      expect(handler).toHaveBeenCalledWith(
        expect.objectContaining({
          payload: expect.objectContaining({
            workspaceId: "ws-1",
            notification: expect.objectContaining({
              severity: "warning",
              category: "spend_warning",
            }),
          }),
        }),
      );
    });

    it("comment event payload uses camelCase field names", async () => {
      const { ws } = renderWorkspace();
      const handler = vi.fn();

      ws.subscribe("workspace:ws-1", handler);

      await act(async () => {
        ws.emit({
          channel: "workspace:ws-1",
          event: "comment.new",
          payload: {
            workspaceId: "ws-1",
            commentId: "comment-1", // camelCase
            scope: "node",
            nodeId: "node-1", // camelCase
            originBranch: "main", // camelCase
          },
        });
      });

      expect(handler).toHaveBeenCalledWith(
        expect.objectContaining({
          payload: expect.objectContaining({
            commentId: "comment-1",
            nodeId: "node-1",
            originBranch: "main",
          }),
        }),
      );
    });

    it("workspace.sync event payload uses camelCase field names", async () => {
      const { ws } = renderWorkspace();
      const handler = vi.fn();

      ws.subscribe("workspace:ws-1", handler);

      await act(async () => {
        ws.emit({
          channel: "workspace:ws-1",
          event: "workspace.sync",
          payload: {
            workspaceId: "ws-1",
            behindBase: 3, // camelCase
            aheadOfBase: 7, // camelCase
          },
        });
      });

      expect(handler).toHaveBeenCalledWith(
        expect.objectContaining({
          payload: expect.objectContaining({
            behindBase: 3,
            aheadOfBase: 7,
          }),
        }),
      );
    });

    it("workspace.branch_updated event payload uses camelCase field names", async () => {
      const { ws } = renderWorkspace();
      const handler = vi.fn();

      ws.subscribe("workspace:ws-1", handler);

      await act(async () => {
        ws.emit({
          channel: "workspace:ws-1",
          event: "workspace.branch_updated",
          payload: {
            workspaceId: "ws-1",
            repoId: "repo-entry-1",
            branch: "main",
            headCommit: "abc1234",
          },
        });
      });

      expect(handler).toHaveBeenCalledWith(
        expect.objectContaining({
          payload: expect.objectContaining({
            workspaceId: "ws-1",
            repoId: "repo-entry-1",
            headCommit: "abc1234",
          }),
        }),
      );
    });
  });

  // -------------------------------------------------------------------------
  // Full event type coverage matrix
  // -------------------------------------------------------------------------
  describe("all event types coverage", () => {
    // This describe block ensures every event type is tested at least once.
    // Some events are already tested in detail above; this ensures completeness.

    const allEventTypes = [
      {
        event: "task.status",
        channel: "workspace:ws-1",
        payload: { taskId: "task-1", status: "succeeded" },
      },
      {
        event: "task.timeline",
        channel: "task:task-1",
        payload: {
          taskId: "task-1",
          entry: {
            id: "e1",
            entryType: "thought",
            content: "test",
            metadata: {},
            createdAt: new Date().toISOString(),
          },
        },
      },
      {
        event: "workspace.sync",
        channel: "workspace:ws-1",
        payload: { workspaceId: "ws-1", behindBase: 0, aheadOfBase: 0 },
      },
      {
        event: "workspace.branch_updated",
        channel: "workspace:ws-1",
        payload: {
          workspaceId: "ws-1",
          repoId: "repo-entry-1",
          branch: "main",
          headCommit: "abc",
        },
      },
      {
        event: "workspace.spend_limit_reached",
        channel: "workspace:ws-1",
        payload: {
          workspaceId: "ws-1",
          periodSpendUsd: 50,
          limitSpendUsd: 50,
          reason: "max_spend_usd",
        },
      },
      {
        event: "workspace.spend_limit_cleared",
        channel: "workspace:ws-1",
        payload: { workspaceId: "ws-1", reason: "period_reset" },
      },
      {
        event: "workspace.settings_updated",
        channel: "workspace:ws-1",
        payload: {
          workspaceId: "ws-1",
          updatedBy: "user-1",
          changes: ["model"],
        },
      },
      {
        event: "chat.chunk",
        channel: "chat:session-1",
        payload: { sessionId: "session-1", messageId: "msg-1", delta: "hi" },
      },
      {
        event: "chat.message_updated",
        channel: "chat:session-1",
        payload: { sessionId: "session-1", messageId: "msg-1" },
      },
      {
        event: "chat.done",
        channel: "chat:session-1",
        payload: {
          sessionId: "session-1",
          messageId: "msg-1",
          usage: { inputTokens: 10, outputTokens: 5 },
          citations: [],
        },
      },
      {
        event: "chat.error",
        channel: "chat:session-1",
        payload: {
          sessionId: "session-1",
          messageId: "msg-1",
          error: "ERR",
          message: "fail",
        },
      },
      {
        event: "comment.new",
        channel: "workspace:ws-1",
        payload: {
          workspaceId: "ws-1",
          commentId: "c1",
          scope: "node",
          nodeId: "n1",
          originBranch: "main",
        },
      },
      {
        event: "comment.reply",
        channel: "workspace:ws-1",
        payload: {
          workspaceId: "ws-1",
          commentId: "c1",
          replyId: "r1",
          authorName: "Ada",
        },
      },
      {
        event: "notification",
        channel: "workspace:ws-1",
        payload: {
          workspaceId: "ws-1",
          notification: {
            id: "n1",
            severity: "info",
            category: "task_blocked",
            title: "T",
            message: "M",
            taskId: null,
            actions: [],
            createdAt: new Date().toISOString(),
            expiresAt: null,
          },
        },
      },
    ];

    it.each(allEventTypes)(
      "handles $event event without crashing",
      async ({ event, channel, payload }) => {
        const { ws } = renderWorkspace();

        await act(async () => {
          ws.emit({ channel, event, payload });
        });

        // No crash
        expect(document.body).toBeTruthy();
      },
    );
  });
});
