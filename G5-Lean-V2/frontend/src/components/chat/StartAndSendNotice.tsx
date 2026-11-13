// =============================================================================
// G5 Client — Start and send notice
// =============================================================================
// Shown above the owner's composer, in the workbench and on Home's personal
// panels (chat g5.chat.start-and-send): while the VM is stopped or failed it
// says what Start and send does; once a Start is under way it shows the
// Start's progress inline, from the same live gate and VM status the rest of
// the surface reads. The workspace is named by the gate's noun ("the
// workspace", "your personal workspace", "your review workspace").
// =============================================================================

import { Loader2, Power } from "lucide-react";

import { useWorkspaceLiveGate } from "../../lib/workspaceLiveGate";

export interface StartAndSendNoticeProps {
  /** Why the Start waits for capacity, when it does. */
  waitReason?: string | null;
}

function capitalize(text: string): string {
  return text.charAt(0).toUpperCase() + text.slice(1);
}

export function StartAndSendNotice({ waitReason }: StartAndSendNoticeProps) {
  const gate = useWorkspaceLiveGate();
  if (!gate || !gate.owner) return null;
  const noun = gate.workspaceNoun;
  const base =
    "mb-2 flex items-start gap-2 rounded-md border px-2.5 py-1.5 text-xs leading-5";
  switch (gate.kind) {
    case "stopped":
    case "failed":
      return (
        <div
          role="note"
          data-testid="start-and-send-note"
          className={`${base} border-[var(--g5-stroke)] bg-[var(--g5-bg1)] text-[var(--g5-text1)]`}
        >
          <Power size={13} className="mt-1 shrink-0 text-[var(--g5-text2)]" />
          <span>
            {gate.kind === "failed"
              ? `${capitalize(noun)} failed to start. `
              : `${capitalize(noun)} is stopped. `}
            <strong className="font-medium text-[var(--g5-text0)]">
              Start and send
            </strong>{" "}
            starts it and sends your message once it is running.
          </span>
        </div>
      );
    case "starting":
    case "queued":
      return (
        <div
          role="status"
          aria-live="polite"
          data-testid="start-progress"
          data-state={gate.kind}
          className={`${base} border-[var(--g5-info)]/40 bg-[var(--g5-info)]/10 text-[var(--g5-text1)]`}
        >
          <Loader2
            size={13}
            className="mt-1 shrink-0 animate-spin text-[var(--g5-info)]"
            aria-hidden
          />
          <span>
            {gate.kind === "starting"
              ? `Starting ${noun}…`
              : `Waiting for capacity to start ${noun}${
                  waitReason ? `: ${waitReason}` : "…"
                }`}{" "}
            Queued messages are sent once it is running.
          </span>
        </div>
      );
    case "stopping":
      return (
        <div
          role="status"
          data-testid="start-progress"
          data-state="stopping"
          className={`${base} border-[var(--g5-stroke)] bg-[var(--g5-bg1)] text-[var(--g5-text2)]`}
        >
          <Loader2
            size={13}
            className="mt-1 shrink-0 animate-spin"
            aria-hidden
          />
          <span>{capitalize(noun)} is stopping…</span>
        </div>
      );
    default:
      return null;
  }
}
