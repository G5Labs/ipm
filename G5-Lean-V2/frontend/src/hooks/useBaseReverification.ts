// =============================================================================
// G5 Client — Base re-verification status on Home (ui g5.ui.home-reverification)
// =============================================================================
// GET /orgs/:orgId/home/base-reverification, read only while the
// organization's verification switch is on. No event reaches Home, so it
// re-reads on focus and on a timer, quickly while a run is in progress.
// =============================================================================

import { useQuery } from "@tanstack/react-query";

import { api } from "../lib/api";
import { routes } from "../lib/routes";
import type { BaseReverificationStatus } from "../lib/types";

export const BASE_REVERIFICATION_IDLE_REFRESH_MS = 60_000;
export const BASE_REVERIFICATION_RUNNING_REFRESH_MS = 15_000;

export const baseReverificationQueryKey = (orgId: string) =>
  ["home-base-reverification", orgId] as const;

export function baseReverificationRefetchInterval(
  status: BaseReverificationStatus | undefined,
): number {
  return status?.enabled && status.state === "in_progress"
    ? BASE_REVERIFICATION_RUNNING_REFRESH_MS
    : BASE_REVERIFICATION_IDLE_REFRESH_MS;
}

export function useBaseReverificationStatus(orgId: string, enabled: boolean) {
  return useQuery<BaseReverificationStatus, Error>({
    queryKey: baseReverificationQueryKey(orgId),
    queryFn: ({ signal }) =>
      api.get<BaseReverificationStatus>(
        routes.orgs.homeBaseReverification(orgId),
        { signal },
      ),
    enabled: enabled && !!orgId,
    staleTime: 15_000,
    refetchOnWindowFocus: true,
    refetchInterval: (query) =>
      baseReverificationRefetchInterval(query.state.data),
  });
}
