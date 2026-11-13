// =============================================================================
// G5 Client — utils.ts Comprehensive Tests
// =============================================================================
// Covers: parseFrontmatter, parseMarkdownToBlocks, highlightCode, highlightRust,
//         renderInlineMarkdown, resolveSpecRef, filterTree,
//         formatLargeNumber, formatCost, classNames
// =============================================================================

import React from "react";
import { describe, expect, it } from "vitest";
import semanticRecordExtentParity from "../../../backend/g5-semantic/tests/fixtures/semantic-record-extent-parity.txt?raw";
import type { Block } from "./utils";
import {
  ARTIFACT_CATEGORY_META,
  ARTIFACT_CATEGORY_ORDER,
  blockToRawMarkdown,
  classifyChangedPath,
  classNames,
  fileNameFromPath,
  formatCost,
  formatLargeNumber,
  highlightCode,
  highlightRust,
  labelWorkspaceFolderInPath,
  parseFrontmatter,
  parseMarkdownToBlocks,
  parseRecordMetadataSuffix,
  renderInlineMarkdown,
  sanitizeInlineSvg,
  serializeRecordMetadataTags,
  tableToRawMarkdown,
} from "./utils";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const blocksToMarkdown = (blocks: Block[]) =>
  blocks.map(blockToRawMarkdown).join("\n\n");

/** Recursively extract text from React elements */
function extractText(node: React.ReactNode): string {
  if (node == null || typeof node === "boolean") return "";
  if (typeof node === "string" || typeof node === "number") return String(node);
  if (Array.isArray(node)) return node.map(extractText).join("");
  if (React.isValidElement(node)) {
    const children = (node.props as Record<string, unknown>).children;
    return extractText(children as React.ReactNode);
  }
  return "";
}

function collectElementsByType(
  node: React.ReactNode,
  type: string,
): React.ReactElement[] {
  if (node == null || typeof node === "boolean") return [];
  if (typeof node === "string" || typeof node === "number") return [];
  if (Array.isArray(node))
    return node.flatMap((child) => collectElementsByType(child, type));
  if (!React.isValidElement(node)) return [];
  const self = node.type === type ? [node] : [];
  const children = (node.props as Record<string, unknown>).children;
  return [...self, ...collectElementsByType(children as React.ReactNode, type)];
}

// =============================================================================
// classNames
// =============================================================================

describe("classNames", () => {
  it("joins multiple class strings", () => {
    expect(classNames("a", "b", "c")).toBe("a b c");
  });

  it("filters out falsy values", () => {
    expect(classNames("a", false, null, undefined, "b")).toBe("a b");
  });

  it("returns empty string for all falsy args", () => {
    expect(classNames(false, null, undefined)).toBe("");
  });

  it("returns empty string for no args", () => {
    expect(classNames()).toBe("");
  });
});

// =============================================================================
// parseFrontmatter
// =============================================================================

describe("parseFrontmatter", () => {
  it("returns empty object for empty input", () => {
    expect(parseFrontmatter("")).toEqual({});
  });

  it("returns empty object when no frontmatter present", () => {
    expect(parseFrontmatter("# Hello World\nsome text")).toEqual({});
  });

  it("parses simple key-value pairs", () => {
    const input = `---
title: My Spec
version: 1.0
---`;
    const result = parseFrontmatter(input);
    expect(result.title).toBe("My Spec");
    expect(result.version).toBe("1.0");
  });

  it("handles null values (null keyword)", () => {
    const input = `---
description: null
---`;
    expect(parseFrontmatter(input).description).toBeNull();
  });

  it("handles tilde (~) as null", () => {
    const input = `---
notes: ~
---`;
    expect(parseFrontmatter(input).notes).toBeNull();
  });

  it("handles empty value as null", () => {
    const input = `---
empty:
---`;
    expect(parseFrontmatter(input).empty).toBeNull();
  });

  it("parses inline arrays", () => {
    const input = `---
tags: [alpha, beta, gamma]
---`;
    const result = parseFrontmatter(input);
    expect(result.tags).toEqual(["alpha", "beta", "gamma"]);
  });

  it("strips quotes from inline array items", () => {
    const input = `---
labels: ["foo", 'bar', baz]
---`;
    const result = parseFrontmatter(input);
    expect(result.labels).toEqual(["foo", "bar", "baz"]);
  });

  it("parses multiline arrays when preceded by scalar value", () => {
    // The parser accumulates - items into currentArray only when
    // the key had a non-empty scalar value (which initializes currentArray=[]).
    // The initial scalar value gets overwritten by the array.
    const input = `---
child_specs: initial
  - spec-a
  - spec-b
---`;
    const result = parseFrontmatter(input);
    // The scalar 'initial' is overwritten — only - items end up in the array
    expect(result.child_specs).toEqual(["spec-a", "spec-b"]);
  });

  it("parses key with empty value followed by - items as a multiline array", () => {
    const input = `---
child_specs:
  - spec-a
  - spec-b
---`;
    const result = parseFrontmatter(input);
    expect(result.child_specs).toEqual(["spec-a", "spec-b"]);
  });

  it("parses current SemLift graph fields from block-style YAML arrays", () => {
    const input = `---
component_id: calculator
parent_spec_id: root
child_spec_ids:
  - calculator/core
  - calculator/ui
dependency_spec_ids:
  - calculator/math
dependencies:
  - calculator/shared
source_files:
  - src/main.ts
  - src/calculator.ts
---`;
    const result = parseFrontmatter(input);
    expect(result.component_id).toBe("calculator");
    expect(result.parent_spec_id).toBe("root");
    expect(result.child_spec_ids).toEqual(["calculator/core", "calculator/ui"]);
    expect(result.dependency_spec_ids).toEqual(["calculator/math"]);
    expect(result.dependencies).toEqual(["calculator/shared"]);
    expect(result.source_files).toEqual(["src/main.ts", "src/calculator.ts"]);
  });

  it("strips quotes from scalar values", () => {
    const input = `---
name: "My Component"
---`;
    expect(parseFrontmatter(input).name).toBe("My Component");
  });

  it("strips single quotes from scalar values", () => {
    const input = `---
name: 'My Component'
---`;
    expect(parseFrontmatter(input).name).toBe("My Component");
  });

  it("handles dotted key names", () => {
    const input = `---
spec.version: 2.1
---`;
    expect(parseFrontmatter(input)["spec.version"]).toBe("2.1");
  });

  it("handles hyphenated key names", () => {
    const input = `---
component-id: abc-123
---`;
    expect(parseFrontmatter(input)["component-id"]).toBe("abc-123");
  });

  it("parses a full realistic frontmatter block", () => {
    const input = `---
component_id: ont-001
title: Ontology Root
parent_spec: ~
child_specs: [ont-002, ont-003]
src_files: [main.rs, lib.rs]
status: active
---

# Body content`;
    const result = parseFrontmatter(input);
    expect(result.component_id).toBe("ont-001");
    expect(result.title).toBe("Ontology Root");
    expect(result.parent_spec).toBeNull();
    expect(result.child_specs).toEqual(["ont-002", "ont-003"]);
    expect(result.src_files).toEqual(["main.rs", "lib.rs"]);
    expect(result.status).toBe("active");
  });

  it("ignores content after the closing ---", () => {
    const input = `---
key: value
---
This is body text with key: other_value`;
    const result = parseFrontmatter(input);
    expect(Object.keys(result)).toEqual(["key"]);
    expect(result.key).toBe("value");
  });
});

// =============================================================================
// parseMarkdownToBlocks
// =============================================================================

describe("parseMarkdownToBlocks", () => {
  it("returns empty array for empty input", () => {
    expect(parseMarkdownToBlocks("")).toEqual([]);
  });

  it("returns empty array for null-ish input", () => {
    expect(parseMarkdownToBlocks(null as unknown as string)).toEqual([]);
    expect(parseMarkdownToBlocks(undefined as unknown as string)).toEqual([]);
  });

  // --- Frontmatter ---

  it("parses frontmatter block at document start", () => {
    const md = `---
title: Hello
---

# Heading`;
    const blocks = parseMarkdownToBlocks(md);
    expect(blocks[0].type).toBe("frontmatter");
    if (blocks[0].type === "frontmatter") {
      expect(blocks[0].fields.title).toBe("Hello");
    }
  });

  it("frontmatter only appears at document start", () => {
    const md = `# Heading

---
title: Not Frontmatter
---`;
    const blocks = parseMarkdownToBlocks(md);
    // Should not have a frontmatter block; --- lines after content are HRs
    expect(blocks.some((b) => b.type === "frontmatter")).toBe(false);
  });

  // --- Headings ---

  it("parses h1–h6 headings", () => {
    const md = `# H1
## H2
### H3
#### H4
##### H5
###### H6`;
    const blocks = parseMarkdownToBlocks(md);
    expect(blocks).toHaveLength(6);
    for (let i = 0; i < 6; i++) {
      expect(blocks[i].type).toBe("heading");
      if (blocks[i].type === "heading") {
        expect((blocks[i] as Extract<Block, { type: "heading" }>).level).toBe(
          i + 1,
        );
        expect((blocks[i] as Extract<Block, { type: "heading" }>).content).toBe(
          `H${i + 1}`,
        );
      }
    }
  });

  it("ignores trailing # in heading content", () => {
    // ATX headings rarely have trailing #, but the regex captures everything after `# `
    const blocks = parseMarkdownToBlocks("## Hello World");
    expect(blocks[0].type).toBe("heading");
    if (blocks[0].type === "heading") {
      expect(blocks[0].content).toBe("Hello World");
    }
  });

  // --- Images ---

  it("parses standalone image blocks with source ranges and raw round-trip", () => {
    const md = '![Guide screenshot](imgs/screenshot.png "Guide")';
    const blocks = parseMarkdownToBlocks(md);

    expect(blocks).toHaveLength(1);
    expect(blocks[0].type).toBe("image");
    if (blocks[0].type === "image") {
      expect(blocks[0].alt).toBe("Guide screenshot");
      expect(blocks[0].src).toBe("imgs/screenshot.png");
      expect(blocks[0].title).toBe("Guide");
      expect(blocks[0].sourceRange).toEqual({ start: 0, length: md.length });
      expect(blockToRawMarkdown(blocks[0])).toBe(md);
    }
  });

  it("serializes image metadata with markdown escaping and parses it back", () => {
    const block: Block = {
      type: "image",
      alt: "Guide [v2]",
      src: "imgs/a b(1).png",
      title: 'The "quoted" shot',
    };

    const raw = blockToRawMarkdown(block);

    expect(raw).toBe(
      '![Guide \\[v2\\]](imgs/a\\ b\\(1\\).png "The \\"quoted\\" shot")',
    );
    const blocks = parseMarkdownToBlocks(raw);
    expect(blocks).toHaveLength(1);
    expect(blocks[0]).toMatchObject(block);
  });

  // --- Code blocks ---

  it("parses fenced code blocks with language", () => {
    const md = "```rust\nfn main() {}\n```";
    const blocks = parseMarkdownToBlocks(md);
    expect(blocks).toHaveLength(1);
    expect(blocks[0].type).toBe("code");
    if (blocks[0].type === "code") {
      expect(blocks[0].language).toBe("rust");
      expect(blocks[0].content).toBe("fn main() {}");
    }
  });

  it("parses fenced code blocks without language", () => {
    const md = "```\nplain code\n```";
    const blocks = parseMarkdownToBlocks(md);
    expect(blocks[0].type).toBe("code");
    if (blocks[0].type === "code") {
      expect(blocks[0].language).toBe("");
      expect(blocks[0].content).toBe("plain code");
    }
  });

  it("preserves multi-line code content", () => {
    const md = '```python\ndef hello():\n    print("hi")\n```';
    const blocks = parseMarkdownToBlocks(md);
    if (blocks[0].type === "code") {
      expect(blocks[0].content).toBe('def hello():\n    print("hi")');
    }
  });

  it("handles mermaid code blocks", () => {
    const md = "```mermaid\ngraph LR\n  A --> B\n```";
    const blocks = parseMarkdownToBlocks(md);
    if (blocks[0].type === "code") {
      expect(blocks[0].language).toBe("mermaid");
    }
  });

  // --- Callouts ---

  it("parses callout blocks (:::note)", () => {
    const md = ":::note\nThis is important.\n:::";
    const blocks = parseMarkdownToBlocks(md);
    expect(blocks).toHaveLength(1);
    expect(blocks[0].type).toBe("callout");
    if (blocks[0].type === "callout") {
      expect(blocks[0].kind).toBe("note");
      expect(blocks[0].content).toBe("This is important.");
    }
  });

  it("parses all callout kinds", () => {
    for (const kind of ["note", "warning", "tip", "danger"] as const) {
      const md = `:::${kind}\nContent for ${kind}\n:::`;
      const blocks = parseMarkdownToBlocks(md);
      if (blocks[0].type === "callout") {
        expect(blocks[0].kind).toBe(kind);
      }
    }
  });

  // --- Tables ---

  it("parses GFM pipe tables", () => {
    const md = `| Name | Type |
| --- | --- |
| foo | string |
| bar | number |`;
    const blocks = parseMarkdownToBlocks(md);
    expect(blocks).toHaveLength(1);
    expect(blocks[0].type).toBe("table");
    if (blocks[0].type === "table") {
      expect(blocks[0].headers).toEqual(["Name", "Type"]);
      expect(blocks[0].rows).toEqual([
        ["foo", "string"],
        ["bar", "number"],
      ]);
    }
  });

  it("parses tables without leading/trailing pipes", () => {
    const md = `Name | Type
--- | ---
foo | string`;
    const blocks = parseMarkdownToBlocks(md);
    expect(blocks[0].type).toBe("table");
    if (blocks[0].type === "table") {
      expect(blocks[0].headers).toEqual(["Name", "Type"]);
    }
  });

  // --- GFM table cell pipe escaping (issue #851) ---

  it("treats an escaped pipe (\\|) as a literal pipe within one cell", () => {
    const md = `| Type | Value |
| --- | --- |
| union | a \\| b \\| c |`;
    const blocks = parseMarkdownToBlocks(md);
    expect(blocks[0].type).toBe("table");
    if (blocks[0].type === "table") {
      expect(blocks[0].headers).toEqual(["Type", "Value"]);
      expect(blocks[0].rows).toEqual([["union", "a | b | c"]]);
    }
  });

  it("keeps an escaped pipe literal inside an inline code span", () => {
    const md = `| Name | Type |
| --- | --- |
| \`HumanOrigin\` | \`'direct_edit' \\| 'chat_instruction'\` |`;
    const blocks = parseMarkdownToBlocks(md);
    expect(blocks[0].type).toBe("table");
    if (blocks[0].type === "table") {
      expect(blocks[0].rows).toEqual([
        ["`HumanOrigin`", "`'direct_edit' | 'chat_instruction'`"],
      ]);
    }
  });

  it("keeps escaped-pipe cells at the correct column count (no drift)", () => {
    const md = `| a | b | c |
| --- | --- | --- |
| \`start_node\` | \`startNode\` | string \\| null |`;
    const blocks = parseMarkdownToBlocks(md);
    if (blocks[0].type === "table") {
      expect(blocks[0].headers).toHaveLength(3);
      expect(blocks[0].rows[0]).toEqual([
        "`start_node`",
        "`startNode`",
        "string | null",
      ]);
    }
  });

  it("still treats a RAW (unescaped) pipe as a delimiter, per GFM/GitHub", () => {
    // A raw pipe inside a code span is malformed GFM and DOES split — we match
    // GitHub rather than silently rendering non-conformant markdown.
    const md = `| a | b |
| --- | --- |
| \`x|y\` | z |`;
    const blocks = parseMarkdownToBlocks(md);
    if (blocks[0].type === "table") {
      expect(blocks[0].rows[0]).toEqual(["`x", "y`", "z"]);
    }
  });

  it("does not alter parsing of tables without pipes in cell content", () => {
    const md = `| Name | Type |
| --- | --- |
| foo | string |
| bar | number |`;
    const blocks = parseMarkdownToBlocks(md);
    if (blocks[0].type === "table") {
      expect(blocks[0].rows).toEqual([
        ["foo", "string"],
        ["bar", "number"],
      ]);
    }
  });

  it("re-escapes literal pipes when serializing a table (blockToRawMarkdown)", () => {
    const md = `| Type | Value |
| --- | --- |
| union | a \\| b |`;
    const [block] = parseMarkdownToBlocks(md);
    const raw = blockToRawMarkdown(block);
    expect(raw).toContain("a \\| b");
    // and it must not emit a bare, unescaped pipe inside the cell text
    expect(raw).not.toContain("| a | b |");
  });

  it("round-trips an escaped-pipe table without column drift", () => {
    const md = `| Name | Type |
| --- | --- |
| \`HumanOrigin\` | \`'direct_edit' \\| 'chat_instruction' \\| 'x'\` |
| \`start_node\` | string \\| null |`;
    const once = parseMarkdownToBlocks(md);
    const twice = parseMarkdownToBlocks(blocksToMarkdown(once));
    expect(once[0].type).toBe("table");
    expect(twice[0].type).toBe("table");
    if (once[0].type === "table" && twice[0].type === "table") {
      // content is preserved and column counts never drift
      expect(twice[0].headers).toEqual(once[0].headers);
      expect(twice[0].rows).toEqual(once[0].rows);
      expect(twice[0].rows.every((r) => r.length === 2)).toBe(true);
    }
  });

  // --- Lists ---

  it("parses bullet lists (- syntax)", () => {
    const md = `- Alpha
- Beta
- Gamma`;
    const blocks = parseMarkdownToBlocks(md);
    expect(blocks).toHaveLength(1);
    expect(blocks[0].type).toBe("bullet-list");
    if (blocks[0].type === "bullet-list") {
      expect(blocks[0].items).toEqual(["Alpha", "Beta", "Gamma"]);
    }
  });

  it("parses bullet lists (* syntax)", () => {
    const md = `* Item 1
* Item 2`;
    const blocks = parseMarkdownToBlocks(md);
    expect(blocks[0].type).toBe("bullet-list");
    if (blocks[0].type === "bullet-list") {
      expect(blocks[0].items).toEqual(["Item 1", "Item 2"]);
    }
  });

  it("parses numbered lists", () => {
    const md = `1. First
2. Second
3. Third`;
    const blocks = parseMarkdownToBlocks(md);
    expect(blocks).toHaveLength(1);
    expect(blocks[0].type).toBe("numbered-list");
    if (blocks[0].type === "numbered-list") {
      expect(blocks[0].items).toEqual(["First", "Second", "Third"]);
      expect(blocks[0].start).toBe(1);
      expect(blockToRawMarkdown(blocks[0])).toBe(md);
    }
  });

  it("preserves non-1 numbered list start markers", () => {
    const md = `3. Third
4. Fourth`;
    const blocks = parseMarkdownToBlocks(md);

    expect(blocks).toHaveLength(1);
    expect(blocks[0].type).toBe("numbered-list");
    if (blocks[0].type === "numbered-list") {
      expect(blocks[0].start).toBe(3);
      expect(blocks[0].items).toEqual(["Third", "Fourth"]);
      expect(blockToRawMarkdown(blocks[0])).toBe(md);
    }
  });

  it("preserves interrupted guide ordered-list starts across intervening blocks", () => {
    const md = `1. Fork workspace

![Fork dialog](imgs/fork.png)

2. Describe intent`;
    const blocks = parseMarkdownToBlocks(md);

    expect(blocks.map((block) => block.type)).toEqual([
      "numbered-list",
      "image",
      "numbered-list",
    ]);
    expect(blocks[0]).toMatchObject({
      type: "numbered-list",
      start: 1,
      items: ["Fork workspace"],
    });
    expect(blocks[2]).toMatchObject({
      type: "numbered-list",
      start: 2,
      items: ["Describe intent"],
    });
    expect(blockToRawMarkdown(blocks[2])).toBe("2. Describe intent");
  });

  it("serializes isolated numbered list blocks from their parsed start marker", () => {
    const blocks = parseMarkdownToBlocks("7. Publish");

    expect(blocks).toHaveLength(1);
    expect(blocks[0].type).toBe("numbered-list");
    if (blocks[0].type === "numbered-list") {
      expect(blocks[0].start).toBe(7);
      expect(blockToRawMarkdown(blocks[0])).toBe("7. Publish");
    }
  });

  // --- List continuation + loose grouping (issue #1189, [D58]) ---

  it("absorbs a wrapped bullet continuation line into the same item", () => {
    const md = `- A bullet that wraps at eighty characters onto
  a second physical line here.
- Second item`;
    const blocks = parseMarkdownToBlocks(md);
    expect(blocks).toHaveLength(1);
    expect(blocks[0].type).toBe("bullet-list");
    if (blocks[0].type === "bullet-list") {
      expect(blocks[0].items).toEqual([
        "A bullet that wraps at eighty characters onto\na second physical line here.",
        "Second item",
      ]);
    }
  });

  it("round-trips a wrapped bullet continuation with re-indented lines", () => {
    const md = `- first line
  continued line
- second`;
    const blocks = parseMarkdownToBlocks(md);
    expect(blocks).toHaveLength(1);
    expect(blockToRawMarkdown(blocks[0])).toBe(md);
    // Re-parsing the serialized form yields identical blocks (lossless).
    expect(parseMarkdownToBlocks(blocksToMarkdown(blocks))).toEqual(blocks);
  });

  it("groups a loose (blank-separated) bullet list into one block", () => {
    const md = `- one

- two

- three`;
    const blocks = parseMarkdownToBlocks(md);
    expect(blocks).toHaveLength(1);
    expect(blocks[0].type).toBe("bullet-list");
    if (blocks[0].type === "bullet-list") {
      expect(blocks[0].items).toEqual(["one", "two", "three"]);
      expect(blocks[0].looseSeparators).toEqual([true, true]);
      expect(blockToRawMarkdown(blocks[0])).toBe(md);
    }
  });

  it("groups a loose numbered list and preserves its start marker", () => {
    const md = `1. first

2. second`;
    const blocks = parseMarkdownToBlocks(md);
    expect(blocks).toHaveLength(1);
    expect(blocks[0].type).toBe("numbered-list");
    if (blocks[0].type === "numbered-list") {
      expect(blocks[0].items).toEqual(["first", "second"]);
      expect(blocks[0].start).toBe(1);
      expect(blocks[0].looseSeparators).toEqual([true]);
      expect(blockToRawMarkdown(blocks[0])).toBe(md);
    }
  });

  it("ends the list at an intervening non-list block", () => {
    const md = `- item one
## Heading
paragraph`;
    const blocks = parseMarkdownToBlocks(md);
    expect(blocks.map((b) => b.type)).toEqual([
      "bullet-list",
      "heading",
      "paragraph",
    ]);
    if (blocks[0].type === "bullet-list") {
      expect(blocks[0].items).toEqual(["item one"]);
    }
  });

  it("keeps tight single-line lists unchanged (no loose flag)", () => {
    const blocks = parseMarkdownToBlocks(`- a\n- b`);
    expect(blocks).toHaveLength(1);
    if (blocks[0].type === "bullet-list") {
      expect(blocks[0].items).toEqual(["a", "b"]);
      expect(blocks[0].looseSeparators).toBeUndefined();
    }
  });

  it("preserves per-boundary spacing in a mixed tight/loose list", () => {
    const md = `- a\n- b\n\n- c`;
    const blocks = parseMarkdownToBlocks(md);
    expect(blocks).toHaveLength(1);
    expect(blocks[0].type).toBe("bullet-list");
    if (blocks[0].type === "bullet-list") {
      expect(blocks[0].items).toEqual(["a", "b", "c"]);
      // a→b tight, b→c loose; must NOT collapse to all-loose on round-trip.
      expect(blocks[0].looseSeparators).toEqual([false, true]);
      expect(blockToRawMarkdown(blocks[0])).toBe(md);
    }
  });

  it("ends list continuation at a GFM table (not absorbed into the item)", () => {
    const md = `- item\n| A | B |\n| --- | --- |\n| x | y |`;
    const blocks = parseMarkdownToBlocks(md);
    expect(blocks.map((b) => b.type)).toEqual(["bullet-list", "table"]);
    if (blocks[0].type === "bullet-list") {
      expect(blocks[0].items).toEqual(["item"]);
    }
  });

  it("treats removed wrapper syntax as ordinary markdown text", () => {
    const blocks = parseMarkdownToBlocks("1. one\n2. two\n</human>");
    expect(blocks[0].type).toBe("numbered-list");
    if (blocks[0].type === "numbered-list") {
      expect(blocks[0].items).toEqual(["one", "two\n</human>"]);
    }
  });

  it("does not group lists separated by two or more blank lines", () => {
    const blocks = parseMarkdownToBlocks(`- a\n\n\n- b`);
    expect(blocks.filter((b) => b.type === "bullet-list")).toHaveLength(2);
    expect(
      blocks.every(
        (b) => b.type !== "bullet-list" || b.looseSeparators === undefined,
      ),
    ).toBe(true);
  });

  // --- Blockquotes ---

  it("parses blockquotes", () => {
    const md = `> This is a quote
> with two lines`;
    const blocks = parseMarkdownToBlocks(md);
    expect(blocks).toHaveLength(1);
    expect(blocks[0].type).toBe("blockquote");
    if (blocks[0].type === "blockquote") {
      expect(blocks[0].content).toBe("This is a quote\nwith two lines");
    }
  });

  it("handles empty blockquote lines (bare >)", () => {
    const md = `> Line one
>
> Line three`;
    const blocks = parseMarkdownToBlocks(md);
    expect(blocks[0].type).toBe("blockquote");
  });

  // --- Horizontal rules ---

  it("parses --- as horizontal rule (not at document start)", () => {
    const md = `# Title

---

Text below`;
    const blocks = parseMarkdownToBlocks(md);
    const hrs = blocks.filter((b) => b.type === "hr");
    expect(hrs.length).toBeGreaterThanOrEqual(1);
  });

  it("parses *** as horizontal rule", () => {
    const md = `Paragraph

***`;
    const blocks = parseMarkdownToBlocks(md);
    expect(blocks.some((b) => b.type === "hr")).toBe(true);
  });

  it("parses ___ as horizontal rule", () => {
    const md = `Paragraph

___`;
    const blocks = parseMarkdownToBlocks(md);
    expect(blocks.some((b) => b.type === "hr")).toBe(true);
  });

  // --- Paragraphs ---

  it("parses plain text as paragraphs", () => {
    const md = `This is a paragraph.`;
    const blocks = parseMarkdownToBlocks(md);
    expect(blocks).toHaveLength(1);
    expect(blocks[0].type).toBe("paragraph");
    if (blocks[0].type === "paragraph") {
      expect(blocks[0].content).toBe("This is a paragraph.");
    }
  });

  it("joins adjacent lines into a single paragraph (preserving newlines)", () => {
    const md = `Line one
Line two
Line three`;
    const blocks = parseMarkdownToBlocks(md);
    expect(blocks).toHaveLength(1);
    if (blocks[0].type === "paragraph") {
      expect(blocks[0].content).toBe("Line one\nLine two\nLine three");
    }
  });

  it("splits adjacent ontology I/D labels into separate paragraph blocks", () => {
    const md = `[I44. Existing intent]
[D44a. New decision]

Body stays below.`;
    const blocks = parseMarkdownToBlocks(md);

    expect(blocks).toHaveLength(3);
    expect(blocks[0].type).toBe("paragraph");
    expect(blocks[1].type).toBe("paragraph");
    expect(blocks[2].type).toBe("paragraph");

    if (blocks[0].type === "paragraph" && blocks[1].type === "paragraph") {
      expect(blocks[0].content).toBe("[I44. Existing intent]");
      expect(blocks[0].sourceRange).toEqual({
        start: 0,
        length: "[I44. Existing intent]".length,
      });
      expect(blocks[1].content).toBe("[D44a. New decision]");
      expect(blocks[1].sourceRange).toEqual({
        start: md.indexOf("[D44a. New decision]"),
        length: "[D44a. New decision]".length,
      });
    }
  });

  // --- Structured Rich-edit source fidelity ---

  it("recognizes and round-trips a tilde-fenced code block", () => {
    const md = "~~~~typescript\nconst answer = 42;\n~~~~";
    const blocks = parseMarkdownToBlocks(md);

    expect(blocks).toHaveLength(1);
    expect(blocks[0]).toMatchObject({
      type: "code",
      language: "typescript",
      fence: "~~~~",
      closed: true,
    });
    expect(blocksToMarkdown(blocks)).toBe(md);
  });

  it("preserves code-fence info spacing and closing suffix bytes", () => {
    const md = "  ````  typescript  \nconst answer = 42;\n `````\t";
    const blocks = parseMarkdownToBlocks(md);

    expect(blocks).toHaveLength(1);
    expect(blocks[0]).toMatchObject({
      type: "code",
      language: "typescript",
      fence: "````",
      indent: "  ",
      closingFence: "`````",
      closingIndent: " ",
      closed: true,
    });
    expect(blocksToMarkdown(blocks)).toBe(md);
  });

  it("does not invent a closing fence for malformed code", () => {
    const md = "```ts\nconst unfinished = true;";
    const blocks = parseMarkdownToBlocks(md);

    expect(blocks[0]).toMatchObject({ type: "code", closed: false });
    expect(blocksToMarkdown(blocks)).toBe(md);
  });

  it("preserves table alignment and escaped pipes on round-trip", () => {
    const md = `| Name | Contract |
| :--- | ---: |
| worker | left\\|right |`;
    const blocks = parseMarkdownToBlocks(md);

    expect(blocks[0]).toMatchObject({
      type: "table",
      alignments: ["left", "right"],
    });
    expect(blocksToMarkdown(blocks)).toBe(md);
  });

  it("uses backslash parity to parse literal table pipes", () => {
    const oneBackslash = String.raw`A | B
--- | ---
one\|two | tail`;
    const twoBackslashes = String.raw`A | B | C
--- | --- | ---
one\\|two | tail`;
    const threeBackslashes = String.raw`A | B
--- | ---
one\\\|two | tail`;

    const one = parseMarkdownToBlocks(oneBackslash)[0];
    const two = parseMarkdownToBlocks(twoBackslashes)[0];
    const three = parseMarkdownToBlocks(threeBackslashes)[0];
    expect(one).toMatchObject({ type: "table", rows: [["one|two", "tail"]] });
    expect(two).toMatchObject({
      type: "table",
      rows: [[String.raw`one\\`, "two", "tail"]],
    });
    expect(three).toMatchObject({
      type: "table",
      rows: [[String.raw`one\\|two`, "tail"]],
    });
  });

  it("keeps ragged table values when structural serialization rectangularizes", () => {
    expect(
      tableToRawMarkdown(["A"], [["one", "two"]], ["right"], true, true),
    ).toBe("| A |  |\n| ---: | --- |\n| one | two |");
  });

  it("preserves each authored bullet marker on round-trip", () => {
    const md = "* first\n- second\n* third";
    const blocks = parseMarkdownToBlocks(md);

    expect(blocks[0]).toMatchObject({
      type: "bullet-list",
      markers: ["*", "-", "*"],
    });
    expect(blocksToMarkdown(blocks)).toBe(md);
  });

  it("marks indented or nested bullet syntax as source-fallback only", () => {
    const blocks = parseMarkdownToBlocks("- parent\n  - nested");

    expect(blocks[0]).toMatchObject({
      type: "bullet-list",
      nativeEditable: false,
    });
  });

  it("preserves an unclosed callout for bounded-source recovery", () => {
    const md = ":::warning\nStill being authored";
    const blocks = parseMarkdownToBlocks(md);

    expect(blocks[0]).toMatchObject({ type: "callout", closed: false });
    expect(blocksToMarkdown(blocks)).toBe(md);
  });

  it("preserves the authored divider marker on round-trip", () => {
    const blocks = parseMarkdownToBlocks("paragraph\n\n***");

    expect(blocks[1]).toMatchObject({ type: "hr", marker: "***" });
    expect(blockToRawMarkdown(blocks[1])).toBe("***");
  });

  // --- Mixed document ---

  it("parses a complex mixed document", () => {
    const md = `---
title: Demo
---

# Introduction

This is the intro paragraph.

## Features

- Fast
- Reliable
- Scalable

\`\`\`rust
fn main() {}
\`\`\`

:::tip
Use this wisely.
:::

| Col1 | Col2 |
| --- | --- |
| a | b |

---

> A wise quote

1. Step one
2. Step two`;

    const blocks = parseMarkdownToBlocks(md);

    const types = blocks.map((b) => b.type);
    expect(types).toContain("frontmatter");
    expect(types).toContain("heading");
    expect(types).toContain("paragraph");
    expect(types).toContain("bullet-list");
    expect(types).toContain("code");
    expect(types).toContain("callout");
    expect(types).toContain("table");
    expect(types).toContain("hr");
    expect(types).toContain("blockquote");
    expect(types).toContain("numbered-list");
  });
});

describe("parseMarkdownToBlocks semantic record spans", () => {
  function recordSource(
    markdown: string,
    blocks: ReturnType<typeof parseMarkdownToBlocks>,
    spanIndex: number,
  ): string {
    const span = blocks.recordSpans[spanIndex];
    const firstRange = blocks[span.spanStart]?.sourceRange;
    const lastRange = blocks[span.spanEnd - 1]?.sourceRange;
    if (!firstRange || !lastRange) {
      throw new Error(
        "record fixture is missing parser-recorded source ranges",
      );
    }
    return markdown.slice(firstRange.start, lastRange.start + lastRange.length);
  }

  it("keeps record metadata additive to ordinary Block[] behavior", () => {
    const blocks = parseMarkdownToBlocks("[I1. Record]");

    expect(Array.isArray(blocks)).toBe(true);
    expect(blocks).toHaveLength(1);
    expect(Object.keys(blocks)).toEqual(["0"]);
    expect(blocks.recordSpans).toEqual([
      {
        id: "I1",
        kind: "I",
        title: "Record",
        spanStart: 0,
        spanEnd: 1,
      },
    ]);
    expect([...blocks]).toEqual([blocks[0]]);
  });

  it("projects inline record tags without including the suffix in the title", () => {
    const blocks = parseMarkdownToBlocks(
      "[D8. Inline metadata] (Tags: human, uncertain)\n\nBody.",
    );

    expect(blocks.recordSpans).toEqual([
      {
        id: "D8",
        kind: "D",
        title: "Inline metadata",
        spanStart: 0,
        spanEnd: 2,
        metadataSuffix: "(Tags: human, uncertain)",
        metadataValid: true,
        metadataTags: [
          {
            kind: "human",
            syntax: "inline",
          },
          {
            kind: "uncertainty",
            syntax: "inline",
          },
        ],
      },
    ]);
    expect(blocks[0]).toMatchObject({ type: "paragraph" });
    if (blocks[0].type !== "paragraph")
      throw new Error("expected label paragraph");
  });

  it("keeps a nested bracketed reference inside the label title (D69 regression)", () => {
    // A legal title may cite another record inline: the closing bracket is
    // the LAST `]` on the line. A lazy match cut the title at `[D59`'s
    // bracket, invalidated the suffix, and dropped the record frame —
    // rendering the whole block as unblocked content in the editor.
    const blocks = parseMarkdownToBlocks(
      "[D69. Markdown blocks render as a card; the HTML path ([D59]) covers only sandboxed docs]\n\nBody.",
    );
    expect(blocks.recordSpans).toEqual([
      {
        id: "D69",
        kind: "D",
        title:
          "Markdown blocks render as a card; the HTML path ([D59]) covers only sandboxed docs",
        spanStart: 0,
        spanEnd: 2,
      },
    ]);
    // The greedy close still leaves a canonical Tags suffix intact.
    const tagged = parseMarkdownToBlocks(
      "[D70. Nested ([D59]) title] (Tags: human)\n\nBody.",
    );
    expect(tagged.recordSpans[0]).toMatchObject({
      id: "D70",
      title: "Nested ([D59]) title",
      metadataSuffix: "(Tags: human)",
      metadataValid: true,
    });
  });

  it("folds a misplaced in-bracket Tags suffix into metadata (tolerant read)", () => {
    // Agents sometimes write `[Q1. Title (Tags: critical)]` despite the
    // canonical `[Q1. Title] (Tags: critical)` — display the title clean and
    // surface the tag as metadata instead of literal title text.
    const blocks = parseMarkdownToBlocks(
      "[Q1. Adopt or reject auto-proceed (Tags: critical)]\n\nBody.",
    );
    expect(blocks.recordSpans[0]).toMatchObject({
      id: "Q1",
      title: "Adopt or reject auto-proceed",
      metadataSuffix: "(Tags: critical)",
    });
  });

  it("never promotes an in-bracket human/authority group into metadata", () => {
    // Review P1: the Rust registrar treats an in-bracket `(Tags: human)` as
    // literal title text and stores is_human:false — the client must render
    // the same literal text, never a client-side authority tag.
    const blocks = parseMarkdownToBlocks(
      "[D1. AI-written decision (Tags: human)]\n\nBody.",
    );
    expect(blocks.recordSpans[0]).toMatchObject({
      id: "D1",
      title: "AI-written decision (Tags: human)",
    });
    expect(blocks.recordSpans[0].metadataTags).toBeUndefined();
    // Same for answered/uncertain: only pure severity tokens fold.
    const answered = parseMarkdownToBlocks(
      "[Q2. Question (Tags: answered)]\n\nBody.",
    );
    expect(answered.recordSpans[0].title).toBe("Question (Tags: answered)");
  });

  it("round-trips the three canonical two-tag suffixes", () => {
    for (const [suffix, tags] of [
      ["(Tags: human)", [{ kind: "human", syntax: "inline" }]],
      ["(Tags: uncertain)", [{ kind: "uncertainty", syntax: "inline" }]],
      [
        "(Tags: human, uncertain)",
        [
          { kind: "human", syntax: "inline" },
          { kind: "uncertainty", syntax: "inline" },
        ],
      ],
    ] as const) {
      const parsed = parseRecordMetadataSuffix(suffix);
      expect(parsed).toEqual({ valid: true, canonical: true, tags });
      expect(serializeRecordMetadataTags(parsed.tags, "D")).toBe(suffix);
    }
  });

  it("refuses to serialize question-state tags onto a non-Q record", () => {
    // Serialization must not be able to write what parsing rejects: emitting
    // `(Tags: answered)` onto a D record would produce a suffix that fails to
    // parse, and an unparseable suffix stops the record being projected as a
    // semantic record at all.
    const answered = [{ kind: "answered", syntax: "inline" }] as const;
    const severity = [
      { kind: "severity", severity: "high", syntax: "inline" },
    ] as const;

    for (const kind of ["I", "D"] as const) {
      expect(serializeRecordMetadataTags(answered, kind)).toBeNull();
      expect(serializeRecordMetadataTags(severity, kind)).toBeNull();
    }
    // On a Q record the same tags serialize canonically.
    expect(serializeRecordMetadataTags(answered, "Q")).toBe("(Tags: answered)");
    expect(serializeRecordMetadataTags(severity, "Q")).toBe("(Tags: high)");
    // Authored tags stay valid on every kind.
    expect(
      serializeRecordMetadataTags([{ kind: "human", syntax: "inline" }], "D"),
    ).toBe("(Tags: human)");
  });

  it("fails closed for duplicate or malformed metadata without changing its bytes", () => {
    const duplicate = "(Tags: human, human)";
    const malformed = '(Tags: uncertain source="agent")';

    expect(parseRecordMetadataSuffix(duplicate)).toEqual({
      tags: [],
      valid: false,
      canonical: false,
    });
    expect(parseRecordMetadataSuffix(malformed)).toEqual({
      tags: [],
      valid: false,
      canonical: false,
    });
    for (const suffix of [duplicate, malformed, "(Tags: uncertain, human)"]) {
      const markdown = `[I7. Keep bytes] ${suffix}`;
      const blocks = parseMarkdownToBlocks(markdown);
      expect(blocks.recordSpans).toEqual([]);
      expect(blocks).toHaveLength(1);
      expect(blocks[0]).toMatchObject({
        type: "paragraph",
        content: markdown,
      });
      expect(blocks[0].sourceRange).toEqual({
        start: 0,
        length: markdown.length,
      });
    }
  });

  it("stops a valid record before a malformed label without absorbing flat fallback bytes", () => {
    const markdown = `[D1. Valid design]

Valid body.

[D2. Malformed metadata] (Tags: uncertain source="agent")

Malformed body stays flat.

[D3. Later valid design]

Later body.`;
    const blocks = parseMarkdownToBlocks(markdown);

    expect(blocks.recordSpans.map((span) => span.id)).toEqual(["D1", "D3"]);
    expect(recordSource(markdown, blocks, 0)).toBe(`[D1. Valid design]

Valid body.`);
    expect(recordSource(markdown, blocks, 1)).toBe(`[D3. Later valid design]

Later body.`);
    expect(
      blocks.some(
        (block) =>
          block.type === "paragraph" &&
          block.content.includes("Malformed metadata"),
      ),
    ).toBe(true);
  });

  it("leaves case-insensitive duplicate identities flat and still frames later unique records", () => {
    const markdown = `[D1. First duplicate]

First body.

[d1. Second duplicate]

Second body.

[D2. Unique design]

Unique body.`;
    const blocks = parseMarkdownToBlocks(markdown);

    expect(blocks.recordSpans.map((span) => span.id)).toEqual(["D2"]);
    expect(recordSource(markdown, blocks, 0)).toBe(`[D2. Unique design]

Unique body.`);
  });

  it("matches the registration parser by treating heading-prefixed labels as headings", () => {
    const markdown = `[I1. Real intent]

Owned body.

### [D2. Heading text, not a record]

Heading body.`;
    const blocks = parseMarkdownToBlocks(markdown);

    expect(blocks.recordSpans.map((span) => span.id)).toEqual(["I1"]);
    expect(recordSource(markdown, blocks, 0)).toBe(`[I1. Real intent]

Owned body.`);
  });

  it("normalizes a lowercase record label before projecting it", () => {
    expect(parseMarkdownToBlocks("[i7. Lowercase label]").recordSpans).toEqual([
      {
        id: "I7",
        kind: "I",
        title: "Lowercase label",
        spanStart: 0,
        spanEnd: 1,
      },
    ]);
  });

  it("rejects old wrappers, attributes, levels, aliases, and reversed order", () => {
    for (const suffix of [
      '(Tags: human origin="direct_edit")',
      '(Tags: uncertainty-high source="agent")',
      "(Tags: uncertainty-high)",
      "(Tags: human-provided)",
      "(Tags: uncertain, human)",
    ]) {
      expect(parseRecordMetadataSuffix(suffix)).toEqual({
        tags: [],
        valid: false,
        canonical: false,
      });
    }

    const [wrapper] = parseMarkdownToBlocks(
      '<human origin="direct_edit">Legacy body.</human>',
    );
    expect(wrapper).toMatchObject({
      type: "paragraph",
      content: '<human origin="direct_edit">Legacy body.</human>',
    });
    expect(
      parseMarkdownToBlocks('<human origin="direct_edit">Legacy body.</human>')
        .recordSpans,
    ).toEqual([]);
  });

  it("owns body blocks through horizontal rules and stops at headings or the next I/D/Q label", () => {
    const markdown = `[I1. Multi-paragraph intent]

First paragraph.

Second paragraph.

---

After the divider.

## Decisions

[D2b. Lettered design]

Design body.
[Q3. Open question]`;
    const blocks = parseMarkdownToBlocks(markdown);

    expect(
      blocks.recordSpans.map(({ id, kind, title }) => ({ id, kind, title })),
    ).toEqual([
      { id: "I1", kind: "I", title: "Multi-paragraph intent" },
      { id: "D2b", kind: "D", title: "Lettered design" },
      { id: "Q3", kind: "Q", title: "Open question" },
    ]);
    expect(recordSource(markdown, blocks, 0)).toBe(`[I1. Multi-paragraph intent]

First paragraph.

Second paragraph.

---

After the divider.`);
    expect(recordSource(markdown, blocks, 1)).toBe(`[D2b. Lettered design]

Design body.`);
    expect(recordSource(markdown, blocks, 2)).toBe("[Q3. Open question]");
    expect(
      blocks
        .slice(blocks.recordSpans[0].spanStart, blocks.recordSpans[0].spanEnd)
        .some((block) => block.type === "hr"),
    ).toBe(true);
  });

  it("keeps deeper schema headings and their tables inside the enclosing record", () => {
    const markdown = `## Decisions

[D2. Final table schema inventory]

### \`orgs\`

| Column | Type |
| --- | --- |
| id | UUID |
| name | TEXT |

### \`users\`

| Column | Type |
| --- | --- |
| id | UUID |

## Questions

Unowned question preface.`;
    const blocks = parseMarkdownToBlocks(markdown);

    expect(blocks.recordSpans).toHaveLength(1);
    expect(recordSource(markdown, blocks, 0)).toContain("### `orgs`");
    expect(recordSource(markdown, blocks, 0)).toContain("| name | TEXT |");
    expect(recordSource(markdown, blocks, 0)).toContain("### `users`");
    expect(recordSource(markdown, blocks, 0)).not.toContain("## Questions");
  });

  it("keeps rich block constructs inside one record with exact first/last source ranges", () => {
    const markdown = `[D4. Composite design]

- Parent
  wrapped continuation

- Second

| Name | Value |
| --- | --- |
| a | b |

\`\`\`ts
const label = "[I99. code, not a record]";
\`\`\`

:::note
Callout body
:::

> Quoted

![Diagram](diagram.png)`;
    const blocks = parseMarkdownToBlocks(markdown);
    const [span] = blocks.recordSpans;
    const owned = blocks.slice(span.spanStart, span.spanEnd);

    expect(blocks.recordSpans).toHaveLength(1);
    expect(owned.map((block) => block.type)).toEqual([
      "paragraph",
      "bullet-list",
      "table",
      "code",
      "callout",
      "blockquote",
      "image",
    ]);
    expect(recordSource(markdown, blocks, 0)).toBe(markdown);
    expect(blocksToMarkdown(blocks)).toBe(markdown);
  });

  it("ignores label-like text in fenced code and the derived References index", () => {
    const markdown = `\`\`\`md
[I90. Not a record]
\`\`\`

[D1. Real record]

Body.

## References

[D99. Derived index entry]

Reference prose.

## Behavior

[Q2. Real question]`;
    const blocks = parseMarkdownToBlocks(markdown);

    expect(blocks.recordSpans.map((span) => span.id)).toEqual(["D1", "Q2"]);
  });

  it("starts records below Boundary headings and keeps label-only EOF records", () => {
    const markdown = `## Boundary

[I2. Boundary intent]

Body.

[D3. Label only]`;
    const blocks = parseMarkdownToBlocks(markdown);

    expect(blocks.recordSpans.map((span) => span.id)).toEqual(["I2", "D3"]);
    expect(recordSource(markdown, blocks, 1)).toBe("[D3. Label only]");
  });

  it("keeps each canonical tag suffix scoped to its own record", () => {
    const markdown = `[I1. Human record] (Tags: human)

Human body.

[D2. Uncertain record] (Tags: uncertain)

Uncertain body.

[Q3. Plain question]

\`\`\`text
code inside the plain record
\`\`\`

---`;
    const blocks = parseMarkdownToBlocks(markdown);
    const [human, uncertain, plain] = blocks.recordSpans;

    expect(human).toMatchObject({
      id: "I1",
      metadataTags: [{ kind: "human", syntax: "inline" }],
    });
    expect(uncertain).toMatchObject({
      id: "D2",
      metadataTags: [{ kind: "uncertainty", syntax: "inline" }],
    });
    expect(recordSource(markdown, blocks, 1))
      .toBe(`[D2. Uncertain record] (Tags: uncertain)

Uncertain body.`);
    expect(plain).toMatchObject({
      id: "Q3",
    });
    expect(plain.metadataTags).toBeUndefined();
  });

  it("keeps canonical metadata and nested rich blocks inside the shared extent fixture", () => {
    const blocks = parseMarkdownToBlocks(semanticRecordExtentParity);

    expect(blocks.recordSpans.map((span) => span.id)).toEqual([
      "I1",
      "D2",
      "D3",
      "Q4",
    ]);
    expect(
      blocks.recordSpans.map((_, index) =>
        recordSource(semanticRecordExtentParity, blocks, index),
      ),
    ).toEqual([
      `[I1. Multi-paragraph intent]

First paragraph.

Second paragraph.

---

After the divider.`,
      `[D2. Canonical uncertain record] (Tags: uncertain)

Owned body.

### Nested inventory

| Name | State |
| --- | --- |
| sample | open |`,
      `[D3. Record before another label]

Design body.`,
      `[Q4. Label at EOF]

Final question body.`,
    ]);
  });
});

describe("parseMarkdownToBlocks label-free section spans", () => {
  it("segments H1/H2/H3 documents flatly at every heading", () => {
    const markdown = `# One

Intro.

## Two

- item

### Three

\`\`\`text
body
\`\`\``;
    const blocks = parseMarkdownToBlocks(markdown);

    expect(blocks.recordSpans).toEqual([]);
    expect(blocks.sectionSpans).toEqual([
      { title: "One", spanStart: 0, spanEnd: 2 },
      { title: "Two", spanStart: 2, spanEnd: 4 },
      { title: "Three", spanStart: 4, spanEnd: 6 },
    ]);
    expect(blocksToMarkdown(blocks)).toBe(markdown);
  });

  it("creates an untitled leading preface section before the first heading", () => {
    const blocks = parseMarkdownToBlocks(`Preface paragraph.

> Preface quote

## Main

Body.`);

    expect(blocks.sectionSpans).toEqual([
      { title: "", spanStart: 0, spanEnd: 2 },
      { title: "Main", spanStart: 2, spanEnd: 4 },
    ]);
  });

  it("treats a non-empty no-heading document as one untitled section", () => {
    const markdown = `Paragraph.

| A | B |
| --- | --- |
| 1 | 2 |

- item

\`\`\`ts
const value = 1;
\`\`\``;
    const blocks = parseMarkdownToBlocks(markdown);

    expect(blocks.sectionSpans).toEqual([
      { title: "", spanStart: 0, spanEnd: blocks.length },
    ]);
    expect(
      blocks
        .slice(blocks.sectionSpans[0].spanStart, blocks.sectionSpans[0].spanEnd)
        .map((block) => block.type),
    ).toEqual(["paragraph", "table", "bullet-list", "code"]);
    expect(blocksToMarkdown(blocks)).toBe(markdown);
  });

  it("uses record mode when any legal I/D/Q label is present", () => {
    const blocks = parseMarkdownToBlocks(`# Intro

Preface.

[I1. Record]

Body.

## Later

More.`);

    expect(blocks.recordSpans.map((span) => span.id)).toEqual(["I1"]);
    expect(blocks.sectionSpans).toEqual([]);
  });

  it("returns no spans for an empty document", () => {
    const blocks = parseMarkdownToBlocks("");

    expect(blocks.recordSpans).toEqual([]);
    expect(blocks.sectionSpans).toEqual([]);
  });
});

// =============================================================================
// highlightRust (backward compat) + highlightCode (multi-language)
// =============================================================================

describe("highlightRust", () => {
  it("returns empty string for empty input", () => {
    expect(highlightRust("")).toBe("");
  });

  it("returns empty string for null-ish input", () => {
    expect(highlightRust(null as unknown as string)).toBe("");
    expect(highlightRust(undefined as unknown as string)).toBe("");
  });

  it("highlights keywords with inline styles", () => {
    const result = highlightRust("fn main() {}");
    expect(result).toContain(
      '<span style="color:var(--g5-syn-keyword)">fn</span>',
    );
  });

  it("highlights multiple keywords", () => {
    const result = highlightRust("let mut x = 5;");
    expect(result).toContain(
      '<span style="color:var(--g5-syn-keyword)">let</span>',
    );
    // hljs includes the trailing space inside the keyword scope for `mut`
    expect(result).toContain('<span style="color:var(--g5-syn-keyword)">mut');
  });

  it("highlights PascalCase types", () => {
    const result = highlightRust("Vec<String>");
    expect(result).toContain('style="color:var(--g5-syn-type)">Vec</span>');
    expect(result).toContain('style="color:var(--g5-syn-type)">String</span>');
  });

  it("highlights string literals", () => {
    const result = highlightRust('"hello world"');
    expect(result).toContain('style="color:var(--g5-syn-string)"');
    expect(result).toContain("hello world");
  });

  it("highlights char literals in context", () => {
    const result = highlightRust("let c = 'a';");
    expect(result).toContain("a");
  });

  it("highlights line comments", () => {
    const result = highlightRust("// this is a comment");
    expect(result).toContain(
      'style="color:var(--g5-syn-comment);font-style:italic">// this is a comment</span>',
    );
  });

  it("highlights block comments", () => {
    const result = highlightRust("/* block comment */");
    expect(result).toContain('style="color:var(--g5-syn-comment)');
  });

  it("highlights macros in context", () => {
    const result = highlightRust('println!("hello")');
    expect(result).toContain("println");
  });

  it("highlights attributes", () => {
    const result = highlightRust("#[derive(Debug)]");
    expect(result).toContain('style="color:var(--g5-syn-attribute)"');
  });

  it("highlights inner attributes", () => {
    const result = highlightRust("#![allow(unused)]");
    expect(result).toContain('style="color:var(--g5-syn-attribute)"');
  });

  it("highlights lifetimes", () => {
    const result = highlightRust("'static");
    expect(result).toContain('style="color:var(--g5-syn-constant)"');
    expect(result).toContain("static");
  });

  it("highlights numbers", () => {
    const result = highlightRust("42");
    expect(result).toContain(
      '<span style="color:var(--g5-syn-number)">42</span>',
    );
  });

  it("highlights numbers with type suffixes", () => {
    const result = highlightRust("42u32");
    expect(result).toContain('style="color:var(--g5-syn-number)"');
    expect(result).toContain("42u32");
  });

  it("escapes HTML entities before highlighting", () => {
    const result = highlightRust("x < 5 && y > 3");
    expect(result).toContain("&lt;");
    expect(result).toContain("&gt;");
    expect(result).toContain("&amp;");
  });

  it("highlights a complete function", () => {
    const code = `fn greet(name: &str) -> String {
    format!("Hello, {}", name)
}`;
    const result = highlightRust(code);
    expect(result).toContain(
      '<span style="color:var(--g5-syn-keyword)">fn</span>',
    );
    expect(result).toContain('style="color:var(--g5-syn-type)">String</span>');
    expect(result).toContain('style="color:var(--g5-syn-string)"');
  });

  it("highlights a struct definition", () => {
    const code = `pub struct Config {
    pub name: String,
    pub count: u32,
}`;
    const result = highlightRust(code);
    expect(result).toContain(
      '<span style="color:var(--g5-syn-keyword)">pub</span>',
    );
    expect(result).toContain(
      '<span style="color:var(--g5-syn-keyword)">struct</span>',
    );
    expect(result).toContain('style="color:var(--g5-syn-type)">Config</span>');
    expect(result).toContain('style="color:var(--g5-syn-type)">String</span>');
  });

  it("handles self and Self as keywords", () => {
    const result = highlightRust("self.value = Self::default()");
    expect(result).toContain(
      '<span style="color:var(--g5-syn-keyword)">self</span>',
    );
    expect(result).toContain(
      '<span style="color:var(--g5-syn-keyword)">Self</span>',
    );
  });

  it("handles true and false as literals", () => {
    const result = highlightRust("let x = true;");
    expect(result).toContain(
      '<span style="color:var(--g5-syn-constant)">true</span>',
    );
  });

  it("renders brackets unstyled (rainbow brackets removed with hljs)", () => {
    const result = highlightRust("fn f() { g(h()) }");
    expect(result).not.toContain("--g5-syn-bracket1");
    expect(result).not.toContain("--g5-syn-bracket2");
    expect(result).toContain("(");
    expect(result).toContain("{");
  });
});

// =============================================================================
// highlightCode — multi-language
// =============================================================================

describe("highlightCode", () => {
  it("returns empty string for empty input", () => {
    expect(highlightCode("", "rust")).toBe("");
  });

  it("delegates rust to the same highlighter as highlightRust", () => {
    const code = "fn main() {}";
    expect(highlightCode(code, "rust")).toBe(highlightRust(code));
    expect(highlightCode(code, "rs")).toBe(highlightRust(code));
  });

  // TypeScript / JavaScript
  it("highlights TypeScript keywords", () => {
    const result = highlightCode('const x: string = "hello";', "typescript");
    expect(result).toContain(
      'style="color:var(--g5-syn-keyword)">const</span>',
    );
    expect(result).toContain('style="color:var(--g5-syn-string)"');
  });

  it("highlights TS with tsx alias", () => {
    const result = highlightCode("export default function App() {}", "tsx");
    expect(result).toContain(
      'style="color:var(--g5-syn-keyword)">export</span>',
    );
    expect(result).toContain(
      'style="color:var(--g5-syn-keyword)">function</span>',
    );
  });

  it("highlights JavaScript", () => {
    const result = highlightCode("let arr = [1, 2, 3];", "js");
    expect(result).toContain('style="color:var(--g5-syn-keyword)">let</span>');
    expect(result).toContain('style="color:var(--g5-syn-number)">1</span>');
  });

  // Python
  it("highlights Python keywords", () => {
    const result = highlightCode(
      'def hello(name):\n    return f"Hello {name}"',
      "python",
    );
    expect(result).toContain('style="color:var(--g5-syn-keyword)">def</span>');
    expect(result).toContain(
      'style="color:var(--g5-syn-keyword)">return</span>',
    );
  });

  it("highlights Python comments", () => {
    const result = highlightCode("# this is a comment", "py");
    expect(result).toContain('style="color:var(--g5-syn-comment)');
  });

  it("highlights Python decorators", () => {
    const result = highlightCode("@property\ndef x(self): pass", "python");
    expect(result).toContain('style="color:var(--g5-syn-attribute)"');
  });

  it("leaves Python UPPER_CASE names unstyled but highlights values", () => {
    // The old UPPER_CASE→constant heuristic was dropped with the hljs
    // migration; hljs only styles genuine tokens.
    const result = highlightCode("MAX_SIZE = 100", "python");
    expect(result).toContain("MAX_SIZE");
    expect(result).toContain('style="color:var(--g5-syn-number)">100</span>');
  });

  // Go
  it("highlights Go keywords", () => {
    const result = highlightCode(
      'func main() {\n\tfmt.Println("hello")\n}',
      "go",
    );
    expect(result).toContain('style="color:var(--g5-syn-keyword)">func</span>');
  });

  // Java
  it("highlights Java keywords", () => {
    const result = highlightCode(
      "public class Main { public static void main(String[] args) {} }",
      "java",
    );
    expect(result).toContain(
      'style="color:var(--g5-syn-keyword)">public</span>',
    );
    expect(result).toContain(
      'style="color:var(--g5-syn-keyword)">class</span>',
    );
    expect(result).toContain('style="color:var(--g5-syn-type)">Main</span>');
    expect(result).toContain(
      'style="color:var(--g5-syn-function)">main</span>',
    );
  });

  // C++
  it("highlights C++ preprocessor directives", () => {
    const result = highlightCode(
      "#include <iostream>\nint main() { return 0; }",
      "cpp",
    );
    expect(result).toContain('style="color:var(--g5-syn-function)');
  });

  // SQL
  it("highlights SQL keywords case-insensitively", () => {
    const result = highlightCode("SELECT * FROM users WHERE id = 1;", "sql");
    expect(result).toContain(
      'style="color:var(--g5-syn-keyword)">SELECT</span>',
    );
    expect(result).toContain('style="color:var(--g5-syn-keyword)">FROM</span>');
    expect(result).toContain(
      'style="color:var(--g5-syn-keyword)">WHERE</span>',
    );
  });

  it("highlights SQL strings", () => {
    const result = highlightCode(
      "SELECT * FROM users WHERE name = 'Alice';",
      "sql",
    );
    expect(result).toContain('style="color:var(--g5-syn-string)"');
  });

  // Bash
  it("highlights Bash keywords and variables", () => {
    const result = highlightCode(
      'if [ "$HOME" = "/root" ]; then echo "root"; fi',
      "bash",
    );
    expect(result).toContain('style="color:var(--g5-syn-keyword)">if</span>');
    expect(result).toContain('style="color:var(--g5-syn-keyword)">then</span>');
  });

  it("highlights shell variables", () => {
    const result = highlightCode("echo $HOME", "sh");
    expect(result).toContain(
      'style="color:var(--g5-syn-variable)">$HOME</span>',
    );
  });

  // YAML
  it("highlights YAML keys and values", () => {
    const result = highlightCode("name: hello\nenabled: true", "yaml");
    expect(result).toContain(
      'style="color:var(--g5-syn-attribute)">name:</span>',
    );
    expect(result).toContain(
      'style="color:var(--g5-syn-constant)">true</span>',
    );
  });

  // TOML
  it("highlights TOML", () => {
    const result = highlightCode(
      '[package]\nname = "my-app"\nversion = "0.1.0"',
      "toml",
    );
    expect(result).toContain('style="color:var(--g5-syn-string)"');
  });

  // JSON
  it("highlights JSON", () => {
    const result = highlightCode(
      '{ "key": "value", "num": 42, "ok": true }',
      "json",
    );
    expect(result).toContain('style="color:var(--g5-syn-string)"');
    expect(result).toContain('style="color:var(--g5-syn-number)">42</span>');
    expect(result).toContain('style="color:var(--g5-syn-keyword)">true</span>');
  });

  // CSS
  it("highlights CSS", () => {
    const result = highlightCode(".header { color: red; }", "css");
    expect(result).toContain('style="color:var(--g5-syn-type)">.header</span>');
    expect(result).toContain(
      'style="color:var(--g5-syn-attribute)">color</span>',
    );
  });

  // Ruby
  it("highlights Ruby", () => {
    const result = highlightCode(
      'def greet(name)\n  puts "Hello #{name}"\nend',
      "ruby",
    );
    expect(result).toContain('style="color:var(--g5-syn-keyword)">def</span>');
    expect(result).toContain('style="color:var(--g5-syn-keyword)">end</span>');
  });

  // Swift
  it("highlights Swift", () => {
    const result = highlightCode(
      'func greet(_ name: String) -> String { return "Hello" }',
      "swift",
    );
    expect(result).toContain('style="color:var(--g5-syn-keyword)">func</span>');
    expect(result).toContain(
      'style="color:var(--g5-syn-keyword)">return</span>',
    );
    expect(result).toContain('style="color:var(--g5-syn-type)">String</span>');
  });

  // Kotlin
  it("highlights Kotlin", () => {
    const result = highlightCode("fun main() { val x = 42 }", "kotlin");
    expect(result).toContain('style="color:var(--g5-syn-keyword)">fun</span>');
    expect(result).toContain('style="color:var(--g5-syn-keyword)">val</span>');
  });

  // Elixir
  it("highlights Elixir", () => {
    const result = highlightCode(
      "defmodule Hello do\n  def greet, do: :world\nend",
      "elixir",
    );
    expect(result).toContain(
      'style="color:var(--g5-syn-keyword)">defmodule</span>',
    );
    expect(result).toContain('style="color:var(--g5-syn-keyword)">def</span>');
  });

  // PHP
  it("highlights PHP", () => {
    const result = highlightCode(
      '<?php\nfunction hello() { echo "hi"; }',
      "php",
    );
    expect(result).toContain(
      'style="color:var(--g5-syn-keyword)">function</span>',
    );
    expect(result).toContain('style="color:var(--g5-syn-keyword)">echo</span>');
  });

  // C#
  it("highlights C#", () => {
    const result = highlightCode(
      "public class App { static void Main() {} }",
      "csharp",
    );
    expect(result).toContain(
      'style="color:var(--g5-syn-keyword)">public</span>',
    );
    expect(result).toContain(
      'style="color:var(--g5-syn-keyword)">class</span>',
    );
  });

  // Haskell
  it("highlights Haskell", () => {
    const result = highlightCode(
      "module Main where\nimport Data.List",
      "haskell",
    );
    expect(result).toContain(
      'style="color:var(--g5-syn-keyword)">module</span>',
    );
    expect(result).toContain(
      'style="color:var(--g5-syn-keyword)">where</span>',
    );
  });

  // Lua
  it("highlights Lua", () => {
    const result = highlightCode(
      'local function greet()\n  return "hello"\nend',
      "lua",
    );
    expect(result).toContain(
      'style="color:var(--g5-syn-keyword)">local</span>',
    );
    expect(result).toContain(
      'style="color:var(--g5-syn-keyword)">function</span>',
    );
    expect(result).toContain('style="color:var(--g5-syn-keyword)">end</span>');
  });

  // Zig
  it("falls back to escaped plain text for Zig (no registered grammar)", () => {
    const result = highlightCode(
      'const std = @import("std");\npub fn main() {}',
      "zig",
    );
    expect(result).not.toContain("<span");
    expect(result).toContain('const std = @import("std");');
  });

  // Unknown language gets generic highlighting
  it("returns escaped plain text for an unknown language", () => {
    // The old generic keyword-guessing fallback was dropped with the hljs
    // migration: an unregistered language renders as escaped plain text.
    const result = highlightCode("// comment\nconst x = 42;", "foobar");
    expect(result).not.toContain("<span");
    expect(result).toContain("// comment");
    expect(result).toContain("const x = 42;");
  });

  // Alias resolution
  it("resolves language aliases", () => {
    expect(highlightCode("let x = 1;", "ts")).toContain(
      'style="color:var(--g5-syn-keyword)">let</span>',
    );
    expect(highlightCode("x = 1", "py")).toContain(
      'style="color:var(--g5-syn-number)">1</span>',
    );
    expect(highlightCode("echo hi", "sh")).toContain(
      'style="color:var(--g5-syn-type)">echo</span>',
    );
  });

  // Rainbow brackets across languages
  it("renders brackets unstyled in all languages (rainbow removed)", () => {
    const result = highlightCode("f(g(h()))", "javascript");
    expect(result).not.toContain("--g5-syn-bracket1");
    expect(result).not.toContain("--g5-syn-bracket2");
    expect(result).toContain("(");
  });
});

// =============================================================================
// renderInlineMarkdown
// =============================================================================

describe("renderInlineMarkdown", () => {
  it("returns null for empty input", () => {
    expect(renderInlineMarkdown("")).toBeNull();
  });

  it("returns null for null input", () => {
    expect(renderInlineMarkdown(null as unknown as string)).toBeNull();
  });

  it("returns plain text as-is", () => {
    const result = renderInlineMarkdown("Hello world");
    expect(extractText(result)).toBe("Hello world");
  });

  it("renders **bold** as <strong>", () => {
    const result = renderInlineMarkdown("This is **bold** text");
    const text = extractText(result);
    expect(text).toContain("bold");
    // The result should contain a strong element
    if (
      Array.isArray(result) ||
      (React.isValidElement(result) && result.type === React.Fragment)
    ) {
      const children: React.ReactNode[] = React.isValidElement(result)
        ? React.Children.toArray(
            (result.props as { children: React.ReactNode }).children,
          )
        : Array.isArray(result)
          ? result
          : [];
      const strongEl = children.find(
        (c) => React.isValidElement(c) && c.type === "strong",
      );
      expect(strongEl).toBeTruthy();
    }
  });

  it("renders combined ***bold italic*** without exposing delimiters", () => {
    const result = renderInlineMarkdown("Before ***both*** after");
    const strong = collectElementsByType(result, "strong");

    expect(strong).toHaveLength(1);
    const emphasis = collectElementsByType(strong[0], "em");
    expect(emphasis).toHaveLength(1);
    expect(extractText(emphasis[0])).toBe("both");
    expect(extractText(result)).toBe("Before both after");
  });

  it("keeps a bold span active across a soft line break", () => {
    const result = renderInlineMarkdown(
      '**The "Hands off to"\ncolumn **names the boundary.',
    );
    const strong = collectElementsByType(result, "strong");

    expect(strong).toHaveLength(1);
    expect(extractText(strong[0])).toBe('The "Hands off to"\ncolumn ');
    expect(extractText(result)).toBe(
      'The "Hands off to"\ncolumn names the boundary.',
    );
  });

  it("renders *italic* as <em>", () => {
    const result = renderInlineMarkdown("This is *italic* text");
    const text = extractText(result);
    expect(text).toContain("italic");
  });

  it("renders `code` as <code>", () => {
    const result = renderInlineMarkdown("Use `npm install`");
    const text = extractText(result);
    expect(text).toContain("npm install");
  });

  it("renders [text](url) as <a>", () => {
    const result = renderInlineMarkdown("Click [here](https://example.com)");
    const text = extractText(result);
    expect(text).toContain("here");
    const anchors = collectElementsByType(result, "a");
    expect(anchors).toHaveLength(1);
    expect((anchors[0].props as { href: string }).href).toBe(
      "https://example.com",
    );
  });

  it.each([
    "/Users/example/plan.md",
    "/workspace/project/plan.md",
    "/work/docs/docs/ws/plan.md",
    "/home/user/plan.md",
    "/etc/g5/plan.md",
    "/opt/g5/plan.md",
    "/root/plan.md",
    "/private/tmp/plan.md",
    "/tmp/plan.md",
    "/var/folders/worker-storage/private.md",
    "file:///tmp/plan.md",
    "data:text/html,<b>x</b>",
    "javascript:alert(1)",
    "//server/share/plan.md",
    "\\\\server\\share\\plan.md",
    "C:\\tmp\\plan.md",
    "C:/tmp/plan.md",
    "Project Docs/plan.md",
  ])("renders unsafe local markdown href %s as non-clickable text", (href) => {
    const result = renderInlineMarkdown(`Open [local plan](${href})`);
    const text = extractText(result);
    expect(text).toContain("local plan");
    expect(text).toContain(href);
    expect(collectElementsByType(result, "a")).toHaveLength(0);
  });

  it("keeps existing app-relative markdown links clickable when not filesystem paths", () => {
    const result = renderInlineMarkdown("Open [workspace](/workspaces/abc)");
    const anchors = collectElementsByType(result, "a");
    expect(anchors).toHaveLength(1);
    expect((anchors[0].props as { href: string }).href).toBe("/workspaces/abc");
  });

  it("keeps repo-relative markdown links clickable", () => {
    const result = renderInlineMarkdown(
      "Open [plan](docs/docs/ws-display-001/plan.md)",
    );
    const anchors = collectElementsByType(result, "a");
    expect(anchors).toHaveLength(1);
    expect((anchors[0].props as { href: string }).href).toBe(
      "docs/docs/ws-display-001/plan.md",
    );
  });

  it("replaces emoji shortcodes", () => {
    const result = renderInlineMarkdown("Great :fire: work :rocket:");
    const text = extractText(result);
    expect(text).toContain("🔥");
    expect(text).toContain("🚀");
  });

  it("handles multiple shortcodes", () => {
    const text = extractText(renderInlineMarkdown(":check: Done :tada:"));
    expect(text).toContain("✅");
    expect(text).toContain("🎉");
  });

  it("supports all 21 emoji shortcodes", () => {
    const shortcodes: Record<string, string> = {
      ":smile:": "😊",
      ":grin:": "😁",
      ":laugh:": "😂",
      ":wink:": "😉",
      ":thumbsup:": "👍",
      ":thumbsdown:": "👎",
      ":heart:": "❤️",
      ":fire:": "🔥",
      ":star:": "⭐",
      ":check:": "✅",
      ":x:": "❌",
      ":warning:": "⚠️",
      ":info:": "ℹ️",
      ":rocket:": "🚀",
      ":bug:": "🐛",
      ":sparkles:": "✨",
      ":tada:": "🎉",
      ":thinking:": "🤔",
      ":eyes:": "👀",
      ":wave:": "👋",
      ":100:": "💯",
    };
    for (const [code, emoji] of Object.entries(shortcodes)) {
      const text = extractText(renderInlineMarkdown(code));
      expect(text).toContain(emoji);
    }
  });

  it("handles mixed inline formatting", () => {
    const text = extractText(
      renderInlineMarkdown("**bold** and *italic* and `code`"),
    );
    expect(text).toContain("bold");
    expect(text).toContain("italic");
    expect(text).toContain("code");
  });

  it("preserves text between formatted segments", () => {
    const text = extractText(renderInlineMarkdown("start **middle** end"));
    expect(text).toBe("start middle end");
  });
});

// =============================================================================
// formatLargeNumber
// =============================================================================

describe("fileNameFromPath", () => {
  it("returns the last segment, excluding directories", () => {
    expect(fileNameFromPath("designs/foo/bar.md")).toBe("bar.md");
  });

  it("keeps the file extension", () => {
    expect(fileNameFromPath("views/pm_views/chat_agent.pm_view.md")).toBe(
      "chat_agent.pm_view.md",
    );
  });

  it("returns the input when there is no directory separator", () => {
    expect(fileNameFromPath("README.md")).toBe("README.md");
  });

  it("ignores a trailing slash", () => {
    expect(fileNameFromPath("a/b/c/")).toBe("c");
  });
});

describe("formatLargeNumber", () => {
  it("formats millions", () => {
    expect(formatLargeNumber(1_234_567)).toBe("1.2M");
  });

  it("formats exactly 1M", () => {
    expect(formatLargeNumber(1_000_000)).toBe("1.0M");
  });

  it("formats thousands", () => {
    expect(formatLargeNumber(3_456)).toBe("3.5K");
  });

  it("formats exactly 1K", () => {
    expect(formatLargeNumber(1_000)).toBe("1.0K");
  });

  it("returns plain number for < 1000", () => {
    expect(formatLargeNumber(123)).toBe("123");
  });

  it('returns "0" for zero', () => {
    expect(formatLargeNumber(0)).toBe("0");
  });

  it('returns "0" for NaN', () => {
    expect(formatLargeNumber(NaN)).toBe("0");
  });

  it('returns "0" for null', () => {
    expect(formatLargeNumber(null as unknown as number)).toBe("0");
  });

  it('returns "0" for undefined', () => {
    expect(formatLargeNumber(undefined as unknown as number)).toBe("0");
  });

  it("handles 10M+", () => {
    expect(formatLargeNumber(15_000_000)).toBe("15.0M");
  });
});

// =============================================================================
// formatCost
// =============================================================================

describe("formatCost", () => {
  it("formats a normal dollar amount", () => {
    expect(formatCost(12.5)).toBe("$12.50");
  });

  it("formats zero", () => {
    expect(formatCost(0)).toBe("$0.00");
  });

  it("formats nonzero sub-cent amounts without hiding them as zero", () => {
    expect(formatCost(0.003)).toBe("<$0.01");
  });

  it("formats large amounts", () => {
    expect(formatCost(1234.56)).toBe("$1234.56");
  });

  it("returns $0.00 for NaN", () => {
    expect(formatCost(NaN)).toBe("$0.00");
  });

  it("returns $0.00 for null", () => {
    expect(formatCost(null as unknown as number)).toBe("$0.00");
  });

  it("returns $0.00 for undefined", () => {
    expect(formatCost(undefined as unknown as number)).toBe("$0.00");
  });

  it("rounds to 2 decimal places", () => {
    expect(formatCost(1.999)).toBe("$2.00");
  });
});

describe("labelWorkspaceFolderInPath", () => {
  const WS = "019ecc9b-8ca2-73c2-9ce3-c184b5d31673";
  const names = { [WS]: "compliance-service" };

  it("swaps the docs/<workspace_id> segment for the workspace name", () => {
    expect(labelWorkspaceFolderInPath(`docs/${WS}/a.md`, names)).toBe(
      "docs/compliance-service/a.md",
    );
  });

  it("relabels nested paths under the workspace folder", () => {
    expect(labelWorkspaceFolderInPath(`docs/${WS}/sub/dir/b.md`, names)).toBe(
      "docs/compliance-service/sub/dir/b.md",
    );
  });

  it("falls back to the raw UUID for unknown or absent ids", () => {
    expect(labelWorkspaceFolderInPath(`docs/${WS}/a.md`, {})).toBe(
      `docs/${WS}/a.md`,
    );
    expect(labelWorkspaceFolderInPath(`docs/${WS}/a.md`, undefined)).toBe(
      `docs/${WS}/a.md`,
    );
  });

  it("is a no-op on non-docs paths and root-level docs files", () => {
    expect(labelWorkspaceFolderInPath("views/x.arch_view.md", names)).toBe(
      "views/x.arch_view.md",
    );
    expect(labelWorkspaceFolderInPath("src/y.ts", names)).toBe("src/y.ts");
    expect(labelWorkspaceFolderInPath("docs/outside.md", names)).toBe(
      "docs/outside.md",
    );
  });

  it("renders a name containing '/' as a single segment (no phantom nesting)", () => {
    expect(labelWorkspaceFolderInPath(`docs/${WS}/a.md`, { [WS]: "a/b" })).toBe(
      "docs/a∕b/a.md",
    );
  });
});

describe("sanitizeInlineSvg", () => {
  it("keeps benign SVG content intact", () => {
    const svg =
      '<svg xmlns="http://www.w3.org/2000/svg"><rect width="10" height="10"></rect></svg>';
    const out = sanitizeInlineSvg(svg);
    expect(out).toContain("<rect");
    expect(out).toContain('width="10"');
  });

  it("strips <script> and on* event handlers", () => {
    const svg =
      '<svg xmlns="http://www.w3.org/2000/svg" onload="alert(1)">' +
      '<script>alert(2)</script><rect onclick="steal()" width="5" height="5"></rect></svg>';
    const out = sanitizeInlineSvg(svg);
    expect(out).not.toContain("<script");
    expect(out).not.toContain("onload");
    expect(out).not.toContain("onclick");
    expect(out).toContain("<rect");
  });

  it("strips javascript: hrefs", () => {
    const svg =
      '<svg xmlns="http://www.w3.org/2000/svg"><a href="javascript:alert(1)">' +
      "<text>x</text></a></svg>";
    const out = sanitizeInlineSvg(svg);
    expect(out).not.toMatch(/href="javascript:/i);
  });
});

// ---------------------------------------------------------------------------
// classifyChangedPath — changed-path artifact taxonomy (draft-management D16)
// ---------------------------------------------------------------------------

describe("classifyChangedPath", () => {
  it("classifies current ontology Markdown without including sibling plans or runtime files", () => {
    expect(classifyChangedPath(".g5/ontology/root.g5.md")).toBe("ontology");
    expect(classifyChangedPath(".g5/ontology/features/index.g5.md")).toBe(
      "ontology",
    );
    // Only `*.g5.md` pages are ontology; other Markdown there is ordinary.
    expect(classifyChangedPath(".g5/ontology/features/index.md")).toBe("other");
    expect(classifyChangedPath(".g5/plans/test.md")).toBe("other");
    expect(classifyChangedPath(".g5/ontology-cache/test.md")).toBe("other");
  });
  it("classifies ontology spec files by `.semlift/specs/` segment", () => {
    expect(
      classifyChangedPath(
        ".semlift/specs/software_architecture/client/workspace-page/draft-management.semlift.md",
      ),
    ).toBe("ontology");
  });

  it("classifies ontology by a `.semlift.md` suffix regardless of folder", () => {
    expect(classifyChangedPath("some/other/place/foo.semlift.md")).toBe(
      "ontology",
    );
  });

  it("classifies ontology when the path carries a repo/route prefix", () => {
    // Segment matching, not anchored prefixes.
    expect(
      classifyChangedPath("generation/.semlift/specs/client/thing.semlift.md"),
    ).toBe("ontology");
  });

  it("classifies intent_record paths as intent_grounding", () => {
    expect(classifyChangedPath("intent_record/code/foo.jsonl")).toBe(
      "intent_grounding",
    );
  });

  it("classifies intent_grounding paths as intent_grounding", () => {
    expect(classifyChangedPath("intent_grounding/code/foo.jsonl")).toBe(
      "intent_grounding",
    );
  });

  it("classifies docs-side intent records as intent_grounding", () => {
    expect(
      classifyChangedPath("g5_docs/intent_record/chat/session.jsonl"),
    ).toBe("intent_grounding");
  });

  it("classifies user source files as code_docs", () => {
    expect(classifyChangedPath("generation/client/src/lib/utils.ts")).toBe(
      "code_docs",
    );
    expect(classifyChangedPath("generation/g5-api/src/main.rs")).toBe(
      "code_docs",
    );
  });

  it("classifies project docs as code_docs", () => {
    expect(classifyChangedPath("g5_docs/docs/019ec/plan.md")).toBe("code_docs");
  });

  it("classifies views (view/test_view) as code_docs", () => {
    expect(classifyChangedPath("g5_docs/views/pm_views/thing.pm_view.md")).toBe(
      "code_docs",
    );
    expect(classifyChangedPath("views/test_views/flow.test_view.md")).toBe(
      "code_docs",
    );
    expect(classifyChangedPath("README.md")).toBe("code_docs");
  });

  it("classifies generated human-view projections as other", () => {
    expect(classifyChangedPath("outputs/thing.human-view.json")).toBe("other");
    expect(classifyChangedPath("outputs/thing.human-view.html")).toBe("other");
  });

  it("classifies internal .g5 state as other", () => {
    expect(classifyChangedPath(".g5/experience/_index.keywords.json")).toBe(
      "other",
    );
  });

  it("does not throw on empty/degenerate input and defaults to code_docs", () => {
    expect(classifyChangedPath("")).toBe("code_docs");
    expect(classifyChangedPath("   ")).toBe("code_docs");
    expect(classifyChangedPath("/")).toBe("code_docs");
  });

  it("exposes stable category order with only code_docs open by default", () => {
    expect(ARTIFACT_CATEGORY_ORDER).toEqual([
      "code_docs",
      "ontology",
      "intent_grounding",
      "other",
    ]);
    const openByDefault = ARTIFACT_CATEGORY_META.filter(
      (meta) => meta.defaultOpen,
    ).map((meta) => meta.category);
    expect(openByDefault).toEqual(["code_docs"]);
  });
});
