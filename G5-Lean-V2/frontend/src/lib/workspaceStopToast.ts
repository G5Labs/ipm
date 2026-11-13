// =============================================================================
// Stop outcome → toast copy, shared by the workspace page and the Home
// Running compute tile (g5.lifecycle.stop, g5.lifecycle.deadline.loss).
// =============================================================================
// The stop route answers with the lifecycle's actual outcome: `stopping` while
// the save window runs (no flush yet), or `stopped` with the last flush id
// when one was recorded. Capture success is only ever claimed from that
// evidence. A forced stop records what it could not save; per the human's
// decision on g5.lifecycle.deadline.loss no loss notice is shown — the copy
// simply makes no flush claim.

import type { WorkspaceStopOutcome } from "./types";

export interface StopToast {
  severity: "info";
  title: string;
  message: string;
}

export function describeStopOutcome(
  outcome: WorkspaceStopOutcome,
  workspaceName?: string,
): StopToast {
  const subject = workspaceName ? `${workspaceName}` : "workspace";
  if (outcome.status !== "stopped") {
    return {
      severity: "info",
      title: `Shutting down ${subject}…`,
      message: "The VM stops once its state is saved.",
    };
  }
  if (outcome.flushId && !outcome.unflushedLoss) {
    return {
      severity: "info",
      title: "Workspace stopped",
      message: "The latest flush was recorded.",
    };
  }
  return {
    severity: "info",
    title: "Workspace stopped",
    message: "The VM is stopped.",
  };
}
