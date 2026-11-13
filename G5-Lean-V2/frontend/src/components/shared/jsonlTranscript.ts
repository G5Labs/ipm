// =============================================================================
// jsonlTranscript — pure parsing/detection helpers for `.jsonl` project-doc
// rich previews (center-panel spec D61). Component rendering lives in
// JsonlTranscriptView.tsx; this file stays component-free so it can be shared
// and unit-tested directly (same split as sandboxedHtml.ts / htmlDocAnswerBlock.ts).
// =============================================================================

/** One physical JSONL line: parsed JSON on success, the parse error otherwise. */
export interface JsonlLineRecord {
  /** 1-based line number in the source document. */
  lineNumber: number;
  raw: string;
  json?: unknown;
  parseError?: string;
}

/**
 * Default parse bound (D61 resilience): lines beyond this are counted but not
 * JSON-parsed, keeping very large transcripts off the main thread. The view
 * surfaces an explicit truncation notice when the bound is hit.
 */
export const MAX_PARSED_LINES = 5_000;

export interface JsonlParseResult {
  records: JsonlLineRecord[];
  /** Total non-empty lines in the document, parsed or not. */
  totalNonEmptyLines: number;
  /** True when non-empty lines beyond `maxRecords` were left unparsed. */
  truncated: boolean;
}

export type JsonObject = Record<string, unknown>;

export function asObject(value: unknown): JsonObject | null {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? (value as JsonObject)
    : null;
}

export function asString(value: unknown): string | null {
  return typeof value === "string" ? value : null;
}

export function asArray(value: unknown): unknown[] | null {
  return Array.isArray(value) ? value : null;
}

/**
 * Scan a JSONL document into per-line records; blank lines are skipped.
 * Parsing stops after `maxRecords` records (remaining non-empty lines are
 * only counted), so a pathological document cannot stall the UI thread.
 *
 * The scan walks the string by character index and allocates raw line text
 * only for records inside the bound — it never materializes a full line
 * array, so a newline-dense document cannot exhaust memory before the bound
 * applies (PR #1642 review).
 */
export function parseJsonlDocument(
  content: string,
  maxRecords: number = MAX_PARSED_LINES,
): JsonlParseResult {
  const records: JsonlLineRecord[] = [];
  let totalNonEmptyLines = 0;
  const length = content.length;
  let start = 0;
  let lineNumber = 0;
  while (start < length) {
    const newlineAt = content.indexOf("\n", start);
    const lineEnd = newlineAt === -1 ? length : newlineAt;
    lineNumber += 1;
    // Drop one trailing \r (CRLF documents) without allocating.
    let sliceEnd = lineEnd;
    if (sliceEnd > start && content.charCodeAt(sliceEnd - 1) === 13) {
      sliceEnd -= 1;
    }
    // Blank-line check by char code — no slice/trim allocation per line.
    let hasContent = false;
    for (let i = start; i < sliceEnd; i += 1) {
      const code = content.charCodeAt(i);
      if (code !== 32 /* space */ && code !== 9 /* tab */) {
        hasContent = true;
        break;
      }
    }
    if (hasContent) {
      totalNonEmptyLines += 1;
      if (records.length < maxRecords) {
        const raw = content.slice(start, sliceEnd);
        try {
          records.push({ lineNumber, raw, json: JSON.parse(raw) });
        } catch (error) {
          records.push({
            lineNumber,
            raw,
            parseError: error instanceof Error ? error.message : "Invalid JSON",
          });
        }
      }
    }
    if (newlineAt === -1) break;
    start = newlineAt + 1;
  }
  return {
    records,
    totalNonEmptyLines,
    truncated: totalNonEmptyLines > records.length,
  };
}

/**
 * Claude-format detection (D61): the transcript layout activates when any
 * parsed record is an object with `type` "user"/"assistant" carrying a
 * `message` object.
 */
export function isClaudeTranscript(records: JsonlLineRecord[]): boolean {
  return records.some((record) => {
    const json = asObject(record.json);
    if (!json) return false;
    return (
      (json.type === "user" || json.type === "assistant") &&
      asObject(json.message) !== null
    );
  });
}

export function formatCharCount(chars: number): string {
  if (chars >= 1024 * 1024)
    return `${(chars / (1024 * 1024)).toFixed(1)} M chars`;
  if (chars >= 1024) return `${(chars / 1024).toFixed(1)} k chars`;
  return `${chars} chars`;
}

export function prettyJson(value: unknown): string {
  try {
    return JSON.stringify(value, null, 2) ?? String(value);
  } catch {
    return String(value);
  }
}
