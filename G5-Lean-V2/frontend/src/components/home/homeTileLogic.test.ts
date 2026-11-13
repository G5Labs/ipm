import { describe, expect, it } from "vitest";

import {
  makeIssueSummary,
  makeNativeIssueSummary,
  NATIVE_ALEX,
  NATIVE_MAYA,
} from "../../test/externalIssueFixtures";
import {
  filterByAssignee,
  formatSlotSize,
  formatUptime,
  HOME_ISSUES_TILE_LIMIT,
  labelChipsFrom,
  labelChipsWithSelection,
  tileRows,
} from "./homeTileLogic";

const row = (slotCpus: number | null, slotMemMib: number | null) => ({
  workspaceId: "ws",
  workspaceName: "ws",
  status: "running" as const,
  startedAt: null,
  lastActivityAt: null,
  ownerUserId: null,
  ownerDisplayName: null,
  hostId: null,
  slotCpus,
  slotMemMib,
  instanceType: null,
});

describe("homeTileLogic", () => {
  it("formats uptime as minutes, hours and days", () => {
    const now = Date.parse("2026-10-07T12:00:00Z");
    expect(formatUptime("2026-10-07T11:19:00Z", now)).toBe("41m");
    expect(formatUptime("2026-10-07T09:46:00Z", now)).toBe("2h 14m");
    expect(formatUptime("2026-10-04T08:00:00Z", now)).toBe("3d 4h");
    expect(formatUptime("2026-10-07T12:00:30Z", now)).toBe("0m");
    expect(formatUptime("not a date", now)).toBe("");
  });

  it("formats the slot size from the placed host", () => {
    expect(formatSlotSize(row(4, 16384))).toBe("4 vCPU · 16 GiB");
    expect(formatSlotSize(row(2, 3072))).toBe("2 vCPU · 3 GiB");
    expect(formatSlotSize(row(2, 2560))).toBe("2 vCPU · 2.5 GiB");
    expect(formatSlotSize(row(null, null))).toBeNull();
  });

  it("derives label chips by frequency then name and narrows rows to every selected label", () => {
    const issues = [
      makeIssueSummary({
        locator: { repositoryId: "11", number: 1 },
        updatedAt: "2026-09-01T00:00:00Z",
        labels: [{ name: "ui", color: null }],
      }),
      makeIssueSummary({
        locator: { repositoryId: "11", number: 2 },
        updatedAt: "2026-09-03T00:00:00Z",
        labels: [
          { name: "bug", color: null },
          { name: "ui", color: null },
        ],
      }),
      makeIssueSummary({
        locator: { repositoryId: "11", number: 3 },
        updatedAt: "2026-09-02T00:00:00Z",
        labels: [{ name: "api", color: null }],
      }),
    ];
    expect(labelChipsFrom(issues)).toEqual(["ui", "api", "bug"]);
    expect(
      tileRows(issues, new Set()).map((issue) => issue.locator.number),
    ).toEqual([2, 3, 1]);
    expect(
      tileRows(issues, new Set(["ui", "bug"])).map(
        (issue) => issue.locator.number,
      ),
    ).toEqual([2]);
    const many = Array.from({ length: 12 }, (_, index) =>
      makeIssueSummary({
        locator: { repositoryId: "11", number: index + 1 },
        updatedAt: new Date(Date.UTC(2026, 8, 1, index)).toISOString(),
      }),
    );
    expect(tileRows(many, new Set())).toHaveLength(HOME_ISSUES_TILE_LIMIT);
    expect(tileRows(many, new Set(), 5)).toHaveLength(5);
    expect(tileRows(many, new Set(), Number.POSITIVE_INFINITY)).toHaveLength(
      many.length,
    );
    expect(tileRows(many, new Set())[0].locator.number).toBe(12);
  });

  it("narrows by assignee from the loaded slice and keeps selected labels as chips", () => {
    const mine = makeIssueSummary({
      locator: { repositoryId: "11", number: 1 },
      assignees: [{ login: "OctoCat", avatarUrl: null }],
      labels: [{ name: "bug", color: null }],
    });
    const theirs = makeIssueSummary({
      locator: { repositoryId: "11", number: 2 },
      assignees: [{ login: "hubot", avatarUrl: null }],
      labels: [],
    });
    const nobody = makeIssueSummary({
      locator: { repositoryId: "11", number: 3 },
      assignees: [],
      labels: [{ name: "ui", color: null }],
    });
    const all = [mine, theirs, nobody];
    expect(filterByAssignee(all, "any", "octocat")).toEqual(all);
    expect(filterByAssignee(all, "none", "octocat")).toEqual([nobody]);
    expect(filterByAssignee(all, "me", "octocat")).toEqual([mine]);
    // No connected account: nothing can be "mine".
    expect(filterByAssignee(all, "me", null)).toEqual([]);
    // Native issues are the caller's by G5 user, connected to GitHub or not.
    const nativeMine = makeNativeIssueSummary({
      locator: { repositoryId: "g5", number: 4 },
      assignees: [NATIVE_ALEX],
    });
    const nativeTheirs = makeNativeIssueSummary({
      locator: { repositoryId: "g5", number: 5 },
      assignees: [NATIVE_MAYA],
    });
    const mixed = [mine, nativeMine, nativeTheirs];
    expect(filterByAssignee(mixed, "me", null, "user-alex")).toEqual([
      nativeMine,
    ]);
    expect(filterByAssignee(mixed, "me", "octocat", "user-alex")).toEqual([
      mine,
      nativeMine,
    ]);
    // A selected label the narrowed slice lacks stays a chip, after the slice's.
    expect(labelChipsWithSelection([nobody], new Set(["bug"]))).toEqual([
      "ui",
      "bug",
    ]);
    expect(labelChipsWithSelection([], new Set(["ui", "bug"]))).toEqual([
      "bug",
      "ui",
    ]);
  });
});
