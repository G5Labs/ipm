import { describe, expect, it } from "vitest";
import {
  MAX_QUESTION_PROPOSAL_CHARS,
  validateQuestionProposal,
} from "./questionCardBridge";
import {
  withSandboxedPlatformTheme,
  withSandboxedQuestionCards,
} from "./sandboxedHtml";
import { QUESTION_CARD_RENDERER_ATTR } from "./questionCardRenderer";
import type { QuestionSetView } from "../../lib/questionSet";

const PATH = "docs/ws/questions/plan.g5q.json";
const KEY = `${PATH}#pick`;
const SET: QuestionSetView = {
  path: PATH,
  setId: "plan",
  source: "live",
  answerable: true,
  questions: [
    {
      id: "pick",
      revision: 4,
      status: "open",
      prompt: "Pick?",
      multiSelect: false,
      allowFreeText: true,
      options: [{ id: "a", label: "A" }],
      answer: {
        selectedOptionIds: [],
        text: "",
        rationale: "",
        answeredBy: null,
        answeredAt: null,
        answeredAtRevision: null,
        version: 0,
      },
      decision: false,
      displayNumber: 1,
    },
  ],
};
const SETS = new Map([[PATH, SET]]);
const message = (overrides: Record<string, unknown> = {}) => ({
  source: "g5-html-doc",
  version: 1,
  generation: "gen-1",
  type: "propose-question-answer",
  requestId: "r1",
  ref: KEY,
  expectedRevision: 4,
  selectedOptionIds: ["a"],
  text: "",
  rationale: "",
  ...overrides,
});

describe("question frame messages", () => {
  it("accepts a well-formed proposal for a listed reference", () => {
    const result = validateQuestionProposal(
      message(),
      "gen-1",
      new Set([KEY]),
      SETS,
    );
    expect("proposal" in result && result.proposal.action).toMatchObject({
      kind: "answer",
      request: {
        path: PATH,
        questionId: "pick",
        expectedRevision: 4,
        selectedOptionIds: ["a"],
      },
    });
  });

  it("accepts a vote and a withdrawal from any reader of a valid set", () => {
    const readOnly = new Map([[PATH, { ...SET, answerable: false }]]);
    const vote = validateQuestionProposal(
      message({ type: "propose-question-vote", text: " why " }),
      "gen-1",
      new Set([KEY]),
      readOnly,
    );
    // The vote carries the revision the person saw, like an answer.
    expect("proposal" in vote && vote.proposal.action).toEqual({
      kind: "vote",
      request: {
        path: PATH,
        questionId: "pick",
        expectedRevision: 4,
        selectedOptionIds: ["a"],
        text: "why",
      },
    });
    const withdrawal = validateQuestionProposal(
      message({
        type: "propose-question-vote",
        withdraw: true,
        selectedOptionIds: [],
        text: "",
      }),
      "gen-1",
      new Set([KEY]),
      readOnly,
    );
    expect("proposal" in withdrawal && withdrawal.proposal.action).toEqual({
      kind: "withdraw-vote",
      request: { path: PATH, questionId: "pick" },
    });
    // A vote still names only offered options, the current revision and a
    // listed reference, and never lands on an invalid set.
    for (const [code, data] of [
      [
        "unknown-option",
        message({ type: "propose-question-vote", selectedOptionIds: ["zzz"] }),
      ],
      [
        "stale-revision",
        message({ type: "propose-question-vote", expectedRevision: 1 }),
      ],
      [
        "empty-answer",
        message({ type: "propose-question-vote", selectedOptionIds: [] }),
      ],
    ] as const) {
      expect(
        validateQuestionProposal(data, "gen-1", new Set([KEY]), readOnly),
      ).toEqual({ rejected: code });
    }
    const invalid = new Map([
      [PATH, { ...SET, invalid: { code: "invalid-json", message: "bad" } }],
    ]);
    expect(
      validateQuestionProposal(
        message({ type: "propose-question-vote" }),
        "gen-1",
        new Set([KEY]),
        invalid,
      ),
    ).toEqual({ rejected: "not-answerable" });
  });

  it.each([
    ["stale-generation", message({ generation: "gen-0" })],
    [
      "unlisted-reference",
      message({ ref: "docs/ws/questions/other.g5q.json#pick" }),
    ],
    ["too-large", message({ text: "x".repeat(MAX_QUESTION_PROPOSAL_CHARS) })],
    ["unknown-option", message({ selectedOptionIds: ["zzz"] })],
    ["stale-revision", message({ expectedRevision: 3 })],
    ["not-a-proposal", message({ type: "propose-answers" })],
    ["bad-request-id", message({ requestId: "" })],
    ["empty-answer", message({ selectedOptionIds: [], text: "  " })],
  ])("drops %s", (code, data) => {
    expect(
      validateQuestionProposal(data, "gen-1", new Set([KEY]), SETS),
    ).toEqual({
      rejected: code,
    });
  });

  it("drops proposals for a set the viewer cannot answer", () => {
    const readOnly = new Map([[PATH, { ...SET, answerable: false }]]);
    expect(
      validateQuestionProposal(message(), "gen-1", new Set([KEY]), readOnly),
    ).toEqual({
      rejected: "not-answerable",
    });
  });
});

describe("question card injection", () => {
  it("injects the renderer only for documents with placeholders", () => {
    const plain = withSandboxedPlatformTheme("<p>hi</p>", "");
    expect(withSandboxedQuestionCards(plain)).toBe(plain);
    const composed = withSandboxedPlatformTheme(
      `<div data-g5-question="${KEY}"></div>`,
      "",
    );
    const injected = withSandboxedQuestionCards(composed);
    expect(injected).toContain(QUESTION_CARD_RENDERER_ATTR);
    // Before every authored byte.
    expect(injected.indexOf(QUESTION_CARD_RENDERER_ATTR)).toBeLessThan(
      injected.indexOf(`<div data-g5-question`),
    );
  });
});
