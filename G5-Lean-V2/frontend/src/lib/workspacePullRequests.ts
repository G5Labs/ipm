import type { WorkspacePullRequestSummary } from "./types";

/** A mutation may create a new PR while the query still contains its closed predecessor. */
export function currentWorkspacePullRequest(
  live: WorkspacePullRequestSummary | null | undefined,
  mutation: WorkspacePullRequestSummary | null | undefined,
): WorkspacePullRequestSummary | null {
  if (!mutation) return live ?? null;
  return live?.id === mutation.id ? live : mutation;
}
