import { describe, expect, it } from "vitest";

import {
  chatLandingQuery,
  forkLandingQuery,
  forkLandingState,
  readForkLandingState,
} from "../workspaceLanding";

// A fork lands on Chat as the center surface (ui g5.workspace.ui.fork-landing).
describe("fork landing", () => {
  it("opens the chat the fork started, carrying no draft", () => {
    const landing = { chatSessionId: "chat 7", draft: null };
    expect(forkLandingQuery(landing)).toBe(chatLandingQuery("chat 7"));
    expect(forkLandingQuery(landing)).toBe("?session=chat%207");
    expect(forkLandingState(landing)).toBeUndefined();
  });

  it("opens a fresh composer with the fork's draft when no chat started", () => {
    const landing = { chatSessionId: null, draft: "Work on acme/api#12" };
    expect(forkLandingQuery(landing)).toBe("?view=chat");
    expect(forkLandingState(landing)).toEqual({
      forkLanding: { draft: "Work on acme/api#12" },
    });
  });

  it("opens an empty composer for a fork without an issue", () => {
    const landing = { chatSessionId: null, draft: null };
    expect(forkLandingQuery(landing)).toBe("?view=chat");
    expect(forkLandingState(landing)).toEqual({
      forkLanding: { draft: null },
    });
  });

  it("reads only a well-formed landing from router state", () => {
    expect(readForkLandingState(null)).toBeNull();
    expect(readForkLandingState({ other: true })).toBeNull();
    expect(readForkLandingState({ forkLanding: "draft" })).toBeNull();
    expect(readForkLandingState({ forkLanding: { draft: 3 } })).toEqual({
      draft: null,
    });
    expect(
      readForkLandingState(
        forkLandingState({ chatSessionId: null, draft: "Hello" }),
      ),
    ).toEqual({ draft: "Hello" });
  });

  // A Base re-verification run lands its prompt under the cost notice
  // (ontology_and_harness g5.reverification.workspace).
  it("carries a seed header with the draft, and only a well-formed one", () => {
    const landing = {
      chatSessionId: null,
      draft: "Base re-verification (full) …",
      header: { kind: "verification" as const },
    };
    expect(forkLandingQuery(landing)).toBe("?view=chat");
    const state = forkLandingState(landing);
    expect(state).toEqual({
      forkLanding: {
        draft: "Base re-verification (full) …",
        header: { kind: "verification" },
      },
    });
    expect(readForkLandingState(state)).toEqual({
      draft: "Base re-verification (full) …",
      header: { kind: "verification" },
    });
    // The header survives a structured clone, as history state is stored.
    expect(readForkLandingState(structuredClone(state))).toEqual(
      readForkLandingState(state),
    );
    expect(
      readForkLandingState({
        forkLanding: { draft: "Run", header: { kind: "base-verification" } },
      }),
    ).toEqual({ draft: "Run", header: { kind: "base-verification" } });
    expect(
      readForkLandingState({
        forkLanding: {
          draft: "Review",
          header: { kind: "review-loop", iterations: 3 },
        },
      }),
    ).toEqual({
      draft: "Review",
      header: { kind: "review-loop", iterations: 3 },
    });
    // Without a header the state is exactly the earlier shape.
    expect(
      Object.keys(
        forkLandingState({ chatSessionId: null, draft: "x" })!.forkLanding,
      ),
    ).toEqual(["draft"]);
    for (const header of [
      "verification",
      { kind: "other" },
      { kind: "review-loop" },
      { kind: "review-loop", iterations: "3" },
    ]) {
      expect(
        readForkLandingState({ forkLanding: { draft: "x", header } }),
      ).toEqual({ draft: "x" });
    }
    // A started chat carries nothing, header included.
    expect(
      forkLandingState({ ...landing, chatSessionId: "chat-1" }),
    ).toBeUndefined();
  });
});
