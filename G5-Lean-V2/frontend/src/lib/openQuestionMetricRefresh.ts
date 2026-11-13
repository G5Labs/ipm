import type { QueryClient } from "@tanstack/react-query";

const pendingRefreshes = new WeakMap<QueryClient, Map<string, number>>();

/**
 * Record a semantic Project Doc answer change. The aggregate must wait for
 * the server's draft.update acknowledgement before it refetches.
 */
export function armOpenQuestionMetricRefresh(
  queryClient: QueryClient,
  workspaceId: string | null | undefined,
): void {
  if (!workspaceId) return;
  const byWorkspace = pendingRefreshes.get(queryClient) ?? new Map();
  byWorkspace.set(workspaceId, (byWorkspace.get(workspaceId) ?? 0) + 1);
  pendingRefreshes.set(queryClient, byWorkspace);
}

/** Consume one answer marker for a server-confirmed workspace draft update. */
export function consumeOpenQuestionMetricRefresh(
  queryClient: QueryClient,
  workspaceId: string,
): boolean {
  const byWorkspace = pendingRefreshes.get(queryClient);
  const pending = byWorkspace?.get(workspaceId) ?? 0;
  if (pending === 0) return false;

  if (pending === 1) {
    byWorkspace?.delete(workspaceId);
    if (byWorkspace?.size === 0) pendingRefreshes.delete(queryClient);
  } else {
    byWorkspace?.set(workspaceId, pending - 1);
  }
  return true;
}
