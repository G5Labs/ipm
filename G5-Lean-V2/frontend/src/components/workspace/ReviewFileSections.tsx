// =============================================================================
// ReviewFileSections — a file's open changes by paragraph, across reviews
// =============================================================================
// The Reviews panel shows the current commit's open text changes to a
// Markdown file as the document does: each edited paragraph once, every
// reviewer's changes marked in it, the paragraphs around it for context and
// the decision cards under it, a connected set of conflicting changes as one
// box (`g5.governance.review.items`, `g5.documents.review-placement`,
// `g5.documents.review-alternatives`). The section reads the file, binds its
// digest and runs the document's projection; the cards are the panel's, under
// the same ids. While the file loads, or when it cannot be read, each change
// is shown alone on its card, with a note.
// =============================================================================

import React, { useEffect, useMemo, useState } from "react";
import { useWorkspaceFileContent } from "../../hooks/useFiles";
import { useReviewSourceDigest } from "../../hooks/useReviewSourceDigest";
import type { SuggestionCardNodes } from "../../hooks/useSuggestionCardNodes";
import { formatDateTime } from "../../lib/dateTime";
import type { ReviewSuggestion } from "../../lib/types";
import {
  blockToRawMarkdown,
  parseMarkdownToBlocks,
  type Block,
} from "../../lib/utils";
import {
  groupSpecReviews,
  projectSpecReviews,
  type SpecReviewProjection,
} from "../editor/specReviewProjection";
import {
  ReviewMarginItemSlot,
  SpecReviewSuggestions,
  reviewMarginItems,
} from "../editor/SpecReviewSuggestions";
import { lineOfOffset } from "../files/documentSource";
import { isMarkdownPath } from "../files/fileText";
import { MarkdownPreview } from "../files/MarkdownPreview";
import { resolveRepoLink } from "../files/repoLinks";
import type { SuggestionOverlayProps } from "./ReviewSuggestionOverlay";
import { SuggestionCardSlot } from "./SuggestionCardPlaces";

/** A file whose open changes the panel shows by paragraph. */
export interface ReviewFile {
  repoId: string;
  path: string;
  /** The open text changes to the file, every reviewer's. */
  suggestions: ReviewSuggestion[];
}

/**
 * The files shown by paragraph: those with open text changes to a Markdown
 * file from the given publications, in path order.
 */
export function reviewFileSections(
  suggestions: readonly ReviewSuggestion[],
  publicationIds: ReadonlySet<string>,
): ReviewFile[] {
  const files = new Map<string, ReviewFile>();
  for (const suggestion of suggestions) {
    if (
      suggestion.status !== "open" ||
      suggestion.kind !== "hunk" ||
      !suggestion.repoId ||
      !suggestion.path ||
      !isMarkdownPath(suggestion.path) ||
      !publicationIds.has(suggestion.publicationId)
    )
      continue;
    const key = `${suggestion.repoId}\u0000${suggestion.path}`;
    const file = files.get(key) ?? {
      repoId: suggestion.repoId,
      path: suggestion.path,
      suggestions: [],
    };
    file.suggestions.push(suggestion);
    files.set(key, file);
  }
  return [...files.values()].sort(
    (a, b) => a.path.localeCompare(b.path) || a.repoId.localeCompare(b.repoId),
  );
}

export interface ReviewFileSectionProps {
  workspaceId: string;
  file: ReviewFile;
  /** The reviewer's name, for the heading's summary and each card. */
  reviewerName: (publicationId: string) => string;
  /** The time a review was published. */
  publishedAt: (publicationId: string) => string | null;
  /** The cards' authority, as every card in the panel has it. */
  overlay: Omit<SuggestionOverlayProps, "suggestions" | "selectedSource">;
  /** The panel's card nodes: the fallback cards and the paragraphs' share them. */
  cardNodes: SuggestionCardNodes;
  /**
   * Opens a file of the workspace at a line: a paragraph's heading opens the
   * file at the paragraph, and a link in a paragraph opens what it names.
   */
  onOpenFile?: (filePath: string, repoRef?: string, line?: number) => void;
  /**
   * The section's state: "loading" while its file is read; afterwards a
   * value that changes whenever its cards may have moved (the file read
   * or failed, the digest that places the changes computed). Reported on
   * each change, and as null when the section leaves; a deep link waits for
   * the sections shown and follows their changes.
   */
  onSettled?: (key: string, state: string | null) => void;
}

/**
 * `items` as long as it holds the same objects: the panel rebuilds the list
 * every render, and the projection below is worth running only when a
 * suggestion actually changed.
 */
function useSameItems<T>(items: T[]): T[] {
  const [kept, setKept] = useState(items);
  const same =
    kept.length === items.length && kept.every((item, i) => item === items[i]);
  if (!same) setKept(items);
  return same ? kept : items;
}

export const ReviewFileSection: React.FC<ReviewFileSectionProps> = ({
  workspaceId,
  file,
  reviewerName,
  publishedAt,
  overlay,
  cardNodes,
  onOpenFile,
  onSettled,
}) => {
  // The read acceptance refreshes: an accepted change is in the text the
  // next projection runs against, so the paragraph shows it applied.
  const { state: content } = useWorkspaceFileContent(
    workspaceId,
    file.repoId,
    file.path,
  );
  const loaded = content.status === "ok" && content.data.encoding === "utf-8";
  const text = loaded ? content.data.content : "";
  const unreadable =
    content.status === "error"
      ? content.error.message
      : content.status === "ok" && !loaded
        ? "it is not a text file"
        : null;
  // The text read earlier stays shown when a refresh fails; the paragraphs
  // may then lag the file (an accepted change, say).
  const stale = loaded ? content.refreshError : null;
  const digest = useReviewSourceDigest(
    `${workspaceId}:${file.repoId}:${file.path}`,
    text,
    loaded,
    true,
  );
  const sectionKey = `${file.repoId}:${file.path}`;
  const sectionState =
    content.status === "loading"
      ? "loading"
      : content.status === "error"
        ? "error"
        : `ok:${content.data.hash}:${digest?.digest ?? "hashing"}`;
  useEffect(() => {
    onSettled?.(sectionKey, sectionState);
  }, [onSettled, sectionKey, sectionState]);
  useEffect(() => () => onSettled?.(sectionKey, null), [onSettled, sectionKey]);
  // Parsed once per text; nothing to parse while the file is not loaded.
  const blocks = useMemo(() => parseMarkdownToBlocks(text), [text]);
  const suggestions = useSameItems(file.suggestions);
  const projections = useMemo(
    () =>
      loaded
        ? projectSpecReviews(text, blocks, suggestions, digest?.digest)
        : [],
    [loaded, text, blocks, suggestions, digest?.digest],
  );
  const groups = useMemo(() => groupSpecReviews(projections), [projections]);
  const [activeItem, setActiveItem] = useState<string | null>(null);
  const previewProps = {
    filePath: file.path,
    workspaceId,
    repoId: file.repoId,
    onOpenWorkspaceFile: onOpenFile,
  };
  const renderBlock = (
    block: Block,
    inline?: (text: string) => React.ReactNode,
  ) => {
    if (inline) {
      if (block.type === "paragraph") return <p>{inline(block.content)}</p>;
      if (block.type === "heading")
        return React.createElement(
          `h${block.level}`,
          {},
          inline(block.content),
        );
      if (block.type === "blockquote")
        return <blockquote>{inline(block.content)}</blockquote>;
    }
    return (
      <MarkdownPreview {...previewProps} content={blockToRawMarkdown(block)} />
    );
  };
  // The reviews whose changes the section holds, each with its open count.
  const reviews = [...new Set(file.suggestions.map((s) => s.publicationId))];
  const open = file.suggestions.length;
  return (
    <section
      data-testid={`reviews-file-${file.repoId}:${file.path}`}
      data-state={sectionState}
      className="overflow-hidden rounded-[var(--g5-radius-md)] border border-[var(--g5-active)]/40 bg-[var(--g5-bg1)]"
    >
      <header className="flex flex-wrap items-center gap-x-3 gap-y-1 border-b border-[var(--g5-stroke)] px-4 py-3 text-xs text-[var(--g5-text2)]">
        <span className="font-mono text-sm text-[var(--g5-text0)]">
          {file.path}
        </span>
        <span className="rounded-full border border-[var(--g5-stroke)] bg-[var(--g5-bg2)] px-2 py-0.5">
          {open} open
        </span>
        <span className="ml-auto flex flex-wrap gap-x-3">
          {reviews.map((publicationId) => (
            <span key={publicationId}>
              {reviewerName(publicationId)}
              {publishedAt(publicationId)
                ? ` · ${formatDateTime(publishedAt(publicationId)!)}`
                : ""}{" "}
              ·{" "}
              {
                file.suggestions.filter(
                  (s) => s.publicationId === publicationId,
                ).length
              }{" "}
              open
            </span>
          ))}
        </span>
      </header>
      <div className="space-y-4 px-4 py-4">
        {content.status === "loading" && (
          <p
            data-testid="reviews-file-loading"
            className="text-xs text-[var(--g5-text2)]"
          >
            Reading the file…
          </p>
        )}
        {unreadable && (
          <p
            role="status"
            data-testid="reviews-file-unreadable"
            className="text-xs text-[var(--g5-warn)]"
          >
            The file could not be read ({unreadable}); each change is shown
            alone.
          </p>
        )}
        {stale && (
          <p
            role="status"
            data-testid="reviews-file-refresh-failed"
            className="text-xs text-[var(--g5-warn)]"
          >
            The file could not be refreshed ({stale.message}); the paragraphs
            show the text read earlier.
          </p>
        )}
        {!loaded && (
          <ul className="divide-y divide-[var(--g5-stroke)] rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)]">
            {file.suggestions.map((suggestion) => (
              <li key={suggestion.id}>
                <SuggestionCardSlot
                  nodes={cardNodes}
                  suggestionId={suggestion.id}
                />
              </li>
            ))}
          </ul>
        )}
        {loaded &&
          groups.map((group) => (
            <ParagraphGroup
              key={group[0].suggestion.id}
              group={group}
              text={text}
              blocks={blocks}
              file={file}
              overlay={{
                ...overlay,
                suggestions: [...suggestions],
                selectedSource: digest,
              }}
              renderBlock={renderBlock}
              cardNodes={cardNodes}
              activeItem={activeItem}
              onActivate={setActiveItem}
              onOpenRepoLink={(href) => {
                const target = onOpenFile
                  ? resolveRepoLink(file.path, href)
                  : null;
                if (!target) return false;
                onOpenFile!(target.path, file.repoId, target.line);
                return true;
              }}
              onOpenFile={
                onOpenFile
                  ? (line) =>
                      onOpenFile(file.path, file.repoId, line ?? undefined)
                  : undefined
              }
            />
          ))}
      </div>
    </section>
  );
};

const ParagraphGroup: React.FC<{
  group: SpecReviewProjection[];
  text: string;
  blocks: Block[];
  file: ReviewFile;
  overlay: SuggestionOverlayProps;
  renderBlock: (
    block: Block,
    inline?: (text: string) => React.ReactNode,
  ) => React.ReactNode;
  cardNodes: SuggestionCardNodes;
  activeItem: string | null;
  onActivate: (itemId: string) => void;
  /** Opens the file at the paragraph's line. */
  onOpenFile?: (line: number | null) => void;
  /** Opens a repository link in a paragraph rendered inline. */
  onOpenRepoLink?: (href: string) => boolean;
}> = ({
  group,
  text,
  blocks,
  file,
  overlay,
  renderBlock,
  cardNodes,
  activeItem,
  onActivate,
  onOpenFile,
  onOpenRepoLink,
}) => {
  const [projection, ...peers] = group;
  const covered = group.flatMap((p) =>
    p.attention
      ? []
      : [...p.coveredBlocks, ...(p.firstBlock === null ? [] : [p.firstBlock])],
  );
  const first = covered.length ? Math.min(...covered) : null;
  const last = covered.length ? Math.max(...covered) : null;
  // A change between blocks (whitespace at a separator or the end of the
  // file) covers none: the blocks either side of its boundary are its
  // context, and its own offset names its line.
  const boundary = group.find(
    (p) =>
      !p.attention && p.firstBlock === null && p.boundaryBlock !== undefined,
  )?.boundaryBlock;
  const before =
    first !== null
      ? first > 0
        ? blocks[first - 1]
        : undefined
      : boundary !== undefined && boundary > 0
        ? blocks[boundary - 1]
        : undefined;
  const after =
    last !== null
      ? blocks[last + 1]
      : boundary !== undefined
        ? blocks[boundary]
        : undefined;
  const start =
    first !== null
      ? blocks[first]?.sourceRange?.start
      : projection.sourcePreview?.start;
  const line =
    start !== undefined
      ? lineOfOffset(text, start)
      : projection.suggestion.anchorStart;
  return (
    <div
      data-testid={`reviews-paragraph-group-${projection.suggestion.id}`}
      className="rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] p-3"
    >
      <p className="mb-2 font-mono text-[11px] text-[var(--g5-text2)]">
        {file.path}
        {line !== null ? `:${line}` : ""}
        {onOpenFile && (
          <>
            {" · "}
            <button
              type="button"
              className="text-[var(--g5-info)] hover:underline"
              data-testid={`reviews-paragraph-open-${projection.suggestion.id}`}
              onClick={() => onOpenFile(line)}
            >
              Open file
            </button>
          </>
        )}
      </p>
      {before && (
        <div
          data-testid="reviews-paragraph-context"
          className="text-sm text-[var(--g5-text2)]"
        >
          {renderBlock(before)}
        </div>
      )}
      <SpecReviewSuggestions
        onOpenRepoLink={onOpenRepoLink}
        projection={projection}
        peers={peers}
        overlay={overlay}
        renderBlock={renderBlock}
        filtered={false}
        marginNodes={cardNodes}
        onActivate={onActivate}
      />
      {after && (
        <div
          data-testid="reviews-paragraph-context"
          className="text-sm text-[var(--g5-text2)]"
        >
          {renderBlock(after)}
        </div>
      )}
      <div className="mt-3 space-y-2" data-testid="reviews-paragraph-cards">
        {reviewMarginItems(group).map((item) => (
          <div
            key={item.id}
            data-margin-item={item.id}
            data-active={item.id === activeItem ? "true" : undefined}
          >
            <ReviewMarginItemSlot item={item} nodes={cardNodes} />
          </div>
        ))}
      </div>
    </div>
  );
};
