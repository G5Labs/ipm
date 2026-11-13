import { currentOntologyDiagnostics, CURRENT_MARKER } from "./currentOntology";
import { parseMarkdownToBlocks, type RecordSpan } from "./utils";

export const MAX_COMPARISON_BYTES = 10 * 1024 * 1024;
export const MAX_COMPARISON_STATEMENTS = 5_000;
export interface ComparisonStatement {
  record: RecordSpan;
  body: string;
  metadata: string;
}
export interface StatementComparison {
  current: ComparisonStatement[];
  changes: Map<string, "Added" | "Modified">;
  before: Map<string, ComparisonStatement>;
  deletedBefore: Map<string, ComparisonStatement[]>;
  deletedAfter: Map<string, ComparisonStatement[]>;
  deletedAtEnd: ComparisonStatement[];
  count: number;
}

// Bound declarations before the parser's record association work. Fenced examples
// are not declarations. The parser remains the authority for structure/identity.
function declarationCount(text: string) {
  let fence = "",
    count = 0;
  for (const line of text.split("\n")) {
    const marker = line.match(/^ {0,3}(`{3,}|~{3,})/);
    if (marker) {
      if (!fence) fence = marker[1];
      else if (marker[1][0] === fence[0] && marker[1].length >= fence.length)
        fence = "";
    } else if (!fence && CURRENT_MARKER.test(line.trim())) count++;
    if (count > MAX_COMPARISON_STATEMENTS) break;
  }
  return count;
}
/** A version the statement rule refuses; `diagnostic` is the first reason. */
export class StatementComparisonRefusal extends Error {
  constructor(readonly diagnostic: string) {
    super(
      `Statement comparison unavailable: ${diagnostic}. Repair in Files or with an agent.`,
    );
  }
}

/** One version's statements under the shared rule (normalized line endings).
 * The server's `g5_semantic::ontology_comparison` must agree on every case of
 * `tests/fixtures/ontology-comparison` (g5.documents.ontology.comparison-owner). */
export function ontologyStatements(text: string): ComparisonStatement[] {
  return statements(text.replace(/\r\n?/g, "\n"));
}

function statements(text: string): ComparisonStatement[] {
  const blocks = parseMarkdownToBlocks(text, true);
  const diagnostics = currentOntologyDiagnostics(blocks);
  for (const record of blocks.recordSpans) {
    if (record.current?.diagnostic) diagnostics.push(record.current.diagnostic);
    if (record.current?.nested)
      diagnostics.push("Nested statement boundaries are ambiguous");
  }
  if (diagnostics.length) throw new StatementComparisonRefusal(diagnostics[0]);
  return blocks.recordSpans.map((record) => {
    const current = record.current!;
    const marker = blocks[record.spanStart].sourceRange;
    const start = marker ? marker.start + marker.length : 0;
    const end = blocks[record.spanEnd]?.sourceRange?.start ?? text.length;
    return {
      record,
      body: text.slice(start, end),
      metadata: text.slice(
        blocks[current.metadataStart + 1]?.sourceRange?.start ?? text.length,
        blocks[current.metadataEnd]?.sourceRange?.start ?? text.length,
      ),
    };
  });
}

export function compareOntologyStatements(
  current: string,
  baseline: string | null,
): StatementComparison {
  if (
    new TextEncoder().encode(current).length +
      new TextEncoder().encode(baseline ?? "").length >
    MAX_COMPARISON_BYTES
  )
    throw new Error(
      "Statement comparison unavailable: combined input exceeds 10 MiB.",
    );
  current = current.replace(/\r\n?/g, "\n");
  baseline = (baseline ?? "").replace(/\r\n?/g, "\n");
  if (
    declarationCount(current) + declarationCount(baseline) >
    MAX_COMPARISON_STATEMENTS
  )
    throw new Error(
      "Statement comparison unavailable: more than 5,000 statements.",
    );
  const now = statements(current),
    old = statements(baseline);
  if (now.length + old.length > MAX_COMPARISON_STATEMENTS)
    throw new Error(
      "Statement comparison unavailable: more than 5,000 statements.",
    );
  const currentIds = new Set(now.map((s) => s.record.id));
  const previous = new Map(old.map((s) => [s.record.id, s]));
  const result: StatementComparison = {
    current: now,
    changes: new Map(),
    before: new Map(),
    deletedBefore: new Map(),
    deletedAfter: new Map(),
    deletedAtEnd: [],
    count: 0,
  };
  for (const statement of now) {
    const prior = previous.get(statement.record.id);
    if (!prior) result.changes.set(statement.record.id, "Added");
    else if (prior.body !== statement.body) {
      result.changes.set(statement.record.id, "Modified");
      result.before.set(statement.record.id, prior);
    }
  }
  // Compute adjacent surviving siblings once; don't repeatedly scan a large file.
  const next: (string | undefined)[] = Array(old.length);
  let following: string | undefined;
  for (let i = old.length - 1; i >= 0; i--) {
    next[i] = following;
    if (currentIds.has(old[i].record.id)) following = old[i].record.id;
  }
  let preceding: string | undefined,
    deleted = 0;
  old.forEach((statement, i) => {
    if (currentIds.has(statement.record.id)) {
      preceding = statement.record.id;
      return;
    }
    deleted++;
    const key = next[i] ?? preceding;
    if (!key) result.deletedAtEnd.push(statement);
    else {
      const map = next[i] ? result.deletedBefore : result.deletedAfter;
      const adjacent = map.get(key) ?? [];
      adjacent.push(statement);
      map.set(key, adjacent);
    }
  });
  result.count = result.changes.size + deleted;
  return result;
}
