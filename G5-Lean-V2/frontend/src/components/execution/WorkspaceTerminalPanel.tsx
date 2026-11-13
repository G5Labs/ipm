import { FitAddon } from "@xterm/addon-fit";
import { Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";
import { useEffect, useRef, useState } from "react";
import {
  SECRETS_REFUSED_STATUS,
  useWorkspaceTerminalSocket,
  type WorkspaceTerminalPort,
} from "./useWorkspaceTerminalSocket";
import {
  StartWorkspaceHint,
  useWorkspaceLiveGate,
} from "../../lib/workspaceLiveGate";
import {
  formatBytes,
  HOSTED_SESSION_RETAINED_BYTES,
  viewerStateText,
  type TerminalViewerState,
} from "./terminalViewer";

interface WorkspaceTerminalPanelProps {
  workspaceId: string;
  enabled: boolean;
  visible?: boolean;
  /**
   * Attach to this named session (`preview:<task id>`) instead of opening a
   * shell. The session must already exist in the guest; a refusal is final.
   */
  terminalId?: string | null;
  /**
   * A viewer: no input reaches the guest, the panel's close only detaches,
   * and output is followed until the reader scrolls up.
   */
  readOnly?: boolean;
  /**
   * Whether to hold an attachment open now (viewer only). False keeps the
   * surface mounted at its size with the socket closed; flipping it true
   * attaches on that same surface, so a parent that waits for the session
   * or retries a refused attach never rebuilds the terminal
   * (`g5.preview.run-log.attach`).
   */
  attach?: boolean;
  /**
   * The header's state words while the parent is still arranging the
   * attachment (held, or refused and about to be retried); the live words
   * take over once the viewer is connected.
   */
  pendingStateText?: string | null;
  /** The header's name for a named session; defaults to the id. */
  sessionLabel?: string;
  /** The guest's retained window for the "X of Y retained" figure. */
  retainedCapacityBytes?: number;
  /** Informs the parent of the viewer's state (attached, ended, refused). */
  onViewerState?: (state: TerminalViewerState) => void;
}

/** Owner-only interactive terminal backed by the workspace VM PTY stream. */
export function WorkspaceTerminalPanel({
  workspaceId,
  enabled,
  visible = true,
  terminalId = null,
  readOnly = false,
  attach = true,
  pendingStateText = null,
  sessionLabel,
  retainedCapacityBytes = HOSTED_SESSION_RETAINED_BYTES,
  onViewerState,
}: WorkspaceTerminalPanelProps) {
  const hostRef = useRef<HTMLDivElement>(null);
  const activateRef = useRef<(() => void) | null>(null);
  const followRef = useRef<(() => void) | null>(null);
  const gate = useWorkspaceLiveGate();
  const [port, setPort] = useState<WorkspaceTerminalPort | null>(null);
  const [following, setFollowing] = useState(true);
  const { status, retryKey, send, restart, takeOver, attachment } =
    useWorkspaceTerminalSocket({
      workspaceId,
      enabled: enabled && attach,
      port,
      terminalId,
      readOnly,
    });

  useEffect(() => {
    if (!onViewerState) return;
    onViewerState({ status, ...attachment });
  }, [onViewerState, status, attachment]);

  useEffect(() => {
    if (!enabled || !hostRef.current) return;
    const terminal = new Terminal({
      convertEol: true,
      cursorBlink: !readOnly,
      disableStdin: readOnly,
      // A build log is long; a shell keeps xterm's default history.
      scrollback: readOnly ? 20_000 : 1_000,
      screenReaderMode: true,
      fontFamily: "var(--g5-font-mono)",
      fontSize: 12,
      theme: {
        background: "#101318",
        foreground: "#e5e7eb",
      },
    });
    const fit = new FitAddon();
    terminal.loadAddon(fit);
    terminal.open(hostRef.current);
    const host = hostRef.current;
    let disposed = false;
    // Follow the newest output until the reader scrolls up to read; the
    // header's button restores following (`g5.preview.run-log`).
    let follow = true;
    // xterm reports its own scrolls while it parses a large write (a 4 MiB
    // replay after a reload) and the viewport can trail the base mid-write;
    // a scroll event alone is the reader's own only with no write in flight.
    // The reader's wheel, touch and keyboard scrolling are known as theirs
    // from the input itself, so they pause following even while a noisy
    // build keeps writes in flight (Codex review of the #693 rebase).
    let writesInFlight = 0;
    const scrollSubscription = readOnly
      ? terminal.onScroll(() => {
          if (writesInFlight > 0) return;
          const buffer = terminal.buffer.active;
          const atBottom = buffer.viewportY >= buffer.baseY;
          if (follow !== atBottom) {
            follow = atBottom;
            setFollowing(atBottom);
          }
        })
      : null;
    const pauseFollowing = () => {
      if (!follow) return;
      follow = false;
      setFollowing(false);
    };
    const onReaderWheel = (event: WheelEvent) => {
      if (event.deltaY < 0) pauseFollowing();
    };
    const onReaderTouch = () => pauseFollowing();
    const onReaderKey = (event: KeyboardEvent) => {
      if (["ArrowUp", "PageUp", "Home"].includes(event.key)) pauseFollowing();
    };
    if (readOnly) {
      host.addEventListener("wheel", onReaderWheel, { passive: true });
      host.addEventListener("touchmove", onReaderTouch, { passive: true });
      host.addEventListener("keydown", onReaderKey);
    }
    followRef.current = readOnly
      ? () => {
          follow = true;
          setFollowing(true);
          terminal.scrollToBottom();
        }
      : null;
    const currentPort: WorkspaceTerminalPort = {
      workspaceId,
      generation: retryKey,
      active: true,
      write: (bytes) => {
        if (readOnly) {
          writesInFlight += 1;
          terminal.write(bytes, () => {
            writesInFlight -= 1;
            if (follow && !disposed) terminal.scrollToBottom();
          });
        } else {
          terminal.write(bytes);
        }
      },
      reset: () => terminal.reset(),
      size: () => ({ cols: terminal.cols, rows: terminal.rows }),
    };
    queueMicrotask(() => {
      if (!disposed) setPort(currentPort);
    });

    // Hidden tabs have zero dimensions. Retain the last usable PTY size until
    // this surface is visible again, then fit the same terminal to the panel.
    const resize = () => {
      if (disposed || !host.clientWidth || !host.clientHeight) return;
      fit.fit();
      send({ type: "resize", cols: terminal.cols, rows: terminal.rows });
    };
    activateRef.current = () => {
      resize();
      if (!readOnly) terminal.focus();
    };
    resize();
    // xterm also emits onData for automatic replies to PTY output. Only
    // data emitted during trusted keyboard/paste/input dispatch is activity.
    // A read-only viewer (a hosted preview session) sends nothing back, so
    // it subscribes to no input at all.
    let pendingKey: string | null = null;
    const keySubscription = readOnly
      ? null
      : terminal.onKey(({ key, domEvent }) => {
          // onKey precedes the matching onData synchronously, including
          // browsers without the legacy window.event property.
          pendingKey = domEvent.isTrusted ? key : null;
        });
    let pendingText: string[] = [];
    let textExpiry: ReturnType<typeof setTimeout> | undefined;
    const beginInput = (event: Event) => {
      // xterm defers IME delivery until after compositionend. Match that
      // exact committed text once; unrelated output replies are responses.
      // Paste also has normalized/bracketed forms in the terminal protocol.
      if (!event.isTrusted) return;
      const text =
        event instanceof ClipboardEvent
          ? event.clipboardData?.getData("text/plain")
          : event instanceof CompositionEvent || event instanceof InputEvent
            ? event.data
            : null;
      pendingText = text ? [text] : [];
      if (text && event.type === "paste") {
        const normalized = text.replace(/\r?\n/g, "\r");
        pendingText.push(normalized, `\x1b[200~${normalized}\x1b[201~`);
      }
      clearTimeout(textExpiry);
      textExpiry = setTimeout(() => {
        pendingText = [];
      }, 1000);
    };
    const inputEvents = readOnly ? [] : ["compositionend", "paste", "input"];
    for (const event of inputEvents)
      host.addEventListener(event, beginInput, true);
    const dataSubscription = readOnly
      ? null
      : terminal.onData((data) => {
          const event = window.event;
          const userInput =
            (event?.isTrusted === true &&
              ["keydown", "keypress", "paste", "input"].includes(event.type) &&
              event.target instanceof Node &&
              host.contains(event.target)) ||
            (pendingKey !== null && data === pendingKey) ||
            pendingText.includes(data);
          pendingKey = null;
          if (pendingText.includes(data)) pendingText = [];
          if (data.length)
            send({ type: userInput ? "input" : "response", data });
        });
    const resizeObserver =
      typeof ResizeObserver === "undefined" ? null : new ResizeObserver(resize);
    resizeObserver?.observe(host);

    return () => {
      disposed = true;
      currentPort.active = false;
      activateRef.current = null;
      followRef.current = null;
      resizeObserver?.disconnect();
      dataSubscription?.dispose();
      scrollSubscription?.dispose();
      if (readOnly) {
        host.removeEventListener("wheel", onReaderWheel);
        host.removeEventListener("touchmove", onReaderTouch);
        host.removeEventListener("keydown", onReaderKey);
      }
      keySubscription?.dispose();
      clearTimeout(textExpiry);
      for (const event of inputEvents)
        host.removeEventListener(event, beginInput, true);
      terminal.dispose();
    };
  }, [enabled, workspaceId, retryKey, send, readOnly, terminalId]);

  useEffect(() => {
    if (visible && enabled) activateRef.current?.();
  }, [visible, enabled, workspaceId, retryKey]);

  if (!enabled) {
    if (readOnly) {
      return (
        <div
          className="flex h-full items-center justify-center text-sm text-[var(--g5-text2)]"
          data-testid="workspace-terminal-disabled"
        >
          Output is not retained after the VM stops.
        </div>
      );
    }
    // The owner reads the state and gets the header's Start beside it
    // (`g5.workspace.ui.unavailable`); a reader reads why not.
    return (
      <div
        className="flex h-full items-center justify-center text-sm text-[var(--g5-text2)]"
        data-testid="workspace-terminal-disabled"
      >
        {gate ? (
          <StartWorkspaceHint
            verb="open a terminal"
            policyFree
            className="inline-flex flex-wrap items-center gap-2 text-sm text-[var(--g5-text2)]"
            testId="workspace-terminal-start-hint"
          />
        ) : (
          "Start the workspace to open a terminal."
        )}
      </div>
    );
  }

  if (readOnly) {
    const retained = Math.min(attachment.received, retainedCapacityBytes);
    const connected = status === "Connected";
    const pending = !connected && pendingStateText !== null;
    return (
      <div
        className="flex h-full min-h-0 min-w-0 flex-col bg-[var(--g5-terminal)]"
        data-testid="workspace-terminal-viewer"
        data-viewer-state={
          attachment.ended
            ? "ended"
            : attachment.superseded
              ? "superseded"
              : pending
                ? "waiting"
                : attachment.rejected
                  ? "unavailable"
                  : connected
                    ? "attached"
                    : "connecting"
        }
      >
        <div className="flex flex-wrap items-center justify-between gap-x-3 gap-y-1 border-b border-[var(--g5-stroke)] px-3 py-1 text-xs text-[var(--g5-text2)]">
          <span className="inline-flex min-w-0 items-center gap-2">
            <span
              className="truncate font-mono text-[var(--g5-text1)]"
              data-testid="workspace-terminal-session-name"
            >
              {sessionLabel ?? terminalId}
            </span>
            <span
              className="rounded border border-[var(--g5-stroke)] px-1 text-[10px] uppercase tracking-wide"
              data-testid="workspace-terminal-readonly-badge"
            >
              read-only
            </span>
            <span data-testid="workspace-terminal-viewer-status">
              {pending
                ? pendingStateText
                : viewerStateText(status, attachment, following)}
            </span>
          </span>
          <span className="inline-flex items-center gap-3">
            <span data-testid="workspace-terminal-retained">
              {formatBytes(retained)} of {formatBytes(retainedCapacityBytes)}{" "}
              retained
            </span>
            {!following && !attachment.ended && !attachment.superseded ? (
              <button
                type="button"
                data-testid="workspace-terminal-follow"
                onClick={() => followRef.current?.()}
                className="underline hover:text-[var(--g5-text1)]"
              >
                Follow output
              </button>
            ) : null}
            {attachment.superseded ? (
              // The session is shown in another window now; taking it back
              // supersedes that viewer in turn, at this viewer's offset
              // (`g5.preview.output.live-tail.viewer`).
              <button
                type="button"
                data-testid="workspace-terminal-take-over"
                title="Show the output here again; the other window is told it was replaced"
                onClick={takeOver}
                className="underline hover:text-[var(--g5-text1)]"
              >
                Take over
              </button>
            ) : null}
            {status === "Connection failed" ? (
              <button
                type="button"
                data-testid="workspace-terminal-retry"
                onClick={restart}
                className="underline hover:text-[var(--g5-text1)]"
              >
                Retry
              </button>
            ) : null}
          </span>
        </div>
        <div
          ref={hostRef}
          className="min-h-0 flex-1 p-2"
          data-testid="workspace-terminal"
        />
      </div>
    );
  }

  return (
    <div
      data-workspace-activity="ignore"
      className="flex h-full min-h-0 min-w-0 flex-col bg-[var(--g5-terminal)]"
    >
      <div className="flex items-center justify-between border-b border-[var(--g5-stroke)] px-3 py-1 text-xs text-[var(--g5-text2)]">
        <span>Workspace terminal · {status}</span>
        {(status === "Session ended" ||
          status === "Connection failed" ||
          status.startsWith(SECRETS_REFUSED_STATUS)) && (
          <button
            type="button"
            data-testid="workspace-terminal-retry"
            onClick={restart}
            className="underline hover:text-[var(--g5-text1)]"
          >
            {status === "Session ended" ? "Open new terminal" : "Retry"}
          </button>
        )}
      </div>
      <div
        ref={hostRef}
        className="min-h-0 flex-1 p-2"
        data-testid="workspace-terminal"
      />
    </div>
  );
}
