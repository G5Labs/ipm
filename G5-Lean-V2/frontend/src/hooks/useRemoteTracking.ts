// =============================================================================
// G5 Client — Remote Repo Tracking queries & mutations
// =============================================================================
//
// React Query hooks for the operator surface around `remote_tracking`
// branches: the per-branch sync-runs audit feed and the retry CTA.
//
// SOT:
// - views/pm_views/remote_repo_tracking.pm_view.md (I11 — audit trail UI)
// - views/arch_views/remote_repo_tracking.arch_view.md (D34, D37)
// - aux_files/future_implementation/remote_tracking_gap_closure.plan.md (G1)
// - Chat session `remote-tracking-gap-closure-plan-2026-05-28`: C2, C3
// =============================================================================

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../lib/api";
import type {
  AgentLaunchSelectionV1,
  AcceptProviderHistoryResponse,
  ApprovedRebasePublishResponse,
  AsyncState,
  CompleteBranchMergeRunResponse,
  ListBranchMergeRunsResponse,
  ListBranchSyncRunsResponse,
  MutationHook,
  PromoteRemoteTrackingResponse,
  ProviderHistoryPreview,
  PublishBranchRepoInput,
  PublishBranchRepoResponse,
  PushMode,
  PushToRemoteResponse,
  RetryBranchSyncRunsResponse,
  SetBranchRepoAutoPullInput,
  SetBranchRepoAutoPullResponse,
  SetPushModeResponse,
  StartBranchMergeRunResponse,
  WorkspaceDetail,
} from "../lib/types";
import { toAsyncState, toMutationHook } from "./hookUtils";

/**
 * Cache key factory for branch sync runs. Exposed so callers (mutations,
 * WS handlers) can target invalidations precisely.
 */
export function syncRunsQueryKey(
  workspaceId: string,
  repoId: string,
): readonly unknown[] {
  return ["branch-sync-runs", workspaceId, repoId] as const;
}

/**
 * `GET /api/v1/workspaces/:id/repos/:repoId/sync_runs`
 *
 * Returns the branch tracking summary (per-repo sync_paused_reason,
 * tracking mode, advisory SHA, latest run status) and the paged audit
 * feed in one round-trip.
 *
 * `refetchInterval` defaults to 15s so the status pill stays in sync with
 * worker-driven inserts to `remote_sync_runs` without depending on
 * `workspace.branch_updated` (that event only fires on `Imported`
 * actions per arch D14 — a `Pushed`/`Noop` cycle does not emit it).
 * G4 will add WS-driven invalidation that runs alongside this poll.
 */
export function useBranchSyncRuns(
  workspaceId: string,
  repoId: string,
  options?: {
    perPage?: number;
    page?: number;
    enabled?: boolean;
    refetchIntervalMs?: number;
  },
): AsyncState<ListBranchSyncRunsResponse> {
  const perPage = options?.perPage ?? 20;
  const page = options?.page ?? 1;
  const refetchInterval = options?.refetchIntervalMs ?? 15_000;
  const query = useQuery<ListBranchSyncRunsResponse, Error>({
    queryKey: [...syncRunsQueryKey(workspaceId, repoId), { page, perPage }],
    queryFn: ({ signal }) =>
      api.get<ListBranchSyncRunsResponse>(
        `/api/v1/workspaces/${workspaceId}/repos/${repoId}/sync_runs?page=${page}&per_page=${perPage}`,
        { signal },
      ),
    enabled: options?.enabled !== false && !!workspaceId && !!repoId,
    refetchInterval,
    // Don't pause the poll when the tab is hidden — the worker may run
    // a cycle while the user is on another tab; we want the status fresh
    // the moment they return.
    refetchIntervalInBackground: true,
  });
  return toAsyncState(query);
}

/**
 * Cache key factory for active merge runs scoped to a branch.
 */
export function branchMergeRunsQueryKey(
  workspaceId: string,
  repoId: string,
): readonly unknown[] {
  return ["branch-merge-runs", workspaceId, repoId] as const;
}

/**
 * `GET /api/v1/workspaces/:id/repos/:repoId/merge_runs?status=active`
 *
 * Lists active (waiting_for_human / running / pending / rolling_back)
 * `workspace_merge_runs` rows with `source_kind = 'remote_tracking'` for the
 * branch, plus active reset-review siblings in the workspace. The latter are
 * needed because approved publication is one workspace-coordinated action and
 * must submit the exact complete checkpoint set.
 *
 * Polls at the same cadence as sync runs by default; the worker opens
 * a merge run inside an integration cycle, so the polling intervals
 * are naturally correlated.
 */
export function useActiveBranchMergeRuns(
  workspaceId: string,
  repoId: string,
  options?: { enabled?: boolean; refetchIntervalMs?: number },
): AsyncState<ListBranchMergeRunsResponse> {
  const refetchInterval = options?.refetchIntervalMs ?? 15_000;
  const query = useQuery<ListBranchMergeRunsResponse, Error>({
    queryKey: branchMergeRunsQueryKey(workspaceId, repoId),
    queryFn: ({ signal }) =>
      api.get<ListBranchMergeRunsResponse>(
        `/api/v1/workspaces/${workspaceId}/repos/${repoId}/merge_runs?status=active&include_coordinated_reset=true`,
        { signal },
      ),
    enabled: options?.enabled !== false && !!workspaceId && !!repoId,
    refetchInterval,
    refetchIntervalInBackground: true,
  });
  return toAsyncState(query);
}

/**
 * `POST /api/v1/workspaces/:id/repos/:repoId/merge_runs/:mergeRunId/complete`
 *
 * Operator-confirmed completion of a remote-tracking merge run. After
 * the operator has resolved the divergence externally (rebased / merged
 * and committed a workspace-tip resolution), this marks the merge run
 * completed, clears `sync_paused_reason`, and enqueues a commit-coupled
 * integration cycle so the worker pushes the resolved tip.
 */
export function useCompleteBranchMergeRun(
  workspaceId: string,
  repoId: string,
): MutationHook<string, CompleteBranchMergeRunResponse> {
  const queryClient = useQueryClient();
  const mutation = useMutation<CompleteBranchMergeRunResponse, Error, string>({
    mutationFn: (mergeRunId) =>
      api.post<CompleteBranchMergeRunResponse>(
        `/api/v1/workspaces/${workspaceId}/repos/${repoId}/merge_runs/${mergeRunId}/complete`,
      ),
    onSuccess: () => {
      // Both the merge-runs list and the sync-runs feed change on
      // completion (queue UPSERT writes a new sync_run on next pickup
      // and the merge run transitions to completed). Invalidate both.
      queryClient.invalidateQueries({
        queryKey: branchMergeRunsQueryKey(workspaceId, repoId),
      });
      queryClient.invalidateQueries({
        queryKey: syncRunsQueryKey(workspaceId, repoId),
      });
    },
  });
  return toMutationHook(mutation);
}

/**
 * `POST /api/v1/workspaces/:id/repos/:repoId/merge_runs/:mergeRunId/start`
 *
 * Without a mergeRunId, `POST .../merge_runs/start` prepares resolution of a
 * pause-only divergence. The server owns checkpoint creation and fresh inputs.
 * Start one workspace agent with the launch settings confirmed in the UI.
 * Repeated starts return the same resolution task and session.
 */
export function useStartBranchMergeRun(
  workspaceId: string,
  repoId: string,
): MutationHook<
  { mergeRunId?: string; agentLaunch: AgentLaunchSelectionV1 },
  StartBranchMergeRunResponse
> {
  const queryClient = useQueryClient();
  const mutation = useMutation<
    StartBranchMergeRunResponse,
    Error,
    { mergeRunId?: string; agentLaunch: AgentLaunchSelectionV1 }
  >({
    mutationFn: ({ mergeRunId, agentLaunch }) =>
      api.post<StartBranchMergeRunResponse>(
        `/api/v1/workspaces/${workspaceId}/repos/${repoId}/merge_runs/${mergeRunId ? `${mergeRunId}/` : ""}start`,
        { agentLaunch },
      ),
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: branchMergeRunsQueryKey(workspaceId, repoId),
      });
      queryClient.invalidateQueries({
        queryKey: syncRunsQueryKey(workspaceId, repoId),
      });
      // Resolution is an ordinary workspace session and agent task.
      queryClient.invalidateQueries({
        queryKey: ["chat-sessions", workspaceId],
      });
      queryClient.invalidateQueries({
        queryKey: ["chat-sessions-all", workspaceId],
      });
      queryClient.invalidateQueries({
        queryKey: ["workspace-tasks", workspaceId],
      });
      queryClient.invalidateQueries({ queryKey: ["workspace-tasks"] });
    },
  });
  return toMutationHook(mutation);
}

/**
 * `POST /api/v1/workspaces/:id/repos/:repoId/sync_runs/retry`
 *
 * Re-enqueue every remote-tracking child whose last cycle ended in a
 * retryable status (`push_failed` / `fetch_failed` / `auth_required`).
 * The backend returns HTTP 409 when the latest status is non-retryable
 * (e.g. `noop`, `pushed`, `diverged`); that surfaces to the caller as a
 * thrown `ApiError`, which the UI maps to a toast.
 */
export function useRetryBranchSyncRuns(
  workspaceId: string,
  repoId: string,
): MutationHook<void, RetryBranchSyncRunsResponse> {
  const queryClient = useQueryClient();
  const mutation = useMutation<RetryBranchSyncRunsResponse, Error, void>({
    mutationFn: () =>
      api.post<RetryBranchSyncRunsResponse>(
        `/api/v1/workspaces/${workspaceId}/repos/${repoId}/sync_runs/retry`,
      ),
    onSuccess: () => {
      // Refresh the audit feed so the user sees the new queue activity.
      // The branch list refresh happens through WS events from the worker
      // when the queue is drained.
      queryClient.invalidateQueries({
        queryKey: syncRunsQueryKey(workspaceId, repoId),
      });
    },
  });
  return toMutationHook(mutation);
}

/**
 * `POST /api/v1/workspaces/:id/push_to_remote`
 *
 * The explicit PushToRemote action (design §6.2 Trigger C, arch D67).
 * Enqueues a `'push'` integration cycle for every unpaused remote-tracked
 * branch in the workspace; the worker publishes workspace-ahead tips
 * regardless of `push_mode`. This is the only publication path in manual mode.
 */
export function usePushToRemote(
  workspaceId: string,
): MutationHook<void, PushToRemoteResponse> {
  const queryClient = useQueryClient();
  const mutation = useMutation<PushToRemoteResponse, Error, void>({
    mutationFn: () =>
      api.post<PushToRemoteResponse>(
        `/api/v1/workspaces/${workspaceId}/push_to_remote`,
      ),
    onSuccess: () => {
      // The push enqueues integration cycles across the workspace's tracked
      // branches; refresh every sync-runs feed + the workspace detail.
      queryClient.invalidateQueries({ queryKey: ["branch-sync-runs"] });
      queryClient.invalidateQueries({ queryKey: ["workspace", workspaceId] });
    },
  });
  return toMutationHook(mutation);
}

/**
 * `POST /api/v1/workspaces/:id/remote_tracking/approved_rebase_publish`
 *
 * Approves the exact complete set of reset-below-baseline publication
 * checkpoints displayed for the coordinated workspace operation. The backend
 * linearizes the reviewed candidate through the canonical finalizer and then
 * uses ordinary fast-forward projection. This remains separate from the
 * unpaused `push_to_remote` entrypoint.
 */
export function useApprovedRebasePublish(
  workspaceId: string,
): MutationHook<string[], ApprovedRebasePublishResponse> {
  const queryClient = useQueryClient();
  const mutation = useMutation<ApprovedRebasePublishResponse, Error, string[]>({
    mutationFn: (expectedMergeRunIds) =>
      api.post<ApprovedRebasePublishResponse>(
        `/api/v1/workspaces/${workspaceId}/remote_tracking/approved_rebase_publish`,
        { expectedMergeRunIds },
      ),
    onSuccess: async () => {
      // A descendant-tip response creates a successor approval checkpoint but
      // deliberately enqueues nothing. Await the active-query refresh so the
      // mutation resolves only after an open popover can render that successor.
      await Promise.all([
        queryClient.invalidateQueries({
          queryKey: ["branch-sync-runs", workspaceId],
        }),
        queryClient.invalidateQueries({
          queryKey: ["branch-merge-runs", workspaceId],
        }),
        queryClient.invalidateQueries({
          queryKey: ["workspace", workspaceId],
        }),
      ]);
    },
  });
  return toMutationHook(mutation);
}

/**
 * Adopting the provider history: `preview` fetches the provider tip and the
 * local commits adopting would discard; `accept` adopts exactly that reviewed
 * history. Base lands it server-side
 * (g5.workspace.remote.base-provider-rewrite); another workspace resets in
 * the owner's running VM and its divergence run then completes in the
 * background (g5.workspace.remote.adopt-provider-history).
 */
export function useProviderHistoryAcceptance(
  workspaceId: string,
  repoId: string,
): {
  preview: MutationHook<void, ProviderHistoryPreview>;
  accept: MutationHook<ProviderHistoryPreview, AcceptProviderHistoryResponse>;
} {
  const queryClient = useQueryClient();
  const base = `/api/v1/workspaces/${workspaceId}/repos/${repoId}`;
  const route = `${base}/accept_provider_history`;
  // The preview fetches provider objects with the owner's credentials, so it
  // is a POST under write scope even though it moves no ref.
  const preview = useMutation<ProviderHistoryPreview, Error, void>({
    mutationFn: () =>
      api.post<ProviderHistoryPreview>(`${base}/provider_history_preview`, {}),
  });
  const accept = useMutation<
    AcceptProviderHistoryResponse,
    Error,
    ProviderHistoryPreview
  >({
    mutationFn: (reviewed) =>
      api.post<AcceptProviderHistoryResponse>(route, {
        expectedProviderTip: reviewed.providerTip,
        expectedHead: reviewed.head,
      }),
    onSuccess: async () => {
      await Promise.all([
        queryClient.invalidateQueries({
          queryKey: ["branch-sync-runs", workspaceId],
        }),
        queryClient.invalidateQueries({
          queryKey: ["branch-merge-runs", workspaceId],
        }),
        queryClient.invalidateQueries({
          queryKey: ["workspace", workspaceId],
        }),
      ]);
    },
  });
  return { preview: toMutationHook(preview), accept: toMutationHook(accept) };
}

export interface PromoteRemoteTrackingInput {
  repoId: string;
  remoteBranch: string;
  /**
   * Make-Remote Adopt-Remote target (workspace_service D47): the real remote
   * clone URL to re-point a `g5-local://` repo onto. Only honored by the
   * backend when the deploy-scoped `make_remote` instance feature is on;
   * omitted for ordinary [D28] promotion so existing callers are unchanged.
   */
  gitUrl?: string;
  /**
   * The actor confirmed creating an absent provider branch. Base refuses an
   * absent branch with `PROVIDER_BRANCH_CREATION_REQUIRES_CONFIRMATION` until
   * this is true, then creates it from Base's history with one create-only
   * push (`g5.workspace.remote.base-parity`). Sent only when true.
   */
  createProviderBranch?: boolean;
}

/**
 * `POST /api/v1/workspaces/:id/repos/:repoId/remote_tracking`
 *
 * Promotes an existing clean/materialized branch repo child to remote tracking.
 * For ordinary [D28] promotion the payload carries only the remote branch name
 * and the backend reuses the repo URL it already stored. When Make-Remote
 * (D46) is enabled an optional `gitUrl` re-points a `g5-local://` repo onto a
 * real remote; the backend ignores it when the flag is off.
 */
export function usePromoteBranchRepoRemoteTracking(
  workspaceId: string,
): MutationHook<PromoteRemoteTrackingInput, PromoteRemoteTrackingResponse> {
  const queryClient = useQueryClient();
  const mutation = useMutation<
    PromoteRemoteTrackingResponse,
    Error,
    PromoteRemoteTrackingInput
  >({
    mutationFn: (input) =>
      api.post<PromoteRemoteTrackingResponse>(
        `/api/v1/workspaces/${workspaceId}/repos/${input.repoId}/remote_tracking`,
        {
          remoteBranch: input.remoteBranch,
          // Only include the URL when the caller supplied one so the ordinary
          // promotion request body stays byte-identical for existing callers.
          ...(input.gitUrl ? { gitUrl: input.gitUrl } : {}),
          ...(input.createProviderBranch ? { createProviderBranch: true } : {}),
        },
      ),
    onSuccess: (_result, input) => {
      queryClient.invalidateQueries({ queryKey: ["workspace", workspaceId] });
      queryClient.invalidateQueries({ queryKey: ["workspaces"] });
      queryClient.invalidateQueries({
        queryKey: syncRunsQueryKey(workspaceId, input.repoId),
      });
    },
  });
  return toMutationHook(mutation);
}

/**
 * `PUT /api/v1/workspaces/:id/push_mode`
 *
 * Sets the per-workspace push mode (arch D66). `'manual'` publishes
 * only via PushToRemote; `'auto'` lets commit-coupled advances AND the
 * background loop publish workspace-ahead commits. Switching to `'auto'` does
 * not itself push; the next cycle publishes.
 */
export function useSetPushMode(
  workspaceId: string,
): MutationHook<PushMode, SetPushModeResponse> {
  const queryClient = useQueryClient();
  const mutation = useMutation<SetPushModeResponse, Error, PushMode>({
    mutationFn: (pushMode) =>
      api.put<SetPushModeResponse>(
        `/api/v1/workspaces/${workspaceId}/push_mode`,
        { pushMode },
      ),
    onSuccess: () => {
      // The toggle reads push_mode from the workspace detail; refresh it.
      queryClient.invalidateQueries({ queryKey: ["workspace", workspaceId] });
    },
  });
  return toMutationHook(mutation);
}

/**
 * `PUT /api/v1/workspaces/:id/repos/:repoId/auto_pull`
 *
 * Option 1 (G5 #1198): per-child passive auto-pull toggle. Optimistically
 * updates the matching branch's `autoPullEnabled` in the cached workspace
 * detail and rolls back on failure (spec §5.5); the authoritative server value
 * always wins after the settle-time invalidation.
 */
export function useSetBranchRepoAutoPull(
  workspaceId: string,
): MutationHook<SetBranchRepoAutoPullInput, SetBranchRepoAutoPullResponse> {
  const queryClient = useQueryClient();
  const workspaceKey = ["workspace", workspaceId] as const;
  const mutation = useMutation<
    SetBranchRepoAutoPullResponse,
    Error,
    SetBranchRepoAutoPullInput,
    { previous: WorkspaceDetail | undefined }
  >({
    mutationFn: (input) =>
      api.put<SetBranchRepoAutoPullResponse>(
        `/api/v1/workspaces/${workspaceId}/repos/${input.repoId}/auto_pull`,
        { enabled: input.enabled },
      ),
    onMutate: async (input) => {
      await queryClient.cancelQueries({ queryKey: workspaceKey });
      const previous = queryClient.getQueryData<WorkspaceDetail>(workspaceKey);
      queryClient.setQueryData<WorkspaceDetail>(workspaceKey, (current) =>
        current
          ? {
              ...current,
              branches: current.branches.map((b) =>
                b.repoId === input.repoId
                  ? { ...b, autoPullEnabled: input.enabled }
                  : b,
              ),
            }
          : current,
      );
      return { previous };
    },
    onError: (_err, _input, context) => {
      // Roll back the optimistic edit so a failed toggle never masquerades as
      // the confirmed state (spec §5.5).
      if (context?.previous !== undefined) {
        queryClient.setQueryData(workspaceKey, context.previous);
      }
    },
    onSettled: (_result, _err, input) => {
      queryClient.invalidateQueries({ queryKey: workspaceKey });
      queryClient.invalidateQueries({
        queryKey: syncRunsQueryKey(workspaceId, input.repoId),
      });
    },
  });
  return toMutationHook(mutation);
}

/**
 * `POST /api/v1/workspaces/:id/repos/:repoId/publish`
 *
 * Option 2 (G5 #1198): explicitly publishes a local-only/pinned branch child to
 * a new remote ref (create-only, confirmed), then transitions it to
 * remote-tracking. Refreshes the workspace detail + the branch sync-runs feed
 * so the repo chip flips to its remote identity.
 */
export function usePublishBranchRepo(
  workspaceId: string,
): MutationHook<PublishBranchRepoInput, PublishBranchRepoResponse> {
  const queryClient = useQueryClient();
  const mutation = useMutation<
    PublishBranchRepoResponse,
    Error,
    PublishBranchRepoInput
  >({
    mutationFn: (input) =>
      api.post<PublishBranchRepoResponse>(
        `/api/v1/workspaces/${workspaceId}/repos/${input.repoId}/publish`,
      ),
    onSuccess: (_result, input) => {
      queryClient.invalidateQueries({ queryKey: ["workspace", workspaceId] });
      queryClient.invalidateQueries({ queryKey: ["workspaces"] });
      queryClient.invalidateQueries({
        queryKey: syncRunsQueryKey(workspaceId, input.repoId),
      });
    },
  });
  return toMutationHook(mutation);
}
