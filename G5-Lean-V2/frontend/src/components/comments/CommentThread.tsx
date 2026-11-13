// =============================================================================
// G5 Shared Components — CommentThread
// =============================================================================
// A reusable, collapsible comment thread card used by DocumentPreview and
// CodeEditor. Renders a thread of comments anchored to a specific location.
//
// Anchor-agnostic: receives positioning from the parent and renders the thread
// UI without knowledge of the anchoring strategy.
// =============================================================================

import { ChevronDown, ChevronRight, Send, Trash2 } from "lucide-react";
import React, { useEffect, useMemo, useState } from "react";
import { classNames } from "../ui/UI";
import { useCommentReplies } from "../../hooks/useComments";
import {
  holdsWords,
  isSet,
  useComposerField,
} from "../../hooks/useComposerStore";
import { formatCommentTime } from "./time";

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

/**
 * A thread action. It may return a promise: a rejection is shown on the
 * thread and keeps what the person typed (`g5.documents.comments.failure`).
 */
export type CommentThreadAction<Args extends unknown[]> = (
  ...args: Args
) => void | Promise<unknown>;

export interface CommentThreadProps {
  thread: {
    id: string;
    lineStart: number;
    lineEnd: number;
    resolved: boolean;
    /**
     * Deleted elsewhere while a reply here held words or a send: nothing
     * more is sent, and the words stay until dismissed.
     */
    deleted?: boolean;
    anchorStatus?: "mapped" | "ambiguous" | "orphan" | "unanchored";
    /** The line the thread was written on, shown when it cannot be placed. */
    originalLine?: number | null;
    originalLineText?: string | null;
    /** Stored replies, known before the replies themselves load. */
    replyCount?: number;
    comments: Array<{
      author: string;
      avatar: string;
      text: string;
      time: string;
    }>;
  };
  expanded?: boolean;
  /** True when this thread was opened from its highlighted source text. */
  active?: boolean;
  onToggle?: () => void;
  /**
   * Thread actions. Each control appears only when its handler is passed:
   * callers pass the ones the viewer may perform
   * (`g5.documents.comments.controls`).
   */
  onReply?: CommentThreadAction<[threadId: string, content: string]>;
  onResolve?: CommentThreadAction<[threadId: string]>;
  onUnresolve?: CommentThreadAction<[threadId: string]>;
  onDelete?: CommentThreadAction<[threadId: string]>;
  /**
   * Opens the thread's line in plain text. Passed where cards are listed beside
   * a view without line rows: the line label of a placed thread becomes the
   * control (`g5.documents.comments.identity`).
   */
  onOpenLine?: (line: number) => void;
}

function actionErrorMessage(error: unknown): string {
  if (error instanceof Error && error.message) return error.message;
  return "The action failed. Please try again.";
}

/**
 * The displayed line a thread is placed on, or null when it cannot be placed
 * (ambiguous, orphaned or unanchored) and keeps only the line it was written on.
 */
export function placedLine(
  thread: Pick<CommentThreadProps["thread"], "lineStart" | "anchorStatus">,
): number | null {
  switch (thread.anchorStatus) {
    case "ambiguous":
    case "orphan":
    case "unanchored":
      return null;
    default:
      return thread.lineStart > 0 ? thread.lineStart : null;
  }
}

function anchorLabel(thread: CommentThreadProps["thread"]): string {
  const was =
    thread.originalLine != null ? ` · was L${thread.originalLine}` : "";
  switch (thread.anchorStatus) {
    case "ambiguous":
      return `Ambiguous anchor${was}`;
    case "orphan":
      return `Orphaned anchor${was}`;
    case "unanchored":
      return "Not anchored to a line";
    default:
      return `L${thread.lineStart}`;
  }
}

// ---------------------------------------------------------------------------
// Author avatar color (deterministic per author name, matching V2 palette)
// ---------------------------------------------------------------------------

function avatarColor(author: string): string {
  if (author === "Alice")
    return "bg-[var(--g5-blocked)]/15 text-[var(--g5-blocked)]";
  // fallback: hash-based
  const colors = [
    "bg-[var(--g5-ok)]/15 text-[var(--g5-ok)]",
    "bg-[var(--g5-active)]/15 text-[var(--g5-active)]",
    "bg-[var(--g5-warn)]/15 text-[var(--g5-warn)]",
    "bg-[var(--g5-info)]/15 text-[var(--g5-info)]",
  ];
  let hash = 0;
  for (let i = 0; i < author.length; i++)
    hash = author.charCodeAt(i) + ((hash << 5) - hash);
  return colors[Math.abs(hash) % colors.length];
}

function splitLeadingCommentQuote(content: string): {
  quote: string | null;
  body: string;
} {
  const lines = content.replace(/\r\n/g, "\n").split("\n");
  const quoteLines: string[] = [];
  let bodyStart = 0;

  while (bodyStart < lines.length) {
    const match = /^> ?(.*)$/.exec(lines[bodyStart]);
    if (!match) break;
    quoteLines.push(match[1]);
    bodyStart += 1;
  }

  if (quoteLines.length === 0) {
    return { quote: null, body: content };
  }
  while (bodyStart < lines.length && lines[bodyStart].trim() === "") {
    bodyStart += 1;
  }

  return {
    quote: quoteLines.join(" ").replace(/\s+/g, " ").trim() || null,
    body: lines.slice(bodyStart).join("\n").trim(),
  };
}

// ---------------------------------------------------------------------------
// CommentThread Component — V2 CommentCard style
// ---------------------------------------------------------------------------

export const CommentThread: React.FC<CommentThreadProps> = ({
  thread,
  expanded: controlledExpanded,
  active = false,
  onToggle,
  onReply,
  onResolve,
  onUnresolve,
  onDelete,
  onOpenLine,
}) => {
  const [internalExpanded, setInternalExpanded] = useState(false);
  // What is typed, a send in flight and its error belong to the page's
  // owner: they outlive this card, and unsent words or a send keep the
  // thread shown if it is deleted elsewhere (`g5.documents.comments.failure`).
  const subject = { kind: "thread" as const, id: thread.id };
  const [replyText, setReplyText] = useComposerField<string>(
    subject,
    "reply",
    "",
    holdsWords,
  );
  const [sendingReply, setSendingReply] = useComposerField<boolean>(
    subject,
    "sending",
    false,
    isSet,
  );
  const [actionError, setActionError] = useComposerField<string | null>(
    subject,
    "error",
    null,
  );
  const [confirmingDelete, setConfirmingDelete] = useState(false);

  const isExpanded = controlledExpanded ?? internalExpanded;
  const replies = useCommentReplies(thread.id, isExpanded && !thread.deleted);

  useEffect(() => {
    if (!isExpanded) setConfirmingDelete(false);
  }, [isExpanded]);

  const mergedComments = useMemo(() => {
    const fetchedReplies =
      replies.status === "ok"
        ? replies.data.map((r) => ({
            author: r.authorName,
            avatar: r.authorAvatarUrl || "",
            text: r.content,
            time: r.createdAt,
          }))
        : [];
    return [...thread.comments, ...fetchedReplies];
  }, [thread.comments, replies]);
  // A failed refresh keeps the replies loaded earlier and says so.
  const repliesProblem =
    replies.status === "error"
      ? `Could not load replies: ${replies.error.message}`
      : replies.status === "ok" && replies.refreshError
        ? `Could not refresh replies: ${replies.refreshError.message}`
        : null;

  const handleToggle = () => {
    if (onToggle) onToggle();
    else setInternalExpanded(!internalExpanded);
  };

  /** Run an action; a failure is shown here and nothing typed is lost. */
  const runAction = async (action: () => void | Promise<unknown>) => {
    setActionError(null);
    try {
      await action();
      return true;
    } catch (error) {
      setActionError(actionErrorMessage(error));
      return false;
    }
  };

  const handleReply = async () => {
    const content = replyText.trim();
    if (!content || !onReply || sendingReply || thread.deleted) return;
    setSendingReply(true);
    const sent = await runAction(() => onReply(thread.id, content));
    setSendingReply(false);
    // Clear only what was sent, never a failed reply or newer typing.
    if (sent)
      setReplyText((current) => (current.trim() === content ? "" : current));
  };

  const firstComment = mergedComments[0];
  const loadedReplies = mergedComments.length - 1;
  const remainingCount = Math.max(loadedReplies, thread.replyCount ?? 0);
  if (!firstComment) return null;
  const firstCommentContent = splitLeadingCommentQuote(firstComment.text);

  return (
    <div
      id={thread.id}
      data-testid={`comment-thread-${thread.id}`}
      data-comment-thread-id={thread.id}
      data-active={active ? "true" : "false"}
      data-resolved={thread.resolved ? "true" : "false"}
      onClick={(e) => {
        e.stopPropagation();
        handleToggle();
      }}
      className={classNames(
        "rounded-lg border transition-all cursor-pointer",
        thread.resolved
          ? active
            ? "border-[var(--g5-ok)]/65 bg-[var(--g5-ok)]/10 ring-2 ring-[var(--g5-ok)]/25 shadow-lg shadow-[var(--g5-ok)]/20"
            : isExpanded
              ? "border-[var(--g5-ok)]/45 bg-[var(--g5-ok)]/10 shadow-lg shadow-[var(--g5-ok)]/15"
              : "border-[var(--g5-ok)]/35 bg-[var(--g5-ok)]/10 shadow-md shadow-[var(--g5-ok)]/15 hover:bg-[var(--g5-ok)]/15"
          : active
            ? "border-[var(--g5-info)]/65 bg-[var(--g5-bg1)] ring-2 ring-[var(--g5-info)]/25 shadow-lg shadow-[var(--g5-info)]/20"
            : isExpanded
              ? "border-[var(--g5-info)]/40 bg-[var(--g5-bg1)] shadow-lg shadow-black/30"
              : "border-[var(--g5-stroke)]/20 bg-[var(--g5-bg1)]/70 hover:border-[var(--g5-info)]/20 hover:bg-[var(--g5-bg1)]/90",
      )}
    >
      {/* First comment (always visible) */}
      <div className="px-2.5 py-2 flex items-start gap-2">
        <div
          className={classNames(
            "w-5 h-5 rounded-full flex items-center justify-center text-[9px] font-bold shrink-0 mt-0.5 overflow-hidden",
            avatarColor(firstComment.author),
          )}
        >
          {firstComment.avatar && firstComment.avatar.startsWith("http") ? (
            <img
              src={firstComment.avatar}
              alt={firstComment.author}
              className="w-full h-full rounded-full object-cover"
            />
          ) : (
            firstComment.author.charAt(0).toUpperCase()
          )}
        </div>
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-1.5 text-[10px]">
            <span className="font-medium text-[var(--g5-text0)]">
              {firstComment.author}
            </span>
            <span className="text-[var(--g5-text2)]/40">
              {formatCommentTime(firstComment.time)}
            </span>
            {onOpenLine && placedLine(thread) !== null ? (
              <button
                type="button"
                data-testid="comment-thread-anchor"
                aria-label={`Open line ${thread.lineStart} in plain text`}
                title="Open this line in plain text"
                onClick={(event) => {
                  event.stopPropagation();
                  onOpenLine(thread.lineStart);
                }}
                className="rounded text-[9px] text-[var(--g5-text2)]/30 hover:text-[var(--g5-info)] hover:underline focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-[var(--g5-info)]"
              >
                {anchorLabel(thread)}
              </button>
            ) : (
              <span
                className="text-[var(--g5-text2)]/30 text-[9px]"
                data-testid="comment-thread-anchor"
              >
                {anchorLabel(thread)}
              </span>
            )}
            {thread.deleted && (
              <span
                className="text-[9px] font-medium text-[var(--g5-blocked)]"
                data-testid="comment-thread-deleted-label"
              >
                Deleted
              </span>
            )}
            <button
              type="button"
              data-testid="comment-thread-toggle-expand"
              aria-label={isExpanded ? "Collapse comment" : "Expand comment"}
              aria-expanded={isExpanded}
              title={isExpanded ? "Collapse comment" : "Expand comment"}
              onClick={(event) => {
                event.stopPropagation();
                handleToggle();
              }}
              className="ml-auto rounded p-0.5 text-[var(--g5-text2)] hover:bg-[var(--g5-overlay-hover)] hover:text-[var(--g5-text0)] focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-[var(--g5-info)]"
            >
              {isExpanded ? (
                <ChevronDown className="h-3 w-3" />
              ) : (
                <ChevronRight className="h-3 w-3" />
              )}
            </button>
          </div>
          {thread.anchorStatus !== "mapped" &&
            thread.anchorStatus !== undefined &&
            thread.originalLineText && (
              <p
                data-testid="comment-thread-original-line"
                className="mt-1 truncate font-mono text-[10px] text-[var(--g5-text2)]"
                title={thread.originalLineText}
              >
                {thread.originalLineText}
              </p>
            )}
          {firstCommentContent.quote && (
            <blockquote
              data-testid="comment-thread-quote"
              className={classNames(
                "mt-1 border-l-2 border-[var(--g5-stroke)]/45 pl-2 text-[10px] italic leading-snug text-[var(--g5-text2)]",
                !isExpanded && "line-clamp-2",
              )}
            >
              {firstCommentContent.quote}
            </blockquote>
          )}
          {firstCommentContent.body && (
            <p
              className={classNames(
                "text-[11px] text-[var(--g5-text1)]/80 mt-1 leading-snug",
                !isExpanded && "line-clamp-2",
              )}
            >
              {firstCommentContent.body}
            </p>
          )}
          {!isExpanded && remainingCount > 0 && (
            <span className="text-[10px] text-[var(--g5-info)]/70 mt-0.5 inline-block">
              {remainingCount} more
            </span>
          )}
        </div>
      </div>

      {/* Expanded: remaining comments */}
      {isExpanded && mergedComments.length > 1 && (
        <div className="border-t border-[var(--g5-stroke)]/20 px-2.5 py-2 space-y-2">
          {mergedComments.slice(1).map((c, ci) => (
            <div key={ci} className="flex items-start gap-2">
              <div
                className={classNames(
                  "w-5 h-5 rounded-full flex items-center justify-center text-[9px] font-bold shrink-0 mt-0.5 overflow-hidden",
                  avatarColor(c.author),
                )}
              >
                {c.avatar && c.avatar.startsWith("http") ? (
                  <img
                    src={c.avatar}
                    alt={c.author}
                    className="w-full h-full rounded-full object-cover"
                  />
                ) : (
                  c.author.charAt(0).toUpperCase()
                )}
              </div>
              <div className="flex-1 min-w-0">
                <div className="flex items-center gap-1.5 text-[10px]">
                  <span className="font-medium text-[var(--g5-text0)]">
                    {c.author}
                  </span>
                  <span className="text-[var(--g5-text2)]/40">
                    {formatCommentTime(c.time)}
                  </span>
                </div>
                <p className="text-[11px] text-[var(--g5-text1)]/80 mt-0.5 leading-snug">
                  {c.text}
                </p>
              </div>
            </div>
          ))}
        </div>
      )}
      {isExpanded && repliesProblem && (
        <p
          role="status"
          data-testid="comment-thread-replies-problem"
          className="border-t border-[var(--g5-stroke)]/20 px-2.5 py-1.5 text-[10px] text-[var(--g5-blocked)]"
        >
          {repliesProblem}
        </p>
      )}

      {isExpanded && thread.deleted && (
        <div
          role="status"
          data-testid="comment-thread-deleted"
          className="flex items-center gap-2 border-t border-[var(--g5-stroke)]/20 px-2.5 py-1.5 text-[10px] text-[var(--g5-blocked)]"
        >
          <span className="mr-auto">
            This thread was deleted, so no reply can be sent. Your words stay
            here until you dismiss them.
          </span>
          <button
            type="button"
            data-testid="comment-thread-dismiss"
            onClick={(event) => {
              event.stopPropagation();
              setReplyText("");
              setActionError(null);
            }}
            className="shrink-0 rounded px-2 py-1 text-[var(--g5-text1)] hover:bg-[var(--g5-overlay-hover)]"
          >
            Dismiss
          </button>
        </div>
      )}

      {isExpanded && !onReply && replyText.trim().length > 0 && (
        <div className="border-t border-[var(--g5-stroke)]/20 px-2.5 py-1.5">
          <input
            aria-label="Reply not sent"
            data-testid="comment-thread-kept-reply"
            value={replyText}
            readOnly
            onClick={(event) => event.stopPropagation()}
            className="w-full bg-[var(--g5-bg0)] border border-[var(--g5-stroke)]/30 rounded px-2 py-1 text-[11px] text-[var(--g5-text1)]"
          />
        </div>
      )}
      {isExpanded &&
        !onReply &&
        !thread.deleted &&
        replyText.trim().length > 0 && (
          <div
            role="status"
            data-testid="comment-thread-reply-closed"
            className="flex items-center gap-2 border-t border-[var(--g5-stroke)]/20 px-2.5 py-1.5 text-[10px] text-[var(--g5-blocked)]"
          >
            <span className="mr-auto">
              Replies are closed here now, so this was not sent. Your words stay
              here until you dismiss them.
            </span>
            <button
              type="button"
              data-testid="comment-thread-dismiss"
              onClick={(event) => {
                event.stopPropagation();
                setReplyText("");
                setActionError(null);
              }}
              className="shrink-0 rounded px-2 py-1 text-[var(--g5-text1)] hover:bg-[var(--g5-overlay-hover)]"
            >
              Dismiss
            </button>
          </div>
        )}

      {/* Expanded: reply + decisions, each only when permitted */}
      {isExpanded && (onReply || onResolve || onUnresolve || onDelete) && (
        <div className="border-t border-[var(--g5-stroke)]/20 px-2.5 py-1.5 flex items-center gap-1.5">
          {onReply && (
            <>
              <input
                placeholder="Reply…"
                value={replyText}
                readOnly={thread.deleted}
                onChange={(e) => setReplyText(e.target.value)}
                onClick={(e) => e.stopPropagation()}
                onKeyDown={(e) => {
                  if (e.key === "Enter" && !e.shiftKey) {
                    e.preventDefault();
                    void handleReply();
                  }
                }}
                className="flex-1 bg-[var(--g5-bg0)] border border-[var(--g5-stroke)]/30 rounded px-2 py-1 text-[11px] text-[var(--g5-text1)] placeholder:text-[var(--g5-text2)]/40"
              />
              <button
                type="button"
                data-testid="comment-thread-send-reply"
                aria-label="Send reply"
                title="Send reply"
                disabled={!replyText.trim() || sendingReply || thread.deleted}
                onClick={(e) => {
                  e.stopPropagation();
                  void handleReply();
                }}
                className={classNames(
                  "flex h-6 w-6 shrink-0 items-center justify-center rounded-full transition-colors",
                  replyText.trim() && !sendingReply && !thread.deleted
                    ? "bg-[var(--g5-info)] text-white hover:brightness-110"
                    : "cursor-not-allowed bg-[var(--g5-bg2)] text-[var(--g5-text2)]/40",
                )}
              >
                <Send className="h-3 w-3" />
              </button>
            </>
          )}
          {thread.deleted
            ? null
            : thread.resolved
              ? onUnresolve && (
                  <button
                    type="button"
                    data-testid="comment-thread-unresolve"
                    aria-label="Reopen comment"
                    title="Reopen comment"
                    onClick={(e) => {
                      e.stopPropagation();
                      void runAction(() => onUnresolve(thread.id));
                    }}
                    className="text-[9px] text-[var(--g5-warn)] hover:underline shrink-0 px-1"
                  >
                    ↩
                  </button>
                )
              : onResolve && (
                  <button
                    type="button"
                    data-testid="comment-thread-resolve"
                    aria-label="Resolve comment"
                    title="Resolve comment"
                    onClick={(e) => {
                      e.stopPropagation();
                      void runAction(() => onResolve(thread.id));
                    }}
                    className="text-[9px] text-[var(--g5-ok)] hover:underline shrink-0 px-1"
                  >
                    ✓
                  </button>
                )}
          {onDelete && !thread.deleted && (
            <button
              type="button"
              data-testid="comment-thread-delete"
              aria-label="Delete comment"
              title="Delete comment"
              onClick={(e) => {
                e.stopPropagation();
                setConfirmingDelete(true);
              }}
              className="flex h-6 w-6 shrink-0 items-center justify-center rounded text-[var(--g5-text2)] hover:bg-[var(--g5-blocked)]/10 hover:text-[var(--g5-blocked)]"
            >
              <Trash2 className="h-3 w-3" />
            </button>
          )}
        </div>
      )}
      {isExpanded && actionError && (
        <p
          role="alert"
          data-testid="comment-thread-error"
          className="border-t border-[var(--g5-stroke)]/20 px-2.5 py-1.5 text-[10px] text-[var(--g5-blocked)]"
          onClick={(event) => event.stopPropagation()}
        >
          {actionError}
        </p>
      )}
      {isExpanded && confirmingDelete && (
        <div
          data-testid="comment-delete-confirmation"
          className="flex items-center justify-end gap-1.5 border-t border-[var(--g5-stroke)]/20 px-2.5 py-1.5 text-[10px]"
          onClick={(event) => event.stopPropagation()}
        >
          <span className="mr-auto text-[var(--g5-text2)]">
            Delete this thread?
          </span>
          <button
            type="button"
            data-testid="comment-thread-delete-cancel"
            onClick={() => setConfirmingDelete(false)}
            className="rounded px-2 py-1 text-[var(--g5-text1)] hover:bg-[var(--g5-overlay-hover)]"
          >
            Cancel
          </button>
          <button
            type="button"
            data-testid="comment-thread-delete-confirm"
            onClick={() => {
              if (onDelete) void runAction(() => onDelete(thread.id));
            }}
            className="rounded bg-[var(--g5-blocked)]/15 px-2 py-1 font-medium text-[var(--g5-blocked)] hover:bg-[var(--g5-blocked)]/25"
          >
            Delete
          </button>
        </div>
      )}
    </div>
  );
};
