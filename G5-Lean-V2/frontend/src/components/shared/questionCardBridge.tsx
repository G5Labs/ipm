// =============================================================================
// Host side of explainer question cards (`g5.documents.question.html`)
// =============================================================================
// Supplies each `data-g5-question` placeholder's card data (the entry and its
// vote tally) to the sandboxed frame and validates each frame proposal: the
// host accepts one only from this frame's window, for this frame load's
// generation, for a reference the document is known to hold, within the size
// bound, for the entry's current revision and with option ids the entry
// offers. Everything else is dropped and saves nothing. An answer needs the
// attached owner and is saved only after a person confirms it in this trusted
// bar; a frame message is never a human answer. A validated vote message
// (`g5.documents.question.votes`, open to every reader) is applied at once:
// a vote is attributed, visible, withdrawable and never a decision. The bar
// confirms answers and surfaces refused votes with their reason and a retry.
// =============================================================================

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { randomUuid } from "../../lib/randomUuid";
import { classNames } from "../../lib/utils";
import {
  answerRefusal,
  extractQuestionPlaceholders,
  findEntry,
  formatQuestionPlaceholder,
  tallyVotes,
  type AnswerRequest,
  type QuestionEntry,
  type QuestionSetView,
  type QuestionVotesView,
  type VoteRequest,
} from "../../lib/questionSet";
import {
  useQuestionSetsByPath,
  useQuestionVotesByPath,
  useQuestionVoteWrite,
  useQuestionWrite,
} from "../../hooks/useQuestionSets";
import {
  cardDataForEntry,
  QUESTION_BRIDGE_VERSION,
  QUESTION_MSG_SOURCE_CHILD,
  QUESTION_MSG_SOURCE_HOST,
  QUESTION_VOTE_SAVING,
  QUESTION_VOTE_WITHDRAWING,
  type QuestionCardsPayload,
} from "./questionCardRenderer";

/** Largest proposal message the host reads, in JSON characters. */
export const MAX_QUESTION_PROPOSAL_CHARS = 16 * 1024;
const MAX_REQUEST_ID_CHARS = 64;

/** What a confirmed proposal does. */
export type QuestionProposalAction =
  | { kind: "answer"; request: AnswerRequest }
  | { kind: "vote"; request: VoteRequest }
  | {
      kind: "withdraw-vote";
      request: Pick<VoteRequest, "path" | "questionId">;
    };

export interface QuestionProposal {
  key: string;
  path: string;
  entry: QuestionEntry;
  action: QuestionProposalAction;
  generation: string;
}

export type QuestionAnswerBarState =
  | { kind: "proposal"; proposal: QuestionProposal; error: string | null }
  | { kind: "saving"; proposal: QuestionProposal }
  | { kind: "saved"; proposal: QuestionProposal }
  | {
      kind: "not-published";
      proposal: QuestionProposal;
      hash: string;
      message: string;
    };

export interface QuestionCardBridge {
  /** Present when the document holds placeholders. */
  frame?: {
    setFrame: (element: HTMLIFrameElement | null) => void;
    onLoad: () => void;
  };
  bar: QuestionAnswerBarState | null;
  confirm: () => void;
  dismiss: () => void;
}

/** Validate one frame message against the known references and sets. */
export function validateQuestionProposal(
  data: unknown,
  generation: string,
  knownKeys: ReadonlySet<string>,
  setsByPath: ReadonlyMap<string, QuestionSetView>,
): { proposal: Omit<QuestionProposal, "generation"> } | { rejected: string } {
  if (!data || typeof data !== "object") return { rejected: "not-an-object" };
  const message = data as Record<string, unknown>;
  const kind =
    message.type === "propose-question-answer"
      ? "answer"
      : message.type === "propose-question-vote"
        ? "vote"
        : null;
  if (
    message.source !== QUESTION_MSG_SOURCE_CHILD ||
    message.version !== QUESTION_BRIDGE_VERSION ||
    kind === null
  ) {
    return { rejected: "not-a-proposal" };
  }
  if (message.generation !== generation)
    return { rejected: "stale-generation" };
  let size = 0;
  try {
    size = JSON.stringify(message).length;
  } catch {
    return { rejected: "unserializable" };
  }
  if (size > MAX_QUESTION_PROPOSAL_CHARS) return { rejected: "too-large" };
  const {
    requestId,
    ref,
    expectedRevision,
    selectedOptionIds,
    text,
    rationale,
    withdraw,
  } = message;
  if (
    typeof requestId !== "string" ||
    requestId.length === 0 ||
    requestId.length > MAX_REQUEST_ID_CHARS
  ) {
    return { rejected: "bad-request-id" };
  }
  if (typeof ref !== "string" || !knownKeys.has(ref)) {
    return { rejected: "unlisted-reference" };
  }
  const hash = ref.lastIndexOf("#");
  const path = ref.slice(0, hash);
  const set = setsByPath.get(path);
  const entry = findEntry(set, ref.slice(hash + 1));
  if (!set || !entry || set.invalid) return { rejected: "not-answerable" };
  if (kind === "answer" && !set.answerable)
    return { rejected: "not-answerable" };
  if (expectedRevision !== entry.revision)
    return { rejected: "stale-revision" };
  if (kind === "vote" && withdraw === true) {
    return {
      proposal: {
        key: ref,
        path,
        entry,
        action: {
          kind: "withdraw-vote",
          request: { path, questionId: entry.id },
        },
      },
    };
  }
  if (
    !Array.isArray(selectedOptionIds) ||
    !selectedOptionIds.every(
      (id) =>
        typeof id === "string" &&
        entry.options.some((option) => option.id === id),
    )
  ) {
    return { rejected: "unknown-option" };
  }
  if (typeof text !== "string" || typeof rationale !== "string") {
    return { rejected: "bad-text" };
  }
  if (selectedOptionIds.length === 0 && text.trim().length === 0) {
    return { rejected: "empty-answer" };
  }
  const selected = selectedOptionIds as string[];
  const action: QuestionProposalAction =
    kind === "answer"
      ? {
          kind: "answer",
          request: {
            path,
            questionId: entry.id,
            expectedRevision: entry.revision,
            idempotencyKey: `ui:${randomUuid()}`,
            selectedOptionIds: selected,
            text: text.trim(),
            rationale: rationale.trim(),
          },
        }
      : {
          kind: "vote",
          request: {
            path,
            questionId: entry.id,
            expectedRevision: entry.revision,
            selectedOptionIds: selected,
            text: text.trim(),
          },
        };
  return { proposal: { key: ref, path, entry, action } };
}

export function useQuestionCardBridge(
  workspaceId: string | null,
  html: string,
  enabled: boolean,
): QuestionCardBridge {
  const refs = useMemo(
    () => (enabled ? extractQuestionPlaceholders(html) : []),
    [enabled, html],
  );
  const knownKeys = useMemo(
    () => new Set(refs.map(formatQuestionPlaceholder)),
    [refs],
  );
  const paths = useMemo(
    () => [...new Set(refs.map((ref) => ref.path))],
    [refs],
  );
  const setQueries = useQuestionSetsByPath(workspaceId, paths);
  const voteQueries = useQuestionVotesByPath(workspaceId, paths);
  const { write, install } = useQuestionWrite(workspaceId);
  const { vote, withdraw } = useQuestionVoteWrite(workspaceId);
  const iframeRef = useRef<HTMLIFrameElement | null>(null);
  const generationRef = useRef(randomUuid());
  const [bar, setBar] = useState<QuestionAnswerBarState | null>(null);
  const statusesRef = useRef<Record<string, string>>({});
  // Keys with a vote save in flight; a repeated press for one is dropped.
  const votingRef = useRef(new Set<string>());

  // Query results change identity every render; this key changes only when a
  // set's or a tally's status or data does.
  const dataKey = [...setQueries, ...voteQueries]
    .map((query) => `${query.status}:${query.dataUpdatedAt}`)
    .join(",");
  const setsByPath = useMemo(() => {
    const map = new Map<string, QuestionSetView>();
    paths.forEach((path, index) => {
      const data = setQueries[index]?.data;
      if (data) map.set(path, data);
    });
    return map;
    // eslint-disable-next-line react-hooks/exhaustive-deps -- `dataKey` tracks the query results
  }, [paths, dataKey]);
  const votesByPath = useMemo(() => {
    const map = new Map<string, QuestionVotesView>();
    paths.forEach((path, index) => {
      const data = voteQueries[index]?.data;
      if (data) map.set(path, data);
    });
    return map;
    // eslint-disable-next-line react-hooks/exhaustive-deps -- `dataKey` tracks the query results
  }, [paths, dataKey]);

  const cards = useMemo<QuestionCardsPayload>(() => {
    const payload: QuestionCardsPayload = {};
    refs.forEach((ref) => {
      const key = formatQuestionPlaceholder(ref);
      const index = paths.indexOf(ref.path);
      const query = setQueries[index];
      const set = setsByPath.get(ref.path);
      if (!set) {
        payload[key] = query?.isError
          ? {
              state: "missing",
              message: `Question set ${ref.path} was not found.`,
            }
          : { state: "loading" };
        return;
      }
      if (set.invalid) {
        payload[key] = {
          state: "invalid",
          message: `${ref.path} is not a valid question set (${set.invalid.code}).`,
        };
        return;
      }
      const entry = findEntry(set, ref.id);
      const voteQuery = voteQueries[index];
      const votesView = votesByPath.get(ref.path);
      payload[key] = entry
        ? cardDataForEntry(
            entry,
            set.answerable,
            set.title ?? set.setId,
            votesView
              ? tallyVotes(entry, votesView)
              : voteQuery?.isError
                ? "unavailable"
                : "loading",
            Boolean(workspaceId),
          )
        : {
            state: "missing",
            message: `Question ${ref.id} is not in ${ref.path}.`,
          };
    });
    return payload;
    // eslint-disable-next-line react-hooks/exhaustive-deps -- `dataKey` tracks the query results
  }, [refs, paths, setsByPath, votesByPath, workspaceId, dataKey]);

  // A save that settles after an await posts the cards current then, never
  // the ones it started with.
  const cardsRef = useRef(cards);
  const post = useCallback(() => {
    if (refs.length === 0) return;
    iframeRef.current?.contentWindow?.postMessage(
      {
        source: QUESTION_MSG_SOURCE_HOST,
        version: QUESTION_BRIDGE_VERSION,
        generation: generationRef.current,
        type: "question-data",
        cards: cardsRef.current,
        statuses: statusesRef.current,
      },
      "*",
    );
  }, [refs.length]);

  // In-place update: new data is posted to the mounted frame; the frame's
  // document is never regenerated for it.
  useEffect(() => {
    cardsRef.current = cards;
    post();
  }, [cards, post]);

  const onLoad = useCallback(() => {
    generationRef.current = randomUuid();
    statusesRef.current = {};
    setBar((current) => (current?.kind === "saving" ? current : null));
    post();
  }, [post]);

  const setStatus = useCallback(
    (key: string, status: string | null) => {
      if (status)
        statusesRef.current = { ...statusesRef.current, [key]: status };
      else {
        const next = { ...statusesRef.current };
        delete next[key];
        statusesRef.current = next;
      }
      post();
    },
    [post],
  );

  const runVote = useCallback(
    async (proposal: QuestionProposal) => {
      const { action, key, generation } = proposal;
      if (action.kind === "answer" || votingRef.current.has(key)) return;
      votingRef.current.add(key);
      const withdrawing = action.kind === "withdraw-vote";
      // The card status always reports the outcome, also to a reloaded
      // frame; the bar belongs to one frame load, so a vote from an earlier
      // load never opens or closes it.
      const sameLoad = () => generationRef.current === generation;
      // This card's refused vote in the bar is settled; an answer is never
      // touched.
      const clearRefusedVote = () => {
        if (!sameLoad()) return;
        setBar((bar) =>
          bar?.kind === "proposal" &&
          bar.error !== null &&
          bar.proposal.key === key &&
          bar.proposal.action.kind !== "answer"
            ? null
            : bar,
        );
      };
      try {
        setStatus(
          key,
          withdrawing ? QUESTION_VOTE_WITHDRAWING : QUESTION_VOTE_SAVING,
        );
        if (withdrawing) {
          await withdraw(action.request.path, action.request.questionId);
        } else {
          await vote(action.request);
        }
        setStatus(key, withdrawing ? "Vote withdrawn." : "Vote recorded.");
        clearRefusedVote();
      } catch (error) {
        const refusal = answerRefusal(error);
        // A moved revision: the card shows the current entry and the person
        // votes again from it; a bar retry of the old revision could only be
        // refused again, so none is offered.
        if (refusal.kind === "revision-moved") {
          if (refusal.current) install(proposal.path, refusal.current);
          setStatus(key, "The question changed; vote again.");
          clearRefusedVote();
          return;
        }
        const reason =
          "message" in refusal
            ? refusal.message
            : "The vote was refused; reload and try again.";
        // The card names the reason, and its Vote press retries; the bar also
        // offers a retry unless it holds an unfinished answer.
        setStatus(key, `Vote not saved: ${reason}`);
        if (sameLoad()) {
          setBar((bar) =>
            bar && bar.kind !== "saved" && bar.proposal.action.kind === "answer"
              ? bar
              : { kind: "proposal", proposal, error: reason },
          );
        }
      } finally {
        votingRef.current.delete(key);
      }
    },
    [install, setStatus, vote, withdraw],
  );

  useEffect(() => {
    if (refs.length === 0) return;
    const onMessage = (event: MessageEvent) => {
      if (event.source !== iframeRef.current?.contentWindow) return;
      const result = validateQuestionProposal(
        event.data,
        generationRef.current,
        knownKeys,
        setsByPath,
      );
      if ("rejected" in result) return;
      const proposal = {
        ...result.proposal,
        generation: generationRef.current,
      };
      // A vote is applied with the frame's one press; an answer waits for
      // the person's confirmation in the bar.
      if (proposal.action.kind !== "answer") {
        void runVote(proposal);
        return;
      }
      setBar((current) =>
        current?.kind === "saving"
          ? current
          : { kind: "proposal", proposal, error: null },
      );
    };
    window.addEventListener("message", onMessage);
    return () => window.removeEventListener("message", onMessage);
  }, [knownKeys, refs.length, runVote, setsByPath]);

  const runAnswer = useCallback(
    async (proposal: QuestionProposal, request: AnswerRequest) => {
      setBar({ kind: "saving", proposal });
      try {
        const outcome = await write(request, "answer");
        if (outcome.publication.status === "notPublished") {
          setBar({
            kind: "not-published",
            proposal: { ...proposal, action: { kind: "answer", request } },
            hash: outcome.hash,
            message: outcome.publication.message,
          });
          setStatus(proposal.key, "Applied, not published.");
        } else {
          setBar({ kind: "saved", proposal });
          setStatus(proposal.key, "Saved.");
        }
      } catch (error) {
        const refusal = answerRefusal(error);
        if (
          (refusal.kind === "revision-moved" ||
            refusal.kind === "reload-required") &&
          refusal.current
        ) {
          install(proposal.path, refusal.current);
        }
        setBar({
          kind: "proposal",
          proposal,
          error:
            refusal.kind === "revision-moved"
              ? "The question changed; review it and answer again."
              : "message" in refusal
                ? refusal.message
                : "The answer was refused; reload and try again.",
        });
        setStatus(proposal.key, null);
      }
    },
    [install, setStatus, write],
  );

  const confirm = useCallback(() => {
    if (!bar) return;
    if (bar.kind === "proposal") {
      if (bar.proposal.generation !== generationRef.current) {
        setBar(null);
        return;
      }
      if (bar.proposal.action.kind === "answer") {
        void runAnswer(bar.proposal, bar.proposal.action.request);
      } else {
        void runVote(bar.proposal);
      }
    } else if (
      bar.kind === "not-published" &&
      bar.proposal.action.kind === "answer"
    ) {
      void runAnswer(bar.proposal, {
        ...bar.proposal.action.request,
        appliedHash: bar.hash,
      });
    }
  }, [bar, runAnswer, runVote]);

  const dismiss = useCallback(() => {
    if (bar && bar.kind !== "saving") {
      setStatus(bar.proposal.key, null);
      setBar(null);
    }
  }, [bar, setStatus]);

  const setFrame = useCallback((element: HTMLIFrameElement | null) => {
    iframeRef.current = element;
  }, []);

  return {
    frame: refs.length > 0 ? { setFrame, onLoad } : undefined,
    bar,
    confirm,
    dismiss,
  };
}

function selectionLabel(proposal: QuestionProposal): string {
  if (proposal.action.kind === "withdraw-vote") return "";
  const request = proposal.action.request;
  const labels = (request.selectedOptionIds ?? []).map(
    (id) =>
      proposal.entry.options.find((option) => option.id === id)?.label ?? id,
  );
  const text = request.text ? `“${request.text}”` : "";
  return [labels.join(", "), text].filter(Boolean).join(" ");
}

function leadOf(action: QuestionProposalAction): string {
  switch (action.kind) {
    case "answer":
      return "This page proposes answering";
    case "vote":
      return "Your vote on";
    case "withdraw-vote":
      return "Withdrawing your vote on";
  }
}

function confirmLabel(action: QuestionProposalAction): string {
  switch (action.kind) {
    case "answer":
      return "Answer";
    case "vote":
      return "Vote";
    case "withdraw-vote":
      return "Withdraw vote";
  }
}

/**
 * The trusted host confirmation for a frame's proposed answer, and the reason
 * and retry for a refused vote.
 */
export function QuestionAnswerBar({
  state,
  onConfirm,
  onDismiss,
  className,
}: {
  state: QuestionAnswerBarState;
  onConfirm: () => void;
  onDismiss: () => void;
  className?: string;
}) {
  const { proposal } = state;
  const { action } = proposal;
  return (
    <div
      data-testid="question-answer-bar"
      data-state={state.kind}
      data-action={action.kind}
      role="status"
      aria-live="polite"
      className={classNames(
        "flex shrink-0 flex-wrap items-center gap-2 border-b border-[var(--g5-stroke)] bg-[var(--g5-bg1)] px-3 py-2 text-xs",
        className,
      )}
    >
      <span className="min-w-0 flex-1 truncate text-[var(--g5-text1)]">
        {state.kind === "proposal" && (
          <>
            {leadOf(action)} Q{proposal.entry.displayNumber} “
            {proposal.entry.prompt}”
            {action.kind === "withdraw-vote" ? "." : ": "}
            <strong>{selectionLabel(proposal)}</strong>
            {state.error && (
              <span className="ml-2 text-[var(--g5-blocked)]">
                {state.error}
              </span>
            )}
          </>
        )}
        {state.kind === "saving" && "Saving the answer…"}
        {state.kind === "saved" && (
          <span className="text-[var(--g5-ok)]">
            Answer saved and published.
          </span>
        )}
        {state.kind === "not-published" && (
          <span className="text-[var(--g5-warn)]">
            Answer applied, not published ({state.message}).
          </span>
        )}
      </span>
      {state.kind === "proposal" && (
        <button
          type="button"
          data-testid="question-answer-confirm"
          onClick={onConfirm}
          className="shrink-0 rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] bg-[var(--g5-bg2)] px-2 py-1 font-medium text-[var(--g5-text0)]"
        >
          {confirmLabel(action)}
        </button>
      )}
      {state.kind === "not-published" && (
        <button
          type="button"
          data-testid="question-answer-retry"
          onClick={onConfirm}
          className="shrink-0 rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] bg-[var(--g5-bg2)] px-2 py-1 font-medium text-[var(--g5-text0)]"
        >
          Retry publish
        </button>
      )}
      {state.kind !== "saving" && (
        <button
          type="button"
          data-testid="question-answer-dismiss"
          onClick={onDismiss}
          className="shrink-0 rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] px-2 py-1 text-[var(--g5-text2)]"
        >
          {state.kind === "proposal" ? "Dismiss" : "Close"}
        </button>
      )}
    </div>
  );
}
