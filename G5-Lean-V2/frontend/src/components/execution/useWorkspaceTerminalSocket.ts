import { useCallback, useEffect, useRef, useState } from "react";
import { api, ApiError } from "../../lib/api";

interface WsTicket {
  ticket: string;
}

/** The socket owns reconnect state; the rendering surface can be xterm or a test stub. */
export interface WorkspaceTerminalPort {
  workspaceId: string;
  generation: number;
  active: boolean;
  write(bytes: Uint8Array): void;
  reset(): void;
  size(): { cols: number; rows: number };
}

/**
 * What the viewer knows about its attachment, for a read-only session's
 * header (`g5.preview.run-log`): the byte offset the next output has,
 * how many bytes arrived since the last `opened` (from a fresh attach that
 * is the retained window), whether the guest said the session ended, whether
 * a newer viewer superseded this one (the session goes on without it until
 * `takeOver`), and whether the attach was refused. `ended` and `superseded`
 * are the two reasons of the guest's `closed` line and exclude each other.
 */
export interface TerminalAttachmentState {
  offset: number | null;
  received: number;
  ended: boolean;
  superseded: boolean;
  rejected: boolean;
}

const MAX_RECONNECT_ATTEMPTS = 8;
const STABLE_CONNECTION_MS = 30_000;
/** A `closed` control line is small; anything longer is output. */
const MAX_CLOSED_LINE_BYTES = 256;

function terminalStorageKey(workspaceId: string): string {
  return `g5:workspace-terminal:${workspaceId}`;
}

function storedTerminalId(key: string): string | null {
  try {
    return window.sessionStorage.getItem(key);
  } catch {
    return null;
  }
}

function storeTerminalId(key: string, terminalId: string): void {
  try {
    window.sessionStorage.setItem(key, terminalId);
  } catch {
    // The in-memory terminal ID still supports transport reconnects.
  }
}

function forgetTerminalId(key: string): void {
  try {
    window.sessionStorage.removeItem(key);
  } catch {
    // A rejected ID is cleared in memory even when storage is unavailable.
  }
}

function terminalUrl(
  workspaceId: string,
  ticket: string,
  terminalId: string | null,
  offset: number | null,
): string {
  const configured = (
    import.meta.env.VITE_API_URL as string | undefined
  )?.trim();
  const base = new URL(configured || window.location.origin);
  base.protocol = base.protocol === "https:" ? "wss:" : "ws:";
  base.pathname = `/api/v1/workspaces/${encodeURIComponent(workspaceId)}/terminal`;
  const params = new URLSearchParams({
    token: ticket,
    cols: "80",
    rows: "24",
  });
  if (terminalId) params.set("terminal_id", terminalId);
  if (terminalId && offset !== null) params.set("offset", String(offset));
  base.search = params.toString();
  return base.toString();
}

/** The status line for a terminal the API or guest refused over its secrets. */
export const SECRETS_REFUSED_STATUS = "Runtime Config secrets refused";

function secretsRefusedStatus(detail: string | undefined): string {
  const sentence = typeof detail === "string" ? detail.trim() : "";
  if (sentence.length === 0) return SECRETS_REFUSED_STATUS;
  // Bounded so a hostile detail cannot flood the header.
  const bounded =
    sentence.length > 400 ? `${sentence.slice(0, 399)}…` : sentence;
  return `${SECRETS_REFUSED_STATUS}: ${bounded}`;
}

/**
 * A hosted session's stream ends with one `closed` control line sent as its
 * own message after the output (`g5.preview.output.live-tail`). Only a whole
 * message that parses as that line counts; output never does.
 */
function closedReason(bytes: Uint8Array): string | null {
  if (
    bytes.length === 0 ||
    bytes.length > MAX_CLOSED_LINE_BYTES ||
    bytes[0] !== 0x7b ||
    bytes[bytes.length - 1] !== 10
  )
    return null;
  try {
    const control = JSON.parse(new TextDecoder().decode(bytes).trim()) as {
      type?: string;
      reason?: string;
    };
    return control.type === "closed" && typeof control.reason === "string"
      ? control.reason
      : null;
  } catch {
    return null;
  }
}

const INITIAL_ATTACHMENT: TerminalAttachmentState = {
  offset: null,
  received: 0,
  ended: false,
  superseded: false,
  rejected: false,
};

export function useWorkspaceTerminalSocket({
  workspaceId,
  enabled,
  port,
  terminalId: fixedTerminalId = null,
  readOnly = false,
}: {
  workspaceId: string;
  enabled: boolean;
  port: WorkspaceTerminalPort | null;
  /**
   * Attach to this named session instead of opening (or remembering) a
   * shell. A refusal is final: nothing can create the session from here.
   */
  terminalId?: string | null;
  /**
   * A viewer: no input is ever sent, and the guest's `closed` line ends the
   * attachment for good instead of prompting a reconnect.
   */
  readOnly?: boolean;
}) {
  const [status, setStatus] = useState("Connecting");
  const [retryKey, setRetryKey] = useState(0);
  // A hosted viewer's reattachment on the same surface (`takeOver`): the
  // surface and its port stay, only the attachment lifetime starts over.
  const [attachKey, setAttachKey] = useState(0);
  const [attachment, setAttachment] =
    useState<TerminalAttachmentState>(INITIAL_ATTACHMENT);
  const socketRef = useRef<WebSocket | null>(null);
  // The viewer's own progress (its offset and what is on screen), kept
  // outside the attachment lifetime so a take over resumes where this viewer
  // stopped rather than redrawing the tail.
  const progressRef = useRef<{ offset: number | null; received: number }>({
    offset: null,
    received: 0,
  });
  const resumeRef = useRef<{ offset: number | null; received: number } | null>(
    null,
  );

  const send = useCallback(
    (message: Record<string, unknown>) => {
      // A viewer never sends input, whatever the surface emits.
      if (readOnly && message.type === "input") return;
      const socket = socketRef.current;
      if (socket?.readyState === WebSocket.OPEN) {
        socket.send(JSON.stringify(message));
      }
    },
    [readOnly],
  );

  const restart = useCallback(() => {
    setStatus("Connecting");
    setAttachment(INITIAL_ATTACHMENT);
    setRetryKey((key) => key + 1);
  }, []);

  /**
   * Reattach a superseded hosted viewer at its own offset, on the same
   * surface (`g5.preview.output.live-tail.viewer`): the other viewer is
   * superseded in turn, by the owner's click rather than reconnect policy.
   */
  const takeOver = useCallback(() => {
    resumeRef.current = { ...progressRef.current };
    setAttachKey((key) => key + 1);
  }, []);

  // Each attachment lifetime (the effect below, keyed on these inputs) starts
  // over: nothing the previous one published (a refusal, an end) describes
  // this one. Adjusted while rendering, so no frame commits stale words into
  // the new lifetime and a parent counting refusals sees each one once.
  // (`send` changes only with `readOnly`, which a mounted surface never
  // changes; `restart` resets on its own.)
  const lifetimeInputs: readonly unknown[] = [
    enabled,
    workspaceId,
    retryKey,
    attachKey,
    port,
    fixedTerminalId,
  ];
  const [seenInputs, setSeenInputs] = useState(lifetimeInputs);
  if (lifetimeInputs.some((input, index) => input !== seenInputs[index])) {
    setSeenInputs(lifetimeInputs);
    setStatus("Connecting");
    setAttachment(INITIAL_ATTACHMENT);
  }

  useEffect(() => {
    if (
      !enabled ||
      !port ||
      !port.active ||
      port.workspaceId !== workspaceId ||
      port.generation !== retryKey
    )
      return;
    let socket: WebSocket | null = null;
    let disposed = false;
    const hosted = fixedTerminalId !== null;
    const storageKey = terminalStorageKey(workspaceId);
    let terminalId: string | null = hosted
      ? fixedTerminalId
      : storedTerminalId(storageKey);
    // A take over resumes at the offset this viewer reached, with what it
    // already shows counted; any other attachment starts from the tail.
    const resume = hosted ? resumeRef.current : null;
    resumeRef.current = null;
    let terminalOffset: number | null = resume?.offset ?? null;
    let received = resume?.received ?? 0;
    progressRef.current = { offset: terminalOffset, received };
    let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
    let stableTimer: ReturnType<typeof setTimeout> | null = null;
    let publishTimer: ReturnType<typeof setTimeout> | null = null;
    let reconnectAttempt = 0;
    let terminalRejected = false;
    const publishAttachment = (patch: Partial<TerminalAttachmentState>) => {
      if (disposed) return;
      if (publishTimer) {
        clearTimeout(publishTimer);
        publishTimer = null;
      }
      setAttachment((previous) => ({
        ...previous,
        offset: terminalOffset,
        received,
        ...patch,
      }));
    };
    // Output arrives in many small frames; the header's byte counts follow
    // them a few times a second rather than once per frame.
    const publishSoon = () => {
      if (disposed || publishTimer) return;
      publishTimer = setTimeout(() => {
        publishTimer = null;
        publishAttachment({});
      }, 250);
    };

    const scheduleReconnect = () => {
      if (stableTimer) {
        clearTimeout(stableTimer);
        stableTimer = null;
      }
      if (disposed || terminalRejected || reconnectTimer) return;
      if (reconnectAttempt >= MAX_RECONNECT_ATTEMPTS) {
        setStatus("Connection failed");
        return;
      }
      const delay = Math.min(250 * 2 ** reconnectAttempt, 5_000);
      const jitteredDelay = Math.round(delay * (0.8 + Math.random() * 0.4));
      reconnectAttempt += 1;
      setStatus("Reconnecting");
      publishAttachment({});
      reconnectTimer = setTimeout(() => {
        reconnectTimer = null;
        void connect();
      }, jitteredDelay);
    };

    const connect = async () => {
      try {
        const { ticket } = await api.post<WsTicket>("/api/v1/auth/ws-ticket", {
          workspaceId,
        });
        if (disposed) return;
        const connecting = new WebSocket(
          terminalUrl(workspaceId, ticket, terminalId, terminalOffset),
        );
        socket = connecting;
        socketRef.current = connecting;
        connecting.binaryType = "arraybuffer";
        let awaitingControl = true;
        let controlPrefix = new Uint8Array();
        connecting.onopen = () => {
          if (!disposed && socket === connecting) setStatus("Connecting");
        };
        connecting.onmessage = (event) => {
          const writeOutput = (bytes: Uint8Array) => {
            if (bytes.length === 0) return;
            port.write(bytes);
            received += bytes.length;
            if (terminalOffset !== null) terminalOffset += bytes.length;
            progressRef.current = { offset: terminalOffset, received };
            publishSoon();
          };
          const consume = (bytes: Uint8Array) => {
            if (
              disposed ||
              socket !== connecting ||
              connecting.readyState !== WebSocket.OPEN
            )
              return;
            if (awaitingControl) {
              const newline = bytes.indexOf(10);
              const prefix = newline < 0 ? bytes : bytes.subarray(0, newline);
              if (controlPrefix.length + prefix.length > 64 * 1024) {
                connecting.close();
                return;
              }
              const line = new Uint8Array(controlPrefix.length + prefix.length);
              line.set(controlPrefix);
              line.set(prefix, controlPrefix.length);
              if (newline < 0) {
                controlPrefix = line;
                return;
              }
              awaitingControl = false;
              try {
                const control = JSON.parse(
                  new TextDecoder().decode(line).trim(),
                ) as {
                  type?: string;
                  terminal_id?: string;
                  reattached?: boolean;
                  offset?: number;
                  reset?: boolean;
                  reason?: string;
                  detail?: string;
                };
                if (control.type === "opened" && control.terminal_id) {
                  const offset =
                    typeof control.offset === "number" &&
                    Number.isSafeInteger(control.offset) &&
                    control.offset >= 0
                      ? control.offset
                      : null;
                  // A shell resets only when it reattaches past its history;
                  // a hosted viewer redraws whenever the guest says the
                  // requested offset was unavailable, including its first,
                  // offset-less attach (an empty screen is unaffected).
                  const reset = hosted
                    ? control.reset === true
                    : control.reattached === true && control.reset !== false;
                  if (reset) port.reset();
                  if (hosted && reset && offset !== null && offset > 0) {
                    port.write(
                      new TextEncoder().encode(
                        "\x1b[2m… earlier output is beyond the retained window …\x1b[0m\r\n",
                      ),
                    );
                  }
                  terminalId = control.terminal_id;
                  terminalOffset = offset;
                  // A redraw starts the on-screen count over; a resume at the
                  // viewer's offset keeps counting what is already shown.
                  if (reset) received = 0;
                  progressRef.current = { offset: terminalOffset, received };
                  if (!hosted) storeTerminalId(storageKey, terminalId);
                  stableTimer = setTimeout(() => {
                    reconnectAttempt = 0;
                    stableTimer = null;
                  }, STABLE_CONNECTION_MS);
                  setStatus("Connected");
                  publishAttachment({
                    ended: false,
                    superseded: false,
                    rejected: false,
                  });
                  send({ type: "resize", ...port.size() });
                  writeOutput(bytes.subarray(newline + 1));
                  return;
                }
                if (control.type === "rejected") {
                  if (!hosted) {
                    terminalId = null;
                    forgetTerminalId(storageKey);
                  }
                  terminalOffset = null;
                  terminalRejected = true;
                  // A refused secret set is the owner's to correct
                  // (`g5.terminal.secret-admission`): show the reason and
                  // let Retry reopen after the fix, without a reload. A
                  // hosted session the guest no longer holds is unavailable.
                  setStatus(
                    control.reason === "secrets_invalid"
                      ? secretsRefusedStatus(control.detail)
                      : hosted
                        ? "Session unavailable"
                        : "Session ended",
                  );
                  publishAttachment({ rejected: true });
                  connecting.close();
                  return;
                }
              } catch {
                // A protocol mismatch is a failed connection. Do not render
                // control bytes into the terminal.
              }
              connecting.close();
              return;
            }
            if (hosted) {
              const reason = closedReason(bytes);
              if (reason !== null) {
                // The guest ended this stream on purpose: stop reconnecting,
                // the output stays on screen. `superseded` means a newer
                // viewer owns the session now (a reconnect would only take
                // it back and be superseded again); anything else means the
                // session's writer is done and every retained byte was
                // delivered.
                terminalRejected = true;
                if (reason === "superseded") {
                  setStatus("Superseded");
                  publishAttachment({ superseded: true });
                } else {
                  setStatus("Session ended");
                  publishAttachment({ ended: true });
                }
                connecting.close();
                return;
              }
            }
            writeOutput(bytes);
          };
          if (event.data instanceof ArrayBuffer) {
            consume(new Uint8Array(event.data));
          } else if (event.data instanceof Blob) {
            void event.data
              .arrayBuffer()
              .then((buffer) => consume(new Uint8Array(buffer)));
          } else {
            consume(new TextEncoder().encode(String(event.data)));
          }
        };
        connecting.onerror = () => {
          if (!disposed && socket === connecting && !terminalRejected)
            setStatus("Connection failed");
        };
        connecting.onclose = () => {
          if (!disposed && socket === connecting) scheduleReconnect();
        };
      } catch (error) {
        if (disposed) return;
        setStatus("Connection failed");
        if (
          error instanceof ApiError &&
          error.isClientError &&
          error.status !== 408 &&
          error.status !== 429
        ) {
          terminalRejected = true;
          return;
        }
        scheduleReconnect();
      }
    };
    void connect();

    return () => {
      disposed = true;
      if (reconnectTimer) clearTimeout(reconnectTimer);
      if (stableTimer) clearTimeout(stableTimer);
      if (publishTimer) clearTimeout(publishTimer);
      socket?.close();
      if (socketRef.current === socket) socketRef.current = null;
    };
  }, [enabled, workspaceId, retryKey, attachKey, port, send, fixedTerminalId]);

  return { status, retryKey, send, restart, takeOver, attachment };
}
