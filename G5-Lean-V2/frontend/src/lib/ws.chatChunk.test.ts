/**
 * Vitest unit test for the REAL `WebSocketManager` chat streaming cache
 * transitions (GitHub issue G5Labs/G5#1106). Unlike `src/test/websocket.test.tsx`
 * (which drives a duplicated mock), this drives production `ws.ts` directly via
 * `handleEvent`, so it is a true regression guard for:
 *
 *  - Defect 2: `chat.chunk` folds the delta into the streamed assistant message
 *    identified by messageId, wherever it sits in the list — NOT the array tail.
 *    A mid-stream steer appends an optimistic pending user message to the tail;
 *    deltas must still reach the still-streaming assistant behind it.
 *  - Defect 3: `chat.error` marks the message errored but PRESERVES any already
 *    -streamed partial content (status-only cache transition).
 *
 * SOT: .semlift/specs/software_architecture/client/data-layer/websocket.semlift.md D15
 */

import { describe, it, expect, vi } from "vitest";
import type { QueryClient } from "@tanstack/react-query";
import { WebSocketManager, type WSEvent } from "./ws";
import type { ChatMessageDTO } from "./types";

const SESSION_ID = "session-1";

/**
 * Minimal in-memory query cache that implements the `setQueryData` /
 * `getQueryData` / `invalidateQueries` surface `WebSocketManager` uses so the
 * real handler runs unmodified.
 */
function makeManager(seed: ChatMessageDTO[]) {
  const store = new Map<string, unknown>();
  store.set(JSON.stringify(["chat-messages", SESSION_ID]), seed);

  const invalidateQueries = vi.fn();
  const queryClient = {
    invalidateQueries,
    getQueryData: (key: unknown[]) => store.get(JSON.stringify(key)),
    setQueryData: (key: unknown[], updater: unknown) => {
      const k = JSON.stringify(key);
      const prev = store.get(k);
      const next =
        typeof updater === "function"
          ? (updater as (old: unknown) => unknown)(prev)
          : updater;
      store.set(k, next);
      return next;
    },
  } as unknown as QueryClient;

  const manager = new WebSocketManager({
    getBaseUrl: () => "ws://test",
    getWsTicket: () => Promise.resolve("ticket"),
    queryClient,
  });

  const dispatch = (event: WSEvent) =>
    (manager as unknown as { handleEvent: (e: WSEvent) => void }).handleEvent(
      event,
    );
  const messages = () =>
    store.get(JSON.stringify(["chat-messages", SESSION_ID])) as
      ChatMessageDTO[] | undefined;

  return { dispatch, messages };
}

function assistant(content: string, status: string): ChatMessageDTO {
  return {
    id: "assistant-1",
    sessionId: SESSION_ID,
    role: "assistant",
    content,
    artifacts: [],
    citations: [],
    inputTokens: null,
    outputTokens: null,
    status,
    messageIndex: 0,
    createdAt: new Date().toISOString(),
  };
}

function optimisticUser(): ChatMessageDTO {
  return {
    id: "optimistic-user",
    sessionId: SESSION_ID,
    role: "user",
    content: "wait, also do this",
    artifacts: [],
    citations: [],
    inputTokens: null,
    outputTokens: null,
    status: "pending",
    messageIndex: 1,
    createdAt: new Date().toISOString(),
  };
}

describe("WebSocketManager chat streaming cache transitions (#1106)", () => {
  it("defect 2: chat.chunk folds into the streaming assistant even when a steered user message is the tail", () => {
    const { dispatch, messages } = makeManager([
      assistant("Partial ", "streaming"),
      optimisticUser(),
    ]);

    dispatch({
      type: "chat.chunk",
      sessionId: SESSION_ID,
      messageId: "assistant-1",
      delta: "answer",
    });

    const msgs = messages();
    expect(msgs?.find((m) => m.id === "assistant-1")?.content).toBe(
      "Partial answer",
    );
    // The tail user message is untouched.
    expect(msgs?.find((m) => m.id === "optimistic-user")?.content).toBe(
      "wait, also do this",
    );
  });

  it("defect 2: a pending assistant flips to streaming on first delta", () => {
    const { dispatch, messages } = makeManager([assistant("", "pending")]);

    dispatch({
      type: "chat.chunk",
      sessionId: SESSION_ID,
      messageId: "assistant-1",
      delta: "hi",
    });

    const msg = messages()?.[0];
    expect(msg?.content).toBe("hi");
    expect(msg?.status).toBe("streaming");
  });

  it("ignores a chat.chunk whose messageId is absent from the cache", () => {
    const { dispatch, messages } = makeManager([
      assistant("Existing", "streaming"),
    ]);

    dispatch({
      type: "chat.chunk",
      sessionId: SESSION_ID,
      messageId: "does-not-exist",
      delta: "stray",
    });

    expect(messages()?.[0]?.content).toBe("Existing");
  });

  it("materializes a persisted assistant placeholder when its first chunk wins the cache race", () => {
    const { dispatch, messages } = makeManager([
      assistant("Existing", "complete"),
    ]);

    dispatch({
      type: "chat.chunk",
      sessionId: SESSION_ID,
      messageId: "assistant-new",
      delta: "first chunk",
      messageIndex: 3,
      createdAt: "2026-09-21T12:00:00Z",
    });

    expect(
      messages()?.find((message) => message.id === "assistant-new"),
    ).toMatchObject({
      role: "assistant",
      content: "first chunk",
      status: "streaming",
      messageIndex: 3,
      createdAt: "2026-09-21T12:00:00Z",
    });
  });

  it("defect 3: chat.error marks the message errored but preserves partial streamed content", () => {
    const { dispatch, messages } = makeManager([
      assistant("The answer so far is", "streaming"),
    ]);

    dispatch({
      type: "chat.error",
      sessionId: SESSION_ID,
      messageId: "assistant-1",
      error: "RATE_LIMIT",
      message: "Rate limit exceeded",
    });

    const msg = messages()?.[0];
    expect(msg?.status).toBe("error");
    // The raw error string does NOT clobber the streamed body.
    expect(msg?.content).toBe("The answer so far is");
    // The human-readable diagnostic is recorded on the client-only
    // errorMessage field so the error indicator can surface it (D15).
    expect(msg?.errorMessage).toBe("Rate limit exceeded");
  });

  it("defect 3: chat.error falls back to the error code when no message is present", () => {
    const { dispatch, messages } = makeManager([assistant("", "streaming")]);

    dispatch({
      type: "chat.error",
      sessionId: SESSION_ID,
      messageId: "assistant-1",
      error: "agent_error",
      message: "",
    });

    const msg = messages()?.[0];
    expect(msg?.status).toBe("error");
    expect(msg?.errorMessage).toBe("agent_error");
  });
});
