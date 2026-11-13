// =============================================================================
// G5 Workspace — ReviewBadge
// =============================================================================
// Marks a review workspace (a reviewer-owned fork of the workspace under
// review) wherever a workspace is identified: the switcher trigger, the
// review banner, the Approvals card. Same shape as ReadOnlyBadge so the two
// read as one family; the review identity color is --g5-active, which no
// other workspace-level badge uses.
// =============================================================================

import React from "react";
import { ClipboardCheck } from "lucide-react";
import { classNames } from "../ui/UI";

export interface ReviewBadgeProps {
  /** Icon-only compact form for dense rows (still exposes an accessible label). */
  compact?: boolean;
  className?: string;
}

export const ReviewBadge: React.FC<ReviewBadgeProps> = ({
  compact = false,
  className,
}) => {
  return (
    <span
      data-testid="workspace-review-badge"
      title="Review workspace"
      // The compact form is icon-only, so it needs a named role for its
      // label; the full form's visible "Review" text is its own name.
      {...(compact ? { role: "img", "aria-label": "Review workspace" } : {})}
      className={classNames(
        "inline-flex items-center gap-1 shrink-0",
        "rounded-[var(--g5-radius-sm)]",
        "border border-[var(--g5-active)]/40 bg-[var(--g5-active)]/10",
        "text-[var(--g5-active)]",
        compact ? "px-1 py-0.5" : "px-1.5 py-0.5",
        "text-[10px] font-semibold uppercase tracking-wide leading-none",
        className,
      )}
    >
      <ClipboardCheck size={10} aria-hidden="true" />
      {!compact && <span>Review</span>}
    </span>
  );
};

/**
 * The thin accent rule that tops every review surface (banner, Approvals
 * card, finish dialog): a 2px line in the review identity color, the same
 * quiet stroke G5 uses for every other status edge. Purely presentational;
 * pairs with ReviewBadge.
 */
export const ReviewStripe: React.FC<{ className?: string }> = ({
  className,
}) => (
  <div
    aria-hidden="true"
    data-testid="workspace-review-stripe"
    className={classNames("h-0.5 w-full bg-[var(--g5-active)]", className)}
  />
);
