import { describe, expect, it } from "vitest";
import type { BranchSummary, WorkspaceDetail } from "./types";
import {
  resolveContentBranch,
  resolveCitationBranch,
  resolveDeepLinkRepoId,
  resolveWorkspaceDocsBranchId,
  sortWorkspaceBranches,
  summarizeRepositoryComparisons,
} from "./workspaceBranches";

function branch(overrides: Partial<BranchSummary>): BranchSummary {
  return {
    workspaceId: "workspace-1",

    repoId: "repo-code",
    repoName: "code",
    branchName: "main",

    trackingMode: "tracking",
    headCommit: "abc123",
    divergence: 0,
    behindBase: 0,
    hasDraft: false,
    dirtyFileCount: 0,
    ...overrides,
  };
}

function workspace(overrides: Partial<WorkspaceDetail>): WorkspaceDetail {
  return {
    id: "workspace-1",
    orgId: "org-1",
    name: "Workspace",
    isBase: false,
    description: null,
    clonedFromId: null,
    docsRepoId: null,
    createdBy: null,
    createdAt: "2026-04-30T00:00:00Z",
    pushMode: "manual",
    updatedAt: "2026-04-30T00:00:00Z",
    branches: [],
    docsFolderPath: "docs/workspace-1",
    ...overrides,
  };
}

describe("workspace repository resolution", () => {
  it("sorts all canonical members without filtering workspace ref names", () => {
    const members = sortWorkspaceBranches([
      branch({
        repoId: "repo-z",
        repoName: "zebra",
        branchName: "feature/123",
      }),
      branch({
        repoId: "repo-a",
        repoName: "alpha",
        branchName: "feature/123",
      }),
    ]);
    expect(members.map((member) => member.repoId)).toEqual([
      "repo-a",
      "repo-z",
    ]);
  });

  it("selects docs by repository identity even when repository names match", () => {
    const members = [
      branch({ repoId: "repo-code", repoName: "docs" }),
      branch({ repoId: "repo-docs", repoName: "docs" }),
    ];
    expect(
      resolveWorkspaceDocsBranchId(
        workspace({ docsRepoId: "repo-docs" }),
        members,
      ),
    ).toBe("repo-docs");
    expect(
      resolveWorkspaceDocsBranchId(
        workspace({ docsRepoId: "missing" }),
        members,
      ),
    ).toBe("");
  });
});

describe("content branch resolution", () => {
  const alpha = branch({
    repoId: "repo-alpha",
    repoName: "alpha",
  });
  const beta = branch({
    repoId: "repo-beta",
    repoName: "beta",
  });

  it("fails closed for a stale request with multiple visible branches", () => {
    expect(
      resolveContentBranch([alpha, beta], "", "", "removed-branch"),
    ).toEqual({
      status: "unresolved",
      reason: "ambiguous-owner",
      candidateBranchIds: ["repo-alpha", "repo-beta"],
    });
  });

  it("resolves a current requested branch before other candidates", () => {
    expect(
      resolveContentBranch([alpha, beta], "repo-alpha", "", "repo-beta"),
    ).toEqual({
      status: "resolved",
      branchId: "repo-beta",
      reason: "requested",
    });
  });

  it("preserves the current workspace docs branch as an explicit owner", () => {
    expect(
      resolveContentBranch([alpha, beta], "", "repo-beta", "repo-beta"),
    ).toEqual({
      status: "resolved",
      branchId: "repo-beta",
      reason: "docs",
    });
  });

  it("resolves a current explicit active branch when the request is stale", () => {
    expect(
      resolveContentBranch([alpha, beta], "repo-beta", "", "removed-branch"),
    ).toEqual({
      status: "resolved",
      branchId: "repo-beta",
      reason: "active",
    });
  });

  it("resolves the sole visible branch without explicit ownership", () => {
    expect(resolveContentBranch([beta], "", "", "removed-branch")).toEqual({
      status: "resolved",
      branchId: "repo-beta",
      reason: "unique",
    });
  });

  it("keeps loading distinct from an unresolved loaded inventory", () => {
    expect(resolveContentBranch(undefined, "", "", "removed-branch")).toEqual({
      status: "loading",
    });
  });
});

describe("citation branch resolution", () => {
  const codeMain = branch({
    repoId: "repo-code",
    repoName: "code",
  });
  const docsMain = branch({
    repoId: "repo-docs",
    repoName: "docs",
  });

  it("preserves active-branch behavior for legacy citations without repoId", () => {
    expect(resolveCitationBranch([codeMain, docsMain], "repo-code")).toEqual({
      status: "resolved",
      branchId: "repo-code",
      reason: "legacy-active",
    });
  });

  it("uses the active branch when it explicitly belongs to the cited repo", () => {
    expect(
      resolveCitationBranch([codeMain, docsMain], "repo-code", "repo-code"),
    ).toEqual({
      status: "resolved",
      branchId: "repo-code",
      reason: "active-repo",
    });
  });

  it("uses the sole visible branch for a non-active cited repo", () => {
    expect(
      resolveCitationBranch([codeMain, docsMain], "repo-code", "repo-docs"),
    ).toEqual({
      status: "resolved",
      branchId: "repo-docs",
      reason: "unique-repo",
    });
  });

  it("fails closed when the cited repo is not visible", () => {
    expect(
      resolveCitationBranch([codeMain, docsMain], "repo-code", "repo-unknown"),
    ).toEqual({
      status: "unresolved",
      reason: "repo-not-visible",
      candidateBranchIds: [],
    });
  });

  it("fails closed when a non-active cited repo has multiple branches", () => {
    expect(
      resolveCitationBranch(
        [
          codeMain,
          docsMain,
          branch({
            workspaceId: "workspace-1",

            repoId: "repo-docs",
            repoName: "docs",
            branchName: "feature",
          }),
        ],
        "repo-code",
        "repo-docs",
      ),
    ).toEqual({
      status: "unresolved",
      reason: "ambiguous-repo",
      candidateBranchIds: ["repo-docs", "repo-docs"],
    });
  });

  it("fails closed when no active branch exists for a legacy citation", () => {
    expect(resolveCitationBranch([codeMain], "")).toEqual({
      status: "unresolved",
      reason: "missing-active",
      candidateBranchIds: [],
    });
  });
});

describe("deep-link repository ownership resolution (center-panel D73, issue #1970)", () => {
  const codeMain = branch({
    repoId: "repo-code",
    repoName: "code",
  });
  const docsMain = branch({
    repoId: "repo-docs",
    repoName: "docs",
  });
  const otherCodeMain = branch({
    repoId: "repo-other",
    repoName: "other-code",
  });
  const docsMeta = { docsRepoId: "repo-docs", docsFolderPath: "docs/ws-1" };

  it("decodes an explicit repoId qualifier against the inventory", () => {
    expect(
      resolveDeepLinkRepoId(
        [codeMain, docsMain],
        docsMeta,
        "g5d/src/main.rs",
        "repo-code",
      ),
    ).toEqual({
      status: "resolved",
      repoId: "repo-code",
      reason: "explicit-ref",
    });
  });

  it("decodes an explicit unique repoName qualifier against the inventory", () => {
    expect(
      resolveDeepLinkRepoId(
        [codeMain, docsMain],
        docsMeta,
        "g5d/src/main.rs",
        "code",
      ),
    ).toEqual({
      status: "resolved",
      repoId: "repo-code",
      reason: "explicit-ref",
    });
  });

  it("lets an explicit qualifier outrank the docs namespace", () => {
    expect(
      resolveDeepLinkRepoId(
        [codeMain, docsMain],
        docsMeta,
        "docs/ws-1/plan.md",
        "code",
      ),
    ).toEqual({
      status: "resolved",
      repoId: "repo-code",
      reason: "explicit-ref",
    });
  });

  it("fails closed on an unknown qualifier with no file request candidates", () => {
    expect(
      resolveDeepLinkRepoId(
        [codeMain, docsMain],
        docsMeta,
        "g5d/src/main.rs",
        "nope",
      ),
    ).toEqual({
      status: "unresolved",
      reason: "unknown-repo-ref",
      candidateRepoIds: [],
    });
  });

  it("fails closed when a repoName qualifier matches multiple repos", () => {
    const clashA = branch({ repoId: "repo-a", repoName: "clash" });
    const clashB = branch({ repoId: "repo-b", repoName: "clash" });
    expect(
      resolveDeepLinkRepoId(
        [clashA, clashB, docsMain],
        docsMeta,
        "g5d/src/main.rs",
        "clash",
      ),
    ).toEqual({
      status: "unresolved",
      reason: "ambiguous-repo-ref",
      candidateRepoIds: ["repo-a", "repo-b"],
    });
  });

  it("resolves the managed docs namespace to the docs repo by workspace metadata", () => {
    expect(
      resolveDeepLinkRepoId(
        [codeMain, otherCodeMain, docsMain],
        docsMeta,
        "docs/ws-1/greenfield_clarifications_pm.md",
      ),
    ).toEqual({
      status: "resolved",
      repoId: "repo-docs",
      reason: "docs-namespace",
    });
  });

  it("keeps the docs-namespace match segment-bounded", () => {
    // "docs/ws-10/…" must not match docsFolderPath "docs/ws-1".
    expect(
      resolveDeepLinkRepoId(
        [codeMain, docsMain],
        docsMeta,
        "docs/ws-10/plan.md",
      ),
    ).toEqual({
      status: "resolved",
      repoId: "repo-code",
      reason: "unique-code-repo",
    });
  });

  it("contributes nothing from the docs rule while docs identity is unknown", () => {
    expect(
      resolveDeepLinkRepoId(
        [codeMain],
        { docsRepoId: null, docsFolderPath: "" },
        "docs/ws-1/plan.md",
      ),
    ).toEqual({
      status: "resolved",
      repoId: "repo-code",
      reason: "unique-code-repo",
    });
  });

  it("auto-resolves an unqualified non-docs path to the sole code repo", () => {
    expect(
      resolveDeepLinkRepoId([codeMain, docsMain], docsMeta, "g5d/src/main.rs"),
    ).toEqual({
      status: "resolved",
      repoId: "repo-code",
      reason: "unique-code-repo",
    });
  });

  it("fails closed listing candidates when multiple code repos are visible", () => {
    expect(
      resolveDeepLinkRepoId(
        [codeMain, otherCodeMain, docsMain],
        docsMeta,
        "g5d/src/main.rs",
      ),
    ).toEqual({
      status: "unresolved",
      reason: "ambiguous-repo",
      candidateRepoIds: ["repo-code", "repo-other"],
    });
  });

  it("fails closed when no code repo is visible for a non-docs path", () => {
    expect(
      resolveDeepLinkRepoId([docsMain], docsMeta, "g5d/src/main.rs"),
    ).toEqual({
      status: "unresolved",
      reason: "no-repo",
      candidateRepoIds: [],
    });
  });
});

// Issue #1995 (prepare-to-ship-modal D3 / workspace_service D50): preserve
// each repository's independent Git history instead of summing commit counts.
describe("summarizeRepositoryComparisons", () => {
  it("returns null when the detail carries no branches", () => {
    expect(summarizeRepositoryComparisons(undefined)).toBeNull();
    expect(summarizeRepositoryComparisons(null)).toBeNull();
    expect(summarizeRepositoryComparisons([])).toBeNull();
  });

  it("preserves dynamic repository names and independent behind/ahead counts", () => {
    expect(
      summarizeRepositoryComparisons([
        branch({
          repoId: "repo-alpha",
          repoName: "payments-api",
          behindBase: 1,
          divergence: 0,
        }),
        branch({
          repoId: "repo-beta",
          repoName: "customer-portal",
          behindBase: 1,
          divergence: 1,
        }),
      ]),
    ).toEqual({
      repositories: [
        {
          repoId: "repo-alpha",
          repoName: "payments-api",
          behindBase: 1,
          divergence: 0,
        },
        {
          repoId: "repo-beta",
          repoName: "customer-portal",
          behindBase: 1,
          divergence: 1,
        },
      ],
      hasBehind: true,
      hasUnknownBehind: false,
      allBehindKnownZero: false,
    });
  });

  it("reports all-known-zero-behind as green-eligible regardless of ahead history", () => {
    expect(
      summarizeRepositoryComparisons([
        branch({ repoId: "repo-a", repoName: "repo-a" }),
        branch({
          repoId: "repo-b",
          repoName: "repo-b",
          divergence: 3,
        }),
      ]),
    ).toMatchObject({
      hasBehind: false,
      hasUnknownBehind: false,
      allBehindKnownZero: true,
    });
  });

  it("flags an unknown behind count so zero cannot read as up to date", () => {
    expect(
      summarizeRepositoryComparisons([
        branch({ repoId: "repo-a", repoName: "repo-a" }),
        branch({
          repoId: "repo-b",
          repoName: "repo-b",
          behindBase: null,
          divergence: null,
        }),
      ]),
    ).toMatchObject({
      hasBehind: false,
      hasUnknownBehind: true,
      allBehindKnownZero: false,
    });
  });

  it("normalizes non-finite values to unknown, never to commit counts", () => {
    const summary = summarizeRepositoryComparisons([
      branch({ behindBase: Number.NaN, divergence: Number.POSITIVE_INFINITY }),
    ]);
    expect(summary?.repositories[0]).toMatchObject({
      behindBase: null,
      divergence: null,
    });
    expect(summary).toMatchObject({
      hasBehind: false,
      hasUnknownBehind: true,
      allBehindKnownZero: false,
    });
  });
});
