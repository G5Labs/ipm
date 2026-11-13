import type { ReviewHead } from "./types";

/** Missing/malformed historical evidence is never silently an empty vector. */
export function approvalHeads(value: unknown): ReviewHead[] | null {
  if (!Array.isArray(value) || value.length === 0) return null;
  const seen = new Set<string>();
  for (const head of value) {
    if (
      !head ||
      typeof head !== "object" ||
      typeof head.repoId !== "string" ||
      typeof head.headCommit !== "string" ||
      !/^[a-f\d]{40,64}$/i.test(head.headCommit) ||
      seen.has(head.repoId)
    )
      return null;
    seen.add(head.repoId);
  }
  return value as ReviewHead[];
}
export function approvalVersionState(
  before: unknown,
  after: unknown,
): "matching" | "changed" | "unknown" {
  const reviewed = approvalHeads(before),
    current = approvalHeads(after);
  if (!reviewed || !current) return "unknown";
  return reviewed.length === current.length &&
    reviewed.every((h) =>
      current.some(
        (c) => c.repoId === h.repoId && c.headCommit === h.headCommit,
      ),
    )
    ? "matching"
    : "changed";
}
