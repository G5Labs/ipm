// =============================================================================
// G5 Client — organization Home projection
// =============================================================================

import { useInfiniteQuery, useQuery } from "@tanstack/react-query";

import { api } from "../lib/api";
import { routes } from "../lib/routes";
import type {
  AsyncState,
  HomeAttentionKind,
  HomeAttentionOrder,
  HomeAttentionPage,
  RunningComputeDTO,
} from "../lib/types";
import { toAsyncState } from "./hookUtils";

export const HOME_ATTENTION_PAGE_SIZE = 20;

/** Home's default attention order, for everyone (g5.ui.home-attention-order). */
export const HOME_ATTENTION_DEFAULT_ORDER: HomeAttentionOrder = "newest";

export const homeAttentionQueryKey = (
  orgId: string,
  kind: HomeAttentionKind | null,
  limit: number,
  order: HomeAttentionOrder,
) => ["home-attention", orgId, kind ?? "all", limit, order] as const;

export function homeAttentionQueryOptions(
  orgId: string,
  kind: HomeAttentionKind | null = null,
  limit: number = HOME_ATTENTION_PAGE_SIZE,
  order: HomeAttentionOrder = HOME_ATTENTION_DEFAULT_ORDER,
) {
  return {
    queryKey: homeAttentionQueryKey(orgId, kind, limit, order),
    queryFn: ({
      pageParam,
      signal,
    }: {
      pageParam: string | null;
      signal: AbortSignal;
    }) =>
      api.get<HomeAttentionPage>(routes.orgs.homeAttention(orgId), {
        signal,
        params: {
          limit,
          cursor: pageParam ?? undefined,
          kind: kind ?? undefined,
          // The server's own default is oldest; Home always names its order.
          order,
        },
      }),
    initialPageParam: null as string | null,
    getNextPageParam: (lastPage: HomeAttentionPage) =>
      lastPage.nextCursor ?? undefined,
    enabled: !!orgId,
    staleTime: 15_000,
    refetchOnWindowFocus: true,
  };
}

export function useHomeAttention(
  orgId: string,
  kind: HomeAttentionKind | null,
  enabled = true,
  limit: number = HOME_ATTENTION_PAGE_SIZE,
  order: HomeAttentionOrder = HOME_ATTENTION_DEFAULT_ORDER,
) {
  return useInfiniteQuery({
    ...homeAttentionQueryOptions(orgId, kind, limit, order),
    enabled: enabled && !!orgId,
  });
}

// ---------------------------------------------------------------------------
// Workspaces tile refresh (g5.ui.home-workspaces)
// ---------------------------------------------------------------------------

/** Re-read the list every 15 s while any workspace runs tasks, else every 60 s. */
export const HOME_WORKSPACES_WORKING_REFRESH_MS = 15_000;
export const HOME_WORKSPACES_IDLE_REFRESH_MS = 60_000;

/**
 * The Workspaces tile's working state (disclosure, presence dot, announce)
 * follows the organization list, which no event invalidates on Home: poll
 * quickly while something runs and slowly while nothing does, so work
 * started elsewhere surfaces without a focus change.
 */
export function homeWorkspacesRefetchInterval(
  rows: ReadonlyArray<{ concurrentTasks: number }> | undefined,
): number {
  return rows?.some((workspace) => workspace.concurrentTasks > 0)
    ? HOME_WORKSPACES_WORKING_REFRESH_MS
    : HOME_WORKSPACES_IDLE_REFRESH_MS;
}

// ---------------------------------------------------------------------------
// Running compute (workbench Phase 6)
// ---------------------------------------------------------------------------

export const runningComputeQueryKey = (orgId: string) =>
  ["home-running-compute", orgId] as const;

/** Poll while a VM is settling (starting/stopping); otherwise every 30 s. */
export function runningComputeRefetchInterval(
  rows: RunningComputeDTO[] | undefined,
): number {
  return rows?.some(
    (row) => row.status === "starting" || row.status === "stopping",
  )
    ? 5_000
    : 30_000;
}

/**
 * Non-stopped workspace VMs of the organization the caller may read
 * (`GET /orgs/:orgId/compute/running`, `workspaces:read`). The server filters
 * to readable workspaces; the tile only decides whether to render.
 */
export function useRunningCompute(
  orgId: string,
  enabled = true,
): AsyncState<RunningComputeDTO[]> & { refetch: () => void } {
  const query = useQuery<RunningComputeDTO[], Error>({
    queryKey: runningComputeQueryKey(orgId),
    queryFn: ({ signal }) =>
      api.get<RunningComputeDTO[]>(routes.orgs.computeRunning(orgId), {
        signal,
      }),
    enabled: !!orgId && enabled,
    refetchInterval: (query) => runningComputeRefetchInterval(query.state.data),
  });
  return { ...toAsyncState(query), refetch: () => void query.refetch() };
}
