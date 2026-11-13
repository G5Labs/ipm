import { describe, it, expect } from "vitest";
import { readFileSync, readdirSync } from "node:fs";
import { resolve } from "node:path";
import { parseMarkdownToBlocks } from "./utils";
import {
  disclosedCurrentMetadataIndices,
  currentOntologyDiagnostics,
  currentRecordSpans,
  ontologyHeadingIds,
  linkPathCandidates,
  metadataDetails,
  ontologyFragmentIndex,
  resolveOntologyLink,
  stateMachineLinks,
} from "./currentOntology";

const fixture =
  "# Document\n\n## Hidden title\n\n[I · example.intent]\n\nOriginal body with [evidence](other.g5.md#evidence).\n\n## Hidden title\n\n[D · example.design]\n\nDesign body.\n\n```md\n[I · fake.example]\n```\n\n## Metadata\n\n### example.intent\n- source: [human](../../source.md)\n\n### example.design\n- status: proposed\n";
describe("current ontology", () => {
  it.each([
    ["- uncertain", true, false],
    ["- agent · uncertain", true, false],
    ["- agent; uncertain", true, false],
    ["- agent, uncertain", true, false],
    ["- human-provided · uncertain", true, true],
    ["- uncertain; human-verified", true, true],
    ["- agent; uncertain, uncertain", true, false],
    ["- Source: agent, uncertain", false, false],
    ["- The outcome is uncertain", false, false],
    ["```md\n- agent; uncertain\n```", false, false],
  ])("recognizes explicit metadata flags: %s", (metadata, uncertain, human) => {
    const records = parseMarkdownToBlocks(
      fixture.replace("- source: [human](../../source.md)", metadata),
      true,
    ).recordSpans;
    expect(records[0].metadataTags).toEqual([
      ...(human ? [{ kind: "human", syntax: "inline" }] : []),
      ...(uncertain ? [{ kind: "uncertainty", syntax: "inline" }] : []),
    ]);
    expect(records[1].metadataTags).toEqual([]);
  });
  it.each([
    ["- human-provided", true],
    ["- human-verified", true],
    ["- agent", false],
    ["- Source: human-provided", false],
    ["```md\n- human-provided\n```", false],
  ])("projects only paired human provenance: %s", (metadata, human) => {
    const text = fixture.replace(
      "- source: [human](../../source.md)",
      metadata,
    );
    const records = parseMarkdownToBlocks(text, true).recordSpans;
    expect(records[0].metadataTags?.some((t) => t.kind === "human")).toBe(
      human,
    );
    expect(records[1].metadataTags).toEqual([]);
  });
  it.each([
    [fixture, true],
    [fixture + "\n### orphan\n- Preserve this unmatched entry.\n", false],
    [
      fixture.replace(
        "## Metadata",
        "## Metadata\n\nAuthored footer introduction.",
      ),
      false,
    ],
    [fixture.replace("### example.intent", "### missing.intent"), false],
    ["## Metadata\n\nOrdinary document content.", false],
  ])("hides only fully disclosed footer headings (%s)", (content, hidden) => {
    const blocks = parseMarkdownToBlocks(content);
    const footer = blocks.findIndex(
      (b) => b.type === "heading" && b.content === "Metadata",
    );
    expect(
      disclosedCurrentMetadataIndices(blocks, blocks.recordSpans).has(footer),
    ).toBe(hidden);
  });
  it("pairs exact IDs, excludes fenced examples and preserves duplicate heading anchors", () => {
    const blocks = parseMarkdownToBlocks(fixture);
    expect(blocks.recordSpans.map((r) => r.id)).toEqual([
      "example.intent",
      "example.design",
    ]);
    expect(blocks.recordSpans.every((r) => !r.current?.diagnostic)).toBe(true);
    expect([...ontologyHeadingIds(blocks).values()]).toContain(
      "hidden-title-2",
    );
  });
  it("resolves authored anchors and rejects ambiguous targets without treating code as anchors", () => {
    const input = fixture.replace(
      "## Hidden title",
      '<a id="authored"></a>\n\n## Hidden title',
    );
    const blocks = parseMarkdownToBlocks(input);
    expect(ontologyFragmentIndex(blocks, "authored")).toBe(
      blocks.recordSpans[0].current?.headingIndex,
    );
    expect(
      ontologyFragmentIndex(
        parseMarkdownToBlocks(input + '\n<a name="authored"></a>'),
        "authored",
      ),
    ).toBe(-1);
    expect(
      ontologyFragmentIndex(
        parseMarkdownToBlocks('<a data-id="not-an-anchor"></a>'),
        "not-an-anchor",
      ),
    ).toBe(-1);
    expect(
      ontologyFragmentIndex(
        parseMarkdownToBlocks('<a class="jump" id="real"></a>'),
        "real",
      ),
    ).toBe(0);
    expect(
      ontologyFragmentIndex(
        parseMarkdownToBlocks('```html\n<a id="example"></a>\n```'),
        "example",
      ),
    ).toBe(-1);
  });
  it("keeps relative navigation inside its repository", () => {
    expect(
      resolveOntologyLink(".g5/ontology/a.g5.md", "../../src/main.rs#L12"),
    ).toEqual({ path: "src/main.rs", fragment: "L12" });
    expect(() =>
      resolveOntologyLink(".g5/ontology/a.g5.md", "../../../secret"),
    ).toThrow();
    expect(
      resolveOntologyLink(".g5/ontology/a.g5.md", "https://example.com"),
    ).toBeNull();
  });
  it("recognizes every authored marker in the actual ontology corpus", () => {
    const root = resolve(process.cwd(), "../.g5/ontology");
    const visit = (dir: string): string[] =>
      readdirSync(dir, { withFileTypes: true }).flatMap((e) =>
        e.isDirectory()
          ? visit(resolve(dir, e.name))
          : e.name.endsWith(".md")
            ? [resolve(dir, e.name)]
            : [],
      );
    for (const path of visit(root)) {
      const content = readFileSync(path, "utf8");
      const records = currentRecordSpans(parseMarkdownToBlocks(content)) ?? [];
      const authored = [
        ...content.matchAll(/^\[([IDQ]) · ([A-Za-z0-9][A-Za-z0-9._-]*)\]$/gm),
      ].map((m) => m[2]);
      expect(
        records.map((r) => r.id),
        path,
      ).toEqual(authored);
      expect(
        records.filter((r) => r.current?.diagnostic),
        path,
      ).toEqual([]);
      const blocks = parseMarkdownToBlocks(content);
      for (const record of records) {
        expect(ontologyFragmentIndex(blocks, record.id), path + record.id).toBe(
          record.spanStart,
        );
      }
    }
  });
});

const parity = JSON.parse(
  readFileSync(
    resolve(process.cwd(), "../tests/fixtures/current-ontology.json"),
    "utf8",
  ),
) as {
  name: string;
  source: string;
  ids: string[];
  diagnostics: number;
  nested: number;
  warnings: number;
  aliases?: Record<string, string[]>;
}[];
it.each(parity)("shared parser fixture: $name", (f) => {
  const records = currentRecordSpans(parseMarkdownToBlocks(f.source)) ?? [];
  expect(records.map((r) => r.id)).toEqual(f.ids);
  expect(records.filter((r) => r.current?.diagnostic).length).toBe(
    f.diagnostics,
  );
  expect(records.filter((r) => r.current?.nested).length).toBe(f.nested);
  expect(
    Object.fromEntries(
      records
        .filter((r) => r.current?.aliases.length)
        .map((r) => [r.id, r.current!.aliases]),
    ),
  ).toEqual(f.aliases ?? {});
  expect(
    records.filter((r) => r.current?.diagnostic).length +
      currentOntologyDiagnostics(parseMarkdownToBlocks(f.source)).length,
  ).toBe(f.warnings);
});

describe("alias anchors", () => {
  const source =
    '# Doc\n\n<a id="a"></a>\n[I · a]\n\nFirst.\n\n<a id="b-older"></a>\n<a id="b-old"></a>\n<a id="b"></a>\n[D · b]\n\nSecond.\n\n## Metadata\n\n### a\n\n- agent\n\n### b\n\n- agent\n';
  const blocks = parseMarkdownToBlocks(source, true);
  const [a, b] = currentRecordSpans(blocks) ?? [];

  it("belong to the following statement, not the previous one", () => {
    expect(b.current?.aliases).toEqual(["b-older", "b-old"]);
    expect(b.current?.diagnostic).toBeUndefined();
    expect(b.current?.sourceStart).toBe(b.current?.aliasIndices[0]);
    expect(a.spanEnd).toBe(b.current?.aliasIndices[0]);
    expect(a.current?.aliases).toEqual([]);
  });

  it("resolve a former ID to the statement", () => {
    expect(ontologyFragmentIndex(blocks, "b-old")).toBe(b.spanStart);
    expect(ontologyFragmentIndex(blocks, "b-older")).toBe(b.spanStart);
    expect(ontologyFragmentIndex(blocks, "b")).toBe(b.spanStart);
  });

  it("follow the record parser's strict rule, never a stack ending at a heading", () => {
    const loose = parseMarkdownToBlocks(
      '# Doc\n\n<a id="nav"></a>\n\n<a id="x"></a>\n\n## Heading\n\n<a id="old" class="c"></a>\n<a id="b"></a>\n[I · b]\n\nBody.\n\n## Metadata\n\n### b\n\n- agent\n',
      true,
    );
    const heading = loose.findIndex(
      (block) => block.type === "heading" && block.level === 2,
    );
    const own = ontologyFragmentIndex(loose, "nav");
    expect(own).not.toBe(heading);
    expect(ontologyFragmentIndex(loose, "x")).toBe(heading);
    const statement = currentRecordSpans(loose)![0];
    expect(ontologyFragmentIndex(loose, "old")).not.toBe(statement.spanStart);
  });

  it("are refused when they duplicate another ID", () => {
    const duplicate = parseMarkdownToBlocks(
      source.replace('id="b-older"', 'id="a"'),
      true,
    );
    const records = currentRecordSpans(duplicate) ?? [];
    expect(records.map((r) => r.current?.diagnostic)).toEqual([
      "Mismatched or ambiguous statement anchor",
      "Mismatched or ambiguous statement anchor",
    ]);
  });
});

describe("ontology link paths", () => {
  const from = ".g5/ontology/features/receiving.g5.md";

  it("try the authored path, then the repository root and `.g5`, resolving `..`", () => {
    expect(
      linkPathCandidates(from, "../../TLA_verification/models/x/manifest.json"),
    ).toEqual([
      ".g5/TLA_verification/models/x/manifest.json",
      "TLA_verification/models/x/manifest.json",
    ]);
    expect(linkPathCandidates(from, "../../../../docs/a/../b.md")).toEqual([
      "docs/b.md",
      ".g5/docs/b.md",
    ]);
    // A fallback that climbs out of the repository is dropped.
    expect(linkPathCandidates(from, "x/../../../y.md")).toEqual([".g5/y.md"]);
  });

  it("keep the authored link's own errors", () => {
    expect(() => linkPathCandidates(from, "../a.md?raw=1")).toThrow(
      /query parameters/,
    );
    expect(() => linkPathCandidates(from, "..\\a.md")).toThrow(
      /Invalid repository link/,
    );
  });

  it("ignore state machine links in code examples", () => {
    const metadata =
      "- agent\n\n```markdown\n[x](../../TLA_verification/models/fenced/manifest.json)\n```\n\nInline `[y](../../TLA_verification/models/inline/manifest.json)`.\n\nState machines: [Real](../../TLA_verification/models/real/manifest.json)\n";
    expect(stateMachineLinks(metadata, from).map((l) => l.concern)).toEqual([
      "real",
    ]);
  });
});

describe("metadata details", () => {
  it("keep sources and evidence, without bare tag bullets or structure links", () => {
    expect(
      metadataDetails(
        "- agent.\n- test-coverage\n- `user-facing-behavior`, security\n\nTests: [T](t.cs) cover reuse.\n\n- See [the guide](g.md).\n\nExpands: [p](#p)\n- Children: [c](#c)\n",
      ),
    ).toBe("Tests: [T](t.cs) cover reuse.\n\n- See [the guide](g.md).");
  });
});
