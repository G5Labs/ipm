import { describe, expect, it } from "vitest";

import { createMockWorkspace } from "../test/fixtures";
import {
  buildWorkspaceTreeRows,
  filterWorkspaceTreeRows,
  workspaceMatchesSearch,
} from "./workspaceTree";

const workspaces = [
  createMockWorkspace({ id: "base", name: "base", isBase: true }),
  createMockWorkspace({ id: "alpha", name: "alpha", clonedFromId: "base" }),
  createMockWorkspace({
    id: "alpha-fix",
    name: "alpha-fix",
    clonedFromId: "alpha",
  }),
  createMockWorkspace({ id: "beta", name: "beta", clonedFromId: "base" }),
  createMockWorkspace({
    id: "other",
    name: "other-root",
    branches: [
      {
        repoName: "g5",
        branchName: "feat/fixture",
        remoteRepoName: null,
        remoteBranch: null,
      },
    ] as never,
  }),
];

const names = (rows: ReturnType<typeof buildWorkspaceTreeRows>) =>
  rows.map((row) => `${row.depth}:${row.workspace.name}`);

// Search over the lineage tree (workbench g5.workbench.shell-chrome).
describe("filterWorkspaceTreeRows", () => {
  const rows = buildWorkspaceTreeRows(workspaces, { baseFirst: true });

  it("returns every row for an empty query", () => {
    expect(filterWorkspaceTreeRows(rows, "  ")).toBe(rows);
  });

  it("keeps each match with its ancestors, in order and at depth", () => {
    expect(names(filterWorkspaceTreeRows(rows, "FIX"))).toEqual([
      "0:base",
      "1:alpha",
      "2:alpha-fix",
      // Matched through its branch name, not its workspace name.
      "0:other-root",
    ]);
    expect(names(filterWorkspaceTreeRows(rows, "beta"))).toEqual([
      "0:base",
      "1:beta",
    ]);
  });

  it("returns nothing when nothing matches", () => {
    expect(filterWorkspaceTreeRows(rows, "zzz")).toEqual([]);
  });
});

describe("workspaceMatchesSearch", () => {
  it("matches the name and the local or remote branch identity", () => {
    const workspace = createMockWorkspace({
      name: "Feature",
      branches: [
        {
          repoName: "api",
          branchName: "wip",
          remoteRepoName: "acme/api",
          remoteBranch: "feat/remote",
        },
      ] as never,
    });
    expect(workspaceMatchesSearch(workspace, "")).toBe(true);
    expect(workspaceMatchesSearch(workspace, "feature")).toBe(true);
    expect(workspaceMatchesSearch(workspace, "acme/api")).toBe(true);
    expect(workspaceMatchesSearch(workspace, "feat/remote")).toBe(true);
    expect(workspaceMatchesSearch(workspace, "nope")).toBe(false);
  });
});

describe("live workspace choices", () => {
  it("omits private agent inventory rows from fork and merge-target trees", () => {
    const review = createMockWorkspace({
      id: "private-review",
      name: "Agent review",
      isReview: true,
      agentReview: {
        reviewId: "run",
        sourceWorkspaceId: "alpha",
        sourceWorkspaceName: "alpha",
      },
    });
    expect(
      buildWorkspaceTreeRows([...workspaces, review]).map(
        (row) => row.workspace.id,
      ),
    ).toEqual(["base", "alpha", "alpha-fix", "beta", "other"]);
  });
});
