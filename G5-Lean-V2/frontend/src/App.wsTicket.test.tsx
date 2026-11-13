import type { ComponentProps } from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { afterEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import { getApiToken, setApiToken } from "./lib/api";
import type { WebSocketManager } from "./lib/ws";
import { createMockUser } from "./test/fixtures";
import { createTestJwt, _activeTestClients } from "./test/render";
import { server } from "./test/msw/server";

vi.mock("./components/execution/WorkspaceTerminalPanel", () => ({
  WorkspaceTerminalPanel: () => <div />,
}));

const captured = vi.hoisted(() => ({ managers: [] as WebSocketManager[] }));
vi.mock("./lib/ws", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./lib/ws")>();
  return {
    ...actual,
    WebSocketManager: class extends actual.WebSocketManager {
      constructor(
        config: ConstructorParameters<typeof actual.WebSocketManager>[0],
      ) {
        super(config);
        captured.managers.push(this);
      }
    },
  };
});
// Isolate the App's real ticket callback and socket manager from the provider's
// independent proactive refresh, which could hide a broken ticket callback.
vi.mock("./hooks/useAuth", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./hooks/useAuth")>();
  return {
    ...actual,
    AuthProvider: ({ children }: ComponentProps<typeof actual.AuthProvider>) =>
      children,
    useAuth: () => ({ user: null, loading: false }),
  };
});

class Socket {
  static instances: Socket[] = [];
  static CONNECTING = 0;
  static OPEN = 1;
  static CLOSED = 3;
  readyState = Socket.CONNECTING;
  onopen: (() => void) | null = null;
  onclose: (() => void) | null = null;
  onerror: (() => void) | null = null;
  onmessage: ((event: MessageEvent) => void) | null = null;
  constructor(readonly url: string) {
    Socket.instances.push(this);
  }
  send() {}
  close() {
    this.readyState = Socket.CLOSED;
  }
}

function mountTicketClient() {
  Socket.instances = [];
  vi.stubGlobal("WebSocket", Socket);
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  _activeTestClients.push(queryClient);
  window.history.replaceState({}, "", "/login");
  render(
    <QueryClientProvider client={queryClient}>
      <App />
    </QueryClientProvider>,
  );
  return captured.managers.at(-1)!;
}

afterEach(() => {
  for (const manager of captured.managers) manager.destroy();
  captured.managers.length = 0;
  setApiToken(null);
  vi.unstubAllGlobals();
});

describe("App WebSocket ticket recovery", () => {
  it("refreshes an expired token and opens the socket with the acquired ticket", async () => {
    const expired = createTestJwt(createMockUser());
    const renewed = `${expired}renewed`;
    const authorizations: (string | null)[] = [];
    let refreshes = 0;
    server.use(
      http.post("*/api/v1/auth/ws-ticket", ({ request }) => {
        const auth = request.headers.get("Authorization");
        authorizations.push(auth);
        return auth === `Bearer ${renewed}`
          ? HttpResponse.json({ data: { ticket: "fresh-ticket" } })
          : HttpResponse.json(
              { error: { code: "UNAUTHORIZED" } },
              { status: 401 },
            );
      }),
      http.post("*/api/v1/auth/refresh", () => {
        refreshes++;
        return HttpResponse.json({ data: { jwt: renewed } });
      }),
    );
    const manager = mountTicketClient();
    setApiToken(expired);
    manager.subscribe("workspace:source");
    await waitFor(() => expect(Socket.instances).toHaveLength(1));
    expect(Socket.instances[0].url).toContain("?token=fresh-ticket");
    expect(authorizations).toEqual([`Bearer ${expired}`, `Bearer ${renewed}`]);
    expect(refreshes).toBe(1);
    expect(getApiToken()).toBe(renewed);
  });

  it("does not bind a ticket refreshed into another organization to the old socket context", async () => {
    const oldToken = createTestJwt(createMockUser());
    const newToken = createTestJwt(
      createMockUser({
        activeOrgId: "other-org",
        orgs: [
          {
            orgId: "other-org",
            orgName: "Other",
            orgSlug: "other",
            role: "member",
          },
        ],
      }),
    );
    const authorizations: (string | null)[] = [];
    server.use(
      http.post("*/api/v1/auth/ws-ticket", ({ request }) => {
        const auth = request.headers.get("Authorization");
        authorizations.push(auth);
        return auth === `Bearer ${oldToken}`
          ? HttpResponse.json(
              { error: { code: "UNAUTHORIZED" } },
              { status: 401 },
            )
          : HttpResponse.json({ data: { ticket: "other-org-ticket" } });
      }),
      http.post("*/api/v1/auth/refresh", () =>
        HttpResponse.json({ data: { jwt: newToken } }),
      ),
    );
    const manager = mountTicketClient();
    setApiToken(oldToken);
    manager.subscribe("org:org-g5labs");
    await manager.connect();
    expect(getApiToken()).toBe(newToken);
    expect(authorizations).toEqual([`Bearer ${oldToken}`]);
    expect(Socket.instances).toHaveLength(0);
    await manager.connect();
    expect(authorizations).toEqual([
      `Bearer ${oldToken}`,
      `Bearer ${newToken}`,
    ]);
    expect(Socket.instances).toHaveLength(1);
    expect(Socket.instances[0].url).toContain("other-org-ticket");
  });

  it.each(["502", "empty ticket"])(
    "backs off after %s without an empty-token handshake, then recovers",
    async (failure) => {
      let requests = 0;
      let available = false;
      server.use(
        http.post("*/api/v1/auth/ws-ticket", () => {
          requests++;
          if (available)
            return HttpResponse.json({ data: { ticket: "recovered-ticket" } });
          return failure === "502"
            ? HttpResponse.text("<html>failed_to_pick_backend</html>", {
                status: 502,
              })
            : HttpResponse.json({ data: { ticket: "" } });
        }),
      );
      const manager = mountTicketClient();
      manager.subscribe("workspace:source");
      // connect() joins the acquisition started by subscribe().
      await manager.connect();
      expect(requests).toBe(1);
      expect(Socket.instances).toHaveLength(0);
      available = true;
      await waitFor(() => expect(Socket.instances).toHaveLength(1));
      expect(requests).toBe(2);
      expect(Socket.instances[0].url).toContain("?token=recovered-ticket");
    },
  );
});
