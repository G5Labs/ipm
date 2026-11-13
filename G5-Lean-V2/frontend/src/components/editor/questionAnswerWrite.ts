// =============================================================================
// questionAnswerWrite — pure markdown helpers for `g5-question`
// =============================================================================
// Ports the D59 write discipline (htmlDocAnswerBridge.applyAnswerProposal) to
// the markdown question card's block-range locator:
//
// - The fence is located by re-parsing the current local document at apply time with
//   `parseMarkdownToBlocks` and using the parser-recorded `sourceRange` —
//   never cached offsets, never indexOf against reconstructed markdown.
// - Only the fence body and its derived label tags change; all other bytes stay exact.
// - The next payload is produced by `rewrite` from the freshly parsed question,
//   never from the stale payload the card rendered from.
// - The caller applies the minimal edit to its local document and persists it
//   through the explicit Save path with the file's expected content hash.
//
// Governing intent: center-panel D69 "Write policy (direct apply; [D23]
// discipline)"; payload library contract: shared-components D38.
// =============================================================================

import {
  parseMarkdownToBlocks,
  parseRecordMetadataSuffix,
  serializeRecordMetadataTags,
  type BlockTag,
  type ParsedMarkdownBlocks,
  type RecordSpan,
} from "../../lib/utils";
import {
  hasDuplicateOptionLabels,
  isQuestionFenceLanguage,
  parseQuestionPayload,
  questionPayloadByteLength,
  serializeQuestionPayload,
  MAX_QUESTION_PAYLOAD_BYTES,
  type ParsedQuestion,
  type QuestionParseResult,
} from "../../lib/questionBlock";

// ---------------------------------------------------------------------------
// Question fence collection (shared derivation with DocumentPreview's dispatch)
// ---------------------------------------------------------------------------

export interface QuestionFenceInfo {
  /** Index of the code block in the parsed block array. */
  blockIndex: number;
  /** Doc-derived default id: enclosing record label lowercased, else index. */
  fallbackId: string;
  /**
   * Effective question id: the parsed payload id (fallback-defaulted) when
   * the payload parses, else the fallback id so a corrupted-in-place fence
   * remains addressable for conflict reporting.
   */
  id: string;
  parse: QuestionParseResult;
  /** True when an earlier PARSED fence in this document claimed the same id
   * (first wins, D69). Parse failures never claim ids. */
  duplicateId: boolean;
  /** Absolute offsets of the fence body within the parsed markdown. */
  bodyStart: number;
  bodyEnd: number;
  /** Exact fence body bytes (no trailing newline, parser convention). */
  body: string;
  /** False for an unterminated fence — never a write target. */
  closed: boolean;
}

/**
 * Collect every `g5-question` fence in one document parse: fallback ids from
 * enclosing record spans, first-wins duplicate bookkeeping, and exact
 * parser-recorded body offsets. Single owner of this derivation for both the
 * DocumentPreview dispatch and the write path.
 */
export function collectQuestionFences(
  markdown: string,
  blocks: ParsedMarkdownBlocks = parseMarkdownToBlocks(markdown),
): QuestionFenceInfo[] {
  const recordByBlockIndex = new Map<number, RecordSpan>();
  blocks.recordSpans.forEach((record) => {
    for (let index = record.spanStart; index < record.spanEnd; index++) {
      recordByBlockIndex.set(index, record);
    }
  });
  const fences: QuestionFenceInfo[] = [];
  const seenIds = new Set<string>();
  blocks.forEach((block, index) => {
    if (block.type !== "code" || !isQuestionFenceLanguage(block.language)) {
      return;
    }
    const record = recordByBlockIndex.get(index);
    const fallbackId = record ? record.id.toLowerCase() : `question-${index}`;
    const parse = parseQuestionPayload(block.content, { fallbackId });
    const id = parse.ok ? parse.question.id : fallbackId;
    let duplicateId = false;
    if (parse.ok) {
      duplicateId = seenIds.has(id);
      seenIds.add(id);
    }
    const sourceRange = block.sourceRange ?? { start: -1, length: 0 };
    const openingLine =
      (block.indent ?? "") +
      (block.fence ?? "```") +
      (block.openingSuffix ?? "");
    const bodyStart = sourceRange.start + openingLine.length + 1;
    const bodyEnd = bodyStart + block.content.length;
    fences.push({
      blockIndex: index,
      fallbackId,
      id,
      parse,
      duplicateId,
      bodyStart,
      bodyEnd,
      body: block.content,
      closed: block.closed !== false && sourceRange.start >= 0,
    });
  });
  return fences;
}

// ---------------------------------------------------------------------------
// Answer-state comparison (the divergence gate the card acted on)
// ---------------------------------------------------------------------------

function stringArraysEqual(
  a: readonly string[],
  b: readonly string[],
): boolean {
  return a.length === b.length && a.every((value, index) => value === b[index]);
}

/**
 * Structural comparison of `answer.answered_by` (D69).
 *
 * It is a mapping now, so `===` would compare references and report every
 * parse as divergent — turning each answer into a false conflict and firing
 * the post-apply watch immediately. Identity changes ARE divergence: a
 * different person answering is exactly the case the gate must catch.
 */
function answeredByMatches(
  a: ParsedQuestion["answer"]["answered_by"],
  b: ParsedQuestion["answer"]["answered_by"],
): boolean {
  if (a === b) return true;
  if (!a || !b) return false;
  return (
    a.kind === b.kind &&
    a.id === b.id &&
    a.email === b.email &&
    a.avatar_url === b.avatar_url
  );
}

/** Record starts of Q statements whose `g5-question` payload carries an answer
 * recorded by a person. Agent-proposed answers await confirmation (D69). One
 * half of the shared Human rule (g5.documents.ontology.human-statements); the
 * other half is footer provenance in `currentRecordSpans`. */
export function answeredQuestionRecordStarts(
  markdown: string,
  blocks: ParsedMarkdownBlocks = parseMarkdownToBlocks(markdown),
): Set<number> {
  const starts = new Set<number>();
  for (const fence of collectQuestionFences(markdown, blocks)) {
    if (!fence.parse.ok || fence.duplicateId) continue;
    if (fence.parse.question.status !== "answered") continue;
    if (fence.parse.question.answer.answered_by?.kind !== "user") continue;
    const record = blocks.recordSpans.find(
      (span) =>
        fence.blockIndex >= span.spanStart && fence.blockIndex < span.spanEnd,
    );
    if (record?.kind === "Q") starts.add(record.spanStart);
  }
  return starts;
}

/** True when status and every answer field the card observed are unchanged. */
export function answerStateMatches(
  a: ParsedQuestion,
  b: ParsedQuestion,
): boolean {
  return (
    a.status === b.status &&
    stringArraysEqual(a.answer.selected, b.answer.selected) &&
    a.answer.text === b.answer.text &&
    a.answer.rationale === b.answer.rationale &&
    answeredByMatches(a.answer.answered_by, b.answer.answered_by) &&
    a.answer.answered_at === b.answer.answered_at
  );
}

// ---------------------------------------------------------------------------
// Pure markdown rewrite
// ---------------------------------------------------------------------------

/**
 * Rewrite one question's payload in a markdown STRING: locate the fence by
 * question id (first-wins duplicate rule), gate on unchanged answer state,
 * serialize the rewritten payload, and converge the `[Q<N>.]` label tags.
 *
 * This is the single owner of the gate+splice sequence for local editor
 * buffers. Answer and Clear both route through it with a `rewrite` from
 * `lib/questionBlock` (`applyAnswer`, `clearAnswer`).
 */
export function rewriteQuestionInMarkdown(
  markdown: string,
  question: ParsedQuestion,
  rewrite: (current: ParsedQuestion) => ParsedQuestion,
): { ok: true; content: string } | { ok: false; message: string } {
  const fence = collectQuestionFences(markdown).find(
    (candidate) => candidate.id === question.id && !candidate.duplicateId,
  );
  if (!fence || !fence.closed) {
    return {
      ok: false,
      message: "Question not found in the current document.",
    };
  }
  if (!fence.parse.ok) {
    return { ok: false, message: "The question payload no longer parses." };
  }
  if (!answerStateMatches(fence.parse.question, question)) {
    return {
      ok: false,
      message: "This question changed since it was loaded — please retry.",
    };
  }
  const rewritten = rewrite(fence.parse.question);
  if (hasDuplicateOptionLabels(rewritten)) {
    return {
      ok: false,
      message: "Two options share a label; the payload must be repaired first.",
    };
  }
  const body = serializeQuestionPayload(rewritten).replace(/\r?\n$/, "");
  if (questionPayloadByteLength(body) > MAX_QUESTION_PAYLOAD_BYTES) {
    return { ok: false, message: "The answered payload exceeds the size cap." };
  }
  return spliceQuestionPayload(markdown, fence, body, rewritten);
}

/** Explicit raw repair still targets the exact closed fence the card displayed. */
export function repairQuestionInMarkdown(
  markdown: string,
  expected: QuestionFenceInfo,
  body: string,
): { ok: true; content: string } | { ok: false; message: string } {
  const fence = collectQuestionFences(markdown).find(
    (f) => f.bodyStart === expected.bodyStart && f.body === expected.body,
  );
  if (!fence?.closed || fence.duplicateId)
    return { ok: false, message: "The question changed. Review it and retry." };
  if (questionPayloadByteLength(body) > MAX_QUESTION_PAYLOAD_BYTES)
    return { ok: false, message: "The repaired payload exceeds the size cap." };
  const parsed = parseQuestionPayload(body, { fallbackId: fence.fallbackId });
  return spliceQuestionPayload(
    markdown,
    fence,
    body.replace(/\r?\n$/, ""),
    parsed.ok ? parsed.question : undefined,
  );
}

function spliceQuestionPayload(
  markdown: string,
  fence: QuestionFenceInfo,
  body: string,
  question?: ParsedQuestion,
): { ok: true; content: string } {
  // The parser excludes the closing fence's LF but includes a CR when present.
  // Preserve that boundary and the authored newline convention inside the payload.
  if (fence.body === "") body += "\n";
  if (markdown.slice(fence.bodyStart - 2, fence.bodyStart) === "\r\n")
    body = body.replace(/\r?\n/g, "\r\n");
  if (fence.body.endsWith("\r") && !body.endsWith("\r")) body += "\r";
  let next =
    markdown.slice(0, fence.bodyStart) + body + markdown.slice(fence.bodyEnd);
  // The label precedes the fence, so its offsets survive the body splice.
  const labelEdit = question && questionLabelTagEdit(markdown, fence, question);
  if (labelEdit) {
    next =
      next.slice(0, labelEdit.start) +
      labelEdit.replacement +
      next.slice(labelEdit.start + labelEdit.length);
  }
  return { ok: true, content: next };
}

/**
 * Question tags the payload implies for its enclosing `[Q<N>.]` label.
 *
 * `answered` requires a USER provenance: it establishes human authorship
 * (D10), so an agent-proposed answer must not mint it. Severity mirrors the
 * payload verbatim.
 */
export function questionStateTags(question: ParsedQuestion): BlockTag[] {
  const tags: BlockTag[] = [];
  if (
    question.status === "answered" &&
    question.answer.answered_by?.kind === "user"
  ) {
    tags.push({ kind: "answered", syntax: "inline" });
  }
  if (question.severity) {
    tags.push({
      kind: "severity",
      severity: question.severity,
      syntax: "inline",
    });
  }
  return tags;
}

/** A single in-place source replacement. */
export interface QuestionLabelTagEdit {
  start: number;
  length: number;
  replacement: string;
}

/** A `[Q<N>.]`-shaped first line, regardless of whether its suffix parses. */
const QUESTION_LABEL_LINE_RE = /^\[([IDQ])\d+[a-zA-Z]?\./i;

/**
 * Authored `human`/`uncertain` tags recoverable from a suffix region that no
 * longer parses as ONE canonical suffix — after a concurrent-answer merge it
 * can hold several `(Tags: …)` groups side by side. Each group that parses on
 * its own contributes; the union is deduplicated by kind, so the result is
 * the same whichever client's write merged first.
 */
function authoredTagsFromSuffixRegion(region: string): BlockTag[] {
  const byKind = new Map<BlockTag["kind"], BlockTag>();
  for (const group of region.match(/\(Tags:[^)]*\)/g) ?? []) {
    const parsed = parseRecordMetadataSuffix(group, "Q");
    if (!parsed.valid) continue;
    for (const tag of parsed.tags) {
      if (tag.kind === "human" || tag.kind === "uncertainty") {
        byKind.set(tag.kind, tag);
      }
    }
  }
  return [...byKind.values()];
}

/**
 * The edit that brings a question's `[Q<N>.]` label suffix into agreement with
 * its payload (D69). The payload is the WRITER and the label is the INDEX, so
 * a label that disagrees is corrected here rather than trusted.
 *
 * This is also the CONVERGENCE point for concurrent answers: two disconnected
 * clients both applying merges their label splices side by side —
 * `(Tags: …) (Tags: …)` — an invalid suffix that stops the record being
 * projected at all. The next apply must therefore be able to find the label
 * WITHOUT the record projection and rewrite the whole region back to one
 * canonical suffix, or the corruption is permanent: nothing else writes the
 * label. Authored tags are recovered as the union of the groups that parse,
 * so repair is deterministic regardless of merge order.
 *
 * Returns null when nothing needs writing: no label-shaped line precedes the
 * fence, the nearest one is an I/D record (question tags are Q-only), or the
 * label is already in sync. Authored `human`/`uncertain` tags are carried
 * through untouched; this never adds or removes them.
 */
export function questionLabelTagEdit(
  markdown: string,
  fence: QuestionFenceInfo,
  question: ParsedQuestion,
  blocks: ParsedMarkdownBlocks = parseMarkdownToBlocks(markdown),
): QuestionLabelTagEdit | null {
  const record = blocks.recordSpans.find(
    (span) =>
      fence.blockIndex >= span.spanStart && fence.blockIndex < span.spanEnd,
  );
  // Current ontology uses stable markers and footer metadata, not label suffixes.
  if (record && (record.kind !== "Q" || record.current)) return null;

  let labelBlockIndex: number;
  let authored: BlockTag[];
  if (record) {
    labelBlockIndex = record.spanStart;
    authored = (record.metadataTags ?? []).filter(
      (tag) => tag.kind === "human" || tag.kind === "uncertainty",
    );
  } else {
    // Repair path: the suffix is corrupted, so no record is projected and the
    // label has to be located from the raw blocks — the nearest preceding
    // label-shaped line, whatever its suffix looks like.
    // Ownership mirrors the projection's rule exactly: a record extends to the
    // next label or the next SAME/HIGHER-level heading, so the candidate label
    // owns this fence iff every heading strictly between them is DEEPER than
    // the label's own enclosing section. Stopping at any heading refused valid
    // shapes (H2 section, corrupted label, H3 details, fence) and left them
    // permanently unrepaired; ignoring headings rewrote an unrelated earlier
    // record's index across a section boundary. Anything ambiguous declines —
    // repairing nothing keeps the detection stance.
    let found = -1;
    const betweenHeadings: number[] = [];
    for (let index = fence.blockIndex - 1; index >= 0; index--) {
      const block = blocks[index];
      if (block.type === "heading") {
        betweenHeadings.push(block.level);
        continue;
      }
      if (block.type !== "paragraph") continue;
      const firstLine = block.content.split("\n", 1)[0]?.trim() ?? "";
      const match = QUESTION_LABEL_LINE_RE.exec(firstLine);
      if (!match) continue;
      // The fence belongs to the nearest label. A non-Q label owning it means
      // no question tags are written at all.
      if (match[1].toUpperCase() !== "Q") return null;
      // The label's enclosing section: nearest H2-H6 above it. H1 is the
      // document title, not a record-owning section.
      let enclosingLevel: number | null = null;
      for (let above = index - 1; above >= 0; above--) {
        const candidate = blocks[above];
        if (candidate.type === "heading" && candidate.level >= 2) {
          enclosingLevel = candidate.level;
          break;
        }
      }
      const owns = betweenHeadings.every((level) =>
        enclosingLevel === null ? false : level > enclosingLevel,
      );
      if (!owns) return null;
      found = index;
      break;
    }
    if (found === -1) return null;
    labelBlockIndex = found;
    authored = [];
  }

  const labelBlock = blocks[labelBlockIndex];
  const range = labelBlock?.sourceRange;
  if (!range) return null;
  const lineEndRelative = markdown.indexOf("\n", range.start);
  const rawLineEnd =
    lineEndRelative === -1 || lineEndRelative > range.start + range.length
      ? range.start + range.length
      : lineEndRelative;
  // Stop BEFORE a CRLF's `\r`. Including it would make an already-synchronized
  // suffix compare unequal — reporting a write that is not needed — and the
  // splice would then delete the `\r`, leaving the document with mixed line
  // endings that every later diff and offset has to carry.
  const lineEnd =
    rawLineEnd > range.start && markdown[rawLineEnd - 1] === "\r"
      ? rawLineEnd - 1
      : rawLineEnd;
  const line = markdown.slice(range.start, lineEnd);
  const closeIndex = line.lastIndexOf("]");
  if (closeIndex === -1) return null;

  // Everything after the label's `]` on that line is the suffix region. Keep
  // the label text itself byte-exact — only the region after it is rewritten.
  const suffixStart = range.start + closeIndex + 1;
  const existing = markdown.slice(suffixStart, lineEnd);
  if (!record) {
    // Authored tags survive the corruption as the union of the groups that
    // still parse individually.
    authored = authoredTagsFromSuffixRegion(existing);
  }

  const nextSuffix = serializeRecordMetadataTags(
    [...authored, ...questionStateTags(question)],
    "Q",
  );
  if (nextSuffix === null) return null;
  const replacement = nextSuffix ? ` ${nextSuffix}` : "";
  if (existing === replacement) return null;
  return { start: suffixStart, length: lineEnd - suffixStart, replacement };
}
