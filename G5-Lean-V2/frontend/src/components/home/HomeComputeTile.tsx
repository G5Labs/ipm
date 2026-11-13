// =============================================================================
// Home — Running compute tile (workbench Phase 6)
// =============================================================================
// Rendered by HomePage only under the `alignment` layout (g5.workbench.gate).
// Lists the organization's non-stopped workspace VMs the caller may read from
// `GET /orgs/:orgId/compute/running` (the server filters to readable
// workspaces), Mine first: status dot, the workspace name as the link to the
// workspace, slot size and uptime, owner and — for the caller's own
// workspaces — Stop through the same stop hook the workspace page uses
// (owner-only `POST /workspaces/:id/stop`, g5.lifecycle.stop). A stopping
// row reads "stopping…" and keeps its control disabled until the inventory
// drops it; the toast names the stop route's actual outcome
// (describeStopOutcome), never an unproven flush. The header carries the
// page's Stop my workspaces control (HomeStopMyWorkspaces) beside the
// filters, since that is where the caller's running VMs are listed.
// =============================================================================

import { Power } from "lucide-react";
import React, { useEffect, useState } from "react";
import { Link } from "react-router-dom";

import { useRunningCompute } from "../../hooks/useHome";
import { useStopWorkspaceVm } from "../../hooks/useWorkspace";
import type { RunningComputeDTO } from "../../lib/types";
import { describeStopOutcome } from "../../lib/workspaceStopToast";
import { IdentityAvatar } from "../shared/IdentityAvatar";
import { STATE_MAP, classNames } from "../ui/UI";
import {
  HomeFilterChip,
  HomeSectionHeader,
  HomeShowMore,
  HOME_ROW_ACTION_CLASSES,
  useHomePages,
} from "./HomeTileParts";
import { formatSlotSize, formatUptime } from "./homeTileLogic";

/** Uptime refresh cadence — the inventory poll cadence, so "up 2h 14m" moves with it. */
const UPTIME_TICK_MS = 30_000;

type ComputeFilter = "all" | "mine";

const COMPUTE_FILTERS: Array<{ value: ComputeFilter; label: string }> = [
  { value: "mine", label: "Mine" },
  { value: "all", label: "All" },
];

/** VM lifecycle status → the shared status palette. */
const STATUS_STATE: Record<
  RunningComputeDTO["status"],
  keyof typeof STATE_MAP
> = {
  starting: "info",
  running: "active",
  stopping: "warn",
  failed: "blocked",
};

const STATUS_WORD: Record<RunningComputeDTO["status"], string> = {
  starting: "starting",
  running: "running",
  stopping: "stopping…",
  failed: "failed",
};

export interface HomeComputeTileProps {
  orgId: string;
  orgSlug: string;
  callerUserId: string;
  /** Rows per page (UserSettings.homeRowsPerTile); omitted shows every row. */
  pageSize?: number;
  /**
   * The filter the tile opens on (ui g5.ui.home-workspaces): Mine once the
   * onboarding tour is finished, All before.
   */
  defaultFilter?: ComputeFilter;
  /** Toast relay shared with the page (same copy as the workspace page). */
  onNotify: (kind: "info" | "error", title: string, message: string) => void;
  /**
   * The page's Stop my workspaces control (g5.lifecycle.stop.by-user),
   * shown in the header before the filters; omitted for viewers, who own
   * no workspace.
   */
  headerAction?: React.ReactNode;
}

interface ComputeRowProps {
  orgSlug: string;
  row: RunningComputeDTO;
  owned: boolean;
  now: number;
  onNotify: HomeComputeTileProps["onNotify"];
  onStopped: () => void;
}

function ComputeRow({
  orgSlug,
  row,
  owned,
  now,
  onNotify,
  onStopped,
}: ComputeRowProps) {
  // The workspace page's stop hook: owner-only on the server, an explicit
  // stop that ends with the VM stopped (g5.lifecycle.stop); the page shows
  // the same toasts.
  const stop = useStopWorkspaceVm(row.workspaceId);
  // Once a stop was accepted the control stays disabled until the inventory
  // drops the row (it unmounts) — the mutation settles before the refetch,
  // and a second stop in that gap would be a second operation.
  const [stopAccepted, setStopAccepted] = useState(false);
  const stopping = row.status === "stopping" || stop.isLoading || stopAccepted;
  const state = STATE_MAP[STATUS_STATE[row.status]];
  const size = formatSlotSize(row);
  const uptime = row.startedAt ? formatUptime(row.startedAt, now) : null;
  const details = [
    size,
    uptime ? `up ${uptime}` : null,
    stopping
      ? STATUS_WORD.stopping
      : row.status !== "running"
        ? STATUS_WORD[row.status]
        : null,
  ].filter((part): part is string => !!part);
  const ownerLabel = row.ownerDisplayName ?? "Unknown owner";
  const shutDown = async () => {
    setStopAccepted(true);
    try {
      const outcome = await stop.mutateAsync(undefined);
      const toast = describeStopOutcome(outcome, row.workspaceName);
      onNotify(toast.severity, toast.title, toast.message);
    } catch (error) {
      setStopAccepted(false);
      onNotify(
        "error",
        "Could not stop workspace",
        error instanceof Error ? error.message : String(error),
      );
    } finally {
      onStopped();
    }
  };
  return (
    <li
      data-testid={`home-compute-${row.workspaceId}`}
      data-status={stopping ? "stopping" : row.status}
      className="flex min-w-0 items-center gap-3 px-4 py-3"
    >
      <span
        aria-hidden="true"
        data-testid="home-compute-status-dot"
        className="h-2 w-2 flex-none rounded-full"
        style={{ backgroundColor: state.dot }}
      />
      <div className="min-w-0 flex-1">
        <div className="flex min-w-0 items-center gap-2 text-sm font-semibold text-[var(--g5-text0)]">
          <Link
            to={`/${orgSlug}/${encodeURIComponent(row.agentReview?.sourceWorkspaceName ?? row.workspaceName)}${row.agentReview ? "?view=approvals" : ""}`}
            data-testid="home-compute-open"
            aria-label={`Open ${row.workspaceName}`}
            className="min-w-0 truncate no-underline hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]"
          >
            {row.workspaceName}
          </Link>
          {row.agentReview && (
            <span className="flex-none text-xs font-normal">Agent review</span>
          )}
        </div>
        <p
          data-testid="home-compute-details"
          className="mt-0.5 truncate text-xs text-[var(--g5-text2)]"
        >
          <span className="sr-only">{STATUS_WORD[row.status]}. </span>
          {details.join(" · ")}
        </p>
      </div>
      <span
        className="flex flex-none items-center gap-1.5 text-xs text-[var(--g5-text1)]"
        title={ownerLabel}
      >
        <IdentityAvatar
          identity={row.ownerDisplayName ?? row.ownerUserId ?? "?"}
          size={18}
        />
        <span className="sr-only">Owner: </span>
        <span className="hidden max-w-[9rem] truncate sm:inline">
          {ownerLabel}
        </span>
      </span>
      {owned && !row.agentReview && (
        <button
          type="button"
          data-testid="home-compute-shutdown"
          aria-label={`Stop ${row.workspaceName}`}
          disabled={stopping}
          onClick={() => void shutDown()}
          className={classNames(
            HOME_ROW_ACTION_CLASSES,
            "inline-flex items-center gap-1 disabled:cursor-wait disabled:opacity-60",
          )}
        >
          <Power aria-hidden="true" className="h-3 w-3" />
          {stopping ? "Stopping…" : "Stop"}
        </button>
      )}
    </li>
  );
}

export function HomeComputeTile({
  orgId,
  orgSlug,
  callerUserId,
  pageSize = Number.POSITIVE_INFINITY,
  defaultFilter = "mine",
  onNotify,
  headerAction,
}: HomeComputeTileProps) {
  const compute = useRunningCompute(orgId);
  const [filter, setFilter] = useState<ComputeFilter>(defaultFilter);
  const paging = useHomePages(pageSize);
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), UPTIME_TICK_MS);
    return () => window.clearInterval(timer);
  }, []);
  const rows = compute.status === "ok" ? compute.data : [];
  const filtered =
    filter === "mine"
      ? rows.filter((row) => row.ownerUserId === callerUserId)
      : rows;
  const visible = filtered.slice(0, paging.pages * pageSize);
  const remaining = Math.max(0, filtered.length - visible.length);

  return (
    <section data-testid="home-compute-tile" className="min-w-0 space-y-2">
      <HomeSectionHeader
        title="Running compute"
        count={compute.status === "ok" ? rows.length : undefined}
        countLabel="running workspace VMs"
        countTestId="home-compute-count"
        actions={
          <div className="flex w-full flex-wrap items-center gap-2 sm:ml-auto sm:w-auto sm:justify-end">
            {headerAction}
            <div
              role="group"
              aria-label="Filter running compute"
              className="flex items-center gap-1"
            >
              {COMPUTE_FILTERS.map((option) => (
                <HomeFilterChip
                  key={option.value}
                  testId={`home-compute-filter-${option.value}`}
                  selected={option.value === filter}
                  label={option.label}
                  onClick={() => {
                    setFilter(option.value);
                    paging.reset();
                  }}
                />
              ))}
            </div>
          </div>
        }
      />
      <div className="overflow-hidden rounded-lg border border-[var(--g5-stroke)] bg-[var(--g5-bg1)]">
        {compute.status === "loading" && (
          <p role="status" className="px-4 py-5 text-sm text-[var(--g5-text2)]">
            Loading running compute…
          </p>
        )}
        {compute.status === "error" && (
          <div
            role="alert"
            aria-label="Running compute error"
            className="flex items-center justify-between gap-3 px-4 py-4"
          >
            <span className="text-sm text-[var(--g5-text1)]">
              Running compute is temporarily unavailable.
            </span>
            <button
              type="button"
              data-testid="home-compute-retry"
              onClick={compute.refetch}
              className="rounded border border-[var(--g5-stroke)] px-2.5 py-1 text-sm text-[var(--g5-text0)] hover:bg-[var(--g5-bg2)]"
            >
              Retry
            </button>
          </div>
        )}
        {compute.status === "ok" && visible.length === 0 && (
          <p
            data-testid="home-compute-empty"
            className="px-4 py-5 text-sm text-[var(--g5-text2)]"
          >
            {filter === "mine"
              ? "Nothing is running under your name."
              : "No workspace VMs are running."}
          </p>
        )}
        {compute.status === "ok" && visible.length > 0 && (
          <ul className="divide-y divide-[var(--g5-stroke)]">
            {visible.map((row) => (
              <ComputeRow
                key={row.workspaceId}
                orgSlug={orgSlug}
                row={row}
                owned={row.ownerUserId === callerUserId}
                now={now}
                onNotify={onNotify}
                onStopped={compute.refetch}
              />
            ))}
          </ul>
        )}
        {compute.status === "ok" && (
          <HomeShowMore
            remaining={remaining}
            pageSize={pageSize}
            noun="workspace"
            testId="home-compute-show-more"
            onShowMore={paging.showMore}
          />
        )}
      </div>
    </section>
  );
}
