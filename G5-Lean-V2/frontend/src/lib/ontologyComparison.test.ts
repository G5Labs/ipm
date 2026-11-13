import { describe, expect, it } from "vitest";
import {
  compareOntologyStatements,
  MAX_COMPARISON_BYTES,
} from "./ontologyComparison";
const doc = (rows: [string, string, string?][], metadata = "human-provided") =>
  "# Ontology\n\n" +
  rows
    .map(
      ([id, body, kind = "I"]) =>
        `<a id="${id}"></a>\n[${kind} · ${id}]\n\n${body}\n\n`,
    )
    .join("") +
  (rows.length
    ? "## Metadata\n\n" +
      rows.map(([id]) => `### ${id}\n\n- ${metadata}\n\n`).join("")
    : "");
describe("statement comparison", () => {
  it.each([0, 1, 2])(
    "deleting statement %i preserves surviving bodies byte-for-byte",
    (index) => {
      const rows: [string, string][] = [
        [
          "a",
          "First paragraph.\n\nSecond paragraph with a hard break.  \nNext line.",
        ],
        ["b", "- Second statement"],
        ["c", "Third statement"],
      ];
      const before = doc(rows);
      const after = doc(rows.filter((_, i) => i !== index));
      const diff = compareOntologyStatements(after, before);
      expect([...diff.changes]).toEqual([]);
      expect(diff.count).toBe(1);
      expect(diff.current.map((s) => s.body)).toEqual(
        compareOntologyStatements(before, null)
          .current.filter((_, i) => i !== index)
          .map((s) => s.body),
      );
    },
  );
  it("pairs body modifications, additions and deletions without source mutation", () => {
    const before = doc([
      ["a", "Original"],
      ["b", "Removed"],
      ["c", "Same"],
    ]);
    const after = doc([
      ["a", "Edited"],
      ["c", "Same"],
      ["d", "New"],
    ]);
    const diff = compareOntologyStatements(after, before);
    expect([...diff.changes]).toEqual([
      ["a", "Modified"],
      ["d", "Added"],
    ]);
    expect(diff.before.get("a")?.body).toContain("Original");
    expect(diff.deletedBefore.get("c")?.map((s) => s.record.id)).toEqual(["b"]);
    expect(diff.count).toBe(3);
    expect(after).not.toContain("Original");
  });
  it("ignores Type, metadata, headings and prose changes", () => {
    const before = doc([["a", "Same"]]);
    const after = doc([["a", "Same", "D"]], "new provenance").replace(
      "# Ontology",
      "# New heading\n\nNew prose",
    );
    expect(compareOntologyStatements(after, before).count).toBe(0);
  });
  it("retains version-specific metadata when bodies differ", () => {
    const result = compareOntologyStatements(
      doc([["a", "new"]], "new source"),
      doc([["a", "old"]], "old source"),
    );
    expect(result.before.get("a")?.metadata).toContain("old source");
    expect(result.before.get("a")?.metadata).not.toContain("new source");
  });
  it("normalizes line endings only and detects authored formatting", () => {
    const text = doc([["a", "word"]]);
    expect(
      compareOntologyStatements(text.replace(/\n/g, "\r\n"), text).count,
    ).toBe(0);
    expect(
      compareOntologyStatements(doc([["a", "**word**"]]), text).count,
    ).toBe(1);
  });
  it("does not mark reorders; handles replacements and consecutive trailing deletions", () => {
    const before = doc([
      ["a", "A"],
      ["b", "B"],
      ["c", "C"],
    ]);
    expect(
      compareOntologyStatements(
        doc([
          ["c", "C"],
          ["a", "A"],
          ["b", "B"],
        ]),
        before,
      ).count,
    ).toBe(0);
    const result = compareOntologyStatements(
      doc([
        ["a", "A"],
        ["new", "B"],
      ]),
      before,
    );
    expect(result.deletedAfter.get("a")?.map((s) => s.record.id)).toEqual([
      "b",
      "c",
    ]);
    expect(result.changes.get("new")).toBe("Added");
  });
  it("supports absent baseline, no statements and deletion of all statements", () => {
    expect(
      compareOntologyStatements(doc([["a", "A"]]), null).changes.get("a"),
    ).toBe("Added");
    expect(compareOntologyStatements("# Empty", "# Empty").count).toBe(0);
    expect(
      compareOntologyStatements("# Empty", doc([["a", "A"]])).deletedAtEnd[0]
        .record.id,
    ).toBe("a");
  });
  it.each([
    doc([
      ["a", "A"],
      ["a", "Duplicate"],
    ]),
    doc([["a", "A"]]).replace('id="a"', 'id="other"'),
    doc([["a", "A"]]).replace("### a", "### orphan"),
    doc([["a", "A"]]).replace("[I · a]", "[I · ]"),
  ])("refuses ambiguous or malformed source", (text) => {
    expect(() => compareOntologyStatements(text, null)).toThrow(/unavailable/);
  });
  it("bounds bytes at the exact boundary and refuses excessive statements", () => {
    expect(
      compareOntologyStatements("x".repeat(MAX_COMPARISON_BYTES), null).count,
    ).toBe(0);
    expect(() =>
      compareOntologyStatements("x".repeat(MAX_COMPARISON_BYTES + 1), null),
    ).toThrow(/10 MiB/);
    expect(() =>
      compareOntologyStatements("[I · a]\n".repeat(5001), null),
    ).toThrow(/5,000/);
  });
  it("accepts exactly 5,000 statements across both versions", () => {
    const text = doc(
      Array.from({ length: 2500 }, (_, i) => [`s${i}`, `Body ${i}`]),
    );
    expect(compareOntologyStatements(text, text).count).toBe(0);
  });
  it("does not interpret declarations in fences", () => {
    const body = "```md\n[I · example]\n```";
    expect(
      compareOntologyStatements(doc([["a", body]]), doc([["a", body]])).count,
    ).toBe(0);
  });
});
