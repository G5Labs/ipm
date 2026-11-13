// =============================================================================
// G5 — BranchTrackingInline
// =============================================================================
//
// Remote-tracking presentation for one repository card on Workspace Details:
//
//   - An always-visible full-text `RemoteStatusPill` carries the state.
//   - An `aria-expanded` expander discloses the tracking detail in normal page
//     flow, collapsed by default. Expanding grows the page, which scrolls;
//     nothing floats over the page or clips at the viewport edge.
//
// The detail is the `BranchTrackingPopover` body, so its merge cards, SHAs,
// Push/Pull and auto-pull/auto-push controls, retry and recent activity keep
// their hooks, preconditions and test ids. Every tracking mutation lives in
// that body; the card head holds only the pill and the expander.
//
// While tracking state is loading, unavailable, or not yet reported for this
// repo, the card shows an explicit pill (and a retry when unavailable) and no
// expander or mutation, so no action is ever derived from missing data.
//
// The detail stays open after an action so its feedback remains visible. When
// the control that had focus disappears (e.g. Confirm resolved), focus returns
// to the expander instead of falling to the document body.
//
// Ontology: g5.workspace.remote-ui.status, .detail, .unknown, .actions
// =============================================================================

import { useCallback, useId, useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { ChevronDown, ChevronUp } from "lucide-react";
import {
  branchMergeRunsQueryKey,
  syncRunsQueryKey,
  useActiveBranchMergeRuns,
  useBranchSyncRuns,
} from "../../hooks/useRemoteTracking";
import { BranchTrackingPopover } from "./BranchTrackingPopover";
import { RemoteStatusPill } from "./RemoteStatusPill";
import { deriveRemoteTrackingStatus } from "./remoteTrackingStatus";

const HEAD_BUTTON_CLASS =
  "inline-flex items-center gap-1 rounded-[var(--g5-radius-sm)] px-1.5 py-0.5 text-xs text-[var(--g5-text1)] hover:bg-[var(--g5-bg2)] hover:text-[var(--g5-text0)] focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-[var(--g5-info)]";

export interface BranchTrackingInlineProps {
  workspaceId: string;
  repoId: string;
  /** Repository label, so each card's expander has a distinct accessible name. */
  repoLabel: string;
}

export function BranchTrackingInline({
  workspaceId,
  repoId,
  repoLabel,
}: BranchTrackingInlineProps) {
  const queryClient = useQueryClient();
  const syncState = useBranchSyncRuns(workspaceId, repoId, { perPage: 20 });
  const mergeState = useActiveBranchMergeRuns(workspaceId, repoId);
  const [expanded, setExpanded] = useState(false);
  const detailId = useId();
  const expanderRef = useRef<HTMLButtonElement | null>(null);
  const lastFocusedRef = useRef<HTMLElement | null>(null);
  const observerRef = useRef<MutationObserver | null>(null);

  // Watch the mounted detail: if the element that last held focus inside it
  // is removed and focus fell to the body, move focus back to the expander.
  const detailRef = useCallback((node: HTMLDivElement | null) => {
    observerRef.current?.disconnect();
    observerRef.current = null;
    if (!node) return;
    const observer = new MutationObserver(() => {
      const last = lastFocusedRef.current;
      if (!last || last.isConnected) return;
      lastFocusedRef.current = null;
      const active = document.activeElement;
      if (!active || active === document.body) expanderRef.current?.focus();
    });
    observer.observe(node, { childList: true, subtree: true });
    observerRef.current = observer;
  }, []);

  // Review vs blocked depends on the active merge runs, so both feeds must be
  // known before the state (and any action) can be derived.
  if (syncState.status === "loading" || mergeState.status === "loading") {
    return (
      <div data-testid="repo-tracking-inline">
        <RemoteStatusPill state="loading" label="Loading tracking status…" />
      </div>
    );
  }

  if (syncState.status === "error" || mergeState.status === "error") {
    return (
      <div
        data-testid="repo-tracking-inline"
        className="flex flex-wrap items-center gap-2"
      >
        <RemoteStatusPill
          state="unavailable"
          label="Tracking status unavailable"
        />
        <button
          type="button"
          data-testid="repo-tracking-retry"
          onClick={() => {
            void queryClient.invalidateQueries({
              queryKey: syncRunsQueryKey(workspaceId, repoId),
            });
            void queryClient.invalidateQueries({
              queryKey: branchMergeRunsQueryKey(workspaceId, repoId),
            });
          }}
          className={HEAD_BUTTON_CLASS}
        >
          Retry loading tracking status
        </button>
      </div>
    );
  }

  const remoteTracking = syncState.data.branch.repos.filter(
    (r) => r.trackingMode === "remote_tracking" && r.repoId === repoId,
  );
  const activeMergeRuns = mergeState.data.items.filter((run) =>
    remoteTracking.some((repo) => repo.repoId === run.repoId),
  );
  const status = deriveRemoteTrackingStatus(remoteTracking, activeMergeRuns);
  if (!status) {
    // The workspace marks this repo remote-tracked but the feed has not
    // reported it yet (e.g. right after Make remote). Say so; derive nothing.
    return (
      <div data-testid="repo-tracking-inline">
        <RemoteStatusPill state="loading" label="Tracking status pending" />
      </div>
    );
  }

  return (
    <div data-testid="repo-tracking-inline" className="flex flex-col gap-1.5">
      <div className="flex flex-wrap items-center gap-2">
        <RemoteStatusPill
          state={status.state}
          label={status.label}
          pauseReason={status.pauseReason}
        />
        <button
          ref={expanderRef}
          type="button"
          data-testid="repo-tracking-expander"
          aria-label={`${expanded ? "Hide details & actions" : "Details & actions"} for ${repoLabel}`}
          aria-expanded={expanded}
          aria-controls={expanded ? detailId : undefined}
          onClick={() => setExpanded((v) => !v)}
          className={HEAD_BUTTON_CLASS}
        >
          {expanded ? (
            <ChevronUp size={13} aria-hidden="true" />
          ) : (
            <ChevronDown size={13} aria-hidden="true" />
          )}
          {expanded ? "Hide details & actions" : "Details & actions"}
        </button>
      </div>
      {expanded && (
        <div
          ref={detailRef}
          id={detailId}
          data-testid="repo-tracking-detail"
          onFocus={(event) => {
            lastFocusedRef.current = event.target;
          }}
        >
          <BranchTrackingPopover workspaceId={workspaceId} repoId={repoId} />
        </div>
      )}
    </div>
  );
}
