// =============================================================================
// G5 Editor — dirtyEditors
// =============================================================================
// Which open editors hold unsaved edits, by file path, for the parts of the
// page that rewrite files behind the editor's back (a review copy's Discard
// from my copy). An editor registers itself while dirty, together with a
// way to drop its edits so it reloads from the refetched document instead
// of saving over it with a stale hash. Each holder of edits (an editor, or
// the page for its FILES drafts) registers under its own owner identity, so
// one turning clean never unlists another's edits on the same path, such as
// the same path in another repository. Module state on purpose: editors and
// the review panel share no ancestor that could hold it.
// =============================================================================

import { useSyncExternalStore } from "react";

type Listener = () => void;

/** Per path, each owner's way to drop its edits there. */
const dirty = new Map<string, Map<object, () => void>>();
const listeners = new Set<Listener>();
let snapshot: readonly string[] = [];

function emit(): void {
  snapshot = Array.from(dirty.keys());
  for (const listener of listeners) listener();
}

/**
 * Mark `owner` (a stable identity of the editor or other holder) as holding
 * unsaved edits to `filePath`, with the function that throws them away, or
 * clear that owner's mark when it is clean again. Other owners' marks on the
 * same path stay.
 */
export function setEditorDirty(
  owner: object,
  filePath: string,
  dropEdits: (() => void) | null,
): void {
  const owners = dirty.get(filePath);
  if (dropEdits) {
    if (owners) {
      owners.set(owner, dropEdits);
    } else {
      dirty.set(filePath, new Map([[owner, dropEdits]]));
    }
  } else {
    if (!owners?.delete(owner)) return;
    if (owners.size === 0) dirty.delete(filePath);
  }
  emit();
}

/** The file paths whose editors hold unsaved edits, in registration order. */
export function dirtyEditorPaths(): readonly string[] {
  return snapshot;
}

/**
 * Drop every owner's unsaved edits on `paths`; returns the paths that had
 * any.
 */
export function dropDirtyEdits(paths: readonly string[]): string[] {
  const dropped: string[] = [];
  for (const path of paths) {
    const owners = dirty.get(path);
    if (!owners) continue;
    dirty.delete(path);
    for (const drop of owners.values()) drop();
    dropped.push(path);
  }
  if (dropped.length > 0) emit();
  return dropped;
}

function subscribe(listener: Listener): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** The dirty editors' paths, as a live value. */
export function useDirtyEditorPaths(): readonly string[] {
  return useSyncExternalStore(subscribe, dirtyEditorPaths, dirtyEditorPaths);
}

/** Tests only: forget every registration. */
export function resetDirtyEditorsForTests(): void {
  dirty.clear();
  emit();
}
