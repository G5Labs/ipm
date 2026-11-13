import React, { useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import type { SuggestionCardNodes } from "../../hooks/useSuggestionCardNodes";
import { SuggestionCardSlot } from "../workspace/SuggestionCardPlaces";
import { renderInlineMarkdown, type Block } from "../../lib/utils";
import { SuggestionCard } from "../workspace/ReviewsPanel";
import {
  viewerPublishedSuggestion,
  type SuggestionOverlayProps,
} from "../workspace/ReviewSuggestionOverlay";
import {
  reviewTextEdits,
  alignReviewBlocks,
  reviewBlockKey,
  reviewNodeText as textOf,
  reviewBlockText as blockText,
  shareInlineStructure,
  type SpecReviewProjection,
} from "./specReviewProjection";

export type SuggestionBlockRenderer = (
  block: Block,
  inline?: (text: string) => ReactNode,
) => ReactNode;
interface Change {
  key: string;
  /** The suggestion the change belongs to: its card's margin anchor. */
  suggestionId: string;
  before: ReactNode;
  after: ReactNode;
  label: string;
  start?: number;
  end?: number;
  originalBlockIndex?: number;
}
interface Row {
  block: Block;
  originalBlockIndex?: number;
  kind: "keep" | "remove" | "add" | "replace";
  inline?: ReactNode;
  change?: Change;
  insertionAfter?: number;
}

function summaryText(node: ReactNode): ReactNode {
  return textOf(node).length > 300 ? <>{sliceTree(node, 0, 300)}…</> : node;
}

/** Display markers only: proposal and decision payloads retain the raw bytes. */
function visibleSource(text: string): string {
  return text
    .replace(/ /g, "·")
    .replace(/\t/g, "⇥")
    .replace(/\r/g, "␍")
    .replace(/\n/g, "↵\n");
}

function sourceChange(projection: SpecReviewProjection): Change {
  const source = projection.sourcePreview!;
  const before = source.content.slice(source.start, source.end);
  return {
    key: projection.suggestion.reviewEdit
      ? projection.suggestion.id
      : `${projection.suggestion.id}-source`,
    suggestionId: projection.suggestion.id,
    before: visibleSource(before),
    after: visibleSource(source.replacement),
    label: before ? (source.replacement ? "Replace" : "Remove") : "Add",
  };
}

function CurrentSourceContext({ content }: { content: string | undefined }) {
  const [open, setOpen] = useState(false);
  return (
    <details
      open={open}
      onToggle={(event) => setOpen(event.currentTarget.open)}
    >
      <summary>Current document source</summary>
      {open && (
        <pre className="max-h-64 overflow-auto whitespace-pre-wrap break-words text-xs">
          {content}
        </pre>
      )}
    </details>
  );
}

/** Slice rendered leaves, retaining emphasis/link/code wrappers across edits. */
function sliceTree(node: ReactNode, start: number, end: number): ReactNode {
  if (end <= start || end <= 0 || start >= textOf(node).length) return null;
  if (typeof node === "string" || typeof node === "number")
    return String(node).slice(start, end);
  if (Array.isArray(node)) {
    let offset = 0;
    return node.map((child, i) => {
      const sliced = sliceTree(
        child,
        Math.max(0, start - offset),
        end - offset,
      );
      offset += textOf(child).length;
      return <React.Fragment key={i}>{sliced}</React.Fragment>;
    });
  }
  if (React.isValidElement<{ children?: ReactNode }>(node)) {
    return React.cloneElement(
      node,
      {},
      sliceTree(node.props.children, start, end),
    );
  }
  return null;
}

const blockKey = reviewBlockKey;
function makeRows(
  projection: SpecReviewProjection,
  mark: (change: Change, kind: "remove" | "add", node: ReactNode) => ReactNode,
): { rows: Row[]; changes: Change[] } {
  const a = projection.before,
    b = projection.after;
  const rows: Row[] = [],
    changes: Change[] = [];
  const pairs = alignReviewBlocks(a, b, projection.sourceInsertion) ?? [];
  let beforeIndex = 0;
  let lastOriginalBlockIndex: number | undefined;
  for (let i = 0; i < pairs.length; i++) {
    const [old, next] = pairs[i];
    const originalBlockIndex = old
      ? projection.beforeBlockIndices?.[beforeIndex++]
      : undefined;
    if (originalBlockIndex !== undefined)
      lastOriginalBlockIndex = originalBlockIndex;
    if (old && next && blockKey(old) === blockKey(next)) {
      rows.push({ block: next, kind: "keep", originalBlockIndex });
      continue;
    }
    if (old && next && shareInlineStructure(old, next)) {
      const before = renderInlineMarkdown(blockText(old)),
        after = renderInlineMarkdown(blockText(next));
      const beforeText = textOf(before),
        afterText = textOf(after);
      const edits =
        beforeText === afterText
          ? [
              { kind: "remove" as const, text: beforeText },
              { kind: "add" as const, text: afterText },
            ]
          : reviewTextEdits(beforeText, afterText);
      let x = 0,
        y = 0;
      const nodes: ReactNode[] = [];
      for (let j = 0; j < edits.length; j++) {
        const edit = edits[j];
        if (edit.kind === "keep") {
          nodes.push(sliceTree(after, y, y + edit.text.length));
          x += edit.text.length;
          y += edit.text.length;
          continue;
        }
        const removal = edit.kind === "remove" ? edit.text : "";
        const addition =
          edit.kind === "add"
            ? edit.text
            : edits[j + 1]?.kind === "add"
              ? edits[++j].text
              : "";
        const change = {
          key:
            (projection.suggestion.editId
              ? projection.suggestion.id
              : undefined) ?? `${projection.suggestion.id}-${i}-${j}`,
          suggestionId: projection.suggestion.id,
          start: x,
          end: x + removal.length,
          originalBlockIndex,
          before: sliceTree(before, x, x + removal.length),
          after: sliceTree(after, y, y + addition.length),
          label: removal && addition ? "Replace" : removal ? "Remove" : "Add",
        };
        changes.push(change);
        if (removal) nodes.push(mark(change, "remove", change.before));
        if (addition) nodes.push(mark(change, "add", change.after));
        x += removal.length;
        y += addition.length;
      }
      rows.push({
        block: next,
        originalBlockIndex,
        kind: "replace",
        inline: nodes.map((node, j) => (
          <React.Fragment key={j}>{node}</React.Fragment>
        )),
      });
    } else {
      const change = {
        key:
          (projection.suggestion.editId
            ? projection.suggestion.id
            : undefined) ?? `${projection.suggestion.id}-${i}-block`,
        suggestionId: projection.suggestion.id,
        originalBlockIndex,
        before: old ? blockText(old) : null,
        after: next ? blockText(next) : null,
        label:
          old && next ? "Replace block" : old ? "Remove block" : "Add block",
      };
      changes.push(change);
      if (old)
        rows.push({ block: old, kind: "remove", change, originalBlockIndex });
      if (next)
        rows.push({
          block: next,
          kind: "add",
          change,
          insertionAfter:
            lastOriginalBlockIndex ??
            (projection.beforeBlockIndices?.[0] ?? 0) - 0.5,
        });
    }
  }
  return { rows, changes };
}

/** Compose disjoint durable edits for display, retaining each edit's own card. */
function combineRows(
  presentations: Array<{
    projection: SpecReviewProjection;
    rows: Row[];
    changes: Change[];
  }>,
  mark: (change: Change, kind: "remove" | "add", node: ReactNode) => ReactNode,
): Row[] {
  const sourceRows = new Map<number, Row[]>();
  const added: Row[] = [];
  for (const item of presentations) {
    for (const row of item.rows) {
      if (row.originalBlockIndex === undefined) added.push(row);
      else
        sourceRows.set(row.originalBlockIndex, [
          ...(sourceRows.get(row.originalBlockIndex) ?? []),
          row,
        ]);
    }
  }
  const merged: Array<{ position: number; row: Row }> = [];
  for (const [index, rows] of sourceRows) {
    const replacements = rows.filter(
      (row) => row.kind === "replace" && row.inline,
    );
    let row = rows.find((row) => row.kind !== "keep") ?? rows[0];
    if (replacements.length > 1) {
      const item = presentations.find((item) =>
        item.projection.beforeBlockIndices?.includes(index),
      )!;
      const original =
        item.projection.before[
          item.projection.beforeBlockIndices!.indexOf(index)
        ];
      const tree = renderInlineMarkdown(blockText(original));
      const changes = presentations
        .flatMap((item) => item.changes)
        .filter(
          (change) =>
            change.originalBlockIndex === index && change.start !== undefined,
        )
        .sort((a, b) => a.start! - b.start!);
      const nodes: ReactNode[] = [];
      let offset = 0;
      for (const change of changes) {
        nodes.push(sliceTree(tree, offset, change.start!));
        if (change.before) nodes.push(mark(change, "remove", change.before));
        if (change.after) nodes.push(mark(change, "add", change.after));
        offset = change.end!;
      }
      nodes.push(sliceTree(tree, offset, textOf(tree).length));
      row = {
        block: original,
        originalBlockIndex: index,
        kind: "replace",
        inline: nodes.map((node, i) => (
          <React.Fragment key={i}>{node}</React.Fragment>
        )),
      };
    }
    merged.push({ position: index, row });
  }
  for (const row of added)
    merged.push({ position: row.insertionAfter ?? -0.5, row });
  merged.sort(
    (a, b) =>
      a.position - b.position ||
      Number(a.row.originalBlockIndex === undefined) -
        Number(b.row.originalBlockIndex === undefined),
  );
  return merged.map(({ row }) => row);
}

/** A margin item a group of projections at one location produces. */
export interface ReviewMarginItem {
  id: string;
  /** The suggestions whose cards the item holds; several only for a conflict set. */
  suggestionIds: string[];
  conflict: boolean;
}

/** The margin node of a suggestion's card. */
export const suggestionCardNode = (suggestionId: string) =>
  `suggestion:${suggestionId}`;
/** The margin node of a conflict box's heading. */
export const conflictHeadNode = (itemId: string) => `${itemId}:head`;

/**
 * Where a group's item is shown: a conflict box adopting its heading and each
 * alternative's card, or the one card. The group renders into these nodes.
 */
export const ReviewMarginItemSlot: React.FC<{
  item: ReviewMarginItem;
  nodes: SuggestionCardNodes;
}> = ({ item, nodes }) =>
  item.conflict ? (
    <section
      data-testid={`spec-review-conflict-${item.id}`}
      className="rounded-lg border border-[var(--g5-info)] bg-[var(--g5-bg0)] shadow-sm"
    >
      <SuggestionCardSlot
        nodes={nodes}
        suggestionId={conflictHeadNode(item.id)}
      />
      <div className="space-y-2 p-2">
        {item.suggestionIds.map((suggestionId) => (
          <SuggestionCardSlot
            key={suggestionId}
            nodes={nodes}
            suggestionId={suggestionCardNode(suggestionId)}
          />
        ))}
      </div>
    </section>
  ) : (
    <SuggestionCardSlot
      nodes={nodes}
      suggestionId={suggestionCardNode(item.suggestionIds[0])}
    />
  );

/**
 * The margin items of one group: one box per connected set of conflicting
 * proposals, holding each alternative's card, and one item per other
 * proposal (`g5.documents.review-alternatives`). The host renders these
 * items' places, each card's by `suggestionCardNode` and a box's heading by
 * `conflictHeadNode`; the group renders the cards and headings into them, so
 * a card keeps its component whichever box it is in.
 */
export function reviewMarginItems(
  group: readonly SpecReviewProjection[],
): ReviewMarginItem[] {
  const items: ReviewMarginItem[] = [];
  for (const p of group) {
    const id = p.suggestion.id;
    const alternatives = p.attention ? undefined : p.alternativeIds;
    if (!alternatives) {
      items.push({
        id: `suggestion:${id}`,
        suggestionIds: [id],
        conflict: false,
      });
      continue;
    }
    const set = items.find(
      (item) =>
        item.conflict &&
        item.suggestionIds.some(
          (member) => member === id || alternatives.includes(member),
        ),
    );
    if (set) {
      set.suggestionIds.push(id);
      set.id = `conflict:${[...set.suggestionIds].sort().join("+")}`;
    } else
      items.push({ id: `conflict:${id}`, suggestionIds: [id], conflict: true });
  }
  return items;
}

/**
 * One location's review: the preview rows in the document, with their marks,
 * and the cards in the margin beside it, rendered into the nodes the host
 * keeps for this group's items (`reviewMarginItems`). The group owns which
 * alternative is previewed and which change is selected; `onActivate` tells
 * the host which item to set level with its text.
 */
export function SpecReviewSuggestions({
  projection,
  peers = [],
  overlay,
  renderBlock,
  filtered,
  marginNodes,
  onActivate,
  onOpenRepoLink,
}: {
  projection: SpecReviewProjection;
  peers?: SpecReviewProjection[];
  overlay: SuggestionOverlayProps;
  renderBlock: SuggestionBlockRenderer;
  filtered: boolean;
  /** The margin node of each of this group's items, by item id. */
  marginNodes: (itemId: string) => HTMLElement;
  onActivate?: (itemId: string) => void;
  /**
   * Opens a repository link found in a preview row rendered inline (a
   * changed paragraph keeps its links as plain anchors); returns whether
   * it was one. Rendered blocks resolve their own links.
   */
  onOpenRepoLink?: (href: string) => boolean;
}) {
  const [selected, setSelected] = useState<string | null>(null);
  const [previewIds, setPreviewIds] = useState<ReadonlySet<string>>(new Set());
  const projections = [projection, ...peers];
  const marginItems = reviewMarginItems(projections);
  const itemOf = (suggestionId: string) =>
    marginItems.find((item) => item.suggestionIds.includes(suggestionId))!;
  const activate = (item: SpecReviewProjection) => {
    onActivate?.(itemOf(item.suggestion.id).id);
    const alternatives = item.alternativeIds;
    if (!alternatives) return;
    setPreviewIds((current) => {
      const next = new Set(
        projections
          .map((p) => p.suggestion.id)
          .filter((id) => current.has(id) && !alternatives.includes(id)),
      );
      next.add(item.suggestion.id);
      return next.size === current.size &&
        [...next].every((id) => current.has(id))
        ? current
        : next;
    });
  };
  const id = projection.suggestion.id;
  const select = (key: string, focus: boolean) => {
    setSelected(key);
    const item = presentations.find((p) =>
      p.changes.some((change) => change.key === key),
    );
    if (item) {
      if (competing) activate(item.projection);
      else onActivate?.(itemOf(item.projection.suggestion.id).id);
    }
    if (focus)
      document
        .getElementById(`spec-review-${key}`)
        ?.focus({ preventScroll: true });
  };
  // Rendering functions close over selection; the source projection itself is memoized by the host.
  const mark = (change: Change, kind: "remove" | "add", node: ReactNode) => {
    const Tag = kind === "remove" ? "del" : "ins";
    return (
      <Tag
        key={`${change.key}-${kind}`}
        data-comment-connector-ignore={kind === "add" ? "" : undefined}
        data-review-anchor={change.suggestionId}
        role="button"
        tabIndex={0}
        aria-label={`${kind === "remove" ? "Suggested removal" : "Suggested addition"}: ${textOf(node)}`}
        aria-pressed={selected === change.key}
        aria-controls={`spec-review-${change.key}`}
        onClick={(e) => {
          e.stopPropagation();
          select(change.key, true);
        }}
        onKeyDown={(e) => {
          if (e.key === "Enter" || e.key === " ") {
            e.preventDefault();
            e.stopPropagation();
            select(change.key, true);
          }
        }}
        className={`cursor-pointer text-[var(--g5-ok)] decoration-2 ${selected === change.key ? "bg-[var(--g5-ok)]/15 outline outline-1 outline-[var(--g5-ok)]" : ""}`}
      >
        {node}
      </Tag>
    );
  };
  const competing =
    !projection.attention && projections.some((p) => p.alternativeIds);
  const activeProjections = competing
    ? projections.filter(
        (p) =>
          !p.alternativeIds ||
          p.suggestion.id ===
            (p.alternativeIds.find((id) => previewIds.has(id)) ??
              p.alternativeIds[0]),
      )
    : projections;
  const activeIds = new Set(activeProjections.map((p) => p.suggestion.id));
  const sourceMode = projections.some((p) => p.sourceDiff);
  const presentations = projections.map((p) => ({
    projection: p,
    ...(sourceMode
      ? { rows: [] as Row[], changes: [sourceChange(p)] }
      : makeRows(p, mark)),
  }));
  const previewPresentations = competing
    ? presentations.filter((item) =>
        activeIds.has(item.projection.suggestion.id),
      )
    : presentations;
  const rows =
    previewPresentations.length > 1
      ? combineRows(previewPresentations, mark)
      : [...previewPresentations[0].rows];
  if (competing && !sourceMode) {
    // A shorter alternative must not hide canonical blocks covered only by a
    // peer. Supply their unchanged rows without rendering that peer's changes.
    const represented = new Set(rows.map((row) => row.originalBlockIndex));
    for (const p of projections) {
      p.beforeBlockIndices?.forEach((index, i) => {
        if (represented.has(index)) return;
        represented.add(index);
        rows.push({
          block: p.before[i],
          originalBlockIndex: index,
          kind: "keep",
        });
      });
    }
    const position = (row: Row) =>
      row.originalBlockIndex ?? row.insertionAfter ?? -0.5;
    rows.sort(
      (a, b) =>
        position(a) - position(b) ||
        Number(a.originalBlockIndex === undefined) -
          Number(b.originalBlockIndex === undefined),
    );
  }
  const sourcePreview = () => {
    const ordered = [...previewPresentations].sort(
      (a, b) =>
        a.projection.sourcePreview!.start - b.projection.sourcePreview!.start ||
        a.projection.sourcePreview!.end - b.projection.sourcePreview!.end,
    );
    const first = ordered[0].projection.sourcePreview!;
    let cursor = Math.min(
      ...ordered.map((p) => p.projection.sourcePreview!.rangeStart),
    );
    const end = Math.max(
      ...ordered.map((p) => p.projection.sourcePreview!.rangeEnd),
    );
    const nodes: ReactNode[] = [];
    for (const {
      projection: item,
      changes: [change],
    } of ordered) {
      const source = item.sourcePreview!;
      nodes.push(visibleSource(first.content.slice(cursor, source.start)));
      if (source.start !== source.end)
        nodes.push(mark(change, "remove", change.before));
      if (source.replacement) nodes.push(mark(change, "add", change.after));
      cursor = source.end;
    }
    nodes.push(visibleSource(first.content.slice(cursor, end)));
    return (
      <div
        data-testid="spec-review-source-diff"
        data-comment-connector-ignore=""
      >
        <p className="mb-2 text-xs text-[var(--g5-text2)]">
          Source diff · space · ⇥ tab · ↵ line break · ␍ carriage return
        </p>
        <pre className="whitespace-pre-wrap break-words rounded border border-[var(--g5-stroke)] p-3 text-sm">
          {nodes.map((node, i) => (
            <React.Fragment key={i}>{node}</React.Fragment>
          ))}
        </pre>
      </div>
    );
  };
  const summaryFor = (projection: SpecReviewProjection, changes: Change[]) => (
    <div className="space-y-2 text-sm text-[var(--g5-text1)]">
      {projection.attention ? (
        <>
          <p className="text-[var(--g5-warn)]">{projection.attention}</p>
          {(projection.suggestion.kind === "hunk" ||
            projection.suggestion.kind === "new_file") && (
            <div>
              <p className="font-medium">This edit</p>
              <pre className="max-h-64 overflow-auto whitespace-pre-wrap break-words text-xs">
                {projection.suggestion.reviewEdit?.beforeText ??
                  projection.suggestion.beforeLines}
              </pre>
              <pre className="max-h-64 overflow-auto whitespace-pre-wrap break-words text-xs text-[var(--g5-ok)]">
                {projection.suggestion.reviewEdit?.afterText ??
                  projection.suggestion.afterLines}
              </pre>
            </div>
          )}
          {(projection.suggestion.kind === "hunk" ||
            projection.suggestion.kind === "new_file") && (
            <>
              <CurrentSourceContext
                content={
                  projection.currentSource ?? projection.sourcePreview?.content
                }
              />
            </>
          )}
        </>
      ) : projection.suggestion.reviewEdit ? (
        <button
          type="button"
          id={`spec-review-${projection.suggestion.id}`}
          data-testid={`spec-review-edit-${projection.suggestion.id}`}
          onClick={() => select(projection.suggestion.id, false)}
          onFocus={() => setSelected(projection.suggestion.id)}
          className="block w-full rounded px-2 py-1 text-left break-words"
        >
          <strong>
            {changes.length === 1
              ? changes[0].label
              : projection.suggestion.reviewEdit.beforeText
                ? projection.suggestion.reviewEdit.afterText
                  ? "Replace"
                  : "Remove"
                : "Add"}
            :{" "}
          </strong>
          {projection.suggestion.reviewEdit.beforeText && (
            <span className="line-through">
              {summaryText(
                changes.length === 1
                  ? changes[0].before
                  : renderInlineMarkdown(
                      projection.suggestion.reviewEdit.beforeText,
                    ),
              )}
            </span>
          )}
          {projection.suggestion.reviewEdit.afterText && (
            <span className="text-[var(--g5-ok)]">
              {" "}
              {summaryText(
                changes.length === 1
                  ? changes[0].after
                  : renderInlineMarkdown(
                      projection.suggestion.reviewEdit.afterText,
                    ),
              )}
            </span>
          )}
        </button>
      ) : (
        changes.map((change) => (
          <button
            key={change.key}
            type="button"
            data-testid={`spec-review-edit-${projection.suggestion.id}-${change.key}`}
            id={`spec-review-${change.key}`}
            onClick={() => select(change.key, false)}
            onFocus={() => setSelected(change.key)}
            className={`block w-full rounded px-2 py-1 text-left break-words ${selected === change.key ? "bg-[var(--g5-ok)]/10" : "hover:bg-[var(--g5-bg2)]"}`}
          >
            <strong>{change.label}: </strong>
            {change.before && (
              <>
                <span className="line-through">
                  {summaryText(change.before)}
                </span>{" "}
              </>
            )}
            {change.after && (
              <span className="text-[var(--g5-ok)]">
                {summaryText(change.after)}
              </span>
            )}
          </button>
        ))
      )}
    </div>
  );
  const card = (item: ReviewMarginItem, suggestionId: string) => {
    const presentation = presentations.find(
      (p) => p.projection.suggestion.id === suggestionId,
    )!;
    const { projection: p, changes } = presentation;
    const previewed = item.conflict && activeIds.has(suggestionId);
    const choose = (moveFocus: boolean) => {
      activate(p);
      // Only an explicit Enter moves focus to this card's change summary,
      // as a mark's click does; a pointer or arrow-key change keeps the
      // radio group focused, so the keyboard can step through alternatives.
      if (changes[0]) select(changes[0].key, moveFocus);
    };
    return (
      <aside
        key={suggestionId}
        data-comment-connector-ignore=""
        data-testid={`spec-suggestion-card-${suggestionId}`}
        data-selected={
          (
            competing
              ? activeIds.has(suggestionId)
              : changes.some((change) => change.key === selected)
          )
            ? "true"
            : "false"
        }
        className={`min-w-0 rounded-lg border bg-[var(--g5-bg0)] shadow-sm ${previewed ? "border-[var(--g5-info)]" : "border-[var(--g5-ok)]/40"}`}
      >
        {item.conflict && !filtered && (
          <label className="mx-3 mt-2 flex items-center gap-2 text-xs text-[var(--g5-info)]">
            <input
              type="radio"
              name={`spec-review-preview-${item.id}`}
              checked={previewed}
              data-testid={`spec-review-select-${suggestionId}`}
              aria-controls={`spec-review-preview-${id}`}
              onChange={() => choose(false)}
              onKeyDown={(e) => {
                // Enter chooses too, as it did on the button this replaced.
                if (e.key === "Enter") {
                  e.preventDefault();
                  choose(true);
                }
              }}
            />
            Preview in document
          </label>
        )}
        <div data-testid="block-suggestions">
          <SuggestionCard
            workspaceId={overlay.workspaceId}
            suggestion={p.suggestion}
            viewerOwnsWorkspace={overlay.viewerOwnsWorkspace}
            // The same authority as every other card: the reviewer
            // may withdraw, and the source's writability decides
            // what can be sent (`g5.governance.review-suggestions.display`).
            viewerIsReviewer={viewerPublishedSuggestion(overlay, p.suggestion)}
            sourceWritable={overlay.sourceWritable}
            sourceClosedNote={overlay.sourceClosedNote}
            ownerVmRunning={overlay.ownerVmRunning}
            compact
            proposalPresentation={summaryFor(p, changes)}
            proposalEditContext={
              p.attention ? summaryFor(p, changes) : undefined
            }
          />
        </div>
      </aside>
    );
  };
  const conflictHead = (item: ReviewMarginItem) => {
    const previewing = projections.find(
      (p) =>
        item.suggestionIds.includes(p.suggestion.id) &&
        activeIds.has(p.suggestion.id),
    );
    return (
      <div className="border-b border-[var(--g5-info)]/30 bg-[var(--g5-info)]/10 px-3 py-2">
        <p className="text-xs font-semibold text-[var(--g5-text0)]">
          {item.suggestionIds.length} suggestions for the same text
        </p>
        <p className="text-[11px] text-[var(--g5-text2)]" aria-live="polite">
          They conflict, so only one can apply. Previewing{" "}
          {previewing?.suggestion.authorLabel ?? "Reviewer"}.
        </p>
      </div>
    );
  };
  return (
    <div
      id={`spec-review-group-${id}`}
      className="spec-review-container min-w-0 max-w-full"
      data-review-suggestions=""
      contentEditable={false}
      onClick={(event) => {
        if (event.defaultPrevented || !onOpenRepoLink) return;
        const anchor = (event.target as HTMLElement).closest("a[href]");
        if (!anchor || anchor.hasAttribute("data-repo-link")) return;
        if (onOpenRepoLink(anchor.getAttribute("href") ?? ""))
          event.preventDefault();
      }}
    >
      {!filtered && !projection.attention && (
        <div
          id={`spec-review-preview-${id}`}
          data-testid={`spec-suggestion-preview-${id}`}
          // Stands in for the text it covers, so a comparison pair shows
          // without change highlights (g5.documents.ontology.change-highlights).
          data-review-preview=""
          className="min-w-0 space-y-2"
        >
          {sourceMode && sourcePreview()}
          {!sourceMode &&
            rows.map((row, i) => (
              <div key={i} data-block-index={row.originalBlockIndex}>
                {row.change ? (
                  <div
                    data-comment-connector-ignore={
                      row.kind === "add" ? "" : undefined
                    }
                    data-review-anchor={row.change.suggestionId}
                    role="button"
                    tabIndex={0}
                    aria-label={`Suggested ${row.kind === "add" ? "addition" : "removal"}: ${blockText(row.block)}`}
                    onClick={() => select(row.change!.key, true)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter" || e.key === " ") {
                        e.preventDefault();
                        e.stopPropagation();
                        select(row.change!.key, true);
                      }
                    }}
                    className={`rounded border-l-2 border-[var(--g5-ok)] pl-2 text-[var(--g5-ok)] ${row.kind === "remove" ? "line-through" : "underline decoration-2"} ${selected === row.change.key ? "bg-[var(--g5-ok)]/10" : ""}`}
                    data-spec-review-change={row.kind}
                  >
                    {renderBlock(row.block)}
                  </div>
                ) : (
                  renderBlock(
                    row.block,
                    row.kind === "replace" ? () => row.inline : undefined,
                  )
                )}
              </div>
            ))}
        </div>
      )}
      {marginItems.flatMap((item) => [
        ...(item.conflict
          ? [
              createPortal(
                conflictHead(item),
                marginNodes(conflictHeadNode(item.id)),
                item.id,
              ),
            ]
          : []),
        ...item.suggestionIds.map((suggestionId) =>
          createPortal(
            card(item, suggestionId),
            marginNodes(suggestionCardNode(suggestionId)),
            suggestionId,
          ),
        ),
      ])}
    </div>
  );
}
