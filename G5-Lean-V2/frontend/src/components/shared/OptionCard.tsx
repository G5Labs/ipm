// =============================================================================
// OptionCard — one suggested option of an escalation question
// =============================================================================
// Shared by the live question-document surfaces (`QuestionCard`,
// `StructuredQuestionEditor`). Renders the option label, description,
// tradeoffs, risks and the recommended marker.
// =============================================================================

import React from "react";
import { classNames } from "../ui/UI";
import type { EscalationOption } from "../../lib/types";

export interface OptionCardProps {
  option: EscalationOption;
  index: number;
}

export const OptionCard: React.FC<OptionCardProps> = ({ option, index }) => (
  <div
    data-testid="escalation-option"
    className={classNames(
      "px-3 py-2 rounded-[var(--g5-radius-sm)]",
      "border",
      option.recommended
        ? "border-[var(--g5-ok)] bg-[color-mix(in_srgb,var(--g5-ok)_5%,transparent)]"
        : "border-[var(--g5-stroke)] bg-[var(--g5-bg1)]",
    )}
  >
    <div className="flex items-center gap-2">
      <span className="text-xs font-mono text-[var(--g5-text2)]">
        #{index + 1}
      </span>
      <span className="text-sm font-medium text-[var(--g5-text0)]">
        {option.label}
      </span>
      {option.recommended && (
        <span className="text-[10px] font-semibold uppercase tracking-wide text-[var(--g5-ok)]">
          Recommended
        </span>
      )}
    </div>
    <p className="mt-1 text-xs text-[var(--g5-text1)]">{option.description}</p>
    {option.tradeoffs && (
      <p className="mt-1 text-xs text-[var(--g5-text2)]">
        <span className="font-medium">Tradeoffs:</span> {option.tradeoffs}
      </p>
    )}
    {option.risks && (
      <p className="mt-1 text-xs text-[var(--g5-text2)]">
        <span className="font-medium text-[var(--g5-warn)]">Risks:</span>{" "}
        {option.risks}
      </p>
    )}
  </div>
);
