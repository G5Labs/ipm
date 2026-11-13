// =============================================================================
// G5 Client — Workspace Approvals Queries & Mutations (B6 governance PR-2)
// =============================================================================
// Data layer for the workspace Approvals tab: requirements for merges INTO
// the workspace (`approval_configs` attach/retire events) and human
// explicit actor-slot requests and Approve/Reject decisions for its outgoing
// merge (`approval_requests` + `approval_statuses`).
// Admission, quorum and merge enforcement stay on the server.
//
// Backend contract (routes/workspace_approvals.rs):
//   GET  /api/v1/workspaces/:workspaceId/approvals                      → WorkspaceApprovalsStateDTO
//   POST /api/v1/workspaces/:workspaceId/approvals/requirements         → attach event
//   POST /api/v1/workspaces/:workspaceId/approvals/requirements/:id/retire → retire event
//   POST /api/v1/workspaces/:workspaceId/approvals/requests             → human request ids
//   POST /api/v1/workspaces/:workspaceId/approvals/requests/:id/cancel  → refreshed state
//   POST /api/v1/workspaces/:workspaceId/approvals/reviews              → agent request id
//   POST /api/v1/workspaces/:workspaceId/approvals/decisions            → settled decision
// =============================================================================

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../lib/api";
import { routes } from "../lib/routes";
import { externalIssueKeys } from "./useExternalIssues";
import type {
  AgentApprovalReviewAdmissionDTO,
  HumanApprovalRequestsAdmissionDTO,
  MutationHook,
  WorkspaceApprovalsStateDTO,
  WorkspacePhase,
} from "../lib/types";
import {
  toMutationHook,
  toRefreshingState,
  type RefreshingState,
} from "./hookUtils";

/** Stable React Query key for a workspace's approvals state. */
function workspaceApprovalsKey(workspaceId: string): [string, string] {
  return ["workspace-approvals", workspaceId];
}

export type ApprovalRequestTarget = {
  clientRequestId?: string;
  note?: string;
} & (
  | {
      key: string;
      label: string;
      kind: "human";
      definitionId: string;
      definitionVersion: number;
      /** The request chain's phase (one chain per definition version × phase). */
      phase: WorkspacePhase;
      actorUserId: string;
    }
  | {
      key: string;
      label: string;
      kind: "agent";
      retryReviewId?: string;
      definitionId: string;
      definitionVersion: number;
      phase: WorkspacePhase;
    }
);

export interface ApprovalRequestResult {
  target: ApprovalRequestTarget;
  status: "fulfilled" | "rejected";
  reused?: boolean;
  message?: string;
}

/**
 * Poll only while returned durable coordination state can advance: fast while
 * an agent run is actively executing or projection settlement is retryable,
 * slow for pending human requests, and stop when nothing returned can advance
 * without a new action.
 */
export function approvalStateRefetchInterval(
  state: WorkspaceApprovalsStateDTO | undefined,
): number | false {
  const activeDefinitionIds = new Set(
    state?.outgoing?.requirements.map(
      (requirement) => requirement.definition.id,
    ) ?? [],
  );
  const requests = [
    ...(state?.outgoing?.history?.flatMap((entry) =>
      entry.requests.map((request) => ({
        definitionId: entry.definitionId,
        request,
      })),
    ) ?? []),
    ...(state?.outgoing?.requirements.flatMap((requirement) =>
      (requirement.requests ?? []).map((request) => ({
        definitionId: requirement.definition.id,
        request,
      })),
    ) ?? []),
  ];
  let hasHumanAdvanceableWait = false;
  let hasPendingCleanup = false;
  for (const { definitionId, request } of requests) {
    if (request.agentReviews?.some((review) => review.cleanupPending))
      hasPendingCleanup = true;
    if (request.state !== "pending") continue;
    // A pending request on a retired definition cannot advance, regardless of
    // actor type. History retains it for evidence, but it must not keep the
    // approvals query polling.
    if (!activeDefinitionIds.has(definitionId)) continue;
    if (request.actorType === "human") {
      hasHumanAdvanceableWait = true;
      continue;
    }
    // Backend ordering assumption: agent runs arrive ascending
    // (`ORDER BY created_at, task_id`), so `.at(-1)` is the latest run.
    const latest = request.agentReviews?.at(-1);
    if (!latest || ["queued", "running", "waiting"].includes(latest.state)) {
      // Actively executing agent run (or an agent slot awaiting its first
      // run): fast tier.
      return 2_500;
    }
  }
  return hasHumanAdvanceableWait || hasPendingCleanup ? 15_000 : false;
}

/** Shared row/bulk orchestrator over the existing idempotent endpoints. */
export function useRequestApprovals(
  workspaceId: string,
): MutationHook<ApprovalRequestTarget[], ApprovalRequestResult[]> {
  const queryClient = useQueryClient();
  const mutation = useMutation<
    ApprovalRequestResult[],
    Error,
    ApprovalRequestTarget[]
  >({
    mutationFn: async (targets) => {
      const settled = await Promise.allSettled(
        targets.map(async (target) => {
          if (target.kind === "human") {
            const result = await api.post<HumanApprovalRequestsAdmissionDTO>(
              routes.workspaceApprovals.requestHumans(workspaceId),
              {
                clientRequestId: target.clientRequestId,
                note: target.note,
                definitionId: target.definitionId,
                definitionVersion: target.definitionVersion,
                phase: target.phase,
                actorUserIds: [target.actorUserId],
              },
            );
            return result.requests[0]?.reused ?? false;
          }
          const result = await api.post<AgentApprovalReviewAdmissionDTO>(
            routes.workspaceApprovals.requestAgentReview(workspaceId),
            {
              clientRequestId: target.clientRequestId,
              retryReviewId: target.retryReviewId,
              definitionId: target.definitionId,
              definitionVersion: target.definitionVersion,
              phase: target.phase,
            },
          );
          return result.reused;
        }),
      );
      return settled.map((result, index) =>
        result.status === "fulfilled"
          ? {
              target: targets[index],
              status: "fulfilled" as const,
              reused: result.value,
            }
          : {
              target: targets[index],
              status: "rejected" as const,
              message:
                result.reason instanceof Error
                  ? result.reason.message
                  : "Request failed",
            },
      );
    },
    onSettled: async (results) => {
      void queryClient.invalidateQueries({
        queryKey: ["workspace-review-flow", workspaceId],
      });
      const refresh = queryClient
        .invalidateQueries({ queryKey: workspaceApprovalsKey(workspaceId) })
        .then(() => {
          invalidateApprovalDerivedQueries(
            queryClient,
            workspaceId,
            queryClient.getQueryData<WorkspaceApprovalsStateDTO>(
              workspaceApprovalsKey(workspaceId),
            ),
          );
          void queryClient.invalidateQueries({
            queryKey: ["workspace-tasks", workspaceId],
          });
        });
      // Full success closes the dialog: wait for a fresh roster before the
      // next selection. Failures keep their captured operations for retry;
      // a slow roster read must not hide their errors or hold retry pending.
      if (results?.every((result) => result.status === "fulfilled"))
        await refresh;
    },
  });
  return toMutationHook(mutation);
}

export function workspaceApprovalsQueryOptions(workspaceId: string) {
  return {
    queryKey: workspaceApprovalsKey(workspaceId),
    queryFn: ({ signal }: { signal: AbortSignal }) =>
      api.get<WorkspaceApprovalsStateDTO>(
        routes.workspaceApprovals.state(workspaceId),
        { signal },
      ),
    enabled: !!workspaceId,
  };
}

/** Combined approvals state for the workspace Approvals tab. */
export function useWorkspaceApprovals(
  workspaceId: string,
  enabled = true,
): RefreshingState<WorkspaceApprovalsStateDTO> {
  const query = useQuery<WorkspaceApprovalsStateDTO, Error>({
    ...workspaceApprovalsQueryOptions(workspaceId),
    enabled: !!workspaceId && enabled,
    refetchInterval: (current) =>
      approvalStateRefetchInterval(current.state.data),
    // Override the app-wide `refetchOnWindowFocus: false` default so a
    // refocused tab converges without waiting for the next slow poll.
    refetchOnWindowFocus: true,
  });
  // A failed poll keeps the page, and the review threads on it, as last
  // loaded (`g5.workspace.page.refresh`).
  return toRefreshingState(query);
}

/** Cancel a formal request, or a legacy task, and refetch authoritative state. */
export function useCancelApprovalReview(
  workspaceId: string,
): MutationHook<{ taskId: string | null; requestId?: string | null }, unknown> {
  const queryClient = useQueryClient();
  const mutation = useMutation<
    unknown,
    Error,
    { taskId: string | null; requestId?: string | null }
  >({
    mutationFn: ({ taskId, requestId }) => {
      if (requestId)
        return api.post(
          routes.workspaceApprovals.cancelRequest(workspaceId, requestId),
        );
      if (taskId) return api.post(`/api/v1/tasks/${taskId}/cancel`);
      throw new Error("Review execution is unavailable.");
    },
    onSettled: () => {
      // Success and a terminal-state race both converge from server authority.
      queryClient.invalidateQueries({
        queryKey: workspaceApprovalsKey(workspaceId),
      });
    },
  });
  return toMutationHook(mutation);
}

/** Close the caller's exact pending human request and preserve decisions. */
export function useCancelHumanApprovalRequest(
  workspaceId: string,
): MutationHook<string, WorkspaceApprovalsStateDTO> {
  const queryClient = useQueryClient();
  const mutation = useMutation<WorkspaceApprovalsStateDTO, Error, string>({
    mutationFn: (requestId) =>
      api.post<WorkspaceApprovalsStateDTO>(
        routes.workspaceApprovals.cancelRequest(workspaceId, requestId),
      ),
    onSuccess: (state) => {
      queryClient.setQueryData(workspaceApprovalsKey(workspaceId), state);
      invalidateApprovalDerivedQueries(queryClient, workspaceId, state);
    },
    onError: () => {
      void queryClient.invalidateQueries({
        queryKey: workspaceApprovalsKey(workspaceId),
      });
    },
  });
  return toMutationHook(mutation);
}

/**
 * A change to the approval policy or its decisions moves every ship verdict
 * derived from it (g5.governance.approval.requirements): the pull-request
 * summaries of the source and of its target carry the server's admission,
 * and the timeline lists the decision. Invalidate them all so no surface
 * keeps a stale verdict. The approvals state itself is replaced by the
 * mutation's response (the server's complete projection), so it is not
 * refetched here.
 */
function invalidateApprovalDerivedQueries(
  queryClient: ReturnType<typeof useQueryClient>,
  workspaceId: string,
  state?: WorkspaceApprovalsStateDTO,
) {
  const workspaces = new Set([workspaceId]);
  if (state?.outgoing?.targetWorkspaceId) {
    workspaces.add(state.outgoing.targetWorkspaceId);
  }
  for (const id of workspaces) {
    void queryClient.invalidateQueries({
      queryKey: ["workspace-pull-requests", id],
    });
  }
  void queryClient.invalidateQueries({
    queryKey: externalIssueKeys.activity(workspaceId),
  });
}

function useStateReplacingMutation<TInput>(
  workspaceId: string,
  mutationFn: (input: TInput) => Promise<WorkspaceApprovalsStateDTO>,
): MutationHook<TInput, WorkspaceApprovalsStateDTO> {
  const queryClient = useQueryClient();
  const mutation = useMutation<WorkspaceApprovalsStateDTO, Error, TInput>({
    mutationFn,
    onSuccess: (state) => {
      queryClient.setQueryData(workspaceApprovalsKey(workspaceId), state);
      invalidateApprovalDerivedQueries(queryClient, workspaceId, state);
    },
  });
  return toMutationHook(mutation);
}

/** Attach a library definition as a merge-into requirement (attach event). */
export function useAttachRequirement(
  workspaceId: string,
): MutationHook<{ definitionId: string }, WorkspaceApprovalsStateDTO> {
  return useStateReplacingMutation(workspaceId, (input) =>
    api.post<WorkspaceApprovalsStateDTO>(
      routes.workspaceApprovals.attach(workspaceId),
      input,
    ),
  );
}

/** Retire a requirement (append-only retire event; history preserved). */
export function useRetireRequirement(
  workspaceId: string,
): MutationHook<{ definitionId: string }, WorkspaceApprovalsStateDTO> {
  return useStateReplacingMutation(workspaceId, ({ definitionId }) =>
    api.post<WorkspaceApprovalsStateDTO>(
      routes.workspaceApprovals.retire(workspaceId, definitionId),
      {},
    ),
  );
}

/**
 * Record the caller's immutable Approve/Reject decision for this workspace's
 * outgoing merge. The server verifies the caller owns the pending request.
 */
export function useRecordApprovalDecision(workspaceId: string): MutationHook<
  {
    definitionId: string;
    requestId?: string;
    outcome: "approved" | "rejected" | "deferred";
    comment?: string;
    /**
     * The phase this decision settles (g5.workbench.gates): the owning
     * entry's phase, so a decision at one phase never moves another's rows.
     */
    phase: WorkspacePhase;
  },
  WorkspaceApprovalsStateDTO
> {
  return useStateReplacingMutation(workspaceId, (input) =>
    api.post<WorkspaceApprovalsStateDTO>(
      routes.workspaceApprovals.decide(workspaceId),
      input,
    ),
  );
}

/** Retry only disposal of this recorded operation; never rerun its provider. */
export function useRetryApprovalCleanup(workspaceId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (reviewId: string) =>
      api.post(
        `${routes.workspaceApprovals.requestAgentReview(workspaceId)}/${encodeURIComponent(reviewId)}/cleanup`,
        {},
      ),
    onSuccess: () =>
      queryClient.invalidateQueries({
        queryKey: workspaceApprovalsKey(workspaceId),
      }),
  });
}
