/**
 * `useChatStopController` (chat g5.chat.controls): one request in flight per
 * turn, `Stopping…` at once, the runtime's disposition decides the state,
 * failure offers Retry, and the state ends with the turn.
 */
import { describe, expect, it } from "vitest";
import React from "react";
import { act, renderHook, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { http, HttpResponse } from "msw";
import { server } from "../test/msw/server";
import { createMockChatMessage } from "../test/fixtures";
import type { ChatMessageDTO } from "../lib/types";
import {
  chatStopActivityLabel,
  useChatStopController,
} from "./useChatStopController";

const SESSION_ID = "session-stop";

function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  });
  const wrapper = ({ children }: { children: React.ReactNode }) =>
    React.createElement(QueryClientProvider, { client: queryClient }, children);
  return { wrapper, queryClient };
}

function transcript(assistantStatus: string): ChatMessageDTO[] {
  return [
    createMockChatMessage({
      id: "user-1",
      sessionId: SESSION_ID,
      role: "user",
      content: "go",
      status: "complete",
      messageIndex: 0,
    }),
    createMockChatMessage({
      id: "assistant-1",
      sessionId: SESSION_ID,
      role: "assistant",
      content: "",
      status: assistantStatus,
      messageIndex: 1,
    }),
  ];
}

type Answer = { disposition?: string; status?: number };

function installStop(answers: () => Answer | Promise<Answer>) {
  const posts: string[] = [];
  server.use(
    http.post(
      "/api/v1/sessions/:sessionId/messages/:messageId/stop",
      async ({ params }) => {
        posts.push(String(params.messageId));
        const answer = await answers();
        if (answer.status && answer.status >= 400) {
          return HttpResponse.json(
            { error: { message: "Injected Stop failure" } },
            { status: answer.status },
          );
        }
        return HttpResponse.json({
          data: { stopped: true, disposition: answer.disposition },
        });
      },
    ),
  );
  return posts;
}

function useController(messages: ChatMessageDTO[]) {
  return useChatStopController({ sessionId: SESSION_ID, messages });
}

describe("useChatStopController", () => {
  it("shows Stopping… at once and sends one request while it is in flight", async () => {
    const { wrapper } = createWrapper();
    let release: () => void = () => {};
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    const posts = installStop(async () => {
      await gate;
      return { disposition: "signalling" };
    });
    const { result } = renderHook(
      () => useController(transcript("streaming")),
      {
        wrapper,
      },
    );
    act(() => result.current.onStop());
    expect(result.current.stopState?.phase).toBe("requesting");
    expect(chatStopActivityLabel(result.current.stopState)).toBe("Stopping…");
    act(() => result.current.onStop());
    act(() => result.current.onStop());
    await waitFor(() => expect(posts).toHaveLength(1));
    release();
    await waitFor(() =>
      expect(result.current.stopState?.phase).toBe("stopping"),
    );
    expect(posts).toHaveLength(1);
    // After the request settled a later click is an explicit, safe repeat.
    act(() => result.current.onStop());
    await waitFor(() => expect(posts).toHaveLength(2));
  });

  it("maps the runtime's disposition to the shown state", async () => {
    for (const [disposition, phase, label] of [
      ["already_finished", "finishing", "Finishing… saving the reply"],
      ["undelivered", "undelivered", "Stop recorded, delivering…"],
      ["cancelled", "stopping", "Stopping…"],
      ["ended", "stopping", "Stopping…"],
    ] as const) {
      const { wrapper } = createWrapper();
      installStop(() => ({ disposition }));
      const { result } = renderHook(
        () => useController(transcript("streaming")),
        { wrapper },
      );
      act(() => result.current.onStop());
      await waitFor(() => expect(result.current.stopState?.phase).toBe(phase));
      expect(chatStopActivityLabel(result.current.stopState)).toBe(label);
    }
  });

  it("clears the state when the turn had already ended", async () => {
    const { wrapper } = createWrapper();
    installStop(() => ({ disposition: "already_ended" }));
    const { result } = renderHook(
      () => useController(transcript("streaming")),
      {
        wrapper,
      },
    );
    act(() => result.current.onStop());
    await waitFor(() => expect(result.current.stopState).toBeNull());
  });

  it("a request that did not commit becomes a failure with Retry", async () => {
    const { wrapper } = createWrapper();
    let failures = 1;
    const posts = installStop(() => {
      if (failures > 0) {
        failures -= 1;
        return { status: 500 };
      }
      return { disposition: "signalling" };
    });
    const { result } = renderHook(
      () => useController(transcript("streaming")),
      {
        wrapper,
      },
    );
    act(() => result.current.onStop());
    await waitFor(() => expect(result.current.stopState?.phase).toBe("failed"));
    expect(result.current.stopState?.error).toMatch(/Injected Stop failure/);
    expect(result.current.stopControlSuppressed).toBe(false);
    act(() => result.current.onStop());
    await waitFor(() =>
      expect(result.current.stopState?.phase).toBe("stopping"),
    );
    expect(posts).toEqual(["assistant-1", "assistant-1"]);
  });

  it("the state ends with the turn, not on a timer", async () => {
    const { wrapper } = createWrapper();
    installStop(() => ({ disposition: "signalling" }));
    const { result, rerender } = renderHook(
      ({ messages }: { messages: ChatMessageDTO[] }) => useController(messages),
      { wrapper, initialProps: { messages: transcript("streaming") } },
    );
    act(() => result.current.onStop());
    await waitFor(() =>
      expect(result.current.stopState?.phase).toBe("stopping"),
    );
    rerender({ messages: transcript("streaming") });
    expect(result.current.stopState?.phase).toBe("stopping");
    rerender({ messages: transcript("stopped") });
    expect(result.current.stopState).toBeNull();
  });

  it("does nothing without a live assistant turn", () => {
    const { wrapper } = createWrapper();
    const posts = installStop(() => ({ disposition: "signalling" }));
    const { result } = renderHook(() => useController(transcript("complete")), {
      wrapper,
    });
    act(() => result.current.onStop());
    expect(result.current.stopState).toBeNull();
    expect(posts).toHaveLength(0);
  });
});
