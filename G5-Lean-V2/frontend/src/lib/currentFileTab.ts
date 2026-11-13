import type { DocNode, TabItem } from "./types";
import { parseRepositoryDocHandle } from "../hooks/useDocs";

/** Canonical repository path, without guessing a repository or document folder. */
export function canonicalFilePath(path: string): string | null {
  if (!path || path.startsWith("/") || path.includes("\0")) return null;
  const parts: string[] = [];
  for (const part of path.split("/")) {
    if (!part || part === ".") continue;
    if (part === "..") {
      if (!parts.length) return null;
      parts.pop();
    } else parts.push(part);
  }
  return parts.length ? parts.join("/") : null;
}

/** Current openers converge; historical tabs and unresolved docs keep their identity. */
export function currentFileTab(
  item: TabItem,
  docs: readonly DocNode[],
  workspaceId?: string,
): TabItem {
  if (item.type === "doc") {
    const doc = docs.find((d) => d.id === item.docId);
    // Restored handles may name files outside the current Docs listing.
    // Their validated handle carries an exact workspace/repository/path;
    // it never falls back to the selected repository.
    const parsedHandle = parseRepositoryDocHandle(item.docId);
    const handle =
      parsedHandle?.workspaceId === workspaceId ? parsedHandle : null;
    const path = canonicalFilePath(doc?.filePath || handle?.path || "");
    const repoId = doc?.repoId || handle?.repoId;
    if (!path || !repoId) return item;
    return {
      type: "file",
      branchId: repoId,
      filePath: path,
      name: item.name,
      ...(item.liveAuthorityKey
        ? { liveAuthorityKey: item.liveAuthorityKey }
        : {}),
    };
  }
  if (item.type === "file") {
    const path = canonicalFilePath(item.filePath);
    return path ? { ...item, filePath: path } : item;
  }
  return item;
}
