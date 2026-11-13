// Pure helpers for the D59 host-confirmed answer persistence contract
// (software_architecture/client/workspace-page/center-panel). An interactive
// HTML project doc embeds exactly one byte-exact answer block; the host only
// ever rewrites that block's inner text with escaped JSON.

/** Byte-exact opening sentinel. Variant spellings are deliberately refused. */
export const ANSWER_BLOCK_OPEN =
  '<script type="application/json" id="g5-doc-answers">';

/** Byte-exact closing sentinel (never a bare `</script` prefix match). */
export const ANSWER_BLOCK_CLOSE = "</script>";

/** UTF-8 byte cap for the proposed answers JSON string. */
export const MAX_ANSWERS_JSON_BYTES = 131072;

export type AnswerBlockErrorCode =
  "missing-block" | "ambiguous-block" | "too-large" | "bad-request";

export interface AnswerBlockLocation {
  /** Offset just past the opening sentinel. */
  innerStart: number;
  /** Offset of the closing sentinel. */
  innerEnd: number;
}

/**
 * Locate the single answer block. Exactly one occurrence of the opening
 * sentinel is required, closed by the byte-exact closing sentinel. Any
 * occurrence anywhere in the text counts (a decoy inside a comment makes the
 * document ambiguous and is refused — predictability over cleverness).
 */
export function locateAnswerBlock(
  text: string,
): AnswerBlockLocation | "missing-block" | "ambiguous-block" {
  const first = text.indexOf(ANSWER_BLOCK_OPEN);
  if (first === -1) return "missing-block";
  if (
    text.indexOf(ANSWER_BLOCK_OPEN, first + ANSWER_BLOCK_OPEN.length) !== -1
  ) {
    return "ambiguous-block";
  }
  const innerStart = first + ANSWER_BLOCK_OPEN.length;
  const innerEnd = text.indexOf(ANSWER_BLOCK_CLOSE, innerStart);
  if (innerEnd === -1) return "missing-block";
  return { innerStart, innerEnd };
}

/**
 * Escape a raw JSON text so it cannot close the script element or introduce
 * markup. `<`, `>`, `&` and U+2028/U+2029 can only appear inside JSON string
 * literals, where `\uXXXX` escapes are transparent to `JSON.parse`, so the
 * escaped text parses to the identical value.
 */
export function escapeAnswersJson(json: string): string {
  return json
    .replace(/&/g, "\\u0026")
    .replace(/</g, "\\u003c")
    .replace(/>/g, "\\u003e")
    .replace(/\u2028/g, "\\u2028")
    .replace(/\u2029/g, "\\u2029");
}

export function answersJsonByteLength(json: string): number {
  return new TextEncoder().encode(json).length;
}

export type BuildNextDocumentResult =
  | { kind: "write"; next: string; expectedInner: string }
  | { kind: "no-op"; expectedInner: string }
  | { kind: "error"; code: AnswerBlockErrorCode };

/**
 * Validate a proposed answers payload against the displayed document and
 * compute the replacement document. `displayed` must be the raw draft file
 * content (never the srcDoc with injected CSP/theme bytes).
 */
export function buildNextDocument(
  displayed: string,
  answersJson: string,
): BuildNextDocumentResult {
  if (answersJsonByteLength(answersJson) > MAX_ANSWERS_JSON_BYTES) {
    return { kind: "error", code: "too-large" };
  }
  try {
    JSON.parse(answersJson);
  } catch {
    return { kind: "error", code: "bad-request" };
  }
  const location = locateAnswerBlock(displayed);
  if (typeof location === "string") {
    return { kind: "error", code: location };
  }
  const expectedInner = escapeAnswersJson(answersJson);
  const currentInner = displayed.slice(location.innerStart, location.innerEnd);
  if (currentInner === expectedInner) {
    return { kind: "no-op", expectedInner };
  }
  return {
    kind: "write",
    next:
      displayed.slice(0, location.innerStart) +
      expectedInner +
      displayed.slice(location.innerEnd),
    expectedInner,
  };
}

/**
 * True when `text` still holds exactly one answer block whose inner text is
 * `expectedInner`. Used by the post-apply late-conflict observer: a merged
 * remote update that removes, duplicates, or rewrites the block flips the
 * applied result to `conflict`.
 */
export function answerBlockMatches(
  text: string,
  expectedInner: string,
): boolean {
  const location = locateAnswerBlock(text);
  if (typeof location === "string") return false;
  return text.slice(location.innerStart, location.innerEnd) === expectedInner;
}
