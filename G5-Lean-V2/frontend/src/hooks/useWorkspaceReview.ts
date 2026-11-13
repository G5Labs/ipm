// =============================================================================
// G5 Client — Review workspace queries & mutations
// =============================================================================
// A reviewer who does not own a workspace cannot edit it, run its VM or send
// chat in it (single-writer rule). The Approvals page's Review Workspace
// action forks the source into a reviewer-owned review workspace and reopens
// that same workspace on later reviews. Push review turns the review
// workspace's committed changes into suggestions the owner accepts into the
// running source workspace one by one; comments need no fork at all.
// =============================================================================

import { useCallback, useMemo, useSyncExternalStore } from "react";
import {
  useMutation,
  useQuery,
  useQueryClient,
  type QueryClient,
} from "@tanstack/react-query";
import { api } from "../lib/api";
import { routes } from "../lib/routes";
import type {
  DiscardReviewChangesInput,
  DiscardReviewChangesResponse,
  AsyncState,
  MutationHook,
  PublishWorkspaceReviewInput,
  PublishWorkspaceReviewResponse,
  PushFileSuggestionsInput,
  PushFileSuggestionsResponse,
  ReviewPublicationsResponse,
  ReviewSuggestionCountsResponse,
  ReviewSuggestionFilter,
  ReviewSuggestionReply,
  ReviewSuggestion,
  ReviewSuggestionResult,
  ReviewSuggestionsResponse,
  ReviewPreview,
  StartWorkspaceReviewResponse,
  WorkspaceReviewContext,
} from "../lib/types";
import {
  toAsyncState,
  toMutationHook,
  toRefreshingState,
  type RefreshingState,
} from "./hookUtils";
import { isLaterSuggestion } from "../lib/suggestionRecords";
import { invalidateAndReadAgain } from "../lib/readAgain";
import { expandReviewSuggestionEdits } from "../lib/reviewSuggestionEdits";

export const WORKSPACE_REVIEW_QUERY_KEY = "workspace-review";
export const REVIEW_PUBLICATIONS_QUERY_KEY = "workspace-review-publications";
export const REVIEW_SUGGESTIONS_QUERY_KEY = "workspace-review-suggestions";
export const REVIEW_SUGGESTION_COUNTS_QUERY_KEY =
  "workspace-review-suggestion-counts";
export const REVIEW_REPLIES_QUERY_KEY = "workspace-review-replies";

/**
 * The review context of `workspaceId`: the caller's review of it (when it is
 * a source) and its review origin (when it is a review workspace). Polled
 * every 5 s so the VM status pill on the Approvals card stays current.
 */
export function useWorkspaceReviewContext(
  workspaceId: string,
  enabled = true,
): AsyncState<WorkspaceReviewContext> {
  const query = useQuery<WorkspaceReviewContext, Error>({
    queryKey: [WORKSPACE_REVIEW_QUERY_KEY, workspaceId],
    queryFn: ({ signal }) =>
      api.get<WorkspaceReviewContext>(
        routes.workspaceReview.context(workspaceId),
        {
          signal,
        },
      ),
    enabled: enabled && !!workspaceId,
    // Poll only where the context is live: the Approvals card (VM status of
    // the caller's review) and the review workspace page. An ordinary
    // workspace with no review context is read once.
    refetchInterval: (query) =>
      query.state.data?.asSource || query.state.data?.asReview ? 5_000 : false,
  });
  return toAsyncState(query);
}

function invalidateReviewCaches(
  queryClient: ReturnType<typeof useQueryClient>,
  workspaceIds: string[],
) {
  queryClient.invalidateQueries({ queryKey: [WORKSPACE_REVIEW_QUERY_KEY] });
  queryClient.invalidateQueries({ queryKey: ["workspaces"] });
  for (const id of workspaceIds) {
    queryClient.invalidateQueries({ queryKey: ["workspace", id] });
    queryClient.invalidateQueries({ queryKey: ["workspace-vm", id] });
  }
}

function invalidateSuggestionCaches(
  queryClient: ReturnType<typeof useQueryClient>,
  sourceWorkspaceId: string,
) {
  queryClient.invalidateQueries({
    queryKey: [REVIEW_PUBLICATIONS_QUERY_KEY, sourceWorkspaceId],
  });
  queryClient.invalidateQueries({
    queryKey: [REVIEW_SUGGESTIONS_QUERY_KEY, sourceWorkspaceId],
  });
  queryClient.invalidateQueries({
    queryKey: [REVIEW_SUGGESTION_COUNTS_QUERY_KEY, sourceWorkspaceId],
  });
  queryClient.invalidateQueries({
    queryKey: [WORKSPACE_REVIEW_QUERY_KEY, sourceWorkspaceId],
  });
}

/**
 * The latest decided view of each suggestion this client has read, from any
 * list or decision answer. Every suggestion list is read through it, so once
 * a decision is known no list shows the suggestion undecided: not a list read
 * before the decision and answered after it, not one another view has not
 * read again, not one whose refresh fails
 * (`g5.governance.review-suggestions.display`). A status leaves open only
 * once, so a decided view never goes stale.
 */
interface DecidedSuggestions {
  /** Replaced, never changed, so each version is its own snapshot. */
  views: ReadonlyMap<string, ReviewSuggestion>;
  listeners: Set<() => void>;
}

const decidedByClient = new WeakMap<QueryClient, DecidedSuggestions>();

function decidedSuggestions(queryClient: QueryClient): DecidedSuggestions {
  let decided = decidedByClient.get(queryClient);
  if (!decided) {
    decided = { views: new Map(), listeners: new Set() };
    decidedByClient.set(queryClient, decided);
  }
  return decided;
}

function decidedKey(sourceWorkspaceId: string, suggestionId: string): string {
  return `${sourceWorkspaceId}/${suggestionId}`;
}

function sameDecision(a: ReviewSuggestion, b: ReviewSuggestion): boolean {
  return a.status === b.status && a.statusAt === b.statusAt;
}

/** Notes the decided views among `suggestions`, keeping the later of two. */
function noteDecidedSuggestions(
  queryClient: QueryClient,
  sourceWorkspaceId: string,
  suggestions: readonly ReviewSuggestion[],
) {
  const decided = decidedSuggestions(queryClient);
  let views: Map<string, ReviewSuggestion> | null = null;
  for (const suggestion of expandReviewSuggestionEdits(suggestions)) {
    if (suggestion.status === "open") continue;
    const key = decidedKey(sourceWorkspaceId, suggestion.id);
    const held = (views ?? decided.views).get(key);
    if (
      held &&
      (sameDecision(held, suggestion) || !isLaterSuggestion(held, suggestion))
    ) {
      continue;
    }
    views ??= new Map(decided.views);
    views.set(key, suggestion);
  }
  if (!views) return;
  decided.views = views;
  for (const listener of decided.listeners) listener();
}

/**
 * `data` with each row at the latest decided view known of it. A list
 * narrowed to open rows, or to another status, drops a row decided since.
 */
function withDecidedSuggestions(
  views: ReadonlyMap<string, ReviewSuggestion>,
  sourceWorkspaceId: string,
  filter: ReviewSuggestionFilter,
  data: ReviewSuggestionsResponse,
): ReviewSuggestionsResponse {
  if (!data?.suggestions) return data;
  data = {
    ...data,
    suggestions: expandReviewSuggestionEdits(data.suggestions).filter(
      (row) =>
        (!filter.openOnly || row.status === "open") &&
        (!filter.status || row.status === filter.status),
    ),
  };
  if (views.size === 0) return data;
  let changed = false;
  const shown: ReviewSuggestion[] = [];
  for (const row of data.suggestions) {
    const known = views.get(decidedKey(sourceWorkspaceId, row.id));
    if (!known || sameDecision(row, known) || !isLaterSuggestion(row, known)) {
      shown.push(row);
      continue;
    }
    changed = true;
    const latest = { ...row, ...known };
    if (filter.openOnly && latest.status !== "open") continue;
    if (filter.status && latest.status !== filter.status) continue;
    shown.push(latest);
  }
  return changed ? { ...data, suggestions: shown } : data;
}

/** Start (or reopen) the caller's review of `sourceWorkspaceId`. */
export function useStartWorkspaceReview(
  sourceWorkspaceId: string,
): MutationHook<void, StartWorkspaceReviewResponse> {
  const queryClient = useQueryClient();
  const mutation = useMutation<StartWorkspaceReviewResponse, Error, void>({
    mutationFn: () =>
      api.post<StartWorkspaceReviewResponse>(
        routes.workspaceReview.start(sourceWorkspaceId),
        {},
      ),
    onSuccess: (result) => {
      invalidateReviewCaches(queryClient, [
        sourceWorkspaceId,
        result.review.reviewWorkspaceId,
      ]);
    },
  });
  return toMutationHook(mutation);
}

/**
 * Push the review whose review workspace is `reviewWorkspaceId`: the
 * committed difference between the review branches and the source branches
 * becomes one publication of suggestions on the source workspace.
 */
export function usePublishWorkspaceReview(
  reviewWorkspaceId: string,
): MutationHook<PublishWorkspaceReviewInput, PublishWorkspaceReviewResponse> {
  const queryClient = useQueryClient();
  const mutation = useMutation<
    PublishWorkspaceReviewResponse,
    Error,
    PublishWorkspaceReviewInput
  >({
    mutationFn: (input) =>
      api.post<PublishWorkspaceReviewResponse>(
        routes.workspaceReview.publish(reviewWorkspaceId),
        input,
      ),
    onSuccess: (result, input) => {
      invalidateReviewCaches(queryClient, [
        reviewWorkspaceId,
        result.publication.sourceWorkspaceId,
      ]);
      invalidateSuggestionCaches(
        queryClient,
        result.publication.sourceWorkspaceId,
      );
      // The reviewer stays on the copy's Review tab: its preview must say
      // what the round now carries (chips, carried notes), so it is taken
      // again. A push is the reviewer's own action, so the save it implies
      // is wanted.
      queryClient.invalidateQueries({
        queryKey: ["workspace-review-preview", reviewWorkspaceId],
      });
      // An approving round also records the reviewer's approval on the
      // source: its Approvals page reads the synced decision.
      if (input.verdict === "approve") {
        queryClient.invalidateQueries({
          queryKey: [
            "workspace-approvals",
            result.publication.sourceWorkspaceId,
          ],
        });
      }
    },
  });
  return toMutationHook(mutation);
}

/**
 * Push a reader's edits to one file as review suggestions on the source
 * workspace (`g5.governance.file-suggestions`): no review workspace or VM.
 * The source's suggestion lists, counts and publications are read again so
 * the new cards appear.
 */
export function usePushFileSuggestions(
  sourceWorkspaceId: string,
): MutationHook<PushFileSuggestionsInput, PushFileSuggestionsResponse> {
  const queryClient = useQueryClient();
  const mutation = useMutation<
    PushFileSuggestionsResponse,
    Error,
    PushFileSuggestionsInput
  >({
    mutationFn: ({ principal, ...body }) =>
      api.post<PushFileSuggestionsResponse>(
        routes.workspaceReview.fileSuggestions(sourceWorkspaceId),
        body,
        principal ? { principal } : undefined,
      ),
    onSuccess: () => {
      invalidateSuggestionCaches(queryClient, sourceWorkspaceId);
    },
  });
  return toMutationHook(mutation);
}

/**
 * Take previewed changes out of the copy's working tree: a rejected
 * proposal stays a difference against the source until the reviewer drops
 * it. Nothing on the source changes. The preview is refetched afterwards
 * (an explicit action of the reviewer, so its save is wanted), and the
 * copy's file views follow.
 */
export function useDiscardReviewChanges(
  reviewWorkspaceId: string,
  sourceWorkspaceId: string,
): MutationHook<DiscardReviewChangesInput, DiscardReviewChangesResponse> {
  const queryClient = useQueryClient();
  const mutation = useMutation<
    DiscardReviewChangesResponse,
    Error,
    DiscardReviewChangesInput
  >({
    mutationFn: (input) =>
      api.post<DiscardReviewChangesResponse>(
        routes.workspaceReview.discardChanges(reviewWorkspaceId),
        input,
      ),
    onSettled: () => {
      queryClient.invalidateQueries({
        queryKey: ["workspace-review-preview", reviewWorkspaceId],
      });
      invalidateSuggestionCaches(queryClient, sourceWorkspaceId);
      queryClient.invalidateQueries({
        queryKey: ["workspace-changes", reviewWorkspaceId],
      });
      queryClient.invalidateQueries({ queryKey: ["file-tree"] });
      queryClient.invalidateQueries({
        queryKey: ["workspace-file", reviewWorkspaceId],
      });
    },
  });
  return toMutationHook(mutation);
}

/** Every publication (all reviewers, all rounds) on `sourceWorkspaceId`. */
export function useReviewPublications(
  sourceWorkspaceId: string,
  enabled = true,
): RefreshingState<ReviewPublicationsResponse> {
  const query = useQuery<ReviewPublicationsResponse, Error>({
    queryKey: [REVIEW_PUBLICATIONS_QUERY_KEY, sourceWorkspaceId],
    queryFn: ({ signal }) =>
      api.get<ReviewPublicationsResponse>(
        routes.workspaceReview.publications(sourceWorkspaceId),
        { signal },
      ),
    enabled: enabled && !!sourceWorkspaceId,
    refetchInterval: 15_000,
  });
  return toRefreshingState(query);
}

function suggestionQuery(filter: ReviewSuggestionFilter): string {
  const params = new URLSearchParams();
  if (filter.publicationId) params.set("publicationId", filter.publicationId);
  if (filter.findingId) params.set("findingId", filter.findingId);
  if (filter.repoId) params.set("repoId", filter.repoId);
  if (filter.path) params.set("path", filter.path);
  if (filter.openOnly) params.set("openOnly", "true");
  if (filter.currentOnly) params.set("currentOnly", "true");
  if (filter.kind) params.set("kind", filter.kind);
  if (filter.status) params.set("status", filter.status);
  const encoded = params.toString();
  return encoded ? `?${encoded}` : "";
}

/** Suggestions on `sourceWorkspaceId`, optionally narrowed by `filter`. */
export function useReviewSuggestions(
  sourceWorkspaceId: string,
  filter: ReviewSuggestionFilter = {},
  enabled = true,
): RefreshingState<ReviewSuggestionsResponse> {
  const queryClient = useQueryClient();
  const decided = decidedSuggestions(queryClient);
  const subscribe = useCallback(
    (onChange: () => void) => {
      decided.listeners.add(onChange);
      return () => {
        decided.listeners.delete(onChange);
      };
    },
    [decided],
  );
  const views = useSyncExternalStore(subscribe, () => decided.views);
  // The filter by value: callers pass it inline.
  const filterKey = JSON.stringify(filter);
  const select = useMemo(() => {
    const shownFilter = JSON.parse(filterKey) as ReviewSuggestionFilter;
    return (data: ReviewSuggestionsResponse) =>
      withDecidedSuggestions(views, sourceWorkspaceId, shownFilter, data);
  }, [views, sourceWorkspaceId, filterKey]);
  const query = useQuery<ReviewSuggestionsResponse, Error>({
    queryKey: [REVIEW_SUGGESTIONS_QUERY_KEY, sourceWorkspaceId, filter],
    queryFn: async ({ signal }) => {
      const data = await api.get<ReviewSuggestionsResponse>(
        routes.workspaceReview.suggestions(sourceWorkspaceId) +
          suggestionQuery(filter),
        { signal },
      );
      noteDecidedSuggestions(
        queryClient,
        sourceWorkspaceId,
        data?.suggestions ?? [],
      );
      return data;
    },
    select,
    enabled: enabled && !!sourceWorkspaceId,
    refetchInterval: 15_000,
  });
  // toRefreshingState reads only these fields. React Query's result object
  // changes on parent renders; the file consumer needs a stable shown state.
  const { isPending, isError, data, error } = query;
  return useMemo(
    () =>
      toRefreshingState<ReviewSuggestionsResponse>({
        isPending,
        isError,
        data,
        error,
      } as typeof query),
    [isPending, isError, data, error],
  );
}

/** Open suggestion counts per (repo, path) for file tree badges. */
export function useReviewSuggestionCounts(
  sourceWorkspaceId: string,
  enabled = true,
): AsyncState<ReviewSuggestionCountsResponse> {
  const query = useQuery<ReviewSuggestionCountsResponse, Error>({
    queryKey: [REVIEW_SUGGESTION_COUNTS_QUERY_KEY, sourceWorkspaceId],
    queryFn: ({ signal }) =>
      api.get<ReviewSuggestionCountsResponse>(
        routes.workspaceReview.suggestionCounts(sourceWorkspaceId),
        { signal },
      ),
    enabled: enabled && !!sourceWorkspaceId,
    refetchInterval: 30_000,
  });
  return toAsyncState(query);
}

export interface SuggestionActionInput {
  suggestionId: string;
  /** A durable edit of suggestionId; omitted actions target remaining edits. */
  editId?: string;
  /** Accept: the owner's edited replacement lines (defaults to the proposal). */
  afterLines?: string;
  /** Reject: why. */
  reason?: string;
  /** Sent only for this principal when given (`g5.security.login.principal`). */
  principal?: string | null;
}

function useSuggestionAction(
  sourceWorkspaceId: string,
  action: "accept" | "reject" | "resolve" | "withdraw",
): MutationHook<SuggestionActionInput, ReviewSuggestionResult> {
  const queryClient = useQueryClient();
  const mutation = useMutation<
    ReviewSuggestionResult,
    Error,
    SuggestionActionInput
  >({
    mutationFn: ({ suggestionId, editId, afterLines, reason, principal }) =>
      api.post<ReviewSuggestionResult>(
        routes.workspaceReview.suggestionAction(
          sourceWorkspaceId,
          suggestionId,
          action,
        ),
        {
          ...(action !== "withdraw" && editId ? { editId } : {}),
          ...(action === "accept" && afterLines !== undefined
            ? { afterLines }
            : {}),
          ...(action === "reject" && reason !== undefined ? { reason } : {}),
        },
        principal ? { principal } : undefined,
      ),
    onSuccess: (result, variables) => {
      if (result?.suggestion) {
        noteDecidedSuggestions(queryClient, sourceWorkspaceId, [
          result.suggestion,
        ]);
      }
      invalidateSuggestionCaches(queryClient, sourceWorkspaceId);
      // The decision's note is read after it, a first read still out
      // included (`g5.governance.review-thread`).
      void invalidateAndReadAgain(queryClient, [
        REVIEW_REPLIES_QUERY_KEY,
        sourceWorkspaceId,
        variables.suggestionId,
        ...(action !== "withdraw" && variables.editId
          ? [variables.editId]
          : []),
      ]);
      if (action === "accept") {
        // Reload the accepted file, its document hash, and the branch updated
        // by the automatic commit and push.
        queryClient.invalidateQueries({ queryKey: ["file-tree"] });
        queryClient.invalidateQueries({
          queryKey: ["workspace-file", sourceWorkspaceId],
        });
        queryClient.invalidateQueries({
          queryKey: ["workspace-changes", sourceWorkspaceId],
        });
        queryClient.invalidateQueries({
          queryKey: ["workspace", sourceWorkspaceId],
        });
      }
    },
  });
  return toMutationHook(mutation);
}

/** Owner: write a suggestion into the running source workspace. */
export function useAcceptReviewSuggestion(sourceWorkspaceId: string) {
  return useSuggestionAction(sourceWorkspaceId, "accept");
}

/** Owner: decline a suggestion, optionally saying why. */
export function useRejectReviewSuggestion(sourceWorkspaceId: string) {
  return useSuggestionAction(sourceWorkspaceId, "reject");
}

/** Owner: mark a comment suggestion as handled. */
export function useResolveReviewSuggestion(sourceWorkspaceId: string) {
  return useSuggestionAction(sourceWorkspaceId, "resolve");
}

/** Reviewer: take back a parent suggestion's remaining open edits. */
export function useWithdrawReviewSuggestion(sourceWorkspaceId: string) {
  return useSuggestionAction(sourceWorkspaceId, "withdraw");
}

/** The discussion thread under one suggestion. */
export function useReviewSuggestionReplies(
  sourceWorkspaceId: string,
  suggestionId: string,
  enabled = true,
  editId?: string,
): RefreshingState<{ replies: ReviewSuggestionReply[] }> {
  const query = useQuery<{ replies: ReviewSuggestionReply[] }, Error>({
    queryKey: [
      REVIEW_REPLIES_QUERY_KEY,
      sourceWorkspaceId,
      suggestionId,
      ...(editId ? [editId] : []),
    ],
    queryFn: ({ signal }) =>
      api.get<{ replies: ReviewSuggestionReply[] }>(
        routes.workspaceReview.suggestionReplies(
          sourceWorkspaceId,
          suggestionId,
        ) + (editId ? `?editId=${encodeURIComponent(editId)}` : ""),
        { signal },
      ),
    enabled: enabled && !!sourceWorkspaceId && !!suggestionId,
    // An open thread is a conversation: the other side's replies and the
    // owner's decision notes arrive without closing and reopening it.
    refetchInterval: 10_000,
  });
  return toRefreshingState(query);
}

/** Reply under a suggestion (any reader of the source workspace). */
export function useAddReviewSuggestionReply(
  sourceWorkspaceId: string,
): MutationHook<
  {
    suggestionId: string;
    editId?: string;
    content: string;
    principal?: string | null;
  },
  { replies: ReviewSuggestionReply[] }
> {
  const queryClient = useQueryClient();
  const mutation = useMutation<
    { replies: ReviewSuggestionReply[] },
    Error,
    {
      suggestionId: string;
      editId?: string;
      content: string;
      principal?: string | null;
    }
  >({
    mutationFn: ({ suggestionId, editId, content, principal }) =>
      api.post<{ replies: ReviewSuggestionReply[] }>(
        routes.workspaceReview.suggestionReplies(
          sourceWorkspaceId,
          suggestionId,
        ),
        { content, ...(editId ? { editId } : {}) },
        principal ? { principal } : undefined,
      ),
    onSuccess: (_result, variables) => {
      // Read after the reply, a first read still out included.
      void invalidateAndReadAgain(queryClient, [
        REVIEW_REPLIES_QUERY_KEY,
        sourceWorkspaceId,
        variables.suggestionId,
        ...(variables.editId ? [variables.editId] : []),
      ]);
    },
  });
  return toMutationHook(mutation);
}

/**
 * What Push review would publish from `reviewWorkspaceId`, without
 * publishing: the Push dialog lists the changed files and lets the reviewer
 * tick which new files should enter the repository.
 */
export function useReviewPreview(
  reviewWorkspaceId: string,
  enabled = true,
): AsyncState<ReviewPreview> & {
  retry: () => void;
  /**
   * A save and preview of this caller's own: cancels any request already in
   * flight (one another consumer started earlier, which may have saved the
   * tree before the latest edits) and starts a new one. Resolves with the
   * fresh preview, or undefined when it failed or was cancelled in turn.
   */
  refresh: () => Promise<ReviewPreview | undefined>;
  /** A save and preview is in flight (the first one or a refetch). */
  fetching: boolean;
  /** When the data shown was fetched (ms since the epoch; 0 for none). */
  updatedAt: number;
} {
  const query = useQuery<ReviewPreview, Error>({
    queryKey: ["workspace-review-preview", reviewWorkspaceId],
    queryFn: ({ signal }) =>
      api.post<ReviewPreview>(
        routes.workspaceReview.preview(reviewWorkspaceId),
        {},
        {
          signal,
        },
      ),
    enabled: enabled && !!reviewWorkspaceId,
    staleTime: 0,
    gcTime: 0,
  });
  const queryClient = useQueryClient();
  const refetch = query.refetch;
  const retry = useCallback(() => {
    void refetch();
  }, [refetch]);
  const refresh = useCallback(async () => {
    // `cancelRefetch` only cancels a request in flight when the query
    // already holds data; the very first preview (another consumer's,
    // possibly from before the latest edits) has to be cancelled by hand
    // so this caller's request is its own.
    await queryClient.cancelQueries({
      queryKey: ["workspace-review-preview", reviewWorkspaceId],
    });
    const result = await refetch({ cancelRefetch: true });
    return result.data;
  }, [queryClient, refetch, reviewWorkspaceId]);
  return {
    ...toAsyncState(query),
    retry,
    refresh,
    fetching: query.isFetching,
    updatedAt: query.dataUpdatedAt,
  };
}
