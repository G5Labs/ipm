import { useQuery } from "@tanstack/react-query";

import { ApiError, api } from "../lib/api";
import type {
  AsyncState,
  RebaseChangeGraphNodeDetailsResponse,
  RebaseChangeGraphResponse,
  SourceControlChangeGraphComparison,
  SourceControlChangeGraphNodeDetails,
  SourceControlChangeGraphResponse,
} from "../lib/types";
import { toAsyncState } from "./hookUtils";

const TERMINAL_CHANGE_GRAPH_ERROR_CODES = new Set([
  "DIFF_GRAPH_UNAVAILABLE",
  "DIFF_GRAPH_TOO_LARGE",
]);

function isTerminalChangeGraphError(error: unknown): error is ApiError {
  return (
    error instanceof ApiError &&
    TERMINAL_CHANGE_GRAPH_ERROR_CODES.has(error.code)
  );
}

type SourceControlChangeGraphQueryResult =
  | { kind: "data"; data: SourceControlChangeGraphResponse }
  | { kind: "terminal_error"; error: ApiError };

export type RetryableAsyncState<T> = AsyncState<T> & { retry: () => void };

const REBASE_CHANGE_GRAPH_REFRESH_INTERVAL_MS = 30_000;

export function useSourceControlChangeGraph(
  workspaceId: string,
  comparison: SourceControlChangeGraphComparison,
  humanIntent: boolean,
): AsyncState<SourceControlChangeGraphResponse> {
  const query = useQuery<SourceControlChangeGraphQueryResult, Error>({
    queryKey: [
      "source-control-change-graph",
      workspaceId,
      comparison,
      humanIntent,
    ],
    queryFn: async ({ signal }) => {
      try {
        const data = await api.get<SourceControlChangeGraphResponse>(
          `/api/v1/workspaces/${workspaceId}/source-control/change-graph`,
          { signal, params: { comparison, humanIntent } },
        );
        return { kind: "data", data };
      } catch (error) {
        if (isTerminalChangeGraphError(error)) {
          return { kind: "terminal_error", error };
        }
        throw error;
      }
    },
    enabled: !!workspaceId,
    retry: (failureCount, error) =>
      !isTerminalChangeGraphError(error) && failureCount < 1,
  });
  if (query.isPending) return { status: "loading" };
  if (query.isError) return { status: "error", error: query.error };
  return query.data.kind === "terminal_error"
    ? { status: "error", error: query.data.error }
    : { status: "ok", data: query.data.data };
}

export function useSourceControlChangeGraphNodeDetails(
  workspaceId: string,
  comparison: SourceControlChangeGraphComparison,
  revision: string | undefined,
  nodeId: string | undefined,
  humanIntent: boolean,
): AsyncState<SourceControlChangeGraphNodeDetails> {
  const query = useQuery<SourceControlChangeGraphNodeDetails, Error>({
    queryKey: [
      "source-control-change-graph-node-details",
      workspaceId,
      comparison,
      revision,
      nodeId,
      humanIntent,
    ],
    queryFn: ({ signal }) =>
      api.get<SourceControlChangeGraphNodeDetails>(
        `/api/v1/workspaces/${workspaceId}/source-control/change-graph/nodes/${encodeURIComponent(nodeId!)}/details`,
        { signal, params: { revision, comparison, humanIntent } },
      ),
    enabled: !!workspaceId && !!revision && !!nodeId,
  });
  return toAsyncState(query);
}

export function useRebaseChangeGraph(
  sourceWorkspaceId: string,
  targetWorkspaceId: string,
): RetryableAsyncState<RebaseChangeGraphResponse> {
  const query = useQuery<RebaseChangeGraphResponse, Error>({
    queryKey: ["rebase-change-graph", sourceWorkspaceId, targetWorkspaceId],
    queryFn: ({ signal }) =>
      api.get<RebaseChangeGraphResponse>(
        `/api/v1/workspaces/${sourceWorkspaceId}/rebase-merge/change-graph`,
        { signal, params: { targetWorkspaceId } },
      ),
    enabled: !!sourceWorkspaceId && !!targetWorkspaceId,
    refetchInterval: REBASE_CHANGE_GRAPH_REFRESH_INTERVAL_MS,
    refetchIntervalInBackground: false,
  });
  return {
    ...toAsyncState(query),
    retry: () => {
      void query.refetch();
    },
  };
}

export function useRebaseChangeGraphNodeDetails(
  sourceWorkspaceId: string,
  targetWorkspaceId: string,
  revision: string | undefined,
  artifactKey: string | undefined,
): AsyncState<RebaseChangeGraphNodeDetailsResponse> {
  const query = useQuery<RebaseChangeGraphNodeDetailsResponse, Error>({
    queryKey: [
      "rebase-change-graph-node-details",
      sourceWorkspaceId,
      targetWorkspaceId,
      revision,
      artifactKey,
    ],
    queryFn: ({ signal }) =>
      api.get<RebaseChangeGraphNodeDetailsResponse>(
        `/api/v1/workspaces/${sourceWorkspaceId}/rebase-merge/change-graph/nodes/${encodeURIComponent(artifactKey!)}/details`,
        { signal, params: { targetWorkspaceId, revision } },
      ),
    enabled:
      !!sourceWorkspaceId && !!targetWorkspaceId && !!revision && !!artifactKey,
  });
  return toAsyncState(query);
}
