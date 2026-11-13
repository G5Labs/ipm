// =============================================================================
// Prepare to Ship — approvals refresh schedule (g5.workbench.ship-approvals)
// =============================================================================
// Only the visible Prepare to Ship dialog reads the shared approvals response
// on this schedule: immediately on open and on returning to visibility, every
// second for the first 10 visible seconds, every 2 seconds afterwards while
// visible, and every 15 seconds while hidden. Returning to visibility restarts
// the fast period. A refresh never starts while another is in flight, closing
// the dialog (unmount) stops the schedule, and a transient failure keeps the
// last good data. The general approvals polling tiers
// (`approvalStateRefetchInterval`) and every other dialog are unchanged.
// =============================================================================

import { useEffect } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";

import type { WorkspaceApprovalsStateDTO } from "../lib/types";
import { toRefreshingState, type RefreshingState } from "./hookUtils";
import { workspaceApprovalsQueryOptions } from "./useWorkspaceApprovals";

export const SHIP_APPROVALS_FAST_MS = 1_000;
export const SHIP_APPROVALS_FAST_WINDOW_MS = 10_000;
export const SHIP_APPROVALS_VISIBLE_MS = 2_000;
export const SHIP_APPROVALS_HIDDEN_MS = 15_000;
/**
 * `visibilitychange` and window `focus` usually arrive together; a refresh
 * started this recently already answers the return.
 */
const RETURN_COALESCE_MS = 250;

/** Delay from one refresh's start to the next (pure; the schedule's rule). */
export function shipApprovalsRefreshDelay(
  visible: boolean,
  msSinceVisible: number,
): number {
  if (!visible) return SHIP_APPROVALS_HIDDEN_MS;
  return msSinceVisible < SHIP_APPROVALS_FAST_WINDOW_MS
    ? SHIP_APPROVALS_FAST_MS
    : SHIP_APPROVALS_VISIBLE_MS;
}

export interface ShipApprovalsRefreshOptions {
  /** One refresh; its rejection is a failed poll, never fatal. */
  refresh: () => Promise<unknown>;
  now?: () => number;
  doc?: Document;
  win?: Window;
}

/**
 * Starts the schedule and returns its stop. A refresh is started at once.
 * Exported for tests; the dialog uses {@link useShipApprovalsRefresh}.
 */
export function startShipApprovalsRefresh({
  refresh,
  now = () => Date.now(),
  doc = document,
  win = window,
}: ShipApprovalsRefreshOptions): () => void {
  let stopped = false;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let inFlight = false;
  let rerun = false;
  let lastStart = now();
  let visibleSince = now();
  const visible = () => doc.visibilityState !== "hidden";

  const schedule = () => {
    clearTimeout(timer);
    const delay = shipApprovalsRefreshDelay(visible(), now() - visibleSince);
    timer = setTimeout(run, Math.max(0, lastStart + delay - now()));
  };

  function run() {
    if (stopped || inFlight) return;
    clearTimeout(timer);
    inFlight = true;
    lastStart = now();
    let pending: Promise<unknown>;
    try {
      pending = refresh();
    } catch (error) {
      pending = Promise.reject(error);
    }
    void pending
      .catch(() => {
        // The query keeps its last good data; the schedule continues.
      })
      .finally(() => {
        inFlight = false;
        if (stopped) return;
        if (rerun) {
          rerun = false;
          run();
        } else {
          schedule();
        }
      });
  }

  const onVisibility = () => {
    if (stopped) return;
    if (!visible()) {
      // Hidden: the slow cadence from the last start.
      if (!inFlight) schedule();
      return;
    }
    visibleSince = now();
    if (!inFlight) run();
    else if (now() - lastStart > RETURN_COALESCE_MS) rerun = true;
  };

  doc.addEventListener("visibilitychange", onVisibility);
  win.addEventListener("focus", onVisibility);
  run();
  return () => {
    stopped = true;
    clearTimeout(timer);
    doc.removeEventListener("visibilitychange", onVisibility);
    win.removeEventListener("focus", onVisibility);
  };
}

/**
 * The shared approvals response for the open Prepare to Ship dialog, read on
 * the dialog's schedule. Mount it only while the dialog is shown.
 */
export function useShipApprovalsRefresh(
  workspaceId: string,
  enabled = true,
): RefreshingState<WorkspaceApprovalsStateDTO> {
  const queryClient = useQueryClient();
  const active = !!workspaceId && enabled;
  const query = useQuery<WorkspaceApprovalsStateDTO, Error>({
    ...workspaceApprovalsQueryOptions(workspaceId),
    enabled: active,
    // The schedule below owns every refresh of this observer.
    refetchOnMount: false,
    refetchOnWindowFocus: false,
    retry: false,
  });
  useEffect(() => {
    if (!active) return;
    return startShipApprovalsRefresh({
      // Joins an in-flight read of the same response rather than starting a
      // second one; React Query discards a cancelled (superseded) read.
      refresh: () =>
        queryClient.fetchQuery({
          ...workspaceApprovalsQueryOptions(workspaceId),
          staleTime: 0,
          retry: false,
        }),
    });
  }, [active, queryClient, workspaceId]);
  return toRefreshingState(query);
}
