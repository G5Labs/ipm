// =============================================================================
// G5 Client — Task Queries & Mutations
// =============================================================================

import {
  useInfiniteQuery,
  useMutation,
  useQueries,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { useMemo } from "react";
import { api } from "../lib/api";
import type {
  AsyncState,
  CommitDiffResponse,
  CommitFileDiffResponse,
  CommitHistoryEntry,
  CommitHistoryPage,
  CommitSummary,
  CommitPreviewProfileResult,
  CreatePreviewProfileInput,
  PreviewApplication,
  PreviewProfile,
  MutationHook,
  Page,
  TaskDetail,
  TaskListEntry,
  TaskStatus,
  TimelineEntry,
} from "../lib/types";
import { toAsyncState, toMutationHook } from "./hookUtils";

/** Task statuses that end a task (no further updates expected). */
export const TERMINAL_TASK_STATUSES: ReadonlySet<TaskStatus> =
  new Set<TaskStatus>(["succeeded", "failed", "cancelled"]);

/** Polling interval (ms) used as a fallback when WebSocket events are missed. */
export const ACTIVE_TASK_POLL_INTERVAL = 5_000;

export type PaginatedAsyncState<T> = AsyncState<T> & {
  fetchNextPage: () => Promise<void>;
  hasNextPage: boolean;
  isFetchingNextPage: boolean;
  total: number;
  paginationError: Error | null;
  retry: () => Promise<void>;
};

/**
 * Fetch the commits on a workspace branch (newest first).
 *
 * Only runs when `enabled` is true and a branch id is present — the preview
 * popover enables it lazily, so the request is not made until the user opens
 * the popover in unmerged (branch) mode.
 */
export function useBranchCommits(
  workspaceId: string,
  repoId: string,
  enabled: boolean,
): AsyncState<CommitSummary[]> {
  const query = useQuery<CommitSummary[], Error>({
    queryKey: ["branch-commits", workspaceId, repoId],
    queryFn: ({ signal }) =>
      api.get<CommitSummary[]>(
        `/api/v1/workspaces/${workspaceId}/repos/${repoId}/commits`,
        {
          signal,
        },
      ),
    enabled: enabled && !!workspaceId,
  });
  return toAsyncState(query);
}

export const SOURCE_CONTROL_HISTORY_QUERY_KEY = "source-control-history";
const SOURCE_CONTROL_HISTORY_PAGE_SIZE = 10;

export interface BranchCommitHistoryQuery {
  commits: CommitHistoryEntry[];
  isLoading: boolean;
  error: Error | null;
  hasNextPage: boolean;
  isFetchingNextPage: boolean;
  fetchNextPage: () => Promise<void>;
  retry: () => Promise<void>;
  restart: () => Promise<void>;
}

/** Demand-loaded, branch-scoped first-parent Source Control history. */
export function useBranchCommitHistory(
  workspaceId: string,
  repoId: string,
  enabled: boolean,
): BranchCommitHistoryQuery {
  const queryClient = useQueryClient();
  const queryKey = [
    SOURCE_CONTROL_HISTORY_QUERY_KEY,
    repoId,
    workspaceId,
  ] as const;
  const query = useInfiniteQuery<CommitHistoryPage, Error>({
    queryKey,
    queryFn: ({ signal, pageParam }) =>
      api.get<CommitHistoryPage>(
        `/api/v1/workspaces/${workspaceId}/repos/${repoId}/commits/history`,
        {
          signal,
          params: {
            limit: SOURCE_CONTROL_HISTORY_PAGE_SIZE,
            cursor: typeof pageParam === "string" ? pageParam : undefined,
          },
        },
      ),
    initialPageParam: null as string | null,
    getNextPageParam: (lastPage) => lastPage.nextCursor ?? undefined,
    enabled: enabled && !!repoId && !!workspaceId,
  });
  const commits = useMemo(() => {
    const seen = new Set<string>();
    const result: CommitHistoryEntry[] = [];
    for (const page of query.data?.pages ?? []) {
      for (const commit of page.commits) {
        if (seen.has(commit.sha)) continue;
        seen.add(commit.sha);
        result.push(commit);
      }
    }
    return result;
  }, [query.data?.pages]);

  return {
    commits,
    isLoading: query.isPending,
    error: query.error,
    hasNextPage: !!query.hasNextPage,
    isFetchingNextPage: query.isFetchingNextPage,
    fetchNextPage: async () => {
      await query.fetchNextPage();
    },
    retry: async () => {
      await query.refetch();
    },
    restart: async () => {
      await queryClient.resetQueries({ queryKey, exact: true });
    },
  };
}

export function useBranchCommitDiff(
  workspaceId: string,
  repoId: string,
  sha: string,
): AsyncState<CommitDiffResponse> {
  const query = useQuery<CommitDiffResponse, Error>({
    queryKey: ["source-control-commit-diff", repoId, workspaceId, sha],
    queryFn: ({ signal }) =>
      api.get<CommitDiffResponse>(
        `/api/v1/workspaces/${workspaceId}/repos/${repoId}/commits/${sha}/diff`,
        { signal },
      ),
    enabled: !!repoId && !!workspaceId && !!sha,
    staleTime: Infinity,
  });
  return toAsyncState(query);
}

export function useBranchCommitFileDiff(
  workspaceId: string,
  repoId: string,
  sha: string,
  path: string,
): AsyncState<CommitFileDiffResponse> {
  const encodedPath = path.split("/").map(encodeURIComponent).join("/");
  const query = useQuery<CommitFileDiffResponse, Error>({
    queryKey: [
      "source-control-commit-file-diff",
      repoId,
      workspaceId,
      sha,
      path,
    ],
    queryFn: ({ signal }) =>
      api.get<CommitFileDiffResponse>(
        `/api/v1/workspaces/${workspaceId}/repos/${repoId}/commits/${sha}/diff/files/${encodedPath}`,
        { signal },
      ),
    enabled: !!repoId && !!workspaceId && !!sha && !!path,
    staleTime: Infinity,
  });
  return toAsyncState(query);
}

/**
 * Fetch the named preview profiles committed on a workspace branch
 * (`.g5/preview_profiles/*.yaml`). Lazily enabled like `useBranchCommits` — the
 * preview modal turns it on only while open.
 */
export function usePreviewProfiles(
  workspaceId: string,
  repoId: string,
  enabled: boolean,
  mode: "unmerged" | "merged" = "unmerged",
): AsyncState<PreviewProfile[]> {
  const query = useQuery<PreviewProfile[], Error>({
    // `mode` is part of the key + URL: a merged preview boots the base branch,
    // so its profiles must be listed from that same ref (else the picker offers
    // branch-only profiles a merged preview would reject). Toggling mode
    // refetches.
    queryKey: ["preview-profiles", workspaceId, repoId, mode],
    queryFn: ({ signal }) =>
      api.get<PreviewProfile[]>(
        `/api/v1/workspaces/${workspaceId}/repos/${repoId}/preview-profiles?mode=${mode}`,
        { signal },
      ),
    enabled: enabled && !!workspaceId,
  });
  return toAsyncState(query);
}

export interface PreviewApplicationTarget extends PreviewApplication {
  key: string;
  workspaceId: string;
  repoId: string;
  repoName: string;
  branchName: string;
}

/**
 * Where preview applications are read from: the owner's live checkout
 * (needs the running VM; the source used at launch) or the configuration
 * committed on the branch, which a stopped workspace can still list
 * (g5.preview.start-and-preview).
 */
export type PreviewApplicationSource = "live" | "committed";

/**
 * Discover repo-qualified Preview applications across an already validated
 * set of code-repository branches. Reads are lazy and occur only while the
 * launcher is open; result order never selects a target.
 */
export function usePreviewApplications(
  branches: readonly {
    workspaceId: string;
    repoId: string;
    repoName: string;
    branchName: string;
  }[],
  enabled: boolean,
  mode: "unmerged" | "merged" = "unmerged",
  source: PreviewApplicationSource = "live",
): AsyncState<PreviewApplicationTarget[]> {
  const queries = useQueries({
    queries: branches.map((branch) => ({
      queryKey: [
        "preview-applications",
        branch.workspaceId,
        branch.repoId,
        mode,
        source,
      ] as const,
      queryFn: ({ signal }: { signal: AbortSignal }) =>
        api.get<PreviewApplication[]>(
          `/api/v1/workspaces/${branch.workspaceId}/repos/${branch.repoId}/preview-applications?mode=${mode}${
            source === "committed" ? "&source=committed" : ""
          }`,
          { signal },
        ),
      enabled: enabled && !!branch.repoId,
    })),
  });

  if (!enabled || queries.some((query) => query.isPending)) {
    return { status: "loading" };
  }
  const failed = queries.find((query) => query.isError);
  if (failed?.error) return { status: "error", error: failed.error };

  return {
    status: "ok",
    data: branches.flatMap((branch, index) =>
      ((queries[index]?.data as PreviewApplication[] | undefined) ?? []).map(
        (application) => ({
          ...application,
          key: `${branch.repoId}:${application.source}:${application.profile ?? ""}`,
          workspaceId: branch.workspaceId,
          repoId: branch.repoId,
          repoName: branch.repoName,
          branchName: branch.branchName,
        }),
      ),
    ),
  };
}

/**
 * Create a named preview profile by committing
 * `.g5/preview_profiles/<name>.yaml` to the branch. Invalidates the profiles
 * list and the branch commits/files (the commit advanced the branch head).
 */
export function useCommitPreviewProfile(
  workspaceId: string,
  repoId: string,
): MutationHook<CreatePreviewProfileInput, CommitPreviewProfileResult> {
  const queryClient = useQueryClient();
  const mutation = useMutation<
    CommitPreviewProfileResult,
    Error,
    CreatePreviewProfileInput
  >({
    mutationFn: (input) =>
      api.post<CommitPreviewProfileResult>(
        `/api/v1/workspaces/${workspaceId}/repos/${repoId}/preview-profiles`,
        input,
      ),
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: ["preview-profiles", workspaceId, repoId],
      });
      queryClient.invalidateQueries({
        queryKey: ["preview-applications", workspaceId, repoId],
      });
      queryClient.invalidateQueries({
        queryKey: ["branch-commits", workspaceId, repoId],
      });
    },
  });
  return toMutationHook(mutation);
}

export function useTask(taskId: string): AsyncState<TaskDetail> {
  const query = useQuery<TaskDetail, Error>({
    queryKey: ["task", taskId],
    queryFn: ({ signal }) =>
      api.get<TaskDetail>(`/api/v1/tasks/${taskId}`, { signal }),
    enabled: !!taskId,
    // Keep the selected task fresh even if an event races or is missed.
    // Native input requests can transition a task into `waiting` after the
    // drawer opens, so polling repairs missed or reordered events.
    refetchInterval: taskId ? ACTIVE_TASK_POLL_INTERVAL : false,
  });
  return toAsyncState(query);
}

type TaskTimelinePage = Page<TimelineEntry> & {};

export function useTaskTimeline(
  taskId: string,
  _cursor?: string,
): {
  timelineState: AsyncState<Page<TimelineEntry>>;
  fetchNextPage: () => void;
  hasNextPage: boolean;
  isFetchingNextPage: boolean;
} {
  const query = useInfiniteQuery<TaskTimelinePage, Error>({
    queryKey: ["task-timeline", taskId],
    queryFn: async ({ pageParam, signal }) => {
      const res = await api.get<{
        entries: TimelineEntry[];
        cursor: string | null;
        hasMore: boolean;
      }>(`/api/v1/tasks/${taskId}/timeline`, {
        signal,
        params: { cursor: pageParam as string | undefined },
      });
      // Normalise API shape ({ entries }) to Page<T> ({ items })
      return {
        items: res.entries,
        cursor: res.cursor ?? null,
        hasMore: res.hasMore ?? false,
      };
    },
    initialPageParam: undefined as string | undefined,
    getNextPageParam: (lastPage) =>
      lastPage.hasMore ? (lastPage.cursor ?? undefined) : undefined,
    enabled: !!taskId,
  });

  // Map infinite query result to AsyncState<Page<TimelineEntry>>
  let timelineState: AsyncState<Page<TimelineEntry>>;
  if (query.isPending) {
    timelineState = { status: "loading" };
  } else if (query.isError) {
    timelineState = { status: "error", error: query.error };
  } else {
    // Flatten pages into a single Page<TimelineEntry>
    const allItems = query.data.pages.flatMap((p) => p.items);
    const lastPage = query.data.pages[query.data.pages.length - 1];
    timelineState = {
      status: "ok",
      data: {
        items: allItems,
        cursor: lastPage?.cursor ?? null,
        hasMore: lastPage?.hasMore ?? false,
      },
    };
  }

  return {
    timelineState,
    fetchNextPage: () => {
      query.fetchNextPage();
    },
    hasNextPage: query.hasNextPage ?? false,
    isFetchingNextPage: query.isFetchingNextPage,
  };
}

export interface WorkspaceTaskPage {
  tasks: TaskListEntry[];
  cursor: string | null;
  hasMore: boolean;
}

export function workspaceTasksQueryOptions(
  workspaceId: string,
  limit = 50,
  activeOnly = false,
) {
  return {
    queryKey: ["workspace-tasks", workspaceId, limit, activeOnly] as const,
    queryFn: ({
      pageParam,
      signal,
    }: {
      pageParam: string | null;
      signal: AbortSignal;
    }) =>
      api.get<WorkspaceTaskPage>(`/api/v1/workspaces/${workspaceId}/tasks`, {
        signal,
        params: {
          limit,
          ...(pageParam ? { cursor: pageParam } : {}),
          ...(activeOnly ? { activeOnly: true } : {}),
        },
      }),
    initialPageParam: null as string | null,
    getNextPageParam: (lastPage: WorkspaceTaskPage) =>
      lastPage.hasMore ? lastPage.cursor : undefined,
    enabled: !!workspaceId,
    refetchInterval: 5000,
  };
}

/** Detect activity independently of which history pages the caller has loaded. */
export function useWorkspaceHasActiveTasks(workspaceId: string): boolean {
  // Filtering happens before the limit, so one result proves activity even
  // when a running task is older than every row in the visible history page.
  const query = useInfiniteQuery(
    workspaceTasksQueryOptions(workspaceId, 1, true),
  );
  return (
    query.data?.pages.some((page) =>
      page.tasks.some((task) => !TERMINAL_TASK_STATUSES.has(task.status)),
    ) ?? false
  );
}

export function useWorkspaceTasks(
  workspaceId: string,
): PaginatedAsyncState<TaskListEntry[]> {
  const query = useInfiniteQuery(workspaceTasksQueryOptions(workspaceId));
  const pagination = {
    fetchNextPage: async () => {
      await query.fetchNextPage();
    },
    paginationError: query.error,
    retry: async () => {
      await query.refetch();
    },
    hasNextPage: query.hasNextPage,
    isFetchingNextPage: query.isFetchingNextPage,
    total: query.data?.pages.reduce((n, page) => n + page.tasks.length, 0) ?? 0,
  };
  if (query.isPending) return { status: "loading", ...pagination };
  if (!query.data)
    return {
      status: "error",
      error: query.error ?? new Error("Tasks unavailable"),
      ...pagination,
    };
  const byId = new Map(
    query.data.pages
      .flatMap((page) => page.tasks)
      .map((task) => [task.id, task]),
  );
  return { status: "ok", data: [...byId.values()], ...pagination };
}

/**
 * The sessions that currently have a non-terminal task, from the active-only
 * task view (ui g5.ui.guided-panel): the Guided panel shows a step as running
 * while the session the page started for it is still working. Polls like the
 * task list; empty while disabled, loading or failed, so an unknown job never
 * reads as running.
 */
export function useWorkspaceActiveSessionIds(
  workspaceId: string,
  enabled = true,
): ReadonlySet<string> {
  const query = useInfiniteQuery({
    ...workspaceTasksQueryOptions(workspaceId, 50, true),
    enabled: enabled && !!workspaceId,
  });
  return useMemo(() => {
    const ids = new Set<string>();
    for (const page of query.data?.pages ?? []) {
      for (const task of page.tasks) {
        if (task.sessionId && !TERMINAL_TASK_STATUSES.has(task.status)) {
          ids.add(task.sessionId);
        }
      }
    }
    return ids;
  }, [query.data]);
}
