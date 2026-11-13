import { describe, expect, it } from "vitest";
import type { WorkspaceDetail } from "./types";
import { sourceThreadWrites } from "./threadWrites";

const detail = (fields: Partial<WorkspaceDetail>) =>
  ({
    isBase: false,
    readOnly: false,
    mutationPolicyTransition: null,
    ...fields,
  }) as WorkspaceDetail;

describe("sourceThreadWrites", () => {
  it("offers thread controls only for a loaded, writable source", () => {
    expect(
      sourceThreadWrites({ status: "ok", data: detail({}) }).writable,
    ).toBe(true);
    for (const closed of [
      { readOnly: true },
      { isBase: true },
      { mutationPolicyTransition: "freezing" },
    ]) {
      expect(
        sourceThreadWrites({ status: "ok", data: detail(closed) }).writable,
      ).toBe(false);
    }
    expect(sourceThreadWrites({ status: "loading" })).toEqual({
      writable: false,
      closedNote: "Replies open once the workspace's state has loaded.",
    });
    expect(
      sourceThreadWrites({ status: "error", error: new Error("down") }),
    ).toEqual({
      writable: false,
      closedNote: "Replies are off: the workspace's state could not be loaded.",
    });
    expect(
      sourceThreadWrites({ status: "ok", data: detail({ isBase: true }) })
        .closedNote,
    ).toContain("Base is read-only");
  });
});
