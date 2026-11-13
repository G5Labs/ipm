/**
 * What the held session timeline covers (g5.chat.history-window): the rows
 * are contiguous from the oldest held row (`floorCreatedAt`) to the newest,
 * and `hasOlder` says whether activity exists below that floor. While the
 * first read is pending or failed, coverage is unknown.
 */
export interface TimelineCoverage {
  status: "loading" | "ready" | "error";
  floorCreatedAt: string | null;
  hasOlder: boolean;
}

/**
 * How much of one assistant turn's activity is loaded. `partial` holds the
 * newest part of the turn; `unloaded` holds none of it.
 */
export type TurnActivityCoverage = "complete" | "partial" | "unloaded";

/**
 * A turn's activity is complete when no older activity exists, or when the
 * held floor is strictly older than the turn's assistant message, which is
 * created with the accepted input before any of the turn's events. Timestamps
 * compare at millisecond precision and an equal timestamp is not covered, so
 * an error always costs one more load rather than a turn that looks complete.
 * Without coverage (a transcript with no timeline source) every turn is
 * complete, as before.
 */
export function turnActivityCoverage(
  turnCreatedAt: string,
  ownedEntryCount: number,
  coverage: TimelineCoverage | undefined,
): TurnActivityCoverage {
  if (!coverage) return "complete";
  if (coverage.status === "ready" && !coverage.hasOlder) return "complete";
  const turnStart = Date.parse(turnCreatedAt);
  if (Number.isNaN(turnStart)) return "complete";
  if (coverage.status === "ready" && coverage.floorCreatedAt) {
    const floor = Date.parse(coverage.floorCreatedAt);
    if (!Number.isNaN(floor) && floor < turnStart) return "complete";
  }
  return ownedEntryCount > 0 ? "partial" : "unloaded";
}
