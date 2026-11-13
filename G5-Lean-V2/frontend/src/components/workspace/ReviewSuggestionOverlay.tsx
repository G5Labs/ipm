// =============================================================================
// G5 Workspace — ReviewSuggestionOverlay
// =============================================================================
// Review suggestions shown *in place* while reading a file of the source
// workspace: every open proposal from every reviewer, flat, at the line it
// targets. Three renderers share one data hook:
//   - Monaco (owner editing): view zones push the code apart under the
//     anchored line and hold the suggestion card (accept / edit / reject).
//   - Read-only code viewer: a gutter marker on the anchored line plus a
//     right-hand rail listing the cards in line order, with a jump link.
//   - Spec/doc editor: cards rendered after the block the line maps to.
// The Reviews panel on the Approvals tab remains the place for a
// side-by-side choice between competing proposals; here reading stays flat.
// =============================================================================

import React, { useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { classNames } from "../ui/UI";
import { useReviewSuggestions } from "../../hooks/useWorkspaceReview";
import type { ReviewSuggestion } from "../../lib/types";
import { findLineMatches, linePatternAt } from "../../lib/reviewLineSearch";
import { hunkDiffRowCount } from "./ReviewHunkDiff";
import type { SuggestionCardNodes } from "../../hooks/useSuggestionCardNodes";
import { SuggestionCard } from "./ReviewsPanel";
import { SuggestionCardSlot } from "./SuggestionCardPlaces";
import { useComposerRecords } from "../../hooks/useComposerStore";
import { isLaterSuggestion } from "../../lib/suggestionRecords";
import {
  REVIEW_EDIT_SOURCE_CONFLICT_NOTE,
  boundReviewSourceDigest,
  reviewEditSourceBasis,
  type ReviewSourceDigest,
  utf8OffsetToUtf16,
} from "../../lib/reviewSuggestionEdits";

/** What a file renderer needs to show suggestions in place. */
export interface SuggestionOverlayProps {
  /** The source workspace the suggestions belong to. */
  workspaceId: string;
  suggestions: ReviewSuggestion[];
  viewerOwnsWorkspace: boolean;
  viewerUserId: string;
  ownerVmRunning: boolean;
  /**
   * The source accepts thread messages and decisions; a read-only,
   * freezing or Base source offers neither, and neither does one whose
   * state is not known (`sourceThreadWrites`).
   */
  sourceWritable?: boolean;
  /** Why the source takes no replies, shown where the reply box would be. */
  sourceClosedNote?: string;
  /**
   * Set on a review copy: the file shown is the copy's, the suggestions are
   * the source's (named here), published by every reviewer.
   */
  onCopyOf?: string;
  /**
   * Suggestions from an earlier commit of the source, kept on the file
   * while they are being answered or decided (`useFileSuggestions`).
   */
  earlierCommitIds?: ReadonlySet<string>;
  /** The source's name, for a card from an earlier commit. */
  sourceName?: string;
  /** Exact displayed raw text and its independently computed digest. */
  selectedSource?: ReviewSourceDigest;
}

/**
 * Whether the viewer published `suggestion`: its reviewer may withdraw it
 * from the card in the file too (`g5.governance.review-suggestions.display`).
 */
export function viewerPublishedSuggestion(
  overlay: SuggestionOverlayProps,
  suggestion: ReviewSuggestion,
): boolean {
  return (
    !!overlay.viewerUserId && suggestion.reviewerUserId === overlay.viewerUserId
  );
}

/** The open suggestions on one file, and why they may be incomplete. */
export interface FileSuggestions {
  suggestions: ReviewSuggestion[];
  /** Set when they could not be loaded, or the last refresh failed. */
  problem: string | null;
  /** Those kept from an earlier commit while being answered or decided. */
  earlierCommitIds: ReadonlySet<string>;
}

const NO_IDS: ReadonlySet<string> = new Set();

/**
 * Open suggestions on one file of `workspaceId`, ordered by target line. A
 * failed refresh keeps the cards already shown, and what is typed in them.
 * A suggestion whose composers hold something (`keep`: a reply, a reason, an
 * edit, a decision) stays until they settle: decided, showing its decision,
 * or, once a commit of the source takes it out of the current list, as last
 * seen, marked as from an earlier commit (`g5.documents.comments.failure`).
 */
export function useFileSuggestions(
  workspaceId: string,
  repoId: string,
  path: string,
  enabled: boolean,
  keep?: ReadonlySet<string>,
): FileSuggestions {
  // A file page shows what is about the source's current commit only:
  // after the owner commits again, earlier rounds' rows leave the file
  // (on the owner's page and in every copy) and stay on the Reviews page.
  // Every status is asked for, so a decided card being answered can stay.
  const query = useReviewSuggestions(
    workspaceId,
    { repoId, path, currentOnly: true },
    enabled && !!workspaceId && !!repoId && !!path,
  );
  const loaded = query.status === "ok" ? query.data.suggestions : null;
  // The page keeps the record of each suggestion being answered or decided
  // while it is listed, so a card a commit takes out of the list is shown
  // again here, after a tab switch too. Writing never runs this again:
  // `remember` stays the same.
  // A decision never gives way to a list read before it.
  const { get, remember } = useComposerRecords<ReviewSuggestion>(
    "suggestion",
    isLaterSuggestion,
  );
  useEffect(() => {
    for (const suggestion of loaded ?? []) {
      if (keep?.has(suggestion.id)) remember(suggestion.id, suggestion);
    }
  }, [loaded, keep, remember]);
  // Kept cards that left the current list: their record belongs to this file.
  const keptAway = useMemo(() => {
    if (!loaded || !keep) return [];
    const listed = new Set(loaded.map((suggestion) => suggestion.id));
    return [...keep].filter((id) => {
      if (listed.has(id)) return false;
      const last = get(id);
      return !!last && last.repoId === repoId && last.path === path;
    });
  }, [loaded, keep, get, repoId, path]);
  // Their status is read from every round on this file while any is kept,
  // so a decision made here or elsewhere shows on the card.
  const history = useReviewSuggestions(
    workspaceId,
    { repoId, path },
    enabled && keptAway.length > 0,
  );
  const everyRound = history.status === "ok" ? history.data.suggestions : null;
  // Applied when that list is read again, or another card is kept away;
  // not when a record changes, so a decision's own answer is not overwritten
  // by the list read before it.
  const keptAwayKey = keptAway.join("\u0000");
  useEffect(() => {
    const away = new Set(keptAwayKey.split("\u0000"));
    for (const suggestion of everyRound ?? []) {
      if (away.has(suggestion.id)) remember(suggestion.id, suggestion);
    }
  }, [everyRound, keptAwayKey, remember]);
  const earlier = useMemo(
    () =>
      keptAway
        .map((id) => get(id))
        .filter((last): last is ReviewSuggestion => !!last),
    [keptAway, get],
  );
  // Kept cards show what was last read of them; when reading the earlier
  // rounds fails, the notice says so (`g5.workspace.page.refresh`).
  const historyProblem =
    keptAway.length === 0
      ? null
      : history.status === "error"
        ? `Could not load the earlier rounds of this file's review suggestions: ${history.error.message}`
        : history.status === "ok" && history.refreshError
          ? `Could not refresh the earlier rounds of this file's review suggestions: ${history.refreshError.message}`
          : null;
  return useMemo(() => {
    if (query.status === "error") {
      return {
        suggestions: [],
        problem: `Could not load review suggestions: ${query.error.message}`,
        earlierCommitIds: NO_IDS,
      };
    }
    if (query.status !== "ok") {
      return { suggestions: [], problem: null, earlierCommitIds: NO_IDS };
    }
    return {
      suggestions: [
        ...query.data.suggestions.filter(
          (suggestion) =>
            suggestion.status === "open" || !!keep?.has(suggestion.id),
        ),
        ...earlier,
      ].sort(
        (a, b) =>
          (a.anchorStart ?? Number.MAX_SAFE_INTEGER) -
            (b.anchorStart ?? Number.MAX_SAFE_INTEGER) ||
          a.createdAt.localeCompare(b.createdAt),
      ),
      problem: query.refreshError
        ? `Could not refresh review suggestions: ${query.refreshError.message}`
        : historyProblem,
      earlierCommitIds:
        earlier.length === 0
          ? NO_IDS
          : new Set(earlier.map((suggestion) => suggestion.id)),
    };
  }, [keep, query, earlier, historyProblem]);
}

/** Suggestions grouped by the 1-based line they target. */
export function suggestionsByLine(
  suggestions: ReviewSuggestion[],
  content: string | null = null,
  selectedRawDigest?: string,
): Map<number, ReviewSuggestion[]> {
  const map = new Map<number, ReviewSuggestion[]>();
  for (const suggestion of suggestions) {
    const line =
      resolveDisplayAnchor(suggestion, content, selectedRawDigest).line ?? 1;
    const list = map.get(line) ?? [];
    list.push(suggestion);
    map.set(line, list);
  }
  return map;
}

/** Where a suggestion is shown in the file as displayed right now. */
export interface DisplayAnchor {
  /** 1-based line in the displayed content (null: no anchor at all). */
  line: number | null;
  /**
   * `anchored`: the replaced lines are at the published line;
   * `moved`: they were found once elsewhere (the file changed above them);
   * `ambiguous`: they occur several times and the published context does
   * not single one out (no context singles out nothing, even when one
   * occurrence is at the published line): shown at the published line,
   * marked, never as a match (`g5.governance.review-suggestions.display`);
   * `lost`: not found at all: shown at the published line, where accepting
   * will report the suggestion obsolete;
   * `unverified`: nothing recorded can place it (a comment without its
   * line's text, an insertion no published context pins down): shown at the
   * published line, marked.
   */
  state: "anchored" | "moved" | "ambiguous" | "lost" | "unverified";
  /** Current raw bytes establish the immutable captured coordinates. */
  capturedSource?: true;
}

function contentLines(text: string): string[] {
  const lines = text.split("\n");
  if (lines[lines.length - 1] === "") lines.pop();
  return lines;
}

/**
 * `anchorStart` names a line of the file the suggestion was published
 * against; the displayed file may have changed since (lines inserted above,
 * another suggestion accepted). The replaced block is looked up in the
 * displayed content so the card sits next to the code it is about.
 */
export function resolveDisplayAnchor(
  suggestion: ReviewSuggestion,
  content: string | null,
  selectedRawDigest?: string,
): DisplayAnchor {
  const published = suggestion.anchorStart;
  const basis = reviewEditSourceBasis(suggestion, selectedRawDigest);
  if (basis?.sourceConflict) return { line: published, state: "unverified" };
  if (published === null) return { line: null, state: "anchored" };
  // A hunk is placed by its replaced block, a comment by the line text it
  // was left on (`line_text`, kept in `beforeLines`); other kinds stay at
  // the published line.
  const relocatable =
    suggestion.kind === "hunk" || suggestion.kind === "comment";
  if (content === null || !relocatable) {
    return { line: published, state: "anchored" };
  }
  const lines = contentLines(content);
  const place = (start: number): DisplayAnchor => ({
    line: start + 1,
    state: start + 1 === published ? "anchored" : "moved",
  });
  const fallback = Math.max(1, Math.min(published, Math.max(1, lines.length)));
  const context = parseAnchorContext(suggestion.anchorContext);
  // A comment's text is its evidence: "" is a blank line, while no text at
  // all places nothing.
  const before =
    suggestion.kind === "comment"
      ? suggestion.beforeLines === null
        ? null
        : suggestion.beforeLines === ""
          ? [""]
          : contentLines(suggestion.beforeLines)
      : contentLines(basis?.source ?? suggestion.beforeLines ?? "");
  if (before === null) return { line: fallback, state: "unverified" };
  if (basis?.captured)
    return published >= 1 &&
      published <= lines.length + 1 &&
      linePatternAt(lines, before, published - 1)
      ? { line: published, state: "anchored", capturedSource: true }
      : { line: fallback, state: "unverified" };
  if (before.length === 0) {
    // A pure insertion has only its point: the published context must pin
    // it down, as acceptance requires once the file changed.
    if (
      selectedRawDigest &&
      selectedRawDigest === suggestion.contentHash &&
      published >= 1 &&
      published <= lines.length + 1
    )
      return place(published - 1);
    const points: number[] = [];
    if (context.leading.length + context.trailing.length > 0) {
      for (let start = 0; start <= lines.length; start += 1) {
        if (contextSurrounds(context, lines, start, 0)) points.push(start);
      }
    }
    return points.length === 1 && points[0] === published - 1
      ? place(points[0])
      : {
          line: fallback,
          state: points.length > 1 ? "ambiguous" : "unverified",
        };
  }
  const found = findLineMatches(lines, before);
  if (found.length === 0) return { line: fallback, state: "lost" };
  if (found.length === 1) return place(found[0]);
  // Several identical blocks (twins that may have swapped places): only the
  // published context around the block can single one out; matching the
  // old line number alone proves nothing. An empty context frames every
  // occurrence, so it never does.
  const framed = found.filter((start) =>
    contextSurrounds(context, lines, start, before.length),
  );
  if (framed.length === 1) return place(framed[0]);
  return { line: fallback, state: "ambiguous" };
}

interface AnchorContext {
  leading: string[];
  trailing: string[];
}

/** The `anchor_context` JSON a hunk was published with. */
/**
 * Whether the published context sits around `len` lines at `start`: the
 * leading lines end just before them and the trailing lines start just
 * after them. An empty side frames anything.
 */
function contextSurrounds(
  context: AnchorContext,
  lines: string[],
  start: number,
  len: number,
): boolean {
  return (
    linePatternAt(lines, context.leading, start - context.leading.length) &&
    linePatternAt(lines, context.trailing, start + len)
  );
}

export function parseAnchorContext(raw: string | null): AnchorContext {
  const empty: AnchorContext = { leading: [], trailing: [] };
  if (!raw) return empty;
  try {
    const value: unknown = JSON.parse(raw);
    if (!value || typeof value !== "object") return empty;
    // Only version 2 (lines outside the block) is trusted; version 1 held
    // the block's own edge rows and would frame every occurrence.
    if ((value as Record<string, unknown>).version !== 2) return empty;
    const pick = (key: "leading" | "trailing"): string[] => {
      const items = (value as Record<string, unknown>)[key];
      return Array.isArray(items)
        ? items.filter((item): item is string => typeof item === "string")
        : [];
    };
    return { leading: pick("leading"), trailing: pick("trailing") };
  } catch {
    return empty;
  }
}

/** Short note for a card whose anchor is not where it was published. */
export function displayAnchorNote(
  suggestion: ReviewSuggestion,
  anchor: DisplayAnchor,
): string | null {
  if (suggestion.reviewEdit?.sourceConflict && !anchor.capturedSource)
    return REVIEW_EDIT_SOURCE_CONFLICT_NOTE;
  if (anchor.state === "moved") {
    return `Moved from line ${suggestion.anchorStart} (the file changed above)`;
  }
  if (anchor.state === "ambiguous") {
    return `The replaced lines occur several times here (published at line ${suggestion.anchorStart}), and nothing published tells which one is meant`;
  }
  if (anchor.state === "lost") {
    return `The replaced lines are not in this file as shown (published at line ${suggestion.anchorStart}); accepting reports it obsolete`;
  }
  if (anchor.state === "unverified") {
    return `Not confirmed here: nothing recorded tells where it belongs now (published at line ${suggestion.anchorStart})`;
  }
  return null;
}

function lineCount(text: string | null): number {
  if (!text) return 0;
  const lines = text.split("\n");
  if (lines[lines.length - 1] === "") lines.pop();
  return lines.length;
}

/** Height reserved for one card in a Monaco view zone. */
/** The editor line a suggestion's zone follows (0 = before the first line). */
export function suggestionZoneAfterLine(
  suggestion: ReviewSuggestion,
  content: string | null = null,
  selectedRawDigest?: string,
): number {
  const anchor = resolveDisplayAnchor(suggestion, content, selectedRawDigest);
  const { line } = anchor;
  const basis = reviewEditSourceBasis(suggestion, selectedRawDigest);
  if (line === null) return 0;
  if (basis?.sourceConflict || (basis?.captured && !anchor.capturedSource))
    return content === null
      ? Math.max(0, line - 1)
      : Math.max(0, Math.min(line - 1, content.split("\n").length));
  if (suggestion.reviewEdit) {
    const source = basis!.source ?? "";
    const end = utf8OffsetToUtf16(source, basis!.end);
    if (end !== undefined) {
      const prefix = source.slice(0, end);
      const after =
        line + prefix.split("\n").length - 1 - (prefix.endsWith("\n") ? 1 : 0);
      return content === null
        ? Math.max(0, after)
        : Math.max(0, Math.min(after, content.split("\n").length));
    }
  }
  const replaced =
    suggestion.kind === "hunk"
      ? lineCount(suggestion.beforeLines)
      : suggestion.kind === "comment"
        ? 1
        : 0;
  const after = Math.max(0, line + replaced - 1);
  // A block that vanished near the end of a shorter file would place the
  // zone past the last row, where nothing renders it: the last row hosts
  // it instead.
  if (content === null) return after;
  return Math.min(after, content.split("\n").length);
}

export function suggestionZoneHeight(suggestion: ReviewSuggestion): number {
  const diffRows =
    suggestion.kind === "hunk"
      ? hunkDiffRowCount(
          suggestion.reviewEdit?.beforeText ?? suggestion.beforeLines,
          suggestion.reviewEdit?.afterText ?? suggestion.afterLines,
        )
      : suggestion.kind === "new_file"
        ? Math.min(12, lineCount(suggestion.afterLines))
        : 0;
  const noteRows = suggestion.note ? 2 : 0;
  return Math.min(420, 72 + diffRows * 20 + noteRows * 20);
}

// ---------------------------------------------------------------------------
// Rail (read-only viewer)
// ---------------------------------------------------------------------------

/** A suggestion's card as shown under its lines: the anchor note, then the card. */
export const SuggestionZoneContent: React.FC<{
  overlay: SuggestionOverlayProps;
  suggestion: ReviewSuggestion;
  anchor: DisplayAnchor;
}> = ({ overlay, suggestion, anchor }) => {
  const note = displayAnchorNote(suggestion, anchor);
  return (
    <div className="border-y border-[var(--g5-warn)]/50 bg-[var(--g5-bg1)] text-left font-sans">
      {overlay.onCopyOf && (
        <p
          className="px-3 pt-1 text-[11px] text-[var(--g5-text2)]"
          data-testid="copy-file-proposals-hint"
        >
          Published proposals on {overlay.onCopyOf} for this file, from every
          reviewer
        </p>
      )}
      {note && (
        <p
          className="px-3 pt-1 text-[11px] text-[var(--g5-warn)]"
          data-testid={`suggestion-anchor-note-${suggestion.id}`}
        >
          {note}
        </p>
      )}
      <SuggestionCard
        workspaceId={overlay.workspaceId}
        suggestion={suggestion}
        viewerOwnsWorkspace={overlay.viewerOwnsWorkspace}
        viewerIsReviewer={viewerPublishedSuggestion(overlay, suggestion)}
        ownerVmRunning={overlay.ownerVmRunning}
        sourceWritable={overlay.sourceWritable}
        sourceClosedNote={overlay.sourceClosedNote}
        outdatedOf={
          overlay.earlierCommitIds?.has(suggestion.id)
            ? (overlay.sourceName ?? overlay.onCopyOf ?? "the source")
            : null
        }
        compact
      />
    </div>
  );
};

// ---------------------------------------------------------------------------
// Cards that keep their state when they move
// ---------------------------------------------------------------------------

// `SuggestionCardPortals` renders each card once and every place adopts it
// through `SuggestionCardSlot` (`SuggestionCardPlaces.tsx`).

/**
 * The suggestions shown under one line of the read-only viewer, the same
 * cards Monaco shows in its view zones. The row reports its rendered
 * height so the line gutter and every line-based position (scroll targets,
 * comment overlays) can make room for it.
 */
export const ReadOnlySuggestionZone: React.FC<{
  overlay: SuggestionOverlayProps;
  suggestions: ReviewSuggestion[];
  content: string | null;
  afterLine: number;
  onHeight: (afterLine: number, height: number) => void;
  /** When given, the cards are the host's, kept when their line moves. */
  cardNodes?: SuggestionCardNodes;
}> = ({ overlay, suggestions, content, afterLine, onHeight, cardNodes }) => {
  const ref = useRef<HTMLDivElement | null>(null);
  useEffect(() => {
    const node = ref.current;
    if (!node) return;
    const report = () =>
      onHeight(afterLine, Math.ceil(node.getBoundingClientRect().height));
    report();
    const observer =
      typeof ResizeObserver === "undefined" ? null : new ResizeObserver(report);
    observer?.observe(node);
    return () => {
      observer?.disconnect();
      onHeight(afterLine, 0);
    };
  }, [afterLine, onHeight]);
  return (
    <div
      ref={ref}
      data-testid={`suggestion-zone-after-${afterLine}`}
      // Sticky on the left: long code lines scroll sideways, the cards stay.
      className="sticky left-0 w-full whitespace-normal"
      data-review-suggestions=""
    >
      {suggestions.map((suggestion) => (
        <div
          key={suggestion.id}
          data-testid={`suggestion-zone-${suggestion.id}`}
        >
          {cardNodes ? (
            <SuggestionCardSlot
              nodes={cardNodes}
              suggestionId={suggestion.id}
            />
          ) : (
            <SuggestionZoneContent
              overlay={overlay}
              suggestion={suggestion}
              anchor={resolveDisplayAnchor(
                suggestion,
                content,
                boundReviewSourceDigest(overlay.selectedSource, content),
              )}
            />
          )}
        </div>
      ))}
    </div>
  );
};

/**
 * The suggestions of a file as a plain list, for the editable textarea
 * fallback (Monaco unavailable): no line rows to sit under, so each card
 * names its line instead.
 */
export const FallbackSuggestionList: React.FC<{
  overlay: SuggestionOverlayProps;
  content: string | null;
  /** When given, the cards are the host's, kept when the editor changes. */
  cardNodes?: SuggestionCardNodes;
  /** The surrounding sidebar owns height and scrolling. */
  layout?: "below-editor" | "sidebar";
}> = ({ overlay, content, cardNodes, layout = "below-editor" }) => {
  if (overlay.suggestions.length === 0) return null;
  return (
    <aside
      data-testid="suggestion-fallback-list"
      className={classNames(
        "shrink-0 border-t border-[var(--g5-stroke)] bg-[var(--g5-bg1)]",
        layout === "below-editor" && "max-h-[40%] overflow-y-auto",
      )}
      data-review-suggestions=""
    >
      <p className="px-3 py-2 text-xs font-medium text-[var(--g5-text1)]">
        {overlay.suggestions.length === 1
          ? "1 suggestion in this file"
          : `${overlay.suggestions.length} suggestions in this file`}
      </p>
      <ul className="divide-y divide-[var(--g5-stroke)]">
        {overlay.suggestions.map((suggestion) => {
          const anchor = resolveDisplayAnchor(
            suggestion,
            content,
            boundReviewSourceDigest(overlay.selectedSource, content),
          );
          return (
            <li
              key={suggestion.id}
              data-testid={`suggestion-fallback-${suggestion.id}`}
            >
              {anchor.line !== null && (
                <p className="px-3 pt-2 text-[11px] text-[var(--g5-text2)]">
                  Line {anchor.line}
                </p>
              )}
              {cardNodes ? (
                <SuggestionCardSlot
                  nodes={cardNodes}
                  suggestionId={suggestion.id}
                />
              ) : (
                <SuggestionZoneContent
                  overlay={overlay}
                  suggestion={suggestion}
                  anchor={anchor}
                />
              )}
            </li>
          );
        })}
      </ul>
    </aside>
  );
};

/** Suggestions by the line their zone follows (`suggestionZoneAfterLine`). */
export function suggestionZonesByLine(
  suggestions: ReviewSuggestion[],
  content: string | null,
  selectedRawDigest?: string,
): Map<number, ReviewSuggestion[]> {
  const map = new Map<number, ReviewSuggestion[]>();
  for (const suggestion of suggestions) {
    const after = suggestionZoneAfterLine(
      suggestion,
      content,
      selectedRawDigest,
    );
    const list = map.get(after) ?? [];
    list.push(suggestion);
    map.set(after, list);
  }
  return map;
}

/**
 * Split syntax-highlighted HTML into one HTML string per line. A span may
 * run across lines (a block comment, a string): it is closed at the end of
 * the line and reopened, with the same attributes, on the next, so every
 * line is well-formed on its own. Text is already escaped, so `<` only
 * starts a tag and a newline only occurs in text.
 */
export function splitHighlightedLines(html: string): string[] {
  const lines: string[] = [];
  const open: string[] = [];
  let current = "";
  let i = 0;
  while (i < html.length) {
    const char = html[i];
    if (char === "<") {
      const close = html.indexOf(">", i);
      if (close < 0) {
        current += html.slice(i);
        break;
      }
      const tag = html.slice(i, close + 1);
      if (tag.startsWith("</")) {
        open.pop();
      } else if (!tag.endsWith("/>")) {
        open.push(tag);
      }
      current += tag;
      i = close + 1;
      continue;
    }
    if (char === "\n") {
      lines.push(current + "</span>".repeat(open.length));
      current = open.join("");
      i += 1;
      continue;
    }
    let j = i;
    while (j < html.length && html[j] !== "<" && html[j] !== "\n") j += 1;
    current += html.slice(i, j);
    i = j;
  }
  lines.push(current + "</span>".repeat(open.length));
  return lines;
}

/** Gutter marker for a line that has suggestions. */
export const SuggestionGutterMarker: React.FC<{ count: number }> = ({
  count,
}) => (
  <span
    data-testid="suggestion-gutter-marker"
    className={classNames(
      "inline-flex h-2.5 w-2.5 items-center justify-center rounded-full",
      "bg-[var(--g5-warn)]",
    )}
    title={count === 1 ? "1 suggestion here" : `${count} suggestions here`}
    aria-label={count === 1 ? "1 suggestion" : `${count} suggestions`}
  />
);

// ---------------------------------------------------------------------------
// Monaco view zones
// ---------------------------------------------------------------------------

interface ZoneHandle {
  suggestionId: string;
  zoneId: string;
  node: HTMLDivElement;
  anchor: DisplayAnchor;
  /** The zone object Monaco holds; `heightInPx` is updated in place before `layoutZone`. */
  zone: { afterLineNumber: number; heightInPx: number; domNode: HTMLElement };
}

export interface MonacoSuggestionZonesProps {
  /** The mounted Monaco editor instance (any, the package is loaded lazily). */
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  editor: any;
  /** Bumped by the host when the editor mounts, so zones are re-added. */
  mountVersion: number;
  overlay: SuggestionOverlayProps;
  /**
   * When given, each zone adopts its suggestion's card from the host editor,
   * which renders every card once (`SuggestionCardPortals`): the card, and
   * what is typed or being sent in it, outlives the editor switching between
   * Monaco, its fallback and the read-only viewer.
   */
  cardNodes?: SuggestionCardNodes;
}

/**
 * One view zone per open suggestion, placed under the line it targets.
 * Zones are DOM nodes owned by Monaco; the cards are rendered into them
 * through portals so they stay ordinary React children of the page.
 */
export const MonacoSuggestionZones: React.FC<MonacoSuggestionZonesProps> = ({
  editor,
  mountVersion,
  overlay,
  cardNodes,
}) => {
  const [handles, setHandles] = useState<ZoneHandle[]>([]);
  const handlesRef = useRef<ZoneHandle[]>([]);

  // The editor's text decides where each card sits (the owner may have
  // typed above a suggestion): zones are re-placed shortly after edits.
  const [contentVersion, setContentVersion] = useState(0);
  useEffect(() => {
    if (!editor || typeof editor.onDidChangeModelContent !== "function") {
      return;
    }
    let timer: ReturnType<typeof setTimeout> | null = null;
    const disposable = editor.onDidChangeModelContent(() => {
      if (timer) clearTimeout(timer);
      timer = setTimeout(() => setContentVersion((v) => v + 1), 300);
    });
    return () => {
      if (timer) clearTimeout(timer);
      disposable?.dispose?.();
    };
  }, [editor, mountVersion]);

  // Zones live as long as the editor instance: they are removed when it
  // unmounts or remounts, never because the suggestion set changed.
  useEffect(() => {
    if (!editor || typeof editor.changeViewZones !== "function") return;
    return () => {
      if (typeof editor.changeViewZones !== "function") return;
      editor.changeViewZones(
        (accessor: { removeZone: (id: string) => void }) => {
          for (const handle of handlesRef.current) {
            accessor.removeZone(handle.zoneId);
          }
        },
      );
      handlesRef.current = [];
      setHandles([]);
    };
  }, [editor, mountVersion]);

  // Reconciled by suggestion id: a refetch that adds or drops a suggestion
  // touches only that zone. A card whose zone survives keeps its DOM
  // container, so the portal does not remount it and the owner's draft
  // (an edited replacement, a reject reason, a reply) stays.
  useEffect(() => {
    if (!editor || typeof editor.changeViewZones !== "function") return;
    const content: string | null =
      typeof editor.getModel === "function"
        ? (editor.getModel()?.getValue?.() ?? null)
        : null;
    const previous = new Map(
      handlesRef.current.map((handle) => [handle.suggestionId, handle]),
    );
    const next: ZoneHandle[] = [];
    editor.changeViewZones(
      (accessor: {
        addZone: (zone: unknown) => string;
        removeZone: (id: string) => void;
        layoutZone: (id: string) => void;
      }) => {
        const wanted = new Set(overlay.suggestions.map((s) => s.id));
        for (const handle of handlesRef.current) {
          if (!wanted.has(handle.suggestionId)) {
            accessor.removeZone(handle.zoneId);
          }
        }
        for (const suggestion of overlay.suggestions) {
          const kept = previous.get(suggestion.id);
          if (kept) {
            kept.zone.afterLineNumber = suggestionZoneAfterLine(
              suggestion,
              content,
              boundReviewSourceDigest(overlay.selectedSource, content),
            );
            kept.anchor = resolveDisplayAnchor(
              suggestion,
              content,
              boundReviewSourceDigest(overlay.selectedSource, content),
            );
            accessor.layoutZone(kept.zoneId);
            next.push(kept);
            continue;
          }
          const node = document.createElement("div");
          // Lifted above Monaco's text layer (index.css), so the card takes
          // the pointer.
          node.className = "g5-suggestion-zone";
          node.dataset.testid = `suggestion-zone-${suggestion.id}`;
          if (cardNodes) node.appendChild(cardNodes(suggestion.id));
          const zone = {
            // Monaco shows a zone after the given line: under the last line
            // of the replaced block, between the lines a pure insertion slips
            // into, and at the top of the file when there is no anchor.
            afterLineNumber: suggestionZoneAfterLine(
              suggestion,
              content,
              boundReviewSourceDigest(overlay.selectedSource, content),
            ),
            heightInPx: suggestionZoneHeight(suggestion),
            domNode: node,
          };
          const zoneId = accessor.addZone(zone);
          next.push({
            suggestionId: suggestion.id,
            zoneId,
            node,
            zone,
            anchor: resolveDisplayAnchor(
              suggestion,
              content,
              boundReviewSourceDigest(overlay.selectedSource, content),
            ),
          });
        }
      },
    );
    handlesRef.current = next;
    setHandles(next);
    // A refreshed row may change its source frame, surrounding context or
    // captured basis without changing its ID or current bounds. Observe the
    // immutable list rather than serializing shared source text per child.
    // Surviving handles keep their nodes, cards and drafts during relayout.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [editor, mountVersion, overlay.suggestions, overlay.selectedSource]);

  // The owner's edits move the zones, they do not recreate them: a new
  // zone node would remount the card inside it and drop what the owner had
  // in it (an edited replacement, a reject reason, an open discussion).
  // Each zone is re-placed in place under the line its suggestion now
  // targets and laid out again; the anchor note follows.
  useEffect(() => {
    if (contentVersion === 0) return;
    if (!editor || typeof editor.changeViewZones !== "function") return;
    const content: string | null =
      typeof editor.getModel === "function"
        ? (editor.getModel()?.getValue?.() ?? null)
        : null;
    const current = handlesRef.current;
    if (current.length === 0) return;
    editor.changeViewZones((accessor: { layoutZone: (id: string) => void }) => {
      for (const handle of current) {
        const suggestion = overlay.suggestions.find(
          (candidate) => candidate.id === handle.suggestionId,
        );
        if (!suggestion) continue;
        handle.zone.afterLineNumber = suggestionZoneAfterLine(
          suggestion,
          content,
          boundReviewSourceDigest(overlay.selectedSource, content),
        );
        handle.anchor = resolveDisplayAnchor(
          suggestion,
          content,
          boundReviewSourceDigest(overlay.selectedSource, content),
        );
        accessor.layoutZone(handle.zoneId);
      }
    });
    setHandles([...current]);
    // Runs for content changes only; the suggestions it reads are the ones
    // the zones were created for.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [editor, contentVersion]);

  // A card grows when the owner edits, rejects with a reason, opens the
  // discussion or hits an error: the zone follows the rendered height so
  // nothing is clipped inside the editor.
  useEffect(() => {
    if (!editor || typeof ResizeObserver === "undefined") return;
    const observers = handles.map((handle) => {
      // Monaco sizes the zone node itself to the height it was told, so it
      // never changes when the card inside grows; the portal's own root is
      // auto-sized by its content and is what to watch and measure.
      const card =
        (handle.node.firstElementChild as HTMLElement | null) ?? handle.node;
      const observer = new ResizeObserver(() => {
        const wanted = Math.ceil(card.getBoundingClientRect().height);
        if (wanted > 0 && wanted !== handle.zone.heightInPx) {
          handle.zone.heightInPx = wanted;
          editor.changeViewZones(
            (accessor: { layoutZone: (id: string) => void }) => {
              accessor.layoutZone(handle.zoneId);
            },
          );
        }
      });
      observer.observe(card);
      return observer;
    });
    return () => {
      for (const observer of observers) observer.disconnect();
    };
  }, [editor, handles]);

  // Adopted cards are rendered by the host editor.
  if (cardNodes) return null;
  return (
    <>
      {handles.map((handle) => {
        const suggestion = overlay.suggestions.find(
          (candidate) => candidate.id === handle.suggestionId,
        );
        if (!suggestion) return null;
        return createPortal(
          <SuggestionZoneContent
            overlay={overlay}
            suggestion={suggestion}
            anchor={handle.anchor}
          />,
          handle.node,
          handle.suggestionId,
        );
      })}
    </>
  );
};

// ---------------------------------------------------------------------------
// Doc blocks
// ---------------------------------------------------------------------------

export interface BlockSuggestionsProps {
  overlay: SuggestionOverlayProps;
  suggestions: ReviewSuggestion[];
  /**
   * The displayed text. A card not found at its published place (moved,
   * lost or ambiguous) says so, as in the code view, and is never shown
   * as a match (`g5.governance.review-suggestions.display`).
   */
  content?: string | null;
  /** When given, the cards are the host's, kept when they change place. */
  cardNodes?: SuggestionCardNodes;
}

/** One suggestion as shown with a document's blocks: its note, then its card. */
export const BlockSuggestionItem: React.FC<{
  overlay: SuggestionOverlayProps;
  suggestion: ReviewSuggestion;
  content: string | null;
}> = ({ overlay, suggestion, content }) => {
  const note =
    content === null
      ? null
      : displayAnchorNote(
          suggestion,
          resolveDisplayAnchor(
            suggestion,
            content,
            boundReviewSourceDigest(overlay.selectedSource, content),
          ),
        );
  return (
    <>
      {note && (
        <p
          className="px-3 pt-1 text-[11px] text-[var(--g5-warn)]"
          data-testid={`suggestion-anchor-note-${suggestion.id}`}
        >
          {note}
        </p>
      )}
      <SuggestionCard
        workspaceId={overlay.workspaceId}
        suggestion={suggestion}
        viewerOwnsWorkspace={overlay.viewerOwnsWorkspace}
        viewerIsReviewer={viewerPublishedSuggestion(overlay, suggestion)}
        ownerVmRunning={overlay.ownerVmRunning}
        sourceWritable={overlay.sourceWritable}
        sourceClosedNote={overlay.sourceClosedNote}
        outdatedOf={
          overlay.earlierCommitIds?.has(suggestion.id)
            ? (overlay.sourceName ?? overlay.onCopyOf ?? "the source")
            : null
        }
        compact
      />
    </>
  );
};

/** Suggestion cards rendered after a document block, read-only rows. */
export const BlockSuggestions: React.FC<BlockSuggestionsProps> = ({
  overlay,
  suggestions,
  content = null,
  cardNodes,
}) => {
  if (suggestions.length === 0) return null;
  return (
    <div
      data-testid="block-suggestions"
      // The host editor's document-history shortcuts skip anything in here.
      data-review-suggestions=""
      // Review metadata and diff text are not canonical comment source text.
      data-comment-connector-ignore=""
      className="my-1 space-y-1 rounded-[var(--g5-radius-sm)] border border-[var(--g5-warn)]/50 bg-[var(--g5-bg1)]"
      contentEditable={false}
    >
      {suggestions.map((suggestion) => (
        <div key={suggestion.id}>
          {cardNodes ? (
            <SuggestionCardSlot
              nodes={cardNodes}
              suggestionId={suggestion.id}
            />
          ) : (
            <BlockSuggestionItem
              overlay={overlay}
              suggestion={suggestion}
              content={content}
            />
          )}
        </div>
      ))}
    </div>
  );
};
