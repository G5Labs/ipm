import { describe, expect, it } from "vitest";

import {
  assistantMessageToStop,
  chatSuccessorExists,
  stopHandoffBridging,
} from "./WorkspacePage";

describe("assistantMessageToStop", () => {
  it.each(["pending", "streaming"] as const)(
    "targets the %s active assistant turn before later queued turns",
    (status) => {
      const active = { id: "assistant-a", role: "assistant", status };
      const queuedB = {
        id: "assistant-b",
        role: "assistant",
        status: "pending",
      };
      const queuedC = {
        id: "assistant-c",
        role: "assistant",
        status: "pending",
      };

      expect(
        assistantMessageToStop([
          { id: "user-a", role: "user", status: "complete" },
          active,
          { id: "user-b", role: "user", status: "complete" },
          queuedB,
          { id: "user-c", role: "user", status: "complete" },
          queuedC,
        ]),
      ).toEqual(active);
    },
  );

  it("skips terminal assistant turns and returns no target when none is active", () => {
    expect(
      assistantMessageToStop([
        { id: "assistant-a", role: "assistant", status: "complete" },
        { id: "assistant-b", role: "assistant", status: "stopped" },
        { id: "assistant-c", role: "assistant", status: "error" },
      ]),
    ).toBeUndefined();
  });
});

describe("stopHandoffBridging", () => {
  it("does not bridge a plain stop with no queued successor", () => {
    // The reported bug: a single-turn stop must not suppress the Stop control,
    // otherwise it vanishes while the turn is still live and only a refresh
    // restores it.
    expect(
      stopHandoffBridging(
        [
          { id: "user-a", role: "user", status: "complete" },
          { id: "assistant-a", role: "assistant", status: "streaming" },
        ],
        "assistant-a",
      ),
    ).toBe(false);
  });

  it("does not bridge once the stopped turn has terminalized alone", () => {
    expect(
      stopHandoffBridging(
        [{ id: "assistant-a", role: "assistant", status: "stopped" }],
        "assistant-a",
      ),
    ).toBe(false);
  });

  it("bridges while a queued successor turn is still pending behind the stopped turn", () => {
    expect(
      stopHandoffBridging(
        [
          { id: "assistant-a", role: "assistant", status: "stopped" },
          { id: "user-b", role: "user", status: "complete" },
          { id: "assistant-b", role: "assistant", status: "pending" },
        ],
        "assistant-a",
      ),
    ).toBe(true);
  });

  it("stops bridging as soon as the successor turn starts streaming", () => {
    expect(
      stopHandoffBridging(
        [
          { id: "assistant-a", role: "assistant", status: "stopped" },
          { id: "assistant-b", role: "assistant", status: "streaming" },
        ],
        "assistant-a",
      ),
    ).toBe(false);
  });

  it("ignores the stopped turn's own pending/streaming status", () => {
    // A still-live stopped turn with no other successor is not a bridge.
    expect(
      stopHandoffBridging(
        [{ id: "assistant-a", role: "assistant", status: "pending" }],
        "assistant-a",
      ),
    ).toBe(false);
  });

  it("does not bridge on a pending assistant that precedes the stopped turn", () => {
    // Only later turns are successors; a queued assistant before the stopped
    // turn must never be mistaken for an A→B handoff.
    expect(
      stopHandoffBridging(
        [
          { id: "assistant-earlier", role: "assistant", status: "pending" },
          { id: "user-a", role: "user", status: "complete" },
          { id: "assistant-a", role: "assistant", status: "streaming" },
        ],
        "assistant-a",
      ),
    ).toBe(false);
  });

  it("does not bridge when the stopped message is absent from the list", () => {
    expect(
      stopHandoffBridging(
        [
          { id: "assistant-x", role: "assistant", status: "pending" },
          { id: "assistant-y", role: "assistant", status: "pending" },
        ],
        "assistant-a",
      ),
    ).toBe(false);
  });
});

describe("chatSuccessorExists", () => {
  const timeline = [
    { id: "assistant-earlier", role: "assistant", status: "pending" },
    { id: "assistant-a", role: "assistant", status: "streaming" },
    { id: "user-b", role: "user", status: "complete" },
    { id: "assistant-b", role: "assistant", status: "pending" },
  ] as const;

  it("only counts assistant turns positioned after the stopped turn", () => {
    expect(chatSuccessorExists(timeline, "assistant-a", "pending")).toBe(true);
    // The earlier pending assistant precedes the stopped turn, so it is not a
    // successor of a turn positioned after it.
    expect(chatSuccessorExists(timeline, "assistant-b", "pending")).toBe(false);
  });

  it("returns false when the stopped message id is not present", () => {
    expect(chatSuccessorExists(timeline, "missing", "pending")).toBe(false);
  });
});
