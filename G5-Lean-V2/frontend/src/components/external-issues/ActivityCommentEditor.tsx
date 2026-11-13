// =============================================================================
// Activity — edit your own comment on the primary issue
// =============================================================================
// Replaces the text of a comment the person wrote (from G5 or on GitHub)
// under their own GitHub account. The same destination rules as posting
// apply. A comment changed on GitHub since the editor opened is not
// overwritten. G5 keeps its hidden reference line on comments it posted.
// Saving the same text again is harmless, so a save GitHub did not confirm
// is simply saved again.
// A native G5 comment is edited in G5, without a review step, under the
// same changed-since-opened rule (g5.issue.native.writes).
// =============================================================================

import { useId, useState } from "react";

import { Button } from "../ui/UI";
import { useEditIssueComment } from "../../hooks/useExternalIssues";
import { ApiError } from "../../lib/api";
import {
  COMMENT_EDIT_UNCONFIRMED_MESSAGE,
  describePostError,
} from "../../lib/issueCommentErrors";
import { randomUuid } from "../../lib/randomUuid";
import type { CommentComposerDTO } from "../../lib/types";

/** The hidden reference line G5 appends to comments it posts. */
export const G5_REFERENCE_AT_END =
  /\s*<!-- g5-comment [0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12} -->\s*$/;

function describeEditError(error: unknown, native: boolean): string {
  if (error instanceof ApiError) {
    switch (error.details.code) {
      case "comment_changed":
        return native
          ? "This comment changed since you opened the editor. Reload Activity to edit the latest version."
          : "This comment changed on GitHub since you opened the editor. Reload Activity to edit the latest version.";
      case "not_your_comment":
        return "Only your own comments can be edited.";
    }
  }
  return describePostError(error);
}

export interface ActivityCommentEditorProps {
  workspaceId: string;
  composer: CommentComposerDTO;
  commentId: string;
  /** The comment as its tracker holds it now. */
  body: string;
  updatedAt: string;
  trusted: boolean;
  onDone: () => void;
}

export function ActivityCommentEditor({
  workspaceId,
  composer,
  commentId,
  body,
  updatedAt,
  trusted,
  onDone,
}: ActivityCommentEditorProps) {
  const inputId = useId();
  const native = composer.provider === "g5";
  const keepsReference = !native && G5_REFERENCE_AT_END.test(body);
  const [text, setText] = useState(() => body.replace(G5_REFERENCE_AT_END, ""));
  // Changed text is a new edit; the same text keeps its key for a retry.
  const [key, setKey] = useState(() => randomUuid());
  const [reviewing, setReviewing] = useState(false);
  const editComment = useEditIssueComment(workspaceId);
  const [savedError, setSavedError] = useState<string | null>(null);

  const save = async () => {
    setSavedError(null);
    try {
      const result = await editComment.mutateAsync({
        provider: native ? "g5" : "github",
        commentId,
        body: text,
        idempotencyKey: key,
        destination: composer.destination,
        expectedUpdatedAt: updatedAt,
      });
      if (result.state === "published") {
        onDone();
      } else {
        // Nothing is recorded; replacing the text again is harmless.
        setSavedError(COMMENT_EDIT_UNCONFIRMED_MESSAGE);
      }
    } catch {
      // Explained below.
    }
  };

  const canSave = text.trim() !== "" && !editComment.isLoading;
  return (
    <div data-testid={`activity-comment-editor-${commentId}`}>
      {reviewing ? (
        <div data-testid="activity-comment-edit-review">
          <p className="mb-1 text-xs text-[var(--g5-text2)]">
            This text will replace the comment on {composer.displayKey} as @
            {composer.githubLogin}.
            {keepsReference &&
              " G5 keeps its hidden reference line at the end."}
          </p>
          <pre className="max-h-64 overflow-auto whitespace-pre-wrap rounded-md border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-3 py-2 text-sm text-[var(--g5-text0)]">
            {text.trimEnd()}
          </pre>
        </div>
      ) : (
        <>
          <label htmlFor={inputId} className="sr-only">
            Edit comment
          </label>
          <textarea
            id={inputId}
            data-testid="activity-comment-edit-input"
            value={text}
            onChange={(event) => {
              setText(event.target.value);
              setKey(randomUuid());
              setSavedError(null);
              editComment.reset();
            }}
            rows={4}
            className="w-full rounded-md border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-3 py-2 text-sm text-[var(--g5-text0)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]"
          />
        </>
      )}
      {(editComment.error || savedError) && (
        <p
          role="alert"
          data-testid="activity-comment-edit-error"
          className="mt-1.5 text-xs text-[var(--g5-blocked)]"
        >
          {savedError ?? describeEditError(editComment.error, native)}
        </p>
      )}
      <div className="mt-2 flex flex-wrap justify-end gap-2">
        <Button
          kind="ghost"
          data-testid="activity-comment-edit-cancel"
          disabled={editComment.isLoading}
          onClick={onDone}
        >
          Cancel
        </Button>
        {reviewing && (
          <Button
            kind="secondary"
            data-testid="activity-comment-edit-back"
            disabled={editComment.isLoading}
            onClick={() => setReviewing(false)}
          >
            Edit
          </Button>
        )}
        {trusted || reviewing || native ? (
          <Button
            kind="primary"
            data-testid="activity-comment-edit-save"
            disabled={!canSave}
            onClick={() => void save()}
          >
            {editComment.isLoading
              ? "Saving…"
              : native
                ? "Save"
                : "Save to GitHub"}
          </Button>
        ) : (
          <Button
            kind="primary"
            data-testid="activity-comment-edit-review-button"
            disabled={!canSave}
            onClick={() => setReviewing(true)}
          >
            Review change
          </Button>
        )}
      </div>
    </div>
  );
}
