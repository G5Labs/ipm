import React, { type ReactNode } from "react";
import {
  blockToRawMarkdown,
  parseMarkdownToBlocks,
  renderInlineMarkdown,
  type Block,
} from "../../lib/utils";
import type { ReviewSuggestion } from "../../lib/types";
import { isQuestionFenceLanguage } from "../../lib/questionBlock";
import { findLineMatches, linePatternAt } from "../../lib/reviewLineSearch";
import { parseAnchorContext } from "../workspace/ReviewSuggestionOverlay";
import { splitHunkLines } from "../workspace/ReviewHunkDiff";
import {
  expandReviewSuggestionEdits,
  REVIEW_EDIT_SOURCE_CONFLICT_NOTE,
  reviewEditSourceBasis,
  utf8OffsetToUtf16,
} from "../../lib/reviewSuggestionEdits";

export interface SpecReviewProjection {
  suggestion: ReviewSuggestion;
  firstBlock: number | null;
  coveredBlocks: number[];
  before: Block[];
  after: Block[];
  /** Canonical indices for original rows; proposed-only rows have no identity. */
  beforeBlockIndices?: number[];
  attention: string | null;
  /** One of these conflicting proposals is previewed; compatible edits still compose. */
  alternativeIds?: string[];
  changedRange?: { start: number; end: number };
  /** Proven insertion coordinates relative to the parsed preview source. */
  sourceInsertion?: { start: number; length: number };
  emptyDocument?: boolean;
  /** Raw bytes and placement survive a lossy rich-text projection. */
  sourcePreview?: {
    content: string;
    start: number;
    end: number;
    replacement: string;
    rangeStart: number;
    rangeEnd: number;
  };
  sourceDiff?: boolean;
  /** Render before this canonical block (blocks.length means EOF). */
  boundaryBlock?: number;
  currentSource?: string;
}

/**
 * Projections at one location, together: a proposal joins the group of a
 * proposal it conflicts with, that starts at the same block, or that is
 * inserted at the same block boundary. Each group is one preview in the
 * document (`g5.documents.review-alternatives`); proposals needing attention
 * have no location and stay alone.
 */
export function groupSpecReviews(
  projections: readonly SpecReviewProjection[],
): SpecReviewProjection[][] {
  const result: SpecReviewProjection[][] = [];
  for (const p of projections) {
    const peer = result.find(
      (g) =>
        p.alternativeIds?.includes(g[0].suggestion.id) ||
        (p.firstBlock !== null && p.firstBlock === g[0].firstBlock) ||
        (p.firstBlock === null &&
          !p.attention &&
          !g[0].attention &&
          g[0].firstBlock === null &&
          p.boundaryBlock !== undefined &&
          p.boundaryBlock === g[0].boundaryBlock),
    );
    if (peer) peer.push(p);
    else result.push([p]);
  }
  return result;
}

/** A display projection only. The server still owns placement for acceptance. */
export function projectSpecReviews(
  content: string,
  blocks: ReturnType<typeof parseMarkdownToBlocks>,
  suggestions: ReviewSuggestion[],
  /** SHA256 of these exact raw UTF8 bytes, bound by the caller to current source. */
  rawContentDigest?: string,
): SpecReviewProjection[] {
  suggestions = expandReviewSuggestionEdits(suggestions);
  const selectedDigest =
    rawContentDigest ??
    (content === ""
      ? "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
      : undefined);
  if (!suggestions.some((suggestion) => suggestion.status === "open"))
    return [];
  const lines = splitHunkLines(content);
  const offsets = [0];
  for (const line of lines) offsets.push(offsets.at(-1)! + line.length + 1);
  const at = (line: number) =>
    Math.min(offsets[line] ?? content.length, content.length);
  const parentSizes = new Map<string, { count: number; context: number }>();
  for (const suggestion of suggestions) {
    if (!suggestion.editId || suggestion.status !== "open") continue;
    const parent = suggestion.parentSuggestionId ?? suggestion.id;
    const size = parentSizes.get(parent) ?? { count: 0, context: 0 };
    size.count++;
    size.context = Math.max(size.context, suggestion.beforeLines?.length ?? 0);
    parentSizes.set(parent, size);
  }
  const projections = suggestions
    .filter((s) => s.status === "open")
    .map((suggestion): SpecReviewProjection => {
      const unavailable = (attention: string): SpecReviewProjection => ({
        suggestion,
        firstBlock: null,
        coveredBlocks: [],
        before: [],
        after: [],
        attention,
        currentSource: content,
      });
      if (suggestion.kind !== "hunk")
        return unavailable("This review item has no inline text location.");
      const basis = reviewEditSourceBasis(suggestion, selectedDigest);
      if (basis?.sourceConflict)
        return unavailable(REVIEW_EDIT_SOURCE_CONFLICT_NOTE);
      const parentSize = parentSizes.get(
        suggestion.parentSuggestionId ?? suggestion.id,
      );
      if (
        suggestion.editId &&
        parentSize &&
        (parentSize.count > 128 || parentSize.context > 16 * 1024)
      )
        return unavailable(
          "This large review is shown as individual edits. Review each edit and its original context separately.",
        );
      const edit = suggestion.reviewEdit;
      const source = basis?.source ?? suggestion.beforeLines;
      const before = splitHunkLines(source);
      const after = splitHunkLines(suggestion.afterLines);
      const context = parseAnchorContext(suggestion.anchorContext);
      const anchorTrusted =
        selectedDigest != null && suggestion.contentHash === selectedDigest;
      const preferred =
        suggestion.anchorStart != null &&
        Number.isInteger(suggestion.anchorStart) &&
        suggestion.anchorStart >= 1 &&
        suggestion.anchorStart <= lines.length + 1
          ? suggestion.anchorStart - 1
          : null;
      const hasContext = context.leading.length + context.trailing.length > 0;
      // Accept refuses ambiguous already-proposed content, or reports an
      // insertion already applied. Do not promise an inline write in either case.
      if (
        !edit &&
        ((!anchorTrusted && findLineMatches(lines, after).length > 0) ||
          (anchorTrusted &&
            preferred != null &&
            after.length > 0 &&
            (before.length !== after.length ||
              !linePatternAt(before, after, 0)) &&
            linePatternAt(lines, after, preferred) &&
            (after.length >= before.length ||
              !linePatternAt(lines, before, preferred))))
      )
        return unavailable(
          "Needs attention: the proposed text already appears in this changed document. Review its original context.",
        );
      const matches = before.length ? findLineMatches(lines, before) : [];
      const framed = hasContext
        ? findLineMatches(lines, [
            ...context.leading,
            ...before,
            ...context.trailing,
          ]).map((i) => i + context.leading.length)
        : [];
      const candidates =
        before.length === 0
          ? preferred != null &&
            (anchorTrusted || (framed.length === 1 && framed[0] === preferred))
            ? [preferred]
            : framed.length > 1
              ? framed
              : []
          : anchorTrusted && preferred != null && matches.includes(preferred)
            ? [preferred]
            : framed.length === 1
              ? framed
              : matches;
      if (candidates.length !== 1)
        return unavailable(
          candidates.length > 1
            ? "Needs attention: the original text appears more than once."
            : "Needs attention: the original text or insertion context has changed.",
        );
      // Publication includes unchanged context. Only changed lines own a preview;
      // nearby semantic headers and other changes keep their ordinary block UI.
      let sharedStart = 0,
        sharedEnd = 0;
      while (
        !edit &&
        sharedStart < Math.min(before.length, after.length) &&
        before[sharedStart] === after[sharedStart]
      )
        sharedStart++;
      while (
        !edit &&
        sharedEnd < Math.min(before.length, after.length) - sharedStart &&
        before[before.length - 1 - sharedEnd] ===
          after[after.length - 1 - sharedEnd]
      )
        sharedEnd++;
      const changedBefore = edit
        ? splitHunkLines(edit.beforeText)
        : before.slice(sharedStart, before.length - sharedEnd);
      const changedAfter = edit
        ? splitHunkLines(edit.afterText)
        : after.slice(sharedStart, after.length - sharedEnd);
      if (!changedBefore.length && !changedAfter.length)
        return unavailable("The proposed text already matches this document.");
      const editStart = edit
        ? utf8OffsetToUtf16(source ?? "", basis!.start)
        : undefined;
      const editEnd = edit
        ? utf8OffsetToUtf16(source ?? "", basis!.end)
        : undefined;
      if (
        edit &&
        (editStart === undefined ||
          editEnd === undefined ||
          editStart > editEnd ||
          (source ?? "").slice(editStart, editEnd) !== edit.beforeText)
      )
        return unavailable(
          "Needs attention: this edit has no verified source range.",
        );
      const start = edit
        ? at(candidates[0]) + editStart!
        : at(candidates[0] + sharedStart);
      const end = edit
        ? at(candidates[0]) + editEnd!
        : at(candidates[0] + before.length - sharedEnd);
      if (
        edit &&
        (end > content.length || content.slice(start, end) !== edit.beforeText)
      )
        return unavailable(
          "Needs attention: this edit's original source has changed.",
        );
      const replacement = edit
        ? edit.afterText
        : (!changedBefore.length &&
          start === content.length &&
          start > 0 &&
          content[start - 1] !== "\n"
            ? "\n"
            : "") +
          changedAfter.join("\n") +
          (changedAfter.length &&
          (content[end - 1] === "\n" ||
            (!changedBefore.length &&
              (end < content.length || suggestion.afterLines?.endsWith("\n"))))
            ? "\n"
            : "");
      const rawCovered = blocks.flatMap((block, i) => {
        const range = block.sourceRange;
        const inlineInsertion =
          start === end &&
          replacement.length > 0 &&
          !replacement.includes("\n");
        return range &&
          ((range.start < end && range.start + range.length > start) ||
            (inlineInsertion &&
              range.start <= start &&
              range.start + range.length >= start))
          ? [i]
          : [];
      });
      const rawProjection = (
        rangeStart = rawCovered.length
          ? Math.min(start, blocks[rawCovered[0]].sourceRange!.start)
          : start,
        rangeEnd = rawCovered.length
          ? Math.max(
              end,
              blocks[rawCovered.at(-1)!].sourceRange!.start +
                blocks[rawCovered.at(-1)!].sourceRange!.length,
            )
          : end,
        placement = rawCovered,
        boundary?: number,
      ): SpecReviewProjection => {
        const following = blocks.findIndex(
          (block) => (block.sourceRange?.start ?? -1) >= end,
        );
        return {
          suggestion,
          firstBlock: placement[0] ?? null,
          coveredBlocks: placement,
          before: parseMarkdownToBlocks(content.slice(rangeStart, rangeEnd)),
          after: [],
          attention: null,
          changedRange: { start, end },
          sourceDiff: true,
          sourcePreview: {
            content,
            start,
            end,
            replacement,
            rangeStart,
            rangeEnd,
          },
          ...(placement.length
            ? {}
            : {
                boundaryBlock:
                  boundary ?? (following < 0 ? blocks.length : following),
              }),
          ...(blocks.length ? {} : { emptyDocument: true }),
        };
      };
      if (content === "" && !changedBefore.length) {
        const proposed = changedAfter.join("\n");
        const afterBlocks = parseMarkdownToBlocks(proposed);
        if (afterBlocks.some(isQuestionBlock))
          return unavailable(
            "Needs attention: this proposal includes a structured question. Review its original context.",
          );
        const lastRange = afterBlocks.at(-1)?.sourceRange;
        if (
          !afterBlocks.length ||
          (lastRange &&
            lastRange.start + lastRange.length !== proposed.length) ||
          hasHiddenSourceChange(
            "",
            proposed,
            [],
            afterBlocks,
            afterBlocks.map((block) => [undefined, block]),
          )
        )
          return rawProjection();
        return {
          suggestion,
          firstBlock: null,
          coveredBlocks: [],
          before: [],
          after: afterBlocks,
          beforeBlockIndices: [],
          attention: null,
          emptyDocument: true,
          changedRange: { start, end },
          sourcePreview: {
            content,
            start,
            end,
            replacement,
            rangeStart: 0,
            rangeEnd: 0,
          },
        };
      }
      // A whitespace-only change in a parser-omitted separator has its own
      // boundary, rather than borrowing a neighbouring block's identity.
      if (
        !rawCovered.length &&
        !content.slice(start, end).trim() &&
        !replacement.trim()
      )
        return rawProjection();
      const covered = [...rawCovered];
      const whitespaceOnly =
        !content.slice(start, end).trim() && !replacement.trim();
      const structuredWhitespace =
        whitespaceOnly &&
        rawCovered.some(
          (i) =>
            isQuestionBlock(blocks[i]) ||
            blocks.recordSpans.some((record) => {
              if (record.spanStart !== i) return false;
              const labelEnd = content.indexOf(
                "\n",
                blocks[i].sourceRange!.start,
              );
              return labelEnd < 0 || start <= labelEnd;
            }),
        );
      if (structuredWhitespace)
        return rawProjection(start, end, [], rawCovered[0]);
      let precedingFirstBody = false;
      if (
        !covered.length &&
        (edit ? edit.beforeText === "" : !changedBefore.length)
      ) {
        // Insertion into a separator/EOF is rendered with the adjacent full block.
        const following = blocks.findIndex(
          (b) => (b.sourceRange?.start ?? -1) >= start,
        );
        const previous = following - 1;
        const previousRange = blocks[previous]?.sourceRange;
        const previousRecord = blocks.recordSpans.find(
          (r) => r.spanStart === previous && r.spanEnd === previous + 1,
        );
        const labelEnd = previousRange
          ? content.indexOf("\n", previousRange.start)
          : -1;
        // A separator before the next semantic label belongs to an unchanged,
        // empty preceding record. Do not attach its first body to the next label.
        precedingFirstBody = !!(
          previousRecord &&
          previousRange &&
          blocks.recordSpans.some((r) => r.spanStart === following) &&
          labelEnd >= 0 &&
          previousRange.start + previousRange.length <= labelEnd &&
          start > labelEnd &&
          content.slice(labelEnd + 1, start).trim() === ""
        );
        if (precedingFirstBody) covered.push(previous);
        else if (following >= 0) covered.push(following);
        else if (blocks.length) covered.push(blocks.length - 1);
      }
      if (!covered.length) return rawProjection();
      const owner = (index: number) =>
        blocks.recordSpans.find(
          (r) => index >= r.spanStart && index < r.spanEnd,
        )?.id;
      if (
        covered.some(
          (i) =>
            owner(i) !== owner(covered[0]) ||
            blocks.recordSpans.some((r) => {
              if (r.spanStart !== i) return false;
              const labelEnd = content.indexOf(
                "\n",
                blocks[i].sourceRange!.start,
              );
              return labelEnd < 0 || start <= labelEnd;
            }),
        )
      ) {
        return unavailable(
          "Needs attention: this change crosses document records. Review its original context.",
        );
      }
      const first = blocks[covered[0]].sourceRange!;
      const last = blocks[covered.at(-1)!].sourceRange!;
      let rangeStart = Math.min(start, first.start);
      // The semantic frame already renders this unchanged first-line label.
      if (blocks.recordSpans.some((r) => r.spanStart === covered[0]))
        rangeStart = content.indexOf("\n", first.start) + 1;
      const rangeEnd = Math.max(end, last.start + last.length);
      const original = content.slice(rangeStart, rangeEnd);
      const proposed =
        content.slice(rangeStart, start) +
        replacement +
        content.slice(end, rangeEnd);
      const beforeBlocks = parseMarkdownToBlocks(original);
      const afterBlocks = parseMarkdownToBlocks(proposed);
      if (precedingFirstBody) {
        const framed = parseMarkdownToBlocks(
          content.slice(first.start, rangeStart) + proposed,
        );
        if (
          framed.recordSpans.length !== 1 ||
          framed.recordSpans[0].spanEnd !== framed.length
        )
          return unavailable(
            "Needs attention: this change crosses document records. Review its original context.",
          );
      }
      if ([...beforeBlocks, ...afterBlocks].some(isQuestionBlock))
        return unavailable(
          "Needs attention: this proposal includes a structured question. Review its original context while the question retains its document controls.",
        );
      const sourceInsertion =
        edit && start === end
          ? { start: start - rangeStart, length: replacement.length }
          : undefined;
      const aligned = alignReviewBlocks(
        beforeBlocks,
        afterBlocks,
        sourceInsertion,
      );
      if (!aligned)
        return unavailable(
          "Needs attention: this structural change is too large to align safely.",
        );
      // Preserve canonical link navigation and comment targets for destination
      // changes; this fallback is about source whitespace, not link controls.
      if (
        aligned.some(
          ([old, next]) =>
            old &&
            next &&
            shareInlineStructure(old, next) &&
            hasHiddenLinkChange(
              renderInlineMarkdown(reviewBlockText(old)),
              renderInlineMarkdown(reviewBlockText(next)),
            ),
        )
      )
        return unavailable(
          "Needs attention: source formatting in this proposal cannot be shown faithfully inline. Review its original context.",
        );
      if (
        hasHiddenSourceChange(
          original,
          proposed,
          beforeBlocks,
          afterBlocks,
          aligned,
          blocks.recordSpans.some((r) => r.spanStart === covered[0]),
        )
      )
        return rawProjection(
          rawCovered.length ? rangeStart : start,
          rawCovered.length ? rangeEnd : end,
        );
      const beforeBlockIndices: number[] = [];
      let coveredIndex = 0;
      for (const block of beforeBlocks) {
        if (!block.sourceRange)
          return unavailable(
            "Needs attention: an original block has no source identity.",
          );
        const blockStart = rangeStart + block.sourceRange.start;
        const blockEnd = blockStart + block.sourceRange.length;
        while (
          coveredIndex < covered.length &&
          blocks[covered[coveredIndex]].sourceRange!.start +
            blocks[covered[coveredIndex]].sourceRange!.length <=
            blockStart
        )
          coveredIndex++;
        const canonical = blocks[covered[coveredIndex]]?.sourceRange;
        if (!canonical || canonical.start >= blockEnd)
          return unavailable(
            "Needs attention: an original block cannot be linked to its document source.",
          );
        beforeBlockIndices.push(covered[coveredIndex]);
      }
      return {
        suggestion,
        firstBlock: covered[0],
        coveredBlocks: covered,
        before: beforeBlocks,
        after: afterBlocks,
        beforeBlockIndices,
        attention: null,
        changedRange: { start, end },
        sourceInsertion,
        sourcePreview: {
          content,
          start,
          end,
          replacement,
          rangeStart,
          rangeEnd,
        },
      };
    });
  // Source validity and competition are independent. Connect shared display
  // ranges transitively so each canonical block and each card has one owner.
  const placed = projections.filter((p) => !p.attention);
  const displayRoots = placed.map((_, i) => i);
  const conflictRoots = [...displayRoots];
  const blockSets = placed.map((p) => new Set(p.coveredBlocks));
  const root = (parents: number[], index: number): number => {
    while (parents[index] !== index) index = parents[index];
    return index;
  };
  const groupsFor = (parents: number[]) => {
    const groups = new Map<number, SpecReviewProjection[]>();
    placed.forEach((p, i) => {
      const key = root(parents, i);
      const group = groups.get(key) ?? [];
      group.push(p);
      groups.set(key, group);
    });
    return groups.values();
  };
  const composable = (item: SpecReviewProjection) =>
    item.sourceDiff ||
    (
      alignReviewBlocks(item.before, item.after, item.sourceInsertion) ?? []
    ).every(
      ([old, next]) =>
        !old ||
        (next &&
          (reviewBlockKey(old) === reviewBlockKey(next) ||
            shareInlineStructure(old, next))),
    );
  for (let i = 0; i < placed.length; i++) {
    const p = placed[i];
    for (let j = i + 1; j < placed.length; j++) {
      const other = placed[j];
      if (!p.changedRange || !other.changedRange) continue;
      const shareBlock =
        (p.emptyDocument && other.emptyDocument) ||
        (p.boundaryBlock !== undefined &&
          p.boundaryBlock === other.boundaryBlock) ||
        p.coveredBlocks.some((index) => blockSets[j].has(index));
      const a = p.changedRange,
        b = other.changedRange;
      // Ordered child insertions at one point still compose within their parent.
      const sameParentInsertions =
        a.start === a.end &&
        b.start === b.end &&
        a.start === b.start &&
        p.suggestion.parentSuggestionId !== undefined &&
        p.suggestion.parentSuggestionId ===
          other.suggestion.parentSuggestionId &&
        !!p.suggestion.editId &&
        !!other.suggestion.editId &&
        p.suggestion.editId !== other.suggestion.editId;
      const overlaps =
        a.start === a.end && b.start === b.end
          ? a.start === b.start && !sameParentInsertions
          : a.start === a.end
            ? b.start < a.start && a.start < b.end
            : b.start === b.end
              ? a.start < b.start && b.start < a.end
              : a.start < b.end && b.start < a.end;
      if (!shareBlock && !overlaps) continue;
      displayRoots[root(displayRoots, j)] = root(displayRoots, i);
      if (
        overlaps ||
        !p.suggestion.reviewEdit ||
        !other.suggestion.reviewEdit ||
        !composable(p) ||
        !composable(other)
      )
        conflictRoots[root(conflictRoots, j)] = root(conflictRoots, i);
    }
  }
  // Sharing a display location does not make an independent edit an
  // alternative. Separate actual conflict components before sharing context.
  for (const alternatives of groupsFor(conflictRoots)) {
    if (alternatives.length < 2) continue;
    const ids = alternatives.map((p) => p.suggestion.id);
    for (const p of alternatives) p.alternativeIds = ids;
  }
  for (const group of groupsFor(displayRoots)) {
    if (group.length < 2) continue;
    const coveredBlocks = [
      ...new Set(group.flatMap((p) => p.coveredBlocks)),
    ].sort((a, b) => a - b);
    const boundaries = group.flatMap((p) =>
      p.boundaryBlock === undefined ? [] : [p.boundaryBlock],
    );
    const boundaryBlock =
      coveredBlocks.length || !boundaries.length
        ? undefined
        : Math.min(...boundaries);
    const rangeStart = Math.min(
      ...group.map((p) => p.sourcePreview!.rangeStart),
    );
    const rangeEnd = Math.max(...group.map((p) => p.sourcePreview!.rangeEnd));
    for (const p of group) {
      p.firstBlock = coveredBlocks[0] ?? null;
      p.coveredBlocks = coveredBlocks;
      p.boundaryBlock = boundaryBlock;
      // Only the context envelope is shared. The actual edit bytes, verified
      // range and original block identities remain specific to each proposal.
      p.sourcePreview = { ...p.sourcePreview!, rangeStart, rangeEnd };
    }
  }
  return projections;
}

const isQuestionBlock = (block: Block) =>
  block.type === "code" && isQuestionFenceLanguage(block.language);

interface InlineLinkSpan {
  start: number;
  end: number;
  href: string;
}

function inlineLinkSpans(node: ReactNode): InlineLinkSpan[] {
  const spans: InlineLinkSpan[] = [];
  let offset = 0;
  const visit = (node: ReactNode) => {
    if (typeof node === "string" || typeof node === "number")
      offset += String(node).length;
    else if (Array.isArray(node)) node.forEach(visit);
    else if (
      React.isValidElement<{ href?: string; children?: ReactNode }>(node)
    ) {
      const start = offset;
      visit(node.props.children);
      if (node.type === "a" && typeof node.props.href === "string")
        spans.push({ start, end: offset, href: node.props.href });
    }
  };
  visit(node);
  return spans;
}

function hasHiddenLinkChange(before: ReactNode, after: ReactNode): boolean {
  const a = inlineLinkSpans(before),
    b = inlineLinkSpans(after);
  if (
    JSON.stringify(a.map((span) => span.href)) !==
    JSON.stringify(b.map((span) => span.href))
  )
    return true;
  if (!a.length) return false;
  const beforeText = reviewNodeText(before),
    afterText = reviewNodeText(after);
  // Equal visible text is fully marked by makeRows, with both original and
  // proposed wrappers. Only unequal text can have proposed-only keep slices.
  if (beforeText === afterText) return false;
  const clip = (
    spans: InlineLinkSpan[],
    cursor: { index: number },
    start: number,
    end: number,
  ) => {
    while (cursor.index < spans.length && spans[cursor.index].end <= start)
      cursor.index++;
    const targets: Array<[number, number, string]> = [];
    for (let i = cursor.index; i < spans.length && spans[i].start < end; i++)
      targets.push([
        Math.max(start, spans[i].start) - start,
        Math.min(end, spans[i].end) - start,
        spans[i].href,
      ]);
    return JSON.stringify(targets);
  };
  const oldCursor = { index: 0 },
    nextCursor = { index: 0 };
  let x = 0,
    y = 0;
  for (const edit of reviewTextEdits(beforeText, afterText)) {
    if (
      edit.kind === "keep" &&
      clip(a, oldCursor, x, x + edit.text.length) !==
        clip(b, nextCursor, y, y + edit.text.length)
    )
      return true;
    if (edit.kind !== "add") x += edit.text.length;
    if (edit.kind !== "remove") y += edit.text.length;
  }
  return false;
}

/** A rich row cannot imply source bytes which its parser has discarded. */
function hasHiddenSourceChange(
  original: string,
  proposed: string,
  before: Block[],
  after: Block[],
  pairs: Array<[Block | undefined, Block | undefined]>,
  recordFrame = false,
): boolean {
  const raw = (source: string, block: Block) => {
    const range = block.sourceRange;
    return range ? source.slice(range.start, range.start + range.length) : "";
  };
  if (
    original !== proposed &&
    before.length === after.length &&
    before.every(
      (block, i) => reviewBlockKey(block) === reviewBlockKey(after[i]),
    )
  )
    return true;
  // Include each line's position, so moving a hard break to another line is
  // detected even when a visible text edit occurs in that same paragraph.
  const edgeWhitespace = (source: string) =>
    JSON.stringify(
      source.split("\n").flatMap((line, index) => {
        const leading = line.match(/^\s*/)?.[0] ?? "";
        const trailing = line.match(/\s*$/)?.[0] ?? "";
        return leading || trailing ? [[index, leading, trailing]] : [];
      }),
    );
  for (const [old, next] of pairs) {
    if (!old || !next) {
      const block = old ?? next!;
      // Without an opposite row there is no raw edge comparison. Require this
      // row to retain all authored bytes through the existing block serializer.
      if (blockToRawMarkdown(block) !== raw(old ? original : proposed, block))
        return true;
      continue;
    }
    const oldRaw = raw(original, old),
      nextRaw = raw(proposed, next);
    if (oldRaw === nextRaw) continue;
    if (
      reviewBlockKey(old) === reviewBlockKey(next) ||
      (old.type === next.type &&
        old.type !== "code" &&
        edgeWhitespace(oldRaw) !== edgeWhitespace(nextRaw))
    )
      return true;
  }
  const prefix = (source: string, blocks: Block[]) =>
    source.slice(0, blocks[0]?.sourceRange?.start ?? source.length);
  const suffix = (source: string, blocks: Block[]) => {
    const last = blocks.at(-1)?.sourceRange;
    return last ? source.slice(last.start + last.length) : source;
  };
  const oldPrefix = prefix(original, before),
    nextPrefix = prefix(proposed, after);
  if (
    oldPrefix !== nextPrefix &&
    !(
      recordFrame &&
      (!before.length || !after.length) &&
      [oldPrefix, nextPrefix].every((gap) => gap === "" || gap === "\n")
    )
  )
    return true;
  const oldSuffix = suffix(original, before),
    nextSuffix = suffix(proposed, after);
  const [lastOld, lastNext] = pairs.at(-1) ?? [];
  if (
    lastOld && lastNext
      ? oldSuffix !== nextSuffix
      : [oldSuffix, nextSuffix].some(
          (gap) =>
            gap !== "" &&
            gap !== "\n" &&
            ((before.length > 0 && after.length > 0) || gap !== "\n\n"),
        )
  )
    return true;

  // Compare all raw gaps between paired anchors, including the flanks of
  // absent rows. An added/removed block may account for one ordinary separator;
  // extra blank bytes still need the raw presentation.
  const sameGaps = (a: string[], b: string[]) => {
    const [shorter, longer] = a.length <= b.length ? [a, b] : [b, a];
    let matched = 0;
    for (const gap of longer) {
      if (matched < shorter.length && gap === shorter[matched]) matched++;
      else if (gap !== "\n\n") return false;
    }
    return matched === shorter.length;
  };
  const gap = (source: string, left: Block, right: Block) =>
    source.slice(
      left.sourceRange!.start + left.sourceRange!.length,
      right.sourceRange!.start,
    );
  let oldLeft: Block | undefined, nextLeft: Block | undefined;
  const oldGaps: string[] = [],
    nextGaps: string[] = [];
  for (const [old, next] of pairs) {
    if (old) {
      if (oldLeft) oldGaps.push(gap(original, oldLeft, old));
      oldLeft = old;
    }
    if (next) {
      if (nextLeft) nextGaps.push(gap(proposed, nextLeft, next));
      nextLeft = next;
    }
    if (old && next) {
      if (!sameGaps(oldGaps, nextGaps)) return true;
      oldGaps.length = nextGaps.length = 0;
    }
  }
  return !sameGaps(oldGaps, nextGaps);
}

export const reviewBlockKey = (block: Block) =>
  JSON.stringify(block, (key, value) =>
    key === "sourceRange" ? undefined : value,
  );

export function reviewNodeText(node: ReactNode): string {
  if (typeof node === "string" || typeof node === "number") return String(node);
  if (Array.isArray(node)) return node.map(reviewNodeText).join("");
  if (React.isValidElement<{ children?: ReactNode }>(node))
    return reviewNodeText(node.props.children);
  return "";
}

export const reviewBlockText = (block: Block) =>
  "content" in block
    ? block.content
    : "items" in block
      ? block.items.join("\n")
      : block.type === "image"
        ? block.alt
        : block.type === "table"
          ? [block.headers, ...block.rows].map((r) => r.join(" · ")).join("\n")
          : "Divider";

export const shareInlineStructure = (a: Block, b: Block) =>
  ["paragraph", "heading", "blockquote", "callout"].includes(a.type) &&
  a.type === b.type &&
  (a.type !== "heading" || (b.type === "heading" && a.level === b.level)) &&
  (a.type !== "callout" || (b.type === "callout" && a.kind === b.kind));

/** Align unchanged interior blocks before pairing each remaining changed gap. */
export function alignReviewBlocks(
  a: Block[],
  b: Block[],
  insertion?: { start: number; length: number },
): Array<[Block | undefined, Block | undefined]> | null {
  const keysA = a.map(reviewBlockKey),
    keysB = b.map(reviewBlockKey);
  const matches = (i: number, j: number) => {
    if (keysA[i] !== keysB[j]) return false;
    if (!insertion) return true;
    const oldStart = a[i].sourceRange?.start;
    const nextStart = b[j].sourceRange?.start;
    // Equal added copies must not borrow a canonical block's comment identity.
    return (
      oldStart !== undefined &&
      nextStart ===
        oldStart + (oldStart >= insertion.start ? insertion.length : 0)
    );
  };
  let prefix = 0,
    suffix = 0;
  while (prefix < Math.min(a.length, b.length) && matches(prefix, prefix))
    prefix++;
  while (
    suffix < Math.min(a.length, b.length) - prefix &&
    matches(a.length - 1 - suffix, b.length - 1 - suffix)
  )
    suffix++;
  const n = a.length - prefix - suffix,
    m = b.length - prefix - suffix;
  if (n * m > 250_000) return null;
  const dp = new Uint32Array((n + 1) * (m + 1));
  const cell = (i: number, j: number) => i * (m + 1) + j;
  for (let i = n - 1; i >= 0; i--)
    for (let j = m - 1; j >= 0; j--)
      dp[cell(i, j)] = matches(prefix + i, prefix + j)
        ? 1 + dp[cell(i + 1, j + 1)]
        : Math.max(dp[cell(i + 1, j)], dp[cell(i, j + 1)]);
  const result: Array<[Block | undefined, Block | undefined]> = a
    .slice(0, prefix)
    .map((old, i) => [old, b[i]]);
  let i = 0,
    j = 0;
  const removed: Block[] = [],
    added: Block[] = [];
  const flush = () => {
    for (let k = 0; k < Math.max(removed.length, added.length); k++)
      result.push([removed[k], added[k]]);
    removed.length = added.length = 0;
  };
  while (i < n || j < m) {
    if (i < n && j < m && matches(prefix + i, prefix + j)) {
      flush();
      result.push([a[prefix + i++], b[prefix + j++]]);
    } else if (i < n && (j === m || dp[cell(i + 1, j)] >= dp[cell(i, j + 1)]))
      removed.push(a[prefix + i++]);
    else added.push(b[prefix + j++]);
  }
  flush();
  for (let k = b.length - suffix; k < b.length; k++)
    result.push([a[a.length - b.length + k], b[k]]);
  return result;
}

export interface TextEdit {
  kind: "keep" | "remove" | "add";
  text: string;
}

/** Bounded word diff: trim shared edges before allocating a small LCS table. */
export function reviewTextEdits(before: string, after: string): TextEdit[] {
  const tokens = (s: string) =>
    s.match(/\s+|[\p{L}\p{N}_]+|[^\s\p{L}\p{N}_]/gu) ?? [];
  const a = tokens(before),
    b = tokens(after);
  let prefix = 0,
    suffix = 0;
  while (prefix < Math.min(a.length, b.length) && a[prefix] === b[prefix])
    prefix++;
  while (
    suffix < Math.min(a.length, b.length) - prefix &&
    a[a.length - 1 - suffix] === b[b.length - 1 - suffix]
  )
    suffix++;
  const n = a.length - prefix - suffix,
    m = b.length - prefix - suffix;
  const edits: TextEdit[] = [];
  const push = (kind: TextEdit["kind"], text: string) => {
    if (!text) return;
    if (edits.at(-1)?.kind === kind) edits[edits.length - 1].text += text;
    else edits.push({ kind, text });
  };
  push("keep", a.slice(0, prefix).join(""));
  if (n * m > 250_000) {
    push("remove", a.slice(prefix, prefix + n).join(""));
    push("add", b.slice(prefix, prefix + m).join(""));
  } else {
    const dp = new Uint32Array((n + 1) * (m + 1));
    const cell = (i: number, j: number) => i * (m + 1) + j;
    for (let i = n - 1; i >= 0; i--)
      for (let j = m - 1; j >= 0; j--) {
        dp[cell(i, j)] =
          a[prefix + i] === b[prefix + j]
            ? 1 + dp[cell(i + 1, j + 1)]
            : Math.max(dp[cell(i + 1, j)], dp[cell(i, j + 1)]);
      }
    let i = 0,
      j = 0;
    while (i < n || j < m) {
      if (i < n && j < m && a[prefix + i] === b[prefix + j]) {
        push("keep", a[prefix + i++]);
        j++;
      } else if (i < n && (j === m || dp[cell(i + 1, j)] >= dp[cell(i, j + 1)]))
        push("remove", a[prefix + i++]);
      else push("add", b[prefix + j++]);
    }
  }
  push("keep", suffix ? a.slice(-suffix).join("") : "");
  // Read a phrase replacement as one suggestion instead of several tiny edits
  // around a shared word. Long unchanged clauses still separate distinct edits.
  const phrases: TextEdit[] = [];
  for (let i = 0; i < edits.length; i++) {
    if (edits[i].kind === "keep") {
      phrases.push(edits[i]);
      continue;
    }
    let removed = "",
      added = "";
    while (i < edits.length) {
      const edit = edits[i];
      if (edit.kind === "keep") {
        const words = edit.text.match(/[\p{L}\p{N}_]+/gu) ?? [];
        if (
          i === edits.length - 1 ||
          words.length > 2 ||
          edit.text.length > 100
        ) {
          i--;
          break;
        }
        removed += edit.text;
        added += edit.text;
      } else if (edit.kind === "remove") removed += edit.text;
      else added += edit.text;
      i++;
    }
    if (removed) phrases.push({ kind: "remove", text: removed });
    if (added) phrases.push({ kind: "add", text: added });
  }
  return phrases;
}
