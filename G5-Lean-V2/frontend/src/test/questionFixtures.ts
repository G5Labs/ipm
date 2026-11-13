// Question-store fixtures and msw handlers shared by the card and bridge tests.
import { HttpResponse, http } from "msw";
import { expect } from "vitest";
import { server } from "./msw/server";
import type {
  QuestionEntry,
  QuestionSetView,
  QuestionVote,
  QuestionVotesView,
} from "../lib/questionSet";

export const WS = "11111111-2222-4333-8444-555555555555";

/** The review-meeting template's three options, as a card receives them. */
export function meetingTaskOptions() {
  return [
    {
      id: "attach",
      label: "Create a task and attach to the current workspace",
    },
    { id: "detach", label: "Create a detached task" },
    { id: "reject", label: "Reject" },
  ];
}

/** The earlier template's four options, still recognised on older sets. */
export function meetingOptions() {
  return [
    { id: "accept", label: "Accept" },
    { id: "delay", label: "Delay" },
    { id: "out-of-scope", label: "Out of scope" },
    { id: "reject", label: "Reject" },
  ];
}
export const PATH = "docs/parent/questions/plan.g5q.json";

export function entry(overrides: Partial<QuestionEntry> = {}): QuestionEntry {
  return {
    id: "pick",
    revision: 1,
    status: "open",
    prompt: "Which storage backend?",
    multiSelect: false,
    allowFreeText: true,
    options: [
      { id: "file", label: "Set file", recommended: true },
      { id: "db", label: "Database" },
    ],
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
    ...overrides,
  };
}

export function set(
  questions: QuestionEntry[],
  answerable = true,
): QuestionSetView {
  return {
    path: PATH,
    setId: "plan",
    source: answerable ? "live" : "committed",
    hash: "h1",
    answerable,
    title: "Plan",
    questions,
  };
}

export function serveSet(view: QuestionSetView) {
  server.use(
    http.get(`*/api/v1/workspaces/${WS}/questions/set`, ({ request }) => {
      expect(new URL(request.url).searchParams.get("path")).toBe(PATH);
      return HttpResponse.json({ data: view });
    }),
  );
}

export function vote(
  userId: string,
  selectedOptionIds: string[],
  text = "",
): QuestionVote {
  return {
    questionId: "pick",
    userId,
    displayName: userId === "me" ? "Me" : `User ${userId}`,
    selectedOptionIds,
    text,
    questionRevision: 1,
    updatedAt: "2026-10-09T10:00:00Z",
  };
}

export function serveVotes(view: Partial<QuestionVotesView> = {}) {
  server.use(
    http.get(`*/api/v1/workspaces/${WS}/questions/votes`, () =>
      HttpResponse.json({
        data: { path: PATH, viewerId: "me", votes: [], ...view },
      }),
    ),
  );
}

/** The API's refusal of a vote naming an option the entry does not offer. */
export function unknownOptionRefusal() {
  return HttpResponse.json(
    {
      error: {
        code: "VALIDATION_ERROR",
        message: "option gone is not offered",
        details: { error: "unknown-option" },
      },
    },
    { status: 422 },
  );
}

/** The API's refusal of a vote cast against an earlier revision. */
export function revisionMovedRefusal(current: QuestionEntry) {
  return HttpResponse.json(
    {
      error: {
        code: "QUESTION_CONFLICT",
        message: "revision moved",
        details: {
          code: "revision-moved",
          currentRevision: current.revision,
          current,
        },
      },
    },
    { status: 409 },
  );
}
