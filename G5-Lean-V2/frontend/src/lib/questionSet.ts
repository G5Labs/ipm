// Referenced questions: the client side of the question set file contract
// (`g5.documents.question.referenced`, `.writer`, `.decision`).
//
// A Project Doc holds a question by reference — a `g5-question-ref` fence
// whose body is `{"path": "<set path>", "id": "<question id>"}` — and an HTML
// explainer by a `data-g5-question="<set path>#<question id>"` placeholder.
// The set file (`docs/<workspace>/questions/<set>.g5q.json`, schema 3) is the
// one home of the question and its answer; only the server's question
// service writes it. This module parses references, mirrors the read shapes
// the service returns, and calls the public question routes. Path rules and
// the decision predicate mirror `backend/g5-contracts/src/types/question_set.rs`
// (parity-tested against its fixtures).

import { api, ApiError } from "./api";

export const QUESTION_REF_FENCE_LANGUAGE = "g5-question-ref";
export const QUESTION_SET_EXT = ".g5q.json";

export function isQuestionRefFenceLanguage(
  language: string | undefined,
): boolean {
  return (language ?? "").trim().toLowerCase() === QUESTION_REF_FENCE_LANGUAGE;
}

// ---------------------------------------------------------------------------
// Shapes (camelCase, as the service serializes them)
// ---------------------------------------------------------------------------

export type SetQuestionStatus = "open" | "proposed" | "answered";
export type SetQuestionSeverity = "low" | "medium" | "high" | "critical";

export interface SetProvenance {
  kind: "user" | "agent";
  id?: string;
  email?: string;
  avatarUrl?: string;
  via?: string;
}

export interface SetQuestionOption {
  id: string;
  label: string;
  description?: string;
  tradeoffs?: string;
  risks?: string;
  recommended?: boolean;
}

export interface SetAnswer {
  selectedOptionIds: string[];
  text: string;
  rationale: string;
  answeredBy: SetProvenance | null;
  answeredAt: string | null;
  answeredAtRevision: number | null;
  version: number;
}

export interface QuestionEntry {
  id: string;
  revision: number;
  status: SetQuestionStatus;
  prompt: string;
  context?: string;
  severity?: SetQuestionSeverity;
  multiSelect: boolean;
  allowFreeText: boolean;
  options: SetQuestionOption[];
  answer: SetAnswer;
  /** `answered` with `user` provenance, as the service evaluated it. */
  decision: boolean;
  displayNumber: number;
}

export interface QuestionSetView {
  path: string;
  setId: string;
  source: "live" | "committed";
  hash?: string;
  vmId?: string;
  /** True only for the attached owner reading the live file. */
  answerable: boolean;
  title?: string;
  questions: QuestionEntry[];
  invalid?: { code: string; message: string };
}

export type Publication =
  | { status: "published" }
  | { status: "notPublished"; message: string }
  | { status: "live" };

export interface AnswerOutcome {
  path: string;
  entry: QuestionEntry;
  replayed: boolean;
  hash: string;
  vmId: string;
  publication: Publication;
}

// ---------------------------------------------------------------------------
// References
// ---------------------------------------------------------------------------

export interface QuestionRef {
  path: string;
  id: string;
}

export type QuestionRefParse =
  | ({ ok: true } & QuestionRef)
  | { ok: false; detail: string };

/**
 * A question set path: `docs/<owner>/questions/<set>.g5q.json` with one
 * plain owner segment and a valid set id. Traversal, extra segments, other
 * extensions and backslashes are refused.
 */
export function isQuestionSetPath(path: string): boolean {
  // eslint-disable-next-line no-control-regex -- refusing control characters is the rule
  if (path.includes("\\") || /[\u0000-\u001f\u007f]/.test(path)) return false;
  const segments = path.split("/");
  if (segments.length !== 4) return false;
  const [root, owner, dir, name] = segments;
  if (root !== "docs" || dir !== "questions") return false;
  if (!owner || owner === "." || owner === ".." || owner.trim() !== owner) {
    return false;
  }
  if (!name.endsWith(QUESTION_SET_EXT)) return false;
  return isSetId(name.slice(0, -QUESTION_SET_EXT.length));
}

export function isSetId(candidate: string): boolean {
  return /^[a-z0-9][a-z0-9-]{0,63}$/.test(candidate);
}

export function isQuestionId(candidate: string): boolean {
  return (
    candidate.length > 0 &&
    candidate.length <= 128 &&
    candidate.trim() === candidate &&
    !candidate.includes("/") &&
    // eslint-disable-next-line no-control-regex -- refusing control characters is the rule
    !/[\u0000-\u001f\u007f]/.test(candidate)
  );
}

function refFrom(path: unknown, id: unknown): QuestionRefParse {
  if (typeof path !== "string" || !isQuestionSetPath(path)) {
    return {
      ok: false,
      detail: "path must be docs/<workspace>/questions/<set>.g5q.json",
    };
  }
  if (typeof id !== "string" || !isQuestionId(id)) {
    return { ok: false, detail: "id must be a question id" };
  }
  return { ok: true, path, id };
}

/** Parse a `g5-question-ref` fence body. Never throws. */
export function parseQuestionRef(body: string): QuestionRefParse {
  let raw: unknown;
  try {
    raw = JSON.parse(body.trim());
  } catch {
    return { ok: false, detail: "the reference is not JSON" };
  }
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) {
    return { ok: false, detail: "the reference is not a JSON object" };
  }
  const keys = Object.keys(raw).sort();
  if (keys.length !== 2 || keys[0] !== "id" || keys[1] !== "path") {
    return {
      ok: false,
      detail: `expected exactly the keys "path" and "id", found ${JSON.stringify(keys)}`,
    };
  }
  const record = raw as Record<string, unknown>;
  return refFrom(record.path, record.id);
}

/** Parse a `data-g5-question` value `<set path>#<question id>`. */
export function parseQuestionPlaceholder(value: string): QuestionRefParse {
  const trimmed = value.trim();
  const hash = trimmed.lastIndexOf("#");
  if (hash < 0) {
    return { ok: false, detail: 'expected "<set path>#<question id>"' };
  }
  return refFrom(trimmed.slice(0, hash), trimmed.slice(hash + 1));
}

export function formatQuestionPlaceholder(ref: QuestionRef): string {
  return `${ref.path}#${ref.id}`;
}

const PLACEHOLDER_ATTR_RE =
  /data-g5-question\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s"'=<>`]+))/gi;

/**
 * Every distinct well-formed `data-g5-question` placeholder in an HTML
 * document, in document order. Purely lexical: the authored bytes are
 * untrusted, so only values `parseQuestionPlaceholder` accepts come out.
 */
export function extractQuestionPlaceholders(html: string): QuestionRef[] {
  const refs: QuestionRef[] = [];
  const seen = new Set<string>();
  for (const match of html.matchAll(PLACEHOLDER_ATTR_RE)) {
    const parsed = parseQuestionPlaceholder(
      match[1] ?? match[2] ?? match[3] ?? "",
    );
    if (!parsed.ok) continue;
    const key = formatQuestionPlaceholder(parsed);
    if (seen.has(key)) continue;
    seen.add(key);
    refs.push({ path: parsed.path, id: parsed.id });
  }
  return refs;
}

// ---------------------------------------------------------------------------
// Entry facts
// ---------------------------------------------------------------------------

/** `g5.documents.question.decision`: answered by a person. */
export function isDecision(entry: Pick<QuestionEntry, "status" | "answer">) {
  return (
    entry.status === "answered" && entry.answer.answeredBy?.kind === "user"
  );
}

/** The earlier revision an answer was given against, when wording moved on. */
export function answeredAgainstEarlierRevision(
  entry: QuestionEntry,
): number | null {
  const at = entry.answer.answeredAtRevision;
  if (entry.status === "open" || at == null) return null;
  // The answer transition itself advances the revision to `at`; only an
  // authoring edit afterwards moves `revision` past it.
  return at < entry.revision ? at : null;
}

export function findEntry(
  set: QuestionSetView | undefined | null,
  id: string,
): QuestionEntry | null {
  return set?.questions.find((question) => question.id === id) ?? null;
}

// ---------------------------------------------------------------------------
// Client
// ---------------------------------------------------------------------------

export function questionSetQueryKey(
  workspaceId: string,
  path: string,
): readonly unknown[] {
  return ["question-set", workspaceId, path] as const;
}

export function fetchQuestionSet(
  workspaceId: string,
  path: string,
): Promise<QuestionSetView> {
  return api.get<QuestionSetView>(
    `/api/v1/workspaces/${encodeURIComponent(workspaceId)}/questions/set`,
    { params: { path } },
  );
}

export interface AnswerRequest {
  path: string;
  questionId: string;
  expectedRevision: number;
  idempotencyKey: string;
  selectedOptionIds?: string[];
  text?: string;
  rationale?: string;
  /** Retry of an applied, unpublished answer: the hash it produced. */
  appliedHash?: string;
}

export function answerQuestion(
  workspaceId: string,
  request: AnswerRequest,
): Promise<AnswerOutcome> {
  return api.post<AnswerOutcome>(
    `/api/v1/workspaces/${encodeURIComponent(workspaceId)}/questions/answer`,
    request,
  );
}

export function clearQuestion(
  workspaceId: string,
  request: Omit<AnswerRequest, "selectedOptionIds" | "text" | "rationale">,
): Promise<AnswerOutcome> {
  return api.post<AnswerOutcome>(
    `/api/v1/workspaces/${encodeURIComponent(workspaceId)}/questions/clear`,
    request,
  );
}

// ---------------------------------------------------------------------------
// Votes (`g5.documents.question.votes`)
// ---------------------------------------------------------------------------

/** One voter's row, as the vote service serializes it. */
export interface QuestionVote {
  questionId: string;
  userId: string;
  displayName: string;
  avatarUrl?: string;
  selectedOptionIds: string[];
  text: string;
  questionRevision: number;
  updatedAt: string;
}

/** Every vote of one set, with the signed-in viewer so a card marks its own. */
export interface QuestionVotesView {
  path: string;
  viewerId: string;
  votes: QuestionVote[];
}

export interface VoteRequest {
  path: string;
  questionId: string;
  /** The entry revision the voter saw; a moved revision is refused. */
  expectedRevision: number;
  selectedOptionIds: string[];
  text: string;
}

export interface VoteOutcome {
  path: string;
  vote: QuestionVote;
}

export interface WithdrawVoteOutcome {
  path: string;
  questionId: string;
  withdrawn: boolean;
}

export function questionVotesQueryKey(
  workspaceId: string,
  path: string,
): readonly unknown[] {
  return ["question-votes", workspaceId, path] as const;
}

export function fetchQuestionVotes(
  workspaceId: string,
  path: string,
): Promise<QuestionVotesView> {
  return api.get<QuestionVotesView>(
    `/api/v1/workspaces/${encodeURIComponent(workspaceId)}/questions/votes`,
    { params: { path } },
  );
}

export function castQuestionVote(
  workspaceId: string,
  request: VoteRequest,
): Promise<VoteOutcome> {
  return api.post<VoteOutcome>(
    `/api/v1/workspaces/${encodeURIComponent(workspaceId)}/questions/vote`,
    request,
  );
}

export function withdrawQuestionVote(
  workspaceId: string,
  request: Pick<VoteRequest, "path" | "questionId">,
): Promise<WithdrawVoteOutcome> {
  return api.post<WithdrawVoteOutcome>(
    `/api/v1/workspaces/${encodeURIComponent(workspaceId)}/questions/unvote`,
    request,
  );
}

/** One question's tally, as every card shows it. */
export interface VoteTally {
  /** Voters on this question. */
  total: number;
  viewerVote: QuestionVote | null;
  /** Option id → the votes that picked it, in casting order. */
  byOption: Record<string, QuestionVote[]>;
  /** Votes that carry text, in casting order. */
  withText: QuestionVote[];
}

/** Aggregate a set's votes for `entry`; votes naming options the entry no
 * longer offers count toward the total but no option. */
export function tallyVotes(
  entry: Pick<QuestionEntry, "id" | "options">,
  view: QuestionVotesView | undefined | null,
): VoteTally {
  const byOption: Record<string, QuestionVote[]> = {};
  for (const option of entry.options) byOption[option.id] = [];
  const tally: VoteTally = {
    total: 0,
    viewerVote: null,
    byOption,
    withText: [],
  };
  if (!view) return tally;
  for (const vote of view.votes) {
    if (vote.questionId !== entry.id) continue;
    tally.total += 1;
    if (vote.userId === view.viewerId) tally.viewerVote = vote;
    for (const id of vote.selectedOptionIds) byOption[id]?.push(vote);
    if (vote.text.trim()) tally.withText.push(vote);
  }
  return tally;
}

/** The voters of one option as a card names them, the viewer marked. */
export function optionVoterNames(tally: VoteTally, optionId: string): string[] {
  return (tally.byOption[optionId] ?? []).map((vote) =>
    tally.viewerVote?.userId === vote.userId
      ? `${vote.displayName} (you)`
      : vote.displayName,
  );
}

/** "1 vote · Alice (you)" / "3 votes" / "0 votes". */
export function optionTallyLabel(names: readonly string[]): string {
  const count = `${names.length} ${names.length === 1 ? "vote" : "votes"}`;
  return names.length > 0 ? `${count} · ${names.join(", ")}` : count;
}

/** A typed refusal of an answer the card can explain. */
export type AnswerRefusal =
  | { kind: "revision-moved"; current: QuestionEntry | null }
  | { kind: "reload-required"; message: string; current: QuestionEntry | null }
  | { kind: "key-reused"; current: QuestionEntry | null }
  | { kind: "not-running"; message: string }
  | { kind: "other"; message: string };

export function answerRefusal(error: unknown): AnswerRefusal {
  if (error instanceof ApiError) {
    const details = error.details as {
      code?: string;
      message?: string;
      current?: QuestionEntry;
    };
    if (error.code === "QUESTION_CONFLICT") {
      const current = details.current ?? null;
      if (details.code === "revision-moved") {
        return { kind: "revision-moved", current };
      }
      if (details.code === "idempotency-key-reused") {
        return { kind: "key-reused", current };
      }
      return {
        kind: "reload-required",
        message: details.message ?? "The set changed; reload it.",
        current,
      };
    }
    if (
      error.code === "WORKSPACE_NOT_RUNNING" ||
      error.code === "WORKSPACE_STOPPING"
    ) {
      return {
        kind: "not-running",
        message: "Start the workspace to answer.",
      };
    }
    return { kind: "other", message: error.message };
  }
  return {
    kind: "other",
    message: error instanceof Error ? error.message : String(error),
  };
}
