import { act, fireEvent, screen, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { Route, Routes } from "react-router-dom";
import { expect, it, vi } from "vitest";

import { ChatPanel } from "../components/chat/ChatPanel";
import { useWorkspaceChatController } from "../hooks/useWorkspaceChatController";
import type { ChatMessageDTO, TimelineEntry } from "../lib/types";
import {
  createMockChatMessage,
  createMockChatSession,
  createMockTask,
  createMockUser,
} from "../test/fixtures";
import { server } from "../test/msw/server";
import { renderApp } from "../test/render";
import { verifiedAskUserQuestionFocus, WorkspacePage } from "./WorkspacePage";

const WORKSPACE_ID = "task-graph-rework";
const SESSION_ID = "notice-session";
const TASK_ID = "notice-task";

// Home mounts ChatPanel through this controller; the workbench assembles its
// props separately. Exercise both derivations with real message-cache updates.
function HomeChat() {
  const chat = useWorkspaceChatController({
    workspaceId: WORKSPACE_ID,
    orgId: "org-g5labs",
    enabled: true,
    writable: true,
    canControlLiveTurn: true,
    startAndSend: false,
  });
  return (
    <>
      <button onClick={() => chat.onSelectSession(SESSION_ID)}>
        Open chat
      </button>
      <ChatPanel
        {...chat}
        scopeType="workspace"
        scopeRef={WORKSPACE_ID}
        scopeLabel="Workspace"
        onSummarize={vi.fn()}
      />
    </>
  );
}

it.each([
  ["Home", "pending", false],
  ["Home", "pending", true],
  ["Home", "streaming", false],
  ["Home", "streaming", true],
  ["workbench", "pending", false],
  ["workbench", "pending", true],
  ["workbench", "streaming", false],
  ["workbench", "streaming", true],
] as const)(
  "keeps %s's %s turn live through a notice (already present: %s)",
  async (surface, status, noticeAtStart) => {
    // Keep network and React Query scheduling real while exercising the
    // workbench's actual three-second active-turn interval.
    vi.useFakeTimers({ toFake: ["setInterval", "clearInterval"] });
    vi.mocked(window.matchMedia).mockImplementation((query: string) => ({
      matches: query.includes("min-width: 1024px"),
      media: query,
      onchange: null,
      addListener: vi.fn(),
      removeListener: vi.fn(),
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      dispatchEvent: vi.fn(),
    }));
    const session = createMockChatSession({
      id: SESSION_ID,
      workspaceId: WORKSPACE_ID,
      scopeRef: null,
    });
    const assistant = createMockChatMessage({
      id: "live-assistant",
      sessionId: SESSION_ID,
      content: "",
      status,
      messageIndex: 2,
    });
    const notice = createMockChatMessage({
      id: "merge-notice",
      sessionId: SESSION_ID,
      role: "system",
      content: "The pull request merged.",
      messageIndex: 3,
    });
    let messages: ChatMessageDTO[] = [
      createMockChatMessage({
        id: "prompt",
        sessionId: SESSION_ID,
        role: "user",
        content: "Keep working",
        messageIndex: 1,
      }),
      assistant,
      ...(noticeAtStart ? [notice] : []),
    ];
    const timeline: TimelineEntry = {
      id: "thinking-started",
      entryType: "checkpoint",
      content: "Claude Code is thinking",
      sequence: 1,
      createdAt: assistant.createdAt,
      metadata: {
        sessionId: SESSION_ID,
        messageId: assistant.id,
        source: "provided_agent",
        provider: "claude_code",
        providerEvent: "provided_agent.thinking.started",
        taskId: TASK_ID,
        attempt: 1,
        seq: 1,
      },
    };
    const recoveredOutput: TimelineEntry = {
      ...timeline,
      id: "recovered-output",
      entryType: "output",
      content: "Recovered output after notice",
      sequence: 2,
      metadata: {
        ...timeline.metadata,
        schema_version: "g5.task-event.v1",
        kind: "message.delta",
        payload: {
          block_id: "notice-output",
          text_delta: "Recovered output after notice",
        },
        seq: 2,
        live: true,
      },
    };
    let messageReads = 0;
    let historyReads = 0;
    let tailReads = 0;
    let recoverTail = false;
    const stopped: string[] = [];
    server.use(
      http.post("/api/v1/workspaces/:id/config/agent-launch/resolve", () =>
        HttpResponse.json(
          { error: { code: "VALIDATION_ERROR", message: "No launch needed" } },
          { status: 422 },
        ),
      ),
      http.get("/api/v1/workspaces/:id/provider-quota", () =>
        HttpResponse.json({ data: null }),
      ),
      http.get("/api/v1/workspaces/:id/sessions", () =>
        HttpResponse.json({ data: [session] }),
      ),
      http.get("/api/v1/sessions/:id", () =>
        HttpResponse.json({ data: session }),
      ),
      http.get("/api/v1/sessions/:id/messages", () => {
        messageReads += 1;
        return HttpResponse.json({
          data: messages.map((message) =>
            message.role === "system"
              ? {
                  ...message,
                  // Server notice metadata is not a renderable Artifact[].
                  artifacts: {
                    platform_notice: {
                      kind: "pull_request",
                      delivery: "queued",
                      outcome: "merged",
                    },
                  },
                }
              : message,
          ),
        });
      }),
      http.get("/api/v1/sessions/:id/timeline", () => {
        historyReads += 1;
        return HttpResponse.json({
          data: { entries: [timeline], cursor: null, hasMore: false },
        });
      }),
      http.get("/api/v1/sessions/:id/live-tail", () => {
        tailReads += 1;
        return HttpResponse.json({
          data: {
            state: "live",
            taskId: TASK_ID,
            attempt: 1,
            persistedThrough: 1,
            fetchedThrough: recoverTail ? 2 : 1,
            entries: recoverTail ? [recoveredOutput] : [],
          },
        });
      }),
      http.get("/api/v1/sessions/:id/task", () =>
        HttpResponse.json({
          data: createMockTask({
            id: TASK_ID,
            status: "running",
            nativeSteeringAvailable: true,
          }),
        }),
      ),
      http.post(
        "/api/v1/sessions/:id/messages/:messageId/stop",
        ({ params }) => {
          stopped.push(String(params.messageId));
          return HttpResponse.json({
            data: { stopped: true, disposition: "signalling" },
          });
        },
      ),
    );
    const { queryClient } = renderApp(
      surface === "Home" ? (
        <HomeChat />
      ) : (
        <Routes>
          <Route path="/:org/:workspace/*" element={<WorkspacePage />} />
        </Routes>
      ),
      {
        route: `/org-g5labs/${WORKSPACE_ID}?session=${SESSION_ID}`,
        user: createMockUser(),
      },
    );
    if (surface === "Home")
      fireEvent.click(await screen.findByText("Open chat"));
    await screen.findByTestId("stop-streaming-button");
    await waitFor(() => expect(tailReads).toBeGreaterThan(0));

    if (!noticeAtStart) {
      messages = [...messages, notice];
      await act(async () => {
        await queryClient.invalidateQueries({
          queryKey: ["chat-messages", SESSION_ID],
        });
      });
    }
    await screen.findByText("The pull request merged.");
    expect(
      queryClient
        .getQueryData<ChatMessageDTO[]>(["chat-messages", SESSION_ID])
        ?.map((message) => message.id),
    ).toEqual(["prompt", assistant.id, notice.id]);
    expect(screen.getByTestId("streaming-indicator")).toBeVisible();
    expect(screen.getByTestId("provider-thinking-indicator")).toBeVisible();
    expect(screen.getByTestId("stop-streaming-button")).toBeEnabled();
    expect(screen.getByTestId("chat-input")).toHaveAttribute(
      "placeholder",
      "Claude reads this after its current step.",
    );

    // A saved-history refresh after the notice must still recover unsaved
    // output, through useLiveTail in each real chat controller.
    recoverTail = true;
    const tailReadsBeforeRefresh = tailReads;
    await act(async () => {
      await queryClient.invalidateQueries({
        queryKey: ["session-timeline", SESSION_ID],
      });
    });
    await screen.findByText("Recovered output after notice");
    expect(tailReads).toBeGreaterThan(tailReadsBeforeRefresh);

    if (surface === "workbench") {
      const messagesBeforePoll = messageReads;
      const historyBeforePoll = historyReads;
      await act(async () => {
        await vi.advanceTimersByTimeAsync(3000);
      });
      await waitFor(() => {
        expect(messageReads).toBeGreaterThan(messagesBeforePoll);
        expect(historyReads).toBeGreaterThan(historyBeforePoll);
      });
      expect(screen.getByTestId("stop-streaming-button")).toBeEnabled();
    }

    fireEvent.keyDown(screen.getByTestId("chat-input"), { key: "Escape" });
    await waitFor(() => expect(stopped).toEqual([assistant.id]));

    // A later user input stays queued behind the same assistant, and the
    // activity indicator stays before that follow-up even with a trailing notice.
    messages = [
      ...messages,
      createMockChatMessage({
        id: "follow-up",
        sessionId: SESSION_ID,
        role: "user",
        content: "A queued follow-up",
        messageIndex: 4,
      }),
      createMockChatMessage({
        id: "another-notice",
        sessionId: SESSION_ID,
        role: "system",
        content: "Another queued platform notice.",
        messageIndex: 5,
      }),
    ];
    await act(async () => {
      await queryClient.invalidateQueries({
        queryKey: ["chat-messages", SESSION_ID],
      });
    });
    const followUp = await screen.findByText("A queued follow-up");
    expect(screen.getByTestId("queued-message-icon")).toBeVisible();
    expect(
      screen
        .getByTestId("streaming-indicator")
        .compareDocumentPosition(followUp) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();

    messages = messages.map((message) =>
      message.id === assistant.id
        ? { ...message, status: "complete" }
        : message,
    );
    await act(async () => {
      await queryClient.invalidateQueries({
        queryKey: ["chat-messages", SESSION_ID],
      });
    });
    await waitFor(() =>
      expect(screen.queryByTestId("stop-streaming-button")).toBeNull(),
    );
    expect(screen.queryByTestId("streaming-indicator")).toBeNull();
    expect(screen.queryByTestId("provider-thinking-indicator")).toBeNull();
    expect(screen.queryByTestId("queued-message-icon")).toBeNull();
    expect(screen.getByTestId("chat-input")).toHaveAttribute(
      "placeholder",
      "Ask about this codebase…",
    );

    // Loading older history must not revive the completed latest reply just
    // because a stale predecessor still says streaming.
    act(() => {
      queryClient.setQueryData(
        ["chat-messages", SESSION_ID],
        [
          createMockChatMessage({
            id: "older-assistant",
            sessionId: SESSION_ID,
            status: "streaming",
            messageIndex: 0,
          }),
          ...messages,
        ],
      );
    });
    expect(screen.queryByTestId("stop-streaming-button")).toBeNull();
    expect(screen.queryByTestId("streaming-indicator")).toBeNull();
    fireEvent.keyDown(screen.getByTestId("chat-input"), { key: "Escape" });
    expect(stopped).toEqual([assistant.id]);
  },
);

it("keeps AskUserQuestion focus tied to its live reply through a later notice", () => {
  const assistant = createMockChatMessage({
    id: "question-assistant",
    sessionId: SESSION_ID,
    status: "streaming",
  });
  const input = {
    approvalId: "notice-question",
    sessionHint: SESSION_ID,
    routeTaskId: TASK_ID,
    selectedTaskId: TASK_ID,
    activeSessionId: SESSION_ID,
    activeSession: { id: SESSION_ID },
    timelineEntries: [
      {
        id: "question-request",
        entryType: "checkpoint",
        content: "Question requested",
        sequence: 1,
        createdAt: assistant.createdAt,
        metadata: {
          providerEvent: "provided_agent.approval.requested",
          toolName: "AskUserQuestion",
          approvalId: "notice-question",
          sessionId: SESSION_ID,
          messageId: assistant.id,
        },
      },
    ],
    messages: [
      assistant,
      createMockChatMessage({
        id: "question-notice",
        sessionId: SESSION_ID,
        role: "system",
        messageIndex: 1,
      }),
    ],
    isStreaming: true,
  };
  expect(verifiedAskUserQuestionFocus(input)).toBe(input.approvalId);
  expect(
    verifiedAskUserQuestionFocus({
      ...input,
      messages: input.messages.map((message) =>
        message.id === assistant.id
          ? { ...message, status: "complete" }
          : message,
      ),
      isStreaming: false,
    }),
  ).toBeNull();
});
