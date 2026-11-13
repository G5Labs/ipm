import { describe, expect, it } from "vitest";
import {
  ANSWER_BLOCK_CLOSE,
  ANSWER_BLOCK_OPEN,
  answerBlockMatches,
  answersJsonByteLength,
  buildNextDocument,
  escapeAnswersJson,
  locateAnswerBlock,
  MAX_ANSWERS_JSON_BYTES,
} from "./htmlDocAnswerBlock";

const wrap = (inner: string, before = "", after = "") =>
  `<!doctype html><html><body>${before}${ANSWER_BLOCK_OPEN}${inner}${ANSWER_BLOCK_CLOSE}${after}</body></html>`;

describe("locateAnswerBlock", () => {
  it("locates the inner range of a single block", () => {
    const doc = wrap("{}");
    const location = locateAnswerBlock(doc);
    expect(location).not.toBeTypeOf("string");
    if (typeof location === "string") throw new Error("unreachable");
    expect(doc.slice(location.innerStart, location.innerEnd)).toBe("{}");
  });

  it("refuses documents without the block", () => {
    expect(locateAnswerBlock("<html><body></body></html>")).toBe(
      "missing-block",
    );
  });

  it("refuses an unclosed block", () => {
    const doc = `<html>${ANSWER_BLOCK_OPEN}{}`;
    expect(locateAnswerBlock(doc)).toBe("missing-block");
  });

  it("refuses duplicate sentinels, including decoys inside comments", () => {
    const decoy = `<!-- ${ANSWER_BLOCK_OPEN}${ANSWER_BLOCK_CLOSE} -->`;
    expect(locateAnswerBlock(wrap("{}", decoy))).toBe("ambiguous-block");
  });

  it("does not recognize variant sentinel spellings", () => {
    const variants = [
      '<script id="g5-doc-answers" type="application/json">{}</script>',
      '<script type="application/json"  id="g5-doc-answers">{}</script>',
      '<SCRIPT type="application/json" id="g5-doc-answers">{}</SCRIPT>',
    ];
    for (const variant of variants) {
      expect(locateAnswerBlock(`<html>${variant}</html>`)).toBe(
        "missing-block",
      );
    }
  });

  it("terminates the inner range only at the exact closing sentinel", () => {
    // `</scriptx>` is text per HTML raw-text rules, not an end tag.
    const doc = wrap('{"a":"</scriptx>"}');
    const location = locateAnswerBlock(doc);
    if (typeof location === "string") throw new Error("expected location");
    expect(doc.slice(location.innerStart, location.innerEnd)).toBe(
      '{"a":"</scriptx>"}',
    );
  });

  it("handles CRLF documents", () => {
    const doc = `<!doctype html>\r\n<html>\r\n${ANSWER_BLOCK_OPEN}{}${ANSWER_BLOCK_CLOSE}\r\n</html>`;
    const location = locateAnswerBlock(doc);
    if (typeof location === "string") throw new Error("expected location");
    expect(doc.slice(location.innerStart, location.innerEnd)).toBe("{}");
  });
});

describe("escapeAnswersJson", () => {
  it("escapes markup-capable characters transparently for JSON.parse", () => {
    const value = {
      html: "</script><b>&amp;</b>",
      sep: "line\u2028para\u2029end",
    };
    const raw = JSON.stringify(value);
    const escaped = escapeAnswersJson(raw);
    expect(escaped).not.toMatch(/[<>&\u2028\u2029]/);
    expect(JSON.parse(escaped)).toEqual(value);
  });

  it("cannot produce a closing sentinel", () => {
    const raw = JSON.stringify({ x: ANSWER_BLOCK_CLOSE + ANSWER_BLOCK_OPEN });
    expect(escapeAnswersJson(raw)).not.toContain("</script");
    expect(escapeAnswersJson(raw)).not.toContain("<script");
  });
});

describe("buildNextDocument", () => {
  it("replaces only the inner text", () => {
    const doc = wrap('{"old":true}', "<h1>before</h1>", "<p>after</p>");
    const result = buildNextDocument(doc, '{"new":1}');
    expect(result.kind).toBe("write");
    if (result.kind !== "write") throw new Error("unreachable");
    expect(result.next).toContain("<h1>before</h1>");
    expect(result.next).toContain("<p>after</p>");
    expect(result.next).toContain(
      `${ANSWER_BLOCK_OPEN}${escapeAnswersJson('{"new":1}')}${ANSWER_BLOCK_CLOSE}`,
    );
    expect(result.next).not.toContain('{"old":true}');
  });

  it("reports a no-op when the block already holds the escaped payload", () => {
    const payload = '{"a":"<x>"}';
    const doc = wrap(escapeAnswersJson(payload));
    expect(buildNextDocument(doc, payload).kind).toBe("no-op");
  });

  it("rejects invalid JSON", () => {
    const result = buildNextDocument(wrap("{}"), "{not json");
    expect(result).toEqual({ kind: "error", code: "bad-request" });
  });

  it("rejects oversize payloads measured in UTF-8 bytes", () => {
    // Multi-byte characters: char count under the cap, byte count over it.
    const smiley = "😀"; // 4 UTF-8 bytes
    const json = JSON.stringify({
      s: smiley.repeat(Math.ceil(MAX_ANSWERS_JSON_BYTES / 4)),
    });
    expect(answersJsonByteLength(json)).toBeGreaterThan(MAX_ANSWERS_JSON_BYTES);
    expect(buildNextDocument(wrap("{}"), json)).toEqual({
      kind: "error",
      code: "too-large",
    });
  });

  it("propagates missing/ambiguous block errors", () => {
    expect(buildNextDocument("<html></html>", "{}")).toEqual({
      kind: "error",
      code: "missing-block",
    });
    const decoy = `${ANSWER_BLOCK_OPEN}${ANSWER_BLOCK_CLOSE}`;
    expect(buildNextDocument(wrap("{}", decoy), "{}")).toEqual({
      kind: "error",
      code: "ambiguous-block",
    });
  });
});

describe("answerBlockMatches", () => {
  it("matches only when the single block holds the expected inner text", () => {
    const expected = escapeAnswersJson('{"v":1}');
    expect(answerBlockMatches(wrap(expected), expected)).toBe(true);
    expect(answerBlockMatches(wrap("{}"), expected)).toBe(false);
    expect(answerBlockMatches("<html></html>", expected)).toBe(false);
    const doubled = wrap(expected, `${ANSWER_BLOCK_OPEN}${ANSWER_BLOCK_CLOSE}`);
    expect(answerBlockMatches(doubled, expected)).toBe(false);
  });
});
