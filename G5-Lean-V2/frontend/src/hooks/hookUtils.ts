// =============================================================================
// G5 Client — Shared Hook Utilities
// =============================================================================
// toAsyncState and toMutationHook helpers used by all domain hook files.
// Ensures consistent mapping between React Query internals and the
// application's AsyncState<T> / MutationHook<TInput, TOutput> type system.
// =============================================================================

import type { UseQueryResult, UseMutationResult } from "@tanstack/react-query";
import type { AsyncState, MutationHook } from "../lib/types";
import { ApiError } from "../lib/api";

/**
 * Map a React Query UseQueryResult to the application's AsyncState<T> type.
 *
 * - isPending → { status: 'loading' }
 * - isError   → { status: 'error', error }
 * - otherwise → { status: 'ok', data }
 */
export function toAsyncState<T>(
  query: UseQueryResult<T, Error>,
): AsyncState<T> {
  if (query.isPending) return { status: "loading" };
  if (query.isError) return { status: "error", error: query.error };
  return { status: "ok", data: query.data };
}

/** An {@link AsyncState} whose loaded data outlives a failed refresh. */
export type RefreshingState<T> =
  | { status: "loading" }
  | { status: "error"; error: Error }
  | { status: "ok"; data: T; refreshError: Error | null };

/**
 * Whether a failed request may succeed when retried unchanged: no answer
 * (a network failure, which the API client reports with status 0), a
 * timeout, rate limiting or a server error. Refusals (401, 403, 404 and
 * other 4xx) are not transient: access was lost or the thing is gone. Nor is
 * an error the client raised itself, such as a name that resolved to nothing.
 */
export function isTransientFailure(error: Error): boolean {
  if (!(error instanceof ApiError)) return false;
  const { status } = error;
  return status === 0 || status === 408 || status === 429 || status >= 500;
}

/**
 * {@link toAsyncState} for what people read and act on in a workspace (the
 * workspace, its files, their comment threads and review suggestions): once
 * loaded, a transient refetch failure keeps the loaded data and reports the
 * failure beside it, so the page, an open thread and what is typed in them
 * stay on screen (`g5.workspace.page.refresh`,
 * `g5.documents.comments.failure`). A refusal still replaces the data with
 * the error.
 */
export function toRefreshingState<T>(
  query: UseQueryResult<T, Error>,
): RefreshingState<T> {
  if (query.isPending) return { status: "loading" };
  if (
    query.isError &&
    (query.data === undefined || !isTransientFailure(query.error))
  ) {
    return { status: "error", error: query.error };
  }
  return {
    status: "ok",
    data: query.data as T,
    refreshError: query.isError ? query.error : null,
  };
}

/**
 * Map a React Query v5 UseMutationResult to the application's MutationHook type.
 *
 * Note: React Query v5 uses `isPending` (not deprecated `isLoading`).
 * The MutationHook interface uses `isLoading` for backward compatibility
 * with the types.ts contract, but we map from v5's `isPending`.
 */
export function toMutationHook<TInput, TOutput>(
  mutation: UseMutationResult<TOutput, Error, TInput>,
): MutationHook<TInput, TOutput> {
  return {
    mutate: mutation.mutate,
    mutateAsync: mutation.mutateAsync,
    isLoading: mutation.isPending,
    error: mutation.error,
    data: mutation.data,
    reset: mutation.reset,
  };
}
