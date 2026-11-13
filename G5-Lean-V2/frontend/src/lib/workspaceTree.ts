// =============================================================================
// G5 Client — Workspace lineage tree
// =============================================================================
// Flattens an organization's workspace list into an indented tree ordered by
// clone lineage. Shared by the fork dialog's target picker (top-bar D31) and
// the organization fork picker (workbench g5.workbench.tabs).
// =============================================================================

import type { WorkspaceSummary } from "./types";
import { withoutPersonalWorkspaces } from "./workspaceKind";

/** One flattened row of the workspace tree. */
export interface WorkspaceTreeRow {
  workspace: WorkspaceSummary;
  depth: number;
}

/**
 * Flatten the org workspace list into an indented tree ordered by clone
 * lineage (`clonedFromId`): roots first, children under their parent, sorted
 * by name at each level. Workspaces whose parent is absent from the list
 * render as roots; a visited-set guards against pathological lineage cycles.
 * Personal workspaces have no lineage and are never fork sources, so the tree
 * leaves them out (personal_workspace g5.personal.home).
 */
export function buildWorkspaceTreeRows(
  allWorkspaces: WorkspaceSummary[],
  options: { baseFirst?: boolean } = {},
): WorkspaceTreeRow[] {
  const workspaces = withoutPersonalWorkspaces(allWorkspaces);
  // Formal review inventory rows grant inspection through their source,
  // so they cannot be selected as live fork sources or merge targets.
  const selectable = workspaces.filter((workspace) => !workspace.agentReview);
  const byId = new Map(selectable.map((w) => [w.id, w]));
  const childrenOf = new Map<string, WorkspaceSummary[]>();
  const roots: WorkspaceSummary[] = [];
  for (const w of selectable) {
    if (w.clonedFromId && byId.has(w.clonedFromId)) {
      const list = childrenOf.get(w.clonedFromId) ?? [];
      list.push(w);
      childrenOf.set(w.clonedFromId, list);
    } else {
      roots.push(w);
    }
  }
  const byName = (a: WorkspaceSummary, b: WorkspaceSummary) =>
    a.name.localeCompare(b.name);
  // The fork picker reads Base → forks (mock story B4); the fork dialog's
  // target picker keeps every root by name.
  const rootOrder = options.baseFirst
    ? (a: WorkspaceSummary, b: WorkspaceSummary) =>
        Number(b.isBase) - Number(a.isBase) || byName(a, b)
    : byName;
  const rows: WorkspaceTreeRow[] = [];
  const visited = new Set<string>();
  const visit = (w: WorkspaceSummary, depth: number) => {
    if (visited.has(w.id)) return;
    visited.add(w.id);
    rows.push({ workspace: w, depth });
    for (const child of (childrenOf.get(w.id) ?? []).sort(byName)) {
      visit(child, depth + 1);
    }
  };
  for (const root of roots.sort(rootOrder)) visit(root, 0);
  // A pathological lineage cycle has no root and would otherwise vanish from
  // the picker; render any unreached workspace flattened at root depth so all
  // live org workspaces stay selectable (top-bar D31).
  for (const workspace of [...selectable].sort(byName)) {
    if (!visited.has(workspace.id)) visit(workspace, 0);
  }
  return rows;
}

/**
 * Whether a workspace matches a search. Both the local repo/branch names and
 * the remote identity count, so the match follows whichever the person
 * types; an empty query matches everything.
 */
export function workspaceMatchesSearch(
  workspace: WorkspaceSummary,
  normalizedQuery: string,
): boolean {
  if (!normalizedQuery) return true;
  const branchText = workspace.branches
    .map(
      (branch) =>
        `${branch.repoName} ${branch.branchName} ${branch.remoteRepoName ?? ""} ${branch.remoteBranch ?? ""}`,
    )
    .join(" ");
  return `${workspace.name} ${branchText}`
    .toLowerCase()
    .includes(normalizedQuery);
}

/**
 * Narrow flattened tree rows to the matches of `query` and their ancestors,
 * so a hit still reads in its lineage (workbench g5.workbench.shell-chrome).
 * Rows keep their order and depth; an empty query returns every row.
 */
export function filterWorkspaceTreeRows(
  rows: WorkspaceTreeRow[],
  query: string,
): WorkspaceTreeRow[] {
  const normalized = query.trim().toLowerCase();
  if (!normalized) return rows;
  const keep = new Set<number>();
  rows.forEach((row, index) => {
    if (!workspaceMatchesSearch(row.workspace, normalized)) return;
    keep.add(index);
    // Walk back to each shallower row: the nearest one at every lesser depth
    // is an ancestor in a depth-first flattening.
    let depth = row.depth;
    for (let cursor = index - 1; cursor >= 0 && depth > 0; cursor -= 1) {
      if (rows[cursor].depth < depth) {
        keep.add(cursor);
        depth = rows[cursor].depth;
      }
    }
  });
  return rows.filter((_, index) => keep.has(index));
}
