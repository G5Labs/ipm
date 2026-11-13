import { describe, expect, it } from "vitest";

import { stripHtmlComments } from "./githubMarkdown";

describe("stripHtmlComments", () => {
  it("removes G5's reference line and template comments", () => {
    expect(
      stripHtmlComments(
        "Looks right.\n\n<!-- g5-comment cbc40edc-c5ed-4f83-ba43-85069252d6b6 -->",
      ),
    ).toBe("Looks right.");
    expect(
      stripHtmlComments(
        "## Steps\n<!-- Describe how to\nreproduce -->\n1. Save twice",
      ),
    ).toBe("## Steps\n\n1. Save twice");
  });

  it("keeps comments inside fenced code blocks", () => {
    const markdown = [
      "Before <!-- hidden -->",
      "```html",
      "<!-- shown -->",
      "```",
      "~~~",
      "<!-- also shown -->",
      "~~~",
      "After",
    ].join("\n");
    expect(stripHtmlComments(markdown)).toBe(
      [
        "Before ",
        "```html",
        "<!-- shown -->",
        "```",
        "~~~",
        "<!-- also shown -->",
        "~~~",
        "After",
      ].join("\n"),
    );
  });

  it("drops an unterminated comment to the end, as GitHub does", () => {
    expect(stripHtmlComments("Visible\n<!-- never closed\nhidden")).toBe(
      "Visible",
    );
  });
});
