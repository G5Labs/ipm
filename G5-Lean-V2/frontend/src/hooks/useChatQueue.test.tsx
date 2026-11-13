import React from "react";
import {
  act,
  fireEvent,
  render,
  renderHook,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import { server } from "../test/msw/server";
import { createMockChatMessage, createMockChatSession } from "../test/fixtures";
import { ChatPanel } from "../components/chat/ChatPanel";
import { visibleChatQueue, type ChatQueueInput } from "../lib/chatQueue";
import type {
  ChatAgentProvider,
  ChatMessageDTO,
  SessionInputDTO,
} from "../lib/types";
import {
  useChatMessages,
  useSendChatMessage,
  useSessionInputs,
  useWithdrawSessionInput,
} from "./useChat";

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

function setup() {
  const client = new QueryClient({
    defaultOptions: {
      queries: { retry: false, staleTime: Infinity, gcTime: Infinity },
      mutations: { retry: false },
    },
  });
  const wrapper = ({ children }: { children: React.ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );
  return { client, wrapper };
}

function queued(
  id: string,
  overrides: Partial<SessionInputDTO> = {},
): SessionInputDTO {
  return {
    id,
    sessionId: "s1",
    inputIndex: 1,
    content: "follow up",
    artifacts: [],
    revision: 1,
    inputOrigin: "human",
    delivery: "queued",
    deliveryGeneration: 0,
    createdAt: "2026-11-03T12:00:00Z",
    editable: true,
    withdrawable: true,
    ...overrides,
  };
}

function Harness({
  provider,
  layout,
  sessionId = "s1",
}: {
  provider: ChatAgentProvider;
  layout: "panel" | "center";
  sessionId?: string;
}) {
  const state = useChatMessages(sessionId);
  const send = useSendChatMessage();
  const messages = state.status === "ok" ? state.data : [];
  const last = messages.at(-1);
  const streaming = last?.role === "assistant" && last.status === "streaming";
  return (
    <>
      <output data-testid="streaming">{String(streaming)}</output>
      <output data-testid="send-pending">{String(send.isLoading)}</output>
      <output data-testid="send-error">{send.error?.message}</output>
      <ChatPanel
        layout={layout}
        session={createMockChatSession({
          id: sessionId,
          agentProvider: provider,
        })}
        messages={messages}
        isStreaming={streaming}
        sessions={[]}
        scopeType="workspace"
        scopeRef="workspace-1"
        scopeLabel="Workspace"
        selectedAgentProvider={provider}
        availableAgentProviders={[provider]}
        onSummarize={() => {}}
        onSelectSession={() => {}}
        onSendMessage={async (content, options) => {
          try {
            await send.mutateAsync({
              sessionId,
              content,
              agentProvider: provider,
              attachments: options?.attachments,
              context: options?.projectReferences?.length
                ? { projectReferences: options.projectReferences }
                : undefined,
            });
            return true;
          } catch {
            return false;
          }
        }}
      />
    </>
  );
}

describe("queue-first chat submission", () => {
  it.each([
    ["claude_code", "panel", true, "Enter"],
    ["codex", "panel", true, "Send"],
    ["claude_code", "center", false, "Send"],
    ["codex", "center", false, "Enter"],
  ] as const)(
    "%s / %s / busy %s / %s retains one representation across slow reads",
    async (provider, layout, busy, action) => {
      const { client, wrapper } = setup();
      const post = deferred<void>();
      const inputs = deferred<SessionInputDTO[]>();
      const history = deferred<ChatMessageDTO[]>();
      const inputId = "server-input";
      let inputsStarted = false;
      let historyFails = true;
      client.setQueryData(
        ["chat-messages", "s1"],
        busy
          ? [
              createMockChatMessage({
                id: "assistant",
                sessionId: "s1",
                role: "assistant",
                status: "streaming",
                content: "Still working",
              }),
            ]
          : [],
      );
      client.setQueryData(["session-inputs", "s1"], []);
      server.use(
        http.get("/api/v1/sessions/s1/messages", async () =>
          historyFails
            ? HttpResponse.json(
                { error: { message: "Unavailable" } },
                { status: 503 },
              )
            : HttpResponse.json({ data: await history.promise }),
        ),
        http.post("/api/v1/sessions/s1/messages", async () => {
          await post.promise;
          return HttpResponse.json({
            data: {
              messageId: inputId,
              inputId,
              delivery: "queued",
              status: "queued",
            },
          });
        }),
        http.get("/api/v1/sessions/s1/inputs", async () => {
          inputsStarted = true;
          return HttpResponse.json({ data: await inputs.promise });
        }),
      );
      render(<Harness provider={provider} layout={layout} />, { wrapper });
      fireEvent.change(screen.getByTestId("chat-input"), {
        target: { value: "follow up" },
      });
      if (action === "Enter")
        fireEvent.keyDown(screen.getByTestId("chat-input"), { key: "Enter" });
      else fireEvent.click(screen.getByTestId("chat-panel-send"));
      await waitFor(() =>
        expect(screen.getByTestId("chat-queued-inputs")).toHaveTextContent(
          "follow up",
        ),
      );
      expect(screen.getAllByText("follow up")).toHaveLength(1);
      expect(screen.getByText("Waiting")).toBeInTheDocument();
      expect(
        screen.queryByLabelText("Edit queued message"),
      ).not.toBeInTheDocument();
      expect(screen.getByTestId("streaming")).toHaveTextContent(String(busy));
      await act(async () => {
        post.resolve();
      });
      await waitFor(() => expect(inputsStarted).toBe(true));
      expect(screen.getAllByText("follow up")).toHaveLength(1);
      expect(
        within(screen.getByTestId("chat-queued-inputs")).getByText("follow up"),
      ).toBeInTheDocument();
      await act(async () => {
        inputs.resolve([queued(inputId)]);
      });
      await waitFor(() =>
        expect(
          screen.getByLabelText("Edit queued message"),
        ).toBeInTheDocument(),
      );

      const savedHistory =
        client.getQueryData<ChatMessageDTO[]>(["chat-messages", "s1"]) ?? [];
      if (busy) {
        act(() =>
          client.setQueryData(
            ["chat-messages", "s1"],
            savedHistory.map((message) => ({
              ...message,
              content: "Still working: more streamed output",
            })),
          ),
        );
      }
      // The queue read wins the race with history and omits the delivered input.
      server.use(
        http.get("/api/v1/sessions/s1/inputs", () =>
          HttpResponse.json({ data: [] }),
        ),
      );
      await act(async () => {
        await client.invalidateQueries({ queryKey: ["session-inputs", "s1"] });
      });
      expect(screen.getAllByText("follow up")).toHaveLength(1);
      await waitFor(() =>
        expect(
          screen.queryByLabelText("Edit queued message"),
        ).not.toBeInTheDocument(),
      );
      await waitFor(() =>
        expect(client.getQueryState(["chat-messages", "s1"])?.status).toBe(
          "error",
        ),
      );
      expect(screen.getAllByText("follow up")).toHaveLength(1);
      if (busy)
        expect(
          screen.getByText("Still working: more streamed output"),
        ).toBeInTheDocument();
      historyFails = false;
      await act(async () => {
        history.resolve([
          ...savedHistory,
          createMockChatMessage({
            id: inputId,
            sessionId: "s1",
            role: "user",
            status: "complete",
            content: "follow up",
            messageIndex: 2,
          }),
        ]);
        await client.invalidateQueries({ queryKey: ["chat-messages", "s1"] });
      });
      await waitFor(() =>
        expect(
          screen.queryByTestId("chat-queued-inputs"),
        ).not.toBeInTheDocument(),
      );
      expect(screen.getAllByText("follow up")).toHaveLength(1);
      if (busy)
        expect(
          screen.getByText("Still working: more streamed output"),
        ).toBeInTheDocument();
      // A stale queue response cannot resurrect the tray after delivery.
      server.use(
        http.get("/api/v1/sessions/s1/inputs", () =>
          HttpResponse.json({ data: [queued(inputId)] }),
        ),
      );
      await act(async () => {
        await client.invalidateQueries({ queryKey: ["session-inputs", "s1"] });
      });
      expect(
        screen.queryByTestId("chat-queued-inputs"),
      ).not.toBeInTheDocument();
      client.clear();
    },
  );

  it("reconciles early delivery and repeated text once responses supply identity, across a session switch", async () => {
    const { client, wrapper } = setup();
    const posts = [deferred<void>(), deferred<void>()];
    const ids: string[] = [];
    client.setQueryData(["chat-messages", "s1"], []);
    client.setQueryData(["chat-messages", "s2"], []);
    client.setQueryData(["session-inputs", "s1"], []);
    client.setQueryData(["session-inputs", "s2"], []);
    server.use(
      http.post("/api/v1/sessions/s1/messages", async () => {
        const inputId = `server-${ids.length}`;
        const index = ids.push(inputId) - 1;
        await posts[index].promise;
        return HttpResponse.json({
          data: {
            messageId: inputId,
            inputId,
            delivery: "queued",
            status: "queued",
          },
        });
      }),
      http.get("/api/v1/sessions/:id/inputs", () =>
        HttpResponse.json({ data: [] }),
      ),
    );
    const { result, rerender } = renderHook(
      ({ sessionId }) => {
        const send = useSendChatMessage();
        const queue = useSessionInputs(sessionId);
        const history = useChatMessages(sessionId);
        const visible = visibleChatQueue(
          queue.status === "ok" ? queue.data : [],
          history.status === "ok" ? history.data : [],
        );
        return { send, visible };
      },
      { wrapper, initialProps: { sessionId: "s1" } },
    );
    act(() => {
      result.current.send.mutate({ sessionId: "s1", content: "same" });
      result.current.send.mutate({ sessionId: "s1", content: "same" });
    });
    await waitFor(() => expect(ids).toHaveLength(2));
    expect(new Set(ids).size).toBe(2);
    await waitFor(() => expect(result.current.visible).toHaveLength(2));
    act(() =>
      client.setQueryData(
        ["chat-messages", "s1"],
        [
          createMockChatMessage({
            id: ids[0],
            sessionId: "s1",
            role: "user",
            content: "same",
          }),
        ],
      ),
    );
    // Existing API limit: no server identity is known until POST resolves.
    // Both local submissions remain; text matching must not discard either.
    expect(result.current.visible).toHaveLength(2);
    act(() =>
      client.setQueryData(
        ["session-inputs", "s1"],
        [
          ...client.getQueryData<ChatQueueInput[]>(["session-inputs", "s1"])!,
          queued(ids[1], {
            delivery: "reserved",
            editable: false,
            withdrawable: false,
          }),
        ],
      ),
    );
    await waitFor(() => expect(result.current.visible).toHaveLength(3));
    rerender({ sessionId: "s2" });
    expect(result.current.visible).toEqual([]);
    await act(async () => {
      posts[1].resolve();
      posts[0].resolve();
    });
    rerender({ sessionId: "s1" });
    await waitFor(() =>
      expect(result.current.visible.map((input) => input.id)).toEqual([ids[1]]),
    );
    expect(client.getQueryData(["session-inputs", "s2"])).toEqual([]);
    client.clear();
  });

  it("removes a rejected submission, restores the existing draft and preserves streamed output", async () => {
    const { client, wrapper } = setup();
    const post = deferred<void>();
    client.setQueryData(["session-inputs", "s1"], []);
    client.setQueryData(
      ["chat-messages", "s1"],
      [
        createMockChatMessage({
          id: "assistant",
          sessionId: "s1",
          role: "assistant",
          status: "streaming",
          content: "Working",
        }),
      ],
    );
    server.use(
      http.post("/api/v1/sessions/s1/messages", async () => {
        await post.promise;
        return HttpResponse.json(
          { error: { message: "Refused" } },
          { status: 409 },
        );
      }),
    );
    render(<Harness provider="codex" layout="panel" />, { wrapper });
    const input = screen.getByTestId("chat-input");
    fireEvent.change(input, { target: { value: "retry me" } });
    fireEvent.keyDown(input, { key: "Enter" });
    await screen.findByTestId("chat-queued-inputs");
    act(() =>
      client.setQueryData<ChatMessageDTO[]>(
        ["chat-messages", "s1"],
        (messages = []) =>
          messages.map((message) => ({ ...message, content: "More output" })),
      ),
    );
    await act(async () => {
      post.resolve();
    });
    await waitFor(() => expect(input).toHaveValue("retry me"));
    expect(screen.queryByTestId("chat-queued-inputs")).toBeNull();
    expect(screen.getByText("More output")).toBeInTheDocument();
    expect(screen.getByTestId("streaming")).toHaveTextContent("true");
    expect(screen.queryByRole("button", { name: "Restore draft" })).toBeNull();
    client.clear();
  });

  it("retains a submission across switching away from an older in-flight queue read", async () => {
    const { client, wrapper } = setup();
    const get = deferred<SessionInputDTO[]>();
    const post = deferred<void>();
    let reading = false;
    const inputId = "server-input";
    let posting = false;
    server.use(
      http.get("/api/v1/sessions/:id/inputs", async () => {
        reading = true;
        return HttpResponse.json({ data: await get.promise });
      }),
      http.post("/api/v1/sessions/s1/messages", async () => {
        posting = true;
        await post.promise;
        return HttpResponse.json({
          data: { inputId, messageId: inputId, status: "queued" },
        });
      }),
    );
    const { result, rerender } = renderHook(
      ({ sessionId }) => ({
        send: useSendChatMessage(),
        queue: useSessionInputs(sessionId),
      }),
      { wrapper, initialProps: { sessionId: "s1" } },
    );
    await waitFor(() => expect(reading).toBe(true));
    act(() =>
      result.current.send.mutate({ sessionId: "s1", content: "follow up" }),
    );
    await waitFor(() => expect(posting).toBe(true));
    rerender({ sessionId: "s2" });
    expect(
      client
        .getQueryData<ChatQueueInput[]>(["session-inputs", "s1"])
        ?.map((input) => input.id),
    ).toEqual([expect.stringMatching(/^optimistic-/)]);
    await act(async () => {
      post.resolve();
    });
    rerender({ sessionId: "s1" });
    await act(async () => {
      get.resolve([queued(inputId)]);
    });
    await waitFor(() =>
      expect(result.current.queue).toMatchObject({
        status: "ok",
        data: [expect.objectContaining({ id: inputId, editable: true })],
      }),
    );
    client.clear();
  });

  it("cannot restore a rejected submission by cancelling a queue read on switch", async () => {
    const { client, wrapper } = setup();
    const get = deferred<SessionInputDTO[]>();
    const post = deferred<void>();
    let posting = false;
    let reading = false;
    client.setQueryData(["session-inputs", "s1"], []);
    client.setQueryData(["session-inputs", "s2"], []);
    server.use(
      http.get("/api/v1/sessions/s1/inputs", async () => {
        reading = true;
        return HttpResponse.json({ data: await get.promise });
      }),
      http.post("/api/v1/sessions/s1/messages", async () => {
        posting = true;
        await post.promise;
        return HttpResponse.json(
          { error: { code: "REFUSED", message: "Refused" } },
          { status: 409 },
        );
      }),
    );
    const { result, rerender } = renderHook(
      ({ sessionId }) => ({
        send: useSendChatMessage(),
        queue: useSessionInputs(sessionId),
      }),
      { wrapper, initialProps: { sessionId: "s1" } },
    );
    act(() =>
      result.current.send.mutate({ sessionId: "s1", content: "rejected" }),
    );
    await waitFor(() => expect(posting).toBe(true));
    act(() => {
      void client.invalidateQueries({ queryKey: ["session-inputs", "s1"] });
    });
    await waitFor(() => expect(reading).toBe(true));
    await act(async () => {
      post.resolve();
    });
    await waitFor(() => expect(result.current.send.error).toBeTruthy());
    rerender({ sessionId: "s2" });
    expect(client.getQueryData(["session-inputs", "s1"])).toEqual([]);
    await act(async () => {
      get.resolve([]);
    });
    expect(client.getQueryData(["session-inputs", "s1"])).toEqual([]);
    client.clear();
  });

  it("cancels an older queue read when withdrawal commits", async () => {
    const { client, wrapper } = setup();
    const oldRead = deferred<SessionInputDTO[]>();
    let readStarted = false;
    client.setQueryData(["session-inputs", "s1"], [queued("input")]);
    server.use(
      http.get("/api/v1/sessions/s1/inputs", async () => {
        readStarted = true;
        return HttpResponse.json({ data: await oldRead.promise });
      }),
      http.post("/api/v1/sessions/s1/inputs/input/withdraw", () =>
        HttpResponse.json({
          data: queued("input", {
            delivery: "withdrawn",
            editable: false,
            withdrawable: false,
          }),
        }),
      ),
    );
    const { result } = renderHook(
      () => ({
        queue: useSessionInputs("s1"),
        withdraw: useWithdrawSessionInput(),
      }),
      { wrapper },
    );
    act(() => {
      void client.invalidateQueries({ queryKey: ["session-inputs", "s1"] });
    });
    await waitFor(() => expect(readStarted).toBe(true));
    await act(async () => {
      await result.current.withdraw.mutateAsync({
        sessionId: "s1",
        inputId: "input",
        expectedRevision: 1,
      });
    });
    await act(async () => {
      oldRead.resolve([queued("input")]);
    });
    expect(
      client.getQueryData<ChatQueueInput[]>(["session-inputs", "s1"])?.[0]
        .delivery,
    ).toBe("withdrawn");
    client.clear();
  });
});
