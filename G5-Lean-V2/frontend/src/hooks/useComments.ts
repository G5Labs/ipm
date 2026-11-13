// =============================================================================
// G5 Client — Comment Queries & Mutations
// =============================================================================

import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import type { QueryClient } from "@tanstack/react-query";
import { api } from "../lib/api";
import { invalidateAndReadAgain } from "../lib/readAgain";
import type {
  CommentDTO,
  CommentReplyDTO,
  CreateCommentInput,
  MutationHook,
} from "../lib/types";
import {
  toMutationHook,
  toRefreshingState,
  type RefreshingState,
} from "./hookUtils";

/** Every cached list of anchored threads: file threads and legacy node threads. */
const COMMENT_LIST_ROOTS = ["comments", "line-comments"] as const;

/** Every list is read again after a change, a first read still out included. */
function invalidateCommentLists(queryClient: QueryClient) {
  for (const root of [
    ...COMMENT_LIST_ROOTS,
    "workspace-review-comments",
    "workspace-review-replies",
    "workspace-activity",
  ]) {
    void invalidateAndReadAgain(queryClient, [root]);
  }
}

/**
 * The threads of one file, identified by workspace, repository and path.
 * Every surface showing that file lists these same threads; the branch is a
 * label and never selects or hides one (`g5.documents.comments.identity`).
 */
export function useFileComments(
  workspaceId: string,
  repoId: string,
  filePath: string,
): RefreshingState<CommentDTO[]> {
  const query = useQuery<CommentDTO[], Error>({
    queryKey: ["line-comments", workspaceId, repoId, filePath],
    queryFn: ({ signal }) =>
      api.get<CommentDTO[]>(`/api/v1/workspaces/${workspaceId}/comments`, {
        signal,
        params: { repoId, filePath },
      }),
    enabled: !!workspaceId && !!repoId && !!filePath,
  });
  return toRefreshingState(query);
}

export function useCommentReplies(
  commentId: string,
  enabled = true,
): RefreshingState<CommentReplyDTO[]> {
  const query = useQuery<CommentReplyDTO[], Error>({
    queryKey: ["comment-replies", commentId],
    queryFn: ({ signal }) =>
      api.get<CommentReplyDTO[]>(`/api/v1/comments/${commentId}/replies`, {
        signal,
      }),
    enabled: !!commentId && enabled,
  });
  return toRefreshingState(query);
}

// ---------------------------------------------------------------------------
// Mutation Hooks
// ---------------------------------------------------------------------------

/** Start a thread on a line of a file. Callers use `mutateAsync` to learn why it failed. */
export function useAddComment(): MutationHook<
  CreateCommentInput & { workspaceId: string; principal?: string | null },
  CommentDTO
> {
  const queryClient = useQueryClient();
  const mutation = useMutation<
    CommentDTO,
    Error,
    CreateCommentInput & { workspaceId: string; principal?: string | null }
  >({
    mutationFn: ({ workspaceId, principal, ...input }) =>
      api.post<CommentDTO>(
        `/api/v1/workspaces/${workspaceId}/comments`,
        input,
        principal ? { principal } : undefined,
      ),
    onSuccess: (created, { workspaceId, repoId, filePath }) => {
      queryClient.setQueryData<CommentDTO[]>(
        ["line-comments", workspaceId, repoId, filePath],
        (old) =>
          old && !old.some((comment) => comment.id === created.id)
            ? [created, ...old]
            : old,
      );
    },
    onSettled: () => invalidateCommentLists(queryClient),
  });
  return toMutationHook(mutation);
}

export function useReplyToComment(): MutationHook<
  {
    commentId: string;
    content: string;
    mentionUserIds?: string[];
    principal?: string | null;
  },
  CommentReplyDTO
> {
  const queryClient = useQueryClient();
  const mutation = useMutation<
    CommentReplyDTO,
    Error,
    {
      commentId: string;
      content: string;
      mentionUserIds?: string[];
      principal?: string | null;
    }
  >({
    mutationFn: ({ commentId, content, mentionUserIds, principal }) =>
      api.post<CommentReplyDTO>(
        `/api/v1/comments/${commentId}/replies`,
        { content, mentionUserIds },
        principal ? { principal } : undefined,
      ),
    onSettled: (_data, _error, vars) => {
      invalidateCommentLists(queryClient);
      void invalidateAndReadAgain(queryClient, [
        "comment-replies",
        vars.commentId,
      ]);
    },
  });
  return toMutationHook(mutation);
}

/** Apply `change` to the thread in every cached list that holds it. */
function updateCommentLists(
  queryClient: QueryClient,
  change: (comments: CommentDTO[]) => CommentDTO[],
) {
  for (const root of COMMENT_LIST_ROOTS) {
    for (const [key, data] of queryClient.getQueriesData<CommentDTO[]>({
      queryKey: [root],
    })) {
      if (data) queryClient.setQueryData<CommentDTO[]>(key, change(data));
    }
  }
}

/** A thread action, sent only for `principal` when given (`g5.security.login.principal`). */
export interface CommentThreadAction {
  commentId: string;
  principal?: string | null;
}

/**
 * Resolve (or reopen) a thread. The lists change once the server accepts:
 * a refused action leaves the thread where it was, still showing why and
 * keeping any reply being typed (`g5.documents.comments.failure`).
 */
export function useResolveComment(
  resolved = true,
): MutationHook<CommentThreadAction, void> {
  const queryClient = useQueryClient();
  const mutation = useMutation<void, Error, CommentThreadAction>({
    mutationFn: async ({ commentId, principal }) => {
      await api.patch<unknown>(
        `/api/v1/comments/${commentId}`,
        { resolved },
        principal ? { principal } : undefined,
      );
    },
    onSuccess: (_data, { commentId }) => {
      updateCommentLists(queryClient, (comments) =>
        comments.map((comment) =>
          comment.id === commentId
            ? {
                ...comment,
                resolved,
                resolvedAt: resolved ? new Date().toISOString() : null,
              }
            : comment,
        ),
      );
    },
    onSettled: () => invalidateCommentLists(queryClient),
  });
  return toMutationHook(mutation);
}

/** Delete a thread; like resolve, the lists change once the server accepts. */
export function useDeleteComment(): MutationHook<CommentThreadAction, void> {
  const queryClient = useQueryClient();
  const mutation = useMutation<void, Error, CommentThreadAction>({
    mutationFn: ({ commentId, principal }) =>
      api.delete<void>(
        `/api/v1/comments/${commentId}`,
        principal ? { principal } : undefined,
      ),
    onSuccess: (_data, { commentId }) => {
      updateCommentLists(queryClient, (comments) =>
        comments.filter((comment) => comment.id !== commentId),
      );
      queryClient.removeQueries({
        queryKey: ["comment-replies", commentId],
        exact: true,
      });
    },
    onSettled: () => invalidateCommentLists(queryClient),
  });
  return toMutationHook(mutation);
}
