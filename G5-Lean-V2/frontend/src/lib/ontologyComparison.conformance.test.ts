// One statement rule, two implementations (g5.documents.ontology.comparison-owner).
// The server's `g5_semantic::ontology_comparison` runs the same corpus.
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { answeredQuestionRecordStarts } from "../components/editor/questionAnswerWrite";
import {
  compareOntologyStatements,
  ontologyStatements,
  StatementComparisonRefusal,
} from "./ontologyComparison";
import { parseMarkdownToBlocks } from "./utils";

const CORPUS = resolve(
  __dirname,
  "../../../tests/fixtures/ontology-comparison",
);

interface VersionExpectation {
  statements?: {
    id: string;
    kind: string;
    human: boolean;
    aliases?: string[];
  }[];
  error?: string;
}
interface CaseExpectation {
  before: VersionExpectation;
  after: VersionExpectation;
  changes?: { id: string; status: string }[];
  error?: string;
}

const read = (dir: string, name: string) =>
  existsSync(join(dir, name)) ? readFileSync(join(dir, name), "utf8") : null;

function version(text: string | null): VersionExpectation {
  if (text === null) return { statements: [] };
  try {
    const normalized = text.replace(/\r\n?/g, "\n");
    const statements = ontologyStatements(normalized);
    const blocks = parseMarkdownToBlocks(normalized, true);
    const answered = answeredQuestionRecordStarts(normalized, blocks);
    return {
      statements: statements.map(({ record }) => ({
        id: record.id,
        kind: record.kind,
        human:
          (record.metadataTags ?? []).some((tag) => tag.kind === "human") ||
          answered.has(record.spanStart),
        // Only alias cases carry the field, so the others stay unchanged.
        ...(record.current?.aliases.length
          ? { aliases: record.current.aliases }
          : {}),
      })),
    };
  } catch (error) {
    if (error instanceof StatementComparisonRefusal)
      return { error: error.diagnostic };
    throw error;
  }
}

const cases = readdirSync(CORPUS, { withFileTypes: true })
  .filter((entry) => entry.isDirectory())
  .map((entry) => entry.name)
  .sort();

describe("ontology statement comparison conformance corpus", () => {
  it("has cases", () => expect(cases.length).toBeGreaterThan(10));

  it.each(cases)("%s", (name) => {
    const dir = join(CORPUS, name);
    const expected = JSON.parse(
      readFileSync(join(dir, "expected.json"), "utf8"),
    ) as CaseExpectation;
    const before = read(dir, "before.g5.md");
    const after = read(dir, "after.g5.md");
    expect(version(before)).toEqual(expected.before);
    expect(version(after)).toEqual(expected.after);

    let changes: { id: string; status: string }[] | undefined;
    let error: string | undefined;
    try {
      const result = compareOntologyStatements(after ?? "", before);
      const deleted = [
        ...result.deletedAtEnd,
        ...[...result.deletedBefore.values()].flat(),
        ...[...result.deletedAfter.values()].flat(),
      ];
      changes = [
        ...[...result.changes].map(([id, status]) => ({ id, status })),
        ...deleted.map((statement) => ({
          id: statement.record.id,
          status: "Deleted",
        })),
      ].sort((a, b) => a.id.localeCompare(b.id));
    } catch (cause) {
      if (!(cause instanceof StatementComparisonRefusal)) throw cause;
      error = cause.diagnostic;
    }
    if (expected.error !== undefined) expect(error).toBe(expected.error);
    else expect(changes).toEqual(expected.changes);
  });
});
