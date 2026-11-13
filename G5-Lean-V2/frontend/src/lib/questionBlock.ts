// g5-question payload parsing/serialization for interactive question blocks in
// markdown Project Docs.
//
// Governing intent:
// - user_experience_model/collaborative_editing_ux D10 (question card UX)
// - software_architecture/client/workspace-page/center-panel D69 (payload
//   contract: lenient-read / strict-write, unknown-field preservation, 32 KiB
//   cap, one shared serializer owning normalization)
//
// This module is the ONLY place that parses or serializes `g5-question`
// payloads; no other client code may import `yaml` for this format.

import { parse as parseYaml, stringify as stringifyYaml } from "yaml";

/** Fence language that dispatches the structured question card. */
export const QUESTION_FENCE_LANGUAGE = "g5-question";

/**
 * The ONE fence-language predicate. Every surface that decides "is this a
 * question fence?" must call this — the read-only renderer, the editor's
 * block dispatch, and the live workspace file write path alike.
 *
 * Detectors that disagree are a correctness bug, not a cosmetic one: a fence
 * opened as ```G5-Question would render as an answerable card while the write
 * path skipped it, so pressing Answer returned `missing-question` for a card
 * the user could plainly see.
 */
export function isQuestionFenceLanguage(language: string | undefined): boolean {
  return (language ?? "").trim().toLowerCase() === QUESTION_FENCE_LANGUAGE;
}

/** Serialized payload cap in UTF-8 bytes (D69). */
export const MAX_QUESTION_PAYLOAD_BYTES = 32768;

export type QuestionSeverity = "low" | "medium" | "high" | "critical";
export type QuestionStatus = "open" | "answered";
export type QuestionAnsweredByKind = "user" | "agent";

/**
 * WHO answered the question (D69).
 *
 * `kind` is the authority discriminator — only `user` is a human decision.
 * The identity fields come from the authenticated session (`CurrentUserDTO`,
 * the same source the comment composer uses for its author row) so an answered
 * block records a real person rather than just a role. They are optional
 * because a LEGACY payload written as the scalar `answered_by: user` normalizes
 * to `{ kind: "user" }` with no identity attached.
 */
export interface QuestionAnsweredBy {
  kind: QuestionAnsweredByKind;
  /** Stable user id. */
  id?: string;
  /** Human-readable audit value. */
  email?: string;
  /** Display image rendered beside the answered summary. */
  avatar_url?: string;
  /** Unknown provenance fields preserved verbatim. */
  extra: Record<string, unknown>;
}

/** Option shape mirrors EscalationOption so one shape serves all surfaces. */
export interface QuestionOption {
  label: string;
  description: string;
  tradeoffs?: string;
  risks?: string;
  recommended?: boolean;
  /** Unknown option fields preserved verbatim for forward compatibility. */
  extra: Record<string, unknown>;
}

export interface QuestionAnswerState {
  selected: string[];
  text: string;
  rationale: string;
  /** Null while unanswered. */
  answered_by: QuestionAnsweredBy | null;
  answered_at: string | null;
  /** Unknown answer fields preserved verbatim. */
  extra: Record<string, unknown>;
}

export interface ParsedQuestion {
  schema: 1;
  id: string;
  status: QuestionStatus;
  prompt: string;
  severity?: QuestionSeverity;
  multiSelect: boolean;
  allowFreeText: boolean;
  options: QuestionOption[];
  answer: QuestionAnswerState;
  /** Unknown top-level fields preserved verbatim, in original order. */
  extra: Record<string, unknown>;
  /** Non-fatal lenient-read findings (e.g. multiple recommended options). */
  warnings: string[];
}

export type QuestionParseErrorCode =
  | "too-large"
  | "invalid-yaml"
  | "not-a-mapping"
  | "unsupported-schema";

export type QuestionParseResult =
  | { ok: true; question: ParsedQuestion }
  | { ok: false; error: QuestionParseErrorCode; detail?: string };

const SEVERITIES: readonly QuestionSeverity[] = [
  "low",
  "medium",
  "high",
  "critical",
];

const KNOWN_TOP_KEYS = new Set([
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
const KNOWN_OPTION_KEYS = new Set([
  "label",
  "description",
  "tradeoffs",
  "risks",
  "recommended",
]);
const KNOWN_ANSWER_KEYS = new Set([
  "selected",
  "text",
  "rationale",
  "answered_by",
  "answered_at",
]);
const KNOWN_ANSWERED_BY_KEYS = new Set(["kind", "id", "email", "avatar_url"]);

/** D69 authoring convention for `id`: lowercase, digits, single hyphens. */
const KEBAB_CASE_ID_RE = /^[a-z0-9]+(-[a-z0-9]+)*$/;

const encoder = new TextEncoder();

export function questionPayloadByteLength(source: string): number {
  return encoder.encode(source).byteLength;
}

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return (
    typeof value === "object" &&
    value !== null &&
    !Array.isArray(value) &&
    Object.getPrototypeOf(value) === Object.prototype
  );
}

function asString(value: unknown, fallback = ""): string {
  return typeof value === "string" ? value : fallback;
}

function asBoolean(value: unknown, fallback: boolean): boolean {
  return typeof value === "boolean" ? value : fallback;
}

function collectExtra(
  raw: Record<string, unknown>,
  known: ReadonlySet<string>,
): Record<string, unknown> {
  const extra: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(raw)) {
    if (!known.has(key)) extra[key] = value;
  }
  return extra;
}

function parseOption(raw: unknown, warnings: string[]): QuestionOption | null {
  if (!isPlainObject(raw)) {
    warnings.push("option entry is not a mapping and was ignored");
    return null;
  }
  const option: QuestionOption = {
    label: asString(raw.label),
    description: asString(raw.description),
    extra: collectExtra(raw, KNOWN_OPTION_KEYS),
  };
  if (typeof raw.tradeoffs === "string") option.tradeoffs = raw.tradeoffs;
  if (typeof raw.risks === "string") option.risks = raw.risks;
  if (typeof raw.recommended === "boolean") {
    option.recommended = raw.recommended;
  }
  if (option.label === "") {
    warnings.push("option without a label was ignored");
    return null;
  }
  return option;
}

function parseAnswer(raw: unknown): QuestionAnswerState {
  const source = isPlainObject(raw) ? raw : {};
  const selectedRaw = source.selected;
  const selected = Array.isArray(selectedRaw)
    ? selectedRaw.filter((item): item is string => typeof item === "string")
    : [];
  return {
    selected,
    text: asString(source.text),
    rationale: asString(source.rationale),
    answered_by: parseAnsweredBy(source.answered_by),
    answered_at:
      typeof source.answered_at === "string" ? source.answered_at : null,
    extra: collectExtra(source, KNOWN_ANSWER_KEYS),
  };
}

/**
 * Lenient-read of `answer.answered_by` (D69).
 *
 * Accepts BOTH shapes: the current mapping `{kind, id, email, avatar_url}` and
 * the LEGACY scalar `answered_by: user`, which normalizes to `{kind: "user"}`
 * with no identity. Documents written before the identity contract therefore
 * keep rendering and keep their authority. Anything else is `null`
 * (unanswered/unrecognized), which is never an authoritative human decision.
 */
function parseAnsweredBy(raw: unknown): QuestionAnsweredBy | null {
  const asKind = (value: unknown): QuestionAnsweredByKind | null =>
    value === "user" || value === "agent" ? value : null;

  if (typeof raw === "string") {
    const kind = asKind(raw);
    return kind ? { kind, extra: {} } : null;
  }
  if (!isPlainObject(raw)) return null;
  const kind = asKind(raw.kind);
  if (!kind) return null;
  const out: QuestionAnsweredBy = {
    kind,
    extra: collectExtra(raw, KNOWN_ANSWERED_BY_KEYS),
  };
  if (typeof raw.id === "string" && raw.id !== "") out.id = raw.id;
  if (typeof raw.email === "string" && raw.email !== "") out.email = raw.email;
  if (typeof raw.avatar_url === "string" && raw.avatar_url !== "") {
    out.avatar_url = raw.avatar_url;
  }
  return out;
}

/**
 * Lenient-read parse of a `g5-question` fence body.
 *
 * Never throws. Oversized input is rejected before parsing. `fallbackId`
 * supplies the doc-derived default id (the `[Q<N>.]` label) when the payload
 * omits `id`.
 */
export function parseQuestionPayload(
  source: string,
  { fallbackId = "" }: { fallbackId?: string } = {},
): QuestionParseResult {
  if (questionPayloadByteLength(source) > MAX_QUESTION_PAYLOAD_BYTES) {
    return { ok: false, error: "too-large" };
  }
  let raw: unknown;
  try {
    // `uniqueKeys` stays at its default `true`: duplicate mapping keys must
    // fail HERE, before `parseYaml` collapses the mapping into a plain object.
    // A JS object cannot hold duplicates, so a repeated `id`, `answer`, or
    // unknown field would otherwise silently keep only the last value —
    // dropping data the strict-write serializer then re-emits as canonical.
    // A fenced CRLF body can end between CR and LF; finish that line for YAML parsing only.
    raw = parseYaml(source.endsWith("\r") ? `${source}\n` : source, {
      schema: "core",
    });
  } catch (error) {
    return {
      ok: false,
      error: "invalid-yaml",
      detail: error instanceof Error ? error.message : String(error),
    };
  }
  if (!isPlainObject(raw)) {
    return { ok: false, error: "not-a-mapping" };
  }
  if (raw.schema !== 1) {
    // The detail feeds the fallback notice a human repairs from, so it names
    // the expectation beside the value found (center-panel D69, issue #1872).
    return {
      ok: false,
      error: "unsupported-schema",
      detail: `schema: ${String(raw.schema)}; expected schema: 1`,
    };
  }

  const warnings: string[] = [];
  const optionsRaw = Array.isArray(raw.options) ? raw.options : [];
  const options = optionsRaw
    .map((entry) => parseOption(entry, warnings))
    .filter((option): option is QuestionOption => option !== null);

  let seenRecommended = false;
  for (const option of options) {
    if (option.recommended === true) {
      if (seenRecommended) {
        option.recommended = false;
        warnings.push(
          "multiple options marked recommended; keeping only the first",
        );
      }
      seenRecommended = true;
    }
  }

  // Option labels ARE the answer identity (`answer.selected` stores labels),
  // so a collision makes a persisted answer ambiguous and makes selection,
  // details expansion, and summary keys address two options at once. Same
  // lenient-read treatment as the `recommended` ambiguity: report, don't fail.
  const seenLabels = new Set<string>();
  const reportedLabels = new Set<string>();
  for (const option of options) {
    if (seenLabels.has(option.label) && !reportedLabels.has(option.label)) {
      reportedLabels.add(option.label);
      warnings.push(
        `duplicate option label ${JSON.stringify(option.label)}; ` +
          "answers referencing it are ambiguous",
      );
    }
    seenLabels.add(option.label);
  }

  const statusRaw = raw.status;
  const status: QuestionStatus = statusRaw === "answered" ? "answered" : "open";
  if (
    statusRaw !== undefined &&
    statusRaw !== "open" &&
    statusRaw !== "answered"
  ) {
    warnings.push(`unknown status ${String(statusRaw)}; treating as open`);
  }

  const severityRaw = raw.severity;
  const severity = SEVERITIES.includes(severityRaw as QuestionSeverity)
    ? (severityRaw as QuestionSeverity)
    : undefined;
  if (severityRaw !== undefined && severity === undefined) {
    warnings.push(`unknown severity ${String(severityRaw)}; hiding chip`);
  }

  const id = asString(raw.id, fallbackId) || fallbackId;
  // D69 authoring convention, surfaced as a warning rather than enforced: a
  // non-conforming id still addresses its fence and still serializes, so
  // rejecting here would turn existing documents into invalid-payload notices.
  if (id !== "" && !KEBAB_CASE_ID_RE.test(id)) {
    warnings.push(
      `question id ${JSON.stringify(id)} is not kebab-case ` +
        "(lowercase letters, digits, and single hyphens)",
    );
  }

  return {
    ok: true,
    question: {
      schema: 1,
      id,
      status,
      prompt: asString(raw.prompt),
      ...(severity !== undefined ? { severity } : {}),
      multiSelect: asBoolean(raw.multi_select, false),
      allowFreeText: asBoolean(raw.allow_free_text, true),
      options,
      answer: parseAnswer(raw.answer),
      extra: collectExtra(raw, KNOWN_TOP_KEYS),
      warnings,
    },
  };
}

export interface AnswerInput {
  selected: string[];
  text?: string;
  rationale?: string;
  /**
   * WHO is answering. The card supplies the authenticated session's identity
   * so the persisted record names a real person; `kind` remains the authority
   * discriminator.
   */
  answeredBy: QuestionAnsweredBy;
  answeredAt: string;
}

/** Returns a new ParsedQuestion with the answer applied and status flipped. */
export function applyAnswer(
  question: ParsedQuestion,
  input: AnswerInput,
): ParsedQuestion {
  return {
    ...question,
    status: "answered",
    answer: {
      ...question.answer,
      selected: [...input.selected],
      text: input.text ?? "",
      rationale: input.rationale ?? "",
      answered_by: input.answeredBy,
      answered_at: input.answeredAt,
    },
  };
}

/**
 * Return a question to its unanswered state (D10 Clear answer).
 *
 * The inverse of `applyAnswer`: the `answer` slot is emptied and `status` goes
 * back to `open`, so the block reads exactly as it did before anyone answered.
 * Unknown payload fields and unknown keys INSIDE `answer` survive, for the same
 * reason every other rewrite preserves them — this path must not be a way to
 * silently drop data a future schema added.
 */
export function clearAnswer(question: ParsedQuestion): ParsedQuestion {
  return {
    ...question,
    status: "open",
    answer: {
      ...question.answer,
      selected: [],
      text: "",
      rationale: "",
      answered_by: null,
      answered_at: null,
    },
  };
}

export class QuestionSerializeError extends Error {
  readonly code = "too-large";
  constructor(byteLength: number) {
    super(
      `serialized g5-question payload is ${byteLength} bytes, over the ` +
        `${MAX_QUESTION_PAYLOAD_BYTES}-byte cap`,
    );
    this.name = "QuestionSerializeError";
  }
}

function serializeOption(option: QuestionOption): Record<string, unknown> {
  const out: Record<string, unknown> = {
    label: option.label,
    description: option.description,
  };
  if (option.tradeoffs !== undefined) out.tradeoffs = option.tradeoffs;
  if (option.risks !== undefined) out.risks = option.risks;
  if (option.recommended !== undefined) out.recommended = option.recommended;
  for (const [key, value] of Object.entries(option.extra)) out[key] = value;
  return out;
}

/**
 * Strict-write form of `answered_by` (D69): always the mapping, never the
 * legacy scalar, with canonical key order and unknown fields re-emitted.
 * `null` while unanswered.
 */
function serializeAnsweredBy(
  answeredBy: QuestionAnsweredBy | null,
): Record<string, unknown> | null {
  if (!answeredBy) return null;
  const out: Record<string, unknown> = { kind: answeredBy.kind };
  if (answeredBy.id !== undefined) out.id = answeredBy.id;
  if (answeredBy.email !== undefined) out.email = answeredBy.email;
  if (answeredBy.avatar_url !== undefined) {
    out.avatar_url = answeredBy.avatar_url;
  }
  for (const [key, value] of Object.entries(answeredBy.extra)) out[key] = value;
  return out;
}

/**
 * Strict-write serializer — the single owner of payload normalization
 * (stable key order, indent 2, no line folding). Unknown fields at every
 * level are re-emitted after the canonical keys, preserving their values
 * verbatim so a newer schema's or an agent's additions survive answering.
 *
 * Throws QuestionSerializeError when the output exceeds the byte cap.
 */
export function serializeQuestionPayload(question: ParsedQuestion): string {
  const answer: Record<string, unknown> = {
    selected: [...question.answer.selected],
    text: question.answer.text,
    rationale: question.answer.rationale,
    answered_by: serializeAnsweredBy(question.answer.answered_by),
    answered_at: question.answer.answered_at,
  };
  for (const [key, value] of Object.entries(question.answer.extra)) {
    answer[key] = value;
  }

  const out: Record<string, unknown> = {
    schema: question.schema,
    id: question.id,
    status: question.status,
    prompt: question.prompt,
  };
  if (question.severity !== undefined) out.severity = question.severity;
  out.multi_select = question.multiSelect;
  out.allow_free_text = question.allowFreeText;
  out.options = question.options.map(serializeOption);
  out.answer = answer;
  for (const [key, value] of Object.entries(question.extra)) out[key] = value;

  const serialized = stringifyYaml(out, {
    indent: 2,
    lineWidth: 0,
    minContentWidth: 0,
  });
  const byteLength = questionPayloadByteLength(serialized);
  if (byteLength > MAX_QUESTION_PAYLOAD_BYTES) {
    throw new QuestionSerializeError(byteLength);
  }
  return serialized;
}

/**
 * True when two options share a label.
 *
 * Labels are the answer identity (`answer.selected` stores labels), so a
 * collision makes a persisted answer ambiguous: it cannot say which option was
 * chosen. Such a payload is read-only until an author fixes the labels — the
 * card refuses to answer it and the write path refuses to apply one.
 */
export function hasDuplicateOptionLabels(question: ParsedQuestion): boolean {
  const seen = new Set<string>();
  for (const option of question.options) {
    if (seen.has(option.label)) return true;
    seen.add(option.label);
  }
  return false;
}

/** The option currently marked recommended, if any. */
export function recommendedOption(
  question: ParsedQuestion,
): QuestionOption | undefined {
  return question.options.find((option) => option.recommended === true);
}
