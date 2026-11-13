import type { WorkspaceSummary } from "../../lib/types";
import { isPersonalWorkspace } from "../../lib/workspaceKind";

export interface WorkspaceRemovePolicyContext {
  workspaceListLoaded: boolean;
  workspaceIdsWithChildren: ReadonlySet<string>;
  routeOrgActive: boolean;
  hasWorkspaceWrite: boolean;
  userId: string | null | undefined;
}

export function workspaceIdsWithLiveChildren(
  workspaces: Pick<WorkspaceSummary, "clonedFromId">[],
): Set<string> {
  const parentIds = new Set<string>();
  for (const workspace of workspaces) {
    if (workspace.clonedFromId) parentIds.add(workspace.clonedFromId);
  }
  return parentIds;
}

/** Shared eligibility policy for the switcher's destructive row action. */
export function canRemoveWorkspaceFromSwitcher(
  candidate: WorkspaceSummary,
  context: WorkspaceRemovePolicyContext,
): boolean {
  if (!context.workspaceListLoaded || candidate.isBase) return false;
  // A personal workspace is never owner-deleted (g5.personal.refusals).
  if (isPersonalWorkspace(candidate)) return false;
  if (context.workspaceIdsWithChildren.has(candidate.id)) return false;
  const isOwner = !!context.userId && candidate.ownerUserId === context.userId;
  return context.routeOrgActive && context.hasWorkspaceWrite && isOwner;
}
