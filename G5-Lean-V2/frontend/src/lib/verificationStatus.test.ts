import { describe, expect, it } from "vitest";
import {
  ownStatus,
  statementStatuses,
  type LinkEvidence,
} from "./verificationStatus";

const found: LinkEvidence = { state: "found", root: ".g5/TLA_verification" };
const missing: LinkEvidence = { state: "missing", reason: "No recorded run" };
const checking: LinkEvidence = { state: "checking" };

describe("verification status", () => {
  it("comes from the one tag, and Design verified needs evidence", () => {
    expect(ownStatus([], [])).toBe("unverified");
    expect(ownStatus(["test-coverage"], [])).toBe("test-coverage");
    expect(ownStatus(["manual-check-review"], [])).toBe("manual-check-review");
    expect(ownStatus(["implementation-verified"], [])).toBe(
      "implementation-verified",
    );
    expect(ownStatus(["design-verified"], [])).toBe("unverified");
    expect(ownStatus(["design-verified"], [missing, found])).toBe(
      "design-verified",
    );
    expect(ownStatus(["design-verified"], [missing, checking])).toBe(
      "checking",
    );
    expect(ownStatus(["design-verified"], [missing])).toBe("unverified");
    // Two different tags set no status.
    expect(ownStatus(["test-coverage", "design-verified"], [found])).toBe(
      "unverified",
    );
  });

  it("shows Design verified through descendants unless the parent's own is Implementation verification", () => {
    // 0 ← 1 ← 2, and 3 ← 4
    const parents = [-1, 0, 1, -1, 3];
    const statuses = statementStatuses(
      parents,
      [
        ["test-coverage"],
        [],
        ["design-verified"],
        ["implementation-verified"],
        ["design-verified"],
      ],
      (i) => (i === 2 || i === 4 ? [found] : []),
    );
    expect(statuses.map((s) => s.shown)).toEqual([
      "design-verified",
      "design-verified",
      "design-verified",
      "implementation-verified",
      "design-verified",
    ]);
    expect(statuses[0]).toMatchObject({
      own: "test-coverage",
      viaChildren: [2],
    });
    expect(statuses[3]).toMatchObject({
      own: "implementation-verified",
      viaChildren: [4],
    });
  });

  it("waits for a child still being checked", () => {
    const statuses = statementStatuses(
      [-1, 0],
      [[], ["design-verified"]],
      () => [checking],
    );
    expect(statuses.map((s) => s.shown)).toEqual(["checking", "checking"]);
    const settled = statementStatuses(
      [-1, 0],
      [[], ["design-verified"]],
      () => [missing],
    );
    expect(settled.map((s) => s.shown)).toEqual(["unverified", "unverified"]);
    expect(settled[1].evidence).toEqual([missing]);
  });

  it("keeps linked evidence for any tag, which sets only Design verified", () => {
    const statuses = statementStatuses(
      [-1, -1],
      [["test-coverage"], []],
      () => [found],
    );
    expect(statuses.map((s) => s.shown)).toEqual([
      "test-coverage",
      "unverified",
    ]);
    expect(statuses.map((s) => s.evidence)).toEqual([[found], [found]]);
  });
});
