import { describe, expect, it } from "vitest";
import { turnActivityCoverage, type TimelineCoverage } from "./historyCoverage";

const ready = (
  floorCreatedAt: string | null,
  hasOlder = true,
): TimelineCoverage => ({ status: "ready", floorCreatedAt, hasOlder });

describe("turnActivityCoverage (g5.chat.history-window)", () => {
  const turn = "2026-10-27T10:00:00.500Z";

  it("is complete when nothing older exists", () => {
    expect(turnActivityCoverage(turn, 0, ready(null, false))).toBe("complete");
    expect(
      turnActivityCoverage(turn, 0, ready("2026-10-27T11:00:00Z", false)),
    ).toBe("complete");
  });

  it("is complete when the floor is strictly older than the turn", () => {
    expect(
      turnActivityCoverage(turn, 3, ready("2026-10-27T10:00:00.499Z")),
    ).toBe("complete");
    // The server's microsecond timestamps compare at millisecond precision.
    expect(
      turnActivityCoverage(turn, 3, ready("2026-10-27T10:00:00.499999+00:00")),
    ).toBe("complete");
  });

  it("does not count a floor at the turn's own instant as covering it", () => {
    expect(turnActivityCoverage(turn, 2, ready(turn))).toBe("partial");
  });

  it("is partial with some of the turn loaded and unloaded with none", () => {
    const floor = ready("2026-10-27T10:05:00Z");
    expect(turnActivityCoverage(turn, 4, floor)).toBe("partial");
    expect(turnActivityCoverage(turn, 0, floor)).toBe("unloaded");
  });

  it("is not complete while the first read is pending or failed", () => {
    for (const status of ["loading", "error"] as const) {
      const coverage = { status, floorCreatedAt: null, hasOlder: false };
      expect(turnActivityCoverage(turn, 0, coverage)).toBe("unloaded");
      expect(turnActivityCoverage(turn, 1, coverage)).toBe("partial");
    }
  });

  it("treats every turn as complete without a coverage source or a time", () => {
    expect(turnActivityCoverage(turn, 0, undefined)).toBe("complete");
    expect(turnActivityCoverage("", 0, ready("2026-10-27T11:00:00Z"))).toBe(
      "complete",
    );
  });
});
