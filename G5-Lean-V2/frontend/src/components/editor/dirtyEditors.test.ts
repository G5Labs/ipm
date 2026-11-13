/**
 * dirtyEditors — the shared list of editors holding unsaved edits, and the
 * way to drop those edits before a file is rewritten under them.
 */

import { afterEach, describe, expect, it, vi } from "vitest";

import {
  dirtyEditorPaths,
  dropDirtyEdits,
  resetDirtyEditorsForTests,
  setEditorDirty,
} from "./dirtyEditors";

describe("dirtyEditors", () => {
  afterEach(() => resetDirtyEditorsForTests());

  it("lists dirty editors, drops their edits on request, and forgets clean ones", () => {
    const editorA = {};
    const editorB = {};
    const dropA = vi.fn();
    const dropB = vi.fn();
    setEditorDirty(editorA, "a.rs", dropA);
    setEditorDirty(editorB, "b.rs", dropB);
    expect(dirtyEditorPaths()).toEqual(["a.rs", "b.rs"]);
    setEditorDirty(editorB, "b.rs", null);
    expect(dirtyEditorPaths()).toEqual(["a.rs"]);
    expect(dropDirtyEdits(["a.rs", "b.rs", "c.rs"])).toEqual(["a.rs"]);
    expect(dropA).toHaveBeenCalledOnce();
    expect(dropB).not.toHaveBeenCalled();
    expect(dirtyEditorPaths()).toEqual([]);
  });

  it("keeps each owner's mark on a shared path until that owner clears it, and drops them all", () => {
    const pageDrafts = {};
    const cleanEditor = {};
    const dropDrafts = vi.fn();
    const dropEditor = vi.fn();
    setEditorDirty(pageDrafts, "docs/a.md", dropDrafts);
    // Another editor of the same path turning clean leaves the drafts listed.
    setEditorDirty(cleanEditor, "docs/a.md", null);
    expect(dirtyEditorPaths()).toEqual(["docs/a.md"]);
    setEditorDirty(cleanEditor, "docs/a.md", dropEditor);
    setEditorDirty(cleanEditor, "docs/a.md", null);
    expect(dirtyEditorPaths()).toEqual(["docs/a.md"]);

    setEditorDirty(cleanEditor, "docs/a.md", dropEditor);
    expect(dropDirtyEdits(["docs/a.md"])).toEqual(["docs/a.md"]);
    expect(dropDrafts).toHaveBeenCalledOnce();
    expect(dropEditor).toHaveBeenCalledOnce();
    expect(dirtyEditorPaths()).toEqual([]);
  });
});
