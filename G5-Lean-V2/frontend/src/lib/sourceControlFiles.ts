import { countChangedFiles } from "./changeFiles";
import type { SourcePatch } from "./sourceControlSelection";

/** One linear grouping pass per applied patch array; selection edits reuse it. */
export function groupSourcePatches(patches: readonly SourcePatch[]) {
  const groups = new Map<string, SourcePatch[]>();
  let additions = 0;
  let deletions = 0;
  for (const patch of patches) {
    const key = `${patch.repoId}\0${patch.fileKey ?? patch.file.path}`;
    const group = groups.get(key);
    if (group) group.push(patch);
    else groups.set(key, [patch]);
    additions += patch.file.diff.additions;
    deletions += patch.file.diff.deletions;
  }
  const counts = countChangedFiles(
    Array.from(groups.values(), ([patch]) => ({
      repoId: patch.repoId,
      path: patch.fileKey ?? patch.file.path,
      kind: patch.kind,
    })),
  );
  return { groups, counts, additions, deletions };
}
