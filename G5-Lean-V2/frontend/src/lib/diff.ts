import type { DiffData } from "./types";

const compareLocaleThenCodeUnit = (left: string, right: string): number => {
  const localeOrder = left.localeCompare(right);
  if (localeOrder !== 0) return localeOrder;
  if (left === right) return 0;
  return left < right ? -1 : 1;
};

/**
 * Deterministic total order over diff-row identity for the workspace Changes
 * diff view: repo display name, then `repoId` (uniquifies same-named repos),
 * then file path. A backing endpoint may return the same rows in a different
 * order on refetch, so rendering must depend only on row data or the list
 * visibly reorders with no underlying change (issue #1236).
 */
export const sortDraftDiffFiles = <
  T extends Pick<DiffData, "repoId" | "repoName" | "file">,
>(
  diffs: T[],
): T[] =>
  [...diffs].sort((a, b) => {
    const byRepoName = compareLocaleThenCodeUnit(a.repoName, b.repoName);
    if (byRepoName !== 0) return byRepoName;
    const byRepoId = compareLocaleThenCodeUnit(a.repoId, b.repoId);
    if (byRepoId !== 0) return byRepoId;
    return compareLocaleThenCodeUnit(a.file, b.file);
  });

export const diffFileKey = (diff: Pick<DiffData, "repoId" | "file">): string =>
  `${diff.repoId}\u0000${diff.file}`;
