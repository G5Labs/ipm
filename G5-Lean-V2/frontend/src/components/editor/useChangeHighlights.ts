import { useLayoutEffect, useState, type RefObject } from "react";
import {
  changeHighlights,
  type BeforeExcerpt,
  type HighlightRange,
} from "../../lib/ontologyChangeHighlights";

/** `::highlight()` names; index.css colors them per row. */
export const INSERTED_HIGHLIGHT = "ontology-inserted";
export const REMOVED_HIGHLIGHT = "ontology-removed";

/** Never highlighted, and not part of the compared text: the card's
 * "(edited)", badges, diagnostics, review cards and controls. */
export const HIGHLIGHT_IGNORE_ATTRIBUTE = "data-change-highlight-ignore";
const IGNORED = `[${HIGHLIGHT_IGNORE_ATTRIBUTE}], [data-review-suggestions], .sr-only, button, input, textarea, select, svg, style, script, [aria-hidden="true"]`;
const BLOCK_TAGS = new Set([
  "ADDRESS",
  "ARTICLE",
  "ASIDE",
  "BLOCKQUOTE",
  "BR",
  "DD",
  "DETAILS",
  "DIV",
  "DL",
  "DT",
  "FIELDSET",
  "FIGCAPTION",
  "FIGURE",
  "FOOTER",
  "FORM",
  "H1",
  "H2",
  "H3",
  "H4",
  "H5",
  "H6",
  "HEADER",
  "HR",
  "LI",
  "MAIN",
  "NAV",
  "OL",
  "P",
  "PRE",
  "SECTION",
  "SUMMARY",
  "TABLE",
  "TBODY",
  "TD",
  "TFOOT",
  "TH",
  "THEAD",
  "TR",
  "UL",
]);

interface TextBlock {
  text: string;
  /** Each text node and where its data starts in `text`. */
  nodes: { node: Text; start: number }[];
}

/** The rendered text a reader sees in a statement body, one entry per block;
 * every block boundary ends a sentence. */
export function textBlocks(root: Element): TextBlock[] {
  const blocks: TextBlock[] = [];
  let open: TextBlock | null = null;
  const walk = (node: Node) => {
    if (node.nodeType === Node.TEXT_NODE) {
      const text = node as Text;
      if (!text.data) return;
      open ??= blocks[blocks.push({ text: "", nodes: [] }) - 1];
      open.nodes.push({ node: text, start: open.text.length });
      open.text += text.data;
      return;
    }
    if (node.nodeType !== Node.ELEMENT_NODE) return;
    const element = node as Element;
    if (element.matches(IGNORED)) {
      // A <br> inside an ignored subtree still never joins words.
      if (BLOCK_TAGS.has(element.tagName)) open = null;
      return;
    }
    const block = BLOCK_TAGS.has(element.tagName);
    if (block) open = null;
    element.childNodes.forEach(walk);
    if (block) open = null;
  };
  root.childNodes.forEach(walk);
  return blocks;
}

function position(block: TextBlock, offset: number, end: boolean) {
  // At a boundary between nodes, a start belongs to the next node and an end
  // to the previous one, so a range never reaches into an unmarked node.
  for (let i = block.nodes.length - 1; i >= 0; i--) {
    const { node, start } = block.nodes[i];
    if (end ? offset > start : offset >= start)
      return { node, offset: offset - start };
  }
  return { node: block.nodes[0].node, offset: 0 };
}

export function domRanges(blocks: TextBlock[], ranges: HighlightRange[]) {
  return ranges.map(({ block, start, end }) => {
    const b = blocks[block];
    const range = document.createRange();
    const from = position(b, start, false);
    const to = position(b, end, true);
    range.setStart(from.node, from.offset);
    range.setEnd(to.node, to.offset);
    return range;
  });
}

/** How a Modified pair shows its Before version: whole, not at all when
 * nothing was removed, or as an excerpt of the sentences that lost text. */
export type PairLayout =
  | { kind: "full" }
  | { kind: "added" }
  | { kind: "excerpt"; excerpt: BeforeExcerpt };

const FULL: PairLayout = { kind: "full" };

/** Marks the full Before row the comparison reads, shown or hidden. */
export const BEFORE_SOURCE_ATTRIBUTE = "data-before-source";

/** One shared `Highlight` per name. Each pair adds and deletes only its own
 * ranges, so many pairs on one page never clear each other's marks. */
function sharedHighlight(name: string): Highlight | null {
  if (typeof CSS === "undefined" || !CSS.highlights) return null;
  if (typeof Highlight === "undefined") return null;
  let highlight = CSS.highlights.get(name);
  if (!highlight) {
    highlight = new Highlight();
    CSS.highlights.set(name, highlight);
  }
  return highlight;
}

/**
 * Marks what changed inside a Modified pair: removed text in the Before row,
 * added text in the Current row (g5.documents.ontology.change-highlights).
 * Paints over the rendered text without changing it, and follows the rows'
 * text as it changes (unsaved edits, refreshes, card edits). Returns how the
 * pair shows its Before version; it reads the full Before row
 * (`BEFORE_SOURCE_ATTRIBUTE`) even while that row is hidden.
 */
export function useChangeHighlights(
  pair: RefObject<HTMLElement | null>,
  enabled: boolean,
): PairLayout {
  const [layout, setLayout] = useState<PairLayout>(FULL);
  useLayoutEffect(() => {
    const root = pair.current;
    const show = (next: PairLayout) =>
      setLayout((prev) =>
        JSON.stringify(prev) === JSON.stringify(next) ? prev : next,
      );
    const inserted = sharedHighlight(INSERTED_HIGHLIGHT);
    const removed = sharedHighlight(REMOVED_HIGHLIGHT);
    if (!enabled || !root || !inserted || !removed) {
      show(FULL);
      return;
    }
    let own: { inserted: Range[]; removed: Range[] } = {
      inserted: [],
      removed: [],
    };
    const clear = () => {
      own.inserted.forEach((r) => inserted.delete(r));
      own.removed.forEach((r) => removed.delete(r));
      own = { inserted: [], removed: [] };
    };
    const blocks = '[data-testid="semantic-record-blocks"]';
    const update = () => {
      clear();
      const now = root.querySelector(
        `:scope > .ontology-comparison-current ${blocks}`,
      );
      const old = root.querySelector(
        `:scope > [${BEFORE_SOURCE_ATTRIBUTE}] ${blocks}`,
      );
      // A pending suggestion's preview stands in for the text it covers, so
      // the Current row no longer shows the whole statement to compare.
      if (!now || !old || now.querySelector("[data-review-preview]"))
        return show(FULL);
      const nowBlocks = textBlocks(now),
        oldBlocks = textBlocks(old);
      const result = changeHighlights(
        oldBlocks.map((b) => b.text),
        nowBlocks.map((b) => b.text),
      );
      if (!result) return show(FULL);
      show(
        !result.before.length
          ? { kind: "added" }
          : result.beforeExcerpt
            ? { kind: "excerpt", excerpt: result.beforeExcerpt }
            : FULL,
      );
      own = {
        inserted: domRanges(nowBlocks, result.current),
        removed: domRanges(oldBlocks, result.before),
      };
      own.inserted.forEach((r) => inserted.add(r));
      own.removed.forEach((r) => removed.add(r));
    };
    update();
    // React replaces text nodes as the text changes, which collapses ranges
    // over them; recompute once per frame after any change.
    let frame = 0;
    const observer = new MutationObserver(() => {
      if (frame) return;
      frame = requestAnimationFrame(() => {
        frame = 0;
        update();
      });
    });
    observer.observe(root, {
      childList: true,
      characterData: true,
      subtree: true,
    });
    return () => {
      observer.disconnect();
      if (frame) cancelAnimationFrame(frame);
      clear();
    };
  }, [pair, enabled]);
  return enabled ? layout : FULL;
}
