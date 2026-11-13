// =============================================================================
// Comment on an issue as yourself — workspace Activity and the issue page
// =============================================================================
// A comment is written here and posted to GitHub under the person's own
// account (GitHub App user authorization): from a workspace to its primary
// issue, or from an issue's own page (g5.issue.native.page). Drafting is
// always available; the draft stays in this browser, per person and issue
// (and workspace, from Activity). Posting needs a connected account, an
// issue domain an org admin opened for G5 comments, and a repository GitHub
// reports as non-public. On an untrusted domain the exact text that will
// leave G5 is shown first. Nothing is recorded: when GitHub does not confirm
// a comment, the draft and its idempotency key stay, and posting again is
// safe because G5 looks for the comment before posting
// (g5.issue.comment-recovery). On a native G5 issue the comment is stored in
// G5 at once: no connection, review step or hidden reference line
// (g5.issue.native.writes). On a native issue's page images are pasted,
// dropped or attached into the comment, uploaded before it is posted
// (g5.issue.native.attachments); GitHub accepts no uploads through its API.
// =============================================================================

import { useEffect, useId, useRef, useState } from "react";
import { MessageSquare } from "lucide-react";

import { Button } from "../ui/UI";
import { useAuth } from "../../hooks/useAuth";
import {
  usePostIssueComment,
  usePostIssuePageComment,
} from "../../hooks/useExternalIssues";
import { ApiError } from "../../lib/api";
import {
  BLOCKED_REASONS,
  OUTCOME_UNKNOWN_COMMENT_MESSAGE,
  describePostError,
} from "../../lib/issueCommentErrors";
import { ISSUE_COMMENT_DRAFT_PREFIX } from "../../lib/issueCommentDrafts";
import { randomUuid } from "../../lib/randomUuid";
import type {
  CommentComposerDTO,
  CommentResultDTO,
  ExternalIssueLocatorDTO,
  IssuePageCommentInput,
  MutationHook,
} from "../../lib/types";

import { githubConnectionPath } from "../../lib/githubConnection";
import {
  useIssueImageUploads,
  type ImageUploadTarget,
} from "./NativeAttachments";

// G5 looks for an earlier send of a draft's key this far back
// (`COMMENT_MARKER_WINDOW_DAYS`), so posting a kept draft again stays safe.
const DRAFT_RETENTION_MS = 14 * 24 * 60 * 60 * 1000;
const MARKER_PREVIEW = "<!-- g5-comment (reference id) -->";

interface CommentDraft {
  body: string;
  /** Stable across retries of the same text. */
  key: string;
  savedAt: number;
}

/** Drafts written on an issue's page, shared by every visit to it. */
const ISSUE_PAGE_DRAFT_SCOPE = "issue-page";

/** A draft's place: the person, a workspace id or the issue page, the issue. */
function draftStorageKey(
  userId: string,
  scope: string,
  composer: CommentComposerDTO,
) {
  const { trackerId, externalIssueId } = composer.destination;
  return `${ISSUE_COMMENT_DRAFT_PREFIX}${userId}:${scope}:${trackerId}:${externalIssueId}`;
}

function loadDraft(storageKey: string): CommentDraft | null {
  try {
    const raw = window.localStorage.getItem(storageKey);
    if (!raw) return null;
    const draft = JSON.parse(raw) as Partial<CommentDraft>;
    if (
      typeof draft.body !== "string" ||
      typeof draft.key !== "string" ||
      typeof draft.savedAt !== "number" ||
      Date.now() - draft.savedAt > DRAFT_RETENTION_MS
    ) {
      window.localStorage.removeItem(storageKey);
      return null;
    }
    return draft as CommentDraft;
  } catch {
    return null;
  }
}

function saveDraft(storageKey: string, draft: CommentDraft | null) {
  try {
    if (draft && draft.body.trim()) {
      window.localStorage.setItem(storageKey, JSON.stringify(draft));
    } else {
      window.localStorage.removeItem(storageKey);
    }
  } catch {
    // The draft still lives in this page when storage is unavailable.
  }
}

/** How a composer posts: from a workspace or from the issue page. */
type CommentSender = Pick<
  MutationHook<IssuePageCommentInput, CommentResultDTO>,
  "mutateAsync" | "isLoading" | "error" | "reset"
>;

interface ComposerViewProps {
  composer: CommentComposerDTO;
  /**
   * The issue's domain is marked trusted: post straight from the text box,
   * without the review step.
   */
  trusted?: boolean;
  /** Contextual Reply reuses the destination's saved draft and publication path. */
  initialBody?: string;
  focusInput?: boolean;
  /** Leave for GitHub's authorization page; tests substitute it. */
  redirect?: (url: string) => void;
}

export interface ActivityCommentComposerProps extends ComposerViewProps {
  workspaceId: string;
}

/** The composer addresses a native G5 issue. */
function isNativeComposer(composer: CommentComposerDTO): boolean {
  return composer.provider === "g5";
}

/** Comment on the workspace's primary issue. */
export function ActivityCommentComposer(props: ActivityCommentComposerProps) {
  const { user } = useAuth();
  // Drafts belong to a person: wait until they are known, and start over
  // when the person or destination changes.
  if (!user?.id) return null;
  const storageKey = draftStorageKey(
    user.id,
    props.workspaceId,
    props.composer,
  );
  return (
    <WorkspaceComposer key={storageKey} storageKey={storageKey} {...props} />
  );
}

function WorkspaceComposer({
  workspaceId,
  ...props
}: ActivityCommentComposerProps & { storageKey: string }) {
  const postComment = usePostIssueComment(workspaceId);
  // The destination the person saw is submitted with the comment.
  const sender: CommentSender = {
    ...postComment,
    mutateAsync: (input) =>
      postComment.mutateAsync({
        ...input,
        destination: props.composer.destination,
      }),
  };
  return <ComposerForDraft {...props} postComment={sender} />;
}

export interface IssuePageCommentComposerProps extends ComposerViewProps {
  orgId: string;
  locator: ExternalIssueLocatorDTO;
  /** A G5 issue takes images, uploaded to it as they are added. */
  imageUploads?: ImageUploadTarget | null;
}

/**
 * Comment on an issue from its own page; the draft is the person's for
 * this issue, whichever page visit wrote it.
 */
export function IssuePageCommentComposer(props: IssuePageCommentComposerProps) {
  const { user } = useAuth();
  if (!user?.id) return null;
  const storageKey = draftStorageKey(
    user.id,
    ISSUE_PAGE_DRAFT_SCOPE,
    props.composer,
  );
  return (
    <IssuePageComposer key={storageKey} storageKey={storageKey} {...props} />
  );
}

function IssuePageComposer({
  orgId,
  locator,
  imageUploads = null,
  ...props
}: IssuePageCommentComposerProps & { storageKey: string }) {
  const postComment = usePostIssuePageComment(orgId, locator);
  return (
    <ComposerForDraft
      {...props}
      postComment={postComment}
      imageUploads={imageUploads}
    />
  );
}

function ComposerForDraft({
  composer,
  trusted = false,
  initialBody,
  focusInput = false,
  storageKey,
  postComment,
  imageUploads = null,
  redirect = (url) => window.location.assign(url),
}: ComposerViewProps & {
  storageKey: string;
  postComment: CommentSender;
  imageUploads?: ImageUploadTarget | null;
}) {
  const inputId = useId();
  const [draft, setDraft] = useState<CommentDraft>(() => {
    const saved = loadDraft(storageKey);
    if (!initialBody || saved?.body.includes(initialBody.trimEnd())) {
      return saved ?? { body: "", key: randomUuid(), savedAt: 0 };
    }
    // Adding a different quote changes the text; opening the same reply again
    // keeps both the saved body and its retry identity intact.
    const next = {
      body: saved?.body.trim()
        ? `${saved.body.trimEnd()}\n\n${initialBody}`
        : initialBody,
      key: randomUuid(),
      savedAt: Date.now(),
    };
    saveDraft(storageKey, next);
    return next;
  });
  const [reviewing, setReviewing] = useState(false);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  useEffect(() => {
    if (!focusInput || reviewing) return;
    // Let the dialog capture its opener before moving focus into the composer.
    const frame = requestAnimationFrame(() => inputRef.current?.focus());
    return () => cancelAnimationFrame(frame);
  }, [focusInput, reviewing]);
  const [posted, setPosted] = useState<string | null>(null);
  // GitHub did not confirm the last submission of this text.
  const [unconfirmed, setUnconfirmed] = useState(false);

  const updateBody = (body: string) => {
    // Changed text is a new submission.
    const next = { body, key: randomUuid(), savedAt: Date.now() };
    setDraft(next);
    saveDraft(storageKey, next);
    setReviewing(false);
    setPosted(null);
    setUnconfirmed(false);
    postComment.reset();
  };

  const submit = async () => {
    setUnconfirmed(false);
    try {
      const result = await postComment.mutateAsync({
        body: draft.body,
        idempotencyKey: draft.key,
      });
      if (result.state === "outcome_unknown") {
        // The draft keeps its key: posting again looks for this comment
        // before sending, so it is never posted twice.
        setUnconfirmed(true);
        return;
      }
      saveDraft(storageKey, null);
      setDraft({ body: "", key: randomUuid(), savedAt: 0 });
      setReviewing(false);
      setPosted(result.externalCommentUrl);
    } catch (error) {
      // The refusal is explained below; the draft is kept. A key already
      // used for other text cannot send this one, so the retry gets a new key.
      if (
        error instanceof ApiError &&
        error.details.code === "idempotency_key_reused"
      ) {
        const next = { ...draft, key: randomUuid(), savedAt: Date.now() };
        setDraft(next);
        saveDraft(storageKey, next);
      }
    }
  };

  const startConnection = () => {
    redirect(
      githubConnectionPath(
        `${window.location.pathname}${window.location.search}`,
      ),
    );
  };

  const native = isNativeComposer(composer);
  const images = useIssueImageUploads({
    target: native ? imageUploads : null,
    textareaRef: inputRef,
    value: draft.body,
    onChange: updateBody,
    disabled: postComment.isLoading || composer.state !== "ready",
    testId: "activity-comment-image",
  });
  const blockedReason =
    composer.state === "ready" || composer.state === "connection_required"
      ? null
      : BLOCKED_REASONS[composer.state];
  const canPost =
    composer.state === "ready" &&
    !images.uploading &&
    draft.body.trim() !== "" &&
    draft.body.trim() !== initialBody?.trim();
  const postLabel = native ? "Post comment" : "Post to GitHub";

  return (
    <section
      data-testid="activity-comment-composer"
      aria-label={`Comment on ${composer.displayKey}`}
      className="mt-4 border-t border-[var(--g5-stroke)] pt-4"
    >
      <label
        htmlFor={inputId}
        className="mb-1.5 flex items-center gap-1.5 text-sm font-medium text-[var(--g5-text0)]"
      >
        <MessageSquare aria-hidden="true" className="h-4 w-4" />
        Comment on {composer.displayKey}
        {!native && composer.githubLogin && ` as @${composer.githubLogin}`}
      </label>
      {reviewing ? (
        <div data-testid="activity-comment-review">
          <p className="mb-1 text-xs text-[var(--g5-text2)]">
            This exact text will be posted to {composer.displayKey} as @
            {composer.githubLogin}. The last line is a hidden reference G5 uses
            to confirm delivery.
          </p>
          <pre className="max-h-64 overflow-auto whitespace-pre-wrap rounded-md border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-3 py-2 text-sm text-[var(--g5-text0)]">
            {`${draft.body.trimEnd()}\n\n${MARKER_PREVIEW}`}
          </pre>
        </div>
      ) : (
        <textarea
          ref={inputRef}
          id={inputId}
          data-testid="activity-comment-input"
          value={draft.body}
          onChange={(event) => updateBody(event.target.value)}
          {...images.handlers}
          placeholder={
            native ? "Comment on this issue…" : "Comment on this GitHub issue…"
          }
          rows={4}
          className="w-full rounded-md border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-3 py-2 text-sm text-[var(--g5-text0)] placeholder:text-[var(--g5-text2)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]"
        />
      )}
      {!reviewing && images.controls}

      {!native && trusted && !reviewing && composer.state === "ready" && (
        <p className="mt-1 text-xs text-[var(--g5-text2)]">
          Posts as @{composer.githubLogin}. G5 adds a hidden reference line so
          it can confirm delivery.
        </p>
      )}
      {blockedReason && (
        <p
          data-testid="activity-comment-blocked"
          className="mt-1.5 text-xs text-[var(--g5-text1)]"
        >
          {blockedReason}
        </p>
      )}
      {postComment.error && (
        <p
          role="alert"
          data-testid="activity-comment-error"
          className="mt-1.5 text-xs text-[var(--g5-blocked)]"
        >
          {describePostError(postComment.error)}
        </p>
      )}
      {unconfirmed && (
        <p
          role="status"
          data-testid="activity-comment-unconfirmed"
          className="mt-1.5 text-xs text-[var(--g5-warn)]"
        >
          {OUTCOME_UNKNOWN_COMMENT_MESSAGE}
        </p>
      )}
      {posted && (
        <p
          role="status"
          data-testid="activity-comment-posted"
          className="mt-1.5 text-xs text-[var(--g5-text1)]"
        >
          {native ? "Posted." : "Posted to GitHub."}{" "}
          <a
            href={posted}
            target={native ? undefined : "_blank"}
            rel="noopener noreferrer"
            className="text-[var(--g5-info)] hover:underline"
          >
            View comment
          </a>
        </p>
      )}

      <div className="mt-2 flex flex-wrap justify-end gap-2">
        {!native && composer.state === "connection_required" ? (
          <Button
            kind="primary"
            data-testid="activity-comment-connect"
            onClick={() => void startConnection()}
          >
            Manage GitHub connection
          </Button>
        ) : reviewing ? (
          <>
            <Button
              kind="secondary"
              data-testid="activity-comment-edit"
              disabled={postComment.isLoading}
              onClick={() => setReviewing(false)}
            >
              Edit
            </Button>
            <Button
              kind="primary"
              data-testid="activity-comment-post"
              disabled={!canPost || postComment.isLoading}
              onClick={() => void submit()}
            >
              {postComment.isLoading ? "Posting…" : postLabel}
            </Button>
          </>
        ) : trusted || native ? (
          <Button
            kind="primary"
            data-testid="activity-comment-post"
            disabled={!canPost || postComment.isLoading}
            onClick={() => void submit()}
          >
            {postComment.isLoading ? "Posting…" : postLabel}
          </Button>
        ) : (
          <Button
            kind="primary"
            data-testid="activity-comment-review-button"
            disabled={!canPost}
            onClick={() => setReviewing(true)}
          >
            Review comment
          </Button>
        )}
      </div>
    </section>
  );
}
