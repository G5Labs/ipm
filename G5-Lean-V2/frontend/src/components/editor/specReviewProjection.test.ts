import { createHash } from "node:crypto";
import { describe, expect, it, vi } from "vitest";
import type { ReviewSuggestion } from "../../lib/types";
import { parseMarkdownToBlocks } from "../../lib/utils";
import { resolveDisplayAnchor } from "../workspace/ReviewSuggestionOverlay";
import {
  projectSpecReviews,
  type SpecReviewProjection,
} from "./specReviewProjection";
import { reviewEditFixture } from "../../test/reviewEdits";

function suggestion(overrides: Partial<ReviewSuggestion>): ReviewSuggestion {
  return {
    id: "projection-parent",
    publicationId: "publication",
    findingId: null,
    repoId: "docs",
    path: "docs/spec.md",
    kind: "hunk",
    beforeLines: "Original.\n",
    afterLines: "Proposed.\n",
    anchorStart: 1,
    contentHash: null,
    anchorContext: null,
    authorKind: "human",
    authorLabel: "reviewer",
    note: "",
    commentId: null,
    status: "open",
    statusBy: null,
    statusAt: null,
    supersedes: null,
    adoptedFrom: null,
    createdAt: "2026-09-30T12:00:00Z",
    ...overrides,
  };
}

const digest = (content: string) =>
  createHash("sha256").update(content, "utf8").digest("hex");
const project = (
  content: string,
  proposal: ReviewSuggestion,
  contentHash?: string,
) =>
  projectSpecReviews(
    content,
    parseMarkdownToBlocks(content),
    [proposal],
    contentHash,
  )[0];

function expectSourceProjection(
  projection: SpecReviewProjection,
  expected?: string,
) {
  expect(projection.attention).toBeNull();
  expect(projection.sourceDiff).toBe(true);
  expect(projection.changedRange).toBeDefined();
  expect(
    projection.firstBlock !== null || projection.boundaryBlock !== undefined,
  ).toBe(true);
  if (expected !== undefined) {
    const raw = projection.sourcePreview!;
    expect(
      raw.content.slice(0, raw.start) +
        raw.replacement +
        raw.content.slice(raw.end),
    ).toBe(expected);
  }
}

describe("durable independently placed edit ledger", () => {
  it("does not separate competing insertions merely because one is at a block boundary", () => {
    const source = "Intro.\n\nLGTM\n";
    const hash = digest(source);
    const projections = projectSpecReviews(
      source,
      parseMarkdownToBlocks(source),
      [" ", "\n"].map((add, i) =>
        suggestion({
          id: `competitor-${i}`,
          beforeLines: source,
          contentHash: hash,
          edits: [reviewEditFixture(`edit-${i}`, source, 8, 8, add)],
        }),
      ),
      hash,
    );
    expect(projections.map((p) => p.attention)).toEqual([null, null]);
  });
  it("refuses repeated insertion context on a changed source even at the old line", () => {
    const projection = project(
      "A.\nB.\nA.\nB.\n",
      suggestion({
        beforeLines: "",
        afterLines: "\n",
        anchorStart: 2,
        anchorContext: JSON.stringify({
          version: 2,
          leading: ["A."],
          trailing: ["B."],
        }),
      }),
    );
    expect(projection.attention).toMatch(/more than once/);
    expect(projection.firstBlock).toBeNull();
    expect(projection.boundaryBlock).toBeUndefined();
  });
  it.each([
    { source: "Intro.\n\nLGTM\n\nEnd.\n", at: 8, remove: "", add: " " },
    { source: "Intro.\n\nLGTM\n\nEnd.\n", at: 7, remove: "", add: "\n" },
    { source: "Intro.\n\n\nLGTM\n\nEnd.\n", at: 7, remove: "\n", add: "" },
    { source: "LGTM\n", at: 0, remove: "", add: "\n" },
    { source: "LGTM\n", at: 5, remove: "", add: "\n" },
    { source: "LGTM\n\n", at: 5, remove: "\n", add: "" },
    { source: "LGTM", at: 4, remove: "", add: "  " },
    { source: "LGTM\n", at: 4, remove: "\n", add: "" },
    { source: "", at: 0, remove: "", add: "\n" },
    { source: "\n  \n", at: 0, remove: "\n  \n", add: "" },
    { source: "é🦀\r\n\r\nLGTM\r\n", at: 7, remove: "", add: "\t " },
  ])(
    "keeps exact whitespace edit $at/$remove/$add in $source",
    ({ source, at, remove, add }) => {
      expect(source.slice(at, at + remove.length)).toBe(remove);
      const after =
        source.slice(0, at) + add + source.slice(at + remove.length);
      const parent = suggestion({
        beforeLines: source,
        afterLines: after,
        contentHash: digest(source),
        edits: [
          reviewEditFixture("whitespace", source, at, at + remove.length, add),
        ],
      });
      const projection = project(source, parent, digest(source));
      expectSourceProjection(projection, after);
      expect(projection.changedRange).toEqual({
        start: at,
        end: at + remove.length,
      });
      expect(projection.suggestion.reviewEdit!.afterText).toBe(add);
      expect(projection.suggestion.parentAfterLines).toBe(after);
    },
  );

  it("places a source diff between semantic record frames without covering either label", () => {
    const source = "[I1. First]\n\n[D2. Second]\n";
    const at = source.indexOf("[D2.");
    const projection = project(
      source,
      suggestion({
        beforeLines: source,
        contentHash: digest(source),
        edits: [reviewEditFixture("blank", source, at, at, "\n")],
      }),
      digest(source),
    );
    expectSourceProjection(
      projection,
      source.slice(0, at) + "\n" + source.slice(at),
    );
    expect(projection.boundaryBlock).toBe(1);
    expect(projection.coveredBlocks).toEqual([]);
  });

  it("uses a source diff for parser-normalized first-body insertion", () => {
    const source = "[I1. First]\n\n[D2. Second]\n";
    const projection = project(
      source,
      suggestion({
        contentHash: digest(source),
        beforeLines: "",
        afterLines: "  Body.\n\n",
        anchorStart: 3,
      }),
      digest(source),
    );
    expectSourceProjection(projection);
    expect(projection.boundaryBlock).toBe(1);
  });

  it.each(
    ["start", "middle", "end"].flatMap((position) =>
      [false, true].map((identical) => ({ position, identical })),
    ),
  )(
    "places ordered same-parent insertion blocks at $position (identical=$identical)",
    ({ position, identical }) => {
      const source = "Left.\n\nRight.\n";
      const at =
        position === "start" ? 0 : position === "middle" ? 7 : source.length;
      const replacement = (label: string) =>
        position === "end" ? `\n${label}\n` : `${label}\n\n`;
      const deltas = [
        replacement("First added."),
        replacement(identical ? "First added." : "Second added."),
      ];
      const parent = suggestion({
        id: "ordered-insertions",
        beforeLines: source,
        afterLines: source.slice(0, at) + deltas.join("") + source.slice(at),
        contentHash: digest(source),
        edits: deltas.map((delta, index) =>
          reviewEditFixture(`insert-${index}`, source, at, at, delta),
        ),
      });
      const rows = projectSpecReviews(
        source,
        parseMarkdownToBlocks(source),
        [parent],
        digest(source),
      );
      expect(rows.map((row) => row.attention)).toEqual([null, null]);
      expect(rows.map((row) => row.changedRange)).toEqual([
        { start: at, end: at },
        { start: at, end: at },
      ]);
      expect(rows.map((row) => row.suggestion.id)).toEqual([
        "ordered-insertions:insert-0",
        "ordered-insertions:insert-1",
      ]);
      const competitors = projectSpecReviews(
        source,
        parseMarkdownToBlocks(source),
        parent.edits!.map((edit, index) => ({
          ...parent,
          id: `other-${index}`,
          edits: [edit],
        })),
        digest(source),
      );
      expect(competitors.every((row) => row.alternativeIds)).toBe(true);
    },
  );

  it("keeps a same-parent insertion inside a replacement separate", () => {
    const source = "Original words.\n";
    const parent = suggestion({
      beforeLines: source,
      contentHash: digest(source),
      edits: [
        reviewEditFixture("replace", source, 0, 8, "Different"),
        reviewEditFixture("insert", source, 4, 4, "new "),
      ],
    });
    const rows = projectSpecReviews(
      source,
      parseMarkdownToBlocks(source),
      [parent],
      digest(source),
    );
    expect(rows.every((row) => row.alternativeIds)).toBe(true);
  });

  it.each(["Added.\n", ""])(
    "keeps an empty captured edit in attention when the owner reports a source conflict: %j",
    (replacement) => {
      const edit = reviewEditFixture("conflicted", "", 0, 0, replacement, {
        sourceConflict: true,
        commentId: "conflicted-discussion",
      });
      const parent = suggestion({
        id: "conflicted-parent",
        beforeLines: "",
        afterLines: replacement,
        contentHash: digest("Historical.\n"),
        edits: [edit],
      });
      const result = project("", parent, digest(""));
      expect(result.attention).toBe(
        "This edit’s source needs attention after other accepted changes.",
      );
      expect(result.firstBlock).toBeNull();
      expect(result.coveredBlocks).toEqual([]);
      expect(result.after).toEqual([]);
      expect(result.emptyDocument).toBeUndefined();
      expect(result.suggestion.id).toBe("conflicted-parent:conflicted");
      expect(result.suggestion.status).toBe("open");
      expect(result.suggestion.commentId).toBe("conflicted-discussion");
      expect(result.suggestion.reviewEdit).toEqual(edit);
    },
  );

  it.each([false, undefined])(
    "places an ordinary empty-document insertion with sourceConflict=%s",
    (sourceConflict) => {
      const result = project(
        "",
        suggestion({
          beforeLines: "",
          afterLines: "Added.\n",
          contentHash: digest(""),
          edits: [
            reviewEditFixture("ordinary", "", 0, 0, "Added.\n", {
              sourceConflict,
            }),
          ],
        }),
        digest(""),
      );
      expect(result.attention).toBeNull();
      expect(result.emptyDocument).toBe(true);
      expect(result.after).toHaveLength(1);
      expect(result.suggestion.status).toBe("open");
    },
  );

  it.each(["long historical prefix", "é🦀\nchanged"])(
    "uses captured bounds when the selected raw source was restored after %j was historically accepted",
    (acceptedPrefix) => {
      const original = "prefix keep old.\n";
      const historical = acceptedPrefix + original.slice(6);
      const oldAt = original.indexOf("old");
      const currentAt = historical.indexOf("old");
      const bytes = (value: string) => new TextEncoder().encode(value).length;
      const parent = suggestion({
        beforeLines: original,
        expectedBeforeLines: historical,
        contentHash: digest(original),
        edits: [
          reviewEditFixture(
            "accepted-history",
            original,
            0,
            6,
            acceptedPrefix,
            {
              status: "accepted",
              acceptedAfterText: acceptedPrefix,
            },
          ),
          reviewEditFixture(
            "restored-open",
            original,
            oldAt,
            oldAt + 3,
            "new",
            {
              currentStart: bytes(historical.slice(0, currentAt)),
              currentEnd: bytes(historical.slice(0, currentAt + 3)),
              sourceConflict: true,
            },
          ),
        ],
      });
      const result = project(original, parent, digest(original));
      expect(result.attention).toBeNull();
      expect(result.changedRange).toEqual({ start: oldAt, end: oldAt + 3 });
      expect(result.suggestion.reviewEdit?.sourceConflict).toBe(true);
      expect(result.suggestion.status).toBe("open");
      expect(result.suggestion.id).toBe("projection-parent:restored-open");
      expect(parent.edits![0].status).toBe("accepted");
      for (const unproven of [undefined, digest(historical)])
        expect(project(original, parent, unproven).attention).toMatch(
          /source needs attention/,
        );
    },
  );

  it("uses exact captured proof for a restored empty insertion while retaining no-op attention", () => {
    const parent = suggestion({
      beforeLines: "",
      expectedBeforeLines: "Historical text.\n",
      contentHash: digest(""),
      edits: [
        reviewEditFixture("empty-restored", "", 0, 0, "Added.\n", {
          sourceConflict: true,
          currentStart: 17,
          currentEnd: 17,
        }),
      ],
    });
    expect(project("", parent, digest("")).attention).toBeNull();
    expect(project("", parent, digest("")).emptyDocument).toBe(true);
    parent.edits![0].afterText = "";
    expect(project("", parent, digest("")).attention).toMatch(
      /already matches/,
    );
  });

  it("projects compact children from the one shared recomposed parent source", () => {
    const original = "Alpha unchanged Omega.\n";
    const current = "B unchanged Omega.\n";
    const at = current.indexOf("Omega");
    const parent = suggestion({
      beforeLines: original,
      afterLines: "B unchanged Gamma.\n",
      expectedBeforeLines: current,
      edits: [
        reviewEditFixture(
          "compact",
          original,
          original.indexOf("Omega"),
          original.indexOf("Omega") + 5,
          "Gamma",
          {
            currentStart: at,
            currentEnd: at + 5,
            expectedBeforeLines: undefined,
            expectedAfterLines: undefined,
          },
        ),
      ],
    });
    const result = project(current, parent, digest(current));
    expect(result.attention).toBeNull();
    expect(result.changedRange).toEqual({ start: at, end: at + 5 });
    expect(result.suggestion.beforeLines).toBe(current);
    expect(result.suggestion.afterLines).toBe("Gamma");
  });

  it("bounds a large ledger without hiding any independent edit or copying parent previews", () => {
    const source = "Alpha ".repeat(10000) + "\n";
    const edits = Array.from({ length: 1000 }, (_, i) =>
      reviewEditFixture(`edit-${i}`, source, i * 6, i * 6 + 5, "Beta", {
        expectedBeforeLines: undefined,
        expectedAfterLines: undefined,
      }),
    );
    const rows = projectSpecReviews(
      source,
      parseMarkdownToBlocks(source),
      [suggestion({ beforeLines: source, expectedBeforeLines: source, edits })],
      digest(source),
    );
    expect(rows).toHaveLength(1000);
    expect(new Set(rows.map((row) => row.suggestion.id)).size).toBe(1000);
    expect(
      rows.every(
        (row) =>
          row.attention?.includes("individual edits") &&
          row.before.length === 0 &&
          row.after.length === 0,
      ),
    ).toBe(true);
    expect(
      rows.every(
        (row) =>
          row.suggestion.afterLines === "Beta" &&
          row.suggestion.beforeLines === source,
      ),
    ).toBe(true);
  });

  it("projects disjoint same-paragraph children with their durable identities", () => {
    const source = "Alpha unchanged middle clause Omega.\n";
    const omega = source.indexOf("Omega");
    const parent = suggestion({
      id: "published-parent",
      beforeLines: source,
      afterLines: "Beta unchanged middle clause Gamma.\n",
      contentHash: digest(source),
      edits: [
        reviewEditFixture("edit-alpha", source, 0, 5, "Beta"),
        reviewEditFixture("edit-omega", source, omega, omega + 5, "Gamma"),
      ],
    });
    const projected = projectSpecReviews(
      source,
      parseMarkdownToBlocks(source),
      [parent],
      digest(source),
    );
    expect(projected.map((row) => row.suggestion.id)).toEqual([
      "published-parent:edit-alpha",
      "published-parent:edit-omega",
    ]);
    expect(projected.map((row) => row.attention)).toEqual([null, null]);
    expect(projected.map((row) => row.firstBlock)).toEqual([0, 0]);
    expect(projected.map((row) => row.changedRange)).toEqual([
      { start: 0, end: 5 },
      { start: omega, end: omega + 5 },
    ]);
  });

  it("retains an open sibling after a changed-length acceptance advances canonical source", () => {
    const original = "Alpha unchanged middle clause Omega.\n";
    const current = "B unchanged middle clause Omega.\n";
    const oldOmega = original.indexOf("Omega"),
      omega = current.indexOf("Omega");
    const parent = suggestion({
      beforeLines: original,
      afterLines: "B unchanged middle clause Gamma.\n",
      contentHash: digest(original),
      edits: [
        reviewEditFixture("accepted-alpha", original, 0, 5, "B", {
          status: "accepted",
          acceptedAfterText: "B",
          statusAt: "2026-10-01T16:09:00Z",
        }),
        reviewEditFixture(
          "pending-omega",
          original,
          oldOmega,
          oldOmega + 5,
          "Gamma",
          {
            currentStart: omega,
            currentEnd: omega + 5,
            expectedBeforeLines: current,
            expectedAfterLines: "B unchanged middle clause Gamma.\n",
          },
        ),
      ],
    });
    const projected = projectSpecReviews(
      current,
      parseMarkdownToBlocks(current),
      [parent],
      digest(current),
    );
    expect(projected).toHaveLength(1);
    expect(projected[0].suggestion.id).toBe("projection-parent:pending-omega");
    expect(projected[0].attention).toBeNull();
    expect(projected[0].changedRange).toEqual({ start: omega, end: omega + 5 });
  });

  it("gives a replacement and nearby added block separate independently placeable children", () => {
    const source = "new line from reviewer.\n";
    const parent = suggestion({
      beforeLines: source,
      afterLines:
        "suggest to add this line from reviewer.\n\n> one more thing....\n",
      contentHash: digest(source),
      edits: [
        reviewEditFixture("replacement", source, 0, 3, "suggest to add this"),
        reviewEditFixture(
          "added-block",
          source,
          source.length,
          source.length,
          "\n> one more thing....\n",
        ),
      ],
    });
    const projected = projectSpecReviews(
      source,
      parseMarkdownToBlocks(source),
      [parent],
      digest(source),
    );
    expect(projected.map((row) => row.suggestion.id)).toEqual([
      "projection-parent:replacement",
      "projection-parent:added-block",
    ]);
    expect(projected.map((row) => row.attention)).toEqual([null, null]);
  });

  it("maps Unicode byte boundaries precisely and refuses malformed child offsets", () => {
    const source = "😀 漢字 old text.\n",
      start = source.indexOf("old");
    const parent = suggestion({
      beforeLines: source,
      afterLines: source.replace("old", "new"),
      contentHash: digest(source),
      edits: [
        reviewEditFixture("unicode-edit", source, start, start + 3, "new"),
      ],
    });
    const valid = projectSpecReviews(
      source,
      parseMarkdownToBlocks(source),
      [parent],
      digest(source),
    )[0];
    expect(valid.attention).toBeNull();
    expect(valid.changedRange).toEqual({ start, end: start + 3 });
    parent.edits![0].currentStart = 1;
    // Exact captured proof uses valid immutable bounds, not historical offsets.
    expect(project(source, parent, digest(source)).attention).toBeNull();
    expect(
      projectSpecReviews(source, parseMarkdownToBlocks(source), [parent])[0]
        .attention,
    ).toMatch(/verified source range/);
    parent.edits![0].start = 1;
    expect(project(source, parent, digest(source)).attention).toMatch(
      /source needs attention/,
    );
  });
});

describe("Shared preview review projection placement and representation", () => {
  it("keeps legacy same-block proposals as verified alternatives", () => {
    const content = "Alpha middle Omega.\n";
    const proposals = [
      suggestion({
        id: "first-word-parent",
        beforeLines: content,
        afterLines: "Beta middle Omega.\n",
      }),
      suggestion({
        id: "last-word-parent",
        beforeLines: content,
        afterLines: "Alpha middle Gamma.\n",
      }),
    ];
    const projections = projectSpecReviews(
      content,
      parseMarkdownToBlocks(content),
      proposals,
    );
    expect(projections.map((p) => p.attention)).toEqual([null, null]);
    expect(projections.map((p) => p.firstBlock)).toEqual([0, 0]);
    expect(projections.every((p) => p.alternativeIds)).toBe(true);
    expect(projections.map((p) => p.suggestion.id)).toEqual([
      "first-word-parent",
      "last-word-parent",
    ]);
  });

  it.each([false, true])(
    "keeps legacy parents on distinct lines as verified alternatives: %s",
    (foldContext) => {
      const content = "Alpha.\nOmega.\n";
      const hash = digest(content);
      const projections = projectSpecReviews(
        content,
        parseMarkdownToBlocks(content),
        [
          suggestion({
            id: "first-line-parent",
            contentHash: hash,
            beforeLines: foldContext ? content : "Alpha.\n",
            afterLines: foldContext ? "Beta.\nOmega.\n" : "Beta.\n",
          }),
          suggestion({
            id: "last-line-parent",
            contentHash: hash,
            beforeLines: foldContext ? content : "Omega.\n",
            afterLines: foldContext ? "Alpha.\nGamma.\n" : "Gamma.\n",
            anchorStart: foldContext ? 1 : 2,
          }),
        ],
        hash,
      );
      expect(projections.map((p) => p.attention)).toEqual([null, null]);
      expect(projections.map((p) => p.coveredBlocks)).toEqual([[0], [0]]);
      expect(projections.every((p) => p.alternativeIds)).toBe(true);
      expect(projections.map((p) => p.suggestion.id)).toEqual([
        "first-line-parent",
        "last-line-parent",
      ]);
    },
  );

  it("keeps separate parents affecting different rich blocks inline", () => {
    const content = "Alpha.\n\nOmega.\n";
    const projections = projectSpecReviews(
      content,
      parseMarkdownToBlocks(content),
      [
        suggestion({
          id: "first",
          beforeLines: "Alpha.\n",
          afterLines: "Beta.\n",
        }),
        suggestion({
          id: "last",
          beforeLines: "Omega.\n",
          afterLines: "Gamma.\n",
          anchorStart: 3,
        }),
      ],
    );
    expect(projections.map((p) => p.attention)).toEqual([null, null]);
    expect(projections.map((p) => p.firstBlock)).toEqual([0, 1]);
  });

  it("keeps a single parent with multiple edits in one rich block inline", () => {
    const content = "Alpha middle Omega.\n";
    expect(
      project(
        content,
        suggestion({
          beforeLines: content,
          afterLines: "Beta middle Gamma.\n",
        }),
      ).attention,
    ).toBeNull();
  });

  it.each([false, true])(
    "attaches first-body insertion to the preceding unchanged label-only record: %s",
    (pureInsertion) => {
      const content = "[I1. First]\n\n[D2. Second]\n";
      const hash = digest(content);
      const proposal = suggestion({
        contentHash: hash,
        beforeLines: pureInsertion ? "" : content,
        afterLines: pureInsertion
          ? "New body.\n\n"
          : "[I1. First]\n\nNew body.\n\n[D2. Second]\n",
        anchorStart: pureInsertion ? 3 : 1,
      });
      const projection = project(content, proposal, hash);
      expect(projection.attention).toBeNull();
      expect(projection.firstBlock).toBe(0);
      expect(projection.coveredBlocks).toEqual([0]);
      expect(projection.before).toEqual([]);
      expect(projection.beforeBlockIndices).toEqual([]);
      expect(projection.after).toEqual([
        expect.objectContaining({ type: "paragraph", content: "New body." }),
      ]);
      expect(projection.suggestion).toBe(proposal);
    },
  );

  it("keeps an actual record label replacement in attention", () => {
    const content = "[I1. First]\n\n[D2. Second]\n";
    expect(
      project(
        content,
        suggestion({
          beforeLines: content,
          afterLines: "[I1. Renamed]\n\n[D2. Second]\n",
        }),
      ).attention,
    ).toMatch(/crosses document records/);
  });

  it.each([
    "[D3. New record]\n\n",
    "## New section\n\n",
    "```g5-question\nid: q-new\nquestion: Why?\nstatus: open\n```\n\n",
  ])(
    "keeps unsupported first-body insertions in attention: %j",
    (afterLines) => {
      const content = "[I1. First]\n\n[D2. Second]\n";
      const hash = digest(content);
      const projection = project(
        content,
        suggestion({
          contentHash: hash,
          beforeLines: "",
          afterLines,
          anchorStart: 3,
        }),
        hash,
      );
      expect(projection.attention).not.toBeNull();
      expect(projection.firstBlock).toBeNull();
      expect(projection.suggestion.afterLines).toBe(afterLines);
    },
  );

  it("does not reassign a separator to a preceding record that already has a body", () => {
    const content = "[I1. First]\nExisting body.\n\n[D2. Second]\n";
    const hash = digest(content);
    expect(
      project(
        content,
        suggestion({
          contentHash: hash,
          beforeLines: "",
          afterLines: "Another body.\n\n",
          anchorStart: 4,
        }),
        hash,
      ).attention,
    ).toMatch(/crosses document records/);
  });

  it("preserves the existing label-only EOF first-body insertion", () => {
    const content = "[I1. First]\n";
    const hash = digest(content);
    const projection = project(
      content,
      suggestion({
        contentHash: hash,
        beforeLines: "",
        afterLines: "New body.\n",
        anchorStart: 2,
      }),
      hash,
    );
    expect(projection.attention).toBeNull();
    expect(projection.firstBlock).toBe(0);
    expect(projection.beforeBlockIndices).toEqual([]);
  });

  it.each([
    ["Beta middle Omega.\n", "Gamma middle Omega.\n"],
    ["Alpha first middle Omega.\n", "Alpha second middle Omega.\n"],
    ["# Alpha middle Omega.\n", "Beta middle Omega.\n"],
  ])(
    "keeps verified same-word, insertion, and structural alternatives in place",
    (first, second) => {
      const content = "Alpha middle Omega.\n";
      const projections = projectSpecReviews(
        content,
        parseMarkdownToBlocks(content),
        [first, second].map((afterLines, i) =>
          suggestion({
            id: `conflicting-parent-${i}`,
            beforeLines: content,
            afterLines,
          }),
        ),
      );
      expect(projections.map((p) => p.attention)).toEqual([null, null]);
      expect(projections.map((p) => p.firstBlock)).toEqual([0, 0]);
      expect(projections.every((p) => p.alternativeIds)).toBe(true);
    },
  );

  it.each([
    ["A.\n\nB.", "A.\n\n\nExtra.\n\nB."],
    ["A.\n\nB.", "A.\n\nExtra.\n\n\nB."],
    ["Old start.\n\nExtra.\n\nOld end.\n", "New start.\n\n\nNew end.\n"],
    ["Old start.\n\n\nExtra.\n\nOld end.\n", "New start.\n\nNew end.\n"],
  ])(
    "retains raw context for hidden separators beside an inserted or removed row",
    (content, afterLines) => {
      const projection = project(
        content,
        suggestion({ beforeLines: content, afterLines }),
      );
      expectSourceProjection(projection, afterLines);
      expect(projection.suggestion.afterLines).toBe(afterLines);
    },
  );

  it.each([true, false])(
    "keeps ordinary insertion and removal separators inline: %s",
    (insert) => {
      const without = "Old start.\n\nOld end.\n";
      const withRow = "Old start.\n\nExtra.\n\nOld end.\n";
      const content = insert ? without : withRow;
      const afterLines = (insert ? withRow : without)
        .replace("Old start.", "New start.")
        .replace("Old end.", "New end.");
      expect(
        project(content, suggestion({ beforeLines: content, afterLines }))
          .attention,
      ).toBeNull();
    },
  );

  it.each([true, false])(
    "retains parser-normalized bytes inside an unpaired row: %s",
    (insert) => {
      const without = "A.\n\nB.\n";
      const withRow = "A.\n\n  Extra.  \n\nB.\n";
      const content = insert ? without : withRow;
      const afterLines = insert ? withRow : without;
      expectSourceProjection(
        project(content, suggestion({ beforeLines: content, afterLines })),
        afterLines,
      );
    },
  );

  it.each(["  Extra.\n", "Extra.  \n", "\nExtra.\n", "Extra.\n\n\n"])(
    "checks raw fidelity for inserted rows in an empty document: %j",
    (afterLines) => {
      expectSourceProjection(
        project(
          "",
          suggestion({ beforeLines: "", afterLines, contentHash: digest("") }),
          digest(""),
        ),
        afterLines,
      );
    },
  );

  it("keeps visible code whitespace in unpaired rows inline", () => {
    const content = "Old start.\n\nOld end.\n";
    const afterLines =
      "New start.\n\n```text\n  visible whitespace  \n```\n\nNew end.\n";
    expect(
      project(content, suggestion({ beforeLines: content, afterLines }))
        .attention,
    ).toBeNull();
    expect(
      project(
        afterLines,
        suggestion({ beforeLines: afterLines, afterLines: content }),
      ).attention,
    ).toBeNull();
  });

  it("keeps a shifted pure insertion in attention when Accept cannot relocate it", () => {
    const proposal = suggestion({
      beforeLines: "",
      afterLines: "Added.\n\n",
      anchorStart: 3,
      anchorContext: JSON.stringify({
        version: 2,
        leading: ["First.", ""],
        trailing: ["Second."],
      }),
    });
    const moved = project("# Intro\n\nFirst.\n\nSecond.\n", proposal);
    expect(moved.attention).toMatch(/insertion context has changed/);
    expect(moved.firstBlock).toBeNull();
    const inPlace = project("First.\n\nSecond.\n", proposal);
    expect(inPlace.attention).toBeNull();
    expect(inPlace.after.map((block) => block.type)).toEqual([
      "paragraph",
      "paragraph",
    ]);
  });

  it("uses an exact raw digest to place the published occurrence among duplicates", () => {
    const content = "Same.\n\nSame.\n";
    const proposal = suggestion({
      beforeLines: "Same.\n",
      afterLines: "Revised.\n",
      anchorStart: 3,
      contentHash: digest(content),
    });
    const trusted = project(content, proposal, digest(content));
    expect(trusted.attention).toBeNull();
    expect(trusted.firstBlock).toBe(1);
    expect(trusted.beforeBlockIndices).toEqual([1]);
    expect(project(content, proposal).attention).toMatch(/more than once/);
    expect(
      project(content, proposal, digest(content.replace(/\n$/, ""))).attention,
    ).toMatch(/more than once/);
    expect(
      project(
        "Preface.\n\n" + content,
        proposal,
        digest("Preface.\n\n" + content),
      ).attention,
    ).toMatch(/more than once/);
  });

  it("accepts a trusted published insertion without requiring unique context", () => {
    const content = "First.\n\nSecond.\n";
    const proposal = suggestion({
      beforeLines: "",
      afterLines: "Added.\n\n",
      anchorStart: 3,
      contentHash: digest(content),
    });
    expect(project(content, proposal, digest(content)).attention).toBeNull();
    expect(project(content, proposal).attention).toMatch(/context has changed/);
    expect(
      project(content, { ...proposal, anchorStart: 99 }, digest(content))
        .attention,
    ).not.toBeNull();
  });

  it("does not promise a write when untrusted Accept sees the full replacement already present", () => {
    for (const beforeLines of ["Original.\n", ""]) {
      const content = "Original.\n\nProposed.\n";
      const proposal = suggestion({
        beforeLines,
        afterLines: "Proposed.\n",
        anchorContext: JSON.stringify({
          version: 2,
          leading: [],
          trailing: ["Original."],
        }),
      });
      expect(project(content, proposal).attention).toMatch(/already appears/);
    }
    const content = "Original.\n\nProposed.\n";
    expect(
      project(
        content,
        suggestion({ contentHash: digest(content) }),
        digest(content),
      ).attention,
    ).toBeNull();
  });

  it("does not promise a duplicate insertion when trusted Accept already sees its replacement at the anchor", () => {
    const content = "Proposed.\n\nOther.\n";
    const proposal = suggestion({
      beforeLines: "",
      contentHash: digest(content),
    });
    expect(project(content, proposal, digest(content)).attention).toMatch(
      /already appears/,
    );
  });

  it.each([
    ["First.\nSecond.\n", "First.  \nSecond.\n"],
    ["First.  \nSecond.\n", "First.\nSecond.\n"],
    ["First.\n\nSecond.\n", "First.\n\n\nSecond.\n"],
    [
      "Old start.\n\nMiddle.\nContinued.\n\nOld end.\n",
      "New start.\n\nMiddle.  \nContinued.\n\nNew end.\n",
    ],
    ["Old start.\nSecond.\n", "New start.  \nSecond.\n"],
  ])(
    "retains raw context when the parser hides changed source bytes",
    (content, afterLines) => {
      const projection = project(
        content,
        suggestion({ beforeLines: content, afterLines }),
      );
      expectSourceProjection(projection, afterLines);
      expect(projection.suggestion.afterLines).toBe(afterLines);
    },
  );

  it("keeps visible edits inline when normalized whitespace nearby remains unchanged", () => {
    const content = "Old start.\n\nMiddle.  \nContinued.\n\nOld end.\n";
    const projection = project(
      content,
      suggestion({
        beforeLines: content,
        afterLines: content
          .replace("Old start", "New start")
          .replace("Old end", "New end"),
      }),
    );
    expect(projection.attention).toBeNull();
    expect(projection.beforeBlockIndices).toEqual([0, 1, 2]);
  });

  it.each([true, false])(
    "checks aligned untouched separators when a paragraph is added: %s",
    (addParagraph) => {
      const without = "Old start.\n\nMiddle one.\n\nMiddle two.\n\nOld end.\n";
      const withParagraph = without.replace(
        "Old start.\n\n",
        "Old start.\n\nExtra.\n\n",
      );
      const content = addParagraph ? without : withParagraph;
      const visibleEdit = (addParagraph ? withParagraph : without)
        .replace("Old start.", "New start.")
        .replace("Old end.", "New end.");
      const hiddenGap = visibleEdit.replace(
        "Middle one.\n\n",
        "Middle one.\n\n\n",
      );
      const projection = project(
        content,
        suggestion({ beforeLines: content, afterLines: hiddenGap }),
      );
      expectSourceProjection(projection, hiddenGap);
      expect(projection.suggestion.afterLines).toBe(hiddenGap);
      expect(
        project(
          content,
          suggestion({ beforeLines: content, afterLines: visibleEdit }),
        ).attention,
      ).toBeNull();
    },
  );

  it("does not compare unrelated ordinal gaps when aligned paragraphs move", () => {
    const content = "Old start.\n\nA.\n\nB.\n\nOld end.\n";
    const afterLines = "New start.\n\nB.\n\nA.\n\nNew end.\n";
    expect(
      project(content, suggestion({ beforeLines: content, afterLines }))
        .attention,
    ).toBeNull();
  });

  it.each(["\n", "  \n"])(
    "retains a raw-only insertion into an empty document: %j",
    (afterLines) => {
      const proposal = suggestion({
        beforeLines: "",
        afterLines,
        contentHash: digest(""),
      });
      expectSourceProjection(project("", proposal, digest("")), afterLines);
    },
  );

  it.each([false, true])(
    "keeps live question blocks canonical when a spanning proposal changes the question: %s",
    (changeQuestion) => {
      const question =
        "```G5-Question\nid: q-one\nquestion: Why?\nstatus: open\n```";
      const content = `Old start.\n\n${question}\n\nOld end.\n`;
      const afterLines = content
        .replace("Old start", "New start")
        .replace("Old end", "New end")
        .replace("Why?", changeQuestion ? "How?" : "Why?");
      const projection = project(
        content,
        suggestion({ beforeLines: content, afterLines }),
      );
      expect(projection.attention).toMatch(/question/);
      expect(projection.coveredBlocks).toEqual([]);
      expect(projection.firstBlock).toBeNull();
    },
  );

  it("maps original rows to canonical indices without giving inserted rows existing identities", () => {
    const content = "Old start.\n\nMiddle.\n\nOld end.\n";
    const projection = project(
      content,
      suggestion({
        beforeLines: content,
        afterLines: "New start.\n\nInserted.\n\nMiddle.\n\nNew end.\n",
      }),
    );
    expect(projection.attention).toBeNull();
    expect(projection.beforeBlockIndices).toEqual([0, 1, 2]);
    expect(projection.after).toHaveLength(4);
    const record = "[I1. Intent]\nBody.\n";
    const body = project(
      record,
      suggestion({
        beforeLines: record,
        afterLines: "[I1. Intent]\nUpdated.\n",
      }),
    );
    expect(body.attention).toBeNull();
    expect(body.beforeBlockIndices).toEqual([0]);
    expect(body.before[0]).toMatchObject({ content: "Body." });
  });

  it("bounds repeated-line and context matching on both production consumers", () => {
    const content = Array(2_000).fill("Repeat.").join("\n");
    const proposal = suggestion({
      beforeLines: Array(1_000).fill("Repeat.").join("\n"),
      afterLines: "Revised.\n",
      anchorContext: JSON.stringify({
        version: 2,
        leading: Array(500).fill("Repeat."),
        trailing: ["Missing."],
      }),
    });
    let comparisons = 0;
    const every = Array.prototype.every;
    const spy = vi.spyOn(Array.prototype, "every").mockImplementation(function (
      this: unknown[],
      predicate,
      thisArg,
    ) {
      return every.call(this, (value, index, array) => {
        comparisons++;
        return predicate.call(thisArg, value, index, array);
      });
    });
    try {
      expect(project(content, proposal).attention).not.toBeNull();
      expect(resolveDisplayAnchor(proposal, content).state).toBe("ambiguous");
      expect(comparisons).toBeLessThan(20_000);
    } finally {
      spy.mockRestore();
    }
  });
});

describe("Shared preview inline source visibility", () => {
  it.each([
    ["https://one.example", "https://two.example"],
    ["g5://file/repo/one.md", "g5://file/repo/two.md"],
  ])(
    "keeps unchanged-label target reassociation in attention: %s",
    (one, two) => {
      const content = `Read A [B](${one}) and [A](${two}).\n`;
      const h = digest(content);
      const projection = project(
        content,
        suggestion({
          beforeLines: content,
          afterLines: `Read [A](${one}) [B](${two}) and B.\n`,
          contentHash: h,
        }),
        h,
      );
      expect(projection.attention).not.toBeNull();
      expect(projection.firstBlock).toBeNull();
    },
  );

  it.each([
    [
      "Read [A](https://one.example) and [B](https://two.example).\n",
      "Read [B](https://one.example) and [A](https://two.example).\n",
    ],
    [
      "Read [A](https://one.example) B.\n",
      "Read A [B](https://one.example).\n",
    ],
    [
      "Read [A](https://one.example) and [B](https://two.example).\n",
      "Please read [A](https://one.example) and [B](https://two.example).\n",
    ],
    [
      "Read [old unchanged suffix](https://same.example).\n",
      "Read [new unchanged suffix](https://same.example).\n",
    ],
  ])(
    "keeps fully marked associations and unchanged keep targets inline: %s",
    (content, afterLines) => {
      const h = digest(content);
      expect(
        project(
          content,
          suggestion({ beforeLines: content, afterLines, contentHash: h }),
          h,
        ).attention,
      ).toBeNull();
    },
  );

  it.each([
    [
      "Read [docs](https://old.example/path).\n",
      "Read [docs](https://new.example/path).\n",
    ],
    [
      "Read [docs](https://old.example/path).\n",
      "Please read [docs](https://new.example/path).\n",
    ],
    [
      "Open [file](g5://file/repo/old.md).\n",
      "Open [file](g5://file/repo/new.md).\n",
    ],
    [
      "Read [one](https://one.example) and [two](https://old.example).\n",
      "Read [first](https://one.example) and [two](https://new.example).\n",
    ],
    ["Read docs.\n", "Read [docs](https://new.example).\n"],
    ["Read [docs](https://old.example).\n", "Read docs.\n"],
  ])(
    "keeps changed non-visible link destinations in raw attention: %s",
    (content, afterLines) => {
      const h = digest(content);
      const s = suggestion({
        beforeLines: content,
        afterLines,
        contentHash: h,
      });
      const x = project(content, s, h);
      expect(x.attention).not.toBeNull();
      expect(x.firstBlock).toBeNull();
      expect(x.suggestion.beforeLines).toBe(content);
      expect(x.suggestion.afterLines).toBe(afterLines);
    },
  );
  it.each([
    [
      "Read [docs](https://same.example).\n",
      "Please read [guides](https://same.example).\n",
    ],
    [
      "Open [file](g5://file/repo/same.md).\n",
      "Open [source](g5://file/repo/same.md).\n",
    ],
    ["Read **old**.\n", "Read *new*.\n"],
    ["Read plain.\n", "Read `plain`.\n"],
    [
      "Read `[docs](https://old.example)`.\n",
      "Read `[docs](https://new.example)`.\n",
    ],
    [
      "```text\n[docs](https://old.example)\n```\n",
      "```text\n[docs](https://new.example)\n```\n",
    ],
    [
      "Open [intent](g5://ontology/module#I1).\n",
      "Open [intent](g5://ontology/module#D2).\n",
    ],
    ["Read [docs](javascript:old).\n", "Read [docs](javascript:new).\n"],
    ["- [docs](https://old.example)\n", "- [docs](https://new.example)\n"],
  ])(
    "keeps actual visible source/unchanged-target controls inline: %s",
    (content, afterLines) => {
      const h = digest(content);
      expect(
        project(
          content,
          suggestion({ beforeLines: content, afterLines, contentHash: h }),
          h,
        ).attention,
      ).toBeNull();
    },
  );
});

describe("competing preview groups", () => {
  it.each([3, 12, 60])(
    "keeps %s authors at one verified Unicode range without changing their edits",
    (count) => {
      const content = "Intro 🦉.\n\nhuman-provided\n\nEnd.\n";
      const start = content.indexOf("human");
      const proposals = Array.from({ length: count }, (_, i) =>
        suggestion({
          id: `author-${i}`,
          beforeLines: content,
          contentHash: digest(content),
          edits: [
            reviewEditFixture(
              `edit-${i}`,
              content,
              start,
              start + 5,
              `person-${i}`,
            ),
          ],
        }),
      );
      for (const ordered of [proposals, [...proposals].reverse()]) {
        const projected = projectSpecReviews(
          content,
          parseMarkdownToBlocks(content),
          ordered,
          digest(content),
        );
        expect(projected).toHaveLength(count);
        for (const p of projected) {
          expect(p.attention).toBeNull();
          expect(p.alternativeIds).toContain(p.suggestion.id);
          expect(p.firstBlock).toBe(1);
          expect(p.coveredBlocks).toEqual([1]);
          expect(
            p.sourcePreview!.content.slice(
              p.sourcePreview!.start,
              p.sourcePreview!.end,
            ),
          ).toBe("human");
          expect(p.sourcePreview!.replacement).toBe(
            p.suggestion.reviewEdit!.afterText,
          );
        }
      }
    },
  );
  it("unifies a chain of four overlapping ranges in any arrival order but excludes unverified edits", () => {
    const content = "Alpha.\n\nBravo.\n\nCharlie.\n\nDelta.\n\nEnd.\n";
    const words = ["Alpha", "Bravo", "Charlie", "Delta"];
    const proposals = words.map((word, i) => {
      const start = content.indexOf(word);
      const end =
        i === words.length - 1
          ? start + word.length
          : content.indexOf(words[i + 1]) + 3;
      return suggestion({
        id: word,
        beforeLines: content,
        contentHash: digest(content),
        edits: [
          reviewEditFixture(
            `${word}-edit`,
            content,
            start,
            end,
            ` ${word} replacement`,
          ),
        ],
      });
    });
    const stale = suggestion({
      id: "stale",
      beforeLines: "Missing.\n",
      afterLines: "Elsewhere.\n",
    });
    for (const ordered of [
      proposals,
      [...proposals].reverse(),
      [proposals[2], proposals[0], proposals[3], proposals[1]],
    ]) {
      const projected = projectSpecReviews(
        content,
        parseMarkdownToBlocks(content),
        [...ordered, stale],
        digest(content),
      );
      for (const p of projected.slice(0, 4)) {
        expect(p.alternativeIds).toContain(p.suggestion.id);
        expect(p.attention).toBeNull();
        expect(p.firstBlock).toBe(0);
        expect(p.coveredBlocks).toEqual([0, 1, 2, 3]);
        expect(p.sourcePreview!.rangeStart).toBe(0);
        expect(p.sourcePreview!.rangeEnd).toBeGreaterThanOrEqual(
          content.indexOf("Delta") + 5,
        );
        expect(p.sourcePreview!.rangeEnd).toBeLessThan(content.indexOf("End"));
      }
      expect(projected[4].attention).toBeTruthy();
      expect(projected[4].alternativeIds).toBeUndefined();
      expect(projected[4].coveredBlocks).toEqual([]);
    }
  });
});

it("distinguishes two conflict components from a compatible member of their display group", () => {
  const content = "Alpha middle Omega ending Final.\n";
  const proposals = [
    ["a1", "Alpha", "North"],
    ["a2", "Alpha", "South"],
    ["b1", "Omega", "East"],
    ["b2", "Omega", "West"],
    ["solo", "Final", "Done"],
  ].map(([id, word, replacement]) =>
    suggestion({
      id,
      beforeLines: content,
      contentHash: digest(content),
      edits: [
        reviewEditFixture(
          `${id}-edit`,
          content,
          content.indexOf(word),
          content.indexOf(word) + word.length,
          replacement,
        ),
      ],
    }),
  );
  const rows = projectSpecReviews(
    content,
    parseMarkdownToBlocks(content),
    proposals,
    digest(content),
  );
  expect(rows.map((p) => p.alternativeIds)).toEqual([
    ["a1:a1-edit", "a2:a2-edit"],
    ["a1:a1-edit", "a2:a2-edit"],
    ["b1:b1-edit", "b2:b2-edit"],
    ["b1:b1-edit", "b2:b2-edit"],
    undefined,
  ]);
  expect(rows.every((p) => p.attention === null && p.firstBlock === 0)).toBe(
    true,
  );
  expect(rows.map((p) => p.coveredBlocks)).toEqual([[0], [0], [0], [0], [0]]);
});
