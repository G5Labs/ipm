import { describe, expect, it } from "vitest";
import {
  createCommentAnchor,
  originalCommentAnchor,
  resolveCommentAnchor,
  resolveCommentAnchors,
  sha256Utf8,
} from "./commentAnchors";

describe("comment anchors", () => {
  it("keeps an anchor after lines are inserted before it", async () => {
    const anchor = await createCommentAnchor("alpha\ntarget\nomega", 2);
    await expect(
      resolveCommentAnchor("new\nalpha\ntarget\nomega", anchor),
    ).resolves.toEqual({ status: "mapped", lineNumber: 3 });
  });

  it("marks a deleted line as orphaned", async () => {
    const anchor = await createCommentAnchor("alpha\ntarget\nomega", 2);
    await expect(resolveCommentAnchor("alpha\nomega", anchor)).resolves.toEqual(
      {
        status: "orphan",
      },
    );
  });

  it("does not guess between duplicate lines without unique context", async () => {
    const anchor = await createCommentAnchor("{\nsame\n}\n{\nsame\n}", 2);
    await expect(
      resolveCommentAnchor("{\nsame\n}\n{\nsame\n}", {
        ...anchor,
        contentHash: "0".repeat(64),
        anchorContext: null,
      }),
    ).resolves.toEqual({ status: "ambiguous" });
  });

  it("uses unique surrounding context to disambiguate duplicate lines", async () => {
    const anchor = await createCommentAnchor(
      "first\nsame\nafter-a\nsecond\nsame\nafter-b",
      5,
    );
    await expect(
      resolveCommentAnchor(
        "inserted\nfirst\nsame\nafter-a\nsecond\nsame\nafter-b",
        anchor,
      ),
    ).resolves.toEqual({ status: "mapped", lineNumber: 6 });
  });
});

describe("comment anchors follow the re-anchoring evidence order", () => {
  const filler = (count: number, prefix: string) =>
    Array.from({ length: count }, (_, index) => `${prefix} ${index}`);

  it("follows a paragraph moved far by an insertion when both sides still match", async () => {
    const original = [
      "# Doc",
      "",
      "Intro.",
      "",
      "Target paragraph.",
      "",
      "Outro.",
    ].join("\n");
    const anchor = await createCommentAnchor(original, 5);
    const moved = [
      "# Doc",
      ...filler(40, "inserted"),
      "",
      "Intro.",
      "",
      "Target paragraph.",
      "",
      "Outro.",
    ].join("\n");
    await expect(resolveCommentAnchor(moved, anchor)).resolves.toEqual({
      status: "mapped",
      lineNumber: 45,
    });
  });

  it("stays on its line when the paragraph above it is edited", async () => {
    const original = ["Intro.", "", "Target paragraph.", "", "Outro."].join(
      "\n",
    );
    const anchor = await createCommentAnchor(original, 3);
    const edited = [
      "Intro, rewritten.",
      "",
      "Target paragraph.",
      "",
      "Outro.",
    ].join("\n");
    await expect(resolveCommentAnchor(edited, anchor)).resolves.toEqual({
      status: "mapped",
      lineNumber: 3,
    });
  });

  it("orphans a deleted heading instead of jumping to a distant twin", async () => {
    const original = [
      "# Doc",
      "",
      "## Notes",
      "near context",
      ...filler(60, "para"),
      "## Notes",
      "far context",
    ].join("\n");
    const anchor = await createCommentAnchor(original, 3);
    const edited = original
      .split("\n")
      .filter((_, index) => index !== 2)
      .join("\n");
    await expect(resolveCommentAnchor(edited, anchor)).resolves.toEqual({
      status: "orphan",
    });
  });

  it("reports two equally good nearby candidates as ambiguous", async () => {
    const original = ["a", "b", "target", "c", "d"].join("\n");
    const anchor = await createCommentAnchor(original, 3);
    const edited = ["a", "b", "target", "x", "y", "a", "b", "target", "z"].join(
      "\n",
    );
    await expect(resolveCommentAnchor(edited, anchor)).resolves.toEqual({
      status: "ambiguous",
    });
  });

  it("does not choose the nearer of two lines carrying the whole recorded context", async () => {
    const section = ["Intro.", "", "Target paragraph.", "", "Outro."];
    const original = ["# Doc", "", ...section].join("\n");
    const anchor = await createCommentAnchor(original, 5);
    // The section is copied 40 lines below; the first copy stays in place.
    const edited = [
      "# Doc",
      "",
      ...section,
      ...filler(40, "inserted"),
      ...section,
    ].join("\n");
    await expect(resolveCommentAnchor(edited, anchor)).resolves.toEqual({
      status: "ambiguous",
    });
  });

  it("does not move to a lone same-text line whose neighbours both changed", async () => {
    const original = ["one", "two", "}", "three", "four"].join("\n");
    const anchor = await createCommentAnchor(original, 3);
    const edited = ["alpha", "beta", "}", "gamma", "delta"].join("\n");
    await expect(resolveCommentAnchor(edited, anchor)).resolves.toEqual({
      status: "orphan",
    });
  });

  it("keeps a first-line comment when a line is inserted above it", async () => {
    const anchor = await createCommentAnchor("first\nsecond\nthird", 1);
    await expect(
      resolveCommentAnchor("new\nfirst\nsecond\nthird", anchor),
    ).resolves.toEqual({ status: "mapped", lineNumber: 2 });
  });

  it("orphans a last-line comment whose block is deleted beside a twin", async () => {
    // Nothing follows the last line: that empty side is no evidence.
    const original = [
      "function a(){",
      "  x();",
      "}",
      "function b(){",
      "  y();",
      "}",
    ];
    for (const ending of ["", "\n"]) {
      const anchor = await createCommentAnchor(original.join("\n") + ending, 6);
      await expect(
        resolveCommentAnchor(original.slice(0, 3).join("\n") + ending, anchor),
      ).resolves.toEqual({ status: "orphan" });
    }
  });

  it("orphans a first-line comment whose following lines all changed", async () => {
    const anchor = await createCommentAnchor("target\nold-a\nold-b", 1);
    await expect(
      resolveCommentAnchor("target\nnew-a\nnew-b", anchor),
    ).resolves.toEqual({ status: "orphan" });
  });

  it("follows a line-two comment far down: its one-line side is compared as recorded", async () => {
    const original = ["L1", "TARGET", "A", "B"].join("\n");
    const anchor = await createCommentAnchor(original, 2);
    const edited = [...filler(40, "new"), "L1", "TARGET", "A", "B"].join("\n");
    await expect(resolveCommentAnchor(edited, anchor)).resolves.toEqual({
      status: "mapped",
      lineNumber: 42,
    });
  });

  it("keeps a last-line comment when a twin block is appended after it", async () => {
    const original = ["fn a(){", "  x();", "}"].join("\n");
    const anchor = await createCommentAnchor(original, 3);
    const edited = ["fn a(){", "  x();", "}", "fn b(){", "  y();", "}"].join(
      "\n",
    );
    await expect(resolveCommentAnchor(edited, anchor)).resolves.toEqual({
      status: "mapped",
      lineNumber: 3,
    });
  });

  it("takes no evidence from a context side shortened to fit the size bound", async () => {
    const long = "x".repeat(5000);
    const anchor = await createCommentAnchor(`${long}\ntarget\nafter`, 2);
    const context = JSON.parse(anchor.anchorContext);
    expect(context.beforeTruncated).toBe(true);
    // The displayed line equals the shortened record, but the record is not
    // the line it came from: no match, and the other side changed.
    const edited = `${context.before[0]}\ntarget\nsomething else`;
    await expect(resolveCommentAnchor(edited, anchor)).resolves.toEqual({
      status: "orphan",
    });
  });

  it("takes no evidence from a side of blank lines only", async () => {
    const anchor = await createCommentAnchor("\ntarget\na", 2);
    await expect(resolveCommentAnchor("\ntarget\nb", anchor)).resolves.toEqual({
      status: "orphan",
    });
  });

  it("records whether each part was shortened, shortened or not", async () => {
    const anchor = await createCommentAnchor("a\ntarget\nb", 2);
    expect(JSON.parse(anchor.anchorContext)).toMatchObject({
      beforeTruncated: false,
      afterTruncated: false,
      lineTruncated: false,
    });
  });

  it("trusts no part of an unflagged context large enough to have been shortened", async () => {
    // Written by the earlier producer from `x` * 5000 above the line: cut to
    // fit, with no record of the cut.
    const legacy = JSON.stringify({
      version: 1,
      before: ["x".repeat(3976)],
      line: "target",
      after: ["after"],
    });
    const anchor = {
      lineNumber: 2,
      lineHash: await sha256Utf8("target"),
      contentHash: "0".repeat(64),
      anchorContext: legacy,
    };
    await expect(
      resolveCommentAnchor(
        `${"x".repeat(3976)}\ntarget\nsomething else`,
        anchor,
      ),
    ).resolves.toEqual({ status: "orphan" });
  });

  it("still uses an unflagged context too small to have been shortened", async () => {
    const legacy = JSON.stringify({
      version: 1,
      before: ["alpha"],
      line: "target",
      after: ["omega"],
    });
    const anchor = {
      lineNumber: 2,
      lineHash: await sha256Utf8("target"),
      contentHash: "0".repeat(64),
      anchorContext: legacy,
    };
    await expect(
      resolveCommentAnchor("new\nalpha\ntarget\nomega", anchor),
    ).resolves.toEqual({ status: "mapped", lineNumber: 3 });
  });

  it("reports a comment stored without a line anchor as unanchored", async () => {
    await expect(
      resolveCommentAnchor("anything", { lineNumber: 7, lineHash: null }),
    ).resolves.toEqual({ status: "unanchored" });
  });

  it("resolves many comments on one text in order", async () => {
    const text = ["a", "b", "c", "d"].join("\n");
    const anchors = await Promise.all([
      createCommentAnchor(text, 1),
      createCommentAnchor(text, 4),
    ]);
    await expect(
      resolveCommentAnchors(`z\n${text}`, [...anchors, { lineNumber: null }]),
    ).resolves.toEqual([
      { status: "mapped", lineNumber: 2 },
      { status: "mapped", lineNumber: 5 },
      { status: "unanchored" },
    ]);
  });

  it("exposes the original line and its text for an unplaced thread", async () => {
    const anchor = await createCommentAnchor("a\nthe line\nc", 2);
    expect(originalCommentAnchor({ ...anchor })).toEqual({
      lineNumber: 2,
      lineText: "the line",
    });
    expect(originalCommentAnchor({ lineNumber: null })).toEqual({
      lineNumber: null,
      lineText: null,
    });
  });
});
