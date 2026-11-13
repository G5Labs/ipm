// =============================================================================
// Suggestion records — which of two views of one suggestion is newer
// =============================================================================

import type { ReviewSuggestion } from "./types";

/**
 * Whether `next` may replace `held` as what a kept card shows: a decision
 * never gives way to an earlier open view of the same suggestion, and of two
 * decisions the later stands. A list read before a decision, or a delayed
 * answer, cannot make a decided card offer decisions again
 * (`g5.governance.review-suggestions.display`).
 */
export function isLaterSuggestion(
  held: ReviewSuggestion,
  next: ReviewSuggestion,
): boolean {
  const heldDecided = held.status !== "open";
  if (!heldDecided) return true;
  if (next.status === "open") return false;
  // Times are compared as instants: the server's text varies in precision.
  return !(decidedAt(next) < decidedAt(held));
}

function decidedAt(suggestion: ReviewSuggestion): number {
  return suggestion.statusAt
    ? Date.parse(suggestion.statusAt)
    : Number.NEGATIVE_INFINITY;
}
