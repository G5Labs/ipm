import { afterEach, describe, expect, it, vi } from "vitest";
import {
  QUESTION_MSG_SOURCE_HOST,
  SANDBOXED_QUESTION_CARD_JS,
  type QuestionCardsPayload,
} from "./questionCardRenderer";

const KEY = "docs/ws/questions/plan.g5q.json#pick";

function payload(
  revision: number,
  status = "open",
  overrides: Partial<
    Extract<QuestionCardsPayload[string], { state: "ok" }>
  > = {},
): QuestionCardsPayload {
  return {
    [KEY]: {
      state: "ok",
      answerable: true,
      votable: true,
      setTitle: "Plan",
      votes: { total: 0, viewer: null, byOption: {}, withText: [] },
      ...overrides,
      entry: {
        id: "pick",
        displayNumber: 1,
        revision,
        status: status as "open",
        decision: status === "answered",
        prompt: "<b>Pick</b> one?",
        context: "",
        severity: "",
        multiSelect: false,
        allowFreeText: true,
        options: [
          { id: "a", label: "A", description: "", recommended: true },
          { id: "b", label: "B", description: "", recommended: false },
        ],
        answer: {
          selectedOptionIds: [],
          text: "",
          rationale: "",
          answeredBy: "",
          answeredByKind: "",
          answeredAgainstRevision: null,
        },
      },
    },
  };
}

function hostMessage(
  cards: QuestionCardsPayload,
  statuses?: Record<string, string>,
) {
  window.dispatchEvent(
    new MessageEvent("message", {
      source: window,
      data: {
        source: QUESTION_MSG_SOURCE_HOST,
        version: 1,
        generation: "gen-1",
        type: "question-data",
        cards,
        statuses,
      },
    }),
  );
}

describe("in-frame question card renderer", () => {
  afterEach(() => {
    document.body.innerHTML = "";
  });

  it("renders host data as text, keeps unsent input, and only proposes", () => {
    document.body.innerHTML = `<div data-g5-question="${KEY}"></div>`;
    const post = vi
      .spyOn(window.parent, "postMessage")
      .mockImplementation(() => {});
    new Function(SANDBOXED_QUESTION_CARD_JS)();
    hostMessage(payload(1));
    const card = document.querySelector(`[data-g5-question-card]`)!;
    // Authored text is never parsed as markup.
    expect(card.querySelector("b")).toBeNull();
    expect(card.textContent).toContain("<b>Pick</b> one?");
    const inputs = card.querySelectorAll<HTMLInputElement>(
      "input[data-g5-option]",
    );
    inputs[1].checked = true;
    card.querySelector<HTMLTextAreaElement>("textarea")!.value = "because";
    // Same revision: an in-place update keeps the unsent input.
    hostMessage(payload(1));
    const again = document.querySelector(`[data-g5-question-card]`)!;
    expect(
      again.querySelectorAll<HTMLInputElement>("input[data-g5-option]")[1]
        .checked,
    ).toBe(true);
    expect(again.querySelector<HTMLTextAreaElement>("textarea")!.value).toBe(
      "because",
    );
    again
      .querySelector<HTMLButtonElement>("button[data-g5-question-answer]")!
      .click();
    expect(post).toHaveBeenCalledWith(
      expect.objectContaining({
        type: "propose-question-answer",
        generation: "gen-1",
        ref: KEY,
        expectedRevision: 1,
        selectedOptionIds: ["b"],
        text: "because",
      }),
      "*",
    );
    // An answer waits for the person's confirmation in the host bar.
    expect(document.body.textContent).toContain(
      "Waiting for confirmation in the host bar",
    );
    // A new revision replaces the inputs with the entry's state.
    hostMessage(payload(2, "answered"));
    const fresh = document.querySelector(`[data-g5-question-card]`)!;
    expect(fresh.getAttribute("data-g5-question-revision")).toBe("2");
    expect(
      fresh.querySelectorAll<HTMLInputElement>("input[data-g5-option]")[1]
        .checked,
    ).toBe(false);
    post.mockRestore();
  });

  it("shows the tally to every reader and proposes votes and withdrawals", () => {
    document.body.innerHTML = `<div data-g5-question="${KEY}"></div>`;
    const post = vi
      .spyOn(window.parent, "postMessage")
      .mockImplementation(() => {});
    new Function(SANDBOXED_QUESTION_CARD_JS)();
    hostMessage(
      payload(1, "open", {
        answerable: false,
        votes: {
          total: 2,
          viewer: { selectedOptionIds: ["b"], text: "" },
          byOption: { a: ["Alice"], b: ["Bob (you)"] },
          withText: [{ name: "Alice", text: "<i>keeps</i> it simple" }],
        },
      }),
    );
    const card = document.querySelector(`[data-g5-question-card]`)!;
    expect(card.getAttribute("data-g5-question-votes")).toBe("2");
    expect(card.querySelector("i")).toBeNull();
    expect(card.textContent).toContain("Bob (you)");
    expect(card.textContent).toContain("<i>keeps</i> it simple");
    // A reader's inputs are live for the vote; there is no Answer button.
    expect(card.querySelector("button[data-g5-question-answer]")).toBeNull();
    const inputs = card.querySelectorAll<HTMLInputElement>(
      "input[data-g5-option]",
    );
    expect(inputs[0].disabled).toBe(false);
    // The viewer's own vote seeds the selection.
    expect(inputs[1].checked).toBe(true);
    inputs[0].checked = true;
    card
      .querySelector<HTMLButtonElement>("button[data-g5-question-vote]")!
      .click();
    expect(post).toHaveBeenLastCalledWith(
      expect.objectContaining({
        type: "propose-question-vote",
        withdraw: false,
        ref: KEY,
        expectedRevision: 1,
        selectedOptionIds: ["a"],
      }),
      "*",
    );
    // A vote is saved with this one press; no host-bar confirmation.
    expect(document.body.textContent).toContain("Saving your vote");
    expect(document.body.textContent).not.toContain("host bar");
    document
      .querySelector<HTMLButtonElement>(
        "button[data-g5-question-withdraw-vote]",
      )!
      .click();
    expect(post).toHaveBeenLastCalledWith(
      expect.objectContaining({
        type: "propose-question-vote",
        withdraw: true,
        selectedOptionIds: [],
      }),
      "*",
    );
    expect(document.body.textContent).toContain("Withdrawing your vote");
    // A reader of a workspace that offers no vote sees inert inputs.
    hostMessage(payload(1, "open", { answerable: false, votable: false }));
    const inert = document.querySelector(`[data-g5-question-card]`)!;
    expect(
      inert.querySelector<HTMLInputElement>("input[data-g5-option]")!.disabled,
    ).toBe(true);
    expect(inert.querySelector("button[data-g5-question-vote]")).toBeNull();
    // While the tally loads or cannot be read, no zero count is shown and
    // the vote waits.
    for (const pending of ["loading", "unavailable"] as const) {
      hostMessage(
        payload(1, "open", { answerable: false, votes: { pending } }),
      );
      const waiting = document.querySelector(`[data-g5-question-card]`)!;
      expect(waiting.getAttribute("data-g5-question-votes")).toBe(pending);
      expect(waiting.textContent).not.toContain("No votes yet");
      expect(waiting.querySelector("button[data-g5-question-vote]")).toBeNull();
    }
  });

  it("keeps an empty vote in the frame and says what is missing", () => {
    document.body.innerHTML = `<div data-g5-question="${KEY}"></div>`;
    const post = vi
      .spyOn(window.parent, "postMessage")
      .mockImplementation(() => {});
    new Function(SANDBOXED_QUESTION_CARD_JS)();
    hostMessage(payload(1, "open", { answerable: false }));
    document
      .querySelector<HTMLButtonElement>("button[data-g5-question-vote]")!
      .click();
    expect(post).not.toHaveBeenCalled();
    expect(document.body.textContent).toContain(
      "Pick an option or write a line first.",
    );
    expect(document.body.textContent).not.toContain("Saving your vote");
    // An unrelated refresh keeps the note over an older host status; a new
    // host status for the card replaces it.
    hostMessage(payload(1, "open", { answerable: false }), {
      [KEY]: "Vote recorded.",
    });
    expect(document.body.textContent).not.toContain("Pick an option");
    document
      .querySelector<HTMLButtonElement>("button[data-g5-question-vote]")!
      .click();
    hostMessage(payload(1, "open", { answerable: false }), {
      [KEY]: "Vote recorded.",
    });
    expect(document.body.textContent).toContain(
      "Pick an option or write a line first.",
    );
    hostMessage(payload(1, "open", { answerable: false }), {
      [KEY]: "Vote withdrawn.",
    });
    expect(document.body.textContent).toContain("Vote withdrawn.");
    expect(document.body.textContent).not.toContain("Pick an option");
    expect(post).not.toHaveBeenCalled();
    post.mockRestore();
  });

  it("names the decided option beside a reader's conflicting vote", () => {
    document.body.innerHTML = `<div data-g5-question="${KEY}"></div>`;
    new Function(SANDBOXED_QUESTION_CARD_JS)();
    const data = payload(2, "answered", {
      answerable: false,
      votes: {
        total: 1,
        viewer: { selectedOptionIds: ["b"], text: "" },
        byOption: { a: [], b: ["Bob (you)"] },
        withText: [],
      },
    });
    const card = data[KEY] as Extract<
      QuestionCardsPayload[string],
      { state: "ok" }
    >;
    card.entry.answer.selectedOptionIds = ["a"];
    card.entry.answer.answeredBy = "owner@example.test";
    card.entry.answer.answeredByKind = "user";
    hostMessage(data);
    const rendered = document.querySelector(`[data-g5-question-card]`)!;
    expect(
      rendered.querySelector("[data-g5-question-decision]")!.textContent,
    ).toBe("Decision: A");
    // The reader's own vote seeds the inputs; the decision stays visible.
    expect(
      rendered.querySelectorAll<HTMLInputElement>("input[data-g5-option]")[1]
        .checked,
    ).toBe(true);
  });
});
