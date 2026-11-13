import { render, screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import {
  hunkDiffLines,
  hunkDiffRowCount,
  hunkHeader,
  ReviewHunkDiff,
} from "./ReviewHunkDiff";

const BASE = "# code\n\nThis is a G5-managed local project repository.\n";

describe("hunkDiffLines", () => {
  it("keeps the lines both sides share as context around an insertion", () => {
    const lines = hunkDiffLines(
      BASE,
      `${BASE}\nTHis is for testing review.\n`,
      1,
      1,
    );
    expect(lines.map((line) => line.type)).toEqual([
      "context",
      "context",
      "context",
      "addition",
      "addition",
    ]);
    expect(lines[2]).toEqual({
      type: "context",
      content: "This is a G5-managed local project repository.",
      oldLine: 3,
      newLine: 3,
    });
    expect(lines[4]).toEqual({
      type: "addition",
      content: "THis is for testing review.",
      newLine: 5,
    });
    expect(hunkHeader(lines, 1, 1)).toBe("@@ -1,3 +1,5 @@");
  });

  it("puts a run of removals ahead of the lines replacing it", () => {
    const lines = hunkDiffLines(
      "a\nold1\nold2\nz\n",
      "a\nnew1\nnew2\nz\n",
      7,
      9,
    );
    expect(
      lines.map((line) => [
        line.type,
        line.content,
        line.oldLine,
        line.newLine,
      ]),
    ).toEqual([
      ["context", "a", 7, 9],
      ["deletion", "old1", 8, undefined],
      ["deletion", "old2", 9, undefined],
      ["addition", "new1", undefined, 10],
      ["addition", "new2", undefined, 11],
      ["context", "z", 10, 12],
    ]);
  });

  it("keeps unchanged lines between two edits of one hunk", () => {
    const lines = hunkDiffLines("a\nb\nc\nd\n", "A\nb\nc\nD\n", 1, 1);
    expect(lines.map((line) => line.type)).toEqual([
      "deletion",
      "addition",
      "context",
      "context",
      "deletion",
      "addition",
    ]);
  });

  it("handles a one-sided hunk and a missing anchor", () => {
    const inserted = hunkDiffLines(null, "x\ny\n", 1, 1);
    expect(inserted.map((line) => line.newLine)).toEqual([1, 2]);
    expect(hunkHeader(inserted, 1, 1)).toBe("@@ -0,0 +1,2 @@");
    const deleted = hunkDiffLines("x\n", "", null, null);
    expect(deleted).toEqual([{ type: "deletion", content: "x" }]);
    expect(hunkDiffRowCount("x\n", "")).toBe(2);
  });

  it("keeps the context of a large hunk with scattered edits", () => {
    // 1,001 lines with every fifth changed: git reports one hunk.
    const before = Array.from({ length: 1001 }, (_, i) => `line ${i}`);
    const after = before.map((line, i) => (i % 5 === 0 ? `${line}!` : line));
    const lines = hunkDiffLines(before.join("\n"), after.join("\n"), 1, 1);
    const count = (type: string) =>
      lines.filter((line) => line.type === type).length;
    expect(count("context")).toBe(800);
    expect(count("deletion")).toBe(201);
    expect(count("addition")).toBe(201);
    expect(lines.slice(0, 3)).toEqual([
      { type: "deletion", content: "line 0", oldLine: 1 },
      { type: "addition", content: "line 0!", newLine: 1 },
      { type: "context", content: "line 1", oldLine: 2, newLine: 2 },
    ]);
  });

  it("rebuilds both sides from the rows, whatever the edit", () => {
    // Deterministic pseudo-random hunks over a small alphabet, so lines
    // repeat and the diff has real choices to make.
    let seed = 7;
    const next = (bound: number) => {
      seed = (seed * 1103515245 + 12345) & 0x7fffffff;
      return seed % bound;
    };
    for (let round = 0; round < 400; round += 1) {
      const before = Array.from({ length: next(9) }, () => "abc"[next(3)]);
      const after = Array.from({ length: next(9) }, () => "abc"[next(3)]);
      const lines = hunkDiffLines(before.join("\n"), after.join("\n"), 10, 20);
      const oldSide = lines.filter((line) => line.type !== "addition");
      const newSide = lines.filter((line) => line.type !== "deletion");
      expect(oldSide.map((line) => line.content)).toEqual(before);
      expect(newSide.map((line) => line.content)).toEqual(after);
      expect(oldSide.map((line) => line.oldLine)).toEqual(
        before.map((_, i) => 10 + i),
      );
      expect(newSide.map((line) => line.newLine)).toEqual(
        after.map((_, i) => 20 + i),
      );
      // Shortest script: the context is a longest common subsequence.
      const table = Array.from({ length: before.length + 1 }, () =>
        new Array<number>(after.length + 1).fill(0),
      );
      for (let i = before.length - 1; i >= 0; i -= 1) {
        for (let j = after.length - 1; j >= 0; j -= 1) {
          table[i][j] =
            before[i] === after[j]
              ? table[i + 1][j + 1] + 1
              : Math.max(table[i + 1][j], table[i][j + 1]);
        }
      }
      expect(lines.filter((line) => line.type === "context")).toHaveLength(
        table[0][0],
      );
      // Within a changed run, no addition precedes a removal.
      for (let i = 1; i < lines.length; i += 1) {
        expect(
          lines[i - 1].type === "addition" && lines[i].type === "deletion",
        ).toBe(false);
      }
    }
  });

  it("shows a hunk with too many changed lines as one replacement", () => {
    const before = Array.from({ length: 1501 }, (_, i) => `b${i}`);
    const after = Array.from({ length: 1501 }, (_, i) => `a${i}`);
    const lines = hunkDiffLines(
      ["head", ...before, "tail"].join("\n"),
      ["head", ...after, "tail"].join("\n"),
      1,
      1,
    );
    expect(lines).toHaveLength(3004);
    expect(lines[0].type).toBe("context");
    expect(lines[1501].type).toBe("deletion");
    expect(lines[1502].type).toBe("addition");
    expect(lines[3003]).toEqual({
      type: "context",
      content: "tail",
      oldLine: 1503,
      newLine: 1503,
    });
  });
});

describe("ReviewHunkDiff", () => {
  it("renders the header and one row per diff line", () => {
    render(
      <ReviewHunkDiff
        before={BASE}
        after={`${BASE}\nTHis is for testing review.\n`}
        oldStart={1}
        newStart={1}
      />,
    );
    const diff = screen.getByTestId("review-hunk-diff");
    expect(within(diff).getByTestId("review-hunk-header")).toHaveTextContent(
      "@@ -1,3 +1,5 @@",
    );
    const rows = within(diff).getAllByTestId("diff-line-row");
    expect(rows.map((row) => row.dataset.lineType)).toEqual([
      "context",
      "context",
      "context",
      "addition",
      "addition",
    ]);
    expect(rows[0]).toHaveTextContent("11 # code");
  });

  it("leaves out the header when the hunk has no anchor", () => {
    render(
      <ReviewHunkDiff
        before={"a\n"}
        after={"b\n"}
        oldStart={null}
        newStart={null}
      />,
    );
    expect(screen.queryByTestId("review-hunk-header")).not.toBeInTheDocument();
    expect(screen.getAllByTestId("diff-line-row")).toHaveLength(2);
  });
});
