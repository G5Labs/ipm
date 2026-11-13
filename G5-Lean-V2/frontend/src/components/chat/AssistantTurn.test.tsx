import React from "react";
import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, onTestFinished, vi } from "vitest";
import {
  AssistantTurn,
  assistantTurnPropsEqual,
  buildAssistantTurnBlocks,
} from "./AssistantTurn";
import { ChatMessage } from "./ChatMessage";
import {
  chatProviders,
  chatFormats,
  chatHistories,
  chatStates,
  chatRenderingFixture,
  fixtureNarration,
  fixtureDiagnostic,
} from "../../test/chatRenderingFixtures";
import type { TimelineEntry } from "../../lib/types";
import { createMockChatMessage } from "../../test/fixtures";
import { readChatViewState, writeChatViewState } from "../../lib/chatViewState";

afterEach(() => vi.restoreAllMocks());

const matrix = chatProviders.flatMap((provider) =>
  chatFormats.flatMap((format) =>
    chatHistories.flatMap((history) =>
      chatStates.map((state) => ({ provider, format, history, state })),
    ),
  ),
);

describe("shared assistant projection and presentation", () => {
  it.each(matrix)("$provider $format $history $state", (options) => {
    const fixture = chatRenderingFixture(options);
    const citation = vi.fn();
    const message = fixture.messages[1];
    render(
      <AssistantTurn
        message={message}
        entries={fixture.entries}
        sessionId={message.sessionId}
        isStreaming={options.state === "streaming"}
        onCitationClick={citation}
      />,
    );
    const assistant = screen.getByTestId("assistant-message");
    const content = screen.getByTestId("assistant-turn-content");
    expect(assistant).toHaveClass("items-start");
    expect(content).toHaveClass(
      "g5-conversation-prose",
      "text-[14px]",
      "max-w-[min(100%,8in)]",
      "rounded-xl",
      "bg-[var(--g5-bg2)]",
      "px-3",
      "py-2",
    );
    expect(assistant).toHaveTextContent(fixtureNarration);
    expect(assistant).toHaveTextContent("The shared renderer is ready.");
    expect(screen.getAllByText("Inline example")).toHaveLength(1);
    expect(screen.getAllByText("Saved example")).toHaveLength(1);
    expect(screen.getAllByTestId("assistant-tool-block")).toHaveLength(1);
    if (options.history === "rich") {
      fireEvent.click(screen.getByTestId("chat-thinking-toggle"));
      expect(screen.getByText("Compare both rendering paths.")).toBeVisible();
      const text = content.textContent!;
      expect(text.indexOf(fixtureNarration)).toBeLessThan(
        text.indexOf("rg renderer src"),
      );
      expect(text.indexOf("rg renderer src")).toBeLessThan(
        text.indexOf("The shared renderer"),
      );
    }
    fireEvent.click(screen.getByRole("button", { name: "Source" }));
    expect(citation).toHaveBeenCalledWith(message.citations![0]);
    expect(screen.queryByLabelText("Generating response") !== null).toBe(
      options.state === "streaming",
    );
    if (options.state === "stopped")
      expect(assistant).toHaveTextContent("Stopped");
    if (options.state === "error")
      expect(screen.getByRole("alert")).toHaveTextContent(fixtureDiagnostic);
  });

  it.each(chatHistories)(
    "equivalent providers and formats produce identical %s blocks",
    (history) => {
      const projections = chatProviders.flatMap((provider) =>
        chatFormats.map((format) => {
          const fixture = chatRenderingFixture({
            provider,
            format,
            history,
            state: "complete",
          });
          return buildAssistantTurnBlocks(
            fixture.messages[1],
            fixture.entries,
          ).map(({ id: _id, ...block }) => block);
        }),
      );
      projections.forEach((projection) =>
        expect(projection).toEqual(projections[0]),
      );
    },
  );

  it("retains the tool DOM and disclosure when sparse DTO history hydrates", () => {
    const options = {
      provider: "codex",
      format: "native",
      state: "complete",
    } as const;
    const sparse = chatRenderingFixture({ ...options, history: "sparse" });
    const rich = chatRenderingFixture({ ...options, history: "rich" });
    const props = {
      message: sparse.messages[1],
      sessionId: "session-fixture",
      isStreaming: false,
    };
    const view = render(<AssistantTurn {...props} />);
    const tool = screen.getByTestId("assistant-tool-block");
    fireEvent.click(tool.querySelector("summary")!);
    expect(tool).toHaveAttribute("open");
    view.rerender(<AssistantTurn {...props} entries={rich.entries} />);
    expect(screen.getByTestId("assistant-tool-block")).toBe(tool);
    expect(tool).toHaveAttribute("open");
    view.unmount();
    render(<AssistantTurn {...props} entries={rich.entries} />);
    expect(screen.getByTestId("assistant-tool-block")).toHaveAttribute("open");
  });

  it.each(
    chatProviders.flatMap((provider) =>
      chatFormats.map((format) => ({ provider, format })),
    ),
  )(
    "keeps $provider $format result images visible outside collapsed tool details",
    async ({ provider, format }) => {
      const fixture = chatRenderingFixture({
        provider,
        format,
        history: "rich",
        state: "complete",
      });
      const entries = fixture.entries.map((entry) =>
        entry.metadata?.providerEvent === "provided_agent.tool.completed"
          ? {
              ...entry,
              metadata: {
                ...entry.metadata,
                imageArtifacts: [
                  { attachmentId: "fixture-image", contentType: "image/png" },
                ],
              },
            }
          : entry,
      );
      const fetch = vi.spyOn(globalThis, "fetch").mockResolvedValue(
        new Response(new Blob(["png"], { type: "image/png" }), {
          status: 200,
        }),
      );
      render(
        <AssistantTurn
          message={fixture.messages[1]}
          entries={entries}
          sessionId="session-fixture"
          isStreaming={false}
        />,
      );
      const tool = screen.getByTestId("assistant-tool-block");
      expect(tool).not.toHaveAttribute("open");
      const image = await screen.findByRole("img", {
        name: "Browser screenshot captured by the agent",
      });
      expect(image).toBeVisible();
      expect(fetch).toHaveBeenCalledWith(
        "/api/v1/sessions/session-fixture/attachments/fixture-image/content",
        expect.anything(),
      );
      fireEvent.click(screen.getByTestId("chat-screenshot-thumb"));
      expect(
        screen.getByRole("dialog", { name: "Screenshot preview" }),
      ).toBeVisible();
      fireEvent.click(screen.getByTestId("chat-screenshot-lightbox-close"));
      expect(
        screen.queryByRole("dialog", { name: "Screenshot preview" }),
      ).not.toBeInTheDocument();
      fetch.mockRestore();
    },
  );

  it("recovers saved narration outside a partial native history window without duplicating it", () => {
    const fixture = chatRenderingFixture({
      provider: "claude_code",
      format: "native",
      history: "rich",
      state: "complete",
    });
    render(
      <AssistantTurn
        message={fixture.messages[1]}
        entries={fixture.entries.slice(2)}
        sessionId="partial"
        isStreaming={false}
      />,
    );
    expect(screen.getAllByText(fixtureNarration)).toHaveLength(1);
    const text = screen.getByTestId("assistant-message").textContent!;
    expect(text.indexOf(fixtureNarration)).toBeLessThan(
      text.indexOf("rg renderer src"),
    );
  });

  describe("activity below the loaded history (g5.chat.history-window)", () => {
    const rich = (format: "native" | "legacy", state = "complete" as const) =>
      chatRenderingFixture({
        provider: "claude_code",
        format,
        history: "rich",
        state,
      });

    it.each(chatFormats)(
      "a partially loaded %s turn withholds saved prose ahead of its first loaded block",
      (format) => {
        const fixture = rich(format);
        const onLoad = vi.fn();
        render(
          <AssistantTurn
            message={fixture.messages[1]}
            entries={fixture.entries.slice(2)}
            activityCoverage="partial"
            earlierActivity={{ state: "idle", onLoad }}
            sessionId="partial"
            isStreaming={false}
          />,
        );
        const turn = screen.getByTestId("assistant-message");
        expect(turn).not.toHaveTextContent(fixtureNarration);
        expect(
          screen.getAllByText("The shared renderer is ready."),
        ).toHaveLength(1);
        expect(screen.getAllByTestId("assistant-tool-block")).toHaveLength(1);
        // The marker sits where the loaded activity begins, above the tool.
        const marker = screen.getByTestId("chat-earlier-activity");
        const text = turn.textContent!;
        expect(text.indexOf(marker.textContent!)).toBeLessThan(
          text.indexOf("rg renderer src"),
        );
        fireEvent.click(screen.getByTestId("chat-earlier-activity-button"));
        expect(onLoad).toHaveBeenCalledTimes(1);
      },
    );

    it("keeps recovering trailing saved prose for a partially loaded turn", () => {
      const fixture = rich("native");
      // The answer block's events are not loaded but come after loaded ones.
      render(
        <AssistantTurn
          message={fixture.messages[1]}
          entries={fixture.entries.slice(1, 4)}
          activityCoverage="partial"
          sessionId="partial-tail"
          isStreaming={false}
        />,
      );
      const text = screen.getByTestId("assistant-message").textContent!;
      expect(text.indexOf(fixtureNarration)).toBeLessThan(
        text.indexOf("rg renderer src"),
      );
      expect(text.indexOf("rg renderer src")).toBeLessThan(
        text.indexOf("The shared renderer is ready."),
      );
    });

    it.each(chatFormats)(
      "a %s turn with no loaded activity shows its marker and nothing of the saved reply",
      (format) => {
        const fixture = rich(format);
        render(
          <AssistantTurn
            message={fixture.messages[1]}
            entries={[]}
            activityCoverage="unloaded"
            earlierActivity={{ state: "idle", onLoad: vi.fn() }}
            sessionId="unloaded"
            isStreaming={false}
          />,
        );
        const turn = screen.getByTestId("assistant-message");
        expect(screen.getByTestId("chat-earlier-activity")).toHaveTextContent(
          "Earlier activity in this reply isn't loaded.",
        );
        expect(turn).not.toHaveTextContent(fixtureNarration);
        expect(turn).not.toHaveTextContent("The shared renderer is ready.");
        expect(screen.queryByText("Saved example")).not.toBeInTheDocument();
        expect(screen.queryByRole("button", { name: "Source" })).toBeNull();
      },
    );

    it("keeps an unloaded turn's terminal state and saved tool list out of its reply", () => {
      const sparse = chatRenderingFixture({
        provider: "codex",
        format: "legacy",
        history: "sparse",
        state: "stopped",
      });
      render(
        <AssistantTurn
          message={sparse.messages[1]}
          activityCoverage="unloaded"
          sessionId="unloaded-stopped"
          isStreaming={false}
        />,
      );
      expect(screen.getByTestId("assistant-message")).toHaveTextContent(
        "Stopped",
      );
      expect(screen.queryByTestId("assistant-tool-block")).toBeNull();
      // Without a loader yet (the first read is pending) it says so.
      expect(screen.getByTestId("chat-earlier-activity")).toHaveAttribute(
        "data-state",
        "loading",
      );
      expect(screen.queryByTestId("chat-earlier-activity-button")).toBeNull();
    });

    it("offers a retry after a failed load", () => {
      const onLoad = vi.fn();
      render(
        <AssistantTurn
          message={rich("native").messages[1]}
          activityCoverage="unloaded"
          earlierActivity={{ state: "error", onLoad }}
          sessionId="unloaded-error"
          isStreaming={false}
        />,
      );
      expect(screen.getByTestId("chat-earlier-activity")).toHaveTextContent(
        "Couldn't load earlier activity in this reply.",
      );
      fireEvent.click(screen.getByRole("button", { name: "Retry" }));
      expect(onLoad).toHaveBeenCalledTimes(1);
    });

    it("loads while the marker is near the viewport, and again after each page", () => {
      let report: (visible: boolean) => void = () => undefined;
      const observe = vi.fn();
      const original = window.IntersectionObserver;
      window.IntersectionObserver = class {
        constructor(
          callback: (records: { isIntersecting: boolean }[]) => void,
        ) {
          report = (visible) => callback([{ isIntersecting: visible }]);
        }
        observe = observe;
        disconnect() {}
      } as unknown as typeof IntersectionObserver;
      onTestFinished(() => {
        window.IntersectionObserver = original;
      });
      const onLoad = vi.fn();
      const message = rich("native").messages[1];
      const turn = (state: "idle" | "loading" | "error") => (
        <AssistantTurn
          message={message}
          activityCoverage="unloaded"
          earlierActivity={{ state, onLoad }}
          sessionId="auto"
          isStreaming={false}
        />
      );
      const { rerender } = render(turn("idle"));
      expect(observe).toHaveBeenCalledTimes(1);
      expect(onLoad).not.toHaveBeenCalled();

      act(() => report(true));
      expect(onLoad).toHaveBeenCalledTimes(1);
      rerender(turn("loading"));
      rerender(turn("idle"));
      expect(onLoad).toHaveBeenCalledTimes(2);
      // A failure waits for an explicit retry.
      rerender(turn("error"));
      expect(onLoad).toHaveBeenCalledTimes(2);
      act(() => report(false));
      rerender(turn("idle"));
      expect(onLoad).toHaveBeenCalledTimes(2);
    });
  });

  it("preserves a saved pending tool without claiming it ran or requesting permission", () => {
    const fixture = chatRenderingFixture({
      provider: "codex",
      format: "legacy",
      history: "sparse",
      state: "streaming",
    });
    const message = {
      ...fixture.messages[1],
      toolCalls: [
        {
          id: "planned",
          name: "Bash",
          arguments: { command: "npm test" },
          status: "pending",
        },
      ],
    };
    render(
      <AssistantTurn message={message} sessionId="pending-tool" isStreaming />,
    );
    const tool = screen.getByTestId("assistant-tool-block");
    expect(tool.querySelector("summary")).toHaveTextContent("pending");
    expect(tool).not.toHaveTextContent("running");
    expect(
      screen.queryByTestId("agent-approval-approve"),
    ).not.toBeInTheDocument();
  });

  it("keeps user and queued prompts boxed on the right", () => {
    const fixture = chatRenderingFixture({
      provider: "codex",
      format: "native",
      history: "sparse",
      state: "streaming",
    });
    render(
      <ChatMessage
        message={fixture.messages[0]}
        isStreaming={false}
        isQueued
      />,
    );
    expect(screen.getByTestId("user-message")).toHaveClass("items-end");
    expect(screen.getByTestId("user-message").firstElementChild).toHaveClass(
      "max-w-[min(100%,8in)]",
      "rounded-xl",
      "bg-[var(--g5-info)]/20",
    );
    expect(screen.getByLabelText("Queued")).toBeInTheDocument();
  });
});

// Both legacy transport shapes and normalized native requests use these same
// cards. Native permissions intentionally lack the legacy Deny & Stop action.
describe("shared approval delivery", () => {
  function permission(native = false, provider = "claude_code"): TimelineEntry {
    return {
      id: "permission",
      entryType: "approval",
      content: "Run tests",
      createdAt: "2026-10-25T00:00:02Z",
      metadata: {
        phase: "approval",
        source: "provided_agent",
        provider,
        providerEvent: "provided_agent.approval.requested",
        messageId: "assistant-fixture",
        approvalId: "permission-fixture",
        toolName: "Bash",
        input: { command: "npm test" },
        ...(native
          ? {
              taskId: "task-fixture",
              attempt: 1,
              providerRequestId: "permission-fixture",
              requestKind: "permission",
            }
          : {}),
      },
    };
  }
  const message = chatRenderingFixture({
    provider: "claude_code",
    format: "legacy",
    history: "sparse",
    state: "streaming",
  }).messages[1];
  it.each([false, true])(
    "awaits delivery and preserves failed/pending feedback, native=%s",
    async (native) => {
      let reject!: (error: Error) => void;
      const resolve = vi.fn(
        () =>
          new Promise<void>((_resolve, rejectPromise) => {
            reject = rejectPromise;
          }),
      );
      const props = {
        message,
        sessionId: message.sessionId,
        isStreaming: true,
        onResolveAgentApproval: resolve,
      };
      const view = render(
        <AssistantTurn {...props} entries={[permission(native)]} />,
      );
      fireEvent.click(screen.getByTestId("agent-approval-approve"));
      fireEvent.click(screen.getByTestId("agent-approval-approve"));
      expect(resolve).toHaveBeenCalledTimes(1);
      expect(screen.getByTestId("agent-approval-approve")).toBeDisabled();
      view.rerender(
        <AssistantTurn
          {...props}
          entries={[permission(native, "provided_agent")]}
        />,
      );
      expect(screen.getByTestId("agent-approval-approve")).toBeDisabled();
      await act(async () => {
        reject(new Error("Delivery failed; retry."));
      });
      expect(screen.getByRole("alert")).toHaveTextContent(
        "Delivery failed; retry.",
      );
      expect(screen.getByTestId("agent-approval-approve")).toBeEnabled();
      expect(screen.queryByTestId("agent-approval-deny-stop") !== null).toBe(
        !native,
      );
      resolve.mockImplementationOnce(
        async () => ({ status: "resolving", delivered: false }) as never,
      );
      fireEvent.click(screen.getByTestId("agent-approval-approve"));
      await screen.findByText(
        "Approval delivery is still resolving. Retry shortly.",
      );
    },
  );

  it("scopes pending delivery to the native task and attempt, even if request ids repeat", async () => {
    let finish!: () => void;
    const resolve = vi.fn(
      () =>
        new Promise<void>((complete) => {
          finish = complete;
        }),
    );
    const first = permission(true);
    const props = {
      message,
      sessionId: message.sessionId,
      isStreaming: true,
      onResolveAgentApproval: resolve,
    };
    const view = render(<AssistantTurn {...props} entries={[first]} />);
    fireEvent.click(screen.getByTestId("agent-approval-approve"));
    expect(screen.getByTestId("agent-approval-approve")).toBeDisabled();
    view.rerender(
      <AssistantTurn
        {...props}
        entries={[
          {
            ...first,
            metadata: { ...first.metadata, taskId: "later-task", attempt: 2 },
          },
        ]}
      />,
    );
    expect(screen.getByTestId("agent-approval-approve")).toBeEnabled();
    await act(async () => {
      finish();
    });
    expect(screen.getByTestId("agent-approval-approve")).toBeEnabled();
  });

  it("offers only permission updates supported by the native reply route", () => {
    const entry = permission(true);
    entry.metadata = {
      ...entry.metadata,
      suggestions: [
        { type: "setMode", mode: "acceptEdits", destination: "session" },
        {
          label: "Use safe command",
          updatedInput: { command: "npm test -- --run" },
        },
      ],
    };
    render(
      <AssistantTurn
        message={message}
        sessionId={message.sessionId}
        entries={[entry]}
        isStreaming
        onResolveAgentApproval={vi.fn()}
      />,
    );
    expect(screen.queryByText(/Allow edits/i)).not.toBeInTheDocument();
    expect(screen.getByTestId("agent-approval-suggestion")).toHaveTextContent(
      "Use safe command",
    );
    expect(
      screen.queryByTestId("agent-approval-deny-stop"),
    ).not.toBeInTheDocument();
  });

  it("treats question-shaped permission inputs as permissions", () => {
    const entry = permission(true);
    entry.metadata = {
      ...entry.metadata,
      toolName: "AskUserQuestion",
      input: {
        questions: [{ question: "Allow?", options: [{ label: "Yes" }] }],
      },
    };
    render(
      <AssistantTurn
        message={message}
        sessionId={message.sessionId}
        entries={[entry]}
        isStreaming
        onResolveAgentApproval={vi.fn()}
      />,
    );
    expect(screen.queryByTestId("agent-question-form")).not.toBeInTheDocument();
    expect(screen.getByTestId("agent-approval-approve")).toBeEnabled();
  });
});

it("bounds tab-local view state and tolerates unavailable storage", () => {
  for (let index = 0; index < 55; index++)
    writeChatViewState("test", String(index), { expanded: true });
  expect(readChatViewState("test", "0")).toBeUndefined();
  expect(readChatViewState("test", "54")).toEqual({ expanded: true });
  writeChatViewState("test", "54", undefined);
  expect(readChatViewState("test", "54")).toBeUndefined();
  writeChatViewState("defaults", "chosen", { expanded: true });
  for (let index = 0; index < 80; index++)
    writeChatViewState("defaults", String(index), {});
  expect(readChatViewState("defaults", "chosen")).toEqual({ expanded: true });
  writeChatViewState("defaults", "chosen", {});
  expect(readChatViewState("defaults", "chosen")).toBeUndefined();
  const get = vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
    throw new Error("disabled");
  });
  expect(readChatViewState("test", "53")).toBeUndefined();
  get.mockRestore();
});

describe("memoized turn (g5.chat.live-tail-recovery)", () => {
  const message = createMockChatMessage({ id: "turn-1" });
  const entries: TimelineEntry[] = [];
  const onCitationClick = () => {};
  const props = {
    message,
    entries,
    isStreaming: false,
    sessionId: "session-1",
    onCitationClick,
  };

  it("re-renders only when a rendered field, the entries or a callback changed", () => {
    expect(assistantTurnPropsEqual(props, { ...props })).toBe(true);
    expect(
      assistantTurnPropsEqual(props, { ...props, message: { ...message } }),
    ).toBe(true);
    expect(assistantTurnPropsEqual(props, { ...props, entries: [] })).toBe(
      true,
    );
    expect(
      assistantTurnPropsEqual(props, {
        ...props,
        message: { ...message, content: "changed" },
      }),
    ).toBe(false);
    expect(
      assistantTurnPropsEqual(props, {
        ...props,
        entries: [
          {
            id: "entry-1",
            entryType: "output",
            content: "delta",
            createdAt: "2026-11-02T00:00:00.000Z",
            metadata: {},
          },
        ],
      }),
    ).toBe(false);
    expect(
      assistantTurnPropsEqual(props, { ...props, isStreaming: true }),
    ).toBe(false);
    expect(
      assistantTurnPropsEqual(props, { ...props, onCitationClick: () => {} }),
    ).toBe(false);
  });

  it("re-renders when its ontology card's state changes (g5.chat.ontology-card)", () => {
    const ontologyCard = { newest: true, turnRunning: false };
    const withCard = { ...props, ontologyCard };
    expect(
      assistantTurnPropsEqual(withCard, {
        ...props,
        ontologyCard: { ...ontologyCard },
      }),
    ).toBe(true);
    expect(
      assistantTurnPropsEqual(withCard, {
        ...props,
        ontologyCard: { ...ontologyCard, newest: false },
      }),
    ).toBe(false);
    expect(
      assistantTurnPropsEqual(withCard, {
        ...props,
        ontologyCard: { ...ontologyCard, turnRunning: true },
      }),
    ).toBe(false);
    expect(assistantTurnPropsEqual(withCard, props)).toBe(false);
  });
});
