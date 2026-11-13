import { sha256 } from "./sha256";

export interface CommentAnchorIdentity {
  lineNumber: number;
  lineHash: string;
  contentHash: string;
  anchorContext: string;
}

export interface PersistedCommentAnchor {
  lineNumber: number | null;
  lineHash?: string | null;
  contentHash?: string | null;
  anchorContext?: string | null;
}

/**
 * Where a thread belongs in the displayed text
 * (`g5.documents.comments.reanchor`, `.ambiguity`):
 * - `mapped`: evidence places it on this line;
 * - `ambiguous`: several lines are equally good candidates;
 * - `orphan`: no line carries the evidence any more;
 * - `unanchored`: the comment was stored without a line anchor (legacy
 *   node-level or pre-hash rows).
 */
export type CommentAnchorResolution =
  | { status: "mapped"; lineNumber: number }
  | { status: "ambiguous" }
  | { status: "orphan" }
  | { status: "unanchored" };

/** The line a thread was written on, shown when it cannot be placed. */
export interface OriginalCommentAnchor {
  lineNumber: number | null;
  lineText: string | null;
}

interface AnchorContextV1 {
  version: 1;
  before: string[];
  line: string;
  after: string[];
  /**
   * Whether each part was shortened to fit the size bound. Always written
   * now; a context without them predates the flags.
   */
  beforeTruncated: boolean;
  afterTruncated: boolean;
  lineTruncated: boolean;
}

const MAX_CONTEXT_BYTES = 4096;
/**
 * A context without truncation flags may have been shortened silently: the
 * earlier producer cut 128 UTF-16 units at a time until the JSON fit
 * `MAX_CONTEXT_BYTES`, and one cut removes at most 768 bytes (a unit escaped
 * as `\uXXXX`). Such a context no larger than this was never shortened.
 */
const UNFLAGGED_NEVER_SHORTENED_BYTES = MAX_CONTEXT_BYTES - 128 * 6;
/** Lines recorded on each side of the commented line. */
const CONTEXT_LINES = 2;
/** How far a one-sided context match may move a thread. */
const NEARBY_LINES = 20;

export async function sha256Utf8(value: string): Promise<string> {
  const bytes = new TextEncoder().encode(value);
  // WebCrypto exists only in secure contexts: a page served over plain HTTP
  // on another host than localhost computes the same digest in script.
  const subtle = globalThis.crypto?.subtle;
  const digest = subtle
    ? new Uint8Array(await subtle.digest("SHA-256", bytes))
    : sha256(bytes);
  return Array.from(digest, (byte) => byte.toString(16).padStart(2, "0")).join(
    "",
  );
}

function normalizedLines(content: string): string[] {
  return content.replace(/\r\n/g, "\n").split("\n");
}

function boundedContext(lines: string[], index: number): string {
  const context: AnchorContextV1 = {
    version: 1,
    before: lines.slice(Math.max(0, index - CONTEXT_LINES), index),
    line: lines[index] ?? "",
    after: lines.slice(index + 1, index + 1 + CONTEXT_LINES),
    beforeTruncated: false,
    afterTruncated: false,
    lineTruncated: false,
  };
  const encoder = new TextEncoder();
  let encoded = JSON.stringify(context);
  while (encoder.encode(encoded).byteLength > MAX_CONTEXT_BYTES) {
    const sides = [
      { lines: context.before, truncated: "beforeTruncated" },
      { lines: context.after, truncated: "afterTruncated" },
    ] as const;
    const longest = sides
      .flatMap((side) =>
        side.lines.map((value, valueIndex) => ({
          side,
          valueIndex,
          length: value.length,
        })),
      )
      .sort((left, right) => right.length - left.length)[0];
    // A shortened side is recorded as such: it is no longer evidence.
    if (longest && longest.length > 0) {
      longest.side.lines[longest.valueIndex] = longest.side.lines[
        longest.valueIndex
      ].slice(0, Math.max(0, longest.length - 128));
      context[longest.side.truncated] = true;
    } else if (context.line.length > 0) {
      context.line = context.line.slice(
        0,
        Math.max(0, context.line.length - 128),
      );
      context.lineTruncated = true;
    } else {
      break;
    }
    encoded = JSON.stringify(context);
  }
  return encoded;
}

/**
 * Anchor a comment to `lineNumber` of exactly the text being displayed,
 * unsaved edits included (`g5.documents.comments`). Throws when the line is
 * outside that text.
 */
export async function createCommentAnchor(
  content: string,
  lineNumber: number,
): Promise<CommentAnchorIdentity> {
  const lines = normalizedLines(content);
  const index = lineNumber - 1;
  if (!Number.isInteger(lineNumber) || index < 0 || index >= lines.length) {
    throw new Error("Comment line is outside the displayed file");
  }
  return {
    lineNumber,
    lineHash: await sha256Utf8(lines[index]),
    contentHash: await sha256Utf8(content),
    anchorContext: boundedContext(lines, index),
  };
}

function parseContext(
  value: string | null | undefined,
): AnchorContextV1 | null {
  if (!value) return null;
  try {
    const parsed = JSON.parse(value) as Partial<AnchorContextV1>;
    if (
      parsed.version !== 1 ||
      !Array.isArray(parsed.before) ||
      typeof parsed.line !== "string" ||
      !Array.isArray(parsed.after) ||
      !parsed.before.every((line) => typeof line === "string") ||
      !parsed.after.every((line) => typeof line === "string")
    ) {
      return null;
    }
    const flagged =
      typeof parsed.beforeTruncated === "boolean" &&
      typeof parsed.afterTruncated === "boolean" &&
      typeof parsed.lineTruncated === "boolean";
    // Unflagged and large enough to have been shortened: no part is trusted.
    const maybeShortened =
      !flagged &&
      new TextEncoder().encode(value).byteLength >
        UNFLAGGED_NEVER_SHORTENED_BYTES;
    return {
      version: 1,
      before: parsed.before,
      line: parsed.line,
      after: parsed.after,
      beforeTruncated: flagged
        ? parsed.beforeTruncated === true
        : maybeShortened,
      afterTruncated: flagged ? parsed.afterTruncated === true : maybeShortened,
      lineTruncated: flagged ? parsed.lineTruncated === true : maybeShortened,
    };
  } catch {
    return null;
  }
}

/** The recorded line number and line text of a comment's anchor. */
export function originalCommentAnchor(
  anchor: PersistedCommentAnchor,
): OriginalCommentAnchor {
  return {
    lineNumber: anchor.lineNumber ?? null,
    lineText: parseContext(anchor.anchorContext)?.line ?? null,
  };
}

/**
 * Whether a recorded context side can serve as evidence. An empty side (the
 * edge of the file), one of only blank lines (including the empty line after
 * a final newline) or one shortened to fit the size bound identifies nothing
 * (`g5.documents.comments.reanchor`).
 */
function isEvidence(side: string[], truncated: boolean): boolean {
  return !truncated && side.some((line) => line.trim() !== "");
}

function sameLines(left: string[], right: string[]): boolean {
  return (
    left.length === right.length &&
    left.every((line, index) => line === right[index])
  );
}

/**
 * One displayed text version, indexed once and shared by every comment on
 * it. Line hashes are computed lazily, and only when a line's recorded text
 * cannot find the candidates by itself.
 */
class DisplayedText {
  private splitLines: string[] | null = null;
  private contentHash: Promise<string> | null = null;
  private lineHashes = new Map<number, Promise<string>>();
  private candidatesByAnchor = new Map<string, Promise<number[]>>();

  constructor(readonly content: string) {}

  /** Split on first use: threads without a line anchor never read the text. */
  get lines(): string[] {
    this.splitLines ??= normalizedLines(this.content);
    return this.splitLines;
  }

  hashOfContent(): Promise<string> {
    this.contentHash ??= sha256Utf8(this.content);
    return this.contentHash;
  }

  hashOfLine(index: number): Promise<string> {
    let hash = this.lineHashes.get(index);
    if (!hash) {
      hash = sha256Utf8(this.lines[index]);
      this.lineHashes.set(index, hash);
    }
    return hash;
  }

  /** Indexes of the lines whose hash is `lineHash`. */
  candidates(lineHash: string, lineText: string | null): Promise<number[]> {
    const key = `${lineHash}\u0000${lineText ?? ""}`;
    let found = this.candidatesByAnchor.get(key);
    if (!found) {
      found = (async () => {
        // A recorded line text with the anchored hash finds candidates by
        // plain comparison, without hashing every line.
        if (lineText != null && (await sha256Utf8(lineText)) === lineHash) {
          return this.lines.flatMap((line, index) =>
            line === lineText ? [index] : [],
          );
        }
        const hashes = await Promise.all(
          this.lines.map((_, index) => this.hashOfLine(index)),
        );
        return hashes.flatMap((hash, index) =>
          hash === lineHash ? [index] : [],
        );
      })();
      this.candidatesByAnchor.set(key, found);
    }
    return found;
  }

  /**
   * The lines just before `index` equal the recorded before side, compared
   * at its own length (fewer than two lines near the start of a file).
   */
  beforeMatches(index: number, context: AnchorContextV1): boolean {
    if (!isEvidence(context.before, context.beforeTruncated)) return false;
    const from = index - context.before.length;
    return (
      from >= 0 && sameLines(this.lines.slice(from, index), context.before)
    );
  }

  /** The lines just after `index` equal the recorded after side. */
  afterMatches(index: number, context: AnchorContextV1): boolean {
    if (!isEvidence(context.after, context.afterTruncated)) return false;
    return sameLines(
      this.lines.slice(index + 1, index + 1 + context.after.length),
      context.after,
    );
  }
}

async function resolveOn(
  text: DisplayedText,
  anchor: PersistedCommentAnchor,
): Promise<CommentAnchorResolution> {
  if (!anchor.lineHash || anchor.lineNumber == null) {
    return { status: "unanchored" };
  }
  const recordedIndex = anchor.lineNumber - 1;

  // 1. Verified equal content keeps the recorded line.
  if (
    anchor.contentHash &&
    recordedIndex >= 0 &&
    recordedIndex < text.lines.length &&
    (await text.hashOfContent()) === anchor.contentHash &&
    (await text.hashOfLine(recordedIndex)) === anchor.lineHash
  ) {
    return { status: "mapped", lineNumber: anchor.lineNumber };
  }

  // 2. Hunk mapping needs the anchored version, which the browser does not
  //    hold; the context evidence below is the remaining route.
  const context = parseContext(anchor.anchorContext);
  const candidates = await text.candidates(
    anchor.lineHash,
    context && !context.lineTruncated ? context.line : null,
  );
  if (candidates.length === 0) return { status: "orphan" };
  if (!context) return { status: "ambiguous" };

  // 3. The only line in the file with the recorded context on both sides.
  const bothSides = candidates.filter(
    (index) =>
      text.beforeMatches(index, context) && text.afterMatches(index, context),
  );
  if (bothSides.length === 1) {
    return { status: "mapped", lineNumber: bothSides[0] + 1 };
  }
  // Several lines carry the whole recorded context: nearness does not tell
  // them apart, so none is chosen.
  if (bothSides.length > 1) return { status: "ambiguous" };

  // 4. The only line within 20 lines with the recorded context on one side.
  const nearby = candidates.filter(
    (index) =>
      Math.abs(index - recordedIndex) <= NEARBY_LINES &&
      (text.beforeMatches(index, context) || text.afterMatches(index, context)),
  );
  if (nearby.length === 1) {
    return { status: "mapped", lineNumber: nearby[0] + 1 };
  }
  if (nearby.length > 1) {
    return { status: "ambiguous" };
  }
  return { status: "orphan" };
}

/**
 * Place several comments on one displayed text, indexing it once. Never
 * silently chooses the nearest duplicate (`g5.documents.comments.ambiguity`).
 */
export async function resolveCommentAnchors(
  content: string,
  anchors: readonly PersistedCommentAnchor[],
): Promise<CommentAnchorResolution[]> {
  if (anchors.length === 0) return [];
  const text = new DisplayedText(content);
  return Promise.all(anchors.map((anchor) => resolveOn(text, anchor)));
}

export async function resolveCommentAnchor(
  content: string,
  anchor: PersistedCommentAnchor,
): Promise<CommentAnchorResolution> {
  const [resolution] = await resolveCommentAnchors(content, [anchor]);
  return resolution;
}
