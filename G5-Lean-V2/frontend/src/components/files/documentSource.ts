// =============================================================================
// Source coordinates of a rendered Markdown document
// =============================================================================
// Lines and blocks of the source a rendering came from, and the line a
// selection in the rendering comes from. These are read coordinates; they
// never become write offsets.
// =============================================================================

import type { Block } from "../../lib/utils";

/** The byte offset at which 1-based `line` of `content` starts. */
export function lineOffset(content: string, line: number): number {
  let offset = 0;
  const lines = content.split("\n");
  for (let i = 1; i < line && i <= lines.length; i++)
    offset += lines[i - 1].length + 1;
  return offset;
}

/** The 1-based line of `content` holding byte `offset`. */
export function lineOfOffset(content: string, offset: number): number {
  return content.slice(0, Math.max(0, offset)).split("\n").length;
}

/** The block whose source holds the start of `line`, or -1 for none. */
export function blockIndexAtLine(
  content: string,
  blocks: readonly Block[],
  line: number,
): number {
  const offset = lineOffset(content, line);
  return blocks.findIndex(
    (b) =>
      b.sourceRange &&
      offset >= b.sourceRange.start &&
      offset < b.sourceRange.start + b.sourceRange.length,
  );
}

/**
 * The rendered element to show for `line`: its block, or for a line between
 * blocks or in a hidden block, the nearest rendered block before it.
 */
export function renderedBlockForLine(
  root: HTMLElement,
  content: string,
  blocks: readonly Block[],
  line: number,
): HTMLElement | null {
  let index = blockIndexAtLine(content, blocks, line);
  if (index < 0) {
    const offset = lineOffset(content, line);
    index = blocks.findLastIndex(
      (b) => b.sourceRange !== undefined && b.sourceRange.start <= offset,
    );
  }
  for (; index >= 0; index--) {
    const el = root.querySelector<HTMLElement>(`[data-block-index="${index}"]`);
    if (el) return el;
  }
  return null;
}

/** Whitespace as the rendering shows it: runs collapse to one space. */
function squash(text: string): string {
  return text.replace(/\s+/g, " ").trim();
}

/**
 * The source line a selection in a rendered block comes from: the block's
 * first line that shows the start of the selected text, found by the
 * selection's opening words so a selection across wrapped source lines or
 * through inline markup still lands on its first line. When the rendering
 * does not show the source (a link's text, an image), the block's first
 * line. Null when the block has no source.
 */
export function selectionSourceLine(
  content: string,
  block: Block | undefined,
  selectedText: string,
): number | null {
  if (!block?.sourceRange) return null;
  const first = lineOfOffset(content, block.sourceRange.start);
  const source = content.slice(
    block.sourceRange.start,
    block.sourceRange.start + block.sourceRange.length,
  );
  const needle = squash(selectedText);
  if (!needle) return first;
  const lines = source.split("\n").map(squash);
  const probes = [needle.slice(0, 40), needle.split(" ")[0]].filter(
    (probe, index, all) => probe.length >= 2 && all.indexOf(probe) === index,
  );
  for (const probe of probes) {
    const at = lines.findIndex((line) => line.includes(probe));
    if (at >= 0) return first + at;
  }
  return first;
}
