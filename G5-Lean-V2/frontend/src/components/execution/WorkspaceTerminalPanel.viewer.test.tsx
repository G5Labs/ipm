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
  scrollToBottom: vi.fn(),
  onData: vi.fn(),
  options: [] as Array<Record<string, unknown>>,
  scrollHandler: null as ((position: number) => void) | null,
  viewportY: 0,
  baseY: 0,
  deferWrites: false,
  pendingWrites: [] as Array<() => void>,
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
    buffer = {
      active: {
        get viewportY() {
          return mocks.viewportY;
        },
        get baseY() {
          return mocks.baseY;
        },
      },
    };
    constructor(options: Record<string, unknown>) {
      mocks.options.push(options);
    }
    loadAddon() {}
    open() {}
    focus() {}
    onData() {
      mocks.onData();
      return { dispose() {} };
    }
    onScroll(handler: (position: number) => void) {
      mocks.scrollHandler = handler;
      return {
        dispose() {
          mocks.scrollHandler = null;
        },
      };
    }
    write(bytes: Uint8Array, callback?: () => void) {
      mocks.write(bytes);
      if (mocks.deferWrites && callback) {
        mocks.pendingWrites.push(callback);
        return;
      }
      callback?.();
    }
    reset() {
      mocks.reset();
    }
    scrollToBottom() {
      mocks.scrollToBottom();
    }
    dispose() {}
  },
}));

import { WorkspaceTerminalPanel } from "./WorkspaceTerminalPanel";
import { viewerStateText, type TerminalViewerState } from "./terminalViewer";

const sockets: FakeWebSocket[] = [];

function frame(message: Record<string, unknown>): string {
  return `${JSON.stringify(message)}\n`;
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

  receive(data: string) {
    this.onmessage?.({ data } as MessageEvent);
  }
}

async function renderViewer(
  onViewerState?: (state: TerminalViewerState) => void,
) {
  await act(async () => {
    render(
      <WorkspaceTerminalPanel
        workspaceId="workspace-1"
        enabled
        readOnly
        terminalId="preview:task-1"
        sessionLabel="preview · workspace default"
        onViewerState={onViewerState}
      />,
    );
    await Promise.resolve();
  });
}

describe("WorkspaceTerminalPanel read-only viewer (g5.preview.run-log)", () => {
  beforeEach(() => {
    sessionStorage.clear();
    sockets.length = 0;
    mocks.post.mockReset().mockResolvedValue({ ticket: "ticket" });
    mocks.write.mockReset();
    mocks.reset.mockReset();
    mocks.scrollToBottom.mockReset();
    mocks.onData.mockReset();
    mocks.options.length = 0;
    mocks.scrollHandler = null;
    mocks.viewportY = 0;
    mocks.baseY = 0;
    mocks.deferWrites = false;
    mocks.pendingWrites.length = 0;
    vi.stubGlobal("WebSocket", FakeWebSocket);
    vi.useFakeTimers();
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it("attaches to the named session read-only: no stdin, a badge, the session name and the retained figure", async () => {
    const states: TerminalViewerState[] = [];
    await renderViewer((state) => states.push(state));
    expect(sockets).toHaveLength(1);
    expect(new URL(sockets[0].url).searchParams.get("terminal_id")).toBe(
      "preview:task-1",
    );
    expect(mocks.options[0]).toMatchObject({
      disableStdin: true,
      cursorBlink: false,
    });
    expect(mocks.onData).not.toHaveBeenCalled();
    expect(
      screen.getByTestId("workspace-terminal-readonly-badge"),
    ).toHaveTextContent("read-only");
    expect(
      screen.getByTestId("workspace-terminal-session-name"),
    ).toHaveTextContent("preview · workspace default");
    act(() => {
      sockets[0].receive(
        frame({
          type: "opened",
          terminal_id: "preview:task-1",
          reattached: false,
          offset: 0,
          reset: true,
        }),
      );
      sockets[0].receive("x".repeat(2048));
      vi.advanceTimersByTime(300);
    });
    expect(
      screen.getByTestId("workspace-terminal-viewer-status"),
    ).toHaveTextContent("attached and following");
    expect(screen.getByTestId("workspace-terminal-retained")).toHaveTextContent(
      "2 KiB of 4 MiB retained",
    );
    expect(screen.getByTestId("workspace-terminal-viewer")).toHaveAttribute(
      "data-viewer-state",
      "attached",
    );
    // The session's close detaches only: no protocol close is ever sent and
    // the panel never sends input.
    expect(
      sockets[0].sent.map((m) => (JSON.parse(m) as { type: string }).type),
    ).toEqual(expect.arrayContaining(["resize"]));
    expect(sockets[0].sent.some((m) => m.includes('"input"'))).toBe(false);
    expect(states.at(-1)).toMatchObject({
      status: "Connected",
      rejected: false,
    });
  });

  it("clears and redraws on reset with the faint earlier-output line, and reports session ended on the closed line", async () => {
    await renderViewer();
    act(() => {
      sockets[0].receive(
        frame({
          type: "opened",
          terminal_id: "preview:task-1",
          reattached: true,
          offset: 10_000,
          reset: true,
        }),
      );
      sockets[0].receive("retained tail");
    });
    expect(mocks.reset).toHaveBeenCalledTimes(1);
    const written = mocks.write.mock.calls.map(([bytes]) =>
      new TextDecoder().decode(bytes as Uint8Array),
    );
    expect(written[0]).toContain(
      "earlier output is beyond the retained window",
    );
    expect(written[1]).toBe("retained tail");
    expect(mocks.reset.mock.invocationCallOrder[0]).toBeLessThan(
      mocks.write.mock.invocationCallOrder[0],
    );
    act(() => {
      sockets[0].receive(frame({ type: "closed", reason: "session_ended" }));
      sockets[0].close();
    });
    expect(
      screen.getByTestId("workspace-terminal-viewer-status"),
    ).toHaveTextContent("session ended · retained");
    expect(screen.getByTestId("workspace-terminal-viewer")).toHaveAttribute(
      "data-viewer-state",
      "ended",
    );
    expect(mocks.write).toHaveBeenCalledTimes(2);
    await act(async () => {
      vi.advanceTimersByTime(60_000);
      await Promise.resolve();
    });
    expect(sockets).toHaveLength(1);
    expect(screen.queryByTestId("workspace-terminal-retry")).toBeNull();
  });

  it("stops following when the reader scrolls up and resumes from the button", async () => {
    await renderViewer();
    act(() => {
      sockets[0].receive(
        frame({
          type: "opened",
          terminal_id: "preview:task-1",
          offset: 0,
          reset: true,
        }),
      );
      sockets[0].receive("line\n");
    });
    expect(mocks.scrollToBottom).toHaveBeenCalledTimes(1);
    expect(screen.queryByTestId("workspace-terminal-follow")).toBeNull();
    act(() => {
      mocks.baseY = 100;
      mocks.viewportY = 40;
      mocks.scrollHandler?.(40);
    });
    expect(
      screen.getByTestId("workspace-terminal-viewer-status"),
    ).toHaveTextContent("attached · follow paused");
    act(() => {
      sockets[0].receive("more\n");
    });
    expect(mocks.scrollToBottom).toHaveBeenCalledTimes(1);
    act(() => {
      screen.getByTestId("workspace-terminal-follow").click();
    });
    expect(mocks.scrollToBottom).toHaveBeenCalledTimes(2);
    expect(
      screen.getByTestId("workspace-terminal-viewer-status"),
    ).toHaveTextContent("attached and following");
  });

  it("keeps following through xterm's own scrolls while a large write is still being parsed", async () => {
    await renderViewer();
    mocks.deferWrites = true;
    act(() => {
      sockets[0].receive(
        frame({
          type: "opened",
          terminal_id: "preview:task-1",
          reattached: true,
          offset: 10_000,
          reset: true,
        }),
      );
      sockets[0].receive("a".repeat(4096));
    });
    // Mid-write the viewport trails the base: not the reader's doing.
    act(() => {
      mocks.baseY = 200;
      mocks.viewportY = 120;
      mocks.scrollHandler?.(120);
    });
    expect(
      screen.getByTestId("workspace-terminal-viewer-status"),
    ).toHaveTextContent("attached and following");
    act(() => {
      mocks.viewportY = 200;
      for (const done of mocks.pendingWrites.splice(0)) done();
    });
    expect(mocks.scrollToBottom).toHaveBeenCalled();
    // With nothing in flight, the same trailing viewport is a real scroll-up.
    act(() => {
      mocks.viewportY = 120;
      mocks.scrollHandler?.(120);
    });
    expect(
      screen.getByTestId("workspace-terminal-viewer-status"),
    ).toHaveTextContent("attached · follow paused");
  });

  it("pauses following on the reader's own wheel even while a write is in flight", async () => {
    await renderViewer();
    mocks.deferWrites = true;
    act(() => {
      sockets[0].receive(
        frame({
          type: "opened",
          terminal_id: "preview:task-1",
          reattached: false,
          offset: 0,
          reset: false,
        }),
      );
      sockets[0].receive("b".repeat(4096));
    });
    // A noisy build keeps writes in flight; the reader's wheel-up is still
    // theirs and pauses following, so the finished write does not snap the
    // viewport back to the bottom.
    act(() => {
      fireEvent.wheel(screen.getByTestId("workspace-terminal"), {
        deltaY: -120,
      });
    });
    expect(
      screen.getByTestId("workspace-terminal-viewer-status"),
    ).toHaveTextContent("attached · follow paused");
    mocks.scrollToBottom.mockReset();
    act(() => {
      for (const done of mocks.pendingWrites.splice(0)) done();
    });
    expect(mocks.scrollToBottom).not.toHaveBeenCalled();
    // A wheel-down is not a pause; the button restores following.
    act(() => {
      fireEvent.wheel(screen.getByTestId("workspace-terminal"), {
        deltaY: 120,
      });
    });
    expect(
      screen.getByTestId("workspace-terminal-viewer-status"),
    ).toHaveTextContent("attached · follow paused");
    fireEvent.click(screen.getByRole("button", { name: "Follow output" }));
    expect(
      screen.getByTestId("workspace-terminal-viewer-status"),
    ).toHaveTextContent("attached and following");
    expect(mocks.scrollToBottom).toHaveBeenCalledTimes(1);
  });

  it("reports a refused attach and shows the state in words", async () => {
    const states: TerminalViewerState[] = [];
    await renderViewer((state) => states.push(state));
    act(() => {
      sockets[0].receive(
        frame({ type: "rejected", reason: "terminal_unavailable" }),
      );
    });
    expect(states.at(-1)).toMatchObject({ rejected: true });
    expect(
      screen.getByTestId("workspace-terminal-viewer-status"),
    ).toHaveTextContent("session unavailable");
    expect(
      viewerStateText(
        "Reconnecting",
        {
          offset: 512,
          received: 0,
          ended: false,
          superseded: false,
          rejected: false,
        },
        true,
      ),
    ).toBe("reconnecting at offset 512");
  });

  it("tells a superseded viewer so, keeps its output and takes the session back at its own offset on the same surface (g5.preview.output.live-tail.viewer)", async () => {
    const states: TerminalViewerState[] = [];
    await renderViewer((state) => states.push(state));
    act(() => {
      sockets[0].receive(
        `${frame({
          type: "opened",
          terminal_id: "preview:task-1",
          reattached: false,
          offset: 0,
          reset: true,
        })}build output`,
      );
    });
    expect(mocks.write).toHaveBeenCalledTimes(1);
    // A second window attached: the guest ends this stream with the reason.
    act(() => {
      sockets[0].receive(frame({ type: "closed", reason: "superseded" }));
      sockets[0].close();
    });
    expect(states.at(-1)).toMatchObject({ superseded: true, ended: false });
    expect(screen.getByTestId("workspace-terminal-viewer")).toHaveAttribute(
      "data-viewer-state",
      "superseded",
    );
    expect(
      screen.getByTestId("workspace-terminal-viewer-status"),
    ).toHaveTextContent("viewing in another window");
    // No reconnect: that would supersede the other window in turn.
    await act(async () => {
      vi.advanceTimersByTime(10_000);
      await Promise.resolve();
    });
    expect(sockets).toHaveLength(1);
    expect(screen.queryByTestId("workspace-terminal-follow")).toBeNull();
    expect(screen.queryByTestId("workspace-terminal-retry")).toBeNull();
    // The output stays on screen: beyond the first attach's redraw the
    // surface was neither reset nor rebuilt.
    expect(mocks.reset).toHaveBeenCalledTimes(1);
    const terminals = mocks.options.length;

    await act(async () => {
      fireEvent.click(screen.getByTestId("workspace-terminal-take-over"));
      await Promise.resolve();
    });
    expect(sockets).toHaveLength(2);
    expect(new URL(sockets[1].url).searchParams.get("offset")).toBe("12");
    expect(mocks.options.length).toBe(terminals);
    act(() => {
      sockets[1].receive(
        `${frame({
          type: "opened",
          terminal_id: "preview:task-1",
          reattached: true,
          offset: 12,
          reset: false,
        })} more`,
      );
    });
    expect(screen.getByTestId("workspace-terminal-viewer")).toHaveAttribute(
      "data-viewer-state",
      "attached",
    );
    expect(mocks.reset).toHaveBeenCalledTimes(1);
    expect(
      screen.getByTestId("workspace-terminal-viewer-status"),
    ).toHaveTextContent("attached and following");
    expect(screen.queryByTestId("workspace-terminal-take-over")).toBeNull();
  });

  it("holds the attach with the surface mounted and the parent's words, then attaches on the same surface when released (g5.preview.run-log.attach)", async () => {
    const states: TerminalViewerState[] = [];
    const view = (attach: boolean) => (
      <WorkspaceTerminalPanel
        workspaceId="workspace-1"
        enabled
        readOnly
        attach={attach}
        pendingStateText="waiting for the application to start…"
        terminalId="preview:task-1"
        onViewerState={(state) => states.push(state)}
      />
    );
    let rerender!: (ui: React.ReactElement) => void;
    await act(async () => {
      ({ rerender } = render(view(false)));
      await Promise.resolve();
    });
    // The surface exists; no socket does.
    expect(screen.getByTestId("workspace-terminal")).toBeInTheDocument();
    expect(mocks.options).toHaveLength(1);
    expect(sockets).toHaveLength(0);
    expect(screen.getByTestId("workspace-terminal-viewer")).toHaveAttribute(
      "data-viewer-state",
      "waiting",
    );
    expect(
      screen.getByTestId("workspace-terminal-viewer-status"),
    ).toHaveTextContent("waiting for the application to start…");
    const host = screen.getByTestId("workspace-terminal");

    await act(async () => {
      rerender(view(true));
      await Promise.resolve();
    });
    expect(sockets).toHaveLength(1);
    expect(new URL(sockets[0].url).searchParams.get("terminal_id")).toBe(
      "preview:task-1",
    );
    // A refusal is reported to the parent; the words stay the parent's.
    act(() => {
      sockets[0].receive(
        frame({ type: "rejected", reason: "terminal_unavailable" }),
      );
    });
    expect(states.at(-1)).toMatchObject({ rejected: true });
    expect(
      screen.getByTestId("workspace-terminal-viewer-status"),
    ).toHaveTextContent("waiting for the application to start…");
    // Held again, then released: a fresh attachment on the same xterm.
    await act(async () => {
      rerender(view(false));
      await Promise.resolve();
    });
    expect(states.at(-1)).toMatchObject({ rejected: false });
    await act(async () => {
      rerender(view(true));
      await Promise.resolve();
    });
    expect(sockets).toHaveLength(2);
    expect(mocks.options).toHaveLength(1);
    expect(screen.getByTestId("workspace-terminal")).toBe(host);
    act(() => {
      sockets[1].receive(
        frame({
          type: "opened",
          terminal_id: "preview:task-1",
          reattached: false,
          offset: 0,
          reset: true,
        }),
      );
    });
    expect(screen.getByTestId("workspace-terminal-viewer")).toHaveAttribute(
      "data-viewer-state",
      "attached",
    );
    expect(
      screen.getByTestId("workspace-terminal-viewer-status"),
    ).toHaveTextContent("attached and following");
  });

  it("says output is not retained when the VM is not running", () => {
    render(
      <WorkspaceTerminalPanel
        workspaceId="workspace-1"
        enabled={false}
        readOnly
        terminalId="preview:task-1"
      />,
    );
    expect(screen.getByTestId("workspace-terminal-disabled")).toHaveTextContent(
      "Output is not retained after the VM stops.",
    );
    expect(sockets).toHaveLength(0);
  });
});
