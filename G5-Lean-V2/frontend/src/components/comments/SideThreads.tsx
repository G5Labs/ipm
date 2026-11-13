// =============================================================================
// SideThreads — a file's or node's threads beside a view without line rows
// =============================================================================
// A rendered HTML or Markdown document lists its threads and suggestion cards
// beside the view, in the same list and order as plain text's margin
// (`orderBesideView`), whenever Comments is on, even with nothing to list.
// Each thread names the line it is on (or was written on, or says it is not
// anchored); a placed thread's line label opens that line in plain text, and
// expanding its card shows the line's rendering. A new thread started from
// the view (a selection's Comment bubble, a record's Comment action) is
// written first in the list, the page staying as it is; threads are answered
// and decided as the viewer may (`g5.documents.comments.identity`,
// `g5.documents.comments.ambiguity`, `g5.governance.review-suggestions.display`).
// =============================================================================

import React from "react";
import type { CommentDTO } from "../../lib/types";
import {
  FallbackSuggestionList,
  type SuggestionOverlayProps,
} from "../workspace/ReviewSuggestionOverlay";
import { CommentColumn, CommentsNotice, NoThreadsNote } from "./CommentColumn";
import {
  CommentDraftCard,
  type CommentDraftCardProps,
} from "./CommentDraftCard";
import { placedLine, type CommentThreadAction } from "./CommentThread";
import { useThreadsBesideView } from "./commentThreadModel";

/** A file's new-thread draft, written first in the list, held by the host. */
export type SideThreadsDraft = CommentDraftCardProps;

export interface SideThreadsProps {
  /** Heading of the list, e.g. "Comments on this file". */
  title: string;
  testId: string;
  /** Whether Comments is on: off shows only the view. */
  shown?: boolean;
  comments: readonly CommentDTO[];
  /** The draft of a new thread, hosted first in the list when given. */
  draft?: SideThreadsDraft | null;
  /** How to start a thread, shown with the empty list when one can be. */
  emptyHint?: string | null;
  /** The source text threads are placed in; null when there is none. */
  content: string | null;
  /** Why the threads shown may be missing or out of date. */
  notice: string | null;
  suggestionOverlay?: SuggestionOverlayProps | null;
  onReply?: CommentThreadAction<[threadId: string, content: string]>;
  onResolve?: CommentThreadAction<[threadId: string]>;
  onUnresolve?: CommentThreadAction<[threadId: string]>;
  onDelete?: CommentThreadAction<[threadId: string]>;
  /** A card placed on a line was expanded: show that line in the view. */
  onRevealLine?: (line: number) => void;
  /** A placed card's line label was activated: open that line in plain text. */
  onOpenLine?: (line: number) => void;
  /** The view itself. */
  children: React.ReactNode;
}

export const SideThreads: React.FC<SideThreadsProps> = ({
  title,
  testId,
  shown = true,
  comments,
  draft = null,
  emptyHint = null,
  content,
  notice,
  suggestionOverlay = null,
  onReply,
  onResolve,
  onUnresolve,
  onDelete,
  onRevealLine,
  onOpenLine,
  children,
}) => {
  const threads = useThreadsBesideView(content ?? "", comments);
  const revealThread = (threadId: string, expanded: boolean) => {
    if (!expanded || !onRevealLine) return;
    const thread = threads.find((listed) => listed.id === threadId);
    const line = thread ? placedLine(thread) : null;
    if (line !== null) onRevealLine(line);
  };
  const cards = suggestionOverlay?.suggestions.length ?? 0;
  const empty = threads.length === 0 && cards === 0 && !notice && !draft?.draft;
  // The view keeps the same place whether or not the list shows, so
  // turning Comments on (from the bubble, say) leaves it, its scroll and its
  // state as they are.
  return (
    <div className="flex h-full min-h-0">
      <div className="min-w-0 flex-1 overflow-auto">{children}</div>
      {shown && (
        <aside
          data-testid={testId}
          className="flex h-full min-h-0 w-[300px] shrink-0 flex-col overflow-y-auto border-l border-[var(--g5-stroke)] py-2"
        >
          <p className="px-3 pb-2 text-xs font-medium text-[var(--g5-text1)]">
            {title}
          </p>
          {notice && (
            <div className="px-2 pb-2">
              <CommentsNotice notice={notice} />
            </div>
          )}
          {draft && <CommentDraftCard {...draft} />}
          {empty && <NoThreadsNote hint={emptyHint} />}
          {threads.length > 0 && (
            <CommentColumn
              threads={threads}
              onReply={onReply}
              onResolve={onResolve}
              onUnresolve={onUnresolve}
              onDelete={onDelete}
              onThreadSelect={onRevealLine ? revealThread : undefined}
              onOpenLine={onOpenLine}
            />
          )}
          {suggestionOverlay && (
            <FallbackSuggestionList
              overlay={suggestionOverlay}
              content={content}
              layout="sidebar"
            />
          )}
        </aside>
      )}
    </div>
  );
};
