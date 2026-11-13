import { useWorkspaceReadAuthority } from "../lib/workspaceLiveGate";
// =============================================================================
// G5 Client — File Tree, Content, Watch & Git Status
// =============================================================================

import { useCallback, useRef, useMemo, useLayoutEffect } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../lib/api";
import { routes } from "../lib/routes";
import type { FileTreeEntry, AsyncState, MutationHook } from "../lib/types";
import {
  toAsyncState,
  toMutationHook,
  toRefreshingState,
  type RefreshingState,
} from "./hookUtils";

// ---------------------------------------------------------------------------
// Query Hooks
// ---------------------------------------------------------------------------

/** Response shape from GET /api/v1/workspaces/:id/repos/:id/files */
export interface FileTreeResponse {
  entries: FileTreeEntry[];
  /** The listing stopped at the tree's cap; `entries` holds what fit. */
  truncated?: boolean;
  source?: string;
  /** `null` for a live listing whose checkout HEAD the VM could not read. */
  repoOid?: string | null;
  snapshotAt?: string;
  liveFallback?: string | null;
}

/** A file tree with the source cut it was read from (g5.documents.read.labels). */
export interface FileTreeRead {
  entries: FileTreeEntry[];
  /**
   * The subtree has more entries than one listing carries: `entries` is
   * the part that fit under the tree's cap.
   */
  truncated: boolean;
  /** `"live"` for the attached owner's checkout; otherwise the durable cut. */
  source: string | null;
  repoOid: string | null;
  snapshotAt: string | null;
  /**
   * Set when the attached owner got the durable cut instead of the live
   * checkout: `"entry_cap"` when the checkout exceeds the live tree's size.
   */
  liveFallback: string | null;
}

export function toFileTreeRead(
  response: FileTreeEntry[] | FileTreeResponse,
): FileTreeRead {
  if (Array.isArray(response)) {
    return {
      entries: response,
      truncated: false,
      source: null,
      repoOid: null,
      snapshotAt: null,
      liveFallback: null,
    };
  }
  return {
    entries: response.entries,
    truncated: response.truncated ?? false,
    source: response.source ?? null,
    repoOid: response.repoOid ?? null,
    snapshotAt: response.snapshotAt ?? null,
    liveFallback: response.liveFallback ?? null,
  };
}

export function useFileTreeRead(
  workspaceId: string,
  repoId: string,
  path?: string,
  depth?: number,
  options: {
    enabled?: boolean;
    refetchIntervalMs?: number | false;
    authorityKey?: string;
  } = {},
): AsyncState<FileTreeRead> {
  const contextAuthority = useWorkspaceReadAuthority();
  const authority = options.authorityKey ?? contextAuthority;
  const enabled = options.enabled ?? true;
  const queryEnabled = !!workspaceId && !!repoId && enabled;
  const query = useQuery<
    FileTreeEntry[] | FileTreeResponse,
    Error,
    FileTreeRead
  >({
    queryKey: [
      "file-tree",
      workspaceId,
      repoId,
      path,
      depth,
      ...(authority ? [authority] : []),
    ],
    queryFn: ({ signal }) =>
      api.get<FileTreeEntry[] | FileTreeResponse>(
        `/api/v1/workspaces/${workspaceId}/repos/${repoId}/files`,
        {
          signal,
          params: { path, depth },
        },
      ),
    select: toFileTreeRead,
    enabled: queryEnabled,
    refetchInterval: queryEnabled
      ? (options.refetchIntervalMs ?? false)
      : false,
  });
  return toAsyncState(query);
}

export function useFileTree(
  workspaceId: string,
  repoId: string,
  path?: string,
  depth?: number,
  options: {
    enabled?: boolean;
    refetchIntervalMs?: number | false;
    authorityKey?: string;
  } = {},
): AsyncState<FileTreeEntry[]> {
  const read = useFileTreeRead(workspaceId, repoId, path, depth, options);
  return read.status === "ok"
    ? { status: "ok", data: read.data.entries }
    : read;
}

/** Response shape from GET /api/v1/branches/:repoId/files/*path */
interface FileContentResponse {
  path: string;
  content: string;
  size: number;
  encoding: string;
  draftDirty: boolean;
}

/** A workspace-scoped file read with the authority source that produced it. */
export interface WorkspaceFileContent {
  path: string;
  content: string;
  size: number;
  encoding: "utf-8" | "base64";
  hash: string;
  source: "live" | "flush" | "committed";
  repoOid: string | null;
  snapshotAt: string | null;
  vmId?: string | null;
}

export interface WorkspaceFileWriteResult {
  path: string;
  size: number;
  encoding: "utf-8" | "base64";
  hash: string;
  vmId?: string | null;
}

export interface WorkspaceChangeFile {
  status: string;
  /** Literal repository-relative path; for a rename, the new path. */
  path: string;
  /** For a rename or copy, the original path. */
  from?: string;
}

export interface WorkspaceChangeMember {
  repo_id: string;
  root_path: string;
  dirty: WorkspaceChangeFile[] | null;
  errors: string[];
  shared_branch?: string;
  head?: string | null;
  on_shared_branch?: boolean;
  against_internal?: { ahead: number | null; behind: number | null };
  against_upstream?: { ahead: number | null; behind: number | null };
  /**
   * Checkout HEAD as the live (attached) report names it; differs from
   * `internal_ref` when commits are unpushed.
   */
  head_oid?: string | null;
  /** Checkout HEAD as the durable (detached) report names it. */
  local_head_oid?: string | null;
  /** Published (internal) branch tip, or null when it does not exist yet. */
  internal_ref?: string | null;
  /** An action's own words on a failure (the VM's stderr), when it has any. */
  detail?: string | null;
  /** Update: paths it could not merge when it failed. */
  conflicts?: string[] | null;
  /** Update: paths where the source's version was kept over the copy's commit. */
  resolved?: string[] | null;
}

export interface WorkspaceChanges {
  workspace_id: string;
  source: "live" | "durable";
  attached: boolean;
  members: WorkspaceChangeMember[];
  status?: "succeeded" | "failed";
}

/**
 * Read the authoritative live or durable Git projection selected by the API.
 *
 * `refetchIntervalMs` polls only while the report is live (attached): the
 * checkout changes underneath the UI through the terminal and agent turns
 * (design v9 §5.7), so a view that shows association state cannot rely on a
 * fetch made at page load. Durable reports change only through the API's own
 * mutations, which invalidate the query.
 */
export function useWorkspaceChanges(
  workspaceId: string,
  options: { refetchIntervalMs?: number | false } = {},
): AsyncState<WorkspaceChanges> {
  const refetchIntervalMs = options.refetchIntervalMs ?? false;
  const query = useQuery<WorkspaceChanges, Error>({
    queryKey: ["workspace-changes", workspaceId],
    queryFn: ({ signal }) =>
      api.get<WorkspaceChanges>(routes.workspaces.changes(workspaceId), {
        signal,
      }),
    enabled: !!workspaceId,
    refetchInterval: (query) =>
      refetchIntervalMs !== false && query.state.data?.attached === true
        ? refetchIntervalMs
        : false,
  });
  return toAsyncState(query);
}

/**
 * The changes report fetched fresh for a decision made *now*: the Finish
 * review dialog must not enable its confirmation on a cached report that
 * predates the reviewer's latest terminal or agent edits. The query key
 * carries a per-mount token, so the request is always issued after the
 * dialog opened: it is neither served from the shared cache nor merged with
 * a request the page already had in flight. The result is not retained.
 */
export type WorkspaceChangesActionInput = {
  action: "push" | "update" | "switch-back" | "commit";
  repoId?: string;
  /** The commit message; `commit` only. */
  message?: string;
};

export function useWorkspaceChangesAction(
  workspaceId: string,
): MutationHook<WorkspaceChangesActionInput, WorkspaceChanges> {
  const queryClient = useQueryClient();
  const mutation = useMutation<
    WorkspaceChanges,
    Error,
    WorkspaceChangesActionInput
  >({
    mutationFn: ({ action, repoId, message }) =>
      api.post<WorkspaceChanges>(
        routes.workspaces.changesAction(workspaceId, action),
        {
          ...(repoId ? { repoId } : {}),
          ...(action === "commit" ? { message } : {}),
        },
      ),
    onSettled: (_data, _error, input) => {
      queryClient.invalidateQueries({
        queryKey: ["workspace-changes", workspaceId],
      });
      if (input.action === "commit" || input.action === "push") {
        // A commit or a push moves the branch tip the reviews are measured
        // against: rounds regroup under the new commit and the flat
        // overlays drop what was reviewed before it.
        queryClient.invalidateQueries({
          queryKey: ["workspace-review-publications", workspaceId],
        });
        queryClient.invalidateQueries({
          queryKey: ["workspace-review-suggestions", workspaceId],
        });
        queryClient.invalidateQueries({
          queryKey: ["workspace-review-suggestion-counts", workspaceId],
        });
        // Source control's History and Diff Graph read the branch tip that
        // just moved; refreshed here rather than only on the branch event.
        queryClient.invalidateQueries({ queryKey: ["source-control-history"] });
        queryClient.invalidateQueries({
          queryKey: ["source-control-change-graph"],
        });
      }
      queryClient.invalidateQueries({ queryKey: ["file-tree"] });
      queryClient.invalidateQueries({
        queryKey: ["workspace-file", workspaceId],
      });
      // A review copy's preview (its changes and where it stands against
      // the source) describes the tree an Update just rebased or a push
      // just published: whoever shows it refetches.
      queryClient.invalidateQueries({
        queryKey: ["workspace-review-preview", workspaceId],
      });
    },
  });
  return toMutationHook(mutation);
}

function workspaceFileUrl(workspaceId: string, path: string): string {
  return `/api/v1/workspaces/${workspaceId}/files/${encodeRepoPath(path)}`;
}

export async function fetchWorkspaceFile(
  workspaceId: string,
  repoId: string,
  path: string,
  signal?: AbortSignal,
): Promise<WorkspaceFileContent> {
  return api.get<WorkspaceFileContent>(workspaceFileUrl(workspaceId, path), {
    signal,
    params: { repoId },
  });
}

/**
 * Reads through the workspace authority facade. The backend resolves the actor
 * to either the attached live incarnation or a durable Git snapshot before it
 * contacts the runtime.
 */
export function useWorkspaceFileContent(
  workspaceId: string,
  repoId: string,
  path: string,
): {
  state: RefreshingState<WorkspaceFileContent>;
  reload: () => Promise<WorkspaceFileContent>;
} {
  const queryClient = useQueryClient();
  const authority = useWorkspaceReadAuthority();
  const currentAuthority = useRef(authority);
  useLayoutEffect(() => {
    currentAuthority.current = authority;
  }, [authority]);
  const queryKey = useMemo(
    () =>
      [
        "workspace-file",
        workspaceId,
        repoId,
        path,
        ...(authority ? [authority] : []),
      ] as const,
    [authority, workspaceId, repoId, path],
  );
  const query = useQuery<WorkspaceFileContent, Error>({
    queryKey,
    queryFn: ({ signal }) =>
      fetchWorkspaceFile(workspaceId, repoId, path, signal),
    enabled: !!workspaceId && !!workspaceId && !!repoId && !!path,
  });

  const reload = useCallback(async () => {
    const value = await fetchWorkspaceFile(workspaceId, repoId, path);
    if (currentAuthority.current !== authority)
      throw new Error("File authority changed while reading");
    queryClient.setQueryData(queryKey, value);
    return value;
  }, [authority, path, queryClient, queryKey, repoId, workspaceId]);

  return { state: toRefreshingState(query), reload };
}

export function useSaveWorkspaceFile(): MutationHook<
  {
    workspaceId: string;
    repoId: string;
    path: string;
    content: string;
    encoding?: "utf-8" | "base64";
    expectedHash: string | null;
    expectedVmId?: string | null;
    commitAndPush?: boolean;
  },
  WorkspaceFileWriteResult
> {
  const queryClient = useQueryClient();
  const authority = useWorkspaceReadAuthority();
  const mutation = useMutation<
    WorkspaceFileWriteResult,
    Error,
    {
      workspaceId: string;
      repoId: string;
      path: string;
      content: string;
      encoding?: "utf-8" | "base64";
      expectedHash: string | null;
      expectedVmId?: string | null;
      commitAndPush?: boolean;
    },
    { authority?: string }
  >({
    onMutate: () => ({ authority }),
    mutationFn: ({
      workspaceId,
      repoId,
      path,
      content,
      encoding = "utf-8",
      expectedHash,
      expectedVmId,
      commitAndPush = true,
    }) =>
      api.put<WorkspaceFileWriteResult>(
        workspaceFileUrl(workspaceId, path),
        {
          content,
          encoding,
          expectedHash,
          expectedVmId,
          commitAndPush,
        },
        {
          params: { repoId },
        },
      ),
    onSuccess: (result, variables, context) => {
      const prefixKey = [
        "workspace-file",
        variables.workspaceId,
        variables.repoId,
        variables.path,
      ];
      const fileKey = [
        ...prefixKey,
        ...(context?.authority ? [context.authority] : []),
      ];
      const shown = queryClient.getQueryData<WorkspaceFileContent>(fileKey);
      if (
        shown &&
        shown.hash !== variables.expectedHash &&
        shown.hash !== result.hash
      ) {
        // The page read a version this Save neither replaced nor wrote while
        // it was in flight: read the file again rather than guess which of
        // the two is newer.
        queryClient.invalidateQueries({ queryKey: prefixKey });
      } else {
        queryClient.setQueryData<WorkspaceFileContent>(fileKey, (previous) =>
          variables.expectedVmId && previous?.vmId !== variables.expectedVmId
            ? previous
            : {
                path: variables.path,
                content: variables.content,
                size: result.size,
                encoding: result.encoding,
                hash: result.hash,
                source: "live",
                repoOid: previous?.repoOid ?? null,
                snapshotAt: null,
                vmId: result.vmId ?? previous?.vmId ?? null,
              },
        );
      }
      queryClient.invalidateQueries({ queryKey: prefixKey });
      // Questions now save through the common file path. Refresh their
      // aggregate once the submitted file is acknowledged, never on typing.
      queryClient.invalidateQueries({
        queryKey: ["workspace-open-question-metrics"],
        predicate: ({ queryKey }) =>
          Array.isArray(queryKey[2]) &&
          queryKey[2].includes(variables.workspaceId),
      });
      queryClient.invalidateQueries({
        queryKey: ["workspace-changes", variables.workspaceId],
      });
      queryClient.invalidateQueries({
        queryKey: ["workspace", variables.workspaceId],
      });
    },
  });
  return toMutationHook(mutation);
}

export function useFileContent(
  workspaceId: string,
  repoId: string,
  path: string,
): AsyncState<string> {
  const query = useQuery<string, Error>({
    queryKey: ["file-content", workspaceId, repoId, path],
    queryFn: async ({ signal }) => {
      const resp = await api.get<FileContentResponse>(
        `/api/v1/workspaces/${workspaceId}/repos/${repoId}/files/${path.split("/").map(encodeURIComponent).join("/")}`,
        { signal },
      );
      return resp.content;
    },
    enabled: !!workspaceId && !!repoId && !!path,
  });
  return toAsyncState(query);
}

function encodeRepoPath(path: string): string {
  return path.split("/").map(encodeURIComponent).join("/");
}

export function useDeleteFileEntry(): MutationHook<
  { workspaceId: string; repoId: string; path: string },
  void
> {
  const queryClient = useQueryClient();
  const mutation = useMutation<
    void,
    Error,
    { workspaceId: string; repoId: string; path: string }
  >({
    mutationFn: ({ workspaceId, repoId, path }) =>
      api.delete<void>(
        routes.files.deleteEntry(workspaceId, repoId, encodeRepoPath(path)),
      ),
    onSuccess: (_data, variables) => {
      queryClient.invalidateQueries({
        queryKey: ["file-tree", variables.workspaceId, variables.repoId],
      });
      queryClient.invalidateQueries({
        queryKey: ["file-content", variables.workspaceId, variables.repoId],
      });
    },
  });
  return toMutationHook(mutation);
}

export function useWriteFileEntry(): MutationHook<
  { workspaceId: string; repoId: string; path: string; content?: string },
  void
> {
  const queryClient = useQueryClient();
  const mutation = useMutation<
    void,
    Error,
    { workspaceId: string; repoId: string; path: string; content?: string }
  >({
    mutationFn: ({ workspaceId, repoId, path, content }) =>
      api.put<void>(
        routes.files.writeEntry(workspaceId, repoId, encodeRepoPath(path)),
        {
          content: content ?? "",
        },
      ),
    onSuccess: (_data, variables) => {
      queryClient.invalidateQueries({
        queryKey: ["file-tree", variables.workspaceId, variables.repoId],
      });
      queryClient.invalidateQueries({
        queryKey: ["file-content", variables.workspaceId, variables.repoId],
      });
    },
  });
  return toMutationHook(mutation);
}
