// =============================================================================
// G5 Client — Base re-verification wording (ui g5.ui.home-reverification)
// =============================================================================
// The words Home's status line and its dialog use for Base's re-verification
// status (ontology_and_harness g5.reverification.status), and the fork name
// a run starts with. Shared so the line, the dialog and their tests agree.
// =============================================================================

import type {
  BaseReverificationStatus,
  ReverificationKind,
  ReverificationRun,
  ReverificationRunState,
} from "./types";

/** The status while the organization's verification switch is on. */
export type EnabledBaseReverificationStatus = Extract<
  BaseReverificationStatus,
  { enabled: true }
>;

/** Why the Re-run verification button is greyed for someone who cannot fork Base. */
export const REVERIFICATION_CANNOT_START_REASON =
  "Only members who can fork Base can start a re-verification";

/** Why incremental re-verification is unavailable before any run completed. */
export const REVERIFICATION_INCREMENTAL_UNAVAILABLE_REASON =
  "Available after the first re-verification completes";

/** A calendar date, as Home's re-verification line and dialog show it. */
export function formatReverificationDate(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return iso;
  return date.toLocaleDateString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
  });
}

function formatCount(value: number): string {
  return value.toLocaleString("en-US");
}

/** "+3,410 −1,022 lines", "lines unavailable", or null when never re-verified. */
export function reverificationLinesText(
  status: EnabledBaseReverificationStatus,
): string | null {
  if (status.linesUnavailable) return "lines unavailable";
  if (!status.lines) return null;
  return `+${formatCount(status.lines.added)} −${formatCount(status.lines.removed)} lines`;
}

export function pullRequestCountText(count: number): string {
  return `${formatCount(count)} merged ${count === 1 ? "PR" : "PRs"}`;
}

/** The status line for every state but in progress, which links its workspace. */
export function reverificationStatusSentence(
  status: EnabledBaseReverificationStatus,
): string {
  if (status.state === "in_progress") return "Verification in progress";
  if (status.state === "never" || !status.lastCompleted) {
    return "Base has never been re-verified";
  }
  if (status.state === "current") {
    const completed =
      status.lastCompleted.completedAt ?? status.lastCompleted.startedAt;
    return `Base was re-verified on ${formatReverificationDate(completed)}; no PRs merged since`;
  }
  const lines = reverificationLinesText(status);
  return `Base has not been re-verified for ${pullRequestCountText(status.pullRequestCount)}${lines ? ` (${lines})` : ""}`;
}

/** Why no run can start now, or null when the caller may start one. */
export function reverificationStartBlockedReason(
  status: EnabledBaseReverificationStatus,
): string | null {
  if (status.state === "in_progress" || status.inProgress) {
    const name = status.inProgress?.workspaceName;
    return name
      ? `A re-verification is already in progress in workspace ${name}`
      : "A re-verification is already in progress";
  }
  if (!status.canStart) return REVERIFICATION_CANNOT_START_REASON;
  return null;
}

export const REVERIFICATION_KIND_LABELS: Record<ReverificationKind, string> = {
  incremental: "Incremental",
  full: "Full",
};

export const REVERIFICATION_STATE_LABELS: Record<
  ReverificationRunState,
  string
> = {
  in_progress: "In progress",
  completed: "Completed",
  abandoned: "Abandoned",
};

/** A run's state, with its completion date once it merged into Base. */
export function reverificationRunStateText(run: ReverificationRun): string {
  return run.state === "completed" && run.completedAt
    ? `Completed ${formatReverificationDate(run.completedAt)}`
    : REVERIFICATION_STATE_LABELS[run.state];
}

/**
 * The name a run's fork asks for; the server takes the first free
 * `-2`, `-3`, … suffix (`uniquifyName`). The date is UTC, so every browser
 * names a run's day alike.
 */
export function baseReverificationWorkspaceName(
  now: Date = new Date(),
): string {
  return `base-reverification-${now.toISOString().slice(0, 10)}`;
}
