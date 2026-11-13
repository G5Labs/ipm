import { describe, expect, it } from "vitest";
import {
  TEXT_MEASURE_PX,
  textMeasureColumns,
  wrapsAtTextMeasure,
} from "./textMeasure";

// `g5.documents.text-measure`: every editor wraps at 8 inches except HTML
// and JSONL.
describe("text measure", () => {
  it("is eight inches at 96 CSS pixels per inch", () => {
    expect(TEXT_MEASURE_PX).toBe(768);
  });

  it("wraps Markdown and code, by language or extension, but not HTML or JSONL", () => {
    expect(wrapsAtTextMeasure("markdown", "docs/readme.md")).toBe(true);
    expect(wrapsAtTextMeasure("typescript", "src/app.ts")).toBe(true);
    expect(wrapsAtTextMeasure("", "notes/plain.txt")).toBe(true);
    expect(wrapsAtTextMeasure("html", "docs/page.html")).toBe(false);
    expect(wrapsAtTextMeasure("htm", "docs/page.htm")).toBe(false);
    expect(wrapsAtTextMeasure("jsonl", "logs/turn.jsonl")).toBe(false);
    // The extension decides when the language name differs from it.
    expect(wrapsAtTextMeasure("plaintext", "docs/page.HTML")).toBe(false);
    expect(wrapsAtTextMeasure("plaintext", "docs/page.XHTML")).toBe(false);
    expect(wrapsAtTextMeasure("plaintext", "docs/page.shtml")).toBe(false);
    expect(wrapsAtTextMeasure("plaintext", "logs/a.JSONL")).toBe(false);
    // A dotfile or an extension-less name wraps.
    expect(wrapsAtTextMeasure("", ".jsonl")).toBe(true);
  });

  it("derives the column count from the font's character width", () => {
    expect(textMeasureColumns(7.68)).toBe(100);
    expect(textMeasureColumns(8)).toBe(96);
    // Wide characters never push the wrap width past the measure.
    expect(textMeasureColumns(100)).toBe(7);
    expect(textMeasureColumns(1000)).toBe(1);
    expect(textMeasureColumns(0)).toBeNull();
    expect(textMeasureColumns(undefined)).toBeNull();
    expect(textMeasureColumns(Number.NaN)).toBeNull();
  });
});
