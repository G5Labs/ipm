// =============================================================================
// Home — per-row workspace signals
// =============================================================================
// The reads a Home workspace row shows beside its name, for the rows on
// screen only: the approvals state (classic's shipping sentence, the
// alignment tile's summary), open source PRs and the caller's open-question
// total (g5.ui.home-attention: question attention is owned-only). Shared by
// the classic Recent Workspaces tile and the alignment Workspaces tile so the
// two never drift. A failed approvals read stays distinguishable from a
// loaded "nothing required" state.
// =============================================================================

import { useQueries, useQuery } from "@tanstack/react-query";
import { useMemo } from "react";

import {
  workspaceOpenQuestionMetricsQueryOptions,
  workspacePullRequestsQueryOptions,
} from "../../hooks/useWorkspace";
import { workspaceApprovalsQueryOptions } from "../../hooks/useWorkspaceApprovals";
import type {
  WorkspaceApprovalsStateDTO,
  WorkspaceSummary,
} from "../../lib/types";

export type HomeApprovalsRead =
  | { status: "loading" }
  | { status: "error"; retry: () => void }
  | { status: "ok"; state: WorkspaceApprovalsStateDTO };

export interface HomeWorkspaceSignals {
  approvals: HomeApprovalsRead;
  /** "N open PRs", "N open questions" — positive counts only. */
  signals: string[];
}

export function useHomeWorkspaceSignals(
  orgId: string,
  workspaces: readonly WorkspaceSummary[],
  /** Question totals are read and shown for this person's workspaces only. */
  callerUserId: string,
): Record<string, HomeWorkspaceSignals> {
  const approvalsQueries = useQueries({
    queries: workspaces.map((workspace) => ({
      ...workspaceApprovalsQueryOptions(workspace.id),
      enabled: !workspace.agentReview,
    })),
  });
  const pullRequestQueries = useQueries({
    queries: workspaces.map((workspace) => ({
      ...workspacePullRequestsQueryOptions(workspace.id, "source", "open"),
      enabled: !workspace.agentReview,
    })),
  });
  const ownedIds = useMemo(
    () =>
      workspaces
        .filter(
          (workspace) =>
            workspace.ownerUserId === callerUserId && !workspace.agentReview,
        )
        .map((workspace) => workspace.id),
    [workspaces, callerUserId],
  );
  const questionMetricsQuery = useQuery(
    workspaceOpenQuestionMetricsQueryOptions(orgId, ownedIds),
  );
  return useMemo(() => {
    const questionCounts = new Map(
      (questionMetricsQuery.data ?? []).map((metric) => [
        metric.workspaceId,
        metric.openQuestionsTotal,
      ]),
    );
    return Object.fromEntries(
      workspaces.map((workspace, index) => {
        const approvalsQuery = approvalsQueries[index];
        const approvals: HomeApprovalsRead =
          approvalsQuery?.isSuccess && approvalsQuery.data
            ? { status: "ok", state: approvalsQuery.data }
            : approvalsQuery?.isError
              ? { status: "error", retry: () => void approvalsQuery.refetch() }
              : { status: "loading" };
        const signals: string[] = [];
        const pullRequests = pullRequestQueries[index];
        if (pullRequests?.isSuccess && pullRequests.data.length > 0) {
          const count = pullRequests.data.length;
          signals.push(`${count} open ${count === 1 ? "PR" : "PRs"}`);
        }
        const openQuestions =
          workspace.ownerUserId === callerUserId
            ? questionCounts.get(workspace.id)
            : undefined;
        if (questionMetricsQuery.isSuccess && openQuestions) {
          signals.push(
            `${openQuestions} open ${openQuestions === 1 ? "question" : "questions"}`,
          );
        }
        return [workspace.id, { approvals, signals }];
      }),
    );
  }, [
    workspaces,
    callerUserId,
    approvalsQueries,
    pullRequestQueries,
    questionMetricsQuery.data,
    questionMetricsQuery.isSuccess,
  ]);
}
