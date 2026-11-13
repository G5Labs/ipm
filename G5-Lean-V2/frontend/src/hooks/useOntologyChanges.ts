import { useQuery } from "@tanstack/react-query";
import { api, ApiError } from "../lib/api";
import {
  compareOntologyStatements,
  type ComparisonStatement,
} from "../lib/ontologyComparison";

export interface OntologyChangesResponse {
  revision: string;
  repositories: {
    repoId: string;
    repoName: string;
    sourceCommit: string;
    parentCommit: string;
    baselineCommit: string;
  }[];
  files: {
    repoId: string;
    path: string;
    source: string;
    current: string | null;
    baseline: string | null;
  }[];
}
export interface OntologyChange {
  status: "Added" | "Modified" | "Deleted";
  current?: ComparisonStatement;
  before?: ComparisonStatement;
}
export function selectOntologyChanges(response: OntologyChangesResponse) {
  const files = response.files
    .map((file) => {
      const entries: OntologyChange[] = [];
      let error: string | undefined;
      try {
        const comparison = compareOntologyStatements(
          file.current ?? "",
          file.baseline,
        );
        const appendDeleted = (statements: ComparisonStatement[] = []) =>
          statements.forEach((before) =>
            entries.push({ status: "Deleted", before }),
          );
        for (const current of comparison.current) {
          appendDeleted(comparison.deletedBefore.get(current.record.id));
          const status = comparison.changes.get(current.record.id);
          if (status)
            entries.push({
              status,
              current,
              before: comparison.before.get(current.record.id),
            });
          appendDeleted(comparison.deletedAfter.get(current.record.id));
        }
        appendDeleted(comparison.deletedAtEnd);
      } catch (cause) {
        error =
          cause instanceof Error
            ? cause.message
            : "Statement comparison unavailable";
      }
      const title = file.path
        .replace(/^\.g5\/ontology\//, "")
        .replace(/(?:\.g5)?\.md$/, "");
      return { ...file, entries, error, title };
    })
    .filter((file) => file.entries.length || file.error)
    .sort(
      (a, b) =>
        a.repoId.localeCompare(b.repoId) ||
        (a.path === ".g5/ontology/root.g5.md"
          ? -1
          : b.path === ".g5/ontology/root.g5.md"
            ? 1
            : a.path.localeCompare(b.path)),
    );
  return {
    ...response,
    files,
    count: files.reduce((sum, file) => sum + file.entries.length, 0),
    incomplete: files.some((file) => !!file.error),
  };
}

export function useOntologyChanges(workspaceId: string, enabled = true) {
  return useQuery({
    queryKey: ["source-control-change-graph", workspaceId, "ontology-changes"],
    queryFn: ({ signal }) =>
      api.get<OntologyChangesResponse>(
        `/api/v1/workspaces/${workspaceId}/source-control/ontology-changes`,
        { signal },
      ),
    select: selectOntologyChanges,
    enabled: enabled && !!workspaceId,
    retry: false,
    staleTime: 15_000,
    refetchOnWindowFocus: true,
    refetchInterval: enabled ? 30_000 : false,
  });
}

export function ontologyChangesError(error: Error): string {
  if (error instanceof ApiError) {
    if (typeof error.details.message === "string") return error.details.message;
    if (error.code === "DIFF_GRAPH_UNAVAILABLE")
      return "A readable parent and compatible repository history are required for All changes.";
  }
  return error.message;
}
