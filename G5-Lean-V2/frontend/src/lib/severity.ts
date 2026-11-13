// Shared severity presentation for every surface that renders a severity chip.
//
// `UncertaintySeverity` (HIL escalations) and `QuestionSeverity` (doc question
// blocks, center-panel D69) are the same `low|medium|high|critical` vocabulary
// — D69 states the question payload reuses the escalation vocabulary so one
// option/severity shape serves escalations, chat questions, and doc questions.
// These mappings were previously copy-pasted into QuestionCard,
// StructuredQuestionEditor and HilPriorityBanner, where
// they could drift apart silently.

import type { State, UncertaintySeverity } from "./types";

/** The severity vocabulary shared by escalations and doc questions. */
export type Severity = UncertaintySeverity;

/** Chip state (colour role) for a severity. */
export const SEVERITY_STATE: Record<Severity, State> = {
  low: "info",
  medium: "warn",
  high: "blocked",
  critical: "blocked",
};

/** Human-readable chip text for a severity. */
export const SEVERITY_LABEL: Record<Severity, string> = {
  low: "Low",
  medium: "Medium",
  high: "High",
  critical: "Critical",
};

/**
 * Design token carrying each severity's colour, for surfaces that paint more
 * than a chip (the question card's urgency spine, center-panel D69). Same
 * colour role as `SEVERITY_STATE` so a card and its chip can never disagree.
 */
export const SEVERITY_TOKEN: Record<Severity, string> = {
  low: "var(--g5-info)",
  medium: "var(--g5-warn)",
  high: "var(--g5-blocked)",
  critical: "var(--g5-blocked)",
};

/** Neutral token for a question that is answered, or carries no severity. */
export const SEVERITY_TOKEN_NEUTRAL = "var(--g5-stroke)";

/** Spine + tint for one question card (D69). */
export interface QuestionUrgencyStyle {
  borderLeftColor: string;
  borderLeftWidth: string;
  backgroundColor?: string;
}

/**
 * Urgency treatment for a question card.
 *
 * An answered question renders neutral REGARDLESS of severity: the decision is
 * made, so the urgency is spent — and a doc that stays red after every question
 * is answered teaches the reader to stop seeing red at all.
 */
export function questionUrgencyStyle(
  severity: Severity | null | undefined,
  answered: boolean,
): QuestionUrgencyStyle {
  if (answered || !severity) {
    return {
      borderLeftColor: SEVERITY_TOKEN_NEUTRAL,
      borderLeftWidth: "3px",
    };
  }
  const token = SEVERITY_TOKEN[severity];
  // `critical` is the blocking tier: a thicker spine and a tint back it up, so
  // it separates from `high` (same red) without relying on colour alone.
  return severity === "critical"
    ? {
        borderLeftColor: token,
        borderLeftWidth: "5px",
        backgroundColor: `color-mix(in srgb, ${token} 7%, var(--g5-bg1))`,
      }
    : { borderLeftColor: token, borderLeftWidth: "3px" };
}
