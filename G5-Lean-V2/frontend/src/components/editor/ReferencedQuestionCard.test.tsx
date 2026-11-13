import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { HttpResponse, http } from "msw";
import { Route, Routes } from "react-router-dom";
import { beforeEach, describe, expect, it } from "vitest";
import { renderApp } from "../../test/render";
import { createMockUser } from "../../test/fixtures";
import {
  makeIssueCreateTarget,
  makeIssueCreation,
  makeIssueLink,
  makeIssueSummary,
  serveExternalIssuesFeature,
} from "../../test/externalIssueFixtures";
import { server } from "../../test/msw/server";
import { ReferencedQuestionCard } from "./ReferencedQuestionCard";
import {
  PATH,
  WS,
  entry,
  meetingOptions,
  meetingTaskOptions,
  serveSet,
  serveVotes,
  revisionMovedRefusal,
  set,
  unknownOptionRefusal,
  vote,
} from "../../test/questionFixtures";
import { questionSetQueryKey } from "../../lib/questionSet";
import type { QuestionEntry } from "../../lib/questionSet";
import type { WorkspaceIssueLinkDTO } from "../../lib/types";

const REF = JSON.stringify({ path: PATH, id: "pick" });

function answered(): QuestionEntry {
  return entry({
    revision: 2,
    status: "answered",
    decision: true,
    answer: {
      selectedOptionIds: ["file"],
      text: "",
      rationale: "",
      answeredBy: { kind: "user", id: "u1", email: "owner@example.test" },
      answeredAt: "2026-10-09T10:00:00Z",
      answeredAtRevision: 2,
      version: 1,
    },
  });
}

describe("ReferencedQuestionCard", () => {
  beforeEach(() => serveVotes());

  it("answers through the question service and reports saved after publication", async () => {
    serveSet(set([entry()]));
    const bodies: Record<string, unknown>[] = [];
    server.use(
      http.post(
        `*/api/v1/workspaces/${WS}/questions/answer`,
        async ({ request }) => {
          bodies.push((await request.json()) as Record<string, unknown>);
          return HttpResponse.json({
            data: {
              path: PATH,
              entry: answered(),
              replayed: false,
              hash: "h2",
              vmId: "vm",
              publication: { status: "published" },
            },
          });
        },
      ),
    );
    renderApp(<ReferencedQuestionCard content={REF} workspaceId={WS} />);
    const options = await screen.findAllByTestId("referenced-question-option");
    await userEvent.click(options[0]);
    await userEvent.click(screen.getByTestId("referenced-question-answer"));
    expect(await screen.findByTestId("referenced-question-saved")).toBeTruthy();
    expect(bodies[0]).toMatchObject({
      path: PATH,
      questionId: "pick",
      expectedRevision: 1,
      selectedOptionIds: ["file"],
    });
    expect(bodies[0]).not.toHaveProperty("answeredBy");
    await waitFor(() =>
      expect(
        screen
          .getByTestId("referenced-question-card")
          .getAttribute("data-question-status"),
      ).toBe("answered"),
    );
  });

  it("retries an applied but unpublished answer with the same key and its hash", async () => {
    serveSet(set([entry()]));
    const bodies: Record<string, unknown>[] = [];
    server.use(
      http.post(
        `*/api/v1/workspaces/${WS}/questions/answer`,
        async ({ request }) => {
          const body = (await request.json()) as Record<string, unknown>;
          bodies.push(body);
          return HttpResponse.json({
            data: {
              path: PATH,
              entry: answered(),
              replayed: bodies.length > 1,
              hash: "h2",
              vmId: "vm",
              publication:
                bodies.length === 1
                  ? { status: "notPublished", message: "push denied" }
                  : { status: "published" },
            },
          });
        },
      ),
    );
    renderApp(<ReferencedQuestionCard content={REF} workspaceId={WS} />);
    await userEvent.click(
      (await screen.findAllByTestId("referenced-question-option"))[1],
    );
    await userEvent.click(screen.getByTestId("referenced-question-answer"));
    expect(
      await screen.findByTestId("referenced-question-not-published"),
    ).toBeTruthy();
    await userEvent.click(screen.getByTestId("referenced-question-retry"));
    expect(await screen.findByTestId("referenced-question-saved")).toBeTruthy();
    expect(bodies[1].idempotencyKey).toBe(bodies[0].idempotencyKey);
    expect(bodies[1].appliedHash).toBe("h2");
    expect(bodies[0].appliedHash).toBeUndefined();
  });

  it("shows the current entry when the revision moved", async () => {
    serveSet(set([entry()]));
    server.use(
      http.post(`*/api/v1/workspaces/${WS}/questions/answer`, () =>
        HttpResponse.json(
          {
            error: {
              code: "QUESTION_CONFLICT",
              message: "Question conflict",
              details: { code: "revision-moved", current: answered() },
            },
          },
          { status: 409 },
        ),
      ),
    );
    renderApp(<ReferencedQuestionCard content={REF} workspaceId={WS} />);
    await userEvent.click(
      (await screen.findAllByTestId("referenced-question-option"))[1],
    );
    await userEvent.click(screen.getByTestId("referenced-question-answer"));
    expect(
      (await screen.findByTestId("referenced-question-notice")).textContent,
    ).toMatch(/changed since you opened it/);
    await waitFor(() =>
      expect(
        screen
          .getByTestId("referenced-question-card")
          .getAttribute("data-question-revision"),
      ).toBe("2"),
    );
  });

  it("renders readers and stopped workspaces read-only for the answer, with the vote open", async () => {
    serveSet(set([answered()], false));
    renderApp(<ReferencedQuestionCard content={REF} workspaceId={WS} />);
    expect(
      await screen.findByTestId("referenced-question-read-only"),
    ).toBeTruthy();
    expect(screen.queryByTestId("referenced-question-answer")).toBeNull();
    for (const option of screen.getAllByTestId("referenced-question-option")) {
      expect((option as HTMLButtonElement).disabled).toBe(false);
    }
    expect(screen.getByTestId("referenced-question-vote")).toBeTruthy();
  });

  it("shows the tally with voter names and lets any reader vote and withdraw", async () => {
    serveSet(set([entry()], false));
    // The server's rows: the GET serves whatever the writes left.
    let rows = [
      vote("a1", ["file"], "keeps git the truth"),
      vote("me", ["db"]),
    ];
    const bodies: Record<string, unknown>[] = [];
    server.use(
      http.get(`*/api/v1/workspaces/${WS}/questions/votes`, () =>
        HttpResponse.json({
          data: { path: PATH, viewerId: "me", votes: rows },
        }),
      ),
      http.post(
        `*/api/v1/workspaces/${WS}/questions/vote`,
        async ({ request }) => {
          bodies.push((await request.json()) as Record<string, unknown>);
          rows = [rows[0], vote("me", ["file"])];
          return HttpResponse.json({
            data: { path: PATH, vote: vote("me", ["file"]) },
          });
        },
      ),
      http.post(
        `*/api/v1/workspaces/${WS}/questions/unvote`,
        async ({ request }) => {
          bodies.push((await request.json()) as Record<string, unknown>);
          rows = [rows[0]];
          return HttpResponse.json({
            data: { path: PATH, questionId: "pick", withdrawn: true },
          });
        },
      ),
    );
    renderApp(<ReferencedQuestionCard content={REF} workspaceId={WS} />);
    const count = await screen.findByTestId("referenced-question-vote-count");
    await waitFor(() => expect(count.textContent).toContain("2 votes"));
    expect(count.textContent).toContain("you voted");
    const tallies = screen.getAllByTestId("referenced-question-option-votes");
    expect(tallies[0].textContent).toContain("1 vote · User a1");
    expect(tallies[1].textContent).toContain("1 vote · Me (you)");
    expect(
      screen.getByTestId("referenced-question-vote-texts").textContent,
    ).toContain("keeps git the truth");

    // Change the vote: the first option, then "Change vote".
    const options = screen.getAllByTestId("referenced-question-option");
    await userEvent.click(options[0]);
    expect(screen.getByTestId("referenced-question-vote").textContent).toBe(
      "Change vote",
    );
    await userEvent.click(screen.getByTestId("referenced-question-vote"));
    expect(await screen.findByTestId("referenced-question-voted")).toBeTruthy();
    expect(bodies[0]).toEqual({
      path: PATH,
      questionId: "pick",
      expectedRevision: 1,
      selectedOptionIds: ["file"],
      text: "",
    });
    // The tally follows the viewer's vote.
    await waitFor(() =>
      expect(
        screen.getAllByTestId("referenced-question-option-votes")[0]
          .textContent,
      ).toContain("2 votes"),
    );

    await userEvent.click(
      screen.getByTestId("referenced-question-withdraw-vote"),
    );
    expect(
      await screen.findByTestId("referenced-question-vote-withdrawn"),
    ).toBeTruthy();
    expect(bodies[1]).toEqual({ path: PATH, questionId: "pick" });
    await waitFor(() =>
      expect(
        screen.getByTestId("referenced-question-vote-count").textContent,
      ).toBe("1 vote"),
    );
  });

  it("shows votes as unavailable, not zero, when the tally cannot be read", async () => {
    serveSet(set([entry()], false));
    server.use(
      http.get(`*/api/v1/workspaces/${WS}/questions/votes`, () =>
        HttpResponse.json({ error: { code: "INTERNAL" } }, { status: 500 }),
      ),
    );
    renderApp(<ReferencedQuestionCard content={REF} workspaceId={WS} />);
    const count = await screen.findByTestId("referenced-question-vote-count");
    await waitFor(() =>
      expect(count.textContent).toContain("Votes unavailable"),
    );
    expect(count.textContent).not.toContain("No votes yet");
    await userEvent.click(
      (await screen.findAllByTestId("referenced-question-option"))[0],
    );
    expect(
      (screen.getByTestId("referenced-question-vote") as HTMLButtonElement)
        .disabled,
    ).toBe(true);
    expect(screen.getByTestId("referenced-question-votes-retry")).toBeTruthy();
  });

  it("shows the current question when the vote's revision moved", async () => {
    serveSet(set([entry()], false));
    server.use(
      http.post(`*/api/v1/workspaces/${WS}/questions/vote`, () =>
        revisionMovedRefusal(entry({ revision: 2, prompt: "Which store?" })),
      ),
    );
    renderApp(<ReferencedQuestionCard content={REF} workspaceId={WS} />);
    await userEvent.click(
      (await screen.findAllByTestId("referenced-question-option"))[0],
    );
    await userEvent.click(screen.getByTestId("referenced-question-vote"));
    expect(
      (await screen.findByTestId("referenced-question-vote-error")).textContent,
    ).toContain("changed since you opened it");
    await waitFor(() =>
      expect(screen.getByTestId("referenced-question-card")).toHaveAttribute(
        "data-question-revision",
        "2",
      ),
    );
    expect(screen.getByText("Which store?")).toBeTruthy();
  });

  it("explains a refused vote", async () => {
    serveSet(set([entry()], false));
    server.use(
      http.post(
        `*/api/v1/workspaces/${WS}/questions/vote`,
        unknownOptionRefusal,
      ),
    );
    renderApp(<ReferencedQuestionCard content={REF} workspaceId={WS} />);
    await userEvent.click(
      (await screen.findAllByTestId("referenced-question-option"))[1],
    );
    await userEvent.click(screen.getByTestId("referenced-question-vote"));
    expect(
      (await screen.findByTestId("referenced-question-vote-error")).textContent,
    ).toContain("not offered");
  });

  it("shows a proposal as unconfirmed and an earlier-revision answer", async () => {
    const proposal = entry({
      revision: 3,
      status: "proposed",
      answer: {
        selectedOptionIds: ["db"],
        text: "",
        rationale: "",
        answeredBy: { kind: "agent", id: "u1", via: "g5d" },
        answeredAt: null,
        answeredAtRevision: 2,
        version: 1,
      },
    });
    serveSet(set([proposal]));
    renderApp(<ReferencedQuestionCard content={REF} workspaceId={WS} />);
    expect(
      await screen.findByTestId("referenced-question-proposal"),
    ).toBeTruthy();
    expect(
      screen.getByTestId("referenced-question-earlier-revision").textContent,
    ).toMatch(/revision 2/);
    expect(screen.getByTestId("referenced-question-answer").textContent).toBe(
      "Confirm answer",
    );
  });

  it("renders a notice for an invalid set or a malformed reference", async () => {
    serveSet({ ...set([]), invalid: { code: "invalid-json", message: "bad" } });
    renderApp(<ReferencedQuestionCard content={REF} workspaceId={WS} />);
    expect(
      await screen.findByTestId("referenced-question-set-invalid"),
    ).toBeTruthy();
  });

  it("refuses a malformed reference without a request", () => {
    renderApp(
      <ReferencedQuestionCard
        content={'{"set":"plan","id":"x"}'}
        workspaceId={WS}
      />,
    );
    expect(screen.getByTestId("referenced-question-invalid-ref")).toBeTruthy();
  });
});

// ---------------------------------------------------------------------------
// Meeting decisions become issues (g5.workbench.meeting)
// ---------------------------------------------------------------------------

const MEETING_OPTIONS = meetingOptions();
const TARGET_URL = "/api/v1/orgs/:orgId/issues/create-target";
const CREATE_URL = "/api/v1/orgs/:orgId/issues";

function meetingEntry(overrides: Partial<QuestionEntry> = {}): QuestionEntry {
  return entry({
    id: "timeline-landing-page",
    prompt: "The project timeline becomes the landing page",
    context:
      "Detail: the overview page goes away. Decided by Tim at 00:28. Target: docs/ws/requirements.md",
    options: MEETING_OPTIONS,
    ...overrides,
  });
}

function meetingSet(questions: QuestionEntry[], answerable = true) {
  return {
    ...set(questions, answerable),
    setId: "meeting-kickoff-2026-10-15",
    title: "Meeting kickoff 2026-10-15",
  };
}

function serveReadyTarget() {
  server.use(
    http.get(TARGET_URL, () =>
      HttpResponse.json({ data: makeIssueCreateTarget() }),
    ),
  );
}

function serveCreate() {
  const bodies: Array<Record<string, unknown>> = [];
  server.use(
    http.post(CREATE_URL, async ({ request }) => {
      bodies.push((await request.json()) as Record<string, unknown>);
      return HttpResponse.json(
        {
          data: makeIssueCreation({
            title: "The project timeline becomes the landing page",
          }),
        },
        { status: 201 },
      );
    }),
  );
  return bodies;
}

/** The service's answer to a successful save: the entry at the next revision. */
function answeredResponse(body: Record<string, unknown>) {
  const revision = (body.expectedRevision as number) + 1;
  return {
    data: {
      path: PATH,
      entry: meetingEntry({
        revision,
        status: "answered",
        decision: true,
        answer: {
          selectedOptionIds: body.selectedOptionIds as string[],
          text: body.text as string,
          rationale: "",
          answeredBy: { kind: "user", id: "u1", email: "owner@example.test" },
          answeredAt: "2026-10-15T10:00:00Z",
          answeredAtRevision: revision,
          version: 1,
        },
      }),
      replayed: false,
      hash: `h${revision}`,
      vmId: "vm",
      publication: { status: "published" },
    },
  };
}

function serveAnswer(answer: "ok" | "refuse" = "ok") {
  const bodies: Record<string, unknown>[] = [];
  server.use(
    http.post(
      `*/api/v1/workspaces/${WS}/questions/answer`,
      async ({ request }) => {
        const body = (await request.json()) as Record<string, unknown>;
        bodies.push(body);
        if (answer === "refuse") {
          return HttpResponse.json(
            {
              error: {
                code: "CONFLICT",
                message: "the workspace has no running VM",
                details: { code: "vm_not_running" },
                requestId: "",
              },
            },
            { status: 409 },
          );
        }
        return HttpResponse.json(answeredResponse(body));
      },
    ),
    http.post(
      `*/api/v1/workspaces/${WS}/questions/clear`,
      async ({ request }) => {
        const body = (await request.json()) as Record<string, unknown>;
        bodies.push({ ...body, cleared: true });
        const revision = (body.expectedRevision as number) + 1;
        return HttpResponse.json({
          data: {
            path: PATH,
            entry: meetingEntry({ revision }),
            replayed: false,
            hash: `h${revision}`,
            vmId: "vm",
            publication: { status: "published" },
          },
        });
      },
    ),
  );
  return bodies;
}

function renderMeetingCard(id = "timeline-landing-page") {
  return renderApp(
    <Routes>
      <Route
        path="/:org/:workspace"
        element={
          <ReferencedQuestionCard
            content={JSON.stringify({ path: PATH, id })}
            workspaceId={WS}
          />
        }
      />
    </Routes>,
    { route: "/g5labs/current", user: createMockUser() },
  );
}

describe("ReferencedQuestionCard — meeting decisions become issues", () => {
  beforeEach(() => {
    serveVotes();
    serveExternalIssuesFeature();
  });

  it("offers Create issue only for a meeting set with Delay or Out of scope selected, disabled with the target's reason", async () => {
    serveSet(meetingSet([meetingEntry()]));
    serveAnswer();
    renderMeetingCard();
    const options = await screen.findAllByTestId("referenced-question-option");
    expect(
      screen.queryByTestId("referenced-question-create-issue"),
    ).not.toBeInTheDocument();
    await userEvent.click(options[0]); // Accept
    expect(
      screen.queryByTestId("referenced-question-create-issue"),
    ).not.toBeInTheDocument();
    await userEvent.click(options[1]); // Delay
    const create = await screen.findByTestId(
      "referenced-question-create-issue",
    );
    // The default target answer: no repository for new issues.
    await waitFor(() =>
      expect(create).toHaveAttribute(
        "title",
        "No repository is set up for new issues",
      ),
    );
    expect(create).toBeDisabled();
    await userEvent.click(options[2]); // Out of scope
    expect(
      screen.getByTestId("referenced-question-create-issue"),
    ).toBeInTheDocument();
    await userEvent.click(options[3]); // Reject
    expect(
      screen.queryByTestId("referenced-question-create-issue"),
    ).not.toBeInTheDocument();
  });

  it("offers nothing on an ordinary set, a reader's card, or when external issues are off", async () => {
    serveReadyTarget();
    const plain = set([entry({ options: MEETING_OPTIONS })]);
    serveSet(plain);
    const first = renderMeetingCard("pick");
    const options = await screen.findAllByTestId("referenced-question-option");
    await userEvent.click(options[1]);
    expect(
      screen.queryByTestId("referenced-question-create-issue"),
    ).not.toBeInTheDocument();
    first.unmount();

    serveSet(
      meetingSet(
        [
          meetingEntry({
            status: "answered",
            decision: true,
            answer: {
              selectedOptionIds: ["delay"],
              text: "",
              rationale: "",
              answeredBy: {
                kind: "user",
                id: "u1",
                email: "owner@example.test",
              },
              answeredAt: "2026-10-15T10:00:00Z",
              answeredAtRevision: 1,
              version: 1,
            },
          }),
        ],
        false,
      ),
    );
    const reader = renderMeetingCard();
    await screen.findByTestId("referenced-question-read-only");
    expect(
      screen.queryByTestId("referenced-question-create-issue"),
    ).not.toBeInTheDocument();
    reader.unmount();
  });

  it("creates the issue pre-filled from the decision and answers with its reference in the comment", async () => {
    serveSet(meetingSet([meetingEntry()]));
    serveReadyTarget();
    const creations = serveCreate();
    const answers = serveAnswer();
    const user = userEvent.setup();
    renderMeetingCard();
    const options = await screen.findAllByTestId("referenced-question-option");
    await user.click(options[1]); // Delay
    await user.type(
      screen.getByTestId("referenced-question-text"),
      "after the launch",
    );
    const create = await screen.findByTestId(
      "referenced-question-create-issue",
    );
    await waitFor(() => expect(create).toBeEnabled());
    await user.click(create);
    expect(await screen.findByTestId("create-issue-title")).toHaveValue(
      "The project timeline becomes the landing page",
    );
    expect(screen.getByTestId("create-issue-body")).toHaveValue(
      "Detail: the overview page goes away. Decided by Tim at 00:28. Target: docs/ws/requirements.md\n\nDecision: Delay — after the launch\n\nFrom meeting: Meeting kickoff 2026-10-15",
    );
    expect(
      screen.queryByTestId("create-issue-submit-fork"),
    ).not.toBeInTheDocument();
    await user.click(screen.getByTestId("create-issue-submit"));

    await waitFor(() => expect(answers).toHaveLength(2));
    expect(creations[0]).toMatchObject({
      title: "The project timeline becomes the landing page",
    });
    expect(creations[0]).not.toHaveProperty("parent");
    // The creation answers again: the decision with the reference appended.
    expect(answers[1]).toMatchObject({
      path: PATH,
      questionId: "timeline-landing-page",
      expectedRevision: 2,
      selectedOptionIds: ["delay"],
      text: "after the launch\nacme/api#77 https://github.com/acme/api/issues/77",
    });
    expect(await screen.findByTestId("referenced-question-saved")).toBeTruthy();
    expect(
      screen.getByTestId("referenced-question-created-issue"),
    ).toHaveTextContent(
      "Created acme/api#77; its reference is in the comment.",
    );
    expect(screen.getByTestId("referenced-question-text")).toHaveValue(
      "after the launch\nacme/api#77 https://github.com/acme/api/issues/77",
    );
  });

  it("keeps the created issue's reference in the comment when the answer is refused", async () => {
    serveSet(meetingSet([meetingEntry()]));
    serveReadyTarget();
    serveCreate();
    const answers = serveAnswer("refuse");
    const user = userEvent.setup();
    renderMeetingCard();
    const options = await screen.findAllByTestId("referenced-question-option");
    await user.click(options[2]); // Out of scope: the one-press save is refused
    expect(
      await screen.findByTestId("referenced-question-notice"),
    ).toHaveTextContent("the workspace has no running VM");
    expect(answers).toHaveLength(1);
    const create = await screen.findByTestId(
      "referenced-question-create-issue",
    );
    await waitFor(() => expect(create).toBeEnabled());
    await user.click(create);
    await user.click(await screen.findByTestId("create-issue-submit"));
    await waitFor(() => expect(answers).toHaveLength(2));
    expect(
      await screen.findByTestId("referenced-question-notice"),
    ).toHaveTextContent("the workspace has no running VM");
    expect(
      screen.getByTestId("referenced-question-created-issue"),
    ).toHaveTextContent("save the answer to record it");
    expect(screen.getByTestId("referenced-question-text")).toHaveValue(
      "acme/api#77 https://github.com/acme/api/issues/77",
    );
    // The owner saves again by hand.
    expect(screen.getByTestId("referenced-question-answer")).toBeEnabled();
  });

  it("keeps the created issue's reference when the save finds a moved revision", async () => {
    serveSet(meetingSet([meetingEntry()]));
    serveReadyTarget();
    serveCreate();
    let calls = 0;
    const moved = meetingEntry({
      revision: 3,
      status: "answered",
      decision: true,
      answer: {
        selectedOptionIds: ["delay"],
        text: "reworded elsewhere",
        rationale: "",
        answeredBy: { kind: "user", id: "u1", email: "owner@example.test" },
        answeredAt: "2026-10-15T10:00:00Z",
        answeredAtRevision: 3,
        version: 2,
      },
    });
    server.use(
      http.post(
        `*/api/v1/workspaces/${WS}/questions/answer`,
        async ({ request }) => {
          const body = (await request.json()) as Record<string, unknown>;
          calls += 1;
          if (calls === 1) return HttpResponse.json(answeredResponse(body));
          return revisionMovedRefusal(moved);
        },
      ),
    );
    const user = userEvent.setup();
    renderMeetingCard();
    const options = await screen.findAllByTestId("referenced-question-option");
    await user.click(options[1]); // Delay
    await screen.findByTestId("referenced-question-saved");
    const create = await screen.findByTestId(
      "referenced-question-create-issue",
    );
    await waitFor(() => expect(create).toBeEnabled());
    await user.click(create);
    await user.click(await screen.findByTestId("create-issue-submit"));
    expect(
      await screen.findByTestId("referenced-question-notice"),
    ).toHaveTextContent("This question changed since you opened it");
    // The moved revision's text is shown with the reference kept, so the
    // owner can save it; the note still asks for that save.
    expect(screen.getByTestId("referenced-question-text")).toHaveValue(
      "reworded elsewhere\nacme/api#77 https://github.com/acme/api/issues/77",
    );
    expect(
      screen.getByTestId("referenced-question-created-issue"),
    ).toHaveTextContent("save the answer to record it");
    expect(screen.getByTestId("referenced-question-answer")).toBeEnabled();
  });

  it("stops restoring the reference once a saved answer carries it", async () => {
    serveSet(meetingSet([meetingEntry()]));
    serveReadyTarget();
    serveCreate();
    const answers = serveAnswer();
    const user = userEvent.setup();
    renderMeetingCard();
    const options = await screen.findAllByTestId("referenced-question-option");
    await user.click(options[1]); // Delay
    await screen.findByTestId("referenced-question-saved");
    const create = await screen.findByTestId(
      "referenced-question-create-issue",
    );
    await waitFor(() => expect(create).toBeEnabled());
    await user.click(create);
    await user.click(await screen.findByTestId("create-issue-submit"));
    await waitFor(() => expect(answers).toHaveLength(2));
    await waitFor(() =>
      expect(
        screen.getByTestId("referenced-question-created-issue"),
      ).toHaveTextContent("its reference is in the comment."),
    );
    // Clearing the answer empties the comment; the reference is not put back.
    await user.click(screen.getByTestId("referenced-question-clear"));
    await waitFor(() => expect(answers).toHaveLength(3));
    expect(answers[2]).toMatchObject({ cleared: true });
    await waitFor(() =>
      expect(screen.getByTestId("referenced-question-text")).toHaveValue(""),
    );
  });

  it("lets the saved option be reselected after a refused change", async () => {
    serveSet(
      meetingSet([
        meetingEntry({
          revision: 2,
          status: "answered",
          decision: true,
          answer: {
            selectedOptionIds: ["delay"],
            text: "",
            rationale: "",
            answeredBy: { kind: "user", id: "u1", email: "owner@example.test" },
            answeredAt: "2026-10-15T10:00:00Z",
            answeredAtRevision: 2,
            version: 1,
          },
        }),
      ]),
    );
    const answers = serveAnswer("refuse");
    const user = userEvent.setup();
    renderMeetingCard();
    const options = await screen.findAllByTestId("referenced-question-option");
    await user.click(options[3]); // Reject: refused
    await screen.findByTestId("referenced-question-notice");
    expect(answers).toHaveLength(1);
    expect(options[3]).toHaveAttribute("aria-checked", "true");
    // The saved option comes back locally without another write.
    await user.click(options[1]);
    expect(options[1]).toHaveAttribute("aria-checked", "true");
    expect(options[3]).toHaveAttribute("aria-checked", "false");
    expect(answers).toHaveLength(1);
  });

  it("answers the revision the dialog was opened for, so a question changed meanwhile is a moved revision", async () => {
    serveSet(meetingSet([meetingEntry()]));
    serveReadyTarget();
    serveCreate();
    const answers = serveAnswer();
    const user = userEvent.setup();
    const { queryClient } = renderMeetingCard();
    const options = await screen.findAllByTestId("referenced-question-option");
    await user.click(options[1]); // Delay: revision 1 → 2
    await screen.findByTestId("referenced-question-saved");
    const create = await screen.findByTestId(
      "referenced-question-create-issue",
    );
    await waitFor(() => expect(create).toBeEnabled());
    await user.click(create);
    await screen.findByTestId("create-issue-title");
    // Another tab rewords the question while the dialog is open.
    queryClient.setQueryData(questionSetQueryKey(WS, PATH), (view: unknown) => {
      const current = view as ReturnType<typeof meetingSet>;
      return {
        ...current,
        questions: current.questions.map((question) => ({
          ...question,
          revision: 3,
          prompt: "The timeline becomes the landing page (reworded)",
        })),
      };
    });
    await user.click(await screen.findByTestId("create-issue-submit"));
    await waitFor(() => expect(answers).toHaveLength(2));
    expect(answers[1]).toMatchObject({
      expectedRevision: 2,
      selectedOptionIds: ["delay"],
    });
  });

  it("keeps an open Create issue dialog when another tab changes the decision", async () => {
    serveSet(meetingSet([meetingEntry()]));
    serveReadyTarget();
    serveCreate();
    const answers = serveAnswer();
    const user = userEvent.setup();
    const { queryClient } = renderMeetingCard();
    const options = await screen.findAllByTestId("referenced-question-option");
    await user.click(options[1]); // Delay: revision 1 → 2
    await screen.findByTestId("referenced-question-saved");
    const create = await screen.findByTestId(
      "referenced-question-create-issue",
    );
    await waitFor(() => expect(create).toBeEnabled());
    await user.click(create);
    await screen.findByTestId("create-issue-title");
    // Another tab accepts the decision while the dialog is open.
    queryClient.setQueryData(questionSetQueryKey(WS, PATH), (view: unknown) => {
      const current = view as ReturnType<typeof meetingSet>;
      return {
        ...current,
        questions: current.questions.map((question) => ({
          ...question,
          revision: 3,
          answer: { ...question.answer, selectedOptionIds: ["accept"] },
        })),
      };
    });
    // The Create issue button is gone with Delay, but the opened dialog stays.
    await waitFor(() =>
      expect(
        screen.queryByTestId("referenced-question-create-issue"),
      ).not.toBeInTheDocument(),
    );
    expect(screen.getByTestId("create-issue-title")).toBeInTheDocument();
    await user.click(screen.getByTestId("create-issue-submit"));
    await waitFor(() => expect(answers).toHaveLength(2));
    expect(answers[1]).toMatchObject({
      expectedRevision: 2,
      selectedOptionIds: ["delay"],
    });
  });
});

// ---------------------------------------------------------------------------
// Meeting decisions become tasks (g5.workbench.meeting, task option set)
// ---------------------------------------------------------------------------

const TASK_OPTIONS = meetingTaskOptions();
const LINKS_URL = `*/api/v1/workspaces/${WS}/issue-links`;
const RESOLVE_URL = "/api/v1/orgs/:orgId/issues/resolve";
const ISSUE = {
  displayKey: "acme/api#77",
  url: "https://github.com/acme/api/issues/77",
};

function taskEntry(overrides: Partial<QuestionEntry> = {}): QuestionEntry {
  return meetingEntry({ options: TASK_OPTIONS, ...overrides });
}

function taskSet(questions: QuestionEntry[], answerable = true) {
  return { ...meetingSet(questions, answerable), setId: "meeting-standup" };
}

/** The service's answer to a save on a task card: the task options stay. */
function serveTaskAnswer(answer: "ok" | "refuse" = "ok") {
  const bodies: Record<string, unknown>[] = [];
  server.use(
    http.post(
      `*/api/v1/workspaces/${WS}/questions/answer`,
      async ({ request }) => {
        const body = (await request.json()) as Record<string, unknown>;
        bodies.push(body);
        if (answer === "refuse") {
          return HttpResponse.json(
            {
              error: {
                code: "CONFLICT",
                message: "the workspace has no running VM",
                details: { code: "vm_not_running" },
                requestId: "",
              },
            },
            { status: 409 },
          );
        }
        const revision = (body.expectedRevision as number) + 1;
        return HttpResponse.json({
          data: {
            path: PATH,
            entry: taskEntry({
              revision,
              status: "answered",
              decision: true,
              answer: {
                selectedOptionIds: body.selectedOptionIds as string[],
                text: body.text as string,
                rationale: "",
                answeredBy: {
                  kind: "user",
                  id: "u1",
                  email: "owner@example.test",
                },
                answeredAt: "2026-10-22T10:00:00Z",
                answeredAtRevision: revision,
                version: 1,
              },
            }),
            replayed: false,
            hash: `h${revision}`,
            vmId: "vm",
            publication: { status: "published" },
          },
        });
      },
    ),
  );
  return bodies;
}

/** The workspace's links (a primary issue in the creation repository by default) and the link write. */
function serveLinks(
  links: WorkspaceIssueLinkDTO[] = [makeIssueLink()],
  linkStatus = 201,
) {
  const linkBodies: unknown[] = [];
  server.use(
    http.get(LINKS_URL, () => HttpResponse.json({ data: { links } })),
    http.post(LINKS_URL, async ({ request }) => {
      linkBodies.push(await request.json());
      return linkStatus === 201
        ? HttpResponse.json({ data: { id: "link-77" } }, { status: 201 })
        : HttpResponse.json(
            { error: { code: "INTERNAL", message: "link failed" } },
            { status: linkStatus },
          );
    }),
  );
  return linkBodies;
}

describe("ReferencedQuestionCard — meeting decisions become tasks", () => {
  beforeEach(() => {
    serveVotes();
    serveExternalIssuesFeature();
  });

  it("shows the three options without vote controls, with the task options disabled by the target's reason", async () => {
    serveSet(taskSet([taskEntry()]));
    renderMeetingCard();
    const options = await screen.findAllByTestId("referenced-question-option");
    expect(options.map((option) => option.textContent)).toEqual([
      "#1Create a task and attach to the current workspace",
      "#2Create a detached task",
      "#3Reject",
    ]);
    expect(
      screen.queryByTestId("referenced-question-vote-controls"),
    ).not.toBeInTheDocument();
    expect(screen.getByTestId("referenced-question-text")).toHaveAttribute(
      "placeholder",
      "Add a comment (optional)",
    );
    // The default target answer: no repository for new issues.
    await waitFor(() =>
      expect(options[0]).toHaveAttribute(
        "title",
        "No repository is set up for new issues",
      ),
    );
    expect(options[0]).toBeDisabled();
    expect(options[1]).toBeDisabled();
    expect(options[2]).toBeEnabled();
  });

  it("attach creates the task under the primary issue, records the decision with its reference, links it and announces it", async () => {
    serveSet(taskSet([taskEntry()]));
    serveReadyTarget();
    const creations = serveCreate();
    const answers = serveTaskAnswer();
    const linkBodies = serveLinks();
    const user = userEvent.setup();
    renderMeetingCard();
    const options = await screen.findAllByTestId("referenced-question-option");
    await waitFor(() => expect(options[0]).toBeEnabled());
    await user.type(screen.getByTestId("referenced-question-text"), "land it");
    await user.click(options[0]);

    await waitFor(() => expect(creations).toHaveLength(1));
    expect(creations[0]).toMatchObject({
      title: "The project timeline becomes the landing page",
      body: "Detail: the overview page goes away. Decided by Tim at 00:28. Target: docs/ws/requirements.md\n\nDecision: land it\n\nFrom meeting: Meeting kickoff 2026-10-15",
      parent: "acme/api#12",
      checkOnly: false,
    });
    await waitFor(() => expect(answers).toHaveLength(1));
    expect(answers[0]).toMatchObject({
      questionId: "timeline-landing-page",
      expectedRevision: 1,
      selectedOptionIds: ["attach"],
      text: `land it\n${ISSUE.displayKey} ${ISSUE.url}`,
    });
    await waitFor(() =>
      expect(linkBodies).toEqual([{ ref: "acme/api#77", role: "related" }]),
    );
    expect(
      await screen.findByTestId("referenced-question-task-status"),
    ).toHaveTextContent("Task acme/api#77 attached to this workspace.");
    expect(
      screen.getByTestId("referenced-question-created-issue"),
    ).toHaveTextContent("Attached to this workspace.");
    expect(screen.getByTestId("referenced-question-saved")).toBeTruthy();
    expect(screen.queryByTestId("create-issue-dialog")).not.toBeInTheDocument();
  });

  it("detach creates without a parent, records the decision and never links", async () => {
    serveSet(taskSet([taskEntry()]));
    serveReadyTarget();
    const creations = serveCreate();
    const answers = serveTaskAnswer();
    const linkBodies = serveLinks([]);
    const user = userEvent.setup();
    renderMeetingCard();
    const options = await screen.findAllByTestId("referenced-question-option");
    await waitFor(() => expect(options[1]).toBeEnabled());
    await user.click(options[1]);
    await waitFor(() => expect(answers).toHaveLength(1));
    expect(creations[0]).not.toHaveProperty("parent");
    expect(answers[0]).toMatchObject({ selectedOptionIds: ["detach"] });
    expect(
      await screen.findByTestId("referenced-question-task-status"),
    ).toHaveTextContent(
      "Task acme/api#77 created, not linked to this workspace.",
    );
    expect(linkBodies).toEqual([]);
    expect(
      screen.getByTestId("referenced-question-created-issue"),
    ).toHaveTextContent("Not linked to this workspace.");
  });

  it("a refused link keeps the issue and the recorded decision and says so", async () => {
    serveSet(taskSet([taskEntry()]));
    serveReadyTarget();
    serveCreate();
    const answers = serveTaskAnswer();
    serveLinks([makeIssueLink()], 500);
    const user = userEvent.setup();
    renderMeetingCard();
    const options = await screen.findAllByTestId("referenced-question-option");
    await waitFor(() => expect(options[0]).toBeEnabled());
    await user.click(options[0]);
    expect(
      await screen.findByTestId("referenced-question-link-failed"),
    ).toHaveTextContent("could not link it to this workspace");
    expect(answers).toHaveLength(1);
    expect(
      screen.getByTestId("referenced-question-task-status"),
    ).toHaveTextContent("linking it to this workspace failed");
  });

  it("an untrusted destination reviews the payload in the Create issue dialog, then the press completes", async () => {
    serveSet(taskSet([taskEntry()]));
    server.use(
      http.get(TARGET_URL, () =>
        HttpResponse.json({
          data: makeIssueCreateTarget({
            destination: {
              ...makeIssueCreateTarget().destination!,
              contentTrusted: false,
            },
          }),
        }),
      ),
    );
    server.use(
      http.get(RESOLVE_URL, () =>
        HttpResponse.json({ data: makeIssueSummary({ title: "Release 2.0" }) }),
      ),
    );
    const creations = serveCreate();
    const answers = serveTaskAnswer();
    const linkBodies = serveLinks();
    const user = userEvent.setup();
    renderMeetingCard();
    const options = await screen.findAllByTestId("referenced-question-option");
    await waitFor(() => expect(options[0]).toBeEnabled());
    await user.click(options[0]);
    const dialog = await screen.findByTestId("create-issue-dialog");
    expect(within(dialog).getByTestId("create-issue-title")).toHaveValue(
      "The project timeline becomes the landing page",
    );
    expect(within(dialog).getByTestId("create-issue-parent")).toHaveValue(
      "acme/api#12",
    );
    expect(creations).toHaveLength(0);
    // The untrusted review step shows the exact payload; Create then sends.
    const review = within(dialog).getByTestId("create-issue-review-button");
    await waitFor(() => expect(review).toBeEnabled());
    await user.click(review);
    await within(dialog).findByTestId("create-issue-review");
    expect(creations).toHaveLength(0);
    await user.click(within(dialog).getByTestId("create-issue-submit"));
    await waitFor(() => expect(answers).toHaveLength(1));
    expect(answers[0]).toMatchObject({ selectedOptionIds: ["attach"] });
    await waitFor(() =>
      expect(linkBodies).toEqual([{ ref: "acme/api#77", role: "related" }]),
    );
  });

  it("a decision whose comment already names an issue creates no second one: attach links it, detach only re-records", async () => {
    const recorded = `after the launch\n${ISSUE.displayKey} ${ISSUE.url}`;
    serveSet(
      taskSet([
        taskEntry({
          revision: 3,
          status: "answered",
          decision: true,
          answer: {
            selectedOptionIds: ["detach"],
            text: recorded,
            rationale: "",
            answeredBy: { kind: "user", id: "u1", email: "owner@example.test" },
            answeredAt: "2026-10-22T10:00:00Z",
            answeredAtRevision: 3,
            version: 1,
          },
        }),
      ]),
    );
    serveReadyTarget();
    const creations = serveCreate();
    const answers = serveTaskAnswer();
    const linkBodies = serveLinks();
    const user = userEvent.setup();
    renderMeetingCard();
    const options = await screen.findAllByTestId("referenced-question-option");
    await waitFor(() => expect(options[0]).toBeEnabled());
    await user.click(options[0]);
    await waitFor(() =>
      expect(linkBodies).toEqual([{ ref: "acme/api#77", role: "related" }]),
    );
    expect(creations).toHaveLength(0);
    expect(answers[0]).toMatchObject({
      selectedOptionIds: ["attach"],
      text: recorded,
      expectedRevision: 3,
    });
  });

  it("Reject records the decision and creates nothing", async () => {
    serveSet(taskSet([taskEntry()]));
    serveReadyTarget();
    const creations = serveCreate();
    const answers = serveTaskAnswer();
    serveLinks();
    const user = userEvent.setup();
    renderMeetingCard();
    const options = await screen.findAllByTestId("referenced-question-option");
    await user.click(options[2]);
    await waitFor(() => expect(answers).toHaveLength(1));
    expect(answers[0]).toMatchObject({ selectedOptionIds: ["reject"] });
    expect(creations).toHaveLength(0);
    expect(await screen.findByTestId("referenced-question-saved")).toBeTruthy();
  });

  it("a reader's task card is read-only: options disabled, no votes", async () => {
    serveSet(taskSet([taskEntry()], false));
    renderMeetingCard();
    const options = await screen.findAllByTestId("referenced-question-option");
    for (const option of options) expect(option).toBeDisabled();
    expect(
      screen.queryByTestId("referenced-question-vote-controls"),
    ).not.toBeInTheDocument();
    expect(
      screen.getByTestId("referenced-question-read-only"),
    ).toHaveTextContent("The workspace owner decides here");
  });
});
