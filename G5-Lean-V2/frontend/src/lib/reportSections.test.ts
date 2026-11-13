import { describe, expect, it } from "vitest";
import { splitGoverningPrompt } from "./reportSections";

describe("splitGoverningPrompt", () => {
  it("splits only the Governing prompt section, keeping every byte", () => {
    const body =
      "## Governing prompt\n\nCheck **UX**.\n\n### Detail\nkept\n\n## Verdict\n\nRejected\n\n## Limitations\nNone.\n";
    const split = splitGoverningPrompt(body)!;
    expect(split.before).toBe("");
    expect(split.prompt).toBe("\nCheck **UX**.\n\n### Detail\nkept\n\n");
    expect(split.after).toBe(
      "## Verdict\n\nRejected\n\n## Limitations\nNone.\n",
    );
    expect(
      split.before + "## Governing prompt\n" + split.prompt + split.after,
    ).toBe(body);
  });

  it("ignores headings inside code fences and other section names", () => {
    expect(splitGoverningPrompt("# Review\n\n## Verdict\nOK\n")).toBeNull();
    expect(
      splitGoverningPrompt("```md\n## Governing prompt\n```\n## Verdict\n"),
    ).toBeNull();
    const fenced = splitGoverningPrompt(
      "## Governing prompt\n~~~\n## Verdict\n~~~\nend\n## Verdict\nOK",
    )!;
    expect(fenced.prompt).toBe("~~~\n## Verdict\n~~~\nend\n");
    expect(fenced.after).toBe("## Verdict\nOK");
  });

  it("runs to the end when no later section follows, and keeps text before it", () => {
    const split = splitGoverningPrompt(
      "Intro\r\n# Governing Prompt ##\r\nAll",
    )!;
    expect(split.before).toBe("Intro\r\n");
    expect(split.prompt).toBe("All");
    expect(split.after).toBe("");
    expect(splitGoverningPrompt("### Governing prompt\nnested")).toBeNull();
  });
});
