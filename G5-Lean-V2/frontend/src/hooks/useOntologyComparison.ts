import { useQuery } from "@tanstack/react-query";
import { api } from "../lib/api";

export interface OntologyBaseline {
  revision: string;
  workspaceId: string;
  repoId: string;
  path: string;
  baselineCommit: string;
  sourceCommit: string;
  parentCommit: string;
  content: string | null;
}
export function useOntologyComparison(
  workspaceId: string | undefined,
  repoId: string | undefined,
  path: string,
  enabled: boolean,
  sourceRevision?: string,
) {
  return useQuery({
    // Shares source-control invalidation; each response atomically carries bytes
    // and revision. No cross-document placeholder or previous-result reuse.
    queryKey: [
      "source-control-change-graph",
      workspaceId,
      "ontology-baseline",
      repoId,
      path,
      sourceRevision,
    ],
    queryFn: ({ signal }) =>
      api.get<OntologyBaseline>(
        `/api/v1/workspaces/${workspaceId}/source-control/ontology-baseline`,
        { signal, params: { repoId: repoId!, path } },
      ),
    enabled: enabled && !!workspaceId && !!repoId,
    retry: false,
    staleTime: 0,
    refetchOnWindowFocus: "always",
    refetchOnMount: "always",
    // Parent changes do not necessarily emit an event to this child workspace.
    refetchInterval: enabled ? 30_000 : false,
  });
}
