// =============================================================================
// G5 Settings Panel — SpendBreakdown
// =============================================================================
// A clean, presentational per-task cost table. It renders caller-supplied
// per-task cost entries — each entry's `costUsd` is the task's server-computed
// persisted cost (`usage_records.cost_usd`, surfaced as `WorkspaceStats.byTask`) — as a
// sortable Task Name / Type / Cost table. It performs no data fetching.
// The headline total can also include ordinary conversations with
// no task identity; their cost is shown separately from retained task rows.
//
// Ontology:
//   @ref software_architecture/client/workspace-page/settings-panel#D6
//   @ref software_architecture/client/workspace-page/settings-panel#D8
//   @ref software_architecture/client/workspace-page/settings-panel#D14
// =============================================================================

import React, { useMemo, useState } from "react";
import { ChevronDown, ChevronUp } from "lucide-react";
import type { TaskCostBreakdown } from "../../lib/types";
import { formatCost } from "../../lib/utils";

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

export interface SpendBreakdownProps {
  /**
   * Per-task cost entries. Each entry's `costUsd` is the task's server-computed
   * persisted cost (`usage_records.cost_usd`).
   */
  byTask: TaskCostBreakdown[];
  totalCostUsd?: number;
}

// ---------------------------------------------------------------------------
// Sort direction type
// ---------------------------------------------------------------------------

type SortDir = "asc" | "desc";

// ---------------------------------------------------------------------------
// SpendBreakdown Component
// ---------------------------------------------------------------------------

export const SpendBreakdown: React.FC<SpendBreakdownProps> = ({
  byTask: byTaskProp,
  totalCostUsd,
}) => {
  // The hook validates the stats body, but a caller could still pass a stale
  // shape (issue #733); anything but a list renders as no retained tasks
  // rather than taking the page down.
  const byTask = useMemo<TaskCostBreakdown[]>(
    () => (Array.isArray(byTaskProp) ? byTaskProp : []),
    [byTaskProp],
  );
  // Sort state for the by-task table.
  const [sortDir, setSortDir] = useState<SortDir>("desc");

  const sortedTasks = useMemo(() => {
    return [...byTask].sort((a, b) =>
      sortDir === "desc" ? b.costUsd - a.costUsd : a.costUsd - b.costUsd,
    );
  }, [byTask, sortDir]);

  const taskCost = byTask.reduce((sum, task) => sum + task.costUsd, 0);
  const remainingCost = Math.max(0, (totalCostUsd ?? taskCost) - taskCost);
  // Ignore floating-point subtraction noise below stored cost precision.
  const otherUsage =
    remainingCost > 1e-9 ? (
      <div
        className="flex justify-between py-2 text-sm"
        data-testid="other-spend"
      >
        <span>Other usage</span>
        <span className="font-mono">{formatCost(remainingCost)}</span>
      </div>
    ) : null;

  if (byTask.length === 0) {
    if (otherUsage) return otherUsage;
    return (
      <div className="py-6 text-center">
        <p className="text-sm text-[var(--g5-text2)]">
          No spend data for this workspace
        </p>
      </div>
    );
  }

  return (
    <div>
      {otherUsage}
      <div className="text-xs font-semibold uppercase tracking-wide text-[var(--g5-text2)] mb-2">
        By Task
      </div>
      <div className="rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] overflow-hidden">
        {/* Table header */}
        <div className="flex items-center bg-[var(--g5-bg2)] px-3 py-1.5 text-xs font-medium text-[var(--g5-text2)]">
          <span className="flex-1">Task Name</span>
          <span className="w-20 text-center">Type</span>
          <button
            type="button"
            data-testid="spend-breakdown-sort-cost"
            className="w-24 text-right flex items-center justify-end gap-1 cursor-pointer hover:text-[var(--g5-text0)]"
            onClick={() => setSortDir((d) => (d === "desc" ? "asc" : "desc"))}
            aria-label="Sort by cost"
          >
            Cost
            {sortDir === "desc" ? (
              <ChevronDown className="w-3 h-3" />
            ) : (
              <ChevronUp className="w-3 h-3" />
            )}
          </button>
        </div>

        {/* Table rows */}
        {sortedTasks.map((task) => (
          <div
            key={task.taskId}
            className="flex items-center px-3 py-2 text-sm border-t border-[var(--g5-stroke)]"
          >
            <span className="flex-1 text-[var(--g5-text0)] truncate">
              {task.taskName}
            </span>
            <span className="w-20 text-center text-xs text-[var(--g5-text2)]">
              {task.taskType}
            </span>
            <span className="w-24 text-right font-mono text-[var(--g5-text0)]">
              {formatCost(task.costUsd)}
            </span>
          </div>
        ))}
      </div>
    </div>
  );
};
