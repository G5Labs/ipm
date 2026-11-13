/**
 * ReviewRoundsOverview — on a review workspace, the source's reviews as the
 * reviewer needs them: every review (one reviewer, one kind) with its
 * rounds, unresolved ones first and by default the only ones shown. Each
 * round carries its verdict, its counts and what it changed since the round
 * before, so a reviewer sees how the owner handled their last round and
 * what their next push would add.
 */
import React, { useMemo, useState } from "react";
import { Chip, classNames } from "../ui/UI";
import {
  useReviewPublications,
  useReviewSuggestions,
} from "../../hooks/useWorkspaceReview";
import type {
  ReviewFinding,
  ReviewPublicationSummary,
  ReviewSuggestion,
} from "../../lib/types";
import { roundDelta, roundDeltaText } from "./ReviewsPanel";
import { isPublicationCurrentWithPendingEdits } from "./reviewWorkspace";

export interface ReviewRoundsOverviewProps {
  sourceWorkspaceId: string;
  sourceWorkspaceName: string;
  viewerUserId: string;
}

/** One reviewer's review of the source: its rounds, newest first. */
export interface ReviewThread {
  key: string;
  reviewerUserId: string;
  authorKind: string;
  authorLabel: string;
  rounds: ReviewPublicationSummary[];
  openCount: number;
  acceptedCount: number;
  rejectedCount: number;
  /** Open findings of its rounds: a findings-only round has no suggestions. */
  openFindings: number;
  /**
   * Open suggestions of rounds about an earlier commit of the source: shown
   * as such, never counted as unresolved (the owner moved on).
   */
  outdatedOpenCount: number;
  /** Ids of its rounds about the source's current commit. */
  currentRoundIds: Set<string>;
}

/**
 * A review is unresolved while a suggestion or a finding of one of its
 * current rounds is open; rounds about an earlier commit do not count.
 */
export function threadUnresolved(thread: ReviewThread): boolean {
  return thread.openCount + thread.openFindings > 0;
}

/**
 * Group the source's round publications into reviews (reviewer × kind).
 * A review is unresolved while any of its suggestions is still open.
 */
export function reviewThreads(
  publications: ReviewPublicationSummary[],
  suggestions: ReviewSuggestion[],
  displayNames: Record<string, string>,
  findings: ReviewFinding[] = [],
  sourceHeads: Record<string, string> = {},
): ReviewThread[] {
  const byKey = new Map<string, ReviewThread>();
  for (const publication of publications) {
    if ((publication.kind ?? "round") !== "round") continue;
    const key = `${publication.reviewerUserId}\u0000${publication.authorKind}`;
    const thread = byKey.get(key) ?? {
      key,
      reviewerUserId: publication.reviewerUserId,
      authorKind: publication.authorKind,
      authorLabel:
        publication.authorKind === "human"
          ? (displayNames[publication.reviewerUserId] ??
            publication.authorLabel)
          : publication.authorLabel,
      rounds: [],
      openCount: 0,
      acceptedCount: 0,
      rejectedCount: 0,
      openFindings: 0,
      outdatedOpenCount: 0,
      currentRoundIds: new Set<string>(),
    };
    thread.rounds.push(publication);
    if (
      isPublicationCurrentWithPendingEdits(
        publication,
        sourceHeads,
        suggestions,
      )
    ) {
      thread.currentRoundIds.add(publication.id);
    }
    byKey.set(key, thread);
  }
  for (const thread of byKey.values()) {
    thread.rounds.sort((a, b) => b.round - a.round);
    const ids = new Set(thread.rounds.map((round) => round.id));
    const roundPositions = new Map(
      thread.rounds.map((round, i) => [round.id, i]),
    );
    const seenEdits = new Set<string>();
    for (const suggestion of [...suggestions].sort(
      (a, b) =>
        (roundPositions.get(a.publicationId) ?? Infinity) -
        (roundPositions.get(b.publicationId) ?? Infinity),
    )) {
      if (!ids.has(suggestion.publicationId)) continue;
      if (suggestion.editId) {
        if (seenEdits.has(suggestion.editId)) continue;
        seenEdits.add(suggestion.editId);
      }
      if (suggestion.status === "open") {
        if (thread.currentRoundIds.has(suggestion.publicationId)) {
          thread.openCount += 1;
        } else {
          thread.outdatedOpenCount += 1;
        }
      } else if (suggestion.status === "accepted") thread.acceptedCount += 1;
      else if (suggestion.status === "rejected") thread.rejectedCount += 1;
    }
    for (const finding of findings) {
      if (
        thread.currentRoundIds.has(finding.publicationId) &&
        finding.status === "open"
      ) {
        thread.openFindings += 1;
      }
    }
  }
  return [...byKey.values()].sort((a, b) => {
    const unresolved =
      Number(threadUnresolved(b)) - Number(threadUnresolved(a));
    if (unresolved !== 0) return unresolved;
    return (b.rounds[0]?.createdAt ?? "").localeCompare(
      a.rounds[0]?.createdAt ?? "",
    );
  });
}

function verdictState(
  verdict: string | null,
): "ok" | "warn" | "info" | "muted" {
  switch (verdict) {
    case "approve":
      return "ok";
    case "request_changes":
      return "warn";
    case "comment_only":
      return "info";
    default:
      return "muted";
  }
}

function verdictLabel(verdict: string | null): string {
  switch (verdict) {
    case "approve":
      return "Approved";
    case "request_changes":
      return "Changes requested";
    case "comment_only":
      return "Comments";
    default:
      return "No verdict";
  }
}

export const ReviewRoundsOverview: React.FC<ReviewRoundsOverviewProps> = ({
  sourceWorkspaceId,
  sourceWorkspaceName,
  viewerUserId,
}) => {
  const publications = useReviewPublications(sourceWorkspaceId);
  const suggestions = useReviewSuggestions(sourceWorkspaceId);
  const [expanded, setExpanded] = useState(true);
  const [showResolved, setShowResolved] = useState(false);
  const threads = useMemo(
    () =>
      publications.status === "ok" && suggestions.status === "ok"
        ? reviewThreads(
            publications.data.publications,
            suggestions.data.suggestions,
            publications.data.reviewerDisplayNames,
            publications.data.findings,
            publications.data.sourceHeads ?? {},
          )
        : [],
    [publications, suggestions],
  );
  if (publications.status !== "ok" || suggestions.status !== "ok") return null;
  const unresolved = threads.filter(threadUnresolved);
  const visible = showResolved ? threads : unresolved;
  // The viewer's own review first among equals.
  visible.sort(
    (a, b) =>
      Number(b.reviewerUserId === viewerUserId && b.authorKind === "human") -
      Number(a.reviewerUserId === viewerUserId && a.authorKind === "human"),
  );
  return (
    <section data-testid="review-rounds-overview" className="space-y-3 text-sm">
      <div className="flex flex-wrap items-baseline gap-2">
        <button
          type="button"
          className="text-sm font-semibold text-[var(--g5-text0)]"
          data-testid="review-rounds-overview-toggle"
          onClick={() => setExpanded((value) => !value)}
          aria-expanded={expanded}
        >
          Reviews of {sourceWorkspaceName}
        </button>
        <span
          className="text-xs text-[var(--g5-text2)]"
          data-testid="review-rounds-overview-summary"
        >
          {unresolved.length === 0
            ? threads.length === 0
              ? "no reviews yet"
              : "all reviews resolved"
            : `${unresolved.length} unresolved of ${threads.length}`}
        </span>
        {threads.length > unresolved.length && (
          <button
            type="button"
            className="ml-auto text-xs text-[var(--g5-info)] hover:underline"
            data-testid="review-rounds-overview-show-resolved"
            onClick={() => setShowResolved((value) => !value)}
          >
            {showResolved ? "Hide resolved" : "Show resolved"}
          </button>
        )}
      </div>
      {expanded && visible.length === 0 && (
        <p className="rounded-[var(--g5-radius-md)] border border-dashed border-[var(--g5-stroke)] px-4 py-6 text-center text-xs text-[var(--g5-text2)]">
          {threads.length === 0
            ? `Nobody has reviewed ${sourceWorkspaceName} yet.`
            : "Every review is resolved."}
        </p>
      )}
      {expanded && visible.length > 0 && (
        <ul className="space-y-3" data-testid="review-rounds-overview-body">
          {visible.map((thread) => {
            const mine =
              thread.reviewerUserId === viewerUserId &&
              thread.authorKind === "human";
            const open = threadUnresolved(thread);
            return (
              <li
                key={thread.key}
                data-testid={`review-thread-${thread.reviewerUserId}-${thread.authorKind}`}
                className="overflow-hidden rounded-[var(--g5-radius-md)] border border-[var(--g5-stroke)] bg-[var(--g5-bg1)]"
              >
                <div className="flex flex-wrap items-center gap-3 border-b border-[var(--g5-stroke)]/60 px-4 py-2.5">
                  <span className="font-medium text-[var(--g5-text0)]">
                    {thread.authorLabel}
                    {mine ? " (you)" : ""}
                  </span>
                  <Chip
                    state={open ? "warn" : "ok"}
                    text={
                      open
                        ? `Unresolved · ${thread.openCount} open${
                            thread.openFindings > 0
                              ? ` · ${thread.openFindings} open finding${thread.openFindings === 1 ? "" : "s"}`
                              : ""
                          }`
                        : "Resolved"
                    }
                  />
                  <span className="ml-auto text-xs tabular-nums text-[var(--g5-text2)]">
                    {thread.acceptedCount} accepted · {thread.rejectedCount}{" "}
                    rejected
                  </span>
                  {thread.outdatedOpenCount > 0 && (
                    <span
                      className="text-xs text-[var(--g5-text2)]"
                      data-testid={`review-thread-outdated-open-${thread.reviewerUserId}-${thread.authorKind}`}
                    >
                      {thread.outdatedOpenCount} open on an earlier commit
                    </span>
                  )}
                </div>
                <ol className="divide-y divide-[var(--g5-stroke)]/60">
                  {thread.rounds.map((round) => {
                    const delta = roundDeltaText(
                      roundDelta(
                        round,
                        publications.data.publications,
                        suggestions.data.suggestions,
                      ),
                    );
                    const current = thread.currentRoundIds.has(round.id);
                    return (
                      <li
                        key={round.id}
                        data-testid={`review-thread-round-${round.id}`}
                        className={classNames(
                          "flex flex-wrap items-center gap-3 px-4 py-2 text-xs text-[var(--g5-text1)]",
                          round.supersededBy !== null && "opacity-70",
                        )}
                      >
                        <span className="inline-flex items-center rounded-full bg-[var(--g5-bg0)] px-2 py-0.5 text-[11px] font-semibold text-[var(--g5-text0)]">
                          Round {round.round}
                        </span>
                        <Chip
                          state={verdictState(round.verdict)}
                          text={verdictLabel(round.verdict)}
                        />
                        {!current && (
                          <Chip
                            state="muted"
                            text="Outdated"
                            data-testid={`review-thread-round-outdated-${round.id}`}
                          />
                        )}
                        <span className="text-[var(--g5-text2)]">
                          {new Date(round.createdAt).toLocaleString()}
                        </span>
                        <span className="tabular-nums text-[var(--g5-text2)]">
                          {round.openCount} open · {round.acceptedCount}{" "}
                          accepted · {round.rejectedCount} rejected
                        </span>
                        {delta && (
                          <span className="text-[var(--g5-text2)]">
                            {delta}
                          </span>
                        )}
                      </li>
                    );
                  })}
                </ol>
              </li>
            );
          })}
        </ul>
      )}
    </section>
  );
};
