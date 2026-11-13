// =============================================================================
// G5 Files — useFileDraftsAsDirtyEditors
// =============================================================================
// FILES drafts outlive their editors (`g5.documents.files-view.drafts`), so
// the workspace page, not an editor, lists them among the page's unsaved
// editors. A review copy's Discard, which rewrites files behind the editors,
// then drops them instead of leaving edits that would later save over the
// rewritten files with a stale hash. The registry keys files by path alone,
// so a Discard of a path drops that path's drafts in every repository; the
// page registers as its own owner, so an editor of the same path turning
// clean never unlists them.
// =============================================================================

import { useEffect, useState } from "react";
import { setEditorDirty } from "../editor/dirtyEditors";
import { setFileDraft, useWorkspaceFileDrafts } from "./fileDrafts";

export function useFileDraftsAsDirtyEditors(
  person: string | null,
  workspaceId: string | null,
): void {
  const drafts = useWorkspaceFileDrafts(person, workspaceId);
  const [owner] = useState(() => ({}));
  useEffect(() => {
    const keysByPath = new Map<string, string[]>();
    for (const entry of drafts) {
      keysByPath.set(entry.path, [
        ...(keysByPath.get(entry.path) ?? []),
        entry.key,
      ]);
    }
    for (const [path, keys] of keysByPath) {
      setEditorDirty(owner, path, () => {
        for (const key of keys) setFileDraft(key, null);
      });
    }
    return () => {
      for (const path of keysByPath.keys()) setEditorDirty(owner, path, null);
    };
  }, [drafts, owner]);
}
