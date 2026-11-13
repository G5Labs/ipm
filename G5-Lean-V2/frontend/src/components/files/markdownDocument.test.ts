import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { getSchema, type JSONContent } from "@tiptap/core";
import type { Node as PMNode } from "@tiptap/pm/model";
import {
  isEmptyStatement,
  protectedSignature,
  richMarkdownExtensions,
  statementLayout,
} from "./richExtensions";
import {
  docToMarkdown,
  markdownToDoc,
  opaqueMarkdown,
  topLevelPositionAt,
} from "./markdownDocument";

// `g5.documents.preview.rich-editing`: the rich editor loads a file into a
// structured document and writes it back. A document saved without edits is
// the file itself; an edit changes the edited block and, at most, the blank
// line around it; constructs the editor does not model survive verbatim.

const root = join(__dirname, "..", "..", "..", "..");
const read = (path: string) => readFileSync(join(root, path), "utf8");
const schema = getSchema(richMarkdownExtensions());

const documents = [
  "README.md",
  "AGENTS.md",
  "docs/architecture.md",
  "docs/g5d-pr.md",
  "docs/reference/designs/TECH_DEBT_PRODUCT_BUG_PILOT.md",
  "docs/preview-block-editing-design.md",
];
const ontologies = [
  ".g5/ontology/root.g5.md",
  ".g5/ontology/features/documents.g5.md",
  ".g5/ontology/concepts/system_invariants.g5.md",
];

function load(source: string, ontology = false) {
  const loaded = markdownToDoc(source, { ontology });
  const doc = schema.nodeFromJSON(loaded.json);
  doc.check();
  loaded.baseline.seal(doc);
  const save = (json: JSONContent) => {
    const edited = schema.nodeFromJSON(json);
    edited.check();
    return docToMarkdown(edited, loaded.baseline).text;
  };
  return { ...loaded, doc, save, copy: () => clone(loaded.json) };
}

const clone = <T>(value: T): T => JSON.parse(JSON.stringify(value)) as T;

/** Strip bookkeeping so two documents compare by what they say. */
function meaning(doc: PMNode): JSONContent {
  return JSON.parse(
    JSON.stringify(doc.toJSON(), (key, value) =>
      key === "uid" || key === "src" ? undefined : value,
    ),
  );
}

/** Append to the last text in a node, keeping its marks. */
function appendText(node: JSONContent, suffix: string): boolean {
  let last: JSONContent | null = null;
  const visit = (child: JSONContent) => {
    if (child.type === "text") last = child;
    else if (child.type !== "codeBlock") (child.content ?? []).forEach(visit);
  };
  visit(node);
  if (!last) return false;
  (last as JSONContent).text = `${(last as JSONContent).text}${suffix}`;
  return true;
}

/** The lines of `after` that differ from `before`, as a range in `before`. */
function changedLines(before: string, after: string) {
  const a = before.split("\n");
  const b = after.split("\n");
  let head = 0;
  while (head < a.length && head < b.length && a[head] === b[head]) head++;
  let tail = 0;
  while (
    tail < a.length - head &&
    tail < b.length - head &&
    a[a.length - 1 - tail] === b[b.length - 1 - tail]
  )
    tail++;
  return { first: head, last: a.length - tail };
}

const lineOf = (text: string, offset: number) =>
  text.slice(0, offset).split("\n").length - 1;

describe.each([
  ...documents.map((path) => [path, false] as const),
  ...ontologies.map((path) => [path, true] as const),
])("the rich document for %s", (path, ontology) => {
  const source = read(path);

  it("saves the file itself when nothing was edited", () => {
    const { doc, baseline } = load(source, ontology);
    expect(docToMarkdown(doc, baseline).text).toBe(source);
  });

  it(
    "changes only an edited block's lines and keeps its meaning",
    { timeout: 120_000 },
    async () => {
      const loaded = load(source, ontology);
      const content = loaded.json.content ?? [];
      let edits = 0;
      for (const [index, node] of content.entries()) {
        // Large files take over a minute in all: yield now and then, so the
        // worker answers the test runner's calls, which time out after 60 s.
        if (index % 25 === 0) await new Promise((r) => setTimeout(r, 0));
        if (node.type === "opaqueBlock" || node.type === "codeBlock") continue;
        const json = loaded.copy();
        if (!appendText(json.content![index], " edited")) continue;
        const text = loaded.save(json);
        edits++;
        // Locality: the change lies within the block's own lines, plus the
        // separator line before or after it at most.
        const start = loaded.doc.child(index).attrs.uid
          ? [...loaded.baseline.entries.values()][index].start
          : 0;
        const src = [...loaded.baseline.entries.values()][index].src;
        const range = changedLines(source, text);
        expect(range.first, `${path} node ${index}`).toBeGreaterThanOrEqual(
          lineOf(source, start) - 1,
        );
        expect(range.last, `${path} node ${index}`).toBeLessThanOrEqual(
          lineOf(source, start + src.length) + 2,
        );
        // Meaning: the saved file loads back into the edited document.
        expect(
          meaning(schema.nodeFromJSON(markdownToDoc(text, { ontology }).json)),
          `${path} node ${index}`,
        ).toEqual(meaning(schema.nodeFromJSON(json)));
      }
      expect(edits).toBeGreaterThan(0);
    },
  );
});

describe("line endings, byte order marks and final newlines", () => {
  it("keeps CRLF in unchanged and serialized blocks", () => {
    const source = "# Title\r\n\r\nFirst *one*.\r\n\r\n- a\r\n- b\r\n";
    const loaded = load(source);
    expect(loaded.save(loaded.copy())).toBe(source);
    const json = loaded.copy();
    appendText(json.content![2], " more");
    expect(loaded.save(json)).toBe(
      "# Title\r\n\r\nFirst *one*.\r\n\r\n- a\r\n- b more\r\n",
    );
  });

  it("keeps a byte order mark and front matter verbatim and protected", () => {
    const source = "\uFEFF---\ntitle: x\n---\n\nBody.\n";
    const loaded = load(source);
    expect(loaded.json.content![0]).toMatchObject({
      type: "opaqueBlock",
      attrs: { kind: "frontmatter", protected: true },
    });
    const json = loaded.copy();
    appendText(json.content![1], " Edited.");
    expect(loaded.save(json)).toBe(
      "\uFEFF---\ntitle: x\n---\n\nBody. Edited.\n",
    );
  });

  it("keeps a missing final newline", () => {
    const loaded = load("One.\n\nTwo.");
    const json = loaded.copy();
    appendText(json.content![1], "!");
    expect(loaded.save(json)).toBe("One.\n\nTwo.!");
  });
});

describe("constructs the editor does not model", () => {
  const source = [
    "# Title",
    "",
    "<details><summary>More</summary>",
    "",
    "Inside.",
    "",
    "</details>",
    "",
    "$$",
    "x^2",
    "$$",
    "",
    "Text with [a link][ref], a note[^1], $y$, <kbd>Ctrl</kbd> and ![logo](logo.png).",
    "",
    "> [!NOTE]",
    "> Mind the gap.",
    "",
    ":::tip",
    "A tip.",
    ":::",
    "",
    "```g5-question",
    '{"id":"q1"}',
    "```",
    "",
    "[ref]: https://example.com",
    "",
    "[^1]: The note.",
    "",
  ].join("\n");

  it("keeps each one as opaque source through an edit elsewhere", () => {
    const loaded = load(source);
    const kinds = loaded.json
      .content!.filter((node) => node.type === "opaqueBlock")
      .map((node) => node.attrs!.kind);
    expect(kinds).toEqual([
      "html",
      "html",
      "math",
      "alert",
      "alert",
      "question",
      "definition",
      "footnote",
    ]);
    const json = loaded.copy();
    appendText(json.content![0], " edited");
    expect(loaded.save(json)).toBe(source.replace("# Title", "# Title edited"));
  });

  it("keeps inline constructs verbatim when their paragraph is edited", () => {
    const loaded = load(source);
    const index = loaded.json.content!.findIndex(
      (node) =>
        node.type === "paragraph" && JSON.stringify(node).includes("Text with"),
    );
    const paragraph = loaded.json.content![index];
    const inline = paragraph
      .content!.filter((node) => node.type === "opaqueInline")
      .map((node) => node.attrs!.kind);
    expect(inline).toEqual([
      "reference",
      "reference",
      "math",
      "html",
      "html",
      "image",
    ]);
    const json = loaded.copy();
    appendText(json.content![index], " Done");
    expect(loaded.save(json)).toContain(
      "Text with [a link][ref], a note[^1], $y$, <kbd>Ctrl</kbd> and ![logo](logo.png). Done",
    );
  });

  it("shows an opaque construct as its own Markdown", () => {
    const loaded = load("Math $a+b$ here.\n");
    const node = loaded.json.content![0].content!.find(
      (child) => child.type === "opaqueInline",
    )!;
    expect(opaqueMarkdown(node.attrs!.mdast, true)).toBe("$a+b$");
  });
});

describe("serialized blocks follow the file's style", () => {
  it("uses the file's bullet and emphasis markers for changed blocks", () => {
    const source = "* one\n* two\n\nSome __strong__ and _soft_ text.\n";
    const loaded = load(source);
    const json = loaded.copy();
    json.content!.push({
      type: "bulletList",
      content: [
        {
          type: "listItem",
          content: [
            {
              type: "paragraph",
              content: [
                { type: "text", text: "bold", marks: [{ type: "bold" }] },
                { type: "text", text: " and " },
                { type: "text", text: "italic", marks: [{ type: "italic" }] },
              ],
            },
          ],
        },
      ],
    });
    expect(loaded.save(json)).toBe(
      "* one\n* two\n\nSome __strong__ and _soft_ text.\n\n* __bold__ and _italic_\n",
    );
  });

  it("moves whitespace at a bold edge outside the mark", () => {
    const loaded = load("Plain.\n");
    const json = loaded.copy();
    json.content![0].content = [
      { type: "text", text: "a " },
      { type: "text", text: "bold ", marks: [{ type: "bold" }] },
      { type: "text", text: "word" },
    ];
    expect(loaded.save(json)).toBe("a **bold** word\n");
  });

  it("nests code inside links", () => {
    const loaded = load(
      "See [`markdownDocument.ts`](src/markdownDocument.ts).\n",
    );
    const json = loaded.copy();
    appendText(json.content![0], " Now");
    expect(loaded.save(json)).toBe(
      "See [`markdownDocument.ts`](src/markdownDocument.ts). Now\n",
    );
  });
});

describe("tables, lists and new blocks", () => {
  it("edits a table cell and keeps alignment, escaping pipes", () => {
    const source =
      "Intro.\n\n| Feature | Status |\n| :------ | -----: |\n| Export  | done   |\n\nOutro.\n";
    const loaded = load(source);
    const json = loaded.copy();
    const cell = json.content![1].content![1].content![1].content![0];
    cell.content = [{ type: "text", text: "a|b" }];
    expect(loaded.save(json)).toBe(
      "Intro.\n\n| Feature | Status |\n| :------ | -----: |\n| Export  |   a\\|b |\n\nOutro.\n",
    );
  });

  it("keeps a column's alignment when the column before it is deleted", () => {
    const source =
      "Intro.\n\n| Feature | Status |\n| :------ | -----: |\n| Export  | done   |\n";
    const loaded = load(source);
    const json = loaded.copy();
    for (const row of json.content![1].content!) row.content!.shift();
    const text = loaded.save(json);
    expect(text).toBe("Intro.\n\n| Status |\n| -----: |\n|   done |\n");
    expect(
      markdownToDoc(text).json.content![1].content![0].content![0],
    ).toMatchObject({
      type: "tableHeader",
      attrs: { align: "right" },
    });
  });

  it("writes an inline construct's own bytes when its paragraph is edited", () => {
    const source =
      "See ![logo](<logo.png> 'Title') and [x][ref], then $a\\,b$.\r\n\r\n[ref]: https://e.com\r\n";
    const loaded = load(source);
    const json = loaded.copy();
    appendText(json.content![0], " Edited.");
    expect(loaded.save(json)).toBe(
      "See ![logo](<logo.png> 'Title') and [x][ref], then $a\\,b$. Edited.\r\n\r\n[ref]: https://e.com\r\n",
    );
  });

  it("slices inline source past a byte order mark and front matter", () => {
    const source =
      "\uFEFF---\ntitle: x\n---\n\nSee ![logo](<logo.png> 'T') here.\n";
    const loaded = load(source);
    expect(loaded.json.content![1].content![1]).toMatchObject({
      type: "opaqueInline",
      attrs: { kind: "image", src: "![logo](<logo.png> 'T')" },
    });
    const json = loaded.copy();
    appendText(json.content![1], " Edited.");
    expect(loaded.save(json)).toBe(
      "\uFEFF---\ntitle: x\n---\n\nSee ![logo](<logo.png> 'T') here. Edited.\n",
    );
  });

  it("escapes an inline construct's pipe once it is moved into a table cell", () => {
    const loaded = load("![a|b](img.png)\n\n| A | B |\n| - | - |\n| 1 | 2 |\n");
    const json = loaded.copy();
    const image = json.content![0].content![0];
    expect(image).toMatchObject({
      type: "opaqueInline",
      attrs: { kind: "image" },
    });
    json.content![1].content![1].content![0].content![0].content = [image];
    const text = loaded.save(json);
    expect(text).toContain("| ![a\\|b](img.png) | 2 |");
    const reloaded = markdownToDoc(text).json.content!;
    expect(reloaded[1].content![1].content).toHaveLength(2);
  });

  it("inserts a new block with a blank line and leaves its neighbours' bytes", () => {
    const source = "# T\nFirst.\n\nLast.\n";
    const loaded = load(source);
    const json = loaded.copy();
    json.content!.splice(2, 0, {
      type: "paragraph",
      content: [{ type: "text", text: "Middle." }],
    });
    expect(loaded.save(json)).toBe("# T\nFirst.\n\nMiddle.\n\nLast.\n");
  });

  it("keeps two lists apart when a deletion makes them neighbours", () => {
    const source = "- a\n\nBetween.\n\n- b\n";
    const loaded = load(source);
    const json = loaded.copy();
    json.content!.splice(1, 1);
    const text = loaded.save(json);
    expect(text).toBe("- a\n\n* b\n");
    const lists = markdownToDoc(text).json.content!;
    expect(lists.map((node) => node.type)).toEqual([
      "bulletList",
      "bulletList",
    ]);
  });

  it("writes task items and keeps a loose list loose", () => {
    const source = "- [ ] one\n\n- [x] two\n";
    const loaded = load(source);
    expect(loaded.json.content![0].type).toBe("taskList");
    const json = loaded.copy();
    json.content![0].content![0].attrs!.checked = true;
    expect(loaded.save(json)).toBe("- [x] one\n\n- [x] two\n");
  });

  it("keeps a list it cannot model opaque", () => {
    const loaded = load("- [ ] task\n- plain\n\n1. [ ] ordered task\n");
    expect(loaded.json.content!.map((node) => node.attrs?.kind)).toEqual([
      "list",
      "list",
    ]);
  });
});

describe("soft breaks, indentation and record labels", () => {
  it("keeps a hard-wrapped paragraph's line breaks where the person did not type", () => {
    const source = "First line of text\nsecond line here\nthird line.\n";
    const loaded = load(source);
    expect(
      loaded.json.content![0].content!.filter(
        (node) => node.type === "softBreak",
      ),
    ).toHaveLength(2);
    const json = loaded.copy();
    appendText(json.content![0], " More.");
    expect(loaded.save(json)).toBe(
      "First line of text\nsecond line here\nthird line. More.\n",
    );
  });

  it("keeps the indentation of an indented block it serializes", () => {
    const source = "  - one\n  - two\n\n  After the list.\n";
    const loaded = load(source);
    const json = loaded.copy();
    appendText(json.content![0], " more");
    const text = loaded.save(json);
    expect(text).toBe("  - one\n  - two more\n\n  After the list.\n");
    expect(markdownToDoc(text).json.content!.map((node) => node.type)).toEqual([
      "bulletList",
      "paragraph",
    ]);
  });

  it("splits a record label from its body and protects only the label", () => {
    const source = "[I1. Uploaded intent] (Tags: human)\nKeep this content.\n";
    const loaded = load(source);
    expect(loaded.json.content![0]).toMatchObject({
      type: "opaqueBlock",
      attrs: {
        kind: "structure",
        protected: true,
        src: "[I1. Uploaded intent] (Tags: human)",
      },
    });
    const json = loaded.copy();
    appendText(json.content![1], " More.");
    expect(loaded.save(json)).toBe(
      "[I1. Uploaded intent] (Tags: human)\nKeep this content. More.\n",
    );
  });

  it("protects ontology anchors, headings and metadata and keeps prose editable", () => {
    const source = [
      '<a id="x.a"></a>',
      "[D · x.a]",
      "",
      "Statement prose.",
      "",
      "## Metadata",
      "",
      "### x.a",
      "",
      "- agent",
      "",
    ].join("\n");
    const loaded = load(source, true);
    const content = loaded.json.content!;
    // The anchor and the marker line beneath it form one HTML block.
    expect(
      content.map((node) => [node.type, node.attrs?.protected ?? null]),
    ).toEqual([
      ["opaqueBlock", true],
      ["paragraph", null],
      ["opaqueBlock", true],
      ["opaqueBlock", true],
      ["opaqueBlock", true],
    ]);
    const json = loaded.copy();
    appendText(json.content![1], " Edited.");
    expect(loaded.save(json)).toBe(source.replace("prose.", "prose. Edited."));
    // Removing a protected line changes the protected signature the editor guards.
    const without = clone(json);
    without.content!.splice(0, 1);
    expect(protectedSignature(schema.nodeFromJSON(without))).not.toBe(
      protectedSignature(schema.nodeFromJSON(json)),
    );
  });
});

// In ontology files Rich text hides exactly the record lines Preview hides,
// keeping them in the document; diagnostics and unpaired metadata stay shown.
describe("record lines Rich text hides in ontology files", () => {
  const shown = (source: string) =>
    load(source, true)
      .json.content!.filter((node) => node.type === "opaqueBlock")
      .map((node) => [
        String(node.attrs!.src).split("\n").at(-1),
        node.attrs!.hidden,
        node.attrs!.statement,
      ]);

  it("hides a clean statement's lines and paired metadata, with the Metadata heading", () => {
    const source = [
      '<a id="x.a"></a>',
      "[D · x.a]",
      "",
      "Anchored prose.",
      "",
      "### Headed",
      "[I · x.h]",
      "",
      "Headed prose.",
      "",
      "## Metadata",
      "",
      "### x.a",
      "",
      "- agent",
      "",
      "### x.h",
      "",
      "- agent",
      "",
    ].join("\n");
    expect(shown(source)).toEqual([
      // The anchor and marker form one block; it carries the marker.
      ["[D · x.a]", true, 6],
      ["### Headed", true, 0],
      ["[I · x.h]", true, 3],
      ["## Metadata", true, 0],
      ["### x.a", true, 0],
      ["- agent", true, 0],
      ["### x.h", true, 0],
      ["- agent", true, 0],
    ]);
    // Hidden, not removed: a save without edits is the file itself.
    const loaded = load(source, true);
    expect(docToMarkdown(loaded.doc, loaded.baseline).text).toBe(source);
  });

  it("keeps a diagnostic statement's lines and unpaired metadata visible", () => {
    const source = [
      '<a id="x.a"></a>',
      "[D · x.a]",
      "",
      "Clean prose.",
      "",
      '<a id="x.other"></a>',
      "[D · x.d]",
      "",
      "Mismatched prose.",
      "",
      "## Metadata",
      "",
      "### x.a",
      "",
      "- agent",
      "",
      "### x.d",
      "",
      "- agent",
      "",
      "### x.orphan",
      "",
      "- agent",
      "",
    ].join("\n");
    expect(shown(source)).toEqual([
      ["[D · x.a]", true, 6],
      ["[D · x.d]", false, 0],
      // Not every entry is paired, so the section keeps its heading.
      ["## Metadata", false, 0],
      ["### x.a", true, 0],
      ["- agent", true, 0],
    ]);
    // Undisclosed entries stay shown, as ordinary blocks.
    const headings = load(source, true)
      .json.content!.filter((node) => node.type === "heading")
      .map((node) => node.content?.[0]?.text);
    expect(headings).toEqual(["x.d", "x.orphan"]);
  });

  it("hides nothing outside ontology files", () => {
    const source = '<a id="x.a"></a>\n[D · x.a]\n\nProse.\n';
    expect(
      load(source)
        .json.content!.filter((node) => node.type === "opaqueBlock")
        .every((node) => !node.attrs!.hidden),
    ).toBe(true);
  });

  it.each(ontologies)(
    "hides record lines in %s and still saves it unchanged",
    (path) => {
      const source = read(path);
      const loaded = load(source, true);
      expect(statementLayout(loaded.doc).statements.length).toBeGreaterThan(0);
      expect(docToMarkdown(loaded.doc, loaded.baseline).text).toBe(source);
    },
  );

  it("saves an edited statement body as that block alone", () => {
    const source = [
      "# Title",
      "",
      '<a id="x.a"></a>',
      "[D · x.a]",
      "",
      "First body.",
      "",
      "Second body.",
      "",
      '<a id="x.b"></a>',
      "[I · x.b]",
      "",
      "Other body.",
      "",
      "## Metadata",
      "",
      "### x.a",
      "",
      "- agent",
      "",
      "### x.b",
      "",
      "- agent",
      "",
    ].join("\n");
    const loaded = load(source, true);
    const json = loaded.copy();
    const index = json.content!.findIndex(
      (node) => node.content?.[0]?.text === "Second body.",
    );
    appendText(json.content![index], " Edited.");
    expect(loaded.save(json)).toBe(
      source.replace("Second body.", "Second body. Edited."),
    );
  });
});

describe("statement layout", () => {
  const layoutOf = (source: string) => {
    const { doc } = load(source, true);
    const layout = statementLayout(doc);
    const text = (index: number) =>
      doc.child(index).textContent || String(doc.child(index).attrs.src);
    return {
      hidden: layout.hidden,
      bodies: layout.statements.map((statement) =>
        Array.from({ length: statement.last - statement.first + 1 }, (_, i) =>
          text(statement.first + i),
        ),
      ),
    };
  };

  it("runs from a marker to the next record line or a section heading", () => {
    const { bodies } = layoutOf(
      [
        "# Title",
        "",
        "Intro.",
        "",
        '<a id="x.a"></a>',
        "[D · x.a]",
        "",
        "A one.",
        "",
        "- A list",
        "",
        '<a id="x.b"></a>',
        "[D · x.b]",
        "",
        "B one.",
        "",
        "## Section",
        "",
        "Section prose.",
        "",
        "### Headed",
        "[I · x.h]",
        "",
        "H one.",
        "",
        "#### Inside",
        "",
        "H two.",
        "",
        "### Next",
        "",
        "After.",
        "",
        "## Metadata",
        "",
        "### x.a",
        "",
        "- agent",
        "",
        "### x.b",
        "",
        "- agent",
        "",
        "### x.h",
        "",
        "- agent",
        "",
      ].join("\n"),
    );
    expect(bodies).toEqual([
      ["A one.", "A list"],
      ["B one."],
      // A heading below the identity heading's level stays in the statement.
      ["H one.", "Inside", "H two."],
    ]);
  });

  it("gives an empty body only where a heading ends the statement", () => {
    const { doc } = load(
      [
        "### Headed",
        "[I · x.h]",
        "",
        "#### Inside",
        "",
        "H one.",
        "",
        '<a id="x.a"></a>',
        "[D · x.a]",
        "",
        "## Section",
        "",
        "## Metadata",
        "",
        "### x.h",
        "",
        "- agent",
        "",
        "### x.a",
        "",
        "- agent",
        "",
      ].join("\n"),
      true,
    );
    const [headed, bodiless] = statementLayout(doc).statements;
    // A heading below the identity heading's level is the body itself.
    expect(doc.child(headed.first).textContent).toBe("Inside");
    expect(isEmptyStatement(doc, headed)).toBe(false);
    expect(isEmptyStatement(doc, bodiless)).toBe(true);
  });

  it("hides an empty paragraph after a document's hidden end", () => {
    const loaded = load(
      '<a id="x.a"></a>\n[D · x.a]\n\nBody.\n\n## Metadata\n\n### x.a\n\n- agent\n',
      true,
    );
    const json = loaded.copy();
    json.content!.push({ type: "paragraph" });
    const doc = schema.nodeFromJSON(json);
    const { hidden } = statementLayout(doc);
    expect(hidden).toEqual([true, false, true, true, true, true]);
    expect(docToMarkdown(doc, loaded.baseline).text).toBe(
      docToMarkdown(loaded.doc, loaded.baseline).text,
    );
  });
});

describe("focusing a block by its source offset", () => {
  it("finds the top-level node that began at or before an offset", () => {
    const source = "# T\n\nFirst.\n\nSecond.\n";
    const loaded = load(source);
    const position = topLevelPositionAt(
      loaded.doc,
      loaded.baseline,
      source.indexOf("Second."),
    );
    expect(position).not.toBeNull();
    expect(loaded.doc.nodeAt(position!)?.textContent).toBe("Second.");
  });
});
