/**
 * The real `WebSocketManager` handling of `chat.activity` hints and reconnect
 * reconciliation (chat g5.chat.activity.refresh): a hint re-reads the
 * workspace's session list, in every scope variant, and the named chat's
 * detail, and leaves transcripts alone; a reconnect re-reads the list too.
 */

import { describe, expect, it, vi } from "vitest";
import { QueryClient } from "@tanstack/react-query";
import { WebSocketManager, type WSEvent } from "./ws";

function makeManager() {
  const queryClient = new QueryClient();
  const invalidate = vi.spyOn(queryClient, "invalidateQueries");
  const manager = new WebSocketManager({
    getBaseUrl: () => "ws://test",
    getWsTicket: () => Promise.resolve("ticket"),
    queryClient,
  });
  const internals = manager as unknown as {
    handleEvent: (event: WSEvent) => void;
    channelRefCounts: Map<string, number>;
    invalidateAllSubscribedCaches: () => void;
  };
  const invalidatedKeys = () =>
    invalidate.mock.calls.map(([filters]) => filters?.queryKey);
  return { internals, invalidate, invalidatedKeys, queryClient };
}

describe("chat.activity hints", () => {
  it("re-reads the workspace's session list and the named chat's detail", () => {
    const { internals, invalidate, invalidatedKeys } = makeManager();
    internals.handleEvent({
      type: "chat.activity",
      workspaceId: "ws-1",
      sessionId: "chat-1",
    });
    expect(invalidatedKeys()).toEqual([
      ["chat-sessions", "ws-1"],
      ["chat-session", "chat-1"],
    ]);
    expect(invalidate).toHaveBeenCalledWith({
      queryKey: ["chat-session", "chat-1"],
      exact: true,
    });
    expect(invalidatedKeys()).not.toContainEqual(["chat-messages", "chat-1"]);
  });

  it("reads the session list again when its first read is still out", async () => {
    const { internals, queryClient } = makeManager();
    let reads = 0;
    const queryFn = () => {
      reads += 1;
      return new Promise<never>(() => {});
    };
    // The hint cancels this read; its rejection is expected.
    const firstRead = queryClient
      .fetchQuery({ queryKey: ["chat-sessions", "ws-1"], queryFn })
      .catch(() => {});
    const cancel = vi.spyOn(queryClient, "cancelQueries");
    internals.handleEvent({ type: "chat.activity", workspaceId: "ws-1" });
    await vi.waitFor(() =>
      expect(cancel).toHaveBeenCalledWith({
        queryKey: ["chat-sessions", "ws-1"],
        exact: true,
      }),
    );
    expect(reads).toBe(1);
    await firstRead;
  });

  it("re-reads every chat's detail when several chats changed", () => {
    const { internals, invalidatedKeys } = makeManager();
    internals.handleEvent({ type: "chat.activity", workspaceId: "ws-1" });
    expect(invalidatedKeys()).toEqual([
      ["chat-sessions", "ws-1"],
      ["chat-session"],
    ]);
  });

  it("reconciles the session list after a reconnect", () => {
    const { internals, invalidatedKeys } = makeManager();
    internals.channelRefCounts.set("workspace:ws-1", 1);
    internals.invalidateAllSubscribedCaches();
    expect(invalidatedKeys()).toContainEqual(["chat-sessions", "ws-1"]);
  });
});
