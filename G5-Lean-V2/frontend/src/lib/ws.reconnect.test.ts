/**
 * Regression for issue #603: a websocket reconnect must reconcile chat caches.
 *
 * The reconnect recovery (`invalidateAllSubscribedCaches`) used to run behind a
 * `if (this._connected)` check immediately after `await this.connect()`. Because
 * `_connected` is only set inside the async `onopen` handler, that check was
 * always false on a fresh reconnect, so the recovery never ran and a chat that
 * missed a terminal event while the socket was down (e.g. completed tools) never
 * reconverged. The fix runs the recovery from inside `onopen` (where the socket
 * is actually OPEN), gated to reconnects only. This test drives the real
 * WebSocketManager through that path with a controllable socket.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { QueryClient, QueryObserver } from "@tanstack/react-query";
import { WebSocketManager } from "./ws";
import { setApiToken, tokenPrincipal } from "./api";

class MockSocket {
  static instances: MockSocket[] = [];
  static readonly CONNECTING = 0;
  static readonly OPEN = 1;
  static readonly CLOSING = 2;
  static readonly CLOSED = 3;

  url: string;
  readyState: number = MockSocket.CONNECTING;
  onopen: (() => void) | null = null;
  onclose: ((event: CloseEvent) => void) | null = null;
  onerror: (() => void) | null = null;
  onmessage: ((event: MessageEvent) => void) | null = null;

  constructor(url: string) {
    this.url = url;
    MockSocket.instances.push(this);
  }

  send(): void {}
  close(): void {
    this.readyState = MockSocket.CLOSED;
  }

  fireOpen(): void {
    this.readyState = MockSocket.OPEN;
    this.onopen?.();
  }
  fireClose(code = 1000): void {
    this.readyState = MockSocket.CLOSED;
    this.onclose?.(new CloseEvent("close", { code }));
  }
}

async function flushMicrotasks(): Promise<void> {
  for (let i = 0; i < 5; i++) {
    await Promise.resolve();
  }
}

function invalidatedKeyHeads(
  invalidateQueries: ReturnType<typeof vi.fn>,
): string[] {
  return invalidateQueries.mock.calls
    .map((call) => {
      const arg = call[0] as { queryKey?: unknown[] } | undefined;
      return Array.isArray(arg?.queryKey) ? arg.queryKey[0] : undefined;
    })
    .filter((head): head is string => typeof head === "string");
}

describe("WebSocketManager reconnect chat recovery (#603)", () => {
  beforeEach(() => {
    MockSocket.instances = [];
    vi.stubGlobal("WebSocket", MockSocket as unknown as typeof WebSocket);
    vi.useFakeTimers();
  });

  afterEach(() => {
    setApiToken(null);
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it.each([1000, 4003])(
    "signals lost organization access only for a membership close (code %s)",
    async (code) => {
      const token = `header.${btoa(JSON.stringify({ sub: "user", org: "org-1" }))}.signature`;
      setApiToken(token);
      const queryClient = new QueryClient();
      const changed = vi.fn();
      window.addEventListener("g5:organization-access-changed", changed);
      const manager = new WebSocketManager({
        getBaseUrl: () => "http://localhost",
        getWsTicket: async () => "ticket",
        queryClient,
      });
      try {
        manager.subscribe("org:org-1");
        await flushMicrotasks();
        const socket = MockSocket.instances.at(-1);
        expect(socket).toBeDefined();
        socket?.fireOpen();
        socket?.fireClose(code);
        expect(changed).toHaveBeenCalledTimes(code === 4003 ? 1 : 0);
        if (code === 4003) {
          expect(changed.mock.calls[0][0].detail).toEqual({
            principal: tokenPrincipal(token),
          });
        }
      } finally {
        manager.destroy();
        queryClient.clear();
        window.removeEventListener("g5:organization-access-changed", changed);
      }
    },
  );

  it("binds a late membership close to the principal that requested the ticket", async () => {
    const original = `header.${btoa(JSON.stringify({ sub: "user", org: "org-a" }))}.signature`;
    const next = `header.${btoa(JSON.stringify({ sub: "user", org: "org-b" }))}.signature`;
    setApiToken(original);
    const changed = vi.fn();
    window.addEventListener("g5:organization-access-changed", changed);
    const queryClient = new QueryClient();
    let resolveTicket!: (ticket: string) => void;
    const manager = new WebSocketManager({
      getBaseUrl: () => "http://localhost",
      getWsTicket: () =>
        new Promise((resolve) => {
          resolveTicket = resolve;
        }),
      queryClient,
    });
    try {
      manager.subscribe("org:org-a");
      setApiToken(next);
      resolveTicket("ticket-for-a");
      await flushMicrotasks();
      MockSocket.instances.at(-1)?.fireClose(4003);
      expect(changed).toHaveBeenCalledOnce();
      expect(changed.mock.calls[0][0].detail).toEqual({
        principal: tokenPrincipal(original),
      });
    } finally {
      manager.destroy();
      queryClient.clear();
      window.removeEventListener("g5:organization-access-changed", changed);
    }
  });

  it("invalidates session-timeline on reconnect onopen, but not on the initial connect", async () => {
    const invalidateQueries = vi.fn();
    const queryClient = { invalidateQueries } as unknown as QueryClient;
    const manager = new WebSocketManager({
      getBaseUrl: () => "http://localhost",
      getWsTicket: async () => "ticket",
      queryClient,
    });

    // Subscribing to a chat channel triggers the deferred connection.
    manager.subscribe("chat:session-1");
    await flushMicrotasks();

    const initialSocket = MockSocket.instances.at(-1);
    expect(initialSocket).toBeDefined();
    expect(initialSocket?.onopen).toBeTypeOf("function");

    // Initial connect must NOT run the full reconnect recovery.
    initialSocket?.fireOpen();
    expect(invalidatedKeyHeads(invalidateQueries)).not.toContain(
      "session-timeline",
    );

    invalidateQueries.mockClear();

    // Drop the socket, then let the backoff timer fire to reconnect.
    initialSocket?.fireClose();
    await vi.advanceTimersByTimeAsync(20_000);
    await flushMicrotasks();

    const reconnectSocket = MockSocket.instances.at(-1);
    expect(reconnectSocket).toBeDefined();
    expect(reconnectSocket).not.toBe(initialSocket);

    // The reconnect onopen must reconcile the chat caches (session-timeline et al).
    reconnectSocket?.fireOpen();
    const heads = invalidatedKeyHeads(invalidateQueries);
    expect(heads).toContain("session-timeline");
    expect(heads).toContain("chat-messages");
    expect(heads).toContain("source-control-history");

    manager.destroy();
  });

  it("reads workspace threads and PRs again on reconnect, though their first reads were out", async () => {
    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    const manager = new WebSocketManager({
      getBaseUrl: () => "http://localhost",
      getWsTicket: async () => "ticket",
      queryClient,
    });
    manager.subscribe("workspace:ws-1");
    await flushMicrotasks();
    const initialSocket = MockSocket.instances.at(-1);
    initialSocket?.fireOpen();

    // Lists on screen whose first reads are still out when the socket drops;
    // each read answers when the test says.
    const lists = [
      ["line-comments", "ws-1", "repo-1", "src/main.ts"],
      ["comments", "ws-1", "node-1"],
      ["comment-replies", "comment-1"],
      ["workspace-review-replies", "ws-1", "sug-1"],
      ["workspace-pull-requests", "ws-1", "source"],
      ["workspace-pull-requests", "ws-1", "target"],
    ].map((queryKey) => {
      const answers: Array<(value: string[]) => void> = [];
      const observer = new QueryObserver<string[]>(queryClient, {
        queryKey,
        queryFn: () =>
          new Promise<string[]>((resolve) => {
            answers.push(resolve);
          }),
      });
      const unsubscribe = observer.subscribe(() => {});
      return { observer, answers, unsubscribe };
    });
    await vi.advanceTimersByTimeAsync(1);
    for (const list of lists) expect(list.answers).toHaveLength(1);

    // Thread changes made while the socket was down arrive with no event.
    initialSocket?.fireClose();
    await vi.advanceTimersByTimeAsync(20_000);
    await flushMicrotasks();
    MockSocket.instances.at(-1)?.fireOpen();
    await vi.advanceTimersByTimeAsync(1);

    for (const list of lists) {
      expect(list.answers).toHaveLength(2);
      list.answers[1](["after"]);
    }
    await vi.advanceTimersByTimeAsync(1);
    // The reads from before answer late, and change nothing.
    for (const list of lists) list.answers[0](["before"]);
    await vi.advanceTimersByTimeAsync(1);
    for (const list of lists) {
      expect(list.observer.getCurrentResult().data).toEqual(["after"]);
      list.unsubscribe();
    }

    manager.destroy();
  });

  it("recovers missed PR updates for loaded source and target lists on reconnect", async () => {
    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false, staleTime: Infinity } },
    });
    const manager = new WebSocketManager({
      getBaseUrl: () => "http://localhost",
      getWsTicket: async () => "ticket",
      queryClient,
    });
    let current = ["old"];
    const lists = ["source", "target", "all"].map((role) => {
      const observer = new QueryObserver(queryClient, {
        queryKey: ["workspace-pull-requests", "ws-1", role],
        queryFn: async () => current,
      });
      return { observer, unsubscribe: observer.subscribe(() => {}) };
    });
    manager.subscribe("workspace:ws-1");
    await flushMicrotasks();
    const initial = MockSocket.instances.at(-1);
    initial?.fireOpen();
    await vi.advanceTimersByTimeAsync(1);
    for (const list of lists)
      expect(list.observer.getCurrentResult().data).toEqual(["old"]);

    initial?.fireClose();
    current = ["current"];
    await vi.advanceTimersByTimeAsync(20_000);
    await flushMicrotasks();
    MockSocket.instances.at(-1)?.fireOpen();
    await vi.advanceTimersByTimeAsync(1);
    for (const list of lists) {
      expect(list.observer.getCurrentResult().data).toEqual(["current"]);
      list.unsubscribe();
    }
    manager.destroy();
    queryClient.clear();
  });
});
