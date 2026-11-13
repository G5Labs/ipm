import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  post: vi.fn(),
  write: vi.fn(),
  reset: vi.fn(),
}));

vi.mock("../../lib/api", () => ({
  api: { post: mocks.post },
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

vi.mock("@xterm/addon-fit", () => ({
  FitAddon: class FitAddon {
    fit() {}
  },
}));

vi.mock("@xterm/xterm", () => ({
  Terminal: class Terminal {
    cols = 80;
    rows = 24;
    loadAddon() {}
    open() {}
    focus() {}
    onData() {
      return { dispose() {} };
    }
    onKey() {
      return { dispose() {} };
    }
    write(bytes: Uint8Array) {
      mocks.write(bytes);
    }
    reset() {
      mocks.reset();
    }
    dispose() {}
  },
}));

import { WorkspaceTerminalPanel } from "./WorkspaceTerminalPanel";
import { ApiError } from "../../lib/api";

const sockets: FakeWebSocket[] = [];

function controlFrame(message: Record<string, unknown>): string {
  return `${JSON.stringify(message)}\n`;
}

function binaryControlFrame(message: Record<string, unknown>): ArrayBuffer {
  const bytes = new TextEncoder().encode(controlFrame(message));
  const frame = new ArrayBuffer(bytes.byteLength);
  new Uint8Array(frame).set(bytes);
  return frame;
}

class FakeWebSocket {
  static OPEN = 1;
  readyState = FakeWebSocket.OPEN;
  binaryType = "";
  onopen: ((event: Event) => void) | null = null;
  onmessage: ((event: MessageEvent) => void) | null = null;
  onerror: ((event: Event) => void) | null = null;
  onclose: ((event: CloseEvent) => void) | null = null;
  sent: string[] = [];

  constructor(readonly url: string) {
    sockets.push(this);
  }

  send(message: string) {
    this.sent.push(message);
  }

  close() {
    this.readyState = 3;
    this.onclose?.(new CloseEvent("close"));
  }

  openThenDrop() {
    this.onopen?.(new Event("open"));
    this.onmessage?.({
      data: binaryControlFrame({
        type: "opened",
        terminal_id: "terminal-1",
        offset: 0,
      }),
    } as MessageEvent);
    this.close();
  }
}

describe("WorkspaceTerminalPanel reconnect", () => {
  beforeEach(() => {
    sessionStorage.clear();
    sockets.length = 0;
    mocks.post.mockReset().mockResolvedValue({ ticket: "ticket" });
    mocks.write.mockReset();
    mocks.reset.mockReset();
    vi.stubGlobal("WebSocket", FakeWebSocket);
    vi.useFakeTimers();
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it("stops retrying sockets that open then immediately close", async () => {
    await act(async () => {
      render(<WorkspaceTerminalPanel workspaceId="workspace-1" enabled />);
      await Promise.resolve();
    });
    expect(sockets).toHaveLength(1);

    for (let attempt = 0; attempt <= 8; attempt += 1) {
      act(() => sockets[attempt].openThenDrop());
      if (attempt === 0) {
        expect(
          sessionStorage.getItem("g5:workspace-terminal:workspace-1"),
        ).toBe("terminal-1");
      }
      if (attempt < 8) {
        await act(async () => {
          vi.advanceTimersByTime(6_000);
          await Promise.resolve();
        });
        expect(sockets).toHaveLength(attempt + 2);
      }
    }
    expect(
      screen.getByText(/Workspace terminal · Connection failed/),
    ).toBeTruthy();
    await act(async () => {
      vi.advanceTimersByTime(60_000);
      await Promise.resolve();
    });
    expect(sockets).toHaveLength(9);
  });

  it("resets the existing screen before rendering reattached scrollback", async () => {
    await act(async () => {
      render(<WorkspaceTerminalPanel workspaceId="workspace-1" enabled />);
      await Promise.resolve();
    });
    const send = (socket: FakeWebSocket, data: string) => {
      socket.onmessage?.({ data } as MessageEvent);
    };
    const opened = (reattached: boolean) =>
      controlFrame({
        type: "opened",
        terminal_id: "terminal-1",
        reattached,
        offset: 0,
        reset: reattached,
      });
    const output = "existing scrollback";
    act(() => {
      send(sockets[0], opened(false));
      send(sockets[0], output);
    });
    expect(screen.getByText(/Workspace terminal · Connected/)).toBeTruthy();
    expect(mocks.reset).not.toHaveBeenCalled();
    expect(mocks.write).toHaveBeenCalledTimes(1);

    act(() => sockets[0].close());
    await act(async () => {
      vi.advanceTimersByTime(6_000);
      await Promise.resolve();
    });
    expect(sockets).toHaveLength(2);
    expect(new URL(sockets[1].url).searchParams.get("terminal_id")).toBe(
      "terminal-1",
    );
    expect(new URL(sockets[1].url).searchParams.get("offset")).toBe(
      String(new TextEncoder().encode(output).length),
    );
    act(() => {
      send(sockets[1], opened(true));
      send(sockets[1], output);
    });
    expect(mocks.reset).toHaveBeenCalledTimes(1);
    expect(mocks.write).toHaveBeenCalledTimes(2);
    expect(mocks.reset.mock.invocationCallOrder[0]).toBeLessThan(
      mocks.write.mock.invocationCallOrder[1],
    );
  });

  it("keeps rendered history when reattach resumes at its byte offset", async () => {
    await act(async () => {
      render(<WorkspaceTerminalPanel workspaceId="workspace-1" enabled />);
      await Promise.resolve();
    });
    act(() => {
      sockets[0].onmessage?.({
        data: controlFrame({
          type: "opened",
          terminal_id: "terminal-1",
          offset: 0,
        }),
      } as MessageEvent);
      sockets[0].onmessage?.({ data: "history" } as MessageEvent);
      sockets[0].close();
    });
    await act(async () => {
      vi.advanceTimersByTime(6_000);
      await Promise.resolve();
    });
    expect(new URL(sockets[1].url).searchParams.get("offset")).toBe("7");
    act(() => {
      sockets[1].onmessage?.({
        data: controlFrame({
          type: "opened",
          terminal_id: "terminal-1",
          reattached: true,
          offset: 7,
          reset: false,
        }),
      } as MessageEvent);
      sockets[1].onmessage?.({ data: "more" } as MessageEvent);
    });
    expect(mocks.reset).not.toHaveBeenCalled();
    expect(mocks.write).toHaveBeenCalledTimes(2);
    expect(new TextDecoder().decode(mocks.write.mock.calls[1][0])).toBe("more");
  });

  it("reads a split control line and renders output merged after its newline", async () => {
    await act(async () => {
      render(<WorkspaceTerminalPanel workspaceId="workspace-1" enabled />);
      await Promise.resolve();
    });
    act(() => {
      sockets[0].onmessage?.({ data: '{"type":"ope' } as MessageEvent);
    });
    expect(mocks.write).not.toHaveBeenCalled();
    const tail = new TextEncoder().encode(
      'ned","terminal_id":"terminal-1","offset":0}\noutput',
    );
    const binary = new ArrayBuffer(tail.length);
    new Uint8Array(binary).set(tail);
    act(() => {
      sockets[0].onmessage?.({ data: binary } as MessageEvent);
    });
    expect(screen.getByText(/Workspace terminal · Connected/)).toBeTruthy();
    expect(new TextDecoder().decode(mocks.write.mock.calls[0][0])).toBe(
      "output",
    );
  });

  it("reattaches the same PTY after a panel remount and clears rejected identities", async () => {
    let view: ReturnType<typeof render>;
    await act(async () => {
      view = render(
        <WorkspaceTerminalPanel workspaceId="workspace-1" enabled />,
      );
      await Promise.resolve();
    });
    act(() => {
      sockets[0].onmessage?.({
        data: controlFrame({
          type: "opened",
          terminal_id: "terminal-1",
          reattached: false,
          offset: 0,
        }),
      } as MessageEvent);
    });
    expect(sessionStorage.getItem("g5:workspace-terminal:workspace-1")).toBe(
      "terminal-1",
    );

    view!.unmount();
    expect(sockets[0].readyState).toBe(3);
    expect(sockets[0].sent).not.toContain(JSON.stringify({ type: "close" }));

    await act(async () => {
      view = render(
        <WorkspaceTerminalPanel workspaceId="workspace-1" enabled />,
      );
      await Promise.resolve();
    });
    expect(new URL(sockets[1].url).searchParams.get("terminal_id")).toBe(
      "terminal-1",
    );
    act(() => {
      sockets[1].onmessage?.({
        data: controlFrame({ type: "rejected" }),
      } as MessageEvent);
    });
    expect(
      sessionStorage.getItem("g5:workspace-terminal:workspace-1"),
    ).toBeNull();
    view!.unmount();

    await act(async () => {
      render(<WorkspaceTerminalPanel workspaceId="workspace-1" enabled />);
      await Promise.resolve();
    });
    expect(new URL(sockets[2].url).searchParams.has("terminal_id")).toBe(false);
  });

  it("connects and retains the PTY in memory when session storage throws", async () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new DOMException("Storage blocked", "SecurityError");
    });
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new DOMException("Storage full", "QuotaExceededError");
    });
    vi.spyOn(Storage.prototype, "removeItem").mockImplementation(() => {
      throw new DOMException("Storage blocked", "SecurityError");
    });
    await act(async () => {
      render(<WorkspaceTerminalPanel workspaceId="workspace-1" enabled />);
      await Promise.resolve();
    });
    expect(sockets).toHaveLength(1);
    act(() => {
      sockets[0].onmessage?.({
        data: controlFrame({
          type: "opened",
          terminal_id: "terminal-1",
          offset: 0,
        }),
      } as MessageEvent);
    });
    expect(screen.getByText(/Workspace terminal · Connected/)).toBeTruthy();
    act(() => sockets[0].close());
    await act(async () => {
      vi.advanceTimersByTime(6_000);
      await Promise.resolve();
    });
    expect(new URL(sockets[1].url).searchParams.get("terminal_id")).toBe(
      "terminal-1",
    );
    act(() => {
      sockets[1].onmessage?.({
        data: controlFrame({ type: "rejected" }),
      } as MessageEvent);
    });
    expect(screen.getByText(/Workspace terminal · Session ended/)).toBeTruthy();
  });

  it("does not retry a rejected session and opens a fresh terminal on request", async () => {
    sessionStorage.setItem("g5:workspace-terminal:workspace-1", "old-terminal");
    await act(async () => {
      render(<WorkspaceTerminalPanel workspaceId="workspace-1" enabled />);
      await Promise.resolve();
    });
    expect(new URL(sockets[0].url).searchParams.get("terminal_id")).toBe(
      "old-terminal",
    );
    act(() => {
      sockets[0].onmessage?.({
        data: binaryControlFrame({
          type: "rejected",
          reason: "terminal_unavailable",
        }),
      } as MessageEvent);
    });
    expect(screen.getByText(/Workspace terminal · Session ended/)).toBeTruthy();
    await act(async () => {
      vi.advanceTimersByTime(60_000);
      await Promise.resolve();
    });
    expect(sockets).toHaveLength(1);
    await act(async () => {
      fireEvent.click(screen.getByText("Open new terminal"));
      await Promise.resolve();
    });
    expect(sockets).toHaveLength(2);
    expect(new URL(sockets[1].url).searchParams.has("terminal_id")).toBe(false);
  });

  it("shows the refusal reason for a rejected secret set and retries on request", async () => {
    await act(async () => {
      render(<WorkspaceTerminalPanel workspaceId="workspace-1" enabled />);
      await Promise.resolve();
    });
    act(() => {
      sockets[0].onmessage?.({
        data: binaryControlFrame({
          type: "rejected",
          reason: "secrets_invalid",
          detail:
            "Runtime Config secret `lower` is not an environment name: use uppercase letters, digits and underscores",
        }),
      } as MessageEvent);
    });
    expect(
      screen.getByText(
        /Workspace terminal · Runtime Config secrets refused: Runtime Config secret `lower` is not an environment name/,
      ),
    ).toBeTruthy();
    // No automatic reconnection: the owner corrects the configuration first.
    await act(async () => {
      vi.advanceTimersByTime(60_000);
      await Promise.resolve();
    });
    expect(sockets).toHaveLength(1);
    await act(async () => {
      fireEvent.click(screen.getByText("Retry"));
      await Promise.resolve();
    });
    expect(sockets).toHaveLength(2);
  });

  it("treats a client error from ticket creation as final", async () => {
    const error = new ApiError({
      code: "TICKET_REFUSED",
      message: "ticket refused",
      status: 400,
    });
    mocks.post.mockRejectedValue(error);
    await act(async () => {
      render(<WorkspaceTerminalPanel workspaceId="workspace-1" enabled />);
      await Promise.resolve();
    });
    expect(
      screen.getByText(/Workspace terminal · Connection failed/),
    ).toBeTruthy();
    await act(async () => {
      vi.advanceTimersByTime(60_000);
      await Promise.resolve();
    });
    expect(mocks.post).toHaveBeenCalledTimes(1);
    expect(sockets).toHaveLength(0);
  });

  it("resets the retry budget only after a stable connection", async () => {
    await act(async () => {
      render(<WorkspaceTerminalPanel workspaceId="workspace-1" enabled />);
      await Promise.resolve();
    });
    for (let attempt = 0; attempt < 7; attempt += 1) {
      act(() => sockets[attempt].openThenDrop());
      await act(async () => {
        vi.advanceTimersByTime(6_000);
        await Promise.resolve();
      });
    }
    act(() => {
      sockets[7].onmessage?.({
        data: controlFrame({
          type: "opened",
          terminal_id: "terminal-1",
          offset: 0,
        }),
      } as MessageEvent);
      vi.advanceTimersByTime(29_000);
      sockets[7].close();
    });
    await act(async () => {
      vi.advanceTimersByTime(6_000);
      await Promise.resolve();
    });
    expect(sockets).toHaveLength(9);
    await act(async () => {
      sockets[8].openThenDrop();
      vi.advanceTimersByTime(60_000);
      await Promise.resolve();
    });
    expect(sockets).toHaveLength(9);
  });

  it("restores the retry budget after a stable connection", async () => {
    await act(async () => {
      render(<WorkspaceTerminalPanel workspaceId="workspace-1" enabled />);
      await Promise.resolve();
    });
    for (let attempt = 0; attempt < 7; attempt += 1) {
      act(() => sockets[attempt].openThenDrop());
      await act(async () => {
        vi.advanceTimersByTime(6_000);
        await Promise.resolve();
      });
    }
    act(() => {
      sockets[7].onmessage?.({
        data: controlFrame({
          type: "opened",
          terminal_id: "terminal-1",
          offset: 0,
        }),
      } as MessageEvent);
      vi.advanceTimersByTime(30_001);
      sockets[7].close();
    });
    await act(async () => {
      vi.advanceTimersByTime(6_000);
      await Promise.resolve();
    });
    expect(sockets).toHaveLength(9);
    act(() => sockets[8].openThenDrop());
    await act(async () => {
      vi.advanceTimersByTime(6_000);
      await Promise.resolve();
    });
    expect(sockets).toHaveLength(10);
  });
});
