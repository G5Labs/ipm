import type { BranchSummary, WorkspaceDetail } from "./types";

/** One repository's independent comparison with the ship target (D3, #1995). */
export interface RepositoryComparison {
  repoId: string;
  repoName: string;
  behindBase: number | null;
  divergence: number | null;
}

/**
 * Repository-aware summary for Prepare-to-Ship's Update-codebase step.
 * Commit counts from unrelated Git histories must never be summed into one
 * workspace count. Nullable or invalid counts remain unknown rather than
 * becoming a fabricated zero.
 */
export interface RepositoryComparisonSummary {
  repositories: RepositoryComparison[];
  hasBehind: boolean;
  hasUnknownBehind: boolean;
  allBehindKnownZero: boolean;
}

function comparisonCount(value: number | null): number | null {
  return typeof value === "number" && Number.isFinite(value) && value >= 0
    ? value
    : null;
}

export function summarizeRepositoryComparisons(
  branches:
    | ReadonlyArray<
        Pick<BranchSummary, "repoId" | "repoName" | "behindBase" | "divergence">
      >
    | null
    | undefined,
): RepositoryComparisonSummary | null {
  if (!Array.isArray(branches) || branches.length === 0) return null;
  const repositories = branches.map((branch) => ({
    repoId: branch.repoId,
    repoName: branch.repoName,
    behindBase: comparisonCount(branch.behindBase),
    divergence: comparisonCount(branch.divergence),
  }));
  return {
    repositories,
    hasBehind: repositories.some(
      ({ behindBase }) => behindBase != null && behindBase > 0,
    ),
    hasUnknownBehind: repositories.some(({ behindBase }) => behindBase == null),
    allBehindKnownZero: repositories.every(
      ({ behindBase }) => behindBase === 0,
    ),
  };
}

export type CitationBranchResolution =
  | {
      status: "resolved";
      branchId: string;
      reason: "legacy-active" | "active-repo" | "unique-repo";
    }
  | {
      status: "unresolved";
      reason: "missing-active" | "repo-not-visible" | "ambiguous-repo";
      candidateBranchIds: string[];
    };

export type ContentBranchResolution =
  | { status: "loading" }
  | {
      status: "resolved";
      branchId: string;
      reason: "requested" | "docs" | "active" | "unique";
    }
  | {
      status: "unresolved";
      reason: "no-visible-branch" | "ambiguous-owner";
      candidateBranchIds: string[];
    };

/**
 * Resolve branch authority for workspace content consumers (shell D57).
 * Undefined inventory is loading, while a loaded array is authoritative even
 * when empty. Presentation order is retained only for candidate display and
 * never selects a branch.
 */
export function resolveContentBranch(
  branches: ReadonlyArray<Pick<BranchSummary, "repoId">> | null | undefined,
  activeBranchId: string,
  docsBranchId: string,
  requestedContentBranchId: string,
): ContentBranchResolution {
  if (!branches) return { status: "loading" };

  const hasBranch = (branchId: string) =>
    !!branchId && branches.some((branch) => branch.repoId === branchId);
  if (hasBranch(requestedContentBranchId)) {
    return {
      status: "resolved",
      branchId: requestedContentBranchId,
      reason: requestedContentBranchId === docsBranchId ? "docs" : "requested",
    };
  }
  if (hasBranch(activeBranchId)) {
    return { status: "resolved", branchId: activeBranchId, reason: "active" };
  }
  if (branches.length === 1) {
    return {
      status: "resolved",
      branchId: branches[0].repoId,
      reason: "unique",
    };
  }
  return {
    status: "unresolved",
    reason: branches.length === 0 ? "no-visible-branch" : "ambiguous-owner",
    candidateBranchIds: branches.map((branch) => branch.repoId),
  };
}

export function sortWorkspaceBranches(
  branches: BranchSummary[],
): BranchSummary[] {
  return [...branches].sort((a, b) => {
    const repoCmp = a.repoName.localeCompare(b.repoName);
    if (repoCmp !== 0) return repoCmp;
    const branchCmp = a.branchName.localeCompare(b.branchName);
    if (branchCmp !== 0) return branchCmp;
    return a.repoId.localeCompare(b.repoId);
  });
}

export function resolveWorkspaceDocsBranchId(
  workspace: WorkspaceDetail,
  branches: BranchSummary[],
): string {
  return branches.some((branch) => branch.repoId === workspace.docsRepoId)
    ? (workspace.docsRepoId ?? "")
    : "";
}

export type DeepLinkRepoResolution =
  | {
      status: "resolved";
      repoId: string;
      reason: "explicit-ref" | "docs-namespace" | "unique-code-repo";
    }
  | {
      status: "unresolved";
      reason:
        | "unknown-repo-ref"
        | "ambiguous-repo-ref"
        | "ambiguous-repo"
        | "no-repo";
      candidateRepoIds: string[];
    };

/**
 * Resolve the owning repository of a `g5://file` deep link (center-panel
 * D73, issue #1970) BEFORE branch selection. Ownership comes from workspace
 * inventory metadata only — never path shape alone, never the active or
 * presentation branch, and never ordering:
 *
 * 1. An explicit `?repo=` qualifier decodes against the visible branch
 *    inventory — exact `repoId` first, else an exact `repoName` naming
 *    exactly one repo; unknown or ambiguous values fail closed.
 * 2. An unqualified path equal to or segment-bounded under the workspace's
 *    server-computed `docsFolderPath` resolves to `docsRepoId` while the
 *    docs identity is known (metadata-confirmed routing, multi-repo
 *    invariant D6/D8).
 * 3. Any other unqualified path auto-resolves only when exactly one
 *    non-docs repo is visible (unique cardinality, invariant D9); zero or
 *    multiple candidates fail closed listing the qualified candidates.
 *
 * The caller hands a resolved `repoId` to `resolveCitationBranch`; deep
 * links never reach that resolver with an undefined `repoId`.
 */
export function resolveDeepLinkRepoId(
  branches: Pick<BranchSummary, "repoId" | "repoName">[],
  docs: { docsRepoId: string | null; docsFolderPath: string },
  filePath: string,
  repoRef?: string,
): DeepLinkRepoResolution {
  if (repoRef) {
    if (branches.some((branch) => branch.repoId === repoRef)) {
      return { status: "resolved", repoId: repoRef, reason: "explicit-ref" };
    }
    const namedRepoIds = [
      ...new Set(
        branches
          .filter((branch) => branch.repoName === repoRef)
          .map((branch) => branch.repoId),
      ),
    ];
    if (namedRepoIds.length === 1) {
      return {
        status: "resolved",
        repoId: namedRepoIds[0],
        reason: "explicit-ref",
      };
    }
    return {
      status: "unresolved",
      reason:
        namedRepoIds.length === 0 ? "unknown-repo-ref" : "ambiguous-repo-ref",
      candidateRepoIds: namedRepoIds,
    };
  }

  const { docsRepoId, docsFolderPath } = docs;
  if (
    docsRepoId &&
    docsFolderPath &&
    (filePath === docsFolderPath || filePath.startsWith(`${docsFolderPath}/`))
  ) {
    return { status: "resolved", repoId: docsRepoId, reason: "docs-namespace" };
  }

  const codeRepoIds = [
    ...new Set(
      branches
        .map((branch) => branch.repoId)
        .filter((repoId) => repoId && repoId !== docsRepoId),
    ),
  ];
  if (codeRepoIds.length === 1) {
    return {
      status: "resolved",
      repoId: codeRepoIds[0],
      reason: "unique-code-repo",
    };
  }
  return {
    status: "unresolved",
    reason: codeRepoIds.length === 0 ? "no-repo" : "ambiguous-repo",
    candidateRepoIds: codeRepoIds,
  };
}

/**
 * Resolve the branch used to open a persisted chat citation.
 *
 * `repoId` is durable repository ownership, not authority to select the first
 * of several branches. A repo-qualified citation may use the global active
 * branch when it belongs to that repo, or the sole visible branch for that
 * repo. Every other repo-qualified case fails closed. Legacy citations without
 * `repoId` preserve their historical active-branch behavior.
 */
export function resolveCitationBranch(
  branches: Pick<BranchSummary, "repoId">[],
  activeBranchId: string,
  repoId?: string,
): CitationBranchResolution {
  if (!repoId) {
    return activeBranchId
      ? {
          status: "resolved",
          branchId: activeBranchId,
          reason: "legacy-active",
        }
      : {
          status: "unresolved",
          reason: "missing-active",
          candidateBranchIds: [],
        };
  }

  const activeBranch = branches.find(
    (branch) => branch.repoId === activeBranchId,
  );
  if (activeBranch?.repoId === repoId) {
    return {
      status: "resolved",
      branchId: activeBranch.repoId,
      reason: "active-repo",
    };
  }

  const candidates = branches.filter((branch) => branch.repoId === repoId);
  if (candidates.length === 1) {
    return {
      status: "resolved",
      branchId: candidates[0].repoId,
      reason: "unique-repo",
    };
  }

  return {
    status: "unresolved",
    reason: candidates.length === 0 ? "repo-not-visible" : "ambiguous-repo",
    candidateBranchIds: candidates.map((branch) => branch.repoId),
  };
}
