import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { api } from "../../lib/api";
import { WorkspaceTerminalPanel } from "./WorkspaceTerminalPanel";
import { WorkspaceLiveGateProvider } from "../../lib/workspaceLiveGate";
import { WORKSPACE_START_EVENT } from "../workspace/reviewWorkspace";

vi.mock("../../lib/api", () => ({
  api: { post: vi.fn() },
  ApiError: class ApiError extends Error {
    isClientError = false;
  },
}));
vi.mock("@xterm/xterm", () => ({
  Terminal: vi.fn(function () {
    return {
      cols: 100,
      rows: 30,
      loadAddon: vi.fn(),
      open: vi.fn(),
      focus: vi.fn(),
      write: vi.fn(),
      reset: vi.fn(),
      dispose: vi.fn(),
      onData: vi.fn(() => ({ dispose: vi.fn() })),
      onKey: vi.fn(() => ({ dispose: vi.fn() })),
    };
  }),
}));
vi.mock("@xterm/addon-fit", () => ({
  FitAddon: vi.fn(function () {
    return { fit: vi.fn() };
  }),
}));

class Socket {
  static OPEN = 1;
  static instances: Socket[] = [];
  readyState = Socket.OPEN;
  onopen: (() => void) | null = null;
  onmessage: ((event: { data: unknown }) => void) | null = null;
  onerror: (() => void) | null = null;
  onclose: (() => void) | null = null;
  open() {
    this.onopen?.();
    this.onmessage?.({
      data: `${JSON.stringify({ type: "opened", terminal_id: "terminal-1", offset: 0 })}\n`,
    });
  }
  send = vi.fn();
  close = vi.fn();
  constructor(public url: string) {
    Socket.instances.push(this);
  }
}
const originalResizeObserver = globalThis.ResizeObserver;
let resize: () => void;
let width = 800;
let height = 600;

beforeEach(() => {
  vi.clearAllMocks();
  sessionStorage.clear();
  Socket.instances = [];
  width = 800;
  height = 600;
  vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockImplementation(
    () => width,
  );
  vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockImplementation(
    () => height,
  );
  vi.stubGlobal("WebSocket", Socket);
  globalThis.ResizeObserver = class {
    constructor(callback: ResizeObserverCallback) {
      resize = () => callback([], this);
    }
    observe = vi.fn();
    unobserve = vi.fn();
    disconnect = vi.fn();
  };
  vi.mocked(api.post).mockResolvedValue({ ticket: "ticket" });
});

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  globalThis.ResizeObserver = originalResizeObserver;
});

describe("WorkspaceTerminalPanel connection lifetime", () => {
  it("distinguishes keyboard/paste data from emulator replies and ignores empty input", async () => {
    render(<WorkspaceTerminalPanel workspaceId="ws-input" enabled />);
    await waitFor(() => expect(Socket.instances).toHaveLength(1));
    const terminal = vi.mocked(Terminal).mock.results[0].value;
    const input = terminal.onData.mock.calls[0][0] as (data: string) => void;
    const socket = Socket.instances[0];
    act(() => socket.open());
    socket.send.mockClear();
    input("");
    expect(socket.send).not.toHaveBeenCalled();
    input("\x1b[0n");
    expect(socket.send).toHaveBeenLastCalledWith(
      JSON.stringify({ type: "response", data: "\x1b[0n" }),
    );
    for (const type of ["keydown", "keypress", "paste", "input"]) {
      vi.stubGlobal("event", {
        isTrusted: true,
        type,
        target: screen.getByTestId("workspace-terminal"),
      });
      input("typed or pasted");
      expect(socket.send).toHaveBeenLastCalledWith(
        JSON.stringify({ type: "input", data: "typed or pasted" }),
      );
    }
    vi.stubGlobal("event", {
      isTrusted: false,
      type: "paste",
      target: screen.getByTestId("workspace-terminal"),
    });
    input("synthetic");
    expect(socket.send).toHaveBeenLastCalledWith(
      JSON.stringify({ type: "response", data: "synthetic" }),
    );
    vi.stubGlobal("event", undefined);
    const key = terminal.onKey.mock.calls[0][0];
    key({ key: "k", domEvent: { isTrusted: true } });
    input("k");
    expect(socket.send).toHaveBeenLastCalledWith(
      JSON.stringify({ type: "input", data: "k" }),
    );
    input("k");
    expect(socket.send).toHaveBeenLastCalledWith(
      JSON.stringify({ type: "response", data: "k" }),
    );
    key({ key: "k", domEvent: { isTrusted: false } });
    input("k");
    expect(socket.send).toHaveBeenLastCalledWith(
      JSON.stringify({ type: "response", data: "k" }),
    );
  });

  it("keeps its PTY on tab switches and ignores hidden dimensions, then fits on return", async () => {
    const { rerender, unmount } = render(
      <WorkspaceTerminalPanel workspaceId="ws-a" enabled visible />,
    );
    await waitFor(() => expect(Socket.instances).toHaveLength(1));
    const socket = Socket.instances[0];
    const terminal = vi.mocked(Terminal).mock.results[0].value;
    const fit = vi.mocked(FitAddon).mock.results[0].value.fit;
    act(() => socket.open());
    expect(screen.getByText(/Connected/)).toBeInTheDocument();
    fit.mockClear();
    socket.send.mockClear();
    width = 0;
    height = 0;
    rerender(
      <WorkspaceTerminalPanel workspaceId="ws-a" enabled visible={false} />,
    );
    act(() => resize());
    expect(fit).not.toHaveBeenCalled();
    expect(socket.send).not.toHaveBeenCalled();
    expect(socket.close).not.toHaveBeenCalled();
    width = 1000;
    height = 700;
    rerender(<WorkspaceTerminalPanel workspaceId="ws-a" enabled visible />);
    expect(fit).toHaveBeenCalledOnce();
    expect(terminal.focus).toHaveBeenCalled();
    expect(socket.send).toHaveBeenCalledWith(
      JSON.stringify({ type: "resize", cols: 100, rows: 30 }),
    );
    expect(api.post).toHaveBeenCalledOnce();
    unmount();
    // Unmount detaches the transport; the retained PTY can be reattached after
    // a browser reload or central-panel remount.
    expect(socket.send).not.toHaveBeenCalledWith(
      JSON.stringify({ type: "close" }),
    );
    expect(socket.close).toHaveBeenCalledOnce();
    expect(terminal.dispose).toHaveBeenCalledOnce();
  });

  it("discards delayed tickets after close or workspace replacement", async () => {
    let resolveOld!: (ticket: { ticket: string }) => void;
    vi.mocked(api.post).mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          resolveOld = resolve;
        }),
    );
    const { rerender, unmount } = render(
      <WorkspaceTerminalPanel workspaceId="old" enabled />,
    );
    await waitFor(() => expect(api.post).toHaveBeenCalledOnce());
    rerender(<WorkspaceTerminalPanel workspaceId="new" enabled />);
    await waitFor(() => expect(Socket.instances).toHaveLength(1));
    expect(Socket.instances[0].url).toContain("/workspaces/new/terminal?");
    await act(async () => resolveOld({ ticket: "obsolete" }));
    expect(Socket.instances).toHaveLength(1);
    unmount();
    expect(Socket.instances[0].close).toHaveBeenCalledOnce();
  });

  it("offers the reason and the header's Start while the owner's VM is stopped (g5.workspace.ui.unavailable)", () => {
    const onStart = vi.fn();
    window.addEventListener(WORKSPACE_START_EVENT, onStart);
    render(
      <WorkspaceLiveGateProvider
        value={{
          kind: "stopped",
          canWrite: true,
          live: false,
          canStart: true,
          reviewCopy: false,
          owner: true,
          reviewIdentityKnown: true,
          workspaceNoun: "the workspace",
        }}
      >
        <WorkspaceTerminalPanel workspaceId="ws-a" enabled={false} />
      </WorkspaceLiveGateProvider>,
    );
    expect(
      screen.getByTestId("workspace-terminal-start-hint"),
    ).toHaveTextContent("Start the workspace to open a terminal.");
    fireEvent.click(screen.getByTestId("workspace-terminal-start-hint-start"));
    expect(onStart).toHaveBeenCalledTimes(1);
    window.removeEventListener(WORKSPACE_START_EVENT, onStart);
  });

  it("offers Start, not a policy change, to a read-only owner whose VM stopped (g5.workspace.ui.gates)", () => {
    const onStart = vi.fn();
    window.addEventListener(WORKSPACE_START_EVENT, onStart);
    render(
      <WorkspaceLiveGateProvider
        value={{
          kind: "read_only",
          vmKind: "stopped",
          canWrite: false,
          live: false,
          canStart: false,
          reviewCopy: false,
          owner: true,
          reviewIdentityKnown: true,
          workspaceNoun: "the workspace",
        }}
      >
        <WorkspaceTerminalPanel workspaceId="ws-a" enabled={false} />
      </WorkspaceLiveGateProvider>,
    );
    expect(
      screen.getByTestId("workspace-terminal-start-hint"),
    ).toHaveTextContent("Start the workspace to open a terminal.");
    fireEvent.click(screen.getByTestId("workspace-terminal-start-hint-start"));
    expect(onStart).toHaveBeenCalledTimes(1);
    window.removeEventListener(WORKSPACE_START_EVENT, onStart);
  });

  it("names the intermediate state for a read-only owner without offering Start (g5.workspace.ui.vm-state)", () => {
    render(
      <WorkspaceLiveGateProvider
        value={{
          kind: "read_only",
          vmKind: "starting",
          canWrite: false,
          live: false,
          canStart: false,
          reviewCopy: false,
          owner: true,
          reviewIdentityKnown: true,
          workspaceNoun: "the workspace",
        }}
      >
        <WorkspaceTerminalPanel workspaceId="ws-a" enabled={false} />
      </WorkspaceLiveGateProvider>,
    );
    expect(
      screen.getByTestId("workspace-terminal-start-hint"),
    ).toHaveTextContent(
      "The workspace is starting; you can open a terminal once it is running.",
    );
    expect(
      screen.queryByTestId("workspace-terminal-start-hint-start"),
    ).toBeNull();
  });

  it("cleans up a stopped VM and ignores events from its socket after restarting", async () => {
    const { rerender } = render(
      <WorkspaceTerminalPanel workspaceId="ws-a" enabled />,
    );
    await waitFor(() => expect(Socket.instances).toHaveLength(1));
    const oldSocket = Socket.instances[0];
    const oldTerminal = vi.mocked(Terminal).mock.results[0].value;
    rerender(<WorkspaceTerminalPanel workspaceId="ws-a" enabled={false} />);
    expect(
      screen.getByText("Start the workspace to open a terminal."),
    ).toBeInTheDocument();
    expect(oldSocket.close).toHaveBeenCalledOnce();
    rerender(<WorkspaceTerminalPanel workspaceId="ws-a" enabled />);
    await waitFor(() => expect(Socket.instances).toHaveLength(2));
    act(() => Socket.instances[1].open());
    act(() => {
      oldSocket.onclose?.();
      oldSocket.onerror?.();
      oldSocket.onmessage?.({ data: "obsolete output" });
    });
    expect(screen.getByText(/Connected/)).toBeInTheDocument();
    expect(oldTerminal.write).not.toHaveBeenCalled();
  });

  it("does not request a terminal ticket without an owner-running attachment", () => {
    render(<WorkspaceTerminalPanel workspaceId="ws-a" enabled={false} />);
    expect(api.post).not.toHaveBeenCalled();
    expect(Terminal).not.toHaveBeenCalled();
  });
});
