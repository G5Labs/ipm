// =============================================================================
// Home tiles — pure projection helpers (workbench Phase 6)
// =============================================================================

import type {
  ExternalIssueSummaryDTO,
  RunningComputeDTO,
} from "../../lib/types";

/** Rows shown on the Issues tile; the full search lives below the grid. */
export const HOME_ISSUES_TILE_LIMIT = 8;

/** Label names of the loaded slice, most frequent first, then by name. */
export function labelChipsFrom(issues: ExternalIssueSummaryDTO[]): string[] {
  const counts = new Map<string, number>();
  for (const issue of issues) {
    for (const label of issue.labels) {
      counts.set(label.name, (counts.get(label.name) ?? 0) + 1);
    }
  }
  return [...counts.entries()]
    .sort(([a, countA], [b, countB]) => countB - countA || a.localeCompare(b))
    .map(([name]) => name);
}

/** The slice a filter shows: newest update first, every selected label, at most the limit. */
export function tileRows(
  issues: ExternalIssueSummaryDTO[],
  selectedLabels: ReadonlySet<string>,
  limit: number = HOME_ISSUES_TILE_LIMIT,
): ExternalIssueSummaryDTO[] {
  return [...issues]
    .filter((issue) =>
      [...selectedLabels].every((name) =>
        issue.labels.some((label) => label.name === name),
      ),
    )
    .sort((a, b) => Date.parse(b.updatedAt) - Date.parse(a.updatedAt))
    .slice(0, limit);
}

/** `2h 14m`, `41m`, `3d 4h` — the elapsed time since the VM became ready. */
export function formatUptime(startedAt: string, now: number): string {
  const elapsed = now - Date.parse(startedAt);
  if (!Number.isFinite(elapsed)) return "";
  const minutes = Math.max(0, Math.floor(elapsed / 60_000));
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ${String(minutes % 60).padStart(2, "0")}m`;
  const days = Math.floor(hours / 24);
  return `${days}d ${hours % 24}h`;
}

/**
 * `4 vCPU · 16 GiB` from the placed host's slot (`slot_mem_mib` is MiB, so
 * 16384 is 16 GiB — labelled as such rather than the mock's loose "GB");
 * `null` before placement.
 */
export function formatSlotSize(row: RunningComputeDTO): string | null {
  if (row.slotCpus === null || row.slotMemMib === null) return null;
  const gib = row.slotMemMib / 1024;
  const memory = Number.isInteger(gib) ? `${gib}` : gib.toFixed(1);
  return `${row.slotCpus} vCPU · ${memory} GiB`;
}

/** Tile assignee filter, derived client-side from the one loaded slice. */
export type HomeIssueAssigneeFilter = "any" | "me" | "none";

/**
 * Narrow the loaded open slice by assignee: "me" matches the caller's
 * connected GitHub login on GitHub issues (the same account the server's
 * `assignee=me` uses) and the caller's G5 user on native issues, "none" the
 * unassigned issues. A lower bound over the loaded slice, not a server
 * count — the tile makes one request per domain, not one per filter.
 */
export function filterByAssignee(
  issues: ExternalIssueSummaryDTO[],
  filter: HomeIssueAssigneeFilter,
  githubLogin: string | null,
  userId: string | null = null,
): ExternalIssueSummaryDTO[] {
  switch (filter) {
    case "any":
      return issues;
    case "none":
      return issues.filter((issue) => issue.assignees.length === 0);
    case "me": {
      const login = githubLogin?.toLowerCase() ?? null;
      return issues.filter((issue) =>
        issue.assignees.some((assignee) =>
          issue.provider === "g5"
            ? !!userId && assignee.userId === userId
            : login !== null && assignee.login.toLowerCase() === login,
        ),
      );
    }
  }
}

/** Label chips: the slice's labels by frequency, plus any selected label the slice lacks. */
export function labelChipsWithSelection(
  issues: ExternalIssueSummaryDTO[],
  selected: ReadonlySet<string>,
): string[] {
  const chips = labelChipsFrom(issues);
  const missing = [...selected]
    .filter((name) => !chips.includes(name))
    .sort((a, b) => a.localeCompare(b));
  return [...chips, ...missing];
}
