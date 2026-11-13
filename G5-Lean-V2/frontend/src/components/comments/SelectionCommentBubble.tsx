// =============================================================================
// G5 Shared Components — SelectionCommentBubble
// =============================================================================
// The "Comment" bubble above selected text, in plain text and in the rendered
// document alike: one click starts a thread on the selection's first line,
// quoting the selection (`g5.documents.comments.identity`).
// =============================================================================

import React from "react";
import { MessageCircle } from "lucide-react";
import { classNames } from "../ui/UI";

export interface SelectionCommentBubbleProps {
  /** Where the bubble floats: the selection's top center, in viewport pixels. */
  position: { top: number; left: number };
  testId: string;
  onComment: () => void;
}

export const SelectionCommentBubble: React.FC<SelectionCommentBubbleProps> = ({
  position,
  testId,
  onComment,
}) => (
  <div
    className={classNames(
      "fixed z-50 flex items-center gap-0.5 px-1 py-0.5",
      "bg-[var(--g5-bg1)] border border-[var(--g5-stroke)]",
      "rounded-[var(--g5-radius-md)] shadow-lg",
    )}
    style={{
      top: Math.max(4, position.top - 40),
      left: position.left,
      transform: "translateX(-50%)",
    }}
  >
    <button
      type="button"
      data-testid={testId}
      className={classNames(
        "flex items-center gap-1.5 px-2.5 py-1.5 text-xs font-medium cursor-pointer",
        "text-[var(--g5-info)] hover:bg-[var(--g5-overlay-hover)] rounded transition-colors",
      )}
      // Pressing the bubble must not collapse the selection it is about.
      onMouseDown={(event) => event.preventDefault()}
      onClick={onComment}
      title="Comment on selection"
    >
      <MessageCircle className="w-3.5 h-3.5" />
      Comment
    </button>
  </div>
);
