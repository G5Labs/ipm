// =============================================================================
// Reading again after a change
// =============================================================================

import type { QueryClient, QueryKey } from "@tanstack/react-query";

/**
 * Invalidates the queries under `queryKey` so that each is read again after
 * this moment. TanStack keeps a first read that is already out instead of
 * starting another, and that read's answer, from before the change, would
 * end the invalidation with nothing left to read again. Such a read is
 * cancelled first, so the next one goes out after the change
 * (`g5.documents.comments.live`).
 */
export async function invalidateAndReadAgain(
  queryClient: QueryClient,
  queryKey: QueryKey,
  exact = false,
): Promise<void> {
  // Only an exact read names `exact`, so prefix filters stay `{ queryKey }`.
  const filters = exact ? { queryKey, exact } : { queryKey };
  const firstReadsOut = queryClient
    .getQueryCache()
    .findAll(filters)
    .filter(
      (query) =>
        query.state.data === undefined &&
        query.state.fetchStatus === "fetching",
    );
  await Promise.all(
    firstReadsOut.map((query) =>
      queryClient.cancelQueries({ queryKey: query.queryKey, exact: true }),
    ),
  );
  await queryClient.invalidateQueries(filters);
}
