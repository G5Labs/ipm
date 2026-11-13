// =============================================================================
// G5 Shared Components — CommentColumn
// =============================================================================
// The side list of a file's comment threads, beside Source and Preview alike:
// the threads in the order given (the text's, see `orderBesideView`), stacked
// from the top, one expanded at a time. The column reports each toggle, so
// the view can bring an expanded card's line into view, and passes the line
// label's action through (`g5.documents.comments.identity`).
// =============================================================================

import React, { useEffect, useState } from "react";
import { CommentThread } from "./CommentThread";
import type { CommentThreadAction, CommentThreadProps } from "./CommentThread";

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

export interface CommentColumnProps {
  threads: readonly CommentThreadProps["thread"][];
  /** Thread actions: pass only those the viewer may perform. */
  onReply?: CommentThreadAction<[threadId: string, content: string]>;
  onResolve?: CommentThreadAction<[threadId: string]>;
  onUnresolve?: CommentThreadAction<[threadId: string]>;
  onDelete?: CommentThreadAction<[threadId: string]>;
  /** A card was expanded or collapsed. */
  onThreadSelect?: (threadId: string, expanded: boolean) => void;
  /** Opens a placed thread's line in plain text from its line label. */
  onOpenLine?: (line: number) => void;
  /** Thread selected through its source highlight. */
  activeThreadId?: string | null;
  /** Why the threads shown may be missing or out of date. */
  notice?: string | null;
}

// ---------------------------------------------------------------------------
// CommentColumn Component
// ---------------------------------------------------------------------------

/** Why the threads shown may be missing or out of date. */
export const CommentsNotice: React.FC<{ notice: string }> = ({ notice }) => (
  <p
    role="status"
    data-testid="comment-column-notice"
    className="rounded border border-[var(--g5-blocked)]/40 bg-[var(--g5-bg1)] px-2 py-1 text-[10px] text-[var(--g5-blocked)]"
  >
    {notice}
  </p>
);

/** Nothing listed yet, and how to start a thread when one can be. */
export const NoThreadsNote: React.FC<{ hint?: string | null }> = ({ hint }) => (
  <p
    data-testid="side-threads-empty"
    className="px-3 text-[11px] text-[var(--g5-text2)]"
  >
    No comments on this file yet.{hint ? ` ${hint}` : ""}
  </p>
);

export const CommentColumn: React.FC<CommentColumnProps> = ({
  threads,
  onReply,
  onResolve,
  onUnresolve,
  onDelete,
  onThreadSelect,
  onOpenLine,
  activeThreadId = null,
  notice = null,
}) => {
  const [expandedThreadId, setExpandedThreadId] = useState<string | null>(null);

  useEffect(() => {
    if (activeThreadId != null) setExpandedThreadId(activeThreadId);
  }, [activeThreadId]);

  const handleToggle = (threadId: string) => {
    const willExpand = expandedThreadId !== threadId;
    setExpandedThreadId(willExpand ? threadId : null);
    onThreadSelect?.(threadId, willExpand);
  };

  return (
    <div
      data-testid="comment-column"
      className="flex min-w-[260px] w-full flex-col gap-2 px-2"
    >
      {notice && <CommentsNotice notice={notice} />}
      {threads.map((thread) => (
        <CommentThread
          key={thread.id}
          thread={thread}
          expanded={expandedThreadId === thread.id}
          active={activeThreadId === thread.id}
          onToggle={() => handleToggle(thread.id)}
          onReply={onReply}
          onResolve={onResolve}
          onUnresolve={onUnresolve}
          onDelete={onDelete}
          onOpenLine={onOpenLine}
        />
      ))}
    </div>
  );
};
