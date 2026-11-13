import { describe, expect, it } from "vitest";

import { chatActivityState, chatBubbleState } from "./chatSessionActivity";
import type { ChatSessionActivityDTO } from "./types";

function activity(
  overrides: Partial<ChatSessionActivityDTO>,
): ChatSessionActivityDTO {
  return {
    activeTurnStartedAt: null,
    lastFinishedAt: null,
    latestResult: null,
    unseenResult: false,
    ...overrides,
  };
}

// ui g5.workspace.ui.chat-bubble-stack and chat g5.chat.activity.awaiting-input:
// the bubble's face comes from the server's activity, waiting first.
describe("chatBubbleState", () => {
  it("is idle without activity and while the chat needs nobody", () => {
    expect(chatBubbleState(undefined)).toBe("idle");
    expect(chatBubbleState(null)).toBe("idle");
    expect(
      chatBubbleState(
        activity({
          lastFinishedAt: "2024-11-15T10:00:00Z",
          latestResult: { taskId: "t1", attempt: 1, hasOutput: true },
        }),
      ),
    ).toBe("idle");
  });

  it("is working while a turn is active and idle while the result is unseen", () => {
    expect(
      chatBubbleState(
        activity({ activeTurnStartedAt: "2024-11-15T10:00:00Z" }),
      ),
    ).toBe("working");
    expect(
      chatBubbleState(
        activity({
          latestResult: { taskId: "t1", attempt: 1, hasOutput: true },
          unseenResult: true,
        }),
      ),
    ).toBe("idle");
  });

  it("is waiting whenever the turn waits for the person, active or held", () => {
    expect(
      chatBubbleState(
        activity({
          activeTurnStartedAt: "2024-11-15T10:00:00Z",
          awaitingInput: true,
        }),
      ),
    ).toBe("waiting");
    // A deferred question holds the turn: no active turn, an unseen result,
    // and still waiting.
    expect(
      chatBubbleState(
        activity({
          latestResult: { taskId: "t1", attempt: 1, hasOutput: true },
          unseenResult: true,
          awaitingInput: true,
        }),
      ),
    ).toBe("waiting");
  });

  it("leaves the picker's indicator states untouched", () => {
    const waiting = activity({
      activeTurnStartedAt: "2024-11-15T10:00:00Z",
      awaitingInput: true,
    });
    expect(chatActivityState(waiting)).toBe("active");
  });
});
