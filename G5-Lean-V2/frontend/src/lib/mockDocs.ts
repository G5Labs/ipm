// =============================================================================
// G5 Client — a workspace's mocks (workbench g5.workbench.mock)
// =============================================================================
// Mocks are Project Docs: the clickable HTML screens under `mocks/` in the
// workspace's Project Docs folder. The Build mock dialog lists them newest
// first and the Guided journey counts them as the mock step's evidence.
// =============================================================================

import {
  docIsUnderFolder,
  sanitizePathForDisplay,
} from "../components/tree/TreeNode";
import type { DocNode } from "./types";
import { fileNameFromPath } from "./utils";

/** The Project Docs subfolder that holds a workspace's mocks. */
export const MOCK_FOLDER = "mocks";

/**
 * This workspace's mocks: every Project Doc under `<docsFolderPath>/mocks/`,
 * newest first by its last change, then by name so equal times are stable.
 */
export function recentMockDocs(
  docs: readonly DocNode[],
  docsFolderPath: string | null | undefined,
): DocNode[] {
  const workspaceFolder = sanitizePathForDisplay(docsFolderPath ?? null);
  if (!workspaceFolder) return [];
  const folder = `${workspaceFolder}/${MOCK_FOLDER}`;
  return docs
    .filter((doc) => docIsUnderFolder(doc, folder))
    .sort((a, b) => {
      const byTime = mockChangedAt(b).localeCompare(mockChangedAt(a));
      if (byTime !== 0) return byTime;
      return mockLabel(a).localeCompare(mockLabel(b), undefined, {
        sensitivity: "base",
      });
    });
}

/** When the mock last changed: content first, then the record's own stamps. */
export function mockChangedAt(doc: DocNode): string {
  return doc.contentUpdatedAt || doc.updatedAt || doc.createdAt || "";
}

/** The row label: the file name, else the document's name. */
export function mockLabel(doc: DocNode): string {
  return (doc.filePath ? fileNameFromPath(doc.filePath) : "") || doc.name;
}

/** The mock's path under `mocks/` without the file name (empty at the root). */
export function mockSubfolder(
  doc: DocNode,
  docsFolderPath: string | null,
): string {
  const workspaceFolder = sanitizePathForDisplay(docsFolderPath);
  if (!workspaceFolder || !doc.filePath) return "";
  const prefix = `${workspaceFolder}/${MOCK_FOLDER}/`;
  if (!doc.filePath.startsWith(prefix)) return "";
  const rest = doc.filePath.slice(prefix.length);
  const slash = rest.lastIndexOf("/");
  return slash > 0 ? rest.slice(0, slash) : "";
}
