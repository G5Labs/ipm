// =============================================================================
// G5 Shared Components — anchored comment threads in an editor
// =============================================================================
// What DocumentPreview and CodeEditor share about anchored comments: the draft an
// editor hands the page, where each stored thread sits in the displayed text,
// and the thread card built from a stored comment.
// =============================================================================

import { useEffect, useMemo, useState } from "react";
import { useComposerRecords } from "../../hooks/useComposerStore";
import {
  originalCommentAnchor,
  resolveCommentAnchors,
  type CommentAnchorIdentity,
  type CommentAnchorResolution,
} from "../../lib/commentAnchors";
import type { CommentDTO } from "../../lib/types";
import { placedLine, type CommentThreadProps } from "./CommentThread";

/** A new thread an editor asks the page to start. */
export interface EditorCommentDraft {
  /** Stable across retries of this composer after a lost response. */
  id?: string;
  /** Built from exactly the text the editor displays, unsaved edits included. */
  anchor: CommentAnchorIdentity;
  content: string;
  /** Repository object the displayed text started from, when known. */
  baseOid: string | null;
}

/** A stored thread as shown. */
export type ShownComment = CommentDTO & {
  /**
   * Deleted elsewhere while its reply box here held words or a send: shown
   * as last seen, with nothing more sent, until those settle.
   */
  deleted?: boolean;
};

/**
 * `comments` as shown. A thread in `answering` (its reply box holds words or
 * a send) that leaves the list, because it was deleted elsewhere, stays as
 * last listed and marked deleted until its reply box settles, so the words
 * and the refusal stay reachable (`g5.documents.comments.failure`). The
 * record is kept by the page's owner, so a tab switch does not lose it;
 * `belongs` says which kept threads this view shows (its file or node).
 */
export function useCommentsKeptWhileAnswered(
  comments: CommentDTO[],
  answering: ReadonlySet<string>,
  belongs: (comment: CommentDTO) => boolean,
): ShownComment[] {
  const { get, remember } = useComposerRecords<CommentDTO>("thread");
  // The page keeps the record of each thread being answered while it is
  // listed. Writing never runs this again: `remember` stays the same.
  useEffect(() => {
    for (const comment of comments) {
      if (answering.has(comment.id)) remember(comment.id, comment);
    }
  }, [comments, answering, remember]);
  return useMemo(() => {
    const listed = new Set(comments.map((comment) => comment.id));
    const deleted: ShownComment[] = [];
    for (const id of answering) {
      if (listed.has(id)) continue;
      const last = get(id);
      if (last && belongs(last)) deleted.push({ ...last, deleted: true });
    }
    return deleted.length === 0 ? comments : [...comments, ...deleted];
  }, [comments, answering, get, belongs]);
}

/**
 * Starts a thread. Editors pass it only to a viewer who may start one; a
 * rejection is shown in the draft, which keeps its text
 * (`g5.documents.comments.controls`, `.failure`).
 */
export type AddCommentHandler = (draft: EditorCommentDraft) => Promise<unknown>;

/**
 * Where each comment sits in `content`, by comment id. One text version is
 * indexed once for all comments, and the previous placements stay until the
 * next ones are ready, so editing never flashes threads as orphaned.
 */
export function useCommentAnchorResolutions(
  content: string,
  comments: readonly CommentDTO[],
): ReadonlyMap<string, CommentAnchorResolution> {
  const [resolutions, setResolutions] = useState<
    ReadonlyMap<string, CommentAnchorResolution>
  >(() => new Map());

  useEffect(() => {
    // Nothing to place: the text is not read at all.
    if (comments.length === 0) return;
    let cancelled = false;
    void resolveCommentAnchors(content, comments).then((placed) => {
      if (cancelled) return;
      setResolutions(
        new Map(comments.map((comment, index) => [comment.id, placed[index]])),
      );
    });
    return () => {
      cancelled = true;
    };
  }, [comments, content]);

  return resolutions;
}

/**
 * The card of a stored comment. `resolution` is where it sits in the
 * displayed text; while it is still being placed the card names the line it
 * was written on.
 */
export function commentThreadFromDTO(
  comment: ShownComment,
  resolution: CommentAnchorResolution | undefined,
): CommentThreadProps["thread"] {
  const original = originalCommentAnchor(comment);
  const lineNumber =
    resolution === undefined
      ? (comment.lineNumber ?? 0)
      : resolution.status === "mapped"
        ? resolution.lineNumber
        : 0;
  return {
    id: comment.id,
    lineStart: lineNumber,
    lineEnd: lineNumber,
    anchorStatus: resolution?.status,
    originalLine: original.lineNumber,
    originalLineText: original.lineText,
    resolved: comment.resolved,
    deleted: comment.deleted,
    replyCount: comment.replyCount,
    comments: [
      {
        author: comment.authorName,
        avatar: comment.authorAvatarUrl || "",
        text: comment.content,
        time: comment.createdAt,
      },
    ],
  };
}

/**
 * `comments` as shown beside a view of `content`: each placed in the text,
 * in the text's order (`orderBesideView`). A row stored without a line
 * anchor needs no placing.
 */
export function useThreadsBesideView(
  content: string,
  comments: readonly CommentDTO[],
): CommentThreadProps["thread"][] {
  const resolutions = useCommentAnchorResolutions(content, comments);
  return useMemo(
    () =>
      orderBesideView(
        comments.map((comment) => ({
          thread: commentThreadFromDTO(
            comment,
            resolutions.get(comment.id) ??
              (comment.lineHash ? undefined : { status: "unanchored" }),
          ),
          createdAt: comment.createdAt,
        })),
      ).map(({ thread }) => thread),
    [comments, resolutions],
  );
}

/** A thread as listed beside a view, with the time it was started. */
export type ListedThread = {
  thread: CommentThreadProps["thread"];
  createdAt: string;
};

function createdAtMs(entry: ListedThread): number {
  const value = Date.parse(entry.createdAt);
  return Number.isNaN(value) ? 0 : value;
}

/**
 * The order of a file's threads in the side list, beside Source and Preview
 * alike: threads placed on a line read with the text, by line then creation
 * time; threads that cannot be placed follow in creation order. The API
 * lists newest first; that order is not kept
 * (`g5.documents.comments.identity`).
 */
export function orderBesideView<T extends ListedThread>(
  entries: readonly T[],
): T[] {
  return [...entries].sort((a, b) => {
    const lineA = placedLine(a.thread);
    const lineB = placedLine(b.thread);
    if (lineA !== null && lineB !== null && lineA !== lineB)
      return lineA - lineB;
    if ((lineA === null) !== (lineB === null)) return lineA === null ? 1 : -1;
    return createdAtMs(a) - createdAtMs(b);
  });
}
