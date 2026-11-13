import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import {
  answeredAgainstEarlierRevision,
  extractQuestionPlaceholders,
  isDecision,
  isQuestionRefFenceLanguage,
  isQuestionSetPath,
  parseQuestionPlaceholder,
  parseQuestionRef,
  tallyVotes,
  type QuestionEntry,
  type QuestionVote,
} from "./questionSet";

const FIXTURES = join(
  __dirname,
  "../../../backend/g5-contracts/fixtures/question_set",
);
const PATH = "docs/parent-ws/questions/plan.g5q.json";

describe("question references", () => {
  it("parse exactly a path and an id", () => {
    expect(
      parseQuestionRef(`{"path": "${PATH}", "id": "pick-0123abcd"}`),
    ).toEqual({
      ok: true,
      path: PATH,
      id: "pick-0123abcd",
    });
    for (const body of [
      `{"path": "${PATH}", "id": "x", "extra": 1}`,
      `{"path": "${PATH}"}`,
      `{"set": "plan", "id": "x"}`,
      `{"path": "docs/../questions/plan.g5q.json", "id": "x"}`,
      `{"path": "${PATH}", "id": "a/b"}`,
      `["${PATH}", "x"]`,
      "not json",
    ]) {
      expect(parseQuestionRef(body).ok, body).toBe(false);
    }
    expect(isQuestionRefFenceLanguage(" G5-Question-Ref ")).toBe(true);
    expect(isQuestionRefFenceLanguage("g5-question")).toBe(false);
  });

  it("confine set paths like the Rust contract", () => {
    expect(isQuestionSetPath(PATH)).toBe(true);
    for (const bad of [
      "docs/../questions/x.g5q.json",
      "docs/./questions/x.g5q.json",
      "docs//questions/x.g5q.json",
      "docs/ws/questions/sub/x.g5q.json",
      "docs/ws/questions/Bad.g5q.json",
      "docs/ws/questions/x.json",
      "docs/ws/questions/.g5q.json",
      "/docs/ws/questions/x.g5q.json",
      "docs\\ws\\questions\\x.g5q.json",
      "code/ws/questions/x.g5q.json",
      "docs/ws/questions/x.g5q.json/",
    ]) {
      expect(isQuestionSetPath(bad), bad).toBe(false);
    }
  });

  it("extract distinct well-formed placeholders in document order", () => {
    const html = `
      <div data-g5-question="${PATH}#b"></div>
      <div data-g5-question='${PATH}#a'></div>
      <div data-g5-question="${PATH}#b"></div>
      <div data-g5-question="docs/../questions/x.g5q.json#c"></div>
      <div data-g5-question="no-hash"></div>`;
    expect(extractQuestionPlaceholders(html)).toEqual([
      { path: PATH, id: "b" },
      { path: PATH, id: "a" },
    ]);
    expect(parseQuestionPlaceholder(`${PATH}#q`).ok).toBe(true);
  });
});

describe("question set fixtures (parity with g5-contracts)", () => {
  const expected = JSON.parse(
    readFileSync(join(FIXTURES, "expected.json"), "utf8"),
  ) as Record<
    string,
    { decisions: Record<string, boolean>; statuses: Record<string, string> }
  >;
  const files = readdirSync(FIXTURES).filter((name) =>
    name.endsWith(".g5q.json"),
  );

  it("covers every fixture", () => {
    expect(files.sort()).toEqual(Object.keys(expected).sort());
  });

  it.each(files)("%s decides like the Rust predicate", (name) => {
    const file = JSON.parse(readFileSync(join(FIXTURES, name), "utf8"));
    expect(file.schema).toBe(3);
    expect(file.integrity).toBeUndefined();
    expect(isQuestionSetPath(`docs/ws/questions/${name}`)).toBe(true);
    for (const question of file.questions) {
      expect(isDecision(question), `${name}/${question.id}`).toBe(
        expected[name].decisions[question.id],
      );
      expect(question.status).toBe(expected[name].statuses[question.id]);
    }
  });
});

describe("entry facts", () => {
  const entry = (
    revision: number,
    at: number | null,
    status: QuestionEntry["status"],
  ) =>
    ({
      status,
      revision,
      answer: { answeredAtRevision: at },
    }) as unknown as QuestionEntry;

  it("names the earlier revision only after a wording edit", () => {
    expect(answeredAgainstEarlierRevision(entry(2, 2, "answered"))).toBeNull();
    expect(answeredAgainstEarlierRevision(entry(3, 2, "answered"))).toBe(2);
    expect(answeredAgainstEarlierRevision(entry(3, null, "open"))).toBeNull();
  });
});

describe("vote tallies", () => {
  const vote = (
    userId: string,
    selectedOptionIds: string[],
    text = "",
    questionId = "pick",
  ): QuestionVote => ({
    questionId,
    userId,
    displayName: userId,
    selectedOptionIds,
    text,
    questionRevision: 1,
    updatedAt: "2026-10-09T10:00:00Z",
  });
  const entry = {
    id: "pick",
    options: [
      { id: "a", label: "A" },
      { id: "b", label: "B" },
    ],
  };

  it("count each voter once, per offered option, and mark the viewer", () => {
    const tally = tallyVotes(entry, {
      path: "docs/ws/questions/plan.g5q.json",
      viewerId: "bob",
      votes: [
        vote("alice", ["a"], " because "),
        vote("bob", ["a", "b"]),
        vote("carol", ["gone"]),
        vote("dan", ["b"], "", "other-question"),
      ],
    });
    expect(tally.total).toBe(3);
    expect(tally.viewerVote?.userId).toBe("bob");
    expect(tally.byOption.a.map((v) => v.userId)).toEqual(["alice", "bob"]);
    expect(tally.byOption.b.map((v) => v.userId)).toEqual(["bob"]);
    expect(tally.byOption.gone).toBeUndefined();
    expect(tally.withText.map((v) => v.userId)).toEqual(["alice"]);
  });

  it("is empty without a view", () => {
    expect(tallyVotes(entry, null)).toEqual({
      total: 0,
      viewerVote: null,
      byOption: { a: [], b: [] },
      withText: [],
    });
  });
});
