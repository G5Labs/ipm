import { describe, expect, it } from "vitest";
import {
  MAX_QUESTION_PAYLOAD_BYTES,
  QUESTION_FENCE_LANGUAGE,
  QuestionSerializeError,
  applyAnswer,
  isQuestionFenceLanguage,
  parseQuestionPayload,
  questionPayloadByteLength,
  recommendedOption,
  serializeQuestionPayload,
} from "./questionBlock";

// The design-doc §5.1 canonical example, lightly reformatted.
const CANONICAL = `schema: 1
id: quiet-hours-owner
status: open
prompt: >
  Should quiet-hours enforcement live in notification_service
  or user_service?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: notification_service
    description: Keep enforcement beside delivery logic.
    tradeoffs: Duplicates a preference read per send.
    risks: Preference-schema drift goes unnoticed.
    recommended: true
  - label: user_service
    description: Enforce where the preference data lives.
    tradeoffs: Couples delivery latency to user_service.
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
`;

function parseOk(source: string, fallbackId?: string) {
  const result = parseQuestionPayload(source, { fallbackId });
  if (!result.ok) throw new Error(`expected ok, got ${result.error}`);
  return result.question;
}

describe("parseQuestionPayload", () => {
  it("parses the canonical example", () => {
    const q = parseOk(CANONICAL);
    expect(q.id).toBe("quiet-hours-owner");
    expect(q.status).toBe("open");
    expect(q.severity).toBe("medium");
    expect(q.multiSelect).toBe(false);
    expect(q.allowFreeText).toBe(true);
    expect(q.options).toHaveLength(2);
    expect(q.options[0].recommended).toBe(true);
    expect(q.options[1].tradeoffs).toBe(
      "Couples delivery latency to user_service.",
    );
    expect(recommendedOption(q)?.label).toBe("notification_service");
    expect(q.answer.selected).toEqual([]);
    expect(q.answer.answered_by).toBeNull();
    expect(q.warnings).toEqual([]);
  });

  it("round-trips: parse → serialize → parse is stable", () => {
    const first = parseOk(CANONICAL);
    const serialized = serializeQuestionPayload(first);
    const second = parseOk(serialized);
    expect(second).toEqual(first);
    // serialization itself is a fixed point
    expect(serializeQuestionPayload(second)).toBe(serialized);
  });

  it("emits canonical key order", () => {
    const serialized = serializeQuestionPayload(parseOk(CANONICAL));
    const keys = serialized
      .split("\n")
      .filter((line) => /^[a-z_]+:/.test(line))
      .map((line) => line.split(":")[0]);
    expect(keys).toEqual([
      "schema",
      "id",
      "status",
      "prompt",
      "severity",
      "multi_select",
      "allow_free_text",
      "options",
      "answer",
    ]);
  });

  it("applies lenient defaults for missing fields", () => {
    const q = parseOk("schema: 1\nprompt: Where?\n", "q3");
    expect(q.id).toBe("q3");
    expect(q.status).toBe("open");
    expect(q.severity).toBeUndefined();
    expect(q.multiSelect).toBe(false);
    expect(q.allowFreeText).toBe(true);
    expect(q.options).toEqual([]);
    expect(q.answer).toEqual({
      selected: [],
      text: "",
      rationale: "",
      answered_by: null,
      answered_at: null,
      extra: {},
    });
  });

  it("supports zero-option pure free-text questions", () => {
    const q = parseOk("schema: 1\nprompt: Anything else?\noptions: []\n");
    expect(q.options).toEqual([]);
    expect(q.allowFreeText).toBe(true);
  });

  it("rejects malformed YAML", () => {
    const result = parseQuestionPayload("schema: 1\noptions:\n  - [unclosed");
    expect(result).toMatchObject({ ok: false, error: "invalid-yaml" });
  });

  it("rejects non-mapping documents", () => {
    const result = parseQuestionPayload("- a\n- b\n");
    expect(result).toMatchObject({ ok: false, error: "not-a-mapping" });
  });

  it("rejects unsupported schema versions", () => {
    const result = parseQuestionPayload("schema: 2\nprompt: hi\n");
    expect(result).toMatchObject({
      ok: false,
      error: "unsupported-schema",
      detail: "schema: 2; expected schema: 1",
    });
  });

  it("rejects the legacy pre-schema shape with a repair-naming detail (#1872)", () => {
    // Verbatim legacy payload shape from issue #1872: an agent improvised
    // AskUserQuestion-style fields with no `schema` declaration at all.
    const legacy = [
      "id: Q1",
      "title: Exclusion mechanism for the report UI",
      "question: >",
      "  Prefer a data-attribute marker or explicit node refs?",
      "options:",
      "  - label: data-screenshot-exclude attribute (recommended)",
      "    description: Flag the drawer container with a data attribute.",
      "  - label: Explicit node refs",
      "    description: Pass the element refs into captureScreenshot.",
      "",
    ].join("\n");
    const result = parseQuestionPayload(legacy);
    expect(result).toMatchObject({
      ok: false,
      error: "unsupported-schema",
      detail: "schema: undefined; expected schema: 1",
    });
  });

  it("rejects oversized payloads before parsing", () => {
    const big = `schema: 1\nprompt: ${"x".repeat(MAX_QUESTION_PAYLOAD_BYTES)}\n`;
    expect(questionPayloadByteLength(big)).toBeGreaterThan(
      MAX_QUESTION_PAYLOAD_BYTES,
    );
    const result = parseQuestionPayload(big);
    expect(result).toMatchObject({ ok: false, error: "too-large" });
  });

  it("keeps only the first of multiple recommended options", () => {
    const q = parseOk(
      [
        "schema: 1",
        "prompt: pick",
        "options:",
        "  - label: a",
        "    description: A",
        "    recommended: true",
        "  - label: b",
        "    description: B",
        "    recommended: true",
        "",
      ].join("\n"),
    );
    expect(q.options[0].recommended).toBe(true);
    expect(q.options[1].recommended).toBe(false);
    expect(q.warnings).toContain(
      "multiple options marked recommended; keeping only the first",
    );
  });

  it("treats unknown status/severity leniently with warnings", () => {
    const q = parseOk("schema: 1\nprompt: hi\nstatus: maybe\nseverity: huge\n");
    expect(q.status).toBe("open");
    expect(q.severity).toBeUndefined();
    expect(q.warnings).toHaveLength(2);
  });
});

describe("unknown-field preservation (forward compatibility)", () => {
  const WITH_EXTRAS = [
    "schema: 1",
    "id: fwd",
    "prompt: pick",
    "agent_note: keep me",
    "options:",
    "  - label: a",
    "    description: A",
    "    weight: 7",
    "answer:",
    "  selected: []",
    '  text: ""',
    '  rationale: ""',
    "  answered_by: null",
    "  answered_at: null",
    "  audit_ref: xyz",
    "",
  ].join("\n");

  it("preserves unknown fields at every level through answering", () => {
    const q = parseOk(WITH_EXTRAS);
    expect(q.extra).toEqual({ agent_note: "keep me" });
    expect(q.options[0].extra).toEqual({ weight: 7 });
    expect(q.answer.extra).toEqual({ audit_ref: "xyz" });

    const answered = applyAnswer(q, {
      selected: ["a"],
      rationale: "fits",
      answeredBy: { kind: "user", extra: {} },
      answeredAt: "2026-08-10T12:00:00Z",
    });
    const serialized = serializeQuestionPayload(answered);
    expect(serialized).toContain("agent_note: keep me");
    expect(serialized).toContain("weight: 7");
    expect(serialized).toContain("audit_ref: xyz");

    const reparsed = parseOk(serialized);
    expect(reparsed.status).toBe("answered");
    expect(reparsed.answer.selected).toEqual(["a"]);
    expect(reparsed.answer.answered_by?.kind).toBe("user");
    expect(reparsed.answer.answered_at).toBe("2026-08-10T12:00:00Z");
    expect(reparsed.extra).toEqual({ agent_note: "keep me" });
    expect(reparsed.options[0].extra).toEqual({ weight: 7 });
    expect(reparsed.answer.extra).toEqual({ audit_ref: "xyz" });
  });
});

describe("applyAnswer", () => {
  it("supports multi-select answers and does not mutate the input", () => {
    const q = parseOk(CANONICAL);
    const answered = applyAnswer(q, {
      selected: ["notification_service", "user_service"],
      text: "both, staged",
      answeredBy: { kind: "agent", extra: {} },
      answeredAt: "2026-08-10T09:30:00Z",
    });
    expect(answered.answer.selected).toEqual([
      "notification_service",
      "user_service",
    ]);
    expect(answered.answer.answered_by?.kind).toBe("agent");
    expect(answered.status).toBe("answered");
    expect(q.status).toBe("open");
    expect(q.answer.selected).toEqual([]);
  });
});

describe("serializeQuestionPayload", () => {
  it("throws a typed error when output exceeds the cap", () => {
    const q = parseOk(CANONICAL);
    const oversized = applyAnswer(q, {
      selected: [],
      text: "y".repeat(MAX_QUESTION_PAYLOAD_BYTES),
      answeredBy: { kind: "user", extra: {} },
      answeredAt: "2026-08-10T12:00:00Z",
    });
    expect(() => serializeQuestionPayload(oversized)).toThrow(
      QuestionSerializeError,
    );
  });
});

describe("constants", () => {
  it("exposes the fence language", () => {
    expect(QUESTION_FENCE_LANGUAGE).toBe("g5-question");
  });

  it("normalizes fence-language detection for every surface", () => {
    // One predicate for renderer, editor dispatch, and write path: a case or
    // whitespace variant must be answerable, not renderable-but-unwritable.
    expect(isQuestionFenceLanguage("g5-question")).toBe(true);
    expect(isQuestionFenceLanguage("G5-Question")).toBe(true);
    expect(isQuestionFenceLanguage("  g5-question  ")).toBe(true);
    expect(isQuestionFenceLanguage("g5-flow")).toBe(false);
    expect(isQuestionFenceLanguage(undefined)).toBe(false);
  });
});

describe("lenient-read integrity guards", () => {
  it("rejects duplicate mapping keys instead of silently keeping the last", () => {
    // Without the unique-key check the second `id` would overwrite the first
    // once the mapping collapsed into a plain object.
    const result = parseQuestionPayload(
      ["schema: 1", "id: first-id", "id: second-id", "prompt: Which?"].join(
        "\n",
      ),
    );
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.error).toBe("invalid-yaml");
  });

  it("rejects a duplicated answer mapping", () => {
    const result = parseQuestionPayload(
      [
        "schema: 1",
        "id: dup-answer",
        "prompt: Which?",
        "answer:",
        "  selected: [A]",
        "answer:",
        "  selected: [B]",
      ].join("\n"),
    );
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.error).toBe("invalid-yaml");
  });

  it("warns on duplicate option labels but still parses", () => {
    // Labels are the answer identity (`answer.selected` stores labels), so a
    // collision is reported rather than silently accepted.
    const q = parseOk(
      [
        "schema: 1",
        "id: dup-labels",
        "prompt: Which?",
        "options:",
        "  - label: Redis",
        "    description: first",
        "  - label: Redis",
        "    description: second",
      ].join("\n"),
    );
    expect(q.options).toHaveLength(2);
    expect(q.warnings.join(" ")).toContain("duplicate option label");
    // Reported once, not once per repeat.
    expect(
      q.warnings.filter((w) => w.includes("duplicate option label")),
    ).toHaveLength(1);
  });

  it("warns on a non-kebab-case id without failing the parse", () => {
    // Kebab-case is an authoring constraint (D69), not a render gate: a legacy
    // id must not degrade an existing doc to an invalid-payload notice.
    const q = parseOk(
      ["schema: 1", "id: Pick_Color", "prompt: Which?"].join("\n"),
    );
    expect(q.id).toBe("Pick_Color");
    expect(q.warnings.join(" ")).toContain("not kebab-case");
  });

  it("accepts a conforming kebab-case id with no warning", () => {
    const q = parseOk(
      ["schema: 1", "id: pick-color-2", "prompt: Which?"].join("\n"),
    );
    expect(q.warnings.join(" ")).not.toContain("kebab-case");
  });
});

describe("answered_by identity (D69)", () => {
  it("round-trips the identity mapping through parse → serialize → parse", () => {
    const q = parseOk(CANONICAL);
    const answered = applyAnswer(q, {
      selected: ["notification_service"],
      answeredBy: {
        kind: "user",
        id: "019fd9ab-3f99-79f2-948d-7f3ed4fe3f6e",
        email: "kaihuiz@g5labs.ai",
        avatar_url: "https://avatars.example/u/298389204",
        extra: {},
      },
      answeredAt: "2026-08-11T09:00:00Z",
    });
    const serialized = serializeQuestionPayload(answered);
    expect(serialized).toContain("kind: user");
    expect(serialized).toContain("email: kaihuiz@g5labs.ai");
    expect(serialized).toContain(
      "avatar_url: https://avatars.example/u/298389204",
    );

    const reparsed = parseOk(serialized);
    expect(reparsed.answer.answered_by).toEqual({
      kind: "user",
      id: "019fd9ab-3f99-79f2-948d-7f3ed4fe3f6e",
      email: "kaihuiz@g5labs.ai",
      avatar_url: "https://avatars.example/u/298389204",
      extra: {},
    });
  });

  it("normalizes the LEGACY scalar form so old documents keep their authority", () => {
    // Payloads written before the identity contract carry `answered_by: user`.
    // They must keep parsing, and keep counting as a human decision.
    const q = parseOk(
      [
        "schema: 1",
        "id: legacy",
        "status: answered",
        "prompt: Which?",
        "answer:",
        "  selected: [A]",
        "  answered_by: user",
        "  answered_at: 2026-08-01T00:00:00Z",
      ].join("\n"),
    );
    expect(q.answer.answered_by).toEqual({ kind: "user", extra: {} });
    // Strict-write upgrades it to the mapping form.
    expect(serializeQuestionPayload(q)).toContain("kind: user");
  });

  it("treats an unrecognized answered_by as unanswered, never authoritative", () => {
    for (const raw of [
      "  answered_by: robot",
      "  answered_by: {}",
      "  answered_by: 7",
    ]) {
      const q = parseOk(
        ["schema: 1", "id: x", "prompt: Which?", "answer:", raw].join("\n"),
      );
      expect(q.answer.answered_by).toBeNull();
    }
  });

  it("preserves unknown provenance fields verbatim", () => {
    const q = parseOk(
      [
        "schema: 1",
        "id: fwd",
        "prompt: Which?",
        "answer:",
        "  answered_by:",
        "    kind: user",
        "    email: a@b.c",
        "    sso_subject: keep-me",
      ].join("\n"),
    );
    expect(q.answer.answered_by?.extra).toEqual({ sso_subject: "keep-me" });
    expect(serializeQuestionPayload(q)).toContain("sso_subject: keep-me");
  });
});
