import { describe, expect, it } from "vitest";
import { parseMarkdownToBlocks } from "../../lib/utils";
import {
  blockIndexAtLine,
  lineOfOffset,
  selectionSourceLine,
} from "./documentSource";

const content =
  "# Doc\n\nFirst paragraph.\n\nSecond paragraph with **bold words**\nruns on to a [link](https://example.com) here.\n\n- item one\n- item two\n";
const blocks = parseMarkdownToBlocks(content);

describe("selectionSourceLine", () => {
  it("finds the block's line showing the start of the selection", () => {
    const paragraph = blocks[blockIndexAtLine(content, blocks, 5)];
    expect(selectionSourceLine(content, paragraph, "Second paragraph")).toBe(5);
    expect(selectionSourceLine(content, paragraph, "runs on to a")).toBe(6);
    // A selection across the wrapped source lines lands on its first line.
    expect(selectionSourceLine(content, paragraph, "bold words runs on")).toBe(
      5,
    );
  });

  it("reads through inline markup by the selection's first word", () => {
    const paragraph = blocks[blockIndexAtLine(content, blocks, 5)];
    expect(selectionSourceLine(content, paragraph, "bold words")).toBe(5);
    expect(selectionSourceLine(content, paragraph, "link here")).toBe(6);
  });

  it("falls back to the block's first line when the rendering hides the source", () => {
    const paragraph = blocks[blockIndexAtLine(content, blocks, 5)];
    expect(selectionSourceLine(content, paragraph, "zzz")).toBe(5);
    expect(selectionSourceLine(content, paragraph, "   ")).toBe(5);
    expect(selectionSourceLine(content, undefined, "anything")).toBeNull();
  });

  it("places list items on their own lines", () => {
    const list = blocks[blockIndexAtLine(content, blocks, 8)];
    expect(selectionSourceLine(content, list, "item two")).toBe(9);
  });
});

describe("lineOfOffset", () => {
  it("counts lines up to an offset", () => {
    expect(lineOfOffset(content, 0)).toBe(1);
    expect(lineOfOffset(content, content.indexOf("First"))).toBe(3);
    expect(lineOfOffset(content, content.length)).toBe(
      content.split("\n").length,
    );
  });
});
