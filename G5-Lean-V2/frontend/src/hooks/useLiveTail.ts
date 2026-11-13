// =============================================================================
// G5 Client — the running turn's unsaved tail (chat g5.chat.live-tail-recovery)
// =============================================================================
// Saved history lags live output by up to the history batch interval. After
// each completed read of saved history, and while the turn is active, the
// browser reads the API's live tail for the session and places its rows into
// the held timeline window under the same identity rules as WebSocket live
// rows. A tail that is still catching up is shown as such and read again with
// backoff; it is never presented as complete. Placing rows is a cache write,
// not a read of saved history: it never triggers another tail read.
// =============================================================================

import { useEffect, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { api } from "../lib/api";
import { routes } from "../lib/routes";
import type { LiveTailResponseDTO } from "../lib/types";
import { projectTaskTimelineEntries } from "../lib/taskTimeline";
import {
  insertLiveRows,
  onLiveTailRequested,
  persistedSequenceCeilings,
  type TimelineWindow,
} from "../lib/liveTimeline";
import { requestTimelineHoleFill } from "../lib/timelineHoles";

export type LiveTailState = "idle" | "live" | "catching_up" | "none" | "error";

/** Retry delays while the tail is catching up or unreachable, capped at 10 s. */
export const LIVE_TAIL_RETRY_DELAYS_MS = [1000, 2000, 4000, 8000, 10000];
/** Gap signals arriving within this window share one tail read. */
export const LIVE_TAIL_GAP_DEBOUNCE_MS = 250;

function isAbort(error: unknown): boolean {
  return (
    (error instanceof DOMException && error.name === "AbortError") ||
    (error instanceof Error && error.name === "AbortError")
  );
}

/**
 * Read the session's live tail on explicit signals only: when the turn
 * becomes active (including after a reconnect, which re-reads saved history),
 * each time a read of saved history completes (`historyReads` counts them;
 * see `useSessionTimeline`), and once per burst of gap signals from the live
 * placement path. Cache writes, including this hook's own placements, are no
 * signal. One read is in flight per session at a time; a session switch or
 * the turn ending cancels the pending read and its retries. The tail is
 * placed into the held window only: without a held window there is nothing
 * to place it into, and the read that creates the window is itself a signal.
 */
export function useLiveTail(
  sessionId: string | null | undefined,
  active: boolean,
  historyReads: number,
  options: { retryDelaysMs?: readonly number[] } = {},
): LiveTailState {
  const retryDelays = options.retryDelaysMs ?? LIVE_TAIL_RETRY_DELAYS_MS;
  const queryClient = useQueryClient();
  // The state is remembered per active session; an inactive turn or another
  // session reads as idle without a render-time state write.
  const activeKey = sessionId && active ? sessionId : null;
  const [stored, setStored] = useState<{
    key: string;
    state: LiveTailState;
  } | null>(null);
  const setState = (state: LiveTailState) => {
    if (activeKey) setStored({ key: activeKey, state });
  };
  // A live row that skipped past the held sequence asks for one
  // resynchronizing read; a burst of such rows shares it.
  const [gapNonce, setGapNonce] = useState(0);
  useEffect(() => {
    if (!sessionId) return;
    let timer: number | undefined;
    const unsubscribe = onLiveTailRequested(sessionId, () => {
      if (timer !== undefined) return;
      timer = window.setTimeout(() => {
        timer = undefined;
        setGapNonce((nonce) => nonce + 1);
      }, LIVE_TAIL_GAP_DEBOUNCE_MS);
    });
    return () => {
      unsubscribe();
      if (timer !== undefined) window.clearTimeout(timer);
    };
  }, [sessionId]);

  useEffect(() => {
    if (!sessionId || !active) return;
    const key = ["session-timeline", sessionId];
    if (!queryClient.getQueryData<TimelineWindow>(key)) return;
    let cancelled = false;
    let timer: number | undefined;
    let controller: AbortController | undefined;
    let attempt = 0;

    const scheduleRetry = () => {
      const delay = retryDelays[Math.min(attempt, retryDelays.length - 1)];
      attempt += 1;
      timer = window.setTimeout(() => void read(), delay);
    };

    const read = async () => {
      controller = new AbortController();
      try {
        const tail = await api.get<LiveTailResponseDTO>(
          routes.sessions.liveTail(sessionId),
          { signal: controller.signal },
        );
        if (cancelled) return;
        if (tail.state === "live") {
          const entries = projectTaskTimelineEntries(tail.entries ?? []);
          const held = queryClient.getQueryData<TimelineWindow>(key);
          // An unchanged window is not written back: the write would reach
          // the window's observers as an update without being a read.
          const placed = held ? insertLiveRows(held, entries) : undefined;
          if (held && placed && placed !== held) {
            queryClient.setQueryData<TimelineWindow>(key, placed);
          }
          // Persistence moved past the saved rows held for this attempt:
          // read saved history again so the prefix the tail no longer carries
          // is not missing. Persisted sequence 0 means nothing saved yet.
          if (
            placed &&
            typeof tail.taskId === "string" &&
            typeof tail.attempt === "number" &&
            typeof tail.persistedThrough === "number"
          ) {
            const attemptKey = `${tail.taskId}:${tail.attempt}`;
            const ceiling = Math.max(
              persistedSequenceCeilings(placed.items).get(attemptKey) ?? 0,
              0,
            );
            if (
              tail.persistedThrough > ceiling &&
              refreshedThrough.get(attemptKey) !== tail.persistedThrough
            ) {
              refreshedThrough.set(attemptKey, tail.persistedThrough);
              void queryClient.invalidateQueries({
                queryKey: key,
                exact: true,
              });
            }
          }
          attempt = 0;
          setState("live");
          // Rows the tail no longer carries and no history read returned
          // (a burst dropped for a slow connection, persisted before this
          // read) are read from saved history by their sequence.
          void requestTimelineHoleFill(
            queryClient,
            sessionId,
            typeof tail.taskId === "string" &&
              typeof tail.attempt === "number" &&
              typeof tail.persistedThrough === "number"
              ? new Map([
                  [`${tail.taskId}:${tail.attempt}`, tail.persistedThrough],
                ])
              : undefined,
          );
        } else if (tail.state === "catching_up") {
          setState("catching_up");
          scheduleRetry();
        } else {
          setState("none");
          void requestTimelineHoleFill(queryClient, sessionId);
        }
      } catch (error) {
        if (cancelled || isAbort(error)) return;
        setState("error");
        scheduleRetry();
      }
    };

    void read();
    return () => {
      cancelled = true;
      controller?.abort();
      if (timer !== undefined) window.clearTimeout(timer);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- setState closes over activeKey, which changes with these deps; retryDelays is a test-only constant
  }, [active, gapNonce, historyReads, queryClient, sessionId]);

  return stored && stored.key === activeKey ? stored.state : "idle";
}

/**
 * Persisted watermarks a saved-history refresh was already requested for, per
 * execution, so one tail read per watermark triggers at most one refresh.
 * Module-level: watermarks only grow and a page holds few live attempts.
 */
const refreshedThrough = new Map<string, number>();

/** Test hook: forget remembered refresh watermarks. */
export function resetLiveTailRefreshMemory(): void {
  refreshedThrough.clear();
}
