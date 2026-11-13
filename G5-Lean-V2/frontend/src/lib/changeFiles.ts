/** Shared presentation of the server's canonical changed-file categories. */
export const CHANGE_FILE_KINDS = ["code", "ontology", "docs"] as const;
export type ChangeFileKind = (typeof CHANGE_FILE_KINDS)[number];
export const CHANGE_FILE_LABELS: Record<ChangeFileKind, string> = {
  code: "Code",
  ontology: "Ontology",
  docs: "Docs",
};
export type ChangeFileCounts = Record<ChangeFileKind, number>;

export function countChangedFiles(
  files: Iterable<{ repoId: string; path: string; kind?: string }>,
): ChangeFileCounts {
  const counts: ChangeFileCounts = { code: 0, ontology: 0, docs: 0 };
  const seen = new Set<string>();
  for (const file of files) {
    const key = `${file.repoId}\0${file.path}`;
    if (seen.has(key)) continue;
    seen.add(key);
    const kind = CHANGE_FILE_KINDS.find((kind) => kind === file.kind) ?? "code";
    counts[kind]++;
  }
  return counts;
}

export function changeFileSummary(
  counts: ChangeFileCounts,
  types: readonly ChangeFileKind[] = CHANGE_FILE_KINDS,
): string {
  return CHANGE_FILE_KINDS.filter((kind) => types.includes(kind))
    .map((kind) => `${CHANGE_FILE_LABELS[kind]} ${counts[kind]}`)
    .join(" · ");
}
