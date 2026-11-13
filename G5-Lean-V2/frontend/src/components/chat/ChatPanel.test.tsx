import {
  act,
  fireEvent,
  render as rtlRender,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import React from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

import {
  ChatPanel,
  currentActivityLabel,
  createChatComposerDraft,
} from "./ChatPanel";
import type { ChatPanelProps } from "./ChatPanel";
import type { CodexAuthView } from "./CodexDeviceAuth";
import { CLAUDE_CHAT_DEFAULT_MODEL } from "../../lib/claudeModels";
import { ApiError } from "../../lib/api";
import {
  reviewWorkspaceChatPrompt,
  reviewWorkspaceSeed,
  VERIFICATION_NOTICE,
  verificationChatPrompt,
  verificationSeed,
} from "../../lib/reviewPrompts";
import {
  chatPromptHistoryKey,
  readChatPromptHistory,
} from "../../lib/chatPromptHistory";
import { projectTaskTimelineEntry } from "../../lib/taskTimeline";
import { createMockTask } from "../../test/fixtures";
import { WorkspaceLiveGateProvider } from "../../lib/workspaceLiveGate";
import type {
  AgentLaunchPreferencesV1,
  ChatAgentProvider,
  ChatMessageDTO,
  ChatSessionDTO,
  ProviderAccountDTO,
  ResolvedAgentExecutionV1,
  SubscriptionUsage,
  TaskSummary,
  TimelineEntry,
} from "../../lib/types";

function render(ui: React.ReactNode) {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  });
  return rtlRender(ui, {
    wrapper: ({ children }) => (
      <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
    ),
  });
}

const CONFIGURED_CHAT_RESOLUTION = {
  schemaVersion: 1 as const,
  resolutionId: "chat-resolution",
  configRevision: "config-2",
  catalogVersion: "catalog-3",
  provider: { value: "codex" as const, provenance: "configured" as const },
  providerRoute: "openai_direct" as const,
  model: { value: "gpt-5.6-sol", provenance: "configured" as const },
  upstreamModel: "openai/gpt-5.6-sol",
  permissionMode: {
    value: "never/danger-full-access",
    provenance: "canonical_default" as const,
  },
  effort: { value: "xhigh", provenance: "configured" as const },
  authMode: "chat_gpt_subscription" as const,
  egressMode: "direct_provider" as const,
  credentialReady: true,
  requiresInteractiveAuth: false,
};

const INHERITED_CLAUDE_CHAT_RESOLUTION = {
  ...CONFIGURED_CHAT_RESOLUTION,
  resolutionId: "chat-resolution-inherited-claude",
  provider: {
    value: "claude_code" as const,
    provenance: "configured" as const,
  },
  providerRoute: "anthropic_direct" as const,
  model: {
    value: "claude-opus-4-8[1m]",
    provenance: "configured" as const,
  },
  upstreamModel: "anthropic/claude-opus-4-8[1m]",
  permissionMode: {
    value: "bypassPermissions",
    provenance: "canonical_default" as const,
  },
  effort: { value: "high", provenance: "configured" as const },
  authMode: "anthropic_api_key" as const,
  egressMode: "direct_provider" as const,
};

const EXPLICIT_CODEX_CHAT_RESOLUTION = {
  ...CONFIGURED_CHAT_RESOLUTION,
  resolutionId: "chat-resolution-explicit-codex",
  model: { value: "gpt-5.5", provenance: "explicit" as const },
  upstreamModel: "openai/gpt-5.5",
  permissionMode: {
    value: "never/danger-full-access",
    provenance: "explicit" as const,
  },
  effort: { value: "medium", provenance: "explicit" as const },
};

function explicitClaudeResolution(
  preferences: AgentLaunchPreferencesV1,
  resolutionId: string,
): ResolvedAgentExecutionV1 {
  const model =
    preferences.model.source === "explicit"
      ? preferences.model.value
      : "claude-opus-4-8[1m]";
  return {
    ...INHERITED_CLAUDE_CHAT_RESOLUTION,
    resolutionId,
    provider: { value: "claude_code", provenance: "explicit" },
    model: { value: model, provenance: "explicit" },
    upstreamModel: `anthropic/${model}`,
    permissionMode: {
      value:
        preferences.permissionMode.source === "explicit"
          ? preferences.permissionMode.value
          : "bypassPermissions",
      provenance: "explicit",
    },
    effort: {
      value:
        preferences.effort.source === "explicit"
          ? preferences.effort.value
          : "high",
      provenance: "explicit",
    },
  };
}

function chooseThemedOption(testId: string, optionName: string) {
  fireEvent.click(screen.getByTestId(testId));
  fireEvent.click(screen.getByRole("option", { name: optionName }));
}

function defaultProps(overrides: Partial<ChatPanelProps> = {}): ChatPanelProps {
  return {
    session: null,
    messages: [],
    isStreaming: false,
    timelineEntries: [],
    scopeType: "workspace",
    scopeRef: "workspace-1",
    scopeLabel: "Workspace",
    sessions: [],
    selectedAgentProvider: "claude_code",
    availableAgentProviders: ["claude_code"],
    onSelectAgentProvider: vi.fn(),
    setupError: null,
    composerDisabledReason: null,
    onSendMessage: vi.fn(),
    onStopStreaming: vi.fn(),
    onForkSession: vi.fn(),
    onSummarize: vi.fn(),
    onCreateSession: vi.fn(),
    onSelectSession: vi.fn(),
    ...overrides,
  };
}

function activeSession(
  overrides: Partial<ChatSessionDTO> = {},
): ChatSessionDTO {
  return {
    id: "session-1",
    workspaceId: "workspace-1",
    scopeType: "workspace",
    scopeRef: "workspace-1",
    model: null,
    agentProvider: "claude_code" as ChatAgentProvider,
    title: "Active session",
    summary: null,
    tags: [],
    messageCount: 0,
    totalInputTokens: 0,
    totalOutputTokens: 0,
    totalCostUsd: 0,
    forkedFromId: null,
    linkedCommits: [],
    createdAt: "2026-05-10T00:00:00Z",
    updatedAt: "2026-05-10T00:00:00Z",
    canSend: true,
    ...overrides,
  };
}

function chatMessage(overrides: Partial<ChatMessageDTO> = {}): ChatMessageDTO {
  return {
    id: "message-1",
    sessionId: "session-1",
    role: "assistant",
    content: "",
    artifacts: null,
    citations: null,
    inputTokens: null,
    outputTokens: null,
    status: "streaming",
    messageIndex: 0,
    createdAt: "2026-05-10T00:00:01Z",
    ...overrides,
  };
}

function timelineEntry(overrides: Partial<TimelineEntry> = {}): TimelineEntry {
  return {
    id: "trace-1",
    entryType: "output",
    content: "streamed content",
    metadata: {},
    sequence: 1,
    createdAt: "2026-05-10T00:00:02Z",
    ...overrides,
  };
}

describe("ChatPanel device authorization layout", () => {
  const authKey = ["codex-device-auth", "session-1"];
  const challenge = (): CodexAuthView => ({
    taskId: "task-1",
    attempt: 1,
    phase: "waiting",
    deadlineUnixMs: Date.now() + 300_000,
    verificationUrl: "https://auth.openai.com/codex/device",
    userCode: "TEST-12345",
    message: null,
  });

  async function setup(overrides: Partial<ChatPanelProps> = {}) {
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    const view = rtlRender(
      <QueryClientProvider client={client}>
        <ChatPanel
          {...defaultProps({
            session: activeSession({ agentProvider: "codex" }),
            canManageSessionInputs: true,
            ...overrides,
          })}
        />
      </QueryClientProvider>,
    );
    await waitFor(() =>
      expect(client.getQueryState(authKey)?.status).toBe("success"),
    );
    // Drain mount scrolling so it cannot mask missing auth-update scrolling.
    await act(async () => {
      await new Promise(requestAnimationFrame);
    });
    const list = screen.getByTestId("chat-messages");
    let height = 1_000;
    Object.defineProperty(list, "scrollHeight", {
      configurable: true,
      get: () => height,
    });
    Object.defineProperty(list, "clientHeight", {
      configurable: true,
      get: () => 400,
    });
    return {
      ...view,
      client,
      list,
      setHeight: (value: number) => {
        height = value;
      },
    };
  }

  it("replaces the full-height empty placeholder when first-send authorization arrives", async () => {
    const { client } = await setup();
    expect(screen.getByText("No messages yet")).toBeVisible();
    act(() => client.setQueryData(authKey, challenge()));
    expect(await screen.findByText("TEST-12345")).toBeVisible();
    expect(screen.queryByText("No messages yet")).not.toBeInTheDocument();
    act(() => client.setQueryData(authKey, null));
    expect(await screen.findByText("No messages yet")).toBeVisible();
  });

  it("keeps the latest content visible when authorization appears and grows asynchronously", async () => {
    const { client, list, setHeight } = await setup({
      messages: [
        chatMessage({ content: "Earlier answer", status: "complete" }),
      ],
    });
    list.scrollTop = 600;
    fireEvent.scroll(list);
    setHeight(1_060);
    act(() =>
      client.setQueryData(authKey, {
        ...challenge(),
        phase: "checking",
        userCode: null,
        verificationUrl: null,
      }),
    );
    await screen.findByText("Checking Codex sign-in…");
    expect(list.scrollTop).toBe(1_060);
    await act(async () => {
      await new Promise(requestAnimationFrame);
    });
    setHeight(1_320);
    act(() => client.setQueryData(authKey, challenge()));
    await screen.findByText("TEST-12345");
    expect(list.scrollTop).toBe(1_320);
  });

  it("does not pull a reader away from scrollback when authorization changes", async () => {
    const { client, list, setHeight } = await setup({
      messages: [
        chatMessage({ content: "Earlier answer", status: "complete" }),
      ],
    });
    list.scrollTop = 150;
    fireEvent.scroll(list);
    setHeight(1_060);
    act(() =>
      client.setQueryData(authKey, {
        ...challenge(),
        phase: "checking",
        userCode: null,
        verificationUrl: null,
      }),
    );
    await screen.findByText("Checking Codex sign-in…");
    expect(list.scrollTop).toBe(150);
    setHeight(1_320);
    act(() => client.setQueryData(authKey, challenge()));
    await screen.findByText("TEST-12345");
    expect(list.scrollTop).toBe(150);
  });

  it("removes the owner-only challenge when input authority is revoked", async () => {
    const { client, rerender } = await setup();
    act(() => client.setQueryData(authKey, challenge()));
    await screen.findByText("TEST-12345");
    rerender(
      <QueryClientProvider client={client}>
        <ChatPanel
          {...defaultProps({
            session: activeSession({ agentProvider: "codex" }),
            canManageSessionInputs: false,
          })}
        />
      </QueryClientProvider>,
    );
    expect(screen.queryByText("TEST-12345")).not.toBeInTheDocument();
    expect(screen.getByText("No messages yet")).toBeVisible();
    await waitFor(() => expect(client.getQueryData(authKey)).toBeUndefined());
  });

  it("does not carry authorization layout state into another session", async () => {
    const { client, rerender } = await setup();
    act(() => client.setQueryData(authKey, challenge()));
    await screen.findByText("TEST-12345");
    rerender(
      <QueryClientProvider client={client}>
        <ChatPanel
          {...defaultProps({
            session: activeSession({ id: "session-2", agentProvider: "codex" }),
            canManageSessionInputs: true,
          })}
        />
      </QueryClientProvider>,
    );
    expect(screen.queryByText("TEST-12345")).not.toBeInTheDocument();
    expect(screen.getByText("No messages yet")).toBeVisible();
  });
});

function codexRichTimelineEntries() {
  return [
    timelineEntry({
      id: "codex-reasoning",
      entryType: "thought",
      content: "I will inspect the focused test first.",
      metadata: {
        source: "provided_agent",
        provider: "codex",
        providerEvent: "provided_agent.thought",
        phase: "thought",
        sessionId: "session-1",
        messageId: "assistant-codex-details",
      },
      sequence: 1,
    }),
    timelineEntry({
      id: "codex-command-start",
      entryType: "tool_call",
      content: "Codex used command_execution",
      metadata: {
        source: "provided_agent",
        provider: "codex",
        providerEvent: "provided_agent.tool.started",
        phase: "tool_call",
        sessionId: "session-1",
        messageId: "assistant-codex-details",
        toolCallId: "item-command-1",
        toolName: "command_execution",
        input: {
          command: "cargo test -p g5d-worker codex_reasoning_summary",
          status: "completed",
        },
      },
      sequence: 2,
    }),
    timelineEntry({
      id: "codex-command-complete",
      entryType: "tool_result",
      content: "Codex completed command_execution",
      metadata: {
        source: "provided_agent",
        provider: "codex",
        providerEvent: "provided_agent.tool.completed",
        phase: "tool_result",
        sessionId: "session-1",
        messageId: "assistant-codex-details",
        toolCallId: "item-command-1",
        toolName: "command_execution",
        contentSnippet: "test result: ok",
        isError: false,
      },
      sequence: 3,
    }),
  ];
}

describe("ChatPanel composer seed (chat_panel_runtime D30)", () => {
  it("applies the seed text to the composer without sending", () => {
    const onSendMessage = vi.fn();
    render(
      <ChatPanel
        {...defaultProps({
          onSendMessage,
          composerSeed: { token: 1, text: "seeded explain prompt" },
        })}
      />,
    );

    expect(screen.getByTestId("chat-input")).toHaveValue(
      "seeded explain prompt",
    );
    expect(onSendMessage).not.toHaveBeenCalled();
  });

  it("pulses Send from the seed until the draft is sent or emptied (workbench g5.workbench.chat-actions)", async () => {
    const onSendMessage = vi.fn().mockResolvedValue(true);
    render(
      <ChatPanel
        {...defaultProps({
          onSendMessage,
          composerSeed: { token: 1, text: "seeded prompt" },
        })}
      />,
    );
    const send = screen.getByTestId("chat-panel-no-session-send");
    expect(send).toHaveAttribute("data-pulse", "true");
    expect(send).toHaveClass("g5-send-pulse");
    // Edits keep the pulse: the ask is to send.
    fireEvent.change(screen.getByTestId("chat-input"), {
      target: { value: "seeded prompt, edited" },
    });
    expect(send).toHaveAttribute("data-pulse", "true");
    // Emptying the draft ends it.
    fireEvent.change(screen.getByTestId("chat-input"), {
      target: { value: "" },
    });
    expect(send).not.toHaveAttribute("data-pulse");
    expect(send).not.toHaveClass("g5-send-pulse");

    // Sending ends it too.
    fireEvent.change(screen.getByTestId("chat-input"), {
      target: { value: "typed again" },
    });
    expect(send).not.toHaveAttribute("data-pulse");
    fireEvent.click(send);
    await waitFor(() => expect(onSendMessage).toHaveBeenCalledTimes(1));
    expect(
      screen.getByTestId("chat-panel-no-session-send"),
    ).not.toHaveAttribute("data-pulse");
  });

  it("keeps pulsing across a remount of the same draft (the fresh composer, Dock, Pop out)", () => {
    const draft = createChatComposerDraft();
    const seed = { token: 3, text: "seeded prompt" };
    const first = render(
      <ChatPanel {...defaultProps({ draft, composerSeed: seed })} />,
    );
    expect(screen.getByTestId("chat-panel-no-session-send")).toHaveAttribute(
      "data-pulse",
      "true",
    );
    first.unmount();
    // The remount finds the seed already applied to the draft: the text
    // and the pulse both come back from the draft.
    render(<ChatPanel {...defaultProps({ draft, composerSeed: seed })} />);
    expect(screen.getByTestId("chat-input")).toHaveValue("seeded prompt");
    expect(screen.getByTestId("chat-panel-no-session-send")).toHaveAttribute(
      "data-pulse",
      "true",
    );
  });

  it("is one-shot: an unchanged token never clobbers later user edits", () => {
    const seed = { token: 7, text: "seeded" };
    const { rerender } = render(
      <ChatPanel {...defaultProps({ composerSeed: seed })} />,
    );
    const input = screen.getByTestId("chat-input");
    expect(input).toHaveValue("seeded");

    fireEvent.change(input, { target: { value: "user edited draft" } });
    rerender(<ChatPanel {...defaultProps({ composerSeed: seed })} />);
    expect(screen.getByTestId("chat-input")).toHaveValue("user edited draft");

    rerender(
      <ChatPanel
        {...defaultProps({ composerSeed: { token: 8, text: "new seed" } })}
      />,
    );
    expect(screen.getByTestId("chat-input")).toHaveValue("new seed");
  });

  it("reports an emptied seeded draft with its applied token, once (ui g5.workbench.chat-actions)", () => {
    const onComposerSeedEmptied = vi.fn();
    render(
      <ChatPanel
        {...defaultProps({
          composerSeed: { token: 4, text: "seeded prompt" },
          onComposerSeedEmptied,
        })}
      />,
    );
    const input = screen.getByTestId("chat-input");
    // Edits keep the seed.
    fireEvent.change(input, { target: { value: "seeded prompt, edited" } });
    expect(onComposerSeedEmptied).not.toHaveBeenCalled();
    // Emptying abandons it: the owner learns which seed, and the pulse ends.
    fireEvent.change(input, { target: { value: "   " } });
    expect(onComposerSeedEmptied).toHaveBeenCalledWith(4);
    expect(
      screen.getByTestId("chat-panel-no-session-send"),
    ).not.toHaveAttribute("data-pulse");
    // An ordinary draft emptied afterwards is no seed.
    fireEvent.change(input, { target: { value: "my own words" } });
    fireEvent.change(input, { target: { value: "" } });
    expect(onComposerSeedEmptied).toHaveBeenCalledTimes(1);
  });

  it("never restores a failed send's draft over a seed applied meanwhile", async () => {
    let settle!: (accepted: boolean) => void;
    const onSendMessage = vi.fn().mockReturnValue(
      new Promise<boolean>((resolve) => {
        settle = resolve;
      }),
    );
    const props = defaultProps({
      session: activeSession(),
      sessions: [activeSession()],
      onSendMessage,
      composerSeed: { token: 1, text: "first seed" },
    });
    const { rerender } = render(<ChatPanel {...props} />);
    const input = screen.getByTestId("chat-input");
    expect(input).toHaveValue("first seed");
    fireEvent.keyDown(input, { key: "Enter" });
    expect(onSendMessage).toHaveBeenCalledTimes(1);
    expect(input).toHaveValue("");

    // A chat action seeds this open chat while the send is in flight: the
    // same chat, so no send generation advances.
    rerender(
      <ChatPanel {...props} composerSeed={{ token: 2, text: "second seed" }} />,
    );
    expect(screen.getByTestId("chat-input")).toHaveValue("second seed");

    await act(async () => {
      settle(false);
    });
    // The refusal restores nothing: the newer seed owns the composer.
    expect(screen.getByTestId("chat-input")).toHaveValue("second seed");
    expect(screen.getByTestId("chat-panel-send")).toHaveAttribute(
      "data-pulse",
      "true",
    );
  });
});

describe("ChatPanel composer seed header (ui g5.workbench.review-prompts)", () => {
  const reviewSeed = () => ({ token: 1, ...reviewWorkspaceSeed() });

  it("offers the review loop at the top of the fresh chat and rewrites the count in the draft", () => {
    render(<ChatPanel {...defaultProps({ composerSeed: reviewSeed() })} />);
    expect(screen.getByTestId("composer-seed-review-loop")).toHaveTextContent(
      "Review in a loop with",
    );
    expect(screen.getByTestId("composer-seed-review-loop")).toHaveTextContent(
      "More iterations give stronger coverage",
    );
    const iterations = screen.getByTestId("composer-seed-review-iterations");
    expect(iterations).toHaveValue("1");
    expect(screen.getByTestId("composer-seed-review-loop")).toHaveTextContent(
      /10iteration(?!s)/,
    );
    expect(screen.getByTestId("chat-input")).toHaveValue(
      reviewWorkspaceChatPrompt(1),
    );

    // An edit elsewhere in the draft survives a count change.
    fireEvent.change(screen.getByTestId("chat-input"), {
      target: { value: `Start with the API. ${reviewWorkspaceChatPrompt(1)}` },
    });
    fireEvent.change(iterations, { target: { value: "3" } });
    expect(iterations).toHaveValue("3");
    expect(screen.getByTestId("composer-seed-review-loop")).toHaveTextContent(
      /10iterations/,
    );
    expect(screen.getByTestId("chat-input")).toHaveValue(
      `Start with the API. ${reviewWorkspaceChatPrompt(3)}`,
    );
  });

  it("places the header above the landing in the center layout", () => {
    render(
      <ChatPanel
        {...defaultProps({ layout: "center", composerSeed: reviewSeed() })}
      />,
    );
    const header = screen.getByTestId("composer-seed-review-loop");
    const landing = screen.getByText("Your workspace is ready");
    expect(
      header.compareDocumentPosition(landing) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });

  it("shows the bold cost notice for Re-run Verification", () => {
    render(
      <ChatPanel
        {...defaultProps({
          composerSeed: { token: 2, ...verificationSeed() },
        })}
      />,
    );
    const notice = screen.getByTestId("composer-seed-verification-notice");
    expect(notice).toHaveTextContent(VERIFICATION_NOTICE);
    expect(notice).toHaveClass("font-semibold");
    expect(screen.getByTestId("chat-input")).toHaveValue(
      verificationChatPrompt(),
    );
    expect(screen.queryByTestId("composer-seed-review-loop")).toBeNull();
  });

  it("leaves with the seed: emptying the draft removes it", () => {
    render(<ChatPanel {...defaultProps({ composerSeed: reviewSeed() })} />);
    fireEvent.change(screen.getByTestId("chat-input"), {
      target: { value: "" },
    });
    expect(screen.queryByTestId("composer-seed-review-loop")).toBeNull();
  });

  it("leaves on send and comes back with the draft when the send is refused", async () => {
    const onSendMessage = vi
      .fn()
      .mockResolvedValueOnce(false)
      .mockResolvedValueOnce(true);
    render(
      <ChatPanel
        {...defaultProps({ onSendMessage, composerSeed: reviewSeed() })}
      />,
    );
    fireEvent.change(screen.getByTestId("composer-seed-review-iterations"), {
      target: { value: "2" },
    });
    fireEvent.click(screen.getByTestId("chat-panel-no-session-send"));
    await waitFor(() => expect(onSendMessage).toHaveBeenCalledTimes(1));
    expect(onSendMessage.mock.calls[0][0]).toBe(reviewWorkspaceChatPrompt(2));
    // Refused: the draft, its header and its pulse are back.
    await waitFor(() =>
      expect(screen.getByTestId("chat-input")).toHaveValue(
        reviewWorkspaceChatPrompt(2),
      ),
    );
    expect(screen.getByTestId("composer-seed-review-iterations")).toHaveValue(
      "2",
    );
    expect(screen.getByTestId("chat-panel-no-session-send")).toHaveAttribute(
      "data-pulse",
      "true",
    );

    fireEvent.click(screen.getByTestId("chat-panel-no-session-send"));
    await waitFor(() => expect(onSendMessage).toHaveBeenCalledTimes(2));
    await waitFor(() =>
      expect(screen.queryByTestId("composer-seed-review-loop")).toBeNull(),
    );
  });

  it("survives a remount of the same draft (Dock, Pop out)", () => {
    const draft = createChatComposerDraft();
    const seed = reviewSeed();
    const first = render(
      <ChatPanel {...defaultProps({ draft, composerSeed: seed })} />,
    );
    fireEvent.change(screen.getByTestId("composer-seed-review-iterations"), {
      target: { value: "4" },
    });
    first.unmount();
    render(<ChatPanel {...defaultProps({ draft, composerSeed: seed })} />);
    expect(screen.getByTestId("composer-seed-review-iterations")).toHaveValue(
      "4",
    );
    expect(screen.getByTestId("chat-input")).toHaveValue(
      reviewWorkspaceChatPrompt(4),
    );
  });

  it("shows nothing for a seed without a header", () => {
    render(
      <ChatPanel
        {...defaultProps({ composerSeed: { token: 5, text: "Explain x" } })}
      />,
    );
    expect(screen.queryByTestId("composer-seed-review-loop")).toBeNull();
    expect(
      screen.queryByTestId("composer-seed-verification-notice"),
    ).toBeNull();
  });
});

describe("ChatPanel composer placeholder", () => {
  it("shows the caller's placeholder on an empty composer, but a disabled reason first", () => {
    const { rerender } = render(
      <ChatPanel
        {...defaultProps({
          layout: "center",
          composerPlaceholder: "What's your idea?",
        })}
      />,
    );
    expect(screen.getByTestId("chat-input")).toHaveAttribute(
      "placeholder",
      "What's your idea?",
    );
    rerender(
      <ChatPanel
        {...defaultProps({
          composerPlaceholder: "What's your idea?",
        })}
      />,
    );
    expect(screen.getByTestId("chat-input")).toHaveAttribute(
      "placeholder",
      "What's your idea?",
    );
    rerender(
      <ChatPanel
        {...defaultProps({
          composerPlaceholder: "What's your idea?",
          composerDisabledReason: "Start the workspace to chat.",
        })}
      />,
    );
    expect(screen.getByTestId("chat-input")).toHaveAttribute(
      "placeholder",
      "Start the workspace to chat.",
    );
  });
});

describe("ChatPanel center layout", () => {
  it("renders the Option 1 welcome hierarchy around the new-chat composer", () => {
    const onSelectSession = vi.fn();
    render(
      <ChatPanel
        {...defaultProps({
          layout: "center",
          onSelectSession,
          sessions: [
            activeSession({
              id: "recent-session",
              title: "Investigate the sync task",
            }),
          ],
        })}
      />,
    );

    const composer = screen.getByTestId("chat-center-composer");
    expect(composer).toHaveClass("w-full");
    expect(screen.getByText("Your workspace is ready")).toBeInTheDocument();
    expect(
      screen.getByRole("heading", { name: "Start with a conversation." }),
    ).toBeInTheDocument();
    expect(screen.getByTestId("chat-input")).toHaveAttribute("rows", "4");
    expect(screen.getByTestId("chat-input")).toHaveAttribute(
      "placeholder",
      "Ask anything about this workspace, or describe a change…",
    );
    expect(screen.queryByTestId("chat-history-header")).not.toBeInTheDocument();
    expect(screen.getByText("Recent conversations")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Plan a feature" }));
    expect(screen.getByTestId("chat-input")).toHaveValue(
      "Design and scope a new capability before we build it.",
    );

    fireEvent.click(screen.getByText("Investigate the sync task"));
    expect(onSelectSession).toHaveBeenCalledWith("recent-session");
  });
});

describe("ChatPanel provider selection", () => {
  it("hides the start-locked picker when only one provider is available (codex_support D23, preserving the D20 single-provider behavior)", () => {
    render(
      <ChatPanel
        {...defaultProps({
          selectedAgentProvider: "claude_code",
          availableAgentProviders: ["claude_code"],
        })}
      />,
    );

    expect(screen.getByText("Start Chat")).toBeInTheDocument();
    // With a single advertised provider the empty new chat renders no provider
    // cards — byte-identical to the pre-Codex Simplified New Chat (D20).
    expect(
      screen.queryByTestId("provider-choice-cards"),
    ).not.toBeInTheDocument();
    expect(screen.queryByTestId("chat-agent-provider")).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("provider-card-claude_code"),
    ).not.toBeInTheDocument();
    expect(screen.getByTestId("claude-code-controls")).toBeInTheDocument();
    expect(screen.getByTestId("claude-permission-control")).toHaveTextContent(
      /Bypass/i,
    );
  });

  it("offers no composer cards row — the settings gear is the sole harness selector (#1370)", () => {
    render(
      <ChatPanel
        {...defaultProps({
          selectedAgentProvider: "claude_code",
          availableAgentProviders: ["claude_code", "codex"],
        })}
      />,
    );

    // The former "AGENT:" cards row is removed; the displayed provider stays
    // exposed for tests on the options row, and the popup carries the picker.
    expect(screen.queryByTestId("chat-agent-provider")).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("provider-choice-cards"),
    ).not.toBeInTheDocument();
    expect(screen.getByTestId("chat-agent-options-row")).toHaveAttribute(
      "data-selected-provider",
      "claude_code",
    );
    expect(screen.getByTestId("claude-code-controls")).toBeInTheDocument();
    expect(screen.queryByTestId("codex-controls")).not.toBeInTheDocument();
  });

  it("keeps an explicit harness choice when the footerless popup closes (#1370/#1926)", () => {
    const onSelectAgentProvider = vi.fn();
    render(
      <ChatPanel
        {...defaultProps({
          selectedAgentProvider: "claude_code",
          availableAgentProviders: ["claude_code", "codex"],
          onSelectAgentProvider,
        })}
      />,
    );

    fireEvent.click(screen.getByTestId("claude-settings-control"));
    chooseThemedOption("chat-agent-select", "Codex");
    expect(onSelectAgentProvider).toHaveBeenCalledWith("codex");
    // The shared open state survives the harness switch — the popup must NOT
    // close just because the provider block swapped.
    expect(screen.getByTestId("codex-settings-modal")).toBeInTheDocument();
    expect(
      screen.queryByTestId("chat-provider-settings-actions"),
    ).not.toBeInTheDocument();
    fireEvent.click(screen.getByTestId("codex-settings-control"));
    expect(
      screen.queryByTestId("codex-settings-modal"),
    ).not.toBeInTheDocument();
    expect(screen.getByTestId("chat-agent-options-row")).toHaveAttribute(
      "data-selected-provider",
      "codex",
    );
  });

  it("shows direct configured values and outside dismissal keeps the latest choice", () => {
    render(
      <ChatPanel
        {...defaultProps({
          selectedAgentProvider: "claude_code",
          availableAgentProviders: ["claude_code", "codex"],
        })}
      />,
    );

    expect(screen.getByTestId("claude-model-control")).toHaveTextContent(
      "Opus 5.5",
    );
    fireEvent.click(screen.getByTestId("claude-settings-control"));
    expect(
      screen.queryByTestId("chat-launch-source-select"),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("chat-resolution-summary"),
    ).not.toBeInTheDocument();
    expect(screen.getByTestId("chat-launch-controls")).toBeInTheDocument();
    expect(screen.getByTestId("chat-agent-select")).toBeEnabled();

    chooseThemedOption("chat-model-select", "Fable 5.2");
    expect(screen.getByTestId("claude-model-control")).toHaveTextContent(
      "Fable 5.2",
    );

    expect(
      screen.queryByTestId("chat-provider-settings-cancel"),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("chat-provider-settings-apply"),
    ).not.toBeInTheDocument();
    fireEvent.pointerDown(document.body);
    expect(screen.getByTestId("claude-model-control")).toHaveTextContent(
      "Fable 5.2",
    );
    expect(
      screen.queryByTestId("claude-settings-modal"),
    ).not.toBeInTheDocument();
  });

  it("keeps a resolved model after outside dismissal and sends its exact confirmation", async () => {
    const resolveAgentLaunch = vi
      .fn()
      .mockImplementation((preferences: AgentLaunchPreferencesV1) =>
        Promise.resolve(
          explicitClaudeResolution(
            preferences,
            preferences.model.source === "explicit"
              ? `resolution-${preferences.model.value}`
              : "resolution-default",
          ),
        ),
      );
    const onSendMessage = vi.fn().mockResolvedValue(true);
    render(
      <ChatPanel
        {...defaultProps({
          resolveAgentLaunch,
          onSendMessage,
        })}
      />,
    );

    fireEvent.click(screen.getByTestId("claude-settings-control"));
    await waitFor(() =>
      expect(screen.getByTestId("chat-model-select")).toBeEnabled(),
    );
    chooseThemedOption("chat-model-select", "Fable 5");
    await waitFor(() =>
      expect(resolveAgentLaunch).toHaveBeenLastCalledWith(
        expect.objectContaining({
          model: { source: "explicit", value: "claude-fable-5" },
        }),
      ),
    );
    await waitFor(() =>
      expect(screen.getByTestId("chat-model-select")).toBeEnabled(),
    );

    fireEvent.pointerDown(document.body);
    expect(
      screen.queryByTestId("claude-settings-modal"),
    ).not.toBeInTheDocument();
    expect(screen.getByTestId("claude-model-control")).toHaveTextContent(
      "Fable 5",
    );

    fireEvent.change(screen.getByTestId("chat-input"), {
      target: { value: "use Fable" },
    });
    fireEvent.keyDown(screen.getByTestId("chat-input"), { key: "Enter" });

    await waitFor(() => expect(onSendMessage).toHaveBeenCalledTimes(1));
    expect(onSendMessage.mock.calls[0][1].agentLaunch).toEqual({
      preferences: expect.objectContaining({
        model: { source: "explicit", value: "claude-fable-5" },
      }),
      confirmedResolutionId: "resolution-claude-fable-5",
    });
  });

  it("keeps resolution alive after dismissal and gates Send while the latest choice is pending", async () => {
    let resolvePending: (resolution: ResolvedAgentExecutionV1) => void = () =>
      undefined;
    const pendingResolution = new Promise<ResolvedAgentExecutionV1>(
      (resolve) => {
        resolvePending = resolve;
      },
    );
    const resolveAgentLaunch = vi
      .fn()
      .mockImplementationOnce((preferences: AgentLaunchPreferencesV1) =>
        Promise.resolve(
          explicitClaudeResolution(preferences, "resolution-initial"),
        ),
      )
      .mockImplementationOnce(() => pendingResolution);
    const onSendMessage = vi.fn().mockResolvedValue(true);
    render(
      <ChatPanel
        {...defaultProps({
          resolveAgentLaunch,
          onSendMessage,
        })}
      />,
    );

    fireEvent.click(screen.getByTestId("claude-settings-control"));
    await waitFor(() =>
      expect(screen.getByTestId("chat-model-select")).toBeEnabled(),
    );
    chooseThemedOption("chat-model-select", "Fable 5");
    await waitFor(() => expect(resolveAgentLaunch).toHaveBeenCalledTimes(2));
    fireEvent.pointerDown(document.body);

    fireEvent.change(screen.getByTestId("chat-input"), {
      target: { value: "wait for the exact launch" },
    });
    expect(screen.getByTestId("chat-panel-no-session-send")).toBeDisabled();
    expect(screen.getByTestId("chat-resolution-loading")).toBeInTheDocument();
    expect(onSendMessage).not.toHaveBeenCalled();

    resolvePending(
      explicitClaudeResolution(
        {
          schemaVersion: 1,
          provider: { source: "explicit", value: "claude_code" },
          model: { source: "explicit", value: "claude-fable-5" },
          permissionMode: {
            source: "explicit",
            value: "bypassPermissions",
          },
          effort: { source: "explicit", value: "high" },
        },
        "resolution-pending-fable",
      ),
    );
    await waitFor(() =>
      expect(screen.getByTestId("chat-panel-no-session-send")).toBeEnabled(),
    );
    fireEvent.click(screen.getByTestId("chat-panel-no-session-send"));

    await waitFor(() => expect(onSendMessage).toHaveBeenCalledTimes(1));
    expect(onSendMessage.mock.calls[0][1].agentLaunch).toEqual(
      expect.objectContaining({
        confirmedResolutionId: "resolution-pending-fable",
      }),
    );
  });

  it("restores the last confirmed launch and keeps a failed choice actionable", async () => {
    const resolveAgentLaunch = vi
      .fn()
      .mockImplementationOnce((preferences: AgentLaunchPreferencesV1) =>
        Promise.resolve(
          explicitClaudeResolution(preferences, "resolution-initial"),
        ),
      )
      .mockRejectedValueOnce(new Error("Fable is unavailable"))
      .mockImplementationOnce((preferences: AgentLaunchPreferencesV1) =>
        Promise.resolve(
          explicitClaudeResolution(preferences, "resolution-restored"),
        ),
      );
    render(
      <ChatPanel
        {...defaultProps({
          resolveAgentLaunch,
        })}
      />,
    );

    await waitFor(() => expect(resolveAgentLaunch).toHaveBeenCalledTimes(1));
    fireEvent.click(screen.getByTestId("claude-settings-control"));
    await waitFor(() =>
      expect(screen.getByTestId("chat-model-select")).toBeEnabled(),
    );
    chooseThemedOption("chat-model-select", "Fable 5");
    fireEvent.pointerDown(document.body);

    expect(
      await screen.findByTestId("chat-resolution-error"),
    ).toHaveTextContent(/Fable is unavailable/i);
    await waitFor(() => expect(resolveAgentLaunch).toHaveBeenCalledTimes(3));
    await waitFor(() =>
      expect(screen.getByTestId("claude-model-control")).toHaveTextContent(
        "Opus 5.5",
      ),
    );
    expect(screen.getByTestId("chat-resolution-error")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Review settings" }));
    expect(screen.getByTestId("claude-settings-modal")).toBeInTheDocument();
    expect(screen.getByTestId("chat-model-select")).toHaveTextContent(
      "Opus 5.5",
    );
  });

  it("keeps a selected Codex draft populated with its saved values when the popup resolves", async () => {
    const resolveAgentLaunch = vi
      .fn()
      .mockImplementation((preferences) =>
        Promise.resolve(
          preferences.provider.source === "explicit" &&
            preferences.provider.value === "codex"
            ? EXPLICIT_CODEX_CHAT_RESOLUTION
            : INHERITED_CLAUDE_CHAT_RESOLUTION,
        ),
      );
    render(
      <ChatPanel
        {...defaultProps({
          session: null,
          selectedAgentProvider: "codex",
          availableAgentProviders: ["claude_code", "codex"],
          resolveAgentLaunch,

          defaultAgentProviderChoices: {
            claude_code: {
              model: "claude-haiku-4-5",
              mode: "plan",
              effort: "high",
            },
            codex: {
              model: "gpt-5.5",
              mode: "never/danger-full-access",
              effort: "medium",
            },
          },
        })}
      />,
    );

    fireEvent.click(screen.getByTestId("codex-settings-control"));
    expect(
      screen.queryByTestId("chat-launch-source-select"),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("chat-resolution-summary"),
    ).not.toBeInTheDocument();
    expect(screen.getByTestId("chat-launch-controls")).toBeInTheDocument();
    expect(screen.getByTestId("chat-agent-select")).toBeDisabled();
    await waitFor(() => expect(resolveAgentLaunch).toHaveBeenCalledTimes(1));
    expect(resolveAgentLaunch).toHaveBeenCalledWith(
      expect.objectContaining({
        provider: { source: "explicit", value: "codex" },
        model: { source: "explicit", value: "gpt-5.5" },
      }),
    );
    expect(screen.getByTestId("codex-settings-modal")).toBeInTheDocument();
    await waitFor(() =>
      expect(screen.getByTestId("chat-model-select")).toBeEnabled(),
    );
    expect(screen.getByTestId("chat-agent-select")).toBeEnabled();
    expect(screen.getByTestId("chat-agent-select")).toHaveTextContent("Codex");
    expect(screen.getByTestId("chat-model-select")).toHaveTextContent(
      "GPT 5.5",
    );
  });

  it("does not re-resolve when the displayed launch values are reselected", async () => {
    const resolveAgentLaunch = vi
      .fn()
      .mockResolvedValue(EXPLICIT_CODEX_CHAT_RESOLUTION);

    render(
      <ChatPanel
        {...defaultProps({
          session: null,
          selectedAgentProvider: "codex",
          availableAgentProviders: ["claude_code", "codex"],
          resolveAgentLaunch,

          defaultAgentProviderChoices: {
            claude_code: {
              model: "claude-opus-4-8[1m]",
              mode: "bypassPermissions",
              effort: "high",
            },
            codex: {
              model: "gpt-5.5",
              mode: "never/danger-full-access",
              effort: "medium",
            },
          },
        })}
      />,
    );

    await waitFor(() => expect(resolveAgentLaunch).toHaveBeenCalledTimes(1));
    fireEvent.click(screen.getByTestId("codex-settings-control"));
    expect(resolveAgentLaunch).toHaveBeenCalledTimes(1);

    chooseThemedOption("chat-agent-select", "Codex");
    chooseThemedOption("chat-model-select", "GPT 5.5");
    chooseThemedOption("chat-effort-select", "Medium");

    expect(resolveAgentLaunch).toHaveBeenCalledTimes(1);
    expect(
      screen.queryByTestId("chat-provider-settings-actions"),
    ).not.toBeInTheDocument();
  });

  it("unlocks a failed launch draft so editing it can resolve again", async () => {
    const resolveAgentLaunch = vi
      .fn()
      .mockRejectedValueOnce(new Error("No compatible route"))
      .mockResolvedValueOnce(EXPLICIT_CODEX_CHAT_RESOLUTION);

    render(
      <ChatPanel
        {...defaultProps({
          session: null,
          selectedAgentProvider: "codex",
          availableAgentProviders: ["claude_code", "codex"],
          resolveAgentLaunch,
        })}
      />,
    );

    fireEvent.click(screen.getByTestId("codex-settings-control"));
    expect(
      await screen.findByTestId("chat-resolution-error"),
    ).toHaveTextContent(/No compatible route/i);
    expect(screen.getByTestId("chat-agent-select")).toBeEnabled();
    expect(screen.getByTestId("chat-model-select")).toBeEnabled();
    expect(screen.getByTestId("chat-effort-select")).toBeEnabled();
    expect(screen.getByTestId("chat-mode-select")).toHaveAttribute("readonly");
    expect(screen.getByTestId("chat-mode-select")).toBeEnabled();

    chooseThemedOption("chat-model-select", "GPT 5.5");

    await waitFor(() => expect(resolveAgentLaunch).toHaveBeenCalledTimes(2));
    await waitFor(() =>
      expect(
        screen.queryByTestId("chat-resolution-error"),
      ).not.toBeInTheDocument(),
    );
    expect(
      screen.queryByTestId("chat-provider-settings-actions"),
    ).not.toBeInTheDocument();
  });

  it("keeps the launch popup footerless with direct controls visible", async () => {
    const resolveAgentLaunch = vi
      .fn()
      .mockResolvedValue(CONFIGURED_CHAT_RESOLUTION);
    render(
      <ChatPanel
        {...defaultProps({
          session: null,
          selectedAgentProvider: "claude_code",
          availableAgentProviders: ["claude_code", "codex"],
          resolveAgentLaunch,
        })}
      />,
    );

    fireEvent.click(screen.getByTestId("claude-settings-control"));
    await waitFor(() => expect(resolveAgentLaunch).toHaveBeenCalledTimes(1));
    const body = screen.getByTestId("chat-provider-settings-body");
    expect(
      screen.queryByTestId("chat-provider-settings-actions"),
    ).not.toBeInTheDocument();
    expect(
      within(body).queryByRole("button", { name: /apply|cancel/i }),
    ).not.toBeInTheDocument();
    expect(screen.getByTestId("chat-launch-controls")).toBeInTheDocument();
  });

  it("sends the exact launch selection confirmed automatically", async () => {
    const resolveAgentLaunch = vi
      .fn()
      .mockResolvedValue(CONFIGURED_CHAT_RESOLUTION);
    const onSendMessage = vi.fn().mockResolvedValue(true);
    render(
      <ChatPanel
        {...defaultProps({
          resolveAgentLaunch,
          onSendMessage,
        })}
      />,
    );

    await waitFor(() => expect(resolveAgentLaunch).toHaveBeenCalledTimes(1));
    fireEvent.change(screen.getByTestId("chat-input"), {
      target: { value: "use the confirmed launch" },
    });
    fireEvent.keyDown(screen.getByTestId("chat-input"), { key: "Enter" });

    await waitFor(() => expect(onSendMessage).toHaveBeenCalledTimes(1));
    expect(onSendMessage.mock.calls[0][1].agentLaunch).toEqual({
      preferences: expect.objectContaining({ schemaVersion: 1 }),
      confirmedResolutionId: "chat-resolution",
    });
  });

  it("keeps the source selector absent when returning to a new chat", async () => {
    const { rerender } = render(
      <ChatPanel {...defaultProps({ session: null })} />,
    );

    fireEvent.click(screen.getByTestId("claude-settings-control"));
    expect(
      screen.queryByTestId("chat-launch-source-select"),
    ).not.toBeInTheDocument();
    rerender(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          sessions: [activeSession()],
        })}
      />,
    );
    rerender(<ChatPanel {...defaultProps({ session: null })} />);
    await waitFor(() =>
      expect(
        screen.queryByTestId("chat-launch-source-select"),
      ).not.toBeInTheDocument(),
    );
  });

  it("locks the picker once a session is bound (codex_support D8/AC-15)", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession({ agentProvider: "claude_code" }),
          sessions: [activeSession({ agentProvider: "claude_code" })],
          availableAgentProviders: ["claude_code", "codex"],
        })}
      />,
    );

    // A bound session renders no picker — provider is immutable mid-session.
    expect(screen.queryByTestId("chat-agent-provider")).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("provider-choice-cards"),
    ).not.toBeInTheDocument();
  });

  it("sends the concrete bundled Claude default model on a new chat", async () => {
    const onSendMessage = vi.fn().mockResolvedValue(true);
    render(<ChatPanel {...defaultProps({ onSendMessage })} />);

    fireEvent.change(screen.getByTestId("chat-input"), {
      target: { value: "hello" },
    });
    fireEvent.keyDown(screen.getByTestId("chat-input"), { key: "Enter" });

    await waitFor(() => expect(onSendMessage).toHaveBeenCalled());
    expect(onSendMessage.mock.calls[0][1]).toMatchObject({
      model: CLAUDE_CHAT_DEFAULT_MODEL,
      // #1370 alignment: blank effort resolves to the default model's
      // catalog defaultEffort — "high" for every Claude family since chat
      // intent 36046a83 — not the legacy flat xhigh.
      effort: "high",
    });
  });

  it("uses the saved chat default model when no session model is pinned", async () => {
    const onSendMessage = vi.fn().mockResolvedValue(true);
    render(
      <ChatPanel
        {...defaultProps({
          defaultClaudeModel: "claude-sonnet-4-6",
          onSendMessage,
        })}
      />,
    );

    fireEvent.change(screen.getByTestId("chat-input"), {
      target: { value: "use my default" },
    });
    fireEvent.keyDown(screen.getByTestId("chat-input"), { key: "Enter" });

    await waitFor(() => expect(onSendMessage).toHaveBeenCalled());
    expect(onSendMessage.mock.calls[0][1]).toMatchObject({
      model: "claude-sonnet-4-6",
      effort: "high",
    });
  });

  it("keeps session history reachable from the no-session start view", () => {
    const onSelectSession = vi.fn();
    render(
      <ChatPanel
        {...defaultProps({
          onSelectSession,
          sessions: [
            activeSession({
              id: "session-old",
              title: "Previous Claude session",
              summary: "Earlier work",
            }),
          ],
        })}
      />,
    );

    fireEvent.click(screen.getByTestId("chat-history-header"));
    expect(screen.getByText("Previous Claude session")).toBeInTheDocument();

    fireEvent.click(screen.getByText("Previous Claude session"));
    expect(onSelectSession).toHaveBeenCalledWith("session-old");
  });

  it("selects the current tab reference and sends it with the next message", async () => {
    const onSendMessage = vi.fn().mockResolvedValue(true);
    const currentReference = {
      id: "source_file:src/main.rs",
      kind: "source_file" as const,
      label: "main.rs",
      path: "src/main.rs",
    };

    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          sessions: [activeSession()],
          onSendMessage,
          projectReferences: [currentReference],
          activeProjectReference: currentReference,
        })}
      />,
    );

    fireEvent.click(screen.getByTestId("chat-add-reference-button"));
    fireEvent.click(screen.getByTestId("chat-current-reference-option"));

    expect(screen.getByTestId("chat-reference-tray")).toHaveTextContent(
      "main.rs",
    );

    fireEvent.click(screen.getByTitle("Send (Enter)"));

    await waitFor(() =>
      expect(onSendMessage).toHaveBeenCalledWith(
        "Referenced main.rs",
        expect.objectContaining({ projectReferences: [currentReference] }),
      ),
    );
    await waitFor(() =>
      expect(screen.queryByText("main.rs")).not.toBeInTheDocument(),
    );
  });

  it("keeps the project reference button centered in the compact composer", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          sessions: [activeSession()],
        })}
      />,
    );

    const referenceButton = screen.getByTestId("chat-add-reference-button");
    expect(referenceButton).toHaveClass(
      "h-7",
      "w-7",
      "items-center",
      "justify-center",
    );
    expect(referenceButton.parentElement).toHaveClass("items-center");
    expect(screen.getByTestId("chat-input")).toHaveAttribute("rows", "1");
  });

  it("searches workspace references from the composer + menu", async () => {
    const onSendMessage = vi.fn().mockResolvedValue(true);
    const references = [
      {
        id: "project_doc:doc-1",
        kind: "project_doc" as const,
        label: "Architecture Notes",
        path: "docs/architecture.md",
        docId: "doc-1",
      },
      {
        id: "source_file:chat_service",
        kind: "source_file" as const,
        label: "Chat Service",
        path: "generation/.semlift/specs/g5/g5-api/services/chat_service.semlift.md",
      },
    ];

    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          sessions: [activeSession()],
          onSendMessage,
          projectReferences: references,
        })}
      />,
    );

    fireEvent.click(screen.getByTestId("chat-add-reference-button"));
    fireEvent.change(screen.getByTestId("chat-reference-search"), {
      target: { value: "chat" },
    });

    expect(screen.queryByText("Architecture Notes")).not.toBeInTheDocument();
    fireEvent.click(screen.getByText("Chat Service"));
    fireEvent.click(screen.getByTitle("Send (Enter)"));

    await waitFor(() =>
      expect(onSendMessage).toHaveBeenCalledWith(
        "Referenced Chat Service",
        expect.objectContaining({ projectReferences: [references[1]] }),
      ),
    );
  });

  it("names the chat in the header and renames it from the header pencil (g5.chat.ui.header-title)", async () => {
    const onRenameSession = vi.fn().mockResolvedValue(undefined);
    const createdAt = new Date(Date.now() - 5 * 60 * 1000).toISOString();
    const title = "A long chat title that the header shows in full on hover";
    const session = activeSession({ title, createdAt });

    render(
      <ChatPanel
        {...defaultProps({
          session,
          sessions: [session],
          onRenameSession,
        })}
      />,
    );

    expect(screen.getByText("Started 5m ago")).toBeInTheDocument();
    expect(screen.getByTestId("chat-active-history")).toHaveAttribute(
      "title",
      title,
    );
    expect(screen.getByTestId("chat-active-history").className).toContain(
      "max-w-full",
    );

    fireEvent.click(screen.getByRole("button", { name: "Rename chat" }));
    const input = screen.getByTestId("chat-header-title-input");
    expect(input).toHaveValue(title);
    fireEvent.change(input, { target: { value: "  Renamed title  " } });
    fireEvent.keyDown(input, { key: "Enter" });

    await waitFor(() =>
      expect(onRenameSession).toHaveBeenCalledWith(
        "session-1",
        "Renamed title",
      ),
    );
    await waitFor(() =>
      expect(
        screen.queryByTestId("chat-header-title-input"),
      ).not.toBeInTheDocument(),
    );
  });

  it("cancels a header rename with Escape and clears a title to null", async () => {
    const onRenameSession = vi.fn().mockResolvedValue(undefined);
    const session = activeSession({ title: "Keep me" });
    render(
      <ChatPanel
        {...defaultProps({ session, sessions: [session], onRenameSession })}
      />,
    );

    fireEvent.click(screen.getByTestId("chat-header-rename"));
    const input = screen.getByTestId("chat-header-title-input");
    fireEvent.change(input, { target: { value: "Discarded" } });
    fireEvent.keyDown(input, { key: "Escape" });
    expect(
      screen.queryByTestId("chat-header-title-input"),
    ).not.toBeInTheDocument();
    expect(onRenameSession).not.toHaveBeenCalled();

    fireEvent.click(screen.getByTestId("chat-header-rename"));
    fireEvent.change(screen.getByTestId("chat-header-title-input"), {
      target: { value: "   " },
    });
    fireEvent.click(screen.getByTestId("chat-header-title-save"));
    await waitFor(() =>
      expect(onRenameSession).toHaveBeenCalledWith("session-1", null),
    );
  });

  it("keeps the header editor open and explains a failed rename", async () => {
    let reject!: (error: Error) => void;
    const onRenameSession = vi.fn(
      () => new Promise<void>((_, rejectRename) => (reject = rejectRename)),
    );
    const session = activeSession({ title: "Original" });
    render(
      <ChatPanel
        {...defaultProps({ session, sessions: [session], onRenameSession })}
      />,
    );

    fireEvent.click(screen.getByTestId("chat-header-rename"));
    const input = screen.getByTestId("chat-header-title-input");
    input.focus();
    fireEvent.change(input, { target: { value: "Refused" } });
    fireEvent.keyDown(input, { key: "Enter" });
    expect(screen.getByTestId("chat-header-title-save")).toHaveTextContent(
      "Saving…",
    );
    // Read-only while saving, so keyboard focus stays for a retry.
    expect(input).toHaveAttribute("readonly");
    expect(input).toHaveAttribute("aria-busy", "true");
    expect(input).not.toBeDisabled();
    expect(input).toHaveFocus();
    await act(async () => reject(new Error("Workspace is read-only")));

    expect(screen.getByRole("alert")).toHaveTextContent(
      "Could not rename: Workspace is read-only",
    );
    expect(screen.getByTestId("chat-header-title-input")).toHaveValue(
      "Refused",
    );
    expect(screen.getByTestId("chat-header-title-input")).not.toHaveAttribute(
      "readonly",
    );
    expect(screen.getByTestId("chat-header-title-input")).toHaveFocus();
  });

  it("offers no rename to a reader", () => {
    const session = activeSession({ title: "Read only" });
    render(<ChatPanel {...defaultProps({ session, sessions: [session] })} />);
    expect(screen.queryByTestId("chat-header-rename")).not.toBeInTheDocument();
  });

  it("lists chats with activity and times but no rename controls (g5.chat.ui.picker)", () => {
    const nowMs = Date.now();
    const ago = (minutes: number) =>
      new Date(nowMs - minutes * 60 * 1000).toISOString();
    const running = activeSession({
      id: "running",
      title: "Running chat",
      createdAt: ago(120),
      activity: {
        activeTurnStartedAt: ago(1),
        lastFinishedAt: ago(30),
        latestResult: { taskId: "t-old", attempt: 1, hasOutput: true },
        unseenResult: true,
      },
    });
    const unseen = activeSession({
      id: "unseen",
      title: "Finished chat",
      createdAt: ago(120),
      activity: {
        activeTurnStartedAt: null,
        lastFinishedAt: ago(5),
        latestResult: { taskId: "t-1", attempt: 1, hasOutput: true },
        unseenResult: true,
      },
    });
    const idle = activeSession({
      id: "idle",
      title: "Quiet chat",
      createdAt: ago(60 * 24 * 2),
      activity: {
        activeTurnStartedAt: null,
        lastFinishedAt: null,
        latestResult: null,
        unseenResult: false,
      },
    });
    const onRenameSession = vi.fn();
    render(
      <ChatPanel
        {...defaultProps({
          session: unseen,
          sessions: [running, unseen, idle],
          onRenameSession,
        })}
      />,
    );

    fireEvent.click(screen.getByTestId("chat-active-history"));
    const rows = screen.getAllByTestId("chat-session-select");
    expect(rows.map((row) => row.textContent)).toEqual([
      "Running chatStarted 2h ago · Active now",
      "Finished chatStarted 2h ago · Last active 5m ago",
      "Quiet chatStarted 2d ago · No finished turns",
    ]);
    const states = rows.map((row) =>
      within(row)
        .getByTestId("chat-activity-indicator")
        .getAttribute("data-state"),
    );
    // The spinner takes precedence over the running chat's earlier result.
    expect(states).toEqual(["active", "unseen", "idle"]);
    expect(
      within(rows[0]).getByRole("img", { name: "Turn running" }),
    ).toBeInTheDocument();
    expect(
      within(rows[1]).getByRole("img", {
        name: "New result you have not viewed",
      }),
    ).toHaveAttribute("title", "New result you have not viewed");
    expect(within(rows[2]).queryByRole("img")).not.toBeInTheDocument();
    expect(rows[1]).toHaveAttribute("aria-current", "true");
    expect(rows[1].className).toContain("border-l-[var(--g5-info)]");
    expect(screen.queryByTestId(/^rename-session-/)).not.toBeInTheDocument();
    expect(
      screen.queryByTestId(/^session-title-input-/),
    ).not.toBeInTheDocument();

    fireEvent.change(screen.getByPlaceholderText("Search sessions…"), {
      target: { value: "quiet" },
    });
    expect(
      screen
        .getAllByTestId("chat-session-select")
        .map((row) => row.textContent),
    ).toEqual(["Quiet chatStarted 2d ago · No finished turns"]);
  });

  it("keeps active sessions provider-pinned in the compact header selector", () => {
    const session: ChatSessionDTO = {
      id: "session-1",
      workspaceId: "workspace-1",
      scopeType: "workspace",
      scopeRef: "workspace-1",
      model: null,
      agentProvider: "claude_code" as ChatAgentProvider,
      title: "Pinned Claude session",
      summary: null,
      tags: [],
      messageCount: 0,
      totalInputTokens: 0,
      totalOutputTokens: 0,
      totalCostUsd: 0,
      forkedFromId: null,
      linkedCommits: [],
      createdAt: "2026-05-10T00:00:00Z",
      updatedAt: "2026-05-10T00:00:00Z",
      canSend: true,
    };

    render(<ChatPanel {...defaultProps({ session })} />);

    expect(
      screen.queryByTestId("provider-choice-cards"),
    ).not.toBeInTheDocument();
    expect(screen.queryByTestId("chat-agent-provider")).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("chat-agent-provider-pinned"),
    ).not.toBeInTheDocument();
    expect(screen.getByTestId("claude-code-controls")).toBeInTheDocument();
  });

  it("hides finish sync chat outside waiting post-sync continuation sessions", () => {
    const session = activeSession({
      id: "ordinary-task-session",
      scopeType: "task",
      scopeRef: "task-1",
    });

    render(<ChatPanel {...defaultProps({ session })} />);

    expect(screen.queryByTestId("finish-sync-chat")).toBeNull();
  });

  it("labels task-scoped sync session panels as sync", () => {
    const session = activeSession({
      id: "task-session",
      scopeType: "task",
      scopeRef: "task-1",
      title: "Sync receiver management",
    });

    render(
      <ChatPanel
        {...defaultProps({
          session,
          scopeType: "task",
          scopeRef: "task-1",
          scopeLabel: "Sync receiver management",
        })}
      />,
    );

    expect(screen.getByText("Sync")).toBeInTheDocument();
  });

  it("labels task-scoped merge session panels as merge", () => {
    const session = activeSession({
      id: "merge-session",
      scopeType: "task",
      scopeRef: "task-merge",
      title: "Merge Sync receiver management",
    });

    render(
      <ChatPanel
        {...defaultProps({
          session,
          scopeType: "task",
          scopeRef: "task-merge",
          scopeLabel: "Merge Sync receiver management",
        })}
      />,
    );

    expect(screen.getByText("Merge")).toBeInTheDocument();
  });

  it("hides internal response delivery acknowledgements from active chat", () => {
    const session: ChatSessionDTO = {
      id: "session-ack",
      workspaceId: "workspace-1",
      scopeType: "workspace",
      scopeRef: "workspace-1",
      model: null,
      agentProvider: "claude_code" as ChatAgentProvider,
      title: "Pinned Claude session",
      summary: null,
      tags: [],
      messageCount: 1,
      totalInputTokens: 0,
      totalOutputTokens: 0,
      totalCostUsd: 0,
      forkedFromId: null,
      linkedCommits: [],
      createdAt: "2026-05-10T00:00:00Z",
      updatedAt: "2026-05-10T00:00:00Z",
      canSend: true,
    };

    render(
      <ChatPanel
        {...defaultProps({
          session,
          selectedAgentProvider: "claude_code",
          messages: [
            chatMessage({
              id: "message-1",
              sessionId: "session-ack",
              role: "assistant",
              content: "Hello from assistant.",
              messageIndex: 0,
              inputTokens: 0,
              outputTokens: 0,
              createdAt: "2026-05-10T00:00:01Z",
            }),
          ],
          timelineEntries: [
            timelineEntry({
              id: "trace-1",
              entryType: "tool_result",
              content: "Response sent (166 chars). Waiting for user input.",
              metadata: { phase: "tool_result", toolName: "respond_to_user" },
              sequence: 1,
              createdAt: "2026-05-10T00:00:00Z",
            }),
          ],
        })}
      />,
    );

    expect(screen.queryByText(/Response sent/)).not.toBeInTheDocument();
    expect(screen.getByText("Hello from assistant.")).toBeInTheDocument();
    expect(
      screen.queryByTestId("chat-agent-provider-pinned"),
    ).not.toBeInTheDocument();
  });
});

describe("ChatPanel file upload attachments (chat_panel_runtime I3)", () => {
  function uploadFile(
    name = "screenshot.png",
    type = "image/png",
    contents: string[] = ["fake-image-bytes"],
  ): File {
    return new File(contents, name, { type });
  }

  it("exposes the click-to-browse file input and an 'Upload a file' menu action", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          sessions: [activeSession()],
        })}
      />,
    );

    const input = screen.getByTestId("chat-attachment-input");
    expect(input).toBeInTheDocument();
    expect(input).toHaveAttribute("type", "file");
    expect(input).not.toHaveAttribute("accept");

    // The upload action lives inside the "+" menu alongside the reference picker.
    expect(
      screen.queryByTestId("chat-upload-file-action"),
    ).not.toBeInTheDocument();
    fireEvent.click(screen.getByTestId("chat-add-reference-button"));
    expect(screen.getByTestId("chat-upload-file-action")).toBeInTheDocument();
  });

  it("attaches a selected image as a removable upload chip and sends it inline", async () => {
    const onSendMessage = vi.fn().mockResolvedValue(true);
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          sessions: [activeSession()],
          onSendMessage,
        })}
      />,
    );

    fireEvent.change(screen.getByTestId("chat-attachment-input"), {
      target: { files: [uploadFile()] },
    });

    const chip = await screen.findByTestId("chat-attachment-chip");
    expect(chip).toHaveTextContent("screenshot.png");
    expect(chip.querySelector("img")).toBeInTheDocument();
    // Distinct from reference chips: no attachment error and no reference tray.
    expect(
      screen.queryByTestId("chat-attachment-error"),
    ).not.toBeInTheDocument();
    expect(screen.queryByTestId("chat-reference-tray")).not.toBeInTheDocument();

    fireEvent.click(screen.getByTitle("Send (Enter)"));

    await waitFor(() => expect(onSendMessage).toHaveBeenCalled());
    expect(onSendMessage).toHaveBeenCalledWith(
      "Uploaded screenshot.png",
      expect.objectContaining({
        attachments: [
          expect.objectContaining({
            fileName: "screenshot.png",
            contentType: "image/png",
            contentBase64: expect.stringMatching(/.+/),
          }),
        ],
      }),
    );

    // Chip clears after the turn is sent.
    await waitFor(() =>
      expect(
        screen.queryByTestId("chat-attachment-chip"),
      ).not.toBeInTheDocument(),
    );
  });

  it("removes an upload chip via its remove control before sending", async () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          sessions: [activeSession()],
        })}
      />,
    );

    fireEvent.change(screen.getByTestId("chat-attachment-input"), {
      target: { files: [uploadFile()] },
    });

    expect(
      await screen.findByTestId("chat-attachment-chip"),
    ).toBeInTheDocument();
    fireEvent.click(
      screen.getByRole("button", { name: "Remove screenshot.png" }),
    );
    expect(
      screen.queryByTestId("chat-attachment-chip"),
    ).not.toBeInTheDocument();
  });

  it("accepts a PDF as an opaque generic chip and sends it inline", async () => {
    const onSendMessage = vi.fn().mockResolvedValue(true);
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          sessions: [activeSession()],
          onSendMessage,
        })}
      />,
    );

    fireEvent.change(screen.getByTestId("chat-attachment-input"), {
      target: {
        files: [uploadFile("evidence.pdf", "application/pdf", ["%PDF-1.7"])],
      },
    });

    const chip = await screen.findByTestId("chat-attachment-chip");
    expect(chip).toHaveTextContent("evidence.pdf");
    expect(chip.querySelector("img")).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("chat-attachment-error"),
    ).not.toBeInTheDocument();

    fireEvent.click(screen.getByTitle("Send (Enter)"));
    await waitFor(() => expect(onSendMessage).toHaveBeenCalled());
    expect(onSendMessage).toHaveBeenCalledWith(
      "Uploaded evidence.pdf",
      expect.objectContaining({
        attachments: [
          expect.objectContaining({
            fileName: "evidence.pdf",
            contentType: "application/pdf",
          }),
        ],
      }),
    );
  });

  it("accepts an extensionless file with no MIME type", async () => {
    const onSendMessage = vi.fn().mockResolvedValue(true);
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          sessions: [activeSession()],
          onSendMessage,
        })}
      />,
    );

    fireEvent.change(screen.getByTestId("chat-attachment-input"), {
      target: { files: [uploadFile("artifact", "", ["binary-ish"])] },
    });

    const chip = await screen.findByTestId("chat-attachment-chip");
    expect(chip).toHaveTextContent("artifact");
    expect(chip.querySelector("img")).not.toBeInTheDocument();
    fireEvent.click(screen.getByTitle("Send (Enter)"));
    await waitFor(() => expect(onSendMessage).toHaveBeenCalled());
    expect(onSendMessage.mock.calls[0][1].attachments[0]).toEqual(
      expect.objectContaining({ fileName: "artifact", contentType: undefined }),
    );
  });

  it("does not preview SVG or HTML even when the MIME type is image-like or renderable", async () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          sessions: [activeSession()],
        })}
      />,
    );

    fireEvent.change(screen.getByTestId("chat-attachment-input"), {
      target: {
        files: [uploadFile("unsafe.svg", "image/svg+xml", ["<svg />"])],
      },
    });
    await waitFor(() =>
      expect(screen.getAllByTestId("chat-attachment-chip")).toHaveLength(1),
    );
    fireEvent.change(screen.getByTestId("chat-attachment-input"), {
      target: {
        files: [uploadFile("unsafe.html", "text/html", ["<script />"])],
      },
    });
    await waitFor(() =>
      expect(screen.getAllByTestId("chat-attachment-chip")).toHaveLength(2),
    );

    for (const chip of screen.getAllByTestId("chat-attachment-chip")) {
      expect(chip.querySelector("img")).not.toBeInTheDocument();
    }
  });

  it("rejects a file larger than 5 MiB with a visible error and no chip", async () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          sessions: [activeSession()],
        })}
      />,
    );

    const oversized = uploadFile("huge.bin", "application/octet-stream");
    Object.defineProperty(oversized, "size", { value: 6 * 1024 * 1024 });

    fireEvent.change(screen.getByTestId("chat-attachment-input"), {
      target: { files: [oversized] },
    });

    const error = await screen.findByTestId("chat-attachment-error");
    expect(error).toHaveTextContent(/5 MiB/i);
    expect(
      screen.queryByTestId("chat-attachment-chip"),
    ).not.toBeInTheDocument();
  });

  it("attaches multiple distinct files and sends them all inline", async () => {
    const onSendMessage = vi.fn().mockResolvedValue(true);
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          sessions: [activeSession()],
          onSendMessage,
        })}
      />,
    );

    fireEvent.change(screen.getByTestId("chat-attachment-input"), {
      target: { files: [uploadFile("one.png", "image/png", ["bytes-one"])] },
    });
    await waitFor(() =>
      expect(screen.getAllByTestId("chat-attachment-chip")).toHaveLength(1),
    );
    fireEvent.change(screen.getByTestId("chat-attachment-input"), {
      target: {
        files: [uploadFile("two.pdf", "application/pdf", ["bytes-two"])],
      },
    });
    await waitFor(() =>
      expect(screen.getAllByTestId("chat-attachment-chip")).toHaveLength(2),
    );

    fireEvent.click(screen.getByTitle("Send (Enter)"));
    await waitFor(() => expect(onSendMessage).toHaveBeenCalled());
    const options = onSendMessage.mock.calls[0][1];
    expect(options.attachments).toHaveLength(2);
    expect(
      options.attachments.map((a: { fileName: string }) => a.fileName),
    ).toEqual(["one.png", "two.pdf"]);
    expect(options.attachments[1].contentType).toBe("application/pdf");
  });

  it("clears session A's pending file before displaying or sending session B's file", async () => {
    const onSendMessage = vi.fn().mockResolvedValue(true);
    const sessionA = activeSession({ id: "session-a", title: "Session A" });
    const sessionB = activeSession({ id: "session-b", title: "Session B" });
    const { rerender } = render(
      <ChatPanel
        {...defaultProps({
          session: sessionA,
          sessions: [sessionA, sessionB],
          onSendMessage,
        })}
      />,
    );

    fireEvent.change(screen.getByTestId("chat-attachment-input"), {
      target: {
        files: [
          uploadFile("session-a.bin", "application/octet-stream", [
            "session-a-bytes",
          ]),
        ],
      },
    });
    expect(await screen.findByText("session-a.bin")).toBeInTheDocument();

    rerender(
      <ChatPanel
        {...defaultProps({
          session: sessionB,
          sessions: [sessionA, sessionB],
          onSendMessage,
        })}
      />,
    );
    await waitFor(() =>
      expect(screen.queryByText("session-a.bin")).not.toBeInTheDocument(),
    );

    fireEvent.change(screen.getByTestId("chat-attachment-input"), {
      target: {
        files: [
          uploadFile("session-b.bin", "application/octet-stream", [
            "session-b-bytes",
          ]),
        ],
      },
    });
    expect(await screen.findByText("session-b.bin")).toBeInTheDocument();
    expect(screen.queryByText("session-a.bin")).not.toBeInTheDocument();

    fireEvent.click(screen.getByTitle("Send (Enter)"));
    await waitFor(() => expect(onSendMessage).toHaveBeenCalledTimes(1));
    expect(
      onSendMessage.mock.calls[0][1].attachments.map(
        (attachment: { fileName: string }) => attachment.fileName,
      ),
    ).toEqual(["session-b.bin"]);
  });

  it("discards a session A file read that completes after switching to session B", async () => {
    const readSpy = vi
      .spyOn(FileReader.prototype, "readAsDataURL")
      .mockImplementation(() => undefined);
    const onSendMessage = vi.fn().mockResolvedValue(true);
    const sessionA = activeSession({ id: "session-a", title: "Session A" });
    const sessionB = activeSession({ id: "session-b", title: "Session B" });

    try {
      const { rerender } = render(
        <ChatPanel
          {...defaultProps({
            session: sessionA,
            sessions: [sessionA, sessionB],
            onSendMessage,
          })}
        />,
      );

      fireEvent.change(screen.getByTestId("chat-attachment-input"), {
        target: {
          files: [
            uploadFile("session-a.bin", "application/octet-stream", [
              "session-a-bytes",
            ]),
          ],
        },
      });
      const sessionAReader = readSpy.mock.contexts[0] as FileReader | undefined;
      expect(sessionAReader).toBeDefined();

      rerender(
        <ChatPanel
          {...defaultProps({
            session: sessionB,
            sessions: [sessionA, sessionB],
            onSendMessage,
          })}
        />,
      );

      fireEvent.change(screen.getByTestId("chat-attachment-input"), {
        target: {
          files: [
            uploadFile("session-b.bin", "application/octet-stream", [
              "session-b-bytes",
            ]),
          ],
        },
      });
      const sessionBReader = readSpy.mock.contexts[1] as FileReader | undefined;
      expect(sessionBReader).toBeDefined();

      Object.defineProperty(sessionBReader!, "result", {
        configurable: true,
        value: "data:application/octet-stream;base64,c2Vzc2lvbi1iLWJ5dGVz",
      });
      act(() => sessionBReader?.dispatchEvent(new Event("load")));
      expect(await screen.findByText("session-b.bin")).toBeInTheDocument();

      Object.defineProperty(sessionAReader!, "result", {
        configurable: true,
        value: "data:application/octet-stream;base64,c2Vzc2lvbi1hLWJ5dGVz",
      });
      act(() => sessionAReader?.dispatchEvent(new Event("load")));

      await waitFor(() => {
        expect(screen.queryByText("session-a.bin")).not.toBeInTheDocument();
        expect(screen.getAllByTestId("chat-attachment-chip")).toHaveLength(1);
      });
      fireEvent.click(screen.getByTitle("Send (Enter)"));
      await waitFor(() => expect(onSendMessage).toHaveBeenCalledTimes(1));
      expect(
        onSendMessage.mock.calls[0][1].attachments.map(
          (attachment: { fileName: string }) => attachment.fileName,
        ),
      ).toEqual(["session-b.bin"]);
    } finally {
      readSpy.mockRestore();
    }
  });

  it("de-dupes an identical file selected twice", async () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          sessions: [activeSession()],
        })}
      />,
    );

    const identical = () =>
      uploadFile("dup.bin", "application/octet-stream", ["identical-bytes"]);
    fireEvent.change(screen.getByTestId("chat-attachment-input"), {
      target: { files: [identical()] },
    });
    await waitFor(() =>
      expect(screen.getAllByTestId("chat-attachment-chip")).toHaveLength(1),
    );
    fireEvent.change(screen.getByTestId("chat-attachment-input"), {
      target: { files: [identical()] },
    });

    const error = await screen.findByTestId("chat-attachment-error");
    expect(error).toHaveTextContent(/already attached/i);
    expect(screen.getAllByTestId("chat-attachment-chip")).toHaveLength(1);
  });

  it("rejects an empty (0-byte) file with a visible error and no chip", async () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          sessions: [activeSession()],
        })}
      />,
    );

    const empty = new File([], "empty.png", { type: "image/png" });
    fireEvent.change(screen.getByTestId("chat-attachment-input"), {
      target: { files: [empty] },
    });

    const error = await screen.findByTestId("chat-attachment-error");
    expect(error).toHaveTextContent(/empty/i);
    expect(
      screen.queryByTestId("chat-attachment-chip"),
    ).not.toBeInTheDocument();
  });

  it("caps the number of files per message", async () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          sessions: [activeSession()],
        })}
      />,
    );

    for (let i = 0; i < 8; i++) {
      fireEvent.change(screen.getByTestId("chat-attachment-input"), {
        target: {
          files: [
            uploadFile(`s${i}.bin`, "application/octet-stream", [`bytes-${i}`]),
          ],
        },
      });
      await waitFor(() =>
        expect(screen.getAllByTestId("chat-attachment-chip")).toHaveLength(
          i + 1,
        ),
      );
    }

    fireEvent.change(screen.getByTestId("chat-attachment-input"), {
      target: {
        files: [uploadFile("s8.bin", "application/octet-stream", ["bytes-8"])],
      },
    });

    const error = await screen.findByTestId("chat-attachment-error");
    expect(error).toHaveTextContent(/at most 8/i);
    expect(screen.getAllByTestId("chat-attachment-chip")).toHaveLength(8);
  });

  it("keeps the attachment reference when the user also types text (D36)", async () => {
    const onSendMessage = vi.fn().mockResolvedValue(true);
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          sessions: [activeSession()],
          onSendMessage,
        })}
      />,
    );

    // Attach an image, then type instructions alongside it.
    fireEvent.change(screen.getByTestId("chat-attachment-input"), {
      target: { files: [uploadFile("screenshot.png", "image/png")] },
    });
    await screen.findByTestId("chat-attachment-chip");
    fireEvent.change(screen.getByTestId("chat-input"), {
      target: { value: "make this button blue" },
    });

    fireEvent.click(screen.getByTitle("Send (Enter)"));
    await waitFor(() => expect(onSendMessage).toHaveBeenCalled());

    const [content, options] = onSendMessage.mock.calls[0];
    // The typed instruction must survive verbatim...
    expect(content).toContain("make this button blue");
    // ...and the turn must still name the materialized file, otherwise the
    // agent is never told the image exists and silently ignores it.
    expect(content).toContain("screenshot.png");
    expect(options.attachments).toHaveLength(1);
  });

  it("attaches an image pasted from the clipboard and sends it inline (D37)", async () => {
    const onSendMessage = vi.fn().mockResolvedValue(true);
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          sessions: [activeSession()],
          onSendMessage,
        })}
      />,
    );

    const pasted = uploadFile("clip.png", "image/png", ["pasted-bytes"]);
    fireEvent.paste(screen.getByTestId("chat-input"), {
      clipboardData: {
        items: [{ kind: "file", type: "image/png", getAsFile: () => pasted }],
      },
    });

    const chip = await screen.findByTestId("chat-attachment-chip");
    expect(chip).toHaveTextContent("clip.png");
    expect(
      screen.queryByTestId("chat-attachment-error"),
    ).not.toBeInTheDocument();

    fireEvent.click(screen.getByTitle("Send (Enter)"));

    await waitFor(() => expect(onSendMessage).toHaveBeenCalled());
    expect(onSendMessage).toHaveBeenCalledWith(
      "Uploaded clip.png",
      expect.objectContaining({
        attachments: [
          expect.objectContaining({
            fileName: "clip.png",
            contentType: "image/png",
            contentBase64: expect.stringMatching(/.+/),
          }),
        ],
      }),
    );
  });

  it("gives a nameless pasted image blob a synthetic file name (D37)", async () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          sessions: [activeSession()],
        })}
      />,
    );

    const nameless = new File(["nameless-bytes"], "", { type: "image/png" });
    fireEvent.paste(screen.getByTestId("chat-input"), {
      clipboardData: {
        items: [{ kind: "file", type: "image/png", getAsFile: () => nameless }],
      },
    });

    const chip = await screen.findByTestId("chat-attachment-chip");
    expect(chip).toHaveTextContent("pasted-image.png");
  });

  it("leaves a text-only paste untouched (no attachment, no error)", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          sessions: [activeSession()],
        })}
      />,
    );

    fireEvent.paste(screen.getByTestId("chat-input"), {
      clipboardData: {
        items: [{ kind: "string", type: "text/plain", getAsFile: () => null }],
      },
    });

    expect(
      screen.queryByTestId("chat-attachment-chip"),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("chat-attachment-error"),
    ).not.toBeInTheDocument();
  });

  it("ignores a pasted non-image: no chip and no error (D37)", async () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          sessions: [activeSession()],
        })}
      />,
    );

    // A file item whose type is not an allowed image is ignored by the paste
    // filter (no image present → nothing consumed, no chip, no error).
    const pdf = new File(["pdf-bytes"], "doc.pdf", { type: "application/pdf" });
    fireEvent.paste(screen.getByTestId("chat-input"), {
      clipboardData: {
        items: [
          { kind: "file", type: "application/pdf", getAsFile: () => pdf },
        ],
      },
    });

    expect(
      screen.queryByTestId("chat-attachment-chip"),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("chat-attachment-error"),
    ).not.toBeInTheDocument();
  });

  it("keeps the file-count cap when FileReader completions race", async () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          sessions: [activeSession()],
        })}
      />,
    );

    for (let i = 0; i < 9; i++) {
      fireEvent.change(screen.getByTestId("chat-attachment-input"), {
        target: {
          files: [
            uploadFile(`rapid-${i}.bin`, "application/octet-stream", [
              `rapid-bytes-${i}`,
            ]),
          ],
        },
      });
    }

    await waitFor(() =>
      expect(screen.getAllByTestId("chat-attachment-chip")).toHaveLength(8),
    );
    expect(
      await screen.findByTestId("chat-attachment-error"),
    ).toHaveTextContent(/at most 8/i);
  });

  it("caps the combined raw attachment bytes at 10 MiB", async () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          sessions: [activeSession()],
        })}
      />,
    );

    const fiveMiB = 5 * 1024 * 1024;
    const firstBytes = new Uint8Array(fiveMiB);
    const secondBytes = new Uint8Array(fiveMiB);
    secondBytes[fiveMiB - 1] = 1;

    for (const file of [
      new File([firstBytes], "first.bin", {
        type: "application/octet-stream",
      }),
      new File([secondBytes], "second.bin", {
        type: "application/octet-stream",
      }),
    ]) {
      fireEvent.change(screen.getByTestId("chat-attachment-input"), {
        target: { files: [file] },
      });
      await waitFor(
        () =>
          expect(screen.getAllByTestId("chat-attachment-chip")).toHaveLength(
            file.name === "first.bin" ? 1 : 2,
          ),
        { timeout: 10_000 },
      );
    }

    fireEvent.change(screen.getByTestId("chat-attachment-input"), {
      target: {
        files: [uploadFile("overflow.bin", "application/octet-stream", ["x"])],
      },
    });

    const error = await screen.findByTestId("chat-attachment-error");
    expect(error).toHaveTextContent(/too large to send together/i);
    expect(screen.getAllByTestId("chat-attachment-chip")).toHaveLength(2);
  });
});

describe("ChatPanel provided-agent streaming shape", () => {
  it("clears provider thinking when a final provider output event arrives", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          isStreaming: true,
          messages: [
            chatMessage({
              id: "assistant-thinking",
              content: "",
              status: "streaming",
            }),
          ],
          timelineEntries: [
            timelineEntry({
              id: "thinking-started",
              content: "Claude Code is thinking",
              metadata: {
                source: "provided_agent",
                provider: "claude_code",
                providerEvent: "provided_agent.thinking.started",
                sessionId: "session-1",
                messageId: "assistant-thinking",
              },
            }),
            timelineEntry({
              id: "final-output",
              content: "Final answer",
              createdAt: "2026-05-10T00:00:03Z",
              metadata: {
                source: "provided_agent",
                provider: "claude_code",
                providerEvent: "provided_agent.output.final",
                phase: "output",
                sessionId: "session-1",
                messageId: "assistant-thinking",
              },
            }),
          ],
        })}
      />,
    );

    expect(
      screen.queryByTestId("provider-thinking-indicator"),
    ).not.toBeInTheDocument();
  });

  // Issue #1313 review (CodeRabbit): the Thinking indicator is derived from
  // `panelTimelineEntries`, which must stay UNprojected. `isProviderThinking`
  // clears only on final/completed/error/interrupted — not on delta — so if the
  // panel entries were projected (dropping `provided_agent.output.final` for a
  // delta-bearing turn) the indicator would stick on. This turn has a delta AND
  // a final; the indicator must still clear.
  it("clears provider thinking after the final output even when the turn also has a delta", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          isStreaming: true,
          messages: [
            chatMessage({
              id: "assistant-thinking-delta",
              content: "",
              status: "streaming",
            }),
          ],
          timelineEntries: [
            timelineEntry({
              id: "thinking-started",
              content: "Claude Code is thinking",
              metadata: {
                source: "provided_agent",
                provider: "claude_code",
                providerEvent: "provided_agent.thinking.started",
                sessionId: "session-1",
                messageId: "assistant-thinking-delta",
              },
            }),
            timelineEntry({
              id: "output-delta",
              content: "The answer.",
              createdAt: "2026-05-10T00:00:03Z",
              metadata: {
                source: "provided_agent",
                provider: "claude_code",
                providerEvent: "provided_agent.output.delta",
                phase: "output",
                streamPersistence: "semantic",
                sessionId: "session-1",
                messageId: "assistant-thinking-delta",
                turnId: "assistant-thinking-delta",
                blockId: "claude-code-assistant-thinking-delta-text-0",
              },
            }),
            timelineEntry({
              id: "output-final",
              content: "The answer.",
              createdAt: "2026-05-10T00:00:04Z",
              metadata: {
                source: "provided_agent",
                provider: "claude_code",
                providerEvent: "provided_agent.output.final",
                phase: "output",
                sessionId: "session-1",
                messageId: "assistant-thinking-delta",
                turnId: "assistant-thinking-delta",
                blockId: "claude-code-assistant-thinking-delta-final",
              },
            }),
          ],
        })}
      />,
    );

    expect(
      screen.queryByTestId("provider-thinking-indicator"),
    ).not.toBeInTheDocument();
  });

  it("renders streaming output deltas in the assistant message shape instead of a Response trace", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          isStreaming: true,
          messages: [
            chatMessage({
              id: "assistant-streaming",
              content: "",
              status: "streaming",
            }),
          ],
          timelineEntries: [
            timelineEntry({
              id: "delta-1",
              content: "Hello while streaming",
              metadata: {
                source: "provided_agent",
                provider: "claude_code",
                providerEvent: "provided_agent.output.delta",
                phase: "output",
                sessionId: "session-1",
                messageId: "assistant-streaming",
                blockId: "block-1",
              },
            }),
          ],
        })}
      />,
    );

    const assistant = screen.getByTestId("assistant-message");
    expect(
      within(assistant).getByText("Hello while streaming"),
    ).toBeInTheDocument();
    expect(screen.queryByText("Response")).not.toBeInTheDocument();
  });

  it("stitches consecutive durable output deltas into one text block without inserting gaps", () => {
    const delta = (id: string, content: string, createdAt: string) =>
      timelineEntry({
        id,
        content,
        createdAt,
        metadata: {
          source: "provided_agent",
          provider: "claude_code",
          providerEvent: "provided_agent.output.delta",
          phase: "output",
          streamPersistence: "durable",
          sessionId: "session-1",
          messageId: "assistant-1",
          turnId: "assistant-1",
          blockId: "native-output",
        },
      });
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          messages: [
            chatMessage({
              id: "assistant-1",
              content: "I'd be happy to help — but I don't yet know what for.",
              status: "complete",
            }),
          ],
          timelineEntries: [
            delta("delta-1", "I", "2026-09-23T01:20:47.000Z"),
            delta("delta-2", "'d be happy", "2026-09-23T01:20:47.100Z"),
            delta("delta-3", " ", "2026-09-23T01:20:47.150Z"),
            delta("delta-4", "to help — but I don", "2026-09-23T01:20:47.200Z"),
            delta(
              "delta-5",
              "'t yet know what for.",
              "2026-09-23T01:20:47.300Z",
            ),
            timelineEntry({
              id: "final-1",
              content: "I'd be happy to help — but I don't yet know what for.",
              createdAt: "2026-09-23T01:20:47.400Z",
              metadata: {
                source: "provided_agent",
                provider: "claude_code",
                providerEvent: "provided_agent.output.final",
                phase: "output",
                sessionId: "session-1",
                messageId: "assistant-1",
                turnId: "assistant-1",
                blockId: "native-output",
              },
            }),
          ],
        })}
      />,
    );

    const assistant = screen.getByTestId("assistant-message");
    expect(
      within(assistant).getAllByText(
        "I'd be happy to help — but I don't yet know what for.",
      ),
    ).toHaveLength(1);
    expect(within(assistant).queryByText(/I 'd/)).not.toBeInTheDocument();
  });

  it("stitches delta-only rows byte for byte when the answer is projected from the timeline", () => {
    // No provider metadata and an empty completed message still use the
    // common output aggregation and assistant block projection.
    const delta = (id: string, content: string, createdAt: string) =>
      timelineEntry({
        id,
        content,
        createdAt,
        metadata: {
          providerEvent: "provided_agent.output.delta",
          phase: "output",
          streamPersistence: "durable",
          sessionId: "session-1",
          messageId: "assistant-2",
          blockId: "native-output",
        },
      });
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          messages: [
            chatMessage({ id: "assistant-2", content: "", status: "complete" }),
          ],
          timelineEntries: [
            delta("p-1", "There", "2026-09-23T01:30:00.000Z"),
            delta("p-2", "'s no pr", "2026-09-23T01:30:00.100Z"),
            delta("p-3", "ior task", "2026-09-23T01:30:00.200Z"),
            delta("p-4", " ", "2026-09-23T01:30:00.250Z"),
            delta("p-5", "here.", "2026-09-23T01:30:00.300Z"),
          ],
        })}
      />,
    );

    const assistant = screen.getByTestId("assistant-message");
    expect(assistant.textContent).toContain("There's no prior task here.");
    expect(assistant.textContent).not.toMatch(/There 's|pr ior|task {2}here/);
  });

  it("renders a streaming Claude block once as live partial rows give way to its persisted row", () => {
    const liveMetadata = {
      source: "provided_agent",
      provider: "claude_code",
      providerEvent: "provided_agent.output.delta",
      phase: "output",
      streamPersistence: "volatile",
      sessionId: "session-1",
      messageId: "assistant-streaming",
      turnId: "assistant-streaming",
      blockId: "claude-code-assistant-streaming-text-1",
    };
    const liveRows = [
      timelineEntry({
        id: "live-1",
        content: "partial ",
        createdAt: "2026-07-18T00:00:01Z",
        metadata: liveMetadata,
      }),
      timelineEntry({
        id: "live-2",
        content: "partial review text",
        createdAt: "2026-07-18T00:00:02Z",
        metadata: liveMetadata,
      }),
    ];
    const streamingMessage = chatMessage({
      id: "assistant-streaming",
      content: "",
      status: "streaming",
    });
    const { rerender } = render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          isStreaming: true,
          messages: [streamingMessage],
          timelineEntries: liveRows,
        })}
      />,
    );
    let assistant = screen.getByTestId("assistant-message");
    expect(within(assistant).getAllByText("partial review text")).toHaveLength(
      1,
    );
    expect(within(assistant).queryByText("partial")).not.toBeInTheDocument();

    rerender(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          isStreaming: true,
          messages: [streamingMessage],
          timelineEntries: [
            ...liveRows,
            timelineEntry({
              id: "delta-1",
              content: "partial review text",
              createdAt: "2026-07-18T00:00:03Z",
              metadata: { ...liveMetadata, streamPersistence: "semantic" },
            }),
          ],
        })}
      />,
    );
    assistant = screen.getByTestId("assistant-message");
    expect(within(assistant).getAllByText("partial review text")).toHaveLength(
      1,
    );
  });

  // Review round 2: a turn that ends without a completed assistant block
  // persists only its final result; the live rows it streamed are provisional
  // and must give way to that final, whichever order the rows arrive in.
  it("shows the final result instead of provisional live rows when a turn ends without a completed block", () => {
    const outputMetadata = {
      source: "provided_agent",
      provider: "claude_code",
      phase: "output",
      sessionId: "session-1",
      messageId: "assistant-fallback",
      turnId: "assistant-fallback",
    };
    const liveRow = timelineEntry({
      id: "live-1",
      content: "provisional text",
      createdAt: "2026-07-18T00:00:02Z",
      metadata: {
        ...outputMetadata,
        providerEvent: "provided_agent.output.delta",
        streamPersistence: "volatile",
        blockId: "claude-code-assistant-fallback-text-1",
      },
    });
    const finalRow = timelineEntry({
      id: "final-1",
      content: "authoritative final answer",
      createdAt: "2026-07-18T00:00:03Z",
      metadata: {
        ...outputMetadata,
        providerEvent: "provided_agent.output.final",
        blockId: "claude-code-assistant-fallback-final",
      },
    });
    const orders = [
      [liveRow, finalRow],
      [finalRow, { ...liveRow, createdAt: "2026-07-18T00:00:04Z" }],
    ];
    for (const timelineEntries of orders) {
      const { unmount } = render(
        <ChatPanel
          {...defaultProps({
            session: activeSession(),
            isStreaming: true,
            messages: [
              chatMessage({
                id: "assistant-fallback",
                content: "",
                status: "streaming",
              }),
            ],
            timelineEntries,
          })}
        />,
      );
      const assistant = screen.getByTestId("assistant-message");
      expect(
        within(assistant).getAllByText("authoritative final answer"),
      ).toHaveLength(1);
      expect(
        within(assistant).queryByText("provisional text"),
      ).not.toBeInTheDocument();
      unmount();
    }
  });

  // Issue #1313 / core_workflow/chat_agent_session#D18: Codex output rows carry
  // messageId + turnId but NO blockId (codex_frame_metadata, codex.rs), so
  // buildProvidedAgentOutputProjections — keyed on messageId:blockId — skips
  // them and both the final-answer delta and the aggregate final render as raw
  // traces during streaming (a real WebSocket-visible double). The shared
  // per-turn projection over the scoped entries must collapse them to one
  // logical answer. A Claude fixture (blockId present) would not cover this.
  it("renders a streaming Codex answer once when delta and final rows lack blockId (#1313)", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession({ agentProvider: "codex" }),
          selectedAgentProvider: "codex",
          availableAgentProviders: ["claude_code", "codex"],
          isStreaming: true,
          messages: [
            chatMessage({
              id: "assistant-codex",
              content: "",
              status: "streaming",
            }),
          ],
          timelineEntries: [
            timelineEntry({
              id: "codex-delta",
              content: "The one Codex answer.",
              createdAt: "2026-05-10T00:00:02Z",
              metadata: {
                source: "provided_agent",
                provider: "codex",
                providerEvent: "provided_agent.output.delta",
                phase: "output",
                streamPersistence: "semantic",
                sessionId: "session-1",
                messageId: "assistant-codex",
                turnId: "assistant-codex",
                // No blockId — matches codex_frame_metadata (codex.rs).
              },
            }),
            timelineEntry({
              id: "codex-final",
              content: "The one Codex answer.",
              createdAt: "2026-05-10T00:00:03Z",
              metadata: {
                source: "provided_agent",
                provider: "codex",
                providerEvent: "provided_agent.output.final",
                phase: "output",
                sessionId: "session-1",
                messageId: "assistant-codex",
                turnId: "assistant-codex",
              },
            }),
          ],
        })}
      />,
    );

    expect(screen.getAllByText("The one Codex answer.")).toHaveLength(1);
  });

  // issue #2042 / chat_panel_runtime D26 / provided_agent_runtime D65:
  // Codex must use the rich provided-agent turn path so reasoning and
  // structured work details do not collapse into generic high-level traces.
  it("renders Codex reasoning and expandable command details in the rich provider turn", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession({ agentProvider: "codex" }),
          selectedAgentProvider: "codex",
          availableAgentProviders: ["claude_code", "codex"],
          isStreaming: false,
          messages: [
            chatMessage({
              id: "assistant-codex-details",
              content: "Finished checking the repository.",
              status: "complete",
            }),
          ],
          timelineEntries: codexRichTimelineEntries(),
        })}
      />,
    );

    const assistant = screen.getByTestId("assistant-message");
    expect(within(assistant).getByText("Thinking")).toBeInTheDocument();
    expect(
      within(assistant).queryByText("I will inspect the focused test first."),
    ).not.toBeInTheDocument();
    fireEvent.click(
      within(assistant).getByRole("button", { name: "Expand thinking" }),
    );
    expect(
      within(assistant).getByText("I will inspect the focused test first."),
    ).toBeInTheDocument();

    const toolBlock = screen.getByTestId("assistant-tool-block");
    expect(toolBlock).toHaveTextContent("Command Execution");
    expect(toolBlock).toHaveTextContent("done");
    expect(toolBlock).not.toHaveAttribute("open");
    const summary = toolBlock.querySelector("summary");
    expect(summary).not.toBeNull();
    fireEvent.click(summary!);
    expect(toolBlock).toHaveAttribute("open");
    expect(toolBlock).toHaveTextContent(
      "cargo test -p g5d-worker codex_reasoning_summary",
    );
    expect(toolBlock).toHaveTextContent("test result: ok");
  });

  // chat_panel_runtime D26 / provided_agent_runtime D65 / TC-CHAT-063: Claude
  // thinking summaries arrive as `thought` entries stamped
  // providerEvent "provided_agent.thought" and render through the existing
  // Thinking row — not a provider status chip, and never merged into the
  // assistant message content.
  it("renders provided_agent.thought entries as a Thinking block inside the Claude turn", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          isStreaming: true,
          messages: [
            chatMessage({
              id: "assistant-reasoning",
              content: "",
              status: "streaming",
            }),
          ],
          timelineEntries: [
            timelineEntry({
              id: "thought-1",
              entryType: "thought",
              content: "Planning the next tool call.",
              metadata: {
                source: "provided_agent",
                provider: "claude_code",
                providerEvent: "provided_agent.thought",
                phase: "thought",
                sessionId: "session-1",
                messageId: "assistant-reasoning",
                blockId: "claude-code-assistant-reasoning-thought-1",
                thoughtSegmentIndex: 1,
              },
            }),
            timelineEntry({
              id: "delta-1",
              content: "Visible answer",
              createdAt: "2026-05-10T00:00:03Z",
              metadata: {
                source: "provided_agent",
                provider: "claude_code",
                providerEvent: "provided_agent.output.delta",
                phase: "output",
                sessionId: "session-1",
                messageId: "assistant-reasoning",
                blockId: "block-1",
              },
            }),
          ],
        })}
      />,
    );

    const assistant = screen.getByTestId("assistant-message");
    expect(within(assistant).getByText("Thinking")).toBeInTheDocument();
    fireEvent.click(
      within(assistant).getByRole("button", { name: "Expand thinking" }),
    );
    const thought = within(assistant).getByText("Planning the next tool call.");
    const answer = within(assistant).getByText("Visible answer");
    // The thought block is ordered ahead of the answer text and stays a
    // distinct labeled block, never merged into the answer markdown.
    expect(
      thought.compareDocumentPosition(answer) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    // No provider status chip is minted for the thought event.
    expect(screen.queryByText("Claude warning")).not.toBeInTheDocument();
  });

  // chat_panel_runtime D26: the chat-turn thought block is a collapsed-by-
  // default Thinking disclosure. Its contents appear only when requested.
  it("renders the Claude thought block as a collapsed-by-default Thinking widget that expands on click", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          isStreaming: true,
          messages: [
            chatMessage({
              id: "assistant-reasoning",
              content: "",
              status: "streaming",
            }),
          ],
          timelineEntries: [
            timelineEntry({
              id: "thought-1",
              entryType: "thought",
              content: "Planning the next tool call.",
              metadata: {
                source: "provided_agent",
                provider: "claude_code",
                providerEvent: "provided_agent.thought",
                phase: "thought",
                sessionId: "session-1",
                messageId: "assistant-reasoning",
                blockId: "claude-code-assistant-reasoning-thought-1",
                thoughtSegmentIndex: 1,
              },
            }),
          ],
        })}
      />,
    );

    const assistant = screen.getByTestId("assistant-message");
    const widget = within(assistant).getByTestId("thinking-block");
    // Reasoning no longer adds a permanent preview to the reading surface.
    const toggle = within(widget).getByRole("button", {
      name: "Expand thinking",
    });
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    expect(
      within(widget).queryByText("Planning the next tool call."),
    ).not.toBeInTheDocument();
    fireEvent.click(toggle);
    const collapse = within(widget).getByRole("button", {
      name: "Collapse thinking",
    });
    expect(collapse).toHaveAttribute("aria-expanded", "true");
    expect(
      within(widget).getByText("Planning the next tool call."),
    ).toBeInTheDocument();

    // The header stays visible while expanded so clicking it collapses again.
    fireEvent.click(collapse);
    expect(
      within(widget).getByRole("button", { name: "Expand thinking" }),
    ).toHaveAttribute("aria-expanded", "false");
  });

  it("synthesizes a streaming assistant message if deltas arrive before the message row", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          isStreaming: true,
          messages: [
            chatMessage({
              id: "user-1",
              role: "user",
              content: "Hello",
              status: "complete",
              messageIndex: 0,
            }),
          ],
          timelineEntries: [
            timelineEntry({
              id: "delta-early",
              content: "Assistant appeared from timeline",
              metadata: {
                source: "provided_agent",
                providerEvent: "provided_agent.output.delta",
                phase: "output",
                sessionId: "session-1",
                messageId: "assistant-late",
                blockId: "block-1",
              },
            }),
          ],
        })}
      />,
    );

    expect(
      screen.getByText("Assistant appeared from timeline"),
    ).toBeInTheDocument();
    expect(screen.queryByText("Response")).not.toBeInTheDocument();
  });

  it("hides sync lifecycle reminder control turns from the chat transcript", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          isStreaming: false,
          messages: [
            chatMessage({
              id: "user-0",
              role: "user",
              content: "Start the sync task",
              status: "complete",
              messageIndex: 0,
            }),
            chatMessage({
              id: "assistant-1",
              content: "Initial sync answer",
              status: "complete",
              messageIndex: 1,
            }),
            chatMessage({
              id: "user-2",
              role: "user",
              content: "Hi",
              status: "complete",
              messageIndex: 2,
            }),
            chatMessage({
              id: "assistant-3",
              content: "Hi! Ready to help.",
              status: "complete",
              messageIndex: 3,
            }),
            chatMessage({
              id: "user-4",
              role: "user",
              content: "what files changed?",
              status: "complete",
              messageIndex: 4,
            }),
            chatMessage({
              id: "assistant-5",
              content: "Two files changed.",
              status: "complete",
              messageIndex: 5,
            }),
            chatMessage({
              id: "user-6",
              role: "user",
              content: "Remove the hint phrase.",
              status: "complete",
              messageIndex: 6,
            }),
            chatMessage({
              id: "assistant-7",
              content:
                "Done. Removed the hint phrase.\n\nLifecycle reminder result:\n\nDone -- committed (compliance-code de038b0, docs 562e00a) and task closed.",
              status: "complete",
              messageIndex: 7,
            }),
          ],
          timelineEntries: [
            timelineEntry({
              id: "reminder-started",
              entryType: "checkpoint",
              content: "Sync lifecycle reminder started",
              metadata: {
                source: "provided_agent",
                provider: "claude_code",
                providerEvent: "provided_agent.sync_lifecycle_reminder.started",
                phase: "checkpoint",
                sessionId: "session-1",
                messageId: "assistant-7-sync-lifecycle-reminder",
              },
              sequence: 1,
              createdAt: "2026-05-10T00:00:10Z",
            }),
            timelineEntry({
              id: "reminder-tool-started",
              entryType: "tool_call",
              content: "Claude Code used Bash",
              metadata: {
                source: "provided_agent",
                provider: "claude_code",
                providerEvent: "provided_agent.tool.started",
                phase: "tool_call",
                sessionId: "session-1",
                messageId: "assistant-7-sync-lifecycle-reminder",
                toolName: "bash",
                args: { command: "g5d task complete" },
              },
              sequence: 2,
              createdAt: "2026-05-10T00:00:11Z",
            }),
            timelineEntry({
              id: "reminder-tool-completed",
              entryType: "tool_result",
              content: "task closed",
              metadata: {
                source: "provided_agent",
                provider: "claude_code",
                providerEvent: "provided_agent.tool.completed",
                phase: "tool_result",
                sessionId: "session-1",
                messageId: "assistant-7-sync-lifecycle-reminder",
                toolName: "bash",
                toolCallId: "tool-reminder",
              },
              sequence: 3,
              createdAt: "2026-05-10T00:00:12Z",
            }),
            timelineEntry({
              id: "reminder-final-output",
              entryType: "output",
              content:
                "Done -- committed (compliance-code de038b0, docs 562e00a) and task closed.",
              metadata: {
                source: "provided_agent",
                provider: "claude_code",
                providerEvent: "provided_agent.output.final",
                phase: "output",
                sessionId: "session-1",
                messageId: "assistant-7-sync-lifecycle-reminder",
                blockId: "block-reminder",
              },
              sequence: 4,
              createdAt: "2026-05-10T00:00:13Z",
            }),
          ],
        })}
      />,
    );

    const transcript = screen.getByTestId("chat-messages");
    expect(
      within(transcript).getByText(/Lifecycle reminder result/),
    ).toBeInTheDocument();
    expect(within(transcript).getAllByText(/Done -- committed/)).toHaveLength(
      1,
    );
    expect(within(transcript).queryByText("Bash")).not.toBeInTheDocument();
    expect(
      within(transcript).queryByText("Sync lifecycle reminder started"),
    ).not.toBeInTheDocument();
  });

  it("still synthesizes unrelated unknown provider output", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          isStreaming: true,
          messages: [
            chatMessage({
              id: "user-1",
              role: "user",
              content: "Hello",
              status: "complete",
              messageIndex: 0,
            }),
          ],
          timelineEntries: [
            timelineEntry({
              id: "delta-unrelated-unknown",
              content: "Visible unknown provider output",
              metadata: {
                source: "provided_agent",
                providerEvent: "provided_agent.output.final",
                phase: "output",
                sessionId: "session-1",
                messageId: "assistant-from-valid-provider-output",
                blockId: "block-1",
              },
            }),
          ],
        })}
      />,
    );

    expect(
      screen.getByText("Visible unknown provider output"),
    ).toBeInTheDocument();
  });

  it("does not render provided-agent timeline entries from another chat session", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession({ id: "session-new" }),
          isStreaming: true,
          messages: [
            chatMessage({
              id: "assistant-new",
              sessionId: "session-new",
              content: "",
              status: "streaming",
            }),
          ],
          timelineEntries: [
            timelineEntry({
              id: "delta-old-session",
              content: "Uplift complete. Summary: stale old session text",
              metadata: {
                source: "provided_agent",
                provider: "claude_code",
                providerEvent: "provided_agent.output.delta",
                phase: "output",
                sessionId: "session-old",
                messageId: "assistant-old",
                blockId: "block-old",
              },
            }),
            timelineEntry({
              id: "approval-new-session",
              entryType: "tool_call",
              content: "Claude Code requests permission for Bash",
              metadata: {
                source: "provided_agent",
                provider: "claude_code",
                providerEvent: "provided_agent.tool.started",
                phase: "tool_call",
                sessionId: "session-new",
                messageId: "assistant-new",
                toolName: "bash",
                args: { command: "ls" },
              },
            }),
          ],
        })}
      />,
    );

    expect(
      screen.queryByText(/Uplift complete\. Summary/),
    ).not.toBeInTheDocument();
    expect(screen.getByText("Bash")).toBeInTheDocument();
  });

  it("uses final assistant content once without duplicate output trace rows", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          isStreaming: false,
          messages: [
            chatMessage({
              id: "assistant-complete",
              content: "Final persisted answer",
              status: "complete",
            }),
          ],
          timelineEntries: [
            timelineEntry({
              id: "delta-complete",
              content: "Transient streamed answer",
              metadata: {
                source: "provided_agent",
                providerEvent: "provided_agent.output.delta",
                phase: "output",
                sessionId: "session-1",
                messageId: "assistant-complete",
                blockId: "block-1",
              },
            }),
          ],
        })}
      />,
    );

    expect(screen.getByText("Final persisted answer")).toBeInTheDocument();
    expect(
      screen.queryByText("Transient streamed answer"),
    ).not.toBeInTheDocument();
    expect(screen.queryByText("Response")).not.toBeInTheDocument();
  });

  it("keeps approvals, tool events, and warnings visible while moving only answer text", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          isStreaming: true,
          messages: [
            chatMessage({
              id: "assistant-visible",
              content: "",
              status: "streaming",
            }),
          ],
          timelineEntries: [
            timelineEntry({
              id: "delta-visible",
              content: "Streaming answer text",
              metadata: {
                source: "provided_agent",
                providerEvent: "provided_agent.output.delta",
                phase: "output",
                sessionId: "session-1",
                messageId: "assistant-visible",
                blockId: "block-1",
              },
            }),
            timelineEntry({
              id: "tool-visible",
              entryType: "tool_call",
              content: "Claude Code used Bash",
              metadata: {
                source: "provided_agent",
                providerEvent: "provided_agent.tool.started",
                phase: "tool_call",
                sessionId: "session-1",
                messageId: "assistant-visible",
                toolName: "bash",
                args: { command: "ls" },
              },
              sequence: 2,
              createdAt: "2026-05-10T00:00:03Z",
            }),
            timelineEntry({
              id: "approval-visible",
              entryType: "checkpoint",
              content: "List files",
              metadata: {
                source: "provided_agent",
                providerEvent: "provided_agent.approval.requested",
                phase: "approval",
                sessionId: "session-1",
                messageId: "assistant-visible",
                approvalId: "approval-1",
                toolName: "ls",
                requestedAction: "List workspace files",
              },
              sequence: 3,
              createdAt: "2026-05-10T00:00:04Z",
            }),
            timelineEntry({
              id: "warning-visible",
              entryType: "checkpoint",
              content: "Claude warning text",
              metadata: {
                source: "provided_agent",
                providerEvent: "provided_agent.warning",
                phase: "status",
                sessionId: "session-1",
                messageId: "assistant-visible",
              },
              sequence: 4,
              createdAt: "2026-05-10T00:00:05Z",
            }),
          ],
        })}
      />,
    );

    expect(screen.getByText("Streaming answer text")).toBeInTheDocument();
    expect(screen.queryByText("Response")).not.toBeInTheDocument();
    expect(screen.getByText("Bash")).toBeInTheDocument();
    expect(screen.getByText("Approval requested: Ls")).toBeInTheDocument();
    expect(screen.getByText("List workspace files")).toBeInTheDocument();
    expect(screen.getByText("Claude warning text")).toBeInTheDocument();
  });

  // Issue #1491 / provided_agent_runtime D68: the Skill tool card is the
  // user-visible "loaded skill <name>" affordance for a mid-turn skill
  // invocation, so its summary must surface the skill name from args.
  it("renders a Skill tool card with the skill name as its summary", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          isStreaming: true,
          messages: [
            chatMessage({
              id: "assistant-skill",
              content: "",
              status: "streaming",
            }),
          ],
          timelineEntries: [
            timelineEntry({
              id: "tool-skill",
              entryType: "tool_call",
              content: "Claude Code used Skill",
              metadata: {
                source: "provided_agent",
                provider: "claude_code",
                providerEvent: "provided_agent.tool.started",
                phase: "tool_call",
                sessionId: "session-1",
                messageId: "assistant-skill",
                toolName: "Skill",
                args: { skill: "g5-node-explainer" },
              },
              sequence: 1,
              createdAt: "2026-05-10T00:00:03Z",
            }),
          ],
        })}
      />,
    );

    expect(screen.getByText("Skill")).toBeInTheDocument();
    expect(screen.getByText("g5-node-explainer")).toBeInTheDocument();
  });

  it("renders a provider NACK as a non-actionable approval failure", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          isStreaming: true,
          messages: [
            chatMessage({
              id: "assistant-failed-approval",
              content: "",
              status: "streaming",
            }),
          ],
          timelineEntries: [
            timelineEntry({
              id: "tool-failed-approval",
              entryType: "tool_call",
              content: "Claude Code used Bash",
              metadata: {
                source: "provided_agent",
                providerEvent: "provided_agent.tool.started",
                phase: "tool_call",
                sessionId: "session-1",
                messageId: "assistant-failed-approval",
                toolCallId: "toolu-failed",
                toolName: "Bash",
                args: { command: "npm test" },
              },
              sequence: 1,
            }),
            timelineEntry({
              id: "approval-failed-request",
              entryType: "checkpoint",
              content: "Approval requested",
              metadata: {
                source: "provided_agent",
                providerEvent: "provided_agent.approval.requested",
                phase: "approval",
                sessionId: "session-1",
                messageId: "assistant-failed-approval",
                toolCallId: "toolu-failed",
                approvalId: "approval-failed",
                toolName: "Bash",
              },
              sequence: 2,
            }),
            timelineEntry({
              id: "approval-failed-result",
              entryType: "checkpoint",
              content: "Approval delivery failed: approval-failed",
              metadata: {
                source: "provided_agent",
                provider: "claude_code",
                providerEvent: "provided_agent.approval.failed",
                phase: "approval",
                sessionId: "session-1",
                messageId: "assistant-failed-approval",
                toolCallId: "toolu-failed",
                approvalId: "approval-failed",
                errorCode: "UNKNOWN_APPROVAL",
              },
              sequence: 3,
            }),
          ],
        })}
      />,
    );

    expect(
      screen.queryByTestId("agent-approval-approve"),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("agent-approval-reject"),
    ).not.toBeInTheDocument();
    expect(
      screen.getAllByText(/Approval delivery failed/i).length,
    ).toBeGreaterThan(0);
    expect(screen.getByTestId("assistant-tool-block")).toHaveTextContent(
      "error",
    );
  });
});

describe("ChatPanel terminal tool reconciliation (#603)", () => {
  it("reconciles an unmatched running tool to 'ended' once the turn is complete", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          isStreaming: false,
          messages: [
            chatMessage({
              id: "assistant-orphan",
              content: "All done.",
              status: "complete",
            }),
          ],
          timelineEntries: [
            timelineEntry({
              id: "tool-orphan-start",
              entryType: "tool_call",
              content: "Claude Code used Bash",
              metadata: {
                source: "provided_agent",
                provider: "claude_code",
                providerEvent: "provided_agent.tool.started",
                phase: "tool_call",
                sessionId: "session-1",
                messageId: "assistant-orphan",
                toolCallId: "toolu-orphan",
                toolName: "Bash",
                args: { command: "ls" },
              },
              sequence: 1,
            }),
          ],
        })}
      />,
    );

    const toolBlock = screen.getByTestId("assistant-tool-block");
    expect(toolBlock).toHaveTextContent("Bash");
    expect(toolBlock).toHaveTextContent("ended");
    expect(toolBlock).not.toHaveTextContent("running");
  });

  it("keeps a tool with a recorded completion as 'done' on a complete turn", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          isStreaming: false,
          messages: [
            chatMessage({
              id: "assistant-done",
              content: "All done.",
              status: "complete",
            }),
          ],
          timelineEntries: [
            timelineEntry({
              id: "tool-done-start",
              entryType: "tool_call",
              content: "Claude Code used Bash",
              metadata: {
                source: "provided_agent",
                provider: "claude_code",
                providerEvent: "provided_agent.tool.started",
                phase: "tool_call",
                sessionId: "session-1",
                messageId: "assistant-done",
                toolCallId: "toolu-done",
                toolName: "Bash",
                args: { command: "ls" },
              },
              sequence: 1,
            }),
            timelineEntry({
              id: "tool-done-result",
              entryType: "tool_result",
              content: "Claude Code completed claude_tool",
              metadata: {
                source: "provided_agent",
                provider: "claude_code",
                providerEvent: "provided_agent.tool.completed",
                phase: "tool_result",
                sessionId: "session-1",
                messageId: "assistant-done",
                toolCallId: "toolu-done",
                toolName: "claude_tool",
                contentSnippet: "listed",
              },
              sequence: 2,
              createdAt: "2026-05-10T00:00:03Z",
            }),
          ],
        })}
      />,
    );

    const toolBlock = screen.getByTestId("assistant-tool-block");
    expect(toolBlock).toHaveTextContent("Bash");
    expect(toolBlock).toHaveTextContent("done");
    expect(toolBlock).not.toHaveTextContent("ended");
    expect(toolBlock).not.toHaveTextContent("running");
  });

  it("leaves an in-flight tool as 'running' while the turn is still streaming", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          isStreaming: true,
          messages: [
            chatMessage({
              id: "assistant-live",
              content: "",
              status: "streaming",
            }),
          ],
          timelineEntries: [
            timelineEntry({
              id: "tool-live-start",
              entryType: "tool_call",
              content: "Claude Code used Bash",
              metadata: {
                source: "provided_agent",
                provider: "claude_code",
                providerEvent: "provided_agent.tool.started",
                phase: "tool_call",
                sessionId: "session-1",
                messageId: "assistant-live",
                toolCallId: "toolu-live",
                toolName: "Bash",
                args: { command: "ls" },
              },
              sequence: 1,
            }),
          ],
        })}
      />,
    );

    const toolBlock = screen.getByTestId("assistant-tool-block");
    expect(toolBlock).toHaveTextContent("running");
    expect(toolBlock).not.toHaveTextContent("ended");
  });
});

describe("ChatPanel tool card details from the task journal", () => {
  function journalEntry(
    id: string,
    kind: string,
    payload: Record<string, unknown>,
    sequence: number,
  ): TimelineEntry {
    return projectTaskTimelineEntry(
      timelineEntry({
        id,
        entryType:
          kind === "tool.started"
            ? "tool_call"
            : kind === "message.delta"
              ? "output"
              : kind === "message.final"
                ? "agent_message"
                : kind === "tool.completed"
                  ? "tool_result"
                  : "checkpoint",
        content:
          typeof payload.content === "string"
            ? payload.content
            : typeof payload.text_delta === "string"
              ? payload.text_delta
              : kind,
        sequence,
        metadata: {
          schema_version: "g5.task-event.v1",
          kind,
          payload,
          taskId: "task-1",
          attempt: 1,
          messageId: "assistant-journal",
          sessionId: "session-1",
        },
      }),
    );
  }

  function activityEntries(count = 6, complete = true): TimelineEntry[] {
    return Array.from({ length: count }, (_, index) => [
      journalEntry(
        `start-${index}`,
        "tool.started",
        {
          call_id: `call-${index}`,
          name: "Bash",
          input: {
            description: `Inspect document ${index + 1}`,
            command: `cat doc-${index + 1}.md`,
          },
        },
        index * 2 + 1,
      ),
      ...(complete
        ? [
            journalEntry(
              `end-${index}`,
              "tool.completed",
              {
                call_id: `call-${index}`,
                name: "Bash",
                status: "ok",
                output: `document ${index + 1}`,
              },
              index * 2 + 2,
            ),
          ]
        : []),
    ]).flat();
  }

  function activityProps(
    entries: TimelineEntry[],
    status: ChatMessageDTO["status"] = "complete",
  ) {
    return defaultProps({
      session: activeSession(),
      isStreaming: status === "streaming",
      messages: [chatMessage({ id: "assistant-journal", content: "", status })],
      timelineEntries: entries,
    });
  }

  it("groups six journaled commands with useful labels and preserves nested disclosure choices", () => {
    const entries = activityEntries();
    const { rerender } = render(<ChatPanel {...activityProps(entries)} />);
    const group = screen
      .getByTestId("chat-activity-group")
      .querySelector("details")!;
    expect(group).not.toHaveAttribute("open");
    expect(group.querySelector("summary")).toHaveTextContent("Ran 6 commands");
    fireEvent.click(group.querySelector("summary")!);
    const tool = screen.getAllByTestId("assistant-tool-block")[0];
    expect(tool.querySelector("summary")).toHaveTextContent(
      "Inspect document 1",
    );
    expect(tool.querySelector("summary")).not.toHaveTextContent("done");
    fireEvent.click(tool.querySelector("summary")!);
    expect(tool).toHaveAttribute("open");
    rerender(<ChatPanel {...activityProps([...entries])} />);
    expect(group).toHaveAttribute("open");
    expect(tool).toHaveAttribute("open");
    expect(within(tool).getByText("cat doc-1.md")).toBeVisible();
  });

  it("preserves an explicitly opened row as a live single action grows into a completed group", () => {
    const entries = activityEntries(2);
    const { rerender } = render(
      <ChatPanel {...activityProps(entries.slice(0, 2), "streaming")} />,
    );
    fireEvent.click(
      screen.getByTestId("assistant-tool-block").querySelector("summary")!,
    );
    rerender(<ChatPanel {...activityProps(entries, "streaming")} />);
    expect(
      screen.getByTestId("chat-activity-group").querySelector("details"),
    ).toHaveAttribute("open");
    expect(screen.getAllByTestId("assistant-tool-block")[0]).toHaveAttribute(
      "open",
    );
    rerender(<ChatPanel {...activityProps(entries)} />);
    expect(screen.getAllByTestId("assistant-tool-block")[0]).toHaveAttribute(
      "open",
    );
  });

  it("preserves disclosure choices when a linked approval splits the first tool out of a group", () => {
    // Legacy linked requests carry toolCallId; native request IDs are independent.
    const entries = activityEntries(3, false).map((entry) => ({
      ...entry,
      metadata: { ...entry.metadata, schema_version: undefined },
    }));
    const { rerender } = render(
      <ChatPanel {...activityProps(entries, "streaming")} />,
    );
    const secondTool = screen.getAllByTestId("assistant-tool-block")[1];
    fireEvent.click(secondTool.querySelector("summary")!);
    fireEvent.click(secondTool.querySelector("summary")!);
    fireEvent.click(
      screen.getByTestId("chat-activity-group").querySelector("summary")!,
    );
    const approval = timelineEntry({
      id: "late-linked-approval",
      entryType: "checkpoint",
      sequence: 9,
      metadata: {
        source: "provided_agent",
        providerEvent: "provided_agent.approval.requested",
        phase: "approval",
        messageId: "assistant-journal",
        toolCallId: "call-0",
        approvalId: "permission-1",
        toolName: "Bash",
        input: { command: "cat doc-1.md" },
      },
    });
    rerender(
      <ChatPanel {...activityProps([...entries, approval], "streaming")} />,
    );
    expect(screen.getByText("Approval requested: Bash")).toBeVisible();
    expect(
      screen.getByTestId("chat-activity-group").querySelector("details"),
    ).not.toHaveAttribute("open");
    expect(screen.getAllByTestId("assistant-tool-block")[1]).toHaveAttribute(
      "open",
    );
  });

  it("keeps running and unsettled activity discoverable with the group closed and omits a failed count", () => {
    const entries = activityEntries(2, false);
    const { rerender } = render(
      <ChatPanel {...activityProps(entries, "streaming")} />,
    );
    const summary = screen
      .getByTestId("chat-activity-group")
      .querySelector("summary")!;
    fireEvent.click(summary);
    expect(screen.getByText("Running: Inspect document 2")).toBeVisible();
    const failed = journalEntry(
      "failure",
      "tool.completed",
      {
        call_id: "call-1",
        name: "Bash",
        status: "error",
        output: "Permission denied",
        output_truncated: true,
      },
      8,
    );
    rerender(<ChatPanel {...activityProps([...entries, failed], "error")} />);
    expect(
      screen.queryByText("Running: Inspect document 2"),
    ).not.toBeInTheDocument();
    expect(summary).not.toHaveTextContent("failed");
    expect(summary).toHaveTextContent("1 without a result");
    expect(screen.getAllByTestId("assistant-tool-block")[1]).toHaveTextContent(
      "Output truncated",
    );
  });

  it("keeps complete native narration snapshots around activity and retains a later block without deltas", () => {
    const entries = [
      journalEntry("partial", "message.delta", { text_delta: "Scan" }, 1),
      journalEntry(
        "narration",
        "message.final",
        { content: "Scanning the logs." },
        2,
      ),
      ...activityEntries(2).map((entry) => ({
        ...entry,
        sequence: (entry.sequence ?? 0) + 2,
      })),
      journalEntry(
        "answer",
        "message.final",
        { content: "Found the documentation source." },
        8,
      ),
    ];
    render(<ChatPanel {...activityProps(entries)} />);
    const assistant = screen.getByTestId("assistant-message");
    expect(within(assistant).getAllByText("Scanning the logs.")).toHaveLength(
      1,
    );
    expect(within(assistant).queryByText("Scan")).not.toBeInTheDocument();
    expect(
      within(assistant).getAllByText("Found the documentation source."),
    ).toHaveLength(1);
    const text = assistant.textContent!;
    expect(text.indexOf("Scanning the logs.")).toBeLessThan(
      text.indexOf("Ran 2 commands"),
    );
    expect(text.indexOf("Ran 2 commands")).toBeLessThan(
      text.indexOf("Found the documentation source."),
    );
  });

  it("renders native block identities once after tool activity, reload, and interruption", () => {
    const entries = [
      journalEntry(
        "partial",
        "message.delta",
        { block_id: "native-1:0", text_delta: "Check" },
        1,
      ),
      journalEntry(
        "first",
        "message.final",
        { block_id: "native-1:0", content: "Checked." },
        2,
      ),
      journalEntry(
        "tool",
        "tool.started",
        { call_id: "tool-1", name: "Bash" },
        3,
      ),
      journalEntry(
        "answer-partial",
        "message.delta",
        { block_id: "native-2:0", text_delta: "Ans" },
        4,
      ),
      journalEntry(
        "answer",
        "message.final",
        { block_id: "native-2:0", content: "Answer." },
        5,
      ),
      journalEntry(
        "repeat",
        "message.final",
        { block_id: "native-2:0", content: "Answer." },
        6,
      ),
    ];
    const { rerender, unmount } = render(
      <ChatPanel {...activityProps(entries.slice(0, 4), "streaming")} />,
    );
    let assistant = screen.getByTestId("assistant-message");
    expect(within(assistant).getAllByText("Checked.")).toHaveLength(1);
    expect(within(assistant).getAllByText("Ans")).toHaveLength(1);
    rerender(<ChatPanel {...activityProps(entries, "stopped")} />);
    assistant = screen.getByTestId("assistant-message");
    expect(within(assistant).getAllByText("Answer.")).toHaveLength(1);
    expect(within(assistant).queryByText("Check")).not.toBeInTheDocument();
    expect(within(assistant).queryByText("Ans")).not.toBeInTheDocument();
    expect(assistant).toHaveTextContent("Stopped");
    unmount();
    render(<ChatPanel {...activityProps(entries.slice(1), "stopped")} />);
    assistant = screen.getByTestId("assistant-message");
    expect(within(assistant).getAllByText("Checked.")).toHaveLength(1);
    expect(within(assistant).getAllByText("Answer.")).toHaveLength(1);
  });

  it("preserves JSON-shaped native answers during streaming, completion, and reload", () => {
    const answer = '{"content":"hello","status":"ok"}';
    const entries = [
      journalEntry(
        "json-d1",
        "message.delta",
        { text_delta: answer.slice(0, 15) },
        1,
      ),
      journalEntry(
        "json-d2",
        "message.delta",
        { text_delta: answer.slice(15) },
        2,
      ),
    ];
    const { rerender, unmount } = render(
      <ChatPanel {...activityProps(entries, "streaming")} />,
    );
    expect(
      within(screen.getByTestId("assistant-message")).getByText(answer),
    ).toBeVisible();
    const completed = [
      ...entries,
      journalEntry("json-final", "message.final", { content: answer }, 3),
    ];
    rerender(<ChatPanel {...activityProps(completed)} />);
    expect(
      within(screen.getByTestId("assistant-message")).getAllByText(answer),
    ).toHaveLength(1);
    unmount();
    // Codex and partial-history reloads can carry only the final snapshot.
    render(<ChatPanel {...activityProps(completed.slice(-1))} />);
    expect(
      within(screen.getByTestId("assistant-message")).getByText(answer),
    ).toBeVisible();
  });

  it("continues unwrapping legacy output envelopes", () => {
    const entry = timelineEntry({
      id: "legacy-envelope",
      entryType: "output",
      content: JSON.stringify({ content: "Legacy answer.", status: "done" }),
      metadata: {
        source: "provided_agent",
        providerEvent: "provided_agent.output.delta",
        phase: "output",
        messageId: "assistant-journal",
        blockId: "legacy-text",
        streamPersistence: "semantic",
      },
    });
    render(<ChatPanel {...activityProps([entry])} />);
    expect(
      within(screen.getByTestId("assistant-message")).getByText(
        "Legacy answer.",
      ),
    ).toBeVisible();
  });

  it("splits groups at narration and keeps a pending native approval outside disclosure", () => {
    const entries = [
      ...activityEntries(2),
      journalEntry(
        "middle",
        "message.final",
        { content: "Now verify the result." },
        5,
      ),
      ...activityEntries(2).map((entry) => ({
        ...entry,
        id: `next-${entry.id}`,
        sequence: (entry.sequence ?? 0) + 5,
        metadata: {
          ...entry.metadata,
          toolCallId: `next-${entry.metadata?.toolCallId}`,
        },
      })),
      journalEntry(
        "approval",
        "provider_request.opened",
        {
          provider_request_id: "request-1",
          request_kind: "permission",
          tool_name: "Bash",
          payload: {
            command: "npm test",
            description: "Run the focused tests",
          },
        },
        12,
      ),
    ];
    render(<ChatPanel {...activityProps(entries, "streaming")} />);
    expect(screen.getAllByTestId("chat-activity-group")).toHaveLength(2);
    expect(screen.getByText("Now verify the result.")).toBeVisible();
    const approval = screen.getByText("Approval requested: Bash");
    expect(approval.closest('[data-testid="chat-activity-group"]')).toBeNull();
    expect(approval).toBeVisible();
  });

  it("shows the command and the output of a journaled Bash call", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          isStreaming: false,
          messages: [
            chatMessage({
              id: "assistant-journal",
              content: "I'll watch the dev-up process in the background.",
              status: "complete",
            }),
          ],
          timelineEntries: [
            journalEntry(
              "journal-tool-start",
              "tool.started",
              {
                call_id: "toolu_1",
                name: "Bash",
                input_ref: "sha256:aaaa",
                input: { command: "just dev-up", run_in_background: true },
                input_truncated: false,
                content: "Used Bash",
              },
              1,
            ),
            journalEntry(
              "journal-tool-complete",
              "tool.completed",
              {
                call_id: "toolu_1",
                name: "Bash",
                status: "ok",
                output_ref: "sha256:bbbb",
                output: "Command running in background with ID: bh5yov60m",
                output_truncated: false,
                content: "Command running in background with ID: bh5yov60m",
              },
              2,
            ),
          ],
        })}
      />,
    );

    const toolBlock = screen.getByTestId("assistant-tool-block");
    expect(toolBlock).toHaveTextContent("Bash");
    expect(toolBlock).toHaveTextContent("done");
    expect(toolBlock).not.toHaveTextContent("ended");
    expect(toolBlock).toHaveTextContent("just dev-up");
    expect(toolBlock).toHaveTextContent(
      "Command running in background with ID: bh5yov60m",
    );
  });

  it("keeps a tool card between the text that preceded and followed it", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          isStreaming: false,
          messages: [
            chatMessage({
              id: "assistant-journal",
              content:
                "I'll watch the dev-up process in the background.Monitoring started.",
              status: "complete",
            }),
          ],
          timelineEntries: [
            journalEntry(
              "journal-delta-before",
              "message.delta",
              {
                message_id: "task-1:1",
                text_delta: "I'll watch the dev-up process in the background.",
              },
              1,
            ),
            journalEntry(
              "journal-tool-start",
              "tool.started",
              {
                call_id: "toolu_1",
                name: "Bash",
                input_ref: "sha256:aaaa",
                input: { command: "just dev-up", run_in_background: true },
                input_truncated: false,
                content: "Used Bash",
              },
              2,
            ),
            journalEntry(
              "journal-tool-complete",
              "tool.completed",
              {
                call_id: "toolu_1",
                name: "Bash",
                status: "ok",
                output_ref: "sha256:bbbb",
                output: "Command running in background",
                output_truncated: false,
                content: "Command running in background",
              },
              3,
            ),
            journalEntry(
              "journal-delta-after",
              "message.delta",
              { message_id: "task-1:1", text_delta: "Monitoring started." },
              4,
            ),
          ],
        })}
      />,
    );

    const turn = screen.getByTestId("assistant-message");
    const text = turn.textContent ?? "";
    const before = text.indexOf("I'll watch the dev-up process");
    const command = text.indexOf("just dev-up");
    const after = text.indexOf("Monitoring started.");
    expect(before).toBeGreaterThanOrEqual(0);
    expect(command).toBeGreaterThan(before);
    expect(after).toBeGreaterThan(command);
    expect(screen.getAllByTestId("assistant-tool-block")).toHaveLength(1);
    expect(text).not.toContain("background.Monitoring");
  });

  it("shows no output for a digest-only completion instead of the event kind", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          isStreaming: false,
          messages: [
            chatMessage({
              id: "assistant-journal",
              content: "Done.",
              status: "complete",
            }),
          ],
          timelineEntries: [
            journalEntry(
              "journal-old-start",
              "tool.started",
              { call_id: "toolu_9", name: "Bash", input_ref: "sha256:aaaa" },
              1,
            ),
            journalEntry(
              "journal-old-complete",
              "tool.completed",
              { call_id: "toolu_9", status: "ok", output_ref: "sha256:bbbb" },
              2,
            ),
          ],
        })}
      />,
    );

    const toolBlock = screen.getByTestId("assistant-tool-block");
    expect(toolBlock).toHaveTextContent("Bash");
    expect(toolBlock).toHaveTextContent("done");
    expect(toolBlock).not.toHaveTextContent("tool.completed");
    expect(toolBlock).not.toHaveTextContent("sha256");
  });

  it("shows a bounded string rendering of an oversized input for a shell tool", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          isStreaming: false,
          messages: [
            chatMessage({
              id: "assistant-journal",
              content: "Done.",
              status: "complete",
            }),
          ],
          timelineEntries: [
            journalEntry(
              "journal-big-start",
              "tool.started",
              {
                call_id: "toolu_8",
                name: "Bash",
                input_ref: "sha256:aaaa",
                input: '{\n  "command": "just dev-up"\n… [truncated]',
                input_truncated: true,
                content: "Used Bash",
              },
              1,
            ),
          ],
        })}
      />,
    );

    const toolBlock = screen.getByTestId("assistant-tool-block");
    expect(toolBlock).toHaveTextContent("just dev-up");
    expect(toolBlock).toHaveTextContent("[truncated]");
  });

  it("pairs a completion that names its tool even when the card was keyed by call id", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          isStreaming: false,
          messages: [
            chatMessage({
              id: "assistant-journal",
              content: "Done.",
              status: "complete",
            }),
          ],
          timelineEntries: [
            journalEntry(
              "journal-read-start",
              "tool.started",
              {
                call_id: "toolu_2",
                name: "Read",
                input_ref: "sha256:cccc",
                input: { file_path: "backend/g5d/src/main.rs" },
                input_truncated: false,
                content: "Used Read",
              },
              1,
            ),
            journalEntry(
              "journal-read-complete",
              "tool.completed",
              {
                call_id: "toolu_2",
                name: "Read",
                status: "error",
                output_ref: "sha256:dddd",
                output: "No such file",
                output_truncated: false,
                content: "No such file",
              },
              2,
            ),
          ],
        })}
      />,
    );

    const toolBlock = screen.getByTestId("assistant-tool-block");
    expect(toolBlock).toHaveTextContent("Read");
    expect(toolBlock).toHaveTextContent("backend/g5d/src/main.rs");
    expect(toolBlock).toHaveTextContent("error");
    expect(toolBlock).toHaveTextContent("No such file");
  });
});

describe("ChatPanel Claude Code manual follow-up fixes", () => {
  it("does not render capabilities updates as transcript badges", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          messages: [
            chatMessage({
              id: "assistant-capabilities",
              content: "Claude answer",
              status: "complete",
            }),
          ],
          timelineEntries: [
            timelineEntry({
              id: "capabilities-1",
              entryType: "checkpoint",
              content: "Claude Code capabilities updated",
              metadata: {
                source: "provided_agent",
                provider: "claude_code",
                providerEvent: "provided_agent.capabilities.updated",
                phase: "status",
                sessionId: "session-1",
                messageId: "assistant-capabilities",
                models: [{ value: "opus", label: "Opus" }],
              },
            }),
          ],
        })}
      />,
    );

    expect(screen.getByText("Claude answer")).toBeInTheDocument();
    expect(
      screen.queryByText("Claude capabilities updated"),
    ).not.toBeInTheDocument();
  });

  it("renders concrete Claude approval suggestion labels and hides vague suggestions", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          isStreaming: true,
          messages: [
            chatMessage({
              id: "assistant-suggestion",
              content: "",
              status: "streaming",
            }),
          ],
          timelineEntries: [
            timelineEntry({
              id: "tool-suggestion",
              entryType: "tool_call",
              content: "Claude Code used Bash",
              metadata: {
                source: "provided_agent",
                provider: "claude_code",
                providerEvent: "provided_agent.tool.started",
                phase: "tool_call",
                sessionId: "session-1",
                messageId: "assistant-suggestion",
                toolCallId: "toolu-1",
                toolName: "Bash",
                args: { command: "npm test" },
              },
              sequence: 1,
            }),
            timelineEntry({
              id: "approval-suggestion",
              entryType: "checkpoint",
              content: "Approval requested",
              metadata: {
                source: "provided_agent",
                provider: "claude_code",
                providerEvent: "provided_agent.approval.requested",
                phase: "approval",
                sessionId: "session-1",
                messageId: "assistant-suggestion",
                toolCallId: "toolu-1",
                approvalId: "approval-1",
                toolName: "Bash",
                requestedAction: "Run tests",
                input: { command: "npm test" },
                context: {
                  suggestions: [
                    {
                      rules: [
                        { toolName: "Bash", ruleContent: "Bash(npm test)" },
                      ],
                    },
                    { label: "Allow suggested rule" },
                  ],
                },
                suggestions: [
                  {
                    rules: [
                      { toolName: "Bash", ruleContent: "Bash(npm test)" },
                    ],
                  },
                  { label: "Allow suggested rule" },
                ],
              },
              sequence: 2,
              createdAt: "2026-05-10T00:00:03Z",
            }),
          ],
        })}
      />,
    );

    const suggestionButtons = screen.getAllByTestId(
      "agent-approval-suggestion",
    );
    expect(suggestionButtons).toHaveLength(1);
    expect(suggestionButtons[0]).toHaveTextContent(
      "Allow Bash: Bash(npm test)",
    );
    expect(screen.queryByText("Allow suggestion")).not.toBeInTheDocument();
    expect(screen.queryAllByText("Allow suggested rule")).toHaveLength(0);
  });

  it("synthesizes concrete approval suggestions when sidecar context omits them", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          isStreaming: true,
          messages: [
            chatMessage({
              id: "assistant-synth-suggestion",
              content: "",
              status: "streaming",
            }),
          ],
          timelineEntries: [
            timelineEntry({
              id: "approval-write",
              entryType: "checkpoint",
              content: "Approval requested",
              metadata: {
                source: "provided_agent",
                provider: "claude_code",
                providerEvent: "provided_agent.approval.requested",
                phase: "approval",
                sessionId: "session-1",
                messageId: "assistant-synth-suggestion",
                toolCallId: "toolu-write",
                approvalId: "approval-write",
                toolName: "Write",
                input: { file_path: "/tmp/hello.txt", content: "hello world" },
                context: {},
              },
              sequence: 1,
              createdAt: "2026-05-10T00:00:03Z",
            }),
          ],
        })}
      />,
    );

    const suggestionButtons = screen.getAllByTestId(
      "agent-approval-suggestion",
    );
    expect(suggestionButtons).toHaveLength(1);
    expect(suggestionButtons[0]).toHaveTextContent(
      "Allow Write: Write(/tmp/hello.txt)",
    );
  });

  it.each(["complete", "stopped", "error"] as const)(
    "keeps a deferred native question answerable after its attempt is %s",
    async (status) => {
      const onResolveAgentApproval = vi
        .fn()
        .mockResolvedValue({ deliveryDisposition: "queued" });
      const entry = projectTaskTimelineEntry(
        timelineEntry({
          id: "durable-question",
          entryType: "checkpoint",
          content: "provider_request.opened",
          metadata: {
            schema_version: "g5.task-event.v1",
            kind: "provider_request.opened",
            taskId: "task-durable",
            attempt: 1,
            sessionId: "session-1",
            messageId: "assistant-durable",
            nativeRequestStatus: "pending",
            payload: {
              provider_request_id: "toolu-durable",
              request_kind: "question",
              tool_name: "AskUserQuestion",
              deadline_unix_ms: null,
              resume_session_id: "native-session",
              display: {
                questions: [
                  {
                    question: "Choose a color",
                    options: [{ label: "Red" }, { label: "Blue" }],
                  },
                ],
              },
            },
          },
        }),
      );
      const props = defaultProps({
        session: activeSession(),
        isStreaming: false,
        onResolveAgentApproval,
        messages: [
          chatMessage({ id: "assistant-durable", content: "", status }),
        ],
        timelineEntries: [entry],
      });
      const view = render(<ChatPanel {...props} />);
      expect(screen.getByTestId("agent-question-form")).toBeVisible();
      expect(screen.getAllByText("Waiting for your answer")).toHaveLength(2);
      expect(screen.queryByText("pending approval")).not.toBeInTheDocument();
      expect(screen.queryByText("Stopped")).not.toBeInTheDocument();
      if (status === "error") {
        expect(screen.getByTestId("assistant-status-block")).toHaveTextContent(
          "Error generating response",
        );
      }
      fireEvent.click(
        screen
          .getAllByTestId("agent-question-option")
          .find((option) => option.textContent?.includes("Blue"))!,
      );
      fireEvent.click(screen.getByTestId("agent-question-submit"));
      await waitFor(() =>
        expect(onResolveAgentApproval).toHaveBeenCalledWith(
          expect.objectContaining({
            taskId: "task-durable",
            attempt: 1,
            providerRequestId: "toolu-durable",
            updatedInput: {
              g5AnswersByIndex: { "0": { selectedOptionIndexes: [1] } },
            },
          }),
        ),
      );
      view.rerender(
        <ChatPanel
          {...props}
          timelineEntries={[
            {
              ...entry,
              metadata: {
                ...entry.metadata,
                nativeRequestStatus: "reply_pending",
              },
            },
          ]}
        />,
      );
      expect(
        screen.queryByTestId("agent-question-form"),
      ).not.toBeInTheDocument();
      expect(
        screen.getByText("Answer submitted. Waiting for the agent to resume…"),
      ).toBeVisible();
      expect(screen.getAllByText("Answer submitted")).toHaveLength(2);
      expect(screen.queryByText("pending approval")).not.toBeInTheDocument();
      expect(screen.queryByText("Stopped")).not.toBeInTheDocument();

      // A reload of the old attempt has only its original opened event plus
      // the authoritative request status, not the new attempt's reply event.
      const answeredProps = {
        ...props,
        timelineEntries: [
          {
            ...entry,
            metadata: { ...entry.metadata, nativeRequestStatus: "responded" },
          },
        ],
      };
      view.rerender(<ChatPanel {...answeredProps} />);
      expect(
        screen.getByTestId("assistant-tool-block").querySelector("summary"),
      ).toHaveTextContent("Answered");
      expect(screen.queryByText("Stopped")).not.toBeInTheDocument();
      expect(screen.queryByText("pending approval")).not.toBeInTheDocument();
      view.unmount();
      const restored = render(<ChatPanel {...answeredProps} />);
      expect(
        screen.getByTestId("assistant-tool-block").querySelector("summary"),
      ).toHaveTextContent("Answered");
      expect(screen.queryByText("Stopped")).not.toBeInTheDocument();

      // A real interrupt remains visible even beside an answerable question.
      restored.rerender(
        <ChatPanel
          {...props}
          timelineEntries={[
            entry,
            timelineEntry({
              id: "explicit-stop",
              metadata: {
                source: "provided_agent",
                provider: "claude_code",
                providerEvent: "provided_agent.interrupted",
                messageId: "assistant-durable",
                reason: "user_requested",
              },
              sequence: 2,
            }),
          ]}
        />,
      );
      expect(screen.getByText("Stopped")).toBeVisible();
      restored.rerender(
        <ChatPanel
          {...props}
          timelineEntries={[
            {
              ...entry,
              metadata: {
                ...entry.metadata,
                nativeRequestStatus: "invalidated",
              },
            },
          ]}
        />,
      );
      expect(
        screen.queryByTestId("agent-question-form"),
      ).not.toBeInTheDocument();
      expect(
        screen.queryByText(
          "Answer submitted. Waiting for the agent to resume…",
        ),
      ).not.toBeInTheDocument();
      expect(
        screen.getByTestId("assistant-tool-block").querySelector("summary"),
      ).toHaveTextContent("Stopped");
      if (status === "stopped") {
        expect(screen.getByTestId("assistant-status-block")).toHaveTextContent(
          "Stopped",
        );
      }
    },
  );

  it("shows a saved-but-undelivered answer and offers sending it as a new message", () => {
    const onSendMessage = vi.fn();
    const entry = projectTaskTimelineEntry(
      timelineEntry({
        id: "undelivered-question",
        entryType: "checkpoint",
        content: "provider_request.opened",
        metadata: {
          schema_version: "g5.task-event.v1",
          kind: "provider_request.opened",
          taskId: "task-undelivered",
          attempt: 2,
          sessionId: "session-1",
          messageId: "assistant-undelivered",
          nativeRequestStatus: "invalidated",
          nativeRequestEndReason: "native_question_restore_failed",
          nativeRequestAnswer: {
            g5AnswersByIndex: { "0": { selectedOptionIndexes: [1] } },
          },
          payload: {
            provider_request_id: "toolu-undelivered",
            request_kind: "question",
            tool_name: "AskUserQuestion",
            deadline_unix_ms: null,
            resume_session_id: "native-session",
            display: {
              questions: [
                {
                  question: "Choose a color",
                  options: [{ label: "Red" }, { label: "Blue" }],
                },
              ],
            },
          },
        },
      }),
    );
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          isStreaming: false,
          onSendMessage,
          messages: [chatMessage({ id: "assistant-undelivered", content: "" })],
          timelineEntries: [entry],
        })}
      />,
    );
    // The card reads Stopped like any ended request and explains nothing;
    // the saved answer and the resend action are the only additions.
    expect(screen.getAllByText("Stopped").length).toBeGreaterThan(0);
    expect(screen.queryByText(/not delivered|not kept|could not/)).toBeNull();
    const panel = screen.getByTestId("agent-question-undelivered");
    expect(within(panel).getByText(/Blue/)).toBeInTheDocument();
    fireEvent.click(screen.getByTestId("agent-question-resend"));
    const composer = screen.getByTestId("chat-input") as HTMLTextAreaElement;
    expect(composer.value).toContain("Choose a color");
    expect(composer.value).toContain("Blue");
    // A second click on the unedited composer appends nothing.
    const seeded = composer.value;
    fireEvent.click(screen.getByTestId("agent-question-resend"));
    expect(composer.value).toBe(seeded);
    // A draft already typed stays first; the saved answer is appended once.
    fireEvent.change(composer, { target: { value: "Also note the size." } });
    fireEvent.click(screen.getByTestId("agent-question-resend"));
    expect(composer.value.startsWith("Also note the size.\n\n")).toBe(true);
    expect(composer.value).toContain("Choose a color");
    const appended = composer.value;
    fireEvent.click(screen.getByTestId("agent-question-resend"));
    expect(composer.value).toBe(appended);
    // Nothing is sent automatically.
    expect(onSendMessage).not.toHaveBeenCalled();
  });

  it("appends a different question's saved answer after a seeded one", () => {
    const undelivered = (id: string, question: string, label: string) =>
      projectTaskTimelineEntry(
        timelineEntry({
          id: `undelivered-${id}`,
          entryType: "checkpoint",
          content: "provider_request.opened",
          metadata: {
            schema_version: "g5.task-event.v1",
            kind: "provider_request.opened",
            taskId: "task-undelivered",
            attempt: 2,
            sessionId: "session-1",
            messageId: "assistant-undelivered",
            nativeRequestStatus: "invalidated",
            nativeRequestEndReason: "native_question_restore_failed",
            nativeRequestAnswer: {
              g5AnswersByIndex: { "0": { selectedOptionIndexes: [0] } },
            },
            payload: {
              provider_request_id: `toolu-${id}`,
              request_kind: "question",
              tool_name: "AskUserQuestion",
              deadline_unix_ms: null,
              resume_session_id: "native-session",
              display: { questions: [{ question, options: [{ label }] }] },
            },
          },
        }),
      );
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          isStreaming: false,
          messages: [chatMessage({ id: "assistant-undelivered", content: "" })],
          timelineEntries: [
            undelivered("a", "Choose a color", "Blue"),
            undelivered("b", "Choose a size", "Large"),
          ],
        })}
      />,
    );
    const [resendA, resendB] = screen.getAllByTestId("agent-question-resend");
    const composer = screen.getByTestId("chat-input") as HTMLTextAreaElement;
    fireEvent.click(resendA);
    expect(composer.value).toContain("Choose a color");
    expect(composer.value).not.toContain("Choose a size");
    // A different question is not a repeat: it is appended, not swallowed.
    fireEvent.click(resendB);
    expect(composer.value).toContain("Choose a color");
    expect(composer.value).toContain("Choose a size");
    const both = composer.value;
    fireEvent.click(resendB);
    expect(composer.value).toBe(both);
  });

  it("shows a lost deferred question as stopped, without explanation or resend", () => {
    const entry = projectTaskTimelineEntry(
      timelineEntry({
        id: "lost-question",
        entryType: "checkpoint",
        content: "provider_request.opened",
        metadata: {
          schema_version: "g5.task-event.v1",
          kind: "provider_request.opened",
          taskId: "task-lost",
          attempt: 1,
          sessionId: "session-1",
          messageId: "assistant-lost",
          nativeRequestStatus: "invalidated",
          nativeRequestEndReason: "native_session_advanced",
          payload: {
            provider_request_id: "toolu-lost",
            request_kind: "question",
            tool_name: "AskUserQuestion",
            deadline_unix_ms: null,
            resume_session_id: "native-session",
            display: {
              questions: [
                {
                  question: "Choose a color",
                  options: [{ label: "Red" }, { label: "Blue" }],
                },
              ],
            },
          },
        },
      }),
    );
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          isStreaming: false,
          messages: [chatMessage({ id: "assistant-lost", content: "" })],
          timelineEntries: [entry],
        })}
      />,
    );
    expect(screen.getAllByText("Stopped").length).toBeGreaterThan(0);
    expect(screen.queryByText(/not kept|could not|unanswered/)).toBeNull();
    expect(
      screen.queryByTestId("agent-question-resend"),
    ).not.toBeInTheDocument();
  });

  it("submits native permission decisions to the attempt-bound request", async () => {
    const onResolveAgentApproval = vi.fn().mockResolvedValue({
      deliveryDisposition: "delivered",
    });
    const entry = projectTaskTimelineEntry(
      timelineEntry({
        id: "native-permission",
        entryType: "checkpoint",
        content: "provider_request.opened",
        metadata: {
          schema_version: "g5.task-event.v1",
          kind: "provider_request.opened",
          taskId: "task-native-2",
          attempt: 5,
          sessionId: "session-1",
          messageId: "assistant-native-permission",
          payload: {
            provider_request_id: "native-permission-1",
            request_kind: "permission",
            tool_name: "Bash",
            display: { input: { command: "npm test" } },
            deadline_unix_ms: 1_800_000_000_000,
          },
        },
      }),
    );
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          isStreaming: true,
          onResolveAgentApproval,
          messages: [
            chatMessage({
              id: "assistant-native-permission",
              content: "",
              status: "streaming",
            }),
          ],
          timelineEntries: [entry],
        })}
      />,
    );

    expect(screen.getAllByText("pending approval")).toHaveLength(2);
    expect(
      screen.queryByText("Waiting for your answer"),
    ).not.toBeInTheDocument();
    fireEvent.click(screen.getByTestId("agent-approval-reject"));
    expect(onResolveAgentApproval).toHaveBeenCalledWith({
      sessionId: "session-1",
      approvalId: "native-permission-1",
      decision: "rejected",
      taskId: "task-native-2",
      attempt: 5,
      providerRequestId: "native-permission-1",
      requestKind: "permission",
      reason: undefined,
      updatedInput: undefined,
      updatedPermissions: undefined,
      interrupt: undefined,
    });
    expect(
      screen.queryByTestId("agent-approval-deny-stop"),
    ).not.toBeInTheDocument();
    await waitFor(() =>
      expect(screen.getByTestId("agent-approval-reject")).toBeEnabled(),
    );
  });

  it("keeps a rejected permission request actionable and shows the delivery error", async () => {
    const onResolveAgentApproval = vi
      .fn()
      .mockRejectedValue(new Error("Approval relay is unavailable"));
    const entry = projectTaskTimelineEntry(
      timelineEntry({
        id: "native-permission-error",
        entryType: "checkpoint",
        content: "provider_request.opened",
        metadata: {
          schema_version: "g5.task-event.v1",
          kind: "provider_request.opened",
          taskId: "task-native-error",
          attempt: 2,
          sessionId: "session-1",
          messageId: "assistant-native-permission-error",
          payload: {
            provider_request_id: "native-permission-error-1",
            request_kind: "permission",
            tool_name: "Bash",
            display: { input: { command: "npm test" } },
            deadline_unix_ms: 1_800_000_000_000,
          },
        },
      }),
    );
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          isStreaming: true,
          onResolveAgentApproval,
          messages: [
            chatMessage({
              id: "assistant-native-permission-error",
              content: "",
              status: "streaming",
            }),
          ],
          timelineEntries: [entry],
        })}
      />,
    );

    const reject = screen.getByTestId("agent-approval-reject");
    fireEvent.click(reject);

    await waitFor(() =>
      expect(screen.getByText("Approval relay is unavailable")).toBeVisible(),
    );
    expect(reject).toBeEnabled();
    fireEvent.click(reject);
    await waitFor(() =>
      expect(onResolveAgentApproval).toHaveBeenCalledTimes(2),
    );
  });

  it("keeps Claude Mode read-only and auto-persists only mutable next-turn settings", async () => {
    const onSendProviderControl = vi.fn().mockResolvedValue(undefined);
    const onSendMessage = vi.fn().mockResolvedValue(true);
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession({
            providerState: {
              claudeCode: {
                settings: {
                  permissionMode: "bypassPermissions",
                  model: "claude-sonnet-4-6",
                  effort: "high",
                },
              },
            },
          }),
          isStreaming: false,
          onSendMessage,
          onSendProviderControl,
        })}
      />,
    );

    expect(screen.getByTestId("claude-permission-control")).toHaveTextContent(
      /Bypass/i,
    );
    expect(screen.getByTestId("claude-model-control")).toHaveTextContent(
      /Sonnet 4\.6/i,
    );

    fireEvent.change(screen.getByTestId("chat-input"), {
      target: { value: "continue" },
    });
    fireEvent.keyDown(screen.getByTestId("chat-input"), { key: "Enter" });
    await waitFor(() => expect(onSendMessage).toHaveBeenCalled());
    expect(onSendMessage.mock.calls[0][1]).toMatchObject({
      model: "claude-sonnet-4-6",
      effort: "high",
    });

    fireEvent.click(screen.getByTestId("claude-settings-control"));
    const settings = screen.getByTestId("claude-settings-modal");
    expect(within(settings).getByText("Mode")).toBeInTheDocument();
    expect(screen.getByTestId("chat-agent-select")).toBeDisabled();
    expect(
      within(settings).getByText(/Agent and Mode are fixed for this chat/i),
    ).toBeInTheDocument();
    expect(
      screen.queryByTestId("chat-provider-settings-actions"),
    ).not.toBeInTheDocument();
    expect(screen.getByTestId("chat-mode-select")).toHaveAttribute("readonly");
    expect(screen.getByTestId("chat-mode-select")).toHaveDisplayValue("Bypass");
    chooseThemedOption("chat-model-select", "Fable 5.2");
    await waitFor(() => expect(onSendProviderControl).toHaveBeenCalledTimes(1));
    await waitFor(() =>
      expect(screen.getByTestId("chat-effort-select")).not.toBeDisabled(),
    );
    chooseThemedOption("chat-effort-select", "Max");
    await waitFor(() => expect(onSendProviderControl).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(onSendProviderControl).toHaveBeenCalledTimes(2));
    expect(onSendProviderControl).toHaveBeenLastCalledWith({
      sessionId: "session-1",
      action: "set_settings",
      payload: {
        model: "claude-fable-5-2",
        effort: "max",
      },
    });
    expect(screen.getByTestId("claude-settings-modal")).toBeInTheDocument();
    fireEvent.mouseDown(document.body);
    expect(
      screen.queryByTestId("claude-settings-modal"),
    ).not.toBeInTheDocument();
  });

  it("keeps the selected model when the fresh session lacks provider-state settings (issue #819)", async () => {
    const onSendMessage = vi.fn().mockResolvedValue(true);
    const { rerender } = render(
      <ChatPanel {...defaultProps({ onSendMessage })} />,
    );

    fireEvent.click(screen.getByTestId("claude-settings-control"));
    await waitFor(() =>
      expect(screen.getByTestId("claude-settings-modal")).toBeInTheDocument(),
    );
    chooseThemedOption("chat-model-select", "Fable 5.2");
    fireEvent.pointerDown(document.body);
    expect(screen.getByTestId("claude-model-control")).toHaveTextContent(
      "Fable 5.2",
    );

    // After send, chat_sessions.model already carries the selection while the
    // worker has not yet recorded providerState.claudeCode.settings
    // (right-panel D30): the composer must not flash the workspace default.
    rerender(
      <ChatPanel
        {...defaultProps({
          onSendMessage,
          session: activeSession({ model: "claude-fable-5-2" }),
        })}
      />,
    );

    expect(screen.getByTestId("claude-model-control")).toHaveTextContent(
      "Fable 5.2",
    );

    fireEvent.change(screen.getByTestId("chat-input"), {
      target: { value: "follow-up" },
    });
    fireEvent.keyDown(screen.getByTestId("chat-input"), { key: "Enter" });
    await waitFor(() => expect(onSendMessage).toHaveBeenCalled());
    expect(onSendMessage.mock.calls[0][1]).toMatchObject({
      model: "claude-fable-5-2",
    });
  });

  it("rehydrates when the session-persisted model changes without provider state", async () => {
    const onSendMessage = vi.fn().mockResolvedValue(true);
    const { rerender } = render(
      <ChatPanel
        {...defaultProps({
          onSendMessage,
          session: activeSession({ model: "claude-fable-5" }),
        })}
      />,
    );
    expect(screen.getByTestId("claude-model-control")).toHaveTextContent(
      "Fable 5",
    );

    // Same session id: chat_sessions.model changed (e.g. a send from another
    // tab) while provider state has not been seeded yet — the composer must
    // reflect the persisted model, not resubmit its stale local value
    // (right-panel D30).
    rerender(
      <ChatPanel
        {...defaultProps({
          onSendMessage,
          session: activeSession({ model: "claude-sonnet-4-6" }),
        })}
      />,
    );
    expect(screen.getByTestId("claude-model-control")).toHaveTextContent(
      "Sonnet 4.6",
    );

    fireEvent.change(screen.getByTestId("chat-input"), {
      target: { value: "follow-up" },
    });
    fireEvent.keyDown(screen.getByTestId("chat-input"), { key: "Enter" });
    await waitFor(() => expect(onSendMessage).toHaveBeenCalled());
    expect(onSendMessage.mock.calls[0][1]).toMatchObject({
      model: "claude-sonnet-4-6",
    });
  });

  it("prefers provider-state settings over the session-persisted model when both exist", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession({
            model: "claude-fable-5[1m]",
            providerState: {
              claudeCode: { settings: { model: "claude-sonnet-4-6" } },
            },
          }),
        })}
      />,
    );

    expect(screen.getByTestId("claude-model-control")).toHaveTextContent(
      /Sonnet 4\.6/i,
    );
  });

  it("does not render Claude token usage before authoritative sidecar accounting", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          timelineEntries: [
            timelineEntry({
              id: "usage-1",
              entryType: "checkpoint",
              content: "Claude Code usage updated",
              metadata: {
                source: "provided_agent",
                provider: "claude_code",
                providerEvent: "provided_agent.usage.updated",
                phase: "status",
                sessionId: "session-1",
                messageId: "message-1",
                usage: {
                  input_tokens: 26,
                  cache_read_input_tokens: 10_000,
                  cache_creation_input_tokens: 512,
                  output_tokens: 8,
                },
              },
            }),
          ],
        })}
      />,
    );

    fireEvent.click(screen.getByTestId("claude-settings-control"));

    expect(
      screen.queryByTestId("claude-input-token-count"),
    ).not.toBeInTheDocument();
    expect(screen.queryByText(/Input Tokens:/i)).not.toBeInTheDocument();
  });

  it("renders Claude setMode and addDirectories permission suggestions", async () => {
    const onResolveAgentApproval = vi.fn().mockResolvedValue(undefined);
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          isStreaming: true,
          onResolveAgentApproval,
          messages: [
            chatMessage({
              id: "assistant-structured-suggestion",
              content: "",
              status: "streaming",
            }),
          ],
          timelineEntries: [
            timelineEntry({
              id: "approval-structured-suggestion",
              entryType: "checkpoint",
              content: "Approval requested",
              metadata: {
                source: "provided_agent",
                provider: "claude_code",
                providerEvent: "provided_agent.approval.requested",
                phase: "approval",
                sessionId: "session-1",
                messageId: "assistant-structured-suggestion",
                toolCallId: "toolu-structured",
                approvalId: "approval-structured",
                toolName: "Bash",
                input: { command: "touch essai.txt" },
                suggestions: [
                  {
                    type: "addDirectories",
                    destination: "session",
                    directories: ["/tmp/g5-chat-worktree"],
                  },
                  {
                    type: "setMode",
                    mode: "acceptEdits",
                    destination: "session",
                  },
                ],
              },
              sequence: 1,
            }),
          ],
        })}
      />,
    );

    const allowDir = screen.getByRole("button", { name: /^allow dir$/i });
    expect(allowDir).toBeInTheDocument();
    expect(allowDir).toHaveAttribute(
      "title",
      "Allow directory: /tmp/g5-chat-worktree (this session)",
    );
    const acceptEdits = screen
      .getAllByTestId("agent-approval-suggestion")
      .find((button) => button.textContent === "Accept edits");
    expect(acceptEdits).toBeDefined();
    if (!acceptEdits) {
      throw new Error("Accept edits suggestion was not rendered");
    }
    expect(acceptEdits).toBeInTheDocument();

    fireEvent.click(acceptEdits);
    expect(onResolveAgentApproval).toHaveBeenCalledWith({
      sessionId: "session-1",
      approvalId: "approval-structured",
      decision: "approved",
      reason: "Accepted agent suggestion.",
      updatedInput: undefined,
      updatedPermissions: [
        { type: "setMode", mode: "acceptEdits", destination: "session" },
      ],
      interrupt: undefined,
    });
    await waitFor(() => expect(acceptEdits).toBeEnabled());
  });

  it("does not duplicate pending approval arguments outside the approval panel", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          isStreaming: true,
          messages: [
            chatMessage({
              id: "assistant-approval-dedupe",
              content: "",
              status: "streaming",
            }),
          ],
          timelineEntries: [
            timelineEntry({
              id: "tool-write-dedupe",
              entryType: "tool_call",
              content: "Claude Code used Write",
              metadata: {
                source: "provided_agent",
                provider: "claude_code",
                providerEvent: "provided_agent.tool.started",
                phase: "tool_call",
                sessionId: "session-1",
                messageId: "assistant-approval-dedupe",
                toolCallId: "toolu-write",
                toolName: "Write",
                args: { file_path: "/tmp/hello.txt", content: "hello world" },
              },
              sequence: 1,
            }),
            timelineEntry({
              id: "approval-write-dedupe",
              entryType: "checkpoint",
              content: "Approval requested",
              metadata: {
                source: "provided_agent",
                provider: "claude_code",
                providerEvent: "provided_agent.approval.requested",
                phase: "approval",
                sessionId: "session-1",
                messageId: "assistant-approval-dedupe",
                toolCallId: "toolu-write",
                approvalId: "approval-write",
                toolName: "Write",
                input: { file_path: "/tmp/hello.txt", content: "hello world" },
              },
              sequence: 2,
              createdAt: "2026-05-10T00:00:03Z",
            }),
          ],
        })}
      />,
    );

    const toolBlock = screen.getByTestId("assistant-tool-block");
    expect(within(toolBlock).getAllByText("hello world")).toHaveLength(1);
  });

  it("hides internal G5 subagent preamble hook dumps from Claude output", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          isStreaming: false,
          messages: [
            chatMessage({
              id: "assistant-hook-dump",
              content: "",
              status: "complete",
            }),
          ],
          timelineEntries: [
            timelineEntry({
              id: "delta-hook-dump",
              content:
                "=== G5_PREAMBLE_V1 - READ IN FULL BEFORE ANY ACTION ===\n\nYou are a subagent in a G5 ontology-driven repo. Read G5_SUBAGENT_BOOTSTRAP.md.",
              metadata: {
                source: "provided_agent",
                provider: "claude_code",
                providerEvent: "provided_agent.output.delta",
                phase: "output",
                sessionId: "session-1",
                messageId: "assistant-hook-dump",
                blockId: "claude-code-hook-text-1",
                streamPersistence: "semantic",
              },
              sequence: 1,
            }),
            timelineEntry({
              id: "delta-real-output",
              content: "Actual assistant response.",
              metadata: {
                source: "provided_agent",
                provider: "claude_code",
                providerEvent: "provided_agent.output.delta",
                phase: "output",
                sessionId: "session-1",
                messageId: "assistant-hook-dump",
                blockId: "claude-code-hook-text-2",
                streamPersistence: "semantic",
              },
              sequence: 2,
            }),
          ],
        })}
      />,
    );

    expect(screen.queryByText(/G5_PREAMBLE_V1/)).not.toBeInTheDocument();
    expect(screen.getByText("Actual assistant response.")).toBeInTheDocument();
  });

  it("keeps Claude text, tool, and later text in order without duplicating final output", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          isStreaming: false,
          messages: [
            chatMessage({
              id: "assistant-linear",
              content: "Before tool.\n\nAfter tool.",
              status: "complete",
            }),
          ],
          timelineEntries: [
            timelineEntry({
              id: "delta-before",
              content: "Before tool.",
              metadata: {
                source: "provided_agent",
                provider: "claude_code",
                providerEvent: "provided_agent.output.delta",
                phase: "output",
                sessionId: "session-1",
                messageId: "assistant-linear",
                blockId: "claude-code-assistant-linear-text-1",
                streamPersistence: "semantic",
              },
              sequence: 1,
              createdAt: "2026-05-10T00:00:01Z",
            }),
            timelineEntry({
              id: "tool-linear",
              entryType: "tool_call",
              content: "Claude Code used Bash",
              metadata: {
                source: "provided_agent",
                provider: "claude_code",
                providerEvent: "provided_agent.tool.started",
                phase: "tool_call",
                sessionId: "session-1",
                messageId: "assistant-linear",
                toolCallId: "toolu-linear",
                toolName: "Bash",
                args: { command: "pwd" },
              },
              sequence: 2,
              createdAt: "2026-05-10T00:00:02Z",
            }),
            timelineEntry({
              id: "delta-after",
              content: "After tool.",
              metadata: {
                source: "provided_agent",
                provider: "claude_code",
                providerEvent: "provided_agent.output.delta",
                phase: "output",
                sessionId: "session-1",
                messageId: "assistant-linear",
                blockId: "claude-code-assistant-linear-text-2",
                streamPersistence: "semantic",
              },
              sequence: 3,
              createdAt: "2026-05-10T00:00:03Z",
            }),
            timelineEntry({
              id: "final-linear",
              content: "Before tool.\n\nAfter tool.",
              metadata: {
                source: "provided_agent",
                provider: "claude_code",
                providerEvent: "provided_agent.output.final",
                phase: "output",
                sessionId: "session-1",
                messageId: "assistant-linear",
                blockId: "claude-code-assistant-linear-final",
              },
              sequence: 4,
              createdAt: "2026-05-10T00:00:04Z",
            }),
          ],
        })}
      />,
    );

    const assistant = screen.getByTestId("assistant-message");
    const text = assistant.textContent ?? "";
    expect(text.indexOf("Before tool.")).toBeGreaterThanOrEqual(0);
    expect(text.indexOf("Bash")).toBeGreaterThan(text.indexOf("Before tool."));
    expect(text.indexOf("After tool.")).toBeGreaterThan(text.indexOf("Bash"));
    expect((text.match(/Before tool\./g) ?? []).length).toBe(1);
    expect((text.match(/After tool\./g) ?? []).length).toBe(1);
  });

  it("projects Claude steering output over the transport acknowledgement", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          isStreaming: false,
          messages: [
            chatMessage({
              id: "user-1",
              role: "user",
              content: "Draft the README.",
              status: "complete",
              messageIndex: 0,
              createdAt: "2026-05-10T00:00:01Z",
            }),
            chatMessage({
              id: "assistant-1",
              content: "I will update it.",
              status: "complete",
              messageIndex: 1,
              createdAt: "2026-05-10T00:00:02Z",
            }),
            chatMessage({
              id: "user-2",
              role: "user",
              content:
                "great, can put your rough description of G5 in this file?",
              status: "complete",
              messageIndex: 2,
              createdAt: "2026-05-10T00:00:03Z",
            }),
            chatMessage({
              id: "assistant-2",
              content: "Steering message sent to Claude Code.",
              status: "complete",
              messageIndex: 3,
              createdAt: "2026-05-10T00:00:04Z",
            }),
          ],
          timelineEntries: [
            timelineEntry({
              id: "steered-output",
              content: "README updated with the G5 description.",
              metadata: {
                source: "provided_agent",
                provider: "claude_code",
                providerEvent: "provided_agent.output.delta",
                phase: "output",
                sessionId: "session-1",
                messageId: "assistant-2",
                blockId: "claude-code-assistant-2-text-1",
                streamPersistence: "semantic",
              },
              sequence: 1,
              createdAt: "2026-05-10T00:00:05Z",
            }),
          ],
        })}
      />,
    );

    expect(
      screen.queryByText("Steering message sent to Claude Code."),
    ).not.toBeInTheDocument();
    expect(
      screen.getByText("README updated with the G5 description."),
    ).toBeInTheDocument();

    const transcript = screen.getByTestId("chat-messages").textContent ?? "";
    expect(transcript.indexOf("great, can put")).toBeLessThan(
      transcript.indexOf("README updated"),
    );
  });
});

describe("ChatPanel Claude command layout stability", () => {
  it("uses the outer message list as the vertical scroll owner for Claude tool blocks", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          isStreaming: true,
          messages: [
            chatMessage({
              id: "assistant-scroll-owner",
              content: "",
              status: "streaming",
            }),
          ],
          timelineEntries: [
            timelineEntry({
              id: "tool-scroll-owner",
              entryType: "tool_call",
              content: "Claude Code used Bash",
              metadata: {
                source: "provided_agent",
                provider: "claude_code",
                providerEvent: "provided_agent.tool.started",
                phase: "tool_call",
                sessionId: "session-1",
                messageId: "assistant-scroll-owner",
                toolCallId: "toolu-scroll-owner",
                toolName: "Bash",
                args: { command: "echo short-command" },
              },
              sequence: 1,
            }),
          ],
        })}
      />,
    );

    expect(screen.getByTestId("chat-messages")).toHaveClass("overflow-y-auto");

    const toolBlock = screen.getByTestId("assistant-tool-block");
    expect(toolBlock).toHaveClass("w-full");

    const preBlocks = toolBlock.querySelectorAll("pre");
    expect(preBlocks.length).toBeGreaterThan(0);
    for (const pre of preBlocks) {
      expect(pre.className).not.toMatch(/max-h-/);
      expect(pre.className).not.toContain("overflow-auto");
      expect(pre.className).toContain("whitespace-pre-wrap");
      expect(pre.className).toContain("break-words");
    }
  });

  it("renders short Claude command turns at a stable capped bubble width", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          isStreaming: true,
          messages: [
            chatMessage({
              id: "assistant-width-stable",
              content: "",
              status: "streaming",
            }),
          ],
          timelineEntries: [
            timelineEntry({
              id: "tool-width-stable",
              entryType: "tool_call",
              content: "Claude Code used Bash",
              metadata: {
                source: "provided_agent",
                provider: "claude_code",
                providerEvent: "provided_agent.tool.started",
                phase: "tool_call",
                sessionId: "session-1",
                messageId: "assistant-width-stable",
                toolCallId: "toolu-width-stable",
                toolName: "Bash",
                args: { command: "pwd" },
              },
              sequence: 1,
            }),
          ],
        })}
      />,
    );

    const assistant = screen.getByTestId("assistant-message");
    const bubble = assistant.firstElementChild;
    expect(bubble).toHaveClass("max-w-[min(100%,8in)]");
    expect(bubble).toHaveClass("min-w-0");
    expect(screen.getByTestId("assistant-tool-block")).toHaveClass("w-full");
  });

  it("constrains collapsed Claude tool headers so long tool names ellipsize", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          isStreaming: true,
          messages: [
            chatMessage({
              id: "assistant-long-tool-name",
              content: "",
              status: "streaming",
            }),
          ],
          timelineEntries: [
            timelineEntry({
              id: "tool-long-name",
              entryType: "tool_call",
              content: "Claude Code used MCP",
              metadata: {
                source: "provided_agent",
                provider: "claude_code",
                providerEvent: "provided_agent.tool.started",
                phase: "tool_call",
                sessionId: "session-1",
                messageId: "assistant-long-tool-name",
                toolCallId: "toolu-long-name",
                toolName:
                  "mcp__g5_archive_intent_grounding__register_chat_intent",
                input: { chat_session_id: "session-with-a-long-name" },
              },
              sequence: 1,
            }),
          ],
        })}
      />,
    );

    const toolBlock = screen.getByTestId("assistant-tool-block");
    const summary = toolBlock.querySelector("summary");
    expect(summary).not.toBeNull();
    expect(summary).toHaveClass("min-w-0");
    expect(summary).toHaveClass("overflow-hidden");

    const toolName = summary?.querySelector("span");
    expect(toolName).not.toBeNull();
    expect(toolName).toHaveClass("min-w-0");
    expect(toolName).toHaveClass("truncate");
  });

  it("constrains metadata tool-call badges so long names ellipsize", () => {
    const longToolName =
      "mcp__g5_archive_intent_grounding__register_chat_intent_with_extra_suffix";
    const message = {
      ...chatMessage({
        id: "assistant-metadata-tool-call",
        content: "Done.",
        status: "complete",
      }),
      toolCalls: [
        {
          name: longToolName,
          arguments: {},
          status: "complete",
        },
      ],
    } as ChatMessageDTO & {
      toolCalls: Array<{
        name: string;
        arguments: Record<string, unknown>;
        status: string;
      }>;
    };

    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          messages: [message],
        })}
      />,
    );

    const toolName = screen
      .getByTestId("assistant-tool-block")
      .querySelector("summary span")!;
    expect(toolName).toHaveClass("min-w-0");
    expect(toolName).toHaveClass("truncate");
    expect(toolName.parentElement).toHaveClass("min-w-0");
    expect(toolName.parentElement).toHaveClass("overflow-hidden");
  });
});

describe("ChatPanel Codex-native controls (codex_support I12/I13/D5)", () => {
  function codexSession(
    overrides: Partial<ChatSessionDTO> = {},
  ): ChatSessionDTO {
    return activeSession({
      id: "codex-session",
      agentProvider: "codex",
      ...overrides,
    });
  }

  it("renders codex-controls (not claude-code-controls) for a Codex session", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: codexSession(),
          sessions: [codexSession()],
          availableAgentProviders: ["claude_code", "codex"],
        })}
      />,
    );

    expect(screen.getByTestId("codex-controls")).toBeInTheDocument();
    expect(
      screen.queryByTestId("claude-code-controls"),
    ).not.toBeInTheDocument();
  });

  it("renders claude-code-controls (not codex-controls) for a Claude session — AC-13", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession({ agentProvider: "claude_code" }),
          sessions: [activeSession({ agentProvider: "claude_code" })],
          availableAgentProviders: ["claude_code", "codex"],
        })}
      />,
    );

    expect(screen.getByTestId("claude-code-controls")).toBeInTheDocument();
    expect(screen.queryByTestId("codex-controls")).not.toBeInTheDocument();
  });

  it("shows the harness-forced Codex mode as read-only", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: null,
          selectedAgentProvider: "codex",
          availableAgentProviders: ["claude_code", "codex"],
        })}
      />,
    );

    // The composer and popup both show the launch mode the harness enforces.
    expect(screen.getByTestId("codex-permission-control")).toHaveTextContent(
      /Full Access/,
    );

    fireEvent.click(screen.getByTestId("codex-settings-control"));
    expect(screen.getByTestId("chat-mode-select")).toHaveAttribute("readonly");
    expect(screen.getByTestId("chat-mode-select")).toBeEnabled();
    expect(screen.getByTestId("chat-mode-select")).toHaveDisplayValue(
      "Full Access",
    );
  });

  it("populates the per-model catalog effort set and selects its default", () => {
    // Right-panel D24 (plan Story 2): the effort group renders the selected
    // model's catalog set — for the gpt-5.6-sol default (Story 4 bump) that
    // is low…ultra; `minimal` is no longer offered (persisted legacy values
    // still display via fallback).
    render(
      <ChatPanel
        {...defaultProps({
          session: null,
          selectedAgentProvider: "codex",
          availableAgentProviders: ["claude_code", "codex"],
        })}
      />,
    );

    fireEvent.click(screen.getByTestId("codex-settings-control"));
    fireEvent.click(screen.getByTestId("chat-effort-select"));
    const efforts = screen.getAllByRole("option");
    expect(efforts).toHaveLength(6);
    for (const level of ["low", "medium", "high", "xhigh", "max", "ultra"]) {
      expect(
        screen.getByTestId(`chat-effort-select-option-${level}`),
      ).toBeInTheDocument();
    }
    expect(
      screen.getByTestId("chat-effort-select-option-xhigh"),
    ).toHaveTextContent("XHigh");
    expect(
      screen.queryByTestId("chat-effort-select-option-minimal"),
    ).not.toBeInTheDocument();
    expect(screen.getByTestId("chat-effort-select")).toHaveTextContent("High");
  });

  it("updates Codex effort during streaming with one composite next-turn request", async () => {
    const onSendProviderControl = vi.fn().mockResolvedValue(undefined);
    render(
      <ChatPanel
        {...defaultProps({
          session: codexSession(),
          sessions: [codexSession()],
          availableAgentProviders: ["claude_code", "codex"],
          isStreaming: true,
          onSendProviderControl,
        })}
      />,
    );

    fireEvent.click(screen.getByTestId("codex-settings-control"));
    chooseThemedOption("chat-effort-select", "Ultra");
    await waitFor(() => expect(onSendProviderControl).toHaveBeenCalledTimes(1));
    expect(onSendProviderControl).toHaveBeenCalledWith({
      sessionId: "codex-session",
      action: "set_settings",
      payload: {
        model: "gpt-5.6-sol",
        effort: "ultra",
      },
    });
  });

  it("offers the full Codex model catalog without a deployment prop", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: null,
          selectedAgentProvider: "codex",
          availableAgentProviders: ["claude_code", "codex"],
        })}
      />,
    );

    expect(screen.getByTestId("codex-model-control")).toHaveTextContent(
      "GPT 5.6 Sol",
    );
    fireEvent.click(screen.getByTestId("codex-settings-control"));
    fireEvent.click(screen.getByTestId("chat-model-select"));
    const models = screen.getAllByRole("option");
    expect(models).toHaveLength(8);
    expect(
      screen.getByTestId("chat-model-select-option-gpt-5.6-sol"),
    ).toBeInTheDocument();
  });

  it("keeps auto-persisted Codex effort across outside click and Escape", async () => {
    const onSendProviderControl = vi.fn().mockResolvedValue(undefined);
    render(
      <ChatPanel
        {...defaultProps({
          session: codexSession(),
          sessions: [codexSession()],
          onSendProviderControl,
        })}
      />,
    );

    fireEvent.click(screen.getByTestId("codex-settings-control"));
    expect(screen.getByTestId("codex-settings-modal")).toBeInTheDocument();
    expect(screen.getByTestId("chat-mode-select")).toHaveAttribute("readonly");
    chooseThemedOption("chat-effort-select", "Ultra");
    await waitFor(() => expect(onSendProviderControl).toHaveBeenCalledTimes(1));
    await waitFor(() =>
      expect(screen.getByTestId("chat-effort-select")).not.toBeDisabled(),
    );

    fireEvent.mouseDown(document.body);
    expect(
      screen.queryByTestId("codex-settings-modal"),
    ).not.toBeInTheDocument();

    fireEvent.click(screen.getByTestId("codex-settings-control"));
    expect(screen.getByTestId("codex-settings-modal")).toBeInTheDocument();
    expect(screen.getByTestId("chat-effort-select")).toHaveTextContent("Ultra");
    chooseThemedOption("chat-effort-select", "High");
    await waitFor(() => expect(onSendProviderControl).toHaveBeenCalledTimes(2));
    await waitFor(() =>
      expect(screen.getByTestId("chat-effort-select")).not.toBeDisabled(),
    );

    fireEvent.keyDown(document, { key: "Escape" });
    expect(
      screen.queryByTestId("codex-settings-modal"),
    ).not.toBeInTheDocument();
    fireEvent.click(screen.getByTestId("codex-settings-control"));
    expect(screen.getByTestId("chat-effort-select")).toHaveTextContent("High");
    expect(screen.getByTestId("chat-mode-select")).toHaveDisplayValue(
      "Not set",
    );
    expect(onSendProviderControl).toHaveBeenCalledTimes(2);
  });

  it("portals the Codex settings popover above workspace chrome", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: codexSession(),
          sessions: [codexSession()],
        })}
      />,
    );

    const trigger = screen.getByTestId("codex-settings-control");
    const controls = screen.getByTestId("codex-controls");
    vi.spyOn(trigger, "getBoundingClientRect").mockReturnValue({
      bottom: 700,
      height: 32,
      left: 600,
      right: 872,
      top: 668,
      width: 272,
      x: 600,
      y: 668,
      toJSON: () => ({}),
    });
    vi.spyOn(controls, "getBoundingClientRect").mockReturnValue({
      bottom: 700,
      height: 32,
      left: 592,
      right: 872,
      top: 668,
      width: 280,
      x: 592,
      y: 668,
      toJSON: () => ({}),
    });

    fireEvent.click(trigger);
    const modal = screen.getByTestId("codex-settings-modal");

    expect(modal.parentElement).toBe(document.body);
    expect(modal.className).toContain("fixed");
    expect(modal.className).toContain("z-[9999]");
    expect(modal.style.bottom).not.toBe("");
    expect(modal.style.right).not.toBe("");
    expect(modal.style.width).toBe("280px");
  });

  it("restores the persisted effort when auto-save fails", async () => {
    const onSendProviderControl = vi
      .fn()
      .mockRejectedValue(new Error("save failed"));
    render(
      <ChatPanel
        {...defaultProps({
          session: codexSession(),
          sessions: [codexSession()],
          onSendProviderControl,
        })}
      />,
    );

    fireEvent.click(screen.getByTestId("codex-settings-control"));
    chooseThemedOption("chat-effort-select", "Ultra");
    await screen.findByText("save failed");
    expect(screen.getByTestId("chat-effort-select")).toHaveTextContent("XHigh");
    expect(screen.getByTestId("chat-mode-select")).toHaveDisplayValue(
      "Not set",
    );
    expect(onSendProviderControl).toHaveBeenCalledTimes(1);
    expect(
      screen.queryByTestId("chat-provider-settings-actions"),
    ).not.toBeInTheDocument();
  });

  it("restores the persisted effort when provider settings are unavailable", async () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: codexSession(),
          sessions: [codexSession()],
          onSendProviderControl: undefined,
        })}
      />,
    );

    fireEvent.click(screen.getByTestId("codex-settings-control"));
    chooseThemedOption("chat-effort-select", "Ultra");

    await screen.findByText("Provider settings are unavailable.");
    expect(screen.getByTestId("chat-effort-select")).toHaveTextContent("XHigh");
    expect(screen.getByTestId("chat-mode-select")).toHaveDisplayValue(
      "Not set",
    );
  });

  it("ignores an old session's rejected save after selecting another chat", async () => {
    let rejectSave: (reason: Error) => void = () => undefined;
    const pendingSave = new Promise<void>((_resolve, reject) => {
      rejectSave = reject;
    });
    const onSendProviderControl = vi.fn().mockReturnValue(pendingSave);
    const sessionA = codexSession({
      id: "codex-session-a",
      providerState: {
        codex: {
          settings: {
            model: "gpt-5.6-sol",
            permissionMode: "on-failure/workspace-write",
            effort: "xhigh",
          },
        },
      },
    });
    const sessionB = codexSession({
      id: "codex-session-b",
      providerState: {
        codex: {
          settings: {
            model: "gpt-5.5",
            permissionMode: "on-request/read-only",
            effort: "low",
          },
        },
      },
    });
    const { rerender } = render(
      <ChatPanel
        {...defaultProps({
          session: sessionA,
          sessions: [sessionA, sessionB],
          onSendProviderControl,
        })}
      />,
    );

    fireEvent.click(screen.getByTestId("codex-settings-control"));
    chooseThemedOption("chat-effort-select", "Ultra");
    await waitFor(() => expect(onSendProviderControl).toHaveBeenCalledTimes(1));

    rerender(
      <ChatPanel
        {...defaultProps({
          session: sessionB,
          sessions: [sessionA, sessionB],
          onSendProviderControl,
        })}
      />,
    );
    await waitFor(() =>
      expect(screen.getByTestId("chat-effort-select")).toHaveTextContent("Low"),
    );

    await act(async () => {
      rejectSave(new Error("stale save failed"));
      await Promise.resolve();
    });
    expect(
      screen.queryByTestId("chat-provider-settings-error"),
    ).not.toBeInTheDocument();
    expect(screen.getByTestId("chat-effort-select")).toHaveTextContent("Low");
    expect(screen.getByTestId("chat-mode-select")).toHaveDisplayValue(
      "Read Only (ask)",
    );
    expect(
      screen.queryByTestId("chat-provider-settings-saving"),
    ).not.toBeInTheDocument();
  });

  it("hydrates Codex controls from the session providerState.codex.settings", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: codexSession({
            providerState: {
              codex: {
                settings: {
                  model: "gpt-5.5",
                  permissionMode: "on-request/read-only",
                  effort: "low",
                },
              },
            },
          }),
          sessions: [codexSession()],
        })}
      />,
    );

    expect(screen.getByTestId("codex-permission-control")).toHaveTextContent(
      /Read Only/,
    );
  });

  it("hydrates a newly bound Codex session from exact provider state before Ultra defaults", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: codexSession({
            model: "gpt-5.6-sol",
            providerState: {
              codex: {
                settings: {
                  model: "gpt-5.6-sol",
                  permissionMode: "never/danger-full-access",
                  effort: "xhigh",
                },
              },
            },
          }),
          sessions: [codexSession()],
          defaultAgentProviderChoices: {
            claude_code: {
              model: "claude-opus-4-8[1m]",
              mode: "bypassPermissions",
              effort: "high",
            },
            codex: {
              model: "gpt-5.6-sol",
              mode: "never/danger-full-access",
              effort: "ultra",
            },
          },
        })}
      />,
    );

    expect(screen.getByTestId("codex-model-control")).toHaveTextContent(
      "GPT 5.6 Sol",
    );
    expect(screen.getByTestId("codex-permission-control")).toHaveTextContent(
      "Full Access",
    );
    expect(screen.getByTestId("codex-effort-control")).toHaveTextContent(
      "XHigh",
    );
  });
});

describe("ChatPanel boot-phase activity labels (chat_panel_runtime D24, #762/#833/#1050)", () => {
  const entry = (
    entryType: string,
    content: string,
    metadata: Record<string, unknown> = {},
  ): TimelineEntry => ({
    id: `e-${entryType}-${content.length}`,
    entryType,
    content,
    metadata,
    createdAt: "2026-07-02T00:00:00Z",
  });

  it("projects task state before an empty or stale boot timeline", () => {
    expect(currentActivityLabel(undefined)).toBe("Creating task…");
    expect(currentActivityLabel([])).toBe("Creating task…");

    const queuedTask = { status: "queued" as const };
    expect(currentActivityLabel([], null, queuedTask)).toBe(
      "Queued for workspace…",
    );
    expect(
      currentActivityLabel([], "claude_code", {
        ...queuedTask,
        status: "running",
      }),
    ).toBe("Initializing Claude…");
    expect(
      currentActivityLabel([], null, {
        ...queuedTask,
        status: "waiting",
      }),
    ).toBe("Waiting for input…");
  });

  it("uses only current-turn phases and lets them supersede a stale task projection", () => {
    const priorTurnOutput = {
      ...entry("output", "previous answer"),
      // Lexically this sorts after `...10.500Z` because `Z > .`, even though
      // it is chronologically older. The turn fence must compare instants.
      createdAt: "2026-07-02T00:00:10Z",
    };
    const currentHostStart = {
      ...entry(
        "host_microvm_started",
        "Host microVM started for the current task",
      ),
      createdAt: "2026-07-02T00:00:10.750Z",
    };
    const activeTurn = {
      id: "assistant-current",
      createdAt: "2026-07-02T00:00:10.500Z",
    };
    const runningTask = { status: "running" as const };

    expect(
      currentActivityLabel(
        [priorTurnOutput],
        "claude_code",
        runningTask,
        activeTurn,
      ),
    ).toBe("Initializing Claude…");
    expect(
      currentActivityLabel(
        [priorTurnOutput, currentHostStart],
        "claude_code",
        runningTask,
        activeTurn,
      ),
    ).toBe("Initializing Claude…");

    const ownedEarlyPhase = {
      ...entry("host_microvm_started", "current turn, early clock", {
        messageId: activeTurn.id,
      }),
      createdAt: "2026-07-02T00:00:00Z",
    };
    const otherTurnPhase = {
      ...entry("tool_call", "{}", {
        messageId: "assistant-previous",
        toolName: "Bash",
      }),
      createdAt: "2026-07-02T00:00:11Z",
    };
    expect(
      currentActivityLabel(
        [ownedEarlyPhase, otherTurnPhase],
        "claude_code",
        runningTask,
        activeTurn,
      ),
    ).toBe("Initializing Claude…");
    expect(
      currentActivityLabel(
        [otherTurnPhase],
        "claude_code",
        runningTask,
        activeTurn,
      ),
    ).toBe("Initializing Claude…");

    const currentAttempt = {
      id: "attempt-current",
      attemptNumber: 2,
      provider: "claude_code",
      providerSessionId: null,
      disposition: null,
      reason: null,
      retryPolicy: null,
      lifecyclePosture: "active" as const,
      startedAt: "2026-07-02T00:00:10.600Z",
      endedAt: null,
    };
    const precedingAttemptExit = {
      ...entry("host_microvm_exited", "preceding attempt exited", {
        executionAttemptId: "attempt-previous",
      }),
      createdAt: "2026-07-02T00:00:10.800Z",
    };
    const tokenlessHostExit = {
      ...entry("host_microvm_exited", "tokenless preceding exit"),
      createdAt: "2026-07-02T00:00:10.850Z",
    };
    expect(
      currentActivityLabel(
        [precedingAttemptExit, tokenlessHostExit],
        "claude_code",
        { ...runningTask, status: "queued", latestAttempt: currentAttempt },
        activeTurn,
      ),
    ).toBe("Queued for workspace…");

    const legacyAgentToolCall = {
      ...entry("tool_call", "{}", { toolName: "Bash" }),
      createdAt: "2026-07-02T00:00:10.900Z",
    };
    expect(
      currentActivityLabel(
        [precedingAttemptExit, legacyAgentToolCall],
        "claude_code",
        { ...runningTask, latestAttempt: currentAttempt },
        activeTurn,
      ),
    ).toBe("Running Bash…");

    const currentAttemptHostStart = {
      ...entry("host_microvm_started", "current attempt started", {
        executionAttemptId: currentAttempt.id,
      }),
      createdAt: "2026-07-02T00:00:10.700Z",
    };
    expect(
      currentActivityLabel(
        [precedingAttemptExit, currentAttemptHostStart],
        "claude_code",
        { ...runningTask, status: "queued", latestAttempt: currentAttempt },
        activeTurn,
      ),
    ).toBe("Initializing Claude…");

    const nextAttemptHostStart = {
      ...entry("host_microvm_started", "next attempt started", {
        executionAttemptId: "attempt-next",
      }),
      createdAt: "2026-07-02T00:00:10.700Z",
    };
    expect(
      currentActivityLabel(
        [nextAttemptHostStart],
        "claude_code",
        { ...runningTask, status: "queued" },
        activeTurn,
      ),
    ).toBe("Initializing Claude…");
    expect(
      currentActivityLabel(
        [
          {
            ...nextAttemptHostStart,
            metadata: {
              ...nextAttemptHostStart.metadata,
              messageId: "assistant-previous",
            },
          },
        ],
        "claude_code",
        { ...runningTask, status: "queued" },
        activeTurn,
      ),
    ).toBe("Queued for workspace…");

    const endedAttempt = {
      ...currentAttempt,
      id: "attempt-previous",
      endedAt: "2026-07-02T00:00:10.650Z",
    };
    expect(
      currentActivityLabel(
        [nextAttemptHostStart],
        "claude_code",
        { ...runningTask, status: "queued", latestAttempt: endedAttempt },
        activeTurn,
      ),
    ).toBe("Initializing Claude…");
    expect(
      currentActivityLabel(
        [
          {
            ...nextAttemptHostStart,
            metadata: { executionAttemptId: endedAttempt.id },
          },
        ],
        "claude_code",
        { ...runningTask, status: "queued", latestAttempt: endedAttempt },
        activeTurn,
      ),
    ).toBe("Queued for workspace…");

    const newerToolCall = {
      ...entry("tool_call", "{}", { toolName: "Bash" }),
      createdAt: "2026-07-02T00:00:10.900Z",
    };
    expect(
      currentActivityLabel(
        [newerToolCall, currentHostStart],
        "claude_code",
        runningTask,
        activeTurn,
      ),
    ).toBe("Running Bash…");

    const laterWithinMillisecond = {
      ...entry("tool_call", "{}", { toolName: "Read" }),
      createdAt: "2026-07-02T00:00:10.123456Z",
    };
    const earlierWithinMillisecond = {
      ...entry("output", "earlier output"),
      createdAt: "2026-07-02T00:00:10.123400Z",
    };
    expect(
      currentActivityLabel(
        [laterWithinMillisecond, earlierWithinMillisecond],
        "claude_code",
        runningTask,
        { ...activeTurn, createdAt: "2026-07-02T00:00:10.123000Z" },
      ),
    ).toBe("Running Read…");
    expect(
      currentActivityLabel(
        [earlierWithinMillisecond],
        "claude_code",
        runningTask,
        { ...activeTurn, createdAt: "2026-07-02T00:00:10.123450Z" },
      ),
    ).toBe("Initializing Claude…");
  });

  it("labels provider-neutral guest bring-up using the displayed provider", () => {
    expect(
      currentActivityLabel(
        [entry("host_microvm_started", "Host microVM started for task t-1")],
        "codex",
      ),
    ).toBe("Initializing Codex…");
    expect(
      currentActivityLabel(
        [entry("host_microvm_started", "Host microVM started for task t-1")],
        "claude_code",
      ),
    ).toBe("Initializing Claude…");
    expect(
      currentActivityLabel([
        entry("host_microvm_started", "Host microVM started for task t-1"),
      ]),
    ).toBe("Initializing agent…");
  });

  it("labels provider-started checkpoints using the checkpoint provider", () => {
    expect(
      currentActivityLabel(
        [entry("checkpoint", "Codex started")],
        "claude_code",
      ),
    ).toBe("Initializing Codex…");
    expect(
      currentActivityLabel(
        [entry("checkpoint", "Claude Code started")],
        "codex",
      ),
    ).toBe("Initializing Claude…");
  });

  it("returns to Thinking… once the agent is live (later checkpoints)", () => {
    expect(
      currentActivityLabel([
        entry("checkpoint", "Claude Code capabilities updated"),
      ]),
    ).toBe("Thinking…");
    expect(
      currentActivityLabel([entry("checkpoint", "Claude Code usage updated")]),
    ).toBe("Thinking…");
    expect(
      currentActivityLabel(
        [entry("checkpoint", "Codex usage updated")],
        "codex",
      ),
    ).toBe("Thinking…");
  });

  it("keeps the existing tool/output activity labels unchanged", () => {
    expect(
      currentActivityLabel([entry("tool_call", "{}", { toolName: "Bash" })]),
    ).toMatch(/^Running .*…$/);
    expect(currentActivityLabel([entry("tool_result", "{}")])).toBe(
      "Awaiting model response…",
    );
    expect(currentActivityLabel([entry("output", "hi")])).toBe(
      "Generating response…",
    );
  });

  it("renders the queued-task label in the streaming indicator when no visible trace entries exist", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          messages: [
            chatMessage({ id: "m-user", role: "user", content: "hello" }),
            chatMessage({ id: "m-assist", status: "pending" }),
          ],
          isStreaming: true,
          timelineEntries: [],
          activeTaskSummary: {
            status: "queued",
            assignedAt: null,
          } as TaskSummary,
        })}
      />,
    );
    expect(screen.getByTestId("streaming-indicator")).toHaveTextContent(
      "Queued for workspace…",
    );
  });

  it("renders the Codex boot label in the streaming indicator for Codex sessions", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession({
            agentProvider: "codex" as ChatAgentProvider,
          }),
          messages: [
            chatMessage({ id: "m-user", role: "user", content: "hello" }),
            chatMessage({ id: "m-assist", status: "pending" }),
          ],
          isStreaming: true,
          timelineEntries: [
            entry("host_microvm_started", "Host microVM started for task t-1"),
          ],
        })}
      />,
    );
    expect(screen.getByTestId("streaming-indicator")).toHaveTextContent(
      "Initializing Codex…",
    );
  });

  it("places the live status before queued follow-up messages and marks them queued", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          messages: [
            chatMessage({
              id: "m-user-1",
              role: "user",
              content: "start the task",
              status: "complete",
              messageIndex: 0,
            }),
            chatMessage({
              id: "m-assist-1",
              role: "assistant",
              status: "streaming",
              messageIndex: 1,
            }),
            chatMessage({
              id: "m-user-2",
              role: "user",
              content: "queued follow-up one",
              status: "complete",
              messageIndex: 2,
            }),
            chatMessage({
              id: "m-assist-2",
              role: "assistant",
              status: "pending",
              messageIndex: 3,
            }),
            chatMessage({
              id: "m-user-3",
              role: "user",
              content: "queued follow-up two",
              status: "complete",
              messageIndex: 4,
            }),
            chatMessage({
              id: "m-assist-3",
              role: "assistant",
              status: "pending",
              messageIndex: 5,
            }),
          ],
          isStreaming: true,
          timelineEntries: [
            entry("host_microvm_started", "Host microVM started for task t-1"),
          ],
        })}
      />,
    );

    const indicator = screen.getByTestId("streaming-indicator");
    const firstQueuedMessage = screen.getByText("queued follow-up one");

    expect(indicator).toHaveTextContent("Initializing Claude…");
    expect(
      indicator.compareDocumentPosition(firstQueuedMessage) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(screen.getAllByTestId("queued-message-icon")).toHaveLength(2);
  });

  it("marks follow-ups queued from durable transcript order even when isStreaming is false", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          messages: [
            chatMessage({
              id: "m-user-1",
              role: "user",
              content: "start the task",
              status: "complete",
              messageIndex: 0,
            }),
            chatMessage({
              id: "m-assist-1",
              role: "assistant",
              status: "pending",
              messageIndex: 1,
            }),
            chatMessage({
              id: "m-user-2",
              role: "user",
              content: "queued follow-up one",
              status: "complete",
              messageIndex: 2,
            }),
            chatMessage({
              id: "m-assist-2",
              role: "assistant",
              status: "pending",
              messageIndex: 3,
            }),
            chatMessage({
              id: "m-user-3",
              role: "user",
              content: "queued follow-up two",
              status: "complete",
              messageIndex: 4,
            }),
          ],
          // Reproduces the send-time flip: `isStreaming` is momentarily false
          // while the durable transcript still holds pending assistant
          // placeholders behind the follow-ups.
          isStreaming: false,
          timelineEntries: [],
        })}
      />,
    );

    // Queued styling is derived from durable order, so both follow-ups stay
    // queued across the isStreaming=false frame (no flash back to the active
    // user-message format).
    expect(screen.getAllByTestId("queued-message-icon")).toHaveLength(2);
    // The live activity indicator stays gated on isStreaming, so it is absent.
    expect(screen.queryByTestId("streaming-indicator")).toBeNull();
  });
});

describe("ChatPanel AskUserQuestion picker (issue #1057)", () => {
  const questionEntry = (overrides: Record<string, unknown> = {}) =>
    timelineEntry({
      id: "approval-question",
      entryType: "checkpoint",
      content: "Claude Code requests permission for AskUserQuestion",
      metadata: {
        source: "provided_agent",
        provider: "claude_code",
        providerEvent: "provided_agent.approval.requested",
        phase: "approval",
        sessionId: "session-1",
        messageId: "assistant-question",
        toolCallId: "toolu-question",
        approvalId: "approval-question-1",
        toolName: "AskUserQuestion",
        input: {
          questions: [
            {
              question: "Which color?",
              header: "Color",
              multiSelect: false,
              options: [
                { label: "Red", description: "Choose red." },
                { label: "Blue", description: "Choose blue." },
              ],
            },
            {
              question: "Which features?",
              header: "Features",
              multiSelect: true,
              options: [
                { label: "Auth", description: "Authentication." },
                { label: "Search", description: "Search." },
                { label: "Export", description: "Export." },
              ],
            },
          ],
        },
        ...overrides,
      },
      sequence: 1,
    });

  const renderQuestionPanel = (
    onResolveAgentApproval = vi.fn(),
    entry = questionEntry(),
  ) => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          isStreaming: true,
          onResolveAgentApproval,
          messages: [
            chatMessage({
              id: "assistant-question",
              content: "",
              status: "streaming",
            }),
          ],
          timelineEntries: [entry],
        })}
      />,
    );
    return onResolveAgentApproval;
  };

  it.each([
    ["ordinary", "Which color?"],
    ["object-key", "__proto__"],
    ["spaces", " Which color? "],
  ])(
    "submits a native question with exact provider answers (%s)",
    async (caseName, questionText) => {
      const requestId = `native-request-${caseName}`;
      const onResolveAgentApproval = vi.fn().mockResolvedValue({
        deliveryDisposition: "delivered",
      });
      const entry = projectTaskTimelineEntry(
        timelineEntry({
          id: "native-question",
          entryType: "checkpoint",
          content: "provider_request.opened",
          metadata: {
            schema_version: "g5.task-event.v1",
            kind: "provider_request.opened",
            taskId: "task-native-1",
            attempt: 4,
            sessionId: "session-1",
            messageId: "assistant-question",
            payload: {
              provider_request_id: requestId,
              request_kind: "question",
              tool_name: "AskUserQuestion",
              display: {
                questions: [
                  {
                    question: questionText,
                    header: "Color",
                    multiSelect: false,
                    options: [{ label: "Red" }, { label: "Blue" }],
                  },
                ],
              },
              deadline_unix_ms: 1_800_000_000_000,
            },
          },
        }),
      );
      renderQuestionPanel(onResolveAgentApproval, entry);

      fireEvent.click(screen.getByRole("button", { name: "Blue" }));
      fireEvent.click(screen.getByTestId("agent-question-submit"));

      await waitFor(() =>
        expect(onResolveAgentApproval).toHaveBeenCalledWith({
          sessionId: "session-1",
          approvalId: requestId,
          decision: "approved",
          taskId: "task-native-1",
          attempt: 4,
          providerRequestId: requestId,
          requestKind: "question",
          updatedInput: {
            g5AnswersByIndex: {
              "0": { selectedOptionIndexes: [1] },
            },
          },
          validatedResponse: {
            g5AnswersByIndex: { "0": { selectedOptionIndexes: [1] } },
          },
        }),
      );
      expect(
        screen.queryByTestId("agent-question-dismiss"),
      ).not.toBeInTheDocument();
    },
  );

  it("keeps native question and option positions when redacted text collides", async () => {
    const onResolveAgentApproval = vi
      .fn()
      .mockResolvedValue({ deliveryDisposition: "delivered" });
    const entry = projectTaskTimelineEntry(
      timelineEntry({
        id: "native-colliding-question",
        entryType: "checkpoint",
        content: "provider_request.opened",
        metadata: {
          schema_version: "g5.task-event.v1",
          kind: "provider_request.opened",
          taskId: "task-native-1",
          attempt: 4,
          sessionId: "session-1",
          messageId: "assistant-question",
          payload: {
            provider_request_id: "native-colliding-request",
            request_kind: "question",
            tool_name: "AskUserQuestion",
            display: {
              questions: ["First", "Second"].map((header) => ({
                question: "Choose [REDACTED]",
                header,
                multiSelect: false,
                options: [
                  { label: "option [REDACTED]" },
                  { label: "option [REDACTED]" },
                ],
              })),
            },
            deadline_unix_ms: 1_800_000_000_000,
          },
        },
      }),
    );
    renderQuestionPanel(onResolveAgentApproval, entry);
    fireEvent.click(screen.getAllByTestId("agent-question-option")[1]);
    fireEvent.change(screen.getAllByTestId("agent-question-other")[1], {
      target: { value: "custom note" },
    });
    fireEvent.click(screen.getByTestId("agent-question-submit"));
    await waitFor(() =>
      expect(onResolveAgentApproval).toHaveBeenCalledWith(
        expect.objectContaining({
          validatedResponse: {
            g5AnswersByIndex: {
              "0": { selectedOptionIndexes: [1] },
              "1": { selectedOptionIndexes: [], otherText: "custom note" },
            },
          },
        }),
      ),
    );
  });

  it("focuses and consumes only the exact actionable question card once", async () => {
    const onFocusApprovalConsumed = vi.fn();
    const props = defaultProps({
      session: activeSession(),
      isStreaming: true,
      messages: [
        chatMessage({
          id: "assistant-question",
          content: "",
          status: "streaming",
        }),
      ],
      timelineEntries: [questionEntry()],
      focusApprovalId: "approval-question-1",
      onFocusApprovalConsumed,
    });
    const view = render(<ChatPanel {...props} />);

    const card = await waitFor(() => {
      const element = document.querySelector<HTMLElement>(
        '[data-ask-user-question-card="approval-question-1"]',
      );
      expect(element).not.toBeNull();
      expect(element).toHaveFocus();
      return element as HTMLElement;
    });
    expect(card).toHaveAttribute("tabindex", "-1");
    expect(card).toHaveClass(
      "focus-visible:ring-2",
      "focus-visible:ring-[var(--g5-info)]",
    );
    expect(onFocusApprovalConsumed).toHaveBeenCalledOnce();
    expect(onFocusApprovalConsumed).toHaveBeenCalledWith("approval-question-1");

    view.rerender(<ChatPanel {...props} />);
    await waitFor(() => expect(onFocusApprovalConsumed).toHaveBeenCalledOnce());
  });

  it("does not focus a different or settled question for a stale hint", async () => {
    const onFocusApprovalConsumed = vi.fn();
    const { rerender } = render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          isStreaming: true,
          messages: [
            chatMessage({
              id: "assistant-question",
              content: "",
              status: "streaming",
            }),
          ],
          timelineEntries: [questionEntry()],
          focusApprovalId: "approval-question-other",
          onFocusApprovalConsumed,
        })}
      />,
    );
    const card = document.querySelector<HTMLElement>(
      '[data-ask-user-question-card="approval-question-1"]',
    );
    expect(card).not.toBeNull();
    expect(card).not.toHaveFocus();
    expect(onFocusApprovalConsumed).not.toHaveBeenCalled();

    rerender(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          isStreaming: false,
          messages: [completedQuestionMessage()],
          timelineEntries: [
            questionEntry(),
            timelineEntry({
              id: "approval-question-resolved",
              metadata: {
                providerEvent: "provided_agent.approval.resolved",
                approvalId: "approval-question-1",
                sessionId: "session-1",
                resolutionSource: "user",
              },
              sequence: 2,
            }),
          ],
          focusApprovalId: "approval-question-1",
          onFocusApprovalConsumed,
        })}
      />,
    );
    await waitFor(() => expect(onFocusApprovalConsumed).not.toHaveBeenCalled());
  });

  // Issue #1217: the server's pending-question idle bound reclaims an
  // unanswered question's runtime and writes a synthetic settlement carrying
  // `resolutionSource: "idle_timeout"`. Nothing was answered and nothing was
  // applied, so it must never read as a resolution.
  const timedOutResolution = (overrides: Record<string, unknown> = {}) =>
    timelineEntry({
      id: "approval-question-timeout",
      entryType: "checkpoint",
      content: "Question cancelled after idle timeout: approval-question-1",
      metadata: {
        source: "provided_agent",
        provider: "claude_code",
        providerEvent: "provided_agent.approval.resolved",
        phase: "approval",
        sessionId: "session-1",
        messageId: "assistant-question",
        approvalId: "approval-question-1",
        status: "cancelled",
        decision: "cancelled",
        resolutionSource: "idle_timeout",
        ...overrides,
      },
      sequence: 2,
    });

  // Story 1 (routes/chat D25) durable marker of an admitted late answer:
  // `provided_agent.approval.late_answered` with `{approvalId,
  // lateAnswerMessageId}` metadata (chat_service `late_answer_marker_metadata`).
  // Deliberately carries no `messageId` — the marker anchors to the approval,
  // not to the assistant turn.
  const lateAnsweredMarker = (approvalId = "approval-question-1") =>
    timelineEntry({
      id: `late-answered-${approvalId}`,
      entryType: "checkpoint",
      content: `Late answer recorded for ${approvalId}`,
      metadata: {
        source: "provided_agent",
        provider: "claude_code",
        providerEvent: "provided_agent.approval.late_answered",
        phase: "approval",
        sessionId: "session-1",
        approvalId,
        lateAnswerMessageId: "late-answer-message-1",
      },
      sequence: 5,
      createdAt: "2026-05-10T00:00:06Z",
    });

  // A newer AskUserQuestion request created after the settlement — the
  // second half of the D25 supersession predicate.
  const newerQuestionRequest = () =>
    timelineEntry({
      id: "approval-question-2-request",
      entryType: "checkpoint",
      content: "Claude Code requests permission for AskUserQuestion",
      metadata: {
        source: "provided_agent",
        provider: "claude_code",
        providerEvent: "provided_agent.approval.requested",
        phase: "approval",
        sessionId: "session-1",
        messageId: "assistant-question-2",
        toolCallId: "toolu-question-2",
        approvalId: "approval-question-2",
        toolName: "AskUserQuestion",
        input: {
          questions: [
            {
              question: "Still there?",
              multiSelect: false,
              options: [{ label: "Yes", description: "Confirm." }],
            },
          ],
        },
      },
      sequence: 6,
      createdAt: "2026-05-10T00:00:10Z",
    });

  const completedQuestionMessage = () =>
    chatMessage({
      id: "assistant-question",
      content: "",
      status: "complete",
    });

  const renderTimedOutQuestion = () =>
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          isStreaming: false,
          messages: [completedQuestionMessage()],
          timelineEntries: [questionEntry(), timedOutResolution()],
        })}
      />,
    );

  // MODIFIED for chat_panel_runtime D25 (plan late_answerable_question v1):
  // this test previously pinned the settlement-only collapse to
  // "Timed out — ask again". A settlement with no late_answered marker and no
  // supersession evidence is now the D25 `timed_out_answerable` sub-state:
  // the form stays fully actionable, visually identical to pending plus
  // exactly one hint line, with Dismiss hidden. The never-Answered /
  // never-Dismissed half of the old invariant is retained verbatim.
  it("keeps a settlement-only timed-out question answerable, never Answered", () => {
    renderTimedOutQuestion();

    // Nobody answered: claiming Answered/Dismissed is still a false statement.
    expect(screen.queryByText("Answered")).not.toBeInTheDocument();
    expect(screen.queryByText("Dismissed")).not.toBeInTheDocument();
    // The card stays the shared, fully actionable question form…
    expect(screen.getByTestId("agent-question-form")).toBeInTheDocument();
    expect(screen.getByText("Which color?")).toBeInTheDocument();
    expect(screen.getByTestId("agent-question-submit")).toBeInTheDocument();
    // …plus exactly one hint line distinguishing it from a live pending form.
    expect(
      screen.getAllByText("Agent is paused — answering will restart it."),
    ).toHaveLength(1);
    // Dismiss is hidden — the server never late-admits a rejection.
    expect(
      screen.queryByTestId("agent-question-dismiss"),
    ).not.toBeInTheDocument();
    // The non-actionable collapse is reserved for the superseded sub-state.
    expect(screen.queryByText("Timed out — ask again")).not.toBeInTheDocument();
  });

  it("renders the question form instead of generic approval buttons", () => {
    renderQuestionPanel();

    expect(screen.getByTestId("agent-question-form")).toBeInTheDocument();
    expect(screen.getByText("Which color?")).toBeInTheDocument();
    expect(screen.getByText("Which features?")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Blue" })).toBeInTheDocument();
    expect(
      screen.queryByTestId("agent-approval-approve"),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("agent-approval-suggestion"),
    ).not.toBeInTheDocument();
    // Submit stays disabled until every question is answered.
    expect(screen.getByTestId("agent-question-submit")).toBeDisabled();
  });

  it("submits by-index answers for selections and free text", async () => {
    const onResolveAgentApproval = renderQuestionPanel();

    fireEvent.click(screen.getByRole("button", { name: "Blue" }));
    await waitFor(() =>
      expect(screen.getByTestId("agent-question-submit")).toBeDisabled(),
    );
    fireEvent.click(screen.getByRole("button", { name: "Export" }));
    fireEvent.click(screen.getByRole("button", { name: "Auth" }));
    const otherInputs = screen.getAllByTestId("agent-question-other");
    fireEvent.change(otherInputs[1], { target: { value: "  SSO  " } });

    const submit = screen.getByTestId("agent-question-submit");
    await waitFor(() => expect(submit).toBeEnabled());
    fireEvent.click(submit);

    await waitFor(() =>
      expect(onResolveAgentApproval).toHaveBeenCalledWith({
        sessionId: "session-1",
        approvalId: "approval-question-1",
        decision: "approved",
        updatedInput: {
          g5AnswersByIndex: {
            "0": { selectedOptionIndexes: [1] },
            "1": { selectedOptionIndexes: [0, 2], otherText: "SSO" },
          },
        },
      }),
    );
  });

  it("keeps single-select options exclusive with free text", async () => {
    const onResolveAgentApproval = renderQuestionPanel();

    fireEvent.click(screen.getByRole("button", { name: "Red" }));
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Red" })).toHaveAttribute(
        "aria-pressed",
        "true",
      ),
    );
    const otherInputs = screen.getAllByTestId("agent-question-other");
    // Typing free text on a single-select question clears the option pick.
    fireEvent.change(otherInputs[0], { target: { value: "a custom shade" } });
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Red" })).toHaveAttribute(
        "aria-pressed",
        "false",
      ),
    );
    fireEvent.click(screen.getByRole("button", { name: "Search" }));
    await waitFor(() =>
      expect(screen.getByTestId("agent-question-submit")).toBeEnabled(),
    );
    fireEvent.click(screen.getByTestId("agent-question-submit"));

    await waitFor(() =>
      expect(onResolveAgentApproval).toHaveBeenCalledWith(
        expect.objectContaining({
          updatedInput: {
            g5AnswersByIndex: {
              "0": { selectedOptionIndexes: [], otherText: "a custom shade" },
              "1": { selectedOptionIndexes: [1] },
            },
          },
        }),
      ),
    );
  });

  it("dismisses with the declined reason", async () => {
    const onResolveAgentApproval = renderQuestionPanel();

    // Interactive chat: plain decline, no fail-task consequence advertised.
    expect(screen.getByTestId("agent-question-dismiss")).toHaveTextContent(
      /^Dismiss$/,
    );
    fireEvent.click(screen.getByTestId("agent-question-dismiss"));

    await waitFor(() =>
      expect(onResolveAgentApproval).toHaveBeenCalledWith({
        sessionId: "session-1",
        approvalId: "approval-question-1",
        decision: "rejected",
        reason: "User declined to answer the questions.",
      }),
    );
  });

  it.each(["agent", "command", "preview"])(
    "allows dismissing a question for %s",
    async (taskType) => {
      const onResolveAgentApproval = vi.fn();
      render(
        <ChatPanel
          {...defaultProps({
            session: activeSession(),
            isStreaming: true,
            onResolveAgentApproval,
            // eslint-disable-next-line @typescript-eslint/no-explicit-any
            activeTaskSummary: { id: "task-sync-1", taskType } as any,
            messages: [
              chatMessage({
                id: "assistant-question",
                content: "",
                status: "streaming",
              }),
            ],
            timelineEntries: [questionEntry()],
          })}
        />,
      );

      expect(screen.getByTestId("agent-question-submit")).toBeInTheDocument();
      fireEvent.click(screen.getByTestId("agent-question-dismiss"));
      await waitFor(() =>
        expect(onResolveAgentApproval).toHaveBeenCalledWith(
          expect.objectContaining({
            decision: "rejected",
            reason: "User declined to answer the questions.",
          }),
        ),
      );
    },
  );

  it("keeps the draft and shows an error when the resolve fails", async () => {
    const onResolveAgentApproval = vi
      .fn()
      .mockRejectedValue(new Error("worker unreachable"));
    renderQuestionPanel(onResolveAgentApproval);

    fireEvent.click(screen.getByRole("button", { name: "Blue" }));
    fireEvent.click(screen.getByRole("button", { name: "Auth" }));
    fireEvent.click(screen.getByTestId("agent-question-submit"));

    await waitFor(() =>
      expect(screen.getByText("worker unreachable")).toBeInTheDocument(),
    );
    // Draft selections survive the failure for retry.
    expect(screen.getByRole("button", { name: "Blue" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(screen.getByTestId("agent-question-submit")).toBeEnabled();
  });

  it("keeps the draft while approval delivery is resolving", async () => {
    const onResolveAgentApproval = vi.fn().mockResolvedValue({
      approvalId: "approval-question-resolving",
      decision: "approved",
      delivered: false,
      status: "resolving",
    });
    renderQuestionPanel(
      onResolveAgentApproval,
      questionEntry({
        approvalId: "approval-question-resolving",
        toolCallId: "toolu-question-resolving",
      }),
    );

    fireEvent.click(screen.getByRole("button", { name: "Blue" }));
    fireEvent.click(screen.getByRole("button", { name: "Auth" }));
    fireEvent.click(screen.getByTestId("agent-question-submit"));

    await waitFor(() =>
      expect(
        screen.getByText(
          "Answer delivery is still resolving. Your draft is preserved.",
        ),
      ).toBeInTheDocument(),
    );
    expect(screen.getByRole("button", { name: "Blue" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(screen.getByTestId("agent-question-submit")).toBeEnabled();
  });

  it("renders no actionable form once the turn is interrupted", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          isStreaming: false,
          messages: [
            chatMessage({
              id: "assistant-question",
              content: "",
              status: "stopped",
            }),
          ],
          timelineEntries: [
            questionEntry(),
            timelineEntry({
              id: "interrupted-after-question",
              entryType: "checkpoint",
              content: "Stopped",
              metadata: {
                source: "provided_agent",
                provider: "claude_code",
                providerEvent: "provided_agent.interrupted",
                sessionId: "session-1",
                messageId: "assistant-question",
              },
              sequence: 2,
              createdAt: "2026-05-10T00:00:05Z",
            }),
          ],
        })}
      />,
    );

    expect(screen.queryByTestId("agent-question-form")).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("agent-question-submit"),
    ).not.toBeInTheDocument();
  });

  it.each([
    {
      provider: "claude_code",
      timeoutGuidance:
        "[Question timed out — the session was paused. You can still answer the question above, or send a new message.]",
      expectedGuidance:
        "[Question timed out — the session was paused. You can still answer the question above, or send a new message.]",
      context: "ask_user_question",
    },
    {
      provider: "claude_code",
      timeoutGuidance: "",
      expectedGuidance: null,
      context: "other",
    },
    {
      provider: "codex",
      timeoutGuidance:
        "[Question timed out — the session was paused. You can still answer the question above, or send a new message.]",
      expectedGuidance:
        "[Question timed out — the session was paused. You can still answer the question above, or send a new message.]",
      context: "ask_user_question",
    },
    {
      provider: "codex",
      timeoutGuidance: "",
      expectedGuidance: null,
      context: "other",
    },
  ])(
    "suppresses $provider user-interrupt text and renders only context-appropriate timeout guidance: $context",
    ({ provider, timeoutGuidance, expectedGuidance }) => {
      render(
        <ChatPanel
          {...defaultProps({
            session: activeSession(),
            isStreaming: false,
            messages: [
              chatMessage({
                id: "assistant-question",
                content: "",
                status: "stopped",
              }),
            ],
            timelineEntries: [
              questionEntry({ provider }),
              timedOutResolution({ provider }),
              timelineEntry({
                id: "provider-interrupt-artifact",
                entryType: "output",
                content: "[Request interrupted by user for tool use]",
                metadata: {
                  source: "provided_agent",
                  provider,
                  providerEvent: "provided_agent.output.final",
                  phase: "output",
                  sessionId: "session-1",
                  messageId: "assistant-question",
                  blockId: "provider-final",
                },
                sequence: 2,
                createdAt: "2026-05-10T00:00:03Z",
              }),
              timelineEntry({
                id: "idle-timeout-immediate",
                entryType: "checkpoint",
                content: timeoutGuidance,
                metadata: {
                  source: "provided_agent",
                  provider,
                  providerEvent: "provided_agent.interrupted",
                  phase: "status",
                  reason: "idle_timeout",
                  sessionId: "session-1",
                  messageId: "assistant-question",
                },
                sequence: 3,
                createdAt: "2026-05-10T00:00:04Z",
              }),
              timelineEntry({
                id: "idle-timeout-final",
                entryType: "checkpoint",
                content: timeoutGuidance,
                metadata: {
                  source: "provided_agent",
                  provider,
                  providerEvent: "provided_agent.interrupted",
                  phase: "status",
                  reason: "idle_timeout",
                  sessionId: "session-1",
                  messageId: "assistant-question",
                },
                sequence: 4,
                createdAt: "2026-05-10T00:00:05Z",
              }),
            ],
          })}
        />,
      );

      expect(
        screen.queryByText("[Request interrupted by user for tool use]"),
      ).not.toBeInTheDocument();
      if (expectedGuidance) {
        expect(screen.getAllByText(expectedGuidance)).toHaveLength(1);
      }
      expect(
        screen.queryByText("[Timeout, Send Any Followup Question to Resume]"),
      ).not.toBeInTheDocument();
      expect(screen.getAllByText("Stopped")).toHaveLength(1);
    },
  );

  it("suppresses a legacy parked-runtime timeout notice without replacing the completed answer", () => {
    const genericGuidance = "[Timeout, Send Any Followup Question to Resume]";

    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          isStreaming: false,
          messages: [
            chatMessage({
              id: "assistant-complete-before-runtime-expiry",
              content: "The completed answer remains visible.",
              status: "complete",
              messageIndex: 0,
              createdAt: "2026-05-10T00:00:03Z",
            }),
            chatMessage({
              id: "user-followup-after-runtime-expiry",
              role: "user",
              content: "A later follow-up remains visible.",
              status: "complete",
              messageIndex: 1,
              createdAt: "2026-05-10T00:02:04Z",
            }),
          ],
          timelineEntries: [
            timelineEntry({
              id: "parked-runtime-idle-timeout",
              entryType: "checkpoint",
              content: genericGuidance,
              metadata: {
                source: "server",
                provider: "claude_code",
                providerEvent: "provided_agent.runtime.idle_timeout",
                phase: "status",
                reason: "idle_timeout",
                timeoutContext: "generic",
                sessionId: "session-1",
                taskId: "task-1",
                runtimeId: "runtime-1",
              },
              sequence: 1,
              createdAt: "2026-05-10T00:02:03Z",
            }),
          ],
        })}
      />,
    );

    const completedAnswer = screen.getByText(
      "The completed answer remains visible.",
    );
    const laterFollowup = screen.getByText(
      "A later follow-up remains visible.",
    );
    expect(completedAnswer).toBeInTheDocument();
    expect(screen.queryByText(genericGuidance)).not.toBeInTheDocument();
    expect(laterFollowup).toBeInTheDocument();
  });

  it.each(["complete", "error", "stopped"] as const)(
    "renders no actionable form once the turn is terminal (%s) without an interrupted entry",
    (status) => {
      // Stale-question unblocking (chat_panel_runtime D25): a stream error,
      // completed, or stopped turn can end WITHOUT a provided_agent.interrupted
      // entry; the unresolved question must still collapse to dead history.
      render(
        <ChatPanel
          {...defaultProps({
            session: activeSession(),
            isStreaming: false,
            messages: [
              chatMessage({
                id: "assistant-question",
                content: "",
                status,
              }),
            ],
            timelineEntries: [questionEntry()],
          })}
        />,
      );

      expect(
        screen.queryByTestId("agent-question-form"),
      ).not.toBeInTheDocument();
      expect(
        screen.queryByTestId("agent-question-submit"),
      ).not.toBeInTheDocument();
      // A dead question must not fall back to actionable generic approval
      // controls either — that would leave the composer blocked on a turn
      // nobody can answer.
      expect(
        screen.queryByTestId("agent-approval-approve"),
      ).not.toBeInTheDocument();
      expect(
        screen.queryByTestId("agent-approval-suggestion"),
      ).not.toBeInTheDocument();
    },
  );

  it("falls back to the generic approval UI when questions are malformed", () => {
    renderQuestionPanel(vi.fn(), questionEntry({ input: { questions: [] } }));

    expect(screen.queryByTestId("agent-question-form")).not.toBeInTheDocument();
  });

  // ---------------------------------------------------------------------------
  // Late-answerable timeout sub-states (chat_panel_runtime D25, plan
  // late_answerable_question v1): an idle_timeout settlement splits into
  // timed_out_answerable / late_answered / timed_out_superseded from durable
  // timeline evidence only, so every fixture below also proves the
  // reload-safe (entries + messages only) derivation.
  // ---------------------------------------------------------------------------
  describe("late-answerable timeout sub-states (D25)", () => {
    const renderFixture = (options: {
      entries: TimelineEntry[];
      messages?: ChatMessageDTO[];
      onResolveAgentApproval?: ReturnType<typeof vi.fn>;
    }) =>
      render(
        <ChatPanel
          {...defaultProps({
            session: activeSession(),
            isStreaming: false,
            onResolveAgentApproval: options.onResolveAgentApproval,
            messages: options.messages ?? [completedQuestionMessage()],
            timelineEntries: options.entries,
          })}
        />,
      );

    it.each([
      {
        order: "settlement before marker",
        entries: () => [
          questionEntry(),
          timedOutResolution(),
          lateAnsweredMarker(),
        ],
      },
      {
        order: "marker loads first (reload-order permutation)",
        entries: () => [
          lateAnsweredMarker(),
          timedOutResolution(),
          questionEntry(),
        ],
      },
    ])(
      "collapses to Answered when the late_answered marker is present: $order",
      ({ entries }) => {
        renderFixture({ entries: entries() });

        expect(
          screen.getByTestId("chat-approval-resolved-toggle"),
        ).toHaveTextContent("Answered");
        expect(
          screen.queryByTestId("agent-question-form"),
        ).not.toBeInTheDocument();
        expect(
          screen.queryByText("Timed out — ask again"),
        ).not.toBeInTheDocument();
        expect(
          screen.queryByText("Agent is paused — answering will restart it."),
        ).not.toBeInTheDocument();
      },
    );

    it("collapses to the non-actionable summary when a later user message supersedes the settlement", () => {
      renderFixture({
        entries: [questionEntry(), timedOutResolution()],
        messages: [
          completedQuestionMessage(),
          chatMessage({
            id: "user-followup",
            role: "user",
            content: "never mind, do it the simple way",
            status: "complete",
            messageIndex: 1,
            createdAt: "2026-05-10T00:05:00Z",
          }),
        ],
      });

      expect(
        screen
          .getAllByTestId("chat-approval-resolved-toggle")
          .find((toggle) => toggle.textContent === "Timed out — ask again"),
      ).toBeDefined();
      expect(
        screen.queryByTestId("agent-question-form"),
      ).not.toBeInTheDocument();
      expect(screen.queryByText("Answered")).not.toBeInTheDocument();
    });

    it("collapses when a newer question request supersedes the settlement", () => {
      renderFixture({
        entries: [
          questionEntry(),
          timedOutResolution(),
          newerQuestionRequest(),
        ],
      });

      expect(
        screen
          .getAllByTestId("chat-approval-resolved-toggle")
          .find((toggle) => toggle.textContent === "Timed out — ask again"),
      ).toBeDefined();
      expect(
        screen.queryByTestId("agent-question-form"),
      ).not.toBeInTheDocument();
      expect(screen.queryByText("Answered")).not.toBeInTheDocument();
    });

    it("retains the draft across the pending → timed_out transition (QuestionFormStateCleanup must not purge)", async () => {
      const draftQuestion = questionEntry({
        approvalId: "approval-question-draft",
        toolCallId: "toolu-question-draft",
      });
      const { rerender } = render(
        <ChatPanel
          {...defaultProps({
            session: activeSession(),
            isStreaming: true,
            messages: [
              chatMessage({
                id: "assistant-question",
                content: "",
                status: "streaming",
              }),
            ],
            timelineEntries: [draftQuestion],
          })}
        />,
      );

      fireEvent.click(screen.getByRole("button", { name: "Blue" }));
      await waitFor(() =>
        expect(screen.getByRole("button", { name: "Blue" })).toHaveAttribute(
          "aria-pressed",
          "true",
        ),
      );

      rerender(
        <ChatPanel
          {...defaultProps({
            session: activeSession(),
            isStreaming: false,
            messages: [completedQuestionMessage()],
            timelineEntries: [
              draftQuestion,
              timedOutResolution({ approvalId: "approval-question-draft" }),
            ],
          })}
        />,
      );

      expect(screen.getByTestId("agent-question-form")).toBeInTheDocument();
      expect(
        screen.getByTestId("agent-question-timeout-hint"),
      ).toBeInTheDocument();
      // The pre-timeout selection survives the transition.
      expect(screen.getByRole("button", { name: "Blue" })).toHaveAttribute(
        "aria-pressed",
        "true",
      );
      expect(
        screen.queryByTestId("agent-question-dismiss"),
      ).not.toBeInTheDocument();
    });

    it("submits through the normal resolve mutation and collapses to Answered when the marker lands", async () => {
      const onResolveAgentApproval = vi.fn().mockResolvedValue({
        approvalId: "approval-question-late",
        decision: "approved",
        delivered: true,
        status: "resolved",
        lateAnswerMessageId: "late-answer-message-1",
      });
      const entries = [
        questionEntry({
          approvalId: "approval-question-late",
          toolCallId: "toolu-question-late",
        }),
        timedOutResolution({ approvalId: "approval-question-late" }),
      ];
      const { rerender } = renderFixture({ entries, onResolveAgentApproval });

      fireEvent.click(screen.getByRole("button", { name: "Blue" }));
      fireEvent.click(screen.getByRole("button", { name: "Auth" }));
      const submit = screen.getByTestId("agent-question-submit");
      await waitFor(() => expect(submit).toBeEnabled());
      fireEvent.click(submit);

      // Same body shape as an in-time answer: decision approved plus
      // updatedInput.g5AnswersByIndex — no late-answer-specific fields.
      await waitFor(() =>
        expect(onResolveAgentApproval).toHaveBeenCalledWith({
          sessionId: "session-1",
          approvalId: "approval-question-late",
          decision: "approved",
          updatedInput: {
            g5AnswersByIndex: {
              "0": { selectedOptionIndexes: [1] },
              "1": { selectedOptionIndexes: [0] },
            },
          },
        }),
      );

      // The refetched timeline delivers the durable marker; the card
      // collapses to the Answered summary.
      rerender(
        <ChatPanel
          {...defaultProps({
            session: activeSession(),
            isStreaming: false,
            messages: [completedQuestionMessage()],
            timelineEntries: [
              ...entries,
              lateAnsweredMarker("approval-question-late"),
            ],
          })}
        />,
      );
      expect(
        screen.getByTestId("chat-approval-resolved-toggle"),
      ).toHaveTextContent("Answered");
      expect(
        screen.queryByTestId("agent-question-form"),
      ).not.toBeInTheDocument();
    });

    it("shows a truthful inline notice on 409 LATE_ANSWER_SUPERSEDED and collapses once the refetched evidence lands", async () => {
      const onResolveAgentApproval = vi.fn().mockRejectedValue(
        // The server rejects late-answer conflicts as the structured CONFLICT
        // envelope with a code-prefixed reason (routes/chat D25); the notice
        // is gated on that typed shape, never on arbitrary error text.
        new ApiError({
          code: "CONFLICT",
          status: 409,
          message:
            "LATE_ANSWER_SUPERSEDED: a newer message or question superseded the late answer for approval 'approval-question-409'",
        }),
      );
      const entries = [
        questionEntry({
          approvalId: "approval-question-409",
          toolCallId: "toolu-question-409",
        }),
        timedOutResolution({ approvalId: "approval-question-409" }),
      ];
      const { rerender } = renderFixture({ entries, onResolveAgentApproval });

      fireEvent.click(screen.getByRole("button", { name: "Blue" }));
      fireEvent.click(screen.getByRole("button", { name: "Auth" }));
      const submit = screen.getByTestId("agent-question-submit");
      await waitFor(() => expect(submit).toBeEnabled());
      fireEvent.click(submit);

      await waitFor(() =>
        expect(
          screen.getByText(
            "A newer message or question superseded this answer — the question is closed.",
          ),
        ).toBeInTheDocument(),
      );

      // WorkspacePage refetches the timeline/messages on this 409; with the
      // superseding user message loaded, the card collapses to its true state.
      rerender(
        <ChatPanel
          {...defaultProps({
            session: activeSession(),
            isStreaming: false,
            messages: [
              completedQuestionMessage(),
              chatMessage({
                id: "user-superseding",
                role: "user",
                content: "moved on",
                status: "complete",
                messageIndex: 1,
                createdAt: "2026-05-10T00:05:00Z",
              }),
            ],
            timelineEntries: entries,
          })}
        />,
      );
      expect(
        screen
          .getAllByTestId("chat-approval-resolved-toggle")
          .find((toggle) => toggle.textContent === "Timed out — ask again"),
      ).toBeDefined();
      expect(
        screen.queryByTestId("agent-question-form"),
      ).not.toBeInTheDocument();
    });

    it("keeps the double-submit guard while a late answer is in flight", async () => {
      const onResolveAgentApproval = vi
        .fn()
        .mockReturnValue(new Promise(() => {}));
      renderFixture({
        entries: [
          questionEntry({
            approvalId: "approval-question-guard",
            toolCallId: "toolu-question-guard",
          }),
          timedOutResolution({ approvalId: "approval-question-guard" }),
        ],
        onResolveAgentApproval,
      });

      fireEvent.click(screen.getByRole("button", { name: "Blue" }));
      fireEvent.click(screen.getByRole("button", { name: "Auth" }));
      const submit = screen.getByTestId("agent-question-submit");
      await waitFor(() => expect(submit).toBeEnabled());
      fireEvent.click(submit);

      await waitFor(() => expect(submit).toBeDisabled());
      fireEvent.click(submit);
      expect(onResolveAgentApproval).toHaveBeenCalledTimes(1);
    });

    // Legacy metadata must produce the same question sub-states and cards
    // as provider-tagged events through the shared turn builder.
    it("keeps the trajectory-path question answerable on a settlement-only timeout (render-path parity)", () => {
      renderFixture({
        entries: [
          questionEntry({ provider: undefined }),
          timedOutResolution({ provider: undefined }),
        ],
      });

      expect(screen.getByTestId("agent-question-form")).toBeInTheDocument();
      expect(
        screen.getByTestId("agent-question-timeout-hint"),
      ).toBeInTheDocument();
      expect(
        screen.queryByTestId("agent-question-dismiss"),
      ).not.toBeInTheDocument();
      expect(
        screen.queryByText("Timed out — ask again"),
      ).not.toBeInTheDocument();
    });

    it.each([
      {
        state: "late_answered collapses to Answered",
        extraEntries: () => [lateAnsweredMarker()],
        extraMessages: () => [],
        expected: "Answered",
      },
      {
        state: "superseded collapses to the timed-out summary",
        extraEntries: () => [],
        extraMessages: () => [
          chatMessage({
            id: "user-followup",
            role: "user",
            content: "moved on",
            status: "complete",
            messageIndex: 1,
            createdAt: "2026-05-10T00:05:00Z",
          }),
        ],
        expected: "Timed out — ask again",
      },
    ])(
      "trajectory-path parity: $state",
      ({ extraEntries, extraMessages, expected }) => {
        renderFixture({
          entries: [
            questionEntry({ provider: undefined }),
            timedOutResolution({ provider: undefined }),
            ...extraEntries(),
          ],
          messages: [completedQuestionMessage(), ...extraMessages()],
        });

        expect(screen.getAllByText(expected).length).toBeGreaterThan(0);
        expect(
          screen.queryByTestId("agent-question-form"),
        ).not.toBeInTheDocument();
      },
    );

    // IDENTITY GATE (chat_panel_runtime D25, 2026-09 merge-review P1s): question
    // detection is keyed on the worker-stamped `toolName: "AskUserQuestion"`
    // tag — the authoritative tool identity — never on input shape alone.
    // (a) Client/server parity: the server's late-answer admission gate,
    // supersession query, window arming, Home discovery, and focus verification
    // are all tag-gated, so the client must not mark answerable a tag-less
    // entry whose late answer the server would reject with the blanket 409.
    it("collapses a tag-less timed-out entry instead of claiming an answerability the server's tag-gated late-answer admission rejects (message-block path)", () => {
      renderFixture({
        entries: [questionEntry({ toolName: undefined }), timedOutResolution()],
      });

      expect(
        screen.queryByTestId("agent-question-form"),
      ).not.toBeInTheDocument();
      expect(
        screen.queryByTestId("agent-question-timeout-hint"),
      ).not.toBeInTheDocument();
      expect(
        screen
          .getAllByTestId("chat-approval-resolved-toggle")
          .find((toggle) => toggle.textContent === "Timed out — ask again"),
      ).toBeDefined();
      expect(screen.queryByText("Answered")).not.toBeInTheDocument();
    });

    it("collapses a tag-less timed-out entry on the trajectory path too (render-path parity)", () => {
      renderFixture({
        entries: [
          questionEntry({ provider: undefined, toolName: undefined }),
          timedOutResolution({ provider: undefined }),
        ],
      });

      expect(
        screen.queryByTestId("agent-question-form"),
      ).not.toBeInTheDocument();
      expect(
        screen.queryByTestId("agent-question-timeout-hint"),
      ).not.toBeInTheDocument();
      // The legacy entry uses the same settled card as the tagged input.
      expect(
        screen.getByTestId("chat-approval-resolved-toggle"),
      ).toHaveTextContent("Timed out — ask again");
      expect(screen.queryByText("Answered")).not.toBeInTheDocument();
    });

    // (b) Authorization clarity: a NON-question tool with a legitimate
    // `questions`-shaped argument must render as a permission approval, never
    // as a question card — a question card's submit resolves
    // `decision: "approved"` + `updatedInput`, so shape-based detection would
    // disguise a tool authorization as a survey.
    it("renders a non-AskUserQuestion tool with a questions-shaped input as a permission approval, not a question card (message-block path)", () => {
      renderFixture({
        entries: [questionEntry({ toolName: "FancyDataTool" })],
        messages: [
          chatMessage({
            id: "assistant-question",
            content: "",
            status: "streaming",
          }),
        ],
      });

      expect(
        screen.queryByTestId("agent-question-form"),
      ).not.toBeInTheDocument();
      expect(screen.getByText(/Approval requested:/)).toBeInTheDocument();
    });

    it("renders a non-AskUserQuestion tool with a questions-shaped input as a permission approval on the trajectory path too", () => {
      renderFixture({
        entries: [
          questionEntry({ provider: undefined, toolName: "FancyDataTool" }),
        ],
        messages: [
          chatMessage({
            id: "assistant-question",
            content: "",
            status: "streaming",
          }),
        ],
      });

      expect(
        screen.queryByTestId("agent-question-form"),
      ).not.toBeInTheDocument();
      expect(screen.getByText(/Approval requested:/)).toBeInTheDocument();
      expect(screen.getByTestId("agent-approval-approve")).toBeInTheDocument();
    });
  });
});

describe("ChatPanel compact launch settings (right-panel D23/D24/D27)", () => {
  const twoProviders: ChatAgentProvider[] = ["claude_code", "codex"];

  it("shows one active-session editor with Agent disabled and Mode read-only", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          sessions: [activeSession()],
          availableAgentProviders: twoProviders,
        })}
      />,
    );

    fireEvent.click(screen.getByTestId("claude-settings-control"));
    expect(screen.getByTestId("claude-settings-modal")).toBeInTheDocument();
    expect(screen.getByTestId("chat-launch-controls")).toBeInTheDocument();
    expect(screen.getByTestId("chat-agent-select")).toBeDisabled();
    expect(screen.getByTestId("chat-model-select")).not.toBeDisabled();
    expect(screen.getByTestId("chat-effort-select")).not.toBeDisabled();
    expect(screen.getByTestId("chat-mode-select")).toHaveAttribute("readonly");
    expect(screen.getByTestId("chat-mode-select")).toHaveDisplayValue(
      "Not set",
    );
    expect(
      screen.getByText(/Agent and Mode are fixed for this chat/i),
    ).toBeInTheDocument();
  });

  it("offers agent options on an empty new chat and drives onSelectAgentProvider", () => {
    const onSelectAgentProvider = vi.fn();
    render(
      <ChatPanel
        {...defaultProps({
          session: null,
          availableAgentProviders: twoProviders,
          onSelectAgentProvider,
        })}
      />,
    );

    fireEvent.click(screen.getByTestId("claude-settings-control"));
    chooseThemedOption("chat-agent-select", "Codex");
    expect(onSelectAgentProvider).toHaveBeenCalledWith("codex");
  });

  it("shows all four values in the composer pill for both harnesses", () => {
    const { unmount } = render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          sessions: [activeSession()],
        })}
      />,
    );
    expect(screen.getByTestId("claude-harness-control")).toHaveTextContent(
      "Claude Code",
    );
    expect(screen.getByTestId("claude-model-control")).toBeInTheDocument();
    expect(screen.getByTestId("claude-permission-control")).toBeInTheDocument();
    expect(screen.getByTestId("claude-effort-control")).toBeInTheDocument();
    unmount();

    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession({ id: "codex-pill", agentProvider: "codex" }),
          sessions: [
            activeSession({ id: "codex-pill", agentProvider: "codex" }),
          ],
        })}
      />,
    );
    expect(screen.getByTestId("codex-harness-control")).toHaveTextContent(
      "Codex",
    );
    expect(screen.getByTestId("codex-model-control")).toBeInTheDocument();
    expect(screen.getByTestId("codex-permission-control")).toBeInTheDocument();
    expect(screen.getByTestId("codex-effort-control")).toBeInTheDocument();
  });

  it("populates the Claude effort group per model — Sonnet caps at high", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: null,
          defaultClaudeModel: "claude-sonnet-4-6",
        })}
      />,
    );

    fireEvent.click(screen.getByTestId("claude-settings-control"));
    const modal = screen.getByTestId("claude-settings-modal");
    expect(within(modal).getByText("Effort")).toBeInTheDocument();
    fireEvent.click(screen.getByTestId("chat-effort-select"));
    expect(
      screen.queryByRole("option", { name: "Max" }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("option", { name: "XHigh" }),
    ).not.toBeInTheDocument();
    expect(screen.getByRole("option", { name: "High" })).toBeInTheDocument();
  });

  // Regression: an unknown/custom pin whose id merely CONTAINS "sonnet" must
  // offer the full ladder (D23) and send the clicked value unchanged. The legacy
  // resolvedClaudeEffortForModel substring heuristic downgraded xhigh/max to
  // "high" here, contradicting clampEffortForModel's documented
  // unknown-models-pass-through contract used by the model-switch path.
  it("offers and sends xhigh verbatim for an unknown pin whose id contains 'sonnet'", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: null,
          defaultAgentProviderChoices: {
            claude_code: {
              mode: "bypassPermissions",
              model: "acme/custom-sonnet-v2",
              effort: "medium",
            },
            codex: {
              mode: "never/danger-full-access",
              model: "gpt-5.6-sol",
              effort: "ultra",
            },
          },
        })}
      />,
    );

    fireEvent.click(screen.getByTestId("claude-settings-control"));
    const modal = screen.getByTestId("claude-settings-modal");
    expect(screen.getByTestId("chat-model-select")).toHaveTextContent(
      "acme/custom-sonnet-v2",
    );
    // unknown pin -> full ladder offered, and no "→ High" downgrade affordance
    fireEvent.click(screen.getByTestId("chat-effort-select"));
    const xhigh = screen.getByRole("option", { name: "XHigh" });
    expect(xhigh).toBeInTheDocument();
    expect(within(modal).queryByText(/→ High/)).not.toBeInTheDocument();

    fireEvent.click(xhigh);
    expect(screen.getByTestId("chat-effort-select")).toHaveTextContent("XHigh");
  });

  it("offers the full Codex model catalog without a deployment prop", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: null,
          selectedAgentProvider: "codex",
          availableAgentProviders: twoProviders,
        })}
      />,
    );

    fireEvent.click(screen.getByTestId("codex-settings-control"));
    fireEvent.click(screen.getByTestId("chat-model-select"));
    expect(
      screen.getByTestId("chat-model-select-option-gpt-5.6-sol"),
    ).toBeInTheDocument();
    expect(
      screen.queryByTestId("chat-model-select-option-gpt-5.5"),
    ).toBeInTheDocument();
  });

  it("offers the full catalog lineup when the runtime gate is on (with the 0.145.0 bump)", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: null,
          selectedAgentProvider: "codex",
          availableAgentProviders: twoProviders,
        })}
      />,
    );

    fireEvent.click(screen.getByTestId("codex-settings-control"));
    fireEvent.click(screen.getByTestId("chat-model-select"));
    expect(
      screen.getByTestId("chat-model-select-option-gpt-5.6-sol"),
    ).toBeInTheDocument();
  });
});

describe("ChatPanel effort clamp + verbatim send (plan D-6/D24, Task 2.3)", () => {
  it("clamps Claude effort with the model in the same composite request", async () => {
    const onSendProviderControl = vi.fn().mockResolvedValue(undefined);
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession({
            providerState: {
              claudeCode: {
                settings: {
                  permissionMode: "bypassPermissions",
                  model: "claude-fable-5",
                  effort: "max",
                },
              },
            },
          }),
          isStreaming: false,
          onSendProviderControl,
        })}
      />,
    );

    fireEvent.click(screen.getByTestId("claude-settings-control"));
    chooseThemedOption("chat-model-select", "Sonnet 4.6");
    expect(screen.getByTestId("chat-effort-select")).toHaveTextContent("High");
    await waitFor(() => expect(onSendProviderControl).toHaveBeenCalledTimes(1));
    expect(onSendProviderControl).toHaveBeenCalledWith({
      sessionId: "session-1",
      action: "set_settings",
      payload: {
        model: "claude-sonnet-4-6",
        effort: "high",
      },
    });
  });

  it("keeps an auto-persisted Claude effort change after Escape", async () => {
    const onSendProviderControl = vi.fn().mockResolvedValue(undefined);
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession({
            providerState: {
              claudeCode: {
                settings: {
                  permissionMode: "bypassPermissions",
                  model: "claude-fable-5",
                  effort: "max",
                },
              },
            },
          }),
          isStreaming: false,
          onSendProviderControl,
        })}
      />,
    );

    fireEvent.click(screen.getByTestId("claude-settings-control"));
    await screen.findByTestId("claude-settings-modal");
    chooseThemedOption("chat-effort-select", "High");
    await waitFor(() => expect(onSendProviderControl).toHaveBeenCalledTimes(1));
    await waitFor(() =>
      expect(screen.getByTestId("chat-effort-select")).not.toBeDisabled(),
    );
    fireEvent.keyDown(document, { key: "Escape" });
    fireEvent.click(screen.getByTestId("claude-settings-control"));
    expect(screen.getByTestId("chat-effort-select")).toHaveTextContent("High");
    expect(onSendProviderControl).toHaveBeenCalledTimes(1);
  });

  it("keeps Codex Full Access read-only while auto-persisting model and effort", async () => {
    const onSendProviderControl = vi.fn().mockResolvedValue(undefined);
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession({
            id: "cx-clamp",
            agentProvider: "codex",
            providerState: {
              codex: {
                settings: {
                  permissionMode: "never/danger-full-access",
                  model: "gpt-5.6-sol",
                  effort: "ultra",
                },
              },
            },
          }),
          isStreaming: false,
          onSendProviderControl,
        })}
      />,
    );

    fireEvent.click(screen.getByTestId("codex-settings-control"));
    chooseThemedOption("chat-model-select", "GPT 5.5");
    await waitFor(() => expect(onSendProviderControl).toHaveBeenCalledTimes(1));
    await waitFor(() =>
      expect(screen.getByTestId("chat-effort-select")).not.toBeDisabled(),
    );
    expect(screen.getByTestId("chat-effort-select")).toHaveTextContent("XHigh");
    expect(screen.getByTestId("chat-mode-select")).toHaveAttribute("readonly");
    expect(screen.getByTestId("chat-mode-select")).toHaveDisplayValue(
      "Full Access",
    );
    await waitFor(() => expect(onSendProviderControl).toHaveBeenCalledTimes(1));
    expect(onSendProviderControl).toHaveBeenLastCalledWith({
      sessionId: "cx-clamp",
      action: "set_settings",
      payload: {
        model: "gpt-5.5",
        effort: "xhigh",
      },
    });
  });

  it("sends the concrete model plus native mode/effort verbatim for Codex", () => {
    const onSendMessage = vi.fn().mockResolvedValue(true);
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession({
            id: "cx-send",
            agentProvider: "codex",
            providerState: {
              codex: {
                settings: {
                  permissionMode: "never/danger-full-access",
                  model: "gpt-5.5",
                  effort: "xhigh",
                },
              },
            },
          }),
          isStreaming: false,
          onSendMessage,
        })}
      />,
    );

    fireEvent.change(screen.getByTestId("chat-input"), {
      target: { value: "hello codex" },
    });
    fireEvent.keyDown(screen.getByTestId("chat-input"), { key: "Enter" });

    expect(onSendMessage).toHaveBeenCalledWith(
      "hello codex",
      expect.objectContaining({
        model: "gpt-5.5",
        permissionMode: "never/danger-full-access",
        effort: "xhigh",
      }),
    );
  });

  it.each(["claude-haiku-4-5", "anthropic/claude-haiku-4.5"])(
    "keeps a legacy %s session usable without presenting or resending its obsolete effort",
    async (model) => {
      const onSendMessage = vi.fn().mockResolvedValue(true);
      const legacy = activeSession({
        providerState: {
          claudeCode: {
            settings: {
              permissionMode: "bypassPermissions",
              model,
              effort: "high",
            },
          },
        },
      });
      render(
        <ChatPanel
          {...defaultProps({
            session: legacy,
            isStreaming: false,
            onSendMessage,
          })}
        />,
      );
      expect(screen.getByTestId("claude-effort-control")).toHaveTextContent(
        "Not supported",
      );
      fireEvent.click(screen.getByTestId("claude-settings-control"));
      expect(screen.getByTestId("chat-effort-select")).toBeDisabled();
      expect(screen.getByTestId("chat-effort-select")).toHaveTextContent(
        "Not supported",
      );
      fireEvent.change(screen.getByTestId("chat-input"), {
        target: { value: "continue" },
      });
      fireEvent.keyDown(screen.getByTestId("chat-input"), { key: "Enter" });
      await waitFor(() =>
        expect(onSendMessage).toHaveBeenCalledWith(
          "continue",
          expect.objectContaining({ model, effort: "none" }),
        ),
      );
      expect(
        (
          legacy.providerState as {
            claudeCode: { settings: { effort: string } };
          }
        ).claudeCode.settings.effort,
      ).toBe("high");
    },
  );

  it("sends the concrete model plus native mode/effort verbatim for Claude", () => {
    const onSendMessage = vi.fn().mockResolvedValue(true);
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession({
            providerState: {
              claudeCode: {
                settings: {
                  permissionMode: "plan",
                  model: "claude-sonnet-4-6",
                  effort: "high",
                },
              },
            },
          }),
          isStreaming: false,
          onSendMessage,
        })}
      />,
    );

    fireEvent.change(screen.getByTestId("chat-input"), {
      target: { value: "hello claude" },
    });
    fireEvent.keyDown(screen.getByTestId("chat-input"), { key: "Enter" });

    expect(onSendMessage).toHaveBeenCalledWith(
      "hello claude",
      expect.objectContaining({
        model: "claude-sonnet-4-6",
        permissionMode: "plan",
        effort: "high",
      }),
    );
  });
});

describe("ChatPanel new-chat pre-fill from defaultAgentProviderChoices (plan Story 3, Task 3.3)", () => {
  const choices = {
    // Deliberately NOT the catalog default (Opus 5.5), and distinct from the
    // Sonnet the precedence test hydrates — otherwise a model assertion here
    // would pass even if the entry's model were ignored.
    claude_code: {
      model: "claude-haiku-4-5",
      mode: "plan",
      effort: "high",
    },
    codex: {
      model: "gpt-5.5",
      mode: "never/danger-full-access",
      effort: "medium",
    },
  } as const;

  it("pre-fills Claude model/mode/effort from the claude_code entry on an empty chat", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: null,
          defaultAgentProviderChoices: choices,
        })}
      />,
    );

    expect(screen.getByTestId("claude-model-control")).toHaveTextContent(
      "Haiku 4.5",
    );
    expect(screen.getByTestId("claude-permission-control")).toHaveTextContent(
      "Bypass",
    );
    expect(screen.getByTestId("claude-effort-control")).toHaveTextContent(
      "Not supported",
    );
  });

  it.each([
    { model: "gpt-5.5", expectedModelLabel: "GPT 5.5" },
    { model: "gpt-5.6-sol", expectedModelLabel: "GPT 5.6 Sol" },
  ])(
    "pre-fills Codex model/mode/effort from the codex entry on an empty chat ($model)",
    ({ model, expectedModelLabel }) => {
      render(
        <ChatPanel
          {...defaultProps({
            session: null,
            selectedAgentProvider: "codex",
            availableAgentProviders: ["claude_code", "codex"],
            defaultAgentProviderChoices: {
              ...choices,
              codex: { ...choices.codex, model },
            },
          })}
        />,
      );

      expect(screen.getByTestId("codex-model-control")).toHaveTextContent(
        expectedModelLabel,
      );
      expect(screen.getByTestId("codex-permission-control")).toHaveTextContent(
        "Full Access",
      );
      expect(screen.getByTestId("codex-effort-control")).toHaveTextContent(
        "Medium",
      );
      fireEvent.click(screen.getByTestId("codex-settings-control"));
      expect(screen.getByTestId("chat-model-select")).toHaveTextContent(
        expectedModelLabel,
      );
    },
  );

  it("clamps a stale saved effort and submits the exact displayed forced values", async () => {
    const resolveAgentLaunch = vi.fn().mockResolvedValue({
      ...EXPLICIT_CODEX_CHAT_RESOLUTION,
      resolutionId: "chat-resolution-clamped-codex",
      effort: { value: "xhigh", provenance: "explicit" as const },
    });
    const onSendMessage = vi.fn().mockResolvedValue(true);

    render(
      <ChatPanel
        {...defaultProps({
          session: null,
          selectedAgentProvider: "codex",
          availableAgentProviders: ["claude_code", "codex"],
          defaultAgentProviderChoices: {
            ...choices,
            codex: {
              model: "gpt-5.5",
              mode: "on-request/read-only",
              effort: "ultra",
            },
          },
          resolveAgentLaunch,
          onSendMessage,
        })}
      />,
    );

    expect(screen.getByTestId("codex-permission-control")).toHaveTextContent(
      "Full Access",
    );
    expect(screen.getByTestId("codex-effort-control")).toHaveTextContent(
      "XHigh",
    );

    fireEvent.click(screen.getByTestId("codex-settings-control"));
    await waitFor(() => expect(resolveAgentLaunch).toHaveBeenCalledTimes(1));
    expect(screen.getByTestId("chat-mode-select")).toHaveAttribute("readonly");
    expect(screen.getByTestId("chat-mode-select")).toBeEnabled();
    expect(screen.getByTestId("chat-effort-select")).toHaveTextContent("XHigh");
    expect(resolveAgentLaunch).toHaveBeenCalledWith(
      expect.objectContaining({
        provider: { source: "explicit", value: "codex" },
        model: { source: "explicit", value: "gpt-5.5" },
        permissionMode: {
          source: "explicit",
          value: "never/danger-full-access",
        },
        effort: { source: "explicit", value: "xhigh" },
      }),
    );

    await waitFor(() =>
      expect(
        screen.queryByTestId("chat-resolution-loading"),
      ).not.toBeInTheDocument(),
    );
    fireEvent.pointerDown(document.body);
    fireEvent.change(screen.getByTestId("chat-input"), {
      target: { value: "launch exactly this" },
    });
    fireEvent.keyDown(screen.getByTestId("chat-input"), { key: "Enter" });

    await waitFor(() => expect(onSendMessage).toHaveBeenCalledTimes(1));
    expect(onSendMessage).toHaveBeenCalledWith(
      "launch exactly this",
      expect.objectContaining({
        model: "gpt-5.5",
        permissionMode: "never/danger-full-access",
        effort: "xhigh",
        agentLaunch: expect.objectContaining({
          confirmedResolutionId: "chat-resolution-clamped-codex",
          preferences: expect.objectContaining({
            model: { source: "explicit", value: "gpt-5.5" },
            permissionMode: {
              source: "explicit",
              value: "never/danger-full-access",
            },
            effort: { source: "explicit", value: "xhigh" },
          }),
        }),
      }),
    );
  });

  it("falls back to the catalog defaults when the choices map is absent", () => {
    render(<ChatPanel {...defaultProps({ session: null })} />);

    expect(screen.getByTestId("claude-permission-control")).toHaveTextContent(
      /Bypass/i,
    );
    // #1370 alignment: the last-resort tail resolves per model — the default
    // Opus family's defaultEffort ("high" since chat intent 36046a83), not
    // the legacy flat XHigh.
    expect(screen.getByTestId("claude-effort-control")).toHaveTextContent(
      "High",
    );
  });

  it("lets active-session hydration win over the pre-fill (D30 precedence)", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession({
            providerState: {
              claudeCode: {
                settings: {
                  permissionMode: "acceptEdits",
                  model: "claude-sonnet-4-6",
                  effort: "low",
                },
              },
            },
          }),
          defaultAgentProviderChoices: choices,
        })}
      />,
    );

    expect(screen.getByTestId("claude-permission-control")).toHaveTextContent(
      "Accept edits",
    );
    expect(screen.getByTestId("claude-effort-control")).toHaveTextContent(
      "Low",
    );
    expect(screen.getByTestId("claude-model-control")).toHaveTextContent(
      "Sonnet 4.6",
    );
  });
});

describe("Claude mode options after the auto removal (#1370, types/core D15)", () => {
  it.each([
    ["claude_code", "claudeCode", "auto", "bypassPermissions", "Bypass"],
    ["claude_code", "claudeCode", "default", "default", "Ask"],
    ["claude_code", "claudeCode", "future-mode", "future-mode", "future-mode"],
    [
      "codex",
      "codex",
      "never/danger-full-access",
      "never/danger-full-access",
      "Full Access",
    ],
    [
      "codex",
      "codex",
      "on-request/read-only",
      "on-request/read-only",
      "Read Only (ask)",
    ],
    ["codex", "codex", "never/read-only", "never/read-only", "Read Only"],
    [
      "codex",
      "codex",
      "never/workspace-write",
      "never/workspace-write",
      "Workspace Write",
    ],
    [
      "codex",
      "codex",
      "future-policy/custom-sandbox",
      "future-policy/custom-sandbox",
      "future-policy/custom-sandbox",
    ],
  ] as const)(
    "displays and sends the session's %s mode %s/%s",
    (provider, namespace, saved, sent, label) => {
      const onSendMessage = vi.fn().mockResolvedValue(true);
      render(
        <ChatPanel
          {...defaultProps({
            session: activeSession({
              agentProvider: provider,
              providerState: {
                [namespace]: { settings: { permissionMode: saved } },
              },
            }),
            defaultAgentProviderChoices: {
              claude_code: {
                model: "claude-sonnet-4-6",
                mode: "plan",
                effort: "high",
              },
              codex: {
                model: "gpt-5.5",
                mode: "on-failure/workspace-write",
                effort: "high",
              },
            },
            onSendMessage,
          })}
        />,
      );
      expect(
        screen.getByTestId(
          provider === "codex"
            ? "codex-permission-control"
            : "claude-permission-control",
        ),
      ).toHaveTextContent(label);
      fireEvent.change(screen.getByTestId("chat-input"), {
        target: { value: "Continue" },
      });
      fireEvent.keyDown(screen.getByTestId("chat-input"), { key: "Enter" });
      expect(onSendMessage).toHaveBeenCalledWith(
        "Continue",
        expect.objectContaining({ permissionMode: sent }),
      );
    },
  );

  it.each(["claude_code", "codex"] as const)(
    "does not invent a fixed %s mode while session settings are missing",
    (provider) => {
      const onSendMessage = vi.fn().mockResolvedValue(true);
      const props = defaultProps({
        session: activeSession({ agentProvider: provider, providerState: {} }),
        defaultAgentProviderChoices: {
          claude_code: {
            model: "claude-sonnet-4-6",
            mode: "default",
            effort: "high",
          },
          codex: {
            model: "gpt-5.5",
            mode: "on-failure/workspace-write",
            effort: "high",
          },
        },
        onSendMessage,
      });
      const { rerender } = render(<ChatPanel {...props} />);
      const control =
        provider === "codex"
          ? "codex-permission-control"
          : "claude-permission-control";
      expect(screen.getByTestId(control)).toHaveTextContent("Not set");
      fireEvent.change(screen.getByTestId("chat-input"), {
        target: { value: "Continue" },
      });
      fireEvent.keyDown(screen.getByTestId("chat-input"), { key: "Enter" });
      expect(onSendMessage).toHaveBeenCalledWith(
        "Continue",
        expect.objectContaining({ permissionMode: undefined }),
      );
      // The detail response supplies the recovered first-attempt mode.
      rerender(
        <ChatPanel
          {...props}
          session={activeSession({
            agentProvider: provider,
            providerState: {
              [provider === "codex" ? "codex" : "claudeCode"]: {
                settings: {
                  permissionMode:
                    provider === "codex"
                      ? "never/danger-full-access"
                      : "bypassPermissions",
                },
              },
            },
          })}
        />,
      );
      expect(screen.getByTestId(control)).toHaveTextContent(
        provider === "codex" ? "Full Access" : "Bypass",
      );
    },
  );

  it("shows the harness-forced Bypass mode as read-only", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: null,
        })}
      />,
    );

    fireEvent.click(screen.getByTestId("claude-settings-control"));
    expect(screen.getByTestId("chat-mode-select")).toHaveAttribute("readonly");
    expect(screen.getByTestId("chat-mode-select")).toBeEnabled();
    expect(screen.getByTestId("chat-mode-select")).toHaveDisplayValue("Bypass");
  });

  it("displays a legacy persisted auto session mode as Bypass", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession({
            providerState: {
              claudeCode: {
                settings: {
                  permissionMode: "auto",
                  model: "claude-sonnet-4-6",
                  effort: "high",
                },
              },
            },
          }),
        })}
      />,
    );

    // Legacy `auto` always behaved as bypassPermissions; the control shows
    // its true behavior instead of a generic fallback.
    expect(screen.getByTestId("claude-permission-control")).toHaveTextContent(
      /Bypass/i,
    );

    fireEvent.click(screen.getByTestId("claude-settings-control"));
    expect(screen.getByTestId("chat-mode-select")).toHaveAttribute("readonly");
    expect(screen.getByTestId("chat-mode-select")).toHaveDisplayValue("Bypass");
  });
});

describe("ChatPanel single-send guard (chat_panel_runtime D32)", () => {
  function deferred<T>() {
    let resolve!: (value: T) => void;
    const promise = new Promise<T>((res) => {
      resolve = res;
    });
    return { promise, resolve };
  }

  function input() {
    return screen.getByTestId("chat-input") as HTMLTextAreaElement;
  }

  it("clears the draft optimistically before the send resolves", async () => {
    const pending = deferred<boolean>();
    const onSendMessage = vi.fn().mockReturnValue(pending.promise);
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          sessions: [activeSession()],
          onSendMessage,
        })}
      />,
    );

    fireEvent.change(input(), { target: { value: "hello world" } });
    fireEvent.keyDown(input(), { key: "Enter" });

    // The composer is cleared immediately, while onSendMessage is still pending
    // — not only after it resolves.
    expect(onSendMessage).toHaveBeenCalledTimes(1);
    expect(input().value).toBe("");

    await act(async () => {
      pending.resolve(true);
      await pending.promise;
    });
    expect(input().value).toBe("");
  });

  it("ignores a second Enter while the first send is in flight (no double-send)", async () => {
    const pending = deferred<boolean>();
    const onSendMessage = vi.fn().mockReturnValue(pending.promise);
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          sessions: [activeSession()],
          onSendMessage,
        })}
      />,
    );

    fireEvent.change(input(), { target: { value: "only once" } });
    fireEvent.keyDown(input(), { key: "Enter" });
    // A rapid second Enter during the in-flight window must NOT re-submit.
    fireEvent.keyDown(input(), { key: "Enter" });
    fireEvent.keyDown(input(), { key: "Enter" });

    expect(onSendMessage).toHaveBeenCalledTimes(1);

    // Once the request resolves, the composer is sendable again for the NEXT
    // turn (the in-flight guard is scoped to one request, not the whole turn).
    await act(async () => {
      pending.resolve(true);
      await pending.promise;
    });
    fireEvent.change(input(), { target: { value: "next turn" } });
    fireEvent.keyDown(input(), { key: "Enter" });
    await waitFor(() => expect(onSendMessage).toHaveBeenCalledTimes(2));
    expect(onSendMessage.mock.calls[1][0]).toBe("next turn");
  });

  it("restores the draft when the send is rejected", async () => {
    const pending = deferred<boolean>();
    const onSendMessage = vi.fn().mockReturnValue(pending.promise);
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          sessions: [activeSession()],
          onSendMessage,
        })}
      />,
    );

    fireEvent.change(input(), { target: { value: "please retry me" } });
    fireEvent.keyDown(input(), { key: "Enter" });
    expect(input().value).toBe("");

    await act(async () => {
      pending.resolve(false);
      await pending.promise;
    });
    // A failed send restores exactly what was typed so the user can retry.
    await waitFor(() => expect(input().value).toBe("please retry me"));
  });

  it.each([false, true])(
    "ignores a retired composer's failure while a replacement send is pending (existing session: %s)",
    async (existingSession) => {
      const first = deferred<boolean>();
      const second = deferred<boolean>();
      const onSendMessage = vi
        .fn()
        .mockReturnValueOnce(first.promise)
        .mockReturnValueOnce(second.promise);
      const props = defaultProps({
        session: existingSession ? activeSession() : null,
        onSendMessage,
      });
      const { rerender } = render(<ChatPanel {...props} composerScope={{}} />);
      fireEvent.change(input(), { target: { value: "old draft" } });
      fireEvent.keyDown(input(), { key: "Enter" });
      expect(onSendMessage).toHaveBeenCalledTimes(1);

      rerender(
        <ChatPanel
          {...props}
          composerScope={{}}
          composerSeed={{ token: 1, text: "replacement draft" }}
        />,
      );
      fireEvent.keyDown(input(), { key: "Enter" });
      expect(onSendMessage).toHaveBeenCalledTimes(2);
      fireEvent.change(input(), { target: { value: "next draft" } });
      await act(async () => {
        first.resolve(false);
        await first.promise;
      });
      expect(input()).toHaveValue("next draft");
      fireEvent.keyDown(input(), { key: "Enter" });
      expect(onSendMessage).toHaveBeenCalledTimes(2);

      await act(async () => {
        second.resolve(true);
        await second.promise;
      });
      expect(
        screen.getByTestId(
          existingSession ? "chat-panel-send" : "chat-panel-no-session-send",
        ),
      ).toBeEnabled();
    },
  );
});

describe("ChatPanel subscription usage chip window + pre-session gate (agent_interaction_ux D8)", () => {
  const NOW = Date.parse("2026-08-19T12:00:00Z");
  beforeEach(() => {
    vi.useFakeTimers({ toFake: ["Date", "setInterval", "clearInterval"] });
    vi.setSystemTime(NOW);
  });

  const OAUTH_SNAPSHOT: SubscriptionUsage = {
    fiveHourUtilization: 0.5,
    fiveHourResetAt: new Date(NOW + 5 * 60 * 60 * 1000).toISOString(),
    sevenDayUtilization: 0.38,
    sevenDayResetAt: new Date(NOW + 7 * 24 * 60 * 60 * 1000).toISOString(),
    representativeClaim: "seven_day",
    unifiedStatus: "allowed",
    capturedAt: new Date(NOW).toISOString(),
  };

  const OAUTH_RESOLUTION: ResolvedAgentExecutionV1 = {
    ...INHERITED_CLAUDE_CHAT_RESOLUTION,
    resolutionId: "chat-resolution-oauth",
    authMode: "anthropic_oauth",
  };

  it("renders the active-session chip headlined in the selected 5-hour window", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession({
            authMode: "anthropic_oauth",
            subscriptionUsage: OAUTH_SNAPSHOT,
          }),
          subscriptionUsageWindow: "5h",
        })}
      />,
    );
    expect(screen.getByTestId("subscription-usage-chip")).toHaveAttribute(
      "data-window",
      "5h",
    );
  });

  it("hides the chip entirely when the window setting is off", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession({
            authMode: "anthropic_oauth",
            subscriptionUsage: OAUTH_SNAPSHOT,
          }),
          subscriptionUsageWindow: "off",
        })}
      />,
    );
    expect(screen.queryByTestId("subscription-usage-chip")).toBeNull();
  });

  it("expires each window on the mounted chat clock without a provider update", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession({
            authMode: "anthropic_oauth",
            subscriptionUsage: {
              ...OAUTH_SNAPSHOT,
              fiveHourResetAt: new Date(NOW + 30_000).toISOString(),
              sevenDayResetAt: new Date(NOW + 90_000).toISOString(),
            },
          }),
          subscriptionUsageWindow: "5h",
        })}
      />,
    );
    const chip = screen.getByTestId("subscription-usage-chip");
    expect(chip).toHaveAttribute("data-window", "5h");
    fireEvent.click(screen.getByTestId("subscription-usage-button"));
    expect(screen.getByTestId("usage-tooltip-window-5h")).toBeVisible();

    act(() => vi.advanceTimersByTime(60_000));
    expect(screen.queryByTestId("usage-tooltip-window-5h")).toBeNull();
    expect(chip).toHaveAttribute("data-window", "7d");
    expect(screen.getByTestId("usage-tooltip-window-7d")).toHaveAttribute(
      "data-selected",
      "true",
    );

    act(() => vi.advanceTimersByTime(60_000));
    expect(screen.queryByTestId("subscription-usage-chip")).toBeNull();
  });

  it("renders a pre-session chip once the resolve preview reports Anthropic OAuth", async () => {
    const resolveAgentLaunch = vi.fn().mockResolvedValue(OAUTH_RESOLUTION);
    render(
      <ChatPanel
        {...defaultProps({
          session: null,
          resolveAgentLaunch,
          workspaceSubscriptionUsage: OAUTH_SNAPSHOT,
          subscriptionUsageWindow: "7d",
        })}
      />,
    );
    expect(
      await screen.findByTestId("subscription-usage-chip"),
    ).toHaveAttribute("data-window", "7d");
  });

  it("shows no pre-session chip when the workspace snapshot is absent", async () => {
    const resolveAgentLaunch = vi.fn().mockResolvedValue(OAUTH_RESOLUTION);
    render(
      <ChatPanel
        {...defaultProps({
          session: null,
          resolveAgentLaunch,
          workspaceSubscriptionUsage: null,
          subscriptionUsageWindow: "7d",
        })}
      />,
    );
    await waitFor(() => expect(resolveAgentLaunch).toHaveBeenCalled());
    expect(screen.queryByTestId("subscription-usage-chip")).toBeNull();
  });

  it("shows no pre-session chip when the resolved auth mode is not OAuth", async () => {
    // INHERITED_CLAUDE_CHAT_RESOLUTION resolves with anthropic_api_key.
    const resolveAgentLaunch = vi
      .fn()
      .mockResolvedValue(INHERITED_CLAUDE_CHAT_RESOLUTION);
    render(
      <ChatPanel
        {...defaultProps({
          session: null,
          resolveAgentLaunch,
          workspaceSubscriptionUsage: OAUTH_SNAPSHOT,
          subscriptionUsageWindow: "7d",
        })}
      />,
    );
    await waitFor(() => expect(resolveAgentLaunch).toHaveBeenCalled());
    expect(screen.queryByTestId("subscription-usage-chip")).toBeNull();
  });

  it("shows no pre-session chip when the window setting is off", async () => {
    const resolveAgentLaunch = vi.fn().mockResolvedValue(OAUTH_RESOLUTION);
    render(
      <ChatPanel
        {...defaultProps({
          session: null,
          resolveAgentLaunch,
          workspaceSubscriptionUsage: OAUTH_SNAPSHOT,
          subscriptionUsageWindow: "off",
        })}
      />,
    );
    await waitFor(() => expect(resolveAgentLaunch).toHaveBeenCalled());
    expect(screen.queryByTestId("subscription-usage-chip")).toBeNull();
  });
});

describe("Browser screenshot thumbnails (provided_agent_runtime D98)", () => {
  // Regression for the first live run, which failed twice: artifacts rendered
  // only on the trajectory fallback path (which provided-agent turns never
  // take), and the plain <img src> load could not carry the bearer header the
  // content route requires (401 -> hidden). The fixture mirrors the live
  // timeline shape field-for-field; the authed fetch is asserted explicitly.
  it("renders the gallery on the provided-agent turn path via an authed fetch", async () => {
    const fetchSpy = vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(new Blob(["png-bytes"], { type: "image/png" }), {
        status: 200,
      }),
    );
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          messages: [
            chatMessage({
              id: "assistant-shot",
              content: "DONE",
              status: "complete",
            }),
          ],
          timelineEntries: [
            timelineEntry({
              id: "shot-call",
              entryType: "tool_call",
              content:
                "Claude Code used mcp__g5-tools__browser_take_screenshot",
              metadata: {
                source: "provided_agent",
                provider: "claude_code",
                providerEvent: "provided_agent.tool.started",
                phase: "tool_call",
                sessionId: "session-1",
                messageId: "assistant-shot",
                toolCallId: "toolu-shot",
                toolName: "mcp__g5-tools__browser_take_screenshot",
                args: { scale: "css" },
              },
              sequence: 1,
            }),
            timelineEntry({
              id: "shot-result",
              entryType: "tool_result",
              content:
                "Claude Code completed mcp__g5-tools__browser_take_screenshot",
              metadata: {
                source: "provided_agent",
                provider: "claude_code",
                providerEvent: "provided_agent.tool.completed",
                phase: "tool_result",
                sessionId: "session-1",
                messageId: "assistant-shot",
                toolCallId: "toolu-shot",
                toolName: "mcp__g5-tools__browser_take_screenshot",
                isError: false,
                contentSnippet: "### Result\n- Screenshot of viewport",
                imageArtifacts: [
                  {
                    attachmentId: "94858e9a-d4b8-4fab-9281-d0fdbf44faae",
                    contentType: "image/png",
                    sizeBytes: 18788,
                  },
                ],
              },
              sequence: 2,
            }),
          ],
        })}
      />,
    );

    const gallery = screen.getByTestId("tool-result-image-artifacts");
    expect(gallery).toBeInTheDocument();
    // The bytes are fetched with the API token (browser-native <img> loads
    // cannot send Authorization) and served to the <img> as an object URL.
    await waitFor(() => {
      expect(fetchSpy).toHaveBeenCalledWith(
        "/api/v1/sessions/session-1/attachments/94858e9a-d4b8-4fab-9281-d0fdbf44faae/content",
        expect.anything(),
      );
      const img = gallery.querySelector("img");
      expect(img).not.toBeNull();
      expect(img?.getAttribute("src")).toBe("blob:mock-url");
    });
    fetchSpy.mockRestore();
  });
});

// ---------------------------------------------------------------------------
// Chat Account field — test_contract/provider_accounts I8 (Vitest half) and
// I9 (client half); right-panel session_provider_controls D30;
// agent_interaction_ux D7; external_agent_provider_contract D12.
// ---------------------------------------------------------------------------
describe("ChatPanel Account field (provider accounts I8/I9)", () => {
  function providerAccount(
    overrides: Partial<ProviderAccountDTO> & Pick<ProviderAccountDTO, "id">,
  ): ProviderAccountDTO {
    return {
      orgId: "org-1",
      name: "Account",
      kind: "anthropic_api_key",
      family: "claude",
      scope: "user",
      workspaceId: null,
      userId: "user-1",
      baseUrl: null,
      hasSecret: true,
      secretHint: null,
      createdBy: "user-1",
      createdAt: "2026-08-30T00:00:00Z",
      updatedAt: "2026-08-30T00:00:00Z",
      usage: null,
      ...overrides,
    };
  }
  const TEAM_SUBSCRIPTION = providerAccount({
    id: "acct-team-subscription-1",
    name: "Team subscription",
    kind: "claude_subscription",
    scope: "org",
    userId: null,
  });
  const MY_API_KEY = providerAccount({
    id: "acct-my-api-key-2",
    name: "My API key",
    kind: "anthropic_api_key",
    scope: "user",
  });
  const CODEX_SUBSCRIPTION = providerAccount({
    id: "acct-codex-subscription-3",
    name: "Codex subscription",
    kind: "chatgpt_subscription",
    family: "codex",
    scope: "workspace",
    workspaceId: "workspace-1",
    userId: null,
    hasSecret: false,
  });
  const ACCOUNTS = [TEAM_SUBSCRIPTION, MY_API_KEY, CODEX_SUBSCRIPTION];
  const ASSIGNED_TEAM = { claude_code: TEAM_SUBSCRIPTION.id, codex: "" };

  function resolver() {
    return vi
      .fn()
      .mockImplementation((preferences: AgentLaunchPreferencesV1) =>
        Promise.resolve(
          explicitClaudeResolution(
            preferences,
            `resolution-${preferences.account?.source === "explicit" && preferences.account.value.kind === "account" ? preferences.account.value.accountId : "runtime"}`,
          ),
        ),
      );
  }

  function lastAccountPreference(resolveAgentLaunch: ReturnType<typeof vi.fn>) {
    const calls = resolveAgentLaunch.mock.calls;
    const preferences = calls[calls.length - 1][0] as AgentLaunchPreferencesV1;
    return preferences.account;
  }

  it("renders no Account field and emits no account preference while the switch is off", async () => {
    const resolveAgentLaunch = resolver();
    render(
      <ChatPanel
        {...defaultProps({
          resolveAgentLaunch,
          providerAccountsEnabled: false,
          providerAccounts: ACCOUNTS,
          providerAccountAssignments: ASSIGNED_TEAM,
        })}
      />,
    );
    fireEvent.click(screen.getByTestId("claude-settings-control"));
    await waitFor(() =>
      expect(screen.getByTestId("chat-model-select")).toBeEnabled(),
    );
    expect(screen.queryByTestId("chat-account-select")).not.toBeInTheDocument();
    expect(lastAccountPreference(resolveAgentLaunch)).toBeUndefined();
  });

  it("initializes from the effective assignment and lists the harness's accounts with scope descriptions plus Runtime variables", async () => {
    const resolveAgentLaunch = resolver();
    render(
      <ChatPanel
        {...defaultProps({
          resolveAgentLaunch,
          providerAccountsEnabled: true,
          providerAccounts: ACCOUNTS,
          providerAccountAssignments: ASSIGNED_TEAM,
        })}
      />,
    );
    fireEvent.click(screen.getByTestId("claude-settings-control"));
    await waitFor(() =>
      expect(screen.getByTestId("chat-account-select")).toBeEnabled(),
    );
    // The field sits between Agent and Model in the shared renderer.
    expect(screen.getByTestId("chat-account-select")).toHaveTextContent(
      "Team subscription",
    );
    expect(lastAccountPreference(resolveAgentLaunch)).toEqual({
      source: "explicit",
      value: { kind: "account", accountId: TEAM_SUBSCRIPTION.id },
    });

    fireEvent.click(screen.getByTestId("chat-account-select"));
    const options = screen.getAllByRole("option").map((o) => o.textContent);
    expect(options).toEqual([
      "Runtime variables",
      "Team subscription",
      "My API key",
    ]);
    expect(screen.getByText("Claude subscription · Org")).toBeInTheDocument();
    expect(screen.getByText("Anthropic API key · You")).toBeInTheDocument();
    expect(
      screen.getByTestId(`chat-account-select-option-${TEAM_SUBSCRIPTION.id}`),
    ).toHaveAttribute("aria-selected", "true");
  });

  it("names the selected account in the composer pill and falls back to the harness label for Runtime variables", async () => {
    const resolveAgentLaunch = resolver();
    render(
      <ChatPanel
        {...defaultProps({
          resolveAgentLaunch,
          providerAccountsEnabled: true,
          providerAccounts: ACCOUNTS,
          providerAccountAssignments: ASSIGNED_TEAM,
        })}
      />,
    );
    // The assigned account replaces "Claude Code" in the pill's first segment.
    expect(screen.getByTestId("claude-harness-control")).toHaveTextContent(
      "Team subscription",
    );
    expect(screen.getByTestId("claude-harness-control")).not.toHaveTextContent(
      "Claude Code",
    );

    fireEvent.click(screen.getByTestId("claude-settings-control"));
    await waitFor(() =>
      expect(screen.getByTestId("chat-account-select")).toBeEnabled(),
    );
    chooseThemedOption("chat-account-select", "My API key");
    await waitFor(() =>
      expect(screen.getByTestId("claude-harness-control")).toHaveTextContent(
        "My API key",
      ),
    );

    chooseThemedOption("chat-account-select", "Runtime variables");
    await waitFor(() =>
      expect(screen.getByTestId("claude-harness-control")).toHaveTextContent(
        "Claude Code",
      ),
    );
  });

  it("names the Codex account in the Codex pill and keeps the harness label while the switch is off", () => {
    const { unmount } = render(
      <ChatPanel
        {...defaultProps({
          providerAccountsEnabled: true,
          providerAccounts: ACCOUNTS,
          providerAccountAssignments: {
            claude_code: "",
            codex: CODEX_SUBSCRIPTION.id,
          },
          selectedAgentProvider: "codex",
        })}
      />,
    );
    expect(screen.getByTestId("codex-harness-control")).toHaveTextContent(
      "Codex subscription",
    );
    unmount();

    render(
      <ChatPanel
        {...defaultProps({
          providerAccountsEnabled: false,
          providerAccounts: ACCOUNTS,
          providerAccountAssignments: ASSIGNED_TEAM,
        })}
      />,
    );
    expect(screen.getByTestId("claude-harness-control")).toHaveTextContent(
      "Claude Code",
    );
  });

  it("submits a picked account and Runtime variables as explicit preferences", async () => {
    const resolveAgentLaunch = resolver();
    render(
      <ChatPanel
        {...defaultProps({
          resolveAgentLaunch,
          providerAccountsEnabled: true,
          providerAccounts: ACCOUNTS,
          providerAccountAssignments: ASSIGNED_TEAM,
        })}
      />,
    );
    fireEvent.click(screen.getByTestId("claude-settings-control"));
    await waitFor(() =>
      expect(screen.getByTestId("chat-account-select")).toBeEnabled(),
    );
    chooseThemedOption("chat-account-select", "My API key");
    await waitFor(() =>
      expect(lastAccountPreference(resolveAgentLaunch)).toEqual({
        source: "explicit",
        value: { kind: "account", accountId: MY_API_KEY.id },
      }),
    );
    await waitFor(() =>
      expect(screen.getByTestId("chat-account-select")).toBeEnabled(),
    );
    chooseThemedOption("chat-account-select", "Runtime variables");
    await waitFor(() =>
      expect(lastAccountPreference(resolveAgentLaunch)).toEqual({
        source: "explicit",
        value: { kind: "runtime_config" },
      }),
    );
  });

  it("seeds Runtime variables when nothing is assigned — never a hidden default", async () => {
    const resolveAgentLaunch = resolver();
    render(
      <ChatPanel
        {...defaultProps({
          resolveAgentLaunch,
          providerAccountsEnabled: true,
          providerAccounts: ACCOUNTS,
          providerAccountAssignments: { claude_code: "", codex: "" },
        })}
      />,
    );
    fireEvent.click(screen.getByTestId("claude-settings-control"));
    await waitFor(() =>
      expect(screen.getByTestId("chat-account-select")).toBeEnabled(),
    );
    expect(screen.getByTestId("chat-account-select")).toHaveTextContent(
      "Runtime variables",
    );
    expect(lastAccountPreference(resolveAgentLaunch)).toEqual({
      source: "explicit",
      value: { kind: "runtime_config" },
    });
  });

  it("reads No account instead of Runtime variables without raw keys, and drops it once an account is chosen", async () => {
    // chat g5.chat.account-defaults: "Runtime variables" is a real choice
    // only while raw credential keys exist for the harness's family.
    const resolveAgentLaunch = resolver();
    render(
      <ChatPanel
        {...defaultProps({
          resolveAgentLaunch,
          providerAccountsEnabled: true,
          providerAccounts: ACCOUNTS,
          providerAccountAssignments: { claude_code: "", codex: "" },
          runtimeVariablesAvailable: { claude_code: false, codex: true },
        })}
      />,
    );
    fireEvent.click(screen.getByTestId("claude-settings-control"));
    await waitFor(() =>
      expect(screen.getByTestId("chat-account-select")).toBeEnabled(),
    );
    expect(screen.getByTestId("chat-account-select")).toHaveTextContent(
      "No account",
    );
    fireEvent.click(screen.getByTestId("chat-account-select"));
    expect(
      screen.getAllByRole("option").map((option) => option.textContent),
    ).toEqual(["No account", "Team subscription", "My API key"]);
    fireEvent.click(screen.getByRole("option", { name: /My API key/ }));
    await waitFor(() =>
      expect(screen.getByTestId("chat-account-select")).toHaveTextContent(
        "My API key",
      ),
    );
    fireEvent.click(screen.getByTestId("chat-account-select"));
    expect(
      screen.getAllByRole("option").map((option) => option.textContent),
    ).toEqual(["Team subscription", "My API key"]);
  });

  it("gates Send while the new account resolves and sends the confirmed account", async () => {
    let resolvePending: (resolution: ResolvedAgentExecutionV1) => void = () =>
      undefined;
    const pending = new Promise<ResolvedAgentExecutionV1>((resolve) => {
      resolvePending = resolve;
    });
    const resolveAgentLaunch = vi
      .fn()
      .mockImplementationOnce((preferences: AgentLaunchPreferencesV1) =>
        Promise.resolve(
          explicitClaudeResolution(preferences, "resolution-team"),
        ),
      )
      .mockImplementationOnce(() => pending);
    const onSendMessage = vi.fn().mockResolvedValue(true);
    render(
      <ChatPanel
        {...defaultProps({
          resolveAgentLaunch,
          onSendMessage,
          providerAccountsEnabled: true,
          providerAccounts: ACCOUNTS,
          providerAccountAssignments: ASSIGNED_TEAM,
        })}
      />,
    );
    fireEvent.change(screen.getByTestId("chat-input"), {
      target: { value: "hello" },
    });
    await waitFor(() =>
      expect(screen.getByTestId("chat-panel-no-session-send")).toBeEnabled(),
    );
    fireEvent.click(screen.getByTestId("claude-settings-control"));
    chooseThemedOption("chat-account-select", "My API key");
    await waitFor(() => expect(resolveAgentLaunch).toHaveBeenCalledTimes(2));
    expect(screen.getByTestId("chat-panel-no-session-send")).toBeDisabled();
    // Escape only closes the popover; the pending selection is kept.
    fireEvent.keyDown(document, { key: "Escape" });
    expect(
      screen.queryByTestId("claude-settings-modal"),
    ).not.toBeInTheDocument();
    expect(screen.getByTestId("chat-panel-no-session-send")).toBeDisabled();

    const secondPreferences = resolveAgentLaunch.mock
      .calls[1][0] as AgentLaunchPreferencesV1;
    await act(async () => {
      resolvePending({
        ...explicitClaudeResolution(secondPreferences, "resolution-my-key"),
        accountId: MY_API_KEY.id,
        accountKind: "anthropic_api_key",
        accountScope: "user",
      });
    });
    await waitFor(() =>
      expect(screen.getByTestId("chat-panel-no-session-send")).toBeEnabled(),
    );
    fireEvent.click(screen.getByTestId("claude-settings-control"));
    expect(screen.getByTestId("chat-account-select")).toHaveTextContent(
      "My API key",
    );
    fireEvent.keyDown(document, { key: "Escape" });
    fireEvent.click(screen.getByTestId("chat-panel-no-session-send"));
    await waitFor(() => expect(onSendMessage).toHaveBeenCalledTimes(1));
    expect(onSendMessage.mock.calls[0][1].agentLaunch).toEqual({
      preferences: expect.objectContaining({
        account: {
          source: "explicit",
          value: { kind: "account", accountId: MY_API_KEY.id },
        },
      }),
      confirmedResolutionId: "resolution-my-key",
    });
  });

  it("restores the previous confirmed account when the new one fails to resolve", async () => {
    const resolveAgentLaunch = vi
      .fn()
      .mockImplementation((preferences: AgentLaunchPreferencesV1) =>
        preferences.account?.source === "explicit" &&
        preferences.account.value.kind === "account" &&
        preferences.account.value.accountId === MY_API_KEY.id
          ? Promise.reject(new Error("account_not_visible"))
          : Promise.resolve(
              explicitClaudeResolution(preferences, "resolution-team"),
            ),
      );
    render(
      <ChatPanel
        {...defaultProps({
          resolveAgentLaunch,
          providerAccountsEnabled: true,
          providerAccounts: ACCOUNTS,
          providerAccountAssignments: ASSIGNED_TEAM,
        })}
      />,
    );
    fireEvent.click(screen.getByTestId("claude-settings-control"));
    await waitFor(() =>
      expect(screen.getByTestId("chat-account-select")).toBeEnabled(),
    );
    chooseThemedOption("chat-account-select", "My API key");
    await waitFor(() =>
      expect(screen.getByTestId("chat-resolution-error")).toHaveTextContent(
        "Could not use your selected settings: account_not_visible",
      ),
    );
    await waitFor(() =>
      expect(screen.getByTestId("chat-account-select")).toHaveTextContent(
        "Team subscription",
      ),
    );
  });

  describe("Claude OpenRouter accounts", () => {
    const OPENROUTER_ACCOUNT = providerAccount({
      id: "acct-openrouter",
      name: "OpenRouter",
      kind: "openrouter_api_key",
      family: "both",
    });
    // Saved `[1m]` picks of two-form families: no longer offered, still
    // resolver-known, and moved to the family's plain id on OpenRouter.
    const FAMILIES = [
      ["claude-opus-5[1m]", "Opus 5 (1M)", "claude-opus-5", "Opus 5"],
      ["claude-opus-4-8[1m]", "Opus 4.8 (1M)", "claude-opus-4-8", "Opus 4.8"],
      [
        "claude-fable-5-1[1m]",
        "Fable 5.1 (1M context)",
        "claude-fable-5-1",
        "Fable 5.1",
      ],
      [
        "claude-fable-5[1m]",
        "Fable 5 (1M context)",
        "claude-fable-5",
        "Fable 5",
      ],
    ];
    const accountProps = {
      providerAccountsEnabled: true,
      providerAccounts: [...ACCOUNTS, OPENROUTER_ACCOUNT],
      providerAccountAssignments: ASSIGNED_TEAM,
    };

    it.each(FAMILIES)(
      "moves a saved %s pick to its plain id and sends with the confirmed OpenRouter account",
      async (savedModel, savedLabel, routerModel, routerLabel) => {
        let confirmRouter!: (resolution: ResolvedAgentExecutionV1) => void;
        const pending = new Promise<ResolvedAgentExecutionV1>((resolve) => {
          confirmRouter = resolve;
        });
        const resolveAgentLaunch = vi.fn(
          (preferences: AgentLaunchPreferencesV1) => {
            if (
              preferences.account?.source === "explicit" &&
              preferences.account.value.kind === "account" &&
              preferences.account.value.accountId === OPENROUTER_ACCOUNT.id
            ) {
              return preferences.model.source === "explicit" &&
                preferences.model.value === routerModel
                ? pending
                : Promise.reject(
                    new Error("agent_model_unavailable_on_provider_route"),
                  );
            }
            return Promise.resolve(
              explicitClaudeResolution(preferences, "resolution-direct"),
            );
          },
        );
        const onSendMessage = vi.fn().mockResolvedValue(true);
        render(
          <ChatPanel
            {...defaultProps({
              ...accountProps,
              defaultClaudeModel: savedModel,
              resolveAgentLaunch,
              onSendMessage,
            })}
          />,
        );
        fireEvent.change(screen.getByTestId("chat-input"), {
          target: { value: "hello" },
        });
        await waitFor(() =>
          expect(
            screen.getByTestId("chat-panel-no-session-send"),
          ).toBeEnabled(),
        );
        fireEvent.click(screen.getByTestId("claude-settings-control"));
        expect(screen.getByTestId("chat-model-select")).toHaveTextContent(
          savedLabel,
        );
        chooseThemedOption("chat-account-select", "OpenRouter");

        const routerPreferences = {
          model: { source: "explicit", value: routerModel },
          account: {
            source: "explicit",
            value: { kind: "account", accountId: OPENROUTER_ACCOUNT.id },
          },
        };
        expect(resolveAgentLaunch.mock.calls[1][0]).toEqual(
          expect.objectContaining(routerPreferences),
        );
        expect(screen.getByTestId("chat-model-select")).toHaveTextContent(
          routerLabel,
        );
        expect(screen.getByTestId("chat-panel-no-session-send")).toBeDisabled();
        fireEvent.keyDown(document, { key: "Escape" });
        expect(
          screen.getByTestId("chat-model-adjustment-notice"),
        ).toHaveTextContent(
          `Switched from ${savedLabel} to ${routerLabel} because OpenRouter takes this family without the 1M suffix.`,
        );

        await act(async () => {
          confirmRouter({
            ...explicitClaudeResolution(
              resolveAgentLaunch.mock.calls[1][0],
              "resolution-router",
            ),
            providerRoute: "openrouter",
            authMode: "open_router_api_key",
            accountId: OPENROUTER_ACCOUNT.id,
            accountKind: "openrouter_api_key",
            accountScope: "user",
          });
        });
        await waitFor(() =>
          expect(
            screen.getByTestId("chat-panel-no-session-send"),
          ).toBeEnabled(),
        );
        expect(
          screen.queryByTestId("chat-resolution-error"),
        ).not.toBeInTheDocument();
        fireEvent.click(screen.getByTestId("chat-panel-no-session-send"));
        await waitFor(() => expect(onSendMessage).toHaveBeenCalledTimes(1));
        expect(onSendMessage.mock.calls[0][1]).toEqual(
          expect.objectContaining({
            model: routerModel,
            agentLaunch: {
              preferences: expect.objectContaining(routerPreferences),
              confirmedResolutionId: "resolution-router",
            },
          }),
        );

        fireEvent.click(screen.getByTestId("claude-settings-control"));
        expect(screen.getByTestId("chat-account-select")).toHaveTextContent(
          "OpenRouter",
        );
        fireEvent.click(screen.getByTestId("chat-model-select"));
        // One plain entry per family, newest first; no `[1m]` entries.
        for (const [, saved, , plain] of FAMILIES) {
          expect(
            screen.queryByRole("option", { name: saved }),
          ).not.toBeInTheDocument();
          expect(
            screen.getByRole("option", { name: plain }),
          ).toBeInTheDocument();
        }
        expect(
          screen
            .getAllByRole("option")
            .map((option) => option.textContent)
            .slice(0, 4),
        ).toEqual(["Fable 5.2", "Sonnet 5.5", "Opus 5.5", "Fable 5.1"]);
        fireEvent.click(screen.getByRole("option", { name: routerLabel }));
        chooseThemedOption("chat-account-select", "Team subscription");
        await waitFor(() =>
          expect(screen.getByTestId("chat-model-select")).toBeEnabled(),
        );
        expect(screen.getByTestId("chat-model-select")).toHaveTextContent(
          routerLabel,
        );
        expect(resolveAgentLaunch.mock.lastCall?.[0]).toEqual(
          expect.objectContaining({
            model: { source: "explicit", value: routerModel },
            account: {
              source: "explicit",
              value: { kind: "account", accountId: TEAM_SUBSCRIPTION.id },
            },
          }),
        );
        expect(
          screen.queryByTestId("chat-model-adjustment-notice"),
        ).not.toBeInTheDocument();
        fireEvent.click(screen.getByTestId("chat-model-select"));
        expect(
          screen.getByRole("option", { name: routerLabel }),
        ).toBeInTheDocument();
      },
    );

    it("represents the default family once: a saved Opus 5.5 1M pick is the bundled default", async () => {
      const resolveAgentLaunch = resolver();
      render(
        <ChatPanel
          {...defaultProps({
            ...accountProps,
            defaultClaudeModel: "claude-opus-5-5[1m]",
            resolveAgentLaunch,
          })}
        />,
      );
      fireEvent.click(screen.getByTestId("claude-settings-control"));
      await waitFor(() =>
        expect(screen.getByTestId("chat-model-select")).toBeEnabled(),
      );
      expect(screen.getByTestId("chat-model-select")).toHaveTextContent(
        "Opus 5.5",
      );
      chooseThemedOption("chat-account-select", "OpenRouter");
      await waitFor(() =>
        expect(screen.getByTestId("chat-model-select")).toBeEnabled(),
      );
      expect(resolveAgentLaunch.mock.lastCall?.[0]).toEqual(
        expect.objectContaining({
          model: { source: "explicit", value: CLAUDE_CHAT_DEFAULT_MODEL },
        }),
      );
      expect(
        screen.queryByTestId("chat-model-adjustment-notice"),
      ).not.toBeInTheDocument();
      fireEvent.click(screen.getByTestId("chat-model-select"));
      expect(screen.getAllByRole("option", { name: "Opus 5.5" })).toHaveLength(
        1,
      );
    });

    it("normalizes a remembered 1M pick before resolving an assigned OpenRouter account", async () => {
      const resolveAgentLaunch = resolver();
      render(
        <ChatPanel
          {...defaultProps({
            ...accountProps,
            providerAccountAssignments: {
              claude_code: OPENROUTER_ACCOUNT.id,
              codex: "",
            },
            defaultClaudeModel: "claude-fable-5[1m]",
            resolveAgentLaunch,
          })}
        />,
      );
      fireEvent.click(screen.getByTestId("claude-settings-control"));
      await waitFor(() =>
        expect(screen.getByTestId("chat-model-select")).toBeEnabled(),
      );
      expect(resolveAgentLaunch).toHaveBeenCalledTimes(1);
      expect(resolveAgentLaunch.mock.calls[0][0]).toEqual(
        expect.objectContaining({
          model: { source: "explicit", value: "claude-fable-5" },
          account: {
            source: "explicit",
            value: { kind: "account", accountId: OPENROUTER_ACCOUNT.id },
          },
        }),
      );
      expect(
        screen.getByTestId("chat-model-adjustment-notice"),
      ).toHaveTextContent("Fable 5 (1M context)");
      fireEvent.click(screen.getByTestId("chat-model-select"));
      expect(
        screen.queryByRole("option", { name: "Fable 5 (1M context)" }),
      ).not.toBeInTheDocument();
      fireEvent.click(screen.getByRole("option", { name: "Fable 5" }));
      chooseThemedOption("chat-account-select", "Team subscription");
      await waitFor(() =>
        expect(screen.getByTestId("chat-model-select")).toBeEnabled(),
      );
      expect(screen.getByTestId("chat-model-select")).toHaveTextContent(
        /^Fable 5$/,
      );
    });

    it.each([
      ["claude-fable-5", "Fable 5"],
      [CLAUDE_CHAT_DEFAULT_MODEL, "Opus 5.5"],
      ["claude-sonnet-5", "Sonnet 5"],
      ["claude-haiku-4-5", "Haiku 4.5"],
      ["claude-opus-4-7", "Opus 4.7"],
      ["custom-model-pin", "custom-model-pin"],
    ])("keeps %s unchanged when selecting OpenRouter", async (model, label) => {
      const resolveAgentLaunch = resolver();
      render(
        <ChatPanel
          {...defaultProps({
            ...accountProps,
            defaultClaudeModel: model,
            resolveAgentLaunch,
          })}
        />,
      );
      fireEvent.click(screen.getByTestId("claude-settings-control"));
      await waitFor(() =>
        expect(screen.getByTestId("chat-account-select")).toBeEnabled(),
      );
      chooseThemedOption("chat-account-select", "OpenRouter");
      await waitFor(() =>
        expect(screen.getByTestId("chat-model-select")).toBeEnabled(),
      );
      expect(resolveAgentLaunch.mock.lastCall?.[0]).toEqual(
        expect.objectContaining({
          model: { source: "explicit", value: model },
          account: {
            source: "explicit",
            value: { kind: "account", accountId: OPENROUTER_ACCOUNT.id },
          },
        }),
      );
      expect(screen.getByTestId("chat-model-select")).toHaveTextContent(label);
      expect(
        screen.queryByTestId("chat-model-adjustment-notice"),
      ).not.toBeInTheDocument();
    });

    it("leaves a Runtime variables choice to the resolver and names its refusal", async () => {
      const resolveAgentLaunch = vi
        .fn()
        .mockRejectedValue(
          new Error("agent_model_unavailable_on_provider_route"),
        );
      render(
        <ChatPanel
          {...defaultProps({
            ...accountProps,
            providerAccountAssignments: { claude_code: "", codex: "" },
            defaultClaudeModel: "claude-sonnet-5",
            resolveAgentLaunch,
          })}
        />,
      );
      await waitFor(() =>
        expect(screen.getByTestId("chat-resolution-error")).toHaveTextContent(
          "Could not use your selected settings: agent_model_unavailable_on_provider_route",
        ),
      );
      expect(resolveAgentLaunch.mock.calls[0][0].model).toEqual({
        source: "explicit",
        value: "claude-sonnet-5",
      });
      expect(
        screen.queryByTestId("chat-model-adjustment-notice"),
      ).not.toBeInTheDocument();
      fireEvent.change(screen.getByTestId("chat-input"), {
        target: { value: "hello" },
      });
      expect(screen.getByTestId("chat-panel-no-session-send")).toBeDisabled();
    });

    it("rolls back a refused account and model together, then saves and sends both on retry", async () => {
      const onSendProviderControl = vi
        .fn()
        .mockRejectedValueOnce(new Error("account_not_visible"))
        .mockResolvedValue(undefined);
      const onSendMessage = vi.fn().mockResolvedValue(true);
      render(
        <ChatPanel
          {...defaultProps({
            ...accountProps,
            session: activeSession({
              model: "claude-fable-5[1m]",
              nextAccount: { kind: "account", accountId: TEAM_SUBSCRIPTION.id },
            }),
            onSendProviderControl,
            onSendMessage,
          })}
        />,
      );
      fireEvent.click(screen.getByTestId("claude-settings-control"));
      chooseThemedOption("chat-account-select", "OpenRouter");
      expect(onSendProviderControl).toHaveBeenCalledWith({
        sessionId: "session-1",
        action: "set_settings",
        payload: {
          model: "claude-fable-5",
          effort: expect.any(String),
          account: { kind: "account", accountId: OPENROUTER_ACCOUNT.id },
        },
      });
      await waitFor(() =>
        expect(
          screen.getByTestId("chat-provider-settings-error"),
        ).toHaveTextContent("account_not_visible"),
      );
      expect(screen.getByTestId("chat-account-select")).toHaveTextContent(
        "Team subscription",
      );
      expect(screen.getByTestId("chat-model-select")).toHaveTextContent(
        "Fable 5 (1M context)",
      );
      expect(
        screen.queryByTestId("chat-model-adjustment-notice"),
      ).not.toBeInTheDocument();

      chooseThemedOption("chat-account-select", "OpenRouter");
      await waitFor(() =>
        expect(screen.getByTestId("chat-account-select")).toBeEnabled(),
      );
      expect(onSendProviderControl).toHaveBeenCalledTimes(2);
      expect(screen.getByTestId("chat-model-select")).toHaveTextContent(
        /^Fable 5$/,
      );
      fireEvent.keyDown(document, { key: "Escape" });
      sendLaterTurn("hello on OpenRouter");
      await waitFor(() => expect(onSendMessage).toHaveBeenCalledTimes(1));
      expect(onSendMessage.mock.calls[0][1]).toEqual(
        expect.objectContaining({
          model: "claude-fable-5",
          account: { kind: "account", accountId: OPENROUTER_ACCOUNT.id },
        }),
      );
    });
  });

  it("resets the field to the new harness's assignment on a provider switch", async () => {
    const resolveAgentLaunch = resolver();
    render(
      <ChatPanel
        {...defaultProps({
          resolveAgentLaunch,
          availableAgentProviders: ["claude_code", "codex"],

          providerAccountsEnabled: true,
          providerAccounts: ACCOUNTS,
          providerAccountAssignments: {
            claude_code: TEAM_SUBSCRIPTION.id,
            codex: CODEX_SUBSCRIPTION.id,
          },
        })}
      />,
    );
    fireEvent.click(screen.getByTestId("claude-settings-control"));
    await waitFor(() =>
      expect(screen.getByTestId("chat-account-select")).toBeEnabled(),
    );
    chooseThemedOption("chat-account-select", "My API key");
    await waitFor(() =>
      expect(screen.getByTestId("chat-account-select")).toBeEnabled(),
    );
    chooseThemedOption("chat-agent-select", "Codex");
    await waitFor(() =>
      expect(screen.getByTestId("chat-account-select")).toHaveTextContent(
        "Codex subscription",
      ),
    );
    fireEvent.click(screen.getByTestId("chat-account-select"));
    expect(screen.getAllByRole("option").map((o) => o.textContent)).toEqual([
      "Runtime variables",
      "Codex subscription",
    ]);
    expect(
      screen.getByText("ChatGPT subscription · Workspace"),
    ).toBeInTheDocument();
  });

  it("hydrates an active session from nextAccount, then the frozen account, then Runtime variables", () => {
    const { rerender } = render(
      <ChatPanel
        {...defaultProps({
          session: activeSession({
            account: {
              id: TEAM_SUBSCRIPTION.id,
              kind: "claude_subscription",
              scope: "org",
            },
            nextAccount: { kind: "account", accountId: MY_API_KEY.id },
          }),
          providerAccountsEnabled: true,
          providerAccounts: ACCOUNTS,
          providerAccountAssignments: ASSIGNED_TEAM,
        })}
      />,
    );
    fireEvent.click(screen.getByTestId("claude-settings-control"));
    expect(screen.getByTestId("chat-account-select")).toHaveTextContent(
      "My API key",
    );
    expect(screen.getByTestId("chat-agent-select")).toBeDisabled();

    rerender(
      <ChatPanel
        {...defaultProps({
          session: activeSession({
            id: "session-2",
            account: {
              id: TEAM_SUBSCRIPTION.id,
              kind: "claude_subscription",
              scope: "org",
            },
            nextAccount: null,
          }),
          providerAccountsEnabled: true,
          providerAccounts: ACCOUNTS,
          providerAccountAssignments: ASSIGNED_TEAM,
        })}
      />,
    );
    expect(screen.getByTestId("chat-account-select")).toHaveTextContent(
      "Team subscription",
    );

    rerender(
      <ChatPanel
        {...defaultProps({
          session: activeSession({ id: "session-3" }),
          providerAccountsEnabled: true,
          providerAccounts: ACCOUNTS,
          providerAccountAssignments: ASSIGNED_TEAM,
        })}
      />,
    );
    expect(screen.getByTestId("chat-account-select")).toHaveTextContent(
      "Runtime variables",
    );
  });

  it("names the hydrated next-turn account in an active session's pill and keeps the harness label for an unavailable id", () => {
    const { rerender } = render(
      <ChatPanel
        {...defaultProps({
          session: activeSession({
            account: {
              id: TEAM_SUBSCRIPTION.id,
              kind: "claude_subscription",
              scope: "org",
            },
            nextAccount: { kind: "account", accountId: MY_API_KEY.id },
          }),
          providerAccountsEnabled: true,
          providerAccounts: ACCOUNTS,
          providerAccountAssignments: ASSIGNED_TEAM,
        })}
      />,
    );
    // The pill names the committed next-turn account, not the frozen one.
    expect(screen.getByTestId("claude-harness-control")).toHaveTextContent(
      "My API key",
    );

    rerender(
      <ChatPanel
        {...defaultProps({
          session: activeSession({
            id: "session-unavailable",
            nextAccount: { kind: "account", accountId: "acct-gone-0000" },
          }),
          providerAccountsEnabled: true,
          providerAccounts: ACCOUNTS,
          providerAccountAssignments: ASSIGNED_TEAM,
        })}
      />,
    );
    // An id absent from the visible list keeps the harness label in the pill
    // while the field itself shows the Unavailable row.
    expect(screen.getByTestId("claude-harness-control")).toHaveTextContent(
      "Claude Code",
    );
    fireEvent.click(screen.getByTestId("claude-settings-control"));
    expect(screen.getByTestId("chat-account-select")).toHaveTextContent(
      "Unavailable (acct-gon)",
    );
  });

  it("auto-persists an active-session account change as one composite set_settings and states next-turn scope while running", async () => {
    const onSendProviderControl = vi.fn().mockResolvedValue(undefined);
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession({
            nextAccount: { kind: "account", accountId: TEAM_SUBSCRIPTION.id },
          }),
          isStreaming: true,
          onSendProviderControl,
          providerAccountsEnabled: true,
          providerAccounts: ACCOUNTS,
          providerAccountAssignments: ASSIGNED_TEAM,
        })}
      />,
    );
    fireEvent.click(screen.getByTestId("claude-settings-control"));
    expect(
      screen.getByTestId("chat-provider-settings-scope"),
    ).toHaveTextContent(
      "Model, Effort, and Account changes apply to the next message you send",
    );
    expect(
      screen.queryByTestId("chat-provider-settings-apply"),
    ).not.toBeInTheDocument();
    chooseThemedOption("chat-account-select", "My API key");
    await waitFor(() => expect(onSendProviderControl).toHaveBeenCalledTimes(1));
    expect(onSendProviderControl).toHaveBeenCalledWith({
      sessionId: "session-1",
      action: "set_settings",
      payload: {
        model: CLAUDE_CHAT_DEFAULT_MODEL,
        effort: expect.any(String),
        account: { kind: "account", accountId: MY_API_KEY.id },
      },
    });
    await waitFor(() =>
      expect(screen.getByTestId("chat-account-select")).toBeEnabled(),
    );
    expect(screen.getByTestId("chat-account-select")).toHaveTextContent(
      "My API key",
    );
    // Escape is close-only: the persisted choice survives reopening.
    fireEvent.keyDown(document, { key: "Escape" });
    fireEvent.click(screen.getByTestId("claude-settings-control"));
    expect(screen.getByTestId("chat-account-select")).toHaveTextContent(
      "My API key",
    );
    expect(onSendProviderControl).toHaveBeenCalledTimes(1);

    // Model changes carry the current account in the same composite payload.
    chooseThemedOption("chat-model-select", "Fable 5");
    await waitFor(() => expect(onSendProviderControl).toHaveBeenCalledTimes(2));
    expect(onSendProviderControl.mock.calls[1][0].payload).toEqual(
      expect.objectContaining({
        model: "claude-fable-5",
        account: { kind: "account", accountId: MY_API_KEY.id },
      }),
    );
  });

  it("rolls back a rejected account change and shows the typed reason", async () => {
    const onSendProviderControl = vi.fn().mockRejectedValue(
      new ApiError({
        code: "VALIDATION_ERROR",
        message: "Validation failed",
        status: 422,
        details: { code: "account_not_visible" },
      }),
    );
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession({
            nextAccount: { kind: "account", accountId: TEAM_SUBSCRIPTION.id },
          }),
          onSendProviderControl,
          providerAccountsEnabled: true,
          providerAccounts: ACCOUNTS,
          providerAccountAssignments: ASSIGNED_TEAM,
        })}
      />,
    );
    fireEvent.click(screen.getByTestId("claude-settings-control"));
    chooseThemedOption("chat-account-select", "Runtime variables");
    await waitFor(() =>
      expect(
        screen.getByTestId("chat-provider-settings-error"),
      ).toHaveTextContent(
        "That account is not visible from this chat or no longer exists.",
      ),
    );
    expect(screen.getByTestId("chat-account-select")).toHaveTextContent(
      "Team subscription",
    );
    expect(onSendProviderControl).toHaveBeenCalledWith(
      expect.objectContaining({
        payload: expect.objectContaining({
          account: { kind: "runtime_config" },
        }),
      }),
    );
  });

  it("keeps an in-flight account change when a session refetch carrying the previous nextAccount lands", async () => {
    // right-panel D30: the active editor hydrates from the COMMITTED value;
    // a refetched session object with the same committed account must not
    // revert the optimistic selection while the composite mutation is in
    // flight — only a server-side change of `nextAccount` re-hydrates, and
    // only until the user picks an Account in this chat.
    let settle: () => void = () => undefined;
    const onSendProviderControl = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          settle = resolve;
        }),
    );
    const committedTeam = {
      nextAccount: { kind: "account", accountId: TEAM_SUBSCRIPTION.id },
    } as const;
    const panelProps = (session: ChatSessionDTO) =>
      defaultProps({
        session,
        onSendProviderControl,
        providerAccountsEnabled: true,
        providerAccounts: ACCOUNTS,
        providerAccountAssignments: ASSIGNED_TEAM,
      });
    const { rerender } = render(
      <ChatPanel {...panelProps(activeSession(committedTeam))} />,
    );
    fireEvent.click(screen.getByTestId("claude-settings-control"));
    chooseThemedOption("chat-account-select", "My API key");
    await waitFor(() => expect(onSendProviderControl).toHaveBeenCalledTimes(1));
    expect(screen.getByTestId("chat-account-select")).toHaveTextContent(
      "My API key",
    );

    // A refetch lands mid-flight: a NEW session object, same committed value.
    rerender(
      <ChatPanel
        {...panelProps(
          activeSession({
            ...committedTeam,
            updatedAt: "2026-05-10T00:00:01Z",
          }),
        )}
      />,
    );
    expect(screen.getByTestId("chat-account-select")).toHaveTextContent(
      "My API key",
    );

    await act(async () => {
      settle();
    });
    await waitFor(() =>
      expect(screen.getByTestId("chat-account-select")).toBeEnabled(),
    );
    expect(screen.getByTestId("chat-account-select")).toHaveTextContent(
      "My API key",
    );

    // The pick is saved by the next turn that carries it (routes/chat D33),
    // so later changes of the saved account, whether an earlier turn's save
    // or another tab's, leave it in place.
    rerender(
      <ChatPanel
        {...panelProps(
          activeSession({
            nextAccount: { kind: "account", accountId: TEAM_SUBSCRIPTION.id },
            updatedAt: "2026-05-10T00:00:02Z",
          }),
        )}
      />,
    );
    expect(screen.getByTestId("chat-account-select")).toHaveTextContent(
      "My API key",
    );
    rerender(
      <ChatPanel
        {...panelProps(
          activeSession({
            nextAccount: { kind: "account", accountId: MY_API_KEY.id },
            updatedAt: "2026-05-10T00:00:03Z",
          }),
        )}
      />,
    );
    expect(screen.getByTestId("chat-account-select")).toHaveTextContent(
      "My API key",
    );
    rerender(
      <ChatPanel
        {...panelProps(
          activeSession({
            nextAccount: { kind: "runtime_config" },
            updatedAt: "2026-05-10T00:00:04Z",
          }),
        )}
      />,
    );
    expect(screen.getByTestId("chat-account-select")).toHaveTextContent(
      "My API key",
    );
  });

  it("shows a no-longer-visible account as Unavailable and lets the user move away from it", async () => {
    const onSendProviderControl = vi.fn().mockResolvedValue(undefined);
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession({
            nextAccount: { kind: "account", accountId: "acct-gone-9999" },
          }),
          onSendProviderControl,
          providerAccountsEnabled: true,
          providerAccounts: ACCOUNTS,
          providerAccountAssignments: ASSIGNED_TEAM,
        })}
      />,
    );
    fireEvent.click(screen.getByTestId("claude-settings-control"));
    expect(screen.getByTestId("chat-account-select")).toHaveTextContent(
      "Unavailable (acct-gon)",
    );
    chooseThemedOption("chat-account-select", "Team subscription");
    await waitFor(() => expect(onSendProviderControl).toHaveBeenCalledTimes(1));
    expect(screen.getByTestId("chat-account-select")).toHaveTextContent(
      "Team subscription",
    );
    // The select is disabled until the account change settles.
    await waitFor(() =>
      expect(screen.getByTestId("chat-account-select")).toBeEnabled(),
    );
    fireEvent.click(screen.getByTestId("chat-account-select"));
    expect(
      screen.getAllByRole("option").map((o) => o.textContent),
    ).not.toContain("Unavailable (acct-gon)");
  });

  function sendLaterTurn(text: string) {
    fireEvent.change(screen.getByTestId("chat-input"), {
      target: { value: text },
    });
    fireEvent.keyDown(screen.getByTestId("chat-input"), { key: "Enter" });
  }

  it("leaves a later turn's account to the chat until the user picks one (routes/chat D33)", async () => {
    // A chat started before its next-turn account was saved: only the
    // frozen account of its first turn names it, and nothing is assigned
    // (QA session 31fc8e20, 2026-09-28). The pill shows that account, and
    // the server applies it to a turn that names none.
    const onSendMessage = vi.fn().mockResolvedValue(true);
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession({
            account: {
              id: TEAM_SUBSCRIPTION.id,
              kind: "claude_subscription",
              scope: "org",
            },
            nextAccount: null,
          }),
          onSendMessage,
          providerAccountsEnabled: true,
          providerAccounts: ACCOUNTS,
          providerAccountAssignments: { claude_code: "", codex: "" },
        })}
      />,
    );
    expect(screen.getByTestId("claude-harness-control")).toHaveTextContent(
      "Team subscription",
    );
    sendLaterTurn("follow up");
    await waitFor(() => expect(onSendMessage).toHaveBeenCalledTimes(1));
    expect(onSendMessage.mock.calls[0][1].account).toBeUndefined();
  });

  it("sends a changed account, including Runtime variables, with the next turn", async () => {
    const onSendMessage = vi.fn().mockResolvedValue(true);
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession({
            nextAccount: { kind: "account", accountId: TEAM_SUBSCRIPTION.id },
          }),
          onSendMessage,
          onSendProviderControl: vi.fn().mockResolvedValue(undefined),
          providerAccountsEnabled: true,
          providerAccounts: ACCOUNTS,
          providerAccountAssignments: ASSIGNED_TEAM,
        })}
      />,
    );
    fireEvent.click(screen.getByTestId("claude-settings-control"));
    chooseThemedOption("chat-account-select", "My API key");
    await waitFor(() =>
      expect(screen.getByTestId("chat-account-select")).toBeEnabled(),
    );
    fireEvent.keyDown(document, { key: "Escape" });
    sendLaterTurn("use my key");
    await waitFor(() => expect(onSendMessage).toHaveBeenCalledTimes(1));
    expect(onSendMessage.mock.calls[0][1].account).toEqual({
      kind: "account",
      accountId: MY_API_KEY.id,
    });

    fireEvent.click(screen.getByTestId("claude-settings-control"));
    chooseThemedOption("chat-account-select", "Runtime variables");
    await waitFor(() =>
      expect(screen.getByTestId("chat-account-select")).toBeEnabled(),
    );
    fireEvent.keyDown(document, { key: "Escape" });
    sendLaterTurn("use the runtime variables");
    await waitFor(() => expect(onSendMessage).toHaveBeenCalledTimes(2));
    expect(onSendMessage.mock.calls[1][1].account).toEqual({
      kind: "runtime_config",
    });
  });

  it("keeps the composer's model and effort when a turn saves only its account", async () => {
    const onSendMessage = vi.fn().mockResolvedValue(true);
    const savedSettings = {
      model: "claude-opus-4-8[1m]",
      permissionMode: "bypassPermissions",
      effort: "high",
    };
    const panelProps = (account: string, updatedAt: string) =>
      defaultProps({
        session: activeSession({
          providerState: {
            claudeCode: {
              settings: {
                ...savedSettings,
                account: { kind: "account", accountId: account },
              },
            },
          },
          nextAccount: { kind: "account", accountId: account },
          updatedAt,
        }),
        onSendMessage,
        onSendProviderControl: vi.fn().mockResolvedValue(undefined),
        providerAccountsEnabled: true,
        providerAccounts: ACCOUNTS,
        providerAccountAssignments: ASSIGNED_TEAM,
      });
    const { rerender } = render(
      <ChatPanel
        {...panelProps(TEAM_SUBSCRIPTION.id, "2026-05-10T00:00:00Z")}
      />,
    );
    fireEvent.click(screen.getByTestId("claude-settings-control"));
    chooseThemedOption("chat-model-select", "Fable 5");
    await waitFor(() =>
      expect(screen.getByTestId("chat-effort-select")).toBeEnabled(),
    );
    chooseThemedOption("chat-effort-select", "Medium");
    await waitFor(() =>
      expect(screen.getByTestId("chat-account-select")).toBeEnabled(),
    );
    chooseThemedOption("chat-account-select", "My API key");
    await waitFor(() =>
      expect(screen.getByTestId("chat-account-select")).toBeEnabled(),
    );
    fireEvent.keyDown(document, { key: "Escape" });
    sendLaterTurn("first");
    await waitFor(() => expect(onSendMessage).toHaveBeenCalledTimes(1));
    const first = onSendMessage.mock.calls[0][1];
    expect(first.model).not.toBe(savedSettings.model);
    expect(first.effort).toBe("medium");

    // The server saves only the account the turn named; the refetched
    // session carries it next to the unchanged saved model and effort.
    rerender(
      <ChatPanel {...panelProps(MY_API_KEY.id, "2026-05-10T00:00:05Z")} />,
    );
    sendLaterTurn("second");
    await waitFor(() => expect(onSendMessage).toHaveBeenCalledTimes(2));
    expect(onSendMessage.mock.calls[1][1]).toMatchObject({
      model: first.model,
      effort: "medium",
      account: { kind: "account", accountId: MY_API_KEY.id },
    });
  });

  it("keeps an Account picked while a turn is sending when that turn's saved account arrives", async () => {
    let finishFirstSend: (sent: boolean) => void = () => {};
    const onSendMessage = vi
      .fn()
      .mockImplementationOnce(
        () =>
          new Promise<boolean>((resolve) => {
            finishFirstSend = resolve;
          }),
      )
      .mockResolvedValue(true);
    const panelProps = (nextAccount: ChatSessionDTO["nextAccount"]) =>
      defaultProps({
        session: activeSession({ nextAccount }),
        onSendMessage,
        onSendProviderControl: vi.fn().mockResolvedValue(undefined),
        providerAccountsEnabled: true,
        providerAccounts: ACCOUNTS,
        providerAccountAssignments: ASSIGNED_TEAM,
      });
    const { rerender } = render(
      <ChatPanel {...panelProps({ kind: "runtime_config" })} />,
    );
    fireEvent.click(screen.getByTestId("claude-settings-control"));
    chooseThemedOption("chat-account-select", "My API key");
    await waitFor(() =>
      expect(screen.getByTestId("chat-account-select")).toBeEnabled(),
    );
    fireEvent.keyDown(document, { key: "Escape" });
    sendLaterTurn("use my key");
    await waitFor(() => expect(onSendMessage).toHaveBeenCalledTimes(1));

    // While that turn is sending, the user goes back to Runtime variables.
    fireEvent.click(screen.getByTestId("claude-settings-control"));
    chooseThemedOption("chat-account-select", "Runtime variables");
    await waitFor(() =>
      expect(screen.getByTestId("chat-account-select")).toBeEnabled(),
    );
    fireEvent.keyDown(document, { key: "Escape" });

    // The turn is accepted and saves the account it named; the refetched
    // session carries that earlier choice.
    await act(async () => finishFirstSend(true));
    rerender(
      <ChatPanel
        {...panelProps({ kind: "account", accountId: MY_API_KEY.id })}
      />,
    );
    sendLaterTurn("use the runtime variables");
    await waitFor(() => expect(onSendMessage).toHaveBeenCalledTimes(2));
    expect(onSendMessage.mock.calls[1][1].account).toEqual({
      kind: "runtime_config",
    });
  });

  it("shows another chat's saved account after a turn changed this chat's", async () => {
    const onSendMessage = vi.fn().mockResolvedValue(true);
    const panelProps = (session: ChatSessionDTO) =>
      defaultProps({
        session,
        onSendMessage,
        onSendProviderControl: vi.fn().mockResolvedValue(undefined),
        providerAccountsEnabled: true,
        providerAccounts: ACCOUNTS,
        providerAccountAssignments: ASSIGNED_TEAM,
      });
    const { rerender } = render(
      <ChatPanel
        {...panelProps(
          activeSession({
            nextAccount: { kind: "account", accountId: TEAM_SUBSCRIPTION.id },
          }),
        )}
      />,
    );
    fireEvent.click(screen.getByTestId("claude-settings-control"));
    chooseThemedOption("chat-account-select", "My API key");
    await waitFor(() =>
      expect(screen.getByTestId("chat-account-select")).toBeEnabled(),
    );
    fireEvent.keyDown(document, { key: "Escape" });
    sendLaterTurn("use my key");
    await waitFor(() => expect(onSendMessage).toHaveBeenCalledTimes(1));
    fireEvent.click(screen.getByTestId("claude-settings-control"));
    chooseThemedOption("chat-account-select", "Team subscription");
    await waitFor(() =>
      expect(screen.getByTestId("chat-account-select")).toBeEnabled(),
    );
    fireEvent.keyDown(document, { key: "Escape" });

    // Another chat whose saved account is the one this chat's turn carried.
    rerender(
      <ChatPanel
        {...panelProps(
          activeSession({
            id: "session-2",
            nextAccount: { kind: "account", accountId: MY_API_KEY.id },
          }),
        )}
      />,
    );
    await waitFor(() =>
      expect(screen.getByTestId("claude-harness-control")).toHaveTextContent(
        "My API key",
      ),
    );
  });

  it("keeps and sends the shown account once the user picks it", async () => {
    const onSendMessage = vi.fn().mockResolvedValue(true);
    const panelProps = (accountId: string) =>
      defaultProps({
        session: activeSession({ nextAccount: { kind: "account", accountId } }),
        onSendMessage,
        onSendProviderControl: vi.fn().mockResolvedValue(undefined),
        providerAccountsEnabled: true,
        providerAccounts: ACCOUNTS,
        providerAccountAssignments: ASSIGNED_TEAM,
      });
    const { rerender } = render(
      <ChatPanel {...panelProps(TEAM_SUBSCRIPTION.id)} />,
    );
    fireEvent.click(screen.getByTestId("claude-settings-control"));
    chooseThemedOption("chat-account-select", "Team subscription");
    fireEvent.keyDown(document, { key: "Escape" });

    // Another tab later saves a different account.
    rerender(<ChatPanel {...panelProps(MY_API_KEY.id)} />);
    sendLaterTurn("stay on the team subscription");
    await waitFor(() => expect(onSendMessage).toHaveBeenCalledTimes(1));
    expect(onSendMessage.mock.calls[0][1].account).toEqual({
      kind: "account",
      accountId: TEAM_SUBSCRIPTION.id,
    });
  });

  it("follows account changes made elsewhere after a turn on the saved account", async () => {
    const onSendMessage = vi.fn().mockResolvedValue(true);
    const panelProps = (accountId: string) =>
      defaultProps({
        session: activeSession({ nextAccount: { kind: "account", accountId } }),
        onSendMessage,
        onSendProviderControl: vi.fn().mockResolvedValue(undefined),
        providerAccountsEnabled: true,
        providerAccounts: ACCOUNTS,
        providerAccountAssignments: ASSIGNED_TEAM,
      });
    const { rerender } = render(
      <ChatPanel {...panelProps(TEAM_SUBSCRIPTION.id)} />,
    );
    sendLaterTurn("keep going");
    await waitFor(() => expect(onSendMessage).toHaveBeenCalledTimes(1));
    expect(onSendMessage.mock.calls[0][1].account).toBeUndefined();

    // Turns sent from another tab move the chat to another account and back.
    rerender(<ChatPanel {...panelProps(MY_API_KEY.id)} />);
    await waitFor(() =>
      expect(screen.getByTestId("claude-harness-control")).toHaveTextContent(
        "My API key",
      ),
    );
    rerender(<ChatPanel {...panelProps(TEAM_SUBSCRIPTION.id)} />);
    await waitFor(() =>
      expect(screen.getByTestId("claude-harness-control")).toHaveTextContent(
        "Team subscription",
      ),
    );
  });

  it("keeps an Account picked while a turn is sending when an earlier turn's save has not come back yet", async () => {
    let finishSecondSend: (sent: boolean) => void = () => {};
    const onSendMessage = vi
      .fn()
      .mockResolvedValueOnce(true)
      .mockImplementationOnce(
        () =>
          new Promise<boolean>((resolve) => {
            finishSecondSend = resolve;
          }),
      )
      .mockResolvedValue(true);
    const panelProps = (nextAccount: ChatSessionDTO["nextAccount"]) =>
      defaultProps({
        session: activeSession({ nextAccount }),
        onSendMessage,
        onSendProviderControl: vi.fn().mockResolvedValue(undefined),
        providerAccountsEnabled: true,
        providerAccounts: ACCOUNTS,
        providerAccountAssignments: ASSIGNED_TEAM,
      });
    const pickAccount = async (name: string) => {
      fireEvent.click(screen.getByTestId("claude-settings-control"));
      chooseThemedOption("chat-account-select", name);
      await waitFor(() =>
        expect(screen.getByTestId("chat-account-select")).toBeEnabled(),
      );
      fireEvent.keyDown(document, { key: "Escape" });
    };
    const { rerender } = render(
      <ChatPanel {...panelProps({ kind: "runtime_config" })} />,
    );
    await pickAccount("My API key");
    sendLaterTurn("use my key");
    await waitFor(() => expect(onSendMessage).toHaveBeenCalledTimes(1));

    // Before the first turn's save comes back, the next turn goes back to
    // Runtime variables, which the cached session still shows.
    await pickAccount("Runtime variables");
    sendLaterTurn("use the runtime variables");
    await waitFor(() => expect(onSendMessage).toHaveBeenCalledTimes(2));
    await pickAccount("My API key");

    // The first save comes back, the second turn is accepted, and its save
    // comes back.
    rerender(
      <ChatPanel
        {...panelProps({ kind: "account", accountId: MY_API_KEY.id })}
      />,
    );
    await act(async () => finishSecondSend(true));
    rerender(<ChatPanel {...panelProps({ kind: "runtime_config" })} />);
    sendLaterTurn("use my key again");
    await waitFor(() => expect(onSendMessage).toHaveBeenCalledTimes(3));
    expect(onSendMessage.mock.calls[2][1].account).toEqual({
      kind: "account",
      accountId: MY_API_KEY.id,
    });
  });

  it("sends no account with a later turn while the switch is off", async () => {
    const onSendMessage = vi.fn().mockResolvedValue(true);
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession({
            nextAccount: { kind: "account", accountId: TEAM_SUBSCRIPTION.id },
          }),
          onSendMessage,
          providerAccountsEnabled: false,
          providerAccounts: ACCOUNTS,
          providerAccountAssignments: ASSIGNED_TEAM,
        })}
      />,
    );
    sendLaterTurn("follow up");
    await waitFor(() => expect(onSendMessage).toHaveBeenCalledTimes(1));
    expect(onSendMessage.mock.calls[0][1].account).toBeUndefined();
  });
});

// ---------------------------------------------------------------------------
// Timeline history pagination — load-earlier affordance and straddled-turn
// completeness (chat_panel_runtime D35, issue #2333)
// ---------------------------------------------------------------------------

describe("ChatPanel timeline history pagination", () => {
  // A window-straddling turn: its EARLY tool call ("Bash") lives on an older
  // page that is not loaded initially, while its tail (final output) is in the
  // newest window, so the turn renders entry-driven with the early tool chip
  // silently absent (issue #2333 Mode 2). Loading the older page restores it.
  const STRADDLE_MESSAGES: ChatMessageDTO[] = [
    chatMessage({
      id: "user-straddle",
      role: "user",
      content: "Investigate the mermaid source leak.",
      status: "complete",
      messageIndex: 0,
      createdAt: "2026-05-10T00:00:00Z",
    }),
    chatMessage({
      id: "assistant-straddle",
      role: "assistant",
      content: "Found the leak in the mermaid config.",
      status: "complete",
      messageIndex: 1,
      createdAt: "2026-05-10T00:00:09Z",
    }),
  ];

  // Newest window (page 1): only the tail of the turn survives — a final output
  // entry that makes it a rich provided-agent turn.
  const TAIL_ENTRIES: TimelineEntry[] = [
    timelineEntry({
      id: "straddle-output-final",
      entryType: "output",
      content: "Found the leak in the mermaid config.",
      metadata: {
        source: "provided_agent",
        provider: "claude_code",
        providerEvent: "provided_agent.output.final",
        phase: "output",
        sessionId: "session-1",
        messageId: "assistant-straddle",
        blockId: "block-straddle",
      },
      sequence: 9,
      createdAt: "2026-05-10T00:00:09Z",
    }),
  ];

  // Older page (page 2): the turn's early Bash tool call + result, invisible
  // until "Load earlier history" fetches this page.
  const EARLIER_ENTRIES: TimelineEntry[] = [
    timelineEntry({
      id: "straddle-bash-started",
      entryType: "tool_call",
      content: "Claude Code used Bash",
      metadata: {
        source: "provided_agent",
        provider: "claude_code",
        providerEvent: "provided_agent.tool.started",
        phase: "tool_call",
        sessionId: "session-1",
        messageId: "assistant-straddle",
        toolName: "bash",
        // Same toolCallId as the completion below so the pair reassembles into
        // ONE completed tool operation (PR #2343 review): the per-turn block
        // builder joins started/completed on toolCallId.
        toolCallId: "tool-straddle-bash",
        args: { command: "rg -n mermaid" },
      },
      sequence: 1,
      createdAt: "2026-05-10T00:00:01Z",
    }),
    timelineEntry({
      id: "straddle-bash-completed",
      entryType: "tool_result",
      content: "match found",
      metadata: {
        source: "provided_agent",
        provider: "claude_code",
        providerEvent: "provided_agent.tool.completed",
        phase: "tool_result",
        sessionId: "session-1",
        messageId: "assistant-straddle",
        toolName: "bash",
        toolCallId: "tool-straddle-bash",
      },
      sequence: 2,
      createdAt: "2026-05-10T00:00:02Z",
    }),
  ];

  it("renders a window-straddling turn fully once its earlier page is loaded", () => {
    const { rerender } = render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          messages: STRADDLE_MESSAGES,
          timelineEntries: TAIL_ENTRIES,
          hasEarlierHistory: true,
          onLoadEarlierHistory: vi.fn(),
        })}
      />,
    );

    const transcript = screen.getByTestId("chat-messages");
    // The tail renders (persisted narration), but the amputated early tool call
    // is silently absent — and the load-earlier affordance is present as the
    // explicit incompleteness signal, so the truncated turn is not shown as
    // complete.
    expect(
      within(transcript).getByText("Found the leak in the mermaid config."),
    ).toBeInTheDocument();
    expect(within(transcript).queryByText("Bash")).not.toBeInTheDocument();
    expect(
      within(transcript).getByTestId("chat-load-earlier-history"),
    ).toBeInTheDocument();

    // Loading the older page restores the turn's full entry sequence.
    rerender(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          messages: STRADDLE_MESSAGES,
          timelineEntries: [...EARLIER_ENTRIES, ...TAIL_ENTRIES],
          hasEarlierHistory: false,
          onLoadEarlierHistory: vi.fn(),
        })}
      />,
    );

    const restored = screen.getByTestId("chat-messages");
    // Exactly ONE Bash tool block: pagination restored one complete
    // started+completed operation, not two half-blocks (PR #2343 review).
    expect(within(restored).getAllByText("Bash")).toHaveLength(1);
    expect(within(restored).getByText("done")).toBeInTheDocument();
    expect(
      within(restored).getByText("Found the leak in the mermaid config."),
    ).toBeInTheDocument();
    // The earliest page is loaded, so the affordance is gone.
    expect(
      within(restored).queryByTestId("chat-load-earlier-history"),
    ).not.toBeInTheDocument();
  });

  it("shows the load-earlier affordance while earlier history remains", () => {
    const onLoadEarlierHistory = vi.fn();
    const { rerender } = render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          messages: STRADDLE_MESSAGES,
          timelineEntries: TAIL_ENTRIES,
          hasEarlierHistory: true,
          onLoadEarlierHistory,
        })}
      />,
    );

    const button = screen.getByTestId("chat-load-earlier-history-button");
    expect(button).toBeInTheDocument();
    fireEvent.click(button);
    expect(onLoadEarlierHistory).toHaveBeenCalledTimes(1);

    // While a load is in flight the control shows a loading affordance.
    rerender(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          messages: STRADDLE_MESSAGES,
          timelineEntries: TAIL_ENTRIES,
          hasEarlierHistory: true,
          isLoadingEarlierHistory: true,
          onLoadEarlierHistory,
        })}
      />,
    );
    expect(screen.getByText("Loading earlier history…")).toBeInTheDocument();

    // With no earlier history the affordance is absent.
    rerender(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          messages: STRADDLE_MESSAGES,
          timelineEntries: TAIL_ENTRIES,
          hasEarlierHistory: false,
          onLoadEarlierHistory,
        })}
      />,
    );
    expect(
      screen.queryByTestId("chat-load-earlier-history-button"),
    ).not.toBeInTheDocument();
  });

  it("keeps the scroll anchor across a poll update until the older page prepends", () => {
    // Regression for PR #2343 review: a poll refresh or live WS entry landing
    // between the load-earlier request and the older page's arrival must not
    // consume the scroll anchor — only an actual prepend (recorded first entry
    // id moving to index > 0) restores and clears it.
    const onLoadEarlierHistory = vi.fn();
    const pollAppendedEntry = timelineEntry({
      id: "straddle-poll-appended",
      entryType: "output",
      content: "poll-appended live output",
      metadata: {
        source: "provided_agent",
        provider: "claude_code",
        providerEvent: "provided_agent.output.final",
        phase: "output",
        sessionId: "session-1",
        messageId: "assistant-straddle",
        blockId: "block-poll-appended",
      },
      sequence: 20,
      createdAt: "2026-05-10T00:00:20Z",
    });

    const { rerender } = render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          messages: STRADDLE_MESSAGES,
          timelineEntries: TAIL_ENTRIES,
          hasEarlierHistory: true,
          onLoadEarlierHistory,
        })}
      />,
    );

    // jsdom reports 0 for scroll geometry; mock a scrollable list so the
    // anchor math is observable.
    const list = screen.getByTestId("chat-messages");
    let scrollHeight = 1000;
    Object.defineProperty(list, "scrollHeight", {
      configurable: true,
      get: () => scrollHeight,
    });
    Object.defineProperty(list, "clientHeight", {
      configurable: true,
      get: () => 400,
    });

    // Scroll up so stick-to-bottom disengages and stays out of the way.
    // 500px keeps the position OUTSIDE the ~200px near-top sentinel zone
    // (chat_panel_runtime D38), so only the explicit button click loads.
    list.scrollTop = 500;
    fireEvent.scroll(list);

    // Load earlier — the continuous top-edge baseline is {1000, tail[0], …}.
    fireEvent.click(screen.getByTestId("chat-load-earlier-history-button"));
    expect(onLoadEarlierHistory).toHaveBeenCalledTimes(1);

    // Poll tick: a live entry appends at the BOTTOM (no prepend — the recorded
    // first id stays at index 0) and the list grows below the viewport.
    scrollHeight = 1100;
    rerender(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          messages: STRADDLE_MESSAGES,
          timelineEntries: [...TAIL_ENTRIES, pollAppendedEntry],
          hasEarlierHistory: true,
          isLoadingEarlierHistory: true,
          onLoadEarlierHistory,
        })}
      />,
    );
    // No top-edge growth: only the baseline refreshes; viewport untouched.
    expect(list.scrollTop).toBe(500);

    // Older page arrives: EARLIER_ENTRIES prepend ahead of the recorded id.
    scrollHeight = 1600;
    rerender(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          messages: STRADDLE_MESSAGES,
          timelineEntries: [
            ...EARLIER_ENTRIES,
            ...TAIL_ENTRIES,
            pollAppendedEntry,
          ],
          hasEarlierHistory: false,
          onLoadEarlierHistory,
        })}
      />,
    );
    // Restored by the prepended height alone (1600 - refreshed baseline 1100),
    // keeping the same rows in view: 500 + 500.
    expect(list.scrollTop).toBe(1000);
  });

  it("preserves a user scroll made after the request but before the prepend", () => {
    // Regression for PR #2343 review (comment 3900530357): if the user scrolls
    // after requesting older history and NO interim timeline update lands before
    // the prepend, the restore must add the prepended height to the LIVE scroll
    // position, not a value captured at request time. The old code used the
    // captured anchor.prevScrollTop and shifted the viewport away from the rows
    // the user had scrolled to.
    const onLoadEarlierHistory = vi.fn();

    const { rerender } = render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          messages: STRADDLE_MESSAGES,
          timelineEntries: TAIL_ENTRIES,
          hasEarlierHistory: true,
          onLoadEarlierHistory,
        })}
      />,
    );

    const list = screen.getByTestId("chat-messages");
    let scrollHeight = 1000;
    Object.defineProperty(list, "scrollHeight", {
      configurable: true,
      get: () => scrollHeight,
    });
    Object.defineProperty(list, "clientHeight", {
      configurable: true,
      get: () => 400,
    });

    // Refresh the continuous top-edge baseline with the mocked geometry (in a
    // real browser the mount commit already recorded real scrollHeight; jsdom
    // reported 0 before the mock above). Cloned arrays model a poll delivering
    // the same data: fresh identities, same content — a pure baseline refresh.
    rerender(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          messages: [...STRADDLE_MESSAGES],
          timelineEntries: [...TAIL_ENTRIES],
          hasEarlierHistory: true,
          onLoadEarlierHistory,
        })}
      />,
    );

    // Scroll up (outside the ~200px sentinel zone, chat_panel_runtime D38) and
    // request earlier history — the continuous baseline is {1000, tail[0], …}.
    list.scrollTop = 500;
    fireEvent.scroll(list);
    fireEvent.click(screen.getByTestId("chat-load-earlier-history-button"));
    expect(onLoadEarlierHistory).toHaveBeenCalledTimes(1);

    // User scrolls further while the older page is still in flight (staying
    // clearly above the near-bottom re-stick threshold: 1000 − 520 − 400 =
    // 80px from bottom). There is NO interim timeline update (no rerender
    // with new entries) — the only thing that changed is the live scroll
    // position.
    list.scrollTop = 520;
    fireEvent.scroll(list);

    // Older page arrives and prepends 500px of height (no interim update ever
    // refreshed the baseline, so delta = 1500 - 1000 = 500).
    scrollHeight = 1500;
    rerender(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          messages: STRADDLE_MESSAGES,
          timelineEntries: [...EARLIER_ENTRIES, ...TAIL_ENTRIES],
          hasEarlierHistory: false,
          onLoadEarlierHistory,
        })}
      />,
    );

    // Live position (520) + prepended height (500) = 1020, preserving the
    // user's scroll. Captured-position restore would have produced 500 + 500.
    expect(list.scrollTop).toBe(1020);
  });
});

describe("ChatPanel history coverage (g5.chat.history-window)", () => {
  // Three turns as a fresh load sees them: the message window holds every
  // prompt and reply, while the newest-first activity window reaches back only
  // into the middle turn. Its floor (00:01:20) lies inside that turn.
  const at = (time: string) => `2026-05-10T00:${time}Z`;
  const prompt = (n: number, time: string) =>
    chatMessage({
      id: `user-${n}`,
      role: "user",
      content: `Prompt ${n}`,
      status: "complete",
      messageIndex: (n - 1) * 2,
      createdAt: at(time),
    });
  const reply = (n: number, time: string, content: string) =>
    chatMessage({
      id: `assistant-${n}`,
      role: "assistant",
      content,
      status: "complete",
      messageIndex: (n - 1) * 2 + 1,
      createdAt: at(time),
    });
  const MESSAGES = [
    prompt(1, "00:00"),
    reply(1, "00:01", "Turn one narration.\n\nTurn one answer."),
    prompt(2, "01:00"),
    reply(2, "01:01", "Turn two narration.\n\nTurn two answer."),
    prompt(3, "02:00"),
    reply(3, "02:01", "Turn three narration.\n\nTurn three answer."),
  ];
  let sequence = 0;
  const text = (turn: number, block: string, content: string, time: string) =>
    timelineEntry({
      id: `text-${turn}-${block}`,
      entryType: "output",
      content,
      metadata: {
        source: "provided_agent",
        provider: "claude_code",
        providerEvent: "provided_agent.output.delta",
        streamPersistence: "semantic",
        phase: "output",
        sessionId: "session-1",
        messageId: `assistant-${turn}`,
        turnId: `assistant-${turn}`,
        blockId: `${turn}-${block}`,
      },
      sequence: ++sequence,
      createdAt: at(time),
    });
  const tool = (
    turn: number,
    call: string,
    phase: "call" | "result",
    time: string,
  ) =>
    timelineEntry({
      id: `tool-${turn}-${call}-${phase}`,
      entryType: phase === "call" ? "tool_call" : "tool_result",
      content: phase === "call" ? `Used Bash ${call}` : "ok",
      metadata: {
        source: "provided_agent",
        provider: "claude_code",
        providerEvent:
          phase === "call"
            ? "provided_agent.tool.started"
            : "provided_agent.tool.completed",
        phase: phase === "call" ? "tool_call" : "tool_result",
        sessionId: "session-1",
        messageId: `assistant-${turn}`,
        toolName: "Bash",
        toolCallId: `call-${turn}-${call}`,
        args: { command: `echo ${turn}-${call}` },
      },
      sequence: ++sequence,
      createdAt: at(time),
    });
  const turn = (n: number, base: string) => [
    text(
      n,
      "narration",
      `Turn ${["", "one", "two", "three"][n]} narration.`,
      `${base}:02`,
    ),
    tool(n, "a", "call", `${base}:10`),
    tool(n, "a", "result", `${base}:11`),
    tool(n, "b", "call", `${base}:20`),
    tool(n, "b", "result", `${base}:21`),
    text(
      n,
      "answer",
      `Turn ${["", "one", "two", "three"][n]} answer.`,
      `${base}:30`,
    ),
  ];
  const ALL = [...turn(1, "00"), ...turn(2, "01"), ...turn(3, "02")];
  // Newest window: turn 3 and the tail of turn 2 from its second tool on.
  const NEWEST = ALL.filter((entry) => entry.createdAt >= at("01:20"));
  const READY = {
    status: "ready" as const,
    floorCreatedAt: at("01:20"),
    hasOlder: true,
  };

  function turnOf(id: string) {
    return screen
      .getAllByTestId("assistant-message")
      .find((element) => element.dataset.messageId === id)!;
  }

  function panel(overrides: Partial<ChatPanelProps>) {
    return (
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          messages: MESSAGES,
          timelineEntries: NEWEST,
          hasEarlierHistory: true,
          timelineCoverage: READY,
          ...overrides,
        })}
      />
    );
  }

  it("marks each turn by how much of its activity is loaded", () => {
    const onLoadEarlierHistory = vi.fn();
    render(panel({ onLoadEarlierHistory }));

    // Newest turn: complete, every block in order, no marker.
    const third = turnOf("assistant-3");
    expect(
      within(third).queryByTestId("chat-earlier-activity"),
    ).not.toBeInTheDocument();
    const thirdText = third.textContent!;
    expect(thirdText.indexOf("Turn three narration.")).toBeLessThan(
      thirdText.indexOf("echo 3-a"),
    );
    expect(thirdText.indexOf("echo 3-b")).toBeLessThan(
      thirdText.indexOf("Turn three answer."),
    );

    // Middle turn: its loaded tail renders, the unloaded head is a marker and
    // its saved narration does not stand in for the missing tool.
    const second = turnOf("assistant-2");
    expect(
      within(second).getByTestId("chat-earlier-activity"),
    ).toBeInTheDocument();
    expect(second).not.toHaveTextContent("Turn two narration.");
    expect(second).not.toHaveTextContent("echo 2-a");
    expect(second).toHaveTextContent("echo 2-b");
    expect(second).toHaveTextContent("Turn two answer.");

    // Oldest turn: nothing of the reply until its activity loads.
    const first = turnOf("assistant-1");
    expect(
      within(first).getByTestId("chat-earlier-activity"),
    ).toBeInTheDocument();
    expect(first).not.toHaveTextContent("Turn one");
    expect(screen.getByText("Prompt 1")).toBeInTheDocument();

    fireEvent.click(within(first).getByTestId("chat-earlier-activity-button"));
    expect(onLoadEarlierHistory).toHaveBeenCalledTimes(1);
  });

  it("restores each turn's chronology once older pages load, without duplicates", () => {
    const { rerender } = render(panel({ onLoadEarlierHistory: vi.fn() }));
    // One older page reaching into turn 1, overlapping the held window.
    const page2 = ALL.filter(
      (entry) =>
        entry.createdAt >= at("00:21") && entry.createdAt <= at("01:21"),
    );
    rerender(
      panel({
        timelineEntries: [...page2, ...NEWEST],
        timelineCoverage: { ...READY, floorCreatedAt: at("00:21") },
      }),
    );
    expect(
      within(turnOf("assistant-2")).queryByTestId("chat-earlier-activity"),
    ).not.toBeInTheDocument();
    expect(
      within(turnOf("assistant-1")).getByTestId("chat-earlier-activity"),
    ).toBeInTheDocument();

    rerender(
      panel({
        timelineEntries: ALL,
        hasEarlierHistory: false,
        timelineCoverage: {
          ...READY,
          floorCreatedAt: at("00:02"),
          hasOlder: false,
        },
      }),
    );
    expect(
      screen.queryByTestId("chat-earlier-activity"),
    ).not.toBeInTheDocument();
    for (const [n, word] of [
      [1, "one"],
      [2, "two"],
      [3, "three"],
    ] as const) {
      const element = turnOf(`assistant-${n}`);
      const content = element.textContent!;
      const order = [
        `Turn ${word} narration.`,
        `echo ${n}-a`,
        `echo ${n}-b`,
        `Turn ${word} answer.`,
      ].map((needle) => content.indexOf(needle));
      expect(order.every((index) => index >= 0)).toBe(true);
      expect([...order].sort((a, b) => a - b)).toEqual(order);
      expect(content.split(`Turn ${word} narration.`)).toHaveLength(2);
      expect(content.split(`Turn ${word} answer.`)).toHaveLength(2);
      expect(
        within(element).getAllByTestId("assistant-tool-block"),
      ).toHaveLength(2);
    }
  });

  it("appends an active turn's events while older history is incomplete", () => {
    const live = chatMessage({
      id: "assistant-4",
      role: "assistant",
      content: "",
      status: "streaming",
      messageIndex: 7,
      createdAt: at("03:01"),
    });
    const messages = [...MESSAGES, prompt(4, "03:00"), live];
    const { rerender } = render(
      panel({ messages, isStreaming: true, onLoadEarlierHistory: vi.fn() }),
    );
    rerender(
      panel({
        messages,
        isStreaming: true,
        timelineEntries: [...NEWEST, tool(4, "a", "call", "03:10")],
      }),
    );
    const fourth = turnOf("assistant-4");
    expect(
      within(fourth).queryByTestId("chat-earlier-activity"),
    ).not.toBeInTheDocument();
    expect(fourth).toHaveTextContent("echo 4-a");
    expect(
      within(turnOf("assistant-2")).getByTestId("chat-earlier-activity"),
    ).toBeInTheDocument();
  });

  it("shows every reply's state while the first activity read is pending or failed", () => {
    const onLoadEarlierHistory = vi.fn();
    const { rerender } = render(
      panel({
        timelineEntries: [],
        hasEarlierHistory: false,
        timelineCoverage: {
          status: "loading",
          floorCreatedAt: null,
          hasOlder: false,
        },
        onLoadEarlierHistory,
      }),
    );
    const markers = screen.getAllByTestId("chat-earlier-activity");
    expect(markers).toHaveLength(3);
    for (const marker of markers)
      expect(marker).toHaveAttribute("data-state", "loading");
    expect(screen.queryByText(/narration/)).not.toBeInTheDocument();

    rerender(
      panel({
        timelineEntries: [],
        hasEarlierHistory: false,
        timelineCoverage: {
          status: "error",
          floorCreatedAt: null,
          hasOlder: false,
        },
        onLoadEarlierHistory,
      }),
    );
    fireEvent.click(screen.getAllByRole("button", { name: "Retry" })[0]);
    expect(onLoadEarlierHistory).toHaveBeenCalledTimes(1);
  });

  it("turns markers into a retry after an older page fails, and back while loading", () => {
    const onLoadEarlierHistory = vi.fn();
    const { rerender } = render(
      panel({
        onLoadEarlierHistory,
        earlierHistoryError: new Error("timeline 500"),
      }),
    );
    expect(
      within(turnOf("assistant-2")).getByTestId("chat-earlier-activity"),
    ).toHaveAttribute("data-state", "error");
    fireEvent.click(
      within(turnOf("assistant-1")).getByRole("button", { name: "Retry" }),
    );
    expect(onLoadEarlierHistory).toHaveBeenCalledTimes(1);

    rerender(panel({ onLoadEarlierHistory, isLoadingEarlierHistory: true }));
    expect(
      within(turnOf("assistant-2")).getByTestId("chat-earlier-activity"),
    ).toHaveAttribute("data-state", "loading");
  });

  it("renders every turn as complete without a coverage source", () => {
    render(panel({ timelineCoverage: undefined }));
    expect(
      screen.queryByTestId("chat-earlier-activity"),
    ).not.toBeInTheDocument();
  });
});

describe("ChatPanel trajectory batch identity and per-source scrollback (chat d45027de)", () => {
  // Generic (unassigned) trace fixtures: entries with a phase but NO messageId
  // and NO provided_agent source, so they render through the trajectory-batch
  // fallback path in buildUnifiedTimeline, bucketed around the messages by
  // createdAt.
  const IDENTITY_MESSAGES: ChatMessageDTO[] = [
    chatMessage({
      id: "u1",
      role: "user",
      content: "first question",
      status: "complete",
      messageIndex: 0,
      createdAt: "2026-05-11T00:00:00Z",
    }),
    chatMessage({
      id: "a1",
      role: "assistant",
      content: "first answer",
      status: "complete",
      messageIndex: 1,
      createdAt: "2026-05-11T00:01:00Z",
    }),
    chatMessage({
      id: "u2",
      role: "user",
      content: "second question",
      status: "complete",
      messageIndex: 2,
      createdAt: "2026-05-11T00:02:00Z",
    }),
    chatMessage({
      id: "a2",
      role: "assistant",
      content: "second answer",
      status: "complete",
      messageIndex: 3,
      createdAt: "2026-05-11T00:04:00Z",
    }),
  ];

  const EARLY_BATCH_ENTRY = timelineEntry({
    id: "traj-early",
    entryType: "tool_call",
    content: "used grep",
    metadata: { phase: "tool_call", toolName: "grep" },
    sequence: 1,
    createdAt: "2026-05-11T00:00:30Z",
  });

  const LATE_BATCH_ENTRY = timelineEntry({
    id: "traj-late",
    entryType: "tool_call",
    content: "used bash",
    metadata: { phase: "tool_call", toolName: "bash" },
    sequence: 2,
    createdAt: "2026-05-11T00:03:00Z",
  });

  it("keeps trajectory batch DOM identity when an earlier batch is evicted", () => {
    // RED for chat d45027de (chat_panel_runtime D38): flushTrajectory keyed
    // batches by render position (`traj-${rendered.length}`), so evicting an
    // earlier batch from the loaded window shifted every later batch's key,
    // remounting them and destroying the browser's native scroll anchoring.
    // Keys must derive from the batch's first entry id.
    const { rerender } = render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          messages: IDENTITY_MESSAGES,
          timelineEntries: [EARLY_BATCH_ENTRY, LATE_BATCH_ENTRY],
        })}
      />,
    );

    const togglesBefore = screen
      .getAllByTestId("assistant-tool-block")
      .map((tool) => tool.querySelector("summary")!);
    expect(togglesBefore).toHaveLength(2);
    const lateRowBefore = togglesBefore[togglesBefore.length - 1];

    // The earlier batch leaves the loaded window (newest-window eviction).
    rerender(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          messages: IDENTITY_MESSAGES,
          timelineEntries: [LATE_BATCH_ENTRY],
        })}
      />,
    );

    const togglesAfter = screen
      .getAllByTestId("assistant-tool-block")
      .map((tool) => tool.querySelector("summary")!);
    expect(togglesAfter).toHaveLength(1);
    // Same batch, same first entry id — the DOM node must be the SAME node,
    // not a remounted copy.
    expect(togglesAfter[0].isSameNode(lateRowBefore)).toBe(true);
  });

  function mockScrollGeometry(
    list: HTMLElement,
    getScrollHeight: () => number,
  ) {
    Object.defineProperty(list, "scrollHeight", {
      configurable: true,
      get: getScrollHeight,
    });
    Object.defineProperty(list, "clientHeight", {
      configurable: true,
      get: () => 400,
    });
  }

  it("near-top scroll fires one single-flight older-page load per source", () => {
    // chat_panel_runtime D38: reaching the top (~200px) triggers ONE load per
    // source per top visit; a source already in flight is skipped, and the
    // sentinel re-arms only after scrolling away.
    const onLoadEarlierHistory = vi.fn();
    const onLoadOlderMessages = vi.fn();
    const props = (overrides: Partial<ChatPanelProps> = {}) =>
      defaultProps({
        session: activeSession(),
        messages: IDENTITY_MESSAGES,
        timelineEntries: [LATE_BATCH_ENTRY],
        hasEarlierHistory: true,
        onLoadEarlierHistory,
        hasOlderMessages: true,
        onLoadOlderMessages,
        ...overrides,
      });
    const { rerender } = render(<ChatPanel {...props()} />);

    const list = screen.getByTestId("chat-messages");
    const scrollHeight = 1000;
    mockScrollGeometry(list, () => scrollHeight);

    // Enter the near-top zone: each source loads exactly once.
    list.scrollTop = 150;
    fireEvent.scroll(list);
    expect(onLoadEarlierHistory).toHaveBeenCalledTimes(1);
    expect(onLoadOlderMessages).toHaveBeenCalledTimes(1);

    // Still pinned near the top: the sentinel is disarmed — no refire.
    list.scrollTop = 120;
    fireEvent.scroll(list);
    expect(onLoadEarlierHistory).toHaveBeenCalledTimes(1);
    expect(onLoadOlderMessages).toHaveBeenCalledTimes(1);

    // EXACTLY ONE source in flight (PR #2435 CodeRabbit): the timeline load
    // is busy while messages are idle — a fresh top visit must fire ONLY the
    // idle messages source. This is what proves the guard is per-source; a
    // single shared in-flight flag would fail here.
    rerender(<ChatPanel {...props({ isLoadingEarlierHistory: true })} />);
    list.scrollTop = 500;
    fireEvent.scroll(list);
    list.scrollTop = 150;
    fireEvent.scroll(list);
    expect(onLoadEarlierHistory).toHaveBeenCalledTimes(1);
    expect(onLoadOlderMessages).toHaveBeenCalledTimes(2);

    // Both loads in flight: re-arm by scrolling away, return to the top —
    // the in-flight guards skip both sources.
    rerender(
      <ChatPanel
        {...props({
          isLoadingEarlierHistory: true,
          isLoadingOlderMessages: true,
        })}
      />,
    );
    list.scrollTop = 500;
    fireEvent.scroll(list);
    list.scrollTop = 150;
    fireEvent.scroll(list);
    expect(onLoadEarlierHistory).toHaveBeenCalledTimes(1);
    expect(onLoadOlderMessages).toHaveBeenCalledTimes(2);

    // Loads settled; the timeline source is exhausted but messages are not:
    // a fresh top visit fires ONLY the messages source.
    rerender(<ChatPanel {...props({ hasEarlierHistory: false })} />);
    list.scrollTop = 500;
    fireEvent.scroll(list);
    list.scrollTop = 150;
    fireEvent.scroll(list);
    expect(onLoadEarlierHistory).toHaveBeenCalledTimes(1);
    expect(onLoadOlderMessages).toHaveBeenCalledTimes(3);
  });

  it("keeps the load-earlier button usable for the idle source while the other loads", () => {
    // PR #2435 review (kaihuiz advisory #4, chat 8029eea1;
    // chat_panel_runtime D38): the shared button is disabled only when
    // NEITHER source is eligible to fire now — a busy timeline load must not
    // block an idle messages load (and vice versa).
    const onLoadEarlierHistory = vi.fn();
    const onLoadOlderMessages = vi.fn();
    const props = (overrides: Partial<ChatPanelProps> = {}) =>
      defaultProps({
        session: activeSession(),
        messages: IDENTITY_MESSAGES,
        timelineEntries: [LATE_BATCH_ENTRY],
        hasEarlierHistory: true,
        onLoadEarlierHistory,
        hasOlderMessages: true,
        onLoadOlderMessages,
        ...overrides,
      });
    const { rerender } = render(
      <ChatPanel {...props({ isLoadingEarlierHistory: true })} />,
    );

    // Timeline busy, messages idle: the button stays enabled and fires ONLY
    // the idle messages source.
    const button = screen.getByTestId("chat-load-earlier-history-button");
    expect(button).not.toBeDisabled();
    fireEvent.click(button);
    expect(onLoadEarlierHistory).toHaveBeenCalledTimes(0);
    expect(onLoadOlderMessages).toHaveBeenCalledTimes(1);

    // Both busy: disabled.
    rerender(
      <ChatPanel
        {...props({
          isLoadingEarlierHistory: true,
          isLoadingOlderMessages: true,
        })}
      />,
    );
    expect(
      screen.getByTestId("chat-load-earlier-history-button"),
    ).toBeDisabled();

    // Timeline exhausted while messages are busy: still disabled — no
    // eligible source.
    rerender(
      <ChatPanel
        {...props({ hasEarlierHistory: false, isLoadingOlderMessages: true })}
      />,
    );
    expect(
      screen.getByTestId("chat-load-earlier-history-button"),
    ).toBeDisabled();
  });

  it("preserves the viewport and keeps sticky-bottom off when the top edge grows without an explicit load", () => {
    // chat_panel_runtime D38: viewport retention is a CONTINUOUS layout
    // effect, not a per-load capture — a messages-source prepend (no timeline
    // involvement, no button click) must still hold the same rows in view,
    // and a prepend must never re-arm sticky-bottom (a later bottom append
    // must not yank the view down).
    const olderRows = [
      chatMessage({
        id: "older-1",
        role: "user",
        content: "much earlier question",
        status: "complete",
        messageIndex: 0,
        createdAt: "2026-05-11T00:00:00Z",
      }),
      chatMessage({
        id: "older-2",
        role: "assistant",
        content: "much earlier answer",
        status: "complete",
        messageIndex: 1,
        createdAt: "2026-05-11T00:00:30Z",
      }),
    ];
    const windowRows = [
      chatMessage({
        id: "window-1",
        role: "user",
        content: "recent question",
        status: "complete",
        messageIndex: 2,
        createdAt: "2026-05-11T00:01:00Z",
      }),
      chatMessage({
        id: "window-2",
        role: "assistant",
        content: "recent answer",
        status: "complete",
        messageIndex: 3,
        createdAt: "2026-05-11T00:02:00Z",
      }),
    ];
    const appendedRow = chatMessage({
      id: "appended-1",
      role: "user",
      content: "newest question",
      status: "complete",
      messageIndex: 4,
      createdAt: "2026-05-11T00:03:00Z",
    });
    const props = (messages: ChatMessageDTO[]) =>
      defaultProps({
        session: activeSession(),
        messages,
        hasOlderMessages: true,
        onLoadOlderMessages: vi.fn(),
      });
    const { rerender } = render(<ChatPanel {...props(windowRows)} />);

    const list = screen.getByTestId("chat-messages");
    let scrollHeight = 1000;
    mockScrollGeometry(list, () => scrollHeight);

    // Refresh the continuous baseline with the mocked geometry (jsdom
    // reported scrollHeight 0 at the mount commit).
    rerender(<ChatPanel {...props(windowRows)} />);

    // Scroll up to 500. Stick-to-bottom disengages because the position is
    // beyond the 24px near-bottom threshold (1000 − 500 − 400 = 100px from
    // bottom); independently, 500 is outside the 200px near-top sentinel
    // zone, so no loader auto-fires (PR #2435 CodeRabbit note).
    list.scrollTop = 500;
    fireEvent.scroll(list);

    // Older messages prepend (top edge grows by 400px).
    scrollHeight = 1400;
    rerender(<ChatPanel {...props([...olderRows, ...windowRows])} />);
    expect(list.scrollTop).toBe(900);

    // A later bottom append must NOT yank to the bottom: sticky-bottom stayed
    // off through the prepend, so only the baseline refreshes.
    scrollHeight = 1600;
    rerender(
      <ChatPanel {...props([...olderRows, ...windowRows, appendedRow])} />,
    );
    expect(list.scrollTop).toBe(900);
  });

  it("pins the viewport to the top when a near-top load prepends older content", () => {
    // chat 4484a12a: "when I scroll up and load, keep my page at the top
    // instead of a big drop" — a load triggered while within the near-top
    // zone must leave the user AT the top of the stream reading the newly
    // revealed history, not delta-shifted a page-height away from it. The
    // delta path is reserved for prepends landing after the user scrolled
    // away (previous test).
    const windowRows = [
      chatMessage({
        id: "pin-window-1",
        role: "user",
        content: "recent question",
        status: "complete",
        messageIndex: 2,
        createdAt: "2026-05-12T00:01:00Z",
      }),
      chatMessage({
        id: "pin-window-2",
        role: "assistant",
        content: "recent answer",
        status: "complete",
        messageIndex: 3,
        createdAt: "2026-05-12T00:02:00Z",
      }),
    ];
    const olderRows = [
      chatMessage({
        id: "pin-older-1",
        role: "user",
        content: "much earlier question",
        status: "complete",
        messageIndex: 0,
        createdAt: "2026-05-12T00:00:00Z",
      }),
      chatMessage({
        id: "pin-older-2",
        role: "assistant",
        content: "much earlier answer",
        status: "complete",
        messageIndex: 1,
        createdAt: "2026-05-12T00:00:30Z",
      }),
    ];
    const appendedRow = chatMessage({
      id: "pin-appended-1",
      role: "user",
      content: "newest question",
      status: "complete",
      messageIndex: 4,
      createdAt: "2026-05-12T00:03:00Z",
    });
    const onLoadOlderMessages = vi.fn();
    const props = (messages: ChatMessageDTO[]) =>
      defaultProps({
        session: activeSession(),
        messages,
        hasOlderMessages: true,
        onLoadOlderMessages,
      });
    const { rerender } = render(<ChatPanel {...props(windowRows)} />);

    const list = screen.getByTestId("chat-messages");
    let scrollHeight = 1000;
    mockScrollGeometry(list, () => scrollHeight);

    // Refresh the continuous baseline with the mocked geometry (jsdom
    // reported scrollHeight 0 at the mount commit).
    rerender(<ChatPanel {...props([...windowRows])} />);

    // Scroll INTO the near-top zone — the sentinel fires the loader once.
    list.scrollTop = 150;
    fireEvent.scroll(list);
    expect(onLoadOlderMessages).toHaveBeenCalledTimes(1);

    // The older page prepends 600px while the user is still near the top:
    // the viewport pins to the very top of the stream.
    scrollHeight = 1600;
    rerender(<ChatPanel {...props([...olderRows, ...windowRows])} />);
    expect(list.scrollTop).toBe(0);

    // Sticky-bottom stayed off: a later bottom append does not yank down.
    scrollHeight = 1800;
    rerender(
      <ChatPanel {...props([...olderRows, ...windowRows, appendedRow])} />,
    );
    expect(list.scrollTop).toBe(0);
  });

  it("a source skipped while ineligible keeps its own arm and fires once eligible without leaving the top", () => {
    // PR #2435 CodeRabbit round 2 (chat_panel_runtime D38): the near-top arm
    // is PER SOURCE. A top visit that fires only the messages source (the
    // timeline load was in flight) must not consume the timeline's arm: once
    // the timeline becomes eligible again while the user is still near the
    // top, the next near-top scroll event fires it — no 400px excursion
    // required. A single shared arm fails this test.
    const onLoadEarlierHistory = vi.fn();
    const onLoadOlderMessages = vi.fn();
    const props = (overrides: Partial<ChatPanelProps> = {}) =>
      defaultProps({
        session: activeSession(),
        messages: IDENTITY_MESSAGES,
        timelineEntries: [LATE_BATCH_ENTRY],
        hasEarlierHistory: true,
        onLoadEarlierHistory,
        hasOlderMessages: true,
        onLoadOlderMessages,
        ...overrides,
      });
    const { rerender } = render(
      <ChatPanel {...props({ isLoadingEarlierHistory: true })} />,
    );

    const list = screen.getByTestId("chat-messages");
    mockScrollGeometry(list, () => 1000);

    // Top visit with the timeline busy: only the messages source fires.
    list.scrollTop = 150;
    fireEvent.scroll(list);
    expect(onLoadEarlierHistory).toHaveBeenCalledTimes(0);
    expect(onLoadOlderMessages).toHaveBeenCalledTimes(1);

    // The timeline load settles while the user stays near the top: a small
    // scroll inside the zone fires ONLY the timeline (its arm was kept); the
    // messages arm was consumed by the visit above.
    rerender(<ChatPanel {...props()} />);
    list.scrollTop = 120;
    fireEvent.scroll(list);
    expect(onLoadEarlierHistory).toHaveBeenCalledTimes(1);
    expect(onLoadOlderMessages).toHaveBeenCalledTimes(1);

    // Both arms consumed: nothing refires while pinned near the top.
    list.scrollTop = 100;
    fireEvent.scroll(list);
    expect(onLoadEarlierHistory).toHaveBeenCalledTimes(1);
    expect(onLoadOlderMessages).toHaveBeenCalledTimes(1);
  });

  it("releases sticky-bottom when a load request lands in a transcript shorter than the viewport", () => {
    // PR #2435 CodeRabbit round 2 (chat_panel_runtime D38): when the
    // transcript is shorter than the viewport, scrollTop 0 is the top AND the
    // bottom at once, so sticky-bottom is still armed when the user clicks
    // "Load earlier". The request must release it BEFORE the prepend lands;
    // otherwise the stick-to-bottom effect (which runs before retention)
    // yanks the freshly revealed history to the bottom.
    const windowRows = [
      chatMessage({
        id: "short-window-1",
        role: "user",
        content: "only question so far",
        status: "complete",
        messageIndex: 2,
        createdAt: "2026-05-12T00:01:00Z",
      }),
      chatMessage({
        id: "short-window-2",
        role: "assistant",
        content: "only answer so far",
        status: "complete",
        messageIndex: 3,
        createdAt: "2026-05-12T00:02:00Z",
      }),
    ];
    const olderRows = [
      chatMessage({
        id: "short-older-1",
        role: "user",
        content: "earlier question",
        status: "complete",
        messageIndex: 0,
        createdAt: "2026-05-12T00:00:00Z",
      }),
      chatMessage({
        id: "short-older-2",
        role: "assistant",
        content: "earlier answer",
        status: "complete",
        messageIndex: 1,
        createdAt: "2026-05-12T00:00:30Z",
      }),
    ];
    const onLoadOlderMessages = vi.fn();
    const props = (messages: ChatMessageDTO[]) =>
      defaultProps({
        session: activeSession(),
        messages,
        hasOlderMessages: true,
        onLoadOlderMessages,
      });
    const { rerender } = render(<ChatPanel {...props(windowRows)} />);

    const list = screen.getByTestId("chat-messages");
    // 300px of content inside a 400px viewport: nothing overflows.
    let scrollHeight = 300;
    mockScrollGeometry(list, () => scrollHeight);
    rerender(<ChatPanel {...props([...windowRows])} />);
    // A real browser clamps scrollTop to 0 when nothing overflows; no scroll
    // event has fired, so sticky-bottom is still in its initial armed state.
    list.scrollTop = 0;

    fireEvent.click(screen.getByTestId("chat-load-earlier-history-button"));
    expect(onLoadOlderMessages).toHaveBeenCalledTimes(1);

    // The older page prepends 600px: the view stays pinned at the top of the
    // stream instead of being scrolled to the new bottom.
    scrollHeight = 900;
    rerender(<ChatPanel {...props([...olderRows, ...windowRows])} />);
    expect(list.scrollTop).toBe(0);
  });
});

describe("Codex sign-in failure presentation", () => {
  const diagnostic =
    "Codex sign-in could not be completed. Send any message to retry.";
  const failure = { taskId: "auth-task", attempt: 1 };
  const user = chatMessage({
    id: "auth-input",
    role: "user",
    content: "test",
    status: "error",
    messageIndex: 0,
    codexAuthFailure: failure,
  });
  const assistant = chatMessage({
    id: "auth-assistant",
    content: diagnostic,
    status: "error",
    messageIndex: 1,
    codexAuthFailure: failure,
  });
  function notice(
    code = "codex_auth_failed",
    message = diagnostic,
    attempt = 1,
  ) {
    return projectTaskTimelineEntry(
      timelineEntry({
        id: `auth-notice-${attempt}`,
        entryType: "error",
        content: message,
        metadata: {
          schema_version: "g5.task-event.v1",
          kind: "notice",
          sessionId: "session-1",
          taskId: failure.taskId,
          attempt,
          messageId: attempt === 1 ? assistant.id : `auth-retry-${attempt}`,
          payload: { level: "error", code, message, retryable: true },
        },
      }),
    );
  }
  function props(overrides: Partial<ChatPanelProps> = {}) {
    return defaultProps({
      session: activeSession({ agentProvider: "codex" }),
      messages: [user, assistant],
      timelineEntries: [notice()],
      ...overrides,
    });
  }

  it.each([true, false])(
    "shows one notice through replay and remount (owner=%s)",
    async (owner) => {
      const client = new QueryClient({
        defaultOptions: { queries: { retry: false } },
      });
      client.setQueryData(["codex-device-auth", "session-1"], {
        taskId: failure.taskId,
        attempt: 1,
        phase: "ended",
        message: diagnostic,
        deadlineUnixMs: null,
        verificationUrl: null,
        userCode: null,
      } satisfies CodexAuthView);
      const panel = (entries: TimelineEntry[]) => (
        <QueryClientProvider client={client}>
          <ChatPanel
            {...props({
              timelineEntries: entries,
              canManageSessionInputs: owner,
            })}
          />
        </QueryClientProvider>
      );
      const view = rtlRender(panel([notice()]));
      expect(screen.getAllByText(diagnostic)).toHaveLength(1);
      expect(screen.getByRole("alert")).toHaveTextContent(diagnostic);
      expect(
        screen.queryByText("Error generating response"),
      ).not.toBeInTheDocument();
      expect(screen.queryByTestId("codex-device-auth")).not.toBeInTheDocument();
      // The withheld input is still failed, not delivered or retried.
      expect(screen.getByTestId("user-message").firstElementChild).toHaveClass(
        "border",
      );
      expect(screen.getByTitle("Message not delivered")).toHaveTextContent(
        "test",
      );
      view.rerender(panel([]));
      expect(screen.getAllByText(diagnostic)).toHaveLength(1);
      view.rerender(panel([notice(), { ...notice(), id: "notice-replay" }]));
      expect(screen.getAllByText(diagnostic)).toHaveLength(1);
      view.unmount();
      rtlRender(panel([]));
      expect(screen.getAllByText(diagnostic)).toHaveLength(1);
      expect(
        screen.queryByText("Error generating response"),
      ).not.toBeInTheDocument();
    },
  );

  it("replaces a generic outcome with the saved auth explanation without the notice page", () => {
    const outcome = projectTaskTimelineEntry(
      timelineEntry({
        id: "auth-outcome",
        entryType: "checkpoint",
        content: "invocation.outcome",
        metadata: {
          schema_version: "g5.task-event.v1",
          kind: "invocation.outcome",
          sessionId: "session-1",
          taskId: failure.taskId,
          attempt: 1,
          messageId: assistant.id,
          payload: { status: "failed" },
        },
      }),
    );
    render(<ChatPanel {...props({ timelineEntries: [outcome] })} />);
    expect(screen.getAllByText(diagnostic)).toHaveLength(1);
    expect(
      screen.queryByText("Error generating response"),
    ).not.toBeInTheDocument();
  });

  it.each([
    [
      "codex_auth_timeout",
      "Device-code authorization timed out. Send any message to retry.",
    ],
    [
      "codex_auth_interrupted",
      "Device-code authorization was interrupted. Send any message to retry.",
    ],
    [
      "codex_auth_superseded",
      "Device-code authorization was canceled because a new message arrived. Retrying with the new message.",
    ],
  ])(
    "keeps the settled %s reason authoritative over late replay",
    (code, message) => {
      render(
        <ChatPanel
          {...props({
            messages: [user, { ...assistant, content: message }],
            timelineEntries: [notice(), notice(code, message)],
          })}
        />,
      );
      expect(screen.getAllByText(message)).toHaveLength(1);
      expect(screen.queryByText(diagnostic)).not.toBeInTheDocument();
    },
  );

  it("preserves real partial output, unrelated failures and identical diagnostics in later attempts", () => {
    const partial = projectTaskTimelineEntry(
      timelineEntry({
        id: "partial-output",
        entryType: "agent_message",
        content: "Real partial response",
        metadata: {
          schema_version: "g5.task-event.v1",
          kind: "message.final",
          sessionId: "session-1",
          taskId: failure.taskId,
          attempt: 1,
          messageId: assistant.id,
          payload: { content: "Real partial response", role: "assistant" },
        },
      }),
    );
    render(
      <ChatPanel
        {...props({
          messages: [
            user,
            assistant,
            chatMessage({
              id: "unrelated-input",
              role: "user",
              content: "Different failed send",
              status: "error",
              messageIndex: 2,
            }),
            {
              ...assistant,
              id: "auth-retry-2",
              messageIndex: 3,
              codexAuthFailure: { ...failure, attempt: 2 },
            },
          ],
          timelineEntries: [
            notice(),
            partial,
            notice("codex_auth_failed", diagnostic, 2),
          ],
        })}
      />,
    );
    expect(screen.getByText("Real partial response")).toBeVisible();
    expect(screen.getAllByText(diagnostic)).toHaveLength(2);
    expect(screen.getAllByText("Error generating response")).toHaveLength(1);
  });

  it("deduplicates the legacy persisted diagnostic only within its own assistant turn", () => {
    render(
      <ChatPanel
        {...props({
          messages: [{ ...assistant, codexAuthFailure: undefined }],
        })}
      />,
    );
    expect(screen.getAllByText(diagnostic)).toHaveLength(1);
  });
});

describe("native failed turn visibility", () => {
  function incidentEntries(diagnostic?: string): TimelineEntry[] {
    const kinds = [
      "invocation.started",
      "input.accepted",
      "terminal.progress",
      "terminal.progress",
      "invocation.outcome",
    ];
    const entries = kinds.map((kind, index) =>
      projectTaskTimelineEntry(
        timelineEntry({
          id: `incident-${index}`,
          entryType: "checkpoint",
          content: kind,
          sequence: index + 1,
          metadata: {
            schema_version: "g5.task-event.v1",
            kind,
            taskId: "failed-task",
            attempt: 1,
            sessionId: "session-1",
            ...(kind === "input.accepted"
              ? {}
              : { messageId: "failed-assistant" }),
            payload:
              kind === "invocation.outcome"
                ? {
                    status: "succeeded",
                    exit_code: 0,
                    result: {
                      schema_version: "g5.agent.turn.outcome/v1",
                      status: "failed",
                      code: "provider_error",
                      last_event_seq: 0,
                    },
                  }
                : {},
          },
        }),
      ),
    );
    if (diagnostic)
      entries.splice(
        4,
        0,
        projectTaskTimelineEntry(
          timelineEntry({
            id: "startup-notice",
            entryType: "checkpoint",
            content: "notice",
            sequence: 4.5,
            metadata: {
              schema_version: "g5.task-event.v1",
              kind: "notice",
              taskId: "failed-task",
              attempt: 1,
              sessionId: "session-1",
              messageId: "failed-assistant",
              payload: {
                code: "provider_start_failed",
                level: "error",
                message: diagnostic,
                retryable: true,
              },
            },
          }),
        ),
      );
    return entries;
  }

  const failed = () =>
    chatMessage({ id: "failed-assistant", status: "error", messageIndex: 1 });

  it.each(["codex", "claude_code"] as const)(
    "renders a saved %s refusal once through replay and partial timeline loads",
    (provider) => {
      const diagnostic = "This message was not sent: correct the tool policy.";
      const props = defaultProps({
        session: activeSession({ agentProvider: provider }),
        selectedAgentProvider: provider,
        availableAgentProviders: [provider],
        canManageSessionInputs: false,
        messages: [{ ...failed(), content: diagnostic, isErrorNotice: true }],
        timelineEntries: [],
      });
      const view = render(<ChatPanel {...props} />);
      for (const entries of [
        [],
        incidentEntries(),
        incidentEntries(diagnostic),
        [
          ...incidentEntries(diagnostic),
          { ...incidentEntries(diagnostic)[4], id: "replayed-error" },
        ],
        [],
      ]) {
        view.rerender(<ChatPanel {...props} timelineEntries={entries} />);
        expect(screen.getAllByText(diagnostic)).toHaveLength(1);
        const alert = screen.getByRole("alert");
        expect(alert).toHaveTextContent(diagnostic);
        expect(alert).toHaveClass("text-[var(--g5-blocked)]");
        expect(alert.querySelector("svg")).toHaveAttribute(
          "aria-hidden",
          "true",
        );
        expect(
          screen.queryByText("Error generating response"),
        ).not.toBeInTheDocument();
      }
    },
  );

  it("keeps actual journaled partial output separate from saved diagnostic content", () => {
    const diagnostic = "The provider stopped after its connection failed.";
    const output = projectTaskTimelineEntry(
      timelineEntry({
        id: "real-output",
        entryType: "agent_message",
        content: "Work completed before the failure",
        metadata: {
          schema_version: "g5.task-event.v1",
          kind: "message.final",
          messageId: "failed-assistant",
          sessionId: "session-1",
          taskId: "failed-task",
          attempt: 1,
          payload: {
            content: "Work completed before the failure",
            role: "assistant",
          },
        },
      }),
    );
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          messages: [{ ...failed(), content: diagnostic, isErrorNotice: true }],
          timelineEntries: [...incidentEntries(), output],
        })}
      />,
    );
    expect(screen.getByText("Work completed before the failure")).toBeVisible();
    expect(screen.getByRole("alert")).toHaveTextContent(diagnostic);
    expect(screen.getAllByText(diagnostic)).toHaveLength(1);
    expect(
      screen.queryByText("Error generating response"),
    ).not.toBeInTheDocument();
  });

  it("uses a live diagnostic instead of a generic outcome while retaining partial output", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          messages: [
            {
              ...failed(),
              content: "Actual partial answer",
              errorMessage: "Rate limit exceeded",
            },
          ],
          timelineEntries: incidentEntries(),
        })}
      />,
    );
    expect(screen.getByText("Actual partial answer")).toBeVisible();
    expect(screen.getByRole("alert")).toHaveTextContent("Rate limit exceeded");
    expect(
      screen.queryByText("Error generating response"),
    ).not.toBeInTheDocument();
  });

  it.each(["codex", "claude_code"] as const)(
    "keeps %s failures visible across reload, partial pages and a later retry",
    (provider) => {
      const props = defaultProps({
        session: activeSession({ agentProvider: provider }),
        selectedAgentProvider: provider,
        availableAgentProviders: [provider],
        messages: [failed()],
        timelineEntries: [],
      });
      const view = render(<ChatPanel {...props} />);
      expect(screen.getByText("Error generating response")).toBeVisible();
      view.rerender(
        <ChatPanel
          {...props}
          timelineEntries={incidentEntries().slice(0, 4)}
        />,
      );
      expect(screen.getAllByText("Error generating response")).toHaveLength(1);
      view.rerender(
        <ChatPanel {...props} timelineEntries={incidentEntries()} />,
      );
      expect(screen.getAllByText("Error generating response")).toHaveLength(1);
      const diagnostic = "The provider could not load its configuration.";
      view.rerender(
        <ChatPanel {...props} timelineEntries={incidentEntries(diagnostic)} />,
      );
      expect(screen.getByRole("alert")).toHaveTextContent(diagnostic);
      expect(
        screen.queryByText("Error generating response"),
      ).not.toBeInTheDocument();
      // Late replay from the old attempt must not mark the new attempt failed.
      view.rerender(
        <ChatPanel
          {...props}
          messages={[
            failed(),
            chatMessage({
              id: "retry-assistant",
              status: "complete",
              content: "Retry succeeded",
              messageIndex: 2,
            }),
          ]}
          timelineEntries={incidentEntries(diagnostic)}
        />,
      );
      expect(screen.getAllByRole("alert")).toHaveLength(1);
      expect(screen.getByText("Retry succeeded")).toBeVisible();
    },
  );

  it("keeps partial output and the terminal error together", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          messages: [{ ...failed(), content: "Partial response" }],
          timelineEntries: incidentEntries(),
        })}
      />,
    );
    expect(screen.getByText("Partial response")).toBeVisible();
    expect(screen.getByRole("alert")).toHaveTextContent(
      "Error generating response",
    );
  });

  it("shows an outcome received before its assistant row and deduplicates when the row arrives", () => {
    const props = defaultProps({
      session: activeSession(),
      messages: [
        chatMessage({
          id: "user",
          role: "user",
          content: "Explain",
          status: "complete",
        }),
      ],
      timelineEntries: incidentEntries(),
    });
    const view = render(<ChatPanel {...props} />);
    expect(screen.getByText("Error generating response")).toBeVisible();
    view.rerender(
      <ChatPanel {...props} messages={[...props.messages, failed()]} />,
    );
    expect(screen.getAllByText("Error generating response")).toHaveLength(1);
  });

  it("prefers a specific diagnostic before the assistant row has arrived", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          messages: [
            chatMessage({
              id: "user",
              role: "user",
              content: "Explain",
              status: "complete",
            }),
          ],
          timelineEntries: incidentEntries(
            "Codex could not load its configuration.",
          ),
        })}
      />,
    );
    expect(screen.getAllByRole("alert")).toHaveLength(1);
    expect(screen.getByRole("alert")).toHaveTextContent(
      "Codex could not load its configuration.",
    );
    expect(
      screen.queryByText("Error generating response"),
    ).not.toBeInTheDocument();
  });

  it.each(["max_output_bytes", "max_log_bytes"])(
    "shows a %s failure before its assistant row arrives",
    (limit) => {
      const props = defaultProps({
        session: activeSession(),
        messages: [],
        timelineEntries: [
          projectTaskTimelineEntry(
            timelineEntry({
              id: "limit-outcome",
              entryType: "checkpoint",
              content: "invocation.outcome",
              metadata: {
                schema_version: "g5.task-event.v1",
                kind: "invocation.outcome",
                payload: { status: "limit_exceeded", limit, value: 1024 },
                messageId: "failed-assistant",
                sessionId: "session-1",
                taskId: "failed-task",
                attempt: 1,
              },
            }),
          ),
        ],
      });
      const view = render(<ChatPanel {...props} />);
      const diagnostic =
        "The agent exceeded its output or log limit before completing its response.";
      expect(screen.getByRole("alert")).toHaveTextContent(diagnostic);
      view.rerender(<ChatPanel {...props} messages={[failed()]} />);
      expect(screen.getAllByRole("alert")).toHaveLength(1);
      expect(screen.getByRole("alert")).toHaveTextContent(diagnostic);
    },
  );

  it("does not expose orphan non-error activity outside the loaded message window", () => {
    const orphan = projectTaskTimelineEntry(
      timelineEntry({
        id: "old-thought",
        entryType: "thought",
        content: "Old hidden thought",
        metadata: {
          schema_version: "g5.task-event.v1",
          kind: "thought.final",
          payload: { content: "Old hidden thought" },
          messageId: "unloaded-assistant",
          sessionId: "session-1",
          taskId: "old-task",
          attempt: 1,
        },
      }),
    );
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          messages: [
            chatMessage({
              id: "newest",
              content: "Current response",
              status: "complete",
            }),
          ],
          timelineEntries: [orphan, ...incidentEntries()],
        })}
      />,
    );
    expect(screen.queryByText("Old hidden thought")).not.toBeInTheDocument();
    expect(screen.getByText("Current response")).toBeVisible();
    expect(screen.getByText("Error generating response")).toBeVisible();
  });
});

describe("ChatPanel Claude background waiting", () => {
  const MESSAGE_ID = "assistant-wait";
  const NOW = Date.parse("2026-09-30T12:00:00.000Z");
  const SHELL_COMMAND =
    "python3 -c 'import time; time.sleep(600); print(\"suite finished\")'";

  // The API's timeline row for one journal event: `entryType` and `content`
  // follow task_lifecycle.rs `timeline_type` / `timeline_content`.
  function waitEntry(
    id: string,
    kind: string,
    payload: Record<string, unknown>,
    sequence: number,
    messageId = MESSAGE_ID,
  ): TimelineEntry {
    const entryType =
      kind === "message.final"
        ? "agent_message"
        : kind === "message.delta"
          ? "output"
          : kind === "tool.started"
            ? "tool_call"
            : kind === "notice" && payload.level === "error"
              ? "error"
              : "checkpoint";
    const content =
      typeof payload.content === "string"
        ? payload.content
        : typeof payload.text_delta === "string"
          ? payload.text_delta
          : typeof payload.message === "string"
            ? payload.message
            : kind;
    return projectTaskTimelineEntry(
      timelineEntry({
        id,
        entryType,
        content,
        sequence,
        createdAt: new Date(NOW - 120_000 + sequence * 1_000).toISOString(),
        metadata: {
          schema_version: "g5.task-event.v1",
          kind,
          payload,
          taskId: "task-1",
          attempt: 1,
          messageId,
          sessionId: "session-1",
        },
      }),
    );
  }

  function backgroundSet(
    id: string,
    sequence: number,
    tasks: Record<string, unknown>[],
    count = tasks.length,
  ): TimelineEntry {
    return waitEntry(
      id,
      "background_tasks.replaced",
      {
        tasks_ref: `sha256:${id}`,
        tasks,
        count,
        truncated: count > tasks.length,
        content:
          count === 0
            ? "No background work remains"
            : `Waiting on ${count} background task${count === 1 ? "" : "s"}`,
      },
      sequence,
    );
  }

  const shellTask = {
    task_id: "b1nohmzbo",
    kind: "shell",
    description: SHELL_COMMAND,
    started_at_unix_ms: NOW - 65_000,
  };
  const agentTask = {
    task_id: "a8bce50d36d7e4d1c",
    kind: "agent",
    description: "Background sleep task",
    started_at_unix_ms: NOW - 30_000,
  };

  // A first turn that started a command and a sub-agent in the background,
  // ended with its result, and left Claude waiting on them.
  function waitingJournal(): TimelineEntry[] {
    return [
      waitEntry(
        "final-1",
        "message.final",
        {
          block_id: "g5:terminal-result",
          content: "Started the suite in the background; I will report back.",
        },
        1,
      ),
      waitEntry(
        "subtask-shell",
        "subtask.started",
        {
          task_id: "b1nohmzbo",
          kind: "shell",
          description: SHELL_COMMAND,
          content: `Started background command: ${SHELL_COMMAND}`,
        },
        2,
      ),
      waitEntry(
        "subtask-agent",
        "subtask.started",
        {
          task_id: "a8bce50d36d7e4d1c",
          kind: "agent",
          description: "Background sleep task",
          content: "Started background agent: Background sleep task",
        },
        3,
      ),
      backgroundSet("set-1", 4, [shellTask, agentTask], 3),
      waitEntry(
        "subtask-progress",
        "subtask.progress",
        {
          task_id: "a8bce50d36d7e4d1c",
          progress_ref: "sha256:progress",
          description: "Reading 12 files",
          content: "Reading 12 files",
        },
        5,
      ),
      waitEntry(
        "usage-1",
        "usage.updated",
        { usage: { input_tokens: 10, output_tokens: 20 } },
        6,
      ),
    ];
  }

  function waitProps(
    entries: TimelineEntry[],
    overrides: Partial<ChatPanelProps> = {},
    status: ChatMessageDTO["status"] = "streaming",
  ): ChatPanelProps {
    return defaultProps({
      session: activeSession(),
      isStreaming: status === "streaming" || status === "pending",
      messages: [
        chatMessage({
          id: "user-wait",
          role: "user",
          content: "Run the suite in the background",
          status: "complete",
          messageIndex: 0,
          createdAt: new Date(NOW - 125_000).toISOString(),
        }),
        chatMessage({
          id: MESSAGE_ID,
          content: "",
          status,
          messageIndex: 1,
          createdAt: new Date(NOW - 121_000).toISOString(),
        }),
      ],
      timelineEntries: entries,
      ...overrides,
    });
  }

  it("shows the reported set and its elapsed wait on the live reply, ticking each second", () => {
    vi.useFakeTimers({ toFake: ["Date", "setInterval", "clearInterval"] });
    vi.setSystemTime(NOW);
    render(<ChatPanel {...waitProps(waitingJournal())} />);

    const panel = screen.getByTestId("background-wait-panel");
    expect(
      within(screen.getByTestId("assistant-message")).getByTestId(
        "background-wait-panel",
      ),
    ).toBe(panel);
    expect(
      within(panel).getByText("Waiting for background work"),
    ).toBeVisible();
    // The wait runs from the earliest reported start.
    expect(
      within(panel).getByTestId("background-wait-elapsed"),
    ).toHaveTextContent("1m 05s");
    const rows = within(panel).getAllByTestId("background-wait-task");
    expect(rows).toHaveLength(2);
    expect(rows[0]).toHaveTextContent("Command");
    expect(rows[0]).toHaveTextContent("1m 05s");
    const description = within(rows[0]).getByTestId(
      "background-wait-task-description",
    );
    expect(description).toHaveTextContent(SHELL_COMMAND);
    expect(description).toHaveAttribute("title", SHELL_COMMAND);
    expect(description).toHaveClass("truncate");
    expect(rows[1]).toHaveTextContent("Agent");
    expect(rows[1]).toHaveTextContent("Background sleep task");
    expect(rows[1]).toHaveTextContent("30s");
    expect(within(panel).getByText("+1 more")).toBeVisible();
    expect(
      within(panel).getByText(
        "New messages are queued and sent when this finishes.",
      ),
    ).toBeVisible();

    act(() => {
      vi.advanceTimersByTime(1_000);
    });
    expect(
      within(panel).getByTestId("background-wait-elapsed"),
    ).toHaveTextContent("1m 06s");
    expect(
      within(rows[1]).getByTestId("background-wait-task-elapsed"),
    ).toHaveTextContent("31s");
  });

  it("keeps the panel from the session's live task once the report row has left the loaded timeline", () => {
    vi.useFakeTimers({ toFake: ["Date", "setInterval", "clearInterval"] });
    vi.setSystemTime(NOW);
    // A long wait: the only background report is older than the loaded page.
    const page = waitingJournal().filter((entry) => entry.id !== "set-1");
    const liveTask = createMockTask({
      id: "task-1",
      sessionId: "session-1",
      status: "running",
      latestAttempt: {
        id: "attempt-2",
        attemptNumber: 2,
        provider: "claude_code",
        providerSessionId: null,
        disposition: null,
        reason: null,
        retryPolicy: null,
        lifecyclePosture: "active",
        startedAt: new Date(NOW - 121_000).toISOString(),
        endedAt: null,
      },
      backgroundWait: {
        tasks: [shellTask],
        count: 1,
        truncated: false,
        wait_started_at_unix_ms: NOW - 90_000,
      },
    });
    const onEndBackgroundWait = vi.fn();
    const { rerender } = render(
      <ChatPanel
        {...waitProps(page, {
          activeTaskSummary: liveTask,
          onEndBackgroundWait,
        })}
      />,
    );

    const panel = screen.getByTestId("background-wait-panel");
    expect(within(panel).getAllByTestId("background-wait-task")).toHaveLength(
      1,
    );
    // Elapsed time counts from when the wait began, not the task's own start.
    expect(
      within(panel).getByTestId("background-wait-elapsed"),
    ).toHaveTextContent("1m 30s");
    expect(screen.getByRole("textbox")).toHaveAttribute(
      "placeholder",
      "Your message will be sent after the background work finishes.",
    );
    // End names the running attempt that carries the set.
    fireEvent.click(within(panel).getByTestId("end-background-wait-button"));
    expect(onEndBackgroundWait).toHaveBeenCalledWith("session-1", {
      taskId: "task-1",
      attempt: 2,
    });

    // Without the attempt's identity the panel stays, but End is not offered:
    // an End that names no attempt could reach a later one.
    rerender(
      <ChatPanel
        {...waitProps(page, {
          activeTaskSummary: { ...liveTask, latestAttempt: null },
          onEndBackgroundWait,
        })}
      />,
    );
    expect(screen.getByTestId("background-wait-panel")).toBeVisible();
    expect(screen.queryByTestId("end-background-wait-button")).toBeNull();

    // A task that is no longer running carries no wait.
    rerender(
      <ChatPanel
        {...waitProps(page, {
          activeTaskSummary: { ...liveTask, status: "waiting" },
        })}
      />,
    );
    expect(screen.queryByTestId("background-wait-panel")).toBeNull();
  });

  it("keeps background reports and sub-task steps out of the transcript and labels the wait", () => {
    render(<ChatPanel {...waitProps(waitingJournal())} />);

    const turn = screen.getByTestId("assistant-message");
    expect(
      within(turn).getByText(
        "Started the suite in the background; I will report back.",
      ),
    ).toBeVisible();
    expect(screen.queryByText(/Started background command/)).toBeNull();
    expect(screen.queryByText(/Started background agent/)).toBeNull();
    expect(screen.queryByText("Reading 12 files")).toBeNull();
    expect(screen.queryByText(/Waiting on 3 background tasks/)).toBeNull();
    expect(screen.queryAllByTestId("assistant-status-block")).toHaveLength(0);
    expect(screen.getByTestId("streaming-indicator")).toHaveTextContent(
      "Waiting for background work…",
    );
  });

  it("lets the owner end the wait, disabling End while the request is in flight", async () => {
    let rejectEnd: (error: Error) => void = () => {};
    let resolveEnd: () => void = () => {};
    const onEndBackgroundWait = vi
      .fn<
        (
          sessionId: string,
          wait: { taskId: string; attempt: number },
        ) => Promise<unknown>
      >()
      .mockImplementationOnce(
        () =>
          new Promise((_, reject) => {
            rejectEnd = reject;
          }),
      )
      .mockImplementationOnce(
        () =>
          new Promise<void>((resolve) => {
            resolveEnd = resolve;
          }),
      );
    render(
      <ChatPanel {...waitProps(waitingJournal(), { onEndBackgroundWait })} />,
    );

    const end = screen.getByTestId("end-background-wait-button");
    expect(end).toHaveTextContent("End wait");
    fireEvent.click(end);
    // End names the attempt that reported the set.
    expect(onEndBackgroundWait).toHaveBeenCalledWith("session-1", {
      taskId: "task-1",
      attempt: 1,
    });
    expect(end).toBeDisabled();
    expect(end).toHaveTextContent("Ending…");
    fireEvent.click(end);
    expect(onEndBackgroundWait).toHaveBeenCalledTimes(1);

    // A failed request re-enables End; the caller reports the failure.
    await act(async () => {
      rejectEnd(new Error("session has no running agent attempt"));
    });
    await waitFor(() => expect(end).toBeEnabled());
    expect(end).toHaveTextContent("End wait");

    // A delivered End leaves the control disabled until the wait ends.
    fireEvent.click(end);
    expect(onEndBackgroundWait).toHaveBeenCalledTimes(2);
    await act(async () => {
      resolveEnd();
    });
    expect(end).toBeDisabled();
    expect(end).toHaveTextContent("Ending…");
  });

  it("offers End again for a later wait after a delivered End", async () => {
    const onEndBackgroundWait = vi.fn().mockResolvedValue({ delivered: true });
    const setReport = (
      id: string,
      sequence: number,
      tasks: Record<string, unknown>[],
      waitStartedAt: number | null,
    ) =>
      waitEntry(
        id,
        "background_tasks.replaced",
        {
          tasks_ref: `sha256:${id}`,
          tasks,
          count: tasks.length,
          truncated: false,
          ...(waitStartedAt === null
            ? {}
            : { wait_started_at_unix_ms: waitStartedAt }),
          content:
            tasks.length === 0
              ? "No background work remains"
              : `Waiting on ${tasks.length} background task`,
        },
        sequence,
      );
    const first = [
      ...waitingJournal().filter((entry) => entry.id !== "set-1"),
      setReport("set-a", 4, [shellTask], NOW - 60_000),
    ];
    const { rerender } = render(
      <ChatPanel {...waitProps(first, { onEndBackgroundWait })} />,
    );
    fireEvent.click(screen.getByTestId("end-background-wait-button"));
    await act(async () => {});
    expect(screen.getByTestId("end-background-wait-button")).toHaveTextContent(
      "Ending…",
    );

    // The set drained before End took effect, so the adapter dropped it, and
    // Claude's report started a new batch. One refresh brings both reports.
    rerender(
      <ChatPanel
        {...waitProps(
          [
            ...first,
            setReport("set-empty", 7, [], null),
            setReport("set-b", 8, [agentTask], NOW - 5_000),
          ],
          { onEndBackgroundWait },
        )}
      />,
    );
    expect(screen.getByText("Background sleep task")).toBeVisible();
    const again = screen.getByTestId("end-background-wait-button");
    expect(again).toBeEnabled();
    expect(again).toHaveTextContent("End wait");
  });

  it("shows readers the same panel without End", () => {
    render(
      <ChatPanel
        {...waitProps(waitingJournal(), { onEndBackgroundWait: undefined })}
      />,
    );
    expect(screen.getByTestId("background-wait-panel")).toBeVisible();
    expect(screen.getAllByTestId("background-wait-task")).toHaveLength(2);
    expect(
      screen.queryByTestId("end-background-wait-button"),
    ).not.toBeInTheDocument();
  });

  it("hides the panel once Claude reports no remaining work and when the reply settles", () => {
    const journal = waitingJournal();
    const { rerender } = render(<ChatPanel {...waitProps(journal)} />);
    expect(screen.getByTestId("background-wait-panel")).toBeVisible();

    const drained = [
      ...journal,
      waitEntry(
        "subtask-done",
        "subtask.completed",
        {
          task_id: "b1nohmzbo",
          status: "ok",
          summary: "Background command completed (exit code 0)",
          result_ref: "sha256:result",
          content:
            "Background task finished: Background command completed (exit code 0)",
        },
        7,
      ),
      backgroundSet("set-empty", 8, []),
    ];
    rerender(<ChatPanel {...waitProps(drained)} />);
    expect(
      screen.queryByTestId("background-wait-panel"),
    ).not.toBeInTheDocument();
    expect(screen.queryByText(/Background task finished/)).toBeNull();
    expect(screen.getByTestId("streaming-indicator")).toHaveTextContent(
      "Working…",
    );

    for (const status of ["complete", "stopped", "error"] as const) {
      rerender(<ChatPanel {...waitProps(journal, {}, status)} />);
      expect(
        screen.queryByTestId("background-wait-panel"),
      ).not.toBeInTheDocument();
    }
  });

  it("renders a follow-up report turn's text in the same reply", () => {
    const journal = [
      ...waitingJournal(),
      waitEntry(
        "subtask-done",
        "subtask.completed",
        {
          task_id: "b1nohmzbo",
          status: "ok",
          summary: "Background command completed (exit code 0)",
          content:
            "Background task finished: Background command completed (exit code 0)",
        },
        7,
      ),
      backgroundSet("set-empty", 8, []),
      waitEntry(
        "final-2",
        "message.final",
        {
          block_id: "g5:terminal-result:2",
          content: "The suite finished: 42 tests passed.",
        },
        9,
      ),
      waitEntry(
        "usage-2",
        "usage.updated",
        { usage: { input_tokens: 5, output_tokens: 8 } },
        10,
      ),
    ];

    for (const status of ["streaming", "complete"] as const) {
      const { unmount } = render(
        <ChatPanel {...waitProps(journal, {}, status)} />,
      );
      const turns = screen.getAllByTestId("assistant-message");
      expect(turns).toHaveLength(1);
      const first = within(turns[0]).getByText(
        "Started the suite in the background; I will report back.",
      );
      const report = within(turns[0]).getByText(
        "The suite finished: 42 tests passed.",
      );
      expect(
        first.compareDocumentPosition(report) &
          Node.DOCUMENT_POSITION_FOLLOWING,
      ).toBeTruthy();
      expect(
        screen.queryByTestId("background-wait-panel"),
      ).not.toBeInTheDocument();
      unmount();
    }
  });

  it("shows the owner-ended notice as a neutral status, not an alert", () => {
    const journal = [
      ...waitingJournal(),
      waitEntry(
        "notice-ended",
        "notice",
        {
          code: "background_wait_ended",
          level: "info",
          message:
            "The owner ended the wait. Claude's background work was stopped.",
          retryable: false,
        },
        7,
      ),
      waitEntry(
        "notice-warning",
        "notice",
        {
          code: "provider_warning",
          level: "warning",
          message: "Claude reported a recoverable problem.",
          retryable: true,
        },
        8,
      ),
    ];
    render(<ChatPanel {...waitProps(journal)} />);

    const ended = screen.getByText(
      "The owner ended the wait. Claude's background work was stopped.",
    );
    expect(ended).toHaveAttribute("data-status-kind", "info");
    expect(ended).not.toHaveAttribute("role");
    expect(
      screen.getByText("Claude reported a recoverable problem."),
    ).toHaveAttribute("data-status-kind", "warning");
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("explains that new messages queue behind the live reply", () => {
    const firstTurn = waitingJournal().slice(0, 1);
    const { rerender } = render(<ChatPanel {...waitProps(firstTurn)} />);
    expect(screen.getByTestId("chat-input")).toHaveAttribute(
      "placeholder",
      "Messages you send now are queued until this reply finishes.",
    );

    rerender(<ChatPanel {...waitProps(waitingJournal())} />);
    expect(screen.getByTestId("chat-input")).toHaveAttribute(
      "placeholder",
      "Your message will be sent after the background work finishes.",
    );

    rerender(
      <ChatPanel
        {...waitProps([...waitingJournal(), backgroundSet("set-empty", 7, [])])}
      />,
    );
    expect(screen.getByTestId("chat-input")).toHaveAttribute(
      "placeholder",
      "Messages you send now are queued until this reply finishes.",
    );

    rerender(<ChatPanel {...waitProps(waitingJournal(), {}, "complete")} />);
    expect(screen.getByTestId("chat-input")).toHaveAttribute(
      "placeholder",
      "Ask about this codebase…",
    );
  });

  it("advertises native steering only for an eligible Claude attempt", () => {
    const task = createMockTask({
      id: "task-1",
      status: "running",
      nativeSteeringAvailable: true,
    });
    const firstTurn = waitingJournal().slice(0, 1);
    const { rerender } = render(
      <ChatPanel {...waitProps(firstTurn, { activeTaskSummary: task })} />,
    );
    expect(screen.getByTestId("chat-input")).toHaveAttribute(
      "placeholder",
      "Claude reads this after its current step.",
    );
    rerender(
      <ChatPanel
        {...waitProps(waitingJournal(), { activeTaskSummary: task })}
      />,
    );
    expect(screen.getByTestId("chat-input")).toHaveAttribute(
      "placeholder",
      "Your message will be sent after the background work finishes.",
    );
    rerender(
      <ChatPanel
        {...waitProps(firstTurn, {
          activeTaskSummary: { ...task, nativeSteeringAvailable: false },
        })}
      />,
    );
    expect(screen.getByTestId("chat-input")).toHaveAttribute(
      "placeholder",
      "Messages you send now are queued until this reply finishes.",
    );
  });

  it("shows steering as Waiting until flush removes it from the tray", async () => {
    const client = new QueryClient({
      defaultOptions: {
        queries: {
          enabled: false,
          retry: false,
          staleTime: Infinity,
          refetchOnMount: false,
        },
      },
    });
    const input = {
      id: "steered",
      sessionId: "session-1",
      inputIndex: 1,
      content: "A pending correction",
      artifacts: [],
      revision: 1,
      inputOrigin: "human",
      deliveryGeneration: 1,
      taskId: "task-1",
      attempt: 1,
      createdAt: new Date(NOW).toISOString(),
    };
    client.setQueryData(
      ["session-inputs", "session-1"],
      [{ ...input, delivery: "queued" }],
    );
    rtlRender(
      <QueryClientProvider client={client}>
        <ChatPanel {...waitProps(waitingJournal().slice(0, 1))} />
      </QueryClientProvider>,
    );
    const tray = screen.getByTestId("chat-queued-input-steered");
    expect(within(tray).getByText("Waiting")).toBeVisible();
    expect(
      within(tray).queryByText(/Redeliver|Delivery uncertain/),
    ).not.toBeInTheDocument();
    act(() => client.setQueryData(["session-inputs", "session-1"], []));
    await waitFor(() =>
      expect(
        screen.queryByTestId("chat-queued-input-steered"),
      ).not.toBeInTheDocument(),
    );
    expect(
      screen.queryByText("Steering message sent to Claude Code."),
    ).not.toBeInTheDocument();
  });

  it("follows the server's queued-input permissions over the local heuristic", () => {
    const client = new QueryClient({
      defaultOptions: {
        queries: {
          enabled: false,
          retry: false,
          staleTime: Infinity,
          refetchOnMount: false,
        },
      },
    });
    const base = {
      sessionId: "session-1",
      content: "Queued text",
      artifacts: [],
      revision: 1,
      inputOrigin: "human",
      delivery: "queued",
      deliveryGeneration: 1,
      taskId: "task-1",
      attempt: null,
      createdAt: new Date(NOW).toISOString(),
    };
    client.setQueryData(
      ["session-inputs", "session-1"],
      [
        // A redelivery-authorized possible duplicate: its revision is frozen,
        // so the API refuses edits but still accepts withdrawal.
        {
          ...base,
          id: "frozen",
          inputIndex: 1,
          editable: false,
          withdrawable: true,
        },
        // A steering reservation: the API refuses both while it is in flight.
        {
          ...base,
          id: "reserved-steer",
          inputIndex: 2,
          editable: false,
          withdrawable: false,
        },
        // An older API pod that does not send the flags yet keeps the local
        // queued-human heuristic.
        { ...base, id: "legacy", inputIndex: 3 },
      ],
    );
    rtlRender(
      <QueryClientProvider client={client}>
        <ChatPanel {...waitProps(waitingJournal().slice(0, 1))} />
      </QueryClientProvider>,
    );
    const frozen = screen.getByTestId("chat-queued-input-frozen");
    expect(
      within(frozen).queryByTestId("chat-queued-input-edit-frozen"),
    ).not.toBeInTheDocument();
    expect(
      within(frozen).getByTestId("chat-queued-input-withdraw-frozen"),
    ).toBeVisible();
    const steered = screen.getByTestId("chat-queued-input-reserved-steer");
    expect(
      within(steered).queryByTestId("chat-queued-input-edit-reserved-steer"),
    ).not.toBeInTheDocument();
    expect(
      within(steered).queryByTestId(
        "chat-queued-input-withdraw-reserved-steer",
      ),
    ).not.toBeInTheDocument();
    const legacy = screen.getByTestId("chat-queued-input-legacy");
    expect(
      within(legacy).getByTestId("chat-queued-input-edit-legacy"),
    ).toBeVisible();
    expect(
      within(legacy).getByTestId("chat-queued-input-withdraw-legacy"),
    ).toBeVisible();
  });

  it("renders the flushed input between assistant segments of one attempt, live and after reload", () => {
    const journal = [
      waitEntry(
        "before",
        "message.final",
        { block_id: "before", content: "Before the correction" },
        1,
      ),
      waitEntry(
        "tool-start",
        "tool.started",
        {
          call_id: "running-tool",
          name: "Bash",
          input: { command: "sleep 1" },
        },
        2,
      ),
      waitEntry(
        "flushed",
        "input.shipped",
        { input_id: "steered-user" },
        3,
        "steered-reply",
      ),
      waitEntry(
        "tool-done",
        "tool.completed",
        {
          call_id: "running-tool",
          name: "Bash",
          status: "success",
          output: "Tool completed after flush",
        },
        4,
      ),
      waitEntry(
        "after",
        "message.final",
        { block_id: "after", content: "After the correction" },
        5,
        "steered-reply",
      ),
    ];
    for (const status of ["streaming", "complete"] as const) {
      const messages = [
        chatMessage({
          id: MESSAGE_ID,
          content: "Before the correction",
          status: "complete",
          messageIndex: 0,
        }),
        chatMessage({
          id: "steered-user",
          role: "user",
          content: "Use the smaller change",
          status: "complete",
          messageIndex: 1,
        }),
        chatMessage({
          id: "steered-reply",
          content: "After the correction",
          status,
          messageIndex: 2,
        }),
      ];
      const { unmount } = render(
        <ChatPanel {...waitProps(journal, { messages }, status)} />,
      );
      const replies = screen.getAllByTestId("assistant-message");
      expect(replies).toHaveLength(2);
      expect(within(replies[0]).getByText("Bash")).toBeInTheDocument();
      expect(within(replies[0]).getByText("done")).toBeInTheDocument();
      expect(within(replies[1]).queryByText("Bash")).not.toBeInTheDocument();
      const before = screen.getByText("Before the correction");
      const user = screen.getByText("Use the smaller change");
      const after = screen.getByText("After the correction");
      expect(
        before.compareDocumentPosition(user) & Node.DOCUMENT_POSITION_FOLLOWING,
      ).toBeTruthy();
      expect(
        user.compareDocumentPosition(after) & Node.DOCUMENT_POSITION_FOLLOWING,
      ).toBeTruthy();
      expect(screen.queryByText("input.shipped")).not.toBeInTheDocument();
      unmount();
    }
  });

  describe("activity label", () => {
    const activeTurn = {
      id: MESSAGE_ID,
      createdAt: new Date(NOW - 121_000).toISOString(),
    };
    const running = { status: "running" as const };
    const label = (entries: TimelineEntry[]) =>
      currentActivityLabel(entries, "claude_code", running, activeTurn);

    it("reads a live background set as waiting when nothing newer shows turn work", () => {
      const journal = waitingJournal();
      // The newest row is the turn's usage: Claude ended its turn.
      expect(label(journal)).toBe("Waiting for background work…");
      // The newest row is the background report itself.
      expect(label(journal.slice(0, 4))).toBe("Waiting for background work…");
    });

    it("keeps the in-turn labels while Claude works beside its background set", () => {
      const journal = waitingJournal();
      expect(
        label([
          ...journal,
          waitEntry(
            "tool-1",
            "tool.started",
            {
              call_id: "toolu_1",
              name: "Bash",
              input: { command: "git status" },
            },
            7,
          ),
        ]),
      ).toBe("Running Bash…");
      expect(
        label([
          ...journal,
          waitEntry(
            "delta-1",
            "message.delta",
            { block_id: "b2", text_delta: "Checking" },
            7,
          ),
        ]),
      ).toBe("Generating response…");
    });

    it("does not claim a wait once the set is empty or was never reported", () => {
      expect(
        label([...waitingJournal(), backgroundSet("set-empty", 7, [])]),
      ).toBe("Working…");
      expect(label(waitingJournal().slice(0, 2))).toBe("Working…");
    });
  });
});

describe("ChatPanel prompt history (g5.chat.ui.prompt-history)", () => {
  function input() {
    return screen.getByTestId("chat-input") as HTMLTextAreaElement;
  }

  function caretAt(position: number) {
    input().setSelectionRange(position, position);
  }

  function press(key: string, init: KeyboardEventInit = {}) {
    return fireEvent.keyDown(input(), { key, ...init });
  }

  async function submit(text: string) {
    fireEvent.change(input(), { target: { value: text } });
    fireEvent.keyDown(input(), { key: "Enter" });
    await act(async () => {});
  }

  function renderPanel(overrides: Partial<ChatPanelProps> = {}) {
    const onSendMessage = vi.fn().mockResolvedValue(true);
    const view = render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          sessions: [activeSession()],
          onSendMessage,
          ...overrides,
        })}
      />,
    );
    return { onSendMessage, ...view };
  }

  afterEach(() => localStorage.clear());

  it("recalls older prompts with Up, stops at the oldest, and never sends", async () => {
    const { onSendMessage } = renderPanel();
    await submit("first prompt");
    await submit("second prompt");
    expect(onSendMessage).toHaveBeenCalledTimes(2);

    press("ArrowUp");
    expect(input().value).toBe("second prompt");
    expect(input().selectionStart).toBe("second prompt".length);
    expect(input().selectionEnd).toBe("second prompt".length);
    press("ArrowUp");
    expect(input().value).toBe("first prompt");
    expect(input().selectionStart).toBe("first prompt".length);
    expect(input().selectionEnd).toBe("first prompt".length);
    const boundary = press("ArrowUp");
    expect(boundary).toBe(false); // stay in history mode at the oldest entry
    expect(input().value).toBe("first prompt");
    expect(onSendMessage).toHaveBeenCalledTimes(2);
  });

  it("walks forward with Down and restores the empty composer after the newest", async () => {
    renderPanel();
    await submit("first prompt");
    await submit("second prompt");
    press("ArrowUp");
    press("ArrowUp");
    expect(input().value).toBe("first prompt");
    press("ArrowDown");
    expect(input().value).toBe("second prompt");
    expect(input().selectionStart).toBe("second prompt".length);
    press("ArrowDown");
    expect(input().value).toBe("");
    expect(input().selectionStart).toBe(0);
    expect(input().selectionEnd).toBe(0);
    // Past the newest entry Down is left to the empty textarea.
    expect(press("ArrowDown")).toBe(true);
    expect(input().value).toBe("");
  });

  it("lets a recalled prompt be edited and sends only on Enter", async () => {
    const { onSendMessage } = renderPanel();
    await submit("fix the tests");
    press("ArrowUp");
    fireEvent.change(input(), { target: { value: "fix the tests again" } });
    // Edited text is the draft now: Down does not discard the edit.
    expect(press("ArrowDown")).toBe(true);
    expect(input().value).toBe("fix the tests again");
    expect(onSendMessage).toHaveBeenCalledTimes(1);
    fireEvent.keyDown(input(), { key: "Enter" });
    await act(async () => {});
    expect(onSendMessage).toHaveBeenCalledTimes(2);
    expect(onSendMessage.mock.calls[1][0]).toBe("fix the tests again");
  });

  it("keeps normal multiline arrows after leaving history, even at text boundaries", async () => {
    renderPanel();
    await submit("older");
    await submit("line one\nline two\nline three");

    press("ArrowUp");
    expect(input().value).toBe("line one\nline two\nline three");
    press("ArrowLeft");
    for (const position of [0, "line one\nline".length, input().value.length]) {
      caretAt(position);
      expect(press("ArrowUp")).toBe(true);
      expect(press("ArrowDown")).toBe(true);
      expect(input().value).toBe("line one\nline two\nline three");
    }

    // Typed drafts also keep ordinary arrows at the very start.
    fireEvent.change(input(), { target: { value: "draft one\ndraft two" } });
    caretAt(0);
    expect(press("ArrowUp")).toBe(true);
    expect(input().value).toBe("draft one\ndraft two");
  });

  it("keeps the end caret while browsing multiline prompts in both directions", async () => {
    const { onSendMessage } = renderPanel();
    const older = "older first line\nolder last line";
    const newer = "newer first line\nnewer last line";
    await submit(older);
    await submit(newer);

    for (const [key, expected] of [
      ["ArrowUp", newer],
      ["ArrowUp", older],
      ["ArrowUp", older],
      ["ArrowDown", newer],
      ["ArrowDown", ""],
    ] as const) {
      expect(press(key)).toBe(false);
      expect(input().value).toBe(expected);
      expect(input().selectionStart).toBe(expected.length);
      expect(input().selectionEnd).toBe(expected.length);
    }
    expect(onSendMessage).toHaveBeenCalledTimes(2);
  });

  it.each(["ArrowLeft", "ArrowRight", "Home", "End", "Tab"])(
    "%s exits history until the composer is cleared",
    async (key) => {
      renderPanel();
      await submit("older");
      await submit("newer first line\nnewer last line");
      press("ArrowUp");
      fireEvent.keyDown(input(), { key });
      caretAt(0);
      expect(press("ArrowUp")).toBe(true);
      caretAt(input().value.length);
      expect(press("ArrowDown")).toBe(true);
      expect(input().value).toBe("newer first line\nnewer last line");

      fireEvent.change(input(), { target: { value: "" } });
      press("ArrowUp");
      press("ArrowUp");
      expect(input().value).toBe("older");
    },
  );

  it("does not re-enter history when an edit is undone back to the recalled text", async () => {
    renderPanel();
    await submit("older");
    await submit("remembered");
    press("ArrowUp");
    fireEvent.change(input(), { target: { value: "remembered edited" } });
    fireEvent.change(input(), { target: { value: "remembered" } });
    caretAt(0);
    expect(press("ArrowUp")).toBe(true);
    expect(input().value).toBe("remembered");
    fireEvent.change(input(), { target: { value: "" } });
    press("ArrowUp");
    press("ArrowUp");
    expect(input().value).toBe("older");
  });

  it("clicking the prompt leaves history until the composer is cleared", async () => {
    renderPanel();
    await submit("older");
    await submit("remembered");
    press("ArrowUp");
    fireEvent.pointerDown(input());
    caretAt(0);
    expect(press("ArrowUp")).toBe(true);
    expect(input().value).toBe("remembered");
    fireEvent.change(input(), { target: { value: "" } });
    press("ArrowUp");
    press("ArrowUp");
    expect(input().value).toBe("older");
  });

  it("ignores modified arrows, selections, IME composition and the reference picker", async () => {
    renderPanel({
      projectReferences: [
        { id: "doc-1", kind: "project_doc", label: "Spec", docId: "doc-1" },
      ],
    });
    await submit("remembered");

    for (const init of [
      { shiftKey: true },
      { altKey: true },
      { ctrlKey: true },
      { metaKey: true },
      { isComposing: true },
    ]) {
      expect(press("ArrowUp", init)).toBe(true);
      expect(input().value).toBe("");
    }

    fireEvent.change(input(), { target: { value: "selected" } });
    input().setSelectionRange(0, 3);
    expect(press("ArrowUp")).toBe(true);
    expect(input().value).toBe("selected");

    fireEvent.change(input(), { target: { value: "" } });
    fireEvent.click(screen.getByTestId("chat-add-reference-button"));
    expect(press("ArrowUp")).toBe(true);
    expect(input().value).toBe("");
  });

  const scope = { userId: "user-1", workspaceId: "ws-1" };
  const chatA = activeSession({ id: "chat-a", title: "Chat A" });
  const chatB = activeSession({ id: "chat-b", title: "Chat B" });
  const keyFor = (chatId: string, workspaceId = "ws-1") =>
    chatPromptHistoryKey("user-1", workspaceId, chatId);

  function renderChats(overrides: Partial<ChatPanelProps> = {}) {
    const onSendMessage = vi.fn().mockResolvedValue(true);
    const props = defaultProps({
      session: chatA,
      sessions: [chatA, chatB],
      onSendMessage,
      promptHistoryScope: scope,
      ...overrides,
    });
    const view = render(<ChatPanel {...props} />);
    const show = (session: ChatSessionDTO | null) =>
      view.rerender(<ChatPanel {...props} session={session} />);
    return { onSendMessage, show, props, ...view };
  }

  it("does not record a refused send, whose draft is restored instead", async () => {
    const { onSendMessage } = renderChats();
    await submit("accepted");
    onSendMessage.mockResolvedValue(false);
    await submit("refused");
    expect(input().value).toBe("refused");
    expect(readChatPromptHistory(keyFor("chat-a"))).toEqual(["accepted"]);
  });

  it("recalls only the current chat's prompts", async () => {
    const { show } = renderChats();
    await submit("A1");
    show(chatB);
    await submit("B1");
    show(chatA);
    press("ArrowUp");
    expect(input().value).toBe("A1");
    press("ArrowUp");
    expect(input().value).toBe("A1"); // A's oldest: nothing from B follows
    press("ArrowDown");
    expect(input().value).toBe("");
    show(chatB);
    press("ArrowUp");
    expect(input().value).toBe("B1");
    expect(readChatPromptHistory(keyFor("chat-a"))).toEqual(["A1"]);
    expect(readChatPromptHistory(keyFor("chat-b"))).toEqual(["B1"]);
  });

  it("ends browsing when the chat changes, without carrying the recalled prompt", async () => {
    localStorage.setItem(
      keyFor("chat-a"),
      JSON.stringify({ usedAt: 1, entries: ["A2", "A1"] }),
    );
    localStorage.setItem(
      keyFor("chat-b"),
      JSON.stringify({ usedAt: 1, entries: ["B1"] }),
    );
    const { show } = renderChats();
    press("ArrowUp");
    press("ArrowUp");
    expect(input().value).toBe("A1");
    show(chatB);
    expect(input().value).toBe("");
    press("ArrowUp");
    expect(input().value).toBe("B1");
    press("ArrowUp");
    expect(input().value).toBe("B1");

    // Edited text is an ordinary draft and stays.
    press("ArrowDown");
    show(chatA);
    press("ArrowUp");
    fireEvent.change(input(), { target: { value: "A2 edited" } });
    show(chatB);
    expect(input().value).toBe("A2 edited");
    caretAt(0);
    expect(press("ArrowUp")).toBe(true);
    expect(input().value).toBe("A2 edited");
  });

  it("starts a new chat empty and records its first send against the chat it created", async () => {
    localStorage.setItem(
      keyFor("chat-a"),
      JSON.stringify({ usedAt: 1, entries: ["from chat A"] }),
    );
    const onSendMessage = vi.fn().mockResolvedValue({ sessionId: "created-1" });
    const { show } = renderChats({ session: null, onSendMessage });
    expect(press("ArrowUp")).toBe(true);
    expect(input().value).toBe("");
    await submit("first prompt of a new chat");
    expect(readChatPromptHistory(keyFor("created-1"))).toEqual([
      "first prompt of a new chat",
    ]);
    expect(readChatPromptHistory(keyFor("chat-a"))).toEqual(["from chat A"]);
    show(activeSession({ id: "created-1", title: "Created" }));
    press("ArrowUp");
    expect(input().value).toBe("first prompt of a new chat");
  });

  it("records a send that resolves after a chat switch against its own chat", async () => {
    let accept!: (result: boolean) => void;
    const onSendMessage = vi.fn(
      () => new Promise<boolean>((resolve) => (accept = resolve)),
    );
    const { show } = renderChats({ onSendMessage });
    fireEvent.change(input(), { target: { value: "slow in A" } });
    fireEvent.keyDown(input(), { key: "Enter" });
    show(chatB);
    await act(async () => accept(true));
    expect(readChatPromptHistory(keyFor("chat-a"))).toEqual(["slow in A"]);
    expect(readChatPromptHistory(keyFor("chat-b"))).toEqual([]);
    press("ArrowUp");
    expect(input().value).toBe("");
  });

  it("browses the new workspace's history after the workspace changes", async () => {
    localStorage.setItem(
      keyFor("chat-a", "ws-a"),
      JSON.stringify({ usedAt: 1, entries: ["from workspace a"] }),
    );
    localStorage.setItem(
      keyFor("chat-a", "ws-b"),
      JSON.stringify({ usedAt: 1, entries: ["from workspace b"] }),
    );
    const { rerender, props } = renderChats({
      promptHistoryScope: { userId: "user-1", workspaceId: "ws-a" },
    });
    press("ArrowUp");
    expect(input().value).toBe("from workspace a");
    rerender(
      <ChatPanel
        {...props}
        promptHistoryScope={{ userId: "user-1", workspaceId: "ws-b" }}
      />,
    );
    expect(input().value).toBe("");
    press("ArrowUp");
    expect(input().value).toBe("from workspace b");
  });

  it("records typed text only and keeps each chat's history across a reload", async () => {
    const { unmount } = renderChats();
    await submit("  persisted prompt  ");
    expect(readChatPromptHistory(keyFor("chat-a"))).toEqual([
      "persisted prompt",
    ]);
    unmount();

    renderChats();
    press("ArrowUp");
    expect(input().value).toBe("persisted prompt");
  });

  it("keeps per-chat history for the mount without a stored scope", async () => {
    const { show } = renderChats({ promptHistoryScope: undefined });
    await submit("only A");
    show(chatB);
    expect(press("ArrowUp")).toBe(true);
    expect(input().value).toBe("");
    show(chatA);
    press("ArrowUp");
    expect(input().value).toBe("only A");
  });
});

describe("ChatPanel result acknowledgement (g5.chat.inspection.acknowledge)", () => {
  const result = { taskId: "task-1", attempt: 1, hasOutput: true };
  const unseenActivity = {
    activeTurnStartedAt: null,
    lastFinishedAt: "2026-05-10T00:05:00Z",
    latestResult: result,
    unseenResult: true,
  };
  const reply = (overrides: Partial<ChatMessageDTO> = {}) =>
    chatMessage({
      id: "reply-1",
      sessionId: "session-1",
      role: "assistant",
      status: "complete",
      taskId: "task-1",
      attempt: 1,
      ...overrides,
    });

  let focused = true;
  let visibility: DocumentVisibilityState = "visible";
  beforeEach(() => {
    focused = true;
    visibility = "visible";
    vi.spyOn(document, "hasFocus").mockImplementation(() => focused);
    vi.spyOn(document, "visibilityState", "get").mockImplementation(
      () => visibility,
    );
  });
  afterEach(() => vi.restoreAllMocks());

  function renderShown(overrides: Partial<ChatPanelProps> = {}) {
    const onAcknowledgeResult = vi.fn();
    const session = activeSession({ activity: unseenActivity });
    const props = defaultProps({
      session,
      sessions: [session],
      messages: [reply()],
      onAcknowledgeResult,
      ...overrides,
    });
    const view = render(<ChatPanel {...props} />);
    return { onAcknowledgeResult, props, ...view };
  }

  it("acknowledges the named result once it is displayed", () => {
    const { onAcknowledgeResult, rerender, props } = renderShown();
    expect(onAcknowledgeResult).toHaveBeenCalledTimes(1);
    expect(onAcknowledgeResult).toHaveBeenCalledWith("session-1", result);
    rerender(<ChatPanel {...props} messages={[reply({ content: "again" })]} />);
    expect(onAcknowledgeResult).toHaveBeenCalledTimes(1);
  });

  it("acknowledges a result again after its request failed", async () => {
    const onAcknowledgeResult = vi
      .fn()
      .mockRejectedValueOnce(new Error("offline"))
      .mockResolvedValue(undefined);
    renderShown({ onAcknowledgeResult });
    expect(onAcknowledgeResult).toHaveBeenCalledTimes(1);
    // Let the refusal settle, then leave and return to the page.
    await act(async () => {});
    focused = false;
    act(() => {
      window.dispatchEvent(new Event("blur"));
    });
    focused = true;
    act(() => {
      window.dispatchEvent(new Event("focus"));
    });
    expect(onAcknowledgeResult).toHaveBeenCalledTimes(2);
    expect(onAcknowledgeResult).toHaveBeenLastCalledWith("session-1", result);
  });

  it("waits while the chat surface is hidden", () => {
    const { onAcknowledgeResult, rerender, props } = renderShown({
      surfaceVisible: false,
    });
    expect(onAcknowledgeResult).not.toHaveBeenCalled();
    rerender(<ChatPanel {...props} surfaceVisible />);
    expect(onAcknowledgeResult).toHaveBeenCalledTimes(1);
  });

  it("waits while the page is unfocused or hidden", () => {
    focused = false;
    const { onAcknowledgeResult } = renderShown();
    expect(onAcknowledgeResult).not.toHaveBeenCalled();
    visibility = "hidden";
    focused = true;
    act(() => {
      document.dispatchEvent(new Event("visibilitychange"));
    });
    expect(onAcknowledgeResult).not.toHaveBeenCalled();
    visibility = "visible";
    act(() => {
      window.dispatchEvent(new Event("focus"));
    });
    expect(onAcknowledgeResult).toHaveBeenCalledTimes(1);
  });

  it("waits until the transcript shows the result's finished output", () => {
    const { onAcknowledgeResult, rerender, props } = renderShown({
      messages: [reply({ taskId: "task-0", id: "older" })],
    });
    expect(onAcknowledgeResult).not.toHaveBeenCalled();
    rerender(
      <ChatPanel {...props} messages={[reply({ status: "streaming" })]} />,
    );
    expect(onAcknowledgeResult).not.toHaveBeenCalled();
    rerender(<ChatPanel {...props} messages={[reply()]} />);
    expect(onAcknowledgeResult).toHaveBeenCalledTimes(1);
  });

  it("acknowledges a result without output once the chat's transcript loaded", () => {
    const noOutput = { taskId: "task-2", attempt: null, hasOutput: false };
    const session = activeSession({
      activity: { ...unseenActivity, latestResult: noOutput },
    });
    const { onAcknowledgeResult, rerender, props } = renderShown({
      session,
      sessions: [session],
      messages: [],
    });
    expect(onAcknowledgeResult).not.toHaveBeenCalled();
    rerender(
      <ChatPanel
        {...props}
        messages={[chatMessage({ role: "user", sessionId: "session-1" })]}
      />,
    );
    expect(onAcknowledgeResult).toHaveBeenCalledWith("session-1", noOutput);
  });

  it("waits while the transcript is scrolled away from its end", () => {
    const { onAcknowledgeResult, rerender, props } = renderShown({
      surfaceVisible: false,
    });
    const list = screen.getByTestId("chat-messages");
    Object.defineProperty(list, "scrollHeight", {
      configurable: true,
      value: 1000,
    });
    Object.defineProperty(list, "clientHeight", {
      configurable: true,
      value: 100,
    });
    list.scrollTop = 200;
    fireEvent.scroll(list);
    rerender(<ChatPanel {...props} surfaceVisible />);
    expect(onAcknowledgeResult).not.toHaveBeenCalled();
    list.scrollTop = 900;
    fireEvent.scroll(list);
    expect(onAcknowledgeResult).toHaveBeenCalledTimes(1);
  });

  it("does not let an acknowledged result swallow a newer one", () => {
    const { onAcknowledgeResult, rerender, props } = renderShown();
    expect(onAcknowledgeResult).toHaveBeenCalledTimes(1);
    const newer = { taskId: "task-2", attempt: 1, hasOutput: true };
    const session = activeSession({
      activity: { ...unseenActivity, latestResult: newer },
    });
    rerender(<ChatPanel {...props} session={session} sessions={[session]} />);
    expect(onAcknowledgeResult).toHaveBeenCalledTimes(1);
    rerender(
      <ChatPanel
        {...props}
        session={session}
        sessions={[session]}
        messages={[reply(), reply({ id: "reply-2", taskId: "task-2" })]}
      />,
    );
    expect(onAcknowledgeResult).toHaveBeenCalledTimes(2);
    expect(onAcknowledgeResult).toHaveBeenLastCalledWith("session-1", newer);
  });

  it("acknowledges nothing for an inspected chat", () => {
    const session = activeSession({
      activity: { ...unseenActivity, unseenResult: false },
    });
    const { onAcknowledgeResult } = renderShown({
      session,
      sessions: [session],
    });
    expect(onAcknowledgeResult).not.toHaveBeenCalled();
  });
});

it("keeps a tool's disclosure and DOM when its saved assistant row arrives after activity", () => {
  const entry = timelineEntry({
    id: "activity-before-message",
    entryType: "tool_call",
    content: "Used Bash",
    metadata: {
      provider: "codex",
      providerEvent: "provided_agent.tool.started",
      phase: "tool_call",
      sessionId: "session-1",
      messageId: "later-saved-assistant",
      toolCallId: "hydrating-call",
      toolName: "Bash",
      args: { command: "rg renderer src" },
    },
  });
  const props = defaultProps({
    session: activeSession({ agentProvider: "codex" }),
    isStreaming: true,
    messages: [],
    timelineEntries: [entry],
  });
  const view = render(<ChatPanel {...props} />);
  const tool = screen.getByTestId("assistant-tool-block");
  fireEvent.click(tool.querySelector("summary")!);
  expect(tool).not.toHaveAttribute("open");
  view.rerender(
    <ChatPanel
      {...props}
      messages={[
        chatMessage({
          id: "later-saved-assistant",
          content: "Saved narration",
          status: "streaming",
        }),
      ]}
    />,
  );
  expect(screen.getByTestId("assistant-tool-block")).toBe(tool);
  expect(tool).not.toHaveAttribute("open");
});

it("keeps an accepted prompt before synthetic output with mixed RFC3339 precision", () => {
  render(
    <ChatPanel
      {...defaultProps({
        session: activeSession({ agentProvider: "codex" }),
        isStreaming: true,
        messages: [
          chatMessage({
            id: "whole-second-user",
            role: "user",
            content: "Ordered prompt",
            status: "complete",
            messageIndex: 0,
            createdAt: "2026-05-10T00:00:01Z",
          }),
        ],
        timelineEntries: [
          timelineEntry({
            id: "fractional-output",
            content: "Ordered reply",
            createdAt: "2026-05-10T00:00:01.100+00:00",
            metadata: {
              provider: "codex",
              providerEvent: "provided_agent.output.delta",
              phase: "output",
              streamPersistence: "durable",
              sessionId: "session-1",
              messageId: "fractional-assistant",
              blockId: "fractional-block",
            },
          }),
        ],
      })}
    />,
  );
  const transcript = screen.getByTestId("chat-messages").textContent!;
  expect(transcript.indexOf("Ordered prompt")).toBeLessThan(
    transcript.indexOf("Ordered reply"),
  );
  expect(screen.queryByTestId("queued-message-icon")).not.toBeInTheDocument();
});

describe("ChatPanel live rows and Stop state (#966)", () => {
  function liveDelta(seq: number, text: string, live = true): TimelineEntry {
    return timelineEntry({
      id: `task-live:1:${seq}`,
      content: text,
      sequence: seq,
      createdAt: `2026-05-10T00:00:${String(seq + 1).padStart(2, "0")}Z`,
      metadata: {
        schema_version: "g5.task-event.v1",
        kind: "message.delta",
        payload: { block_id: "b1", text_delta: text, message_id: "message-1" },
        taskId: "task-live",
        attempt: 1,
        seq,
        sessionId: "session-1",
        messageId: "message-1",
        source: "provided_agent",
        provider: "provided_agent",
        providerEvent: "provided_agent.output.delta",
        phase: "output",
        blockId: "b1",
        text,
        streamPersistence: "durable",
        ...(live ? { live: true } : {}),
      },
    });
  }

  it("renders live rows through the same projection as saved rows", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          isStreaming: true,
          messages: [
            chatMessage({ id: "message-1", content: "", status: "streaming" }),
          ],
          timelineEntries: [
            liveDelta(1, "Saved start. ", false),
            liveDelta(2, "Live middle. "),
            liveDelta(3, "Live end."),
          ],
        })}
      />,
    );
    expect(
      screen.getByText(/Saved start\. Live middle\. Live end\./),
    ).toBeInTheDocument();
  });

  it("shows Stopping… and disables Stop while the request is in flight", () => {
    const onStopStreaming = vi.fn();
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          isStreaming: true,
          messages: [
            chatMessage({ id: "message-1", content: "", status: "streaming" }),
          ],
          timelineEntries: [liveDelta(1, "working")],
          onStopStreaming,
          stopState: {
            sessionId: "session-1",
            messageId: "message-1",
            phase: "requesting",
            error: null,
          },
        })}
      />,
    );
    const stop = screen.getByTestId("stop-streaming-button");
    expect(stop).toBeDisabled();
    expect(stop).toHaveAttribute("data-stop-phase", "requesting");
    expect(stop).toHaveAttribute("title", "Stopping…");
    expect(screen.getByTestId("streaming-indicator")).toHaveTextContent(
      "Stopping…",
    );
    fireEvent.click(stop);
    expect(onStopStreaming).not.toHaveBeenCalled();
  });

  it("shows Finishing… when the execution had already finished", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          isStreaming: true,
          messages: [
            chatMessage({ id: "message-1", content: "", status: "streaming" }),
          ],
          timelineEntries: [liveDelta(1, "working")],
          stopState: {
            sessionId: "session-1",
            messageId: "message-1",
            phase: "finishing",
            error: null,
          },
        })}
      />,
    );
    expect(screen.getByTestId("streaming-indicator")).toHaveTextContent(
      "Finishing…",
    );
    expect(screen.getByTestId("stop-streaming-button")).not.toBeDisabled();
  });

  it("a failed Stop shows an inline error whose Retry re-issues the stop", () => {
    const onStopStreaming = vi.fn();
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          isStreaming: true,
          messages: [
            chatMessage({ id: "message-1", content: "", status: "streaming" }),
          ],
          timelineEntries: [liveDelta(1, "working")],
          onStopStreaming,
          stopState: {
            sessionId: "session-1",
            messageId: "message-1",
            phase: "failed",
            error: "Injected Stop failure",
          },
        })}
      />,
    );
    expect(screen.getByTestId("chat-stop-error")).toHaveTextContent(
      "Stop failed: Injected Stop failure",
    );
    fireEvent.click(screen.getByTestId("chat-stop-retry"));
    expect(onStopStreaming).toHaveBeenCalledTimes(1);
  });

  it("ignores a Stop state that belongs to another session", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          isStreaming: true,
          messages: [
            chatMessage({ id: "message-1", content: "", status: "streaming" }),
          ],
          timelineEntries: [liveDelta(1, "working")],
          stopState: {
            sessionId: "session-other",
            messageId: "message-9",
            phase: "requesting",
            error: null,
          },
        })}
      />,
    );
    expect(screen.getByTestId("stop-streaming-button")).not.toBeDisabled();
    expect(screen.queryByTestId("chat-stop-error")).not.toBeInTheDocument();
  });

  it("shows Catching up… while the live tail is still being read", () => {
    render(
      <ChatPanel
        {...defaultProps({
          session: activeSession(),
          isStreaming: true,
          messages: [
            chatMessage({ id: "message-1", content: "", status: "streaming" }),
          ],
          timelineEntries: [liveDelta(1, "working")],
          liveTailState: "catching_up",
        })}
      />,
    );
    expect(screen.getByTestId("streaming-indicator")).toHaveTextContent(
      "Catching up…",
    );
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });
});

/**
 * A provider request row reaches the browser live, ahead of the history
 * transaction that validates and stores it (chat g5.chat.live-delivery,
 * #966). It is shown at once but not actionable: a reply claim before the
 * durable row exists fails. Its stored row (same id, no live mark, a request
 * status) opens the controls; a request the validation rejects arrives as an
 * error row with the same id.
 */
describe("ChatPanel provisional provider requests (#966)", () => {
  function providerRequest(options: {
    kind: "question" | "permission";
    stored?: boolean;
    error?: string;
  }): TimelineEntry {
    const stored = options.stored === true;
    const payload =
      options.kind === "question"
        ? {
            provider_request_id: "req-5",
            request_kind: "question",
            tool_name: "AskUserQuestion",
            resume_session_id: "native-session",
            deadline_unix_ms: null,
            display: {
              questions: [
                {
                  question: "Choose a color",
                  options: [{ label: "Red" }, { label: "Blue" }],
                },
              ],
            },
          }
        : {
            provider_request_id: "req-5",
            request_kind: "permission",
            tool_name: "Bash",
            deadline_unix_ms: null,
            display: { input: { command: "npm test" } },
          };
    return projectTaskTimelineEntry(
      timelineEntry({
        id: "task-live:1:5",
        entryType: "checkpoint",
        content: "provider_request.opened",
        sequence: 5,
        createdAt: "2026-05-10T00:00:06Z",
        metadata: {
          schema_version: "g5.task-event.v1",
          kind: "provider_request.opened",
          taskId: "task-live",
          attempt: 1,
          seq: 5,
          sessionId: "session-1",
          messageId: "message-1",
          ...(options.error
            ? { projection_error: options.error }
            : stored
              ? { nativeRequestStatus: "pending" }
              : { live: true }),
          payload,
        },
      }),
    );
  }

  function renderRequest(
    entry: TimelineEntry,
    onResolveAgentApproval = vi.fn(),
  ) {
    const props = defaultProps({
      session: activeSession(),
      isStreaming: true,
      onResolveAgentApproval,
      messages: [
        chatMessage({ id: "message-1", content: "", status: "streaming" }),
      ],
      timelineEntries: [entry],
    });
    const view = render(<ChatPanel {...props} />);
    return {
      onResolveAgentApproval,
      rerenderWith: (next: TimelineEntry) =>
        view.rerender(<ChatPanel {...props} timelineEntries={[next]} />),
    };
  }

  it("does not enable a provider question before its history transaction commits", async () => {
    const onResolveAgentApproval = vi
      .fn()
      .mockResolvedValue({ deliveryDisposition: "delivered" });
    const { rerenderWith } = renderRequest(
      providerRequest({ kind: "question" }),
      onResolveAgentApproval,
    );
    // Shown at once, with its state named, and nothing actionable.
    expect(screen.getByTestId("agent-question-form")).toBeVisible();
    expect(screen.getByTestId("agent-request-confirming")).toHaveTextContent(
      "Confirming…",
    );
    expect(screen.queryByText("Waiting for your answer")).toBeNull();
    for (const option of screen.getAllByTestId("agent-question-option"))
      expect(option).toBeDisabled();
    expect(screen.getByTestId("agent-question-submit")).toBeDisabled();
    fireEvent.click(screen.getAllByTestId("agent-question-option")[1]);
    fireEvent.click(screen.getByTestId("agent-question-submit"));
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(onResolveAgentApproval).not.toHaveBeenCalled();
    // The transaction has not committed yet: the same live row again (a
    // redelivery, a tail read) changes nothing.
    rerenderWith(providerRequest({ kind: "question" }));
    expect(screen.getByTestId("agent-question-submit")).toBeDisabled();

    // The stored row confirms the request; the form opens and answers reach
    // the request by its durable identity.
    rerenderWith(providerRequest({ kind: "question", stored: true }));
    expect(screen.queryByTestId("agent-request-confirming")).toBeNull();
    expect(
      screen.getAllByText("Waiting for your answer").length,
    ).toBeGreaterThan(0);
    for (const option of screen.getAllByTestId("agent-question-option"))
      expect(option).toBeEnabled();
    fireEvent.click(
      screen
        .getAllByTestId("agent-question-option")
        .find((option) => option.textContent?.includes("Blue"))!,
    );
    expect(screen.getByTestId("agent-question-submit")).toBeEnabled();
    fireEvent.click(screen.getByTestId("agent-question-submit"));
    await waitFor(() =>
      expect(onResolveAgentApproval).toHaveBeenCalledWith(
        expect.objectContaining({
          taskId: "task-live",
          attempt: 1,
          providerRequestId: "req-5",
          validatedResponse: {
            g5AnswersByIndex: { "0": { selectedOptionIndexes: [1] } },
          },
        }),
      ),
    );
    expect(onResolveAgentApproval).toHaveBeenCalledTimes(1);
  });

  it("a question its history transaction rejects shows the error instead of a card", () => {
    const { rerenderWith, onResolveAgentApproval } = renderRequest(
      providerRequest({ kind: "question" }),
    );
    expect(screen.getByTestId("agent-question-form")).toBeVisible();
    rerenderWith(
      providerRequest({
        kind: "question",
        error: "provider request rejected: unknown tool call",
      }),
    );
    expect(screen.queryByTestId("agent-question-form")).toBeNull();
    expect(screen.queryByTestId("agent-request-confirming")).toBeNull();
    expect(screen.getByTestId("assistant-status-block")).toHaveTextContent(
      "provider request rejected: unknown tool call",
    );
    expect(onResolveAgentApproval).not.toHaveBeenCalled();
  });

  it("does not enable a permission approval before its history transaction commits", async () => {
    const onResolveAgentApproval = vi
      .fn()
      .mockResolvedValue({ deliveryDisposition: "delivered" });
    const { rerenderWith } = renderRequest(
      providerRequest({ kind: "permission" }),
      onResolveAgentApproval,
    );
    expect(screen.getByTestId("agent-request-confirming")).toHaveTextContent(
      "Confirming…",
    );
    expect(screen.getByTestId("agent-approval-approve")).toBeDisabled();
    expect(screen.getByTestId("agent-approval-reject")).toBeDisabled();
    fireEvent.click(screen.getByTestId("agent-approval-approve"));
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(onResolveAgentApproval).not.toHaveBeenCalled();

    rerenderWith(providerRequest({ kind: "permission", stored: true }));
    expect(screen.queryByTestId("agent-request-confirming")).toBeNull();
    expect(screen.getByTestId("agent-approval-approve")).toBeEnabled();
    expect(screen.getByTestId("agent-approval-reject")).toBeEnabled();
    fireEvent.click(screen.getByTestId("agent-approval-approve"));
    await waitFor(() =>
      expect(onResolveAgentApproval).toHaveBeenCalledWith(
        expect.objectContaining({
          decision: "approved",
          taskId: "task-live",
          attempt: 1,
          providerRequestId: "req-5",
          requestKind: "permission",
        }),
      ),
    );
    expect(onResolveAgentApproval).toHaveBeenCalledTimes(1);
  });
});

describe("ontology changes cards in the transcript", () => {
  const card = (body: string) => ({
    schema: "g5.ontology-card.v1",
    files: [
      {
        repoId: "repo-1",
        rootPath: "code",
        path: ".g5/ontology/features/chat.g5.md",
        changes: [
          {
            id: "x.a",
            change: "added",
            after: {
              source: `<a id="x.a"></a>\n[I · x.a]\n\n${body}\n\n`,
              metadata: "\n\n- agent\n\n",
            },
          },
        ],
      },
    ],
    governing: [],
    edits: [],
  });
  const reply = (id: string, index: number, body: string) =>
    chatMessage({
      id,
      messageIndex: index,
      status: "complete",
      content: `Reply ${index}.`,
      taskId: `task-${id}`,
      attempt: 1,
      artifacts: {
        ontology_card: card(body),
      } as unknown as ChatMessageDTO["artifacts"],
    });

  function renderTranscript(
    overrides: Partial<ChatPanelProps> = {},
    second = "Second turn's statement.",
  ) {
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    return rtlRender(
      <QueryClientProvider client={client}>
        <WorkspaceLiveGateProvider
          value={{
            kind: "live",
            canWrite: true,
            live: true,
            canStart: false,
            reviewCopy: false,
            reviewIdentityKnown: true,
            owner: true,
            workspaceNoun: "the workspace",
          }}
        >
          <ChatPanel
            {...defaultProps({
              session: activeSession(),
              messages: [
                chatMessage({
                  id: "user-1",
                  role: "user",
                  content: "First",
                  status: "complete",
                  messageIndex: 0,
                }),
                reply("reply-1", 1, "First turn's statement."),
                chatMessage({
                  id: "user-2",
                  role: "user",
                  content: "Second",
                  status: "complete",
                  messageIndex: 2,
                }),
                reply("reply-2", 3, second),
              ],
              timelineEntries: [
                projectTaskTimelineEntry(
                  timelineEntry({
                    id: "card-event",
                    entryType: "checkpoint",
                    content: "ontology.card",
                    metadata: {
                      schema_version: "g5.task-event.v1",
                      kind: "ontology.card",
                      taskId: "task-reply-2",
                      attempt: 1,
                      sessionId: "session-1",
                      messageId: "reply-2",
                      payload: card(second),
                    },
                  }),
                ),
              ],
              ...overrides,
            })}
          />
        </WorkspaceLiveGateProvider>
      </QueryClientProvider>,
    );
  }

  it("ends each reply with its card, the newest expanded and editable", () => {
    const onOpenOntologyChanges = vi.fn();
    renderTranscript({ onOpenOntologyChanges });
    const cards = screen.getAllByTestId("ontology-changes-card");
    expect(cards).toHaveLength(2);
    expect(cards[0]).not.toHaveAttribute("open");
    expect(cards[0]).toHaveAttribute("data-newest", "false");
    expect(cards[1]).toHaveAttribute("open");
    expect(within(cards[0]).queryByTestId("ontology-card-edit")).toBeNull();
    expect(within(cards[1]).getByTestId("ontology-card-edit")).toBeEnabled();
    // The card event itself adds nothing to the transcript.
    expect(screen.queryByText("ontology.card")).toBeNull();
    expect(screen.getAllByText("Second turn's statement.")).toHaveLength(1);
    fireEvent.click(within(cards[1]).getByTestId("ontology-card-view-all"));
    expect(onOpenOntologyChanges).toHaveBeenCalledTimes(1);
  });

  it("opens a card statement's links at the statement they name", () => {
    const onOpenFile = vi.fn();
    const onOpenOntologyStatement = vi.fn();
    renderTranscript(
      { onOpenFile, onOpenOntologyStatement },
      "Follows [the rule](documents.g5.md#g5.rule); see [notes](../../docs/notes.md#L3).",
    );
    const newest = screen.getAllByTestId("ontology-changes-card")[1];
    fireEvent.click(within(newest).getByRole("link", { name: "the rule" }));
    expect(onOpenOntologyStatement).toHaveBeenCalledWith(
      ".g5/ontology/features/documents.g5.md",
      "repo-1",
      "g5.rule",
    );
    expect(onOpenFile).not.toHaveBeenCalled();
    // A line link still opens the file at its line.
    fireEvent.click(within(newest).getByRole("link", { name: "notes" }));
    expect(onOpenFile).toHaveBeenCalledWith(
      ".g5/docs/notes.md",
      [3, 3],
      "repo-1",
    );
  });

  it("disables Edit while a turn of the chat runs", () => {
    renderTranscript({
      activeTaskSummary: {
        ...createMockTask(),
        status: "waiting",
      } as unknown as ChatPanelProps["activeTaskSummary"],
    });
    const cards = screen.getAllByTestId("ontology-changes-card");
    expect(within(cards[1]).getByTestId("ontology-card-edit")).toHaveAttribute(
      "title",
      "Wait for the reply in progress to finish before editing the card.",
    );
  });
});
