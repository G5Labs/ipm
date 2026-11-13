// =============================================================================
// G5 Client — one page of a session's saved timeline
// =============================================================================
// The session timeline endpoint pages backward: without a cursor it answers
// the newest rows, with a cursor (`createdAt|id` of a held row) the rows
// strictly older than that row, each page in chronological order with the
// cursor of its oldest row when more remain (routes/chat).
// =============================================================================

import { api } from "./api";
import type { TimelineEntry } from "./types";
import { projectTaskTimelineEntries } from "./taskTimeline";
import type { TimelineWindow } from "./liveTimeline";

/** The cursor that reads the rows older than `row`. */
export function timelineCursorBefore(
  row: Pick<TimelineEntry, "id" | "createdAt">,
): string {
  return `${row.createdAt}|${row.id}`;
}

/**
 * Read one page of saved history: the newest page without a cursor, the
 * rows older than the cursor's row with one.
 */
export async function readSessionTimelinePage(
  sessionId: string,
  cursor: string | null | undefined,
  signal?: AbortSignal,
): Promise<TimelineWindow> {
  const res = await api.get<{
    entries: TimelineEntry[];
    cursor: string | null;
    hasMore: boolean;
  }>(`/api/v1/sessions/${sessionId}/timeline`, {
    signal,
    params: { cursor: cursor ?? undefined },
  });
  return {
    items: projectTaskTimelineEntries(res.entries ?? []),
    cursor: res.cursor ?? null,
    hasMore: res.hasMore ?? false,
  };
}
