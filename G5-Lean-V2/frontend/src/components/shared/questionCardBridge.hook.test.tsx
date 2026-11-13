import { act, fireEvent, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { HttpResponse, http } from "msw";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { renderApp } from "../../test/render";
import { server } from "../../test/msw/server";
import {
  PATH,
  WS,
  entry,
  serveSet,
  serveVotes,
  revisionMovedRefusal,
  set,
  unknownOptionRefusal as refused,
  vote,
} from "../../test/questionFixtures";
import { QuestionAnswerBar, useQuestionCardBridge } from "./questionCardBridge";
import {
  QUESTION_BRIDGE_VERSION,
  QUESTION_MSG_SOURCE_CHILD,
  QUESTION_MSG_SOURCE_HOST,
  type QuestionCardsPayload,
} from "./questionCardRenderer";

const KEY = `${PATH}#pick`;
const HTML = `<div data-g5-question="${KEY}"></div>`;
const NOT_SAVED = "Vote not saved: option gone is not offered";

const recorded = () =>
  HttpResponse.json({ data: { path: PATH, vote: vote("me", ["file"]) } });
function Harness() {
  const bridge = useQuestionCardBridge(WS, HTML, true);
  return (
    <>
      {bridge.bar && (
        <QuestionAnswerBar
          state={bridge.bar}
          onConfirm={bridge.confirm}
          onDismiss={bridge.dismiss}
        />
      )}
      <iframe
        title="explainer"
        data-testid="frame"
        ref={bridge.frame?.setFrame}
        onLoad={bridge.frame?.onLoad}
      />
    </>
  );
}

interface HostPost {
  source: string;
  generation: string;
  cards: QuestionCardsPayload;
  statuses: Record<string, string>;
}

/** The card's data once loaded, or undefined. */
function okCard(post: HostPost | undefined) {
  const card = post?.cards[KEY];
  return card?.state === "ok" ? card : undefined;
}

/** The card's vote total once the tally is loaded, or undefined. */
function total(post: HostPost | undefined) {
  const votes = okCard(post)?.votes;
  return votes && "total" in votes ? votes.total : undefined;
}

/** Mount the bridge and wait until the frame holds the card's data. */
async function mount() {
  const { queryClient } = renderApp(<Harness />);
  const frame = screen.getByTestId("frame") as HTMLIFrameElement;
  const target = frame.contentWindow!;
  const posted = vi.spyOn(target, "postMessage").mockImplementation(() => {});
  const hostPosts = () =>
    posted.mock.calls
      .map(([data]) => data as HostPost)
      .filter((data) => data.source === QUESTION_MSG_SOURCE_HOST);
  const last = () => hostPosts().at(-1);
  await waitFor(() => expect(okCard(last())).toBeDefined());
  const propose = (fields: Record<string, unknown>) =>
    act(() => {
      window.dispatchEvent(
        new MessageEvent("message", {
          source: target,
          data: {
            source: QUESTION_MSG_SOURCE_CHILD,
            version: QUESTION_BRIDGE_VERSION,
            generation: last()!.generation,
            requestId: `r${Math.random().toString(36).slice(2)}`,
            ref: KEY,
            expectedRevision: 1,
            withdraw: false,
            selectedOptionIds: ["file"],
            text: "",
            rationale: "",
            ...fields,
          },
        }),
      );
    });
  const lastStatus = () => last()?.statuses[KEY];
  return { frame, last, lastStatus, propose, queryClient };
}

/** A vote route that answers only when released. */
function heldVote(reply: () => Response) {
  let release!: () => void;
  const released = new Promise<void>((resolve) => (release = resolve));
  server.use(
    http.post(`*/api/v1/workspaces/${WS}/questions/vote`, async () => {
      await released;
      return reply();
    }),
  );
  return release;
}

describe("useQuestionCardBridge votes", () => {
  beforeEach(() => serveVotes());

  it("records a frame vote with one press and no host bar", async () => {
    serveSet(set([entry()], false));
    const bodies: unknown[] = [];
    server.use(
      http.post(
        `*/api/v1/workspaces/${WS}/questions/vote`,
        async ({ request }) => {
          bodies.push(await request.json());
          return recorded();
        },
      ),
    );
    const { propose, lastStatus } = await mount();
    // A second press while the first is in flight is dropped.
    propose({ type: "propose-question-vote" });
    propose({ type: "propose-question-vote" });
    await waitFor(() => expect(lastStatus()).toBe("Vote recorded."));
    expect(bodies).toEqual([
      {
        path: PATH,
        questionId: "pick",
        expectedRevision: 1,
        selectedOptionIds: ["file"],
        text: "",
      },
    ]);
    expect(screen.queryByTestId("question-answer-bar")).toBeNull();
  });

  it("withdraws a vote with one press", async () => {
    serveSet(set([entry()], false));
    const bodies: unknown[] = [];
    server.use(
      http.post(
        `*/api/v1/workspaces/${WS}/questions/unvote`,
        async ({ request }) => {
          bodies.push(await request.json());
          return HttpResponse.json({
            data: { path: PATH, withdrawn: true },
          });
        },
      ),
    );
    const { propose, lastStatus } = await mount();
    propose({ type: "propose-question-vote", withdraw: true });
    await waitFor(() => expect(lastStatus()).toBe("Vote withdrawn."));
    expect(bodies).toEqual([{ path: PATH, questionId: "pick" }]);
    expect(screen.queryByTestId("question-answer-bar")).toBeNull();
  });

  it("shows a refused vote in the bar with a retry", async () => {
    serveSet(set([entry()], false));
    let refuse = true;
    server.use(
      http.post(`*/api/v1/workspaces/${WS}/questions/vote`, () =>
        refuse ? refused() : recorded(),
      ),
    );
    const { propose, lastStatus } = await mount();
    propose({ type: "propose-question-vote" });
    const bar = await screen.findByTestId("question-answer-bar");
    expect(bar).toHaveAttribute("data-action", "vote");
    expect(bar.textContent).toContain("Your vote on Q1");
    expect(bar.textContent).toContain("option gone is not offered");
    expect(lastStatus()).toBe(NOT_SAVED);
    // The retry saves and clears the refused vote from the bar.
    refuse = false;
    await userEvent.click(screen.getByTestId("question-answer-confirm"));
    await waitFor(() => expect(lastStatus()).toBe("Vote recorded."));
    await waitFor(() =>
      expect(screen.queryByTestId("question-answer-bar")).toBeNull(),
    );
  });

  it("shows the current entry, not a doomed retry, when the question changed", async () => {
    serveSet(set([entry()], false));
    const replies = [
      () => refused(),
      () =>
        revisionMovedRefusal(entry({ revision: 2, prompt: "Which store?" })),
    ];
    server.use(
      http.post(`*/api/v1/workspaces/${WS}/questions/vote`, () =>
        replies.shift()!(),
      ),
    );
    const { last, lastStatus, propose } = await mount();
    propose({ type: "propose-question-vote" });
    await screen.findByTestId("question-answer-bar");
    // The bar's retry meets a moved revision: the bar closes and the card
    // shows the current entry to vote on again.
    await userEvent.click(screen.getByTestId("question-answer-confirm"));
    await waitFor(() =>
      expect(lastStatus()).toBe("The question changed; vote again."),
    );
    await waitFor(() => expect(okCard(last())?.entry.revision).toBe(2));
    expect(screen.queryByTestId("question-answer-bar")).toBeNull();
  });

  it("settles a slow vote against the cards current then", async () => {
    serveSet(set([entry()], false));
    const release = heldVote(refused);
    const { last, lastStatus, propose, queryClient } = await mount();
    await waitFor(() => expect(total(last())).toBe(0));
    propose({ type: "propose-question-vote" });
    // Another reader's vote arrives while this one is in flight.
    serveVotes({ votes: [vote("a1", ["db"])] });
    await act(() => queryClient.invalidateQueries());
    await waitFor(() => expect(total(last())).toBe(1));
    release();
    await waitFor(() => expect(lastStatus()).toBe(NOT_SAVED));
    expect(total(last())).toBe(1);
  });

  it("reports a vote from a reloaded frame on the card, never in the new bar", async () => {
    serveSet(set([entry()], false));
    const release = heldVote(refused);
    const { frame, last, lastStatus, propose } = await mount();
    propose({ type: "propose-question-vote" });
    expect(lastStatus()).toBe("Saving your vote…");
    const before = last()!.generation;
    fireEvent.load(frame);
    expect(last()!.generation).not.toBe(before);
    // A press from the new load for the same card waits for the old save.
    propose({ type: "propose-question-vote" });
    release();
    // The outcome reaches the new load's card; the old load's refusal opens
    // no bar there.
    await waitFor(() => expect(lastStatus()).toBe(NOT_SAVED));
    expect(last()!.generation).not.toBe(before);
    expect(screen.queryByTestId("question-answer-bar")).toBeNull();
  });

  it("keeps an answer awaiting confirmation and names a refused vote on the card", async () => {
    serveSet(set([entry()], true));
    const answered = vi.fn();
    server.use(
      http.post(`*/api/v1/workspaces/${WS}/questions/answer`, () => {
        answered();
        return HttpResponse.json({}, { status: 500 });
      }),
      http.post(`*/api/v1/workspaces/${WS}/questions/vote`, refused),
    );
    const { lastStatus, propose } = await mount();
    propose({ type: "propose-question-answer" });
    const bar = await screen.findByTestId("question-answer-bar");
    expect(bar).toHaveAttribute("data-action", "answer");
    expect(bar.textContent).toContain("This page proposes answering Q1");
    expect(answered).not.toHaveBeenCalled();
    propose({ type: "propose-question-vote" });
    await waitFor(() => expect(lastStatus()).toBe(NOT_SAVED));
    expect(screen.getByTestId("question-answer-bar")).toHaveAttribute(
      "data-action",
      "answer",
    );
  });
});
