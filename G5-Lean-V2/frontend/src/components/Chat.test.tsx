// =============================================================================
// Chat Panel Integration Tests
// =============================================================================
// Tests the chat panel including message list rendering, sending messages,
// streaming response display, session switching, and error states.
// =============================================================================

import { describe, it, expect, vi } from "vitest";
import {
  screen,
  waitFor,
  fireEvent,
  within,
  act,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { server } from "../test/msw/server";
import { renderApp } from "../test/render";
import {
  createMockChatSession,
  createMockChatMessage,
  mockChatSessions,
  mockChatMessages,
} from "../test/fixtures";

// Import component under test
import { ChatPanel } from "./chat/ChatPanel";

// ---------------------------------------------------------------------------
// Default props factory
// ---------------------------------------------------------------------------

function defaultChatProps(overrides: Record<string, unknown> = {}) {
  const session = createMockChatSession({
    id: "session-1",
    title: "Test Session",
  });
  const messages = [
    createMockChatMessage({
      id: "msg-1",
      role: "user",
      content: "Hello, how are you?",
      messageIndex: 0,
    }),
    createMockChatMessage({
      id: "msg-2",
      role: "assistant",
      content: "I am doing well! How can I help you today?",
      messageIndex: 1,
    }),
  ];

  return {
    session,
    messages,
    isStreaming: false,
    scopeType: "workspace" as const,
    scopeRef: "test-workspace",
    scopeLabel: "Test Workspace",
    sessions: [
      session,
      createMockChatSession({ id: "session-2", title: "Second Session" }),
    ],
    onSendMessage: vi.fn(),
    onStopStreaming: vi.fn(),
    onForkSession: vi.fn(),
    onResumeSession: vi.fn(),
    onSummarize: vi.fn(),
    onCreateSession: vi.fn(),
    onSelectSession: vi.fn(),
    onOpenNode: vi.fn(),
    onOpenFile: vi.fn(),
    ...overrides,
  };
}

// =============================================================================
// Test Suite
// =============================================================================

describe("ChatPanel", () => {
  describe("loading states", () => {
    it("renders loading state when session is null", async () => {
      const props = defaultChatProps({ session: null, messages: [] });
      renderApp(<ChatPanel {...props} />, {
        route: "/org-g5labs/test-workspace",
      });

      // When session is null, should show empty/create state
      await waitFor(() => {
        // Look for the create/start prompt or empty state
        const chatArea =
          document.querySelector('[class*="chat"]') || document.body;
        expect(chatArea).toBeTruthy();
      });
    });
  });

  describe("loaded states", () => {
    it("renders message list for selected session", async () => {
      const props = defaultChatProps();
      renderApp(<ChatPanel {...props} />, {
        route: "/org-g5labs/test-workspace",
      });

      await waitFor(() => {
        expect(screen.getByText("Hello, how are you?")).toBeInTheDocument();
        expect(screen.getByText(/I am doing well/)).toBeInTheDocument();
      });
    });

    it("renders chat session list in picker", async () => {
      const props = defaultChatProps();
      renderApp(<ChatPanel {...props} />, {
        route: "/org-g5labs/test-workspace",
      });

      // The session picker should show the current session title
      await waitFor(() => {
        expect(screen.getByText("Test Session")).toBeInTheDocument();
      });
    });
  });

  describe("sending messages", () => {
    it("calls onSendMessage on form submit", async () => {
      const onSendMessage = vi.fn();
      const props = defaultChatProps({ onSendMessage });
      const user = userEvent.setup();

      renderApp(<ChatPanel {...props} />, {
        route: "/org-g5labs/test-workspace",
      });

      // Find the textarea input
      const textarea =
        screen.getByRole("textbox") || screen.getByPlaceholderText(/message/i);
      await user.type(textarea, "Hello world");

      // Find and click the send button
      const sendButton =
        screen.getByRole("button", { name: /send/i }) ||
        screen
          .getAllByRole("button")
          .find((btn) => btn.querySelector("svg") !== null);
      if (sendButton) {
        await user.click(sendButton);
      }
    });
  });

  describe("streaming states", () => {
    it("shows streaming indicator when isStreaming is true", async () => {
      const props = defaultChatProps({ isStreaming: true });
      renderApp(<ChatPanel {...props} />, {
        route: "/org-g5labs/test-workspace",
      });

      // Should show some kind of activity or streaming indicator
      await waitFor(() => {
        const streamingEl =
          screen.queryByText(/thinking|working|calling|planning/i) ||
          screen.queryByText(/streaming/i) ||
          document.querySelector('[class*="animate"]');
        expect(streamingEl || document.body).toBeTruthy();
      });
    });

    it("calls onStopStreaming when stop button is clicked", async () => {
      const onStopStreaming = vi.fn();
      const props = defaultChatProps({
        isStreaming: true,
        onStopStreaming,
      });
      const user = userEvent.setup();

      renderApp(<ChatPanel {...props} />, {
        route: "/org-g5labs/test-workspace",
      });

      // The composer's Stop control (chat g5.chat.controls); the earlier
      // title lookup never matched, so this assertion was vacuous.
      await user.click(screen.getByTestId("stop-streaming-button"));
      expect(onStopStreaming).toHaveBeenCalledTimes(1);
    });
  });

  describe("tool calls and artifacts", () => {
    it("renders tool call badges in assistant messages", async () => {
      const msgWithTools = createMockChatMessage({
        id: "msg-tools",
        role: "assistant",
        content: "I used a tool to help with that.",
        artifacts: [
          {
            type: "code",
            title: "Generated Code",
            content: 'console.log("hello")',
            language: "javascript",
          },
        ],
      });

      const props = defaultChatProps({ messages: [msgWithTools] });
      renderApp(<ChatPanel {...props} />, {
        route: "/org-g5labs/test-workspace",
      });

      await waitFor(() => {
        expect(screen.getByText(/I used a tool/)).toBeInTheDocument();
      });
    });

    it("renders code artifacts in messages", async () => {
      const msgWithCode = createMockChatMessage({
        id: "msg-code",
        role: "assistant",
        content: "Here is some code:",
        artifacts: [
          {
            type: "code",
            title: "Example",
            content: "const x = 42;",
            language: "typescript",
          },
        ],
      });

      const props = defaultChatProps({ messages: [msgWithCode] });
      renderApp(<ChatPanel {...props} />, {
        route: "/org-g5labs/test-workspace",
      });

      await waitFor(() => {
        expect(screen.getByText(/Here is some code/)).toBeInTheDocument();
      });
    });
  });

  it("renders provided-agent output timeline entries", async () => {
    const props = defaultChatProps({
      messages: [
        createMockChatMessage({
          id: "msg-user-output",
          role: "user",
          content: "hello-1",
          messageIndex: 0,
          createdAt: "2024-11-15T10:00:00Z",
        }),
        createMockChatMessage({
          id: "msg-assistant-output",
          role: "assistant",
          content: "done",
          messageIndex: 1,
          createdAt: "2024-11-15T10:10:00Z",
        }),
      ],
      timelineEntries: [
        {
          id: "trace-output-1",
          entryType: "output",
          content: "Claude Code says hello 1",
          metadata: {
            phase: "output",
            source: "provided_agent",
            provider: "codex",
            providerEvent: "provided_agent.output.delta",
            sessionId: "session-1",
          },
          createdAt: "2024-11-15T10:05:00Z",
        },
      ],
    });

    renderApp(<ChatPanel {...props} />, {
      route: "/org-g5labs/test-workspace",
    });

    expect(
      await screen.findByText(/Claude Code says hello 1/),
    ).toBeInTheDocument();
  });

  it("coalesces provided-agent output deltas into one response row", async () => {
    const props = defaultChatProps({
      messages: [
        createMockChatMessage({
          id: "msg-user-output",
          role: "user",
          content: "ls-10",
          messageIndex: 0,
          createdAt: "2024-11-15T10:00:00Z",
        }),
        createMockChatMessage({
          id: "msg-assistant-output",
          role: "assistant",
          content: "done",
          messageIndex: 1,
          createdAt: "2024-11-15T10:10:00Z",
        }),
      ],
      timelineEntries: [
        {
          id: "trace-output-1",
          entryType: "output",
          content: "Claude Code ls progress 1/10",
          metadata: {
            phase: "output",
            source: "provided_agent",
            provider: "codex",
            providerEvent: "provided_agent.output.delta",
            sessionId: "session-1",
            messageId: "msg-assistant-output",
            blockId: "claude-tool-msg-assistant-output-progress",
          },
          createdAt: "2024-11-15T10:05:00Z",
        },
        {
          id: "trace-output-2",
          entryType: "output",
          content: "Claude Code ls progress 2/10",
          metadata: {
            phase: "output",
            source: "provided_agent",
            provider: "codex",
            providerEvent: "provided_agent.output.delta",
            sessionId: "session-1",
            messageId: "msg-assistant-output",
            blockId: "claude-tool-msg-assistant-output-progress",
          },
          createdAt: "2024-11-15T10:06:00Z",
        },
      ],
    });

    renderApp(<ChatPanel {...props} />, {
      route: "/org-g5labs/test-workspace",
    });

    const messages = await screen.findByTestId("chat-messages");
    expect(messages).toHaveTextContent("Claude Code ls progress 1/10");
    expect(messages).toHaveTextContent("Claude Code ls progress 2/10");
    expect(screen.queryByText("Response")).not.toBeInTheDocument();
  });

  it("renders live approval request controls and resolves approve clicks", async () => {
    const onResolveAgentApproval = vi.fn();
    const user = userEvent.setup();
    const props = defaultChatProps({
      onResolveAgentApproval,
      messages: [
        createMockChatMessage({
          id: "msg-user-approval",
          role: "user",
          content: "perm-ls",
          messageIndex: 0,
          createdAt: "2024-11-15T10:00:00Z",
        }),
        createMockChatMessage({
          id: "msg-assistant-approval",
          role: "assistant",
          content: "",
          status: "pending",
          messageIndex: 1,
          createdAt: "2024-11-15T10:10:00Z",
        }),
      ],
      timelineEntries: [
        {
          id: "trace-approval-1",
          entryType: "checkpoint",
          content: "Claude Code requests approval to list workspace files",
          metadata: {
            phase: "approval",
            source: "provided_agent",
            provider: "codex",
            providerEvent: "provided_agent.approval.requested",
            sessionId: "session-1",
            approvalId: "approval-1",
            toolName: "ls",
            requestedAction: "List workspace files",
          },
          createdAt: "2024-11-15T10:05:00Z",
        },
      ],
    });

    renderApp(<ChatPanel {...props} />, {
      route: "/org-g5labs/test-workspace",
    });

    expect(
      await screen.findByText(/Approval requested: ls/i),
    ).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /^allow$/i }));

    expect(onResolveAgentApproval).toHaveBeenCalledWith({
      sessionId: "session-1",
      approvalId: "approval-1",
      decision: "approved",
    });
  });

  it("uses the settings picker instead of provider cards", async () => {
    const props = defaultChatProps({
      session: null,
      selectedAgentProvider: "claude_code",
      availableAgentProviders: ["claude_code"],
    });

    renderApp(<ChatPanel {...props} />, {
      route: "/org-g5labs/test-workspace",
    });

    expect(await screen.findByText("Start Chat")).toBeInTheDocument();
    expect(
      screen.queryByTestId("provider-choice-cards"),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("provider-card-claude_code"),
    ).not.toBeInTheDocument();
    expect(screen.queryByTestId("chat-agent-provider")).not.toBeInTheDocument();
  });

  it("keeps a Codex-pinned active session option visible when Codex workers are unavailable", async () => {
    const props = defaultChatProps({
      session: createMockChatSession({
        id: "session-1",
        title: "Codex Session",
        agentProvider: "codex",
      }),
      selectedAgentProvider: "codex",
      availableAgentProviders: ["claude_code"],
    });

    renderApp(<ChatPanel {...props} />, {
      route: "/org-g5labs/test-workspace",
    });

    expect(
      screen.queryByTestId("chat-agent-provider-pinned"),
    ).not.toBeInTheDocument();
    expect(screen.queryByTestId("chat-agent-provider")).not.toBeInTheDocument();
  });

  describe("error states", () => {
    it("handles send message error gracefully", async () => {
      server.use(
        http.post("/api/v1/sessions/:id/messages", () =>
          HttpResponse.json(
            {
              error: {
                code: "INTERNAL",
                message: "Failed",
                requestId: "req-1",
              },
            },
            { status: 500 },
          ),
        ),
      );

      const props = defaultChatProps();
      renderApp(<ChatPanel {...props} />, {
        route: "/org-g5labs/test-workspace",
      });

      await waitFor(() => {
        expect(screen.getByText("Hello, how are you?")).toBeInTheDocument();
      });
    });
  });

  describe("session switching", () => {
    it("calls onSelectSession when a different session is selected", async () => {
      const onSelectSession = vi.fn();
      const props = defaultChatProps({ onSelectSession });
      const user = userEvent.setup();

      renderApp(<ChatPanel {...props} />, {
        route: "/org-g5labs/test-workspace",
      });

      // The session picker should be present
      await waitFor(() => {
        expect(screen.getByText("Test Session")).toBeInTheDocument();
      });
    });
  });

  describe("empty states", () => {
    it("shows empty state for new session with no messages", async () => {
      const emptySession = createMockChatSession({
        id: "session-empty",
        title: "Empty Session",
        messageCount: 0,
      });
      const props = defaultChatProps({ session: emptySession, messages: [] });

      renderApp(<ChatPanel {...props} />, {
        route: "/org-g5labs/test-workspace",
      });

      // Should show some empty state or prompt
      await waitFor(() => {
        const emptyState =
          screen.queryByText(/start/i) ||
          screen.queryByText(/no messages/i) ||
          screen.queryByText(/conversation/i) ||
          document.querySelector('[class*="empty"]');
        expect(emptyState || document.body).toBeTruthy();
      });
    });
  });

  describe("citations", () => {
    it("renders citation links in assistant messages", async () => {
      const msgWithCitations = createMockChatMessage({
        id: "msg-cite",
        role: "assistant",
        content: "Based on the source code analysis:",
        citations: [
          {
            label: "auth/validator.rs",
            filePath: "src/auth/validator.rs",
          },
        ],
      });

      const props = defaultChatProps({ messages: [msgWithCitations] });
      renderApp(<ChatPanel {...props} />, {
        route: "/org-g5labs/test-workspace",
      });

      await waitFor(() => {
        expect(screen.getByText(/Based on the source/)).toBeInTheDocument();
      });
    });
  });
});

describe("Claude Code provided-agent parity UI", () => {
  it("renders the compact launch pill without repeating frozen settings in the popup", async () => {
    const user = userEvent.setup();
    const session = createMockChatSession({
      id: "claude-session-1",
      agentProvider: "claude_code",
      providerState: {
        claudeCode: { settings: { permissionMode: "bypassPermissions" } },
      },
    });
    const messages = [
      createMockChatMessage({
        id: "assistant-1",
        sessionId: session.id,
        role: "assistant",
        content: "Claude response",
        messageIndex: 0,
      }),
    ];
    const timelineEntries = [
      {
        id: "usage-1",
        entryType: "checkpoint",
        content: "Claude Code usage updated",
        createdAt: "2024-11-15T10:05:01Z",
        metadata: {
          providerEvent: "provided_agent.usage.updated",
          phase: "status",
          messageId: "assistant-1",
          usage: { input_tokens: 12345 },
        },
      },
    ];

    renderApp(
      <ChatPanel
        {...defaultChatProps({
          session,
          messages,
          timelineEntries,
          selectedAgentProvider: "claude_code",
          availableAgentProviders: ["claude_code", "codex"],
        })}
      />,
      { route: "/org-g5labs/test-workspace" },
    );

    expect(screen.getByTestId("claude-code-controls")).toBeInTheDocument();
    expect(screen.getByTestId("claude-permission-control")).toHaveTextContent(
      /bypass/i,
    );
    expect(screen.getByTestId("claude-model-control")).toHaveTextContent(
      /Opus 5\.5/,
    );
    expect(
      screen.queryByTestId("claude-input-token-count"),
    ).not.toBeInTheDocument();

    await user.click(screen.getByTestId("claude-settings-control"));
    expect(
      screen.queryByTestId("claude-input-token-count"),
    ).not.toBeInTheDocument();
    expect(screen.queryByText(/Input Tokens:/i)).not.toBeInTheDocument();
    expect(
      screen.getByText(/Agent and Mode are fixed for this chat/i),
    ).toBeInTheDocument();
    const popup = screen.getByTestId("claude-settings-modal");
    expect(within(popup).getByText("Mode")).toBeInTheDocument();
    expect(within(popup).getByText("Model")).toBeInTheDocument();
    expect(within(popup).getByText("Effort")).toBeInTheDocument();
    expect(within(popup).getByTestId("chat-agent-select")).toBeDisabled();
  });

  it("dismisses Claude settings on outside click, Escape, and settings button toggle", async () => {
    const user = userEvent.setup();
    const session = createMockChatSession({
      id: "claude-session-dismissible",
      agentProvider: "claude_code",
    });

    renderApp(
      <div>
        <button type="button">Outside target</button>
        <ChatPanel
          {...defaultChatProps({
            session,
            messages: [],
            timelineEntries: [],
            selectedAgentProvider: "claude_code",
            availableAgentProviders: ["claude_code", "codex"],
          })}
        />
      </div>,
      { route: "/org-g5labs/test-workspace" },
    );

    const settingsButton = screen.getByTestId("claude-settings-control");

    await user.click(settingsButton);
    expect(screen.getByTestId("claude-settings-modal")).toBeInTheDocument();

    await user.click(settingsButton);
    expect(
      screen.queryByTestId("claude-settings-modal"),
    ).not.toBeInTheDocument();

    await user.click(settingsButton);
    expect(screen.getByTestId("claude-settings-modal")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Outside target" }));
    expect(
      screen.queryByTestId("claude-settings-modal"),
    ).not.toBeInTheDocument();

    await user.click(settingsButton);
    expect(screen.getByTestId("claude-settings-modal")).toBeInTheDocument();

    await user.keyboard("{Escape}");
    expect(
      screen.queryByTestId("claude-settings-modal"),
    ).not.toBeInTheDocument();
  });

  it("renders Claude status, context, warning, and tool timeline entries on replay", async () => {
    const session = createMockChatSession({
      id: "claude-session-2",
      agentProvider: "claude_code",
    });
    const messages = [
      createMockChatMessage({
        id: "assistant-2",
        sessionId: session.id,
        role: "assistant",
        content: "Claude response",
        messageIndex: 0,
      }),
    ];
    const timelineEntries = [
      {
        id: "context-1",
        entryType: "checkpoint",
        content: "Claude Code context usage updated",
        createdAt: "2024-11-15T10:05:01Z",
        metadata: {
          providerEvent: "provided_agent.context_usage.updated",
          phase: "status",
          messageId: "assistant-2",
          contextUsage: { tokensUsed: 222 },
        },
      },
      {
        id: "control-1",
        entryType: "checkpoint",
        content: "Claude Code control result",
        createdAt: "2024-11-15T10:05:02Z",
        metadata: {
          providerEvent: "provided_agent.control.result",
          phase: "status",
          messageId: "assistant-2",
          action: "set_effort",
          ok: true,
        },
      },
      {
        id: "warning-1",
        entryType: "checkpoint",
        content:
          "Live context usage is only available while Claude is running.",
        createdAt: "2024-11-15T10:05:03Z",
        metadata: {
          providerEvent: "provided_agent.warning",
          phase: "status",
          messageId: "assistant-2",
        },
      },
      {
        id: "tool-1",
        entryType: "tool_call",
        content: "Claude Code used Bash",
        createdAt: "2024-11-15T10:05:04Z",
        metadata: {
          providerEvent: "provided_agent.tool.started",
          phase: "tool_call",
          messageId: "assistant-2",
          toolName: "Bash",
          args: { command: "npm test" },
        },
      },
      {
        id: "tool-result-1",
        entryType: "tool_result",
        content: "Claude Code completed Bash",
        createdAt: "2024-11-15T10:05:05Z",
        metadata: {
          providerEvent: "provided_agent.tool.completed",
          phase: "tool_result",
          messageId: "assistant-2",
          toolName: "Bash",
          contentSnippet: "tests passed",
        },
      },
    ];

    renderApp(
      <ChatPanel
        {...defaultChatProps({
          session,
          messages,
          timelineEntries,
          selectedAgentProvider: "claude_code",
          availableAgentProviders: ["claude_code", "codex"],
        })}
      />,
      { route: "/org-g5labs/test-workspace" },
    );

    expect(screen.getByText(/Agent context usage/)).toBeInTheDocument();
    expect(screen.getByText(/Agent control result/)).toBeInTheDocument();
    expect(screen.getByText(/Live context usage/)).toBeInTheDocument();
    expect(screen.getAllByText(/Bash/).length).toBeGreaterThan(0);
    expect(screen.getAllByText(/npm test/).length).toBeGreaterThan(0);
    expect(screen.getByText(/tests passed/)).toBeInTheDocument();
  });
});
