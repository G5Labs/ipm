// Referenced question sets: reads and the answer write
// (`g5.documents.question.referenced`, `.answer-checkpoint`).
//
// One query per set path (`["question-set", workspaceId, path]`). The live
// owner's views refresh through `workspace.file_changed` for the docs
// repository and readers' through the durable-cut invalidation (lib/ws.ts);
// there is no question event. An answer updates the cached set in place so
// every card showing the entry re-renders at once.

import { useQueries, useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback } from "react";
import { armOpenQuestionMetricRefresh } from "../lib/openQuestionMetricRefresh";
import {
  answerQuestion,
  castQuestionVote,
  clearQuestion,
  fetchQuestionSet,
  fetchQuestionVotes,
  questionSetQueryKey,
  questionVotesQueryKey,
  withdrawQuestionVote,
  type AnswerOutcome,
  type AnswerRequest,
  type QuestionEntry,
  type QuestionSetView,
  type QuestionVote,
  type QuestionVotesView,
  type VoteRequest,
} from "../lib/questionSet";

function questionSetQueryOptions(workspaceId: string | null, path: string) {
  return {
    queryKey: questionSetQueryKey(workspaceId ?? "", path),
    queryFn: () => fetchQuestionSet(workspaceId ?? "", path),
    enabled: Boolean(workspaceId) && Boolean(path),
    staleTime: 10_000,
    retry: false,
  };
}

export function useQuestionSet(
  workspaceId: string | null,
  path: string | null,
) {
  return useQuery<QuestionSetView, Error>(
    questionSetQueryOptions(workspaceId, path ?? ""),
  );
}

/** The sets of several paths, in input order. */
export function useQuestionSetsByPath(
  workspaceId: string | null,
  paths: readonly string[],
) {
  return useQueries({
    queries: paths.map((path) => questionSetQueryOptions(workspaceId, path)),
  });
}

/** Replace one entry of a cached set (or install the refusal's current entry). */
export function withEntry(
  set: QuestionSetView | undefined,
  entry: QuestionEntry,
  hash?: string,
): QuestionSetView | undefined {
  if (!set) return set;
  return {
    ...set,
    hash: hash ?? set.hash,
    questions: set.questions.map((question) =>
      question.id === entry.id ? entry : question,
    ),
  };
}

/** Answer or clear one referenced question and update every card at once. */
export function useQuestionWrite(workspaceId: string | null) {
  const queryClient = useQueryClient();
  const install = useCallback(
    (path: string, entry: QuestionEntry, hash?: string) => {
      if (!workspaceId) return;
      queryClient.setQueryData<QuestionSetView>(
        questionSetQueryKey(workspaceId, path),
        (set) => withEntry(set, entry, hash),
      );
    },
    [queryClient, workspaceId],
  );
  const write = useCallback(
    async (
      request: AnswerRequest,
      mode: "answer" | "clear",
    ): Promise<AnswerOutcome> => {
      if (!workspaceId) throw new Error("no workspace");
      const outcome =
        mode === "answer"
          ? await answerQuestion(workspaceId, request)
          : await clearQuestion(workspaceId, {
              path: request.path,
              questionId: request.questionId,
              expectedRevision: request.expectedRevision,
              idempotencyKey: request.idempotencyKey,
              appliedHash: request.appliedHash,
            });
      install(outcome.path, outcome.entry, outcome.hash);
      armOpenQuestionMetricRefresh(queryClient, workspaceId);
      void queryClient.invalidateQueries({
        queryKey: ["workspace-open-question-metrics"],
      });
      return outcome;
    },
    [install, queryClient, workspaceId],
  );
  const refresh = useCallback(
    (path: string) => {
      if (!workspaceId) return;
      void queryClient.invalidateQueries({
        queryKey: questionSetQueryKey(workspaceId, path),
      });
    },
    [queryClient, workspaceId],
  );
  return { write, install, refresh };
}

// ---------------------------------------------------------------------------
// Votes (`g5.documents.question.votes`): one query per set path, refreshed by
// the `question.vote` event (lib/ws.ts); a cast or withdrawal updates the
// cached view in place so every card showing the question re-renders at once.
// ---------------------------------------------------------------------------

function questionVotesQueryOptions(workspaceId: string | null, path: string) {
  return {
    queryKey: questionVotesQueryKey(workspaceId ?? "", path),
    queryFn: () => fetchQuestionVotes(workspaceId ?? "", path),
    enabled: Boolean(workspaceId) && Boolean(path),
    staleTime: 10_000,
    retry: false,
  };
}

export function useQuestionVotes(
  workspaceId: string | null,
  path: string | null,
) {
  return useQuery<QuestionVotesView, Error>(
    questionVotesQueryOptions(workspaceId, path ?? ""),
  );
}

/** The votes of several set paths, in input order. */
export function useQuestionVotesByPath(
  workspaceId: string | null,
  paths: readonly string[],
) {
  return useQueries({
    queries: paths.map((path) => questionVotesQueryOptions(workspaceId, path)),
  });
}

/** Replace the viewer's vote on one question in a cached view, or drop it. */
export function withVote(
  view: QuestionVotesView | undefined,
  questionId: string,
  vote: QuestionVote | null,
): QuestionVotesView | undefined {
  if (!view) return view;
  const others = view.votes.filter(
    (entry) =>
      !(entry.questionId === questionId && entry.userId === view.viewerId),
  );
  return { ...view, votes: vote ? [...others, vote] : others };
}

/** Cast, change or withdraw the signed-in person's vote. */
export function useQuestionVoteWrite(workspaceId: string | null) {
  const queryClient = useQueryClient();
  const settle = useCallback(
    (path: string, questionId: string, vote: QuestionVote | null) => {
      if (!workspaceId) return;
      const key = questionVotesQueryKey(workspaceId, path);
      queryClient.setQueryData<QuestionVotesView>(key, (view) =>
        withVote(view, questionId, vote),
      );
      void queryClient.invalidateQueries({ queryKey: key });
    },
    [queryClient, workspaceId],
  );
  const vote = useCallback(
    async (request: VoteRequest): Promise<QuestionVote> => {
      if (!workspaceId) throw new Error("no workspace");
      const outcome = await castQuestionVote(workspaceId, request);
      settle(outcome.path, request.questionId, outcome.vote);
      return outcome.vote;
    },
    [settle, workspaceId],
  );
  const withdraw = useCallback(
    async (path: string, questionId: string): Promise<boolean> => {
      if (!workspaceId) throw new Error("no workspace");
      const outcome = await withdrawQuestionVote(workspaceId, {
        path,
        questionId,
      });
      settle(path, questionId, null);
      return outcome.withdrawn;
    },
    [settle, workspaceId],
  );
  return { vote, withdraw };
}
