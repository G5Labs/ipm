// =============================================================================
// G5 Shared Components — DraftCommentInput
// =============================================================================
// Compact comment composer used by editor-supplied floating draft surfaces.
// Allows typing a comment before saving, rather than immediately sending an
// empty comment to the API.
// =============================================================================

import React, { useRef, useEffect } from "react";
import { classNames } from "../ui/UI";
import {
  useClearComposer,
  useComposerField,
  useComposerRef,
  type ComposerSubject,
} from "../../hooks/useComposerStore";

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

export interface DraftCommentInputProps {
  /**
   * The draft the words belong to. The page's owner holds them, with a
   * send in flight and its error, so they outlive this composer.
   */
  subject: ComposerSubject;
  /** Current author name shown above the draft field. */
  authorName?: string;
  /** Current author avatar URL, when available. */
  authorAvatarUrl?: string | null;
  /** Awareness color used by the initial fallback avatar. */
  authorColor?: string | null;
  /** Quoted text from selection, shown as a blockquote above the input */
  quotedText?: string;
  /**
   * Whether taking focus may scroll the input into view. Off where the
   * composer sits beside the text it is about, so starting a comment does
   * not move the passage being discussed.
   */
  focusScroll?: boolean;
  /**
   * Called when the user submits the comment. While a returned promise is
   * pending the draft cannot be sent again; a rejection is shown here and
   * the typed text stays (`g5.documents.comments.failure`).
   */
  onSubmit: (content: string) => void | Promise<unknown>;
  /** Called when the user cancels (Esc or Cancel button) */
  onCancel: () => void;
  /**
   * Why the draft can no longer be sent (the viewer may no longer start a
   * thread here). The words then stay readable, read-only, with no way to
   * send them, until dismissed (`g5.documents.comments.failure`).
   */
  closed?: string | null;
  /** Pointer handlers supplied by a floating host to make the identity row draggable. */
  dragHandleProps?: Pick<
    React.HTMLAttributes<HTMLDivElement>,
    "onPointerDown" | "onPointerMove" | "onPointerUp" | "onPointerCancel"
  >;
}

function submitErrorMessage(error: unknown): string {
  if (error instanceof Error && error.message) return error.message;
  return "The comment could not be added. Please try again.";
}

// ---------------------------------------------------------------------------
// DraftCommentInput Component
// ---------------------------------------------------------------------------

export const DraftCommentInput: React.FC<DraftCommentInputProps> = ({
  subject,
  authorName = "You",
  authorAvatarUrl,
  authorColor,
  quotedText,
  focusScroll = true,
  onSubmit,
  onCancel,
  closed = null,
  dragHandleProps,
}) => {
  const [text, setText] = useComposerField<string>(subject, "words", "");
  const [submitting, setSubmitting] = useComposerField<boolean>(
    subject,
    "submitting",
    false,
  );
  const [error, setError] = useComposerField<string | null>(
    subject,
    "error",
    null,
  );
  const submittingRef = useComposerRef<boolean>(
    subject,
    "submittingNow",
    false,
  );
  const clearComposer = useClearComposer();
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  // Cancelled, the draft's words go with it.
  const cancel = () => {
    clearComposer(subject);
    onCancel();
  };

  // Auto-focus the textarea when the draft opens
  useEffect(() => {
    // Small timeout so the floating draft surface has settled in the DOM.
    const timer = setTimeout(
      () => textareaRef.current?.focus({ preventScroll: !focusScroll }),
      50,
    );
    return () => clearTimeout(timer);
  }, [focusScroll]);

  const handleSubmit = async () => {
    const trimmed = text.trim();
    if (!trimmed || submittingRef.current || closed) return;

    // Prepend the quoted selection if present
    const fullContent = quotedText
      ? quotedText
          .split("\n")
          .map((l) => `> ${l}`)
          .join("\n") +
        "\n\n" +
        trimmed
      : trimmed;

    submittingRef.current = true;
    setSubmitting(true);
    setError(null);
    try {
      await onSubmit(fullContent);
      // Sent. A host that keeps this composer for another draft gets it
      // empty, so the words are not sent twice.
      setText("");
    } catch (submitError) {
      setError(submitErrorMessage(submitError));
    } finally {
      submittingRef.current = false;
      setSubmitting(false);
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      void handleSubmit();
    }
    if (e.key === "Escape" && !submittingRef.current) {
      e.preventDefault();
      cancel();
    }
  };

  const canSubmit = !!text.trim() && !submitting && !closed;

  return (
    <div
      className={classNames(
        "rounded-xl border border-[var(--g5-stroke)]/70",
        "bg-[var(--g5-bg1)] shadow-xl shadow-black/20",
        "p-4",
      )}
      onClick={(e) => e.stopPropagation()}
    >
      <div
        {...dragHandleProps}
        data-testid={dragHandleProps ? "draft-comment-drag-handle" : undefined}
        title={dragHandleProps ? "Drag comment window" : undefined}
        className={classNames(
          "mb-3 flex items-center gap-2.5",
          dragHandleProps &&
            "cursor-grab touch-none select-none active:cursor-grabbing",
        )}
      >
        <span
          className="flex h-8 w-8 shrink-0 items-center justify-center overflow-hidden rounded-full text-xs font-semibold text-white"
          style={{ backgroundColor: authorColor || "var(--g5-info)" }}
        >
          {authorAvatarUrl ? (
            <img
              src={authorAvatarUrl}
              alt={authorName}
              className="h-full w-full object-cover"
            />
          ) : (
            authorName.charAt(0).toUpperCase()
          )}
        </span>
        <span className="truncate text-sm font-semibold text-[var(--g5-text0)]">
          {authorName}
        </span>
      </div>

      {/* Quoted selection preview */}
      {quotedText && (
        <div
          data-testid="draft-comment-quote"
          className={classNames(
            "mb-2 px-2 py-1 text-[10px] text-[var(--g5-text2)]",
            "border-l-2 border-[var(--g5-info)]/40",
            "bg-[var(--g5-bg0)]/50 rounded-r",
            "line-clamp-3 italic",
          )}
        >
          {quotedText}
        </div>
      )}

      {/* Comment textarea, locked while it is being sent: a sent draft
          closes, and words typed meanwhile would close with it. */}
      <textarea
        ref={textareaRef}
        aria-label="Comment"
        value={text}
        readOnly={submitting || !!closed}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={handleKeyDown}
        rows={2}
        className={classNames(
          "w-full rounded-xl border border-[var(--g5-info)] bg-[var(--g5-bg0)]",
          "px-3 py-2 text-sm text-[var(--g5-text1)]",
          "focus-visible:outline-none resize-none",
          "focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]/20",
        )}
      />

      {closed && (
        <p
          role="status"
          data-testid="draft-comment-closed"
          className="mt-2 text-xs text-[var(--g5-text2)]"
        >
          {closed}
        </p>
      )}

      {error && (
        <p
          role="alert"
          data-testid="draft-comment-error"
          className="mt-2 text-xs text-[var(--g5-blocked)]"
        >
          {error}
        </p>
      )}

      {/* Actions row */}
      <div className="mt-3 flex items-center justify-end gap-2">
        {/* A draft being sent waits for its answer: closing it would hide
            a refusal and the words it keeps. */}
        <button
          type="button"
          data-testid="draft-comment-cancel"
          onClick={cancel}
          disabled={submitting}
          className={classNames(
            "rounded-full px-3 py-1.5 text-xs font-medium",
            "text-[var(--g5-info)] hover:bg-[var(--g5-info)]/10",
            "transition-colors cursor-pointer",
          )}
        >
          {closed ? "Dismiss" : "Cancel"}
        </button>
        {!closed && (
          <button
            type="button"
            data-testid="draft-comment-save"
            onClick={() => void handleSubmit()}
            disabled={!canSubmit}
            className={classNames(
              "rounded-full px-4 py-1.5 text-xs font-semibold",
              "transition-colors cursor-pointer",
              canSubmit
                ? "bg-[var(--g5-info)] text-white hover:brightness-110"
                : "cursor-not-allowed bg-[var(--g5-bg2)] text-[var(--g5-text2)]/60",
            )}
          >
            {submitting ? "Commenting…" : "Comment"}
          </button>
        )}
      </div>
    </div>
  );
};
