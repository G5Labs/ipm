import { describe, expect, it } from "vitest";
import {
  countSyncReadyFiles,
  getPreferredAvailableAgentProvider,
  openWorkspacePullRequestForTarget,
  shouldShowDirectMergePreviewAction,
  shouldShowWorkspaceMergeReview,
  workspaceMergeErrorMessage,
  workspaceMergeReviewApprovalAction,
  workspaceMergeTargetsForCurrentWorkspace,
} from "./WorkspacePage";
import { ApiError } from "../lib/api";
import type { MergePreflightResponse, PendingSyncFile } from "../lib/types";

describe("WorkspacePage agent provider defaults (codex_support I11/D3)", () => {
  it("honors the persisted default agent provider when it is advertised", () => {
    expect(
      getPreferredAvailableAgentProvider(["claude_code", "codex"], "codex"),
    ).toBe("codex");
    expect(
      getPreferredAvailableAgentProvider(
        ["claude_code", "codex"],
        "claude_code",
      ),
    ).toBe("claude_code");
  });

  it("ignores a default that is not currently advertised and falls back to first available", () => {
    // Default is Codex but no Codex-capable worker is advertised — never force
    // an unroutable provider (D3); fall back to the first available.
    expect(getPreferredAvailableAgentProvider(["claude_code"], "codex")).toBe(
      "claude_code",
    );
  });

  it("falls back to the first available provider when no default is set", () => {
    expect(getPreferredAvailableAgentProvider(["claude_code", "codex"])).toBe(
      "claude_code",
    );
    expect(getPreferredAvailableAgentProvider(["codex"])).toBe("codex");
  });

  it("returns claude_code when no provider is available", () => {
    expect(getPreferredAvailableAgentProvider([])).toBe("claude_code");
    expect(getPreferredAvailableAgentProvider([], "codex")).toBe("claude_code");
  });
});

describe("WorkspacePage sync ready count", () => {
  it("includes Project Docs and ontology, excludes files outside their scope", () => {
    const workspaceId = "019edbc0-8c0c-7791-8b9c-be69dd0e8d0d";
    const otherWorkspaceId = "019edbc0-ffff-7791-8b9c-000000000000";
    const baseRow: PendingSyncFile = {
      id: `doc:repo-1:docs/${workspaceId}/brief.md`,
      targetKind: "doc",
      path: `docs/${workspaceId}/brief.md`,
      repoId: "repo-1",
      repoName: "docs",
      branchName: "main",
      headCommit: "a".repeat(40),
      lastDownliftedCommitSha: null,
      targetDocRef: "doc-1",
    };

    expect(
      countSyncReadyFiles(
        [
          baseRow,
          {
            ...baseRow,
            id: `doc:repo-1:docs/${workspaceId}/running.md`,
            path: `docs/${workspaceId}/running.md`,
          },
          {
            ...baseRow,
            id: "doc:repo-1:views/plan.md",
            path: "views/plan.md",
            targetDocRef: "view-doc",
          },
          {
            ...baseRow,
            id: "doc:repo-1:designs/plan.md",
            path: "designs/plan.md",
            targetDocRef: "design-doc",
          },
          {
            ...baseRow,
            id: "ontology:node-1:.semlift/specs/core.semlift.md",
            targetKind: "ontology",
            path: ".semlift/specs/core.semlift.md",
            targetDocRef: undefined,
            targetNodeId: "node-1",
          } as unknown as PendingSyncFile,
          // Root-level docs files and other workspaces' Project Docs folders
          // are not sync-ready for this workspace.
          {
            ...baseRow,
            id: "doc:repo-1:docs/root-note.md",
            path: "docs/root-note.md",
            targetDocRef: "root-doc",
          },
          {
            ...baseRow,
            id: `doc:repo-1:docs/${otherWorkspaceId}/leaked.md`,
            path: `docs/${otherWorkspaceId}/leaked.md`,
            targetDocRef: "leaked-doc",
          },
          {
            ...baseRow,
            id: `doc:repo-1:docs/${workspaceId}/ready.md`,
            path: `docs/${workspaceId}/ready.md`,
            targetDocRef: "doc-2",
          },
          {
            ...baseRow,
            id: `doc:repo-1:docs/${workspaceId}/sync/sync-auth--aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa/review/code_review.md`,
            path: `docs/${workspaceId}/sync/sync-auth--aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa/review/code_review.md`,
            targetDocRef: "code-review-doc",
          },
          {
            ...baseRow,
            id: `doc:repo-1:docs/${workspaceId}/sync/sync-auth--aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa/merge/run-1_merge_report.md`,
            path: `docs/${workspaceId}/sync/sync-auth--aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa/merge/run-1_merge_report.md`,
            targetDocRef: "merge-report-doc",
          },
          {
            ...baseRow,
            id: `doc:repo-1:docs/${workspaceId}/sync/sync-auth--aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa/future-report.jsonl`,
            path: `docs/${workspaceId}/sync/sync-auth--aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa/future-report.jsonl`,
            targetDocRef: "future-report-doc",
          },
          {
            ...baseRow,
            id: `doc:repo-1:docs/${workspaceId}/code_review.md`,
            path: `docs/${workspaceId}/code_review.md`,
            targetDocRef: "legacy-code-review-doc",
          },
          {
            ...baseRow,
            id: `doc:repo-1:docs/${workspaceId}/merge/run-1_merge_report.md`,
            path: `docs/${workspaceId}/merge/run-1_merge_report.md`,
            targetDocRef: "legacy-merge-report-doc",
          },
        ],
        workspaceId,
      ),
    ).toBe(3);
  });
});

describe("WorkspacePage workspace merge review routing", () => {
  function preflight(
    decision: MergePreflightResponse["decision"],
  ): MergePreflightResponse {
    return {
      sourceKind: "workspace",
      sourceWorkspaceId: "source-workspace",
      targetWorkspaceId: "target-workspace",
      decision,
      canDirectMerge: decision !== "workflowRequired",
      preflightFingerprint: "sha256:test",
      repoResults: [],
      message: "test",
    };
  }

  it("reviews fast-forward workspace merges before approval", () => {
    const result = preflight("fastForwardOrNoOp");

    expect(shouldShowWorkspaceMergeReview(result)).toBe(true);
    expect(workspaceMergeReviewApprovalAction(result)).toBe("start-route");
    expect(shouldShowDirectMergePreviewAction(result)).toBe(false);
  });

  it("routes clean divergent workspace rebases through source-only approval", () => {
    const result = preflight("cleanMergeNoGitConflicts");

    expect(shouldShowWorkspaceMergeReview(result)).toBe(true);
    expect(workspaceMergeReviewApprovalAction(result)).toBe("start-route");
    expect(shouldShowDirectMergePreviewAction(result)).toBe(true);
  });

  it("shows the full-workflow review surface for conflict or workflow-required decisions", () => {
    for (const decision of ["gitConflicts", "workflowRequired"] as const) {
      const result = preflight(decision);
      expect(shouldShowWorkspaceMergeReview(result)).toBe(true);
      expect(workspaceMergeReviewApprovalAction(result)).toBe("start-route");
    }
  });
});

describe("openWorkspacePullRequestForTarget", () => {
  const pullRequests = [
    { targetWorkspaceId: "base", status: "merged" },
    { targetWorkspaceId: "base", status: "open" },
    { targetWorkspaceId: "other", status: "open" },
  ] as const;

  it("returns the open source PR for the selected target", () => {
    expect(
      openWorkspacePullRequestForTarget(pullRequests, "base"),
    ).toMatchObject({ targetWorkspaceId: "base", status: "open" });
  });

  it("returns null when the selected target has no open source PR", () => {
    expect(
      openWorkspacePullRequestForTarget(pullRequests, "missing"),
    ).toBeNull();
    expect(openWorkspacePullRequestForTarget(pullRequests, "")).toBeNull();
  });
});

describe("WorkspacePage workspace merge target selection", () => {
  const base = { id: "base", name: "Base", clonedFromId: null };
  const fork = { id: "fork", name: "Base copy", clonedFromId: "base" };
  const sibling = { id: "sibling", name: "Sibling", clonedFromId: "base" };

  it("offers a fork's source workspace as the merge target", () => {
    expect(
      workspaceMergeTargetsForCurrentWorkspace(fork, [base, fork, sibling]),
    ).toEqual([{ id: "base", name: "Base" }]);
  });

  it("does not offer child forks from a base workspace", () => {
    expect(
      workspaceMergeTargetsForCurrentWorkspace(base, [base, fork, sibling]),
    ).toEqual([]);
  });
});

describe("WorkspacePage workspace merge errors", () => {
  it("explains missing target repo branches without raw validation JSON", () => {
    const error = new ApiError({
      code: "VALIDATION_ERROR",
      status: 422,
      message:
        'Validation error: {"message":"Target workspace has no materialized branch row for source repo"}',
      details: {
        message:
          "Target workspace has no materialized branch row for source repo",
      },
    });

    expect(workspaceMergeErrorMessage(error)).toContain(
      "does not contain a materialized branch",
    );
    expect(workspaceMergeErrorMessage(error)).not.toContain(
      "candidate_target_branch_names",
    );
  });
});
