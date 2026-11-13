// =============================================================================
// G5 Client — Workspace registry kind
// =============================================================================
// Presentation follows the registry kind, never a workspace's name, owner or
// ancestry (personal_workspace g5.personal.identity). Older servers send no
// `kind`; their workspaces are Base or working, told apart by `isBase`.
// =============================================================================

import type { WorkspaceKind } from "./types";

interface KindSource {
  isBase?: boolean;
  kind?: WorkspaceKind | null;
}

/** The workspace's registry kind, derived from `isBase` when not reported. */
export function workspaceKind(workspace: KindSource): WorkspaceKind {
  if (
    workspace.kind === "base" ||
    workspace.kind === "working" ||
    workspace.kind === "personal"
  ) {
    return workspace.kind;
  }
  return workspace.isBase ? "base" : "working";
}

/** True for a member's personal workspace (g5.personal.identity). */
export function isPersonalWorkspace(
  workspace: KindSource | null | undefined,
): boolean {
  return !!workspace && workspaceKind(workspace) === "personal";
}

/**
 * Leave personal workspaces out of an organization workspace list: they are
 * never in the Home table, the lineage tree, the fork picker, the switcher,
 * clone targets or merge-target pickers (g5.personal.home,
 * g5.personal.refusals).
 */
export function withoutPersonalWorkspaces<T extends KindSource>(
  workspaces: readonly T[],
): T[] {
  return workspaces.filter((workspace) => !isPersonalWorkspace(workspace));
}
