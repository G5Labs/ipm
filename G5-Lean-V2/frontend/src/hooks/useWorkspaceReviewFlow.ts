import { useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../lib/api";
import { externalIssueKeys } from "./useExternalIssues";
import type { ReviewHead, ReviewOverview } from "../lib/types";
export type {
  ReviewHead,
  ReviewRequest,
  ReviewDecision,
  ReviewOverview,
  ReviewComment,
  VectorRevision,
  ReviewFile,
  ReviewChanges,
} from "../lib/types";
export const reviewUrl = (id: string) => `/api/v1/workspaces/${id}/review-flow`;
export function useWorkspaceReviewFlow(id: string) {
  return useQuery({
    queryKey: ["workspace-review-flow", id],
    queryFn: ({ signal }) => api.get<ReviewOverview>(reviewUrl(id), { signal }),
    enabled: !!id,
    refetchInterval: 5000,
    refetchOnWindowFocus: true,
  });
}
export function useRefreshReview(id: string) {
  const client = useQueryClient();
  return async () => {
    await Promise.all([
      client.invalidateQueries({ queryKey: ["workspace-review-flow", id] }),
      client.invalidateQueries({ queryKey: ["workspace-approvals", id] }),
      client.invalidateQueries({ queryKey: ["workspace-review-comments", id] }),
      client.invalidateQueries({ queryKey: ["workspace-review-replies"] }),
      client.invalidateQueries({ queryKey: ["workspace-review-diff", id] }),
      client.invalidateQueries({ queryKey: ["workspace-review-files", id] }),
      client.invalidateQueries({ queryKey: externalIssueKeys.activity(id) }),
      client.invalidateQueries({ queryKey: ["workspace-pull-requests", id] }),
    ]);
  };
}
export const changedRepositories = (
  before: ReviewHead[],
  after: ReviewHead[],
) =>
  [
    ...new Set([...before.map((h) => h.repoId), ...after.map((h) => h.repoId)]),
  ].filter(
    (id) =>
      before.find((h) => h.repoId === id)?.headCommit !==
      after.find((h) => h.repoId === id)?.headCommit,
  );
