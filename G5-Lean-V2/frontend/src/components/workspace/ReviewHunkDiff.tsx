// =============================================================================
// G5 Workspace — ReviewHunkDiff
// =============================================================================
// A review hunk as a unified diff, the way the commit diff view shows one:
// an `@@` header, then context, removed and added rows with the old and new
// line numbers side by side. A hunk is stored as the block it replaces and
// its replacement, both carrying the diff's context rows, so the rows are
// recovered here by a line diff of the two.
// =============================================================================

import React, { useMemo } from "react";
import type { DiffLine } from "../../lib/types";
import { DiffLineRow } from "../editor/DiffLineRow";
import { classNames } from "../ui/UI";

/**
 * The most removed plus added lines a hunk is diffed for. The search keeps
 * one row of state per edit, so its memory grows with the square of this.
 * A file's diff is published only up to 3,000 rows (`g5_scm`
 * `MAX_COMMIT_FILE_DIFF_LINES`), so only an owner's own edit can exceed it.
 */
const MAX_EDIT_DISTANCE = 3000;

type EditOp = "keep" | "remove" | "add";

/**
 * A shortest edit script turning `before[i0, n)` into `after[j0, m)`
 * (Myers' greedy O(ND) search, so the cost follows the number of changed
 * lines, not the size of the hunk). Null past `MAX_EDIT_DISTANCE` edits.
 */
function editScript(
  before: string[],
  after: string[],
  i0: number,
  n: number,
  j0: number,
  m: number,
): EditOp[] | null {
  const rows = n - i0;
  const cols = m - j0;
  const limit = Math.min(rows + cols, MAX_EDIT_DISTANCE);
  // furthest[offset + k]: how far into `before` diagonal k (x - y) reaches.
  const offset = limit + 1;
  const furthest = new Int32Array(2 * limit + 3);
  // Before step d, the diagonals -d-1..d+1 (diagonal k at index k + d + 1).
  const trace: Int32Array[] = [];
  const cameDown = (row: Int32Array, d: number, k: number) =>
    k === -d || (k !== d && row[k - 1 + d + 1] < row[k + 1 + d + 1]);
  for (let d = 0; d <= limit; d += 1) {
    const row = furthest.slice(offset - d - 1, offset + d + 2);
    trace.push(row);
    for (let k = -d; k <= d; k += 2) {
      let x = cameDown(row, d, k) ? row[k + 1 + d + 1] : row[k - 1 + d + 1] + 1;
      let y = x - k;
      while (x < rows && y < cols && before[i0 + x] === after[j0 + y]) {
        x += 1;
        y += 1;
      }
      furthest[offset + k] = x;
      if (x < rows || y < cols) continue;
      // Reached the end: walk the steps back to the start.
      const ops: EditOp[] = [];
      for (let step = d; step > 0; step -= 1) {
        const prev = trace[step];
        const diagonal = x - y;
        const down = cameDown(prev, step, diagonal);
        const prevK = down ? diagonal + 1 : diagonal - 1;
        const prevX = prev[prevK + step + 1];
        const prevY = prevX - prevK;
        while (x > prevX + (down ? 0 : 1) || y > prevY + (down ? 1 : 0)) {
          ops.push("keep");
          x -= 1;
          y -= 1;
        }
        ops.push(down ? "add" : "remove");
        x = prevX;
        y = prevY;
      }
      for (; x > 0; x -= 1) ops.push("keep");
      return ops.reverse();
    }
  }
  return null;
}

/** The lines of a hunk side; the final newline ends the last line. */
export function splitHunkLines(text: string | null): string[] {
  if (text === null || text === "") return [];
  const lines = text.split("\n");
  if (lines[lines.length - 1] === "") lines.pop();
  return lines;
}

/**
 * The rows of a hunk: lines both sides share are context, the rest are
 * removed or added, each run of removals ahead of the additions replacing
 * it. `oldStart` and `newStart` are the first line of each side in its own
 * file (null leaves that gutter empty). A hunk with more changed lines than
 * `MAX_EDIT_DISTANCE` shows its differing middle as one replacement.
 */
export function hunkDiffLines(
  beforeText: string | null,
  afterText: string | null,
  oldStart: number | null,
  newStart: number | null,
): DiffLine[] {
  const before = splitHunkLines(beforeText);
  const after = splitHunkLines(afterText);
  const rows: DiffLine[] = [];
  let i = 0;
  let j = 0;
  const oldLine = () => (oldStart === null ? undefined : oldStart + i);
  const newLine = () => (newStart === null ? undefined : newStart + j);
  const context = () => {
    rows.push({
      type: "context",
      content: before[i],
      oldLine: oldLine(),
      newLine: newLine(),
    });
    i += 1;
    j += 1;
  };
  const remove = () => {
    rows.push({ type: "deletion", content: before[i], oldLine: oldLine() });
    i += 1;
  };
  const add = () => {
    rows.push({ type: "addition", content: after[j], newLine: newLine() });
    j += 1;
  };

  // The shared head and tail are context whatever lies between them.
  let tail = 0;
  while (
    tail < before.length &&
    tail < after.length &&
    before[before.length - 1 - tail] === after[after.length - 1 - tail]
  ) {
    tail += 1;
  }
  const n = before.length - tail;
  const m = after.length - tail;
  while (i < n && j < m && before[i] === after[j]) context();

  const ops = editScript(before, after, i, n, j, m);
  if (ops === null) {
    while (i < n) remove();
    while (j < m) add();
  } else {
    // Between two kept lines, the removals come ahead of the additions.
    let pendingAdds = 0;
    const flushAdds = () => {
      for (; pendingAdds > 0; pendingAdds -= 1) add();
    };
    for (const op of ops) {
      if (op === "remove") {
        remove();
      } else if (op === "add") {
        pendingAdds += 1;
      } else {
        flushAdds();
        context();
      }
    }
    flushAdds();
  }
  while (i < before.length) context();
  return rows;
}

/** Git's `@@ -old,count +new,count @@`; an empty side starts a line early. */
export function hunkHeader(
  lines: DiffLine[],
  oldStart: number,
  newStart: number,
): string {
  const oldCount = lines.filter((line) => line.type !== "addition").length;
  const newCount = lines.filter((line) => line.type !== "deletion").length;
  const side = (start: number, count: number) =>
    `${count === 0 ? Math.max(0, start - 1) : start},${count}`;
  return `@@ -${side(oldStart, oldCount)} +${side(newStart, newCount)} @@`;
}

/** Rows a hunk's diff takes on screen, its header included. */
export function hunkDiffRowCount(
  beforeText: string | null,
  afterText: string | null,
): number {
  return hunkDiffLines(beforeText, afterText, null, null).length + 1;
}

export const ReviewHunkDiff: React.FC<{
  before: string | null;
  after: string | null;
  /** First line of the replaced block in the file the hunk applies to. */
  oldStart: number | null;
  /** First line of the replacement in the file that carries it. */
  newStart: number | null;
  className?: string;
}> = ({ before, after, oldStart, newStart, className }) => {
  const lines = useMemo(
    () => hunkDiffLines(before, after, oldStart, newStart),
    [before, after, oldStart, newStart],
  );
  return (
    <div
      data-testid="review-hunk-diff"
      className={classNames(
        "overflow-auto rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)]",
        "bg-[var(--g5-bg0)] font-mono text-[12px] leading-5",
        className,
      )}
    >
      {/* As wide as the longest line, so the row tints span a sideways scroll. */}
      <div style={{ width: "max-content", minWidth: "100%" }}>
        {oldStart !== null && newStart !== null && (
          <div
            data-testid="review-hunk-header"
            className={classNames(
              "whitespace-pre px-2 text-[var(--g5-info)]",
              "border-b border-[var(--g5-stroke)]/50 bg-[color-mix(in_srgb,var(--g5-info)_10%,transparent)]",
            )}
          >
            {hunkHeader(lines, oldStart, newStart)}
          </div>
        )}
        {lines.map((line, index) => (
          <DiffLineRow key={index} line={line} />
        ))}
      </div>
    </div>
  );
};
