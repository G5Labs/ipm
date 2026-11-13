// =============================================================================
// QuestionCard — read-only `g5-question` card (center-panel D69)
// =============================================================================
// Renders a fenced `g5-question` code block in read-only markdown surfaces
// (MarkdownRenderer) as a static question card: header (id + status/severity
// chips), prompt, HIL-language option cards with the recommended badge and
// tradeoffs/risks statically expanded, and — when answered — the answer
// summary (selected labels, free text, rationale, answered_by/at).
//
// Strictly non-interactive: no selection, no Answer/Discuss actions, no
// drafts, no live workspace file writes. The interactive counterpart lives in
// editor/StructuredQuestionEditor.tsx (DocumentPreview dispatch).
//
// Payload parsing goes through the shared questionBlock library only. An
// unparseable or oversized payload falls back to plain code rendering with an
// inline notice and never blocks the rest of the document (lenient read).
// =============================================================================

import React, { useMemo } from "react";
import { Chip, classNames } from "../ui/UI";
import type { EscalationOption, State } from "../../lib/types";
import { OptionCard } from "./OptionCard";
import {
  parseQuestionPayload,
  type QuestionOption,
  type QuestionSeverity,
} from "../../lib/questionBlock";
import {
  SEVERITY_LABEL,
  SEVERITY_STATE,
  questionUrgencyStyle,
} from "../../lib/severity";
import { IdentityAvatar } from "./IdentityAvatar";

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

export interface QuestionCardProps {
  /** Body of the fenced `g5-question` code block. */
  content: string;
}

// ---------------------------------------------------------------------------
// Display helpers
// ---------------------------------------------------------------------------

function toEscalationOption(option: QuestionOption): EscalationOption {
  return {
    label: option.label,
    description: option.description,
    // Read-only card: tradeoffs/risks render statically expanded (no
    // collapse toggle — the card has zero interactive controls).
    tradeoffs: option.tradeoffs ?? null,
    risks: option.risks ?? null,
    recommended: option.recommended === true,
  };
}

/** Plain code fallback for invalid/oversized payloads — mirrors the
 * MarkdownRenderer CodeBlock convention (language label bar + pre/code) with
 * an inline notice, keeping the fence content visible. */
function QuestionCardFallback({
  content,
  notice,
  noticeTestId,
}: {
  content: string;
  notice: string;
  noticeTestId: string;
}) {
  return (
    <div
      data-testid="question-card-fallback"
      className="mb-3 rounded-[var(--g5-radius-sm)] overflow-hidden border border-[var(--g5-stroke)]/50"
    >
      <div
        className={classNames(
          "px-3 py-1",
          "text-[10px] font-medium uppercase tracking-wider",
          "text-[var(--g5-text2)]",
          "bg-[var(--g5-bg2)]",
          "border-b border-[var(--g5-stroke)]",
        )}
        style={{ borderBottomWidth: "var(--g5-border-w)" }}
      >
        g5-question
      </div>
      <div
        data-testid={noticeTestId}
        className="border-b border-[var(--g5-warn)]/40 bg-[color-mix(in_srgb,var(--g5-warn)_8%,transparent)] px-3 py-1.5 text-xs text-[var(--g5-text1)]"
      >
        {notice}
      </div>
      <pre
        className={classNames(
          "p-3 overflow-x-auto max-h-[600px] overflow-y-auto",
          "text-sm leading-relaxed",
          "bg-[var(--g5-bg0)]",
          "text-[var(--g5-text1)]",
        )}
        style={{ fontFamily: "var(--g5-font-mono)" }}
      >
        <code>{content}</code>
      </pre>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export const QuestionCard: React.FC<QuestionCardProps> = ({ content }) => {
  const parsed = useMemo(() => parseQuestionPayload(content), [content]);

  if (!parsed.ok) {
    if (parsed.error === "too-large") {
      return (
        <QuestionCardFallback
          content={content}
          notice="Question payload exceeds the 32 KiB cap — rendering read-only."
          noticeTestId="question-card-oversized-notice"
        />
      );
    }
    const detail = parsed.detail ? ` (${parsed.detail})` : "";
    return (
      <QuestionCardFallback
        content={content}
        notice={`Invalid question payload${detail} — rendering as plain code.`}
        noticeTestId="question-card-invalid-notice"
      />
    );
  }

  const question = parsed.question;
  const answered = question.status === "answered";
  const { answer } = question;
  // Only a USER provenance is a settled human decision (D10). An agent-proposed
  // answer keeps the OPEN treatment — spine, tint, Blocking chip, and the
  // un-muted severity — because the urgency is not spent until a person
  // decides. The legacy scalar `answered_by: user` normalizes to `{kind: user}`
  // on read, so pre-migration documents keep their authority here.
  const humanAnswered = answered && answer.answered_by?.kind === "user";
  const hasAnswerDetails =
    answer.selected.length > 0 ||
    answer.text !== "" ||
    answer.rationale !== "" ||
    answer.answered_by !== null ||
    answer.answered_at !== null;

  // Most human-readable identity the payload carries (D69). A legacy scalar
  // payload normalizes to kind-only, so this degrades to the role.
  const answeredByLabel =
    answer.answered_by?.email ??
    answer.answered_by?.id ??
    answer.answered_by?.kind ??
    "";

  return (
    <div
      data-testid="question-card"
      className="my-2 mb-3 space-y-3 rounded-lg border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] p-3"
      // This card is rendered standalone by `MarkdownRenderer`, with no
      // enclosing `[Q<N>.]` record frame to inherit the accent from — so it
      // carries the urgency spine and tint itself, or the read-only surface
      // would show no urgency at all (D69).
      style={questionUrgencyStyle(question.severity, humanAnswered)}
    >
      {/* Header: id + status/severity chips */}
      <div className="flex flex-wrap items-center gap-2">
        {question.id && (
          <span className="font-mono text-xs text-[var(--g5-text2)]">
            {question.id}
          </span>
        )}
        <Chip
          // Reports the payload's status, but takes the green reserved for
          // a settled human decision only under the provenance rule.
          state={humanAnswered ? "ok" : "warn"}
          text={answered ? "Answered" : "Open"}
          data-testid="question-card-status-chip"
        />
        {question.severity && (
          <Chip
            // Muted once answered (D69): the read-only card carries the
            // WHOLE urgency language, not part of it.
            state={humanAnswered ? "muted" : SEVERITY_STATE[question.severity]}
            text={SEVERITY_LABEL[question.severity]}
            data-testid="question-card-severity-chip"
          />
        )}
        {/* The blocking tier's third signal, missing here while the
            interactive card had it — the same question must not read as a
            lower severity depending on which surface shows it (D69). */}
        {question.severity === "critical" && !humanAnswered && (
          <Chip
            state="blocked"
            text="Blocking"
            data-testid="question-card-blocking-chip"
          />
        )}
      </div>

      {/* Prompt */}
      {question.prompt && (
        <p className="text-sm text-[var(--g5-text0)]">{question.prompt}</p>
      )}

      {/* Options in the HIL visual language, statically rendered */}
      {question.options.length > 0 && (
        <div className="space-y-2">
          {question.options.map((option, index) => (
            <OptionCard
              key={`${option.label}-${index}`}
              option={toEscalationOption(option)}
              index={index}
            />
          ))}
        </div>
      )}

      {/* Answered summary */}
      {answered && hasAnswerDetails && (
        <div
          data-testid="question-card-answered-summary"
          className="space-y-2 rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-3 py-2"
        >
          {answer.selected.length > 0 && (
            <div className="flex flex-wrap items-center gap-1.5">
              {answer.selected.map((label) => (
                <span
                  key={label}
                  className="inline-flex items-center rounded-[var(--g5-radius-sm)] border border-[var(--g5-info)] bg-[color-mix(in_srgb,var(--g5-info)_10%,transparent)] px-2 py-0.5 text-xs font-medium text-[var(--g5-text0)]"
                >
                  {label}
                </span>
              ))}
            </div>
          )}
          {answer.text && (
            <p className="text-sm text-[var(--g5-text0)]">{answer.text}</p>
          )}
          {answer.rationale && (
            <p className="text-xs text-[var(--g5-text1)]">
              <span className="font-medium">Rationale:</span> {answer.rationale}
            </p>
          )}
          {(answer.answered_by || answer.answered_at) && (
            <div
              data-testid="question-card-answered-by"
              className="flex items-center gap-1.5 text-[10px] text-[var(--g5-text2)]"
            >
              {answeredByLabel ? (
                <IdentityAvatar
                  identity={answeredByLabel}
                  avatarUrl={answer.answered_by?.avatar_url}
                  size={14}
                />
              ) : null}
              <span>
                Answered
                {answeredByLabel ? ` by ${answeredByLabel}` : ""}
                {answer.answered_at ? ` at ${answer.answered_at}` : ""}
              </span>
            </div>
          )}
        </div>
      )}
    </div>
  );
};
