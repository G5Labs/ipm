import { describe, expect, it } from "vitest";
import {
  resolveCurrentWorkspaceSummary,
  workspaceIdsWithLiveChildren,
} from "./WorkspacePage";
import type {
  BranchSummary,
  WorkspaceDetail,
  WorkspaceSummary,
} from "../lib/types";

// Regression coverage for workspace-page D25 / chat C17: the current-workspace
// clone/fork source must reflect the authoritative workspace-detail branches so
// the CloneDialog isolate/co-track selector (top-bar D4) renders on first open,
// even when the cached org list lags a just-activated remote_tracking repo.

const branch = (overrides: Partial<BranchSummary> = {}): BranchSummary => ({
  workspaceId: "workspace-1",

  repoId: "r1",
  repoName: "repo",
  branchName: "main",
  trackingMode: "tracking",
  headCommit: "c1",
  divergence: 0,
  behindBase: 0,
  hasDraft: false,
  dirtyFileCount: 0,
  ...overrides,
});

const listItem = (
  overrides: Partial<WorkspaceSummary> = {},
): WorkspaceSummary => ({
  id: "ws1",
  name: "ws",
  description: null,
  clonedFromId: null,
  // The list lags: its branch is still 'tracking' (parent overlay), not yet
  // 'remote_tracking'.
  branches: [branch()],
  tasksCompleted: 3,
  totalTasks: 5,
  runningTime: "1h",
  cost: 12.5,
  concurrentTasks: 2,
  lastUpdated: "2026-06-14T00:00:00Z",
  createdAt: "2026-06-01T00:00:00Z",
  ...overrides,
  isBase: overrides.isBase ?? false,
  createdBy: overrides.createdBy ?? null,
});

const detail = (overrides: Partial<WorkspaceDetail> = {}): WorkspaceDetail => ({
  id: "ws1",
  orgId: "org1",
  name: "ws",
  isBase: false,
  description: null,
  clonedFromId: null,
  docsRepoId: null,

  docsFolderPath: "docs/ws1",
  createdBy: null,
  createdAt: "2026-06-01T00:00:00Z",
  updatedAt: "2026-06-14T12:00:00Z",
  pushMode: "manual",
  // The detail is authoritative: the repo is remote_tracking.
  branches: [branch({ trackingMode: "remote_tracking", remoteBranch: "main" })],
  ...overrides,
});

describe("resolveCurrentWorkspaceSummary", () => {
  it("overlays authoritative detail branches over a stale list item (C17 regression)", () => {
    const result = resolveCurrentWorkspaceSummary(listItem(), detail());
    expect(result).not.toBeNull();
    // The fork dialog keys off this: remote_tracking must be present on first open.
    expect(
      result!.branches.some((b) => b.trackingMode === "remote_tracking"),
    ).toBe(true);
  });

  it("keeps aggregate stats from the list item while overlaying detail branches", () => {
    const li = listItem({ cost: 99, tasksCompleted: 7 });
    const result = resolveCurrentWorkspaceSummary(li, detail());
    expect(result!.cost).toBe(99);
    expect(result!.tasksCompleted).toBe(7);
    expect(
      result!.branches.some((b) => b.trackingMode === "remote_tracking"),
    ).toBe(true);
    // The overlay must NOT mutate the cached list item's branches (React Query
    // cache integrity) — guards against a future `listed.branches = detail.branches`.
    expect(li.branches.every((b) => b.trackingMode === "tracking")).toBe(true);
  });

  it("returns the list item unchanged when detail is not yet loaded", () => {
    const li = listItem();
    expect(resolveCurrentWorkspaceSummary(li, null)).toBe(li);
  });

  it("falls back to a detail-built summary (zeroed stats) when not in the list", () => {
    const result = resolveCurrentWorkspaceSummary(undefined, detail());
    expect(result).not.toBeNull();
    expect(result!.id).toBe("ws1");
    expect(result!.cost).toBe(0);
    expect(result!.lastUpdated).toBe("2026-06-14T12:00:00Z");
    expect(
      result!.branches.some((b) => b.trackingMode === "remote_tracking"),
    ).toBe(true);
  });

  it("returns null when neither list nor detail is available", () => {
    expect(resolveCurrentWorkspaceSummary(undefined, null)).toBeNull();
  });
});

describe("workspaceIdsWithLiveChildren", () => {
  it("collects parent ids from the live workspace list for remove-button gating", () => {
    expect(
      Array.from(
        workspaceIdsWithLiveChildren([
          { clonedFromId: null },
          { clonedFromId: "ws-parent" },
          { clonedFromId: "ws-parent" },
          { clonedFromId: "ws-other-parent" },
        ]),
      ).sort(),
    ).toEqual(["ws-other-parent", "ws-parent"]);
  });
});
