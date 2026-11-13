// =============================================================================
// G5 Shared Components — CommentDraftCard
// =============================================================================
// The card where a new thread is written, first in the side list of plain
// text and of the rendered document alike. The draft itself, its anchoring
// and its send belong to the host's `useCommentDraft`; this card shows it and
// says when it can no longer be sent (`g5.documents.comments.identity`,
// `.drafts`).
// =============================================================================

import React from "react";
import type { ComposerSubject } from "../../hooks/useComposerStore";
import { DraftCommentInput } from "./DraftCommentInput";
import type { CommentDraft } from "./commentDraft";

export interface CommentDraftCardProps {
  subject: ComposerSubject;
  /** The draft being written; nothing is shown without one. */
  draft: CommentDraft | null;
  /** Why the draft can no longer be sent, or null while it can. */
  closed: string | null;
  /** Sends the typed text; a rejection is shown and keeps the words. */
  onSubmit: (content: string) => void | Promise<unknown>;
  onCancel: () => void;
  /** Whether taking focus may scroll the composer into view. */
  focusScroll?: boolean;
}

export const CommentDraftCard: React.FC<CommentDraftCardProps> = ({
  subject,
  draft,
  closed,
  onSubmit,
  onCancel,
  focusScroll,
}) => {
  if (!draft) return null;
  return (
    <div className="px-2 pb-2 pt-2" data-testid="comment-draft-card">
      <DraftCommentInput
        subject={subject}
        closed={closed}
        quotedText={draft.quotedText || undefined}
        focusScroll={focusScroll}
        onSubmit={onSubmit}
        onCancel={onCancel}
      />
    </div>
  );
};
