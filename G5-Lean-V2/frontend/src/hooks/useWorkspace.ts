// =============================================================================
// G5 Client — Workspace Queries & Mutations
// =============================================================================

import {
  type QueryClient,
  useMutation,
  useMutationState,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { useEffect, useRef } from "react";
import { invalidateWorkspaceVmQueries } from "./useChat";
import { ApiError, api } from "../lib/api";
import { WorkspaceStatsDTOSchema } from "../lib/api-schemas";
import { randomUuid } from "../lib/randomUuid";
import { routes } from "../lib/routes";
import { externalIssueKeys } from "./useExternalIssues";
import type {
  AsyncState,
  CloneWorkspaceInput,
  CloneWorkspaceResponse,
  ForkSourceAdvisory,
  CreateWorkspacePullRequestInput,
  CreateWorkspacePullRequestResponse,
  PreviewWorkspacePullRequestTextInput,
  PreviewWorkspacePullRequestTextResponse,
  MergeWorkspacePullRequestInput,
  MergeWorkspacePullRequestResponse,
  MergeOverridePreview,
  MergeWithOverrideInput,
  MergeWorkspaceResult,
  MutationPolicyResult,
  SetWorkspaceMutationPolicyInput,
  WorkspacePullRequestActionInput,
  WorkspacePullRequestActionResponse,
  StartWorkspaceMergeInput,
  StartWorkspaceMergeResponse,
  MutationHook,
  PendingSyncFile,
  PersonalWorkspaceDTO,
  SyncWorkspaceResult,
  WorkspaceBranchDetail,
  WorkspaceMergePreflightResponse,
  WorkspaceMergePreflightInput,
  WorkspacePullRequestSummary,
  PullRequestDescriptionDTO,
  AgentLaunchSelectionV1,
  WorkspaceDetail,
  WorkspacePhase,
  WorkspaceStats,
  WorkspaceSummary,
  WorkspaceOpenQuestionMetrics,
  WorkspaceResumeOutcome,
  WorkspaceStopOutcome,
  WorkspaceTransferOutcome,
  WorkspaceVmView,
  StartComputeOutcome,
} from "../lib/types";
import {
  toAsyncState,
  toMutationHook,
  toRefreshingState,
  isTransientFailure,
  type RefreshingState,
} from "./hookUtils";
import {
  checksRefetchInterval,
  parseChecksSnapshot,
  type ExternalChecksSnapshot,
} from "../lib/externalPrChecks";

// ---------------------------------------------------------------------------
// Slug → UUID Resolution
// ---------------------------------------------------------------------------

/** Resolved IDs from the slug resolution endpoint. */
export interface ResolvedIds {
  orgId: string;
  workspaceId: string;
  /**
   * The workspace's current name. Differs from the routed slug when a former
   * name resolved; the page replaces the URL with it
   * (g5.workspace.former-names).
   */
  workspaceName: string;
}

export const resolveWorkspaceKey = (
  orgSlug: string | undefined,
  workspaceSlug: string | undefined,
) => ["resolve-workspace", orgSlug, workspaceSlug] as const;

/**
 * Forget cached name → id resolutions that a workspace change made stale:
 * those pointing at a deleted workspace, and those for a name a new
 * workspace now holds. The server matches names case-insensitively, so
 * names are compared the same way.
 */
export function forgetWorkspaceResolutions(
  queryClient: QueryClient,
  { workspaceId, name }: { workspaceId?: string; name?: string },
): void {
  const lowerName = name?.toLowerCase();
  queryClient.removeQueries({
    queryKey: ["resolve-workspace"],
    predicate: (query) => {
      const resolved = query.state.data as ResolvedIds | undefined;
      const slug = query.queryKey[2];
      return (
        (workspaceId !== undefined && resolved?.workspaceId === workspaceId) ||
        (lowerName !== undefined &&
          typeof slug === "string" &&
          slug.toLowerCase() === lowerName)
      );
    },
  });
}

/**
 * Resolve an org slug + workspace name to the workspace's UUID **and** org UUID.
 *
 * The URL uses human-readable `/:org/:workspace` but the API endpoints
 * expect a UUID workspace ID. This hook calls the dedicated resolve
 * endpoint which accepts slugs and returns canonical UUIDs. A name can be
 * reused once its workspace is deleted, so deleting and forking forget the
 * affected entries (`forgetWorkspaceResolutions`).
 */
export function useResolveWorkspaceSlug(
  orgSlug: string | undefined,
  workspaceSlug: string | undefined,
): RefreshingState<ResolvedIds> {
  const query = useQuery<ResolvedIds, Error>({
    queryKey: resolveWorkspaceKey(orgSlug, workspaceSlug),
    queryFn: async ({ signal }) => {
      const resolved = await api.get<{
        orgId: string;
        workspaceId?: string;
        workspaceName?: string;
      }>(
        `/api/v1/resolve?org=${encodeURIComponent(orgSlug!)}&workspace=${encodeURIComponent(workspaceSlug!)}`,
        { signal },
      );
      if (!resolved.workspaceId) {
        // The server answers a missing name with 404; an answer without an
        // id is the same fact, reported the same way.
        throw new ApiError({
          code: "NOT_FOUND",
          message: `Workspace "${workspaceSlug}" not found in org "${orgSlug}"`,
          status: 404,
        });
      }
      return {
        orgId: resolved.orgId,
        workspaceId: resolved.workspaceId,
        workspaceName: resolved.workspaceName ?? workspaceSlug!,
      };
    },
    enabled: !!orgSlug && !!workspaceSlug,
    staleTime: 5 * 60_000, // slugs rarely change
    retry: 1,
  });
  // A failed re-resolution (a reconnect refetches it) keeps the page on the
  // workspace it shows (`g5.workspace.page.refresh`).
  return toRefreshingState(query);
}

// ---------------------------------------------------------------------------
// Personal workspace resolution (personal_workspace g5.personal.routes)
// ---------------------------------------------------------------------------

/**
 * Query key of a personal workspace resolution: the caller's own (`handle`
 * null, which also ensures it exists) or a member's by handle.
 */
export const personalWorkspaceKey = (orgId: string, handle: string | null) =>
  handle === null
    ? (["personal-workspace", orgId, "me"] as const)
    : (["personal-workspace", orgId, "user", handle] as const);

/**
 * Resolve a personal workspace by registry kind, never by matching a name:
 * `GET /orgs/:orgId/me` ensures and returns the caller's own (viewers are
 * refused), `GET /orgs/:orgId/users/:handle/workspace` a member's for the
 * owner or an administrator.
 */
export function personalWorkspaceQueryOptions(
  orgId: string,
  handle: string | null,
) {
  return {
    queryKey: personalWorkspaceKey(orgId, handle),
    queryFn: ({ signal }: { signal: AbortSignal }) =>
      api.get<PersonalWorkspaceDTO>(
        handle === null
          ? `/api/v1/orgs/${orgId}/me`
          : `/api/v1/orgs/${orgId}/users/${encodeURIComponent(handle)}/workspace`,
        { signal },
      ),
    enabled: !!orgId && handle !== "",
    // The binding never changes while the membership lasts.
    staleTime: 5 * 60_000,
    retry: (failures: number, error: unknown) =>
      !(
        error instanceof ApiError &&
        (error.status === 403 || error.status === 404)
      ) && failures < 1,
  };
}

/** The caller's personal workspace in an organization (ensured on read). */
export function usePersonalWorkspace(
  orgId: string,
): RefreshingState<PersonalWorkspaceDTO> {
  const query = useQuery<PersonalWorkspaceDTO, Error>(
    personalWorkspaceQueryOptions(orgId, null),
  );
  return toRefreshingState(query);
}

/** How the workspace page's route names its workspace. */
export type WorkspaceRouteTarget =
  | { family: "workspace"; orgSlug?: string; workspaceSlug?: string }
  | { family: "me"; orgId: string }
  | { family: "user"; orgId: string; handle?: string };

/** Query key the page's route resolution reads (for a forced re-resolve). */
export function workspaceRouteResolutionKey(target: WorkspaceRouteTarget) {
  switch (target.family) {
    case "me":
      return personalWorkspaceKey(target.orgId, null);
    case "user":
      return personalWorkspaceKey(target.orgId, target.handle ?? "");
    default:
      return resolveWorkspaceKey(target.orgSlug, target.workspaceSlug);
  }
}

/**
 * Resolve the workspace page's route to IDs: a name through the slug
 * resolution, a personal route through its kind-based endpoint. Personal
 * results carry the whole `PersonalWorkspaceDTO` (name and handle too), and
 * their current name is the personal name, which never changes.
 */
export function useResolveWorkspaceRoute(
  target: WorkspaceRouteTarget,
): RefreshingState<ResolvedIds & Partial<PersonalWorkspaceDTO>> {
  const nameTarget =
    target.family === "workspace"
      ? target
      : { family: "workspace" as const, orgSlug: undefined };
  const byName = useResolveWorkspaceSlug(
    nameTarget.orgSlug,
    nameTarget.workspaceSlug,
  );
  const personalOptions =
    target.family === "workspace"
      ? { ...personalWorkspaceQueryOptions("", null), enabled: false }
      : personalWorkspaceQueryOptions(
          target.orgId,
          target.family === "me" ? null : (target.handle ?? ""),
        );
  const personal = toRefreshingState(
    useQuery<PersonalWorkspaceDTO, Error, ResolvedIds & PersonalWorkspaceDTO>({
      ...personalOptions,
      select: (workspace) => ({ ...workspace, workspaceName: workspace.name }),
    }),
  );
  return target.family === "workspace" ? byName : personal;
}

// ---------------------------------------------------------------------------
// Query Hooks
// ---------------------------------------------------------------------------

/**
 * Query config for an org's workspace list — one queryFn for one key.
 *
 * Shared by `useWorkspaces` and `ProjectsPage`'s `useQueries` fan-out. The
 * Rules of Hooks stop `ProjectsPage` from calling `useWorkspaces` inside a
 * `.map()`, so without this factory both register *different closures against
 * the same key* and the stored queryFn is whichever observer called
 * `setOptions` last.
 *
 * The endpoint is unpaginated, so the returned array is the org's complete
 * workspace set — no `perPage`, no draining, no truncation to detect.
 */
export function workspacesQueryOptions(orgId: string) {
  return {
    queryKey: ["workspaces", orgId] as const,
    queryFn: ({ signal }: { signal: AbortSignal }) =>
      api.get<WorkspaceSummary[]>(`/api/v1/orgs/${orgId}/workspaces`, {
        signal,
      }),
    enabled: !!orgId,
    refetchOnWindowFocus: true,
  };
}

export function useWorkspaces(orgId: string): AsyncState<WorkspaceSummary[]> {
  const query = useQuery<WorkspaceSummary[], Error>(
    workspacesQueryOptions(orgId),
  );
  return toAsyncState(query);
}

export function workspaceOpenQuestionMetricsQueryOptions(
  orgId: string,
  workspaceIds: string[],
) {
  const ids = [...new Set(workspaceIds)].sort();
  return {
    queryKey: ["workspace-open-question-metrics", orgId, ids] as const,
    queryFn: ({ signal }: { signal: AbortSignal }) =>
      api.get<WorkspaceOpenQuestionMetrics[]>(
        `/api/v1/orgs/${orgId}/workspaces/open-question-metrics?workspaceIds=${encodeURIComponent(ids.join(","))}`,
        { signal },
      ),
    enabled: !!orgId && ids.length > 0,
    staleTime: 30_000,
    refetchOnWindowFocus: true,
  };
}

export function useWorkspaceOpenQuestionMetrics(
  orgId: string,
  workspaceId: string,
): AsyncState<WorkspaceOpenQuestionMetrics[]> {
  const query = useQuery<WorkspaceOpenQuestionMetrics[], Error>(
    workspaceOpenQuestionMetricsQueryOptions(
      orgId,
      workspaceId ? [workspaceId] : [],
    ),
  );
  return toAsyncState(query);
}

/**
 * The workspace's detail, polled. A transient refetch failure keeps the
 * last loaded detail (with `refreshError`), so the page and what is typed
 * in it stay; a refusal (access lost, workspace gone) does not
 * (`g5.workspace.page.refresh`).
 */
export function useWorkspace(
  workspaceId: string,
): RefreshingState<WorkspaceDetail> {
  const query = useQuery<WorkspaceDetail, Error>({
    queryKey: ["workspace", workspaceId],
    queryFn: ({ signal }) =>
      api.get<WorkspaceDetail>(`/api/v1/workspaces/${workspaceId}`, { signal }),
    enabled: !!workspaceId,
    // While a read-write → read-only transition drains in the background
    // (`mutationPolicyTransition === "freezing"`), poll so the read-only badge
    // and the transition toggle settle when the async drain commits or reverts.
    // The transition endpoint returns 202 immediately and does not block; the
    // drain runs server-side (@ref software_architecture/g5-api/services/
    // workspace_service#I5, workspace_mutation_policy#I9).
    // Ownership can change in another browser while this page is open. Keep
    // the authority projection fresh even when no local mutation fires.
    refetchInterval: (query) =>
      query.state.data?.mutationPolicyTransition === "freezing" ? 2500 : 5000,
  });
  return toRefreshingState(query);
}

export function usePendingSyncFiles(
  workspaceId: string,
  enabled: boolean,
): AsyncState<PendingSyncFile[]> {
  const query = useQuery<PendingSyncFile[], Error>({
    queryKey: ["pending-sync-files", workspaceId, "downlift"],
    queryFn: ({ signal }) =>
      api.get<PendingSyncFile[]>(
        `/api/v1/workspaces/${workspaceId}/sync/pending?direction=downlift`,
        { signal },
      ),
    enabled: enabled && !!workspaceId,
  });
  return toAsyncState(query);
}

/**
 * Fetch a workspace's statistics, checked against the stats DTO schema.
 *
 * A body that is not the DTO — a page loaded before a deploy talking to a
 * server that has since renamed a field, or a proxy answering 200 with
 * something else — is reported as the hook's error, so the Details panel
 * shows its unavailable state instead of handing the shape to components
 * that iterate it (issue #733: `e is not iterable` from the spend table).
 */
export function useWorkspaceStats(
  workspaceId: string,
): AsyncState<WorkspaceStats> {
  const query = useQuery<WorkspaceStats, Error>({
    queryKey: ["workspace-stats", workspaceId],
    queryFn: async ({ signal }) => {
      const body = await api.get<unknown>(
        `/api/v1/workspaces/${workspaceId}/stats`,
        { signal },
      );
      return parseWorkspaceStats(body);
    },
    enabled: !!workspaceId,
  });
  return toAsyncState(query);
}

/** Check a stats response body against the DTO schema; throws on a mismatch. */
export function parseWorkspaceStats(body: unknown): WorkspaceStats {
  const parsed = WorkspaceStatsDTOSchema.safeParse(body);
  if (parsed.success) return parsed.data as WorkspaceStats;
  const where = parsed.error.issues
    .slice(0, 3)
    .map((issue) => `${issue.path.join(".") || "(root)"}: ${issue.message}`)
    .join("; ");
  const keys =
    body && typeof body === "object"
      ? Object.keys(body).join(", ")
      : String(body);
  console.warn(
    "[useWorkspaceStats] stats response did not match the DTO:",
    where,
    "— body keys:",
    keys,
  );
  throw new Error(`Workspace stats response did not match the DTO (${where})`);
}

/** Fetch each repository member and its currently selected Git ref. */
export function useWorkspaceBranches(
  workspaceId: string,
): AsyncState<WorkspaceBranchDetail[]> {
  const query = useQuery<WorkspaceBranchDetail[], Error>({
    queryKey: ["workspace-branches-all", workspaceId],
    queryFn: ({ signal }) =>
      api.get<WorkspaceBranchDetail[]>(
        `/api/v1/workspaces/${workspaceId}/branches`,
        { signal },
      ),
    enabled: !!workspaceId,
  });
  return toAsyncState(query);
}

export function usePatchWorkspace(
  workspaceId: string,
): MutationHook<{ description?: string }, WorkspaceSummary> {
  const queryClient = useQueryClient();
  const mutation = useMutation<
    WorkspaceSummary,
    Error,
    { description?: string }
  >({
    mutationFn: (input) =>
      api.patch<WorkspaceSummary>(`/api/v1/workspaces/${workspaceId}`, input),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["workspace", workspaceId] });
      queryClient.invalidateQueries({ queryKey: ["workspaces"] });
      queryClient.invalidateQueries({
        queryKey: ["workspace-tasks", workspaceId],
      });
    },
  });
  return toMutationHook(mutation);
}

/**
 * Mutation key of a workspace's phase move. Every instance of
 * `useUpdateWorkspacePhase` for that workspace — the chevrons, the Home chip —
 * shares the moves filed under it, so a move started on one page is pending
 * (and its refusal reported) on the next.
 */
export function workspacePhaseMutationKey(workspaceId: string) {
  return ["workspace-phase", workspaceId] as const;
}

/**
 * Move the workbench phase (workbench g5.workbench.phases.state) through
 * `PATCH /workspaces/:id {phase}`. The chevron reflects the choice at once
 * (optimistic detail cache), rolls back on refusal (non-owner, Base), and the
 * detail + list queries refetch so every reader sees the durable value.
 *
 * `isLoading`, `error` and `data` describe the LATEST move filed for this
 * workspace under `workspacePhaseMutationKey`, whichever page started it, and
 * `mutate` refuses a move while one is still in flight so two writes never
 * race to overwrite each other.
 */
export function useUpdateWorkspacePhase(
  workspaceId: string,
): MutationHook<WorkspacePhase, WorkspaceDetail> {
  const queryClient = useQueryClient();
  const mutationKey = workspacePhaseMutationKey(workspaceId);
  const moves = useMutationState({
    filters: { mutationKey },
    select: (entry) => ({
      status: entry.state.status,
      error: entry.state.error as Error | null,
      data: entry.state.data as WorkspaceDetail | undefined,
    }),
  });
  const latestMove = moves[moves.length - 1];
  const moveInFlight = () => queryClient.isMutating({ mutationKey }) > 0;
  // The workspace a move belongs to is bound at mutate time and carried in
  // the mutation VARIABLES, not only the context: the hook may be re-rendered
  // for another workspace while the PATCH is pending, and React Query runs an
  // in-flight or paused-then-resumed mutation with the latest `mutationFn`
  // and callbacks, so the request URL, rollback and invalidation must never
  // read the render's id.
  type PhaseMove = { workspaceId: string; phase: WorkspacePhase };
  type PhaseMoveContext = {
    workspaceId: string;
    previousPhase: WorkspacePhase | undefined;
  };
  const detailKeyFor = (id: string) => ["workspace", id] as const;
  const mutation = useMutation<
    WorkspaceDetail,
    Error,
    PhaseMove,
    PhaseMoveContext
  >({
    mutationKey,
    mutationFn: ({ workspaceId: target, phase }) =>
      api.patch<WorkspaceDetail>(routes.workspaces.update(target), { phase }),
    onMutate: async ({ workspaceId: target, phase }) => {
      const detailKey = detailKeyFor(target);
      await queryClient.cancelQueries({ queryKey: detailKey });
      const previous = queryClient.getQueryData<WorkspaceDetail>(detailKey);
      if (previous) {
        queryClient.setQueryData<WorkspaceDetail>(detailKey, {
          ...previous,
          phase,
        });
      }
      return { workspaceId: target, previousPhase: previous?.phase };
    },
    onError: (_error, _phase, context) => {
      if (!context) return;
      // Restore only the phase onto whatever that workspace's detail holds
      // now, so newer unrelated fields from a refetch are kept.
      queryClient.setQueryData<WorkspaceDetail>(
        detailKeyFor(context.workspaceId),
        (current) =>
          current ? { ...current, phase: context.previousPhase } : current,
      );
    },
    onSettled: (_data, _error, _phase, context) => {
      if (context) {
        queryClient.invalidateQueries({
          queryKey: detailKeyFor(context.workspaceId),
        });
        queryClient.invalidateQueries({
          queryKey: ["workspace-review-flow", context.workspaceId],
        });
        // The move's durable event is a timeline entry (workbench
        // g5.workbench.landing): refetch the feed so it appears at once.
        queryClient.invalidateQueries({
          queryKey: externalIssueKeys.activity(context.workspaceId),
        });
      }
      queryClient.invalidateQueries({ queryKey: ["workspaces"] });
    },
  });
  // Public shape stays `mutate(phase)`; the render's workspace is captured
  // here, at call time, into the variables. State comes from the shared key
  // (see above), not from this observer alone.
  const hook = toMutationHook(mutation);
  return {
    ...hook,
    isLoading: latestMove?.status === "pending",
    error: latestMove?.status === "error" ? latestMove.error : null,
    data: latestMove?.status === "success" ? latestMove.data : undefined,
    mutate: (phase: WorkspacePhase) => {
      if (moveInFlight()) return;
      mutation.mutate({ workspaceId, phase });
    },
    mutateAsync: (phase: WorkspacePhase) =>
      moveInFlight()
        ? Promise.reject(new Error("A phase move is already in flight"))
        : mutation.mutateAsync({ workspaceId, phase }),
  };
}

/** `POST /api/v1/workspaces/:id/rename` response. */
export interface RenameWorkspaceResponse {
  id: string;
  name: string;
  /** Former names the workspace still reserves, newest first. */
  formerNames: string[];
}

/**
 * Rename a workspace (g5.workspace.rename). The branch it owns does not
 * change; the previous name keeps resolving to it. On success the cached
 * name resolutions that point at this workspace learn the new name in place,
 * so a page showing it under the old name replaces its URL through its
 * former-name handling without re-resolving, and the workspace lists are
 * refreshed.
 */
export function useRenameWorkspace(
  workspaceId: string,
  orgId?: string,
): MutationHook<{ name: string }, RenameWorkspaceResponse> {
  const queryClient = useQueryClient();
  const mutation = useMutation<
    RenameWorkspaceResponse,
    Error,
    { name: string }
  >({
    mutationFn: (input) =>
      api.post<RenameWorkspaceResponse>(
        `/api/v1/workspaces/${workspaceId}/rename`,
        input,
      ),
    onSuccess: (renamed) => {
      queryClient.setQueriesData<ResolvedIds>(
        {
          queryKey: ["resolve-workspace"],
          predicate: (query) =>
            (query.state.data as ResolvedIds | undefined)?.workspaceId ===
            workspaceId,
        },
        (resolved) =>
          resolved ? { ...resolved, workspaceName: renamed.name } : resolved,
      );
      queryClient.invalidateQueries({ queryKey: ["workspace", workspaceId] });
      queryClient.invalidateQueries({ queryKey: ["workspaces"] });
      if (orgId) {
        queryClient.invalidateQueries({ queryKey: ["workspaces", orgId] });
      }
    },
  });
  return toMutationHook(mutation);
}

export function useDeleteWorkspace(
  workspaceId: string,
  orgId?: string,
): MutationHook<void, void> {
  const queryClient = useQueryClient();
  const operationId = useRef<string | null>(null);
  const mutation = useMutation<void, Error, void>({
    mutationFn: () => {
      operationId.current ??= randomUuid();
      return api.post<void>(`/api/v1/workspaces/${workspaceId}/delete`, {
        force: false,
        operationId: operationId.current,
      });
    },
    onSuccess: () => {
      operationId.current = null;
      forgetWorkspaceResolutions(queryClient, { workspaceId });
      queryClient.invalidateQueries({ queryKey: ["workspace", workspaceId] });
      queryClient.invalidateQueries({ queryKey: ["workspaces"] });
      if (orgId) {
        queryClient.invalidateQueries({ queryKey: ["workspaces", orgId] });
      }
    },
  });
  return toMutationHook(mutation);
}

/**
 * Tracking-state advisories about the workspace's remote-tracked repositories
 * that a fork would capture (g5.fork.source.advisory). Advisory only.
 */
export function useForkSourceAdvisories(
  workspaceId: string,
  enabled = true,
): AsyncState<ForkSourceAdvisory[]> {
  const query = useQuery<ForkSourceAdvisory[], Error>({
    queryKey: ["fork-source-advisories", workspaceId],
    queryFn: ({ signal }) =>
      api.get<ForkSourceAdvisory[]>(
        `/api/v1/workspaces/${workspaceId}/fork_advisories`,
        { signal },
      ),
    enabled: enabled && !!workspaceId,
  });
  return toAsyncState(query);
}

export function useCloneWorkspace(
  workspaceId: string,
): MutationHook<CloneWorkspaceInput, CloneWorkspaceResponse> {
  const queryClient = useQueryClient();
  const mutation = useMutation<
    CloneWorkspaceResponse,
    Error,
    CloneWorkspaceInput
  >({
    mutationFn: (input) =>
      api.post<CloneWorkspaceResponse>(
        `/api/v1/workspaces/${workspaceId}/clone`,
        input,
      ),
    onSuccess: (result) => {
      // The new name may still resolve to a deleted workspace that held it.
      forgetWorkspaceResolutions(queryClient, { name: result.workspace.name });
      queryClient.invalidateQueries({ queryKey: ["workspaces"] });
    },
  });
  return toMutationHook(mutation);
}

/**
 * Transition a workspace's read-only policy (read_write ↔ read_only).
 *
 * Posts to the already-implemented, admin-gated transition endpoint
 * `POST /api/v1/workspaces/:id/mutation-policy` (the only `mutation-policy`
 * caller in the client). RW→RO is a two-phase freeze-then-drain and may return
 * `transition: "freezing"` and/or a `blockers` list; RO→RW is instantaneous.
 * On success it invalidates the workspace detail/list caches so the read-only
 * badge/banner/switcher update.
 * @ref software_architecture/client/data-layer/hooks/workspace_ontology_file_hooks#D16
 * @ref software_architecture/g5-api/services/workspace_mutation_policy#I9
 */
export function useSetWorkspaceMutationPolicy(
  workspaceId: string,
): MutationHook<SetWorkspaceMutationPolicyInput, MutationPolicyResult> {
  const queryClient = useQueryClient();
  const mutation = useMutation<
    MutationPolicyResult,
    Error,
    SetWorkspaceMutationPolicyInput
  >({
    mutationFn: (input) =>
      api.post<MutationPolicyResult>(
        `/api/v1/workspaces/${workspaceId}/mutation-policy`,
        input,
      ),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["workspace", workspaceId] });
      queryClient.invalidateQueries({ queryKey: ["workspaces"] });
    },
  });
  return toMutationHook(mutation);
}

export function useSyncWorkspace(
  workspaceId: string,
): MutationHook<void, SyncWorkspaceResult> {
  const queryClient = useQueryClient();
  const mutation = useMutation<SyncWorkspaceResult, Error, void>({
    mutationFn: () =>
      api.post<SyncWorkspaceResult>(
        `/api/v1/workspaces/${workspaceId}/sync`,
        {},
      ),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["workspace", workspaceId] });
      queryClient.invalidateQueries({
        queryKey: ["workspace-stats", workspaceId],
      });
      queryClient.invalidateQueries({ queryKey: ["branch-sync-runs"] });
      queryClient.invalidateQueries({ queryKey: ["docs", workspaceId] });
      queryClient.invalidateQueries({ queryKey: ["views", workspaceId] });
      queryClient.invalidateQueries({
        queryKey: ["workspace-file", workspaceId],
      });
      queryClient.invalidateQueries({ queryKey: ["view", workspaceId] });
    },
  });
  return toMutationHook(mutation);
}

export function useMergeWorkspace(
  workspaceId: string,
): MutationHook<void, MergeWorkspaceResult> {
  const queryClient = useQueryClient();
  const mutation = useMutation<MergeWorkspaceResult, Error, void>({
    mutationFn: () =>
      api.post<MergeWorkspaceResult>(
        `/api/v1/workspaces/${workspaceId}/merge`,
        {},
      ),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["workspace", workspaceId] });
      queryClient.invalidateQueries({
        queryKey: ["workspace-stats", workspaceId],
      });
      queryClient.invalidateQueries({ queryKey: ["workspaces"] });
    },
  });
  return toMutationHook(mutation);
}

export function useStartWorkspaceMerge(
  workspaceId: string,
): MutationHook<StartWorkspaceMergeInput, StartWorkspaceMergeResponse> {
  const queryClient = useQueryClient();
  const mutation = useMutation<
    StartWorkspaceMergeResponse,
    Error,
    StartWorkspaceMergeInput
  >({
    mutationFn: (input) =>
      api.post<StartWorkspaceMergeResponse>(
        `/api/v1/workspaces/${workspaceId}/rebase-merge`,
        input,
      ),
    onSuccess: (result, input) => {
      queryClient.invalidateQueries({ queryKey: ["workspace", workspaceId] });
      queryClient.invalidateQueries({
        queryKey: ["workspace", input.targetWorkspaceId],
      });
      queryClient.invalidateQueries({
        queryKey: ["workspace-stats", workspaceId],
      });
      queryClient.invalidateQueries({
        queryKey: ["workspace-stats", input.targetWorkspaceId],
      });
      queryClient.invalidateQueries({
        queryKey: ["workspace-tasks", workspaceId],
      });
      queryClient.invalidateQueries({
        queryKey: ["workspace-tasks", input.targetWorkspaceId],
      });
      queryClient.invalidateQueries({ queryKey: ["chat-sessions"] });
      queryClient.invalidateQueries({ queryKey: ["workspaces"] });
      // A Start and resolve with agent recorded a Start: re-read the VM
      // state, as the chat's Start and send does.
      if (result.resolution && result.startCompute) {
        invalidateWorkspaceVmQueries(queryClient);
      }
    },
    // A failed compound request may still have recorded a Start (the
    // brief committed, a later step failed): re-read the gate before the
    // next action, so it does not offer a second Start on stale state.
    onError: (_error, input) => {
      if (input.startCompute) invalidateWorkspaceVmQueries(queryClient);
    },
  });
  return toMutationHook(mutation);
}

export function usePreflightWorkspaceMerge(
  workspaceId: string,
): MutationHook<WorkspaceMergePreflightInput, WorkspaceMergePreflightResponse> {
  const mutation = useMutation<
    WorkspaceMergePreflightResponse,
    Error,
    WorkspaceMergePreflightInput
  >({
    mutationFn: (input) =>
      api.post<WorkspaceMergePreflightResponse>(
        `/api/v1/workspaces/${workspaceId}/rebase-merge/preflight`,
        input,
      ),
  });
  return toMutationHook(mutation);
}

/** Ordered by the shared PR owner: the first choice is the default destination. */
export function useWorkspacePullRequestTargets(
  workspaceId: string,
): RefreshingState<{ id: string; name: string }[]> {
  const queryClient = useQueryClient();
  useEffect(() => {
    const queryKey = ["workspaces", "pull-request-targets", workspaceId];
    // A real detail read proves the page can reach the API again, even when
    // its data is unchanged. Keep retrying retained destinations on these
    // reads, including when their failure arrived after the page recovered.
    return queryClient.getQueryCache().subscribe((event) => {
      if (
        event.type !== "updated" ||
        event.action.type !== "success" ||
        event.action.manual ||
        event.query.queryKey.length !== 2 ||
        event.query.queryKey[0] !== "workspace" ||
        event.query.queryKey[1] !== workspaceId
      )
        return;
      const state = queryClient.getQueryState(queryKey);
      if (
        state?.data !== undefined &&
        state.error &&
        isTransientFailure(state.error) &&
        state.fetchStatus === "idle"
      ) {
        void queryClient.refetchQueries(
          { queryKey, exact: true, type: "active" },
          { cancelRefetch: false },
        );
      }
    });
  }, [queryClient, workspaceId]);
  return toRefreshingState(
    useQuery({
      queryKey: ["workspaces", "pull-request-targets", workspaceId],
      queryFn: ({ signal }) =>
        api.get<{ id: string; name: string }[]>(
          `/api/v1/workspaces/${workspaceId}/pull-requests/targets`,
          { signal },
        ),
      enabled: !!workspaceId,
      refetchOnWindowFocus: true,
    }),
  );
}

export function useWorkspacePullRequests(
  workspaceId: string,
  role: "source" | "target" | "all" = "target",
  status: "open" | "merged" | "closed" | "all" = "open",
): AsyncState<WorkspacePullRequestSummary[]> {
  const query = useQuery<WorkspacePullRequestSummary[], Error>(
    workspacePullRequestsQueryOptions(workspaceId, role, status),
  );
  return toAsyncState(query);
}

export function workspacePullRequestsQueryOptions(
  workspaceId: string,
  role: "source" | "target" | "all" = "target",
  status: "open" | "merged" | "closed" | "all" = "open",
) {
  return {
    queryKey: ["workspace-pull-requests", workspaceId, role, status],
    queryFn: ({ signal }: { signal: AbortSignal }) =>
      api.get<WorkspacePullRequestSummary[]>(
        `/api/v1/workspaces/${workspaceId}/pull-requests?role=${encodeURIComponent(role)}&status=${encodeURIComponent(status)}`,
        { signal },
      ),
    enabled: !!workspaceId,
    refetchOnWindowFocus: true,
  };
}

/**
 * Live, revision-bound external PR check snapshot for one workspace PR, read
 * through `GET /pull-requests/info?checks=true` (governance
 * g5.governance.pr.checks.surface). Parsed into the typed snapshot; polls while
 * any check is still pending so a just-opened PR updates in place. Disabled
 * until there is a pull request id to select.
 */
export function useWorkspacePullRequestChecks(
  workspaceId: string,
  pullRequestId: string | null | undefined,
  enabled = true,
): AsyncState<ExternalChecksSnapshot | null> {
  const query = useQuery<ExternalChecksSnapshot | null, Error>({
    queryKey: ["workspace-pr-checks", workspaceId, pullRequestId],
    queryFn: async ({ signal }) => {
      const raw = await api.get<unknown>(
        `/api/v1/workspaces/${workspaceId}/pull-requests/info?checks=true&selector=${encodeURIComponent(
          pullRequestId ?? "",
        )}`,
        { signal },
      );
      return parseChecksSnapshot(raw);
    },
    enabled: !!workspaceId && !!pullRequestId && enabled,
    // Follow pending checks without a durable monitor, including ones the
    // base branch does not require (a later failure still blocks); hold off
    // while GitHub rate-limits or refuses the read, and stop once the verdict
    // settles and nothing reported is pending.
    refetchInterval: (query) => checksRefetchInterval(query.state.data),
  });
  return toAsyncState(query);
}

/**
 * The G5 PR's canonical description with each provider publication's
 * lifecycle, body and comparison (git g5.git.pr-description). The key carries
 * the PR's update time, so a saved description or publication change reads
 * again; provider-side edits are picked up after the short stale window.
 */
export function useWorkspacePullRequestDescription(
  workspaceId: string,
  pullRequest: Pick<WorkspacePullRequestSummary, "id" | "updatedAt"> | null,
): AsyncState<PullRequestDescriptionDTO> {
  const query = useQuery<PullRequestDescriptionDTO, Error>({
    queryKey: [
      "workspace-pr-description",
      workspaceId,
      pullRequest?.id,
      pullRequest?.updatedAt,
    ],
    queryFn: ({ signal }) =>
      api.get<PullRequestDescriptionDTO>(
        `/api/v1/workspaces/${workspaceId}/pull-requests/${encodeURIComponent(pullRequest?.id ?? "")}/description`,
        { signal },
      ),
    enabled: !!workspaceId && !!pullRequest,
    staleTime: 30_000,
    retry: false,
    refetchOnWindowFocus: true,
  });
  return toAsyncState(query);
}

/** Read-shaped mutation: derives the PR text a create would use (spec
 * workspace-pr-api I3). Invoked on demand to prefill the Prepare-to-Ship
 * description editor; touches no query cache. */
export function usePreviewWorkspacePullRequestText(
  workspaceId: string,
): MutationHook<
  PreviewWorkspacePullRequestTextInput,
  PreviewWorkspacePullRequestTextResponse
> {
  const mutation = useMutation<
    PreviewWorkspacePullRequestTextResponse,
    Error,
    PreviewWorkspacePullRequestTextInput
  >({
    mutationFn: (input) =>
      api.post<PreviewWorkspacePullRequestTextResponse>(
        `/api/v1/workspaces/${workspaceId}/pull-requests/preview`,
        input,
      ),
  });
  return toMutationHook(mutation);
}

export function useCreateWorkspacePullRequest(
  workspaceId: string,
): MutationHook<
  CreateWorkspacePullRequestInput,
  CreateWorkspacePullRequestResponse
> {
  const queryClient = useQueryClient();
  const mutation = useMutation<
    CreateWorkspacePullRequestResponse,
    Error,
    CreateWorkspacePullRequestInput
  >({
    mutationFn: (input) =>
      api.post<CreateWorkspacePullRequestResponse>(
        `/api/v1/workspaces/${workspaceId}/pull-requests`,
        input,
      ),
    onSuccess: async (result) => {
      // The dialogs use the live PR query after this mutation. Cancel older
      // reads before replacing cached copies, then let onSettled refetch them.
      const workspaceIds = [workspaceId, result.pullRequest.targetWorkspaceId];
      await Promise.all(
        workspaceIds.map((id) =>
          queryClient.cancelQueries({
            queryKey: ["workspace-pull-requests", id],
          }),
        ),
      );
      for (const id of workspaceIds) {
        queryClient.setQueriesData<WorkspacePullRequestSummary[]>(
          { queryKey: ["workspace-pull-requests", id] },
          (rows) =>
            rows?.map((row) =>
              row.id === result.pullRequest.id ? result.pullRequest : row,
            ),
        );
      }
      queryClient.invalidateQueries({ queryKey: ["workspace", workspaceId] });
      queryClient.invalidateQueries({
        queryKey: ["workspace", result.pullRequest.targetWorkspaceId],
      });
      queryClient.invalidateQueries({ queryKey: ["workspaces"] });
      if (result.pullRequest.status === "merged") {
        queryClient.invalidateQueries({
          queryKey: ["workspace-tasks", workspaceId],
        });
      }
    },
    onSettled: () => {
      // A failed provider phase may follow a durable G5 PR and saved commits.
      queryClient.invalidateQueries({ queryKey: ["workspace-pull-requests"] });
      queryClient.invalidateQueries({
        queryKey: ["workspace-changes", workspaceId],
      });
      queryClient.invalidateQueries({ queryKey: ["workspace", workspaceId] });
    },
  });
  return toMutationHook(mutation);
}

export type WorkspacePullRequestAutoMergeInput = {
  pullRequestId: string;
} & (
  | {
      enabled: boolean;
      /**
       * Merge with agent (g5.git.pr-auto-merge.with-agent): also start a chat in
       * the source workspace that keeps the pull request green and
       * rebase-mergeable. Only when turning it on.
       */
      agent?: {
        agentLaunch: AgentLaunchSelectionV1;
        requestKey: string;
        /**
         * Start and ship with agent (g5.chat.start-and-send): the source
         * workspace's owner records Start consent in the same request, and the
         * agent's chat is queued bound to it.
         */
        startCompute?: boolean;
      };
      stopVmAfterMerge?: boolean;
    }
  | {
      /** Saving this choice neither enables auto merge nor starts a chat. */
      stopVmAfterMerge: boolean;
      enabled?: never;
      agent?: never;
    }
);

export interface WorkspacePullRequestAutoMergeResult {
  pullRequest: WorkspacePullRequestSummary;
  /** The chat Merge with agent started. */
  chatSessionId?: string | null;
  /** The Start a `startCompute: true` request recorded. */
  startCompute?: StartComputeOutcome;
}

/**
 * Turn merge when ready (g5.git.pr-auto-merge) on or off for a pull request
 * into `targetWorkspaceId`. G5 then merges it as the signed-in person once it
 * is ready.
 */
export function useSetWorkspacePullRequestAutoMerge(
  targetWorkspaceId: string,
): MutationHook<
  WorkspacePullRequestAutoMergeInput,
  WorkspacePullRequestAutoMergeResult
> {
  const queryClient = useQueryClient();
  const mutation = useMutation<
    WorkspacePullRequestAutoMergeResult,
    Error,
    WorkspacePullRequestAutoMergeInput
  >({
    mutationFn: ({ pullRequestId, enabled, agent, stopVmAfterMerge }) => {
      const path = `/api/v1/workspaces/${targetWorkspaceId}/pull-requests/${pullRequestId}/auto-merge`;
      if (enabled === undefined) {
        return api.patch<WorkspacePullRequestAutoMergeResult>(path, {
          stopVmAfterMerge,
        });
      }
      return enabled
        ? api.post<WorkspacePullRequestAutoMergeResult>(path, {
            ...(agent ? { agent } : {}),
            ...(stopVmAfterMerge === undefined ? {} : { stopVmAfterMerge }),
          })
        : api.delete<WorkspacePullRequestAutoMergeResult>(path);
    },
    onSettled: (result) => {
      queryClient.invalidateQueries({
        queryKey: ["workspace-pull-requests", targetWorkspaceId],
      });
      if (result) {
        queryClient.invalidateQueries({
          queryKey: [
            "workspace-pull-requests",
            result.pullRequest.sourceWorkspaceId,
          ],
        });
        // A Start and ship with agent recorded a Start: re-read the VM
        // state, as the chat's Start and send does.
        if (result.startCompute) invalidateWorkspaceVmQueries(queryClient);
      }
    },
    // A failed Start and ship with agent may still have recorded a Start
    // (the chat committed, the decision did not): re-read the gate before
    // the next action, so it does not offer a second Start on stale state.
    onError: (_error, input) => {
      if (input.agent?.startCompute) invalidateWorkspaceVmQueries(queryClient);
    },
  });
  return toMutationHook(mutation);
}

export function useMergeWorkspacePullRequest(
  workspaceId: string,
): MutationHook<
  MergeWorkspacePullRequestInput,
  MergeWorkspacePullRequestResponse
> {
  const queryClient = useQueryClient();
  const mutation = useMutation<
    MergeWorkspacePullRequestResponse,
    Error,
    MergeWorkspacePullRequestInput
  >({
    mutationFn: (input) =>
      api.post<MergeWorkspacePullRequestResponse>(
        `/api/v1/workspaces/${workspaceId}/pull-requests/${input.pullRequestId}/merge`,
        {},
      ),
    onSettled: (result) =>
      invalidatePullRequestMerge(queryClient, workspaceId, result),
  });
  return toMutationHook(mutation);
}

/**
 * Invalidate on settled (not only success): when a stale `ready` PR is
 * revalidated as conflicted/blocked, the backend commits the PR status change
 * and returns HTTP 409. Refreshing the target-scoped queries keeps the Base PR
 * panel from showing a stale `ready` row with an enabled Merge button.
 * Source-workspace ids are only known on success.
 */
function invalidatePullRequestMerge(
  queryClient: ReturnType<typeof useQueryClient>,
  workspaceId: string,
  result?: MergeWorkspacePullRequestResponse,
) {
  queryClient.invalidateQueries({
    queryKey: ["workspace-pull-requests", workspaceId],
  });
  queryClient.invalidateQueries({ queryKey: ["workspace", workspaceId] });
  queryClient.invalidateQueries({
    queryKey: ["workspace-tasks", workspaceId],
  });
  queryClient.invalidateQueries({ queryKey: ["workspaces"] });
  queryClient.invalidateQueries({
    queryKey: ["workspace-pr-merge-override", workspaceId],
  });
  if (result) {
    queryClient.invalidateQueries({
      queryKey: [
        "workspace-pull-requests",
        result.pullRequest.sourceWorkspaceId,
      ],
    });
    queryClient.invalidateQueries({
      queryKey: ["workspace", result.pullRequest.sourceWorkspaceId],
    });
    queryClient.invalidateQueries({
      queryKey: ["workspace-tasks", result.pullRequest.sourceWorkspaceId],
    });
  }
}

/**
 * What a merge with override would waive now and whether the caller may
 * waive it (g5.workspace.merge.admin-override.formal). The server alone
 * decides the authority; a refusal (403 for a caller who may not merge or is
 * not in a browser session) means no override is offered, so it is not
 * retried.
 */
export function useMergeOverridePreview(
  targetWorkspaceId: string,
  pullRequestId: string | null | undefined,
  enabled = true,
): AsyncState<MergeOverridePreview> & { refetch: () => Promise<unknown> } {
  const query = useQuery<MergeOverridePreview, Error>({
    queryKey: ["workspace-pr-merge-override", targetWorkspaceId, pullRequestId],
    queryFn: ({ signal }) =>
      api.get<MergeOverridePreview>(
        `/api/v1/workspaces/${targetWorkspaceId}/pull-requests/${encodeURIComponent(
          pullRequestId ?? "",
        )}/merge-override`,
        { signal },
      ),
    enabled: !!targetWorkspaceId && !!pullRequestId && enabled,
    retry: false,
  });
  return { ...toAsyncState(query), refetch: query.refetch };
}

/**
 * Merge with override: the ordinary merge body under a confirmed override.
 * Refusals keep their envelope (`ApiError`); `CONFLICT` with
 * `details.code === "merge_override_changed"` means the shown set changed.
 */
export function useMergeWithOverride(
  targetWorkspaceId: string,
): MutationHook<MergeWithOverrideInput, MergeWorkspacePullRequestResponse> {
  const queryClient = useQueryClient();
  const mutation = useMutation<
    MergeWorkspacePullRequestResponse,
    Error,
    MergeWithOverrideInput
  >({
    mutationFn: ({ pullRequestId, reason, digest }) =>
      api.post<MergeWorkspacePullRequestResponse>(
        `/api/v1/workspaces/${targetWorkspaceId}/pull-requests/${encodeURIComponent(
          pullRequestId,
        )}/merge-override`,
        { reason, confirmed: true, digest },
      ),
    onSettled: (result) =>
      invalidatePullRequestMerge(queryClient, targetWorkspaceId, result),
  });
  return toMutationHook(mutation);
}

function invalidatePullRequestAction(
  queryClient: ReturnType<typeof useQueryClient>,
  workspaceId: string,
  result?: WorkspacePullRequestActionResponse,
) {
  queryClient.invalidateQueries({
    queryKey: ["workspace-pull-requests", workspaceId],
  });
  queryClient.invalidateQueries({ queryKey: ["workspace", workspaceId] });
  queryClient.invalidateQueries({ queryKey: ["workspaces"] });
  if (result) {
    queryClient.invalidateQueries({
      queryKey: [
        "workspace-pull-requests",
        result.pullRequest.sourceWorkspaceId,
      ],
    });
    queryClient.invalidateQueries({
      queryKey: [
        "workspace-pull-requests",
        result.pullRequest.targetWorkspaceId,
      ],
    });
    queryClient.invalidateQueries({
      queryKey: ["workspace", result.pullRequest.sourceWorkspaceId],
    });
    queryClient.invalidateQueries({
      queryKey: ["workspace", result.pullRequest.targetWorkspaceId],
    });
  }
}

export function useRefreshWorkspacePullRequest(
  workspaceId: string,
): MutationHook<
  WorkspacePullRequestActionInput,
  WorkspacePullRequestActionResponse
> {
  const queryClient = useQueryClient();
  const mutation = useMutation<
    WorkspacePullRequestActionResponse,
    Error,
    WorkspacePullRequestActionInput
  >({
    mutationFn: (input) =>
      api.post<WorkspacePullRequestActionResponse>(
        `/api/v1/workspaces/${workspaceId}/pull-requests/${input.pullRequestId}/refresh`,
        {},
      ),
    onSettled: (result) =>
      invalidatePullRequestAction(queryClient, workspaceId, result),
  });
  return toMutationHook(mutation);
}

export function useCloseWorkspacePullRequest(
  workspaceId: string,
): MutationHook<
  WorkspacePullRequestActionInput,
  WorkspacePullRequestActionResponse
> {
  const queryClient = useQueryClient();
  const mutation = useMutation<
    WorkspacePullRequestActionResponse,
    Error,
    WorkspacePullRequestActionInput
  >({
    mutationFn: (input) =>
      api.post<WorkspacePullRequestActionResponse>(
        `/api/v1/workspaces/${workspaceId}/pull-requests/${input.pullRequestId}/close`,
        {},
      ),
    onSettled: (result) =>
      invalidatePullRequestAction(queryClient, workspaceId, result),
  });
  return toMutationHook(mutation);
}

export function useWorkspaceVm(
  workspaceId: string,
): RefreshingState<WorkspaceVmView> {
  const query = useQuery<WorkspaceVmView, Error>({
    queryKey: ["workspace-vm", workspaceId],
    queryFn: ({ signal }) =>
      api.get<WorkspaceVmView>(`/api/v1/workspaces/${workspaceId}/vm`, {
        signal,
      }),
    enabled: !!workspaceId,
    refetchInterval: (query) => {
      const status = query.state.data?.status;
      return status === "starting" ||
        status === "queued_for_capacity" ||
        status === "stopping"
        ? 2_000
        : 5_000;
    },
  });
  // A failed poll is not a stop: the editors stay as they were until the
  // state is known again (`g5.workspace.page.refresh`).
  return toRefreshingState(query);
}

const workspaceLifecycleMutationKey = ["workspace-lifecycle"] as const;

/** Request state follows the submitted workspace, including across page remounts. */
function useWorkspaceLifecycleAction<TInput, TOutput>(
  workspaceId: string,
  action: "resume" | "stop" | "transfer",
  request: (target: string, input: TInput) => Promise<TOutput>,
  settle: (target: string) => void,
): MutationHook<TInput, TOutput> {
  const queryClient = useQueryClient();
  const workspaceKey = [...workspaceLifecycleMutationKey, workspaceId];
  const mutationKey = [...workspaceKey, action];
  // Keep this subscription's filter stable. useMutationState does not refresh
  // its snapshot just because filters change; select the workspace during render.
  const actions = useMutationState({
    filters: { mutationKey: workspaceLifecycleMutationKey },
    select: (entry) => ({
      workspaceId: (
        entry.state.variables as { workspaceId: string } | undefined
      )?.workspaceId,
      action: entry.options.mutationKey?.[2],
      status: entry.state.status,
      error: entry.state.error as Error | null,
      data: entry.state.data as TOutput | undefined,
    }),
  });
  const latest = actions
    .filter(
      (entry) => entry.workspaceId === workspaceId && entry.action === action,
    )
    .at(-1);
  const mutation = useMutation<
    TOutput,
    Error,
    { workspaceId: string; input: TInput }
  >({
    mutationKey,
    mutationFn: ({ workspaceId: target, input }) => request(target, input),
    onSettled: (_data, _error, { workspaceId: target }) => settle(target),
  });
  const inFlight = () =>
    queryClient.isMutating({ mutationKey: workspaceKey }) > 0;
  return {
    isLoading: latest?.status === "pending",
    error: latest?.status === "error" ? latest.error : null,
    data: latest?.status === "success" ? latest.data : undefined,
    mutate: (input) => {
      if (!inFlight()) mutation.mutate({ workspaceId, input });
    },
    mutateAsync: (input) =>
      inFlight()
        ? Promise.reject(
            new Error("A workspace lifecycle action is already in flight"),
          )
        : mutation.mutateAsync({ workspaceId, input }),
    reset: () => {
      // Reset presentation only; a pending request must remain discoverable.
      for (const entry of queryClient
        .getMutationCache()
        .findAll({ mutationKey, exact: true })) {
        if (entry.state.status !== "pending")
          queryClient.getMutationCache().remove(entry);
      }
      mutation.reset();
    },
  };
}

function useWorkspaceVmAction<T>(
  workspaceId: string,
  action: "resume" | "stop",
): MutationHook<void, T> {
  const queryClient = useQueryClient();
  return useWorkspaceLifecycleAction<void, T>(
    workspaceId,
    action,
    (target) => api.post<T>(`/api/v1/workspaces/${target}/${action}`, {}),
    (target) => {
      queryClient.invalidateQueries({
        queryKey: ["workspace-vm", target],
      });
      queryClient.invalidateQueries({ queryKey: ["workspace", target] });
      queryClient.invalidateQueries({
        queryKey: ["workspace-file", target],
      });
      queryClient.invalidateQueries({
        queryKey: ["workspace-changes", target],
      });
      // A start or stop decides whether queued turns count as active
      // (chat g5.chat.activity.turns).
      queryClient.invalidateQueries({
        queryKey: ["chat-sessions", target],
      });
    },
  );
}

export function useResumeWorkspaceVm(
  workspaceId: string,
): MutationHook<void, WorkspaceResumeOutcome> {
  return useWorkspaceVmAction(workspaceId, "resume");
}

export function useStopWorkspaceVm(
  workspaceId: string,
): MutationHook<void, WorkspaceStopOutcome> {
  return useWorkspaceVmAction(workspaceId, "stop");
}

export function useTransferWorkspace(
  workspaceId: string,
): MutationHook<
  { toUserId: string; idempotencyKey: string },
  WorkspaceTransferOutcome
> {
  const queryClient = useQueryClient();
  return useWorkspaceLifecycleAction<
    { toUserId: string; idempotencyKey: string },
    WorkspaceTransferOutcome
  >(
    workspaceId,
    "transfer",
    (target, { toUserId, idempotencyKey }) =>
      api.post<WorkspaceTransferOutcome>(
        `/api/v1/workspaces/${target}/transfer`,
        { toUserId, idempotencyKey },
      ),
    (target) => {
      queryClient.removeQueries({
        queryKey: ["workspace-file", target],
      });
      queryClient.removeQueries({ queryKey: ["file-tree", target] });
      queryClient.removeQueries({ queryKey: ["file-content", target] });
      queryClient.removeQueries({
        queryKey: ["workspace-changes", target],
      });
      queryClient.removeQueries({
        queryKey: ["workspace-preview", target],
      });
      queryClient.invalidateQueries({
        queryKey: ["workspace-vm", target],
      });
      queryClient.invalidateQueries({
        queryKey: ["workspace", target],
      });
      queryClient.invalidateQueries({ queryKey: ["workspaces"] });
    },
  );
}
