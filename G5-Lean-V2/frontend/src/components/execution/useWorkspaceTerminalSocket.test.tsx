import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { api, ApiError } from "../../lib/api";
import {
  useWorkspaceTerminalSocket,
  type WorkspaceTerminalPort,
} from "./useWorkspaceTerminalSocket";

vi.mock("../../lib/api", () => ({
  api: { post: vi.fn() },
  ApiError: class ApiError extends Error {
    status: number;
    constructor(params: { message: string; status: number }) {
      super(params.message);
      this.status = params.status;
    }
    get isClientError() {
      return this.status >= 400 && this.status < 500;
    }
  },
}));

const sockets: Socket[] = [];
class Socket {
  static OPEN = 1;
  readyState = Socket.OPEN;
  onopen: (() => void) | null = null;
  onmessage: ((event: { data: unknown }) => void) | null = null;
  onerror: (() => void) | null = null;
  onclose: (() => void) | null = null;
  send = vi.fn();
  constructor(public url: string) {
    sockets.push(this);
  }
  opened() {
    this.onopen?.();
    this.onmessage?.({
      data: `${JSON.stringify({ type: "opened", terminal_id: "terminal-1", offset: 0 })}\n`,
    });
  }
  close() {
    this.onclose?.();
  }
}

function port(generation = 0): WorkspaceTerminalPort {
  return {
    workspaceId: "ws-a",
    generation,
    active: true,
    write: vi.fn(),
    reset: vi.fn(),
    size: () => ({ cols: 80, rows: 24 }),
  };
}

beforeEach(() => {
  vi.useFakeTimers();
  vi.spyOn(Math, "random").mockReturnValue(0.5);
  vi.stubGlobal("WebSocket", Socket);
  vi.mocked(api.post).mockReset();
  vi.mocked(api.post).mockResolvedValue({ ticket: "ticket" });
  sockets.length = 0;
  sessionStorage.clear();
});

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

async function flushTicket() {
  await act(async () => {
    await Promise.resolve();
  });
}

async function advanceReconnect() {
  await act(async () => {
    vi.advanceTimersByTime(6_000);
    await Promise.resolve();
  });
}

describe("useWorkspaceTerminalSocket", () => {
  it("stops after rejected and starts a fresh identity only on restart", async () => {
    sessionStorage.setItem("g5:workspace-terminal:ws-a", "old-terminal");
    let terminalPort = port();
    const hook = renderHook(() =>
      useWorkspaceTerminalSocket({
        workspaceId: "ws-a",
        enabled: true,
        port: terminalPort,
      }),
    );
    await flushTicket();
    expect(new URL(sockets[0].url).searchParams.get("terminal_id")).toBe(
      "old-terminal",
    );
    act(() => {
      sockets[0].onmessage?.({
        data: `${JSON.stringify({ type: "rejected" })}\n`,
      });
    });
    expect(hook.result.current.status).toBe("Session ended");
    await advanceReconnect();
    expect(sockets).toHaveLength(1);
    act(() => hook.result.current.restart());
    terminalPort = port(1);
    hook.rerender();
    await flushTicket();
    expect(sockets).toHaveLength(2);
    expect(new URL(sockets[1].url).searchParams.has("terminal_id")).toBe(false);
    hook.unmount();
  });

  it("reports a refused secret set with its reason and stops reconnecting", async () => {
    const terminalPort = port();
    const hook = renderHook(() =>
      useWorkspaceTerminalSocket({
        workspaceId: "ws-a",
        enabled: true,
        port: terminalPort,
      }),
    );
    await flushTicket();
    act(() => {
      sockets[0].onmessage?.({
        data: `${JSON.stringify({
          type: "rejected",
          reason: "secrets_invalid",
          detail:
            "Runtime Config secret `HOME` is reserved for the platform and cannot be set",
        })}\n`,
      });
    });
    expect(hook.result.current.status).toBe(
      "Runtime Config secrets refused: Runtime Config secret `HOME` is reserved for the platform and cannot be set",
    );
    await advanceReconnect();
    expect(sockets).toHaveLength(1);
    hook.unmount();
  });

  it("treats a 4xx ticket error as final", async () => {
    vi.mocked(api.post).mockRejectedValue(
      new ApiError({
        code: "TICKET_REFUSED",
        message: "refused",
        status: 400,
      }),
    );
    const terminalPort = port();
    const hook = renderHook(() =>
      useWorkspaceTerminalSocket({
        workspaceId: "ws-a",
        enabled: true,
        port: terminalPort,
      }),
    );
    await flushTicket();
    expect(hook.result.current.status).toBe("Connection failed");
    await advanceReconnect();
    expect(api.post).toHaveBeenCalledTimes(1);
    expect(sockets).toHaveLength(0);
    hook.unmount();
  });

  it.each([408, 429])("retries a temporary %i ticket error", async (status) => {
    vi.mocked(api.post).mockRejectedValueOnce(
      new ApiError({ code: "TICKET_UNAVAILABLE", message: "retry", status }),
    );
    const terminalPort = port();
    const hook = renderHook(() =>
      useWorkspaceTerminalSocket({
        workspaceId: "ws-a",
        enabled: true,
        port: terminalPort,
      }),
    );
    await flushTicket();
    expect(hook.result.current.status).toBe("Reconnecting");
    await advanceReconnect();
    expect(api.post).toHaveBeenCalledTimes(2);
    expect(sockets).toHaveLength(1);
    hook.unmount();
  });

  it("caps unstable reconnects at eight attempts", async () => {
    const terminalPort = port();
    const hook = renderHook(() =>
      useWorkspaceTerminalSocket({
        workspaceId: "ws-a",
        enabled: true,
        port: terminalPort,
      }),
    );
    await flushTicket();
    for (let attempt = 0; attempt <= 8; attempt += 1) {
      act(() => sockets[attempt].close());
      await advanceReconnect();
    }
    expect(sockets).toHaveLength(9);
    expect(hook.result.current.status).toBe("Connection failed");
    hook.unmount();
  });

  it("keeps the retry budget until a connection survives the stability window", async () => {
    const terminalPort = port();
    const hook = renderHook(() =>
      useWorkspaceTerminalSocket({
        workspaceId: "ws-a",
        enabled: true,
        port: terminalPort,
      }),
    );
    await flushTicket();
    for (let attempt = 0; attempt < 7; attempt += 1) {
      act(() => sockets[attempt].close());
      await advanceReconnect();
    }
    act(() => {
      sockets[7].opened();
      vi.advanceTimersByTime(29_000);
      sockets[7].close();
    });
    await advanceReconnect();
    act(() => sockets[8].close());
    await advanceReconnect();
    expect(sockets).toHaveLength(9);
    hook.unmount();
  });

  it("restores the retry budget after a stable connection", async () => {
    const terminalPort = port();
    const hook = renderHook(() =>
      useWorkspaceTerminalSocket({
        workspaceId: "ws-a",
        enabled: true,
        port: terminalPort,
      }),
    );
    await flushTicket();
    for (let attempt = 0; attempt < 7; attempt += 1) {
      act(() => sockets[attempt].close());
      await advanceReconnect();
    }
    act(() => {
      sockets[7].opened();
      vi.advanceTimersByTime(30_000);
      sockets[7].close();
    });
    await advanceReconnect();
    act(() => sockets[8].close());
    await advanceReconnect();
    expect(sockets).toHaveLength(10);
    hook.unmount();
  });

  describe("hosted read-only session (g5.preview.output.live-tail)", () => {
    const closedFrame = () =>
      `${JSON.stringify({ type: "closed", reason: "session_ended" })}\n`;

    it("attaches by the given name without touching the shell identity store, drops input and ends on the guest's closed line", async () => {
      sessionStorage.setItem("g5:workspace-terminal:ws-a", "shell-1");
      const terminalPort = port();
      const hook = renderHook(() =>
        useWorkspaceTerminalSocket({
          workspaceId: "ws-a",
          enabled: true,
          port: terminalPort,
          terminalId: "preview:task-1",
          readOnly: true,
        }),
      );
      await flushTicket();
      const url = new URL(sockets[0].url);
      expect(url.searchParams.get("terminal_id")).toBe("preview:task-1");
      expect(url.searchParams.has("offset")).toBe(false);
      act(() => {
        sockets[0].onmessage?.({
          data: `${JSON.stringify({
            type: "opened",
            terminal_id: "preview:task-1",
            reattached: false,
            offset: 0,
            reset: true,
          })}\n`,
        });
      });
      expect(hook.result.current.status).toBe("Connected");
      // A first attach resets an empty screen and writes no "earlier output"
      // line: nothing was evicted.
      expect(terminalPort.reset).toHaveBeenCalledTimes(1);
      expect(terminalPort.write).not.toHaveBeenCalled();
      expect(sessionStorage.getItem("g5:workspace-terminal:ws-a")).toBe(
        "shell-1",
      );
      act(() => {
        hook.result.current.send({ type: "input", data: "rm -rf /\n" });
        hook.result.current.send({ type: "resize", cols: 100, rows: 30 });
      });
      const sentTypes = sockets[0].send.mock.calls.map(
        ([message]) => (JSON.parse(message as string) as { type: string }).type,
      );
      expect(sentTypes).not.toContain("input");
      expect(sentTypes).toContain("resize");

      act(() => {
        sockets[0].onmessage?.({ data: "build output\n" });
      });
      // Byte counts are published a few times a second, not per frame.
      act(() => {
        vi.advanceTimersByTime(300);
      });
      expect(hook.result.current.attachment.offset).toBe(13);
      expect(hook.result.current.attachment.received).toBe(13);

      act(() => {
        sockets[0].onmessage?.({ data: closedFrame() });
      });
      expect(hook.result.current.status).toBe("Session ended");
      expect(hook.result.current.attachment.ended).toBe(true);
      // The closed line is a control, never rendered as output.
      expect(terminalPort.write).toHaveBeenCalledTimes(1);
      act(() => sockets[0].close());
      await advanceReconnect();
      expect(sockets).toHaveLength(1);
      hook.unmount();
    });

    it("redraws from the retained tail on a reset past the window and keeps reconnecting at its offset otherwise", async () => {
      const terminalPort = port();
      const hook = renderHook(() =>
        useWorkspaceTerminalSocket({
          workspaceId: "ws-a",
          enabled: true,
          port: terminalPort,
          terminalId: "preview:task-1",
          readOnly: true,
        }),
      );
      await flushTicket();
      act(() => {
        sockets[0].onmessage?.({
          data: `${JSON.stringify({
            type: "opened",
            terminal_id: "preview:task-1",
            reattached: true,
            offset: 4096,
            reset: true,
          })}\ntail`,
        });
      });
      expect(terminalPort.reset).toHaveBeenCalledTimes(1);
      const written = vi
        .mocked(terminalPort.write)
        .mock.calls.map(([bytes]) => new TextDecoder().decode(bytes));
      expect(written[0]).toContain(
        "earlier output is beyond the retained window",
      );
      expect(written[1]).toBe("tail");
      expect(hook.result.current.attachment.offset).toBe(4100);

      act(() => sockets[0].close());
      expect(hook.result.current.status).toBe("Reconnecting");
      await advanceReconnect();
      expect(sockets).toHaveLength(2);
      expect(new URL(sockets[1].url).searchParams.get("offset")).toBe("4100");
      act(() => {
        sockets[1].onmessage?.({
          data: `${JSON.stringify({
            type: "opened",
            terminal_id: "preview:task-1",
            reattached: true,
            offset: 4100,
            reset: false,
          })}\n`,
        });
      });
      expect(terminalPort.reset).toHaveBeenCalledTimes(1);
      expect(hook.result.current.status).toBe("Connected");
      hook.unmount();
    });

    it("stops on the guest's superseded line and takes the session back at its own offset without a redraw", async () => {
      const terminalPort = port();
      const hook = renderHook(() =>
        useWorkspaceTerminalSocket({
          workspaceId: "ws-a",
          enabled: true,
          port: terminalPort,
          terminalId: "preview:task-1",
          readOnly: true,
        }),
      );
      await flushTicket();
      act(() => {
        sockets[0].onmessage?.({
          data: `${JSON.stringify({
            type: "opened",
            terminal_id: "preview:task-1",
            reattached: false,
            offset: 0,
            reset: true,
          })}\n`,
        });
        sockets[0].onmessage?.({ data: "twenty bytes of log\n" });
      });
      act(() => {
        vi.advanceTimersByTime(300);
      });
      expect(hook.result.current.attachment.offset).toBe(20);
      act(() => {
        sockets[0].onmessage?.({
          data: `${JSON.stringify({ type: "closed", reason: "superseded" })}\n`,
        });
      });
      expect(hook.result.current.status).toBe("Superseded");
      expect(hook.result.current.attachment).toMatchObject({
        superseded: true,
        ended: false,
        rejected: false,
      });
      // The close that follows prompts no reconnect: a reconnect would
      // supersede the newer viewer in turn.
      act(() => sockets[0].close());
      await advanceReconnect();
      expect(sockets).toHaveLength(1);
      // The control line is never rendered.
      expect(terminalPort.write).toHaveBeenCalledTimes(1);

      act(() => hook.result.current.takeOver());
      await flushTicket();
      expect(sockets).toHaveLength(2);
      expect(new URL(sockets[1].url).searchParams.get("offset")).toBe("20");
      expect(hook.result.current.status).toBe("Connecting");
      expect(hook.result.current.attachment.superseded).toBe(false);
      act(() => {
        sockets[1].onmessage?.({
          data: `${JSON.stringify({
            type: "opened",
            terminal_id: "preview:task-1",
            reattached: true,
            offset: 20,
            reset: false,
          })}\n`,
        });
      });
      expect(hook.result.current.status).toBe("Connected");
      // Resumed at the offset: no second reset, the on-screen count continues.
      expect(terminalPort.reset).toHaveBeenCalledTimes(1);
      act(() => {
        vi.advanceTimersByTime(300);
      });
      expect(hook.result.current.attachment.received).toBe(20);
      hook.unmount();
    });

    it("starts each attachment lifetime over: a held attach publishes nothing stale and opens once released", async () => {
      const terminalPort = port();
      let enabled = true;
      const hook = renderHook(() =>
        useWorkspaceTerminalSocket({
          workspaceId: "ws-a",
          enabled,
          port: terminalPort,
          terminalId: "preview:task-1",
          readOnly: true,
        }),
      );
      await flushTicket();
      act(() => {
        sockets[0].onmessage?.({
          data: `${JSON.stringify({ type: "rejected" })}\n`,
        });
      });
      expect(hook.result.current.attachment.rejected).toBe(true);
      enabled = false;
      hook.rerender();
      expect(hook.result.current.status).toBe("Connecting");
      expect(hook.result.current.attachment.rejected).toBe(false);
      await advanceReconnect();
      expect(sockets).toHaveLength(1);
      enabled = true;
      hook.rerender();
      await flushTicket();
      expect(sockets).toHaveLength(2);
      // A fresh attachment, not a resume: no offset is requested.
      expect(new URL(sockets[1].url).searchParams.has("offset")).toBe(false);
      hook.unmount();
    });

    it("treats a refused attach as final: nothing here can create the session", async () => {
      const terminalPort = port();
      const hook = renderHook(() =>
        useWorkspaceTerminalSocket({
          workspaceId: "ws-a",
          enabled: true,
          port: terminalPort,
          terminalId: "preview:gone",
          readOnly: true,
        }),
      );
      await flushTicket();
      act(() => {
        sockets[0].onmessage?.({
          data: `${JSON.stringify({ type: "rejected" })}\n`,
        });
      });
      expect(hook.result.current.status).toBe("Session unavailable");
      expect(hook.result.current.attachment.rejected).toBe(true);
      await advanceReconnect();
      expect(sockets).toHaveLength(1);
      hook.unmount();
    });
  });
});
