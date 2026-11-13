// =============================================================================
// G5 Workspace Page — WorkspaceTable
// =============================================================================
// A reusable table component consumed by WorkspaceSwitcher showing workspace
// rows with columns: Name, Branch Mode, Concurrent Tasks,
// Updated, and Actions.
//
// Features:
// - Search filtering by workspace name
// - Branch mode badges ("TRACKING" vs "pinned")
// - Row actions: Abort, Clone, more menu with Delete
// - Aggregate stats row at top
// - Row click triggers onSelectWorkspace
// =============================================================================

import { Copy, MoreHorizontal, Square, Trash2 } from "lucide-react";
import React, { useMemo, useState } from "react";
import {
  getWorkspaceUiCapabilities,
  type WorkspaceUiCapabilities,
} from "../../lib/capabilities";
import type { WorkspaceSummary } from "../../lib/types";
import { classNames } from "../ui/UI";
import { ReadOnlyBadge } from "./ReadOnlyBadge";
import { withoutPersonalWorkspaces } from "../../lib/workspaceKind";

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

export interface WorkspaceTableProps {
  /** List of workspaces to display */
  workspaces: WorkspaceSummary[];
  /** Search query string for filtering */
  searchQuery: string;
  /** Called when user selects a workspace row */
  onSelectWorkspace: (workspaceId: string) => void;
  /** Reserved for lifecycle compatibility; v2 renders no manual Pause control */
  onPauseWorkspace: (workspaceId: string) => void;
  /** Called when user aborts a workspace's active tasks */
  onAbortWorkspace: (workspaceId: string) => void;
  /** Called when user clicks clone action */
  onCloneWorkspace: (workspaceId: string) => void;
  /** Called when user clicks "New Workspace" */
  onNewWorkspace: () => void;
  /** UI capability gates for cloud/project workflow affordances */
  capabilities?: WorkspaceUiCapabilities;
}

// ---------------------------------------------------------------------------
// MoreMenu — dropdown with Delete option
// ---------------------------------------------------------------------------

const MoreMenu: React.FC<{
  workspaceId: string;
  onDelete?: (workspaceId: string) => void;
}> = ({ workspaceId, onDelete }) => {
  const [open, setOpen] = useState(false);

  return (
    <div className="relative">
      <button
        type="button"
        className={classNames(
          "inline-flex items-center justify-center",
          "w-7 h-7",
          "rounded-[var(--g5-radius-sm)]",
          "text-[var(--g5-text2)]",
          "hover:bg-[var(--g5-bg2)] hover:text-[var(--g5-text1)]",
          "transition-colors duration-150",
          "cursor-pointer",
        )}
        onClick={(e) => {
          e.stopPropagation();
          setOpen(!open);
        }}
        data-testid="workspace-table-more-actions"
        title="More actions"
        aria-label="More actions"
      >
        <MoreHorizontal size={14} />
      </button>

      {open && (
        <>
          {/* Backdrop */}
          <div className="fixed inset-0 z-40" onClick={() => setOpen(false)} />
          {/* Menu */}
          <div
            className={classNames(
              "absolute right-0 top-full mt-1 z-50",
              "w-40",
              "bg-[var(--g5-bg1)]",
              "border border-[var(--g5-stroke)]",
              "rounded-[var(--g5-radius-sm)]",
              "shadow-lg",
              "py-1",
            )}
          >
            <button
              type="button"
              className={classNames(
                "w-full flex items-center gap-2 px-3 py-1.5",
                "text-sm text-[var(--g5-blocked)]",
                "hover:bg-[var(--g5-bg2)]",
                "transition-colors duration-100",
                "cursor-pointer",
              )}
              data-testid="workspace-table-delete"
              onClick={(e) => {
                e.stopPropagation();
                onDelete?.(workspaceId);
                setOpen(false);
              }}
            >
              <Trash2 size={14} />
              Delete
            </button>
          </div>
        </>
      )}
    </div>
  );
};

// ---------------------------------------------------------------------------
// formatRelativeTime helper
// ---------------------------------------------------------------------------

function formatRelativeTime(isoDate: string): string {
  const now = Date.now();
  const then = new Date(isoDate).getTime();
  const diffMs = now - then;
  const diffMin = Math.floor(diffMs / 60_000);
  const diffHrs = Math.floor(diffMs / 3_600_000);
  const diffDays = Math.floor(diffMs / 86_400_000);

  if (diffMin < 1) return "just now";
  if (diffMin < 60) return `${diffMin}m ago`;
  if (diffHrs < 24) return `${diffHrs}h ago`;
  if (diffDays < 30) return `${diffDays}d ago`;
  return new Date(isoDate).toLocaleDateString();
}

// ---------------------------------------------------------------------------
// WorkspaceTable Component
// ---------------------------------------------------------------------------

/**
 * Filterable workspace list table with search, branch mode badges, and row actions.
 *
 * Columns: Name, Branch Mode badge, Concurrent Tasks, Updated, Actions.
 *
 * Tracking-mode rows show "—" for Concurrent Tasks.
 * Aggregate stats row at top shows active tasks.
 */
export const WorkspaceTable: React.FC<WorkspaceTableProps> = ({
  workspaces,
  searchQuery,
  onSelectWorkspace,
  onPauseWorkspace,
  onAbortWorkspace,
  onCloneWorkspace,
  onNewWorkspace,
  capabilities = getWorkspaceUiCapabilities(),
}) => {
  void onNewWorkspace;
  void onPauseWorkspace;
  const showCloudWorkspaceWorkflows = capabilities.cloudWorkspaceWorkflows;
  // -----------------------------------------------------------------------
  // Filter workspaces
  // -----------------------------------------------------------------------

  const filteredWorkspaces = useMemo(() => {
    // Personal workspaces are reached from Home or `/{org}/me`, never listed
    // with the organization's workspaces (g5.personal.home).
    let result = withoutPersonalWorkspaces(workspaces);

    // Search filter
    if (searchQuery.trim()) {
      const q = searchQuery.toLowerCase();
      result = result.filter((ws) => ws.name.toLowerCase().includes(q));
    }

    return result;
  }, [workspaces, searchQuery]);

  // -----------------------------------------------------------------------
  // Aggregate stats
  // -----------------------------------------------------------------------

  const totalActiveTasks = useMemo(
    () =>
      filteredWorkspaces.reduce(
        (sum, ws) => sum + (ws.concurrentTasks ?? 0),
        0,
      ),
    [filteredWorkspaces],
  );

  // -----------------------------------------------------------------------
  // Determine if workspace is tracking mode
  // -----------------------------------------------------------------------

  const isTrackingMode = (ws: WorkspaceSummary): boolean => {
    // §38.36: Derive tracking mode from branch data instead of hardcoding false.
    // G4 (C1): `remote_tracking` is a first-class tracked mode and must count
    // here too, otherwise remote-tracking workspaces are hidden from the
    // "tracked workspaces" surface.
    return ws.branches.some(
      (b) =>
        b.trackingMode === "tracking" || b.trackingMode === "remote_tracking",
    );
  };

  // G4: surface remote-tracking distinctly from legacy local `tracking`.
  const isRemoteTrackingMode = (ws: WorkspaceSummary): boolean =>
    ws.branches.some((b) => b.trackingMode === "remote_tracking");

  // -----------------------------------------------------------------------
  // Render
  // -----------------------------------------------------------------------

  return (
    <div className="flex flex-col">
      {/* Aggregate stats row */}
      <div
        className={classNames(
          "flex items-center gap-4 px-4 py-2",
          "bg-[var(--g5-bg0)]",
          "border-b border-[var(--g5-stroke)]",
          "text-xs text-[var(--g5-text2)]",
        )}
      >
        <span>
          <strong className="text-[var(--g5-text1)]">
            {filteredWorkspaces.length}
          </strong>{" "}
          workspace{filteredWorkspaces.length !== 1 ? "s" : ""}
        </span>
        <span>
          Active tasks:{" "}
          <strong className="text-[var(--g5-text1)]">{totalActiveTasks}</strong>
        </span>
      </div>

      {/* Table */}
      <div className="overflow-x-auto">
        <table className="w-full text-sm">
          <thead>
            <tr
              className={classNames(
                "border-b border-[var(--g5-stroke)]",
                "text-xs text-[var(--g5-text2)] uppercase tracking-wide",
              )}
            >
              <th className="text-left px-4 py-2 font-medium">Name</th>
              <th className="text-left px-4 py-2 font-medium">Mode</th>
              <th className="text-right px-4 py-2 font-medium">Tasks</th>
              <th className="text-left px-4 py-2 font-medium">Updated</th>
              <th className="text-right px-4 py-2 font-medium">Actions</th>
            </tr>
          </thead>
          <tbody>
            {filteredWorkspaces.map((ws) => {
              const tracking = isTrackingMode(ws);
              const remoteTracking = isRemoteTrackingMode(ws);
              const isActive = (ws.concurrentTasks ?? 0) > 0;

              return (
                <tr
                  key={ws.id}
                  className={classNames(
                    "border-b border-[var(--g5-stroke)]",
                    "hover:bg-[var(--g5-bg2)]",
                    "transition-colors duration-100",
                    "cursor-pointer",
                  )}
                  onClick={() => onSelectWorkspace(ws.id)}
                >
                  {/* Name */}
                  <td className="px-4 py-2">
                    <div className="flex items-center gap-2">
                      <span className="font-medium text-[var(--g5-text0)]">
                        {ws.name}
                      </span>
                      {ws.agentReview && (
                        <span className="text-xs text-[var(--g5-text2)]">
                          Agent review
                        </span>
                      )}
                      {ws.readOnly === true && !ws.agentReview && (
                        <ReadOnlyBadge />
                      )}
                    </div>
                  </td>

                  {/* Branch Mode */}
                  <td className="px-4 py-2">
                    {tracking ? (
                      <span
                        data-testid="workspace-tracking-badge"
                        data-tracking-mode={
                          remoteTracking ? "remote_tracking" : "tracking"
                        }
                        className={classNames(
                          "inline-flex items-center",
                          "px-1.5 py-0.5",
                          "text-[10px] font-semibold uppercase tracking-wider",
                          "rounded-[var(--g5-radius-sm)]",
                          remoteTracking
                            ? "bg-[var(--g5-warn)]/10 text-[var(--g5-warn)]"
                            : "bg-[var(--g5-info)]/10 text-[var(--g5-info)]",
                        )}
                      >
                        {remoteTracking ? "REMOTE" : "TRACKING"}
                      </span>
                    ) : (
                      <span className="text-xs text-[var(--g5-text2)]">
                        pinned
                      </span>
                    )}
                  </td>

                  {/* Concurrent Tasks */}
                  <td className="px-4 py-2 text-right">
                    <span className="text-[var(--g5-text1)]">
                      {tracking ? "—" : (ws.concurrentTasks ?? 0)}
                    </span>
                  </td>

                  {/* Updated */}
                  <td className="px-4 py-2">
                    <span className="text-[var(--g5-text2)]">
                      {formatRelativeTime(ws.lastUpdated)}
                    </span>
                  </td>

                  {/* Actions */}
                  <td className="px-4 py-2">
                    <div
                      className="flex items-center justify-end gap-1"
                      onClick={(e) => e.stopPropagation()}
                    >
                      {/* Abort (only for active workspaces) */}
                      {showCloudWorkspaceWorkflows &&
                        !ws.agentReview &&
                        isActive && (
                          <button
                            type="button"
                            className={classNames(
                              "inline-flex items-center justify-center",
                              "w-7 h-7",
                              "rounded-[var(--g5-radius-sm)]",
                              "text-[var(--g5-text2)]",
                              "hover:bg-[var(--g5-bg1)] hover:text-[var(--g5-blocked)]",
                              "transition-colors duration-150",
                              "cursor-pointer",
                            )}
                            onClick={() => onAbortWorkspace(ws.id)}
                            data-testid="workspace-table-abort"
                            title="Abort workspace tasks"
                            aria-label="Abort workspace tasks"
                          >
                            <Square size={14} />
                          </button>
                        )}

                      {/* Clone */}
                      {showCloudWorkspaceWorkflows && !ws.agentReview && (
                        <button
                          type="button"
                          className={classNames(
                            "inline-flex items-center justify-center",
                            "w-7 h-7",
                            "rounded-[var(--g5-radius-sm)]",
                            "text-[var(--g5-text2)]",
                            "hover:bg-[var(--g5-bg1)] hover:text-[var(--g5-text0)]",
                            "transition-colors duration-150",
                            "cursor-pointer",
                          )}
                          onClick={() => onCloneWorkspace(ws.id)}
                          data-testid="workspace-table-fork"
                          title="Fork workspace"
                          aria-label="Fork workspace"
                        >
                          <Copy size={14} />
                        </button>
                      )}

                      {/* More menu */}
                      {!ws.agentReview && <MoreMenu workspaceId={ws.id} />}
                    </div>
                  </td>
                </tr>
              );
            })}

            {filteredWorkspaces.length === 0 && (
              <tr>
                <td
                  colSpan={5}
                  className="px-4 py-8 text-center text-sm text-[var(--g5-text2)]"
                >
                  No workspaces match the current filters.
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
};
