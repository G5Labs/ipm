import type { TerminalAttachmentState } from "./useWorkspaceTerminalSocket";

/** The retained window of a hosted preview session (`g5.preview.output.live-tail`). */
export const HOSTED_SESSION_RETAINED_BYTES = 4 * 1024 * 1024;

/** What a read-only viewer reports upward: its socket status and attachment. */
export interface TerminalViewerState extends TerminalAttachmentState {
  status: string;
}

export function formatBytes(bytes: number): string {
  if (bytes >= 1024 * 1024) {
    const mib = bytes / (1024 * 1024);
    return `${Number.isInteger(mib) || mib >= 10 ? mib.toFixed(0) : mib.toFixed(1)} MiB`;
  }
  if (bytes >= 1024) return `${Math.round(bytes / 1024)} KiB`;
  return `${bytes} B`;
}

/** The header's words while the replaced viewer waits for a Take over. */
export const VIEWER_SUPERSEDED_TEXT = "viewing in another window";

/**
 * The read-only header's attachment state in words
 * (`g5.preview.run-log`): attached and following, reconnecting at its
 * offset, replaced by a newer viewer, refused, or ended with the output
 * retained.
 */
export function viewerStateText(
  status: string,
  attachment: TerminalAttachmentState,
  following: boolean,
): string {
  if (attachment.ended) return "session ended · retained";
  if (attachment.superseded) return VIEWER_SUPERSEDED_TEXT;
  if (attachment.rejected || status === "Session unavailable")
    return "session unavailable";
  if (status === "Connected")
    return following ? "attached and following" : "attached · follow paused";
  if (status === "Reconnecting")
    return attachment.offset !== null
      ? `reconnecting at offset ${attachment.offset}`
      : "reconnecting";
  if (status === "Connection failed") return "connection failed";
  return "attaching…";
}
