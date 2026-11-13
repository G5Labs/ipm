// =============================================================================
// G5 Shared Components — a file's new-thread draft
// =============================================================================
// Where a new comment thread is being written: the line chosen in the text on
// screen, the words quoted from it and the text as it was then. The draft,
// its words and a send in flight belong to the page's owner under one subject
// per file, so it is the same draft in plain text and in the rendered
// document, and it outlives mode and tab switches
// (`g5.documents.comments.drafts`). Sending anchors the thread to the chosen
// line; if the text changed since, the line is found again or the draft
// stays open with the reason, and a refusal keeps the words.
// =============================================================================

import { useCallback } from "react";
import {
  useComposerField,
  useComposerRef,
  type ComposerSubject,
} from "../../hooks/useComposerStore";
import {
  createCommentAnchor,
  resolveCommentAnchor,
} from "../../lib/commentAnchors";
import { randomUuid } from "../../lib/randomUuid";
import type { AddCommentHandler } from "./commentThreadModel";

/** A new-thread draft on a line of the displayed text. */
export interface CommentDraft {
  lineNumber: number;
  quotedText: string;
  /** The displayed text when the line was chosen. */
  text: string;
}

/** Why a draft can no longer be sent: the viewer may not start a thread here now. */
export const COMMENT_DRAFT_CLOSED =
  "You can no longer start a thread here. Your words stay until you dismiss them.";

/** The subject of a file's new-thread draft, shared by every view of the file. */
export function commentDraftSubject(key: string): ComposerSubject {
  return { kind: "draft", id: `code:${key}` };
}

export function useCommentDraft(subject: ComposerSubject) {
  const [draft, setDraft] = useComposerField<CommentDraft | null>(
    subject,
    "draft",
    null,
  );
  /** Set while the thread is being sent. */
  const sendingRef = useComposerRef<boolean>(subject, "sending", false);
  /** Stable across retries of one draft after a lost response. */
  const messageIdRef = useComposerRef<string | null>(
    subject,
    "messageId",
    null,
  );
  const open = useCallback(
    (lineNumber: number, quotedText: string, text: string) => {
      // A draft being sent keeps its line with its words until the answer
      // comes, so a retry after a refusal still goes to that line.
      if (sendingRef.current) return;
      messageIdRef.current = null;
      setDraft({ lineNumber, quotedText, text });
    },
    [sendingRef, messageIdRef, setDraft],
  );
  const cancel = useCallback(() => {
    messageIdRef.current = null;
    setDraft(null);
  }, [messageIdRef, setDraft]);
  /**
   * Sends the draft as a thread on its line of `displayedContent`, the text
   * on screen now. Rejects with the reason when the line is gone or the
   * start is refused; the draft then stays open.
   */
  const send = useCallback(
    async (
      onAddComment: AddCommentHandler | undefined,
      displayedContent: string,
      repoOid: string | null,
      content: string,
    ) => {
      if (!onAddComment) throw new Error("You can no longer comment here.");
      const submitted = draft;
      if (!submitted) return;
      sendingRef.current = true;
      try {
        const chosen = await createCommentAnchor(
          submitted.text,
          submitted.lineNumber,
        );
        let anchor = chosen;
        if (displayedContent !== submitted.text) {
          const placed = await resolveCommentAnchor(displayedContent, chosen);
          if (placed.status !== "mapped") {
            throw new Error(
              "The commented line changed. Choose the line again to comment on it.",
            );
          }
          anchor = await createCommentAnchor(
            displayedContent,
            placed.lineNumber,
          );
        }
        messageIdRef.current ??= randomUuid();
        await onAddComment({
          id: messageIdRef.current,
          anchor,
          content,
          baseOid: repoOid,
        });
        messageIdRef.current = null;
        // Close the draft that was sent, and only that one.
        setDraft((current) => (current === submitted ? null : current));
      } finally {
        sendingRef.current = false;
      }
    },
    [draft, sendingRef, messageIdRef, setDraft],
  );
  return { draft, open, cancel, send };
}
